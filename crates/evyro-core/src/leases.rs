//! Freeing task leases whose session ended or whose agent is gone, and undoing that.
//!
//! A lease is freed whenever its session ends (see `set_session_status`), and each freed
//! lease is logged as a [`RELEASED`] event. A session whose agent is gone (its process exited,
//! or with no process it stayed silent past the heartbeat timeout) is closed out the moment
//! someone else starts its task, so a dead AI never keeps a task. A live agent's lease is
//! never touched. The latest release of a task can be undone while nobody else has taken it.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::sessions::{NewSession, Session};
use serde::Serialize;

/// Event kind for a lease freed because its session ended.
pub const RELEASED: &str = "lease.auto_released";
/// Event kind for a released lease given back to the AI that had it.
pub const RESTORED: &str = "lease.restored";

/// A task given back to the AI that held it before its lease was freed.
#[derive(Debug, Clone, Serialize)]
pub struct RestoredLease {
    pub task: String,
    pub harness: String,
    /// The new session that now holds the task.
    pub session: Session,
    /// The session whose lease was freed.
    pub released_session_id: String,
    /// The release event this undoes.
    pub release_event_id: String,
}

impl Store {
    /// If the task is held by a session whose agent is gone, close that session out (recovery
    /// checkpoint, marked failed), which frees and logs the lease. A live holder is left alone.
    pub(crate) fn free_stale_lease(&self, project_id: &str, task_id: &str) -> Result<()> {
        let Some(l) = self.lease(task_id)? else {
            return Ok(());
        };
        let holder = match self.get_session(&l.session_id) {
            Ok(s) => s,
            Err(Error::NotFound(_)) => return Ok(()),
            Err(e) => return Err(e),
        };
        if holder.project_id != project_id || holder.status.is_ended() || holder.is_live() {
            return Ok(());
        }
        self.recover_one(project_id, holder)?;
        Ok(())
    }

    /// Undo the latest automatic release of a task's lease: a new session for the same AI takes
    /// the task again. Refused when the task is finished, when anyone has claimed it since, or
    /// when a handoff for it is waiting.
    pub fn undo_lease_release(&self, project_id: &str, task_ref: &str) -> Result<RestoredLease> {
        let task = self.resolve_task(project_id, task_ref)?;
        self.atomic(|| {
            let (seq, event_id, session_id, payload): (i64, String, String, String) = self
                .conn
                .query_row(
                    "SELECT seq, id, session_id, payload FROM events
                     WHERE task_id = ?1 AND kind = ?2 ORDER BY seq DESC LIMIT 1",
                    rusqlite::params![task.id, RELEASED],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
                )
                .map_err(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => {
                        Error::NotFound(format!("no released lease to undo for {}", task.key))
                    }
                    e => e.into(),
                })?;
            let undone: bool = self.conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM events WHERE task_id = ?1 AND kind = ?2
                 AND json_extract(payload, '$.release_event_id') = ?3)",
                rusqlite::params![task.id, RESTORED, event_id],
                |r| r.get(0),
            )?;
            if undone {
                return Err(Error::Invalid(format!(
                    "the last release of {} was already undone",
                    task.key
                )));
            }
            if task.status.is_terminal() {
                return Err(Error::Invalid(format!("{} is already finished", task.key)));
            }
            let claimed: Option<String> = self
                .conn
                .query_row(
                    "SELECT s.harness FROM events e JOIN sessions s ON s.id = e.session_id
                     WHERE e.task_id = ?1 AND e.seq > ?2
                     AND e.kind IN ('session.started', 'session.task_claimed') LIMIT 1",
                    rusqlite::params![task.id, seq],
                    |r| r.get(0),
                )
                .map(Some)
                .or_else(|e| match e {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    e => Err(e),
                })?;
            if let Some(by) = claimed.or(self.lease(&task.id)?.map(|l| l.harness)) {
                return Err(Error::Conflict(format!(
                    "{} was claimed by {by} after its lease was freed; nothing was restored",
                    task.key
                )));
            }
            if let Some(h) = self.pending_handoff_for_task(&task.id)? {
                return Err(Error::Conflict(format!(
                    "{} has a handoff {} waiting for {}; nothing was restored",
                    task.key, h.id, h.to_harness
                )));
            }
            let harness = serde_json::from_str::<serde_json::Value>(&payload)
                .ok()
                .and_then(|p| p["harness"].as_str().map(str::to_owned))
                .ok_or_else(|| Error::Invalid("the release event names no AI".into()))?;
            let session = self.start_session(
                project_id,
                NewSession {
                    task_id: Some(task.id.clone()),
                    harness: harness.clone(),
                    ..Default::default()
                },
            )?;
            self.emit(
                Some(project_id),
                Some(&task.id),
                Some(&session.id),
                RESTORED,
                serde_json::json!({"harness": harness, "release_event_id": event_id, "released_session_id": session_id}),
            )?;
            Ok(RestoredLease {
                task: task.key.clone(),
                harness,
                session,
                released_session_id: session_id,
                release_event_id: event_id,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::SessionStatus;
    use crate::tasks::NewTask;

    struct Fx {
        _dir: tempfile::TempDir,
        store: Store,
        project: String,
        task: String,
    }

    fn setup() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", "init"]);
        let project = store.register_project(&root, None).unwrap().project.id;
        let task = store
            .create_task(
                &project,
                NewTask {
                    title: "Work".into(),
                    acceptance: vec!["done".into()],
                    ..Default::default()
                },
            )
            .unwrap()
            .id;
        Fx {
            _dir: dir,
            store,
            project,
            task,
        }
    }

    impl Fx {
        fn start(&self, harness: &str) -> Result<Session> {
            self.store.start_session(
                &self.project,
                NewSession {
                    task_id: Some(self.task.clone()),
                    harness: harness.into(),
                    ..Default::default()
                },
            )
        }
        /// Make a session look like its agent went quiet an hour ago with no process to check.
        fn silence(&self, id: &str) {
            self.store
                .conn
                .execute(
                    "UPDATE sessions SET last_event_at = ?1, started_at = ?1 WHERE id = ?2",
                    rusqlite::params!["2000-01-01T00:00:00.000Z", id],
                )
                .unwrap();
        }
        fn releases(&self) -> Vec<crate::events::Event> {
            self.store
                .list_events(&self.project, Some(&self.task), 100)
                .unwrap()
                .into_iter()
                .filter(|e| e.kind == RELEASED)
                .collect()
        }
    }

    #[test]
    fn an_ended_session_frees_its_lease_and_logs_it() {
        let fx = setup();
        let s = fx.start("claude-code").unwrap();
        fx.store
            .set_session_status(&s.id, SessionStatus::Cancelled, Some("stopped"))
            .unwrap();
        assert!(fx.store.lease(&fx.task).unwrap().is_none());
        let r = fx.releases();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].session_id.as_deref(), Some(s.id.as_str()));
        assert_eq!(r[0].payload["harness"], "claude-code");
        assert_eq!(r[0].payload["session_status"], "cancelled");
        fx.start("codex").unwrap();
    }

    #[test]
    fn a_stale_session_is_closed_out_when_another_ai_starts_its_task() {
        let fx = setup();
        let s = fx.start("claude-code").unwrap();
        fx.silence(&s.id);
        let other = fx.start("codex").unwrap();
        assert_eq!(
            fx.store.lease(&fx.task).unwrap().unwrap().session_id,
            other.id
        );
        let old = fx.store.get_session(&s.id).unwrap();
        assert_eq!(old.status, SessionStatus::Failed);
        let r = fx.releases();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].payload["reason"], "process_exited_unexpectedly");
    }

    #[test]
    fn a_live_session_keeps_its_lease() {
        let fx = setup();
        let s = fx.start("claude-code").unwrap();
        // No process recorded but reported in just now.
        let err = fx.start("codex").unwrap_err();
        assert!(matches!(err, Error::Conflict(_)), "{err}");
        // A recorded process that is still running (this test) is alive however quiet.
        fx.silence(&s.id);
        fx.store
            .set_session_pid(&s.id, Some(std::process::id() as i64))
            .unwrap();
        let err = fx.start("codex").unwrap_err();
        assert!(matches!(err, Error::Conflict(_)), "{err}");
        assert_eq!(fx.store.lease(&fx.task).unwrap().unwrap().session_id, s.id);
        assert!(!fx.store.get_session(&s.id).unwrap().status.is_ended());
        assert!(fx.releases().is_empty());
    }

    #[test]
    fn undo_gives_the_task_back_to_the_same_ai_once() {
        let fx = setup();
        let s = fx.start("claude-code").unwrap();
        fx.silence(&s.id);
        fx.store.recover_stale_sessions(&fx.project, false).unwrap();
        assert!(fx.store.lease(&fx.task).unwrap().is_none());
        let r = fx.store.undo_lease_release(&fx.project, &fx.task).unwrap();
        assert_eq!(r.harness, "claude-code");
        assert_eq!(r.released_session_id, s.id);
        let l = fx.store.lease(&fx.task).unwrap().unwrap();
        assert_eq!(
            (l.session_id.as_str(), l.harness.as_str()),
            (r.session.id.as_str(), "claude-code")
        );
        let err = fx
            .store
            .undo_lease_release(&fx.project, &fx.task)
            .unwrap_err();
        assert!(matches!(err, Error::Invalid(_)), "{err}");
    }

    #[test]
    fn undo_is_refused_after_someone_else_claimed_the_task() {
        let fx = setup();
        let s = fx.start("claude-code").unwrap();
        fx.silence(&s.id);
        let other = fx.start("codex").unwrap();
        let err = fx
            .store
            .undo_lease_release(&fx.project, &fx.task)
            .unwrap_err();
        assert!(matches!(err, Error::Conflict(_)), "{err}");
        assert_eq!(
            fx.store.lease(&fx.task).unwrap().unwrap().session_id,
            other.id
        );
        // Still refused once that AI has let go of it again: its own release is the latest,
        // and undoing that gives the task back to codex, not to the first AI.
        fx.store
            .set_session_status(&other.id, SessionStatus::Cancelled, None)
            .unwrap();
        let r = fx.store.undo_lease_release(&fx.project, &fx.task).unwrap();
        assert_eq!(r.harness, "codex");
    }

    #[test]
    fn nothing_to_undo_without_a_release() {
        let fx = setup();
        let err = fx
            .store
            .undo_lease_release(&fx.project, &fx.task)
            .unwrap_err();
        assert!(matches!(err, Error::NotFound(_)), "{err}");
    }
}
