//! Verification receipts: Evyro runs the command itself and records evidence
//! (argv, exit code, timing, commit, repo fingerprint, output hash). Agent
//! claims never produce receipts.

use crate::db::{from_json, to_json, Store};
use crate::error::{Error, Result};
use crate::{git, redact, util};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub command: Vec<String>,
    pub exit_code: Option<i32>,
    pub passed: bool,
    pub started_at: String,
    pub duration_ms: i64,
    pub commit_sha: Option<String>,
    pub fingerprint: Option<String>,
    pub worktree: String,
    pub environment: serde_json::Value,
    pub output_path: Option<String>,
    pub output_sha256: Option<String>,
}

const V_COLS: &str = "id, project_id, task_id, session_id, command, exit_code, passed, started_at, duration_ms, commit_sha, fingerprint, worktree, environment, output_path, output_sha256";

fn row_to_receipt(r: &rusqlite::Row) -> rusqlite::Result<Receipt> {
    Ok(Receipt {
        id: r.get(0)?,
        project_id: r.get(1)?,
        task_id: r.get(2)?,
        session_id: r.get(3)?,
        command: from_json(r.get(4)?),
        exit_code: r.get(5)?,
        passed: r.get::<_, i64>(6)? != 0,
        started_at: r.get(7)?,
        duration_ms: r.get(8)?,
        commit_sha: r.get(9)?,
        fingerprint: r.get(10)?,
        worktree: r.get(11)?,
        environment: serde_json::from_str(&r.get::<_, String>(12)?).unwrap_or_default(),
        output_path: r.get(13)?,
        output_sha256: r.get(14)?,
    })
}

pub struct RunResult {
    pub receipt: Receipt,
    /// Redacted combined output (tail), for display.
    pub output_tail: String,
}

impl Store {
    pub fn run_verification(
        &self,
        project_id: &str,
        task_id: Option<&str>,
        session_id: Option<&str>,
        argv: &[String],
        cwd: &Path,
    ) -> Result<RunResult> {
        if argv.is_empty() {
            return Err(Error::Invalid("verification command is empty".into()));
        }
        let started_at = util::now();
        let t0 = Instant::now();
        let out = Command::new(&argv[0])
            .args(&argv[1..])
            .current_dir(cwd)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| Error::Invalid(format!("cannot run `{}`: {e}", argv[0])))?;
        let duration_ms = t0.elapsed().as_millis() as i64;
        let mut combined = out.stdout.clone();
        combined.extend_from_slice(&out.stderr);
        let (text, _) = redact::redact(&String::from_utf8_lossy(&combined));

        let id = util::new_id("ver");
        let log_dir = self.paths.logs_dir().join("verifications");
        std::fs::create_dir_all(&log_dir)?;
        let log_path = log_dir.join(format!("{id}.log"));
        std::fs::write(&log_path, text.as_bytes())?;
        let output_sha = util::sha256_hex(text.as_bytes());

        let snap = git::snapshot(cwd).ok();
        let passed = out.status.success();
        let env = serde_json::json!({
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "evyro": crate::brand::VERSION,
        });
        self.conn.execute(
            &format!("INSERT INTO verifications({V_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)"),
            rusqlite::params![
                id, project_id, task_id, session_id, to_json(&argv), out.status.code(), passed, started_at, duration_ms,
                snap.as_ref().and_then(|s| s.head.clone()), snap.as_ref().map(|s| s.fingerprint()),
                cwd.to_string_lossy(), env.to_string(), log_path.to_string_lossy(), output_sha
            ],
        )?;
        self.emit(Some(project_id), task_id, session_id, "verification.completed",
            serde_json::json!({"receipt_id": id, "command": argv, "passed": passed, "exit_code": out.status.code()}))?;
        let tail: String = {
            let lines: Vec<&str> = text.lines().collect();
            lines[lines.len().saturating_sub(40)..].join("\n")
        };
        Ok(RunResult {
            receipt: self.get_receipt(&id)?,
            output_tail: tail,
        })
    }

    pub fn get_receipt(&self, id: &str) -> Result<Receipt> {
        self.conn
            .query_row(
                &format!("SELECT {V_COLS} FROM verifications WHERE id = ?1"),
                [id],
                row_to_receipt,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => {
                    Error::NotFound(format!("verification {id}"))
                }
                e => e.into(),
            })
    }

    /// Receipts in chronological order.
    pub fn list_verifications(
        &self,
        project_id: &str,
        task_id: Option<&str>,
    ) -> Result<Vec<Receipt>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {V_COLS} FROM verifications WHERE project_id = ?1 AND (?2 IS NULL OR task_id = ?2) ORDER BY started_at, rowid"
        ))?;
        let rows = st.query_map(rusqlite::params![project_id, task_id], row_to_receipt)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// Suggest verification commands for changed files given the project's
/// configured commands (task-aware verification, never auto-run).
pub fn suggest_commands(
    commands: &crate::projects::Commands,
    changed: &[String],
) -> Vec<Vec<String>> {
    let touches = |exts: &[&str]| changed.iter().any(|f| exts.iter().any(|e| f.ends_with(e)));
    let mut out: Vec<Vec<String>> = Vec::new();
    for t in &commands.test {
        let tool = t.first().map(|s| s.as_str()).unwrap_or("");
        let relevant = match tool {
            "cargo" => touches(&[".rs", "Cargo.toml", "Cargo.lock"]),
            "go" => touches(&[".go", "go.mod"]),
            "python" | "pytest" => touches(&[".py", "pyproject.toml"]),
            "npm" | "pnpm" | "yarn" | "bun" => {
                touches(&[".js", ".ts", ".tsx", ".jsx", "package.json"])
            }
            _ => true,
        };
        if relevant || changed.is_empty() {
            out.push(t.clone());
        }
    }
    if out.is_empty() {
        out.extend(commands.test.iter().cloned());
    }
    out
}
