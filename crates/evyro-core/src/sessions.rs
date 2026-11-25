//! Durable agent sessions with an explicit lifecycle and task leases.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::util;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Created,
    Starting,
    Running,
    Pausing,
    Paused,
    Checkpointing,
    HandingOff,
    Recovering,
    Completed,
    Failed,
    Cancelled,
}

impl SessionStatus {
    pub const ALL: [SessionStatus; 11] = [
        Self::Created,
        Self::Starting,
        Self::Running,
        Self::Pausing,
        Self::Paused,
        Self::Checkpointing,
        Self::HandingOff,
        Self::Recovering,
        Self::Completed,
        Self::Failed,
        Self::Cancelled,
    ];
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Checkpointing => "checkpointing",
            Self::HandingOff => "handing_off",
            Self::Recovering => "recovering",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn is_ended(&self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
    pub fn can_transition_to(&self, to: SessionStatus) -> bool {
        use SessionStatus::*;
        if self.is_ended() {
            return false;
        }
        if *self == to || matches!(to, Failed | Cancelled | Recovering) {
            return true;
        }
        matches!(
            (self, to),
            (Created, Starting)
                | (Created, Running)
                | (Starting, Running)
                | (Running, Pausing)
                | (Running, Paused)
                | (Running, Checkpointing)
                | (Running, HandingOff)
                | (Running, Completed)
                | (Pausing, Paused)
                | (Paused, Running)
                | (Paused, HandingOff)
                | (Paused, Completed)
                | (Checkpointing, Running)
                | (Checkpointing, HandingOff)
                | (HandingOff, Completed)
                | (HandingOff, Running)
                | (Recovering, Running)
                | (Recovering, Paused)
                | (Recovering, HandingOff)
                | (Recovering, Completed)
        )
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SessionStatus {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| Error::Invalid(format!("unknown session status '{s}'")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub harness: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub worktree: Option<String>,
    pub pid: Option<i64>,
    /// When the process with `pid` started (Unix seconds), if known.
    pub pid_started: Option<i64>,
    pub status: SessionStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_reason: Option<String>,
    pub last_event_at: Option<String>,
    pub latest_checkpoint_id: Option<String>,
    pub handoff_id: Option<String>,
}

/// How long a session without a recorded process may stay silent before it counts as gone.
pub const HEARTBEAT_TIMEOUT_SECS: i64 = 30 * 60;

impl Session {
    /// Whether this session still looks alive. With a recorded process that is the process;
    /// without one it is whether the session reported in (any event, checkpoint or
    /// `evyro heartbeat`) within [`HEARTBEAT_TIMEOUT_SECS`].
    pub fn is_live(&self) -> bool {
        self.is_live_at(time::OffsetDateTime::now_utc().unix_timestamp())
    }

    pub(crate) fn is_live_at(&self, now: i64) -> bool {
        if let Some(pid) = self.pid {
            return session_process_alive(pid, self.pid_started);
        }
        let seen = self.last_event_at.as_deref().unwrap_or(&self.started_at);
        time::OffsetDateTime::parse(seen, &time::format_description::well_known::Rfc3339)
            .map_or(true, |t| now - t.unix_timestamp() <= HEARTBEAT_TIMEOUT_SECS)
    }
}

/// A task and the AI chosen to start it.
#[derive(Debug, Clone, Serialize)]
pub struct NextWork {
    pub task: crate::tasks::Task,
    pub agent: String,
}

#[derive(Debug, Clone, Default)]
pub struct NewSession {
    pub task_id: Option<String>,
    pub harness: String,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub worktree: Option<String>,
    pub pid: Option<i64>,
    pub handoff_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lease {
    pub task_id: String,
    pub session_id: String,
    pub harness: String,
    pub acquired_at: String,
}

const SESSION_COLS: &str = "id, project_id, task_id, harness, model, provider, worktree, pid, status, started_at, ended_at, exit_reason, last_event_at, latest_checkpoint_id, handoff_id, pid_started";

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<Session> {
    let st: String = r.get(8)?;
    Ok(Session {
        id: r.get(0)?,
        project_id: r.get(1)?,
        task_id: r.get(2)?,
        harness: r.get(3)?,
        model: r.get(4)?,
        provider: r.get(5)?,
        worktree: r.get(6)?,
        pid: r.get(7)?,
        status: st.parse().unwrap_or(SessionStatus::Failed),
        started_at: r.get(9)?,
        ended_at: r.get(10)?,
        exit_reason: r.get(11)?,
        last_event_at: r.get(12)?,
        latest_checkpoint_id: r.get(13)?,
        handoff_id: r.get(14)?,
        pid_started: r.get(15)?,
    })
}

/// When process `pid` started, in Unix seconds (to about a second), or `None`
/// when there is no such process or the system will not say.
pub fn process_start_time(pid: i64) -> Option<i64> {
    if pid <= 0 {
        return None;
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    #[cfg(unix)]
    {
        // `etime` is [[dd-]hh:]mm:ss on both Linux and macOS.
        let out = std::process::Command::new("ps")
            .args(["-o", "etime=", "-p", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let elapsed = parse_etime(String::from_utf8_lossy(&out.stdout).trim())?;
        Some(now - elapsed)
    }
    #[cfg(windows)]
    {
        let script = format!(
            "[DateTimeOffset]::new((Get-Process -Id {pid} -ErrorAction Stop).StartTime).ToUnixTimeSeconds()"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let _ = now;
        String::from_utf8_lossy(&out.stdout).trim().parse().ok()
    }
}

/// Parse `ps -o etime` output: `[[dd-]hh:]mm:ss`.
fn parse_etime(s: &str) -> Option<i64> {
    let (days, rest) = match s.split_once('-') {
        Some((d, r)) => (d.parse::<i64>().ok()?, r),
        None => (0, s),
    };
    let parts: Vec<i64> = rest
        .split(':')
        .map(|p| p.parse::<i64>().ok())
        .collect::<Option<_>>()?;
    let (h, m, sec) = match parts.as_slice() {
        [m, s] => (0, *m, *s),
        [h, m, s] => (*h, *m, *s),
        _ => return None,
    };
    Some(((days * 24 + h) * 60 + m) * 60 + sec)
}

/// A recorded process is still the one Evyro started: it exists and, when
/// its start time was recorded, started at that time. A pid reused by an
/// unrelated process after the agent exited does not count.
/// Whether two declared task paths are the same or one contains the other.
fn paths_overlap(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.trim_start_matches("./").trim_end_matches('/').to_owned();
    let (a, b) = (norm(a), norm(b));
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

/// Ask the process of a session, and everything it started, to stop. Refuses a pid that
/// now belongs to a different process. Returns whether a stop signal was sent.
pub fn stop_session_process(pid: i64, recorded_start: Option<i64>) -> bool {
    if !session_process_alive(pid, recorded_start) {
        return false;
    }
    #[cfg(unix)]
    {
        // An agent's shell commands (tests, servers, sleeps) are its children and would
        // otherwise outlive it, so stop the whole tree, deepest first.
        let mut tree = process_descendants(pid);
        tree.reverse();
        tree.push(pid);
        let mut sent = false;
        for p in tree {
            sent |= std::process::Command::new("kill")
                .args(["-TERM", &p.to_string()])
                .status()
                .is_ok_and(|s| s.success());
        }
        sent
    }
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status()
            .is_ok_and(|s| s.success())
    }
}

/// Every process started by `pid`, parents before children.
#[cfg(unix)]
fn process_descendants(pid: i64) -> Vec<i64> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
    else {
        return Vec::new();
    };
    let table: Vec<(i64, i64)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut f = l.split_whitespace();
            Some((f.next()?.parse().ok()?, f.next()?.parse().ok()?))
        })
        .collect();
    let mut found = vec![pid];
    let mut i = 0;
    while i < found.len() {
        let parent = found[i];
        found.extend(
            table
                .iter()
                .filter(|(_, pp)| *pp == parent)
                .map(|(p, _)| *p),
        );
        i += 1;
    }
    found.remove(0);
    found
}

pub fn session_process_alive(pid: i64, recorded_start: Option<i64>) -> bool {
    if !pid_alive(pid) {
        return false;
    }
    match (recorded_start, process_start_time(pid)) {
        (Some(want), Some(got)) => (want - got).abs() <= 3,
        _ => true,
    }
}

/// Best-effort liveness check for a recorded process id.
pub fn pid_alive(pid: i64) -> bool {
    if pid <= 0 {
        return false;
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    }
}

impl Store {
    /// Start a session. If it is for a task, takes the task lease; a lease held
    /// by another live session is a conflict (no silent takeover).
    pub fn start_session(&self, project_id: &str, s: NewSession) -> Result<Session> {
        crate::adapters::validate_harness_name(&s.harness)?;
        let id = util::new_id("ses");
        let now = util::now();
        if let Some(tid) = &s.task_id {
            self.check_claim(project_id, tid, &s.harness, None)?;
        }
        self.conn.execute(
            &format!("INSERT INTO sessions({SESSION_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'running',?9,NULL,NULL,?9,NULL,?10,NULL)"),
            rusqlite::params![id, project_id, s.task_id, s.harness, s.model, s.provider, s.worktree, s.pid, now, s.handoff_id],
        )?;
        if let Some(pid) = s.pid {
            self.conn.execute(
                "UPDATE sessions SET pid_started = ?1 WHERE id = ?2",
                rusqlite::params![process_start_time(pid), id],
            )?;
        }
        if let Some(tid) = &s.task_id {
            self.conn.execute(
                "INSERT OR REPLACE INTO leases(task_id, session_id, harness, acquired_at) VALUES (?1,?2,?3,?4)",
                rusqlite::params![tid, id, s.harness, now],
            )?;
            self.conn.execute(
                "UPDATE tasks SET assigned_agent = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![s.harness, now, tid],
            )?;
        }
        self.emit(Some(project_id), s.task_id.as_deref(), Some(&id), "session.started",
            serde_json::json!({"harness": s.harness, "model": s.model, "provider": s.provider, "handoff_id": s.handoff_id}))?;
        self.get_session(&id)
    }

    /// Whether `harness` may take `task_id`: not blocked, and not owned by a
    /// live session other than `by_session`.
    fn check_claim(
        &self,
        project_id: &str,
        task_id: &str,
        harness: &str,
        by_session: Option<&str>,
    ) -> Result<()> {
        let t = self.get_task(task_id)?;
        if t.project_id != project_id {
            return Err(Error::Invalid("task belongs to a different project".into()));
        }
        let blockers = self.open_blockers(task_id)?;
        if !blockers.is_empty() {
            return Err(Error::Conflict(format!(
                "{} is blocked by unfinished dependencies: {}",
                t.key,
                blockers
                    .iter()
                    .map(|b| b.key.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        if let Some(l) = self.lease(task_id)? {
            let holder = self.get_session(&l.session_id)?;
            if !holder.status.is_ended() && Some(holder.id.as_str()) != by_session {
                return Err(Error::Conflict(format!(
                    "{} is owned by session {} ({}); if its AI is out of usage, switch the task with `evyro switch --task {} --to {} --yes`, otherwise hand off or end that session first",
                    t.key, holder.id, holder.harness, t.key, harness
                )));
            }
        }
        self.check_file_overlap(&t, by_session)
    }

    /// One AI at a time on a set of files: refuse a task whose declared files overlap a task
    /// that another live session holds in the same checkout.
    fn check_file_overlap(&self, t: &crate::tasks::Task, by_session: Option<&str>) -> Result<()> {
        if t.files.is_empty() {
            return Ok(());
        }
        let mut st = self
            .conn
            .prepare("SELECT task_id, session_id FROM leases WHERE task_id != ?1")?;
        let held: Vec<(String, String)> = st
            .query_map([&t.id], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<std::result::Result<_, _>>()?;
        for (other_id, session_id) in held {
            let holder = self.get_session(&session_id)?;
            if holder.status.is_ended() || Some(holder.id.as_str()) == by_session {
                continue;
            }
            let other = self.get_task(&other_id)?;
            if other.worktree != t.worktree {
                continue;
            }
            if let Some(file) = t
                .files
                .iter()
                .find(|a| other.files.iter().any(|b| paths_overlap(a, b)))
            {
                return Err(Error::Conflict(format!(
                    "{} and {} both change {file}, and {} is held by session {} ({}). Only one AI works on a set of files at a time: finish or switch that task first",
                    t.key, other.key, other.key, holder.id, holder.harness
                )));
            }
        }
        Ok(())
    }

    /// What to start next with nothing named: an installed AI that is not working on anything
    /// (or the one given) and the most important ready task it can start (or the one given).
    pub fn next_work(
        &self,
        project_id: &str,
        agent: Option<&str>,
        task: Option<&str>,
        under: Option<&str>,
    ) -> Result<NextWork> {
        let agent = match agent {
            Some(a) => a.to_owned(),
            None => {
                let busy: std::collections::HashSet<String> = self
                    .list_sessions(project_id, true)?
                    .into_iter()
                    .map(|s| s.harness)
                    .collect();
                let usable: Vec<_> = crate::adapters::detect_all()
                    .into_iter()
                    .filter(|d| {
                        d.installed
                            && matches!(d.compatibility.as_str(), "ok" | "unknown_version")
                            && (d.capabilities.headless_launch || d.capabilities.interactive_launch)
                    })
                    .collect();
                usable
                    .iter()
                    .find(|d| !busy.contains(&d.id))
                    .or_else(|| usable.first())
                    .map(|d| d.id.clone())
                    .ok_or_else(|| {
                        Error::NotFound("no installed AI found; run `evyro doctor`".into())
                    })?
            }
        };
        let task = match task {
            Some(r) => self.resolve_task(project_id, r)?,
            None => {
                let parent = match under {
                    Some(r) => Some(self.resolve_task(project_id, r)?),
                    None => None,
                };
                self.next_claimable_under(project_id, &agent, parent.as_ref())?
                    .ok_or_else(|| match &parent {
                        Some(p) => Error::NotFound(format!(
                            "no ready piece of {} that {agent} can start: finish a blocker or split more",
                            p.key
                        )),
                        None => Error::NotFound(format!(
                            "no ready task {agent} can start: add one, or finish a blocker"
                        )),
                    })?
            }
        };
        Ok(NextWork { task, agent })
    }

    /// The most important ready task `harness` could start right now: not blocked, not held,
    /// and not overlapping the files of a task another live session holds.
    pub fn next_claimable_task(
        &self,
        project_id: &str,
        harness: &str,
    ) -> Result<Option<crate::tasks::Task>> {
        self.next_claimable_under(project_id, harness, None)
    }

    /// Like [`Self::next_claimable_task`], limited to the direct pieces of `parent` when given.
    pub fn next_claimable_under(
        &self,
        project_id: &str,
        harness: &str,
        parent: Option<&crate::tasks::Task>,
    ) -> Result<Option<crate::tasks::Task>> {
        for t in self.ready_tasks(project_id)? {
            if parent.is_some_and(|p| t.parent_id.as_deref() != Some(p.id.as_str())) {
                continue;
            }
            if crate::comparison::is_candidate(&t) || t.needs_person().is_some() {
                continue;
            }
            if self.check_claim(project_id, &t.id, harness, None).is_ok() {
                return Ok(Some(t));
            }
        }
        Ok(None)
    }

    /// The running session named by `$EVYRO_SESSION_ID` (set by `evyro run`
    /// for the agent it starts), if it belongs to this project.
    pub fn running_session_from_env(&self, project_id: &str) -> Option<Session> {
        let id = std::env::var("EVYRO_SESSION_ID")
            .ok()
            .filter(|v| !v.is_empty())?;
        self.get_session(&id)
            .ok()
            .filter(|s| s.project_id == project_id && !s.status.is_ended())
    }

    /// Give a task to a session that is already running, for an agent that
    /// starts a task from inside `evyro run`. The task is freed when that
    /// session ends, so it is never left held by a session nobody watches.
    pub fn claim_task(&self, project_id: &str, session_id: &str, task_id: &str) -> Result<Session> {
        let ses = self.get_session(session_id)?;
        if ses.project_id != project_id || ses.status.is_ended() {
            return Err(Error::Invalid(format!(
                "session {session_id} is not running in this project"
            )));
        }
        let tx = self.conn.unchecked_transaction()?;
        self.check_claim(project_id, task_id, &ses.harness, Some(&ses.id))?;
        let now = util::now();
        self.conn.execute(
            "INSERT OR REPLACE INTO leases(task_id, session_id, harness, acquired_at) VALUES (?1,?2,?3,?4)",
            rusqlite::params![task_id, ses.id, ses.harness, now],
        )?;
        self.conn.execute(
            "UPDATE tasks SET assigned_agent = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![ses.harness, now, task_id],
        )?;
        // The session works on this task now; checkpoints go to it.
        self.conn.execute(
            "UPDATE sessions SET task_id = ?1 WHERE id = ?2",
            rusqlite::params![task_id, ses.id],
        )?;
        self.emit(
            Some(project_id),
            Some(task_id),
            Some(&ses.id),
            "session.task_claimed",
            serde_json::json!({"harness": ses.harness}),
        )?;
        tx.commit()?;
        self.get_session(&ses.id)
    }

    pub fn get_session(&self, id: &str) -> Result<Session> {
        self.conn
            .query_row(
                &format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"),
                [id],
                row_to_session,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("session {id}")),
                e => e.into(),
            })
    }

    pub fn list_sessions(&self, project_id: &str, active_only: bool) -> Result<Vec<Session>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {SESSION_COLS} FROM sessions WHERE project_id = ?1
             AND (?2 = 0 OR status NOT IN ('completed','failed','cancelled')) ORDER BY started_at DESC"
        ))?;
        let rows = st.query_map(
            rusqlite::params![project_id, active_only as i64],
            row_to_session,
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The live session that owns the task, if any.
    pub fn active_session_for_task(&self, task_id: &str) -> Result<Option<Session>> {
        if let Some(l) = self.lease(task_id)? {
            let s = self.get_session(&l.session_id)?;
            if !s.status.is_ended() {
                return Ok(Some(s));
            }
        }
        Ok(None)
    }

    pub fn set_session_status(
        &self,
        id: &str,
        to: SessionStatus,
        reason: Option<&str>,
    ) -> Result<Session> {
        let s = self.get_session(id)?;
        if !s.status.can_transition_to(to) {
            return Err(Error::Transition(format!(
                "session {} cannot move from {} to {}",
                s.id, s.status, to
            )));
        }
        let now = util::now();
        let ended = if to.is_ended() {
            Some(now.clone())
        } else {
            None
        };
        self.conn.execute(
            "UPDATE sessions SET status = ?1, ended_at = COALESCE(?2, ended_at), exit_reason = COALESCE(?3, exit_reason) WHERE id = ?4",
            rusqlite::params![to.as_str(), ended, reason, s.id],
        )?;
        if to.is_ended() {
            self.conn
                .execute("DELETE FROM leases WHERE session_id = ?1", [&s.id])?;
        }
        self.emit(
            Some(&s.project_id),
            s.task_id.as_deref(),
            Some(&s.id),
            "session.status_changed",
            serde_json::json!({"from": s.status, "to": to, "reason": reason}),
        )?;
        if matches!(to, SessionStatus::Failed | SessionStatus::Cancelled) {
            // A receiver that failed must not strand the handoff it accepted.
            self.reopen_handoff_for_failed_session(&s.id)?;
        }
        self.get_session(&s.id)
    }

    pub fn set_session_pid(&self, id: &str, pid: Option<i64>) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET pid = ?1, pid_started = ?2 WHERE id = ?3",
            rusqlite::params![pid, pid.and_then(process_start_time), id],
        )?;
        Ok(())
    }

    pub fn lease(&self, task_id: &str) -> Result<Option<Lease>> {
        match self.conn.query_row(
            "SELECT task_id, session_id, harness, acquired_at FROM leases WHERE task_id = ?1",
            [task_id],
            |r| {
                Ok(Lease {
                    task_id: r.get(0)?,
                    session_id: r.get(1)?,
                    harness: r.get(2)?,
                    acquired_at: r.get(3)?,
                })
            },
        ) {
            Ok(l) => Ok(Some(l)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Sessions that claim to be live but whose process is gone (crash, quota
    /// kill, closed terminal). A session without a pid is reported once it has
    /// been silent past the heartbeat timeout, or always when `include_unknown` is set.
    /// Record that a session is still working, so one without a pid is not taken for dead.
    pub fn heartbeat(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET last_event_at = ?1 WHERE id = ?2",
            rusqlite::params![crate::util::now(), session_id],
        )?;
        Ok(())
    }

    pub fn stale_sessions(&self, project_id: &str, include_unknown: bool) -> Result<Vec<Session>> {
        Ok(self
            .list_sessions(project_id, true)?
            .into_iter()
            .filter(|s| match s.pid {
                Some(pid) => !session_process_alive(pid, s.pid_started),
                None => include_unknown || !s.is_live(),
            })
            .collect())
    }
}

#[cfg(test)]
mod liveness_tests {
    use super::*;
    #[test]
    fn etime_formats() {
        assert_eq!(parse_etime("00:07"), Some(7));
        assert_eq!(parse_etime("12:34"), Some(754));
        assert_eq!(parse_etime("01:00:00"), Some(3600));
        assert_eq!(
            parse_etime("2-03:04:05"),
            Some(2 * 86400 + 3 * 3600 + 4 * 60 + 5)
        );
        assert_eq!(parse_etime(""), None);
        assert_eq!(parse_etime("x:y"), None);
    }
    #[cfg(unix)]
    #[test]
    fn stopping_a_session_process_also_stops_what_it_started() {
        use std::io::{BufRead, BufReader};
        use std::process::{Command, Stdio};
        let mut parent = Command::new("sh")
            .args(["-c", "sleep 300 & echo $!; wait"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut line = String::new();
        BufReader::new(parent.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let child: i64 = line.trim().parse().unwrap();
        assert!(pid_alive(child));
        assert!(stop_session_process(parent.id() as i64, None));
        parent.wait().unwrap();
        for _ in 0..50 {
            if !pid_alive(child) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert!(!pid_alive(child), "the child outlived its stopped parent");
    }
    #[test]
    fn this_process_is_alive_and_a_reused_pid_is_not() {
        let me = std::process::id() as i64;
        let started = process_start_time(me).expect("start time of this process");
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        assert!(
            started <= now && now - started < 3600,
            "started {started}, now {now}"
        );
        assert!(session_process_alive(me, Some(started)));
        assert!(session_process_alive(me, None));
        // Same pid, but recorded for a process that started an hour earlier:
        // that is a different process that happened to get the same pid.
        assert!(!session_process_alive(me, Some(started - 3600)));
        assert!(!session_process_alive(-1, None));
    }
}

#[cfg(test)]
mod tests {
    use super::SessionStatus::*;
    #[test]
    fn session_transitions() {
        assert!(Running.can_transition_to(HandingOff));
        assert!(HandingOff.can_transition_to(Completed));
        assert!(!Completed.can_transition_to(Running));
        assert!(Paused.can_transition_to(Running));
        assert!(Running.can_transition_to(Recovering));
    }
}
