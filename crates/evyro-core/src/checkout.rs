//! Where the AI on a task works: the main checkout, a worktree that already holds the task's
//! work, or a new worktree. A new worktree is made only when another AI is working in the
//! main checkout and the two cannot be shown to touch different files.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::projects::Project;
use crate::sessions::Session;
use crate::tasks::{Actor, Task, TaskUpdate};
use crate::{comparison, git, util};
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
    /// Bytes on disk, measured for worktrees that are (or would be) removed.
    pub bytes: u64,
}

/// How a prune decides which worktrees are finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneOptions {
    pub dry_run: bool,
    /// A worktree of an unfinished task also counts as finished once its last session ended
    /// this many days ago. `None` leaves those alone.
    pub idle_days: Option<u32>,
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
        git::mark_created(&path);
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
                in_main && s.is_live() && s.task_id.as_deref() != Some(task.id.as_str())
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
                && s.is_live()
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
                    git::mark_created(&path);
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

    /// Whether Evyro made the worktree at `path`: it carries Evyro's mark, or one of the tasks
    /// in it is on the branch Evyro names after that task.
    fn evyro_made(path: &Path, tasks: &[&Task]) -> bool {
        git::is_marked_created(path)
            || tasks
                .iter()
                .any(|t| t.branch.as_deref() == Some(&format!("evyro/{}", t.key.to_lowercase())))
    }

    /// Why the work in a task's worktree counts as finished, or `None` when it does not.
    fn finished_why(
        &self,
        p: &Project,
        main: &str,
        task: &Task,
        path: &Path,
        idle_days: Option<u32>,
    ) -> Result<Option<String>> {
        if task.status.is_closed() {
            return Ok(Some(format!("{} is {}", task.key, task.status.as_str())));
        }
        let root = p.root();
        if let Some(b) = task.branch.as_deref() {
            if git::merged_by_merge(&root, main, b) {
                return Ok(Some(format!("its branch {b} is merged into {main}")));
            }
        }
        let Some(days) = idle_days else {
            return Ok(None);
        };
        let sessions: Vec<Session> = self
            .list_sessions(&p.id, false)?
            .into_iter()
            .filter(|s| {
                s.worktree
                    .as_deref()
                    .is_some_and(|w| same_dir(Path::new(w), path))
            })
            .collect();
        if sessions.is_empty() || sessions.iter().any(|s| s.ended_at.is_none() && s.is_live()) {
            return Ok(None);
        }
        let last = sessions
            .iter()
            .filter_map(|s| s.ended_at.as_deref().or(s.last_event_at.as_deref()))
            .filter_map(|t| {
                time::OffsetDateTime::parse(t, &time::format_description::well_known::Rfc3339).ok()
            })
            .max();
        let Some(last) = last else { return Ok(None) };
        let age = (time::OffsetDateTime::now_utc() - last).whole_days();
        Ok((age >= i64::from(days)).then(|| format!("its last session ended {age} days ago")))
    }

    /// Remove worktrees Evyro made whose work is finished (the task is closed, its branch is
    /// merged, or its last session ended long enough ago), and delete their merged branches.
    /// Only clean worktrees with no unmerged or unpushed commits go; anything else is kept and
    /// says why. Worktrees Evyro did not make and the main checkout are never touched.
    pub fn prune_worktrees(&self, p: &Project, opts: PruneOptions) -> Result<Vec<Pruned>> {
        let root = p.root();
        git::prune_worktree_records(&root);
        let main = p
            .main_branch
            .clone()
            .filter(|b| git::branch_exists(&root, b))
            .unwrap_or_else(|| "HEAD".to_owned());
        let registered = git::worktrees(&root)?;
        let tasks: Vec<Task> = self
            .list_tasks(&p.id, None, true)?
            .into_iter()
            .filter(|t| t.worktree.is_some() && !comparison::is_candidate(t))
            .collect();
        let mut seen: Vec<PathBuf> = Vec::new();
        let mut out = Vec::new();
        for task in &tasks {
            let wt = task.worktree.clone().unwrap_or_default();
            let path = PathBuf::from(&wt);
            if seen.iter().any(|s| same_dir(s, &path)) {
                continue;
            }
            seen.push(path.clone());
            // Tasks that continue in the same worktree (a follow-up in its parent's).
            let sharing: Vec<&Task> = tasks
                .iter()
                .filter(|t| {
                    t.worktree
                        .as_deref()
                        .is_some_and(|w| same_dir(Path::new(w), &path))
                })
                .collect();
            let mut item = Pruned {
                task: sharing
                    .iter()
                    .map(|t| t.key.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                path: wt.clone(),
                branch: task.branch.clone(),
                removed: false,
                reason: String::new(),
                bytes: 0,
            };
            let unfinished = sharing.iter().find_map(|t| {
                match self.finished_why(p, &main, t, &path, opts.idle_days) {
                    Ok(Some(_)) => None,
                    Ok(None) => Some(Ok(t.key.clone())),
                    Err(e) => Some(Err(e)),
                }
            });
            let is_registered = registered
                .iter()
                .any(|w| same_dir(Path::new(&w.path), &path));
            if !path.is_dir() {
                item.reason = "already gone".into();
            } else if same_dir(&path, &root) {
                item.reason = "kept: it is the main checkout".into();
            } else if !is_registered || !Self::evyro_made(&path, &sharing) {
                item.reason = "kept: Evyro did not create it".into();
            } else if let Some(key) = unfinished.transpose()? {
                item.reason = format!("kept: {key} is not finished");
            } else if self.live_in(p, &path)? {
                item.reason = "kept: an AI is still working in it".into();
            } else if git::dirty_paths(&path).map_or(true, |d| !d.is_empty()) {
                item.reason = "kept: it has uncommitted changes".into();
            } else if task
                .branch
                .as_deref()
                .is_some_and(|b| git::commits_ahead(&root, &main, b) > 0)
            {
                item.reason =
                    format!("kept: its branch has commits that are not merged into {main}");
            } else if task
                .branch
                .as_deref()
                .is_some_and(|b| git::commits_unpushed(&root, b) > 0)
            {
                item.reason = "kept: its branch has commits that are not pushed".into();
            } else {
                let why = sharing
                    .iter()
                    .find_map(|t| {
                        self.finished_why(p, &main, t, &path, opts.idle_days)
                            .ok()
                            .flatten()
                    })
                    .unwrap_or_default();
                item.bytes = util::dir_size(&path);
                if opts.dry_run {
                    item.reason = format!("would remove: {why}");
                } else {
                    git::remove_worktree(&root, &path, false)?;
                    if let Some(b) = task.branch.as_deref() {
                        git::delete_merged_branch(&root, b);
                    }
                    item.removed = true;
                    item.reason = format!("removed: {why}");
                }
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
        let dry = f.store.prune_worktrees(&f.project, opts(true)).unwrap();
        assert_eq!(worktree_count(&f), before, "a dry run removes nothing");
        assert!(dry
            .iter()
            .any(|i| i.task == clean.key && i.reason.starts_with("would remove") && i.bytes > 0));
        let done = f.store.prune_worktrees(&f.project, opts(false)).unwrap();
        let by = |k: &str| done.iter().find(|i| i.task == k).unwrap();
        assert!(by(&clean.key).removed);
        assert!(!Path::new(clean.worktree.as_deref().unwrap()).exists());
        assert!(!by(&dirty.key).removed && by(&dirty.key).reason.contains("uncommitted"));
        assert!(!by(&unmerged.key).removed && by(&unmerged.key).reason.contains("not merged"));
        assert!(!by(&open.key).removed && by(&open.key).reason.contains("not finished"));
        assert!(Path::new(open.worktree.as_deref().unwrap()).is_dir());
    }

    fn opts(dry_run: bool) -> PruneOptions {
        PruneOptions {
            dry_run,
            idle_days: Some(7),
        }
    }

    fn close(f: &Fix, t: &Task) {
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

    /// A task with its own Evyro worktree holding one commit.
    fn with_commit(f: &Fix, title: &str) -> (Task, PathBuf) {
        let t = task(f, title, &[], None);
        let t = f
            .store
            .add_task_worktree(&f.project, &t, None, None)
            .unwrap();
        let wt = PathBuf::from(t.worktree.clone().unwrap());
        std::fs::write(wt.join(format!("{title}.rs")), "work\n").unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-qm", title]);
        (t, wt)
    }

    #[test]
    fn a_merged_branch_counts_as_finished_even_when_the_task_is_open() {
        let f = fixture(true);
        let (t, wt) = with_commit(&f, "merged");
        let fresh = task(&f, "fresh", &[], None);
        let fresh = f
            .store
            .add_task_worktree(&f.project, &fresh, None, None)
            .unwrap();
        git(
            &f.root,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "merge",
                t.branch.as_deref().unwrap(),
            ],
        );
        let done = f.store.prune_worktrees(&f.project, opts(false)).unwrap();
        let by = |k: &str| done.iter().find(|i| i.task == k).unwrap();
        assert!(by(&t.key).removed, "{:?}", by(&t.key));
        assert!(by(&t.key).reason.contains("merged"));
        assert!(by(&t.key).bytes > 0);
        assert!(!wt.exists());
        assert!(!git::branch_exists(&f.root, t.branch.as_deref().unwrap()));
        // A branch that only started from main has nothing merged yet.
        assert!(!by(&fresh.key).removed && by(&fresh.key).reason.contains("not finished"));
    }

    #[test]
    fn unpushed_commits_keep_a_worktree() {
        let f = fixture(true);
        let remote = f._dir.path().join("remote.git");
        git(
            f._dir.path(),
            &["init", "-q", "--bare", remote.to_str().unwrap()],
        );
        git(
            &f.root,
            &["remote", "add", "origin", remote.to_str().unwrap()],
        );
        let (t, wt) = with_commit(&f, "pushed");
        git(
            &wt,
            &["push", "-q", "-u", "origin", t.branch.as_deref().unwrap()],
        );
        std::fs::write(wt.join("later.rs"), "later\n").unwrap();
        git(&wt, &["add", "-A"]);
        git(&wt, &["commit", "-qm", "not pushed"]);
        git(
            &f.root,
            &[
                "merge",
                "-q",
                "--no-ff",
                "-m",
                "merge",
                t.branch.as_deref().unwrap(),
            ],
        );
        close(&f, &t);
        let done = f.store.prune_worktrees(&f.project, opts(false)).unwrap();
        assert!(
            !done[0].removed && done[0].reason.contains("not pushed"),
            "{:?}",
            done[0]
        );
        assert!(wt.is_dir());
    }

    #[test]
    fn worktrees_evyro_did_not_make_and_the_main_checkout_are_never_removed() {
        let f = fixture(true);
        let theirs = f._dir.path().join("theirs");
        git(
            &f.root,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "mine",
                theirs.to_str().unwrap(),
            ],
        );
        let a = task(&f, "Theirs", &[], None);
        let a = f.store.point_task_at(&a, "mine", &theirs).unwrap();
        let b = task(&f, "Main", &[], None);
        let b = f.store.point_task_at(&b, "main", &f.root).unwrap();
        close(&f, &a);
        close(&f, &b);
        let done = f.store.prune_worktrees(&f.project, opts(false)).unwrap();
        let by = |k: &str| done.iter().find(|i| i.task == k).unwrap();
        assert!(!by(&a.key).removed && by(&a.key).reason.contains("did not create"));
        assert!(!by(&b.key).removed && by(&b.key).reason.contains("main checkout"));
        assert!(theirs.is_dir() && f.root.is_dir());
    }

    #[test]
    fn a_worktree_idle_past_the_set_age_counts_as_finished() {
        let f = fixture(true);
        let t = task(&f, "Idle", &[], None);
        let t = f
            .store
            .add_task_worktree(&f.project, &t, None, None)
            .unwrap();
        let s = work_on(&f, &t, "codex");
        f.store
            .set_session_status(&s.id, crate::sessions::SessionStatus::Completed, None)
            .unwrap();
        let keep = f.store.prune_worktrees(&f.project, opts(true)).unwrap();
        assert!(keep[0].reason.contains("not finished"), "{:?}", keep[0]);
        f.store
            .conn
            .execute(
                "UPDATE sessions SET ended_at = '2020-01-01T00:00:00.000Z', last_event_at = NULL WHERE id = ?1",
                [&s.id],
            )
            .unwrap();
        let dry = f.store.prune_worktrees(&f.project, opts(true)).unwrap();
        assert!(dry[0].reason.contains("last session ended"), "{:?}", dry[0]);
        assert!(
            Path::new(t.worktree.as_deref().unwrap()).is_dir(),
            "a dry run removes nothing"
        );
        let off = PruneOptions {
            dry_run: true,
            idle_days: None,
        };
        assert!(f.store.prune_worktrees(&f.project, off).unwrap()[0]
            .reason
            .contains("not finished"));
    }
}
