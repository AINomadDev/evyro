# Known limitations

Status as of 2026-11-23 (v0.1.0). Everything not listed under "Works" is pending.

## Works (tested in CI on Linux)

- Project registry, init, move detection, isolation between projects.
- Task graph with state machine, dependencies, cycle detection, ready work, leases.
- Sessions, checkpoints (manual, hook, adaptive dedupe, process exit, recovery).
- Signed continuation contracts, resume validation, drift detection.
- Verification receipts and the evidence gate on `verified` (all acceptance criteria met plus a passing receipt for the current code).
- Secret redaction in contracts, context, exports, MCP output and verification logs.
- Export/import, database backup/restore.
- MCP stdio server.
- Three-agent continuation using simulated harness binaries.
- A receiving agent that fails to launch reopens its handoff instead of stranding the task.
- Switch targets of every kind, with simulated binaries: an API target (key from an env var, model mapped for OpenCode), refusal before any state change when the key is missing, `app_login` removing `ANTHROPIC_API_KEY`, an IDE launch, a redacted chat pack, and continuing a handoff with a different AI than it named.

## Works (checked by hand on Linux)

- Desktop app: every screen opened against a real database under Xvfb on Linux; creating a task, checking criteria, running tests, accepting a handoff with drift, and saving commands all worked. On 2026-11-23: the AI targets screen, adding an API-key target from the form, and switching to a chat app through a handoff. There are no automated UI tests yet.

## Not yet verified

- **Real agents.** No real Claude Code, Codex, Gemini or OpenCode run has been recorded. Argv and MCP setup are from docs. The first real run on your Mac is the next acceptance step.
- **macOS and Windows.** Code is cross-platform by design (paths, PATHEXT, `tasklist` for pid checks) but only Linux has been built and tested. CI config for macOS and Windows exists and has not run yet. Pratik ran the CLI by hand on macOS on 2026-11-22 and the main flows worked.
- **Desktop app on macOS.** Built and used on Linux (WebKitGTK) only. macOS uses WKWebView; layout differences are possible.
- **Sign-in and key checks against real tools.** Login detection looks for each tool's credential file or Keychain item by name; the locations come from the tools' docs and have not been checked against real installs. Opening IDEs and chat apps, the clipboard (`pbcopy`), Keychain reads (`security`) and the Terminal launch use macOS commands that have not run on a Mac yet.
- **Real API keys.** No target has been run with a real key; model name mapping per agent follows each agent's docs.

## Not implemented

- Desktop: embedded terminal, native folder picker, notifications, tray icon, auto-update.
- Web UI, mobile companion, remote access.
- Terminal/PTY, diff viewer, review workflow, inline comments (planned via px0 integration).
- Code search, symbol index, LSP, codebase map.
- Skills registry, context deduplication across sessions, embeddings.
- Model discovery (listing a provider's models), quota tracking, routing policies, automation rules. A provider registry exists (`evyro targets providers`).
- GitHub Issues sync, Relay contract import/export, ACP.
- Handoff benchmark suite (fresh vs. transcript vs. summary vs. contract) with real agents.
- Installers (DMG, MSI, AppImage), signing, Homebrew/winget.
- Storing keys. By design Evyro never stores keys; targets can read one from the macOS Keychain at launch (`--key-keychain`).

## Known gaps in what exists

- Process liveness uses `kill -0` / `tasklist`; a recycled pid can make a dead session look alive.
- The contract HMAC key is per install, so contracts moved to another machine verify by content hash only.
- Redaction is pattern-based and can miss unusual secret formats.
- Checkpoint progress fields are agent-reported; Evyro verifies Git state and receipts, not prose.
- Chat targets cannot apply changes: the person pastes the pack and applies the answer. Readiness for chat targets is "unknown" because a web sign-in cannot be checked.
- A coding CLI Evyro does not know cannot be launched by `evyro run`; use `evyro resume --agent <name>` and start it yourself.
- Apps opened from the Dock do not see variables from `~/.zshrc`, so an env-var key target may look unset in the desktop app; use a Keychain target there.
