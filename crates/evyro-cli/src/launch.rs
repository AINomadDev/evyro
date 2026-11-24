//! Hand work to a switch target under Evyro session tracking.
//!
//! - Harness and API targets: launch the CLI with the resume prompt, the
//!   target's model and its account/key environment.
//! - IDE targets: open the repository in the IDE with the prompt on the clipboard.
//! - Chat targets: build a self-contained chat pack and copy it.

use crate::out::Out;
use evyro_core::adapters::{self, LaunchMode};
use evyro_core::checkpoints::NewCheckpoint;
use evyro_core::handoff::resume_prompt;
use evyro_core::handover::Claim;
use evyro_core::projects::Project;
use evyro_core::sessions::SessionStatus;
use evyro_core::targets::{self, Target, TargetKind};
use evyro_core::{brand, Error, Store};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::Command;

fn bin_override(id: &str) -> Option<PathBuf> {
    let key = format!(
        "EVYRO_AGENT_BIN_{}",
        id.to_ascii_uppercase().replace('-', "_")
    );
    std::env::var_os(key).map(PathBuf::from)
}

pub struct RunOpts<'a> {
    pub target: &'a str,
    pub task: Option<&'a str>,
    /// Continue this handoff even if it was prepared for another target.
    pub handoff: Option<&'a str>,
    pub headless: bool,
    pub prompt: Option<String>,
    pub model: Option<&'a str>,
    pub accept_drift: bool,
    /// IDE/chat targets: prepare everything but do not open apps or touch the clipboard.
    pub no_open: bool,
}

pub fn run_agent(store: &Store, p: &Project, opts: RunOpts, o: &Out) -> anyhow::Result<i32> {
    let target = targets::resolve_target(&store.paths, opts.target)?;
    match target.kind {
        TargetKind::Ide | TargetKind::Chat => return hand_to_app(store, p, &target, &opts, o),
        TargetKind::Harness | TargetKind::Api => {}
    }
    // Resolve model and keys before touching any state: a missing key must
    // not accept a handoff.
    let mut plan = targets::launch_plan(&target)?;
    if let Some(m) = opts.model {
        plan.model = Some(m.to_string());
    }
    let spec = adapters::get(&plan.harness)?;
    let bin =
        bin_override(spec.id).or_else(|| spec.binaries.iter().find_map(|b| adapters::which(b)));

    // Interactive agents need a terminal. Check before touching any state so
    // a launch that cannot work never accepts a handoff or claims a task.
    if !opts.headless && bin.is_some() && !std::io::stdin().is_terminal() {
        eprintln!(
            "{} needs an interactive terminal. Run this from a terminal, or pass --headless.",
            spec.display_name
        );
        return Ok(Error::Unsupported(String::new()).exit_code());
    }
    if bin.is_none() {
        let prompt = match store
            .pending_handoff(&p.id, Some(&target.name))?
            .filter(|h| h.to_harness == target.name)
        {
            Some(h) => resume_prompt(&evyro_core::contract::load(std::path::Path::new(
                &h.contract_path,
            ))?),
            None => evyro_core::handover::task_prompt(
                match opts.task {
                    Some(r) => Some(store.resolve_task(&p.id, r)?),
                    None => store.active_task(&p.id)?,
                }
                .as_ref(),
            ),
        };
        println!("{} is not installed or has no launchable CLI. Fallback: open this repository in {} and give it this prompt:\n\n{}", spec.display_name, spec.display_name, prompt);
        return Ok(Error::Unsupported(String::new()).exit_code());
    }
    let mode = if opts.headless {
        LaunchMode::Headless
    } else {
        LaunchMode::Interactive
    };
    let bin = bin.unwrap();
    // Validate the argv shape (model support) before claiming.
    spec.launch_argv(&bin.to_string_lossy(), mode, "probe", plan.model.as_deref())?;

    // Decide where the AI works. It stays in the main checkout unless another AI is there and the
    // two cannot be shown to touch different files; unfinished work keeps its worktree.
    let task_for_checkout = match (opts.task, opts.handoff) {
        (Some(r), _) => Some(store.resolve_task(&p.id, r)?),
        (None, Some(h)) => match store.get_handoff(h)?.task_id {
            Some(id) => Some(store.get_task(&id)?),
            None => None,
        },
        (None, None) => store.active_task(&p.id)?.filter(|t| {
            store
                .active_session_for_task(&t.id)
                .ok()
                .flatten()
                .is_none()
        }),
    };
    if let Some(task) = task_for_checkout {
        let checkout = store.prepare_checkout(p, &task)?;
        if let (Some(note), false, false) = (&checkout.note, o.quiet, o.json) {
            eprintln!("[{}] {note}", brand::CLI_NAME);
        }
    }
    let (session, default_prompt) = match store.claim_for_target(
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
        Claim::Drift(r) => {
            o.print(&r, || crate::out::render_resume(&r));
            return Ok(Error::Drift(String::new()).exit_code());
        }
    };
    let mut argv = spec.launch_argv(
        &bin.to_string_lossy(),
        mode,
        &opts.prompt.clone().unwrap_or(default_prompt),
        plan.model.as_deref(),
    )?;
    for (i, a) in plan.extra_args.iter().enumerate() {
        argv.insert(1 + i, a.clone());
    }
    let desktop_copilot = [
        ("EVYRO_DESKTOP_COPILOT_BUDGET", "--max-ai-credits"),
        ("EVYRO_DESKTOP_COPILOT_SESSION", "--session-id"),
        ("EVYRO_DESKTOP_COPILOT_USAGE", "--usage-output-file"),
    ];
    if spec.id == "copilot" && !opts.headless {
        for (env, flag) in desktop_copilot {
            if let Ok(value) = std::env::var(env) {
                argv.splice(1..1, [flag.to_string(), value]);
            }
        }
    }
    if opts.headless && spec.id == "codex" {
        // The workspace-write sandbox lets Codex write only in the repo and
        // temp dirs. Its own `evyro checkpoint` / `evyro verify` calls write
        // Evyro's database, so that folder has to be writable too.
        if let Some(at) = argv.iter().position(|a| a == "workspace-write") {
            let data = evyro_core::paths::GlobalPaths::resolve()?.data_dir;
            argv.insert(at + 1, "--add-dir".into());
            argv.insert(at + 2, data.to_string_lossy().into_owned());
        }
    }
    let cwd = match &session.task_id {
        Some(t) => store.task_root(&store.get_task(t)?)?,
        None => p.root(),
    };
    if !o.quiet && !o.json {
        eprintln!(
            "[{}] launching {} for target '{}' ({}; session {})",
            brand::CLI_NAME,
            spec.display_name,
            target.name,
            target.auth.label(),
            session.id
        );
    }
    // Windows: npm `.cmd` shims cannot carry the prompt's line breaks.
    let argv = evyro_core::adapters::launchable_argv(argv);
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..])
        .current_dir(&cwd)
        .env("EVYRO_SESSION_ID", &session.id)
        .env("EVYRO_PROJECT_ID", &p.id);
    if opts.headless {
        // Nobody types into a headless run. An open stdin makes some CLIs
        // (Codex among them) wait for more prompt text forever.
        cmd.stdin(std::process::Stdio::null());
        if spec.id == "gemini" {
            // Gemini asks whether to trust a new folder before it will edit
            // there; headless runs have nobody to answer.
            cmd.env("GEMINI_CLI_TRUST_WORKSPACE", "true");
        }
    }
    for k in &plan.env_remove {
        cmd.env_remove(k);
    }
    for (k, v) in &plan.env_set {
        cmd.env(k, v);
    }
    for (env, _) in desktop_copilot {
        cmd.env_remove(env);
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            store.set_session_status(&session.id, SessionStatus::Failed, Some("launch_failed"))?;
            return Err(Error::Other(format!("could not launch {}: {e}", argv[0])).into());
        }
    };
    drop(plan); // key values are not needed any longer
    store.set_session_pid(&session.id, Some(child.id() as i64))?;
    store.emit(Some(&p.id), session.task_id.as_deref(), Some(&session.id), "agent.started", serde_json::json!({"argv0": argv[0], "target": target.name, "auth": target.auth, "mode": if opts.headless { "headless" } else { "interactive" }}))?;
    let status = child.wait()?;
    let code = status.code();
    store.emit(
        Some(&p.id),
        session.task_id.as_deref(),
        Some(&session.id),
        "agent.stopped",
        serde_json::json!({"exit_code": code}),
    )?;

    let s = store.get_session(&session.id)?;
    if !s.status.is_ended() {
        let _ = store.create_checkpoint(
            &p.id,
            NewCheckpoint {
                task_id: s.task_id.clone(),
                session_id: Some(s.id.clone()),
                trigger: Some("process_exit".into()),
                dedupe: true,
                ..Default::default()
            },
        );
        let (to, reason) = match code {
            Some(0) => (SessionStatus::Completed, "process_exit:0".to_string()),
            Some(c) => (SessionStatus::Failed, format!("process_exit:{c}")),
            None => (SessionStatus::Failed, "terminated_by_signal".to_string()),
        };
        store.set_session_status(&s.id, to, Some(&reason))?;
        if !o.quiet && !o.json {
            eprintln!("[{}] {} exited ({}); progress checkpointed. Continue later with `evyro run --agent <target>` or `evyro switch --to <target>`.", brand::CLI_NAME, spec.display_name, reason);
        }
    }
    Ok(code.unwrap_or(1))
}

/// IDE and chat targets: the person drives the AI; Evyro prepares the hand-over.
fn hand_to_app(
    store: &Store,
    p: &Project,
    target: &Target,
    opts: &RunOpts,
    o: &Out,
) -> anyhow::Result<i32> {
    let h = match store.handover_to_app(
        p,
        target,
        opts.task,
        opts.handoff,
        opts.accept_drift,
        opts.no_open,
    )? {
        Ok(h) => h,
        Err(r) => {
            o.print(&r, || crate::out::render_resume(&r));
            return Ok(Error::Drift(String::new()).exit_code());
        }
    };
    let mut v = serde_json::to_value(&h)?;
    v.as_object_mut().map(|m| m.remove("text"));
    o.print(&v, || {
        let mut s = String::new();
        if h.opened {
            s.push_str(&format!("Opened {}.\n", h.display));
        }
        let paste = if h.kind == TargetKind::Ide {
            "Paste it into the IDE's AI chat or agent panel."
        } else {
            "Paste it into a new chat. The chat cannot see your files, so apply the changes it suggests yourself."
        };
        s.push_str(&format!(
            "{} {}\n  saved at: {}\n  session: {} (tracked as '{}')\n\nWhen you stop: `{c} checkpoint --done \"...\" --next \"...\"`, then `{c} switch --to <target>` for the next AI or `{c} verify -- <tests>`.",
            if h.copied_to_clipboard { "The hand-over text is on your clipboard." } else { "The hand-over text is ready." },
            paste, h.file, h.session, h.target, c = brand::CLI_NAME
        ));
        s
    });
    Ok(0)
}
