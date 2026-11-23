//! Agent Client Protocol (ACP) support for Evyro.
//!
//! ACP lets a client talk to a coding agent in structured messages instead of
//! a terminal: the agent streams its reply, its plan and each tool call, and
//! asks before it runs something. Evyro uses it for chat threads: the agent
//! runs under an Evyro session (claimed task or handoff, checkpoints at the
//! end of each turn) and gets Evyro's own MCP server, so it can read the task,
//! record progress and prepare a handoff from inside the chat.
//!
//! Spec: <https://agentclientprotocol.com> (protocol version 1).

pub mod client;
pub mod host;

pub use client::{pick_option, AcpClient, AcpError, Event, Launch, McpStdio, OnEvent};

use evyro_core::checkpoints::NewCheckpoint;
use evyro_core::handover::Claim;
use evyro_core::projects::Project;
use evyro_core::sessions::{Session, SessionStatus};
use evyro_core::{adapters, targets, Error, Result, Store};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// How to start a coding agent in ACP mode. Commands follow each agent's
/// own documentation; Evyro never downloads anything to start one.
#[derive(Debug, Clone, Serialize)]
pub struct AcpAgent {
    /// Evyro harness id (the same ids as `evyro agents`).
    pub harness: &'static str,
    pub display_name: &'static str,
    /// Executables to look for, in order.
    pub binaries: &'static [&'static str],
    pub args: &'static [&'static str],
    /// How to install the ACP side when it is missing.
    pub install: &'static str,
    pub evidence: &'static str,
}

pub const AGENTS: &[AcpAgent] = &[
    AcpAgent {
        harness: "claude-code",
        display_name: "Claude Code (Claude Agent ACP adapter)",
        binaries: &["claude-agent-acp"],
        args: &[],
        install: "npm install -g @agentclientprotocol/claude-agent-acp",
        evidence:
            "documented (agentclientprotocol.com agent list; npm package bin `claude-agent-acp`)",
    },
    AcpAgent {
        harness: "codex",
        display_name: "Codex CLI (Codex ACP adapter)",
        binaries: &["codex-acp"],
        args: &[],
        install: "npm install -g @agentclientprotocol/codex-acp",
        evidence: "documented (agentclientprotocol.com agent list; npm package bin `codex-acp`)",
    },
    AcpAgent {
        harness: "gemini",
        display_name: "Gemini CLI",
        binaries: &["gemini"],
        args: &["--experimental-acp"],
        install: "npm install -g @google/gemini-cli",
        evidence: "documented (Gemini CLI ACP mode)",
    },
    AcpAgent {
        harness: "goose",
        display_name: "Goose",
        binaries: &["goose"],
        args: &["acp"],
        install: "see https://block.github.io/goose/docs/guides/acp-clients",
        evidence: "documented (Goose ACP guide)",
    },
    AcpAgent {
        harness: "opencode",
        display_name: "OpenCode",
        binaries: &["opencode"],
        args: &["acp"],
        install: "see https://opencode.ai",
        evidence: "documented (agentclientprotocol.com agent list)",
    },
    AcpAgent {
        harness: "copilot",
        display_name: "GitHub Copilot CLI",
        binaries: &["copilot"],
        args: &["--acp"],
        install: "npm install -g @github/copilot",
        evidence: "documented (Copilot CLI ACP public preview, 2026-01)",
    },
    AcpAgent {
        harness: "cursor",
        display_name: "Cursor CLI",
        binaries: &["cursor-agent"],
        args: &["acp"],
        install: "see https://cursor.com/docs/cli/acp",
        evidence: "documented (cursor.com/docs/cli/acp)",
    },
];

pub fn agent(harness: &str) -> Option<&'static AcpAgent> {
    AGENTS.iter().find(|a| a.harness == harness)
}

/// `EVYRO_ACP_BIN_<HARNESS>` points at another ACP executable (no extra
/// arguments): for custom builds, wrappers and tests.
fn bin_override(harness: &str) -> Option<PathBuf> {
    let key = format!(
        "EVYRO_ACP_BIN_{}",
        harness.to_ascii_uppercase().replace('-', "_")
    );
    std::env::var_os(key).map(PathBuf::from)
}

/// The command that starts this harness in ACP mode, if it is installed.
pub fn command_for(harness: &str) -> Option<(PathBuf, Vec<String>)> {
    if let Some(p) = bin_override(harness) {
        return Some((p, vec![]));
    }
    let a = agent(harness)?;
    let bin = a.binaries.iter().find_map(|b| adapters::which(b))?;
    Some((bin, a.args.iter().map(|s| s.to_string()).collect()))
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub harness: String,
    pub display_name: String,
    pub command: Option<String>,
    pub install: String,
    pub evidence: String,
}

/// Every known ACP agent and whether it can be started here.
pub fn agent_status() -> Vec<AgentStatus> {
    AGENTS
        .iter()
        .map(|a| AgentStatus {
            harness: a.harness.into(),
            display_name: a.display_name.into(),
            command: command_for(a.harness).map(|(p, args)| {
                std::iter::once(p.to_string_lossy().into_owned())
                    .chain(args)
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
            install: a.install.into(),
            evidence: a.evidence.into(),
        })
        .collect()
}

pub struct ChatOpts<'a> {
    /// A switch target name (harness or API target) such as `codex`.
    pub target: &'a str,
    pub task: Option<&'a str>,
    pub handoff: Option<&'a str>,
    pub accept_drift: bool,
    pub model: Option<&'a str>,
    /// The `evyro` executable to offer the agent as an MCP server. `None`
    /// leaves Evyro's MCP server out.
    pub evyro_mcp: Option<PathBuf>,
    /// Start this command instead of the agent's usual ACP command.
    pub command: Option<(PathBuf, Vec<String>)>,
}

/// What the chat has done so far, for checkpoints.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Activity {
    /// The agent's reply in the current turn, as streamed.
    pub reply: String,
    /// The last finished reply.
    pub last_reply: String,
    pub edited: Vec<String>,
    pub tools_done: Vec<String>,
    pub turns: usize,
    pub exited: Option<Option<i32>>,
}

pub struct Chat {
    pub client: AcpClient,
    pub session: Session,
    pub project_id: String,
    pub acp_session: String,
    pub cwd: PathBuf,
    pub harness: String,
    pub target: String,
    pub display_name: String,
    /// Evyro's opening message: the task or the handoff's resume prompt.
    pub first_prompt: String,
    pub agent_info: Value,
    pub activity: Arc<Mutex<Activity>>,
}

#[allow(clippy::large_enum_variant)]
pub enum Started {
    Chat(Chat),
    /// The handoff's recorded state no longer matches the repository.
    Drift(Box<evyro_core::handoff::ResumeOutcome>),
}

fn track(a: &mut Activity, e: &Event) {
    match e {
        Event::Update { update, .. } => {
            let kind = update
                .get("sessionUpdate")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            match kind {
                "agent_message_chunk" => {
                    if let Some(t) = update.pointer("/content/text").and_then(|v| v.as_str()) {
                        a.reply.push_str(t);
                    }
                }
                "tool_call" | "tool_call_update" => {
                    let done = update.get("status").and_then(|v| v.as_str()) == Some("completed");
                    if done {
                        if let Some(t) = update.get("title").and_then(|v| v.as_str()) {
                            a.tools_done.push(t.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        Event::FileWrite { path, .. } => {
            if !a.edited.contains(path) {
                a.edited.push(path.clone());
            }
        }
        Event::Exit(code) => a.exited = Some(*code),
        _ => {}
    }
}

/// Claim the task (or handoff) for `target`, start its agent in ACP mode,
/// and open an ACP session in the task's directory. Nothing is claimed when
/// the agent cannot be found.
pub fn start(store: &Store, p: &Project, opts: ChatOpts, on_event: OnEvent) -> Result<Started> {
    let target = targets::resolve_target(&store.paths, opts.target)?;
    if !matches!(
        target.kind,
        targets::TargetKind::Harness | targets::TargetKind::Api
    ) {
        return Err(Error::Unsupported(format!(
            "{} is an app, not a coding agent; chat works with coding agents and API targets",
            target.name
        )));
    }
    let mut plan = targets::launch_plan(&target)?;
    if let Some(m) = opts.model {
        plan.model = Some(m.to_string());
    }
    let harness = plan.harness.clone();
    let display_name = agent(&harness)
        .map(|a| a.display_name.to_string())
        .unwrap_or_else(|| harness.clone());
    let (program, args) = match opts.command.clone().or_else(|| command_for(&harness)) {
        Some(c) => c,
        None => {
            return Err(Error::Unsupported(match agent(&harness) {
                Some(a) => format!(
                    "{} cannot be started in ACP mode here: none of {} is on PATH. Install it with `{}`, or use a terminal thread instead.",
                    a.display_name,
                    a.binaries.join(", "),
                    a.install
                ),
                None => format!("{harness} has no ACP mode Evyro knows of; use a terminal thread instead"),
            }))
        }
    };

    let (session, first_prompt) = match store.claim_for_target(
        p,
        &target.name,
        opts.task,
        opts.handoff,
        opts.accept_drift,
        plan.model.as_deref(),
    )? {
        Claim::Started {
            session, prompt, ..
        } => (session, prompt),
        Claim::Drift(r) => return Ok(Started::Drift(r)),
    };
    let cwd = match &session.task_id {
        Some(t) => store.task_root(&store.get_task(t)?)?,
        None => p.root(),
    };

    let activity = Arc::new(Mutex::new(Activity::default()));
    let act = activity.clone();
    let forward = on_event.clone();
    let tracked: OnEvent = Arc::new(move |e: Event| {
        if let Ok(mut a) = act.lock() {
            track(&mut a, &e);
        }
        forward(e);
    });

    let mut env_set = plan.env_set.clone();
    env_set.push(("EVYRO_SESSION_ID".into(), session.id.clone()));
    env_set.push(("EVYRO_PROJECT_ID".into(), p.id.clone()));
    let launch = Launch {
        program: program.clone(),
        args,
        cwd: cwd.clone(),
        env_set,
        env_remove: plan.env_remove.clone(),
    };
    drop(plan); // key values are not needed any longer

    let fail = |why: String| -> Error {
        let _ = store.set_session_status(&session.id, SessionStatus::Failed, Some("launch_failed"));
        Error::Other(why)
    };
    let client = AcpClient::spawn(&launch, tracked).map_err(|e| fail(e.0))?;
    let agent_info = match client.initialize() {
        Ok(v) => v,
        Err(e) => {
            client.shutdown();
            return Err(fail(format!(
                "{display_name} did not start an ACP session: {e}"
            )));
        }
    };
    let mut mcp = Vec::new();
    if let Some(bin) = &opts.evyro_mcp {
        let mut env = vec![("EVYRO_SESSION_ID".to_string(), session.id.clone())];
        if let Ok(h) = std::env::var("EVYRO_HOME") {
            env.push(("EVYRO_HOME".into(), h));
        }
        mcp.push(McpStdio {
            name: "evyro".into(),
            command: bin.clone(),
            args: vec![
                "--project".into(),
                p.id.clone(),
                "mcp".into(),
                "--agent".into(),
                harness.clone(),
            ],
            env,
        });
    }
    let acp_session = match client.new_session(&cwd, &mcp) {
        Ok(s) => s,
        Err(e) => {
            client.shutdown();
            return Err(fail(format!("{display_name} did not open a session: {e}")));
        }
    };
    store.set_session_pid(&session.id, client.pid().map(|x| x as i64))?;
    store.emit(
        Some(&p.id),
        session.task_id.as_deref(),
        Some(&session.id),
        "agent.started",
        json!({"argv0": program.to_string_lossy(), "target": target.name, "auth": target.auth, "mode": "acp", "acp_session": acp_session, "agent": agent_info.get("agentInfo")}),
    )?;
    Ok(Started::Chat(Chat {
        client,
        session,
        project_id: p.id.clone(),
        acp_session,
        cwd,
        harness,
        target: target.name,
        display_name,
        first_prompt,
        agent_info,
        activity,
    }))
}

impl Chat {
    /// Send a message and wait for the agent's turn to end. Returns the stop reason.
    pub fn send(&self, text: &str) -> std::result::Result<String, AcpError> {
        if let Ok(mut a) = self.activity.lock() {
            a.reply.clear();
        }
        let r = self.client.prompt(&self.acp_session, text);
        if let Ok(mut a) = self.activity.lock() {
            a.turns += 1;
            if !a.reply.trim().is_empty() {
                a.last_reply = std::mem::take(&mut a.reply);
            }
        }
        r
    }

    pub fn cancel(&self) -> std::result::Result<(), AcpError> {
        self.client.cancel(&self.acp_session)
    }

    fn checkpoint(&self, store: &Store, trigger: &str) -> Result<()> {
        let a = self.activity.lock().map(|a| a.clone()).unwrap_or_default();
        let mut completed: Vec<String> = a.edited.iter().map(|f| format!("Edited {f}")).collect();
        completed.extend(a.tools_done.iter().rev().take(10).rev().cloned());
        completed.dedup();
        let summary: String = a.last_reply.trim().chars().take(600).collect();
        store.create_checkpoint(
            &self.project_id,
            NewCheckpoint {
                task_id: self.session.task_id.clone(),
                session_id: Some(self.session.id.clone()),
                trigger: Some(trigger.into()),
                summary,
                completed,
                dedupe: true,
                ..Default::default()
            },
        )?;
        Ok(())
    }

    /// Record progress after a turn (skipped when nothing changed).
    pub fn turn_checkpoint(&self, store: &Store) -> Result<()> {
        let s = store.get_session(&self.session.id)?;
        if s.status.is_ended() {
            return Ok(());
        }
        self.checkpoint(store, "auto")
    }

    /// Stop the agent, checkpoint, and close the Evyro session.
    pub fn finish(&self, store: &Store) -> Result<()> {
        let crashed = self
            .activity
            .lock()
            .ok()
            .and_then(|a| a.exited)
            .map(|c| c != Some(0));
        self.client.shutdown();
        let s = store.get_session(&self.session.id)?;
        let code = self.activity.lock().ok().and_then(|a| a.exited).flatten();
        store.emit(
            Some(&self.project_id),
            s.task_id.as_deref(),
            Some(&s.id),
            "agent.stopped",
            json!({"exit_code": code, "mode": "acp"}),
        )?;
        if s.status.is_ended() {
            return Ok(());
        }
        self.checkpoint(store, "process_exit")?;
        let (to, why) = match crashed {
            Some(true) => (
                SessionStatus::Failed,
                format!(
                    "process_exit:{}",
                    code.map_or("signal".into(), |c| c.to_string())
                ),
            ),
            _ => (SessionStatus::Completed, "chat_closed".to_string()),
        };
        store.set_session_status(&s.id, to, Some(&why))?;
        Ok(())
    }
}
