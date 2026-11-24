//! A small, synchronous ACP client over stdio (newline-delimited JSON-RPC 2.0).
//!
//! The agent runs as a child process. One thread reads its stdout and routes
//! responses, notifications and agent-to-client requests; another forwards
//! its stderr. Evyro answers file reads and writes itself, inside the session
//! root only, and hands permission requests to the caller, who answers them
//! with [`AcpClient::answer_permission`] whenever the person decides.

use evyro_core::redact;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

/// The ACP protocol version this client speaks.
pub const PROTOCOL_VERSION: u64 = 1;
/// Largest file the agent may read or write through the client.
const MAX_FILE_BYTES: usize = 4_000_000;

/// Something the agent did, for the caller to show or record.
#[derive(Debug, Clone)]
pub enum Event {
    /// A `session/update` notification: message chunks, tool calls, plans...
    Update { session_id: String, update: Value },
    /// The agent asks to run a tool. Answer with [`AcpClient::answer_permission`].
    Permission { request_id: Value, params: Value },
    /// The agent read a file through the client (repo-relative path).
    FileRead { path: String },
    /// The agent wrote a file through the client (repo-relative path).
    FileWrite { path: String, bytes: usize },
    /// The client refused a file request, with the reason.
    FileRefused { path: String, reason: String },
    /// A line the agent printed on stderr.
    Stderr(String),
    /// The agent process ended.
    Exit(Option<i32>),
}

pub type OnEvent = Arc<dyn Fn(Event) + Send + Sync>;

/// An MCP server the agent should connect to (stdio transport).
#[derive(Debug, Clone)]
pub struct McpStdio {
    pub name: String,
    /// Absolute path, as ACP requires.
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// How to start an ACP agent.
#[derive(Debug, Clone)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env_set: Vec<(String, String)>,
    pub env_remove: Vec<String>,
}

type Reply = std::result::Result<Value, Value>;

struct Inner {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Sender<Reply>>>,
    /// Permission requests not answered yet: request id (as JSON text) -> session id.
    open_permissions: Mutex<HashMap<String, (Value, String)>>,
    root: PathBuf,
    alive: AtomicBool,
    on_event: OnEvent,
}

#[derive(Clone)]
pub struct AcpClient {
    inner: Arc<Inner>,
}

/// An error from the agent or the connection, as text for the person.
#[derive(Debug, Clone)]
pub struct AcpError(pub String);

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for AcpError {}

pub type Result<T> = std::result::Result<T, AcpError>;

fn rpc_error_text(e: &Value) -> String {
    let msg = e.get("message").and_then(|m| m.as_str()).unwrap_or("error");
    match e.get("data") {
        Some(d) if !d.is_null() => format!("{msg} ({d})"),
        _ => msg.to_string(),
    }
}

impl AcpClient {
    /// Start the agent. Nothing is sent until [`AcpClient::initialize`].
    pub fn spawn(l: &Launch, on_event: OnEvent) -> Result<Self> {
        let mut cmd = Command::new(&l.program);
        cmd.args(&l.args)
            .current_dir(&l.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for k in &l.env_remove {
            cmd.env_remove(k);
        }
        for (k, v) in &l.env_set {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| AcpError(format!("could not start {}: {e}", l.program.display())))?;
        let stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let root = std::fs::canonicalize(&l.cwd).unwrap_or_else(|_| l.cwd.clone());
        let inner = Arc::new(Inner {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            next_id: AtomicU64::new(0),
            pending: Mutex::new(HashMap::new()),
            open_permissions: Mutex::new(HashMap::new()),
            root,
            alive: AtomicBool::new(true),
            on_event,
        });
        let me = AcpClient { inner };
        let reader = me.clone();
        std::thread::spawn(move || reader.read_loop(stdout));
        let err = me.inner.on_event.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                err(Event::Stderr(line));
            }
        });
        Ok(me)
    }

    pub fn is_alive(&self) -> bool {
        self.inner.alive.load(Ordering::SeqCst)
    }

    pub fn pid(&self) -> Option<u32> {
        self.inner.child.lock().ok().map(|c| c.id())
    }

    fn send(&self, msg: &Value) -> Result<()> {
        let mut line = serde_json::to_string(msg).expect("json");
        line.push('\n');
        let mut w = self
            .inner
            .stdin
            .lock()
            .map_err(|_| AcpError("agent pipe poisoned".into()))?;
        w.write_all(line.as_bytes())
            .and_then(|_| w.flush())
            .map_err(|e| AcpError(format!("the agent is not listening any more: {e}")))
    }

    /// Call a method on the agent and wait for its answer.
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.request_wait(method, params, None)
    }

    pub(crate) fn request_wait(
        &self,
        method: &str,
        params: Value,
        timeout: Option<std::time::Duration>,
    ) -> Result<Value> {
        if !self.is_alive() {
            return Err(AcpError("the agent has stopped".into()));
        }
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = channel();
        self.inner.pending.lock().unwrap().insert(id, tx);
        if let Err(e) =
            self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
        {
            self.inner.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        use std::sync::mpsc::RecvTimeoutError;
        let reply = match timeout {
            Some(duration) => rx.recv_timeout(duration),
            None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match reply {
            Err(RecvTimeoutError::Timeout) => {
                self.inner.pending.lock().unwrap().remove(&id);
                Err(AcpError(format!(
                    "{method}: the agent did not respond in time"
                )))
            }
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(AcpError(format!("{method}: {}", rpc_error_text(&e)))),
            Err(_) => Err(AcpError(format!(
                "{method}: the agent stopped before answering"
            ))),
        }
    }

    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    /// `initialize`: negotiate the protocol and tell the agent what Evyro offers.
    pub fn initialize(&self) -> Result<Value> {
        let r = self.request(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "clientCapabilities": {
                    "fs": {"readTextFile": true, "writeTextFile": true},
                    "terminal": false
                },
                "clientInfo": {"name": evyro_core::brand::CLI_NAME, "title": evyro_core::brand::PRODUCT_NAME, "version": evyro_core::brand::VERSION}
            }),
        )?;
        match r.get("protocolVersion").and_then(|v| v.as_u64()) {
            Some(PROTOCOL_VERSION) => Ok(r),
            other => Err(AcpError(format!(
                "the agent speaks ACP version {}, Evyro speaks version {PROTOCOL_VERSION}",
                other.map_or("unknown".into(), |v| v.to_string())
            ))),
        }
    }

    /// `session/new` in `cwd` with these MCP servers. Returns the session id.
    pub fn new_session(&self, cwd: &Path, mcp: &[McpStdio]) -> Result<String> {
        Ok(self.open_session(cwd, mcp)?["sessionId"]
            .as_str()
            .expect("validated by open_session")
            .to_owned())
    }

    pub(crate) fn open_session(&self, cwd: &Path, mcp: &[McpStdio]) -> Result<Value> {
        let servers: Vec<Value> = mcp
            .iter()
            .map(|m| {
                json!({
                    "name": m.name,
                    "command": m.command.to_string_lossy(),
                    "args": m.args,
                    "env": m.env.iter().map(|(k, v)| json!({"name": k, "value": v})).collect::<Vec<_>>(),
                })
            })
            .collect();
        let r = self.request(
            "session/new",
            json!({"cwd": cwd.to_string_lossy(), "mcpServers": servers}),
        )?;
        if !r["sessionId"].as_str().is_some_and(|id| !id.is_empty()) {
            return Err(AcpError("session/new: the agent gave no session id".into()));
        }
        Ok(r)
    }

    /// `session/prompt`: send the person's message and wait for the turn to
    /// end. Updates arrive as events meanwhile. Returns the stop reason.
    pub fn prompt(&self, session_id: &str, text: &str) -> Result<String> {
        let r = self.request(
            "session/prompt",
            json!({"sessionId": session_id, "prompt": [{"type": "text", "text": text}]}),
        )?;
        Ok(r.get("stopReason")
            .and_then(|s| s.as_str())
            .unwrap_or("end_turn")
            .to_string())
    }

    /// `session/cancel`, and answer every open permission request as cancelled,
    /// as the protocol requires.
    pub fn cancel(&self, session_id: &str) -> Result<()> {
        let open: Vec<Value> = self
            .inner
            .open_permissions
            .lock()
            .unwrap()
            .values()
            .filter(|(_, s)| s == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        for id in open {
            let _ = self.answer_permission(&id, None);
        }
        self.notify("session/cancel", json!({"sessionId": session_id}))
    }

    /// Answer a permission request: `Some(option_id)` picks that option,
    /// `None` reports the request as cancelled.
    pub fn answer_permission(&self, request_id: &Value, option_id: Option<&str>) -> Result<()> {
        let key = request_id.to_string();
        if self
            .inner
            .open_permissions
            .lock()
            .unwrap()
            .remove(&key)
            .is_none()
        {
            return Err(AcpError(
                "that permission request was already answered".into(),
            ));
        }
        let outcome = match option_id {
            Some(o) => json!({"outcome": "selected", "optionId": o}),
            None => json!({"outcome": "cancelled"}),
        };
        self.send(&json!({"jsonrpc": "2.0", "id": request_id, "result": {"outcome": outcome}}))
    }

    /// Stop the agent process.
    pub fn shutdown(&self) {
        if let Ok(mut c) = self.inner.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.inner.alive.store(false, Ordering::SeqCst);
    }

    fn read_loop(&self, stdout: std::process::ChildStdout) {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(msg) = serde_json::from_str::<Value>(line) else {
                (self.inner.on_event)(Event::Stderr(format!("(not JSON-RPC) {line}")));
                continue;
            };
            let method = msg.get("method").and_then(|m| m.as_str());
            let id = msg.get("id").cloned().filter(|v| !v.is_null());
            match (method, id) {
                (Some(m), Some(id)) => {
                    self.on_request(m, id, msg.get("params").cloned().unwrap_or(json!({})))
                }
                (Some(m), None) => {
                    self.on_notification(m, msg.get("params").cloned().unwrap_or(json!({})))
                }
                (None, Some(id)) => {
                    let Some(n) = id.as_u64() else { continue };
                    if let Some(tx) = self.inner.pending.lock().unwrap().remove(&n) {
                        let reply = match msg.get("error") {
                            Some(e) => Err(e.clone()),
                            None => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(reply);
                    }
                }
                (None, None) => {}
            }
        }
        // The agent is gone: fail every waiting call, then report the exit.
        self.inner.alive.store(false, Ordering::SeqCst);
        for (_, tx) in self.inner.pending.lock().unwrap().drain() {
            let _ = tx.send(Err(json!({"code": -32000, "message": "the agent stopped"})));
        }
        self.inner.open_permissions.lock().unwrap().clear();
        let code = self
            .inner
            .child
            .lock()
            .ok()
            .and_then(|mut c| c.wait().ok())
            .and_then(|s| s.code());
        (self.inner.on_event)(Event::Exit(code));
    }

    fn on_notification(&self, method: &str, params: Value) {
        if method == "session/update" {
            let session_id = params
                .get("sessionId")
                .and_then(|s| s.as_str())
                .unwrap_or("")
                .to_string();
            let update = params.get("update").cloned().unwrap_or(Value::Null);
            (self.inner.on_event)(Event::Update { session_id, update });
        }
    }

    fn reply(&self, id: &Value, result: std::result::Result<Value, (i64, String)>) {
        let msg = match result {
            Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
            Err((code, message)) => {
                json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
            }
        };
        let _ = self.send(&msg);
    }

    fn on_request(&self, method: &str, id: Value, params: Value) {
        match method {
            "session/request_permission" => {
                let sid = params
                    .get("sessionId")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string();
                self.inner
                    .open_permissions
                    .lock()
                    .unwrap()
                    .insert(id.to_string(), (id.clone(), sid));
                (self.inner.on_event)(Event::Permission {
                    request_id: id,
                    params,
                });
            }
            "fs/read_text_file" => {
                let r = self.read_file(&params);
                self.reply(&id, r);
            }
            "fs/write_text_file" => {
                let r = self.write_file(&params);
                self.reply(&id, r);
            }
            _ => self.reply(&id, Err((-32601, format!("Evyro does not offer {method}")))),
        }
    }

    /// Resolve an absolute path from the agent to a repo-relative one inside
    /// the session root. Refuses anything outside it, `.git/`, and files that
    /// may hold secrets.
    fn inside_root(&self, params: &Value) -> std::result::Result<(PathBuf, String), (i64, String)> {
        let raw = params.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let p = Path::new(raw);
        if !p.is_absolute() {
            return Err((-32602, "the path must be absolute".to_string()));
        }
        // Normalise without touching the disk (the file may not exist yet).
        let mut norm = PathBuf::new();
        for c in p.components() {
            match c {
                Component::ParentDir => {
                    norm.pop();
                }
                Component::CurDir => {}
                other => norm.push(other.as_os_str()),
            }
        }
        // Resolve symlinks in the existing part of the path.
        let mut existing = norm.clone();
        let mut tail = Vec::new();
        while !existing.exists() {
            match (
                existing.file_name().map(|f| f.to_os_string()),
                existing.parent(),
            ) {
                (Some(f), Some(parent)) => {
                    tail.push(f);
                    existing = parent.to_path_buf();
                }
                _ => break,
            }
        }
        let mut real = std::fs::canonicalize(&existing).unwrap_or(existing);
        for f in tail.into_iter().rev() {
            real.push(f);
        }
        let rel = real
            .strip_prefix(&self.inner.root)
            .map_err(|_| {
                (
                    -32602,
                    "outside the project; Evyro only shares files inside it".to_string(),
                )
            })?
            .to_string_lossy()
            .replace('\\', "/");
        if rel.is_empty() || rel == ".git" || rel.starts_with(".git/") {
            return Err((
                -32602,
                "Git's own files are not shared with agents".to_string(),
            ));
        }
        if redact::is_sensitive_path(&rel) {
            return Err((
                -32602,
                "may hold secrets, so it is not shared with agents".to_string(),
            ));
        }
        Ok((real, rel))
    }

    /// The path the agent asked for, relative to the root when it starts there.
    fn shown_path(&self, params: &Value) -> String {
        let raw = params.get("path").and_then(|p| p.as_str()).unwrap_or("");
        let root = self.inner.root.to_string_lossy();
        raw.strip_prefix(root.as_ref())
            .and_then(|r| r.strip_prefix('/').or_else(|| r.strip_prefix('\\')))
            .unwrap_or(raw)
            .to_string()
    }

    fn read_file(&self, params: &Value) -> std::result::Result<Value, (i64, String)> {
        let (full, rel) = match self.inside_root(params) {
            Ok(x) => x,
            Err(e) => {
                (self.inner.on_event)(Event::FileRefused {
                    path: self.shown_path(params),
                    reason: e.1.clone(),
                });
                return Err(e);
            }
        };
        let bytes =
            std::fs::read(&full).map_err(|e| (-32002, format!("could not read {rel}: {e}")))?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err((
                -32602,
                format!("{rel} is too large to share ({} bytes)", bytes.len()),
            ));
        }
        let text = String::from_utf8_lossy(&bytes);
        let line = params
            .get("line")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1) as usize;
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize);
        let content = if line == 1 && limit.is_none() {
            text.into_owned()
        } else {
            let lines = text.split_inclusive('\n').skip(line - 1);
            match limit {
                Some(n) => lines.take(n).collect(),
                None => lines.collect(),
            }
        };
        (self.inner.on_event)(Event::FileRead { path: rel });
        Ok(json!({"content": content}))
    }

    fn write_file(&self, params: &Value) -> std::result::Result<Value, (i64, String)> {
        let (full, rel) = match self.inside_root(params) {
            Ok(x) => x,
            Err(e) => {
                (self.inner.on_event)(Event::FileRefused {
                    path: self.shown_path(params),
                    reason: e.1.clone(),
                });
                return Err(e);
            }
        };
        let content = params.get("content").and_then(|c| c.as_str()).unwrap_or("");
        if content.len() > MAX_FILE_BYTES {
            return Err((-32602, "content is too large".into()));
        }
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| (-32002, format!("could not create {}: {e}", dir.display())))?;
        }
        std::fs::write(&full, content)
            .map_err(|e| (-32002, format!("could not write {rel}: {e}")))?;
        (self.inner.on_event)(Event::FileWrite {
            path: rel,
            bytes: content.len(),
        });
        Ok(Value::Null)
    }
}

/// Pick the option of the wanted kind ("allow" or "reject") from a
/// permission request, preferring the one-time variant.
pub fn pick_option(params: &Value, allow: bool) -> Option<String> {
    let opts = params.get("options")?.as_array()?;
    let kinds: &[&str] = if allow {
        &["allow_once", "allow_always"]
    } else {
        &["reject_once", "reject_always"]
    };
    kinds.iter().find_map(|k| {
        opts.iter()
            .find(|o| o.get("kind").and_then(|v| v.as_str()) == Some(k))
            .and_then(|o| o.get("optionId").and_then(|v| v.as_str()))
            .map(String::from)
    })
}
