//! `evyro chat`: talk to a coding agent over ACP from the terminal.
//!
//! The agent works under an Evyro session like `evyro run`, but the
//! conversation is structured: Evyro prints the agent's reply, its plan and
//! each tool call, and asks you before the agent runs a tool. Each line you
//! type is the next message; an empty line or EOF ends the chat.

use crate::out::Out;
use evyro_acp::{pick_option, AcpClient, ChatOpts, Event, Started};
use evyro_core::projects::Project;
use evyro_core::{brand, Error, Store};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum Approve {
    /// Ask in the terminal before each tool call (refuse when there is no terminal).
    Ask,
    /// Allow reading, searching and thinking; ask about everything else.
    Reads,
    /// Allow every tool call.
    All,
    /// Refuse every tool call.
    None,
}

pub struct ChatArgs<'a> {
    pub to: &'a str,
    pub task: Option<&'a str>,
    pub handoff: Option<&'a str>,
    pub accept_drift: bool,
    pub model: Option<&'a str>,
    pub approve: Approve,
    pub no_mcp: bool,
    pub command: Option<Vec<String>>,
    pub prompt: Option<String>,
}

fn status_mark(s: &str) -> &'static str {
    match s {
        "completed" => "✓",
        "failed" => "✗",
        "in_progress" => "…",
        _ => "·",
    }
}

/// Print one event for a person reading the terminal.
fn render(e: &Event, titles: &mut HashMap<String, String>) -> Option<String> {
    match e {
        Event::Update { update, .. } => {
            let kind = update
                .get("sessionUpdate")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let s = |k: &str| {
                update
                    .get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            };
            match kind {
                "agent_message_chunk" => update
                    .pointer("/content/text")
                    .and_then(|t| t.as_str())
                    .map(String::from),
                "tool_call" => {
                    let id = s("toolCallId");
                    let title = s("title");
                    titles.insert(id, title.clone());
                    Some(format!(
                        "\n  {} {} ({})\n",
                        status_mark(&s("status")),
                        title,
                        s("kind")
                    ))
                }
                "tool_call_update" => {
                    let st = s("status");
                    if !matches!(st.as_str(), "completed" | "failed") {
                        return None;
                    }
                    let id = s("toolCallId");
                    let title = update
                        .get("title")
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| titles.get(&id).cloned())
                        .unwrap_or(id);
                    Some(format!("  {} {}\n", status_mark(&st), title))
                }
                "plan" => {
                    let rows: Vec<String> = update
                        .get("entries")
                        .and_then(|v| v.as_array())
                        .map(|es| {
                            es.iter()
                                .map(|x| {
                                    let mark = match x.get("status").and_then(|v| v.as_str()) {
                                        Some("completed") => "[x]",
                                        Some("in_progress") => "[~]",
                                        _ => "[ ]",
                                    };
                                    format!(
                                        "  {mark} {}",
                                        x.get("content").and_then(|v| v.as_str()).unwrap_or("")
                                    )
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    Some(format!("\n  Plan\n{}\n", rows.join("\n")))
                }
                _ => None,
            }
        }
        Event::FileWrite { path, bytes } => Some(format!("  ✎ wrote {path} ({bytes} bytes)\n")),
        Event::FileRefused { path, reason } => Some(format!("  ⊘ refused {path}: {reason}\n")),
        Event::Exit(code) => Some(format!(
            "\n[{}] the agent exited ({})\n",
            brand::CLI_NAME,
            code.map_or("signal".into(), |c| format!("code {c}"))
        )),
        _ => None,
    }
}

fn event_json(e: &Event) -> Value {
    match e {
        Event::Update { session_id, update } => {
            json!({"event": "update", "session": session_id, "update": update})
        }
        Event::Permission { request_id, params } => {
            json!({"event": "permission", "id": request_id, "params": params})
        }
        Event::FileRead { path } => json!({"event": "file_read", "path": path}),
        Event::FileWrite { path, bytes } => {
            json!({"event": "file_write", "path": path, "bytes": bytes})
        }
        Event::FileRefused { path, reason } => {
            json!({"event": "file_refused", "path": path, "reason": reason})
        }
        Event::Stderr(l) => json!({"event": "stderr", "line": l}),
        Event::Exit(c) => json!({"event": "exit", "code": c}),
    }
}

/// Decide a permission request by policy, asking in the terminal when needed.
fn decide(params: &Value, approve: Approve, interactive: bool) -> Option<String> {
    let kind = params
        .pointer("/toolCall/kind")
        .and_then(|v| v.as_str())
        .unwrap_or("other");
    let title = params
        .pointer("/toolCall/title")
        .and_then(|v| v.as_str())
        .unwrap_or("a tool call");
    let read_only = matches!(kind, "read" | "search" | "think");
    let auto = match approve {
        Approve::All => Some(true),
        Approve::None => Some(false),
        Approve::Reads if read_only => Some(true),
        _ => None,
    };
    if let Some(allow) = auto {
        return pick_option(params, allow);
    }
    if !interactive {
        eprintln!(
            "[{}] refused \"{title}\": no terminal to ask in (use --approve to decide ahead)",
            brand::CLI_NAME
        );
        return pick_option(params, false);
    }
    let opts: Vec<(String, String)> = params
        .get("options")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|o| {
                    Some((
                        o.get("optionId")?.as_str()?.to_string(),
                        o.get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .to_string(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    let menu: Vec<String> = opts
        .iter()
        .enumerate()
        .map(|(i, (_, n))| format!("[{}] {n}", i + 1))
        .collect();
    eprint!(
        "\n? The agent wants to: {title} ({kind})\n  {}  > ",
        menu.join("  ")
    );
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return None;
    }
    let pick = line
        .trim()
        .parse::<usize>()
        .ok()
        .and_then(|n| opts.get(n.wrapping_sub(1)));
    match pick {
        Some((id, _)) => Some(id.clone()),
        None => pick_option(params, false),
    }
}

pub fn run_chat(store: &Store, p: &Project, a: ChatArgs, o: &Out) -> anyhow::Result<i32> {
    let interactive = std::io::stdin().is_terminal();
    let slot: Arc<Mutex<Option<AcpClient>>> = Arc::new(Mutex::new(None));
    let titles = Arc::new(Mutex::new(HashMap::new()));
    let (json_out, quiet, approve) = (o.json, o.quiet, a.approve);
    let s2 = slot.clone();
    // Set when Evyro itself stops the agent, so that exit is not reported as news.
    let closing = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let closing2 = closing.clone();
    let on_event: evyro_acp::OnEvent = Arc::new(move |e: Event| {
        if matches!(e, Event::Exit(_)) && closing2.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        if json_out {
            println!("{}", event_json(&e));
        } else if let Some(t) = render(&e, &mut titles.lock().unwrap()) {
            print!("{t}");
            let _ = std::io::stdout().flush();
        } else if let Event::Stderr(l) = &e {
            if !quiet && std::env::var_os("EVYRO_DEBUG").is_some() {
                eprintln!("  agent: {l}");
            }
        }
        if let Event::Permission { request_id, params } = e {
            let client = s2.lock().unwrap().clone();
            // Ask on a separate thread so the agent's other messages keep flowing.
            std::thread::spawn(move || {
                let choice = decide(&params, approve, interactive);
                if let Some(c) = client {
                    let _ = c.answer_permission(&request_id, choice.as_deref());
                }
            });
        }
    });
    let command = a.command.and_then(|c| {
        let mut it = c.into_iter();
        it.next().map(|p| (PathBuf::from(p), it.collect()))
    });
    let started = evyro_acp::start(
        store,
        p,
        ChatOpts {
            target: a.to,
            task: a.task,
            handoff: a.handoff,
            accept_drift: a.accept_drift,
            model: a.model,
            evyro_mcp: if a.no_mcp {
                None
            } else {
                std::env::current_exe().ok()
            },
            command,
        },
        on_event,
    )?;
    let chat = match started {
        Started::Chat(c) => c,
        Started::Drift(r) => {
            o.print(&r, || crate::out::render_resume(&r));
            return Ok(Error::Drift(String::new()).exit_code());
        }
    };
    *slot.lock().unwrap() = Some(chat.client.clone());
    let agent_name = chat
        .agent_info
        .pointer("/agentInfo/title")
        .or_else(|| chat.agent_info.pointer("/agentInfo/name"))
        .and_then(|v| v.as_str())
        .unwrap_or(&chat.display_name)
        .to_string();
    if !quiet && !json_out {
        eprintln!(
            "[{}] chatting with {agent_name} over ACP (session {}). Type a message and press Enter; an empty line ends the chat.",
            brand::CLI_NAME,
            chat.session.id
        );
    }

    let mut message = match a.prompt {
        Some(p) => format!("{}\n\n{p}", chat.first_prompt),
        None => chat.first_prompt.clone(),
    };
    let mut reasons = Vec::new();
    let stdin = std::io::stdin();
    loop {
        match chat.send(&message) {
            Ok(r) => {
                if !json_out && !quiet {
                    println!("\n[{}] turn ended: {r}", brand::CLI_NAME);
                }
                reasons.push(r);
                let _ = chat.turn_checkpoint(store);
            }
            Err(e) => {
                eprintln!("[{}] {e}", brand::CLI_NAME);
                break;
            }
        }
        if !chat.client.is_alive() {
            break;
        }
        if interactive && !json_out {
            print!("you> ");
            let _ = std::io::stdout().flush();
        }
        let mut line = String::new();
        if stdin.lock().read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() || line.trim() == "/exit" {
            break;
        }
        message = line.to_string();
    }
    closing.store(true, std::sync::atomic::Ordering::SeqCst);
    chat.finish(store)?;
    let s = store.get_session(&chat.session.id)?;
    let summary = json!({"event": "done", "session": s.id, "status": s.status, "turns": reasons.len(), "stop_reasons": reasons});
    if json_out {
        println!("{summary}");
    } else if !quiet {
        eprintln!(
            "[{}] chat closed after {} turn(s); progress checkpointed. Continue later with `evyro chat --to <agent>` or `evyro switch --to <target>`.",
            brand::CLI_NAME,
            reasons.len()
        );
    }
    Ok(
        if s.status == evyro_core::sessions::SessionStatus::Completed {
            0
        } else {
            1
        },
    )
}

/// `evyro chat --list`: which agents can chat over ACP here.
pub fn list(o: &Out) {
    let st = evyro_acp::agent_status();
    o.print(&st, || {
        let mut s = String::from("Agents that can chat over ACP\n");
        for a in &st {
            match &a.command {
                Some(c) => s.push_str(&format!(
                    "  [ready] {:<12} {}  ({c})\n",
                    a.harness, a.display_name
                )),
                None => s.push_str(&format!(
                    "  [setup] {:<12} {}  install: {}\n",
                    a.harness, a.display_name, a.install
                )),
            }
        }
        s.push_str("\nStart one with `evyro chat --to <agent> --task T-1`.");
        s
    });
}
