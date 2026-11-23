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

    /// Events from every project with `seq` greater than `after`, oldest
    /// first. Watchers (the desktop app) keep the last `seq` they saw.
    pub fn events_after(&self, after: i64, limit: usize) -> Result<Vec<Event>> {
        let mut st = self.conn.prepare(
            "SELECT seq, id, project_id, task_id, session_id, kind, payload, created_at FROM events
             WHERE seq > ?1 ORDER BY seq ASC LIMIT ?2",
        )?;
        let rows = st.query_map(rusqlite::params![after, limit as i64], |r| {
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
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// The newest event `seq`, or 0 when there are none.
    pub fn last_event_seq(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(seq), 0) FROM events", [], |r| r.get(0))?)
    }
}

#[cfg(test)]
mod tests {
    use crate::paths::GlobalPaths;
    use crate::Store;

    #[test]
    fn events_after_returns_new_events_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let paths = GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let s = Store::open_at(&paths.db_path(), paths.clone()).unwrap();
        let start = s.last_event_seq().unwrap();
        s.emit(None, None, None, "a.one", serde_json::json!({}))
            .unwrap();
        s.emit(None, None, None, "a.two", serde_json::json!({}))
            .unwrap();
        let evs = s.events_after(start, 10).unwrap();
        assert_eq!(
            evs.iter().map(|e| e.kind.as_str()).collect::<Vec<_>>(),
            ["a.one", "a.two"]
        );
        assert!(s.events_after(evs[1].seq, 10).unwrap().is_empty());
        assert_eq!(s.last_event_seq().unwrap(), evs[1].seq);
    }
}
