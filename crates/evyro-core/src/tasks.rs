//! Task graph: validated state machine, dependencies, ready-work discovery.

use crate::db::{from_json, to_json, Store};
use crate::error::{Error, Result};
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Backlog,
    Ready,
    InProgress,
    Blocked,
    InReview,
    Verified,
    Done,
    Cancelled,
}

impl TaskStatus {
    pub const ALL: [TaskStatus; 8] = [
        Self::Backlog,
        Self::Ready,
        Self::InProgress,
        Self::Blocked,
        Self::InReview,
        Self::Verified,
        Self::Done,
        Self::Cancelled,
    ];
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Ready => "ready",
            Self::InProgress => "in_progress",
            Self::Blocked => "blocked",
            Self::InReview => "in_review",
            Self::Verified => "verified",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
    pub fn is_closed(&self) -> bool {
        matches!(self, Self::Done | Self::Cancelled | Self::Verified)
    }
    /// Allowed transitions. `Verified` additionally requires a passing receipt.
    pub fn can_transition_to(&self, to: TaskStatus) -> bool {
        use TaskStatus::*;
        if *self == to {
            return true;
        }
        if to == Cancelled {
            return !self.is_terminal();
        }
        matches!(
            (self, to),
            (Backlog, Ready)
                | (Backlog, InProgress)
                | (Ready, Backlog)
                | (Ready, InProgress)
                | (InProgress, Blocked)
                | (InProgress, InReview)
                | (InProgress, Ready)
                | (Blocked, InProgress)
                | (Blocked, Ready)
                | (InReview, InProgress)
                | (InReview, Verified)
                | (InProgress, Verified)
                | (Verified, Done)
                | (Verified, InProgress)
                | (Cancelled, Backlog)
        )
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let n = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.as_str() == n)
            .ok_or_else(|| Error::Invalid(format!("unknown task status '{s}'")))
    }
}

pub const TASK_KINDS: &[&str] = &[
    "epic",
    "feature",
    "task",
    "subtask",
    "bug",
    "tech_debt",
    "spike",
    "docs",
    "maintenance",
    "integration",
    "security",
    "release",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub project_id: String,
    /// Human-friendly per-project reference, e.g. `T-12`.
    pub key: String,
    pub seq: i64,
    pub parent_id: Option<String>,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub priority: i64,
    pub status: TaskStatus,
    pub acceptance: Vec<String>,
    pub constraints: Vec<String>,
    pub files: Vec<String>,
    pub assigned_agent: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct NewTask {
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub acceptance: Vec<String>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub status: Option<TaskStatus>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TaskUpdate {
    pub status: Option<TaskStatus>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<i64>,
    #[serde(default)]
    pub add_acceptance: Vec<String>,
    #[serde(default)]
    pub add_constraints: Vec<String>,
    #[serde(default)]
    pub add_files: Vec<String>,
    pub assigned_agent: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
}

/// Who is asking for a change. Agents can never bypass verification.
#[derive(Debug, Clone, PartialEq)]
pub enum Actor {
    Agent(String),
    Human { override_reason: Option<String> },
}

impl Actor {
    pub fn label(&self) -> String {
        match self {
            Actor::Agent(a) => format!("agent:{a}"),
            Actor::Human { .. } => "human".into(),
        }
    }
}

const TASK_COLS: &str = "id, project_id, seq, parent_id, kind, title, description, priority, status, acceptance, constraints, files, assigned_agent, branch, worktree, created_at, updated_at";

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let seq: i64 = r.get(2)?;
    let status: String = r.get(8)?;
    Ok(Task {
        id: r.get(0)?,
        project_id: r.get(1)?,
        key: format!("T-{seq}"),
        seq,
        parent_id: r.get(3)?,
        kind: r.get(4)?,
        title: r.get(5)?,
        description: r.get(6)?,
        priority: r.get(7)?,
        status: status.parse().unwrap_or(TaskStatus::Backlog),
        acceptance: from_json(r.get(9)?),
        constraints: from_json(r.get(10)?),
        files: from_json(r.get(11)?),
        assigned_agent: r.get(12)?,
        branch: r.get(13)?,
        worktree: r.get(14)?,
        created_at: r.get(15)?,
        updated_at: r.get(16)?,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Dependency {
    pub task: String,
    pub depends_on: String,
    pub kind: String,
}

/// A task constraint that keeps `evyro work` and the desktop from handing the task to an AI.
pub const NEEDS_PERSON: &str = "needs a person:";

/// A task constraint on pieces an AI wrote: `evyro work` skips them until a person approves.
pub const AWAITING_REVIEW: &str = "awaiting review:";

impl Task {
    /// Whether an AI wrote this task and no person has approved it for work yet.
    pub fn awaiting_review(&self) -> bool {
        self.constraints
            .iter()
            .any(|c| c.starts_with(AWAITING_REVIEW))
    }

    /// Why this task needs a person (a phone, a certificate, an account sign-in), if it does.
    /// Such a task is never picked automatically; naming it explicitly still works.
    pub fn needs_person(&self) -> Option<&str> {
        self.constraints
            .iter()
            .find_map(|c| c.strip_prefix(NEEDS_PERSON))
            .map(str::trim)
    }
}

impl Store {
    pub fn create_task(&self, project_id: &str, t: NewTask) -> Result<Task> {
        let tx = self.conn.unchecked_transaction()?;
        let task = self.insert_task(project_id, t, None)?;
        tx.commit()?;
        Ok(task)
    }

    /// Insert a task and its creation event in the caller's transaction.
    pub(crate) fn insert_task(
        &self,
        project_id: &str,
        t: NewTask,
        worktree: Option<(&str, &str)>,
    ) -> Result<Task> {
        if t.title.trim().is_empty() {
            return Err(Error::Invalid("task title must not be empty".into()));
        }
        let kind = t.kind.unwrap_or_else(|| "task".into());
        if !TASK_KINDS.contains(&kind.as_str()) {
            return Err(Error::Invalid(format!(
                "unknown task kind '{kind}' (expected one of {})",
                TASK_KINDS.join(", ")
            )));
        }
        let priority = t.priority.unwrap_or(2);
        if !(0..=4).contains(&priority) {
            return Err(Error::Invalid(
                "priority must be 0 (critical) .. 4 (trivial)".into(),
            ));
        }
        let parent = match t.parent {
            Some(p) => Some(self.resolve_task(project_id, &p)?.id),
            None => None,
        };
        let seq: i64 = self
            .conn
            .query_row(
                "SELECT next_task_seq FROM projects WHERE id = ?1",
                [project_id],
                |r| r.get(0),
            )
            .map_err(|_| Error::NotFound(format!("project {project_id}")))?;
        self.conn.execute(
            "UPDATE projects SET next_task_seq = next_task_seq + 1 WHERE id = ?1",
            [project_id],
        )?;
        let id = util::new_id("tsk");
        let now = util::now();
        let status = t.status.unwrap_or(TaskStatus::Backlog);
        let (branch, path) = worktree.unzip();
        self.conn.execute(
            &format!("INSERT INTO tasks({TASK_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,NULL,?13,?14,?15,?15)"),
            rusqlite::params![
                id, project_id, seq, parent, kind, t.title.trim(), t.description, priority, status.as_str(),
                to_json(&t.acceptance), to_json(&t.constraints), to_json(&t.files), branch, path, now
            ],
        )?;
        self.emit(
            Some(project_id),
            Some(&id),
            None,
            "task.created",
            serde_json::json!({"key": format!("T-{seq}"), "title": t.title}),
        )?;
        self.get_task(&id)
    }

    pub fn get_task(&self, id: &str) -> Result<Task> {
        self.conn
            .query_row(
                &format!("SELECT {TASK_COLS} FROM tasks WHERE id = ?1"),
                [id],
                row_to_task,
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Error::NotFound(format!("task {id}")),
                e => e.into(),
            })
    }

    /// Resolve `T-12`, `12`, or a full id, scoped to one project (isolation).
    pub fn resolve_task(&self, project_id: &str, r: &str) -> Result<Task> {
        let r = r.trim();
        let seq = r
            .strip_prefix("T-")
            .or_else(|| r.strip_prefix("t-"))
            .unwrap_or(r);
        if let Ok(n) = seq.parse::<i64>() {
            return self
                .conn
                .query_row(
                    &format!("SELECT {TASK_COLS} FROM tasks WHERE project_id = ?1 AND seq = ?2"),
                    rusqlite::params![project_id, n],
                    row_to_task,
                )
                .map_err(|_| Error::NotFound(format!("task {r} in this project")));
        }
        let t = self.get_task(r)?;
        if t.project_id != project_id {
            return Err(Error::NotFound(format!("task {r} in this project")));
        }
        Ok(t)
    }

    pub fn list_tasks(
        &self,
        project_id: &str,
        status: Option<TaskStatus>,
        include_closed: bool,
    ) -> Result<Vec<Task>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {TASK_COLS} FROM tasks WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY CASE status WHEN 'in_progress' THEN 0 WHEN 'blocked' THEN 1 WHEN 'in_review' THEN 2
               WHEN 'ready' THEN 3 WHEN 'backlog' THEN 4 WHEN 'verified' THEN 5 ELSE 6 END, priority, seq"
        ))?;
        let rows = st.query_map(
            rusqlite::params![project_id, status.map(|s| s.as_str())],
            row_to_task,
        )?;
        let v: Vec<Task> = rows.collect::<std::result::Result<_, _>>()?;
        Ok(v.into_iter()
            .filter(|t| include_closed || status.is_some() || !t.status.is_terminal())
            .collect())
    }

    pub fn update_task(&self, task_id: &str, u: TaskUpdate, actor: &Actor) -> Result<Task> {
        let t = self.get_task(task_id)?;
        let now = util::now();
        if let Some(to) = u.status {
            if !t.status.can_transition_to(to) {
                return Err(Error::Transition(format!(
                    "{} cannot move from {} to {}",
                    t.key, t.status, to
                )));
            }
            if to == TaskStatus::Verified && t.status != TaskStatus::Verified {
                self.require_verification(&t, actor)?;
            }
            if to == TaskStatus::Done
                && t.status != TaskStatus::Verified
                && t.status != TaskStatus::Done
            {
                return Err(Error::Transition(format!(
                    "{} must be verified before done",
                    t.key
                )));
            }
            if to == TaskStatus::InProgress {
                let blockers = self.open_blockers(&t.id)?;
                if !blockers.is_empty() {
                    return Err(Error::Conflict(format!(
                        "{} is blocked by unfinished dependencies: {}",
                        t.key,
                        blockers
                            .iter()
                            .map(|b| b.key.clone())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
        let mut acceptance = t.acceptance.clone();
        acceptance.extend(u.add_acceptance.iter().cloned());
        let mut constraints = t.constraints.clone();
        constraints.extend(u.add_constraints.iter().cloned());
        let mut files = t.files.clone();
        for f in &u.add_files {
            if !files.contains(f) {
                files.push(f.clone());
            }
        }
        if let Some(p) = u.priority {
            if !(0..=4).contains(&p) {
                return Err(Error::Invalid("priority must be 0..4".into()));
            }
        }
        self.conn.execute(
            "UPDATE tasks SET status = ?1, title = ?2, description = ?3, priority = ?4, acceptance = ?5,
               constraints = ?6, files = ?7, assigned_agent = ?8, branch = ?9, worktree = ?10, updated_at = ?11
             WHERE id = ?12",
            rusqlite::params![
                u.status.unwrap_or(t.status).as_str(),
                u.title.clone().unwrap_or(t.title.clone()),
                u.description.clone().unwrap_or(t.description.clone()),
                u.priority.unwrap_or(t.priority),
                to_json(&acceptance),
                to_json(&constraints),
                to_json(&files),
                u.assigned_agent.clone().or(t.assigned_agent.clone()),
                u.branch.clone().or(t.branch.clone()),
                u.worktree.clone().or(t.worktree.clone()),
                now,
                t.id
            ],
        )?;
        if let Some(to) = u.status {
            if to != t.status {
                let mut payload =
                    serde_json::json!({"from": t.status, "to": to, "actor": actor.label()});
                if let Actor::Human {
                    override_reason: Some(r),
                } = actor
                {
                    payload["override_reason"] = serde_json::Value::String(r.clone());
                }
                self.emit(
                    Some(&t.project_id),
                    Some(&t.id),
                    None,
                    "task.status_changed",
                    payload,
                )?;
                // Work handed to review, or finished, is no longer being worked on: end the
                // session that held it so its lease does not outlive the work.
                if matches!(
                    to,
                    TaskStatus::InReview
                        | TaskStatus::Verified
                        | TaskStatus::Done
                        | TaskStatus::Cancelled
                ) {
                    if let Some(s) = self.active_session_for_task(&t.id)? {
                        self.set_session_status(
                            &s.id,
                            crate::sessions::SessionStatus::Completed,
                            Some(&format!("task_{}", to.as_str())),
                        )?;
                    }
                }
            }
        } else {
            self.emit(
                Some(&t.project_id),
                Some(&t.id),
                None,
                "task.updated",
                serde_json::json!({"actor": actor.label()}),
            )?;
        }
        self.get_task(&t.id)
    }

    /// Verified requires a passing receipt that matches the *current* repo state.
    fn require_verification(&self, t: &Task, actor: &Actor) -> Result<()> {
        if let Actor::Human {
            override_reason: Some(reason),
        } = actor
        {
            if !reason.trim().is_empty() {
                return Ok(());
            }
        }
        let unmet = self.unmet_criteria(t)?;
        if !unmet.is_empty() {
            return Err(Error::VerificationRequired(format!(
                "{}: {} of {} acceptance criteria not met: {}. Check them off with `evyro task check {} <n>`",
                t.key,
                unmet.len(),
                t.acceptance.len(),
                unmet
                    .iter()
                    .map(|c| {
                        if c.met {
                            format!("#{} (agent-reported, needs confirmation)", c.index)
                        } else {
                            format!("#{} {}", c.index, c.text)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("; "),
                t.key
            )));
        }
        let root = self.task_root(t)?;
        let fp = crate::git::snapshot(&root).map(|s| s.fingerprint()).ok();
        let receipts = self.list_verifications(&t.project_id, Some(&t.id))?;
        if receipts.is_empty() {
            return Err(Error::VerificationRequired(format!(
                "{} has no verification receipts; run `evyro verify --task {} -- <test command>`",
                t.key, t.key
            )));
        }
        let latest = receipts.last().unwrap();
        if !latest.passed {
            return Err(Error::VerificationRequired(format!(
                "{}: latest verification `{}` failed (exit {:?})",
                t.key,
                latest.command.join(" "),
                latest.exit_code
            )));
        }
        if fp.is_some() && latest.fingerprint != fp {
            return Err(Error::VerificationRequired(format!(
                "{}: code changed after the last passing verification; re-run `evyro verify`",
                t.key
            )));
        }
        Ok(())
    }

    /// The directory a task's work happens in (its worktree, else project root).
    pub fn task_root(&self, t: &Task) -> Result<std::path::PathBuf> {
        if crate::comparison::is_candidate(t) {
            return crate::comparison::candidate_root(&self.get_project(&t.project_id)?.root(), t);
        }
        if let Some(w) = &t.worktree {
            let p = std::path::PathBuf::from(w);
            if p.exists() {
                return Ok(p);
            }
        }
        Ok(self.get_project(&t.project_id)?.root())
    }

    pub fn add_comment(&self, task_id: &str, author: &str, body: &str) -> Result<String> {
        let t = self.get_task(task_id)?;
        let id = util::new_id("cmt");
        self.conn.execute(
            "INSERT INTO task_comments(id, task_id, author, body, created_at) VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![id, t.id, author, body, util::now()],
        )?;
        self.emit(
            Some(&t.project_id),
            Some(&t.id),
            None,
            "task.commented",
            serde_json::json!({"author": author}),
        )?;
        Ok(id)
    }

    pub fn comments(&self, task_id: &str) -> Result<Vec<(String, String, String)>> {
        let mut st = self.conn.prepare("SELECT author, body, created_at FROM task_comments WHERE task_id = ?1 ORDER BY created_at")?;
        let rows = st.query_map([task_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Add a dependency edge. `blocks` edges are checked for cycles.
    pub fn add_dependency(&self, task_id: &str, depends_on: &str, kind: &str) -> Result<()> {
        if !["blocks", "related"].contains(&kind) {
            return Err(Error::Invalid(
                "dependency kind must be 'blocks' or 'related'".into(),
            ));
        }
        let a = self.get_task(task_id)?;
        let b = self.get_task(depends_on)?;
        if a.project_id != b.project_id {
            return Err(Error::Invalid(
                "dependencies must stay inside one project".into(),
            ));
        }
        if a.id == b.id {
            return Err(Error::Invalid("a task cannot depend on itself".into()));
        }
        if kind == "blocks" && self.reachable(&b.id, &a.id)? {
            return Err(Error::Conflict(format!(
                "adding {} -> {} would create a dependency cycle",
                a.key, b.key
            )));
        }
        self.conn.execute(
            "INSERT OR REPLACE INTO task_deps(task_id, depends_on_id, kind) VALUES (?1,?2,?3)",
            rusqlite::params![a.id, b.id, kind],
        )?;
        self.emit(
            Some(&a.project_id),
            Some(&a.id),
            None,
            "task.dependency_added",
            serde_json::json!({"depends_on": b.key, "kind": kind}),
        )?;
        Ok(())
    }

    fn blocking_edges(&self, project_id: &str) -> Result<HashMap<String, Vec<String>>> {
        let mut st = self.conn.prepare(
            "SELECT d.task_id, d.depends_on_id FROM task_deps d JOIN tasks t ON t.id = d.task_id
             WHERE t.project_id = ?1 AND d.kind = 'blocks'",
        )?;
        let mut m: HashMap<String, Vec<String>> = HashMap::new();
        for r in st.query_map([project_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })? {
            let (a, b) = r?;
            m.entry(a).or_default().push(b);
        }
        Ok(m)
    }

    /// Is `to` reachable from `from` following depends-on edges?
    fn reachable(&self, from: &str, to: &str) -> Result<bool> {
        let t = self.get_task(from)?;
        let edges = self.blocking_edges(&t.project_id)?;
        let mut stack = vec![from.to_string()];
        let mut seen = HashSet::new();
        while let Some(n) = stack.pop() {
            if n == to {
                return Ok(true);
            }
            if seen.insert(n.clone()) {
                if let Some(next) = edges.get(&n) {
                    stack.extend(next.iter().cloned());
                }
            }
        }
        Ok(false)
    }

    pub fn dependencies(&self, task_id: &str) -> Result<Vec<Dependency>> {
        let mut st = self.conn.prepare(
            "SELECT a.seq, b.seq, d.kind FROM task_deps d JOIN tasks a ON a.id = d.task_id JOIN tasks b ON b.id = d.depends_on_id
             WHERE d.task_id = ?1 OR d.depends_on_id = ?1",
        )?;
        let rows = st.query_map([task_id], |r| {
            Ok(Dependency {
                task: format!("T-{}", r.get::<_, i64>(0)?),
                depends_on: format!("T-{}", r.get::<_, i64>(1)?),
                kind: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Unfinished tasks that block `task_id`.
    pub fn open_blockers(&self, task_id: &str) -> Result<Vec<Task>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {} FROM tasks WHERE id IN (SELECT depends_on_id FROM task_deps WHERE task_id = ?1 AND kind = 'blocks')
             AND status NOT IN ('done', 'verified', 'cancelled')",
            TASK_COLS
        ))?;
        let rows = st.query_map([task_id], row_to_task)?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Ready work: backlog/ready tasks with no open blockers and no active lease.
    pub fn ready_tasks(&self, project_id: &str) -> Result<Vec<Task>> {
        let mut out = Vec::new();
        for t in self.list_tasks(project_id, None, false)? {
            if matches!(t.status, TaskStatus::Backlog | TaskStatus::Ready)
                && t.kind != "epic"
                && self.open_blockers(&t.id)?.is_empty()
                && self.lease(&t.id)?.is_none()
            {
                out.push(t);
            }
        }
        Ok(out)
    }

    /// The task currently in progress in this project (most recently updated).
    pub fn active_task(&self, project_id: &str) -> Result<Option<Task>> {
        let v = self.list_tasks(project_id, Some(TaskStatus::InProgress), false)?;
        Ok(v.into_iter().max_by(|a, b| a.updated_at.cmp(&b.updated_at)))
    }
}

#[cfg(test)]
mod tests {
    use super::TaskStatus::*;
    #[test]
    fn transitions() {
        assert!(Backlog.can_transition_to(InProgress));
        assert!(InReview.can_transition_to(Verified));
        assert!(!Backlog.can_transition_to(Done));
        assert!(!InProgress.can_transition_to(Done));
        assert!(!Done.can_transition_to(Cancelled));
        assert!(Verified.can_transition_to(Done));
    }
    #[test]
    fn parse_status() {
        assert_eq!(
            "in-progress".parse::<super::TaskStatus>().unwrap(),
            InProgress
        );
        assert!("nope".parse::<super::TaskStatus>().is_err());
    }
}
