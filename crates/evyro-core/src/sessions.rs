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
    pub status: SessionStatus,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub exit_reason: Option<String>,
    pub last_event_at: Option<String>,
    pub latest_checkpoint_id: Option<String>,
    pub handoff_id: Option<String>,
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

const SESSION_COLS: &str = "id, project_id, task_id, harness, model, provider, worktree, pid, status, started_at, ended_at, exit_reason, last_event_at, latest_checkpoint_id, handoff_id";

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
    })
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
            let t = self.get_task(tid)?;
            if t.project_id != project_id {
                return Err(Error::Invalid("task belongs to a different project".into()));
            }
            let blockers = self.open_blockers(tid)?;
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
            if let Some(l) = self.lease(tid)? {
                let holder = self.get_session(&l.session_id)?;
                if !holder.status.is_ended() {
                    return Err(Error::Conflict(format!(
                        "{} is owned by session {} ({}); hand off with `evyro handoff --to {}` or end that session first",
                        t.key, holder.id, holder.harness, s.harness
                    )));
                }
            }
        }
        self.conn.execute(
            &format!("INSERT INTO sessions({SESSION_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'running',?9,NULL,NULL,?9,NULL,?10)"),
            rusqlite::params![id, project_id, s.task_id, s.harness, s.model, s.provider, s.worktree, s.pid, now, s.handoff_id],
        )?;
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
            "UPDATE sessions SET pid = ?1 WHERE id = ?2",
            rusqlite::params![pid, id],
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
    /// kill, closed terminal). Sessions without a pid are reported only when
    /// `include_unknown` is set, since we cannot prove they died.
    pub fn stale_sessions(&self, project_id: &str, include_unknown: bool) -> Result<Vec<Session>> {
        Ok(self
            .list_sessions(project_id, true)?
            .into_iter()
            .filter(|s| match s.pid {
                Some(pid) => !pid_alive(pid),
                None => include_unknown,
            })
            .collect())
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
