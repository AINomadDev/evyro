//! Project health findings, each linked to concrete evidence (ids).

use crate::db::Store;
use crate::error::Result;
use crate::tasks::TaskStatus;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// failing_verification | unverified_completion | missing_acceptance | blocked | stale_session | pending_handoff | stale_memory
    pub kind: String,
    pub detail: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Health {
    pub project_id: String,
    pub task_counts: BTreeMap<String, usize>,
    pub active_sessions: usize,
    pub findings: Vec<Finding>,
}

impl Store {
    pub fn project_health(&self, project_id: &str) -> Result<Health> {
        let tasks = self.list_tasks(project_id, None, true)?;
        let mut counts = BTreeMap::new();
        for s in TaskStatus::ALL {
            counts.insert(s.to_string(), 0usize);
        }
        let mut findings = Vec::new();
        for t in &tasks {
            *counts.entry(t.status.to_string()).or_default() += 1;
            if t.status.is_terminal() {
                continue;
            }
            let receipts = self.list_verifications(project_id, Some(&t.id))?;
            if let Some(last) = receipts.last() {
                if !last.passed {
                    findings.push(Finding {
                        kind: "failing_verification".into(),
                        detail: format!("{} latest `{}` failed", t.key, last.command.join(" ")),
                        evidence: vec![t.key.clone(), last.id.clone()],
                    });
                }
            }
            if t.status == TaskStatus::InReview && !receipts.iter().any(|r| r.passed) {
                findings.push(Finding {
                    kind: "unverified_completion".into(),
                    detail: format!("{} is in review with no passing verification", t.key),
                    evidence: vec![t.key.clone()],
                });
            }
            if matches!(t.status, TaskStatus::InProgress | TaskStatus::Ready)
                && t.acceptance.is_empty()
                && t.kind != "epic"
            {
                findings.push(Finding {
                    kind: "missing_acceptance".into(),
                    detail: format!("{} has no acceptance criteria", t.key),
                    evidence: vec![t.key.clone()],
                });
            }
            if t.status == TaskStatus::Blocked
                || !self.open_blockers(&t.id)?.is_empty() && t.status == TaskStatus::InProgress
            {
                findings.push(Finding {
                    kind: "blocked".into(),
                    detail: format!("{} is blocked", t.key),
                    evidence: vec![t.key.clone()],
                });
            }
        }
        for s in self.stale_sessions(project_id, false)? {
            findings.push(Finding {
                kind: "stale_session".into(),
                detail: format!(
                    "{} session's process is gone; run `evyro recover`",
                    s.harness
                ),
                evidence: vec![s.id],
            });
        }
        for h in self
            .list_handoffs(project_id, 20)?
            .into_iter()
            .filter(|h| h.status == "prepared")
        {
            findings.push(Finding {
                kind: "pending_handoff".into(),
                detail: format!("handoff to {} awaiting `evyro resume`", h.to_harness),
                evidence: vec![h.id],
            });
        }
        for m in self
            .search_memory(project_id, None, None, 1000)?
            .into_iter()
            .filter(|m| m.verification == "stale")
        {
            findings.push(Finding {
                kind: "stale_memory".into(),
                detail: format!(
                    "memory references files that changed: {}",
                    m.content.chars().take(60).collect::<String>()
                ),
                evidence: vec![m.id],
            });
        }
        Ok(Health {
            project_id: project_id.into(),
            task_counts: counts,
            active_sessions: self.list_sessions(project_id, true)?.len(),
            findings,
        })
    }
}
