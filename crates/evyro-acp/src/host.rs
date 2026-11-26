//! Hosting several chats at once: the desktop app and `evyro serve` both keep
//! their chats here. Each chat has an info record (state, agent, task), a
//! transcript of items (the agent's updates, file access, permission requests
//! and answers), and the permission requests still waiting for a person.
//!
//! A host tells its owner about changes through a [`Notify`] callback, so the
//! desktop can forward them as window events and the server can wake its
//! event streams.

pub use crate::archive::SearchHit;
use crate::archive::{self, SavedChat, MAX_SAVED};
use crate::{ChatOpts, Event, Started};
use evyro_core::at_rest::Sealer;
use evyro_core::Store;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    dirty: bool,
}

#[derive(Default)]
struct Inner {
    map: HashMap<String, Entry>,
    saved: HashMap<String, SavedChat>,
}

#[derive(Clone)]
pub struct Host {
    inner: Arc<Mutex<Inner>>,
    notify: Notify,
    evyro_mcp: Option<PathBuf>,
    archive: Option<PathBuf>,
    /// Encrypts saved transcripts when the person turned that on.
    sealer: Sealer,
    /// How many ended saved transcripts to keep; older ones are deleted.
    keep: Arc<AtomicUsize>,
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

fn model_choice(config: &Value) -> Value {
    let Some(model) = config.as_array().and_then(|options| {
        options
            .iter()
            .find(|x| x["category"] == "model" && x["type"] == "select")
    }) else {
        return Value::Null;
    };
    let Some(items) = model["options"].as_array() else {
        return Value::Null;
    };
    let options: Vec<_> = items
        .iter()
        .flat_map(|item| {
            item["options"]
                .as_array()
                .map_or_else(|| std::slice::from_ref(item), Vec::as_slice)
        })
        .filter(|item| item["value"].is_string() && item["name"].is_string())
        .map(|item| json!({"value": item["value"], "name": item["name"]}))
        .collect();
    if options.is_empty() || !model["id"].is_string() || !model["currentValue"].is_string() {
        return Value::Null;
    }
    json!({"id": model["id"], "current": model["currentValue"], "options": options})
}

impl Host {
    /// `evyro_mcp` is the `evyro` program the agents get as their MCP server.
    pub fn new(notify: Notify, evyro_mcp: Option<PathBuf>) -> Self {
        Host {
            inner: Default::default(),
            notify,
            evyro_mcp,
            archive: None,
            sealer: Sealer::plain(),
            keep: Arc::new(AtomicUsize::new(MAX_SAVED)),
        }
    }

    /// Keep this many ended chat transcripts; older ones are deleted.
    pub fn with_retention(self, keep: usize) -> Self {
        self.keep.store(keep, Ordering::SeqCst);
        self
    }

    /// Change how many ended transcripts are kept and delete the surplus now. Returns how many went.
    pub fn set_retention(&self, keep: usize) -> usize {
        self.keep.store(keep, Ordering::SeqCst);
        let (Some(dir), Ok(mut c)) = (&self.archive, self.inner.lock()) else {
            return 0;
        };
        Self::trim_ended(dir, &mut c, keep)
    }

    /// Delete every ended chat transcript. Chats still running keep their crash-safe copy.
    pub fn clear_saved(&self) -> usize {
        let (Some(dir), Ok(mut c)) = (&self.archive, self.inner.lock()) else {
            return 0;
        };
        Self::trim_ended(dir, &mut c, 0)
    }

    /// How many ended chat transcripts are saved.
    pub fn saved_count(&self) -> usize {
        self.inner.lock().map_or(0, |c| {
            c.saved.keys().filter(|id| !c.map.contains_key(*id)).count()
        })
    }

    /// Keep the newest `keep` ended transcripts and delete the rest from memory and disk.
    fn trim_ended(dir: &std::path::Path, c: &mut Inner, keep: usize) -> usize {
        let mut ended: Vec<String> = c
            .saved
            .keys()
            .filter(|id| !c.map.contains_key(*id))
            .cloned()
            .collect();
        ended.sort_unstable_by(|a, b| b.cmp(a));
        let mut removed = 0;
        for id in ended.into_iter().skip(keep) {
            match archive::remove(dir, &id) {
                Ok(()) => {
                    c.saved.remove(&id);
                    removed += 1;
                }
                Err(e) => eprintln!("evyro: could not delete transcript {id}: {e}"),
            }
        }
        removed
    }

    /// Seal saved transcripts with `sealer`. Call before [`Host::with_archive`].
    pub fn with_sealer(mut self, sealer: Sealer) -> Self {
        self.sealer = sealer;
        self
    }

    /// Enable local desktop history. Live chat APIs continue to see only this run.
    pub fn with_archive(mut self, dir: PathBuf) -> Self {
        match archive::load(&dir, self.keep.load(Ordering::SeqCst), &self.sealer) {
            Ok(saved) => self.inner.lock().unwrap().saved = saved,
            Err(e) => eprintln!("evyro: could not read chat history: {e}"),
        }
        self.archive = Some(dir);
        self
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
            e.dirty = true;
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
            e.dirty = true;
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
            let id = evyro_core::util::new_id("chat");
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
                    dirty: true,
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
            } else if let Event::Update { update, .. } = &e {
                if update["sessionUpdate"] == "config_option_update" {
                    h.set_info(
                        &i,
                        json!({"model_choice": model_choice(&update["configOptions"])}),
                    );
                }
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
        let attached = self.inner.lock().ok().is_some_and(|mut c| {
            let Some(e) = c.map.get_mut(&id) else {
                return false;
            };
            e.chat = Some(chat.clone());
            true
        });
        if !attached {
            let _ = chat.finish(&store);
            return;
        }
        self.set_info(
            &id,
            json!({"state": "ready", "busy": false, "draft_available": true, "agent": agent, "session": chat.session.id, "task": task_key, "cwd": chat.cwd.to_string_lossy(), "model_choice": model_choice(&chat.config_options)}),
        );
    }

    fn run_turn(&self, id: &str, chat: Arc<crate::Chat>, text: String, shown: Value) {
        self.push(id, json!({"t": "user", "text": shown}));
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
        v.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        v
    }

    /// Desktop-only list, including transcripts from earlier runs.
    pub fn list_with_history(&self) -> Vec<Value> {
        let c = self.inner.lock().unwrap();
        let mut v: Vec<Value> = c.map.values().map(|e| e.info.clone()).collect();
        v.extend(
            c.saved
                .iter()
                .filter(|(id, _)| !c.map.contains_key(*id))
                .map(|(_, chat)| chat.info.clone()),
        );
        v.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
        v
    }

    pub fn saved_log(&self, id: &str) -> Option<Value> {
        let c = self.inner.lock().ok()?;
        let chat = c.saved.get(id)?;
        Some(json!({"info": chat.info, "log": chat.log}))
    }

    /// Literal, case-insensitive search of saved desktop chat messages in one project.
    pub fn search_saved(&self, project_id: &str, query: &str) -> Result<Vec<SearchHit>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if query.chars().count() > 256 {
            return Err("Search terms are limited to 256 characters".into());
        }
        let pattern = regex::RegexBuilder::new(&regex::escape(query))
            .case_insensitive(true)
            .build()
            .map_err(|e| e.to_string())?;
        let c = self.inner.lock().map_err(|e| e.to_string())?;
        let mut chats: Vec<_> = c
            .saved
            .iter()
            .filter(|(id, chat)| !c.map.contains_key(*id) && chat.info["project_id"] == project_id)
            .collect();
        chats.sort_unstable_by(|(a, _), (b, _)| b.cmp(a));
        let mut hits = Vec::new();
        for (_, chat) in chats {
            hits.extend(chat.matches(&pattern, 60 - hits.len()));
            if hits.len() == 60 {
                break;
            }
        }
        Ok(hits)
    }

    pub fn forget_saved(&self, id: &str) -> Result<(), String> {
        let mut c = self.inner.lock().map_err(|e| e.to_string())?;
        if c.map.contains_key(id) {
            return Err("close the chat before deleting its transcript".into());
        }
        if c.saved.contains_key(id) {
            if let Some(dir) = &self.archive {
                archive::remove(dir, id).map_err(|e| e.to_string())?;
            }
            c.saved.remove(id);
        }
        Ok(())
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
            "draft": e.chat.as_ref().filter(|_| e.info["draft_available"] == true).map(|chat| &chat.first_prompt),
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
        let (chat, info) = {
            let mut c = self.inner.lock().map_err(|e| e.to_string())?;
            let e = c.map.get_mut(id).ok_or("no such chat")?;
            let chat = e.chat.clone().ok_or("the chat is still starting")?;
            if !chat.client.is_alive() {
                return Err("the agent has stopped; start a new chat".into());
            }
            if e.info["switching_model"] == true {
                return Err("wait for the model to change".into());
            }
            if e.info["busy"] == true {
                return Err("the agent is still answering; wait or stop the turn".into());
            }
            e.info["busy"] = json!(true);
            e.info["draft_available"] = json!(false);
            (chat, e.info.clone())
        };
        (self.notify)(Change::State(info));
        self.run_turn(id, chat, text.to_string(), json!(text));
        Ok(())
    }

    pub fn set_model(&self, id: &str, value: &str) -> Result<(), String> {
        let (chat, config_id, info) = {
            let mut c = self.inner.lock().map_err(|e| e.to_string())?;
            let e = c.map.get_mut(id).ok_or("no such chat")?;
            let chat = e.chat.clone().ok_or("the chat is still starting")?;
            if !chat.client.is_alive()
                || e.info["busy"] == true
                || e.info["switching_model"] == true
            {
                return Err("the chat must be running and idle to change its model".into());
            }
            let model = &e.info["model_choice"];
            let config_id = model["id"]
                .as_str()
                .ok_or("this agent did not advertise a model selector")?;
            if !model["options"]
                .as_array()
                .is_some_and(|options| options.iter().any(|option| option["value"] == value))
            {
                return Err("this model is not available in the chat".into());
            }
            let config_id = config_id.to_owned();
            e.info["switching_model"] = json!(true);
            (chat, config_id, e.info.clone())
        };
        (self.notify)(Change::State(info));
        let result = chat.client.request_wait(
            "session/set_config_option",
            json!({"sessionId": chat.acp_session, "configId": config_id, "value": value}),
            Some(std::time::Duration::from_secs(10)),
        );
        let mut patch = json!({"switching_model": false});
        if let Ok(response) = &result {
            patch["model_choice"] = model_choice(&response["configOptions"]);
        }
        self.set_info(id, patch);
        result.map(|_| ()).map_err(|e| {
            let note = if e.0.contains("did not respond in time") {
                " The model may still change; check the picker before sending."
            } else {
                ""
            };
            format!("{e}{note}")
        })
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

    /// Stop the agent and close its session. Desktop hosts retain the transcript.
    pub fn close(&self, id: &str) -> Result<(), String> {
        let (entry, save) = {
            let mut c = self.inner.lock().map_err(|e| e.to_string())?;
            let entry = c.map.remove(id);
            let save = match (&self.archive, &entry) {
                (Some(dir), Some(e)) => Self::save_entry(dir, &self.sealer, &mut c.saved, id, e),
                _ => Ok(()),
            };
            if let Some(dir) = &self.archive {
                Self::trim_ended(dir, &mut c, self.keep.load(Ordering::SeqCst));
            }
            (entry, save)
        };
        if let Some(chat) = entry.and_then(|e| e.chat) {
            let store = Store::open_default().map_err(|e| {
                chat.client.shutdown();
                e.to_string()
            })?;
            chat.finish(&store).map_err(|e| e.to_string())?;
        }
        save.map_err(|e| format!("Chat closed. Could not save its transcript: {e}"))
    }

    pub fn save_all(&self) {
        if let Ok(mut c) = self.inner.lock() {
            self.save_locked(&mut c);
        }
    }

    fn save_locked(&self, c: &mut Inner) {
        let Some(dir) = &self.archive else { return };
        let mut newest: Vec<_> = c
            .map
            .keys()
            .cloned()
            .chain(c.saved.keys().cloned())
            .collect();
        newest.sort_unstable();
        newest.dedup();
        for id in newest.iter().rev() {
            if let Some(e) = c.map.get_mut(id).filter(|e| e.dirty) {
                match Self::save_entry(dir, &self.sealer, &mut c.saved, id, e) {
                    Ok(()) => e.dirty = false,
                    Err(e) => eprintln!("evyro: could not save transcript {id}: {e}"),
                }
            }
        }
        Self::trim_ended(dir, c, self.keep.load(Ordering::SeqCst));
    }

    fn save_entry(
        dir: &std::path::Path,
        sealer: &Sealer,
        saved: &mut HashMap<String, SavedChat>,
        id: &str,
        e: &Entry,
    ) -> std::io::Result<()> {
        let record =
            SavedChat::capture(e.info.clone(), e.log.clone(), e.base > 0).write(dir, id, sealer)?;
        saved.insert(id.to_owned(), record);
        Ok(())
    }

    /// Stop every agent and close its session (when the app or server quits).
    pub fn close_all(&self) {
        let all: Vec<Arc<crate::Chat>> = match self.inner.lock() {
            Ok(mut c) => {
                self.save_locked(&mut c);
                c.map.drain().filter_map(|(_, e)| e.chat).collect()
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_deletes_old_ended_transcripts_and_clear_leaves_live_chats() {
        let dir = tempfile::tempdir().unwrap();
        let host = Host::new(Arc::new(|_| {}), None).with_archive(dir.path().to_owned());
        let add = |running: bool| {
            let id = evyro_core::util::new_id("chat");
            host.inner.lock().unwrap().map.insert(
                id.clone(),
                Entry {
                    info: json!({"id": id, "kind": "chat", "running": running, "project_id": "p"}),
                    chat: None,
                    log: Vec::new(),
                    base: 0,
                    pending: Vec::new(),
                    dirty: true,
                },
            );
            host.push(&id, json!({"t": "user", "text": "hello"}));
            id
        };
        let files = || std::fs::read_dir(dir.path()).unwrap().count();
        let ended: Vec<String> = (0..3).map(|_| add(false)).collect();
        for id in &ended {
            host.close(id).unwrap();
        }
        let live = add(true);
        host.save_all();
        assert_eq!(host.saved_count(), 3);
        assert_eq!(
            files(),
            4,
            "three ended transcripts and the live chat's crash copy"
        );
        // Keep two: the oldest ended transcript goes, from memory and from disk.
        assert_eq!(host.set_retention(2), 1);
        assert_eq!(host.saved_count(), 2);
        assert!(host.saved_log(&ended[0]).is_none() && host.saved_log(&ended[2]).is_some());
        assert_eq!(files(), 3);
        // A lower limit applies to later closes too.
        host.close(&live).unwrap();
        assert_eq!(
            host.saved_count(),
            2,
            "closing a chat never exceeds the limit"
        );
        // Clear removes every ended transcript.
        assert_eq!(host.clear_saved(), 2);
        assert_eq!(host.saved_count(), 0);
        assert_eq!(files(), 0);
        assert_eq!(host.clear_saved(), 0);
    }

    #[test]
    fn desktop_history_survives_shutdown_and_stays_out_of_the_live_api() {
        let dir = tempfile::tempdir().unwrap();
        let open = || Host::new(Arc::new(|_| {}), None).with_archive(dir.path().to_owned());
        let host = open();
        let mut ids = Vec::new();
        for method in ["end chat", "quit app"] {
            let id = evyro_core::util::new_id("chat");
            let project = if method == "end chat" {
                "project"
            } else {
                "another-project"
            };
            host.inner.lock().unwrap().map.insert(id.clone(), Entry {
                info: json!({"id": id, "kind": "chat", "running": true, "busy": true, "waiting": 1,
                    "state": "ready", "draft_available": true, "project_id": project}),
                chat: None, log: Vec::new(), base: 0, pending: Vec::new(), dirty: true,
            });
            host.push(&id, json!({"t": "user", "text": method}));
            host.push(&id, json!({"t": "user", "text": "debug a.b"}));
            host.push(&id, json!({"t": "permission", "request": 1, "params": {}}));
            assert!(host.forget_saved(&id).is_err());
            host.save_all();
            assert!(host.search_saved(project, "a.b").unwrap().is_empty());
            if method == "end chat" {
                host.close(&id).unwrap();
                assert!(host.saved_log(&id).is_some());
            }
            ids.push(id);
        }
        host.close_all();
        let reopened = open();
        assert!(reopened.list().is_empty());
        assert_eq!(reopened.list_with_history().len(), 2);
        let hits = serde_json::to_value(reopened.search_saved("project", "A.B").unwrap()).unwrap();
        assert_eq!(hits.as_array().unwrap().len(), 1);
        assert_eq!(hits[0]["id"], ids[0]);
        assert_eq!(hits[0]["message_index"], 1);
        assert!(reopened.search_saved("project", "a.b.").unwrap().is_empty());
        for (id, method) in ids.iter().zip(["end chat", "quit app"]) {
            let record = reopened.saved_log(id).unwrap();
            assert_eq!(record["log"][0]["text"], method);
            assert_eq!(record["info"]["running"], false);
            assert_eq!(record["info"]["busy"], false);
            assert_eq!(record["info"]["waiting"], 0);
            assert_eq!(record["info"]["draft_available"], false);
            assert!(reopened.log(id, None).is_err());
            assert!(reopened.pending(id).is_err());
        }
        reopened.forget_saved(&ids[0]).unwrap();
        let reopened_again = open();
        assert!(reopened_again.saved_log(&ids[0]).is_none());
        assert!(reopened_again
            .search_saved("project", "a.b")
            .unwrap()
            .is_empty());
        assert!(reopened_again.saved_log(&ids[1]).is_some());
    }
}
