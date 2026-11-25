//! `evyro` CLI. Works without the desktop app; every command supports `--json`.

mod chat;
mod launch;
mod out;

use anyhow::Context as _;
use clap::{Args, CommandFactory, Parser, Subcommand};
use evyro_core::checkpoints::NewCheckpoint;
use evyro_core::handoff::HandoffRequest;
use evyro_core::memory::NewMemory;
use evyro_core::sessions::NewSession;
use evyro_core::tasks::{Actor, NewTask, TaskStatus, TaskUpdate};
use evyro_core::{adapters, backup, brand, git, Error, Store};
use out::Out;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "evyro", version, about = "Evyro: switch agents, keep building.", long_about = None)]
struct Cli {
    /// Machine-readable JSON output.
    #[arg(long, global = true)]
    json: bool,
    /// Suppress non-essential output.
    #[arg(long, short, global = true)]
    quiet: bool,
    /// Print debug details on errors.
    #[arg(long, global = true)]
    debug: bool,
    /// Run as if started in this directory.
    #[arg(short = 'C', long = "dir", global = true)]
    dir: Option<PathBuf>,
    /// Target a registered project by id or alias instead of the current directory.
    #[arg(long, global = true)]
    project: Option<String>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Save reported limits from a Claude Code or Antigravity status-line hook.
    CaptureUsage {
        #[arg(value_parser = ["claude-code", "antigravity-cli"])]
        harness: String,
        #[arg(long, hide = true)]
        output: Option<PathBuf>,
    },
    /// Register this repository and create minimal .evyro scaffolding.
    Init(InitArgs),
    /// Register a repository without scaffolding.
    Register {
        path: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
    },
    /// List registered projects (global tracker).
    Projects {
        #[arg(long)]
        all: bool,
        #[arg(long)]
        search: Option<String>,
    },
    /// Set project alias / archive / favorite.
    Project {
        #[arg(long)]
        alias: Option<String>,
        #[arg(long)]
        archive: Option<bool>,
        #[arg(long)]
        favorite: Option<bool>,
    },
    /// Project status: active task, owner, git state, last checkpoint.
    Status,
    /// Diagnose installation, database, agents and projects.
    Doctor,
    /// List tasks.
    Tasks {
        #[arg(long)]
        ready: bool,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        all: bool,
    },
    /// Create, inspect and update tasks.
    #[command(subcommand)]
    Task(TaskCmd),
    /// Print the budgeted context packet for agents.
    Context {
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value = "small")]
        budget: String,
        #[arg(long)]
        quiet_errors: bool,
    },
    /// Tell Evyro this session is still working. A session that has no process Evyro can watch
    /// counts as gone after 30 minutes without any checkpoint, event or heartbeat.
    Heartbeat {
        /// The task the session holds (default: the session this command runs in, or the
        /// active task)
        #[arg(long)]
        task: Option<String>,
    },
    /// Record a checkpoint (git state captured automatically).
    Checkpoint(CheckpointArgs),
    /// Prepare a signed continuation contract for another agent.
    Handoff(HandoffArgs),
    /// Start the next ready task on a free AI. Nothing to name or set up: Evyro picks the most
    /// important task that can start, an installed AI that is not busy, and gives the task its own
    /// worktree when another AI is already working in the main checkout.
    Work(WorkArgs),
    /// Switch a task to another AI because the current one ran out of usage.
    /// You confirm the usage is used up; Evyro carries the latest checkpoint over, stops the
    /// current AI and starts the new one, so only one AI works on the task at a time.
    /// Use `handoff` to write your own notes or to prepare the contract without launching.
    Switch(SwitchArgs),
    /// Accept a pending handoff as the receiving agent (validates state first).
    Resume {
        #[arg(long)]
        agent: String,
        #[arg(long)]
        handoff: Option<String>,
        #[arg(long)]
        accept_drift: bool,
        #[arg(long)]
        model: Option<String>,
    },
    /// Launch an agent with session tracking (pid, exit checkpoint).
    Run(RunArgs),
    /// Chat with a coding agent over ACP (Agent Client Protocol): its reply,
    /// plan and tool calls in the terminal, and you approve each tool call.
    /// `evyro chat --list` shows which agents can.
    Chat(ChatArgs),
    /// Detected agents and their capabilities.
    Agents,
    /// Switch targets: every AI you can hand work to (agent CLIs, IDEs, chat
    /// apps, API models). `evyro targets` lists them with their setup status.
    Targets {
        #[command(subcommand)]
        cmd: Option<TargetsCmd>,
    },
    /// Model selection support per agent.
    Models,
    /// Agent sessions.
    Sessions {
        #[arg(long)]
        active: bool,
    },
    /// Recover sessions whose process died (quota kill, crash).
    Recover {
        #[arg(long)]
        include_unknown: bool,
    },
    /// Git worktrees.
    #[command(subcommand)]
    Worktrees(WorktreeCmd),
    /// Plan first: an AI proposes a plan, you approve it, then it builds.
    Plan {
        #[command(subcommand)]
        cmd: PlanCmd,
    },
    /// Run a verification command and record a receipt: `evyro verify -- cargo test`.
    Verify {
        #[arg(long)]
        task: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 0)]
        index: usize,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Record a decision.
    /// Record a decision: `evyro decide "<title>" "<decision>" [--why ...]`.
    Decide {
        title: String,
        /// The decision itself (or pass it with --decision).
        decision_text: Option<String>,
        #[arg(long = "decision", conflicts_with = "decision_text")]
        decision: Option<String>,
        #[arg(long, default_value = "")]
        why: String,
        #[arg(long)]
        task: Option<String>,
    },
    /// Decisions for the project.
    Decisions,
    /// Search the code: file names, text or regex, symbols, a file's outline.
    #[command(subcommand)]
    Search(SearchCmd),
    /// Let paired devices (a phone, another computer) follow and approve the
    /// work over HTTP: an API, an event stream, MCP over HTTP and a web companion.
    Serve {
        /// Address to listen on. 127.0.0.1 is this computer only; a Tailscale IP reaches your tailnet.
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        #[arg(long, default_value_t = 7420)]
        port: u16,
        /// Allow an address that is neither this computer nor Tailscale (plain HTTP, not encrypted).
        #[arg(long)]
        allow_lan: bool,
    },
    /// Devices that may use `evyro serve`: pair a new one, list, revoke.
    #[command(subcommand)]
    Remote(RemoteCmd),
    /// Skills: reusable instructions agents load when a task needs them.
    Skills {
        #[command(subcommand)]
        cmd: Option<SkillsCmd>,
    },
    /// Review comments on the working-tree changes: leave them on files and
    /// lines, send the open ones to an agent, resolve them when addressed.
    Review {
        #[command(subcommand)]
        cmd: Option<ReviewCmd>,
    },
    /// Project memory.
    #[command(subcommand)]
    Memory(MemoryCmd),
    /// Handoff history.
    Handoffs,
    /// Evidence-linked project health.
    Health,
    /// Export this project's records (redacted, portable JSON).
    Export {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Import a project export into this machine's database.
    Import {
        file: PathBuf,
        #[arg(long)]
        root: Option<PathBuf>,
    },
    /// Snapshot the global database.
    Backup { file: PathBuf },
    /// Replace the global database with a backup (keeps the old one).
    Restore {
        file: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Event log (timeline).
    Logs {
        #[arg(long)]
        task: Option<String>,
        #[arg(long, default_value_t = 50)]
        limit: usize,
    },
    /// Run the MCP server on stdio for this project.
    Mcp {
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        agent: Option<String>,
    },
    /// Integration helpers (MCP registration, hooks).
    Integrate {
        agent: String,
        #[arg(long)]
        hooks: bool,
        /// IDEs: write the project MCP config (.cursor/mcp.json, .vscode/mcp.json) if it does not exist.
        #[arg(long)]
        write: bool,
    },
    /// Shell completions.
    Completions { shell: clap_complete::Shell },
    /// Version information.
    Version,
    /// Write a bug report: versions, system, doctor checks and recent crash
    /// logs, with secrets redacted. Read it before you share it.
    BugReport {
        /// Write to this file instead of printing.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Args)]
struct InitArgs {
    #[arg(long)]
    name: Option<String>,
    /// Do not create AGENTS.md / CLAUDE.md / GEMINI.md entry files.
    #[arg(long)]
    no_agent_files: bool,
    /// Append the Evyro section to existing agent files that lack it.
    #[arg(long)]
    append_agent_files: bool,
}

#[derive(Subcommand)]
enum TaskCmd {
    /// Split a task into pieces other AIs can take. Reads a JSON list of pieces
    /// (`title`, optional `description`, `files`, `acceptance`, `after`) from a file or `-`.
    /// Pieces that share files run one after the other. Nothing is started.
    Split {
        /// The task to split.
        task: String,
        /// The pieces as JSON, from a file or `-` for standard input.
        #[arg(long)]
        file: PathBuf,
    },
    /// Approve the pieces an AI wrote for a task, so workers may start them. A person has to
    /// run it at a terminal and type the task key.
    ApprovePieces {
        /// The task whose pieces to approve.
        task: String,
    },
    /// Add a task.
    Add {
        title: String,
        #[arg(long, default_value = "")]
        desc: String,
        /// epic, feature, task, subtask, bug, tech_debt, spike, docs, maintenance, integration, security or release.
        #[arg(long)]
        kind: Option<String>,
        /// 0 (critical) to 4 (trivial); default 2.
        #[arg(long)]
        priority: Option<i64>,
        /// An acceptance criterion; repeat for several.
        #[arg(long = "accept")]
        acceptance: Vec<String>,
        /// A constraint the AI must respect; repeat for several.
        #[arg(long = "constraint")]
        constraints: Vec<String>,
        /// A file or folder the task changes. Tasks with overlapping files never run side by side.
        #[arg(long = "file")]
        files: Vec<String>,
        /// Mark the task as needing a person (a phone, a certificate, a sign-in) and say why.
        /// `evyro work` and the desktop never hand such a task to an AI.
        #[arg(long)]
        needs_person: Option<String>,
        /// The epic or task this one belongs to.
        #[arg(long)]
        parent: Option<String>,
        /// Start in the ready state instead of the backlog.
        #[arg(long)]
        ready: bool,
    },
    /// Show a task with dependencies, checkpoints and receipts.
    Show { task: String },
    /// Mark acceptance criterion <n> (1-based) met. `verified` needs all of them.
    Check {
        task: String,
        index: usize,
        #[arg(long, default_value = "")]
        note: String,
        /// Link a passing verification receipt as evidence.
        #[arg(long)]
        receipt: Option<String>,
        /// Mark it not met instead.
        #[arg(long)]
        unmet: bool,
    },
    /// Claim a task for an agent (starts a session, takes the lease).
    Start {
        task: String,
        #[arg(long)]
        agent: String,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        provider: Option<String>,
        /// If the task has an open handoff with drift, continue anyway.
        #[arg(long)]
        accept_drift: bool,
    },
    /// Update a task.
    Update {
        task: String,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        desc: Option<String>,
        #[arg(long)]
        priority: Option<i64>,
        #[arg(long = "accept")]
        acceptance: Vec<String>,
        #[arg(long = "file")]
        files: Vec<String>,
        /// Mark the task as needing a person (a phone, a certificate, a sign-in) and say why.
        #[arg(long)]
        needs_person: Option<String>,
        #[arg(long)]
        comment: Option<String>,
        /// Acting agent (for provenance). Omit when a human runs the command.
        #[arg(long)]
        agent: Option<String>,
        /// Human-only: mark verified without a receipt, recording the reason.
        #[arg(long)]
        human_override: Option<String>,
    },
    /// Add a dependency: TASK depends on --on.
    Dep {
        task: String,
        #[arg(long)]
        on: String,
        #[arg(long)]
        related: bool,
    },
}

#[derive(Args, Clone)]
struct CheckpointArgs {
    #[arg(long)]
    task: Option<String>,
    #[arg(long, default_value = "")]
    summary: String,
    #[arg(long = "done")]
    done: Vec<String>,
    #[arg(long = "remaining")]
    remaining: Vec<String>,
    #[arg(long = "blocker")]
    blockers: Vec<String>,
    #[arg(long, default_value = "")]
    next: String,
    #[arg(long)]
    trigger: Option<String>,
    /// Hook mode: skip when nothing changed, never fail when no task is active.
    #[arg(long)]
    auto: bool,
}

#[derive(Args, Clone)]
struct HandoffArgs {
    #[arg(long)]
    to: String,
    #[arg(long)]
    task: Option<String>,
    #[arg(long, default_value = "")]
    summary: String,
    #[arg(long = "done")]
    done: Vec<String>,
    #[arg(long = "remaining")]
    remaining: Vec<String>,
    #[arg(long, default_value = "")]
    next: String,
    #[arg(long)]
    model: Option<String>,
    /// Launch the target agent after preparing the handoff.
    #[arg(long)]
    launch: bool,
    /// Launch in headless (non-interactive) mode.
    #[arg(long)]
    headless: bool,
    /// Who is preparing the handoff (default: the owning session's agent, else human).
    #[arg(long)]
    from: Option<String>,
    /// IDE/chat targets: prepare the hand-over without opening apps or the clipboard.
    #[arg(long)]
    no_open: bool,
}

#[derive(Args)]
struct WorkArgs {
    /// The AI to use (default: an installed AI that is not working on anything).
    #[arg(long)]
    agent: Option<String>,
    /// The task to start (default: the most important ready task).
    #[arg(long)]
    task: Option<String>,
    /// Pick only among the ready pieces of this task (see `evyro task split`).
    #[arg(long, conflicts_with = "task")]
    under: Option<String>,
    /// Launch in headless (non-interactive) mode.
    #[arg(long)]
    headless: bool,
    /// Show what would start without starting it.
    #[arg(long)]
    dry_run: bool,
}

#[derive(Args)]
struct SwitchArgs {
    /// The AI to continue with (a harness, or a model on the same harness with --model).
    #[arg(long)]
    to: String,
    /// The task to switch (default: the active task).
    #[arg(long)]
    task: Option<String>,
    /// The model to run on the new AI (default: its own).
    #[arg(long)]
    model: Option<String>,
    /// Confirm that the current AI's usage has run out or is about to.
    #[arg(long, short = 'y')]
    yes: bool,
    /// Launch in headless (non-interactive) mode.
    #[arg(long)]
    headless: bool,
    /// Prepare the switch and stop the old AI, but do not start the new one.
    #[arg(long)]
    no_launch: bool,
    /// IDE/chat targets: prepare the hand-over without opening apps or the clipboard.
    #[arg(long)]
    no_open: bool,
}

#[derive(Args)]
struct RunArgs {
    /// Switch target: a harness (claude, codex…), an IDE (cursor-ide, vscode…),
    /// a chat app (chatgpt, claude-app…) or one of your own targets. See `evyro targets`.
    #[arg(long, alias = "to")]
    agent: String,
    #[arg(long)]
    task: Option<String>,
    #[arg(long)]
    headless: bool,
    #[arg(long)]
    prompt: Option<String>,
    #[arg(long)]
    model: Option<String>,
    #[arg(long)]
    accept_drift: bool,
    /// IDE/chat targets: prepare the hand-over without opening apps or the clipboard.
    #[arg(long)]
    no_open: bool,
    /// Continue this handoff, even one prepared for a different AI.
    #[arg(long)]
    handoff: Option<String>,
    /// Plan first: the AI proposes a plan for the task and waits for you to approve it
    /// (`evyro plan approve`). Claude Code and Codex start read-only; other AIs are asked.
    #[arg(long)]
    plan_first: bool,
}

#[derive(Args)]
struct ChatArgs {
    /// The coding agent or API target to chat with (see `evyro chat --list`).
    #[arg(long, alias = "agent", required_unless_present = "list")]
    to: Option<String>,
    /// The task to work on (default: a handoff waiting for this agent, or the active task).
    #[arg(long)]
    task: Option<String>,
    /// Continue this handoff, even one prepared for a different AI.
    #[arg(long)]
    handoff: Option<String>,
    /// Continue a handoff even though the repository changed since it was written.
    #[arg(long)]
    accept_drift: bool,
    /// Model for API targets (passed to the agent's environment as `evyro run` does).
    #[arg(long)]
    model: Option<String>,
    /// Which tool calls to allow without asking.
    #[arg(long, value_enum, default_value = "ask")]
    approve: chat::Approve,
    /// Do not offer Evyro's MCP server to the agent.
    #[arg(long)]
    no_mcp: bool,
    /// Start this ACP command instead of the agent's usual one (program and arguments).
    #[arg(long, num_args = 1.., allow_hyphen_values = true, value_delimiter = ' ')]
    command: Option<Vec<String>>,
    /// List the agents that can chat over ACP and how to install the missing ones.
    #[arg(long)]
    list: bool,
    /// The first message (after Evyro's task or handoff context).
    prompt: Option<String>,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
enum TargetsCmd {
    /// List every target with its setup status (the default).
    List {
        /// Show only targets that are ready to use.
        #[arg(long)]
        ready: bool,
    },
    /// Show one target's checks and next step.
    Check { name: String },
    /// Add or replace your own target. Keys are never stored: give the
    /// environment variable name (--key-env) or a macOS Keychain service (--key-keychain).
    Add {
        name: String,
        /// harness | api | ide | chat
        #[arg(long)]
        kind: String,
        /// Agent CLI for harness/api targets (api defaults to opencode).
        #[arg(long)]
        harness: Option<String>,
        /// IDE id (ide targets) or chat app id (chat targets).
        #[arg(long)]
        app: Option<String>,
        /// auto | app_login | api_key | local
        #[arg(long, default_value = "auto")]
        auth: String,
        #[arg(long)]
        provider: Option<String>,
        #[arg(long)]
        model: Option<String>,
        #[arg(long)]
        key_env: Option<String>,
        #[arg(long)]
        key_keychain: Option<String>,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long, default_value = "")]
        description: String,
    },
    /// Remove one of your own targets.
    Remove { name: String },
    /// Providers you can use with API targets.
    Providers,
}

#[derive(Subcommand)]
enum RemoteCmd {
    /// Make a one-time pairing code for a new device.
    Pair {
        /// A name you will recognise, like "My phone".
        #[arg(long)]
        name: String,
        /// read: look only; approve: also answer agents' permission requests and comment; control: also start and message chats.
        #[arg(long, default_value = "approve", value_parser = ["read", "approve", "control"])]
        scope: String,
        /// Minutes the code stays valid (1 to 60).
        #[arg(long, default_value_t = 10)]
        minutes: i64,
    },
    /// Paired devices and waiting codes.
    Devices,
    /// Revoke a device by id or name. Its token stops working at once.
    Revoke { device: String },
}

#[derive(Subcommand)]
enum SkillsCmd {
    /// Every skill here, with its source, version and whether it is on. The default.
    List {
        /// Only the skills that fit this text, such as a task title.
        #[arg(long = "for")]
        for_text: Option<String>,
    },
    /// Print one skill's full text.
    Show { name: String },
    /// Start a new skill from a template (in this project, or --global).
    New {
        name: String,
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)]
        global: bool,
    },
    /// Turn skills on (for this project, or --global for every project).
    Enable {
        #[arg(required = true)]
        names: Vec<String>,
        #[arg(long)]
        global: bool,
    },
    /// Turn skills off (for this project, or --global for every project).
    Disable {
        #[arg(required = true)]
        names: Vec<String>,
        #[arg(long)]
        global: bool,
    },
    /// Forget this project's on/off choice, so the global one applies.
    Reset {
        #[arg(required = true)]
        names: Vec<String>,
    },
    /// List problems that stop skills from being used.
    Check,
    /// Copy skills in from a SKILL.md, a skill folder or a folder of skills.
    Import {
        path: PathBuf,
        #[arg(long)]
        global: bool,
        /// Replace an installed skill even when its version is the same or newer.
        #[arg(long)]
        force: bool,
    },
    /// Write skills out as <dir>/<name>/SKILL.md (all usable skills if none named).
    Export {
        names: Vec<String>,
        #[arg(long)]
        out: PathBuf,
    },
    /// Write skill references where an agent reads its instructions.
    Sync {
        /// claude-code, codex, gemini, copilot, opencode, goose, cursor, amp or aider.
        #[arg(long = "for")]
        harness: String,
        /// Only these skills (default: every skill that is on).
        names: Vec<String>,
        /// Add the skills list to an instruction file that has no Evyro section yet.
        #[arg(long)]
        append: bool,
        /// Replace skill files that someone else wrote.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum SearchCmd {
    /// Fuzzy-find files by name: `evyro search files hndrs` finds handoff.rs.
    Files {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Find text in files (plain text by default, `--regex` for a pattern).
    Text {
        /// What to look for.
        pattern: String,
        /// Treat the pattern as a regular expression.
        #[arg(long)]
        regex: bool,
        /// Match case exactly.
        #[arg(long)]
        case: bool,
        /// Only files whose path contains this.
        #[arg(long)]
        path: Option<String>,
        /// Stop after this many matching lines.
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    /// Find functions, types and other symbols by name across the repository.
    Symbols {
        query: String,
        #[arg(long, default_value_t = 30)]
        limit: usize,
    },
    /// List the symbols in one file.
    Outline { path: String },
}

#[derive(Subcommand)]
enum ReviewCmd {
    /// Comments still waiting for an agent (open or sent). The default.
    List {
        /// Include resolved comments.
        #[arg(long)]
        all: bool,
    },
    /// Comment on a file or a line: `evyro review comment src/lib.rs:42 "Handle the empty case"`.
    Comment {
        /// `path` or `path:line`, relative to the repository root.
        location: String,
        text: String,
        #[arg(long)]
        task: Option<String>,
    },
    /// Mark comments as addressed.
    Resolve { ids: Vec<String> },
    /// Open a resolved or sent comment again.
    Reopen { id: String },
    /// Remove a comment.
    Delete { id: String },
    /// Print the open comments as one prompt to paste into any AI.
    Prompt {
        /// Mark the printed comments as sent.
        #[arg(long)]
        mark_sent: bool,
    },
}

#[derive(Subcommand)]
enum PlanCmd {
    /// Show the plan for a task and whether it is approved.
    Show { task: String },
    /// Propose a plan for a task. An AI in plan mode does this; the plan then waits for you.
    Set {
        task: String,
        /// The plan text.
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        /// Read the plan from a file, or from standard input with `-`.
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Approve the plan that is waiting. A person has to run it at a terminal and type the task
    /// key; it refuses when run by an AI or from a script.
    Approve {
        task: String,
        #[arg(long)]
        note: Option<String>,
    },
    /// Reject the plan that is waiting, with the reason the AI should read.
    Reject {
        task: String,
        #[arg(long)]
        note: Option<String>,
    },
}

#[derive(Subcommand)]
enum WorktreeCmd {
    List,
    /// Create a worktree + branch for a task.
    Add {
        task: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    /// Tidy finished work: remove the worktrees of finished tasks that are clean and merged,
    /// and keep any with uncommitted or unmerged work.
    Prune {
        /// Show what would be removed without removing it.
        #[arg(long)]
        dry_run: bool,
    },
    /// Remove a worktree (refuses with uncommitted changes unless --force).
    Remove {
        path: PathBuf,
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum MemoryCmd {
    /// Save a memory: `evyro memory add "<text>" [--category fact]`.
    /// The older form `memory add <category> "<text>"` still works.
    Add {
        #[arg(num_args = 1..=2, required = true)]
        text: Vec<String>,
        #[arg(long)]
        category: Option<String>,
        #[arg(long = "file")]
        files: Vec<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        task: Option<String>,
    },
    Search {
        query: Option<String>,
        #[arg(long)]
        category: Option<String>,
    },
    Verify {
        id: String,
    },
    Archive {
        id: String,
    },
    Delete {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Mark memories whose files changed as stale.
    Refresh,
}

/// A reader that stops early (`evyro ... | head`, `grep -q`) closes the pipe.
/// That is not an error: exit quietly instead of panicking with a backtrace.
fn is_broken_pipe(msg: &str) -> bool {
    msg.contains("Broken pipe") || msg.contains("os error 32") || msg.contains("os error 232")
}

fn broken_pipe_in(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
            || is_broken_pipe(&c.to_string())
    })
}

/// Exit status for a closed pipe: 141 (128 + SIGPIPE) like other Unix tools.
const BROKEN_PIPE_EXIT: i32 = 141;

fn quiet_on_broken_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let msg = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or("");
        if is_broken_pipe(msg) {
            std::process::exit(BROKEN_PIPE_EXIT);
        }
        default(info);
    }));
}

/// Windows gives the main thread 1 MB, which the command tree and the large `run` function
/// outgrow in debug builds (even `--help` crashed). Run on a thread with room to spare.
fn main() {
    let worker = std::thread::Builder::new()
        .name("evyro".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(real_main)
        .expect("start the command thread");
    if worker.join().is_err() {
        std::process::exit(101);
    }
}

fn real_main() {
    evyro_core::crash::install_panic_hook("evyro");
    quiet_on_broken_pipe();
    let cli = Cli::parse();
    let o = Out {
        json: cli.json,
        quiet: cli.quiet,
    };
    let debug = cli.debug;
    let quiet_errors = matches!(
        &cli.cmd,
        Cmd::Context {
            quiet_errors: true,
            ..
        }
    );
    match run(cli, &o) {
        Ok(code) => std::process::exit(code),
        Err(e) if broken_pipe_in(&e) => std::process::exit(BROKEN_PIPE_EXIT),
        Err(e) => {
            let (code, kind, msg) = match e.downcast_ref::<Error>() {
                Some(ce) => (ce.exit_code(), ce.code(), ce.to_string()),
                None => (1, "error", format!("{e:#}")),
            };
            if quiet_errors {
                std::process::exit(0);
            }
            if o.json {
                println!("{}", json!({"error": {"code": kind, "message": msg}}));
            } else {
                eprintln!("error: {msg}");
                if debug {
                    eprintln!("{e:?}");
                }
            }
            std::process::exit(code);
        }
    }
}

fn cwd(cli_dir: &Option<PathBuf>) -> anyhow::Result<PathBuf> {
    Ok(match cli_dir {
        Some(d) => d.clone(),
        None => std::env::current_dir()?,
    })
}

fn project(
    store: &Store,
    cli_project: &Option<String>,
    dir: &Path,
) -> evyro_core::Result<evyro_core::projects::Project> {
    match cli_project {
        Some(p) => store.get_project(p),
        None => store.project_for_path(dir),
    }
}

/// Session to attribute an action to: $EVYRO_SESSION_ID (set by `evyro run`) or the task's owner.
/// Make sure a person is deciding. The commands an AI runs have no terminal, so this asks for
/// one and has the person type the task key. It is a strong speed bump, not a lock: a program
/// running as the same user could still open a terminal of its own or write the database, so
/// the desktop app's button is the stronger route and the docs say so.
fn confirm_person(key: &str, verb: &str, in_ai_session: bool) -> anyhow::Result<()> {
    use std::io::{BufRead, IsTerminal, Write};
    if in_ai_session || !std::io::stdin().is_terminal() || !std::io::stderr().is_terminal() {
        return Err(Error::Invalid(format!(
            "to {verb} a plan, a person has to do it at a terminal (or with the button in the desktop app); this command was not run from one"
        ))
        .into());
    }
    eprint!("Type {key} to {verb} its plan: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    if line.trim() != key {
        return Err(Error::Invalid(format!("not {verb}d: you did not type {key}")).into());
    }
    Ok(())
}

fn render_plan(key: &str, plan: &evyro_core::plans::TaskPlan) -> String {
    let mut out = format!(
        "Plan for {key}: {} (by {})\n\n{}",
        plan.status, plan.proposed_by, plan.body
    );
    if let Some(n) = &plan.note {
        out.push_str(&format!("\n\nNote: {n}"));
    }
    out
}

fn attribute_session(store: &Store, task_id: Option<&str>) -> Option<String> {
    if let Ok(sid) = std::env::var("EVYRO_SESSION_ID") {
        if let Ok(s) = store.get_session(&sid) {
            if !s.status.is_ended() {
                return Some(s.id);
            }
        }
    }
    task_id
        .and_then(|t| store.active_session_for_task(t).ok().flatten())
        .map(|s| s.id)
}

fn run(cli: Cli, o: &Out) -> anyhow::Result<i32> {
    if let Cmd::CaptureUsage { harness, output } = &cli.cmd {
        use std::io::Read;
        let mut data = Vec::new();
        std::io::stdin().take(1024 * 1024).read_to_end(&mut data)?;
        // A status-line hook must never disturb its host: ignore input it cannot read.
        let Ok(payload) = serde_json::from_slice(&data) else {
            return Ok(0);
        };
        if let Some(snapshot) = evyro_core::quota::status_line_snapshot(harness, &payload) {
            let path = match output {
                Some(path) => path.clone(),
                None => {
                    let paths = evyro_core::paths::GlobalPaths::resolve()?;
                    evyro_core::quota::cache_path(&paths, harness)
                        .context("unsupported quota source")?
                }
            };
            evyro_core::util::write_atomic(&path, snapshot.to_string().as_bytes())?;
        }
        return Ok(0);
    }
    let dir = cwd(&cli.dir)?;
    if let Cmd::Completions { shell } = &cli.cmd {
        clap_complete::generate(*shell, &mut Cli::command(), "evyro", &mut std::io::stdout());
        return Ok(0);
    }
    if let Cmd::Version = &cli.cmd {
        o.print(&json!({"name": brand::CLI_NAME, "version": brand::VERSION, "schema_version": evyro_core::db::SCHEMA_VERSION, "contract_schema": evyro_core::contract::SCHEMA_VERSION, "os": std::env::consts::OS, "arch": std::env::consts::ARCH}),
            || format!("{} {} (db schema v{}, contract {})", brand::CLI_NAME, brand::VERSION, evyro_core::db::SCHEMA_VERSION, evyro_core::contract::SCHEMA_VERSION));
        return Ok(0);
    }
    if let Cmd::BugReport { out } = &cli.cmd {
        // Works even when the database cannot be opened.
        let (store, err) = match Store::open_default() {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e.to_string())),
        };
        let report = evyro_core::crash::bug_report(store.as_ref(), err.as_deref(), &dir);
        match out {
            Some(p) => {
                std::fs::write(p, &report)?;
                if !o.quiet {
                    eprintln!(
                        "Bug report written to {}. Read it before you share it.",
                        p.display()
                    );
                }
            }
            None => print!("{report}"),
        }
        return Ok(0);
    }
    if let Cmd::Restore { file, yes } = &cli.cmd {
        let paths = evyro_core::paths::GlobalPaths::resolve()?;
        if !yes {
            anyhow::bail!(Error::Invalid(format!(
                "restore replaces {}; re-run with --yes to confirm",
                paths.db_path().display()
            )));
        }
        paths.ensure()?;
        let kept = backup::restore_db(file, &paths.db_path())?;
        let store = Store::open_default()?;
        o.print(&json!({"restored": file, "previous_kept_at": kept, "schema_version": store.schema_version()?}),
            || format!("Restored {}. Previous database kept at {}", file.display(), kept));
        return Ok(0);
    }

    let store = Store::open_default().context("opening the Evyro database")?;
    match cli.cmd {
        Cmd::Init(a) => {
            let r = evyro_core::init::init_project(
                &store,
                &dir,
                &evyro_core::init::InitOptions {
                    name: a.name,
                    append_agent_files: a.append_agent_files,
                    agent_files: !a.no_agent_files,
                },
            )?;
            o.print(&r, || {
                let mut s = format!("{} {} ({})\n  root: {}\n", if r.action == "created" { "Registered" } else { "Already registered:" }, r.project.name, r.project.id, r.project.root_path);
                for c in &r.created { s.push_str(&format!("  created  {c}\n")); }
                for c in &r.updated { s.push_str(&format!("  updated  {c}\n")); }
                for (f, why) in &r.skipped { s.push_str(&format!("  skipped  {f}: {why}\n")); }
                s.push_str("Readiness:\n");
                for (n, ok, d) in &r.readiness { s.push_str(&format!("  {} {n}: {d}\n", if *ok { "ok  " } else { "WARN" })); }
                s.push_str("Next: `evyro task add \"<title>\" --accept \"<criterion>\"`, then `evyro task start T-1 --agent claude-code`.");
                s
            });
        }
        Cmd::Register { path, name } => {
            let r = store.register_project(&path.unwrap_or(dir), name.as_deref())?;
            o.print(&r, || {
                format!(
                    "{} {} ({}) at {}",
                    r.action, r.project.name, r.project.id, r.project.root_path
                )
            });
        }
        Cmd::Projects { all, search } => {
            let ps = store.list_projects(all, search.as_deref())?;
            let mut rows = Vec::new();
            for p in &ps {
                let active = store.active_task(&p.id)?;
                let owner = match &active {
                    Some(t) => store.active_session_for_task(&t.id)?.map(|s| s.harness),
                    None => None,
                };
                rows.push(json!({"project": p, "active_task": active.as_ref().map(|t| json!({"key": t.key, "title": t.title})), "agent": owner,
                    "open_tasks": store.list_tasks(&p.id, None, false)?.len(), "health_findings": store.project_health(&p.id)?.findings.len()}));
            }
            o.print(&rows, || {
                if ps.is_empty() {
                    return "No projects registered. Run `evyro init` inside a repository.".into();
                }
                let mut s = String::new();
                for (p, r) in ps.iter().zip(&rows) {
                    s.push_str(&format!(
                        "{}{} {}  [{}]\n  {}\n  open tasks: {}  active: {}  agent: {}\n",
                        if p.favorite { "★ " } else { "" },
                        p.name,
                        p.alias
                            .as_deref()
                            .map(|a| format!("({a})"))
                            .unwrap_or_default(),
                        p.stack.join(","),
                        p.root_path,
                        r["open_tasks"],
                        r["active_task"]["key"].as_str().unwrap_or("-"),
                        r["agent"].as_str().unwrap_or("-")
                    ));
                }
                s
            });
        }
        Cmd::Project {
            alias,
            archive,
            favorite,
        } => {
            let p = project(&store, &cli.project, &dir)?;
            let p = store.set_project_flags(&p.id, alias.as_deref(), archive, favorite)?;
            o.print(&p, || format!("Updated {}", p.name));
        }
        Cmd::Status => {
            let p = project(&store, &cli.project, &dir)?;
            auto_recover(&store, &p.id, o);
            let t = store.active_task(&p.id)?;
            let owner = match &t {
                Some(t) => store.active_session_for_task(&t.id)?,
                None => None,
            };
            let cp = match &t {
                Some(t) => store.latest_checkpoint(&p.id, Some(&t.id))?,
                None => None,
            };
            let snap = git::snapshot(&p.root()).ok();
            let pending = store.pending_handoff(&p.id, None)?;
            let v = json!({"project": p, "active_task": t, "owner": owner, "latest_checkpoint": cp.as_ref().map(|c| json!({"id": c.id, "at": c.created_at, "trigger": c.trigger, "next_action": c.next_action})), "git": snap, "pending_handoff": pending});
            o.print(&v, || {
                let mut s = format!("{} ({})\n", p.name, p.root_path);
                if let Some(g) = &snap {
                    s.push_str(&format!(
                        "git: {} @ {} · {} changed file(s)\n",
                        g.branch.as_deref().unwrap_or("(detached)"),
                        g.head
                            .as_deref()
                            .map(|h| &h[..h.len().min(10)])
                            .unwrap_or("-"),
                        g.files.len()
                    ));
                }
                match &t {
                    Some(t) => s.push_str(&format!(
                        "active: {} {} [{}] owner: {}\n",
                        t.key,
                        t.title,
                        t.status,
                        owner.as_ref().map(|o| o.harness.as_str()).unwrap_or("none")
                    )),
                    None => s.push_str("active: none (see `evyro tasks --ready`)\n"),
                }
                if let Some(c) = &cp {
                    s.push_str(&format!(
                        "last checkpoint: {} ({}) next: {}\n",
                        c.created_at,
                        c.trigger,
                        if c.next_action.is_empty() {
                            "-"
                        } else {
                            &c.next_action
                        }
                    ));
                }
                if let Some(h) = &pending {
                    s.push_str(&format!(
                        "pending handoff to {}: run `evyro resume --agent {}`\n",
                        h.to_harness, h.to_harness
                    ));
                }
                s
            });
        }
        Cmd::Doctor => {
            let checks = evyro_core::doctor::run(&store, &dir);
            let failed = checks.iter().any(|c| c.status == "fail");
            o.print(&checks, || {
                checks
                    .iter()
                    .map(|c| {
                        format!(
                            "{:<5} {:<28} {}{}",
                            c.status.to_uppercase(),
                            c.name,
                            c.detail,
                            c.remediation
                                .as_ref()
                                .map(|r| format!("\n      fix: {r}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
            return Ok(if failed { 1 } else { 0 });
        }
        Cmd::Tasks { ready, status, all } => {
            let p = project(&store, &cli.project, &dir)?;
            let ts = if ready {
                store.ready_tasks(&p.id)?
            } else {
                store.list_tasks(
                    &p.id,
                    status.as_deref().map(|s| s.parse()).transpose()?,
                    all,
                )?
            };
            o.print(&ts, || {
                if ts.is_empty() {
                    return "No tasks. Add one: evyro task add \"<title>\"".into();
                }
                ts.iter()
                    .map(|t| {
                        format!(
                            "{:<6} {:<12} P{} {:<8} {}{}{}",
                            t.key,
                            t.status,
                            t.priority,
                            t.kind,
                            t.title,
                            t.assigned_agent
                                .as_ref()
                                .map(|a| format!("  [{a}]"))
                                .unwrap_or_default(),
                            if t.needs_person().is_some() {
                                "  (needs a person)"
                            } else if t.awaiting_review() {
                                "  (awaiting review)"
                            } else {
                                ""
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Task(tc) => return task_cmd(&store, &cli.project, &dir, tc, o),
        Cmd::Context { task, budget, .. } => {
            let p = project(&store, &cli.project, &dir)?;
            let t = match task {
                Some(r) => Some(store.resolve_task(&p.id, &r)?),
                None => store.active_task(&p.id)?,
            };
            let b = evyro_core::context::budget_for(&budget)
                .ok_or_else(|| Error::Invalid(format!("unknown budget '{budget}'")))?;
            let pk = store.context_packet(&p, t.as_ref(), b)?;
            o.print(&pk, || pk.text.clone());
        }
        Cmd::Checkpoint(a) => {
            let p = match project(&store, &cli.project, &dir) {
                Ok(p) => p,
                Err(e) if a.auto => {
                    let _ = e;
                    return Ok(0);
                }
                Err(e) => return Err(e.into()),
            };
            let t = match &a.task {
                Some(r) => Some(store.resolve_task(&p.id, r)?),
                None => store.active_task(&p.id)?,
            };
            if t.is_none() && a.auto {
                return Ok(0);
            }
            let sid = attribute_session(&store, t.as_ref().map(|t| t.id.as_str()));
            let r = store.create_checkpoint(
                &p.id,
                NewCheckpoint {
                    task_id: t.as_ref().map(|t| t.id.clone()),
                    session_id: sid,
                    trigger: a
                        .trigger
                        .clone()
                        .or(if a.auto { Some("auto".into()) } else { None }),
                    summary: a.summary,
                    completed: a.done,
                    remaining: if a.remaining.is_empty() {
                        None
                    } else {
                        Some(a.remaining)
                    },
                    blockers: a.blockers,
                    next_action: a.next,
                    dedupe: a.auto,
                },
            )?;
            o.print(&r, || {
                if r.created {
                    format!(
                        "Checkpoint {} · {} changed file(s) at {}",
                        r.checkpoint.id,
                        r.checkpoint.git.files.len(),
                        r.checkpoint
                            .git
                            .head
                            .as_deref()
                            .map(|h| &h[..h.len().min(10)])
                            .unwrap_or("-")
                    )
                } else {
                    "No changes since the last checkpoint; skipped.".into()
                }
            });
        }
        Cmd::Handoff(a) => return handoff_cmd(&store, &cli.project, &dir, a, false, o),
        Cmd::Work(a) => return work_cmd(&store, &cli.project, &dir, a, o),
        Cmd::Switch(a) => return switch_cmd(&store, &cli.project, &dir, a, o),
        Cmd::Resume {
            agent,
            handoff,
            accept_drift,
            model,
        } => {
            let p = project(&store, &cli.project, &dir)?;
            let r = store.resume_handoff(
                &p.id,
                handoff.as_deref(),
                &agent,
                accept_drift,
                model.as_deref(),
            )?;
            let code = if r.accepted {
                0
            } else {
                Error::Drift(String::new()).exit_code()
            };
            o.print(&r, || out::render_resume(&r));
            return Ok(code);
        }
        Cmd::Chat(a) => {
            if a.list {
                chat::list(o);
                return Ok(0);
            }
            let p = project(&store, &cli.project, &dir)?;
            return chat::run_chat(
                &store,
                &p,
                chat::ChatArgs {
                    to: a.to.as_deref().unwrap_or_default(),
                    task: a.task.as_deref(),
                    handoff: a.handoff.as_deref(),
                    accept_drift: a.accept_drift,
                    model: a.model.as_deref(),
                    approve: a.approve,
                    no_mcp: a.no_mcp,
                    command: a.command,
                    prompt: a.prompt,
                },
                o,
            );
        }
        Cmd::Run(a) => {
            let p = project(&store, &cli.project, &dir)?;
            return launch::run_agent(
                &store,
                &p,
                launch::RunOpts {
                    target: &a.agent,
                    task: a.task.as_deref(),
                    handoff: a.handoff.as_deref(),
                    headless: a.headless,
                    prompt: a.prompt,
                    model: a.model.as_deref(),
                    accept_drift: a.accept_drift,
                    no_open: a.no_open,
                    plan_first: a.plan_first,
                },
                o,
            );
        }
        Cmd::Targets { cmd } => {
            use evyro_core::targets::{self as tg, AuthMode, Target, TargetKind};
            match cmd.unwrap_or(TargetsCmd::List { ready: false }) {
                TargetsCmd::List { ready } => {
                    let st: Vec<_> = tg::list_targets(&store.paths)?
                        .iter()
                        .map(tg::check_target)
                        .filter(|s| !ready || s.state == "ready")
                        .collect();
                    o.print(&st, || {
                        let mut s = String::new();
                        for (kind, title) in [(TargetKind::Harness, "Agent CLIs"), (TargetKind::Api, "API models"), (TargetKind::Ide, "IDEs"), (TargetKind::Chat, "Chat apps")] {
                            let rows: Vec<_> = st.iter().filter(|x| x.target.kind == kind).collect();
                            if rows.is_empty() { continue; }
                            s.push_str(&format!("{title}\n"));
                            for r in rows {
                                let mark = match r.state.as_str() { "ready" => "ready", "needs_setup" => "setup", _ => "  ?  " };
                                s.push_str(&format!("  [{mark}] {:<18} {} ({})\n", r.target.name, r.display, r.target.auth.label()));
                            }
                        }
                        s.push_str("\nHand work over with `evyro switch --to <name>`. Details: `evyro targets check <name>`. Add your own: `evyro targets add`.");
                        s
                    });
                }
                TargetsCmd::Check { name } => {
                    let t = tg::resolve_target(&store.paths, &name)?;
                    let r = tg::check_target(&t);
                    o.print(&r, || {
                        let mut s = format!(
                            "{} — {} ({})\n  state: {}\n",
                            r.target.name,
                            r.display,
                            r.target.auth.label(),
                            r.state
                        );
                        for c in &r.checks {
                            s.push_str(&format!("  {:<8} {}: {}\n", c.state, c.name, c.detail));
                        }
                        if let Some(n) = &r.next_step {
                            s.push_str(&format!("  next: {n}\n"));
                        }
                        s
                    });
                }
                TargetsCmd::Add {
                    name,
                    kind,
                    harness,
                    app,
                    auth,
                    provider,
                    model,
                    key_env,
                    key_keychain,
                    base_url,
                    description,
                } => {
                    let kind: TargetKind = serde_json::from_value(json!(kind)).map_err(|_| {
                        Error::Invalid("kind must be harness, api, ide or chat".into())
                    })?;
                    let auth: AuthMode = serde_json::from_value(json!(auth)).map_err(|_| {
                        Error::Invalid("auth must be auto, app_login, api_key or local".into())
                    })?;
                    let t = tg::add_target(
                        &store.paths,
                        Target {
                            name,
                            kind,
                            harness: harness.map(|h| adapters::normalize_id(&h)),
                            app,
                            auth,
                            provider,
                            model,
                            key_env,
                            key_keychain,
                            base_url,
                            description,
                            builtin: false,
                        },
                    )?;
                    let r = tg::check_target(&t);
                    o.print(&r, || {
                        format!(
                            "Saved target '{}' ({}). Status: {}{}",
                            t.name,
                            r.display,
                            r.state,
                            r.next_step
                                .as_ref()
                                .map(|n| format!(". Next: {n}"))
                                .unwrap_or_default()
                        )
                    });
                }
                TargetsCmd::Remove { name } => {
                    tg::remove_target(&store.paths, &name)?;
                    o.print(&json!({"removed": name}), || {
                        format!("Removed target '{name}'")
                    });
                }
                TargetsCmd::Providers => {
                    let ps = tg::providers();
                    o.print(&ps, || {
                        let mut s =
                            String::from("Providers for API targets (use with --provider):\n");
                        for p in &ps {
                            let users: Vec<&str> = [
                                "claude-code",
                                "codex",
                                "gemini",
                                "opencode",
                                "aider",
                                "goose",
                            ]
                            .into_iter()
                            .filter(|h| tg::harness_providers(h).contains(&p.id))
                            .collect();
                            s.push_str(&format!(
                                "  {:<22} key: {:<20} via: {}\n",
                                p.id,
                                p.key_env.map(|k| format!("${k}")).unwrap_or(if p.local {
                                    "none (local)".into()
                                } else {
                                    "cloud credentials".into()
                                }),
                                users.join(", ")
                            ));
                        }
                        s
                    });
                }
            }
        }
        Cmd::Agents => {
            let ds = adapters::detect_all();
            o.print(&ds, || {
                ds.iter()
                    .map(|d| {
                        let caps = d.capabilities;
                        let flags: Vec<&str> = [
                            ("headless", caps.headless_launch),
                            ("interactive", caps.interactive_launch),
                            ("mcp", caps.mcp),
                            ("hooks", caps.checkpoint_hooks),
                            ("acp", caps.acp),
                            ("instructions", caps.instruction_file),
                        ]
                        .iter()
                        .filter(|(_, b)| *b)
                        .map(|(n, _)| *n)
                        .collect();
                        format!(
                            "{:<12} {:<16} {:<10} {}\n             caps: {}  evidence: {}{}",
                            d.id,
                            d.compatibility,
                            d.version.clone().unwrap_or("-".into()),
                            d.binary.clone().unwrap_or_default(),
                            flags.join(","),
                            d.evidence,
                            d.diagnostic
                                .as_ref()
                                .map(|x| format!("\n             note: {x}"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Models => {
            let v: Vec<_> = adapters::registry().iter().map(|a| json!({"agent": a.id, "model_selection": a.capabilities.model_selection && a.model_flag.is_some(), "flag": a.model_flag, "discovery": "not implemented: pass --model <id> as documented by the agent"})).collect();
            o.print(&v, || {
                let mut s = String::from("Model discovery is not implemented yet; Evyro passes --model through to agents that support it.\n");
                for a in adapters::registry() { s.push_str(&format!("  {:<12} {}\n", a.id, a.model_flag.map(|f| format!("supports {f} <model>")).unwrap_or("no model flag".into()))); }
                s
            });
        }
        Cmd::Sessions { active } => {
            let p = project(&store, &cli.project, &dir)?;
            auto_recover(&store, &p.id, o);
            let ss = store.list_sessions(&p.id, active)?;
            o.print(&ss, || {
                ss.iter()
                    .map(|s| {
                        format!(
                            "{} {:<12} {:<12} task:{} started {}{}",
                            s.id,
                            s.harness,
                            s.status,
                            s.task_id
                                .as_ref()
                                .and_then(|t| store.get_task(t).ok())
                                .map(|t| t.key)
                                .unwrap_or("-".into()),
                            s.started_at,
                            s.exit_reason
                                .as_ref()
                                .map(|r| format!(" ({r})"))
                                .unwrap_or_default()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Heartbeat { task } => {
            let p = project(&store, &cli.project, &dir)?;
            let t = match &task {
                Some(r) => Some(store.resolve_task(&p.id, r)?),
                None => store.active_task(&p.id)?,
            };
            match attribute_session(&store, t.as_ref().map(|t| t.id.as_str())) {
                Some(sid) => {
                    store.heartbeat(&sid)?;
                    o.print(&sid, || format!("Session {sid} reported in."));
                }
                None => return Err(anyhow::anyhow!("no running session to report for")),
            }
        }
        Cmd::Recover { include_unknown } => {
            let p = project(&store, &cli.project, &dir)?;
            let r = store.recover_stale_sessions(&p.id, include_unknown)?;
            o.print(&r, || if r.is_empty() { "No stale sessions.".into() } else {
                r.iter().map(|x| format!("Recovered {} session {}: checkpoint {}; changed since last checkpoint: {}\n  Continue with `evyro handoff --to <agent>`.", x.harness, x.session_id, x.checkpoint_id, if x.changed_since_last_checkpoint.is_empty() { "none".into() } else { x.changed_since_last_checkpoint.join(", ") })).collect::<Vec<_>>().join("\n")
            });
        }
        Cmd::Plan { cmd } => {
            let p = project(&store, &cli.project, &dir)?;
            let in_session = std::env::var("EVYRO_SESSION_ID").ok();
            match cmd {
                PlanCmd::Show { task } => {
                    let t = store.resolve_task(&p.id, &task)?;
                    match store.plan(&t.id)? {
                        Some(plan) => o.print(&plan, || render_plan(&t.key, &plan)),
                        None => o.print(&json!({"task": t.key, "plan": null}), || {
                            format!("{} has no plan yet.", t.key)
                        }),
                    }
                }
                PlanCmd::Set { task, text, file } => {
                    use std::io::Read;
                    let t = store.resolve_task(&p.id, &task)?;
                    let body = match (text, file) {
                        (Some(text), _) => text,
                        (None, Some(f)) if f.as_os_str() == "-" => {
                            let mut b = String::new();
                            std::io::stdin().take(1 << 20).read_to_string(&mut b)?;
                            b
                        }
                        (None, Some(f)) => std::fs::read_to_string(f)?,
                        (None, None) => {
                            return Err(Error::Invalid(
                                "give the plan with --text or --file".into(),
                            )
                            .into())
                        }
                    };
                    let by = match in_session
                        .as_deref()
                        .and_then(|s| store.get_session(s).ok())
                    {
                        Some(s) => format!("agent:{}", s.harness),
                        None => "human".to_owned(),
                    };
                    let plan = store.propose_plan(&t, &body, &by)?;
                    o.print(&plan, || {
                        format!(
                            "Plan for {} saved. It waits for a person: `evyro plan approve {}`.",
                            t.key, t.key
                        )
                    });
                }
                PlanCmd::Approve { task, note } => {
                    let t = store.resolve_task(&p.id, &task)?;
                    confirm_person(&t.key, "approve", in_session.is_some())?;
                    let plan = store.decide_plan(&t, true, note.as_deref())?;
                    o.print(&plan, || {
                        format!(
                            "Plan for {} approved. Start it with `evyro run --to <ai> --task {}`.",
                            t.key, t.key
                        )
                    });
                }
                PlanCmd::Reject { task, note } => {
                    let t = store.resolve_task(&p.id, &task)?;
                    confirm_person(&t.key, "reject", in_session.is_some())?;
                    let plan = store.decide_plan(&t, false, note.as_deref())?;
                    o.print(&plan, || format!("Plan for {} rejected. The AI reads your note with `evyro plan show {}`.", t.key, t.key));
                }
            }
        }
        Cmd::Worktrees(w) => {
            let p = project(&store, &cli.project, &dir)?;
            match w {
                WorktreeCmd::List => {
                    let ws = git::worktrees(&p.root())?;
                    o.print(&ws, || {
                        ws.iter()
                            .map(|w| {
                                format!(
                                    "{}  {}  {}",
                                    w.path,
                                    w.branch.as_deref().unwrap_or("(detached)"),
                                    w.head
                                        .as_deref()
                                        .map(|h| &h[..h.len().min(10)])
                                        .unwrap_or("")
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
                WorktreeCmd::Add { task, branch, path } => {
                    let t = store.resolve_task(&p.id, &task)?;
                    let t = store.add_task_worktree(&p, &t, branch, path)?;
                    let (wt, branch) = (
                        t.worktree.clone().unwrap_or_default(),
                        t.branch.clone().unwrap_or_default(),
                    );
                    o.print(&t, || {
                        format!("Worktree {} on branch {} for {}", wt, branch, t.key)
                    });
                }
                WorktreeCmd::Prune { dry_run } => {
                    let items = store.prune_worktrees(&p, dry_run)?;
                    o.print(&items, || {
                        if items.is_empty() {
                            return "No task worktrees to tidy.".to_owned();
                        }
                        items
                            .iter()
                            .map(|i| format!("{} {} ({})", i.task, i.path, i.reason))
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
                WorktreeCmd::Remove { path, force } => {
                    git::remove_worktree(&p.root(), &path, force)?;
                    o.print(&json!({"removed": path}), || {
                        format!("Removed {}", path.display())
                    });
                }
            }
        }
        Cmd::Verify {
            task,
            kind,
            index,
            command,
        } => {
            let p = project(&store, &cli.project, &dir)?;
            let t = match task {
                Some(r) => Some(store.resolve_task(&p.id, &r)?),
                None => store.active_task(&p.id)?,
            };
            let argv = if !command.is_empty() {
                command
            } else {
                let k = kind.unwrap_or("test".into());
                let list = match k.as_str() {
                    "test" => &p.commands.test,
                    "lint" => &p.commands.lint,
                    "build" => &p.commands.build,
                    _ => anyhow::bail!(Error::Invalid("kind must be test|lint|build".into())),
                };
                list.get(index).cloned().ok_or_else(|| {
                    Error::NotFound(format!("no configured {k} command; pass one after `--`"))
                })?
            };
            let root = match &t {
                Some(t) => store.task_root(t)?,
                None => p.root(),
            };
            let sid = attribute_session(&store, t.as_ref().map(|t| t.id.as_str()));
            let r = store.run_verification(
                &p.id,
                t.as_ref().map(|t| t.id.as_str()),
                sid.as_deref(),
                &argv,
                &root,
            )?;
            o.print(
                &json!({"receipt": r.receipt, "output_tail": r.output_tail}),
                || {
                    format!(
                        "{}\n{} `{}` exit {} in {} ms · receipt {}",
                        r.output_tail,
                        if r.receipt.passed { "PASS" } else { "FAIL" },
                        argv.join(" "),
                        r.receipt
                            .exit_code
                            .map(|c| c.to_string())
                            .unwrap_or("signal".into()),
                        r.receipt.duration_ms,
                        r.receipt.id
                    )
                },
            );
            return Ok(if r.receipt.passed { 0 } else { 1 });
        }
        Cmd::Decide {
            title,
            decision_text,
            decision,
            why,
            task,
        } => {
            let p = project(&store, &cli.project, &dir)?;
            let decision = decision_text.or(decision).ok_or_else(|| {
                Error::Invalid("give the decision: evyro decide \"<title>\" \"<decision>\"".into())
            })?;
            let t = match task {
                Some(r) => Some(store.resolve_task(&p.id, &r)?.id),
                None => None,
            };
            let d = store.record_decision(&p.id, t.as_deref(), &title, &decision, &why, "human")?;
            o.print(&d, || format!("Recorded decision {}", d.id));
        }
        Cmd::Decisions => {
            let p = project(&store, &cli.project, &dir)?;
            let ds = store.list_decisions(&p.id, None)?;
            o.print(&ds, || {
                ds.iter()
                    .map(|d| format!("- {}: {} ({})", d.title, d.decision, d.source))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Serve {
            host,
            port,
            allow_lan,
        } => {
            let running = evyro_remote::start(
                evyro_remote::ServeOpts {
                    host,
                    port,
                    allow_lan,
                    evyro_mcp: std::env::current_exe().ok(),
                },
                None,
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            let url = running.url();
            let info =
                json!({"url": url, "reach": running.reach, "api": evyro_remote::API_VERSION});
            if o.json {
                println!("{info}");
            } else {
                println!("{} is serving at {url}", brand::PRODUCT_NAME);
            }
            let _ = std::io::Write::flush(&mut std::io::stdout());
            if !o.json && !o.quiet {
                eprintln!(
                    "{}\nPair a device with `{} remote pair --name <name>`, then open {url} on it (or reach it through Tailscale). Press Ctrl-C to stop.",
                    match running.reach {
                        evyro_remote::Reach::Loopback => "Only this computer can connect.",
                        evyro_remote::Reach::Tailscale => "Devices on your Tailscale network can connect.",
                        evyro_remote::Reach::Lan => "WARNING: any device that reaches this address can connect, and plain HTTP is not encrypted.",
                    },
                    brand::CLI_NAME
                );
            }
            let flag = running.stop_flag();
            let _ =
                ctrlc::set_handler(move || flag.store(true, std::sync::atomic::Ordering::SeqCst));
            running.wait();
            if !o.json && !o.quiet {
                eprintln!("[{}] stopped serving", brand::CLI_NAME);
            }
        }
        Cmd::Remote(cmd) => match cmd {
            RemoteCmd::Pair {
                name,
                scope,
                minutes,
            } => {
                let p = store.pair_device(&name, &scope, minutes * 60)?;
                o.print(&p, || {
                    format!(
                        "Pairing code for \"{}\" ({} access): {}\nIt works once and expires at {} (UTC).\nOn the device, open the address `{} serve` prints and enter the code.",
                        p.device.name, p.device.scope, p.code, p.expires_at, brand::CLI_NAME
                    )
                });
            }
            RemoteCmd::Devices => {
                let ds = store.list_devices()?;
                o.print(&ds, || {
                    if ds.is_empty() {
                        return format!(
                            "No devices. Pair one with `{} remote pair --name <name>`.",
                            brand::CLI_NAME
                        );
                    }
                    ds.iter()
                        .map(|d| {
                            format!(
                                "{:<8} {:<8} {}  {}  last seen {}",
                                d.state,
                                d.scope,
                                d.id,
                                d.name,
                                d.last_seen_at.as_deref().unwrap_or("never")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                });
            }
            RemoteCmd::Revoke { device } => {
                let d = store.revoke_device(&device)?;
                o.print(&d, || {
                    format!("Revoked {} ({}). Its token no longer works.", d.name, d.id)
                });
            }
        },
        Cmd::Skills { cmd } => {
            use evyro_core::skills;
            let here = project(&store, &cli.project, &dir).ok();
            let scoped = |global: bool| -> anyhow::Result<Option<&evyro_core::projects::Project>> {
                if global {
                    return Ok(None);
                }
                match &here {
                    Some(p) => Ok(Some(p)),
                    None => anyhow::bail!(
                        "not inside an {} project: run this in one, or add --global",
                        brand::PRODUCT_NAME
                    ),
                }
            };
            match cmd.unwrap_or(SkillsCmd::List { for_text: None }) {
                SkillsCmd::List { for_text } => {
                    let all = store.skills(here.as_ref())?;
                    let shown: Vec<&skills::Skill> = match &for_text {
                        Some(t) => skills::relevant(&all, t, 8),
                        None => all.iter().collect(),
                    };
                    o.print(&shown, || {
                        let mut out: Vec<String> = shown
                            .iter()
                            .map(|k| {
                                let state = if !k.usable() {
                                    "problem"
                                } else if k.enabled {
                                    "on"
                                } else {
                                    "off"
                                };
                                format!(
                                    "[{state:>7}] {:<26} {:<8} {:<8} {}",
                                    k.name, k.version, k.source, k.description
                                )
                            })
                            .collect();
                        if out.is_empty() {
                            out.push("No skill fits that.".into());
                        }
                        out.push(format!(
                            "\n{} skill(s). Show one with `{} skills show <name>`.",
                            shown.len(),
                            brand::CLI_NAME
                        ));
                        out.join("\n")
                    });
                }
                SkillsCmd::Show { name } => {
                    let k = store.skill(here.as_ref(), &name)?;
                    if !k.usable() {
                        anyhow::bail!("skill '{name}' cannot be used: {}", k.problems.join("; "));
                    }
                    let v = json!({"skill": k, "text": k.body()});
                    o.print(&v, || k.raw.clone());
                }
                SkillsCmd::New {
                    name,
                    description,
                    global,
                } => {
                    let f = skills::create(&store, scoped(global)?, &name, &description)?;
                    o.print(&json!({"path": f}), || {
                        format!(
                            "Created {}. Edit it, then check it with `{} skills check`.",
                            f.display(),
                            brand::CLI_NAME
                        )
                    });
                }
                SkillsCmd::Enable { names, global } => {
                    let p = scoped(global)?;
                    for n in &names {
                        store.set_skill_enabled(p, n, true)?;
                    }
                    o.print(&json!({"enabled": names}), || {
                        format!(
                            "Turned on: {}{}",
                            names.join(", "),
                            if global { " (every project)" } else { "" }
                        )
                    });
                }
                SkillsCmd::Disable { names, global } => {
                    let p = scoped(global)?;
                    for n in &names {
                        store.set_skill_enabled(p, n, false)?;
                    }
                    o.print(&json!({"disabled": names}), || {
                        format!(
                            "Turned off: {}{}",
                            names.join(", "),
                            if global { " (every project)" } else { "" }
                        )
                    });
                }
                SkillsCmd::Reset { names } => {
                    let p = scoped(false)?;
                    for n in &names {
                        store.reset_skill_setting(p, n)?;
                    }
                    o.print(&json!({"reset": names}), || {
                        format!(
                            "This project now follows the global choice for: {}",
                            names.join(", ")
                        )
                    });
                }
                SkillsCmd::Check => {
                    let all = store.skills(here.as_ref())?;
                    let bad: Vec<&skills::Skill> = all.iter().filter(|k| !k.usable()).collect();
                    o.print(&bad, || {
                        if bad.is_empty() {
                            format!("All {} skills can be used.", all.len())
                        } else {
                            bad.iter()
                                .map(|k| {
                                    format!(
                                        "{} ({}): {}",
                                        k.name,
                                        k.path.as_deref().unwrap_or(&k.source),
                                        k.problems.join("; ")
                                    )
                                })
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    });
                    if !bad.is_empty() {
                        return Ok(1);
                    }
                }
                SkillsCmd::Import {
                    path,
                    global,
                    force,
                } => {
                    let r = skills::import(&store, &path, scoped(global)?, force)?;
                    o.print(&r, || {
                        let mut out: Vec<String> = r
                            .imported
                            .iter()
                            .map(|i| match &i.replaced {
                                Some(old) => {
                                    format!("imported {} {} (replaced {old})", i.name, i.version)
                                }
                                None => format!("imported {} {}", i.name, i.version),
                            })
                            .collect();
                        out.extend(
                            r.skipped
                                .iter()
                                .map(|k| format!("skipped {}: {}", k.name, k.reason)),
                        );
                        out.join("\n")
                    });
                    if r.imported.is_empty() && !r.skipped.is_empty() {
                        return Ok(1);
                    }
                }
                SkillsCmd::Export { names, out } => {
                    let files = skills::export(&store, here.as_ref(), &names, &out)?;
                    o.print(&files, || {
                        format!("Wrote {} skill(s) to {}", files.len(), out.display())
                    });
                }
                SkillsCmd::Sync {
                    harness,
                    names,
                    append,
                    force,
                } => {
                    let p = scoped(false)?.unwrap();
                    let r = skills::sync(&store, p, &harness, &names, append, force)?;
                    o.print(&r, || {
                        let mut out = Vec::new();
                        for w in &r.written {
                            out.push(format!("wrote {w}"));
                        }
                        for w in &r.removed {
                            out.push(format!(
                                "removed {w} (Evyro wrote it; the skill is no longer on)"
                            ));
                        }
                        if !r.unchanged.is_empty() {
                            out.push(format!("{} file(s) already up to date", r.unchanged.len()));
                        }
                        for k in &r.skipped {
                            out.push(format!("skipped {}: {}", k.name, k.reason));
                        }
                        out.join("\n")
                    });
                    if r.written.is_empty() && r.unchanged.is_empty() && !r.skipped.is_empty() {
                        return Ok(1);
                    }
                }
            }
        }
        Cmd::Search(cmd) => {
            use evyro_core::search;
            let p = project(&store, &cli.project, &dir)?;
            let root = p.root();
            match cmd {
                SearchCmd::Files { query, limit } => {
                    let hits = search::find_files(&root, &query, limit)?;
                    o.print(&hits, || {
                        hits.iter()
                            .map(|h| h.path.clone())
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
                SearchCmd::Text {
                    pattern,
                    regex,
                    case,
                    path,
                    limit,
                } => {
                    let r = search::search_text(
                        &root,
                        &search::TextQuery {
                            pattern: &pattern,
                            regex,
                            case_sensitive: case,
                            path_filter: path.as_deref(),
                            limit,
                        },
                    )?;
                    o.print(&r, || {
                        let mut out: Vec<String> = r
                            .hits
                            .iter()
                            .map(|h| {
                                format!("{}:{}:{}: {}", h.path, h.line, h.column, h.text.trim())
                            })
                            .collect();
                        out.push(format!(
                            "{} match(es) in {} file(s) searched{}",
                            r.hits.len(),
                            r.files_searched,
                            if r.truncated {
                                "; stopped at the limit, use --limit for more"
                            } else {
                                ""
                            }
                        ));
                        out.join("\n")
                    });
                }
                SearchCmd::Symbols { query, limit } => {
                    let syms = search::find_symbols(&root, &query, limit)?;
                    o.print(&syms, || {
                        syms.iter()
                            .map(|s| format!("{:10} {}  {}:{}", s.kind, s.name, s.path, s.line))
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
                SearchCmd::Outline { path } => {
                    let syms = search::outline(&root, &path)?;
                    o.print(&syms, || {
                        if syms.is_empty() {
                            "No symbols found (unknown language or none defined).".into()
                        } else {
                            syms.iter()
                                .map(|s| format!("{:>5}  {:10} {}", s.line, s.kind, s.name))
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                    });
                }
            }
        }
        Cmd::Review { cmd } => {
            let p = project(&store, &cli.project, &dir)?;
            let actor = match std::env::var("EVYRO_SESSION_ID") {
                Ok(sid) if !sid.is_empty() => format!("agent:{sid}"),
                _ => "human".to_string(),
            };
            let list = |cs: &Vec<evyro_core::review::ReviewComment>| {
                if cs.is_empty() {
                    return "No review comments waiting.".to_string();
                }
                cs.iter()
                    .map(|c| {
                        format!(
                            "{}  {:8} {}  {}",
                            c.short_id(),
                            c.status,
                            c.location(),
                            c.body.replace('\n', " ")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            match cmd.unwrap_or(ReviewCmd::List { all: false }) {
                ReviewCmd::List { all } => {
                    let cs = if all {
                        store.list_review_comments(&p.id, &[])?
                    } else {
                        store.pending_review_comments(&p.id)?
                    };
                    o.print(&cs, || list(&cs));
                }
                ReviewCmd::Comment {
                    location,
                    text,
                    task,
                } => {
                    let (path, line) = evyro_core::review::parse_location(&location)?;
                    let root = p.root();
                    // The commented line as it is now, for context.
                    let snippet = line
                        .and_then(|l| {
                            std::fs::read_to_string(root.join(&path))
                                .ok()
                                .and_then(|t| t.lines().nth(l as usize - 1).map(str::to_string))
                        })
                        .unwrap_or_default();
                    let t = match task {
                        Some(r) => Some(store.resolve_task(&p.id, &r)?.id),
                        None => None,
                    };
                    let head = git::head_sha(&root).ok().flatten();
                    let c = store.add_review_comment(
                        &p.id,
                        evyro_core::review::NewReviewComment {
                            task_id: t.as_deref(),
                            path: &path,
                            line,
                            snippet: &snippet,
                            body: &text,
                            author: &actor,
                            commit_sha: head.as_deref(),
                        },
                    )?;
                    o.print(&c, || {
                        format!("Comment {} on {}", c.short_id(), c.location())
                    });
                }
                ReviewCmd::Resolve { ids } => {
                    if ids.is_empty() {
                        return Err(Error::Invalid("give the comment ids to resolve".into()).into());
                    }
                    let mut done = Vec::new();
                    for id in &ids {
                        let c = store.resolve_review_ref(&p.id, id)?;
                        done.push(store.set_review_status(&c.id, "resolved", &actor)?);
                    }
                    o.print(&done, || format!("Resolved {} comment(s).", done.len()));
                }
                ReviewCmd::Reopen { id } => {
                    let c = store.resolve_review_ref(&p.id, &id)?;
                    let c = store.set_review_status(&c.id, "open", &actor)?;
                    o.print(&c, || format!("Reopened {}", c.id));
                }
                ReviewCmd::Delete { id } => {
                    let c = store.resolve_review_ref(&p.id, &id)?;
                    store.delete_review_comment(&c.id)?;
                    o.print(&json!({"deleted": c.id}), || format!("Deleted {}", c.id));
                }
                ReviewCmd::Prompt { mark_sent } => {
                    let cs = store.list_review_comments(&p.id, &["open"])?;
                    let text = if cs.is_empty() {
                        String::new()
                    } else {
                        evyro_core::review::review_prompt(&cs)
                    };
                    if mark_sent {
                        for c in &cs {
                            store.set_review_status(&c.id, "sent", &actor)?;
                        }
                    }
                    o.print(&json!({"prompt": text, "count": cs.len()}), || {
                        if cs.is_empty() {
                            "No open review comments.".into()
                        } else {
                            text.clone()
                        }
                    });
                }
            }
        }
        Cmd::Memory(m) => {
            let p = project(&store, &cli.project, &dir)?;
            match m {
                MemoryCmd::Add {
                    mut text,
                    category,
                    files,
                    global,
                    task,
                } => {
                    let content = text.pop().unwrap_or_default();
                    let category = match (category, text.pop()) {
                        (Some(_), Some(_)) => {
                            return Err(Error::Invalid(
                                "give the category once: positional or --category".into(),
                            )
                            .into())
                        }
                        (Some(c), None) | (None, Some(c)) => c,
                        (None, None) => "note".to_string(),
                    };
                    let t = match task {
                        Some(r) => Some(store.resolve_task(&p.id, &r)?.id),
                        None => None,
                    };
                    let mm = store.add_memory(
                        &p.id,
                        NewMemory {
                            category,
                            content,
                            files,
                            task_id: t,
                            confidence: None,
                            global,
                        },
                        "human",
                    )?;
                    o.print(&mm, || format!("Saved memory {}", mm.id));
                }
                MemoryCmd::Search { query, category } => {
                    let ms =
                        store.search_memory(&p.id, query.as_deref(), category.as_deref(), 50)?;
                    o.print(&ms, || {
                        ms.iter()
                            .map(|m| {
                                format!(
                                    "{} [{}|{}] {}",
                                    m.id, m.category, m.verification, m.content
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    });
                }
                MemoryCmd::Verify { id } => {
                    let mm = store.set_memory_state(&id, None, Some("verified"))?;
                    o.print(&mm, || "Marked verified".into());
                }
                MemoryCmd::Archive { id } => {
                    let mm = store.set_memory_state(&id, Some(true), None)?;
                    o.print(&mm, || "Archived".into());
                }
                MemoryCmd::Delete { id, yes } => {
                    if !yes {
                        anyhow::bail!(Error::Invalid(
                            "deleting a memory is permanent; re-run with --yes".into()
                        ));
                    }
                    store.delete_memory(&id)?;
                    o.print(&json!({"deleted": id}), || "Deleted".into());
                }
                MemoryCmd::Refresh => {
                    let s = store.refresh_memory_staleness(&p.id)?;
                    o.print(&s, || format!("{} memory record(s) marked stale", s.len()));
                }
            }
        }
        Cmd::Handoffs => {
            let p = project(&store, &cli.project, &dir)?;
            let hs = store.list_handoffs(&p.id, 50)?;
            o.print(&hs, || {
                hs.iter()
                    .map(|h| {
                        format!(
                            "{} {} -> {} [{}] quality {}/{} {}",
                            h.id,
                            h.from_harness.as_deref().unwrap_or("?"),
                            h.to_harness,
                            h.status,
                            h.quality.met,
                            h.quality.total,
                            h.created_at
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Health => {
            let p = project(&store, &cli.project, &dir)?;
            let h = store.project_health(&p.id)?;
            o.print(&h, || {
                let mut s = format!(
                    "{} tasks: {}\n",
                    p.name,
                    h.task_counts
                        .iter()
                        .filter(|(_, n)| **n > 0)
                        .map(|(k, n)| format!("{k}={n}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                );
                if h.findings.is_empty() {
                    s.push_str("No findings.");
                }
                for f in &h.findings {
                    s.push_str(&format!(
                        "- {}: {} [{}]\n",
                        f.kind,
                        f.detail,
                        f.evidence.join(", ")
                    ));
                }
                s
            });
        }
        Cmd::Export { out } => {
            let p = project(&store, &cli.project, &dir)?;
            let b = backup::export_project(&store, &p.id)?;
            let path = out.unwrap_or_else(|| {
                p.evyro_dir().join("exports").join(format!(
                    "{}-{}.json",
                    p.name,
                    evyro_core::util::now().replace([':', '.'], "-")
                ))
            });
            evyro_core::util::write_atomic(&path, &serde_json::to_vec_pretty(&b)?)?;
            o.print(
                &json!({"path": path, "redactions": b.redaction_findings}),
                || {
                    format!(
                        "Exported {} to {} ({} secret(s) redacted)",
                        p.name,
                        path.display(),
                        b.redaction_findings
                    )
                },
            );
        }
        Cmd::Import { file, root } => {
            let b: backup::ExportBundle = serde_json::from_str(&std::fs::read_to_string(&file)?)
                .map_err(|e| Error::Invalid(format!("not an Evyro export: {e}")))?;
            let r = backup::import_project(&store, &b, root.as_deref())?;
            o.print(&r, || {
                format!(
                    "Imported project {} at {} ({} rows)",
                    r.project_id,
                    r.root_path,
                    r.rows.iter().map(|(_, n)| n).sum::<usize>()
                )
            });
        }
        Cmd::Backup { file } => {
            backup::backup_db(&store, &file)?;
            o.print(&json!({"backup": file}), || {
                format!("Backup written to {}", file.display())
            });
        }
        Cmd::Logs { task, limit } => {
            let p = project(&store, &cli.project, &dir)?;
            let t = match task {
                Some(r) => Some(store.resolve_task(&p.id, &r)?.id),
                None => None,
            };
            let ev = store.list_events(&p.id, t.as_deref(), limit)?;
            o.print(&ev, || {
                ev.iter()
                    .map(|e| format!("{} {:<26} {}", e.created_at, e.kind, e.payload))
                    .collect::<Vec<_>>()
                    .join("\n")
            });
        }
        Cmd::Mcp { read_only, agent } => {
            let p = project(&store, &cli.project, &dir)?;
            let mut server =
                evyro_mcp::Server::new(store, p, evyro_mcp::ServerOptions { read_only, agent });
            let stdin = std::io::stdin();
            server.serve(stdin.lock(), std::io::stdout())?;
        }
        Cmd::Integrate {
            agent,
            hooks,
            write,
        } => {
            // The Cursor CLI and the Cursor IDE read the same .cursor/mcp.json.
            let ide_id = if agent == "cursor" && write {
                "cursor-ide"
            } else {
                agent.as_str()
            };
            if let Some(i) = evyro_core::targets::ides()
                .into_iter()
                .find(|i| i.id == ide_id)
            {
                let p = project(&store, &cli.project, &dir)?;
                // `evyro` by name when it is on PATH, else this binary's full path.
                let command = if adapters::which(brand::CLI_NAME).is_some() {
                    brand::CLI_NAME.to_string()
                } else {
                    std::env::current_exe()
                        .map(|e| e.to_string_lossy().into_owned())
                        .unwrap_or_else(|_| brand::CLI_NAME.to_string())
                };
                let cmd = serde_json::to_string(&command)?;
                let (rel, body) = match i.id {
                    "cursor-ide" => (".cursor/mcp.json", format!("{{\n  \"mcpServers\": {{\n    \"evyro\": {{ \"command\": {cmd}, \"args\": [\"mcp\"] }}\n  }}\n}}\n")),
                    "vscode" => (".vscode/mcp.json", format!("{{\n  \"servers\": {{\n    \"evyro\": {{ \"type\": \"stdio\", \"command\": {cmd}, \"args\": [\"mcp\"] }}\n  }}\n}}\n")),
                    _ => ("", String::new()),
                };
                let mut result = "printed".to_string();
                if write && !rel.is_empty() {
                    let path = p.root().join(rel);
                    if path.exists() {
                        let cur = std::fs::read_to_string(&path).unwrap_or_default();
                        result = if cur.contains("\"evyro\"") {
                            format!("{rel} already lists evyro")
                        } else {
                            format!("{rel} exists; add the evyro entry yourself (left unchanged)")
                        };
                    } else {
                        evyro_core::util::write_atomic(&path, body.as_bytes())?;
                        result = format!("wrote {rel}");
                    }
                } else if write {
                    result = "this IDE is configured in its settings; see the instructions".into();
                }
                let v = json!({"ide": i.id, "mcp_setup": i.mcp_setup, "instruction_file": i.instruction_file, "result": result});
                o.print(&v, || {
                    format!(
                        "{}:\n  MCP: {}\n  Instruction file: {}\n  {}",
                        i.display_name, i.mcp_setup, i.instruction_file, result
                    )
                });
                return Ok(0);
            }
            if let Some(c) = evyro_core::targets::chat_apps()
                .into_iter()
                .find(|c| c.id == agent)
            {
                let v = json!({"chat": c.id, "mcp_setup": c.mcp_setup});
                o.print(&v, || {
                    format!(
                        "{}: {}",
                        c.display_name,
                        c.mcp_setup.unwrap_or(
                            "no MCP support; use `evyro switch --to` to get a chat pack"
                        )
                    )
                });
                return Ok(0);
            }
            let a = adapters::get(&agent)?;
            let mut v = json!({"agent": a.id, "mcp_setup": a.mcp_setup, "instruction_file": a.instruction_file});
            if hooks {
                if a.id != "claude-code" {
                    anyhow::bail!(Error::Unsupported(format!("automatic hook install is implemented for claude-code only; {} uses instruction files + MCP", a.display_name)));
                }
                let p = project(&store, &cli.project, &dir)?;
                v["hooks"] = json!(evyro_core::init::install_claude_hooks(&p.root())?);
            }
            o.print(&v, || {
                format!(
                    "{}:\n  MCP: {}\n  Instruction file: {}{}",
                    a.display_name,
                    a.mcp_setup,
                    a.instruction_file,
                    v.get("hooks")
                        .and_then(|h| h.as_str())
                        .map(|h| format!("\n  Hooks: {h}"))
                        .unwrap_or_default()
                )
            });
        }
        Cmd::Completions { .. }
        | Cmd::Version
        | Cmd::Restore { .. }
        | Cmd::BugReport { .. }
        | Cmd::CaptureUsage { .. } => {
            unreachable!()
        }
    }
    Ok(0)
}

fn task_cmd(
    store: &Store,
    cli_project: &Option<String>,
    dir: &Path,
    tc: TaskCmd,
    o: &Out,
) -> anyhow::Result<i32> {
    let p = project(store, cli_project, dir)?;
    match tc {
        TaskCmd::Add {
            title,
            desc,
            kind,
            priority,
            acceptance,
            mut constraints,
            files,
            needs_person,
            parent,
            ready,
        } => {
            constraints.extend(
                needs_person
                    .iter()
                    .map(|w| format!("{} {w}", evyro_core::tasks::NEEDS_PERSON)),
            );
            let t = store.create_task(
                &p.id,
                NewTask {
                    title,
                    description: desc,
                    kind,
                    priority,
                    parent,
                    acceptance,
                    constraints,
                    files,
                    status: if ready { Some(TaskStatus::Ready) } else { None },
                },
            )?;
            o.print(&t, || format!("Created {} {}", t.key, t.title));
        }
        TaskCmd::Show { task } => {
            let t = store.resolve_task(&p.id, &task)?;
            let deps = store.dependencies(&t.id)?;
            let cps = store.list_checkpoints(&p.id, Some(&t.id), 5)?;
            let rec = store.list_verifications(&p.id, Some(&t.id))?;
            let owner = store.active_session_for_task(&t.id)?;
            let comments = store.comments(&t.id)?;
            let criteria = store.criteria(&t)?;
            let v = json!({"task": t, "acceptance": criteria, "dependencies": deps, "owner": owner, "checkpoints": cps, "verifications": rec, "comments": comments});
            o.print(&v, || {
                let mut s = format!(
                    "{} {} [{}] {} P{}\n",
                    t.key, t.title, t.status, t.kind, t.priority
                );
                if !t.description.is_empty() {
                    s.push_str(&format!("{}\n", t.description));
                }
                for c in &criteria {
                    let mark = match (c.met, c.counts) {
                        (true, true) => "[x]",
                        (true, false) => "[?]",
                        _ => "[ ]",
                    };
                    s.push_str(&format!("  {mark} {}. {}", c.index, c.text));
                    if let Some(pv) = &c.provenance {
                        s.push_str(&format!("  ({pv})"));
                    }
                    s.push('\n');
                }
                if let Some(ow) = &owner {
                    s.push_str(&format!("owner: {} ({})\n", ow.harness, ow.id));
                }
                for d in &deps {
                    s.push_str(&format!("dep: {} {} {}\n", d.task, d.kind, d.depends_on));
                }
                for c in &cps {
                    s.push_str(&format!(
                        "checkpoint {} {} next: {}\n",
                        c.created_at, c.trigger, c.next_action
                    ));
                }
                for r in &rec {
                    s.push_str(&format!(
                        "verify {} `{}` {}\n",
                        if r.passed { "PASS" } else { "FAIL" },
                        r.command.join(" "),
                        r.started_at
                    ));
                }
                s
            });
        }
        TaskCmd::Check {
            task,
            index,
            note,
            receipt,
            unmet,
        } => {
            let t = store.resolve_task(&p.id, &task)?;
            let c = store.check_criterion(
                &t.id,
                index,
                !unmet,
                &note,
                receipt.as_deref(),
                &Actor::Human {
                    override_reason: None,
                },
            )?;
            let left = store.unmet_criteria(&t)?.len();
            o.print(&c, || {
                format!(
                    "{} criterion {} marked {}. {} of {} still open.",
                    t.key,
                    c.index,
                    if c.met { "met" } else { "not met" },
                    left,
                    t.acceptance.len()
                )
            });
        }
        TaskCmd::ApprovePieces { task } => {
            let t = store.resolve_task(&p.id, &task)?;
            confirm_person(
                &t.key,
                "approve the pieces of",
                std::env::var("EVYRO_SESSION_ID").is_ok_and(|v| !v.is_empty()),
            )?;
            let approved = store.approve_pieces(&p.id, &t.key)?;
            o.print(&approved, || {
                format!(
                    "Approved {} pieces of {}. Start them with `evyro work --under {}`.",
                    approved.len(),
                    t.key,
                    t.key
                )
            });
        }
        TaskCmd::Split { task, file } => {
            use std::io::Read;
            let text = if file.as_os_str() == "-" {
                let mut b = String::new();
                std::io::stdin().take(1 << 20).read_to_string(&mut b)?;
                b
            } else {
                std::fs::read_to_string(&file)?
            };
            let parts: Vec<evyro_core::split::SplitPart> = serde_json::from_str(&text)
                .map_err(|e| Error::Invalid(format!("the pieces are not valid JSON: {e}")))?;
            let by_ai = std::env::var("EVYRO_SESSION_ID").is_ok_and(|v| !v.is_empty());
            let r = store.split_task_as(&p.id, &task, &parts, by_ai)?;
            o.print(&r, || {
                let mut out = format!("Split {} into {} pieces:\n", r.parent, r.created.len());
                for t in &r.created {
                    let waits = store
                        .open_blockers(&t.id)
                        .map(|b| b.into_iter().map(|x| x.key).collect::<Vec<_>>())
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "  {}  {}{}\n",
                        t.key,
                        t.title,
                        if waits.is_empty() {
                            String::new()
                        } else {
                            format!("  (after {})", waits.join(", "))
                        }
                    ));
                }
                out.push_str(&if by_ai {
                    format!(
                        "An AI wrote these, so they wait for you: read them, then `evyro task approve-pieces {}`.",
                        r.parent
                    )
                } else {
                    format!("Start them with `evyro work --under {}` (one AI each).", r.parent)
                });
                out
            });
        }
        TaskCmd::Start {
            task,
            agent,
            model,
            provider,
            accept_drift,
        } => {
            let t = store.resolve_task(&p.id, &task)?;
            let agent = adapters::normalize_id(&agent);
            // An open handoff for this task is accepted through the normal
            // resume checks, so the new session carries the contract.
            if let Some(h) = store.pending_handoff_for_task(&t.id)? {
                if h.to_harness == agent || h.to_harness == "any" {
                    let r = store.resume_handoff(
                        &p.id,
                        Some(&h.id),
                        &agent,
                        accept_drift,
                        model.as_deref(),
                    )?;
                    o.print(&r, || out::render_resume(&r));
                    return Ok(if r.accepted {
                        0
                    } else {
                        Error::Drift(String::new()).exit_code()
                    });
                }
                if !o.quiet && !o.json {
                    eprintln!(
                        "note: handoff {} for {} is still open for {}; starting {} without it",
                        h.id, t.key, h.to_harness, agent
                    );
                }
            }
            // Inside `evyro run` the task goes to that run's session, so it
            // is freed when the agent exits instead of staying held.
            let ses = match store.running_session_from_env(&p.id) {
                Some(run) => store.claim_task(&p.id, &run.id, &t.id)?,
                None => store.start_session(
                    &p.id,
                    NewSession {
                        task_id: Some(t.id.clone()),
                        harness: agent.clone(),
                        model,
                        provider,
                        worktree: t.worktree.clone(),
                        pid: None,
                        handoff_id: None,
                    },
                )?,
            };
            let t = if t.status == TaskStatus::InProgress {
                t
            } else {
                store.update_task(
                    &t.id,
                    TaskUpdate {
                        status: Some(TaskStatus::InProgress),
                        ..Default::default()
                    },
                    &Actor::Agent(agent.clone()),
                )?
            };
            store.create_checkpoint(
                &p.id,
                NewCheckpoint {
                    task_id: Some(t.id.clone()),
                    session_id: Some(ses.id.clone()),
                    trigger: Some("task_started".into()),
                    ..Default::default()
                },
            )?;
            o.print(&json!({"task": t, "session": ses}), || {
                format!("{} started by {} (session {})", t.key, agent, ses.id)
            });
        }
        TaskCmd::Update {
            task,
            status,
            title,
            desc,
            priority,
            acceptance,
            files,
            needs_person,
            comment,
            agent,
            human_override,
        } => {
            let t = store.resolve_task(&p.id, &task)?;
            let actor = match (&agent, &human_override) {
                (Some(_), Some(_)) => anyhow::bail!(Error::Invalid(
                    "--human-override cannot be combined with --agent".into()
                )),
                (Some(a), None) => Actor::Agent(adapters::normalize_id(a)),
                (None, h) => Actor::Human {
                    override_reason: h.clone(),
                },
            };
            if let Some(c) = &comment {
                store.add_comment(&t.id, &actor.label(), c)?;
            }
            let t = store.update_task(
                &t.id,
                TaskUpdate {
                    status: status.as_deref().map(|s| s.parse()).transpose()?,
                    title,
                    description: desc,
                    priority,
                    add_acceptance: acceptance,
                    add_files: files,
                    add_constraints: needs_person
                        .iter()
                        .map(|w| format!("{} {w}", evyro_core::tasks::NEEDS_PERSON))
                        .collect(),
                    ..Default::default()
                },
                &actor,
            )?;
            if t.status.is_closed() {
                if let Some(s) = store.active_session_for_task(&t.id)? {
                    if t.status.is_terminal() {
                        store.set_session_status(
                            &s.id,
                            evyro_core::sessions::SessionStatus::Completed,
                            Some("task_closed"),
                        )?;
                    }
                }
            }
            o.print(&t, || format!("{} is {}", t.key, t.status));
        }
        TaskCmd::Dep { task, on, related } => {
            let a = store.resolve_task(&p.id, &task)?;
            let b = store.resolve_task(&p.id, &on)?;
            store.add_dependency(&a.id, &b.id, if related { "related" } else { "blocks" })?;
            o.print(&json!({"task": a.key, "depends_on": b.key}), || {
                format!("{} now depends on {}", a.key, b.key)
            });
        }
    }
    Ok(0)
}

/// Sessions whose recorded process is gone are closed out on read, so
/// `status` never shows a dead agent as running. Sessions with no recorded
/// pid (manual `task start`) are left for `evyro recover --include-unknown`.
fn auto_recover(store: &Store, project_id: &str, o: &Out) {
    if let Ok(rs) = store.recover_stale_sessions(project_id, false) {
        if !rs.is_empty() && !o.quiet && !o.json {
            for r in rs {
                eprintln!(
                    "note: {} session {} had exited; marked failed and checkpointed",
                    r.harness, r.session_id
                );
            }
        }
    }
}

/// `evyro work`: pick the next ready task and a free AI, and start it.
fn work_cmd(
    store: &Store,
    cli_project: &Option<String>,
    dir: &Path,
    a: WorkArgs,
    o: &Out,
) -> anyhow::Result<i32> {
    let p = project(store, cli_project, dir)?;
    let next = store.next_work(
        &p.id,
        a.agent.as_deref(),
        a.task.as_deref(),
        a.under.as_deref(),
    )?;
    let (task, agent) = (next.task, next.agent);
    let summary = format!(
        "{} (P{}) {} with {agent}",
        task.key, task.priority, task.title
    );
    if a.dry_run {
        o.print(&json!({"task": task.key, "agent": agent}), || {
            format!("Would start {summary}")
        });
        return Ok(0);
    }
    if !o.quiet && !o.json {
        eprintln!("Starting {summary}");
    }
    launch::run_agent(
        store,
        &p,
        launch::RunOpts {
            target: &agent,
            task: Some(&task.key),
            handoff: None,
            headless: a.headless,
            prompt: None,
            model: None,
            accept_drift: false,
            no_open: false,
            plan_first: false,
        },
        o,
    )
}

/// `evyro switch`: one confirmed step from the AI that is out of usage to the next one.
fn switch_cmd(
    store: &Store,
    cli_project: &Option<String>,
    dir: &Path,
    a: SwitchArgs,
    o: &Out,
) -> anyhow::Result<i32> {
    use std::io::{IsTerminal, Write};
    let p = project(store, cli_project, dir)?;
    let task = match &a.task {
        Some(r) => store.resolve_task(&p.id, r)?,
        None => match store.active_task(&p.id)? {
            Some(t) => t,
            // Nothing is being worked on, so there is no AI to stop: hand off and launch.
            None => {
                let args = HandoffArgs {
                    to: a.to,
                    task: None,
                    summary: String::new(),
                    done: vec![],
                    remaining: vec![],
                    next: String::new(),
                    model: a.model,
                    launch: true,
                    headless: a.headless,
                    from: None,
                    no_open: a.no_open,
                };
                return handoff_cmd(store, cli_project, dir, args, true, o);
            }
        },
    };
    // Refuse before asking or changing anything if the new AI could not start.
    store.check_switch_target(&a.to, a.model.as_deref())?;
    let holder = store.active_session_for_task(&task.id)?;
    let mut confirmed = a.yes || holder.is_none();
    if !confirmed {
        let who = holder
            .as_ref()
            .map_or("the current AI", |s| s.harness.as_str());
        if !std::io::stdin().is_terminal() || o.json {
            anyhow::bail!(
                "{} is held by {who}. If its usage has run out or is about to, run again with --yes",
                task.key
            );
        }
        eprint!(
            "{who} holds {}. Has its usage run out or is it about to? Evyro will stop it and start {} on this task. [y/N] ",
            task.key, a.to
        );
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin().read_line(&mut line)?;
        confirmed = matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes");
        if !confirmed {
            anyhow::bail!("not switched: {who} keeps {}", task.key);
        }
    }
    let out = store.switch_task_owner(&p.id, &task.key, &a.to, a.model.clone(), confirmed)?;
    let h = &out.handoff;
    if let Some(from) = &out.from {
        let stopped = from
            .pid
            .is_some_and(|pid| evyro_core::sessions::stop_session_process(pid, from.pid_started));
        if !o.quiet && !o.json {
            eprintln!(
                "{} ended on {}{}.",
                from.harness,
                task.key,
                if stopped {
                    " and was told to stop"
                } else if from.pid.is_some() {
                    " (its process had already exited)"
                } else {
                    "; no process was recorded, so close it yourself if it is still open"
                }
            );
        }
    }
    if a.no_launch {
        o.print(&out, || format!(
            "Handoff {} prepared for {} (quality {}/{}). Start it with `evyro run --agent {} --task {} --handoff {}`.",
            h.record.id, h.record.to_harness, h.contract.quality.met, h.contract.quality.total, h.record.to_harness, task.key, h.record.id));
        return Ok(0);
    }
    if !o.quiet && !o.json {
        eprintln!("Starting {} on {}…", h.record.to_harness, task.key);
    }
    launch::run_agent(
        store,
        &p,
        launch::RunOpts {
            target: &h.record.to_harness,
            task: Some(&task.key),
            handoff: Some(&h.record.id),
            headless: a.headless,
            prompt: None,
            model: a.model.as_deref(),
            accept_drift: false,
            no_open: a.no_open,
            plan_first: false,
        },
        o,
    )
}

fn handoff_cmd(
    store: &Store,
    cli_project: &Option<String>,
    dir: &Path,
    a: HandoffArgs,
    force_launch: bool,
    o: &Out,
) -> anyhow::Result<i32> {
    let p = project(store, cli_project, dir)?;
    let h = store.prepare_handoff(
        &p.id,
        HandoffRequest {
            task: a.task.clone(),
            to: a.to.clone(),
            to_model: a.model.clone(),
            summary: a.summary,
            completed: a.done,
            remaining: if a.remaining.is_empty() {
                None
            } else {
                Some(a.remaining)
            },
            next_action: a.next,
            actor: a.from.clone(),
        },
    )?;
    let launch = a.launch || force_launch;
    if !launch || o.json {
        o.print(&h, || format!(
            "Handoff {} prepared for {} (quality {}/{}).\n  contract: {}\n  readable: {}\n\nStart {} in this repository with this prompt (or run `evyro run --agent {}`):\n\n{}",
            h.record.id, h.record.to_harness, h.contract.quality.met, h.contract.quality.total, h.record.contract_path, h.markdown_path, h.record.to_harness, h.record.to_harness, h.resume_prompt));
        if !launch {
            return Ok(0);
        }
    } else if !o.quiet {
        eprintln!(
            "Handoff {} prepared (quality {}/{}). Launching {}…",
            h.record.id, h.contract.quality.met, h.contract.quality.total, h.record.to_harness
        );
    }
    launch::run_agent(
        store,
        &p,
        launch::RunOpts {
            target: &h.record.to_harness,
            task: a.task.as_deref(),
            handoff: Some(&h.record.id),
            headless: a.headless,
            prompt: None,
            model: a.model.as_deref(),
            accept_drift: false,
            no_open: a.no_open,
            plan_first: false,
        },
        o,
    )
}
