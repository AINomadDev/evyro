//! Claiming work for a switch target, and handing it to apps the person drives
//! (IDEs and chat apps). Shared by the CLI and the desktop app.

use crate::checkpoints::NewCheckpoint;
use crate::db::Store;
use crate::error::{Error, Result};
use crate::handoff::{resume_prompt, ResumeOutcome};
use crate::projects::Project;
use crate::sessions::{NewSession, Session};
use crate::targets::{self, Target, TargetKind};
use crate::tasks::{Actor, Task, TaskStatus, TaskUpdate};
use crate::{brand, util};
use serde::Serialize;
use std::process::{Command, Stdio};

pub fn task_prompt(t: Option<&Task>) -> String {
    match t {
        Some(t) => format!(
            "Work on task {} \"{}\" in this repository, tracked by {}. First run `{} context` to load the task, acceptance criteria and latest checkpoint. Record progress with `{} checkpoint --done \"...\" --next \"...\"` and run tests via `{} verify -- <cmd>`.",
            t.key, t.title, brand::PRODUCT_NAME, brand::CLI_NAME, brand::CLI_NAME, brand::CLI_NAME
        ),
        None => format!(
            "Run `{} context` and `{} tasks --ready` to pick up work in this repository.",
            brand::CLI_NAME,
            brand::CLI_NAME
        ),
    }
}

#[allow(clippy::large_enum_variant)]
pub enum Claim {
    Started {
        session: Session,
        prompt: String,
        handoff_id: Option<String>,
    },
    /// Resume stopped on drift; nothing changed.
    Drift(Box<ResumeOutcome>),
}

#[derive(Debug, Clone, Serialize)]
pub struct AppHandover {
    pub target: String,
    pub display: String,
    pub kind: TargetKind,
    pub session: String,
    pub handoff: Option<String>,
    pub file: String,
    pub text: String,
    pub copied_to_clipboard: bool,
    pub opened: bool,
}

impl Store {
    /// Accept the pending handoff for this target, or start a session on the task.
    pub fn claim_for_target(
        &self,
        p: &Project,
        target: &str,
        task_ref: Option<&str>,
        handoff_id: Option<&str>,
        accept_drift: bool,
        model: Option<&str>,
    ) -> Result<Claim> {
        // An explicit handoff can go to any target (the person changed their
        // mind about which AI continues). Otherwise only one addressed to this
        // target, or to "any", is picked up.
        let pending = match handoff_id {
            Some(id) => Some(self.get_handoff(id)?),
            None => self
                .pending_handoff(&p.id, Some(target))?
                .filter(|h| h.to_harness == target || h.to_harness == "any"),
        };
        if let Some(h) = pending {
            let r = self.resume_handoff(&p.id, Some(&h.id), target, accept_drift, model)?;
            if !r.accepted {
                return Ok(Claim::Drift(Box::new(r)));
            }
            return Ok(Claim::Started {
                session: r.session.clone().expect("accepted resume has a session"),
                prompt: resume_prompt(&r.contract),
                handoff_id: Some(h.id),
            });
        }
        let t = match task_ref {
            Some(r) => Some(self.resolve_task(&p.id, r)?),
            // No task named: continue the active task only if no other AI is
            // working on it; otherwise start free and let the agent pick one.
            None => match self.active_task(&p.id)? {
                Some(t) if self.active_session_for_task(&t.id)?.is_some() => None,
                t => t,
            },
        };
        if let Some(candidate) = t
            .as_ref()
            .filter(|task| crate::comparison::is_candidate(task))
        {
            self.task_root(candidate)?;
        }
        let ses = self.start_session(
            &p.id,
            NewSession {
                task_id: t.as_ref().map(|t| t.id.clone()),
                harness: target.to_string(),
                model: model.map(String::from),
                worktree: t.as_ref().and_then(|t| t.worktree.clone()),
                ..Default::default()
            },
        )?;
        if let Some(t) = &t {
            if matches!(t.status, TaskStatus::Backlog | TaskStatus::Ready) {
                self.update_task(
                    &t.id,
                    TaskUpdate {
                        status: Some(TaskStatus::InProgress),
                        ..Default::default()
                    },
                    &Actor::Agent(target.to_string()),
                )?;
            }
            self.create_checkpoint(
                &p.id,
                NewCheckpoint {
                    task_id: Some(t.id.clone()),
                    session_id: Some(ses.id.clone()),
                    trigger: Some("task_started".into()),
                    dedupe: true,
                    ..Default::default()
                },
            )?;
        }
        Ok(Claim::Started {
            session: ses,
            prompt: task_prompt(t.as_ref()),
            handoff_id: None,
        })
    }

    /// IDE and chat targets: the person drives the AI; Evyro prepares the
    /// hand-over text, saves it under `.evyro/`, copies it, and opens the app.
    /// `Err(outcome)` inside `Ok` means drift stopped the resume.
    pub fn handover_to_app(
        &self,
        p: &Project,
        target: &Target,
        task_ref: Option<&str>,
        handoff_id: Option<&str>,
        accept_drift: bool,
        no_open: bool,
    ) -> Result<std::result::Result<AppHandover, Box<ResumeOutcome>>> {
        let app = target.app.clone().unwrap_or_default();
        if !matches!(target.kind, TargetKind::Ide | TargetKind::Chat) {
            return Err(Error::Invalid(format!(
                "{} is not an IDE or chat target",
                target.name
            )));
        }
        // Work out how to open the app first, so a missing app changes nothing.
        let root = match task_ref {
            Some(r) => self.task_root(&self.resolve_task(&p.id, r)?)?,
            None => match self.active_task(&p.id)? {
                Some(t) => self.task_root(&t)?,
                None => p.root(),
            },
        };
        let open_argv = if no_open {
            None
        } else {
            Some(match target.kind {
                TargetKind::Ide => targets::ide_open_argv(&app, &root)?,
                _ => targets::chat_open_argv(&app)?,
            })
        };
        let (session, prompt, handoff_id) = match self.claim_for_target(
            p,
            &target.name,
            task_ref,
            handoff_id,
            accept_drift,
            None,
        )? {
            Claim::Drift(r) => return Ok(Err(r)),
            Claim::Started {
                session,
                prompt,
                handoff_id,
            } => (session, prompt, handoff_id),
        };
        let (file, text) = if target.kind == TargetKind::Ide {
            (p.evyro_dir().join("HANDOFF_PROMPT.md"), prompt)
        } else {
            let key = match &session.task_id {
                Some(t) => Some(self.get_task(t)?.key),
                None => None,
            };
            (
                p.evyro_dir().join("CHAT_PACK.md"),
                self.chat_pack(&p.id, handoff_id.as_deref(), key.as_deref())?,
            )
        };
        util::write_atomic(&file, text.as_bytes())?;
        let copied = !no_open && targets::copy_to_clipboard(&text);
        let opened = open_argv
            .map(|argv| {
                Command::new(&argv[0])
                    .args(&argv[1..])
                    .current_dir(&root)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .is_ok()
            })
            .unwrap_or(false);
        self.emit(
            Some(&p.id),
            session.task_id.as_deref(),
            Some(&session.id),
            "agent.handed_to_app",
            serde_json::json!({"target": target.name, "kind": target.kind, "opened": opened, "copied": copied}),
        )?;
        Ok(Ok(AppHandover {
            target: target.name.clone(),
            display: target.display(),
            kind: target.kind,
            session: session.id,
            handoff: handoff_id,
            file: file.display().to_string(),
            text,
            copied_to_clipboard: copied,
            opened,
        }))
    }
}
