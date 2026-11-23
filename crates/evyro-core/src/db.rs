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
];

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

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
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
