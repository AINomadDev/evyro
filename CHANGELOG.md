# Changelog

All notable changes to Evyro are listed here. Versions follow [Semantic Versioning](https://semver.org). A release's notes on GitHub come from its section below (`scripts/release/notes.sh`).

## [Unreleased]

### Added

- Custom agents from a TOML file: drop a file in `agents/` under Evyro's config directory or in a project's `.evyro/agents/` with a command, argv templates (`{prompt}`, `{cwd}`, `{model}`), environment variables and an optional login check. `evyro agents` lists them beside the built-in agents and `evyro doctor` reports bad files and failed login checks. Starting them with `evyro run` comes later.
- File ownership on the task board and in `evyro tasks`: a task blocked by a live AI that holds overlapping files shows **Waits for T-3 (codex)**, a task that merely shares files with others says so, and each card lists its declared files. Approving a plan declares the files it names that exist in the checkout (`evyro plan files T-4` previews them).
- A translation layer for the desktop app: strings are read from `locales/<code>.js` with `t("key")`, fall back to English, and support `{placeholders}` and plurals. The window frame, the shortcut list, the welcome screen, the guided tour, updates, agent threads, search and the file viewer, the Changes screen, agent comparison, remote access, skills, chat threads, the Appearance settings and the workspace (panes, the thread list, switching AI, the message box, the shared-context panel, usage, the terminal drawer and the status bar), and in the main window the project list, overview, task board and New task dialog are moved over; a **Language** setting offers English, following the system, and a pseudo-language that marks every moved string. A lint (run in CI) checks that every key exists, that other languages keep placeholders, and that the count of hard-coded strings per file can only go down. About 340 strings still remain to be moved.
- When a running agent prints that it hit its usage limit, its thread shows **limit hit?**, a note appears, and the pane's banner quotes the message and offers **Switch to <AI>…** (still confirmed by you) without waiting for a usage reading. The switch dialog shows what the AI printed.
- A keyboard shortcut list: press `?` (outside a field or terminal) or choose **Keyboard shortcuts** in the command palette. The palette now covers every action, including panes and focus mode, recover sessions, open any task, and run tests, hand off, switch AI, compare agents and PR feedback for the task in progress. Dialogs keep Tab inside them and return focus to the button that opened them. The smoke test presses every listed shortcut and checks the palette's coverage.
- `evyro sample` (and **Try the sample project** on the desktop welcome screen and in the command palette) creates a small real project inside Evyro's data folder, with two ready tasks on different files and a test command that works anywhere with Git. A checklist on the Tasks screen ticks each first-run step from what Evyro recorded: an AI started, a checkpoint saved, a test receipt kept, a handoff made.
- Two high-contrast themes (light and dark) with every text pair at 7:1 or better, solid borders and a thick focus ring. The System theme switches to them when the operating system asks for more contrast, and Windows contrast themes are honored. A **Motion** setting (Follow system, Reduce always) turns off animation, smooth scrolling and the terminal cursor blink; the system's reduced-motion request does the same. The accessibility check now runs in both themes and requires 7:1.
- `evyro pr feedback T-1` and **PR feedback…** on a task in the desktop turn the failing checks (with the end of their log) and unresolved review comments on the task's pull request into one prompt, with **Send to <AI>** for an AI running on the task. Logs are redacted and GitHub's text is fenced as data; `--links-only` leaves all of that text out and gives links instead. Needs the GitHub CLI.
- `evyro task split T-1 --file pieces.json` (and the `split_task` MCP tool) splits a task into up to eight pieces, each with the files it changes. Pieces that share files run one after the other, so two AIs never edit the same files at once. `evyro work --under T-1` then starts the next piece on a free AI; it refuses to run inside an AI's session, since starting an AI spends usage. Nothing starts by itself: you start the workers. Pieces an AI wrote (the `split_task` tool, or the command run inside an AI's session) also wait for you: `evyro work` skips them until you read them and run `evyro task approve-pieces T-1` at a terminal.
- Plan first: `evyro run --plan-first` (and **Plan first** in New agent thread) has an AI propose a plan for a task and wait. You read it with `evyro plan show`, approve or reject it with `evyro plan approve` / `reject` or on the task in the desktop, and the AI that builds gets the approved plan in its context. Claude Code and Codex start read-only; other AIs are asked. Approving needs a person: the command wants a real terminal and the task key typed, so an AI or script is refused, the desktop has a button, and the MCP tools (`propose_plan`, `get_plan`) cannot approve. This is a strong speed bump, not a lock against a program running as you.
- A running AI thread that prints nothing for ten minutes, or repeats the same line 30 times in a row, is marked "stuck?" and shows a banner with **Switch to <AI>…** and **Stop**. Nothing is stopped for you, since a long silent step can be real work.
- Sessions Evyro cannot watch as a process (a hand-started task, for instance) now count as live while they report in and as gone after 30 minutes of silence. Any checkpoint or event counts, and `evyro heartbeat` is there for a long quiet stretch. `evyro recover` and the task checkout no longer need a confirmation or wait on a session that went silent.
- A monthly dependency review: Dependabot opens one grouped update pull request a month for Rust crates and GitHub Actions, and a scheduled workflow re-runs `cargo deny` so a new advisory shows up even when no pull request is open.
- Desktop workspace for several AIs at once: threads side by side in 1, 2 or 4 panes, a sidebar that groups threads as Needs you, Working, Your turn and Ended, a shared-context panel (task and criteria, handoff, decisions, review notes, memory, activity across AIs, changed files) that sends any item to the focused AI, a box that messages several AIs at once, a terminal drawer and a status bar.
- The task board shows which AI is working on each card, with its latest line of output, and starts an AI from a card.
- Appearance settings: five themes, accent colors including your own, interface and terminal fonts and sizes, density, corner radius and layout options, with copy and paste of the settings as JSON.
- Threads and their output are saved and listed after the desktop app restarts (marked "earlier run"), with a **Resume** button; the pane layout is kept too.
- A **Usage, by AI** section in the shared-context panel: tasks each AI holds, runs, failed runs, time worked and when it was last active.
- A pane that stopped because a task is held by an AI that is gone offers **Free T-n and retry**.
- `evyro switch` and a desktop **Switch to <AI>…** step move a task to another AI (or another model) after you confirm the usage is spent: the old AI is stopped, the latest checkpoint carries over and only one AI holds the task. A task whose declared files overlap one another AI holds is refused.
- Stopping a session during a switch also stops the processes the AI started, so nothing it launched keeps running.
- `evyro work` and **Work on next task** start the most important ready task on a free AI with nothing to name. Evyro makes a worktree only when another AI is in the main checkout and the tasks can't be shown to touch different files; unfinished work (and a parent's idle worktree) is reused, overlapping files are refused, and `evyro worktrees prune` tidies finished ones.
- Moving a task to review, verified, done or cancelled ends the session that held it, so an AI's lease no longer outlives its work and another AI can take the task.
- `--needs-person <why>` on `evyro task add` and `update` marks work only a person can do (a phone, a certificate, a sign-in); `evyro work` and the desktop never hand it to an AI, and `evyro tasks` flags it.
- **Saved history** in Settings sets how many ended threads and chats stay saved (0 to 500 each; older ones are deleted) and has **Clear saved threads** and **Clear saved chats**, which ask first. Running threads and chats are never touched.
- Saved terminal output and chat transcripts are redacted and private to your user. Community files: code of conduct, support guide, issue and pull request templates.
- `docs/DEVELOPING_EVYRO_WITH_EVYRO.md` explains how several AIs build Evyro together.

- New coding agents: Amp, Qwen Code, Kiro CLI, Factory Droid, Cline CLI and Crush (from their docs; not run with Evyro yet). Copilot CLI and Cursor now start in a terminal pane.
- Local model servers: MLX, llama.cpp (also for Unsloth GGUF models) and vLLM, besides Ollama and LM Studio.
- Panes: grids of six and nine; starting an AI that is already running points to the running thread first.

### Fixed

- Saved chat transcripts and agent thread records are synced to disk before they replace the old file, and the previous version is kept as a last good copy. If a file is cut short or damaged anyway, loading falls back to that copy instead of dropping the chat or thread; other saved chats and threads are unaffected.
- Starting, ending and recovering a session, saving a checkpoint, preparing or accepting a handoff and changing a task now write all their rows in one transaction, so a failure part way leaves nothing half-written (no lease left on an ended session, no handoff checkpoint without its handoff). `evyro doctor` adds a **store consistency** check that reports leases, events, checkpoints and handoffs pointing at rows that are missing.
- A desktop dialog whose action finished late (sending a prompt to an AI, for instance) no longer closes a newer dialog that was opened in the meantime.
- Two AIs running `evyro` at the same moment no longer fail with "database is locked": writes that read first now take the write lock when they begin and wait their turn.
- Clicking anywhere in a terminal pane now focuses it, so Enter reaches prompts such as a workspace-trust question; a pane redraw no longer drops focus.
- Reopening a thread no longer types stray "3R3R3R" into the running AI: the terminal's replies to replayed output are ignored.
- Saved and replayed terminal output no longer starts in the middle of an escape sequence.
- The message box is compact, grows with the text and shows a line count with Discard for long drafts. Failed sends no longer say "Sent to 0 AIs".
- A running AI that is low on or out of usage shows a banner with **Continue with…**, suggesting the AI with the most usage left.
- `evyro integrate cursor --write` writes `.cursor/mcp.json`, with the full path to `evyro` when it is not on PATH.

- A task an agent starts from inside `evyro run` (CLI or MCP) now belongs to that run and is freed when the agent exits. Before, it stayed held by a session nobody watched, and every later thread on that task failed with "owned by session".
- `evyro run` with no task named no longer fails when the active task is held by another AI; it starts free.
- Agents started from the desktop find `evyro` by name, and the header names each running AI once.

### Changed

- Desktop screens: Overview and Threads became **Workspace**, Handoffs and Sessions became **History**, and Decisions & memory is now **Knowledge**.

## [0.2.0-rc.1] - 2026-11-23

### Added

- Agent threads: run an agent or a shell in a terminal inside the desktop app.
- Review workbench: comment on lines of a diff and send the comments to a running agent.
- Code search in the CLI, over MCP and in the desktop app: file names, text and regex, symbols and outlines.
- ACP chat (`evyro chat` and desktop chat threads), with approval for each tool call.
- Skills library: 15 built-in skills, project and global skills, checks, import, export and sync into agent files.
- Remote access (`evyro serve`): pair a phone or another computer, follow the work, approve agents, chat.
- Release pipeline: a version tag publishes installers, CLI archives for macOS, Linux and Windows, `SHA256SUMS`, an SBOM and a Homebrew formula, with signed build provenance. macOS signing and notarization switch on once the certificate secrets are set.
- Menu bar and tray icon. Closing the window while agents run hides it instead of stopping them.
- Desktop auto-update: the app checks for a newer release at startup and in Settings, and installs it on request after checking its signature.
- `evyro bug-report`: a Markdown report (versions, `evyro doctor`, recent crashes, with home paths and secrets hidden) to paste into an issue. The CLI and the desktop app write a crash log when they panic.
- Evyro copies the database aside before upgrading its schema, and keeps the last 5 copies.
- Install scripts for the CLI (`scripts/install.sh`, `scripts/install.ps1`) that check the release checksum before installing.

### Changed

- Headless Codex runs with `--sandbox workspace-write` and can write Evyro's data folder; headless Claude Code gets `--permission-mode acceptEdits` and may run only `evyro`; headless Gemini skips the folder-trust question; headless runs get a closed stdin.
- `.evyro/project.toml` accepts a command as a string, a list or a list of lists, and `evyro doctor` reports a file it cannot read.
- The desktop app can be used with the keyboard alone, and passes WCAG 2.1 AA checks (axe) on every screen in light and dark themes.

### Fixed

- A session whose process is gone no longer looks alive when the system reuses its process id.
- npm-installed agents (`.cmd` shims) launch on Windows.
- The CLI exits quietly when a reader closes its output early.

## [0.1.0] - 2026-11-23

First version: continuity core (tasks, sessions, checkpoints, signed continuation contracts, verification receipts, drift checks, crash recovery), the `evyro` CLI, the MCP server, the Tauri desktop app, and switching to any AI (agent CLIs, API keys, IDEs, chat apps).
