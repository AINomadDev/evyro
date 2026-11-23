//! Skills library: reusable, versioned instructions for agents.
//!
//! A skill is a folder holding a `SKILL.md`: YAML-style frontmatter (`name`,
//! `description`, `version`, optional `tags`, `harnesses`, `requires`)
//! followed by Markdown. Skills come from three places, and a later one
//! replaces an earlier one with the same name:
//!
//! 1. built in (shipped inside Evyro);
//! 2. global (`<config dir>/skills/<name>/SKILL.md`), shared by every project;
//! 3. project (`<repo>/.evyro/skills/<name>/SKILL.md`), kept with the code.
//!
//! Skills are disclosed progressively: an agent sees names and one-line
//! descriptions, and loads a skill's text only when it needs it. Evyro never
//! overwrites instructions a person wrote; see [`sync`].

use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use crate::projects::Project;
use crate::{brand, redact, util, Store};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const MAX_SKILL_BYTES: u64 = 64 * 1024;
const GLOBAL_SCOPE: &str = "*";
/// Marks a file that `evyro skills sync` wrote, so a later sync may replace it.
pub const MANAGED_MARK: &str =
    "<!-- Managed by Evyro. Edit the skill with `evyro skills`, then run `evyro skills sync` again. -->";
pub const BLOCK_START: &str = "<!-- evyro:skills:start -->";
pub const BLOCK_END: &str = "<!-- evyro:skills:end -->";

const BUILTIN: &[(&str, &str)] = &[
    (
        "agent-handoff",
        include_str!("../skills/agent-handoff/SKILL.md"),
    ),
    (
        "architecture-design",
        include_str!("../skills/architecture-design/SKILL.md"),
    ),
    (
        "code-review",
        include_str!("../skills/code-review/SKILL.md"),
    ),
    (
        "context-checkpointing",
        include_str!("../skills/context-checkpointing/SKILL.md"),
    ),
    ("debugging", include_str!("../skills/debugging/SKILL.md")),
    (
        "dependency-updates",
        include_str!("../skills/dependency-updates/SKILL.md"),
    ),
    (
        "documentation",
        include_str!("../skills/documentation/SKILL.md"),
    ),
    (
        "implementation-planning",
        include_str!("../skills/implementation-planning/SKILL.md"),
    ),
    (
        "performance-optimization",
        include_str!("../skills/performance-optimization/SKILL.md"),
    ),
    ("recovery", include_str!("../skills/recovery/SKILL.md")),
    (
        "refactoring",
        include_str!("../skills/refactoring/SKILL.md"),
    ),
    (
        "release-preparation",
        include_str!("../skills/release-preparation/SKILL.md"),
    ),
    (
        "requirements-analysis",
        include_str!("../skills/requirements-analysis/SKILL.md"),
    ),
    (
        "security-review",
        include_str!("../skills/security-review/SKILL.md"),
    ),
    (
        "test-driven-development",
        include_str!("../skills/test-driven-development/SKILL.md"),
    ),
];

/// Where each harness reads its instructions, for `sync`.
pub const SYNC_TARGETS: &[(&str, &str)] = &[
    ("claude-code", ".claude/skills/<name>/SKILL.md"),
    ("codex", "AGENTS.md"),
    ("opencode", "AGENTS.md"),
    ("goose", "AGENTS.md"),
    ("cursor", "AGENTS.md"),
    ("amp", "AGENTS.md"),
    ("aider", "AGENTS.md"),
    ("gemini", "GEMINI.md"),
    ("copilot", ".github/copilot-instructions.md"),
];

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tags: Vec<String>,
    /// Harnesses this skill is meant for; empty means every harness.
    pub harnesses: Vec<String>,
    /// Evyro version requirement, such as `>=0.1.0`.
    pub requires: Option<String>,
    /// `builtin`, `global` or `project`.
    pub source: String,
    /// The file it was read from (none for built-in skills).
    pub path: Option<String>,
    /// The source this one replaces, when a skill with the same name exists there.
    pub overrides: Option<String>,
    pub enabled: bool,
    /// Where the on/off choice comes from: `default`, `global` or `project`.
    pub enabled_by: String,
    /// Why the skill cannot be used. Empty when it is fine.
    pub problems: Vec<String>,
    pub sha256: String,
    #[serde(skip)]
    pub raw: String,
}

impl Skill {
    pub fn usable(&self) -> bool {
        self.problems.is_empty()
    }
    pub fn supports(&self, harness: &str) -> bool {
        self.harnesses.is_empty() || self.harnesses.iter().any(|h| h == harness)
    }
    /// The Markdown after the frontmatter.
    pub fn body(&self) -> String {
        parse(&self.raw).map(|p| p.body).unwrap_or_default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct Parsed {
    pub name: String,
    pub description: String,
    pub version: String,
    pub tags: Vec<String>,
    pub harnesses: Vec<String>,
    pub requires: Option<String>,
    pub body: String,
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

fn list(v: &str) -> Vec<String> {
    let v = v.trim();
    let inner = v
        .strip_prefix('[')
        .and_then(|x| x.strip_suffix(']'))
        .unwrap_or(v);
    inner
        .split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Read a SKILL.md: frontmatter between `---` lines, then the body.
pub fn parse(raw: &str) -> Result<Parsed> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    // split_inclusive keeps byte offsets right for both \n and \r\n files.
    let mut lines = raw.split_inclusive('\n');
    if lines.next().map(str::trim) != Some("---") {
        return Err(Error::Invalid(
            "a skill starts with frontmatter between `---` lines".into(),
        ));
    }
    let mut p = Parsed::default();
    let mut closed = false;
    let mut current: Option<String> = None;
    let mut consumed = raw.find('\n').map(|i| i + 1).unwrap_or(raw.len());
    for line in lines {
        consumed += line.len();
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim() == "---" {
            closed = true;
            break;
        }
        if let Some(item) = line.trim_start().strip_prefix("- ") {
            if let Some(k) = &current {
                let item = unquote(item);
                match k.as_str() {
                    "tags" => p.tags.push(item),
                    "harnesses" => p.harnesses.push(item),
                    _ => {}
                }
            }
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        current = Some(k.clone());
        match k.as_str() {
            "name" => p.name = unquote(v),
            "description" => p.description = unquote(v),
            "version" => p.version = unquote(v),
            "tags" => p.tags = list(v),
            "harnesses" => p.harnesses = list(v),
            "requires" => p.requires = Some(unquote(v)).filter(|s| !s.is_empty()),
            _ => {}
        }
    }
    if !closed {
        return Err(Error::Invalid(
            "the frontmatter is not closed with `---`".into(),
        ));
    }
    p.body = raw
        .get(consumed.min(raw.len())..)
        .unwrap_or("")
        .trim_start_matches(['\r', '\n'])
        .to_string();
    p.tags = p.tags.into_iter().map(|t| t.to_ascii_lowercase()).collect();
    Ok(p)
}

pub fn valid_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() <= 64
        && n.chars()
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && n.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let v = v.trim().trim_start_matches('v');
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let a = it.next()?.parse().ok()?;
    let b = it.next().unwrap_or("0").parse().ok()?;
    let c = it.next().unwrap_or("0").parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((a, b, c))
}

/// Compare two versions; None when either is not a version.
pub fn compare_versions(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    Some(parse_version(a)?.cmp(&parse_version(b)?))
}

/// Does `have` meet a requirement like `>=0.1.0, <2`? A bare version means `>=`.
pub fn satisfies(req: &str, have: &str) -> Option<bool> {
    let have = parse_version(have)?;
    for clause in req.split(',').map(str::trim).filter(|c| !c.is_empty()) {
        let (op, v) = ["<=", ">=", "<", ">", "="]
            .iter()
            .find_map(|op| clause.strip_prefix(op).map(|r| (*op, r)))
            .unwrap_or((">=", clause));
        let v = parse_version(v)?;
        let ok = match op {
            "<=" => have <= v,
            ">=" => have >= v,
            "<" => have < v,
            ">" => have > v,
            _ => have == v,
        };
        if !ok {
            return Some(false);
        }
    }
    Some(true)
}

/// Why a skill cannot be used. `folder` is the folder name it was found in.
pub fn check(p: &Parsed, raw: &str, folder: Option<&str>) -> Vec<String> {
    let mut out = Vec::new();
    if !valid_name(&p.name) {
        out.push(format!(
            "the name '{}' must be 1 to 64 lowercase letters, digits or hyphens",
            p.name
        ));
    }
    if let Some(f) = folder {
        if f != p.name {
            out.push(format!(
                "the folder is '{f}' but the skill is named '{}'",
                p.name
            ));
        }
    }
    if p.description.trim().is_empty() {
        out.push("it has no description".into());
    } else if p.description.len() > 1024 {
        out.push("the description is longer than 1024 characters".into());
    }
    if !p.version.is_empty() && parse_version(&p.version).is_none() {
        out.push(format!("the version '{}' is not like 1.2.0", p.version));
    }
    if raw.len() as u64 > MAX_SKILL_BYTES {
        out.push(format!("it is larger than {} KB", MAX_SKILL_BYTES / 1024));
    }
    let (_, found) = redact::redact(raw);
    if !found.is_empty() {
        let mut kinds: Vec<String> = found.iter().map(|f| f.kind.to_string()).collect();
        kinds.dedup();
        out.push(format!(
            "it contains what looks like a secret ({}), so it is not shared with agents",
            kinds.join(", ")
        ));
    }
    if let Some(req) = &p.requires {
        let req = req.trim_start_matches("evyro").trim();
        match satisfies(req, brand::VERSION) {
            Some(true) => {}
            Some(false) => out.push(format!(
                "it needs {} {req}, and this is {}",
                brand::PRODUCT_NAME,
                brand::VERSION
            )),
            None => out.push(format!("the requirement '{req}' is not understood")),
        }
    }
    out
}

pub fn global_dir(paths: &GlobalPaths) -> PathBuf {
    paths.config_dir.join("skills")
}

pub fn project_dir(p: &Project) -> PathBuf {
    p.evyro_dir().join("skills")
}

fn build(raw: String, source: &str, path: Option<PathBuf>, folder: Option<&str>) -> Skill {
    let sha = util::sha256_hex(raw.as_bytes());
    let (p, problems) = match parse(&raw) {
        Ok(p) => {
            let pr = check(&p, &raw, folder);
            (p, pr)
        }
        Err(e) => (
            Parsed {
                name: folder.unwrap_or("").to_string(),
                ..Default::default()
            },
            vec![e.to_string()],
        ),
    };
    Skill {
        name: p.name,
        description: p.description,
        version: if p.version.is_empty() {
            "0.0.0".into()
        } else {
            p.version
        },
        tags: p.tags,
        harnesses: p.harnesses,
        requires: p.requires,
        source: source.into(),
        path: path.map(|x| x.display().to_string()),
        overrides: None,
        enabled: true,
        enabled_by: "default".into(),
        problems,
        sha256: sha,
        raw,
    }
}

fn read_limited(path: &Path) -> std::io::Result<String> {
    let len = std::fs::metadata(path)?.len();
    if len > MAX_SKILL_BYTES * 4 {
        return Ok(format!(
            "---\nname: too-large\n---\n{}",
            "x".repeat(MAX_SKILL_BYTES as usize + 1)
        ));
    }
    let bytes = std::fs::read(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn load_dir(dir: &Path, source: &str) -> Vec<Skill> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let folder = e.file_name().to_string_lossy().to_string();
        if folder.starts_with('.') {
            continue;
        }
        let f = e.path().join("SKILL.md");
        if !f.is_file() {
            continue;
        }
        let raw = match read_limited(&f) {
            Ok(r) => r,
            Err(err) => format!("cannot read: {err}"),
        };
        let mut s = build(raw, source, Some(f), Some(&folder));
        if s.name.is_empty() {
            s.name = folder;
        }
        out.push(s);
    }
    out
}

pub fn builtin() -> Vec<Skill> {
    BUILTIN
        .iter()
        .map(|(n, raw)| build(raw.to_string(), "builtin", None, Some(n)))
        .collect()
}

/// Rank usable, enabled skills by how well their name and tags match `text`.
pub fn relevant<'a>(skills: &'a [Skill], text: &str, limit: usize) -> Vec<&'a Skill> {
    let words: Vec<String> = text
        .to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|w| w.len() >= 3)
        .map(String::from)
        .collect();
    // A light stemmer: "saving", "saves" and "save" all become "sav".
    let stem = |w: &str| -> String {
        let mut w = w;
        for suf in ["ing", "es", "ed", "s"] {
            if w.len() > suf.len() + 2 {
                if let Some(x) = w.strip_suffix(suf) {
                    w = x;
                    break;
                }
            }
        }
        w.strip_suffix('e')
            .filter(|x| x.len() >= 3)
            .unwrap_or(w)
            .to_string()
    };
    let words: Vec<String> = words.iter().map(|w| stem(w)).collect();
    let mut scored: Vec<(usize, &Skill)> = skills
        .iter()
        .filter(|s| s.enabled && s.usable())
        .map(|s| {
            let keys: Vec<String> = s
                .tags
                .iter()
                .map(|t| stem(t))
                .chain(s.name.split('-').map(stem))
                .collect();
            let score = words
                .iter()
                .filter(|w| keys.iter().any(|k| k == *w))
                .count();
            (score, s)
        })
        .filter(|(n, _)| *n > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    scored.into_iter().take(limit).map(|(_, s)| s).collect()
}

impl Store {
    /// Every skill visible here, one per name (project over global over built-in).
    pub fn skills(&self, project: Option<&Project>) -> Result<Vec<Skill>> {
        let mut layers = vec![builtin(), load_dir(&global_dir(&self.paths), "global")];
        if let Some(p) = project {
            layers.push(load_dir(&project_dir(p), "project"));
        }
        let mut merged: Vec<Skill> = Vec::new();
        for layer in layers {
            for s in layer {
                if let Some(i) = merged.iter().position(|m| m.name == s.name) {
                    let prev = merged.remove(i);
                    let mut s = s;
                    s.overrides = Some(prev.source);
                    merged.push(s);
                } else {
                    merged.push(s);
                }
            }
        }
        let mut st = self.conn.prepare(
            "SELECT scope, name, enabled FROM skill_settings WHERE scope = ?1 OR scope = ?2",
        )?;
        let pid = project.map(|p| p.id.as_str()).unwrap_or(GLOBAL_SCOPE);
        let rows: Vec<(String, String, bool)> = st
            .query_map(rusqlite::params![GLOBAL_SCOPE, pid], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, i64>(2)? != 0))
            })?
            .collect::<std::result::Result<_, _>>()?;
        for s in merged.iter_mut() {
            for scope in [GLOBAL_SCOPE, pid] {
                if let Some((_, _, on)) = rows.iter().find(|(sc, n, _)| sc == scope && *n == s.name)
                {
                    s.enabled = *on;
                    s.enabled_by = if scope == GLOBAL_SCOPE {
                        "global"
                    } else {
                        "project"
                    }
                    .into();
                }
            }
        }
        merged.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(merged)
    }

    pub fn skill(&self, project: Option<&Project>, name: &str) -> Result<Skill> {
        self.skills(project)?
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "skill '{name}' (see `{} skills list`)",
                    brand::CLI_NAME
                ))
            })
    }

    /// A skill's text for an agent. Refuses a skill that has problems.
    pub fn skill_for_agent(&self, project: Option<&Project>, name: &str) -> Result<Skill> {
        let s = self.skill(project, name)?;
        if !s.usable() {
            return Err(Error::Invalid(format!(
                "skill '{name}' cannot be used: {}",
                s.problems.join("; ")
            )));
        }
        if !s.enabled {
            return Err(Error::Invalid(format!("skill '{name}' is turned off here")));
        }
        Ok(s)
    }

    /// Turn a skill on or off for one project, or for every project when `project` is None.
    pub fn set_skill_enabled(
        &self,
        project: Option<&Project>,
        name: &str,
        enabled: bool,
    ) -> Result<()> {
        self.skill(project, name)?;
        let scope = project.map(|p| p.id.as_str()).unwrap_or(GLOBAL_SCOPE);
        self.conn.execute(
            "INSERT INTO skill_settings(scope, name, enabled, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(scope, name) DO UPDATE SET enabled = excluded.enabled, updated_at = excluded.updated_at",
            rusqlite::params![scope, name, enabled as i64, util::now()],
        )?;
        Ok(())
    }

    /// Forget a project's own on/off choice so the global one applies again.
    pub fn reset_skill_setting(&self, project: Option<&Project>, name: &str) -> Result<bool> {
        let scope = project.map(|p| p.id.as_str()).unwrap_or(GLOBAL_SCOPE);
        Ok(self.conn.execute(
            "DELETE FROM skill_settings WHERE scope = ?1 AND name = ?2",
            rusqlite::params![scope, name],
        )? > 0)
    }
}

fn scope_dir(store: &Store, project: Option<&Project>) -> PathBuf {
    match project {
        Some(p) => project_dir(p),
        None => global_dir(&store.paths),
    }
}

/// Start a new skill from a template, in the project or (with None) globally.
pub fn create(
    store: &Store,
    project: Option<&Project>,
    name: &str,
    description: &str,
) -> Result<PathBuf> {
    if !valid_name(name) {
        return Err(Error::Invalid(format!(
            "'{name}' is not a skill name: use lowercase letters, digits and hyphens"
        )));
    }
    let dir = scope_dir(store, project).join(name);
    let f = dir.join("SKILL.md");
    if f.exists() {
        return Err(Error::Conflict(format!("{} already exists", f.display())));
    }
    let desc = if description.trim().is_empty() {
        "Use when ... (say when an agent should load this skill)".to_string()
    } else {
        description.trim().replace('\n', " ")
    };
    let title: String = name
        .split('-')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ");
    let raw = format!(
        "---\nname: {name}\ndescription: {desc}\nversion: 0.1.0\ntags: []\n---\n# {title}\n\nWhat this skill helps an agent do, in one or two sentences.\n\n## Steps\n\n1. First step.\n2. Second step.\n\n## Checks\n\n- How the agent knows it is done.\n"
    );
    std::fs::create_dir_all(&dir)?;
    util::write_atomic(&f, raw.as_bytes())?;
    Ok(f)
}

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub imported: Vec<ImportedSkill>,
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Serialize)]
pub struct ImportedSkill {
    pub name: String,
    pub version: String,
    pub path: String,
    pub replaced: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

fn candidates(src: &Path) -> Result<Vec<(PathBuf, Option<String>)>> {
    if src.is_file() {
        return Ok(vec![(src.to_path_buf(), None)]);
    }
    if !src.is_dir() {
        return Err(Error::NotFound(format!("{}", src.display())));
    }
    if src.join("SKILL.md").is_file() {
        let folder = src.file_name().map(|f| f.to_string_lossy().to_string());
        return Ok(vec![(src.join("SKILL.md"), folder)]);
    }
    let mut out = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(src)?.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let f = e.path().join("SKILL.md");
        if f.is_file() {
            out.push((f, Some(e.file_name().to_string_lossy().to_string())));
        }
    }
    if out.is_empty() {
        return Err(Error::NotFound(format!("no SKILL.md in {}", src.display())));
    }
    Ok(out)
}

/// Copy skills (a SKILL.md, a skill folder, or a folder of skill folders) into
/// the project or the global library. Only SKILL.md is copied. A skill with
/// problems is skipped, and an installed version that is the same or newer is
/// kept unless `force`.
pub fn import(
    store: &Store,
    src: &Path,
    project: Option<&Project>,
    force: bool,
) -> Result<ImportReport> {
    let mut rep = ImportReport::default();
    let dest_root = scope_dir(store, project);
    for (file, folder) in candidates(src)? {
        let raw = read_limited(&file)?;
        let label = folder.clone().unwrap_or_else(|| file.display().to_string());
        let p = match parse(&raw) {
            Ok(p) => p,
            Err(e) => {
                rep.skipped.push(Skipped {
                    name: label,
                    reason: e.to_string(),
                });
                continue;
            }
        };
        let problems = check(&p, &raw, None);
        if !problems.is_empty() {
            rep.skipped.push(Skipped {
                name: if p.name.is_empty() {
                    label
                } else {
                    p.name.clone()
                },
                reason: problems.join("; "),
            });
            continue;
        }
        let version = if p.version.is_empty() {
            "0.0.0".to_string()
        } else {
            p.version.clone()
        };
        let dest = dest_root.join(&p.name).join("SKILL.md");
        let mut replaced = None;
        if let Ok(old) = std::fs::read_to_string(&dest) {
            if old == raw {
                rep.skipped.push(Skipped {
                    name: p.name.clone(),
                    reason: "already installed".into(),
                });
                continue;
            }
            let old_v = parse(&old).map(|o| o.version).unwrap_or_default();
            let old_v = if old_v.is_empty() {
                "0.0.0".to_string()
            } else {
                old_v
            };
            let newer = compare_versions(&version, &old_v) == Some(std::cmp::Ordering::Greater);
            if !newer && !force {
                rep.skipped.push(Skipped {
                    name: p.name.clone(),
                    reason: format!(
                        "version {old_v} is installed and this is {version}; use --force to replace it"
                    ),
                });
                continue;
            }
            replaced = Some(old_v);
        }
        std::fs::create_dir_all(dest.parent().unwrap())?;
        util::write_atomic(&dest, raw.as_bytes())?;
        rep.imported.push(ImportedSkill {
            name: p.name,
            version,
            path: dest.display().to_string(),
            replaced,
        });
    }
    Ok(rep)
}

/// Write skills to `<out>/<name>/SKILL.md`. Every skill when `names` is empty.
pub fn export(
    store: &Store,
    project: Option<&Project>,
    names: &[String],
    out: &Path,
) -> Result<Vec<PathBuf>> {
    let all = store.skills(project)?;
    let chosen: Vec<&Skill> = if names.is_empty() {
        all.iter().filter(|s| s.usable()).collect()
    } else {
        names
            .iter()
            .map(|n| {
                all.iter()
                    .find(|s| &s.name == n)
                    .ok_or_else(|| Error::NotFound(format!("skill '{n}'")))
            })
            .collect::<Result<_>>()?
    };
    let mut written = Vec::new();
    for s in chosen {
        if !s.usable() {
            return Err(Error::Invalid(format!(
                "skill '{}' cannot be exported: {}",
                s.name,
                s.problems.join("; ")
            )));
        }
        let f = out.join(&s.name).join("SKILL.md");
        std::fs::create_dir_all(f.parent().unwrap())?;
        util::write_atomic(&f, s.raw.as_bytes())?;
        written.push(f);
    }
    Ok(written)
}

#[derive(Debug, Default, Serialize)]
pub struct SyncReport {
    pub harness: String,
    pub written: Vec<String>,
    pub unchanged: Vec<String>,
    pub removed: Vec<String>,
    pub skipped: Vec<Skipped>,
}

fn index_block(skills: &[&Skill]) -> String {
    let mut s = format!(
        "{BLOCK_START}\n## Skills\n\nThese skills come from {}. Load one only when it fits the work: get its full text with the `get_skill` tool of {}'s MCP server, or run `{} skills show <name>`.\n\n",
        brand::PRODUCT_NAME,
        brand::PRODUCT_NAME,
        brand::CLI_NAME
    );
    for k in skills {
        s.push_str(&format!(
            "- **{}**: {}\n",
            k.name,
            k.description.replace('\n', " ")
        ));
    }
    s.push_str(BLOCK_END);
    s.push('\n');
    s
}

/// Write instruction references for one harness into the repository.
///
/// - `claude-code`: each skill becomes `.claude/skills/<name>/SKILL.md`.
/// - Other harnesses: a list of skills between Evyro's markers in their
///   instruction file (AGENTS.md, GEMINI.md or `.github/copilot-instructions.md`).
///
/// A file a person wrote is never changed without asking: an existing skill
/// file without Evyro's mark is kept unless `force`, and an instruction file
/// without the markers gets the list appended only with `append`.
pub fn sync(
    store: &Store,
    project: &Project,
    harness: &str,
    names: &[String],
    append: bool,
    force: bool,
) -> Result<SyncReport> {
    let Some((_, target)) = SYNC_TARGETS.iter().find(|(h, _)| *h == harness) else {
        return Err(Error::Unsupported(format!(
            "no skills target for '{harness}'; supported: {}",
            SYNC_TARGETS
                .iter()
                .map(|(h, _)| *h)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let all = store.skills(Some(project))?;
    for n in names {
        if !all.iter().any(|s| &s.name == n) {
            return Err(Error::NotFound(format!("skill '{n}'")));
        }
    }
    let mut rep = SyncReport {
        harness: harness.into(),
        ..Default::default()
    };
    let mut chosen: Vec<&Skill> = Vec::new();
    for s in &all {
        if !names.is_empty() && !names.contains(&s.name) {
            continue;
        }
        let why = if !s.usable() {
            Some(s.problems.join("; "))
        } else if !s.enabled {
            Some("turned off".to_string())
        } else if !s.supports(harness) {
            Some(format!("made for {}", s.harnesses.join(", ")))
        } else {
            None
        };
        match why {
            Some(reason) if !names.is_empty() || s.enabled => rep.skipped.push(Skipped {
                name: s.name.clone(),
                reason,
            }),
            Some(_) => {}
            None => chosen.push(s),
        }
    }
    let root = project.root();
    if harness == "claude-code" {
        let base = root.join(".claude").join("skills");
        for s in &chosen {
            let f = base.join(&s.name).join("SKILL.md");
            let content = format!("{}\n\n{MANAGED_MARK}\n", s.raw.trim_end());
            let shown = format!(".claude/skills/{}/SKILL.md", s.name);
            match std::fs::read_to_string(&f) {
                Ok(old) if old == content => {
                    rep.unchanged.push(shown);
                    continue;
                }
                Ok(old) if !old.contains(MANAGED_MARK) && !force => {
                    rep.skipped.push(Skipped {
                        name: s.name.clone(),
                        reason: format!("{shown} was written by someone else, so it was kept; use --force to replace it"),
                    });
                    continue;
                }
                _ => {}
            }
            std::fs::create_dir_all(f.parent().unwrap())?;
            util::write_atomic(&f, content.as_bytes())?;
            rep.written.push(shown);
        }
        // Remove files Evyro wrote earlier for skills that are no longer chosen.
        if names.is_empty() {
            if let Ok(rd) = std::fs::read_dir(&base) {
                for e in rd.flatten() {
                    let n = e.file_name().to_string_lossy().to_string();
                    let f = e.path().join("SKILL.md");
                    if chosen.iter().any(|s| s.name == n) {
                        continue;
                    }
                    if std::fs::read_to_string(&f).is_ok_and(|t| t.contains(MANAGED_MARK)) {
                        std::fs::remove_file(&f)?;
                        let _ = std::fs::remove_dir(e.path());
                        rep.removed.push(format!(".claude/skills/{n}/SKILL.md"));
                    }
                }
            }
        }
        return Ok(rep);
    }
    let f = root.join(target);
    let block = index_block(&chosen);
    let new = match std::fs::read_to_string(&f) {
        Err(_) => block,
        Ok(old) => match (old.find(BLOCK_START), old.find(BLOCK_END)) {
            (Some(a), Some(b)) if b > a => {
                let end = b + BLOCK_END.len();
                let end = if old[end..].starts_with('\n') {
                    end + 1
                } else {
                    end
                };
                format!("{}{}{}", &old[..a], block, &old[end..])
            }
            _ if append => {
                let sep = if old.ends_with("\n\n") || old.is_empty() {
                    ""
                } else if old.ends_with('\n') {
                    "\n"
                } else {
                    "\n\n"
                };
                format!("{old}{sep}{block}")
            }
            _ => {
                rep.skipped.push(Skipped {
                    name: (*target).to_string(),
                    reason: format!(
                        "{target} has instructions someone wrote and no Evyro section, so it was not changed. Allow adding the list at its end (`--append`) to put the skills there"
                    ),
                });
                return Ok(rep);
            }
        },
    };
    if std::fs::read_to_string(&f).ok().as_deref() == Some(new.as_str()) {
        rep.unchanged.push((*target).to_string());
    } else {
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d)?;
        }
        util::write_atomic(&f, new.as_bytes())?;
        rep.written.push((*target).to_string());
    }
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_skills_are_valid() {
        let b = builtin();
        assert_eq!(b.len(), 15);
        for s in &b {
            assert!(s.problems.is_empty(), "{}: {:?}", s.name, s.problems);
            assert!(!s.tags.is_empty(), "{} has tags", s.name);
            assert_eq!(s.version, "1.0.0");
            assert!(s.body().starts_with("# "), "{} body", s.name);
        }
    }

    #[test]
    fn parses_frontmatter_forms() {
        let p = parse("---\nname: x\ndescription: \"Use when: things\"\nversion: 1.2.0\ntags:\n  - a\n  - B\nharnesses: [codex, 'gemini']\nrequires: >=0.1.0\n---\n\n# X\nbody\n").unwrap();
        assert_eq!(p.name, "x");
        assert_eq!(p.description, "Use when: things");
        assert_eq!(p.tags, vec!["a", "b"]);
        assert_eq!(p.harnesses, vec!["codex", "gemini"]);
        assert_eq!(p.requires.as_deref(), Some(">=0.1.0"));
        assert_eq!(p.body, "# X\nbody\n");
        let crlf = parse("---\r\nname: x\r\ndescription: d\r\n---\r\n# X\r\n").unwrap();
        assert_eq!((crlf.name.as_str(), crlf.body.as_str()), ("x", "# X\r\n"));
        assert!(parse("# no frontmatter").is_err());
        assert!(parse("---\nname: x\n").is_err());
    }

    #[test]
    fn checks_problems() {
        let raw = "---\nname: Bad_Name\ndescription: \nversion: one\nrequires: >=99.0.0\n---\napi_key = sk_live_abcdefghijklmnop\n";
        let p = parse(raw).unwrap();
        let pr = check(&p, raw, Some("other"));
        let all = pr.join(" | ");
        assert!(all.contains("lowercase"), "{all}");
        assert!(all.contains("folder is 'other'"), "{all}");
        assert!(all.contains("no description"), "{all}");
        assert!(all.contains("not like 1.2.0"), "{all}");
        assert!(all.contains("needs Evyro >=99.0.0"), "{all}");
        assert!(all.contains("secret"), "{all}");
    }

    #[test]
    fn versions_compare() {
        assert_eq!(
            compare_versions("1.10.0", "1.9.9"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(satisfies(">=0.1.0, <2", "1.4.0"), Some(true));
        assert_eq!(satisfies("<1", "1.0.0"), Some(false));
        assert_eq!(satisfies("0.2", "0.1.9"), Some(false));
        assert_eq!(satisfies(">=x", "0.1.0"), None);
    }

    #[test]
    fn relevance_follows_task_words() {
        let b = builtin();
        let r = relevant(&b, "bug: the export crashes on empty input", 3);
        assert_eq!(r[0].name, "debugging");
        let r = relevant(&b, "release: prepare version 0.2 and the changelog", 3);
        assert_eq!(r[0].name, "release-preparation");
        let r = relevant(&b, "tech_debt: refactor the parser", 3);
        assert_eq!(r[0].name, "refactoring");
        assert!(relevant(&b, "zzz qqq", 3).is_empty());
    }
}
