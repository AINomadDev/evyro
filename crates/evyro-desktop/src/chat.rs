//! Chat threads: coding agents over ACP (Agent Client Protocol), shown as a
//! conversation instead of a terminal. The agent's reply, plan, tool calls and
//! permission requests arrive as "chat-event" events; the window answers
//! permission requests with `chat_permission`. Each chat runs under an Evyro
//! session (task or handoff claimed, checkpoint after each turn and at the end)
//! and gets Evyro's MCP server. The chats themselves live in
//! `evyro_acp::host`, which `evyro serve` uses too.

use crate::CmdResult;
use evyro_acp::host::{Change, Host, OpenOpts};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Emitter, State};

pub struct ChatState {
    pub host: Host,
    /// The same changes, for `evyro serve` running inside the app.
    pub feed: Arc<evyro_remote::Feed>,
    app: Arc<OnceLock<AppHandle>>,
}

pub type Chats = Arc<ChatState>;

fn bad(msg: impl std::fmt::Display) -> Value {
    json!({"code": "chat", "message": msg.to_string()})
}

/// The `evyro` CLI next to the app, for the MCP server the agent gets.
fn evyro_mcp() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "evyro.exe" } else { "evyro" };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .filter(|p| p.is_file())
        .or_else(|| evyro_core::adapters::which(exe))
}

/// The chats of this window. Events go to the window once `attach` has run.
pub fn new_chats() -> Chats {
    let app: Arc<OnceLock<AppHandle>> = Default::default();
    let a2 = app.clone();
    let feed = evyro_remote::Feed::new();
    let f2 = feed.clone();
    let notify: evyro_acp::host::Notify = Arc::new(move |c: Change| {
        evyro_remote::feed_change(&f2, &c);
        let Some(app) = a2.get() else { return };
        match c {
            Change::Item { id, item, .. } => {
                let _ = app.emit("chat-event", json!({"id": id, "item": item}));
            }
            Change::State(info) => {
                let _ = app.emit("chat-state", info);
            }
        }
    });
    let mut host = Host::new(notify, evyro_mcp());
    if let Ok(paths) = evyro_core::paths::GlobalPaths::resolve() {
        host = host
            .with_retention(evyro_core::retention::Retention::load(&paths).chats)
            .with_sealer(evyro_core::at_rest::Sealer::from_settings(&paths))
            .with_archive(paths.data_dir.join(evyro_core::at_rest::CHATS_DIR));
    }
    Arc::new(ChatState { host, feed, app })
}

pub fn attach(chats: &Chats, app: AppHandle) {
    let _ = chats.app.set(app);
}

/// Agents that can chat here, with the command Evyro would start.
#[tauri::command]
pub async fn acp_agents() -> CmdResult {
    tauri::async_runtime::spawn_blocking(|| json!(evyro_acp::agent_status()))
        .await
        .map_err(bad)
}

#[tauri::command]
pub async fn chat_open(
    chats: State<'_, Chats>,
    project_id: String,
    target: String,
    task: Option<String>,
    handoff_id: Option<String>,
    accept_drift: Option<bool>,
) -> CmdResult {
    Ok(chats.host.open(OpenOpts {
        project_id,
        target,
        task,
        handoff: handoff_id,
        accept_drift: accept_drift.unwrap_or(false),
        started_by: "desktop".into(),
    }))
}

#[tauri::command]
pub async fn chat_list(chats: State<'_, Chats>) -> CmdResult {
    Ok(json!(chats.host.list_with_history()))
}

#[tauri::command]
pub async fn chat_search_saved(
    chats: State<'_, Chats>,
    project_id: String,
    query: String,
) -> CmdResult {
    let host = chats.host.clone();
    tauri::async_runtime::spawn_blocking(move || {
        host.search_saved(&project_id, &query)
            .map(|hits| json!(hits))
            .map_err(bad)
    })
    .await
    .map_err(bad)?
}

#[tauri::command]
pub async fn chat_log(chats: State<'_, Chats>, id: String) -> CmdResult {
    let v = chats
        .host
        .log(&id, None)
        .or_else(|e| chats.host.saved_log(&id).ok_or(e))
        .map_err(bad)?;
    Ok(json!({"info": v["info"], "log": v["log"], "draft": v["draft"]}))
}

#[tauri::command]
pub async fn chat_set_model(chats: State<'_, Chats>, id: String, model: String) -> CmdResult {
    let host = chats.host.clone();
    tauri::async_runtime::spawn_blocking(move || host.set_model(&id, &model))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub async fn chat_send(chats: State<'_, Chats>, id: String, text: String) -> CmdResult {
    chats.host.send(&id, &text).map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub async fn chat_permission(
    chats: State<'_, Chats>,
    id: String,
    request: Value,
    option_id: Option<String>,
) -> CmdResult {
    chats
        .host
        .answer(&id, &request, option_id.as_deref(), "desktop")
        .map_err(bad)?;
    Ok(Value::Null)
}

#[tauri::command]
pub async fn chat_cancel(chats: State<'_, Chats>, id: String) -> CmdResult {
    chats.host.cancel(&id).map_err(bad)?;
    Ok(Value::Null)
}

/// Stop a live chat, or move an already saved transcript to the trash (see `undo`).
#[tauri::command]
pub async fn chat_close(chats: State<'_, Chats>, id: String, forget: Option<bool>) -> CmdResult {
    let host = chats.host.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if forget == Some(true) {
            // The transcript goes to the trash, so the window can offer to put it back.
            let del = crate::undo::Deleting::start()?;
            host.forget_saved(&id, Some(&del.chats())).map_err(bad)?;
            Ok(json!({"undo": del.finish()}))
        } else {
            host.close(&id).map_err(bad).map(|()| Value::Null)
        }
    })
    .await
    .map_err(bad)?
}

/// When the app quits: stop every agent and close its session.
pub fn close_all(chats: &Chats) {
    chats.host.close_all();
}
