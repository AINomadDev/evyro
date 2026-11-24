//! Two isolated implementations of the same task. Work stays in its own Git checkout.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::targets::TargetKind;
use crate::tasks::{NewTask, Task, TaskStatus};
use crate::{git, redact, targets, util};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const BRANCH_PREFIX: &str = "feature/compare-";
const CREATED: &str = "comparison.created";
const PREFERRED: &str = "comparison.preferred";

#[derive(Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub task_id: String,
    pub key: String,
    pub target: String,
    pub worktree: String,
    pub branch: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Comparison {
    pub id: String,
    pub base: String,
    pub candidates: [Candidate; 2],
    pub preferred: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComparisonStagePlan {
    pub candidate: String,
    pub candidate_commit: String,
    pub destination: String,
    pub destination_branch: Option<String>,
    pub destination_commit: String,
    pub files: Vec<String>,
}

pub(crate) fn is_candidate(task: &Task) -> bool {
    task.parent_id.is_some()
        && task
            .branch
            .as_deref()
            .and_then(|branch| branch.strip_prefix(BRANCH_PREFIX))
            .and_then(|suffix| suffix.rsplit_once('-'))
            .is_some_and(|(id, slot)| {
                id.len() == 32
                    && id.bytes().all(|b| b.is_ascii_hexdigit())
                    && matches!(slot, "a" | "b")
            })
}

pub(crate) fn candidate_root(project: &Path, task: &Task) -> Result<PathBuf> {
    let invalid = || {
        Error::Conflict(format!(
            "{} needs its original comparison worktree and branch",
            task.key
        ))
    };
    let path = task
        .worktree
        .as_deref()
        .map(Path::new)
        .ok_or_else(invalid)?;
    let root = util::canonical(path).map_err(|_| invalid())?;
    if root == util::canonical(project)?
        || util::canonical(&git::toplevel(&root).map_err(|_| invalid())?)? != root
        || util::canonical(&git::main_worktree(&root).map_err(|_| invalid())?)?
            != util::canonical(&git::main_worktree(project)?)?
        || git::current_branch(&root)?.as_deref() != task.branch.as_deref()
    {
        return Err(invalid());
    }
    Ok(root)
}

impl Store {
    pub fn create_comparison(
        &self,
        project_id: &str,
        task: &str,
        names: [&str; 2],
    ) -> Result<Comparison> {
        let project = self.get_project(project_id)?;
        let parent = self.resolve_task(project_id, task)?;
        if is_candidate(&parent)
            || parent.status.is_terminal()
            || parent.status == TaskStatus::Blocked
            || !self.open_blockers(&parent.id)?.is_empty()
        {
            return Err(Error::Invalid(
                "choose an unblocked, open task to compare".into(),
            ));
        }
        let resolve = |name| -> Result<String> {
            let target = targets::resolve_target(&self.paths, name)?;
            if !matches!(target.kind, TargetKind::Harness | TargetKind::Api) {
                return Err(Error::Invalid(
                    "comparison targets must be terminal coding agents".into(),
                ));
            }
            Ok(target.name)
        };
        let targets = [resolve(names[0])?, resolve(names[1])?];
        if targets[0].eq_ignore_ascii_case(&targets[1]) {
            return Err(Error::Invalid("choose two different agent targets".into()));
        }
        let root = project.root();
        let source = parent
            .worktree
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| root.clone());
        if util::canonical(&git::main_worktree(&source)?)?
            != util::canonical(&git::main_worktree(&root)?)?
        {
            return Err(Error::Invalid(
                "the task worktree belongs to a different repository".into(),
            ));
        }
        let base = git::head_sha(&source)?.ok_or_else(|| {
            Error::Invalid("make an initial commit before comparing agents".into())
        })?;
        let id = util::new_id("cmp");
        let dir = self.paths.data_dir.join("comparisons").join(&id);
        std::fs::create_dir_all(&dir)?;
        let mut created = Vec::new();
        let prepare = (|| -> Result<Comparison> {
            for slot in ["a", "b"] {
                let path = dir.join(slot);
                let branch = format!("{BRANCH_PREFIX}{}-{slot}", &id[4..]);
                let result = git::add_worktree_at(&root, &path, &branch, &base);
                // Git can create the checkout before a post-checkout hook fails.
                if result.is_ok() || path.is_dir() {
                    created.push((path, branch));
                }
                result?;
            }
            let tx = self.conn.unchecked_transaction()?;
            let make = |index: usize| -> Result<Candidate> {
                let (path, branch) = &created[index];
                let worktree = util::canonical(path)?.to_string_lossy().into_owned();
                let child = self.insert_task(
                    project_id,
                    NewTask {
                        title: format!("{} comparison for {}", targets[index], parent.key),
                        description: format!(
                            "Compare an implementation for {}: {}.\n\n{}",
                            parent.key, parent.title, parent.description
                        ),
                        kind: Some("subtask".into()),
                        priority: Some(parent.priority),
                        parent: Some(parent.id.clone()),
                        acceptance: parent.acceptance.clone(),
                        constraints: parent.constraints.clone(),
                        files: parent.files.clone(),
                        ..Default::default()
                    },
                    Some((branch, &worktree)),
                )?;
                Ok(Candidate {
                    task_id: child.id,
                    key: child.key,
                    target: targets[index].clone(),
                    worktree,
                    branch: branch.clone(),
                })
            };
            let run = Comparison {
                id,
                base: base.clone(),
                candidates: [make(0)?, make(1)?],
                preferred: None,
            };
            self.emit(
                Some(project_id),
                Some(&parent.id),
                None,
                CREATED,
                serde_json::to_value(&run)?,
            )?;
            tx.commit()?;
            Ok(run)
        })();
        if let Err(error) = prepare {
            let leftovers: Vec<_> = created
                .iter()
                .rev()
                .filter_map(|(path, branch)| {
                    git::remove_worktree(&root, path, false)
                        .and_then(|_| git::drop_unchanged_branch(&root, branch, &base))
                        .err()
                        .map(|_| path.display().to_string())
                })
                .collect();
            let _ = std::fs::remove_dir(&dir);
            if leftovers.is_empty() {
                return Err(error);
            }
            return Err(Error::Conflict(format!(
                "{error}. Check remaining worktrees or branches in {}",
                leftovers.join(", ")
            )));
        }
        prepare
    }

    pub fn comparisons(&self, project_id: &str, task: &str) -> Result<Vec<Comparison>> {
        let parent = self.resolve_task(project_id, task)?;
        let mut stmt = self.conn.prepare("SELECT kind, payload FROM events WHERE project_id = ?1 AND task_id = ?2 AND kind IN (?3, ?4) ORDER BY seq")?;
        let rows = stmt.query_map(
            rusqlite::params![project_id, parent.id, CREATED, PREFERRED],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?;
        let mut runs: Vec<Comparison> = Vec::new();
        for row in rows {
            let (kind, payload) = row?;
            if kind == CREATED {
                runs.push(serde_json::from_str(&payload)?);
            } else {
                let preference: Value = serde_json::from_str(&payload)?;
                if let Some(run) = runs
                    .iter_mut()
                    .rev()
                    .find(|r| preference["comparison"] == r.id)
                {
                    run.preferred = preference["candidate"].as_str().map(str::to_owned);
                }
            }
        }
        runs.reverse();
        Ok(runs)
    }

    fn comparison(&self, project: &str, task: &str, id: &str) -> Result<Comparison> {
        self.comparisons(project, task)?
            .into_iter()
            .find(|run| run.id == id)
            .ok_or_else(|| Error::NotFound(format!("comparison {id}")))
    }

    pub fn prefer_comparison(
        &self,
        project: &str,
        task: &str,
        id: &str,
        candidate: &str,
    ) -> Result<()> {
        let run = self.comparison(project, task, id)?;
        if !run.candidates.iter().any(|c| c.key == candidate) {
            return Err(Error::Invalid(
                "choose a candidate from this comparison".into(),
            ));
        }
        let parent = self.resolve_task(project, task)?;
        self.emit(
            Some(project),
            Some(&parent.id),
            None,
            PREFERRED,
            json!({"comparison": id, "candidate": candidate}),
        )?;
        Ok(())
    }

    fn comparison_stage_patch(
        &self,
        project: &str,
        task: &str,
        id: &str,
    ) -> Result<(Task, ComparisonStagePlan, std::fs::File)> {
        let project_root = self.get_project(project)?.root();
        let parent = self.resolve_task(project, task)?;
        let run = self.comparison(project, task, id)?;
        let winner = run
            .candidates
            .iter()
            .find(|c| Some(&c.key) == run.preferred.as_ref())
            .ok_or_else(|| Error::Invalid("Choose a preferred result first".into()))?;
        let child = self.get_task(&winner.task_id)?;
        if child.worktree.as_deref() != Some(&winner.worktree)
            || child.branch.as_deref() != Some(&winner.branch)
        {
            return Err(Error::Conflict(format!(
                "{} is no longer assigned to its comparison worktree",
                winner.key
            )));
        }
        let source = candidate_root(&project_root, &child)?;
        let destination = util::canonical(
            parent
                .worktree
                .as_deref()
                .map(Path::new)
                .unwrap_or(&project_root),
        )
        .map_err(|_| Error::Conflict("The task checkout is unavailable".into()))?;
        if run
            .candidates
            .iter()
            .any(|c| Path::new(&c.worktree) == destination)
            || util::canonical(&git::toplevel(&destination)?)? != destination
            || util::canonical(&git::main_worktree(&destination)?)?
                != util::canonical(&git::main_worktree(&project_root)?)?
        {
            return Err(Error::Conflict(
                "The task checkout is outside the original repository or is a candidate worktree"
                    .into(),
            ));
        }
        for session in self.list_sessions(project, true)? {
            let path = session
                .worktree
                .as_deref()
                .map(Path::new)
                .unwrap_or(&project_root);
            if util::canonical(path).is_ok_and(|root| root == source || root == destination) {
                return Err(Error::Conflict(
                    "End active sessions in the candidate and destination checkouts before staging"
                        .into(),
                ));
            }
        }
        if !git::is_clean(&source)? {
            return Err(Error::Conflict(format!(
                "Commit or set aside changes in {} before staging",
                winner.key
            )));
        }
        if !git::is_clean(&destination)? {
            return Err(Error::Conflict(
                "Commit or set aside changes in the destination checkout before staging".into(),
            ));
        }
        let commit = |root: &Path| -> Result<String> {
            let head = git::head_sha(root)?
                .ok_or_else(|| Error::Conflict("The checkout has no current commit".into()))?;
            if !git::is_ancestor(root, &run.base, &head) {
                return Err(Error::Conflict(
                    "Both checkouts must include the shared comparison base".into(),
                ));
            }
            Ok(head)
        };
        let candidate_commit = commit(&source)?;
        let destination_commit = commit(&destination)?;
        let (files, patch) = git::commit_patch(&source, &run.base, &candidate_commit)?;
        if files.is_empty() {
            return Err(Error::Conflict(
                "The preferred candidate has no committed changes since the shared base".into(),
            ));
        }
        let plan = ComparisonStagePlan {
            candidate: winner.key.clone(),
            candidate_commit,
            destination_branch: git::current_branch(&destination)?,
            destination: destination.into_os_string().into_string().map_err(|_| {
                Error::Invalid("The task checkout path must be valid Unicode".into())
            })?,
            destination_commit,
            files,
        };
        Ok((parent, plan, patch))
    }

    pub fn preview_comparison_stage(
        &self,
        project: &str,
        task: &str,
        id: &str,
    ) -> Result<ComparisonStagePlan> {
        let (_, plan, mut patch) = self.comparison_stage_patch(project, task, id)?;
        git::apply_to_index(Path::new(&plan.destination), &mut patch, true)?;
        Ok(plan)
    }

    pub fn stage_comparison(
        &self,
        project: &str,
        task: &str,
        id: &str,
        expected: &ComparisonStagePlan,
    ) -> Result<()> {
        // Block preference and session changes from other Evyro processes during the transfer.
        let tx = rusqlite::Transaction::new_unchecked(
            &self.conn,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let (parent, plan, mut patch) = self.comparison_stage_patch(project, task, id)?;
        if &plan != expected {
            return Err(Error::Conflict(
                "The comparison or destination changed. Preview staging again".into(),
            ));
        }
        self.emit(
            Some(project),
            Some(&parent.id),
            None,
            "comparison.staged",
            json!({"comparison": id, "plan": plan}),
        )?;
        // Git rechecks the destination index and working tree before writing.
        git::apply_to_index(Path::new(&plan.destination), &mut patch, false)?;
        tx.commit().map_err(|e| {
            Error::Conflict(format!(
                "Changes are staged in {}. Evyro could not record the transfer: {e}",
                plan.destination
            ))
        })?;
        Ok(())
    }

    pub fn comparison_results(&self, project: &str, task: &str, id: &str) -> Result<Value> {
        let run = self.comparison(project, task, id)?;
        let project_root = self.get_project(project)?.root();
        let details = run.candidates.iter().map(|c| {
            let read = || -> Result<Value> {
                let child = self.get_task(&c.task_id)?;
                if child.worktree.as_deref() != Some(&c.worktree) || child.branch.as_deref() != Some(&c.branch) {
                    return Err(Error::Conflict(format!("{} is no longer assigned to its comparison worktree", c.key)));
                }
                let root = candidate_root(&project_root, &child)?;
                let snapshot = git::snapshot(&root)?;
                let mut diff = git::diff_text_from(&root, &run.base, 100_000)?;
                if !diff.contains("… diff truncated …") {
                    let new: Vec<_> = snapshot.files.iter().filter(|f| f.status == "??"
                        && root.join(&f.path).symlink_metadata().is_ok_and(|m| m.is_file())).take(64).cloned().collect();
                    diff.push_str(&git::untracked_diff(&root, &new, 16_000));
                }
                if diff.len() > 100_000 {
                    let end = diff.char_indices().map(|(i, _)| i).take_while(|&i| i <= 100_000).last().unwrap_or(0);
                    diff.truncate(end);
                    diff.push_str("\n… diff truncated …\n");
                }
                let (diff, findings) = redact::redact(&diff);
                let receipt = self.list_verifications(project, Some(&child.id))?.pop();
                let current = receipt.as_ref().and_then(|r| r.fingerprint.as_deref()).is_some_and(|fp| fp == snapshot.fingerprint());
                Ok(json!({"status": child.status, "diff": diff, "redactions": findings.len(), "verification": receipt, "verification_current": current, "running": self.active_session_for_task(&child.id)?.is_some()}))
            };
            read().unwrap_or_else(|e| json!({"error": e.to_string()}))
        }).collect::<Vec<_>>();
        Ok(json!({"comparison": run, "results": details}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::GlobalPaths;
    use std::process::Command;

    fn git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap()
    }

    #[test]
    fn candidates_share_a_base_and_keep_work_and_choices_separate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repo");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "-b", "main"]);
        git(&root, &["config", "user.name", "Test"]);
        git(&root, &["config", "user.email", "test@example.com"]);
        std::fs::write(root.join("app.txt"), "base\n").unwrap();
        git(&root, &["add", "app.txt"]);
        git(&root, &["commit", "-qm", "base"]);
        let base = git(&root, &["rev-parse", "HEAD"]).trim().to_owned();
        std::fs::write(root.join("app.txt"), "local edit\n").unwrap();
        let paths = GlobalPaths {
            data_dir: tmp.path().join("data"),
            config_dir: tmp.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths.clone()).unwrap();
        let project = store.register_project(&root, None).unwrap().project;
        let parent = store
            .create_task(
                &project.id,
                NewTask {
                    title: "Improve the parser".into(),
                    acceptance: vec!["Preserve existing inputs".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(store
            .create_comparison(&project.id, &parent.key, ["codex", "codex"])
            .is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let hook = root.join(".git/hooks/post-checkout");
            std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(store
                .create_comparison(&project.id, &parent.key, ["codex", "claude-code"])
                .is_err());
            std::fs::remove_file(hook).unwrap();
            assert_eq!(super::git::worktrees(&root).unwrap().len(), 1);
            assert!(git(&root, &["branch", "--list", "feature/compare-*"]).is_empty());
        }

        // A database failure after the first candidate insert must roll back both tasks and worktrees.
        store.conn.execute_batch("CREATE TRIGGER reject_candidate BEFORE INSERT ON tasks WHEN new.title LIKE 'claude-code comparison%' BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;").unwrap();
        assert!(store
            .create_comparison(&project.id, &parent.key, ["codex", "claude-code"])
            .is_err());
        assert_eq!(store.list_tasks(&project.id, None, true).unwrap().len(), 1);
        assert_eq!(super::git::worktrees(&root).unwrap().len(), 1);
        assert!(git(&root, &["branch", "--list", "feature/compare-*"]).is_empty());
        store
            .conn
            .execute_batch("DROP TRIGGER reject_candidate")
            .unwrap();

        let run = store
            .create_comparison(&project.id, &parent.key, ["codex", "claude-code"])
            .unwrap();
        assert_eq!(run.base, base);
        let [first, second] = &run.candidates;
        let a = Path::new(&first.worktree);
        let b = Path::new(&second.worktree);
        for path in [a, b] {
            assert_eq!(git(path, &["rev-parse", "HEAD"]).trim(), base);
        }
        assert_ne!(a, b);
        let child = store.get_task(&first.task_id).unwrap();
        assert_eq!(child.parent_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.acceptance, parent.acceptance);
        assert_eq!(store.task_root(&child).unwrap(), a);
        let claim = store
            .claim_for_target(&project, "codex", Some(&first.key), None, false, None)
            .unwrap();
        let crate::handover::Claim::Started { session, .. } = claim else {
            panic!("candidate should start")
        };
        assert_eq!(session.worktree.as_deref(), Some(first.worktree.as_str()));
        std::fs::write(a.join("app.txt"), "first implementation\n").unwrap();
        git(a, &["commit", "-qam", "first"]);
        store
            .run_verification(
                &project.id,
                Some(&child.id),
                Some(&session.id),
                &["git".into(), "diff".into(), "--quiet".into()],
                a,
            )
            .unwrap();
        std::fs::write(b.join("new.txt"), "second implementation\n").unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join("app.txt")).unwrap(),
            "local edit\n"
        );
        assert_eq!(
            std::fs::read_to_string(b.join("app.txt")).unwrap(),
            "base\n"
        );
        let view = store
            .comparison_results(&project.id, &parent.key, &run.id)
            .unwrap();
        assert_eq!(view["results"][0]["verification"]["passed"], true);
        assert_eq!(view["results"][0]["verification_current"], true);
        assert!(view["results"][0]["diff"]
            .as_str()
            .unwrap()
            .contains("+first implementation"));
        assert!(view["results"][1]["diff"]
            .as_str()
            .unwrap()
            .contains("+second implementation"));
        std::fs::write(a.join("app.txt"), "edited after verification\n").unwrap();
        let stale = store
            .comparison_results(&project.id, &parent.key, &run.id)
            .unwrap();
        assert_eq!(stale["results"][0]["verification_current"], false);
        assert!(store
            .prefer_comparison(&project.id, &parent.key, &run.id, &parent.key)
            .is_err());
        store
            .prefer_comparison(&project.id, &parent.key, &run.id, &first.key)
            .unwrap();
        assert!(store
            .preview_comparison_stage(&project.id, &parent.key, &run.id)
            .unwrap_err()
            .to_string()
            .contains("End active sessions"));
        store
            .prefer_comparison(&project.id, &parent.key, &run.id, &second.key)
            .unwrap();
        drop(store);
        let reopened = Store::open_at(&paths.db_path(), paths).unwrap();
        assert_eq!(
            reopened.comparisons(&project.id, &parent.key).unwrap()[0]
                .preferred
                .as_deref(),
            Some(second.key.as_str())
        );
        assert_eq!(git(&root, &["rev-parse", "HEAD"]).trim(), base);
        let moved = tmp.path().join("moved");
        git(
            &root,
            &["worktree", "move", &first.worktree, moved.to_str().unwrap()],
        );
        assert!(reopened.task_root(&child).is_err());
        let view = reopened
            .comparison_results(&project.id, &parent.key, &run.id)
            .unwrap();
        assert!(view["results"][0]["error"].is_string());
        assert!(view["results"][1]["diff"]
            .as_str()
            .unwrap()
            .contains("+second implementation"));

        let preview = || reopened.preview_comparison_stage(&project.id, &parent.key, &run.id);
        let stage = |plan| reopened.stage_comparison(&project.id, &parent.key, &run.id, plan);
        assert!(preview().unwrap_err().to_string().contains(&second.key));
        let binary = [0, 255, 1, 128];
        std::fs::write(b.join("image.bin"), binary).unwrap();
        std::fs::write(b.join("app.txt"), "winner\n").unwrap();
        git(b, &["add", "."]);
        git(b, &["commit", "-qm", "second"]);
        assert!(preview()
            .unwrap_err()
            .to_string()
            .contains("destination checkout"));
        assert_eq!(
            git(&root, &["status", "--porcelain", "--", "app.txt"]),
            " M app.txt\n"
        );
        git(&root, &["restore", "app.txt"]);
        let plan = preview().unwrap();
        assert_eq!(
            plan.destination,
            util::canonical(&root).unwrap().to_str().unwrap()
        );
        assert_eq!(plan.destination_branch.as_deref(), Some("main"));
        assert_eq!(plan.files, ["app.txt", "image.bin", "new.txt"]);
        assert!(super::git::is_clean(&root).unwrap());

        git(
            b,
            &["commit", "--allow-empty", "-qm", "new candidate commit"],
        );
        assert!(stage(&plan).unwrap_err().to_string().contains("changed"));
        let plan = preview().unwrap();
        std::fs::write(root.join("app.txt"), "conflicting main commit\n").unwrap();
        git(&root, &["commit", "-qam", "conflict"]);
        assert!(stage(&plan).unwrap_err().to_string().contains("changed"));
        assert!(preview().unwrap_err().to_string().contains("Cannot stage"));
        assert!(super::git::is_clean(&root).unwrap());
        assert!(!root.join("image.bin").exists());

        git(&root, &["revert", "--no-edit", "HEAD"]);
        let plan = preview().unwrap();
        assert_ne!(plan.destination_commit, base);
        stage(&plan).unwrap();
        assert_eq!(
            git(&root, &["rev-parse", "HEAD"]).trim(),
            plan.destination_commit
        );
        assert_eq!(git(&root, &["branch", "--show-current"]).trim(), "main");
        assert_eq!(
            git(&root, &["diff", "--cached", "--name-only"]),
            "app.txt\nimage.bin\nnew.txt\n"
        );
        assert_eq!(std::fs::read(root.join("image.bin")).unwrap(), binary);
        assert!(git(&root, &["diff", "--name-only"]).is_empty());
        assert!(git(b, &["status", "--porcelain"]).is_empty());
        assert!(stage(&plan).is_err());
    }
}
