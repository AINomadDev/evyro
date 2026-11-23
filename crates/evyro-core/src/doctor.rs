//! `evyro doctor`: actionable environment and state diagnostics.

use crate::db::Store;
use crate::{adapters, brand, git};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    /// ok | warn | fail
    pub status: String,
    pub detail: String,
    pub remediation: Option<String>,
}

fn c(name: &str, status: &str, detail: impl Into<String>, fix: Option<&str>) -> Check {
    Check {
        name: name.into(),
        status: status.into(),
        detail: detail.into(),
        remediation: fix.map(|s| s.into()),
    }
}

pub fn run(store: &Store, cwd: &Path) -> Vec<Check> {
    let mut out = Vec::new();
    out.push(c(
        "evyro",
        "ok",
        format!(
            "{} {} on {}/{}",
            brand::CLI_NAME,
            brand::VERSION,
            std::env::consts::OS,
            std::env::consts::ARCH
        ),
        None,
    ));
    out.push(c(
        "data directory",
        "ok",
        store.paths.data_dir.display().to_string(),
        None,
    ));
    out.push(c(
        "config directory",
        "ok",
        format!(
            "{} (switch targets: targets.toml)",
            store.paths.config_dir.display()
        ),
        None,
    ));
    match store.integrity_check() {
        Ok(v) if v.is_empty() => out.push(c(
            "database integrity",
            "ok",
            format!("schema v{}", store.schema_version().unwrap_or(0)),
            None,
        )),
        Ok(v) => out.push(c(
            "database integrity",
            "fail",
            v.join("; "),
            Some("restore from a backup: `evyro restore <file>`"),
        )),
        Err(e) => out.push(c(
            "database integrity",
            "fail",
            e.to_string(),
            Some("restore from a backup: `evyro restore <file>`"),
        )),
    }
    match git::version() {
        Ok(v) => out.push(c("git", "ok", v, None)),
        Err(_) => out.push(c(
            "git",
            "fail",
            "git not found",
            Some("install Git and make sure it is on PATH"),
        )),
    }
    let mut any_agent = false;
    for d in adapters::detect_all() {
        let (st, fix) = match d.compatibility.as_str() {
            "ok" => {
                any_agent = true;
                ("ok", None)
            }
            "unknown_version" => {
                any_agent = true;
                ("warn", Some("check the CLI runs: `<agent> --version`"))
            }
            "incompatible" => ("fail", Some("upgrade the agent CLI")),
            "fallback_only" => ("ok", None),
            _ => ("warn", None),
        };
        let detail = match (&d.version, &d.diagnostic) {
            (Some(v), None) => format!("{} {}", d.binary.clone().unwrap_or_default(), v),
            (_, Some(diag)) => diag.clone(),
            _ => d.compatibility.clone(),
        };
        let st = if d.compatibility == "not_installed" {
            "warn"
        } else {
            st
        };
        out.push(c(&format!("agent: {}", d.id), st, detail, fix));
    }
    if !any_agent {
        out.push(c(
            "agents",
            "warn",
            "no supported coding agent found on PATH",
            Some("install Claude Code, Codex, Gemini CLI or OpenCode"),
        ));
    }
    match store.list_projects(true, None) {
        Ok(ps) => {
            for p in ps {
                if !Path::new(&p.root_path).exists() {
                    out.push(c(&format!("project: {}", p.name), "fail", format!("{} is missing", p.root_path),
                        Some("if it moved, run `evyro register <new path>` there (the id in .evyro/project.toml re-links it)")));
                }
            }
        }
        Err(e) => out.push(c("projects", "fail", e.to_string(), None)),
    }
    if let Ok(p) = store.project_for_path(cwd) {
        out.push(c(
            "current project",
            "ok",
            format!("{} ({})", p.name, p.id),
            None,
        ));
        if let Err(e) = crate::projects::project_file_problem(&p.root()) {
            out.push(c(
                "project.toml",
                "fail",
                e,
                Some("fix the file; until then Evyro keeps the last commands it read"),
            ));
        }
        if p.commands.test.is_empty() {
            out.push(c(
                "test command",
                "warn",
                "none configured",
                Some("set [commands] test in .evyro/project.toml"),
            ));
        } else {
            let cmds: Vec<String> = p.commands.test.iter().map(|a| a.join(" ")).collect();
            out.push(c("test command", "ok", cmds.join("; "), None));
        }
        if let Ok(stale) = store.stale_sessions(&p.id, false) {
            if !stale.is_empty() {
                out.push(c(
                    "stale sessions",
                    "warn",
                    format!("{} session(s) whose process is gone", stale.len()),
                    Some("run `evyro recover`"),
                ));
            }
        }
    }
    out
}
