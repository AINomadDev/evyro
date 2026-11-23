//! Development memory with provenance, FTS5 search, and staleness detection.

use crate::db::{from_json, to_json, Store};
use crate::error::{Error, Result};
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const CATEGORIES: &[&str] = &[
    "architecture",
    "structure",
    "convention",
    "discovery",
    "bug",
    "debugging",
    "solution",
    "constraint",
    "dependency",
    "outcome",
    "note",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub id: String,
    pub project_id: Option<String>,
    pub category: String,
    pub content: String,
    pub source: String,
    pub task_id: Option<String>,
    pub files: Vec<String>,
    pub file_hashes: BTreeMap<String, String>,
    pub confidence: String,
    pub verification: String,
    pub archived: bool,
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub validated_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewMemory {
    pub category: String,
    pub content: String,
    #[serde(default)]
    pub files: Vec<String>,
    pub task_id: Option<String>,
    pub confidence: Option<String>,
    /// Global (cross-project) memories are opt-in.
    #[serde(default)]
    pub global: bool,
}

const M_COLS: &str = "m.id, m.project_id, m.category, m.content, m.source, m.task_id, m.files, m.file_hashes, m.confidence, m.verification, m.archived, m.superseded_by, m.created_at, m.validated_at";

fn row_to_memory(r: &rusqlite::Row) -> rusqlite::Result<Memory> {
    Ok(Memory {
        id: r.get(0)?,
        project_id: r.get(1)?,
        category: r.get(2)?,
        content: r.get(3)?,
        source: r.get(4)?,
        task_id: r.get(5)?,
        files: from_json(r.get(6)?),
        file_hashes: from_json(r.get(7)?),
        confidence: r.get(8)?,
        verification: r.get(9)?,
        archived: r.get::<_, i64>(10)? != 0,
        superseded_by: r.get(11)?,
        created_at: r.get(12)?,
        validated_at: r.get(13)?,
    })
}

impl Store {
    pub fn add_memory(&self, project_id: &str, m: NewMemory, source: &str) -> Result<Memory> {
        if !CATEGORIES.contains(&m.category.as_str()) {
            return Err(Error::Invalid(format!(
                "unknown memory category '{}' (expected {})",
                m.category,
                CATEGORIES.join(", ")
            )));
        }
        let (content, findings) = crate::redact::redact(&m.content);
        if content.trim().is_empty() {
            return Err(Error::Invalid("memory content is empty".into()));
        }
        let project = self.get_project(project_id)?;
        let mut hashes = BTreeMap::new();
        for f in &m.files {
            let p = crate::context::safe_join(&project.root(), f)?;
            if let Ok(h) = util::sha256_file(&p) {
                hashes.insert(f.clone(), h);
            }
        }
        let id = util::new_id("mem");
        let pid = if m.global {
            None
        } else {
            Some(project_id.to_string())
        };
        self.conn.execute(
            "INSERT INTO memories(id, project_id, category, content, source, task_id, files, file_hashes, confidence, verification, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'unverified',?10)",
            rusqlite::params![id, pid, m.category, content, source, m.task_id, to_json(&m.files), to_json(&hashes),
                m.confidence.unwrap_or_else(|| "medium".into()), util::now()],
        )?;
        self.emit(
            Some(project_id),
            m.task_id.as_deref(),
            None,
            "memory.added",
            serde_json::json!({"memory_id": id, "redactions": findings.len()}),
        )?;
        self.get_memory(&id)
    }

    pub fn get_memory(&self, id: &str) -> Result<Memory> {
        self.conn
            .query_row(
                &format!("SELECT {M_COLS} FROM memories m WHERE m.id = ?1"),
                [id],
                row_to_memory,
            )
            .map_err(|_| Error::NotFound(format!("memory {id}")))
    }

    /// Full-text search scoped to one project plus opt-in global memories.
    pub fn search_memory(
        &self,
        project_id: &str,
        query: Option<&str>,
        category: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Memory>> {
        let rows: Vec<Memory> = match query.filter(|q| !q.trim().is_empty()) {
            Some(q) => {
                // Quote each term so user input cannot inject FTS syntax.
                let fts: String = q
                    .split_whitespace()
                    .map(|w| format!("\"{}\"", w.replace('"', "")))
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut st = self.conn.prepare(&format!(
                    "SELECT {M_COLS} FROM memories_fts f JOIN memories m ON m.rowid = f.rowid
                     WHERE memories_fts MATCH ?1 AND (m.project_id = ?2 OR m.project_id IS NULL)
                     AND m.archived = 0 AND (?3 IS NULL OR m.category = ?3) ORDER BY rank LIMIT ?4"
                ))?;
                let r = st.query_map(
                    rusqlite::params![fts, project_id, category, limit as i64],
                    row_to_memory,
                )?;
                r.collect::<std::result::Result<_, _>>()?
            }
            None => {
                let mut st = self.conn.prepare(&format!(
                    "SELECT {M_COLS} FROM memories m WHERE (m.project_id = ?1 OR m.project_id IS NULL) AND m.archived = 0
                     AND (?2 IS NULL OR m.category = ?2) ORDER BY m.created_at DESC LIMIT ?3"
                ))?;
                let r = st.query_map(
                    rusqlite::params![project_id, category, limit as i64],
                    row_to_memory,
                )?;
                r.collect::<std::result::Result<_, _>>()?
            }
        };
        Ok(rows)
    }

    /// Mark memories whose referenced files changed as stale (drift detection).
    /// Returns ids newly marked stale.
    pub fn refresh_memory_staleness(&self, project_id: &str) -> Result<Vec<String>> {
        let project = self.get_project(project_id)?;
        let mut stale = Vec::new();
        for m in self.search_memory(project_id, None, None, 10_000)? {
            if m.project_id.is_none() || m.verification == "stale" || m.file_hashes.is_empty() {
                continue;
            }
            let changed = m.file_hashes.iter().any(|(f, h)| {
                crate::context::safe_join(&project.root(), f)
                    .ok()
                    .and_then(|p| util::sha256_file(&p).ok())
                    .map(|cur| &cur != h)
                    .unwrap_or(true)
            });
            if changed {
                self.conn.execute(
                    "UPDATE memories SET verification = 'stale' WHERE id = ?1",
                    [&m.id],
                )?;
                stale.push(m.id);
            }
        }
        Ok(stale)
    }

    pub fn set_memory_state(
        &self,
        id: &str,
        archived: Option<bool>,
        verification: Option<&str>,
    ) -> Result<Memory> {
        if let Some(v) = verification {
            if !["unverified", "verified", "stale"].contains(&v) {
                return Err(Error::Invalid(
                    "verification must be unverified|verified|stale".into(),
                ));
            }
            self.conn.execute(
                "UPDATE memories SET verification = ?1, validated_at = ?2 WHERE id = ?3",
                rusqlite::params![v, util::now(), id],
            )?;
        }
        if let Some(a) = archived {
            self.conn.execute(
                "UPDATE memories SET archived = ?1 WHERE id = ?2",
                rusqlite::params![a, id],
            )?;
        }
        self.get_memory(id)
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        let n = self
            .conn
            .execute("DELETE FROM memories WHERE id = ?1", [id])?;
        if n == 0 {
            return Err(Error::NotFound(format!("memory {id}")));
        }
        Ok(())
    }
}
