//! Secret redaction for anything that leaves the local store (handoff
//! contracts, context packets, exports, MCP responses, logs).

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Finding {
    pub kind: String,
    pub count: usize,
}

fn patterns() -> &'static [(&'static str, Regex)] {
    static P: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    P.get_or_init(|| {
        let v: Vec<(&str, &str)> = vec![
            ("private_key", r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----"),
            ("aws_access_key", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
            ("github_token", r"\b(?:ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{36,}\b|\bgithub_pat_[A-Za-z0-9_]{22,}\b"),
            ("anthropic_key", r"\bsk-ant-[A-Za-z0-9_\-]{20,}"),
            ("openai_key", r"\bsk-(?:proj-)?[A-Za-z0-9_\-]{20,}"),
            ("google_api_key", r"\bAIza[0-9A-Za-z_\-]{35}\b"),
            ("slack_token", r"\bxox[abposr]-[A-Za-z0-9\-]{10,}"),
            ("stripe_key", r"\b(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{16,}"),
            ("jwt", r"\beyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}"),
            ("url_credentials", r"(?i)\b[a-z][a-z0-9+.\-]*://[^/\s:@]+:[^/\s@]+@"),
            (
                "assignment",
                r#"(?i)\b([A-Za-z0-9_]*(?:api[_\-]?key|secret|token|passwd|password|auth[_\-]?token|access[_\-]?key|client[_\-]?secret)[A-Za-z0-9_]*)\s*[:=]\s*["']?([^\s"'`,;]{6,})"#,
            ),
        ];
        v.into_iter().map(|(k, p)| (k, Regex::new(p).expect("valid redaction regex"))).collect()
    })
}

/// Replace secrets with `[REDACTED:<kind>]`. Returns the text and what was found.
pub fn redact(input: &str) -> (String, Vec<Finding>) {
    let mut out = input.to_string();
    let mut findings = Vec::new();
    for (kind, re) in patterns() {
        let count = re.find_iter(&out).count();
        if count == 0 {
            continue;
        }
        out = if *kind == "assignment" {
            re.replace_all(&out, |c: &regex::Captures| {
                format!("{}=[REDACTED:secret]", &c[1])
            })
            .to_string()
        } else if *kind == "url_credentials" {
            re.replace_all(&out, |c: &regex::Captures| {
                let m = &c[0];
                let scheme_end = m.find("://").map(|i| i + 3).unwrap_or(0);
                format!("{}[REDACTED:credentials]@", &m[..scheme_end])
            })
            .to_string()
        } else {
            re.replace_all(&out, format!("[REDACTED:{kind}]").as_str())
                .to_string()
        };
        findings.push(Finding {
            kind: kind.to_string(),
            count,
        });
    }
    (out, findings)
}

/// Redact every string inside a JSON value in place.
pub fn redact_json(v: &mut serde_json::Value, findings: &mut Vec<Finding>) {
    match v {
        serde_json::Value::String(s) => {
            let (r, f) = redact(s);
            if !f.is_empty() {
                *s = r;
                for x in f {
                    match findings.iter_mut().find(|y| y.kind == x.kind) {
                        Some(y) => y.count += x.count,
                        None => findings.push(x),
                    }
                }
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(|x| redact_json(x, findings)),
        serde_json::Value::Object(o) => o.values_mut().for_each(|x| redact_json(x, findings)),
        _ => {}
    }
}

/// Files whose *content* must never be embedded in exported context.
pub fn is_sensitive_path(path: &str) -> bool {
    let name = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".keystore")
        || name.starts_with("id_rsa")
        || name.starts_with("id_ed25519")
        || name == ".npmrc"
        || name == ".netrc"
        || name == ".pypirc"
        || name == "credentials"
        || name == "credentials.json"
        || name.contains("secret")
        || name == "service-account.json"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts_common_secrets() {
        let s = "key AKIAABCDEFGHIJKLMNOP and ghp_abcdefghijklmnopqrstuvwxyz0123456789AB and OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwx";
        let (r, f) = redact(s);
        assert!(!r.contains("AKIAABCDEFGHIJKLMNOP"));
        assert!(!r.contains("ghp_abcdefghijklmnopqrstuvwxyz0123456789AB"));
        assert!(!r.contains("sk-proj-abcdefghijklmnopqrstuvwx"));
        assert!(f.len() >= 3);
    }
    #[test]
    fn redacts_url_credentials_and_private_keys() {
        let s = "postgres://admin:hunter2pass@db.local/x\n-----BEGIN RSA PRIVATE KEY-----\nMIIabc\n-----END RSA PRIVATE KEY-----";
        let (r, _) = redact(s);
        assert!(!r.contains("hunter2pass"));
        assert!(!r.contains("MIIabc"));
    }
    #[test]
    fn leaves_normal_text() {
        let (r, f) = redact("Implement the token bucket rate limiter in src/limit.rs");
        assert_eq!(r, "Implement the token bucket rate limiter in src/limit.rs");
        assert!(f.is_empty());
    }
    #[test]
    fn sensitive_paths() {
        assert!(is_sensitive_path("config/.env.local"));
        assert!(is_sensitive_path("certs/server.pem"));
        assert!(!is_sensitive_path("src/main.rs"));
    }
}
