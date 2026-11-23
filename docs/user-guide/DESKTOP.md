# Desktop app

The Evyro desktop app is a window over the same local database the `evyro` CLI and MCP server use. Anything you do in one shows up in the others immediately. The CLI stays the full-power tool; the app makes everyday review and bookkeeping easier.

## Run it

| Goal | Command |
|---|---|
| Open the app from source | `cargo run --release -p evyro-desktop` |
| Build a macOS `.app` and `.dmg` | `cargo install tauri-cli --version "^2"`, then `cd crates/evyro-desktop && cargo tauri build` |
| Use a separate data folder (testing) | `EVYRO_HOME=/tmp/evyro-test cargo run -p evyro-desktop` |

Requirements: Rust 1.80+ and Git. On Linux, the WebKitGTK 4.1 development packages (`libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`). macOS and Windows need nothing extra.

## Screens

| Screen | What it shows | What you can do |
|---|---|---|
| Overview | Active task and owner, last checkpoint and next step, Git branch and changed files, health findings, recent activity (refreshes every few seconds) | Open the task, hand it off, copy a context packet for any agent |
| Changes | Files changed since the last commit (new files included) with line numbers, secrets hidden, and review comments inline | Comment on any line or file, **Send comments to agent** (into a running thread, or copied as a prompt), resolve or delete comments, copy the diff |
| Threads | Coding agents and shells running inside the window, one terminal each, with status and exit code; chats with agents over ACP | **+ Agent** starts any coding agent or API target on a task through `evyro run`; **+ Shell** opens a shell in the repository; type straight into the terminal, **Hand off** from a running agent, **Stop** or **Close** a thread |
| Tasks | Board by status with criteria progress, owner and blockers | Create tasks with acceptance criteria; open a task to tick criteria, change status, run tests, add notes |
| Handoffs | Every handoff with quality score and status | Prepare a handoff to any AI, read the signed contract, see drift, then **Start with this AI** or **Accept only** |
| Sessions | Every agent session with status and exit reason | Recover sessions whose agent is gone |
| Decisions & memory | Recorded decisions and project memory with search | Record a decision, save a memory |
| Skills | Every skill (built in, yours, this project's) with its source, version and whether it is on; type a task to see which fit | Read a skill, **Turn on** or **Turn off** here or in every project, **+ New skill**, **Import…**, **Add to agent files…** (never replaces a file someone wrote) |
| AI targets | Every AI you can switch to, grouped as coding agents, API keys, AI IDEs and chat apps, with status and next step; installed agent versions below | **+ Add target** (agent, API key by variable name or Keychain item, IDE, chat app), **Switch to**, open a target to see its checks or remove it |
| Settings | Verification commands, project details, Evyro data folder, doctor checks | Edit test/lint/build commands (saved to `.evyro/project.toml`), archive a project, back up the database. **Remote access**: start or stop serving to paired devices (they see this window's chats), **+ Pair a device** shows a one-time code, **Revoke** cuts a device off |

## Working faster

| Shortcut | Does |
|---|---|
| ⌘K / Ctrl+K | Command palette: every screen, new task, new handoff, switch to any AI, open any project |
| ⌘1 to ⌘9 / Ctrl+1 to Ctrl+9 | Jump to a screen, in tab order (Settings has no number; use ⌘K) |
| ⌘N / Ctrl+N | New task |
| ⌘P / Ctrl+P | Go to a file by name |
| ⌘⇧F / Ctrl+Shift+F | Search the code for text or a regex |
| ⌘⇧O / Ctrl+Shift+O | Go to a function, type or class |
| Esc | Close a dialog, the palette or the task drawer |

Inside a terminal, Ctrl keys go to the program (Ctrl+C, Ctrl+K, Ctrl+N work as in any terminal). On macOS the ⌘ shortcuts still work there; on Windows and Linux click **Ctrl K** in the top bar for the palette.

Everything works from the keyboard alone. Tab reaches every button, row and card, and Enter or Space opens a row. In a terminal, Tab goes to the program (for shell completion), so press **F6** to move focus out to the thread's buttons. The UI smoke test checks this on every screen.

**Live updates.** The app watches Evyro's database. When an agent, the CLI or MCP records a checkpoint, a handoff or a test run, the open screen refreshes within about two seconds. The top bar shows which AI is working on the project.

**Notifications.** While the window is in the background, Evyro shows a desktop notification when an agent stops, a handoff is prepared, accepted or reopened, or tests fail. macOS asks once for permission.

**Agent threads.** **Switch to** a coding agent (or **+ Agent** on the Threads screen) starts it in a thread inside the window. The thread runs `evyro run --to <target> --task <key>` through your login shell, so the agent sees the same PATH and keys as in your own terminal, and Evyro tracks the session, claims any waiting handoff and checkpoints when the agent exits. Threads keep running while you look at other screens and stop when you quit the app. To use your own terminal instead, pick **Open in Terminal instead** in the new-thread dialog.

**Code search.** **Search** in the top bar (or ⌘P) opens one panel with three modes: **Files** (fuzzy names, like `ratelim` for `rate_limiter.rs`), **Text** (with **Regex**, **Match case** and a path filter) and **Symbols**. **Tab** switches mode, the arrow keys move and Enter opens. A file opens read only with line numbers and an outline; click a symbol to jump to it, or **Copy path** for a `path:line` to paste to an AI. Search skips ignored, binary, very large and sensitive files, and hides secrets in what it shows.
**Chat threads (ACP).** **+ Chat** on the Threads screen talks to an agent over ACP (Agent Client Protocol) instead of a terminal. The conversation shows the agent's reply, its plan as a checklist, each tool call with its status (edits as a diff), and a card with the agent's options whenever it asks to run something; nothing runs until you pick one. **Stop turn** cancels the current turn, **Hand off** passes the task on, **End chat** stops the agent, checkpoints and closes the session. The agent gets Evyro's MCP tools and can read or write files only inside the project, never `.env` or keys. The thread list shows **needs you** while a permission request waits. Agents need their ACP side installed (the new-chat dialog shows the command); see [ACP](../protocols/ACP.md).

**First run.** With no projects yet, the welcome screen walks through adding a project (with a folder picker), checking your AIs and handing work over.

## Rules the app follows

- **Verified needs evidence.** Moving a task to Verified needs every acceptance criterion ticked and a passing test run on the current code. If that is missing, the app offers a human override, which asks for a reason and logs it.
- **No arbitrary commands.** "Run tests" runs only the commands configured in Settings, without a shell.
- **Agents run in a real terminal.** Coding agents run in a pseudo-terminal inside the window (a real PTY, so interactive CLIs behave as they do in Terminal), always through `evyro run`, never as a hidden background process. IDE and chat targets open the app with the hand-over text on the clipboard, saved under `.evyro/` as well.
- **No silent takeover.** If another AI still owns the task, **Switch to** opens the handoff form instead.
- **Keys stay out.** The add-target form takes a variable name or Keychain item name, never the key.
- **Untrusted text stays text.** Task text, agent notes and repository content are never rendered as HTML.

## Known gaps

- No tray icon or auto-update.
- Agent threads were checked by hand on Linux with a real PTY (a stand-in agent, a shell, typing, resizing, quitting). They have not been run with a real Claude Code or Codex yet, or on a real Mac or Windows desktop. The terminal engine's tests run in CI on all three systems.
- **Open in Terminal instead** uses macOS Terminal through AppleScript; elsewhere it copies the command.
- Checked by hand on Linux; opened on a real Mac (macOS 27.2) on 2026-11-23 with every screen rendering. An automated UI smoke test runs in CI on Linux, macOS and Windows. Notifications have not been checked on a real desktop yet.
