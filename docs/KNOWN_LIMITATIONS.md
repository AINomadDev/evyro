# Known limitations

Status as of 2026-11-23 (v0.1.0). Everything not listed under "Works" is pending.

## Works (tested in CI on Linux and macOS; unit tests and builds also on Windows)

- Project registry, init, move detection, isolation between projects.
- Task graph with state machine, dependencies, cycle detection, ready work, leases.
- Sessions, checkpoints (manual, hook, adaptive dedupe, process exit, recovery).
- Signed continuation contracts, resume validation, drift detection.
- Verification receipts and the evidence gate on `verified` (all acceptance criteria met plus a passing receipt for the current code).
- Secret redaction in contracts, context, exports, MCP output and verification logs.
- Export/import, database backup/restore.
- MCP stdio server.
- Three-agent continuation using simulated harness binaries.
- Code search through the CLI and MCP (scenario test): fuzzy file names, text and regex, symbols and outlines; `.gitignore`, binary and sensitive files left out, secrets in matched lines redacted, paths outside the repository refused.
- ACP chat (`evyro chat`) with a stand-in ACP agent: streamed plan, tool calls and permission requests, Evyro's MCP server reached from inside the agent, file access limited to the project (`.env` and paths outside refused), `--approve` policies, cancellation, a handoff picked up, a checkpoint at the end.
- A receiving agent that fails to launch reopens its handoff instead of stranding the task.
- Skills library: 15 built-in skills, on and off per project and globally, a project skill replacing a built-in one, `check` refusing a skill with a secret, import refusing secrets and older versions (`--force` replaces), export, sync for Claude Code that keeps a person's own skill file and removes its own file when a skill is turned off, sync for Codex that leaves `AGENTS.md` alone without `--append` and then changes only its own section, the context packet listing fitting skills without their text, and the MCP tools `list_skills` and `get_skill` (a skill that is off cannot be loaded). Desktop Skills screen in the UI smoke test.
- Switch targets of every kind, with simulated binaries: an API target (key from an env var, model mapped for OpenCode), refusal before any state change when the key is missing, `app_login` removing `ANTHROPIC_API_KEY`, an IDE launch, a redacted chat pack, and continuing a handoff with a different AI than it named.

- Desktop UI smoke test (Playwright): every screen, the add-target form, the command palette, code search and the file viewer, shortcuts, live refresh and first-run setup, on Linux, macOS and Windows.
- Desktop app builds on Linux, macOS and Windows.
- Installers build in CI (2026-11-23): a universal macOS .dmg, Windows .msi and .exe, Linux .deb and .AppImage, each with SHA-256 checksums.

## Works (checked on a real Mac, 2026-11-23, macOS 27.2 arm64)

- Workspace and desktop build, clippy and fmt, and every step of `scripts/mac-smoke-test.sh`.
- Detection of installed, signed-in Claude Code, Codex and Gemini CLI (version and login evidence).
- Keychain check for a missing item, with no password prompt; a pasted key refused.
- Chat hand-over: clipboard (`pbcopy`) and the browser opened.
- The desktop app opened with the full UI.

## Works (checked by hand on Linux)

- Desktop app under Xvfb: creating a task, checking criteria, running tests, accepting a handoff with drift, saving commands, the AI targets screen, adding a target, switching to a chat app through a handoff, the Changes view, the palette and live refresh after a CLI checkpoint.
- Agent threads in the real app under Xvfb (2026-11-23): an agent thread ran `evyro run --to aider --task T-1` with a stand-in agent in a PTY, took typed input, was tracked as a session (top bar showed "With aider") and checkpointed on exit; a shell thread ran commands and followed window resizes; quitting the app stopped a running thread's program.
- Code search in the real app under Xvfb (2026-11-23): Ctrl+P found `rate_limiter.rs` from "ratelim", text search showed both matches with the token in `config.ts` as `[REDACTED:github_token]` and left `.env` out, symbol search jumped to `allow_request`, and the file viewer opened at line 6 with its outline.
- Skills screen in the real app under Xvfb (2026-11-23): the list showed the 15 built-in skills plus a project skill; **Add to agent files…** for Codex left an `AGENTS.md` someone wrote unchanged, then with "add the list at its end" ticked appended only Evyro's section; **Turn off** for this project stored the choice, and `evyro skills list` showed it.
- Chat threads (ACP) in the real app under Xvfb (2026-11-23): **+ Chat** started a stand-in ACP agent on T-1; the conversation showed its plan, tool calls, the task read through Evyro's MCP server and the refusals for `.env` and `../outside.txt`; **Allow** on its permission card let it write `notes.txt`; **End chat** stopped the agent and closed the session with a checkpoint listing "Edited notes.txt".
- Review workbench in the real app under Xvfb (2026-11-23): commented on a diff line, saw it inline, sent it to a running stand-in agent thread; the agent (in raw mode with bracketed paste on, like real agent TUIs) received it as one bracketed message submitted with Enter, and the comment showed as sent.

## Not yet verified

- **Real ACP agents.** Chat has been run only with a stand-in ACP agent. Its messages matched the official ACP v1 schema in a hand check (2026-11-23). No run with Claude Agent ACP, Codex ACP, Gemini CLI, Goose, OpenCode or Copilot has been recorded; their commands come from their docs.
- **Real agents.** No real Claude Code, Codex, Gemini or OpenCode run has been recorded. Argv and MCP setup are from docs. The first real run on your Mac is the next acceptance step.
- **Windows end-to-end.** Unit tests and builds pass on Windows CI, but the end-to-end scenario tests use shell-script stand-ins for agents and run only on Linux and macOS. No one has used Evyro on a Windows machine.
- **Notifications, agent threads and the Terminal launch** on a real Mac or Windows desktop. Agent threads were checked by hand on Linux with a real PTY and a stand-in agent.
- **Installers on real machines.** The installers build in CI but have not been installed on a real Mac, Windows or Linux machine yet. They are not code-signed or notarized (needs an Apple Developer ID and a Windows certificate), so macOS and Windows will warn on first open.
- **Sign-in and key checks against real tools.** Login detection is confirmed for Claude Code, Codex and Gemini CLI on a real Mac; the others follow their docs. Opening IDEs from Evyro has not been checked on a real Mac yet.
- **Real API keys.** No target has been run with a real key; model name mapping per agent follows each agent's docs.

## Not implemented

- Desktop: tray icon, auto-update.
- Web UI, mobile companion, remote access.
- Code intelligence beyond search: tree-sitter parsing, find references, LSP (types, go to definition across files), a codebase map. Symbols come from per-language patterns, so an unusual declaration can be missed.
- Skills registry, context deduplication across sessions, embeddings.
- Model discovery (listing a provider's models), quota tracking, routing policies, automation rules. A provider registry exists (`evyro targets providers`).
- GitHub Issues sync, Relay contract import/export.
- Skills: matching a skill to a task is word matching on names and tags, not a model. Skills hold one `SKILL.md`; extra files in a skill folder (scripts, references) are not imported or synced. Single-project `evyro export` does not include on/off choices (a full `evyro backup` does).
- ACP: `terminal/*`, `authenticate`, `session/load`, session modes, and image or audio prompts (see [ACP](protocols/ACP.md)).
- Handoff benchmark suite (fresh vs. transcript vs. summary vs. contract) with real agents.
- Code signing and notarization, Homebrew/winget, published releases.
- Storing keys. By design Evyro never stores keys; targets can read one from the macOS Keychain at launch (`--key-keychain`).

## Known gaps in what exists

- Process liveness uses `kill -0` / `tasklist`; a recycled pid can make a dead session look alive.
- The contract HMAC key is per install, so contracts moved to another machine verify by content hash only.
- Redaction is pattern-based and can miss unusual secret formats.
- Checkpoint progress fields are agent-reported; Evyro verifies Git state and receipts, not prose.
- Chat targets cannot apply changes: the person pastes the pack and applies the answer. Readiness for chat targets is "unknown" because a web sign-in cannot be checked.
- A coding CLI Evyro does not know cannot be launched by `evyro run`; use `evyro resume --agent <name>` and start it yourself.
- Apps opened from the Dock do not see variables from `~/.zshrc`, so an env-var key target may look unset in the desktop app; use a Keychain target there.
