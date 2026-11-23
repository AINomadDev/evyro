//! Chat threads: coding agents over ACP (Agent Client Protocol), shown as a
//! conversation instead of a terminal. The agent's reply, plan, tool calls and
//! permission requests arrive as "chat-event" events; the window answers
//! permission requests with `chat_permission`. Each chat runs under an Evyro
//! session (task or handoff claimed, checkpoint after each turn and at the end)
//! and gets Evyro's MCP server.

use crate::{err, CmdResult};
use evyro_acp::{ChatOpts, Event, Started};
use evyro_core::Store;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

/// Transcript entries kept per chat, so the window can redraw after a screen change.
const MAX_LOG: usize = 4000;

pub struct Entry {
    info: Value,
    chat: Option<Arc<evyro_acp::Chat>>,
    log: Vec<Value>,
}

#[derive(Default)]
pub struct ChatsInner {
    map: HashMap<String, Entry>,
    next: u64,
}

pub type Chats = Arc<Mutex<ChatsInner>>;

fn bad(msg: impl std::fmt::Display) -> Value {
    json!({"code": "chat", "message": msg.to_string()})
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

/// Add an entry to a chat's transcript and send it to the window.
fn push(app: &AppHandle, chats: &Chats, id: &str, item: Value) {
    if let Ok(mut c) = chats.lock() {
        if let Some(e) = c.map.get_mut(id) {
            e.log.push(item.clone());
            if e.log.len() > MAX_LOG {
                let cut = e.log.len() - MAX_LOG;
                e.log.drain(..cut);
            }
        }
    }
    let _ = app.emit("chat-event", json!({"id": id, "item": item}));
}

/// Change fields of a chat's info and tell the window.
fn set_info(app: &AppHandle, chats: &Chats, id: &str, patch: Value) {
    let info = {
        let Ok(mut c) = chats.lock() else { return };
        let Some(e) = c.map.get_mut(id) else { return };
        if let (Some(o), Some(p)) = (e.info.as_object_mut(), patch.as_object()) {
            for (k, v) in p {
                o.insert(k.clone(), v.clone());
            }
        }
        e.info.clone()
    };
    let _ = app.emit("chat-state", info);
}

fn item_for(e: &Event) -> Option<Value> {
    Some(match e {
        Event::Update { update, .. } => json!({"t": "update", "update": update}),
        Event::Permission { request_id, params } => {
            json!({"t": "permission", "request": request_id, "params": params})
        }
        Event::FileWrite { path, bytes } => {
            json!({"t": "file", "op": "write", "path": path, "bytes": bytes})
        }
        Event::FileRead { path } => json!({"t": "file", "op": "read", "path": path}),
        Event::FileRefused { path, reason } => {
            json!({"t": "file", "op": "refused", "path": path, "reason": reason})
        }
        Event::Exit(code) => json!({"t": "exit", "code": code}),
        Event::Stderr(_) => return None,
    })
}

fn chat_of(chats: &Chats, id: &str) -> Result<Arc<evyro_acp::Chat>, Value> {
    let c = chats.lock().map_err(bad)?;
    let e = c.map.get(id).ok_or_else(|| bad("no such chat"))?;
    e.chat
        .clone()
        .ok_or_else(|| bad("the chat is still starting"))
}

/// Send a message on a background thread; the turn's progress arrives as events.
fn run_turn(
    app: AppHandle,
    chats: Chats,
    id: String,
    chat: Arc<evyro_acp::Chat>,
    text: String,
    shown: Value,
) {
    push(&app, &chats, &id, json!({"t": "user", "text": shown}));
    set_info(&app, &chats, &id, json!({"busy": true}));
    std::thread::spawn(move || {
        let r = chat.send(&text);
        match &r {
            Ok(reason) => push(
                &app,
                &chats,
                &id,
                json!({"t": "turn_end", "reason": reason}),
            ),
            Err(e) => push(
                &app,
                &chats,
                &id,
                json!({"t": "error", "message": e.to_string()}),
            ),
        }
        if let Ok(store) = Store::open_default() {
            let _ = chat.turn_checkpoint(&store);
        }
        let alive = chat.client.is_alive();
        set_info(&app, &chats, &id, json!({"busy": false, "running": alive}));
    });
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
    app: AppHandle,
    chats: State<'_, Chats>,
    project_id: String,
    target: String,
    task: Option<String>,
    handoff_id: Option<String>,
    accept_drift: Option<bool>,
) -> CmdResult {
    let chats: Chats = chats.inner().clone();
    let id = {
        let mut c = chats.lock().map_err(bad)?;
        c.next += 1;
        let id = format!("chat_{}", c.next);
        let title = match task.as_deref().filter(|t| !t.is_empty()) {
            Some(t) => format!("{target} on {t}"),
            None => target.clone(),
        };
        let info = json!({
            "id": id, "kind": "chat", "title": title, "target": target, "task": task,
            "project_id": project_id, "started_at": now(), "running": true, "busy": true,
            "state": "starting", "exit_code": null, "command": "ACP chat",
        });
        c.map.insert(
            id.clone(),
            Entry {
                info: info.clone(),
                chat: None,
                log: vec![],
            },
        );
        id
    };
    let info = chats.lock().map_err(bad)?.map[&id].info.clone();
    let (app2, chats2, id2) = (app.clone(), chats.clone(), id.clone());
    std::thread::spawn(move || {
        let fail = |msg: String| {
            push(&app2, &chats2, &id2, json!({"t": "error", "message": msg}));
            set_info(
                &app2,
                &chats2,
                &id2,
                json!({"busy": false, "running": false, "state": "failed"}),
            );
        };
        let store = match Store::open_default() {
            Ok(s) => s,
            Err(e) => return fail(e.to_string()),
        };
        let p = match store.get_project(&project_id) {
            Ok(p) => p,
            Err(e) => return fail(e.to_string()),
        };
        let (a3, c3, i3) = (app2.clone(), chats2.clone(), id2.clone());
        let on_event: evyro_acp::OnEvent = Arc::new(move |e: Event| {
            if let Event::Exit(code) = &e {
                set_info(
                    &a3,
                    &c3,
                    &i3,
                    json!({"running": false, "busy": false, "exit_code": code}),
                );
            }
            if let Some(item) = item_for(&e) {
                push(&a3, &c3, &i3, item);
            }
        });
        let started = evyro_acp::start(
            &store,
            &p,
            ChatOpts {
                target: &target,
                task: task.as_deref().filter(|t| !t.is_empty()),
                handoff: handoff_id.as_deref().filter(|h| !h.is_empty()),
                accept_drift: accept_drift.unwrap_or(false),
                model: None,
                evyro_mcp: evyro_mcp(),
                command: None,
            },
            on_event,
        );
        let chat = match started {
            Ok(Started::Chat(c)) => Arc::new(c),
            Ok(Started::Drift(_)) => {
                return fail("The repository changed since the handoff was written. Review the changes under Handoffs, then continue from there.".into())
            }
            Err(e) => return fail(e.to_string()),
        };
        let agent = chat
            .agent_info
            .pointer("/agentInfo/title")
            .or_else(|| chat.agent_info.pointer("/agentInfo/name"))
            .and_then(|v| v.as_str())
            .unwrap_or(&chat.display_name)
            .to_string();
        let task_key = chat
            .session
            .task_id
            .as_deref()
            .and_then(|t| store.get_task(t).ok())
            .map(|t| t.key);
        if let Ok(mut c) = chats2.lock() {
            if let Some(e) = c.map.get_mut(&id2) {
                e.chat = Some(chat.clone());
            }
        }
        set_info(
            &app2,
            &chats2,
            &id2,
            json!({"state": "ready", "agent": agent, "session": chat.session.id, "task": task_key, "cwd": chat.cwd.to_string_lossy()}),
        );
        let first = chat.first_prompt.clone();
        run_turn(
            app2.clone(),
            chats2.clone(),
            id2.clone(),
            chat,
            first.clone(),
            json!({"context": first}),
        );
    });
    Ok(info)
}

#[tauri::command]
pub async fn chat_list(chats: State<'_, Chats>) -> CmdResult {
    let c = chats.lock().map_err(bad)?;
    let mut v: Vec<Value> = c.map.values().map(|e| e.info.clone()).collect();
    v.sort_by_key(|i| {
        i["id"]
            .as_str()
            .and_then(|s| s.trim_start_matches("chat_").parse::<u64>().ok())
    });
    Ok(json!(v))
}

#[tauri::command]
pub async fn chat_log(chats: State<'_, Chats>, id: String) -> CmdResult {
    let c = chats.lock().map_err(bad)?;
    let e = c.map.get(&id).ok_or_else(|| bad("no such chat"))?;
    Ok(json!({"info": e.info, "log": e.log}))
}

#[tauri::command]
pub async fn chat_send(
    app: AppHandle,
    chats: State<'_, Chats>,
    id: String,
    text: String,
) -> CmdResult {
    let chats: Chats = chats.inner().clone();
    if text.trim().is_empty() {
        return Err(bad("write a message first"));
    }
    let chat = chat_of(&chats, &id)?;
    let busy = chats
        .lock()
        .map_err(bad)?
        .map
        .get(&id)
        .and_then(|e| e.info["busy"].as_bool())
        .unwrap_or(false);
    if busy {
        return Err(bad("the agent is still answering; wait or press Stop"));
    }
    if !chat.client.is_alive() {
        return Err(bad("the agent has stopped; start a new chat"));
    }
    run_turn(app, chats, id, chat, text.clone(), json!(text));
    Ok(Value::Null)
}

#[tauri::command]
pub async fn chat_permission(
    app: AppHandle,
    chats: State<'_, Chats>,
    id: String,
    request: Value,
    option_id: Option<String>,
) -> CmdResult {
    let chats: Chats = chats.inner().clone();
    let chat = chat_of(&chats, &id)?;
    chat.client
        .answer_permission(&request, option_id.as_deref())
        .map_err(bad)?;
    push(
        &app,
        &chats,
        &id,
        json!({"t": "permission_answer", "request": request, "option": option_id}),
    );
    Ok(Value::Null)
}

#[tauri::command]
pub async fn chat_cancel(chats: State<'_, Chats>, id: String) -> CmdResult {
    let chats: Chats = chats.inner().clone();
    let chat = chat_of(&chats, &id)?;
    chat.cancel().map_err(bad)?;
    Ok(Value::Null)
}

/// Stop the agent, checkpoint and close its session, and forget the chat.
#[tauri::command]
pub async fn chat_close(chats: State<'_, Chats>, id: String) -> CmdResult {
    let chats: Chats = chats.inner().clone();
    let entry = chats.lock().map_err(bad)?.map.remove(&id);
    if let Some(chat) = entry.and_then(|e| e.chat) {
        tauri::async_runtime::spawn_blocking(move || {
            let store = Store::open_default().map_err(err)?;
            chat.finish(&store).map_err(err)
        })
        .await
        .map_err(bad)??;
    }
    Ok(Value::Null)
}

/// When the app quits: stop every agent and close its session.
pub fn close_all(chats: &Chats) {
    let all: Vec<Arc<evyro_acp::Chat>> = match chats.lock() {
        Ok(mut c) => c.map.drain().filter_map(|(_, e)| e.chat).collect(),
        Err(_) => return,
    };
    if all.is_empty() {
        return;
    }
    if let Ok(store) = Store::open_default() {
        for c in all {
            let _ = c.finish(&store);
        }
    } else {
        for c in all {
            c.client.shutdown();
        }
    }
}
