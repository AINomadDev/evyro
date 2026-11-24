//! Where the AI on a task works: the main checkout, a worktree that already holds the task's
//! work, or a new worktree. A new worktree is made only when another AI is working in the
//! main checkout and the two cannot be shown to touch different files.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::projects::Project;
use crate::sessions::Session;
use crate::tasks::{Actor, Task, TaskUpdate};
use crate::{comparison, git, sessions, util};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// What to do about the working copy before an AI starts a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "plan", rename_all = "snake_case")]
pub enum CheckoutPlan {
    /// Work in the main checkout.
    Main { why: String },
    /// Continue in the task's own worktree, which already holds its work.
    Reuse { why: String },
    /// Continue in the parent task's worktree, which holds unfinished work nobody is using.
    AdoptParent { why: String },
    /// Make a worktree for this task.
    Isolate { why: String },
    /// Two AIs would edit the same files in one working copy.
    Blocked { why: String },
}

impl CheckoutPlan {
    pub fn why(&self) -> &str {
        match self {
            Self::Main { why }
            | Self::Reuse { why }
            | Self::AdoptParent { why }
            | Self::Isolate { why }
            | Self::Blocked { why } => why,
        }
    }
}

/// The task as it will run, and a line saying what was decided when it matters.
#[derive(Debug, Clone, Serialize)]
pub struct Checkout {
    pub task: Task,
    pub plan: CheckoutPlan,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pruned {
    pub task: String,
    pub path: String,
    pub branch: Option<String>,
    pub removed: bool,
    pub reason: String,
}

fn paths_overlap(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.trim_start_matches("./").trim_end_matches('/').to_owned();
    let (a, b) = (norm(a), norm(b));
    a == b || a.starts_with(&format!("{b}/")) || b.starts_with(&format!("{a}/"))
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (util::canonical(a), util::canonical(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

impl Store {
    /// Give `task` its own worktree and branch (`evyro/<key>`), next to the repository unless
    /// `path` says otherwise. Reuses the branch when an earlier run left it behind.
    pub fn add_task_worktree(
        &self,
        p: &Project,
        task: &Task,
        branch: Option<String>,
        path: Option<PathBuf>,
    ) -> Result<Task> {
        let root = p.root();
        let branch = branch.unwrap_or_else(|| format!("evyro/{}", task.key.to_lowercase()));
        let path = path.unwrap_or_else(|| {
            root.parent()
                .unwrap_or(&root)
                .join(format!("{}-{}", p.name, task.key.to_lowercase()))
        });
        git::prune_worktree_records(&root);
        if git::branch_exists(&root, &branch) {
            git::add_worktree_on_branch(&root, &path, &branch)?;
        } else {
            git::add_worktree(&root, &path, &branch)?;
        }
        self.point_task_at(task, &branch, &path)
    }

    fn point_task_at(&self, task: &Task, branch: &str, path: &Path) -> Result<Task> {
        let worktree = util::canonical(path)?.to_string_lossy().into_owned();
        self.update_task(
            &task.id,
            TaskUpdate {
                branch: Some(branch.to_owned()),
                worktree: Some(worktree),
                ..Default::default()
            },
            &Actor::Human {
                override_reason: None,
            },
        )
    }

    /// Live sessions working in the main checkout on something other than `task`.
    fn others_in_main(&self, p: &Project, task: &Task) -> Result<Vec<Session>> {
        let root = p.root();
        Ok(self
            .list_sessions(&p.id, true)?
            .into_iter()
            .filter(|s| {
                let in_main = s
                    .worktree
                    .as_deref()
                    .map_or(true, |w| same_dir(Path::new(w), &root));
                let alive = s.pid.map_or(true, |pid| {
                    sessions::session_process_alive(pid, s.pid_started)
                });
                in_main && alive && s.task_id.as_deref() != Some(task.id.as_str())
            })
            .collect())
    }

    /// Whether a worktree holds work that is not in the main checkout's history yet.
    fn has_pending_work(&self, p: &Project, worktree: &Path, branch: Option<&str>) -> bool {
        git::dirty_paths(worktree).map_or(true, |d| !d.is_empty())
            || branch.is_some_and(|b| git::commits_ahead(&p.root(), "HEAD", b) > 0)
    }

    fn live_in(&self, p: &Project, dir: &Path) -> Result<bool> {
        Ok(self.list_sessions(&p.id, true)?.into_iter().any(|s| {
            s.worktree
                .as_deref()
                .is_some_and(|w| same_dir(Path::new(w), dir))
                && s.pid.map_or(true, |pid| {
                    sessions::session_process_alive(pid, s.pid_started)
                })
        }))
    }

    /// Decide where the AI on `task` should work. Changes nothing.
    pub fn plan_checkout(&self, p: &Project, task: &Task) -> Result<CheckoutPlan> {
        let main = |why: &str| Ok(CheckoutPlan::Main { why: why.into() });
        if comparison::is_candidate(task) {
            return main("a comparison candidate already has its own checkout");
        }
        // Work that is already in progress stays where it is.
        if task.worktree.is_some() {
            return Ok(CheckoutPlan::Reuse {
                why: format!("{} continues in its own worktree", task.key),
            });
        }
        // A follow-up to an unfinished task continues in that task's idle worktree.
        if let Some(parent) = task
            .parent_id
            .as_deref()
            .and_then(|id| self.get_task(id).ok())
            .filter(|t| !comparison::is_candidate(t))
        {
            if let Some(w) = parent.worktree.as_deref().map(Path::new) {
                if w.is_dir()
                    && !self.live_in(p, w)?
                    && self.has_pending_work(p, w, parent.branch.as_deref())
                {
                    return Ok(CheckoutPlan::AdoptParent {
                        why: format!(
                            "{} continues {}'s unfinished work in its worktree",
                            task.key, parent.key
                        ),
                    });
                }
            }
        }
        let others = self.others_in_main(p, task)?;
        if others.is_empty() {
            return main("no other AI is working in the main checkout");
        }
        let root = p.root();
        let mut busy: Vec<(String, String)> = Vec::new(); // (path, who)
        let mut unknown_scope = false;
        for s in &others {
            let other = s.task_id.as_deref().and_then(|id| self.get_task(id).ok());
            match &other {
                Some(t) if !t.files.is_empty() => {
                    busy.extend(
                        t.files
                            .iter()
                            .map(|f| (f.clone(), format!("{} ({})", t.key, s.harness))),
                    );
                }
                _ => unknown_scope = true,
            }
        }
        let dirty = git::dirty_paths(&root).unwrap_or_default();
        if !task.files.is_empty() {
            let hit = task.files.iter().find_map(|f| {
                busy.iter()
                    .find(|(b, _)| paths_overlap(f, b))
                    .map(|(_, who)| (f.clone(), who.clone()))
                    .or_else(|| {
                        dirty
                            .iter()
                            .find(|d| paths_overlap(f, d))
                            .map(|d| (d.clone(), "another AI's uncommitted changes".to_owned()))
                    })
            });
            if let Some((file, who)) = hit {
                return Ok(CheckoutPlan::Blocked {
                    why: format!(
                        "{} changes {file}, which {who} is working on in the main checkout. Only one AI works on a set of files at a time: finish or switch that task first",
                        task.key
                    ),
                });
            }
            if !unknown_scope {
                return main("its files are independent of what the other AI is working on");
            }
        }
        if git::head_sha(&root)?.is_none() {
            return main("the repository has no commit yet, so it cannot have worktrees");
        }
        Ok(CheckoutPlan::Isolate {
            why: if task.files.is_empty() {
                format!(
                    "{} has no declared files, so Evyro cannot show it is independent of the AI already in the main checkout",
                    task.key
                )
            } else {
                "the AI already in the main checkout has no declared files, so Evyro cannot show the two are independent".to_owned()
            },
        })
    }

    /// Apply the plan: set up the worktree if one is needed and return the task as it will run.
    pub fn prepare_checkout(&self, p: &Project, task: &Task) -> Result<Checkout> {
        let plan = self.plan_checkout(p, task)?;
        let root = p.root();
        let mut note = None;
        let task = match &plan {
            CheckoutPlan::Main { .. } => task.clone(),
            CheckoutPlan::Blocked { why } => return Err(Error::Conflict(why.clone())),
            CheckoutPlan::Reuse { .. } => {
                let path = PathBuf::from(task.worktree.as_deref().unwrap_or_default());
                let branch = task
                    .branch
                    .clone()
                    .unwrap_or_else(|| format!("evyro/{}", task.key.to_lowercase()));
                let registered = git::worktrees(&root)
                    .map(|ws| ws.iter().any(|w| same_dir(Path::new(&w.path), &path)))
                    .unwrap_or(false);
                if !(path.is_dir() && registered) {
                    // The folder was removed (or never made): bring it back on the same branch.
                    if path.exists() {
                        return Err(Error::Conflict(format!(
                            "{} exists but is not a worktree of this repository; move it away and run again",
                            path.display()
                        )));
                    }
                    git::prune_worktree_records(&root);
                    if git::branch_exists(&root, &branch) {
                        git::add_worktree_on_branch(&root, &path, &branch)?;
                    } else {
                        git::add_worktree(&root, &path, &branch)?;
                    }
                    note = Some(format!(
                        "{} continues in its worktree {} (restored on branch {branch}).",
                        task.key,
                        path.display()
                    ));
                }
                task.clone()
            }
            CheckoutPlan::AdoptParent { why } => {
                let parent = task
                    .parent_id
                    .as_deref()
                    .map(|id| self.get_task(id))
                    .transpose()?
                    .ok_or_else(|| Error::NotFound("parent task".into()))?;
                note = Some(format!("{why}."));
                self.point_task_at(
                    task,
                    parent.branch.as_deref().unwrap_or_default(),
                    Path::new(parent.worktree.as_deref().unwrap_or_default()),
                )?
            }
            CheckoutPlan::Isolate { why } => {
                let dirty_main = git::dirty_paths(&root).is_ok_and(|d| !d.is_empty());
                let t = self.add_task_worktree(p, task, None, None)?;
                note = Some(format!(
                    "{} runs in its own worktree {} (branch {}) because {why}.{}",
                    t.key,
                    t.worktree.as_deref().unwrap_or_default(),
                    t.branch.as_deref().unwrap_or_default(),
                    if dirty_main {
                        " Uncommitted changes in the main checkout are not in it."
                    } else {
                        ""
                    }
                ));
                t
            }
        };
        Ok(Checkout { task, plan, note })
    }

    /// Remove worktrees of finished tasks that are clean, merged and not in use, and delete their
    /// merged branches. Anything with unmerged or uncommitted work is kept and says why.
    pub fn prune_worktrees(&self, p: &Project, dry_run: bool) -> Result<Vec<Pruned>> {
        let root = p.root();
        git::prune_worktree_records(&root);
        let mut out = Vec::new();
        for task in self.list_tasks(&p.id, None, true)? {
            let (Some(wt), false) = (task.worktree.clone(), comparison::is_candidate(&task)) else {
                continue;
            };
            let path = PathBuf::from(&wt);
            let mut item = Pruned {
                task: task.key.clone(),
                path: wt.clone(),
                branch: task.branch.clone(),
                removed: false,
                reason: String::new(),
            };
            let keep = |item: &mut Pruned, why: &str| {
                item.reason = why.to_owned();
            };
            if !task.status.is_closed() {
                keep(&mut item, "the task is not finished");
            } else if !path.is_dir() {
                item.reason = "already gone".into();
            } else if self.live_in(p, &path)? {
                keep(&mut item, "an AI is still working in it");
            } else if git::dirty_paths(&path).map_or(true, |d| !d.is_empty()) {
                keep(&mut item, "it has uncommitted changes");
            } else if task
                .branch
                .as_deref()
                .is_some_and(|b| git::commits_ahead(&root, "HEAD", b) > 0)
            {
                keep(&mut item, "its branch has commits that are not merged");
            } else if dry_run {
                item.reason = "would remove".into();
            } else {
                git::remove_worktree(&root, &path, false)?;
                if let Some(b) = task.branch.as_deref() {
                    git::delete_merged_branch(&root, b);
                }
                item.removed = true;
                item.reason = "removed".into();
            }
            out.push(item);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::NewSession;
    use crate::tasks::{NewTask, TaskStatus};

    struct Fix {
        _dir: tempfile::TempDir,
        store: Store,
        project: Project,
        root: PathBuf,
    }

    fn git(root: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn fixture(commit: bool) -> Fix {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let root = dir.path().join("repo");
        std::fs::create_dir_all(root.join("src")).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.email", "t@example.com"]);
        git(&root, &["config", "user.name", "t"]);
        git(&root, &["config", "core.autocrlf", "false"]);
        std::fs::write(root.join("src/a.rs"), "a\n").unwrap();
        if commit {
            git(&root, &["add", "-A"]);
            git(&root, &["commit", "-qm", "init"]);
        }
        let project = store.register_project(&root, None).unwrap().project;
        Fix {
            _dir: dir,
            store,
            project,
            root,
        }
    }

    fn task(f: &Fix, title: &str, files: &[&str], parent: Option<&Task>) -> Task {
        f.store
            .create_task(
                &f.project.id,
                NewTask {
                    title: title.into(),
                    files: files.iter().map(|x| x.to_string()).collect(),
                    parent: parent.map(|t| t.key.clone()),
                    ..Default::default()
                },
            )
            .unwrap()
    }

    fn work_on(f: &Fix, t: &Task, harness: &str) -> crate::sessions::Session {
        f.store
            .start_session(
                &f.project.id,
                NewSession {
                    task_id: Some(t.id.clone()),
                    harness: harness.into(),
                    worktree: t.worktree.clone(),
                    ..Default::default()
                },
            )
            .unwrap()
    }

    fn worktree_count(f: &Fix) -> usize {
        crate::git::worktrees(&f.root).unwrap().len()
    }

    fn refresh(f: &Fix, t: &Task) -> Task {
        f.store.get_task(&t.id).unwrap()
    }

    #[test]
    fn an_ai_working_alone_stays_in_the_main_checkout() {
        let f = fixture(true);
        let t = task(&f, "Work", &[], None);
        let c = f.store.prepare_checkout(&f.project, &t).unwrap();
        assert!(matches!(c.plan, CheckoutPlan::Main { .. }), "{:?}", c.plan);
        assert!(c.task.worktree.is_none() && c.note.is_none());
        assert_eq!(worktree_count(&f), 1);
    }

    #[test]
    fn a_busy_main_checkout_with_unknown_scope_gets_a_worktree_for_the_second_task() {
        let f = fixture(true);
        let (t1, t2) = (task(&f, "One", &[], None), task(&f, "Two", &[], None));
        work_on(&f, &t1, "claude-code");
        let c = f.store.prepare_checkout(&f.project, &t2).unwrap();
        assert!(
            matches!(c.plan, CheckoutPlan::Isolate { .. }),
            "{:?}",
            c.plan
        );
        let wt = refresh(&f, &t2);
        assert_eq!(wt.branch.as_deref(), Some("evyro/t-2"));
        assert!(Path::new(wt.worktree.as_deref().unwrap()).is_dir());
        assert_eq!(worktree_count(&f), 2);
        assert!(c.note.unwrap().contains("own worktree"));
    }

    #[test]
    fn tasks_with_independent_declared_files_share_the_checkout() {
        let f = fixture(true);
        let t1 = task(&f, "Src", &["src"], None);
        let t2 = task(&f, "Docs", &["docs"], None);
        work_on(&f, &t1, "claude-code");
        let c = f.store.prepare_checkout(&f.project, &t2).unwrap();
        assert!(matches!(c.plan, CheckoutPlan::Main { .. }), "{:?}", c.plan);
        assert!(refresh(&f, &t2).worktree.is_none());
        assert_eq!(
            worktree_count(&f),
            1,
            "no worktree for tasks that do not conflict"
        );
    }

    #[test]
    fn overlapping_declared_files_are_refused_not_worked_around() {
        let f = fixture(true);
        let t1 = task(&f, "Src", &["src"], None);
        let t2 = task(&f, "One file", &["src/a.rs"], None);
        work_on(&f, &t1, "claude-code");
        let err = f.store.prepare_checkout(&f.project, &t2).unwrap_err();
        assert!(matches!(err, Error::Conflict(_)), "{err}");
        assert!(
            err.to_string()
                .contains("Only one AI works on a set of files"),
            "{err}"
        );
        assert_eq!(worktree_count(&f), 1);
    }

    #[test]
    fn a_task_with_files_still_gets_a_worktree_when_the_other_ai_has_no_declared_files() {
        let f = fixture(true);
        let t1 = task(&f, "Unscoped", &[], None);
        let t2 = task(&f, "Docs", &["docs"], None);
        work_on(&f, &t1, "claude-code");
        let c = f.store.prepare_checkout(&f.project, &t2).unwrap();
        assert!(
            matches!(c.plan, CheckoutPlan::Isolate { .. }),
            "{:?}",
            c.plan
        );
    }

    #[test]
    fn uncommitted_changes_in_the_files_a_task_needs_block_it() {
        let f = fixture(true);
        let t1 = task(&f, "Unscoped", &[], None);
        let t2 = task(&f, "Src", &["src"], None);
        work_on(&f, &t1, "claude-code");
        std::fs::write(f.root.join("src/a.rs"), "being edited\n").unwrap();
        let err = f.store.prepare_checkout(&f.project, &t2).unwrap_err();
        assert!(err.to_string().contains("uncommitted changes"), "{err}");
    }

    #[test]
    fn unfinished_work_keeps_its_worktree_and_asking_twice_makes_no_second_one() {
        let f = fixture(true);
        let (t1, t2) = (task(&f, "One", &[], None), task(&f, "Two", &[], None));
        work_on(&f, &t1, "claude-code");
        f.store.prepare_checkout(&f.project, &t2).unwrap();
        let first = refresh(&f, &t2);
        let again = f.store.prepare_checkout(&f.project, &first).unwrap();
        assert!(
            matches!(again.plan, CheckoutPlan::Reuse { .. }),
            "{:?}",
            again.plan
        );
        assert_eq!(again.task.worktree, first.worktree);
        assert_eq!(worktree_count(&f), 2);
        assert!(
            again.note.is_none(),
            "nothing to announce when nothing changed"
        );
    }

    #[test]
    fn a_removed_worktree_is_restored_on_its_branch_with_the_pending_commits() {
        let f = fixture(true);
        let (t1, t2) = (task(&f, "One", &[], None), task(&f, "Two", &[], None));
        work_on(&f, &t1, "claude-code");
        f.store.prepare_checkout(&f.project, &t2).unwrap();
        let t2 = refresh(&f, &t2);
        let wt = PathBuf::from(t2.worktree.clone().unwrap());
        std::fs::write(wt.join("src/new.rs"), "work\n").unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-qm", "pending work"]);
        std::fs::remove_dir_all(&wt).unwrap();
        let c = f.store.prepare_checkout(&f.project, &t2).unwrap();
        assert!(matches!(c.plan, CheckoutPlan::Reuse { .. }));
        assert!(
            wt.join("src/new.rs").is_file(),
            "the committed work came back"
        );
        assert!(c.note.unwrap().contains("restored"));
        assert_eq!(worktree_count(&f), 2);
    }

    #[test]
    fn a_follow_up_task_continues_in_its_parents_idle_worktree() {
        let f = fixture(true);
        let other = task(&f, "Other", &[], None);
        let epic = task(&f, "Feature", &[], None);
        work_on(&f, &other, "claude-code");
        f.store.prepare_checkout(&f.project, &epic).unwrap();
        let epic = refresh(&f, &epic);
        let wt = PathBuf::from(epic.worktree.clone().unwrap());
        std::fs::write(wt.join("src/wip.rs"), "unfinished\n").unwrap();
        let child = task(&f, "Follow-up", &[], Some(&epic));
        let c = f.store.prepare_checkout(&f.project, &child).unwrap();
        assert!(
            matches!(c.plan, CheckoutPlan::AdoptParent { .. }),
            "{:?}",
            c.plan
        );
        assert_eq!(refresh(&f, &child).worktree, epic.worktree);
        assert_eq!(worktree_count(&f), 2, "no extra worktree");
    }

    #[test]
    fn a_parents_worktree_in_use_is_not_shared() {
        let f = fixture(true);
        let other = task(&f, "Other", &[], None);
        let epic = task(&f, "Feature", &[], None);
        work_on(&f, &other, "claude-code");
        f.store.prepare_checkout(&f.project, &epic).unwrap();
        let epic = refresh(&f, &epic);
        std::fs::write(
            Path::new(epic.worktree.as_deref().unwrap()).join("wip.rs"),
            "x\n",
        )
        .unwrap();
        work_on(&f, &epic, "codex");
        let child = task(&f, "Follow-up", &[], Some(&epic));
        let c = f.store.prepare_checkout(&f.project, &child).unwrap();
        assert!(
            !matches!(c.plan, CheckoutPlan::AdoptParent { .. }),
            "{:?}",
            c.plan
        );
    }

    #[test]
    fn once_the_other_ai_is_done_the_next_task_is_back_in_the_main_checkout() {
        let f = fixture(true);
        let (t1, t2, t3) = (
            task(&f, "One", &[], None),
            task(&f, "Two", &[], None),
            task(&f, "Three", &[], None),
        );
        let s1 = work_on(&f, &t1, "claude-code");
        assert!(matches!(
            f.store.prepare_checkout(&f.project, &t2).unwrap().plan,
            CheckoutPlan::Isolate { .. }
        ));
        f.store
            .set_session_status(&s1.id, crate::sessions::SessionStatus::Completed, None)
            .unwrap();
        let c = f.store.prepare_checkout(&f.project, &t3).unwrap();
        assert!(
            matches!(c.plan, CheckoutPlan::Main { .. }),
            "not a worktree every time: {:?}",
            c.plan
        );
        assert_eq!(worktree_count(&f), 2);
    }

    #[test]
    fn a_repository_with_no_commit_cannot_have_worktrees_so_work_stays_in_main() {
        let f = fixture(false);
        let (t1, t2) = (task(&f, "One", &[], None), task(&f, "Two", &[], None));
        work_on(&f, &t1, "claude-code");
        let c = f.store.prepare_checkout(&f.project, &t2).unwrap();
        assert!(matches!(c.plan, CheckoutPlan::Main { .. }), "{:?}", c.plan);
    }

    #[test]
    fn prune_removes_only_finished_clean_merged_worktrees() {
        let f = fixture(true);
        let busy = task(&f, "Busy", &[], None);
        work_on(&f, &busy, "claude-code");
        let mk = |title: &str| {
            let t = task(&f, title, &[], None);
            f.store.prepare_checkout(&f.project, &t).unwrap();
            refresh(&f, &t)
        };
        let (clean, dirty, unmerged, open) = (mk("Clean"), mk("Dirty"), mk("Unmerged"), mk("Open"));
        std::fs::write(
            Path::new(dirty.worktree.as_deref().unwrap()).join("x.rs"),
            "x\n",
        )
        .unwrap();
        let wt = Path::new(unmerged.worktree.as_deref().unwrap()).to_path_buf();
        std::fs::write(wt.join("y.rs"), "y\n").unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-qm", "not merged"]);
        for t in [&clean, &dirty, &unmerged] {
            f.store
                .update_task(
                    &t.id,
                    TaskUpdate {
                        status: Some(TaskStatus::Cancelled),
                        ..Default::default()
                    },
                    &Actor::Human {
                        override_reason: None,
                    },
                )
                .unwrap();
        }
        let before = worktree_count(&f);
        let dry = f.store.prune_worktrees(&f.project, true).unwrap();
        assert_eq!(worktree_count(&f), before, "a dry run removes nothing");
        assert!(dry
            .iter()
            .any(|i| i.task == clean.key && i.reason == "would remove"));
        let done = f.store.prune_worktrees(&f.project, false).unwrap();
        let by = |k: &str| done.iter().find(|i| i.task == k).unwrap();
        assert!(by(&clean.key).removed);
        assert!(!Path::new(clean.worktree.as_deref().unwrap()).exists());
        assert!(!by(&dirty.key).removed && by(&dirty.key).reason.contains("uncommitted"));
        assert!(!by(&unmerged.key).removed && by(&unmerged.key).reason.contains("not merged"));
        assert!(!by(&open.key).removed && by(&open.key).reason.contains("not finished"));
        assert!(Path::new(open.worktree.as_deref().unwrap()).is_dir());
    }
}
