//! Normalize the quota fields from documented CLI status-line payloads.
use crate::{paths::GlobalPaths, util};
use serde_json::{json, Value};
use std::path::PathBuf;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub fn cache_path(paths: &GlobalPaths, harness: &str) -> Option<PathBuf> {
    match harness {
        "codex" | "copilot" | "antigravity-cli" | "claude-code" => {
            Some(paths.data_dir.join("quota").join(format!("{harness}.json")))
        }
        _ => None,
    }
}

pub fn status_line_snapshot(harness: &str, payload: &Value) -> Option<Value> {
    let mut windows = Vec::new();
    let mut credits = None;
    match harness {
        "claude-code" => {
            for (key, minutes) in [("five_hour", 300), ("seven_day", 10080)] {
                let window = &payload["rate_limits"][key];
                if let Some(used) = percentage(&window["used_percentage"]) {
                    windows.push(json!({"remaining": 100.0 - used, "minutes": minutes, "resets_at": window["resets_at"].as_i64()}));
                }
            }
        }
        "antigravity-cli" => {
            for (key, bucket) in payload["quota"].as_object()? {
                if bucket["disabled"] == true {
                    continue;
                }
                let label = match key.as_str() {
                    "gemini-5h" | "gemini-weekly" => "Gemini",
                    "3p-5h" | "3p-weekly" => "Partner models",
                    _ => key,
                };
                let minutes = if key.ends_with("-5h") {
                    Some(300)
                } else if key.ends_with("-weekly") {
                    Some(10080)
                } else {
                    None
                };
                if let Some(fraction) = bucket["remaining_fraction"]
                    .as_f64()
                    .filter(|n| (0.0..=1.0).contains(n))
                {
                    let reset = bucket["reset_time"]
                        .as_str()
                        .and_then(|time| OffsetDateTime::parse(time, &Rfc3339).ok())
                        .map(|time| time.unix_timestamp());
                    windows.push(json!({"label": label, "remaining": fraction * 100.0, "minutes": minutes, "resets_at": reset}));
                } else if key == "credits" {
                    credits = bucket["remaining_amount"].as_f64().filter(|n| *n >= 0.0);
                }
            }
        }
        _ => return None,
    }
    if windows.is_empty() && credits.is_none() {
        return None;
    }
    Some(
        json!({"state": "ready", "windows": windows, "credits": credits, "observed_at": util::now()}),
    )
}

fn percentage(value: &Value) -> Option<f64> {
    value.as_f64().filter(|p| (0.0..=100.0).contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_line_keeps_only_quota_and_accepts_zero_remaining() {
        let snapshot = status_line_snapshot(
            "antigravity-cli",
            &json!({
                "email": "redacted@example.com",
                "quota": {
                    "gemini-5h": {"remaining_fraction": 0.0, "reset_time": "2026-07-06T07:50:32Z"},
                    "3p-weekly": {"remaining_fraction": 0.4, "disabled": true}
                }
            }),
        )
        .unwrap();
        assert_eq!(snapshot["windows"][0]["remaining"], 0.0);
        assert_eq!(snapshot["windows"][0]["resets_at"], 1783324232);
        assert_eq!(snapshot["windows"].as_array().unwrap().len(), 1);
        assert!(!snapshot.to_string().contains("redacted"));
        assert!(status_line_snapshot("claude-code", &json!({})).is_none());
        let snapshot = status_line_snapshot(
            "claude-code",
            &json!({
                "rate_limits": {"five_hour": {"used_percentage": 23.5, "resets_at": 1234}}
            }),
        )
        .unwrap();
        assert_eq!(snapshot["windows"][0]["remaining"], 76.5);
    }
}
