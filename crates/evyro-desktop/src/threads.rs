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
pub(crate) fn evyro_cli() -> String {
    let exe = if cfg!(windows) { "evyro.exe" } else { "evyro" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| brand::CLI_NAME.to_string())
}

/// Environment for a thread: the folder holding `evyro_cli()` goes first on
/// PATH, so an agent can run `evyro context` and other commands by name.
fn thread_env(cli: &str) -> Vec<(String, String)> {
    let Some(dir) = std::path::Path::new(cli)
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
    else {
        return vec![];
    };
    let mut parts = vec![dir.to_path_buf()];
    if let Some(p) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&p).filter(|x| x != dir));
    }
    match std::env::join_paths(parts) {
        Ok(p) => vec![("PATH".into(), p.to_string_lossy().into_owned())],
        Err(_) => vec![],
    }
}

pub(crate) fn quote(s: &str, windows: bool) -> String {
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
    model: Option<&str>,
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
    if let Some(m) = model.filter(|m| !m.is_empty()) {
        line.push_str(&format!(" --model {}", q(m)));
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
    model: Option<String>,
    title: Option<String>,
    resume_id: Option<String>,
    credit_budget: Option<u32>,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CmdResult {
    let t2 = target.clone();
    let task_root = (kind == "agent").then(|| task.clone()).flatten();
    let resolved = with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let root = match task_root {
            Some(key) => s.task_root(&s.resolve_task(&project_id, &key)?)?,
            None => p.root(),
        };
        let target = match t2.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => Some(targets::resolve_target(&s.paths, t)?),
            None => None,
        };
        Ok(json!({"root": root.to_string_lossy(), "project_id": p.id,
            "target": target.as_ref().map(|t| &t.name), "harness": target.and_then(|t| t.effective_harness())}))
    })
    .await?;
    let root = PathBuf::from(resolved["root"].as_str().unwrap_or("."));
    let pid = resolved["project_id"].as_str().map(String::from);
    let target = resolved["target"].as_str().map(String::from);
    if credit_budget
        .is_some_and(|limit| limit < 30 || resolved["harness"] != "copilot" || kind != "agent")
    {
        return Err(bad(
            "Copilot thread budgets must be at least 30 AI credits.",
        ));
    }

    let cli = evyro_cli();
    let (program, args, default_title) = if kind == "agent" {
        let Some(t) = target.as_deref() else {
            return Err(json!({"code": "invalid_input", "message": "choose an AI to start"}));
        };
        let mut line = agent_line(
            &cli,
            t,
            task.as_deref(),
            handoff_id.as_deref(),
            model.as_deref(),
            cfg!(windows),
        );
        line.push_str(&format!(" --prompt {}", quote("", cfg!(windows))));
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

    let custom_title = title
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if custom_title
        .as_ref()
        .is_some_and(|s| s.chars().count() > 80 || s.chars().any(char::is_control))
    {
        return Err(bad(
            "Use a thread name of at most 80 characters on one line.",
        ));
    }
    let spec = Spawn {
        program,
        args,
        cwd: root,
        env: thread_env(&cli),
        cols: cols.unwrap_or(100),
        rows: rows.unwrap_or(30),
        title: custom_title.unwrap_or(default_title),
        kind,
        target,
        task: task.filter(|k| !k.is_empty()),
        project_id: pid,
        credit_budget,
    };
    let (a1, a2) = (app.clone(), app);
    let exit_terms = Arc::clone(&terms);
    let on_output: evyro_term::OnOutput = Arc::new(move |id, data, end| {
        let _ = a1.emit("term-output", json!({"id": id, "data": data, "end": end}));
    });
    let on_exit: evyro_term::OnExit = Arc::new(move |id, code| {
        let _ = a2.emit(
            "term-exit",
            json!({"id": id, "code": code, "credits_used": exit_terms.credits_used(id)}),
        );
    });
    let info = match resume_id {
        Some(id) => terms.resume(&id, spec, on_output, on_exit),
        None => terms.open(spec, on_output, on_exit),
    }
    .map_err(bad)?;
    Ok(json!(info))
}

/// Stop a running thread's program but keep the thread and its saved output.
#[tauri::command]
pub fn term_stop(terms: State<'_, Terms>, id: String) -> CmdResult {
    terms.stop(&id).map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub fn term_list(terms: State<'_, Terms>) -> CmdResult {
    let rows: Vec<_> = terms
        .list()
        .into_iter()
        .map(|t| {
            let mut row = json!(t);
            if t.credit_budget.is_some() {
                row["credits_used"] = json!(terms.credits_used(&t.id));
            }
            row
        })
        .collect();
    Ok(json!(rows))
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
pub fn term_snapshot(terms: State<'_, Terms>, id: String, attach: Option<bool>) -> CmdResult {
    if attach == Some(true) {
        let snap = terms.attach_snapshot(&id).map_err(bad)?;
        return Ok(json!({"data": snap.data, "live": snap.live, "end": snap.end}));
    }
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
    fn thread_env_puts_the_cli_folder_first_on_path() {
        let dir = std::env::temp_dir().join("evyro-bin");
        let cli = dir.join("evyro");
        let env = thread_env(&cli.to_string_lossy());
        let path = &env.iter().find(|(k, _)| k == "PATH").unwrap().1;
        let first = std::env::split_paths(path).next().unwrap();
        assert_eq!(first, dir);
        // A bare name (found on PATH by the shell) changes nothing.
        assert!(thread_env("evyro").is_empty());
    }

    #[test]
    fn agent_line_quotes_for_each_shell() {
        assert_eq!(
            agent_line(
                "/opt/my apps/evyro",
                "codex",
                Some("T-1"),
                None,
                None,
                false
            ),
            "'/opt/my apps/evyro' run --to 'codex' --task 'T-1'"
        );
        assert_eq!(
            agent_line("evyro", "it's", None, Some("hof_1"), None, false),
            "'evyro' run --to 'it'\\''s' --handoff 'hof_1'"
        );
        assert_eq!(
            agent_line("C:\\Evyro\\evyro.exe", "codex", Some(""), None, None, true),
            "\"C:\\Evyro\\evyro.exe\" run --to \"codex\""
        );
        assert_eq!(
            agent_line(
                "evyro",
                "claude-code",
                Some("T-2"),
                Some("hnd_1"),
                Some("sonnet"),
                false
            ),
            "'evyro' run --to 'claude-code' --task 'T-2' --handoff 'hnd_1' --model 'sonnet'"
        );
    }
}
