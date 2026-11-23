# Evyro

**Switch agents. Keep building.**

Evyro is a local-first continuity layer for AI coding work. It keeps the task, decisions, checkpoints, Git state and test evidence for each of your repositories, so the next AI picks up exactly where the previous one stopped: a coding agent (Claude Code, Codex, Gemini CLI, Cursor, Copilot, OpenCode, Goose, Aider), any model behind an API key, an AI IDE (Cursor, VS Code, Windsurf, Zed, Antigravity, JetBrains) or a chat app (ChatGPT, Claude, Gemini, Copilot).

Status: **v0.1.0: CLI, MCP server and a first desktop app.** See [Known limitations](docs/KNOWN_LIMITATIONS.md).

## Install from source

```bash
# Rust 1.80+ and git required
cargo install --path crates/evyro-cli
evyro doctor
```

### Desktop app

```bash
cargo run --release -p evyro-desktop          # opens the Evyro window
```

The desktop app reads and writes the same local database as the CLI, so both always show the same state. It covers the project overview, a task board with acceptance criteria, handoff review with drift checks, sessions, decisions and memory, detected agents, and settings. See [Desktop guide](docs/user-guide/DESKTOP.md). To build an installable `.app`/`.dmg`, use the Tauri CLI: `cargo install tauri-cli --version "^2" && cd crates/evyro-desktop && cargo tauri build`.

## Five-minute tour

```bash
cd ~/code/my-app
evyro init                                   # registers the repo, writes .evyro/ and agent entry files
evyro task add "Add rate limiter" --accept "429 after 10 req/s" --accept "tests pass"
evyro run --to claude-code --task T-1        # launches Claude Code with session tracking

# ...Claude hits its usage limit...
evyro switch --to codex                      # signed handoff, then Codex starts where Claude stopped
evyro switch --to cursor-ide                 # ...or open Cursor with the handoff on the clipboard
evyro switch --to chatgpt                    # ...or a self-contained chat pack for ChatGPT
evyro targets                                # every AI you can switch to, and whether it is ready

evyro verify -- cargo test                   # Evyro runs the tests and records a receipt
evyro task check T-1 1                       # tick off acceptance criteria (or in the desktop app)
evyro task check T-1 2
evyro task update T-1 --status verified      # needs every criterion met and a passing receipt for this exact code
```

Inside an agent session the agent uses the same commands (the generated `AGENTS.md` / `CLAUDE.md` / `GEMINI.md` tell it how) or the MCP server: `claude mcp add evyro -- evyro mcp`.

## What makes it different

- **Durable, portable state.** Projects, tasks, sessions, checkpoints and handoffs live in local SQLite; `.evyro/project.toml` keeps a stable project id across clones and moves.
- **Verified over claimed.** Git facts and test receipts are captured by Evyro. Agent statements are stored as *agent-reported*. A task reaches verified only when every acceptance criterion is met and a test run passed on the current code; an agent saying so is not enough.
- **Safe resume.** Contracts are signed. Resume checks branch, commit and per-file hashes and stops on drift instead of overwriting.
- **Crash-tolerant.** Checkpoints happen during work (hooks, `evyro run` exit, `evyro recover`), not only at the end.
- **Any AI, any way you pay.** Subscriptions (signed-in apps), API keys from an environment variable or the macOS Keychain, or local models. Evyro stores where a key is, never the key.
- **Private by default.** No cloud account. Secrets are redacted from everything Evyro exports.

## Documentation

| Topic | File |
|---|---|
| **User guide (start here)** | [docs/user-guide/USER_GUIDE.md](docs/user-guide/USER_GUIDE.md) |
| CLI reference | [docs/user-guide/CLI_REFERENCE.md](docs/user-guide/CLI_REFERENCE.md) |
| Desktop app | [docs/user-guide/DESKTOP.md](docs/user-guide/DESKTOP.md) |
| Handoff protocol | [docs/protocols/HANDOFF_PROTOCOL.md](docs/protocols/HANDOFF_PROTOCOL.md) |
| MCP server | [docs/protocols/MCP_INTEGRATION.md](docs/protocols/MCP_INTEGRATION.md) |
| Agent support | [docs/AGENT_COMPATIBILITY.md](docs/AGENT_COMPATIBILITY.md) |
| Architecture decisions | [docs/decisions/ARCHITECTURE_DECISIONS.md](docs/decisions/ARCHITECTURE_DECISIONS.md) |
| Research and fork-vs-build | [docs/research/](docs/research/) |
| Performance | [docs/PERFORMANCE.md](docs/PERFORMANCE.md) |
| Backlog | [docs/BACKLOG.md](docs/BACKLOG.md) |
| Limitations | [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md) |

## Layout

```text
crates/evyro-core   continuity engine (storage, tasks, sessions, checkpoints, handoff, context, memory, verify, adapters)
crates/evyro-mcp    MCP stdio server
crates/evyro-cli    the `evyro` binary + end-to-end scenario tests
crates/evyro-desktop  Tauri 2 desktop app (Rust commands + plain HTML/JS UI in ui/)
schemas/            continuation contract JSON Schema
docs/               research, ADRs, protocols, guides
```

## License

MIT OR Apache-2.0.
