//! Backup/restore of the global database and portable per-project exports.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::{brand, redact, util};
use rusqlite::types::{Value as SqlValue, ValueRef};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const EXPORT_FORMAT: &str = "evyro-project-export";
pub const EXPORT_VERSION: u32 = 1;

/// Consistent online snapshot of the whole database (SQLite backup API).
pub fn backup_db(store: &Store, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Err(Error::Conflict(format!(
            "{} already exists; choose a new file",
            dest.display()
        )));
    }
    if let Some(d) = dest.parent() {
        std::fs::create_dir_all(d)?;
    }
    let tmp = dest.with_extension("partial");
    {
        let mut dst = rusqlite::Connection::open(&tmp)?;
        let b = rusqlite::backup::Backup::new(&store.conn, &mut dst)?;
        b.run_to_completion(256, std::time::Duration::from_millis(5), None)?;
    }
    std::fs::rename(&tmp, dest)?;
    Ok(())
}

/// Validate a backup and atomically replace the live database with it.
/// The previous database is kept next to it as `evyro.db.before-restore-<ts>`.
pub fn restore_db(src: &Path, db_path: &Path) -> Result<String> {
    {
        let c =
            rusqlite::Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ok: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(Error::Invalid(format!(
                "backup failed integrity check: {ok}"
            )));
        }
        let v: i64 = c
            .query_row(
                "SELECT COALESCE(MAX(version),0) FROM schema_migrations",
                [],
                |r| r.get(0),
            )
            .map_err(|_| Error::Invalid("file is not an Evyro database".into()))?;
        if v > crate::db::SCHEMA_VERSION {
            return Err(Error::Conflict(format!(
                "backup schema v{v} is newer than this build"
            )));
        }
    }
    let stamp = util::now().replace([':', '.'], "-");
    let keep = db_path.with_file_name(format!("evyro.db.before-restore-{stamp}"));
    if db_path.exists() {
        // Checkpoint WAL so the kept copy is complete.
        if let Ok(c) = rusqlite::Connection::open(db_path) {
            let _ = c.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
        }
        std::fs::copy(db_path, &keep)?;
    }
    let tmp = db_path.with_extension("restoring");
    std::fs::copy(src, &tmp)?;
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db_path.with_extension(ext));
    }
    std::fs::rename(&tmp, db_path)?;
    Ok(keep.to_string_lossy().to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub format: String,
    pub version: u32,
    pub schema_version: i64,
    pub exported_at: String,
    pub producer: String,
    pub project_id: String,
    pub redaction_findings: usize,
    /// table -> rows (column -> value)
    pub tables: Vec<(String, Vec<serde_json::Map<String, serde_json::Value>>)>,
}

/// Export order also defines import order (parents before children).
const TABLES: &[(&str, &str)] = &[
    ("projects", "id = ?1"),
    ("tasks", "project_id = ?1 ORDER BY seq"),
    (
        "task_deps",
        "task_id IN (SELECT id FROM tasks WHERE project_id = ?1)",
    ),
    (
        "task_comments",
        "task_id IN (SELECT id FROM tasks WHERE project_id = ?1)",
    ),
    ("sessions", "project_id = ?1 ORDER BY started_at"),
    ("checkpoints", "project_id = ?1 ORDER BY created_at, rowid"),
    ("handoffs", "project_id = ?1 ORDER BY created_at"),
    ("verifications", "project_id = ?1 ORDER BY started_at"),
    (
        "acceptance_checks",
        "task_id IN (SELECT id FROM tasks WHERE project_id = ?1)",
    ),
    ("decisions", "project_id = ?1 ORDER BY created_at"),
    ("memories", "project_id = ?1 ORDER BY created_at"),
    ("events", "project_id = ?1 ORDER BY seq"),
];

fn to_json_value(v: ValueRef) -> serde_json::Value {
    match v {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Integer(i) => i.into(),
        ValueRef::Real(f) => serde_json::json!(f),
        ValueRef::Text(t) => serde_json::Value::String(String::from_utf8_lossy(t).to_string()),
        ValueRef::Blob(b) => serde_json::Value::String(util::hex(b)),
    }
}

fn to_sql(v: &serde_json::Value) -> SqlValue {
    match v {
        serde_json::Value::Null => SqlValue::Null,
        serde_json::Value::Bool(b) => SqlValue::Integer(*b as i64),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map(SqlValue::Integer)
            .unwrap_or_else(|| SqlValue::Real(n.as_f64().unwrap_or(0.0))),
        serde_json::Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

pub fn export_project(store: &Store, project_id: &str) -> Result<ExportBundle> {
    let p = store.get_project(project_id)?;
    let mut tables = Vec::new();
    let mut findings = Vec::new();
    for (t, filter) in TABLES {
        let mut st = store
            .conn
            .prepare(&format!("SELECT * FROM {t} WHERE {filter}"))?;
        let cols: Vec<String> = st.column_names().map(|s| s.to_string()).collect();
        let mut rows = st.query([&p.id])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            let mut m = serde_json::Map::new();
            for (i, c) in cols.iter().enumerate() {
                if *t == "events" && c == "seq" {
                    continue;
                }
                m.insert(c.clone(), to_json_value(r.get_ref(i)?));
            }
            let mut v = serde_json::Value::Object(m);
            redact::redact_json(&mut v, &mut findings);
            if let serde_json::Value::Object(m) = v {
                out.push(m);
            }
        }
        tables.push((t.to_string(), out));
    }
    Ok(ExportBundle {
        format: EXPORT_FORMAT.into(),
        version: EXPORT_VERSION,
        schema_version: store.schema_version()?,
        exported_at: util::now(),
        producer: format!("{} {}", brand::CLI_NAME, brand::VERSION),
        project_id: p.id,
        redaction_findings: findings.iter().map(|f| f.count).sum(),
        tables,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    pub project_id: String,
    pub rows: Vec<(String, usize)>,
    pub root_path: String,
}

/// Import a bundle into this store. Refuses if the project id already exists
/// or if the target root is registered to another project.
pub fn import_project(
    store: &Store,
    bundle: &ExportBundle,
    new_root: Option<&Path>,
) -> Result<ImportReport> {
    if bundle.format != EXPORT_FORMAT || bundle.version != EXPORT_VERSION {
        return Err(Error::Unsupported(format!(
            "unsupported export format {} v{}",
            bundle.format, bundle.version
        )));
    }
    if bundle.schema_version > store.schema_version()? {
        return Err(Error::Conflict(
            "export was made by a newer Evyro schema; upgrade first".into(),
        ));
    }
    if store.get_project(&bundle.project_id).is_ok() {
        return Err(Error::Conflict(format!(
            "project {} already exists in this database",
            bundle.project_id
        )));
    }
    let tx = store.conn.unchecked_transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON;")?;
    let mut counts = Vec::new();
    let mut root_path = String::new();
    for (table, rows) in &bundle.tables {
        if !TABLES.iter().any(|(t, _)| t == table) {
            return Err(Error::Invalid(format!(
                "unexpected table in export: {table}"
            )));
        }
        for row in rows {
            let mut row = row.clone();
            if table == "projects" {
                if let Some(r) = new_root {
                    row.insert(
                        "root_path".into(),
                        serde_json::Value::String(
                            util::canonical(r)?.to_string_lossy().to_string(),
                        ),
                    );
                }
                root_path = row
                    .get("root_path")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                if store.find_project_by_root(&root_path)?.is_some() {
                    return Err(Error::Conflict(format!(
                        "{root_path} is already registered to another project"
                    )));
                }
            }
            let cols: Vec<&String> = row.keys().collect();
            for c in &cols {
                if !c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') {
                    return Err(Error::Invalid(format!("bad column name {c}")));
                }
            }
            let sql = format!(
                "INSERT INTO {table} ({}) VALUES ({})",
                cols.iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                (1..=cols.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let vals: Vec<SqlValue> = cols.iter().map(|c| to_sql(&row[c.as_str()])).collect();
            tx.execute(&sql, rusqlite::params_from_iter(vals))?;
        }
        counts.push((table.clone(), rows.len()));
    }
    tx.commit()?;
    store.emit(
        Some(&bundle.project_id),
        None,
        None,
        "project.imported",
        serde_json::json!({"exported_at": bundle.exported_at}),
    )?;
    Ok(ImportReport {
        project_id: bundle.project_id.clone(),
        rows: counts,
        root_path,
    })
}
