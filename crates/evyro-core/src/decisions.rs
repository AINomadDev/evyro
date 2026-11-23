//! Architecture / implementation decisions (ADR-lite records).

use crate::db::Store;
use crate::error::{Error, Result};
use crate::util;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub title: String,
    pub decision: String,
    pub rationale: String,
    pub status: String,
    pub source: String,
    pub created_at: String,
}

impl Store {
    pub fn record_decision(
        &self,
        project_id: &str,
        task_id: Option<&str>,
        title: &str,
        decision: &str,
        rationale: &str,
        source: &str,
    ) -> Result<Decision> {
        if title.trim().is_empty() || decision.trim().is_empty() {
            return Err(Error::Invalid(
                "decision needs a title and the decision text".into(),
            ));
        }
        let id = util::new_id("dec");
        self.conn.execute(
            "INSERT INTO decisions(id, project_id, task_id, title, decision, rationale, status, source, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,'accepted',?7,?8)",
            rusqlite::params![id, project_id, task_id, title, decision, rationale, source, util::now()],
        )?;
        self.emit(
            Some(project_id),
            task_id,
            None,
            "decision.recorded",
            serde_json::json!({"decision_id": id, "title": title}),
        )?;
        self.list_decisions(project_id, None)
            .map(|v| v.into_iter().find(|d| d.id == id).unwrap())
    }

    pub fn list_decisions(&self, project_id: &str, task_id: Option<&str>) -> Result<Vec<Decision>> {
        let mut st = self.conn.prepare(
            "SELECT id, project_id, task_id, title, decision, rationale, status, source, created_at FROM decisions
             WHERE project_id = ?1 AND (?2 IS NULL OR task_id = ?2 OR task_id IS NULL) AND status != 'superseded'
             ORDER BY created_at",
        )?;
        let rows = st.query_map(rusqlite::params![project_id, task_id], |r| {
            Ok(Decision {
                id: r.get(0)?,
                project_id: r.get(1)?,
                task_id: r.get(2)?,
                title: r.get(3)?,
                decision: r.get(4)?,
                rationale: r.get(5)?,
                status: r.get(6)?,
                source: r.get(7)?,
                created_at: r.get(8)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}
