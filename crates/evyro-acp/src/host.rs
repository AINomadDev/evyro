//! Hosting several chats at once: the desktop app and `evyro serve` both keep
//! their chats here. Each chat has an info record (state, agent, task), a
//! transcript of items (the agent's updates, file access, permission requests
//! and answers), and the permission requests still waiting for a person.
//!
//! A host tells its owner about changes through a [`Notify`] callback, so the
//! desktop can forward them as window events and the server can wake its
//! event streams.

use crate::{ChatOpts, Event, Started};
use evyro_core::Store;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Transcript entries kept per chat.
const MAX_LOG: usize = 4000;

/// What changed in a host.
#[derive(Debug, Clone)]
pub enum Change {
    /// A new transcript item; `seq` counts from 0 for each chat.
    Item { id: String, seq: u64, item: Value },
    /// The chat's info record changed (the whole record).
    State(Value),
}

pub type Notify = Arc<dyn Fn(Change) + Send + Sync>;

struct Entry {
    info: Value,
    chat: Option<Arc<crate::Chat>>,
    log: Vec<Value>,
    /// Sequence number of `log[0]`.
    base: u64,
    /// Permission requests waiting for an answer: (request id, params).
    pending: Vec<(Value, Value)>,
}

#[derive(Default)]
struct Inner {
    map: HashMap<String, Entry>,
    next: u64,
}

#[derive(Clone)]
pub struct Host {
    inner: Arc<Mutex<Inner>>,
    notify: Notify,
    evyro_mcp: Option<PathBuf>,
}

/// Where to start a chat.
pub struct OpenOpts {
    pub project_id: String,
    pub target: String,
    pub task: Option<String>,
    pub handoff: Option<String>,
    pub accept_drift: bool,
    /// Who started it, for the record (`desktop`, or a remote device's name).
    pub started_by: String,
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

impl Host {
    /// `evyro_mcp` is the `evyro` program the agents get as their MCP server.
    pub fn new(notify: Notify, evyro_mcp: Option<PathBuf>) -> Self {
        Host {
            inner: Default::default(),
            notify,
            evyro_mcp,
        }
    }

    fn push(&self, id: &str, item: Value) {
        let seq = {
            let Ok(mut c) = self.inner.lock() else { return };
            let Some(e) = c.map.get_mut(id) else { return };
            match item["t"].as_str() {
                Some("permission") => e
                    .pending
                    .push((item["request"].clone(), item["params"].clone())),
                Some("permission_answer") => e.pending.retain(|(r, _)| *r != item["request"]),
                Some("turn_end") | Some("exit") | Some("error") => e.pending.clear(),
                _ => {}
            }
            e.log.push(item.clone());
            if e.log.len() > MAX_LOG {
                let cut = e.log.len() - MAX_LOG;
                e.log.drain(..cut);
                e.base += cut as u64;
            }
            e.base + e.log.len() as u64 - 1
        };
        (self.notify)(Change::Item {
            id: id.to_string(),
            seq,
            item,
        });
    }

    fn set_info(&self, id: &str, patch: Value) {
        let info = {
            let Ok(mut c) = self.inner.lock() else { return };
            let Some(e) = c.map.get_mut(id) else { return };
            if let (Some(o), Some(p)) = (e.info.as_object_mut(), patch.as_object()) {
                for (k, v) in p {
                    o.insert(k.clone(), v.clone());
                }
            }
            let pending = e.pending.len();
            if let Some(o) = e.info.as_object_mut() {
                o.insert("waiting".into(), json!(pending));
            }
            e.info.clone()
        };
        (self.notify)(Change::State(info));
    }

    fn chat_of(&self, id: &str) -> Result<Arc<crate::Chat>, String> {
        let c = self.inner.lock().map_err(|e| e.to_string())?;
        let e = c.map.get(id).ok_or("no such chat")?;
        e.chat
            .clone()
            .ok_or_else(|| "the chat is still starting".to_string())
    }

    /// Start a chat. Returns its info at once; the agent starts in the background.
    pub fn open(&self, o: OpenOpts) -> Value {
        let id = {
            let mut c = self.inner.lock().unwrap();
            c.next += 1;
            let id = format!("chat_{}", c.next);
            let title = match o.task.as_deref().filter(|t| !t.is_empty()) {
                Some(t) => format!("{} on {t}", o.target),
                None => o.target.clone(),
            };
            let info = json!({
                "id": id, "kind": "chat", "title": title, "target": o.target, "task": o.task,
                "project_id": o.project_id, "started_at": now(), "running": true, "busy": true,
                "state": "starting", "exit_code": null, "command": "ACP chat",
                "started_by": o.started_by, "waiting": 0,
            });
            c.map.insert(
                id.clone(),
                Entry {
                    info,
                    chat: None,
                    log: vec![],
                    base: 0,
                    pending: vec![],
                },
            );
            id
        };
        let info = self.inner.lock().unwrap().map[&id].info.clone();
        let host = self.clone();
        let id2 = id.clone();
        std::thread::spawn(move || host.start(id2, o));
        info
    }

    fn start(&self, id: String, o: OpenOpts) {
        let fail = |msg: String| {
            self.push(&id, json!({"t": "error", "message": msg}));
            self.set_info(
                &id,
                json!({"busy": false, "running": false, "state": "failed"}),
            );
        };
        let store = match Store::open_default() {
            Ok(s) => s,
            Err(e) => return fail(e.to_string()),
        };
        let p = match store.get_project(&o.project_id) {
            Ok(p) => p,
            Err(e) => return fail(e.to_string()),
        };
        let (h, i) = (self.clone(), id.clone());
        let on_event: crate::OnEvent = Arc::new(move |e: Event| {
            if let Some(item) = item_for(&e) {
                h.push(&i, item);
            }
            if let Event::Exit(code) = &e {
                h.set_info(
                    &i,
                    json!({"running": false, "busy": false, "exit_code": code}),
                );
            } else if matches!(e, Event::Permission { .. }) {
                h.set_info(&i, json!({}));
            }
        });
        let started = crate::start(
            &store,
            &p,
            ChatOpts {
                target: &o.target,
                task: o.task.as_deref().filter(|t| !t.is_empty()),
                handoff: o.handoff.as_deref().filter(|h| !h.is_empty()),
                accept_drift: o.accept_drift,
                model: None,
                evyro_mcp: self.evyro_mcp.clone(),
                command: None,
            },
            on_event,
        );
        let chat = match started {
            Ok(Started::Chat(c)) => Arc::new(c),
            Ok(Started::Drift(_)) => {
                return fail("The repository changed since the handoff was written. Review the changes first, then continue from there.".into())
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
        if let Ok(mut c) = self.inner.lock() {
            if let Some(e) = c.map.get_mut(&id) {
                e.chat = Some(chat.clone());
            }
        }
        self.set_info(
            &id,
            json!({"state": "ready", "agent": agent, "session": chat.session.id, "task": task_key, "cwd": chat.cwd.to_string_lossy()}),
        );
        let first = chat.first_prompt.clone();
        self.run_turn(&id, chat, first.clone(), json!({"context": first}));
    }

    fn run_turn(&self, id: &str, chat: Arc<crate::Chat>, text: String, shown: Value) {
        self.push(id, json!({"t": "user", "text": shown}));
        self.set_info(id, json!({"busy": true}));
        let host = self.clone();
        let id = id.to_string();
        std::thread::spawn(move || {
            match chat.send(&text) {
                Ok(reason) => host.push(&id, json!({"t": "turn_end", "reason": reason})),
                Err(e) => host.push(&id, json!({"t": "error", "message": e.to_string()})),
            }
            if let Ok(store) = Store::open_default() {
                let _ = chat.turn_checkpoint(&store);
            }
            let alive = chat.client.is_alive();
            host.set_info(&id, json!({"busy": false, "running": alive}));
        });
    }

    /// Every chat's info, oldest first.
    pub fn list(&self) -> Vec<Value> {
        let c = self.inner.lock().unwrap();
        let mut v: Vec<Value> = c.map.values().map(|e| e.info.clone()).collect();
        v.sort_by_key(|i| {
            i["id"]
                .as_str()
                .and_then(|s| s.trim_start_matches("chat_").parse::<u64>().ok())
        });
        v
    }

    /// A chat's info and its transcript from sequence number `after` on (all when None).
    pub fn log(&self, id: &str, after: Option<u64>) -> Result<Value, String> {
        let c = self.inner.lock().map_err(|e| e.to_string())?;
        let e = c.map.get(id).ok_or("no such chat")?;
        let from = after.map(|a| a + 1).unwrap_or(0).max(e.base);
        let skip = (from - e.base) as usize;
        let items: Vec<Value> = e.log.iter().skip(skip).cloned().collect();
        Ok(json!({
            "info": e.info,
            "log": items,
            "first_seq": from,
            "next_seq": e.base + e.log.len() as u64,
            "pending": e.pending.iter().map(|(r, p)| json!({"request": r, "params": p})).collect::<Vec<_>>(),
        }))
    }

    /// Permission requests waiting in one chat.
    pub fn pending(&self, id: &str) -> Result<Vec<(Value, Value)>, String> {
        let c = self.inner.lock().map_err(|e| e.to_string())?;
        Ok(c.map.get(id).ok_or("no such chat")?.pending.clone())
    }

    pub fn info(&self, id: &str) -> Option<Value> {
        self.inner.lock().ok()?.map.get(id).map(|e| e.info.clone())
    }

    pub fn send(&self, id: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("write a message first".into());
        }
        let chat = self.chat_of(id)?;
        let busy = self
            .info(id)
            .and_then(|i| i["busy"].as_bool())
            .unwrap_or(false);
        if busy {
            return Err("the agent is still answering; wait or stop the turn".into());
        }
        if !chat.client.is_alive() {
            return Err("the agent has stopped; start a new chat".into());
        }
        self.run_turn(id, chat, text.to_string(), json!(text));
        Ok(())
    }

    /// Answer a permission request. `by` says who answered, for the transcript.
    pub fn answer(
        &self,
        id: &str,
        request: &Value,
        option_id: Option<&str>,
        by: &str,
    ) -> Result<(), String> {
        let chat = self.chat_of(id)?;
        chat.client
            .answer_permission(request, option_id)
            .map_err(|e| e.to_string())?;
        self.push(
            id,
            json!({"t": "permission_answer", "request": request, "option": option_id, "by": by}),
        );
        self.set_info(id, json!({}));
        Ok(())
    }

    pub fn cancel(&self, id: &str) -> Result<(), String> {
        self.chat_of(id)?.cancel().map_err(|e| e.to_string())
    }

    /// Stop the agent, checkpoint and close its session, and forget the chat.
    pub fn close(&self, id: &str) -> Result<(), String> {
        let entry = self.inner.lock().map_err(|e| e.to_string())?.map.remove(id);
        if let Some(chat) = entry.and_then(|e| e.chat) {
            let store = Store::open_default().map_err(|e| e.to_string())?;
            chat.finish(&store).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Stop every agent and close its session (when the app or server quits).
    pub fn close_all(&self) {
        let all: Vec<Arc<crate::Chat>> = match self.inner.lock() {
            Ok(mut c) => c.map.drain().filter_map(|(_, e)| e.chat).collect(),
            Err(_) => return,
        };
        if all.is_empty() {
            return;
        }
        match Store::open_default() {
            Ok(store) => {
                for c in all {
                    let _ = c.finish(&store);
                }
            }
            Err(_) => {
                for c in all {
                    c.client.shutdown();
                }
            }
        }
    }
}
