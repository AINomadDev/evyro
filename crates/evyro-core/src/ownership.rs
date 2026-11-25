//! Who owns which files: what the board shows, and declaring a task's files from its plan.
//!
//! A task declares the files or folders it changes. Two tasks that declare overlapping files are
//! never run side by side, so the board names, for each one, the live AI that blocks it and the
//! other open tasks it overlaps. An approved plan can declare the files for the task: Evyro reads
//! the paths the plan names and keeps those that exist in the checkout.

use crate::db::Store;
use crate::error::Result;
use crate::sessions::paths_overlap;
use crate::tasks::{Actor, Task, TaskUpdate};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;

/// The most paths one plan may declare.
pub const MAX_PLAN_FILES: usize = 12;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Blocker {
    pub task: String,
    /// The AI holding it.
    pub owner: String,
    /// The file or folder that overlaps.
    pub file: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct FileView {
    /// A live AI holds a task with overlapping files: this one cannot start until it is done.
    pub blocked_by: Option<Blocker>,
    /// Other open tasks that declare overlapping files (they will have to take turns).
    pub overlaps: Vec<String>,
}

impl Store {
    /// For each task of the project that declares files, who blocks it and what it overlaps.
    pub fn file_views(&self, project_id: &str) -> Result<HashMap<String, FileView>> {
        let open: Vec<Task> = self
            .list_tasks(project_id, None, false)?
            .into_iter()
            .filter(|t| !t.files.is_empty())
            .collect();
        let mut out = HashMap::new();
        for t in &open {
            let held = self.active_session_for_task(&t.id)?.is_some();
            let blocked_by = if held {
                None // it is the holder; nothing blocks it
            } else {
                self.file_holder(t, None)?.map(|h| Blocker {
                    task: h.task.key,
                    owner: h.harness,
                    file: h.file,
                })
            };
            let overlaps: Vec<String> = open
                .iter()
                .filter(|o| o.id != t.id && o.worktree == t.worktree)
                .filter(|o| {
                    t.files
                        .iter()
                        .any(|a| o.files.iter().any(|b| paths_overlap(a, b)))
                })
                .map(|o| o.key.clone())
                .collect();
            if blocked_by.is_some() || !overlaps.is_empty() {
                out.insert(
                    t.id.clone(),
                    FileView {
                        blocked_by,
                        overlaps,
                    },
                );
            }
        }
        Ok(out)
    }

    /// Declare the files the plan of `task` names. Returns the paths added (empty when the plan
    /// names no path that exists, or the task already declares them).
    pub fn declare_plan_files(&self, task: &Task) -> Result<Vec<String>> {
        let Some(plan) = self.plan(&task.id)? else {
            return Ok(vec![]);
        };
        let root = self.task_root(task)?;
        let named = plan_files(&plan.body, &root);
        let new: Vec<String> = named
            .into_iter()
            .filter(|f| !task.files.iter().any(|have| have == f))
            .collect();
        if new.is_empty() {
            return Ok(new);
        }
        self.update_task(
            &task.id,
            TaskUpdate {
                add_files: new.clone(),
                ..Default::default()
            },
            &Actor::Human {
                override_reason: None,
            },
        )?;
        self.emit(
            Some(&task.project_id),
            Some(&task.id),
            None,
            "task.files_declared",
            serde_json::json!({"task": task.key, "files": new, "from": "plan"}),
        )?;
        Ok(new)
    }
}

/// Paths a plan names that exist under `root`, as repository-relative paths (folders without a
/// trailing slash). Absolute paths, URLs and anything leaving the repository are ignored.
pub fn plan_files(plan: &str, root: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for raw in plan.split(|c: char| c.is_whitespace() || "`'\"(),;:<>[]{}".contains(c)) {
        let tok = raw
            .trim_end_matches(|c| ".!?".contains(c))
            .trim_start_matches("./");
        let tok = tok.trim_end_matches('/');
        if tok.is_empty()
            || tok.starts_with('/')
            || tok.starts_with('~')
            || tok.contains("..")
            || tok.contains("//")
            || tok.contains('*')
            || tok.contains("://")
            || tok
                .chars()
                .any(|c| !(c.is_alphanumeric() || "/._-".contains(c)))
        {
            continue;
        }
        let name = tok.rsplit('/').next().unwrap_or(tok);
        let looks_like_path = tok.contains('/')
            || name.rsplit_once('.').is_some_and(|(stem, ext)| {
                !stem.is_empty()
                    && !ext.is_empty()
                    && ext.len() <= 5
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
                    && ext.chars().any(|c| c.is_ascii_alphabetic())
            });
        if !looks_like_path || !root.join(tok).exists() {
            continue;
        }
        if !found.iter().any(|f| f == tok) {
            found.push(tok.to_owned());
        }
        if found.len() >= MAX_PLAN_FILES {
            break;
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{NewTask, TaskStatus};

    fn project() -> (tempfile::TempDir, Store, String, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("src/api")).unwrap();
        std::fs::write(root.join("src/api/mod.rs"), "x\n").unwrap();
        std::fs::write(root.join("src/ui.rs"), "x\n").unwrap();
        std::fs::write(root.join("README.md"), "x\n").unwrap();
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "t@example.com"]);
        git(&["config", "user.name", "t"]);
        git(&["add", "-A"]);
        git(&["commit", "-qm", "init"]);
        let pid = store.register_project(&root, None).unwrap().project.id;
        (dir, store, pid, root)
    }

    fn task(store: &Store, pid: &str, title: &str, files: &[&str]) -> Task {
        store
            .create_task(
                pid,
                NewTask {
                    title: title.into(),
                    files: files.iter().map(|f| f.to_string()).collect(),
                    status: Some(TaskStatus::Ready),
                    ..Default::default()
                },
            )
            .unwrap()
    }

    #[test]
    fn only_paths_that_exist_in_the_checkout_are_taken_from_a_plan() {
        let (_d, _s, _p, root) = project();
        let plan = "1. Edit `src/api/mod.rs` and src/ui.rs, then update README.md.\n2. Add src/new_file.rs (new), see https://example.com/a/b.html and /etc/passwd, ../outside.rs, e.g. v1.2 and src/api/.\n3. Run `cargo test` in ./src/api.";
        assert_eq!(
            plan_files(plan, &root),
            ["src/api/mod.rs", "src/ui.rs", "README.md", "src/api"]
        );
        assert!(plan_files("no paths here", &root).is_empty());
    }

    #[test]
    fn an_approved_plan_declares_its_files_once() {
        let (_d, store, pid, _root) = project();
        let t = task(&store, &pid, "Work", &[]);
        assert!(
            store.declare_plan_files(&t).unwrap().is_empty(),
            "no plan, nothing declared"
        );
        store
            .propose_plan(&t, "Change src/ui.rs and README.md", "agent:codex")
            .unwrap();
        let added = store.declare_plan_files(&t).unwrap();
        assert_eq!(added, ["src/ui.rs", "README.md"]);
        let t = store.get_task(&t.id).unwrap();
        assert_eq!(t.files, ["src/ui.rs", "README.md"]);
        assert!(
            store.declare_plan_files(&t).unwrap().is_empty(),
            "already declared"
        );
    }

    #[test]
    fn the_board_names_the_owner_that_blocks_a_task_and_the_tasks_that_overlap() {
        let (_d, store, pid, _root) = project();
        let api = task(&store, &pid, "API", &["src/api"]);
        let tests = task(&store, &pid, "API tests", &["src/api/mod.rs"]);
        let ui = task(&store, &pid, "UI", &["src/ui.rs"]);
        let _none = task(&store, &pid, "No files", &[]);
        // Nobody holds anything yet: the overlap is shown, nothing is blocked.
        let v = store.file_views(&pid).unwrap();
        assert_eq!(v[&api.id].overlaps, std::slice::from_ref(&tests.key));
        assert!(v[&api.id].blocked_by.is_none());
        assert!(!v.contains_key(&ui.id), "UI overlaps nothing");
        // An AI takes the API task: the tests task is blocked by it, by name.
        store
            .start_session(
                &pid,
                crate::sessions::NewSession {
                    task_id: Some(api.id.clone()),
                    harness: "codex".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let v = store.file_views(&pid).unwrap();
        let b = v[&tests.id].blocked_by.clone().unwrap();
        assert_eq!(
            (b.task.as_str(), b.owner.as_str(), b.file.as_str()),
            (api.key.as_str(), "codex", "src/api/mod.rs")
        );
        assert!(
            v[&api.id].blocked_by.is_none(),
            "the holder is not blocked by itself"
        );
        assert!(!v.contains_key(&ui.id));
    }
}
