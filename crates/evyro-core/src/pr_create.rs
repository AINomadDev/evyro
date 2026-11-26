//! Open a pull request from a task. The body is written from Evyro's own records: the task's
//! title and description, its acceptance criteria with who marked each one and how, the latest
//! verification receipts, and the latest checkpoint. The whole body is redacted before it leaves
//! the machine. With `dry_run` nothing is pushed and GitHub is not called.

use crate::acceptance::Criterion;
use crate::checkpoints::Checkpoint;
use crate::db::Store;
use crate::error::{Error, Result};
use crate::pr_feedback::Gh;
use crate::tasks::Task;
use crate::verify::Receipt;
use crate::{git, redact};
use serde::Serialize;
use serde_json::Value;

/// Receipts listed in the body, newest first.
const MAX_RECEIPTS: usize = 5;

#[derive(Debug, Clone, Serialize)]
pub struct PrCreate {
    pub task: String,
    pub branch: String,
    pub base: String,
    pub title: String,
    pub body: String,
    pub draft: bool,
    /// True when nothing was pushed or sent to GitHub.
    pub dry_run: bool,
    /// The pull request's address; `None` on a dry run.
    pub url: Option<String>,
    /// False when the branch already had an open pull request, which is returned instead.
    pub created: bool,
}

fn short(sha: &str) -> &str {
    &sha[..sha.len().min(10)]
}

fn criterion_line(c: &Criterion) -> String {
    let (mark, state) = match (c.met, c.counts) {
        (true, true) => ("x", "met"),
        (true, false) => (" ", "reported by an AI, not confirmed"),
        (false, _) => (" ", "not met"),
    };
    let mut how = Vec::new();
    match c.provenance.as_deref() {
        Some("evyro_verified") => how.push("shown by an Evyro receipt".to_owned()),
        Some("human_recorded") => how.push("recorded by a person".to_owned()),
        Some("agent_reported") | None => {}
        Some(p) => how.push(p.to_owned()),
    }
    if let Some(r) = &c.receipt_id {
        how.push(format!("receipt `{r}`"));
    }
    if let Some(a) = c.actor.as_deref().filter(|a| !a.is_empty()) {
        how.push(format!("by {a}"));
    }
    if let Some(at) = &c.checked_at {
        how.push(at.clone());
    }
    let how = if how.is_empty() || !c.met {
        String::new()
    } else {
        format!(" ({})", how.join(", "))
    };
    format!("- [{mark}] {}. {} — **{state}**{how}\n", c.index, c.text)
}

fn receipt_line(r: &Receipt, head: Option<&str>) -> String {
    let result = if r.passed {
        "passed".to_owned()
    } else {
        match r.exit_code {
            Some(c) => format!("failed (exit {c})"),
            None => "failed".to_owned(),
        }
    };
    let commit = match r.commit_sha.as_deref() {
        Some(s) if Some(s) == head => format!(", at `{}` (the pushed commit)", short(s)),
        Some(s) => format!(", at `{}`", short(s)),
        None => String::new(),
    };
    format!(
        "- `{}` — **{result}**, {}, {:.1}s{commit}\n",
        r.command.join(" ").replace('`', "'"),
        r.started_at,
        r.duration_ms as f64 / 1000.0
    )
}

/// The pull request body for a task. `receipts` are in chronological order; `head` is the commit
/// being proposed, to say which receipts ran on it. The result is redacted.
pub fn build_body(
    task: &Task,
    criteria: &[Criterion],
    receipts: &[Receipt],
    checkpoint: Option<&Checkpoint>,
    head: Option<&str>,
) -> String {
    let mut out = format!("Evyro task **{}**: {}\n", task.key, task.title);
    if !task.description.trim().is_empty() {
        out.push_str(&format!("\n{}\n", task.description.trim()));
    }

    out.push_str("\n## Acceptance criteria\n\n");
    if criteria.is_empty() {
        out.push_str("The task lists no acceptance criteria.\n");
    } else {
        let met = criteria.iter().filter(|c| c.satisfied()).count();
        out.push_str(&format!("{met} of {} met.\n\n", criteria.len()));
        for c in criteria {
            out.push_str(&criterion_line(c));
        }
    }

    out.push_str("\n## Verification receipts\n\n");
    if receipts.is_empty() {
        out.push_str("No test run was recorded with `evyro verify` for this task.\n");
    } else {
        let shown = receipts.len().min(MAX_RECEIPTS);
        out.push_str(&format!(
            "Runs recorded by `evyro verify`, newest first ({shown} of {}):\n\n",
            receipts.len()
        ));
        for r in receipts.iter().rev().take(MAX_RECEIPTS) {
            out.push_str(&receipt_line(r, head));
        }
    }

    out.push_str("\n## Latest checkpoint\n\n");
    match checkpoint {
        None => out.push_str("No checkpoint was recorded for this task.\n"),
        Some(c) => {
            if !c.summary.trim().is_empty() {
                out.push_str(&format!("{}\n\n", c.summary.trim()));
            }
            if !c.completed.is_empty() {
                out.push_str(&format!("**Done:** {}\n\n", c.completed.join("; ")));
            }
            out.push_str(&format!("_Recorded {}_\n", c.created_at));
            if !c.next_action.trim().is_empty() {
                out.push_str(&format!("\n**Next step:** {}\n", c.next_action.trim()));
            }
            if !c.blockers.is_empty() {
                out.push_str(&format!("\n**Blockers:** {}\n", c.blockers.join("; ")));
            }
        }
    }

    out.push_str("\n---\nWritten by `evyro pr create` from the task's records.\n");
    redact::redact(&out).0
}

impl Store {
    /// Push the task's branch and open its pull request, or with `dry_run` only build the title
    /// and body. Refuses when `gh` is missing or not signed in.
    pub fn pr_create(
        &self,
        task: &Task,
        gh: &dyn Gh,
        base: Option<&str>,
        draft: bool,
        dry_run: bool,
    ) -> Result<PrCreate> {
        let root = self.task_root(task)?;
        let branch = match &task.branch {
            Some(b) => b.clone(),
            None => git::current_branch(&root)?.ok_or_else(|| {
                Error::Invalid("the checkout is not on a branch, so there is nothing to open a pull request from".into())
            })?,
        };
        let base = match base {
            Some(b) => b.to_owned(),
            None => git::default_branch(&root)?.unwrap_or_else(|| "main".into()),
        };
        if branch == base {
            return Err(Error::Invalid(format!(
                "{} is on {branch}, the base branch; start the work on its own branch first",
                task.key
            )));
        }
        let criteria = self.criteria(task)?;
        let receipts = self.list_verifications(&task.project_id, Some(&task.id))?;
        let checkpoint = self.latest_checkpoint(&task.project_id, Some(&task.id))?;
        let head = git::head_sha(&root)?;
        let title = redact::redact(&format!("{} ({})", task.title, task.key)).0;
        // Local paths say who and where you are; the body names them relative to the checkout.
        let mut body = build_body(
            task,
            &criteria,
            &receipts,
            checkpoint.as_ref(),
            head.as_deref(),
        )
        .replace(&*root.to_string_lossy(), ".");
        if let Some(home) = std::env::var_os("HOME").filter(|h| h.len() > 1) {
            body = body.replace(&*home.to_string_lossy(), "~");
        }
        let mut out = PrCreate {
            task: task.key.clone(),
            branch: branch.clone(),
            base: base.clone(),
            title,
            body,
            draft,
            dry_run,
            url: None,
            created: false,
        };
        if dry_run {
            return Ok(out);
        }

        gh.run(&root, &["auth", "status"]).map_err(|e| match e {
            Error::Unsupported(m) => Error::Unsupported(m),
            e => Error::Unsupported(format!(
                "the GitHub CLI is not signed in; run `gh auth login` first ({e})"
            )),
        })?;
        match gh.run(&root, &["pr", "view", &branch, "--json", "url,state"]) {
            Ok(v) => {
                let v: Value = serde_json::from_str(&v).unwrap_or_default();
                if v["state"].as_str() == Some("OPEN") {
                    out.url = v["url"].as_str().map(str::to_owned);
                    return Ok(out);
                }
            }
            Err(Error::Invalid(m)) if m.contains("no pull requests found") => {}
            Err(e) => return Err(e),
        }
        git::push_branch(&root, "origin", &branch)?;
        let mut args = vec![
            "pr", "create", "--head", &branch, "--base", &base, "--title", &out.title, "--body",
            &out.body,
        ];
        if draft {
            args.push("--draft");
        }
        let created = gh.run(&root, &args)?;
        out.url = created
            .lines()
            .rev()
            .find(|l| l.trim().starts_with("http"))
            .map(|l| l.trim().to_owned());
        out.created = true;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{Actor, NewTask};
    use std::path::Path;
    use std::sync::Mutex;

    /// Answers `gh` from a table and records every call.
    struct MockGh(
        Vec<(&'static str, std::result::Result<String, String>)>,
        Mutex<Vec<String>>,
    );
    impl Gh for MockGh {
        fn run(&self, _cwd: &Path, args: &[&str]) -> Result<String> {
            let line = args.join(" ");
            self.1.lock().unwrap().push(line.clone());
            for (needle, answer) in &self.0 {
                if line.contains(needle) {
                    return answer.clone().map_err(Error::Invalid);
                }
            }
            Err(Error::Invalid(format!("unexpected gh {line}")))
        }
    }
    fn gh(table: Vec<(&'static str, std::result::Result<String, String>)>) -> MockGh {
        MockGh(table, Mutex::new(vec![]))
    }

    fn run_git(root: &Path, args: &[&str]) {
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

    /// A task on branch `feat/x` in a repository whose `origin` is a local bare repository.
    fn setup() -> (tempfile::TempDir, Store, Task, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let paths = crate::paths::GlobalPaths {
            data_dir: dir.path().join("data"),
            config_dir: dir.path().join("config"),
        };
        paths.ensure().unwrap();
        let store = Store::open_at(&paths.db_path(), paths).unwrap();
        let origin = dir.path().join("origin.git");
        let root = dir.path().join("repo");
        std::fs::create_dir_all(&root).unwrap();
        run_git(
            dir.path(),
            &["init", "-q", "--bare", origin.to_str().unwrap()],
        );
        run_git(&root, &["init", "-q", "-b", "main"]);
        run_git(&root, &["config", "user.email", "t@example.com"]);
        run_git(&root, &["config", "user.name", "t"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        run_git(&root, &["add", "-A"]);
        run_git(&root, &["commit", "-qm", "init"]);
        run_git(
            &root,
            &["remote", "add", "origin", origin.to_str().unwrap()],
        );
        run_git(&root, &["checkout", "-q", "-b", "feat/x"]);
        let pid = store.register_project(&root, None).unwrap().project.id;
        let task = store
            .create_task(
                &pid,
                NewTask {
                    title: "Export the report".into(),
                    description: format!("Use token ghp_{} for the API.", "A".repeat(36)),
                    acceptance: vec!["Tests pass".into(), "Docs updated".into()],
                    ..Default::default()
                },
            )
            .unwrap();
        (dir, store, task, origin)
    }

    #[test]
    fn the_body_lists_criteria_receipts_and_the_checkpoint_without_secrets() {
        let (_d, store, task, _o) = setup();
        let root = store.task_root(&task).unwrap();
        let r = store
            .run_verification(
                &task.project_id,
                Some(&task.id),
                None,
                &["git".into(), "diff".into(), "--check".into()],
                &root,
            )
            .unwrap()
            .receipt;
        store
            .check_criterion(
                &task.id,
                1,
                true,
                "",
                Some(&r.id),
                &Actor::Human {
                    override_reason: None,
                },
            )
            .unwrap();
        store
            .create_checkpoint(
                &task.project_id,
                crate::checkpoints::NewCheckpoint {
                    task_id: Some(task.id.clone()),
                    summary: "Export works".into(),
                    completed: vec!["Wrote the exporter".into()],
                    next_action: "Write the docs".into(),
                    ..Default::default()
                },
            )
            .unwrap();
        let p = store
            .pr_create(&task, &gh(vec![]), None, false, true)
            .unwrap();
        assert!(p.dry_run && p.url.is_none());
        assert_eq!((p.branch.as_str(), p.base.as_str()), ("feat/x", "main"));
        assert_eq!(p.title, "Export the report (T-1)");
        let b = &p.body;
        assert!(b.contains("1 of 2 met"), "{b}");
        assert!(
            b.contains("- [x] 1. Tests pass — **met** (shown by an Evyro receipt"),
            "{b}"
        );
        assert!(b.contains("- [ ] 2. Docs updated — **not met**"), "{b}");
        assert!(b.contains("`git diff --check` — **passed**"), "{b}");
        assert!(b.contains("(the pushed commit)"), "{b}");
        assert!(
            b.contains("Export works") && b.contains("**Next step:** Write the docs"),
            "{b}"
        );
        assert!(b.contains("**Done:** Wrote the exporter"), "{b}");
        assert!(
            !b.contains(&"A".repeat(36)),
            "the description is redacted: {b}"
        );
    }

    #[test]
    fn a_dry_run_calls_neither_github_nor_git_push() {
        let (_d, store, task, origin) = setup();
        let g = gh(vec![]);
        let p = store
            .pr_create(&task, &g, Some("develop"), true, true)
            .unwrap();
        assert!(p.draft && p.base == "develop");
        assert!(g.1.lock().unwrap().is_empty());
        assert!(p.body.contains("No test run was recorded") && p.body.contains("No checkpoint"));
        let heads = std::process::Command::new("git")
            .args(["--git-dir", origin.to_str().unwrap(), "branch", "--list"])
            .output()
            .unwrap();
        assert!(heads.stdout.is_empty(), "nothing was pushed");
    }

    #[test]
    fn gh_not_signed_in_is_refused_before_anything_is_pushed() {
        let (_d, store, task, _o) = setup();
        let g = gh(vec![(
            "auth status",
            Err("You are not logged into any GitHub hosts".into()),
        )]);
        let e = store.pr_create(&task, &g, None, false, false).unwrap_err();
        assert!(
            matches!(e, Error::Unsupported(ref m) if m.contains("gh auth login")),
            "{e}"
        );
        assert_eq!(g.1.lock().unwrap().len(), 1);
    }

    #[test]
    fn create_pushes_the_branch_and_opens_the_pull_request() {
        let (_d, store, task, origin) = setup();
        let g = gh(vec![
            ("auth status", Ok("Logged in".into())),
            (
                "pr view",
                Err("no pull requests found for branch \"feat/x\"".into()),
            ),
            ("pr create", Ok("https://github.com/o/r/pull/7\n".into())),
        ]);
        let p = store.pr_create(&task, &g, None, true, false).unwrap();
        assert_eq!(p.url.as_deref(), Some("https://github.com/o/r/pull/7"));
        assert!(p.created);
        let calls = g.1.lock().unwrap();
        let create = calls.iter().find(|c| c.starts_with("pr create")).unwrap();
        assert!(create.contains("--head feat/x --base main") && create.ends_with("--draft"));
        let pushed = std::process::Command::new("git")
            .args([
                "--git-dir",
                origin.to_str().unwrap(),
                "rev-parse",
                "--verify",
                "feat/x",
            ])
            .output()
            .unwrap();
        assert!(pushed.status.success(), "the branch is on origin");
    }

    #[test]
    fn an_open_pull_request_is_returned_not_duplicated() {
        let (_d, store, task, _o) = setup();
        let g = gh(vec![
            ("auth status", Ok(String::new())),
            (
                "pr view",
                Ok(r#"{"url":"https://github.com/o/r/pull/3","state":"OPEN"}"#.into()),
            ),
        ]);
        let p = store.pr_create(&task, &g, None, false, false).unwrap();
        assert_eq!(p.url.as_deref(), Some("https://github.com/o/r/pull/3"));
        assert!(!p.created);
        assert!(!g
            .1
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.starts_with("pr create")));
    }

    #[test]
    fn the_base_branch_itself_is_refused() {
        let (_d, store, task, _o) = setup();
        let e = store.pr_create(&task, &gh(vec![]), Some("feat/x"), false, true);
        assert!(e.is_err());
    }
}
