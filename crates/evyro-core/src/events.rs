//! Append-only event log. The DB tables are the operational view; events are
//! the audit trail used by timelines and reconciliation.

use crate::db::Store;
use crate::error::Result;
use crate::util;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: i64,
    pub id: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub kind: String,
    pub payload: serde_json::Value,
    pub created_at: String,
}

impl Store {
    pub fn emit(
        &self,
        project_id: Option<&str>,
        task_id: Option<&str>,
        session_id: Option<&str>,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<String> {
        let id = util::new_id("evt");
        let now = util::now();
        self.conn.execute(
            "INSERT INTO events(id, project_id, task_id, session_id, kind, payload, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                project_id,
                task_id,
                session_id,
                kind,
                payload.to_string(),
                now
            ],
        )?;
        if let Some(pid) = project_id {
            self.conn.execute(
                "UPDATE projects SET last_activity_at = ?1 WHERE id = ?2",
                rusqlite::params![now, pid],
            )?;
        }
        if let Some(sid) = session_id {
            self.conn.execute(
                "UPDATE sessions SET last_event_at = ?1 WHERE id = ?2",
                rusqlite::params![now, sid],
            )?;
        }
        Ok(id)
    }

    pub fn list_events(
        &self,
        project_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Event>> {
        let mut st = self.conn.prepare(
            "SELECT seq, id, project_id, task_id, session_id, kind, payload, created_at FROM events
             WHERE project_id = ?1 AND (?2 IS NULL OR task_id = ?2)
             ORDER BY seq DESC LIMIT ?3",
        )?;
        let rows = st.query_map(rusqlite::params![project_id, task_id, limit as i64], |r| {
            Ok(Event {
                seq: r.get(0)?,
                id: r.get(1)?,
                project_id: r.get(2)?,
                task_id: r.get(3)?,
                session_id: r.get(4)?,
                kind: r.get(5)?,
                payload: serde_json::from_str(&r.get::<_, String>(6)?)
                    .unwrap_or(serde_json::Value::Null),
                created_at: r.get(7)?,
            })
        })?;
        let mut v: Vec<Event> = rows.collect::<std::result::Result<_, _>>()?;
        v.reverse();
        Ok(v)
    }
}
