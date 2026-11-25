//! A tiny sample project to try Evyro on, and a plain read of how far someone got with it.
//!
//! The sample lives inside Evyro's own data folder, never in the user's folders. It has two
//! small tasks on different files, so one AI can take each, and a test command that works on any
//! machine with Git (`git diff --check`). It is real: tasks, checkpoints, receipts and handoffs
//! behave exactly as in any project, and it can be archived like one.

use crate::db::Store;
use crate::error::{Error, Result};
use crate::init::{init_project, render_project_file, InitOptions};
use crate::projects::{Commands, Project, ProjectFile};
use crate::tasks::{NewTask, TaskStatus};
use crate::util;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

pub const SAMPLE_NAME: &str = "Evyro sample";

#[derive(Debug, Clone, Serialize)]
pub struct SampleReport {
    pub project: Project,
    /// False when the sample already existed and was only opened again.
    pub created: bool,
    pub tasks: Vec<String>,
}

fn git(dir: &Path, args: &[&str]) -> Result<()> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

impl Store {
    /// Create (or reopen) the sample project and register it.
    pub fn create_sample_project(&self) -> Result<SampleReport> {
        let dir = self.paths.data_dir.join("samples").join("evyro-sample");
        if dir.join(".git").exists() {
            let root = util::canonical(&dir)?.to_string_lossy().into_owned();
            if let Some(p) = self.find_project_by_root(&root)? {
                let tasks = self
                    .list_tasks(&p.id, None, true)?
                    .into_iter()
                    .map(|t| t.key)
                    .collect();
                return Ok(SampleReport {
                    project: p,
                    created: false,
                    tasks,
                });
            }
        }
        std::fs::create_dir_all(dir.join(".evyro"))?;
        std::fs::write(
            dir.join("README.md"),
            "# Evyro sample\n\nTODO: say what this project is in two sentences.\n",
        )?;
        std::fs::write(dir.join("notes.txt"), "Notes\n")?;
        // The test command works wherever Git does, so a first receipt needs no setup.
        let pf = ProjectFile {
            schema_version: 1,
            id: util::new_id("prj"),
            name: SAMPLE_NAME.into(),
            commands: Commands {
                test: vec![vec!["git".into(), "diff".into(), "--check".into()]],
                ..Default::default()
            },
        };
        util::write_atomic(
            &dir.join(".evyro").join("project.toml"),
            render_project_file(&pf).as_bytes(),
        )?;
        if !dir.join(".git").exists() {
            git(&dir, &["init", "-q", "-b", "main"])?;
        }
        git(&dir, &["config", "user.email", "sample@evyro.invalid"])?;
        git(&dir, &["config", "user.name", "Evyro sample"])?;
        git(&dir, &["add", "-A"])?;
        git(&dir, &["commit", "-qm", "Start the sample project"])?;
        let report = init_project(
            self,
            &dir,
            &InitOptions {
                name: Some(SAMPLE_NAME.into()),
                append_agent_files: false,
                agent_files: true,
            },
        )?;
        let project = self.get_project(&report.project.id)?;
        let mut tasks = Vec::new();
        for (title, files, acceptance) in [
            (
                "Describe the sample in README.md",
                vec!["README.md"],
                vec![
                    "README.md says what this project is in two sentences",
                    "The tests pass (`git diff --check`)",
                ],
            ),
            (
                "Add three notes to notes.txt",
                vec!["notes.txt"],
                vec![
                    "notes.txt has three notes, one per line",
                    "The tests pass (`git diff --check`)",
                ],
            ),
        ] {
            let t = self.create_task(
                &project.id,
                NewTask {
                    title: title.into(),
                    kind: Some("task".into()),
                    files: files.into_iter().map(String::from).collect(),
                    acceptance: acceptance.into_iter().map(String::from).collect(),
                    status: Some(TaskStatus::Ready),
                    ..Default::default()
                },
            )?;
            tasks.push(t.key);
        }
        Ok(SampleReport {
            project,
            created: true,
            tasks,
        })
    }

    /// How far a project got through the first steps, from what Evyro actually recorded.
    pub fn tour_status(&self, project_id: &str) -> Result<TourStatus> {
        Ok(TourStatus {
            started: !self.list_sessions(project_id, false)?.is_empty(),
            checkpoint: self.latest_checkpoint(project_id, None)?.is_some(),
            verified: !self.list_verifications(project_id, None)?.is_empty(),
            handoff: !self.list_handoffs(project_id, 1)?.is_empty(),
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TourStatus {
    /// An AI has worked on a task here.
    pub started: bool,
    pub checkpoint: bool,
    /// A test run was recorded as a receipt.
    pub verified: bool,
    pub handoff: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        (dir, store)
    }

    #[test]
    fn the_sample_is_a_real_project_with_two_ready_tasks_and_a_test_command() {
        let (_d, store) = store();
        let r = store.create_sample_project().unwrap();
        assert!(r.created);
        assert_eq!(r.project.name, SAMPLE_NAME);
        assert_eq!(r.tasks, ["T-1", "T-2"]);
        assert_eq!(r.project.commands.test, [["git", "diff", "--check"]]);
        let root = r.project.root();
        assert!(
            root.starts_with(store.paths.data_dir.canonicalize().unwrap()),
            "{root:?}"
        );
        let t1 = store.resolve_task(&r.project.id, "T-1").unwrap();
        assert_eq!(t1.status, TaskStatus::Ready);
        assert_eq!(t1.acceptance.len(), 2);
        assert_eq!(t1.files, ["README.md"]);
        // The two tasks touch different files, so two AIs can take one each.
        assert_ne!(
            t1.files,
            store.resolve_task(&r.project.id, "T-2").unwrap().files
        );
        // The tests it names pass on a clean copy.
        let ok = Command::new("git")
            .args(["diff", "--check"])
            .current_dir(&root)
            .status()
            .unwrap();
        assert!(ok.success());
    }

    #[test]
    fn asking_again_reopens_the_same_project_and_adds_nothing() {
        let (_d, store) = store();
        let first = store.create_sample_project().unwrap();
        let again = store.create_sample_project().unwrap();
        assert!(!again.created);
        assert_eq!(again.project.id, first.project.id);
        assert_eq!(again.tasks, ["T-1", "T-2"]);
        assert_eq!(store.list_projects(true, None).unwrap().len(), 1);
    }

    #[test]
    fn the_tour_reads_what_was_really_recorded() {
        let (_d, store) = store();
        let p = store.create_sample_project().unwrap().project;
        let none = TourStatus {
            started: false,
            checkpoint: false,
            verified: false,
            handoff: false,
        };
        assert_eq!(store.tour_status(&p.id).unwrap(), none);
        let t = store.resolve_task(&p.id, "T-1").unwrap();
        store
            .start_session(
                &p.id,
                crate::sessions::NewSession {
                    task_id: Some(t.id.clone()),
                    harness: "claude-code".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let s = store.tour_status(&p.id).unwrap();
        assert!(
            s.started && !s.checkpoint && !s.verified && !s.handoff,
            "{s:?}"
        );
    }
}
