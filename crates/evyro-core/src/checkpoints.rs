//! Checkpoints: durable snapshots of verified Git state plus agent-reported
//! progress. Git facts are captured by Evyro itself; summaries and step lists
//! are stored as agent-reported (unverified) provenance.

use crate::db::{from_json, to_json, Store};
use crate::error::{Error, Result};
use crate::git::{self, GitSnapshot};
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const TRIGGERS: &[&str] = &[
    "manual",
    "task_started",
    "plan_finalized",
    "files_changed",
    "tool_completed",
    "tests_completed",
    "status_changed",
    "commit",
    "compaction",
    "handoff",
    "process_exit",
    "recovery",
    "hook",
    "auto",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub id: String,
    pub project_id: String,
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub parent_id: Option<String>,
    pub trigger: String,
    pub summary: String,
    pub completed: Vec<String>,
    pub remaining: Vec<String>,
    pub blockers: Vec<String>,
    pub next_action: String,
    pub git: GitSnapshot,
    pub fingerprint: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewCheckpoint {
    pub task_id: Option<String>,
    pub session_id: Option<String>,
    pub trigger: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub completed: Vec<String>,
    /// None = inherit from the previous checkpoint (minus newly completed items).
    pub remaining: Option<Vec<String>>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default)]
    pub next_action: String,
    /// Skip when nothing changed since the previous checkpoint (adaptive checkpointing).
    #[serde(default)]
    pub dedupe: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckpointOutcome {
    pub checkpoint: Checkpoint,
    /// False when deduplicated against the previous checkpoint.
    pub created: bool,
}

const CP_COLS: &str = "id, project_id, task_id, session_id, parent_id, trigger, summary, completed, remaining, blockers, next_action, git, fingerprint, created_at";

fn row_to_cp(r: &rusqlite::Row) -> rusqlite::Result<Checkpoint> {
    Ok(Checkpoint {
        id: r.get(0)?,
        project_id: r.get(1)?,
        task_id: r.get(2)?,
        session_id: r.get(3)?,
        parent_id: r.get(4)?,
        trigger: r.get(5)?,
        summary: r.get(6)?,
        completed: from_json(r.get(7)?),
        remaining: from_json(r.get(8)?),
        blockers: from_json(r.get(9)?),
        next_action: r.get(10)?,
        git: from_json(r.get(11)?),
        fingerprint: r.get(12)?,
        created_at: r.get(13)?,
    })
}

impl Store {
    pub fn create_checkpoint(
        &self,
        project_id: &str,
        c: NewCheckpoint,
    ) -> Result<CheckpointOutcome> {
        let trigger = c.trigger.clone().unwrap_or_else(|| "manual".into());
        if !TRIGGERS.contains(&trigger.as_str()) {
            return Err(Error::Invalid(format!(
                "unknown checkpoint trigger '{trigger}'"
            )));
        }
        let project = self.get_project(project_id)?;
        let task = match &c.task_id {
            Some(t) => Some(self.get_task(t)?),
            None => None,
        };
        if let Some(t) = &task {
            if t.project_id != project.id {
                return Err(Error::Invalid("task belongs to a different project".into()));
            }
        }
        let root = match &task {
            Some(t) => self.task_root(t)?,
            None => project.root(),
        };
        let snap = git::snapshot(&root)?;
        let fp = snap.fingerprint();
        let prev = match &c.task_id {
            Some(t) => self.latest_checkpoint(project_id, Some(t))?,
            None => None,
        };

        let has_new_info = !c.summary.is_empty()
            || !c.completed.is_empty()
            || c.remaining.is_some()
            || !c.blockers.is_empty()
            || !c.next_action.is_empty();
        if c.dedupe && !has_new_info {
            if let Some(p) = &prev {
                if p.fingerprint == fp {
                    return Ok(CheckpointOutcome {
                        checkpoint: p.clone(),
                        created: false,
                    });
                }
            }
        }

        let mut completed = prev
            .as_ref()
            .map(|p| p.completed.clone())
            .unwrap_or_default();
        for item in &c.completed {
            if !completed.contains(item) {
                completed.push(item.clone());
            }
        }
        let remaining: Vec<String> = match &c.remaining {
            Some(r) => r.clone(),
            None => prev
                .as_ref()
                .map(|p| p.remaining.clone())
                .unwrap_or_default(),
        }
        .into_iter()
        .filter(|r| !completed.contains(r))
        .collect();
        let summary = if c.summary.is_empty() {
            prev.as_ref().map(|p| p.summary.clone()).unwrap_or_default()
        } else {
            c.summary.clone()
        };
        let next_action = if c.next_action.is_empty() {
            prev.as_ref()
                .map(|p| p.next_action.clone())
                .unwrap_or_default()
        } else {
            c.next_action.clone()
        };
        let blockers = if c.blockers.is_empty() {
            prev.as_ref()
                .map(|p| p.blockers.clone())
                .unwrap_or_default()
        } else {
            c.blockers.clone()
        };

        let id = util::new_id("chk");
        let now = util::now();
        self.conn.execute(
            &format!("INSERT INTO checkpoints({CP_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)"),
            rusqlite::params![
                id, project_id, c.task_id, c.session_id, prev.as_ref().map(|p| p.id.clone()), trigger, summary,
                to_json(&completed), to_json(&remaining), to_json(&blockers), next_action, to_json(&snap), fp, now
            ],
        )?;
        if let Some(sid) = &c.session_id {
            self.conn.execute(
                "UPDATE sessions SET latest_checkpoint_id = ?1 WHERE id = ?2",
                rusqlite::params![id, sid],
            )?;
        }
        self.emit(Some(project_id), c.task_id.as_deref(), c.session_id.as_deref(), "checkpoint.created",
            serde_json::json!({"checkpoint_id": id, "trigger": trigger, "dirty_files": snap.files.len(), "head": snap.head}))?;
        let cp = self.get_checkpoint(&id)?;
        if let Some(t) = &task {
            let _ = crate::init::write_current_task(self, &project, t, Some(&cp));
        }
        Ok(CheckpointOutcome {
            checkpoint: cp,
            created: true,
        })
    }

    pub fn get_checkpoint(&self, id: &str) -> Result<Checkpoint> {
        self.conn
            .query_row(
                &format!("SELECT {CP_COLS} FROM checkpoints WHERE id = ?1"),
                [id],
                row_to_cp,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("checkpoint {id}")),
                e => e.into(),
            })
    }

    pub fn latest_checkpoint(
        &self,
        project_id: &str,
        task_id: Option<&str>,
    ) -> Result<Option<Checkpoint>> {
        match self.conn.query_row(
            &format!("SELECT {CP_COLS} FROM checkpoints WHERE project_id = ?1 AND (?2 IS NULL OR task_id = ?2) ORDER BY created_at DESC, rowid DESC LIMIT 1"),
            rusqlite::params![project_id, task_id],
            row_to_cp,
        ) {
            Ok(c) => Ok(Some(c)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn list_checkpoints(
        &self,
        project_id: &str,
        task_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<Checkpoint>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {CP_COLS} FROM checkpoints WHERE project_id = ?1 AND (?2 IS NULL OR task_id = ?2) ORDER BY created_at DESC, rowid DESC LIMIT ?3"
        ))?;
        let rows = st.query_map(
            rusqlite::params![project_id, task_id, limit as i64],
            row_to_cp,
        )?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }
}

/// Differences between a recorded snapshot and the live repository.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriftReport {
    pub drifted: bool,
    pub branch_changed: Option<(Option<String>, Option<String>)>,
    /// same | descendant (new commits on top) | diverged | missing
    pub head_relation: String,
    pub recorded_head: Option<String>,
    pub current_head: Option<String>,
    pub new_commits: Vec<String>,
    /// Dirty files whose content changed since the snapshot.
    pub modified_since: Vec<String>,
    /// Files dirty now that were clean at snapshot time.
    pub newly_dirty: Vec<String>,
    /// Files dirty at snapshot time that are now clean (committed or reverted).
    pub no_longer_dirty: Vec<String>,
}

impl DriftReport {
    pub fn summary(&self) -> String {
        if !self.drifted {
            return "repository matches the checkpoint".into();
        }
        let mut parts = Vec::new();
        if let Some((a, b)) = &self.branch_changed {
            parts.push(format!(
                "branch changed {} -> {}",
                a.as_deref().unwrap_or("(detached)"),
                b.as_deref().unwrap_or("(detached)")
            ));
        }
        if self.head_relation != "same" {
            parts.push(format!(
                "HEAD is {} ({} new commits)",
                self.head_relation,
                self.new_commits.len()
            ));
        }
        if !self.modified_since.is_empty() {
            parts.push(format!(
                "{} file(s) modified since checkpoint",
                self.modified_since.len()
            ));
        }
        if !self.newly_dirty.is_empty() {
            parts.push(format!("{} newly changed file(s)", self.newly_dirty.len()));
        }
        if !self.no_longer_dirty.is_empty() {
            parts.push(format!(
                "{} file(s) no longer dirty",
                self.no_longer_dirty.len()
            ));
        }
        parts.join("; ")
    }
}

pub fn detect_drift(recorded: &GitSnapshot, root: &std::path::Path) -> Result<DriftReport> {
    let cur = git::snapshot(root)?;
    let mut r = DriftReport {
        recorded_head: recorded.head.clone(),
        current_head: cur.head.clone(),
        ..Default::default()
    };
    if recorded.branch != cur.branch {
        r.branch_changed = Some((recorded.branch.clone(), cur.branch.clone()));
    }
    r.head_relation = match (&recorded.head, &cur.head) {
        (a, b) if a == b => "same".into(),
        (Some(a), Some(b)) => {
            if !git::commit_exists(root, a) {
                "missing".into()
            } else if git::is_ancestor(root, a, b) {
                r.new_commits = git::log_oneline(root, &format!("{a}..{b}"), 50)?;
                "descendant".into()
            } else {
                "diverged".into()
            }
        }
        _ => "diverged".into(),
    };
    let before: HashMap<&str, &git::FileState> = recorded
        .files
        .iter()
        .map(|f| (f.path.as_str(), f))
        .collect();
    let after: HashMap<&str, &git::FileState> =
        cur.files.iter().map(|f| (f.path.as_str(), f)).collect();
    for (p, f) in &after {
        match before.get(p) {
            Some(b) if b.sha256 != f.sha256 || b.status != f.status => {
                r.modified_since.push(p.to_string())
            }
            Some(_) => {}
            None => r.newly_dirty.push(p.to_string()),
        }
    }
    for p in before.keys() {
        if !after.contains_key(p) {
            r.no_longer_dirty.push(p.to_string());
        }
    }
    r.modified_since.sort();
    r.newly_dirty.sort();
    r.no_longer_dirty.sort();
    r.drifted = r.branch_changed.is_some()
        || r.head_relation != "same"
        || !r.modified_since.is_empty()
        || !r.newly_dirty.is_empty()
        || !r.no_longer_dirty.is_empty();
    Ok(r)
}
