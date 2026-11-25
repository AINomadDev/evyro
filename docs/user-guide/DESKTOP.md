# Desktop app

The Evyro desktop app is a window over the same local database the `evyro` CLI and MCP server use. Anything you do in one shows up in the others immediately. The CLI stays the full-power tool; the app makes everyday review and bookkeeping easier.

## Run it

| Goal | Command |
|---|---|
| Open the app from source | `cargo run --release -p evyro-desktop` |
| Build a macOS `.app` and `.dmg` | `cargo install tauri-cli --version "^2"`, then `cd crates/evyro-desktop && cargo tauri build` |
| Use a separate data folder (testing) | `EVYRO_HOME=/tmp/evyro-test cargo run -p evyro-desktop` |

Requirements: Rust 1.80+ and Git. On Linux, the WebKitGTK 4.1 development packages (`libwebkit2gtk-4.1-dev`, `libsoup-3.0-dev`). macOS and Windows need nothing extra.

### First launch of a downloaded build

The release builds aren't code-signed yet.

**macOS:** you'll see "Apple could not verify Evyro is free of malware". Recent versions of macOS no longer let you right-click and choose Open to get past this. Do this instead:

1. Drag Evyro to Applications, try to open it once, and click **Done**.
2. Open **System Settings › Privacy & Security**, scroll to **Security**, and click **Open Anyway** next to the Evyro message.
3. Open Evyro again, click **Open Anyway**, and enter your password.

Or run `xattr -dr com.apple.quarantine /Applications/Evyro.app` once in Terminal. Either way, macOS asks only the first time. Before you do, check the download against `SHA256SUMS` if you want to be sure it's the real file.

**Windows:** in the SmartScreen box, click **More info**, then **Run anyway**.

## The window

| Area | What it shows | What you can do |
|---|---|---|
| Sidebar | Your projects, then the open project's AI threads grouped as **Needs you** (a chat waiting for approval), **Working**, **Your turn** (a chat waiting for your next message) and **Ended**, each with a colored agent badge and its task | Click a thread to show it in the focused pane; **+ Agent**, **+ Chat**, **+ Shell** |
| Shared context (right, or left in Appearance) | What every AI in the project can draw from, live: the active task and its acceptance criteria, owner and next step, a waiting handoff, decisions, open review notes, memory, and activity from all AIs. The **Changes** tab lists changed files and which have review notes | **Send** a decision, memory note, the task's context or the open review notes to the focused AI; open the task or the handoff. ⌘. / Ctrl+. or **Hide context** hides it |
| Status bar | Branch and changed files, how many AIs are running, how many need you, a waiting handoff | |

## Screens

| Screen | What it shows | What you can do |
|---|---|---|
| Workspace | Terminal and chat threads with an adjustable pane count and column count, saved per project | Use the **Panes** stepper and **Columns** menu to adjust the grid. A new thread fills a free pane or adds one. **Focus pane** expands the selected thread, the picker switches threads in Focus, and **Exit focus** restores the grid. **Review changes** opens the diff and review comments |
| Changes | Files changed since the last commit (new files included) with line numbers, secrets hidden, and review comments inline | Comment on any line or file, **Send comments to agent** (into a running thread, or copied as a prompt), resolve or delete comments, copy the diff |
| Tasks | Board by status with criteria progress, owner and blockers. A card with an AI on it shows that AI, its status and its latest line of output | Create tasks with acceptance criteria; open a task to tick criteria, change status, run tests, add notes; **Start with…** puts an AI on a card; **Open** or **Answer** jumps to its thread |
| History | Every handoff with quality score and status, then every agent session with status and exit reason | Prepare a handoff to any AI, read the signed contract, see drift, then **Start with this AI** or **Accept only**; recover sessions whose agent is gone |
| Knowledge | Recorded decisions and project memory with search | Record a decision, save a memory |
| Skills | Every skill (built in, yours, this project's) with its source, version and whether it is on; type a task to see which fit | Read a skill, **Turn on** or **Turn off** here or in every project, **+ New skill**, **Import…**, **Add to agent files…** (never replaces a file someone wrote) |
| AI targets | Every AI you can switch to, grouped as coding agents, API keys, AI IDEs and chat apps, with status and next step; installed agent versions below | **+ Add target** (agent, API key by variable name or Keychain item, IDE, chat app), **Switch to**, open a target to see its checks or remove it |
| Settings | Appearance, verification commands, project details, saved history, Evyro data folder, doctor checks | Change the look (see below). **Saved history** sets how many ended threads and chats stay saved and deletes them on request (see below). Edit test/lint/build commands (saved to `.evyro/project.toml`), archive a project, back up the database. **Remote access**: start or stop serving to paired devices (they see this window's chats), **+ Pair a device** shows a one-time code, **Revoke** cuts a device off |

Choose **Columns → Side by side** to keep panes in one horizontal row. New threads extend the row and scroll into view when needed. Choose a numbered column count for a grid with a fixed width in columns.

## Threads and shared messages

- Choose **+ Agent**, pick an AI and a task, and enter an optional thread name. **Resume** reopens the saved terminal thread in place, keeps its scrollback, and relaunches the agent with Evyro's task and checkpoint context. A Copilot thread created with a session credit budget also retains its Copilot session ID.
- The shared composer can target any running AI thread in the current project, including threads outside the visible panes. Use the recipient buttons or **All AIs**. If some deliveries fail, the composer keeps the message and selects the failed recipients for retry.
- Opening an agent terminal prepares a task-context draft in the shared composer or through **Load draft** when the composer already has text. A new ACP chat prepares its own opening draft. Edit the text and choose **Send** to start work.
- Use **+ Shell** for shell commands and **+ Chat** for agents that support ACP. The shared composer sends multiline messages to ACP chats and terminal agents that are ready for bracketed paste.
- A stopped terminal displays saved output. Choose **Restart shell** to open a fresh shell in the same pane with its scrollback, or **Resume** to relaunch a saved agent thread.
- If Cursor opens to a sign-in screen, run `cursor-agent login` or follow the pane's prompt. If Copilot asks about folder trust, review the directory and answer in its pane before retrying the message.
- For individual Gemini Code Assist accounts, choose `antigravity-cli` in **+ Chat** after signing in with `agy acp --login`. Cursor chat uses the CLI account from `cursor-agent login`.

## Compare two agent implementations

Open a task and choose **Compare agents**. Pick two agent targets and select **Prepare comparison**. Evyro creates two child tasks and two Git worktrees at the same committed revision. Uncommitted edits in your checkout stay there.

Use **Start agent** on each candidate when you’re ready. Choose **Run tests in both** to run the project's test command in each worktree one after the other (it can take a while), or run your own verification there. **Refresh results** shows each candidate's diff, a files and lines summary, and its latest test result. **Prefer this result** records your choice and leaves both worktrees available.

To move the preferred result into the original task’s checkout, finish any active sessions in both checkouts and commit the candidate’s changes. Both checkouts must be clean. Select **Preview staging** to check the destination and review the file list, then choose **Stage changes**. Evyro rechecks the preview and stages the candidate’s committed changes for you to review and commit. If Git detects a conflict, handle it before trying again.

## Usage and continuation

**Usage, by AI** refreshes supported readings in the background while you use the Context panel. Click **Refresh** to check sooner. The timestamp identifies the last successful reading. Account limits are shared across the account's threads.

| Agent | How to get usage | Change the model in a running terminal |
|---|---|---|
| Codex | The signed-in CLI reports quota windows, resets, and any credit balance. | `/model` |
| GitHub Copilot | The CLI reports a plan snapshot that can lag. Choose a session credit budget of 30 or more when starting a thread. The remaining budget updates from Copilot's complete report on a normal CLI exit with `/exit`. | `/model` |
| Antigravity | On macOS and Linux, Evyro reads the installed CLI's status line in a separate session without sending a prompt. The terminal's `/usage` shows quota if you want to check it directly. | `/model` |
| Claude Code | Choose **Connect usage** for the status-line command. Claude Code can report eligible subscription limits after its first normal model response. For API usage and cost, use [Claude Console](https://platform.claude.com/usage). | `/model` |
| Cursor | Check the Cursor or Grok usage dashboard for the account's plan. Both dashboard links are available in the panel. | `/model` |
| OpenCode | Evyro runs `opencode stats` in the background for the current project's last seven days. The connected provider reports balances and resets. | `/models` |

An ACP chat shows **Chat model** when its agent advertises model selection. Pick a model while the chat is idle to keep using that session and its transcript.

Ended ACP chats stay in the desktop sidebar as **saved chats**. Open one to read its transcript, or use **Delete transcript** to remove the saved copy. Evyro keeps up to 60 saved chats on this computer, with up to 512 KiB per transcript. It saves active chats about every 10 seconds and when the app quits. Saved chats are read-only and only appear on the desktop.

Choose **Search → Saved chats** to find a word or phrase in user messages and agent replies for the current project. Select a result to open its transcript at that message.

If the account limit is shared across the available models, use **Continue with…** in a task's pane to move to another AI. Evyro fills the handoff from the last checkpoint. Review the summary and next action, prepare the handoff, then start the chosen AI. Its first message is an editable draft that points to the signed task checkpoint and the repo changes. Send it when you're ready.

When an AI's usage is low or out, the banner offers **Switch to <AI>…**. Pick the next AI (or the same AI with another model), tick that the usage has run out or is about to, and choose **Switch AI**. Only one AI works on a task at a time, so Evyro stops the old AI (keeping its saved output), carries the latest checkpoint into a handoff and starts the next AI with an editable draft. **Work on next task** (Tasks view, the empty workspace and the command palette) starts the most important ready task on an AI that is free, with nothing to pick. Evyro makes a worktree only when another AI is in the main checkout and the two tasks can't be shown to touch different files; unfinished work keeps its worktree. If a new thread is refused because another AI holds the task, its banner offers **Switch T-n here…** for the same step. Work since the last checkpoint stays in the files but is not described, so ask the first AI to save a checkpoint beforehand if it can.

**A thread that looks stuck.** A running agent thread that prints nothing for ten minutes, or repeats the same line 30 times in a row, shows **stuck?** in the sidebar and a banner with **Switch to <AI>…** and **Stop**. Evyro stops nothing by itself: a long silent step such as a big test run can be real work. The check reads plain terminal output only, so a program that redraws its screen in place rather than printing new lines is judged by silence alone; chats are not checked yet. See [Developing Evyro with Evyro](../DEVELOPING_EVYRO_WITH_EVYRO.md).

## Away-from-home access

Keep the Mac awake with Evyro serving on `127.0.0.1`. Connect the Mac and phone to the same Tailscale network. They can use different Wi-Fi networks, or the phone can use mobile data. In **Settings → Remote access → Away from home**, choose **Check private address**, then **Enable private address** if needed. Select **Pair over Tailscale** and use the displayed address and one-time code on the phone. Use the **control** pairing scope to start or message ACP chats remotely. See [Remote access](../REMOTE_ACCESS.md) for setup and troubleshooting.

## Saved history

Ended threads and chat transcripts are saved on this computer so you can reopen them. Settings › **Saved history** controls how many stay:

- **Saved threads to keep** and **Saved chats to keep** take a whole number from 0 to 500 (default 60 each). When you go over a limit the oldest ended ones are deleted at once; 0 keeps none.
- **Clear saved threads** and **Clear saved chats** delete every ended one after you confirm. Threads and chats that are still running are never touched.

Saved output has secrets redacted and is readable only by you. Deleting is permanent: the files are removed from disk.

## Appearance

Settings › **Appearance** changes the look at once and keeps it on this computer.

| Group | Options |
|---|---|
| Theme and color | Theme: System, Light, Dark, Midnight, Paper. Accent: seven colors, or any color you pick (Evyro warns when it is too faint to read as text). Density: Comfortable or Compact. Corner radius |
| Typography | Interface font (System, Inter, Atkinson Hyperlegible, Rounded, Serif, or any installed font by name), text size, code and terminal font (JetBrains Mono, system mono, Fira Code, Cascadia Code, Iosevka, or any installed font), terminal text size |
| Layout | How many panes the Workspace opens with, which side the shared context sits on and whether it opens on start, sidebar, context and terminal drawer sizes, sidebar threads by state or newest first, status bar on or off |

**Copy settings** puts them on the clipboard as JSON, and **Paste settings** applies JSON someone shared (unknown keys and bad values are ignored). **Reset** goes back to the defaults. Fonts are not downloaded: a font that is not installed falls back to the system font.

## Working faster

| Shortcut | Does |
|---|---|
| ⌘K / Ctrl+K | Command palette: every screen, new task, new handoff, switch to any AI, open any project |
| ⌘1 to ⌘8 / Ctrl+1 to Ctrl+8 | Jump to a screen, in tab order |
| ⌘. / Ctrl+. | Show or hide the shared context |
| Ctrl+` | Open or close the terminal drawer |
| ⌘N / Ctrl+N | New task |
| ⌘P / Ctrl+P | Go to a file by name |
| ⌘⇧F / Ctrl+Shift+F | Search the code for text or a regex |
| ⌘⇧O / Ctrl+Shift+O | Go to a function, type or class |
| Esc | Close a dialog, the palette or the task drawer |

Inside a terminal, Ctrl keys go to the program (Ctrl+C, Ctrl+K, Ctrl+N work as in any terminal). On macOS the ⌘ shortcuts still work there; on Windows and Linux click **Ctrl K** in the top bar for the palette.

Everything works from the keyboard alone. Tab reaches every button, row and card, and Enter or Space opens a row. In a terminal, Tab goes to the program (for shell completion), so press **F6** to move focus out to the thread's buttons. The UI smoke test checks this on every screen.

**Live updates.** The app watches Evyro's database. When an agent, the CLI or MCP records a checkpoint, a handoff or a test run, the open screen refreshes within about two seconds. The top bar shows which AI is working on the project.

**Notifications.** While the window is in the background, Evyro shows a desktop notification when an agent stops, a handoff is prepared, accepted or reopened, or tests fail. macOS asks once for permission.

**Agent threads.** **Switch to** a coding agent (or **+ Agent** in the sidebar) starts it in a thread inside the window. The thread runs `evyro run --to <target> --task <key>` through your login shell, so the agent sees the same PATH and keys as in your own terminal, and Evyro tracks the session, claims any waiting handoff and checkpoints when the agent exits. Threads keep running while you look at other screens and stop when you quit the app. To use your own terminal instead, pick **Open in Terminal instead** in the new-thread dialog.

**Code search.** **Search** in the top bar (or ⌘P) opens one panel with three modes: **Files** (fuzzy names, like `ratelim` for `rate_limiter.rs`), **Text** (with **Regex**, **Match case** and a path filter) and **Symbols**. **Tab** switches mode, the arrow keys move and Enter opens. A file opens read only with line numbers and an outline; click a symbol to jump to it, or **Copy path** for a `path:line` to paste to an AI. Search skips ignored, binary, very large and sensitive files, and hides secrets in what it shows.
**Chat threads (ACP).** **+ Chat** in the sidebar talks to an agent over ACP (Agent Client Protocol) instead of a terminal. The conversation shows the agent's reply, its plan as a checklist, each tool call with its status (edits as a diff), and a card with the agent's options whenever it asks to run something; nothing runs until you pick one. **Stop turn** cancels the current turn, **Continue with…** prepares a checkpoint handoff for a tracked task, **End chat** stops the agent, checkpoints and closes the session. The agent gets Evyro's MCP tools and can read or write files only inside the project, never `.env` or keys. The sidebar lists the chat under **Needs you** while a permission request waits. Agents need their ACP side installed (the new-chat dialog shows the command); see [ACP](../protocols/ACP.md).

**First run.** With no projects yet, the welcome screen walks through adding a project (with a folder picker), checking your AIs and handing work over.

**Updates.** A few seconds after it starts, the app checks once for a newer release. When there is one, a button at the bottom of the sidebar says **Update to x.y.z**. Settings › **Updates** shows the same thing, with **Check now** and a switch to turn the startup check off. Nothing is downloaded until you click **Install and restart**. The app then checks the download's signature, installs it and restarts. Agents running in the window stop first, and their progress is checkpointed. On Linux only the AppImage updates itself; a `.deb` updates through a new download.

## Menu bar and tray

Evyro puts an icon in the macOS menu bar, or the Windows or Linux tray. Click it to bring the window back. Its menu has **Show Evyro** and **Quit Evyro**.

If you close the window while agent threads or chats are still running, Evyro hides the window instead of stopping them, and a notification says so. On macOS, clicking the Dock icon also brings it back. Quitting from the menu stops every running agent and closes their sessions with a checkpoint. With nothing running, closing the window quits as before.

On Linux the icon needs `libayatana-appindicator3-1`. The `.deb` installs it. Without it, Evyro runs with no tray icon, and closing the window always quits.

## Rules the app follows

- **Verified needs evidence.** Moving a task to Verified needs every acceptance criterion ticked and a passing test run on the current code. If that is missing, the app offers a human override, which asks for a reason and logs it.
- **No arbitrary commands.** "Run tests" runs only the commands configured in Settings, without a shell.
- **Agents run in a real terminal.** Coding agents run in a pseudo-terminal inside the window (a real PTY, so interactive CLIs behave as they do in Terminal), always through `evyro run`, never as a hidden background process. IDE and chat targets open the app with the hand-over text on the clipboard, saved under `.evyro/` as well.
- **No silent takeover.** If another AI still owns the task, **Switch to** opens the handoff form instead.
- **Keys stay out.** The add-target form takes a variable name or Keychain item name, never the key.
- **Untrusted text stays text.** Task text, agent notes and repository content are never rendered as HTML.

## Known gaps

- Agent threads were checked by hand on Linux with a real PTY (a stand-in agent, a shell, typing, resizing, quitting). They have not been run with a real Claude Code or Codex yet, or on a real Mac or Windows desktop. The terminal engine's tests run in CI on all three systems.
- **Open in Terminal instead** uses macOS Terminal through AppleScript; elsewhere it copies the command.
- Checked by hand on Linux; opened on a real Mac (macOS 27.2) on 2026-11-23 with every screen rendering. An automated UI smoke test runs in CI on Linux, macOS and Windows. Notifications have not been checked on a real desktop yet.
