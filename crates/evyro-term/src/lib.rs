//! Pseudo-terminals for agent threads.
//!
//! The desktop app runs each agent (or a plain shell) in its own pseudo-terminal,
//! so interactive CLIs such as Claude Code or Codex work exactly as they do in a
//! terminal window. Output is streamed to a callback as UTF-8 text and kept in a
//! bounded scrollback, so a thread can be re-attached after the view changes.

use anyhow::{anyhow, Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
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
}

#[derive(Debug, Clone, Serialize)]
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
}

struct Term {
    info: Mutex<TermInfo>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    scrollback: Mutex<Scrollback>,
}

#[derive(Default)]
struct Scrollback {
    buf: VecDeque<u8>,
    /// Bytes of output since the start, including bytes dropped from `buf`.
    total: u64,
}

/// All terminals of one app instance.
#[derive(Default)]
pub struct Terminals {
    terms: Mutex<HashMap<String, Arc<Term>>>,
    next: AtomicU64,
}

/// `(id, text, end)`: `end` is the byte offset of the end of this chunk in the
/// terminal's whole output, so a view can drop chunks a snapshot already holds.
pub type OnOutput = Arc<dyn Fn(&str, &str, u64) + Send + Sync>;
pub type OnExit = Arc<dyn Fn(&str, Option<u32>) + Send + Sync>;

impl Terminals {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start `spec` in a new pseudo-terminal. `on_output(id, text)` receives
    /// output as it arrives (see [`OnOutput`]); `on_exit(id, code)` is called once when the
    /// program ends. Returns the terminal id.
    pub fn open(&self, spec: Spawn, on_output: OnOutput, on_exit: OnExit) -> Result<TermInfo> {
        let id = format!("term_{}", self.next.fetch_add(1, Ordering::SeqCst) + 1);
        let pair = native_pty_system()
            .openpty(size(spec.cols, spec.rows))
            .context("could not open a pseudo-terminal")?;
        let mut cmd = CommandBuilder::new(&spec.program);
        cmd.args(&spec.args);
        cmd.cwd(&spec.cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .with_context(|| format!("could not start {}", spec.program))?;
        drop(pair.slave);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;

        let info = TermInfo {
            id: id.clone(),
            title: spec.title.clone(),
            kind: spec.kind.clone(),
            target: spec.target.clone(),
            task: spec.task.clone(),
            project_id: spec.project_id.clone(),
            cwd: spec.cwd.to_string_lossy().into_owned(),
            command: command_line(&spec.program, &spec.args),
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            running: true,
            exit_code: None,
        };
        let term = Arc::new(Term {
            info: Mutex::new(info.clone()),
            writer: Mutex::new(writer),
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            scrollback: Mutex::new(Scrollback::default()),
        });
        self.terms.lock().unwrap().insert(id.clone(), term.clone());

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
                        if !text.is_empty() {
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
        let m = t.master.lock().unwrap();
        m.resize(size(cols, rows))
            .map_err(|e| anyhow!(e.to_string()))
    }

    /// Output so far (up to [`SCROLLBACK_BYTES`]) and the offset it ends at,
    /// for re-attaching a view.
    pub fn snapshot(&self, id: &str) -> Result<(String, u64)> {
        let t = self.get(id)?;
        let sb = t.scrollback.lock().unwrap();
        let (a, b) = sb.buf.as_slices();
        let mut bytes = Vec::with_capacity(sb.buf.len());
        bytes.extend_from_slice(a);
        bytes.extend_from_slice(b);
        Ok((String::from_utf8_lossy(&bytes).into_owned(), sb.total))
    }

    /// Stop the program if it is still running and forget the terminal.
    pub fn close(&self, id: &str) -> Result<()> {
        let t = self
            .terms
            .lock()
            .unwrap()
            .remove(id)
            .ok_or_else(|| anyhow!("no terminal {id}"))?;
        if t.info.lock().unwrap().running {
            let _ = t.killer.lock().unwrap().kill();
        }
        Ok(())
    }

    pub fn list(&self) -> Vec<TermInfo> {
        let mut v: Vec<TermInfo> = self
            .terms
            .lock()
            .unwrap()
            .values()
            .map(|t| t.info.lock().unwrap().clone())
            .collect();
        v.sort_by(|a, b| a.started_at.cmp(&b.started_at).then(a.id.cmp(&b.id)));
        v
    }

    pub fn info(&self, id: &str) -> Result<TermInfo> {
        Ok(self.get(id)?.info.lock().unwrap().clone())
    }

    /// Stop every running program, e.g. when the app quits.
    pub fn close_all(&self) {
        let ids: Vec<String> = self.terms.lock().unwrap().keys().cloned().collect();
        for id in ids {
            let _ = self.close(&id);
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

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.max(2),
        cols: cols.max(10),
        pixel_width: 0,
        pixel_height: 0,
    }
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
