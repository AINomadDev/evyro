//! Agent threads: coding agents and shells running in pseudo-terminals inside
//! the window. An agent thread runs `evyro run --to <target>`, so it gets the
//! same session tracking, checkpoints and handoff claim as a run from a
//! terminal.

use crate::{with_store, CmdResult};
use evyro_core::{brand, targets};
use evyro_term::{shell_command, Spawn, Terminals};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

pub type Terms = Arc<Terminals>;

fn bad(e: impl std::fmt::Display) -> Value {
    json!({"code": "terminal", "message": e.to_string()})
}

/// The `evyro` CLI next to the app binary if it is there, otherwise the name,
/// left for the login shell's PATH to find.
fn evyro_cli() -> String {
    let exe = if cfg!(windows) { "evyro.exe" } else { "evyro" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| brand::CLI_NAME.to_string())
}

fn quote(s: &str, windows: bool) -> String {
    if windows {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// The command line an agent thread runs.
pub fn agent_line(
    cli: &str,
    target: &str,
    task: Option<&str>,
    handoff: Option<&str>,
    windows: bool,
) -> String {
    let q = |s: &str| quote(s, windows);
    let mut line = format!("{} run --to {}", q(cli), q(target));
    if let Some(t) = task.filter(|t| !t.is_empty()) {
        line.push_str(&format!(" --task {}", q(t)));
    }
    if let Some(h) = handoff.filter(|h| !h.is_empty()) {
        line.push_str(&format!(" --handoff {}", q(h)));
    }
    line
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn term_open(
    app: AppHandle,
    terms: State<'_, Terms>,
    project_id: String,
    kind: String,
    target: Option<String>,
    task: Option<String>,
    handoff_id: Option<String>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CmdResult {
    let t2 = target.clone();
    let resolved = with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let name = match t2.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => Some(targets::resolve_target(&s.paths, t)?.name),
            None => None,
        };
        Ok(json!({"root": p.root().to_string_lossy(), "project_id": p.id, "target": name}))
    })
    .await?;
    let root = PathBuf::from(resolved["root"].as_str().unwrap_or("."));
    let pid = resolved["project_id"].as_str().map(String::from);
    let target = resolved["target"].as_str().map(String::from);

    let (program, args, title) = if kind == "agent" {
        let Some(t) = target.as_deref() else {
            return Err(json!({"code": "invalid_input", "message": "choose an AI to start"}));
        };
        let line = agent_line(
            &evyro_cli(),
            t,
            task.as_deref(),
            handoff_id.as_deref(),
            cfg!(windows),
        );
        let (p, a) = shell_command(Some(&line));
        let title = match task.as_deref().filter(|k| !k.is_empty()) {
            Some(k) => format!("{t} on {k}"),
            None => t.to_string(),
        };
        (p, a, title)
    } else {
        let (p, a) = shell_command(None);
        (p, a, "Shell".to_string())
    };

    let spec = Spawn {
        program,
        args,
        cwd: root,
        env: vec![],
        cols: cols.unwrap_or(100),
        rows: rows.unwrap_or(30),
        title,
        kind,
        target,
        task: task.filter(|k| !k.is_empty()),
        project_id: pid,
    };
    let (a1, a2) = (app.clone(), app);
    let info = terms
        .open(
            spec,
            Arc::new(move |id, data, end| {
                let _ = a1.emit("term-output", json!({"id": id, "data": data, "end": end}));
            }),
            Arc::new(move |id, code| {
                let _ = a2.emit("term-exit", json!({"id": id, "code": code}));
            }),
        )
        .map_err(bad)?;
    Ok(json!(info))
}

#[tauri::command]
pub fn term_list(terms: State<'_, Terms>) -> CmdResult {
    Ok(json!(terms.list()))
}

#[tauri::command]
pub fn term_write(terms: State<'_, Terms>, id: String, data: String) -> CmdResult {
    terms.write(&id, &data).map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub fn term_resize(terms: State<'_, Terms>, id: String, cols: u16, rows: u16) -> CmdResult {
    terms.resize(&id, cols, rows).map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub fn term_snapshot(terms: State<'_, Terms>, id: String) -> CmdResult {
    let (data, end) = terms.snapshot(&id).map_err(bad)?;
    Ok(json!({"data": data, "end": end}))
}

#[tauri::command]
pub fn term_close(terms: State<'_, Terms>, id: String) -> CmdResult {
    terms.close(&id).map_err(bad)?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_line_quotes_for_each_shell() {
        assert_eq!(
            agent_line("/opt/my apps/evyro", "codex", Some("T-1"), None, false),
            "'/opt/my apps/evyro' run --to 'codex' --task 'T-1'"
        );
        assert_eq!(
            agent_line("evyro", "it's", None, Some("hof_1"), false),
            "'evyro' run --to 'it'\\''s' --handoff 'hof_1'"
        );
        assert_eq!(
            agent_line("C:\\Evyro\\evyro.exe", "codex", Some(""), None, true),
            "\"C:\\Evyro\\evyro.exe\" run --to \"codex\""
        );
    }
}
