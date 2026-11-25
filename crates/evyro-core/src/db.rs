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
                "DROP TABLE task_plans;
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
