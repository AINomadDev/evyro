//! Acceptance criteria tracking. A task reaches `verified` only when every
//! criterion is met, as well as having a passing receipt for the current code.
//!
//! Who can mark a criterion met:
//! - a human, from the CLI or desktop app (`human_recorded`);
//! - an agent, only by citing a passing verification receipt for the same task
//!   (`evyro_verified`). An agent's bare claim is stored as `agent_reported`
//!   and does not satisfy the gate until a human confirms it.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::tasks::{Actor, Task};
use crate::util;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Criterion {
    /// 1-based position.
    pub index: usize,
    pub text: String,
    pub met: bool,
    /// Whether this criterion counts toward the `verified` gate.
    pub counts: bool,
    pub provenance: Option<String>,
    pub actor: Option<String>,
    pub note: String,
    pub receipt_id: Option<String>,
    pub checked_at: Option<String>,
}

impl Criterion {
    pub fn satisfied(&self) -> bool {
        self.met && self.counts
    }
}

impl Store {
    /// Acceptance criteria for a task with their current check state.
    pub fn criteria(&self, t: &Task) -> Result<Vec<Criterion>> {
        let mut st = self.conn.prepare(
            "SELECT idx, criterion, met, note, receipt_id, actor, provenance, checked_at
             FROM acceptance_checks WHERE task_id = ?1",
        )?;
        type Row = (
            i64,
            String,
            bool,
            String,
            Option<String>,
            String,
            String,
            String,
        );
        let rows: Vec<Row> = st
            .query_map([&t.id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get::<_, i64>(2)? != 0,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(t.acceptance
            .iter()
            .enumerate()
            .map(|(i, text)| {
                let idx = i + 1;
                // A check only applies to the wording that was checked.
                match rows.iter().find(|r| r.0 as usize == idx && &r.1 == text) {
                    Some((_, _, met, note, receipt, actor, prov, at)) => Criterion {
                        index: idx,
                        text: text.clone(),
                        met: *met,
                        counts: prov != "agent_reported",
                        provenance: Some(prov.clone()),
                        actor: Some(actor.clone()),
                        note: note.clone(),
                        receipt_id: receipt.clone(),
                        checked_at: Some(at.clone()),
                    },
                    None => Criterion {
                        index: idx,
                        text: text.clone(),
                        met: false,
                        counts: false,
                        provenance: None,
                        actor: None,
                        note: String::new(),
                        receipt_id: None,
                        checked_at: None,
                    },
                }
            })
            .collect())
    }

    /// Mark criterion `index` (1-based) of a task met or unmet.
    pub fn check_criterion(
        &self,
        task_id: &str,
        index: usize,
        met: bool,
        note: &str,
        receipt_id: Option<&str>,
        actor: &Actor,
    ) -> Result<Criterion> {
        let t = self.get_task(task_id)?;
        let text = t
            .acceptance
            .get(index.wrapping_sub(1))
            .cloned()
            .ok_or_else(|| {
                Error::Invalid(format!(
                    "{} has {} acceptance criteria; {} is out of range",
                    t.key,
                    t.acceptance.len(),
                    index
                ))
            })?;
        let provenance = match (actor, receipt_id) {
            (_, Some(rid)) => {
                let r = self.get_receipt(rid)?;
                if r.task_id.as_deref() != Some(t.id.as_str()) {
                    return Err(Error::Invalid(format!(
                        "receipt {rid} was not recorded for {}",
                        t.key
                    )));
                }
                if met && !r.passed {
                    return Err(Error::VerificationRequired(format!(
                        "receipt {rid} failed; it cannot show a criterion is met"
                    )));
                }
                "evyro_verified"
            }
            (Actor::Human { .. }, None) => "human_recorded",
            (Actor::Agent(_), None) => "agent_reported",
        };
        if matches!(actor, Actor::Agent(_)) && receipt_id.is_none() && note.trim().is_empty() {
            return Err(Error::Invalid(
                "agents must give a note or a passing receipt id as evidence".into(),
            ));
        }
        let note = crate::redact::redact(note).0;
        self.conn.execute(
            "INSERT INTO acceptance_checks(task_id, idx, criterion, met, note, receipt_id, actor, provenance, checked_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
             ON CONFLICT(task_id, idx) DO UPDATE SET criterion = excluded.criterion, met = excluded.met,
               note = excluded.note, receipt_id = excluded.receipt_id, actor = excluded.actor,
               provenance = excluded.provenance, checked_at = excluded.checked_at",
            rusqlite::params![
                t.id,
                index as i64,
                text,
                met as i64,
                note,
                receipt_id,
                actor.label(),
                provenance,
                util::now()
            ],
        )?;
        self.emit(
            Some(&t.project_id),
            Some(&t.id),
            None,
            "task.criterion_checked",
            serde_json::json!({"index": index, "met": met, "provenance": provenance, "actor": actor.label()}),
        )?;
        Ok(self
            .criteria(&t)?
            .into_iter()
            .nth(index - 1)
            .expect("index checked above"))
    }

    /// Criteria that still block `verified`.
    pub fn unmet_criteria(&self, t: &Task) -> Result<Vec<Criterion>> {
        Ok(self
            .criteria(t)?
            .into_iter()
            .filter(|c| !c.satisfied())
            .collect())
    }
}
