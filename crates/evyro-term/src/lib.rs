//! Pseudo-terminals for agent threads.
//!
//! The desktop app runs each agent (or a plain shell) in its own pseudo-terminal,
//! so interactive CLIs such as Claude Code or Codex work exactly as they do in a
//! terminal window. Output is streamed to a callback as UTF-8 text and kept in a
//! bounded scrollback, so a thread can be re-attached after the view changes.
//!
//! With [`Terminals::with_store`], each terminal's details and output are also
//! saved to disk, so threads from an earlier run of the app are still listed
//! (as ended, `restored`) with their output after a restart.

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Bytes of output kept per terminal for re-attaching.
pub const SCROLLBACK_BYTES: usize = 512 * 1024;

/// What to start in a new terminal.
#[derive(Debug, Clone)]
pub struct Spawn {
    pub program: String,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// Shown in the thread list, e.g. "codex on T-1".
    pub title: String,
    /// "agent" or "shell".
    pub kind: String,
    pub target: Option<String>,
    pub task: Option<String>,
    pub project_id: Option<String>,
    pub credit_budget: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermInfo {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub target: Option<String>,
    pub task: Option<String>,
    pub project_id: Option<String>,
    pub cwd: String,
    pub command: String,
    pub started_at: u64,
    pub running: bool,
    pub exit_code: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    /// From an earlier run of the app: ended, output read from disk.
    #[serde(default)]
    pub restored: bool,
    /// Set when the app closed while the program was still running.
    #[serde(default)]
    pub stopped_by_app: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit_budget: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider_session: Option<uuid::Uuid>,
}

/// Saved threads kept on disk; older ones are removed.
pub const SAVED_THREADS: usize = 60;

struct Term {
    info: Mutex<TermInfo>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    scrollback: Mutex<Scrollback>,
    /// Closed by the person: never saved again.
    forgotten: std::sync::atomic::AtomicBool,
    /// Applied to output before it is written to disk (never to the live view).
    redactor: Option<Redactor>,
}

#[derive(Default)]
struct Scrollback {
    buf: VecDeque<u8>,
    /// Bytes of output since the start, including bytes dropped from `buf`.
    total: u64,
    /// The first byte of this process's output, until a frontend attaches.
    startup_from: Option<u64>,
}

pub struct AttachmentSnapshot {
    pub data: String,
    pub live: String,
    pub end: u64,
}

/// All terminals of one app instance.
#[derive(Default)]
pub struct Terminals {
    terms: Mutex<HashMap<String, Arc<Term>>>,
    lifecycle: Mutex<()>,
    next: AtomicU64,
    /// Where threads are saved, if anywhere.
    store: Option<PathBuf>,
    /// Makes ids unique across runs of the app, so saved threads never collide.
    run: String,
    redactor: Option<Redactor>,
}

/// Turns output text into the text that may be saved, for example by hiding secrets.
pub type Redactor = Arc<dyn Fn(&str) -> String + Send + Sync>;

/// `(id, text, end)`: `end` is the byte offset of the end of this chunk in the
/// terminal's whole output, so a view can drop chunks a snapshot already holds.
pub type OnOutput = Arc<dyn Fn(&str, &str, u64) + Send + Sync>;
pub type OnExit = Arc<dyn Fn(&str, Option<u32>) + Send + Sync>;

impl Terminals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Redact saved output with `redactor`. The live view and the in-memory
    /// scrollback are left as they are.
    pub fn with_redactor(mut self, redactor: Redactor) -> Self {
        self.redactor = Some(redactor);
        self
    }

    /// Terminals that save each thread under `dir` and list saved ones from
    /// earlier runs.
    pub fn with_store(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        Self {
            store: Some(dir),
            run: format!("{secs:x}_"),
            ..Default::default()
        }
    }

    /// Start `spec` in a new pseudo-terminal. `on_output(id, text)` receives
    /// output as it arrives (see [`OnOutput`]); `on_exit(id, code)` is called once when the
    /// program ends. Returns the terminal id.
    pub fn open(&self, spec: Spawn, on_output: OnOutput, on_exit: OnExit) -> Result<TermInfo> {
        let _guard = self.lifecycle.lock().unwrap();
        let id = format!(
            "term_{}{}",
            self.run,
            self.next.fetch_add(1, Ordering::SeqCst) + 1
        );
        self.spawn(id, spec, None, on_output, on_exit)
    }

    /// Relaunch the process under the same thread id and append to its saved output.
    pub fn resume(
        &self,
        id: &str,
        spec: Spawn,
        on_output: OnOutput,
        on_exit: OnExit,
    ) -> Result<TermInfo> {
        let _guard = self.lifecycle.lock().unwrap();
        let old = self.get(id).ok();
        let previous = match &old {
            Some(t) => t.info.lock().unwrap().clone(),
            None => {
                let dir = self
                    .store
                    .as_ref()
                    .ok_or_else(|| anyhow!("no terminal {id}"))?;
                let bytes = std::fs::read(saved_path(dir, id, "json")?)
                    .with_context(|| format!("no terminal {id}"))?;
                serde_json::from_slice::<TermInfo>(&bytes)?
            }
        };
        if old.is_some() && previous.running {
            return Err(anyhow!("{} is already running", previous.title));
        }
        if previous.kind != spec.kind
            || previous.project_id != spec.project_id
            || previous.target != spec.target
            || previous.task != spec.task
        {
            return Err(anyhow!("thread settings changed; start a new thread"));
        }
        let (data, end) = self.snapshot(id).unwrap_or_default();
        let mut scrollback = Scrollback {
            buf: data.into_bytes().into(),
            total: end,
            startup_from: None,
        };
        let marker = b"\x1b[?1049l\x1b[?2004l\x1b[0m\r\n[Resumed Evyro thread]\r\n";
        push_scrollback(&mut scrollback.buf, marker);
        scrollback.total += marker.len() as u64;
        self.spawn(
            id.to_string(),
            spec,
            Some((previous, scrollback, old)),
            on_output,
            on_exit,
        )
    }

    fn spawn(
        &self,
        id: String,
        spec: Spawn,
        previous: Option<(TermInfo, Scrollback, Option<Arc<Term>>)>,
        on_output: OnOutput,
        on_exit: OnExit,
    ) -> Result<TermInfo> {
        let (cols, rows) = previous
            .as_ref()
            .and_then(|(info, _, _)| info.cols.zip(info.rows))
            .unwrap_or((spec.cols, spec.rows));
        let pty_size = size(cols, rows);
        let pair = native_pty_system()
            .openpty(pty_size)
            .context("could not open a pseudo-terminal")?;
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let credit_budget = previous
            .as_ref()
            .map_or(spec.credit_budget, |(i, _, _)| i.credit_budget);
        let provider_session = previous
            .as_ref()
            .and_then(|(i, _, _)| i.provider_session)
            .or_else(|| credit_budget.map(|_| uuid::Uuid::new_v4()));
        if let (Some(dir), Some(budget), Some(session)) =
            (&self.store, credit_budget, provider_session)
        {
            let path = saved_path(dir, &id, "usage.json")?;
            cmd.env("EVYRO_DESKTOP_COPILOT_BUDGET", budget.to_string());
            cmd.env("EVYRO_DESKTOP_COPILOT_SESSION", session.to_string());
            cmd.env("EVYRO_DESKTOP_COPILOT_USAGE", path);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("could not start {}", spec.program))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let (previous, mut scrollback) = if let Some((info, scrollback, old)) = previous {
            if let Some(t) = old {
                let _info = t.info.lock().unwrap();
                t.forgotten.store(true, Ordering::SeqCst);
            }
            (Some(info), scrollback)
        } else {
            (None, Scrollback::default())
        };
        scrollback.startup_from = Some(scrollback.total);
        let info = TermInfo {
            id: id.clone(),
            title: previous.as_ref().map_or(spec.title, |i| i.title.clone()),
            kind: spec.kind.clone(),
            target: spec.target.clone(),
            task: spec.task.clone(),
            project_id: spec.project_id.clone(),
            cwd: spec.cwd.to_string_lossy().into_owned(),
            command: command_line(&spec.program, &spec.args),
            started_at: previous.map_or_else(
                || {
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                },
                |i| i.started_at,
            ),
            running: true,
            exit_code: None,
            cols: Some(pty_size.cols),
            rows: Some(pty_size.rows),
            restored: false,
            stopped_by_app: false,
            credit_budget,
            provider_session,
        };
        let term = Arc::new(Term {
            info: Mutex::new(info.clone()),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            scrollback: Mutex::new(scrollback),
            forgotten: Default::default(),
            redactor: self.redactor.clone(),
        });
        self.terms.lock().unwrap().insert(id.clone(), term.clone());
        let store = self.store.clone();
        if let Some(dir) = &store {
            save_term(dir, &term);
            prune(dir, SAVED_THREADS);
        }

        // Reader: stream output, keep scrollback. Ends at EOF (program exit).
        let (t, rid, out) = (term.clone(), id.clone(), on_output);
        let reader_done = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            let mut carry = Utf8Carry::default();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // Hold the lock while emitting so offsets arrive in order
                        // and a snapshot never falls inside a chunk.
                        let mut sb = t.scrollback.lock().unwrap();
                        push_scrollback(&mut sb.buf, &buf[..n]);
                        sb.total += n as u64;
                        let text = carry.push(&buf[..n]);
                        if !text.is_empty() && !t.forgotten.load(Ordering::SeqCst) {
                            out(&rid, &text, sb.total);
                        }
                    }
                }
            }
        });

        // Waiter: record the exit code once the program ends and output is drained.
        let (t, wid) = (term, id);
        std::thread::spawn(move || {
            let code = child.wait().ok().map(|s| s.exit_code());
            // Windows ConPTY keeps the pipe open until the master is dropped,
            // so do not wait forever for the reader there.
            if !cfg!(windows) {
                let _ = reader_done.join();
            }
            {
                let mut i = t.info.lock().unwrap();
                i.running = false;
                i.exit_code = code;
                if let Some(dir) = &store {
                    save_term_locked(dir, &t, &i);
                }
            }
            on_exit(&wid, code);
        });
        Ok(info)
    }

    pub fn write(&self, id: &str, data: &str) -> Result<()> {
        let t = self.get(id)?;
        let mut w = t.writer.lock().unwrap();
        w.write_all(data.as_bytes())?;
        w.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let t = self.get(id)?;
        let mut info = t.info.lock().unwrap();
        if !info.running {
            return Ok(());
        }
        let pty_size = size(cols, rows);
        t.master
            .lock()
            .unwrap()
            .resize(pty_size)
            .map_err(|e| anyhow!(e.to_string()))?;
        info.cols = Some(pty_size.cols);
        info.rows = Some(pty_size.rows);
        Ok(())
    }

    /// Output so far (up to [`SCROLLBACK_BYTES`]) and the offset it ends at,
    /// for re-attaching a view.
    pub fn snapshot(&self, id: &str) -> Result<(String, u64)> {
        let snapshot = self.read_snapshot(id, false)?;
        Ok((snapshot.data, snapshot.end))
    }

    /// The first view of a running process also receives its pre-attachment output as live.
    pub fn attach_snapshot(&self, id: &str) -> Result<AttachmentSnapshot> {
        self.read_snapshot(id, true)
    }

    fn read_snapshot(&self, id: &str, attach: bool) -> Result<AttachmentSnapshot> {
        let t = match self.get(id) {
            Ok(t) => t,
            Err(e) => {
                let dir = self.store.as_ref().ok_or(e)?;
                let bytes = std::fs::read(saved_path(dir, id, "log")?)
                    .map_err(|_| anyhow!("no terminal {id}"))?;
                let trimmed = if bytes.len() >= SCROLLBACK_BYTES {
                    drop_partial_start(&bytes)
                } else {
                    &bytes[..]
                };
                return Ok(AttachmentSnapshot {
                    data: String::from_utf8_lossy(trimmed).into_owned(),
                    live: String::new(),
                    end: bytes.len() as u64,
                });
            }
        };
        let first_attach = attach && t.info.lock().unwrap().running;
        let mut sb = t.scrollback.lock().unwrap();
        let live_from = if first_attach {
            sb.startup_from.take()
        } else {
            None
        };
        let (a, b) = sb.buf.as_slices();
        let mut bytes = Vec::with_capacity(sb.buf.len());
        bytes.extend_from_slice(a);
        bytes.extend_from_slice(b);
        let trimmed = if sb.total > bytes.len() as u64 {
            drop_partial_start(&bytes)
        } else {
            &bytes[..]
        };
        let start = sb.total.saturating_sub(trimmed.len() as u64);
        let split = live_from.map_or(trimmed.len(), |offset| {
            offset.saturating_sub(start).min(trimmed.len() as u64) as usize
        });
        let (data, live) = trimmed.split_at(split);
        Ok(AttachmentSnapshot {
            data: String::from_utf8_lossy(data).into_owned(),
            live: String::from_utf8_lossy(live).into_owned(),
            end: sb.total,
        })
    }

    /// Stop the program if it is still running and forget the terminal.
    /// A saved thread from an earlier run is deleted from disk.
    pub fn close(&self, id: &str) -> Result<()> {
        let _guard = self.lifecycle.lock().unwrap();
        let removed = self.terms.lock().unwrap().remove(id);
        if let Some(t) = &removed {
            let _info = t.info.lock().unwrap();
            t.forgotten.store(true, Ordering::SeqCst);
        }
        if let Some(dir) = &self.store {
            let json = saved_path(dir, id, "json")?;
            let had = json.exists();
            let _ = std::fs::remove_file(json);
            let _ = std::fs::remove_file(saved_path(dir, id, "log")?);
            let _ = std::fs::remove_file(saved_path(dir, id, "usage.json")?);
            if removed.is_none() && had {
                return Ok(());
            }
        }
        let t = removed.ok_or_else(|| anyhow!("no terminal {id}"))?;
        if t.info.lock().unwrap().running {
            let _ = t.killer.lock().unwrap().kill();
        }
        Ok(())
    }

    /// Running and ended terminals of this run, then saved ones from earlier runs.
    pub fn list(&self) -> Vec<TermInfo> {
        let mut v: Vec<TermInfo> = self
            .terms
            .lock()
            .unwrap()
            .values()
            .map(|t| t.info.lock().unwrap().clone())
            .collect();
        if let Some(dir) = &self.store {
            let live: std::collections::HashSet<String> = v.iter().map(|i| i.id.clone()).collect();
            v.extend(
                load_saved(dir)
                    .into_iter()
                    .filter(|i| !live.contains(&i.id)),
            );
        }
        v.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)));
        v
    }

    pub fn info(&self, id: &str) -> Result<TermInfo> {
        Ok(self.get(id)?.info.lock().unwrap().clone())
    }

    pub fn credits_used(&self, id: &str) -> Option<f64> {
        let path = saved_path(self.store.as_ref()?, id, "usage.json").ok()?;
        let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        if data["aiCreditsStatus"] != "complete" {
            return None;
        }
        let nano = data["totalNanoAiu"].as_f64()?;
        (nano >= 0.0).then_some(nano / 1_000_000_000.0)
    }

    /// Save every terminal's details and output now (the app calls this now
    /// and then, so a crash loses little).
    pub fn save_all(&self) {
        let Some(dir) = &self.store else { return };
        let all: Vec<Arc<Term>> = self.terms.lock().unwrap().values().cloned().collect();
        for t in all {
            save_term(dir, &t);
        }
    }

    /// Stop every running program, e.g. when the app quits. With a store,
    /// the threads stay saved, marked as stopped by the app.
    pub fn close_all(&self) {
        let _guard = self.lifecycle.lock().unwrap();
        let all: Vec<Arc<Term>> = self.terms.lock().unwrap().drain().map(|(_, t)| t).collect();
        for t in all {
            let running = {
                let mut i = t.info.lock().unwrap();
                let r = i.running;
                if r {
                    i.running = false;
                    i.stopped_by_app = true;
                }
                r
            };
            if let Some(dir) = &self.store {
                save_term(dir, &t);
            }
            if running {
                let _ = t.killer.lock().unwrap().kill();
            }
        }
    }

    fn get(&self, id: &str) -> Result<Arc<Term>> {
        self.terms
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| anyhow!("no terminal {id}"))
    }
}

/// `<dir>/<id>.<ext>`, refusing ids that could reach outside `dir`.
fn saved_path(dir: &std::path::Path, id: &str, ext: &str) -> Result<PathBuf> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(anyhow!("no terminal {id}"));
    }
    Ok(dir.join(format!("{id}.{ext}")))
}

/// Write a terminal's details and output; written to a temporary file first
/// so a crash never leaves half a file.
fn save_term(dir: &std::path::Path, t: &Term) {
    save_term_locked(dir, t, &t.info.lock().unwrap());
}

fn save_term_locked(dir: &std::path::Path, t: &Term, info: &TermInfo) {
    if t.forgotten.load(Ordering::SeqCst) {
        return;
    }
    let (Ok(json), Ok(log)) = (
        saved_path(dir, &info.id, "json"),
        saved_path(dir, &info.id, "log"),
    ) else {
        return;
    };
    let mut bytes: Vec<u8> = t.scrollback.lock().unwrap().buf.iter().copied().collect();
    if let Some(redact) = &t.redactor {
        bytes = redact(&String::from_utf8_lossy(&bytes)).into_bytes();
    }
    let write = |p: &PathBuf, data: &[u8]| {
        let tmp = p.with_extension("tmp");
        if write_private(&tmp, data).is_ok() {
            let _ = std::fs::rename(&tmp, p);
        }
    };
    write(&log, &bytes);
    if let Ok(j) = serde_json::to_vec_pretty(info) {
        write(&json, &j);
    }
}

/// Saved output can hold secrets, so only the current user may read it.
fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(data)
}

/// Threads saved by earlier runs, as ended.
fn load_saved(dir: &std::path::Path) -> Vec<TermInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return vec![];
    };
    rd.flatten()
        .filter(|e| {
            e.path().extension().is_some_and(|x| x == "json")
                && !e.file_name().to_string_lossy().ends_with(".usage.json")
        })
        .filter_map(|e| std::fs::read(e.path()).ok())
        .filter_map(|b| serde_json::from_slice::<TermInfo>(&b).ok())
        .map(|mut i| {
            if i.running {
                // Saved while running and the app never closed it cleanly.
                i.running = false;
                i.stopped_by_app = true;
            }
            i.restored = true;
            i
        })
        .collect()
}

/// Keep only the newest `keep` saved threads.
fn prune(dir: &std::path::Path, keep: usize) {
    let mut all = load_saved(dir);
    if all.len() <= keep {
        return;
    }
    all.sort_by_key(|i| std::cmp::Reverse(i.started_at));
    for i in &all[keep..] {
        for ext in ["json", "log", "usage.json"] {
            if let Ok(p) = saved_path(dir, &i.id, ext) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(2),
        cols: cols.max(10),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Output cut off at the front of the buffer can start inside a UTF-8 character or an
/// escape sequence ("38;5;12m..."), which a terminal prints as stray text and then
/// misplaces the cursor. Skip that fragment.
fn drop_partial_start(bytes: &[u8]) -> &[u8] {
    let mut i = 0;
    while i < bytes.len() && (0x80..0xC0).contains(&bytes[i]) {
        i += 1;
    }
    let rest = &bytes[i..];
    let params = rest
        .iter()
        .take_while(|b| b.is_ascii_digit() || matches!(b, b';' | b':' | b'?' | b'<' | b'=' | b'>'))
        .count();
    if params > 0 && rest.get(params).is_some_and(|b| (0x40..=0x7E).contains(b)) {
        return &rest[params + 1..];
    }
    rest
}

fn push_scrollback(sb: &mut VecDeque<u8>, bytes: &[u8]) {
    sb.extend(bytes);
    let extra = sb.len().saturating_sub(SCROLLBACK_BYTES);
    sb.drain(..extra);
}

fn command_line(program: &str, args: &[String]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Turns a byte stream into text without breaking multi-byte characters that
/// are split across reads.
#[derive(Default)]
pub struct Utf8Carry {
    pending: Vec<u8>,
}

impl Utf8Carry {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let cut = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(), // incomplete tail: keep it
            Err(_) => self.pending.len(),                         // invalid bytes: replace them
        };
        let rest = self.pending.split_off(cut);
        let text = String::from_utf8_lossy(&self.pending).into_owned();
        self.pending = rest;
        text
    }
}

/// The user's shell and the arguments that run `line` in it as a login shell,
/// so PATH matches what they see in their own terminal (apps opened from the
/// Dock do not inherit it).
pub fn shell_command(line: Option<&str>) -> (String, Vec<String>) {
    if cfg!(windows) {
        let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
        match line {
            Some(l) => (shell, vec!["/C".into(), l.into()]),
            None => (shell, vec![]),
        }
    } else {
        let shell = std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                if std::path::Path::new("/bin/zsh").exists() {
                    "/bin/zsh".into()
                } else {
                    "/bin/sh".into()
                }
            });
        match line {
            Some(l) => (shell, vec!["-l".into(), "-c".into(), l.into()]),
            None => (shell, vec!["-l".into()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn spec(program: &str, args: &[&str]) -> Spawn {
        Spawn {
            program: program.into(),
            args: args.iter().map(|s| s.to_string()).collect(),
            cwd: std::env::temp_dir(),
            env: vec![("EVYRO_TERM_TEST".into(), "marker-42".into())],
            cols: 100,
            rows: 30,
            title: "test".into(),
            kind: "shell".into(),
            target: None,
            task: None,
            project_id: None,
            credit_budget: None,
        }
    }

    fn collect() -> (
        OnOutput,
        OnExit,
        Arc<Mutex<String>>,
        mpsc::Receiver<Option<u32>>,
    ) {
        let out = Arc::new(Mutex::new(String::new()));
        let o = out.clone();
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        (
            Arc::new(move |_id: &str, s: &str, _end: u64| o.lock().unwrap().push_str(s)),
            Arc::new(move |_id: &str, c| {
                let _ = tx.lock().unwrap().send(c);
            }),
            out,
            rx,
        )
    }

    /// Windows ConPTY asks the terminal for the cursor position (ESC[6n) at
    /// startup and waits for the answer. xterm.js answers in the app; here the
    /// test plays the terminal.
    fn wait_answering_cursor_query(
        terms: &Terminals,
        id: &str,
        out: &Arc<Mutex<String>>,
        rx: &mpsc::Receiver<Option<u32>>,
    ) -> Option<u32> {
        let mut answered = false;
        for _ in 0..400 {
            if let Ok(code) = rx.try_recv() {
                return code;
            }
            if !answered && out.lock().unwrap().contains("\x1b[6n") {
                let _ = terms.write(id, "\x1b[1;1R");
                answered = true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("program did not exit; output: {:?}", out.lock().unwrap());
    }

    fn echo_env() -> Spawn {
        if cfg!(windows) {
            spec("cmd.exe", &["/C", "echo %EVYRO_TERM_TEST%"])
        } else {
            spec("/bin/sh", &["-c", "echo $EVYRO_TERM_TEST; exit 3"])
        }
    }

    #[test]
    fn runs_a_program_and_reports_output_and_exit() {
        let terms = Terminals::new();
        let (o, e, out, rx) = collect();
        let info = terms.open(echo_env(), o, e).unwrap();
        assert!(info.running);
        let code = wait_answering_cursor_query(&terms, &info.id, &out, &rx);
        if cfg!(windows) {
            assert_eq!(code, Some(0));
        } else {
            assert_eq!(code, Some(3));
        }
        // Output may still be arriving on Windows; wait for it briefly.
        for _ in 0..50 {
            if out.lock().unwrap().contains("marker-42") {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            out.lock().unwrap().contains("marker-42"),
            "output: {:?}",
            out.lock().unwrap()
        );
        let (snap, end) = terms.snapshot(&info.id).unwrap();
        assert!(snap.contains("marker-42"));
        assert_eq!(end as usize, snap.len());
        let after = terms.info(&info.id).unwrap();
        assert!(!after.running);
        assert_eq!(terms.list().len(), 1);
        terms.close(&info.id).unwrap();
        assert!(terms.list().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn first_attachment_only_exposes_the_current_process_query_as_live() {
        let terms = Terminals::new();
        let query = "\x1b[2;5H\x1b[6n";
        let program = spec("/bin/sh", &[
            "-c",
            "stty -icanon -echo min 0 time 50; printf '\\033[2;5H\\033[6n'; test \"$(dd bs=1 count=6 2>/dev/null)\" = \"$(printf '\\033[2;5R')\"",
        ]);
        let verify_attachment =
            |id: &str, out: Arc<Mutex<String>>, rx: mpsc::Receiver<Option<u32>>| {
                for _ in 0..200 {
                    if out.lock().unwrap().contains(query) {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                let (full, end) = terms.snapshot(id).unwrap();
                assert!(
                    full.ends_with(query),
                    "startup query did not arrive: {full:?}"
                );
                let first = terms.attach_snapshot(id).unwrap();
                assert_eq!(first.end, end);
                assert_eq!(first.live, query);
                assert_eq!(first.data, &full[..full.len() - query.len()]);
                let again = terms.attach_snapshot(id).unwrap();
                assert_eq!(again.data, full);
                assert!(again.live.is_empty());
                terms.write(id, "\x1b[2;5R").unwrap();
                assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), Some(0));
                first.data
            };
        let (o, e, out, rx) = collect();
        let info = terms.open(program.clone(), o, e).unwrap();
        assert!(verify_attachment(&info.id, out, rx).is_empty());
        let (o, e, out, rx) = collect();
        terms.resume(&info.id, program, o, e).unwrap();
        let history = verify_attachment(&info.id, out, rx);
        assert!(history.contains(query));
        assert!(history.contains("[Resumed Evyro thread]"));
    }

    #[cfg(unix)]
    #[test]
    fn saved_output_is_redacted_but_the_live_view_is_not() {
        let dir = std::env::temp_dir().join(format!("evyro-term-redact-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let secret = format!("ghp_{}", "A".repeat(36));
        let terms = Terminals::with_store(dir.clone()).with_redactor(Arc::new({
            let secret = secret.clone();
            move |text| text.replace(&secret, "[REDACTED]")
        }));
        let (o, e, _, rx) = collect();
        let info = terms
            .open(
                spec("/bin/sh", &["-c", &format!("echo key {secret}")]),
                o,
                e,
            )
            .unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), Some(0));
        terms.save_all();
        let (live, _) = terms.snapshot(&info.id).unwrap();
        assert!(live.contains(&secret), "the live view must stay unchanged");
        let saved = std::fs::read_to_string(dir.join(format!("{}.log", info.id))).unwrap();
        assert!(
            saved.contains("[REDACTED]") && !saved.contains(&secret),
            "{saved:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn saved_threads_come_back_after_a_restart() {
        let dir = std::env::temp_dir().join(format!("evyro-term-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = Terminals::with_store(dir.clone());
        let (o, e, out, rx) = collect();
        let done = first.open(echo_env(), o, e).unwrap();
        let code = wait_answering_cursor_query(&first, &done.id, &out, &rx);
        assert_eq!(code, Some(3));
        let (o, e, _, _) = collect();
        let live = first.open(spec("/bin/cat", &[]), o, e).unwrap();
        first.resize(&live.id, 120, 40).unwrap();
        first.close_all(); // the app quits with cat still running
        {
            use std::os::unix::fs::PermissionsExt;
            let mode =
                |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir), 0o700);
            assert_eq!(mode(&dir.join(format!("{}.log", live.id))), 0o600);
            assert_eq!(mode(&dir.join(format!("{}.json", live.id))), 0o600);
        }

        let second = Terminals::with_store(dir.clone());
        let saved = second.list();
        assert_eq!(saved.len(), 2, "{saved:?}");
        assert!(saved.iter().all(|i| i.restored && !i.running));
        let d = saved.iter().find(|i| i.id == done.id).unwrap();
        assert_eq!(d.exit_code, Some(3));
        assert!(!d.stopped_by_app);
        let saved_live = saved.iter().find(|i| i.id == live.id).unwrap();
        assert_eq!((saved_live.cols, saved_live.rows), (Some(120), Some(40)));
        assert!(
            saved
                .iter()
                .find(|i| i.id == live.id)
                .unwrap()
                .stopped_by_app
        );
        assert!(second.snapshot(&done.id).unwrap().0.contains("marker-42"));
        let (o, e, out, rx) = collect();
        let mut retry = echo_env();
        retry.env[0].1 = "marker-second".into();
        let resumed = second.resume(&done.id, retry, o, e).unwrap();
        assert_eq!(resumed.id, done.id);
        assert_eq!(resumed.started_at, done.started_at);
        assert_eq!(second.list().len(), 2);
        wait_answering_cursor_query(&second, &done.id, &out, &rx);
        let (transcript, _) = second.snapshot(&done.id).unwrap();
        assert!(transcript.contains("marker-42"));
        assert!(transcript.contains("Resumed Evyro thread"));
        assert!(transcript.contains("marker-second"));
        let (o, e, out, rx) = collect();
        let echo_input = spec(
            "/bin/sh",
            &[
                "-c",
                "IFS= read -r line && printf 'received: %s\\n' \"$line\"",
            ],
        );
        let restarted_shell = second.resume(&live.id, echo_input, o, e).unwrap();
        assert_eq!(restarted_shell.id, live.id);
        assert_eq!(
            (restarted_shell.cols, restarted_shell.rows),
            (Some(120), Some(40))
        );
        second.write(&live.id, "new input\r").unwrap();
        assert_eq!(
            wait_answering_cursor_query(&second, &live.id, &out, &rx),
            Some(0)
        );
        assert!(second
            .snapshot(&live.id)
            .unwrap()
            .0
            .contains("received: new input"));
        // A new run never reuses a saved id.
        let (o, e, _, _) = collect();
        let fresh = second.open(spec("/bin/cat", &[]), o, e).unwrap();
        assert!(saved.iter().all(|i| i.id != fresh.id));
        second.close(&fresh.id).unwrap();
        // Closing a saved thread forgets it; odd ids are refused.
        second.close(&done.id).unwrap();
        assert!(second.snapshot(&done.id).is_err());
        assert!(second.snapshot("../x").is_err());
        assert_eq!(second.list().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn input_reaches_the_program_and_close_stops_it() {
        let terms = Terminals::new();
        let (o, e, out, rx) = collect();
        let info = terms.open(spec("/bin/cat", &[]), o, e).unwrap();
        terms.resize(&info.id, 120, 40).unwrap();
        terms.write(&info.id, "hello-from-input\n").unwrap();
        for _ in 0..100 {
            if out.lock().unwrap().contains("hello-from-input") {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(out.lock().unwrap().contains("hello-from-input"));
        assert!(terms.info(&info.id).unwrap().running);
        terms.close(&info.id).unwrap();
        rx.recv_timeout(Duration::from_secs(10))
            .expect("cat should stop after close");
        assert!(terms.write(&info.id, "x").is_err());
    }

    #[test]
    fn utf8_split_across_reads_is_kept_whole() {
        let mut c = Utf8Carry::default();
        let bytes = "a✓b".as_bytes(); // ✓ is three bytes
        assert_eq!(c.push(&bytes[..2]), "a");
        assert_eq!(c.push(&bytes[2..3]), "");
        assert_eq!(c.push(&bytes[3..]), "✓b");
        assert_eq!(c.push(&[0xff, b'z']), "\u{fffd}z");
    }

    #[test]
    fn scrollback_is_bounded() {
        let mut sb = VecDeque::new();
        push_scrollback(&mut sb, &vec![b'x'; SCROLLBACK_BYTES]);
        push_scrollback(&mut sb, b"tail");
        assert_eq!(sb.len(), SCROLLBACK_BYTES);
        assert_eq!(
            sb.iter().rev().take(4).rev().copied().collect::<Vec<_>>(),
            b"tail"
        );
    }

    #[test]
    fn shell_command_wraps_a_line() {
        let (_, args) = shell_command(Some("evyro run --to codex"));
        assert_eq!(args.last().unwrap(), "evyro run --to codex");
    }
}

#[cfg(test)]
mod partial_start_tests {
    use super::drop_partial_start;

    #[test]
    fn skips_a_cut_escape_sequence_and_utf8_tail() {
        assert_eq!(drop_partial_start(b"38;5;12mhello"), b"hello");
        assert_eq!(drop_partial_start(&[0xA9, b'h', b'i']), b"hi");
        assert_eq!(drop_partial_start(b"plain"), b"plain");
        assert_eq!(drop_partial_start(b"\x1b[0mok"), b"\x1b[0mok");
    }
}
