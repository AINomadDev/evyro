//! SQLite storage: WAL, foreign keys, explicit versioned migrations.

use crate::error::{Error, Result};
use crate::paths::GlobalPaths;
use rusqlite::Connection;
use std::path::Path;
use std::time::Duration;

/// Ordered migrations. Never edit a shipped migration; append a new one.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_initial",
        include_str!("../migrations/0001_initial.sql"),
    ),
    (
        "0002_acceptance_checks",
        include_str!("../migrations/0002_acceptance_checks.sql"),
    ),
    (
        "0003_review_comments",
        include_str!("../migrations/0003_review_comments.sql"),
    ),
    (
        "0004_skill_settings",
        include_str!("../migrations/0004_skill_settings.sql"),
    ),
    (
        "0005_remote_devices",
        include_str!("../migrations/0005_remote_devices.sql"),
    ),
    (
        "0006_session_pid_started",
        include_str!("../migrations/0006_session_pid_started.sql"),
    ),
    (
        "0007_task_plans",
        include_str!("../migrations/0007_task_plans.sql"),
    ),
    (
        "0008_remote_expiry_audit",
        include_str!("../migrations/0008_remote_expiry_audit.sql"),
    ),
];

/// How many pre-upgrade copies of the database to keep.
const KEEP_UPGRADE_BACKUPS: usize = 5;

pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

pub struct Store {
    pub conn: Connection,
    pub paths: GlobalPaths,
}

impl Store {
    /// Open the global store at the resolved data dir, migrating if needed.
    pub fn open_default() -> Result<Self> {
        let paths = GlobalPaths::resolve()?;
        paths.ensure()?;
        Self::open_at(&paths.db_path(), paths.clone())
    }

    pub fn open_at(db: &Path, paths: GlobalPaths) -> Result<Self> {
        let conn = Connection::open(db)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;",
        )?;
        let mut s = Self { conn, paths };
        s.migrate()?;
        Ok(s)
    }

    fn migrate(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            );",
        )?;
        let current: i64 = self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?;
        if current > SCHEMA_VERSION {
            return Err(Error::Conflict(format!(
                "database schema v{current} is newer than this build (v{SCHEMA_VERSION}); upgrade evyro"
            )));
        }
        if current > 0 && current < SCHEMA_VERSION {
            self.backup_before_upgrade(current)?;
        }
        for (i, (name, sql)) in MIGRATIONS.iter().enumerate() {
            let v = (i + 1) as i64;
            if v <= current {
                continue;
            }
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO schema_migrations(version, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![v, name, crate::util::now()],
            )?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Copy the database aside before upgrading its schema, so a failed or
    /// unwanted upgrade can be undone with `evyro restore`. Keeps the newest
    /// few copies in `<data>/backups/`.
    fn backup_before_upgrade(&self, from: i64) -> Result<()> {
        let dir = self.paths.data_dir.join("backups");
        std::fs::create_dir_all(&dir)?;
        let stamp = crate::util::now().replace([':', '-'], "").replace('.', "_");
        let path = dir.join(format!(
            "pre-upgrade-v{from}-to-v{SCHEMA_VERSION}-{stamp}.db"
        ));
        let mut dst = Connection::open(&path)?;
        rusqlite::backup::Backup::new(&self.conn, &mut dst)?.run_to_completion(
            256,
            Duration::from_millis(0),
            None,
        )?;
        drop(dst);
        let mut old: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("pre-upgrade-") && n.ends_with(".db"))
            })
            .collect();
        old.sort();
        while old.len() > KEEP_UPGRADE_BACKUPS {
            let _ = std::fs::remove_file(old.remove(0));
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )?)
    }

    /// A transaction that takes the write lock when it begins. A deferred transaction that reads
    /// and then writes fails at once with "database is locked" when another process (another AI
    /// running `evyro`) commits in between, ignoring the busy timeout; this one waits its turn.
    pub(crate) fn write_tx(&self) -> Result<rusqlite::Transaction<'_>> {
        Ok(rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?)
    }

    /// Run `f` so that all of its writes land or none do. Outside a transaction it opens one
    /// (`write_tx`); inside one it uses a savepoint, so a failure undoes only `f`'s writes and
    /// the caller decides what happens to the rest.
    pub(crate) fn atomic<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        if self.conn.is_autocommit() {
            let tx = self.write_tx()?;
            let v = f()?; // dropping `tx` rolls back
            tx.commit()?;
            return Ok(v);
        }
        self.conn.execute_batch("SAVEPOINT evyro_atomic")?;
        match f() {
            Ok(v) => {
                self.conn.execute_batch("RELEASE evyro_atomic")?;
                Ok(v)
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO evyro_atomic; RELEASE evyro_atomic");
                Err(e)
            }
        }
    }

    /// Rows that point at rows that are not there, or a lease held by a session that has ended:
    /// what an interrupted multi-row write leaves behind. Covers the links foreign keys do not.
    pub fn consistency_problems(&self) -> Result<Vec<String>> {
        const CHECKS: &[(&str, &str)] = &[
            (
                "lease(s) held by a missing or ended session",
                "SELECT COUNT(*) FROM leases l LEFT JOIN sessions s ON s.id = l.session_id
                 WHERE s.id IS NULL OR s.status IN ('completed','failed','cancelled')",
            ),
            (
                "lease(s) on a missing task",
                "SELECT COUNT(*) FROM leases WHERE task_id NOT IN (SELECT id FROM tasks)",
            ),
            (
                "event(s) for a missing task",
                "SELECT COUNT(*) FROM events WHERE task_id IS NOT NULL
                 AND task_id NOT IN (SELECT id FROM tasks)",
            ),
            (
                "event(s) for a missing session",
                "SELECT COUNT(*) FROM events WHERE session_id IS NOT NULL
                 AND session_id NOT IN (SELECT id FROM sessions)",
            ),
            (
                "checkpoint(s) for a missing task or session",
                "SELECT COUNT(*) FROM checkpoints
                 WHERE (task_id IS NOT NULL AND task_id NOT IN (SELECT id FROM tasks))
                    OR (session_id IS NOT NULL AND session_id NOT IN (SELECT id FROM sessions))",
            ),
            (
                "session(s) whose latest checkpoint is missing",
                "SELECT COUNT(*) FROM sessions WHERE latest_checkpoint_id IS NOT NULL
                 AND latest_checkpoint_id NOT IN (SELECT id FROM checkpoints)",
            ),
            (
                "handoff(s) whose checkpoint or accepting session is missing",
                "SELECT COUNT(*) FROM handoffs
                 WHERE (checkpoint_id IS NOT NULL AND checkpoint_id NOT IN (SELECT id FROM checkpoints))
                    OR (accepted_session_id IS NOT NULL
                        AND accepted_session_id NOT IN (SELECT id FROM sessions))",
            ),
        ];
        let mut out = Vec::new();
        for (what, sql) in CHECKS {
            let n: i64 = self.conn.query_row(sql, [], |r| r.get(0))?;
            if n > 0 {
                out.push(format!("{n} {what}"));
            }
        }
        Ok(out)
    }

    /// `PRAGMA integrity_check` + foreign key check. Returns problems found.
    pub fn integrity_check(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        let mut st = self.conn.prepare("PRAGMA integrity_check")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        for r in rows {
            let r = r?;
            if r != "ok" {
                out.push(r);
            }
        }
        let mut st = self.conn.prepare("PRAGMA foreign_key_check")?;
        let rows = st.query_map([], |r| {
            Ok(format!(
                "foreign key violation in {}",
                r.get::<_, String>(0)?
            ))
        })?;
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

/// Helpers for JSON columns.
pub fn to_json<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| "null".into())
}

pub fn from_json<T: serde::de::DeserializeOwned + Default>(s: Option<String>) -> T {
    s.and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    #[test]
    fn an_older_database_is_copied_aside_before_upgrading() {
        let d = tempfile::tempdir().unwrap();
        let paths = GlobalPaths {
            data_dir: d.path().join("data"),
            config_dir: d.path().join("config"),
        };
        paths.ensure().unwrap();
        let db = paths.db_path();
        let s = Store::open_at(&db, paths.clone()).unwrap();
        // A fresh database is not backed up: there is nothing to lose.
        assert!(!paths.data_dir.join("backups").exists());
        // Turn it back into the schema before the newest migration.
        s.conn
            .execute_batch(&format!(
                "DROP TABLE remote_audit;
                 ALTER TABLE remote_devices DROP COLUMN expires_at;
                 DELETE FROM schema_migrations WHERE version = {SCHEMA_VERSION};"
            ))
            .unwrap();
        drop(s);

        let s = Store::open_at(&db, paths.clone()).unwrap();
        assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
        let backups: Vec<_> = std::fs::read_dir(paths.data_dir.join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1, "{backups:?}");
        let name = backups[0]
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert!(
            name.starts_with(&format!(
                "pre-upgrade-v{}-to-v{SCHEMA_VERSION}-",
                SCHEMA_VERSION - 1
            )),
            "{name}"
        );
        // The copy is the old schema, intact.
        let old = Connection::open(&backups[0]).unwrap();
        let v: i64 = old
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION - 1);
        let ok: String = old
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ok, "ok");
    }
}

#[cfg(test)]
mod consistency_tests {
    use super::*;
    use crate::sessions::{NewSession, SessionStatus};
    use crate::tasks::{NewTask, Task};

    fn setup() -> (tempfile::TempDir, Store, Task) {
        let dir = tempfile::tempdir().unwrap();
        let paths = GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["init", "-q", "-b", "main"])
            .output()
            .unwrap();
        assert!(out.status.success());
        let project = store.register_project(&root, None).unwrap().project;
        let task = store
            .create_task(
                &project.id,
                NewTask {
                    title: "Work".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        (dir, store, task)
    }

    fn new_session(task: &Task) -> NewSession {
        NewSession {
            task_id: Some(task.id.clone()),
            harness: "codex".into(),
            ..Default::default()
        }
    }

    /// Make the next event insert fail: a write in the middle of each operation below.
    fn fail_events(store: &Store) {
        store
            .conn
            .execute_batch(
                "CREATE TEMP TRIGGER fail_events BEFORE INSERT ON events
                 BEGIN SELECT RAISE(ABORT, 'injected failure'); END;",
            )
            .unwrap();
    }

    fn count(store: &Store, sql: &str) -> i64 {
        store.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn a_failed_session_start_leaves_no_session_or_lease() {
        let (_d, store, task) = setup();
        fail_events(&store);
        let err = store
            .start_session(&task.project_id, new_session(&task))
            .unwrap_err();
        assert!(err.to_string().contains("injected failure"), "{err}");
        assert_eq!(count(&store, "SELECT COUNT(*) FROM sessions"), 0);
        assert_eq!(count(&store, "SELECT COUNT(*) FROM leases"), 0);
        assert_eq!(store.get_task(&task.id).unwrap().assigned_agent, None);
        assert!(store.conn.is_autocommit(), "no transaction left open");
        assert!(store.consistency_problems().unwrap().is_empty());
    }

    #[test]
    fn a_failed_session_end_keeps_the_session_and_its_lease() {
        let (_d, store, task) = setup();
        let s = store
            .start_session(&task.project_id, new_session(&task))
            .unwrap();
        fail_events(&store);
        assert!(store
            .set_session_status(&s.id, SessionStatus::Completed, None)
            .is_err());
        assert_eq!(
            store.get_session(&s.id).unwrap().status,
            SessionStatus::Running
        );
        assert_eq!(store.lease(&task.id).unwrap().unwrap().session_id, s.id);
        assert!(store.consistency_problems().unwrap().is_empty());
    }

    #[test]
    fn a_failure_inside_a_transaction_undoes_only_its_own_writes() {
        let (_d, store, task) = setup();
        let before = count(&store, "SELECT COUNT(*) FROM events");
        store
            .atomic(|| {
                store.emit(
                    Some(&task.project_id),
                    Some(&task.id),
                    None,
                    "kept",
                    serde_json::json!({}),
                )?;
                let inner: Result<()> = store.atomic(|| {
                    store.emit(None, None, None, "undone", serde_json::json!({}))?;
                    Err(Error::Invalid("inner".into()))
                });
                assert!(inner.is_err());
                Ok(())
            })
            .unwrap();
        assert_eq!(count(&store, "SELECT COUNT(*) FROM events"), before + 1);
        assert_eq!(
            count(&store, "SELECT COUNT(*) FROM events WHERE kind = 'undone'"),
            0
        );
    }

    #[test]
    fn doctor_reports_dangling_references() {
        let (d, store, task) = setup();
        let s = store
            .start_session(&task.project_id, new_session(&task))
            .unwrap();
        let ok = crate::doctor::run(&store, d.path());
        let check = ok.iter().find(|c| c.name == "store consistency").unwrap();
        assert_eq!(check.status, "ok", "{check:?}");

        // What a half-finished write leaves: a session ended without freeing its lease, and
        // events and a checkpoint pointer naming rows that were never written.
        store
            .conn
            .execute_batch(&format!(
                "UPDATE sessions SET status = 'failed', latest_checkpoint_id = 'chk_gone'
                   WHERE id = '{}';
                 INSERT INTO events(id, project_id, task_id, session_id, kind, created_at)
                   VALUES ('evt_x', '{}', 'tsk_gone', 'ses_gone', 'x', 'now');",
                s.id, task.project_id
            ))
            .unwrap();
        let problems = store.consistency_problems().unwrap();
        for want in [
            "1 lease(s) held by a missing or ended session",
            "1 event(s) for a missing task",
            "1 event(s) for a missing session",
            "1 session(s) whose latest checkpoint is missing",
        ] {
            assert!(problems.iter().any(|p| p == want), "{want}: {problems:?}");
        }
        let checks = crate::doctor::run(&store, d.path());
        let check = checks
            .iter()
            .find(|c| c.name == "store consistency")
            .unwrap();
        assert_eq!(check.status, "warn");
        assert!(check
            .detail
            .contains("lease(s) held by a missing or ended session"));
    }
}
