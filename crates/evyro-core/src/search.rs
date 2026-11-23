//! Code search: fuzzy file names, text or regex search, and symbol outlines.
//!
//! Works on the files Git tracks or would track (so `.gitignore` is honoured),
//! never on sensitive paths, binaries or very large files. Matched text is
//! redacted before it leaves this module. Symbols come from per-language
//! patterns: fast and dependency-free, not a full parser.

use crate::error::{Error, Result};
use crate::{git, redact};
use regex::{Regex, RegexBuilder};
use serde::Serialize;
use std::path::Path;
use std::sync::OnceLock;

/// Files larger than this are not searched or opened.
pub const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_LINE_CHARS: usize = 300;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileHit {
    pub path: String,
    pub score: i64,
    /// Character positions in `path` that matched, for highlighting.
    pub matched: Vec<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TextHit {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Symbol {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TextResult {
    pub hits: Vec<TextHit>,
    pub files_searched: usize,
    pub truncated: bool,
}

/// Searchable files: tracked or new, not ignored, not sensitive, not Evyro's own.
pub fn searchable_files(root: &Path) -> Result<Vec<String>> {
    Ok(git::ls_files(root)?
        .into_iter()
        .filter(|p| !p.starts_with(".evyro/") && !redact::is_sensitive_path(p))
        .collect())
}

/// Read a repository file as text for search or viewing. `None` when it is
/// missing, sensitive, binary or too large.
pub fn read_text(root: &Path, rel: &str) -> Option<String> {
    let rel = safe_rel(rel).ok()?;
    if redact::is_sensitive_path(&rel) {
        return None;
    }
    let full = root.join(&rel);
    let meta = std::fs::metadata(&full).ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(&full).ok()?;
    if bytes.iter().take(8000).any(|b| *b == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// A repository-relative path that cannot leave the repository.
pub fn safe_rel(rel: &str) -> Result<String> {
    let p = rel.trim().trim_start_matches("./").replace('\\', "/");
    if p.is_empty() || p.starts_with('/') || p.contains(':') || p.split('/').any(|c| c == "..") {
        return Err(Error::Invalid(format!(
            "`{rel}` is not a path inside the repository"
        )));
    }
    Ok(p)
}

// ---------- fuzzy file finder ----------

/// Score `query` against `path` as an in-order subsequence (case-insensitive).
/// Higher is better; `None` when not all query characters appear in order.
/// Bonuses for consecutive matches, matches at word starts and in the file
/// name; a small penalty for long paths.
pub fn fuzzy_score(query: &str, path: &str) -> Option<(i64, Vec<usize>)> {
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect();
    if q.is_empty() {
        return Some((0, vec![]));
    }
    let chars: Vec<char> = path.chars().collect();
    let lower: Vec<char> = chars
        .iter()
        .map(|c| c.to_lowercase().next().unwrap_or(*c))
        .collect();
    let name_start = path
        .rfind('/')
        .map(|i| path[..=i].chars().count())
        .unwrap_or(0);
    // Match from the end, so the query lands in the file name and the
    // deepest folders where it can ("ratelim" takes "rate_limiter.rs", not
    // the "r" of "src/").
    let mut matched = Vec::with_capacity(q.len());
    let mut qi = q.len();
    for i in (0..lower.len()).rev() {
        if qi == 0 {
            break;
        }
        if lower[i] == q[qi - 1] {
            matched.push(i);
            qi -= 1;
        }
    }
    if qi > 0 {
        return None;
    }
    matched.reverse();
    let mut score = 0i64;
    let mut prev: Option<usize> = None;
    for &i in &matched {
        let mut s = 1;
        if prev == Some(i.wrapping_sub(1)) {
            s += 5;
        }
        let boundary = i == 0
            || matches!(chars[i - 1], '/' | '_' | '-' | '.' | ' ')
            || (chars[i].is_uppercase() && chars[i - 1].is_lowercase());
        if boundary {
            s += 4;
        }
        if i >= name_start {
            s += 2;
        }
        score += s;
        prev = Some(i);
    }
    score -= (chars.len() as i64) / 8;
    Some((score, matched))
}

pub fn find_files(root: &Path, query: &str, limit: usize) -> Result<Vec<FileHit>> {
    let mut hits: Vec<FileHit> = searchable_files(root)?
        .into_iter()
        .filter_map(|p| {
            fuzzy_score(query, &p).map(|(score, matched)| FileHit {
                path: p,
                score,
                matched,
            })
        })
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.path.len().cmp(&b.path.len()))
            .then(a.path.cmp(&b.path))
    });
    hits.truncate(limit);
    Ok(hits)
}

// ---------- text / regex search ----------

pub struct TextQuery<'a> {
    pub pattern: &'a str,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Only files whose path contains this (plain substring), if set.
    pub path_filter: Option<&'a str>,
    pub limit: usize,
}

pub fn search_text(root: &Path, q: &TextQuery) -> Result<TextResult> {
    if q.pattern.is_empty() {
        return Err(Error::Invalid("give something to search for".into()));
    }
    let src = if q.regex {
        q.pattern.to_string()
    } else {
        regex::escape(q.pattern)
    };
    let re = RegexBuilder::new(&src)
        .case_insensitive(!q.case_sensitive)
        .size_limit(1 << 20)
        .build()
        .map_err(|e| Error::Invalid(format!("not a valid regular expression: {e}")))?;
    let mut hits = Vec::new();
    let mut files_searched = 0;
    let mut truncated = false;
    'files: for path in searchable_files(root)? {
        if let Some(f) = q.path_filter {
            if !path.contains(f) {
                continue;
            }
        }
        let Some(text) = read_text(root, &path) else {
            continue;
        };
        files_searched += 1;
        for (i, line) in text.lines().enumerate() {
            if let Some(m) = re.find(line) {
                if hits.len() >= q.limit {
                    truncated = true;
                    break 'files;
                }
                let (clean, _) = redact::redact(line);
                hits.push(TextHit {
                    path: path.clone(),
                    line: i + 1,
                    column: line[..m.start()].chars().count() + 1,
                    text: clean.chars().take(MAX_LINE_CHARS).collect(),
                });
            }
        }
    }
    Ok(TextResult {
        hits,
        files_searched,
        truncated,
    })
}

// ---------- symbols ----------

struct Lang {
    exts: &'static [&'static str],
    rules: Vec<(Regex, &'static str)>,
}

fn langs() -> &'static Vec<Lang> {
    static L: OnceLock<Vec<Lang>> = OnceLock::new();
    L.get_or_init(|| {
        let r = |p: &str, k: &'static str| (Regex::new(p).expect("symbol pattern"), k);
        vec![
            Lang {
                exts: &["rs"],
                rules: vec![
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\S+\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)", "function"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?struct\s+([A-Za-z_][A-Za-z0-9_]*)", "struct"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?enum\s+([A-Za-z_][A-Za-z0-9_]*)", "enum"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:unsafe\s+)?trait\s+([A-Za-z_][A-Za-z0-9_]*)", "trait"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?type\s+([A-Za-z_][A-Za-z0-9_]*)", "type"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+([A-Za-z_][A-Za-z0-9_]*)", "module"),
                    r(r"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static)\s+([A-Z_][A-Z0-9_]*)\s*:", "constant"),
                    r(r"^\s*impl(?:<[^>]*>)?\s+(?:[A-Za-z_][A-Za-z0-9_:<>, ]*\s+for\s+)?([A-Za-z_][A-Za-z0-9_]*)", "impl"),
                    r(r"^\s*macro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)", "macro"),
                ],
            },
            Lang {
                exts: &["ts", "tsx", "js", "jsx", "mjs", "cjs"],
                rules: vec![
                    r(r"^\s*(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s*\*?\s*([A-Za-z_$][\w$]*)", "function"),
                    r(r"^\s*(?:export\s+)?(?:default\s+)?(?:abstract\s+)?class\s+([A-Za-z_$][\w$]*)", "class"),
                    r(r"^\s*(?:export\s+)?interface\s+([A-Za-z_$][\w$]*)", "interface"),
                    r(r"^\s*(?:export\s+)?type\s+([A-Za-z_$][\w$]*)\s*(?:<[^=]*>)?\s*=", "type"),
                    r(r"^\s*(?:export\s+)?(?:const\s+)?enum\s+([A-Za-z_$][\w$]*)", "enum"),
                    r(r"^\s*(?:export\s+)?(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s+)?(?:function|\([^)]*\)\s*=>|[A-Za-z_$][\w$]*\s*=>)", "function"),
                ],
            },
            Lang {
                exts: &["py", "pyi"],
                rules: vec![
                    r(r"^\s*(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)", "function"),
                    r(r"^\s*class\s+([A-Za-z_][A-Za-z0-9_]*)", "class"),
                ],
            },
            Lang {
                exts: &["go"],
                rules: vec![
                    r(r"^func\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_]*)", "function"),
                    r(r"^type\s+([A-Za-z_][A-Za-z0-9_]*)\s+struct", "struct"),
                    r(r"^type\s+([A-Za-z_][A-Za-z0-9_]*)\s+interface", "interface"),
                    r(r"^type\s+([A-Za-z_][A-Za-z0-9_]*)", "type"),
                ],
            },
            Lang {
                exts: &["java", "kt", "kts", "cs", "scala", "swift", "dart"],
                rules: vec![
                    r(r"^\s*(?:(?:public|private|protected|internal|open|abstract|final|sealed|static|data|partial|override)\s+)*(?:class|object|record|struct|actor)\s+([A-Za-z_][A-Za-z0-9_]*)", "class"),
                    r(r"^\s*(?:(?:public|private|protected|internal|sealed)\s+)*(?:interface|protocol|trait)\s+([A-Za-z_][A-Za-z0-9_]*)", "interface"),
                    r(r"^\s*(?:(?:public|private|protected|internal)\s+)*enum\s+(?:class\s+)?([A-Za-z_][A-Za-z0-9_]*)", "enum"),
                    r(r"^\s*(?:(?:public|private|protected|internal|open|override|static|suspend|inline|mutating)\s+)*(?:fun|func|def)\s+(?:<[^>]*>\s*)?([A-Za-z_][A-Za-z0-9_]*)", "function"),
                    r(r"^\s*(?:(?:public|private|protected|internal|static|final|abstract|synchronized|async|override|virtual)\s+)+[A-Za-z_][\w<>\[\], ?]*\s+([A-Za-z_][A-Za-z0-9_]*)\s*\([^;]*$", "method"),
                ],
            },
            Lang {
                exts: &["rb"],
                rules: vec![
                    r(r"^\s*def\s+(?:self\.)?([A-Za-z_][A-Za-z0-9_?!=]*)", "function"),
                    r(r"^\s*class\s+([A-Z][A-Za-z0-9_:]*)", "class"),
                    r(r"^\s*module\s+([A-Z][A-Za-z0-9_:]*)", "module"),
                ],
            },
            Lang {
                exts: &["c", "h", "cc", "cpp", "cxx", "hpp", "hh"],
                rules: vec![
                    r(r"^(?:struct|class|union|enum(?:\s+class)?)\s+([A-Za-z_][A-Za-z0-9_]*)\s*(?:[:{]|$)", "type"),
                    r(r"^[A-Za-z_][\w\s\*&:<>,]*?\b([A-Za-z_][A-Za-z0-9_:~]*)\s*\([^;]*$", "function"),
                ],
            },
            Lang {
                exts: &["md", "markdown"],
                rules: vec![r(r"^#{1,6}\s+(.+?)\s*#*$", "heading")],
            },
        ]
    })
}

/// Outline of one file's text. Unknown languages give an empty outline.
pub fn outline_text(path: &str, text: &str) -> Vec<Symbol> {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let Some(lang) = langs().iter().find(|l| l.exts.contains(&ext.as_str())) else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut in_fence = false;
    for (i, line) in text.lines().enumerate() {
        if lang.exts[0] == "md" && line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || line.len() > 400 {
            continue;
        }
        let t = line.trim_start();
        if t.starts_with("//") || t.starts_with('*') || (t.starts_with('#') && lang.exts[0] != "md")
        {
            continue;
        }
        for (re, kind) in &lang.rules {
            if let Some(c) = re.captures(line) {
                let name = c[1].trim().to_string();
                if matches!(
                    name.as_str(),
                    "if" | "for" | "while" | "switch" | "return" | "catch" | "else" | "sizeof"
                ) {
                    break;
                }
                out.push(Symbol {
                    path: path.to_string(),
                    line: i + 1,
                    kind: kind.to_string(),
                    name,
                });
                break;
            }
        }
    }
    out
}

pub fn outline(root: &Path, rel: &str) -> Result<Vec<Symbol>> {
    let rel = safe_rel(rel)?;
    let text = read_text(root, &rel)
        .ok_or_else(|| Error::NotFound(format!("readable text file {rel}")))?;
    Ok(outline_text(&rel, &text))
}

/// Symbols across the repository whose name fuzzy-matches `query`.
pub fn find_symbols(root: &Path, query: &str, limit: usize) -> Result<Vec<Symbol>> {
    let mut scored: Vec<(i64, Symbol)> = Vec::new();
    for path in searchable_files(root)? {
        let ext = path.rsplit('.').next().unwrap_or("");
        if !langs()
            .iter()
            .any(|l| l.exts.contains(&ext) && l.exts[0] != "md")
        {
            continue;
        }
        let Some(text) = read_text(root, &path) else {
            continue;
        };
        for s in outline_text(&path, &text) {
            if let Some((score, _)) = fuzzy_score(query, &s.name) {
                // Exact and prefix matches first.
                let bonus = if s.name.eq_ignore_ascii_case(query) {
                    100
                } else if s.name.to_lowercase().starts_with(&query.to_lowercase()) {
                    40
                } else {
                    0
                };
                scored.push((score + bonus, s));
            }
        }
    }
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then(a.1.name.len().cmp(&b.1.name.len()))
            .then(a.1.path.cmp(&b.1.path))
    });
    Ok(scored.into_iter().take(limit).map(|(_, s)| s).collect())
}

/// A file's text for viewing: redacted, with its outline.
pub fn view_file(root: &Path, rel: &str) -> Result<serde_json::Value> {
    let rel = safe_rel(rel)?;
    if redact::is_sensitive_path(&rel) {
        return Err(Error::Invalid(format!(
            "{rel} may hold secrets and is not shown"
        )));
    }
    let text = read_text(root, &rel).ok_or_else(|| {
        Error::NotFound(format!(
            "readable text file {rel} (missing, binary or over 1 MB)"
        ))
    })?;
    let (clean, findings) = redact::redact(&text);
    Ok(serde_json::json!({
        "path": rel,
        "text": clean,
        "lines": clean.lines().count(),
        "redactions": findings.len(),
        "symbols": outline_text(&rel, &text),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_prefers_file_names_and_word_starts() {
        let (a, m) = fuzzy_score("mrs", "crates/core/src/main.rs").unwrap();
        assert_eq!(m.len(), 3);
        let (b, _) = fuzzy_score("mrs", "crates/more/src/x.rs").unwrap();
        assert!(a > b, "{a} vs {b}");
        assert!(fuzzy_score("zzz", "src/main.rs").is_none());
        let (c, _) = fuzzy_score("handoff", "crates/evyro-core/src/handoff.rs").unwrap();
        let (d, _) = fuzzy_score("handoff", "docs/protocols/HANDOFF_PROTOCOL.md").unwrap();
        assert!(c > 0 && d > 0);
        assert_eq!(fuzzy_score("", "a").unwrap().0, 0);
        // The match lands in the file name, not on an early folder letter.
        let (_, m) = fuzzy_score("ratelim", "src/net/rate_limiter.rs").unwrap();
        assert_eq!(m, vec![8, 9, 10, 11, 13, 14, 15]);
    }

    #[test]
    fn outlines_common_languages() {
        let rs = "pub struct Limiter {\n}\nimpl Limiter {\n    pub async fn check(&self) {}\n}\n// fn commented() {}\nenum Mode { A }\npub(crate) const MAX: u32 = 1;\n";
        let names: Vec<_> = outline_text("a.rs", rs)
            .into_iter()
            .map(|s| (s.kind, s.name, s.line))
            .collect();
        assert_eq!(
            names,
            vec![
                ("struct".into(), "Limiter".into(), 1),
                ("impl".into(), "Limiter".into(), 3),
                ("function".into(), "check".into(), 4),
                ("enum".into(), "Mode".into(), 7),
                ("constant".into(), "MAX".into(), 8),
            ]
        );
        let ts = "export class Store {}\nexport const load = async (x) => x;\ninterface Opts {}\nfunction helper() {}\n";
        let n: Vec<_> = outline_text("a.ts", ts)
            .into_iter()
            .map(|s| s.name)
            .collect();
        assert_eq!(n, ["Store", "load", "Opts", "helper"]);
        let py = "class A:\n    def run(self):\n        pass\nasync def main():\n";
        assert_eq!(outline_text("a.py", py).len(), 3);
        let go = "func (s *Server) Start() {}\ntype Config struct {}\n";
        assert_eq!(
            outline_text("a.go", go)
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>(),
            ["Start", "Config"]
        );
        let md = "# Title\n```\n# not a heading\n```\n## Part\n";
        assert_eq!(
            outline_text("a.md", md)
                .into_iter()
                .map(|s| s.name)
                .collect::<Vec<_>>(),
            ["Title", "Part"]
        );
        assert!(outline_text("a.unknown", "fn x() {}").is_empty());
    }

    #[test]
    fn paths_cannot_leave_the_repository() {
        assert!(safe_rel("../x").is_err());
        assert!(safe_rel("/etc/passwd").is_err());
        assert!(safe_rel("C:/x").is_err());
        assert_eq!(safe_rel("./src/a.rs").unwrap(), "src/a.rs");
    }
}
