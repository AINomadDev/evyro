//! Evyro desktop: a Tauri 2 shell over `evyro-core`.
//!
//! The window is a thin client. Every command opens the same SQLite store the
//! CLI and MCP server use, so the three always agree. Commands run on a
//! blocking thread so Git and test runs never freeze the window.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use evyro_core::comparison::ComparisonStagePlan;
use evyro_core::handoff::HandoffRequest;
use evyro_core::init::InitOptions;
use evyro_core::memory::NewMemory;
use evyro_core::projects::Commands;
use evyro_core::targets::{self, Target};
use evyro_core::tasks::{Actor, NewTask, TaskStatus, TaskUpdate};
use evyro_core::{adapters, backup, brand, doctor, git, Error, Store};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

mod chat;
mod remote;
mod review;
mod search;
mod skills;
mod threads;
mod tray;
mod updater;
mod usage;

type CmdResult = Result<Value, Value>;

fn err(e: Error) -> Value {
    json!({"code": e.code(), "message": e.to_string()})
}

/// Run `f` with a fresh store on a blocking thread.
async fn with_store<F>(f: F) -> CmdResult
where
    F: FnOnce(&Store) -> evyro_core::Result<Value> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        let store = Store::open_default().map_err(err)?;
        f(&store).map_err(err)
    })
    .await
    .map_err(|e| json!({"code": "internal", "message": e.to_string()}))?
}

fn human() -> Actor {
    Actor::Human {
        override_reason: None,
    }
}

#[tauri::command]
async fn app_info() -> CmdResult {
    with_store(|s| {
        Ok(json!({
            "product": brand::PRODUCT_NAME,
            "version": env!("CARGO_PKG_VERSION"),
            "data_dir": s.paths.data_dir.display().to_string(),
            "schema_version": s.schema_version()?,
            "git": git::version().ok(),
        }))
    })
    .await
}

#[tauri::command]
async fn list_projects(include_archived: Option<bool>) -> CmdResult {
    with_store(move |s| {
        Ok(json!(
            s.list_projects(include_archived.unwrap_or(false), None)?
        ))
    })
    .await
}

#[tauri::command]
async fn add_project(path: String) -> CmdResult {
    with_store(move |s| {
        let r = evyro_core::init::init_project(
            s,
            &PathBuf::from(path),
            &InitOptions {
                name: None,
                append_agent_files: false,
                agent_files: true,
            },
        )?;
        Ok(json!(r))
    })
    .await
}

#[tauri::command]
async fn set_project_archived(project_id: String, archived: bool) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.set_project_flags(
            &project_id,
            None,
            Some(archived),
            None
        )?))
    })
    .await
}

/// Everything the dashboard shows, in one call.
#[tauri::command]
async fn overview(project_id: String) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let p = s.sync_project_file(p)?;
        // Close out sessions whose process is gone, like `evyro status`.
        let recovered = s.recover_stale_sessions(&p.id, false)?;
        let active = s.active_task(&p.id)?;
        let owner = match &active {
            Some(t) => s.active_session_for_task(&t.id)?,
            None => None,
        };
        let git = git::snapshot(&p.root()).ok();
        let health = s.project_health(&p.id)?;
        let pending = s.pending_handoff(&p.id, None)?;
        let sessions = s.list_sessions(&p.id, true)?;
        let events = s.list_events(&p.id, None, 25)?;
        let latest = match &active {
            Some(t) => s.latest_checkpoint(&p.id, Some(&t.id))?,
            None => None,
        };
        Ok(json!({
            "project": p,
            "active_task": active,
            "owner": owner,
            "git": git,
            "health": health,
            "pending_handoff": pending,
            "active_sessions": sessions,
            "events": events,
            "latest_checkpoint": latest,
            "recovered": recovered,
        }))
    })
    .await
}

#[tauri::command]
async fn list_tasks(project_id: String, include_closed: Option<bool>) -> CmdResult {
    with_store(move |s| {
        let ts = s.list_tasks(&project_id, None, include_closed.unwrap_or(true))?;
        let mut out = Vec::new();
        for t in ts {
            let criteria = s.criteria(&t)?;
            let met = criteria.iter().filter(|c| c.satisfied()).count();
            let owner = s.active_session_for_task(&t.id)?;
            let blockers = s.open_blockers(&t.id)?;
            out.push(json!({
                "task": t,
                "criteria_met": met,
                "criteria_total": criteria.len(),
                "owner": owner.map(|o| o.harness),
                "blocked_by": blockers.iter().map(|b| b.key.clone()).collect::<Vec<_>>(),
            }));
        }
        Ok(json!(out))
    })
    .await
}

#[tauri::command]
async fn comparison_list(project_id: String, task: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.comparisons(&project_id, &task)?))).await
}

#[tauri::command]
async fn comparison_create(project_id: String, task: String, targets: [String; 2]) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.create_comparison(
            &project_id,
            &task,
            [&targets[0], &targets[1]]
        )?))
    })
    .await
}

#[tauri::command]
async fn comparison_results(project_id: String, task: String, id: String) -> CmdResult {
    with_store(move |s| s.comparison_results(&project_id, &task, &id)).await
}

#[tauri::command]
async fn comparison_prefer(
    project_id: String,
    task: String,
    id: String,
    candidate: String,
) -> CmdResult {
    with_store(move |s| {
        s.prefer_comparison(&project_id, &task, &id, &candidate)?;
        Ok(Value::Null)
    })
    .await
}

#[tauri::command]
async fn comparison_preview_stage(project_id: String, task: String, id: String) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.preview_comparison_stage(
            &project_id,
            &task,
            &id
        )?))
    })
    .await
}

#[tauri::command]
async fn comparison_stage(
    project_id: String,
    task: String,
    id: String,
    plan: ComparisonStagePlan,
) -> CmdResult {
    with_store(move |s| {
        s.stage_comparison(&project_id, &task, &id, &plan)?;
        Ok(Value::Null)
    })
    .await
}

#[tauri::command]
async fn task_detail(project_id: String, task: String) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        Ok(json!({
            "task": t,
            "criteria": s.criteria(&t)?,
            "dependencies": s.dependencies(&t.id)?,
            "blocked_by": s.open_blockers(&t.id)?.iter().map(|b| b.key.clone()).collect::<Vec<_>>(),
            "owner": s.active_session_for_task(&t.id)?,
            "checkpoints": s.list_checkpoints(&project_id, Some(&t.id), 10)?,
            "verifications": s.list_verifications(&project_id, Some(&t.id))?,
            "comments": s.comments(&t.id)?,
            "pending_handoff": s.pending_handoff_for_task(&t.id)?,
            "events": s.list_events(&project_id, Some(&t.id), 30)?,
            "allowed_next": TaskStatus::ALL.iter().filter(|to| t.status.can_transition_to(**to)).map(|x| x.as_str()).collect::<Vec<_>>(),
        }))
    })
    .await
}

#[derive(Deserialize)]
struct CreateTaskArgs {
    title: String,
    #[serde(default)]
    description: String,
    kind: Option<String>,
    priority: Option<i64>,
    #[serde(default)]
    acceptance: Vec<String>,
    #[serde(default)]
    ready: bool,
}

#[tauri::command]
async fn create_task(project_id: String, input: CreateTaskArgs) -> CmdResult {
    with_store(move |s| {
        let acceptance = input
            .acceptance
            .into_iter()
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();
        let t = s.create_task(
            &project_id,
            NewTask {
                title: input.title,
                description: input.description,
                kind: input.kind,
                priority: input.priority,
                acceptance,
                status: input.ready.then_some(TaskStatus::Ready),
                ..Default::default()
            },
        )?;
        Ok(json!(t))
    })
    .await
}

#[tauri::command]
async fn set_task_status(
    project_id: String,
    task: String,
    status: String,
    override_reason: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        let to: TaskStatus = status.parse()?;
        let actor = Actor::Human {
            override_reason: override_reason.filter(|r| !r.trim().is_empty()),
        };
        Ok(json!(s.update_task(
            &t.id,
            TaskUpdate {
                status: Some(to),
                ..Default::default()
            },
            &actor
        )?))
    })
    .await
}

#[tauri::command]
async fn add_criterion(project_id: String, task: String, text: String) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        let text = text.trim().to_string();
        if text.is_empty() {
            return Err(Error::Invalid("criterion text is empty".into()));
        }
        Ok(json!(s.update_task(
            &t.id,
            TaskUpdate {
                add_acceptance: vec![text],
                ..Default::default()
            },
            &human()
        )?))
    })
    .await
}

#[tauri::command]
async fn check_criterion(
    project_id: String,
    task: String,
    index: usize,
    met: bool,
    note: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        Ok(json!(s.check_criterion(
            &t.id,
            index,
            met,
            note.as_deref().unwrap_or(""),
            None,
            &human()
        )?))
    })
    .await
}

#[tauri::command]
async fn add_comment(project_id: String, task: String, body: String) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        let body = evyro_core::redact::redact(&body).0;
        Ok(json!(s.add_comment(&t.id, "human", &body)?))
    })
    .await
}

/// Runs one of the project's configured commands. Arbitrary commands are not
/// accepted from the window, the same rule the MCP server follows.
#[tauri::command]
async fn run_verification(
    project_id: String,
    task: Option<String>,
    kind: String,
    index: Option<usize>,
) -> CmdResult {
    with_store(move |s| {
        let p = s.sync_project_file(s.get_project(&project_id)?)?;
        let list = match kind.as_str() {
            "test" => &p.commands.test,
            "lint" => &p.commands.lint,
            "build" => &p.commands.build,
            _ => return Err(Error::Invalid("kind must be test, lint or build".into())),
        };
        let argv = list.get(index.unwrap_or(0)).cloned().ok_or_else(|| {
            Error::Invalid(format!("no {kind} command configured; add one in Settings"))
        })?;
        let t = match &task {
            Some(r) => Some(s.resolve_task(&p.id, r)?),
            None => None,
        };
        let cwd = match &t {
            Some(t) => s.task_root(t)?,
            None => p.root(),
        };
        let r = s.run_verification(&p.id, t.as_ref().map(|t| t.id.as_str()), None, &argv, &cwd)?;
        Ok(json!({"receipt": r.receipt, "output_tail": r.output_tail}))
    })
    .await
}

#[tauri::command]
async fn list_sessions(project_id: String, active_only: Option<bool>) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.list_sessions(
            &project_id,
            active_only.unwrap_or(false)
        )?))
    })
    .await
}

#[tauri::command]
async fn recover_sessions(project_id: String, include_unknown: bool) -> CmdResult {
    with_store(move |s| {
        Ok(json!(
            s.recover_stale_sessions(&project_id, include_unknown)?
        ))
    })
    .await
}

/// Free the tasks of one session whose agent is gone (see `recover_session`).
#[tauri::command]
async fn release_session(
    project_id: String,
    session_id: String,
    confirmed: Option<bool>,
) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.recover_session(
            &project_id,
            &session_id,
            confirmed == Some(true)
        )?))
    })
    .await
}

/// Switch a held task to another AI after the person confirms the usage is spent.
/// Ends the old session, prepares the handoff and stops any process still recorded for it.
#[tauri::command]
async fn switch_owner(
    project_id: String,
    task: String,
    to: String,
    model: Option<String>,
    confirmed: bool,
) -> CmdResult {
    with_store(move |s| {
        let out = s.switch_task_owner(&project_id, &task, &to, model, confirmed)?;
        let stopped = out.from.as_ref().is_some_and(|f| {
            f.pid
                .is_some_and(|pid| evyro_core::sessions::stop_session_process(pid, f.pid_started))
        });
        Ok(json!({"handoff": out.handoff, "from": out.from, "stopped": stopped}))
    })
    .await
}

/// The plan an AI proposed for a task, and whether a person approved it.
#[tauri::command]
async fn plan_get(project_id: String, task: String) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        Ok(json!({"task": t.key, "plan": s.plan(&t.id)?}))
    })
    .await
}

/// A person approves or rejects the waiting plan. There is no equivalent for an AI.
#[tauri::command]
async fn plan_decide(
    project_id: String,
    task: String,
    approve: bool,
    note: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let t = s.resolve_task(&project_id, &task)?;
        Ok(json!(s.decide_plan(&t, approve, note.as_deref())?))
    })
    .await
}

/// The next ready task and a free AI to start it, chosen by Evyro.
#[tauri::command]
async fn next_work(project_id: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.next_work(&project_id, None, None, None)?))).await
}

fn history_status(terms: &threads::Terms, chats: &chat::Chats, removed: Value) -> CmdResult {
    let paths =
        evyro_core::paths::GlobalPaths::resolve().map_err(|e| json!({"message": e.to_string()}))?;
    let keep = evyro_core::retention::Retention::load(&paths);
    Ok(json!({
        "keep": {"threads": keep.threads, "chats": keep.chats},
        "max": evyro_core::retention::MAX_KEEP,
        "saved": {"threads": terms.saved_count(), "chats": chats.host.saved_count()},
        "removed": removed,
    }))
}

/// How many ended threads and chats are saved, and how many are kept.
#[tauri::command]
async fn history_status_cmd(
    terms: tauri::State<'_, threads::Terms>,
    chats: tauri::State<'_, chat::Chats>,
) -> CmdResult {
    history_status(&terms, &chats, json!({"threads": 0, "chats": 0}))
}

/// Change how many ended threads and chats are kept; older ones are deleted now.
#[tauri::command]
async fn history_set(
    terms: tauri::State<'_, threads::Terms>,
    chats: tauri::State<'_, chat::Chats>,
    threads_keep: usize,
    chats_keep: usize,
) -> CmdResult {
    let keep = evyro_core::retention::Retention {
        threads: threads_keep,
        chats: chats_keep,
    };
    let paths =
        evyro_core::paths::GlobalPaths::resolve().map_err(|e| json!({"message": e.to_string()}))?;
    keep.save(&paths)
        .map_err(|e| json!({"code": "invalid_input", "message": e.to_string()}))?;
    let removed = json!({"threads": terms.set_keep(keep.threads), "chats": chats.host.set_retention(keep.chats)});
    history_status(&terms, &chats, removed)
}

/// Delete saved ended threads and chats ("threads", "chats" or "all"). Running ones stay.
#[tauri::command]
async fn history_clear(
    terms: tauri::State<'_, threads::Terms>,
    chats: tauri::State<'_, chat::Chats>,
    what: String,
) -> CmdResult {
    if !["threads", "chats", "all"].contains(&what.as_str()) {
        return Err(json!({"code": "invalid_input", "message": "choose threads, chats or all"}));
    }
    let removed = json!({
        "threads": if what != "chats" { terms.clear_saved() } else { 0 },
        "chats": if what != "threads" { chats.host.clear_saved() } else { 0 },
    });
    history_status(&terms, &chats, removed)
}

#[tauri::command]
async fn list_handoffs(project_id: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.list_handoffs(&project_id, 100)?))).await
}

#[tauri::command]
async fn handoff_detail(handoff_id: String) -> CmdResult {
    with_store(move |s| {
        let h = s.get_handoff(&handoff_id)?;
        let c = evyro_core::contract::load(std::path::Path::new(&h.contract_path))?;
        let integrity = c.verify_integrity(&s.paths.key_path());
        Ok(json!({
            "record": h,
            "markdown": c.to_markdown(),
            "resume_prompt": evyro_core::handoff::resume_prompt(&c),
            "integrity_ok": integrity.is_ok(),
            "integrity_error": integrity.err().map(|e| e.to_string()),
            "contract": c,
        }))
    })
    .await
}

#[tauri::command]
async fn handoff_checkpoint(project_id: String, task: String) -> CmdResult {
    with_store(move |s| {
        let task = s.resolve_task(&project_id, &task)?;
        Ok(json!(s.latest_checkpoint(&project_id, Some(&task.id))?))
    })
    .await
}

#[derive(Deserialize)]
struct PrepareArgs {
    task: Option<String>,
    to: String,
    model: Option<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    completed: Vec<String>,
    #[serde(default)]
    remaining: Vec<String>,
    #[serde(default)]
    next_action: String,
}

#[tauri::command]
async fn prepare_handoff(project_id: String, input: PrepareArgs) -> CmdResult {
    with_store(move |s| {
        let clean = |v: Vec<String>| -> Vec<String> {
            v.into_iter()
                .map(|x| x.trim().to_string())
                .filter(|x| !x.is_empty())
                .collect()
        };
        let remaining = clean(input.remaining);
        let h = s.prepare_handoff(
            &project_id,
            HandoffRequest {
                task: input.task,
                to: input.to,
                to_model: input.model,
                summary: input.summary,
                completed: clean(input.completed),
                remaining: (!remaining.is_empty()).then_some(remaining),
                next_action: input.next_action,
                actor: Some("human".into()),
            },
        )?;
        Ok(json!(h))
    })
    .await
}

/// Accept a handoff for an agent the person will start themselves. The
/// window shows drift and checks; nothing launches from here.
#[tauri::command]
async fn resume_handoff(
    project_id: String,
    handoff_id: String,
    agent: String,
    accept_drift: bool,
) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.resume_handoff(
            &project_id,
            Some(&handoff_id),
            &agent,
            accept_drift,
            None
        )?))
    })
    .await
}

#[tauri::command]
async fn context_packet(
    project_id: String,
    task: Option<String>,
    budget: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let t = match &task {
            Some(r) => Some(s.resolve_task(&p.id, r)?),
            None => s.active_task(&p.id)?,
        };
        let b =
            evyro_core::context::budget_for(budget.as_deref().unwrap_or("large")).unwrap_or(12000);
        let c = s.context_packet(&p, t.as_ref(), b)?;
        let text = c
            .sections
            .iter()
            .filter(|x| x.included)
            .map(|x| x.content.clone())
            .collect::<Vec<_>>()
            .join("\n\n");
        Ok(json!({"packet": c, "text": text}))
    })
    .await
}

#[tauri::command]
async fn list_decisions(project_id: String) -> CmdResult {
    with_store(move |s| Ok(json!(s.list_decisions(&project_id, None)?))).await
}

#[tauri::command]
async fn add_decision(
    project_id: String,
    title: String,
    decision: String,
    rationale: String,
    task: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let t = match task.filter(|t| !t.is_empty()) {
            Some(r) => Some(s.resolve_task(&project_id, &r)?.id),
            None => None,
        };
        Ok(json!(s.record_decision(
            &project_id,
            t.as_deref(),
            &title,
            &decision,
            &rationale,
            "human"
        )?))
    })
    .await
}

#[tauri::command]
async fn search_memory(project_id: String, query: Option<String>) -> CmdResult {
    with_store(move |s| {
        let q = query.filter(|q| !q.trim().is_empty());
        Ok(json!(s.search_memory(
            &project_id,
            q.as_deref(),
            None,
            100
        )?))
    })
    .await
}

#[tauri::command]
async fn add_memory(project_id: String, category: String, content: String) -> CmdResult {
    with_store(move |s| {
        Ok(json!(s.add_memory(
            &project_id,
            NewMemory {
                category,
                content,
                files: vec![],
                task_id: None,
                confidence: None,
                global: false,
            },
            "human"
        )?))
    })
    .await
}

#[tauri::command]
async fn memory_categories() -> CmdResult {
    Ok(json!(evyro_core::memory::CATEGORIES))
}

#[tauri::command]
async fn detect_agents() -> CmdResult {
    tauri::async_runtime::spawn_blocking(|| json!(adapters::detect_all()))
        .await
        .map_err(|e| json!({"code": "internal", "message": e.to_string()}))
}

#[tauri::command]
async fn run_doctor(project_id: Option<String>) -> CmdResult {
    with_store(move |s| {
        let cwd = match project_id {
            Some(id) => s.get_project(&id)?.root(),
            None => std::env::current_dir().unwrap_or_default(),
        };
        Ok(json!(doctor::run(s, &cwd)))
    })
    .await
}

#[tauri::command]
async fn save_commands(project_id: String, commands: Commands) -> CmdResult {
    with_store(move |s| {
        Ok(json!(evyro_core::init::save_project_commands(
            s,
            &project_id,
            &commands
        )?))
    })
    .await
}

#[tauri::command]
async fn backup_now() -> CmdResult {
    with_store(|s| {
        let dir = s.paths.data_dir.join("backups");
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(format!(
            "evyro-{}.db",
            evyro_core::util::now().replace([':', '.'], "-")
        ));
        backup::backup_db(s, &dest)?;
        Ok(json!({"path": dest.display().to_string()}))
    })
    .await
}

// ---------- switch targets ----------

/// Every switch target with its readiness. Checks look for evidence only and
/// never read key values.
#[tauri::command]
async fn list_targets() -> CmdResult {
    with_store(|s| {
        let all = targets::list_targets(&s.paths)?;
        Ok(json!(all
            .into_iter()
            .map(|t| {
                let builtin = t.builtin;
                let mut v = json!(targets::check_target(&t));
                v["builtin"] = json!(builtin);
                v
            })
            .collect::<Vec<_>>()))
    })
    .await
}

/// Choices for the "add target" form.
#[tauri::command]
async fn target_options() -> CmdResult {
    let harnesses: Vec<Value> = adapters::registry()
        .into_iter()
        .map(|a| {
            json!({
                "id": a.id,
                "display_name": a.display_name,
                "providers": targets::harness_providers(a.id),
                "app_login": targets::supports_app_login(a.id),
            })
        })
        .collect();
    Ok(json!({
        "harnesses": harnesses,
        "providers": targets::providers(),
        "ides": targets::ides(),
        "chat_apps": targets::chat_apps(),
    }))
}

#[tauri::command]
async fn add_target(target: Target) -> CmdResult {
    with_store(move |s| Ok(json!(targets::add_target(&s.paths, target)?))).await
}

#[tauri::command]
async fn remove_target(name: String) -> CmdResult {
    with_store(move |s| {
        targets::remove_target(&s.paths, &name)?;
        Ok(json!({"removed": name}))
    })
    .await
}

/// Hand work to an IDE or chat target: save the hand-over text, copy it and
/// open the app. Returns `{handover}` or, when the repository drifted,
/// `{drift}` with nothing changed.
#[tauri::command]
async fn handover_to_app(
    project_id: String,
    target: String,
    task: Option<String>,
    handoff_id: Option<String>,
    accept_drift: bool,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let t = targets::resolve_target(&s.paths, &target)?;
        match s.handover_to_app(
            &p,
            &t,
            task.as_deref(),
            handoff_id.as_deref(),
            accept_drift,
            false,
        )? {
            Ok(h) => {
                let mut v = json!(h);
                v.as_object_mut().map(|m| m.remove("text"));
                Ok(json!({"handover": v}))
            }
            Err(r) => Ok(json!({"drift": r})),
        }
    })
    .await
}

/// Start `evyro run --to <target>` in a new Terminal window (macOS). Agents
/// need a real terminal, so the window never runs them itself.
#[tauri::command]
async fn open_in_terminal(
    project_id: String,
    target: String,
    task: Option<String>,
    handoff_id: Option<String>,
) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let t = targets::resolve_target(&s.paths, &target)?;
        let q = |x: &str| format!("'{}'", x.replace('\'', "'\\''"));
        let mut line = format!(
            "cd {} && {} run --to {}",
            q(&p.root().to_string_lossy()),
            brand::CLI_NAME,
            q(&t.name)
        );
        if let Some(k) = task.as_deref().filter(|k| !k.is_empty()) {
            line.push_str(&format!(" --task {}", q(k)));
        }
        if let Some(h) = handoff_id.as_deref().filter(|h| !h.is_empty()) {
            line.push_str(&format!(" --handoff {}", q(h)));
        }
        if !cfg!(target_os = "macos") {
            return Ok(json!({"opened": false, "command": line}));
        }
        let script = format!(
            "tell application \"Terminal\"\nactivate\ndo script \"{}\"\nend tell",
            line.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let ok = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .status()
            .map(|st| st.success())
            .unwrap_or(false);
        Ok(json!({"opened": ok, "command": line}))
    })
    .await
}

// ---------- folder picker, changes, live events ----------

/// Native folder chooser. Returns the chosen path, or null when cancelled.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> CmdResult {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Choose a Git repository")
            .blocking_pick_folder();
        json!(picked.map(|p| p.to_string()))
    })
    .await
    .map_err(|e| json!({"code": "internal", "message": e.to_string()}))
}

/// Working-tree changes for review: changed files plus the redacted diff of
/// tracked files against HEAD. Sensitive files (.env, keys) are left out.
#[tauri::command]
async fn repo_changes(project_id: String) -> CmdResult {
    with_store(move |s| {
        let p = s.get_project(&project_id)?;
        let root = p.root();
        let files = git::status_files(&root)?;
        let mut raw = git::diff_text(&root, 400_000);
        // New files the agent created, shown as added so they can be reviewed too.
        if !raw.contains("… diff truncated …") {
            raw.push_str(&git::untracked_diff(&root, &files, 200_000));
        }
        let (diff, findings) = evyro_core::redact::redact(&raw);
        Ok(json!({
            "files": files,
            "diff": diff,
            "redactions": findings.len(),
            "truncated": diff.contains("… diff truncated …"),
        }))
    })
    .await
}

/// Events worth a desktop notification, with the text to show.
fn notable(e: &evyro_core::events::Event) -> Option<(String, String)> {
    let p = &e.payload;
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    match e.kind.as_str() {
        "agent.stopped" => {
            let code = p.get("exit_code").and_then(|v| v.as_i64());
            Some((
                if code == Some(0) { "Agent finished".into() } else { "Agent stopped".into() },
                match code {
                    Some(0) => "Progress was checkpointed. Review it, or hand off to the next AI.".into(),
                    Some(c) => format!("Exit code {c}. Progress was checkpointed; you can switch to another AI."),
                    None => "Ended by a signal. Progress was checkpointed; you can switch to another AI.".into(),
                },
            ))
        }
        "handoff.prepared" => Some(("Handoff ready".into(), format!("Ready for {}.", s("to")))),
        "handoff.accepted" => Some((
            "Handoff accepted".into(),
            format!("{} took over the work.", s("agent")),
        )),
        "handoff.reopened" => Some((
            "Handoff reopened".into(),
            "The receiving AI did not start. Pick another one.".into(),
        )),
        "verification.completed" if p.get("passed") == Some(&json!(false)) => Some((
            "Tests failed".into(),
            "Open the task to see the output.".into(),
        )),
        _ => None,
    }
}

/// Watch the database for new events from the CLI, agents and MCP. Each batch
/// goes to the window (so screens refresh at once) and notable ones become
/// desktop notifications while the window is in the background.
fn start_event_watcher(app: tauri::AppHandle) {
    use tauri::{Emitter, Manager};
    use tauri_plugin_notification::NotificationExt;
    std::thread::spawn(move || {
        let store = loop {
            match Store::open_default() {
                Ok(s) => break s,
                Err(_) => std::thread::sleep(std::time::Duration::from_secs(5)),
            }
        };
        let mut last = store.last_event_seq().unwrap_or(0);
        loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            let Ok(evs) = store.events_after(last, 200) else {
                continue;
            };
            let Some(newest) = evs.last() else { continue };
            last = newest.seq;
            let _ = app.emit("evyro-events", &evs);
            let focused = app
                .get_webview_window("main")
                .and_then(|w| w.is_focused().ok())
                .unwrap_or(false);
            if focused {
                continue;
            }
            for e in &evs {
                if let Some((title, body)) = notable(e) {
                    let _ = app.notification().builder().title(title).body(body).show();
                }
            }
        }
    });
}

/// Apps opened from the Dock or Start menu get a short PATH. Take the login
/// shell's PATH once at startup, so agents installed with npm, Homebrew or
/// cargo are found the same way the person's terminal finds them.
fn adopt_login_path() {
    if cfg!(windows) || std::env::var_os("EVYRO_KEEP_PATH").is_some() {
        return;
    }
    let (shell, args) = evyro_term::shell_command(Some("printf '%s' \"$PATH\""));
    let out = std::process::Command::new(shell)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output();
    if let Ok(o) = out {
        let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if o.status.success() && path.contains('/') {
            // The login shell's order first, then anything only the app had.
            let mut dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();
            if let Some(old) = std::env::var_os("PATH") {
                for d in std::env::split_paths(&old) {
                    if !dirs.contains(&d) {
                        dirs.push(d);
                    }
                }
            }
            if let Ok(joined) = std::env::join_paths(dirs) {
                // Single-threaded here: nothing else reads the environment yet.
                std::env::set_var("PATH", joined);
            }
        }
    }
}

fn main() {
    evyro_core::crash::install_panic_hook("Evyro desktop");
    adopt_login_path();
    // Threads are saved under the data folder, so they are still listed
    // (with their output) after the app restarts.
    let terms: threads::Terms =
        std::sync::Arc::new(match evyro_core::paths::GlobalPaths::resolve() {
            Ok(p) => evyro_term::Terminals::with_store(p.data_dir.join("threads"))
                .with_keep(evyro_core::retention::Retention::load(&p).threads)
                .with_redactor(std::sync::Arc::new(|text| {
                    evyro_core::redact::redact(text).0
                })),
            Err(_) => evyro_term::Terminals::new(),
        });
    let chats: chat::Chats = chat::new_chats();
    {
        let terms = terms.clone();
        let chats = chats.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            terms.save_all();
            chats.host.save_all();
        });
    }
    let remote: remote::Remote = Default::default();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            app_info,
            updater::check_update,
            updater::install_update,
            list_projects,
            add_project,
            set_project_archived,
            overview,
            list_tasks,
            task_detail,
            comparison_list,
            comparison_create,
            comparison_results,
            comparison_prefer,
            comparison_preview_stage,
            comparison_stage,
            create_task,
            set_task_status,
            add_criterion,
            check_criterion,
            add_comment,
            run_verification,
            list_sessions,
            recover_sessions,
            release_session,
            switch_owner,
            next_work,
            plan_get,
            plan_decide,
            history_status_cmd,
            history_set,
            history_clear,
            list_handoffs,
            handoff_detail,
            handoff_checkpoint,
            prepare_handoff,
            resume_handoff,
            context_packet,
            list_decisions,
            add_decision,
            search_memory,
            add_memory,
            memory_categories,
            detect_agents,
            run_doctor,
            save_commands,
            backup_now,
            list_targets,
            target_options,
            add_target,
            remove_target,
            handover_to_app,
            open_in_terminal,
            pick_folder,
            repo_changes,
            search::find_files,
            skills::list_skills,
            remote::remote_status,
            remote::remote_tailscale,
            remote::remote_start,
            remote::remote_stop,
            remote::remote_devices,
            remote::remote_pair,
            remote::remote_revoke,
            skills::skill_text,
            skills::set_skill_enabled,
            skills::new_skill,
            skills::import_skills,
            skills::sync_skills,
            skills::pick_skill_folder,
            search::search_text,
            search::find_symbols,
            search::view_file,
            review::list_review_comments,
            review::add_review_comment,
            review::set_review_status,
            review::delete_review_comment,
            review::review_prompt_text,
            review::review_mark_sent,
            chat::acp_agents,
            chat::chat_open,
            chat::chat_list,
            chat::chat_search_saved,
            chat::chat_log,
            chat::chat_send,
            chat::chat_set_model,
            chat::chat_permission,
            chat::chat_cancel,
            chat::chat_close,
            threads::term_open,
            usage::agent_quota,
            usage::opencode_usage,
            threads::term_stop,
            threads::term_list,
            threads::term_write,
            threads::term_resize,
            threads::term_snapshot,
            threads::term_close,
        ])
        .manage(terms.clone())
        .manage(chats.clone())
        .manage(remote.clone())
        .setup({
            let chats = chats.clone();
            move |app| {
                chat::attach(&chats, app.handle().clone());
                start_event_watcher(app.handle().clone());
                // A missing tray (some Linux desktops) is not fatal.
                if let Err(e) = tray::install(app.handle()) {
                    eprintln!("evyro: no tray icon: {e}");
                }
                Ok(())
            }
        })
        .on_window_event({
            let terms = terms.clone();
            let chats = chats.clone();
            move |w, e| tray::on_window_event(w, e, &terms, &chats)
        })
        .build(tauri::generate_context!())
        .expect("error while running Evyro desktop")
        .run(move |app, event| match event {
            // Agents started in the window stop with it.
            tauri::RunEvent::Exit => {
                terms.close_all();
                remote::stop(&remote);
                chat::close_all(&chats);
            }
            // macOS: clicking the Dock icon brings a hidden window back.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => tray::show_main(app),
            _ => {
                let _ = app;
            }
        });
}
