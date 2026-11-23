# Agent compatibility

**Evidence levels.** *Simulated*: exercised in CI by a stand-in script with the same binary name and argv shape. *Documented*: argv and MCP setup come from the agent's docs or source. *Real*: run by hand against the real, signed-in agent (none in CI; real agents need accounts). Run `evyro agents` on your machine to see detection and version checks.

| Agent | Binary | Headless argv | Interactive argv | Instruction file | MCP | Hooks | Evidence |
|---|---|---|---|---|---|---|---|
| Claude Code | `claude` | `claude -p <prompt> --permission-mode acceptEdits --allowedTools "Bash(evyro *)"` | `claude <prompt>` | CLAUDE.md | yes | yes (`evyro integrate claude-code --hooks`) | documented + simulated |
| Codex CLI | `codex` | `codex exec --sandbox workspace-write --add-dir <evyro data dir> <prompt>` | `codex <prompt>` | AGENTS.md | yes | no (fallback: instructions + MCP) | documented + simulated + real (macOS, 2026-11-23) |
| Gemini CLI | `gemini` | `gemini -p <prompt>` (with `GEMINI_CLI_TRUST_WORKSPACE=true`) | `gemini -i <prompt>` | GEMINI.md | yes | no | documented + simulated |
| OpenCode | `opencode` | `opencode run <prompt>` | `opencode --prompt <prompt>` | AGENTS.md | yes | no | documented (source at 4332e24) |
| Cursor | `cursor-agent` | `cursor-agent -p <prompt>` | fallback | AGENTS.md | yes | no | documented |
| Copilot CLI | `copilot` | `copilot -p <prompt>` | fallback | AGENTS.md | yes | no | documented |
| Goose | `goose` | `goose run -t <prompt>` | fallback | AGENTS.md | yes | no | documented |
| Aider | `aider` | `aider --read .evyro/CURRENT_TASK.md --message <prompt>` | fallback | AGENTS.md | no | no | documented |
| Antigravity | none | fallback only | fallback only | AGENTS.md | yes (settings) | no | fallback |

All launches use explicit argv; the prompt is one argument, so task text cannot inject shell commands. Headless runs get a closed stdin, since nobody types into them and an open one makes some CLIs wait for more prompt text. Headless permissions are kept narrow: Claude Code may edit files and run `evyro` only, and Codex may write inside the workspace and Evyro's data folder only, so its own checkpoints and receipts work (its sandbox still blocks the network). `--model <id>` is passed through for agents with a model flag. `EVYRO_AGENT_BIN_<ID>` overrides the binary path (e.g. `EVYRO_AGENT_BIN_CLAUDE_CODE`).

`evyro targets check` reports a sign-in as found when the tool's login file exists. It cannot see the account's plan, usage limit or forced migrations; those show up only when the tool runs, and Evyro then reopens the handoff.

Minimum versions checked by `evyro agents`: Claude Code 1.0.0, Codex 0.1.0, Gemini CLI 0.1.0, OpenCode 0.1.0. Older versions report `incompatible` with an upgrade message.

Quota telemetry: not available from any agent today; Evyro never shows invented usage numbers.

## Chat over ACP

`evyro chat` and the desktop's chat threads talk to agents over ACP instead of a terminal. The ACP command per agent (`claude-agent-acp`, `codex-acp`, `gemini --experimental-acp`, `goose acp`, `opencode acp`, `copilot --acp`, `cursor-agent acp`) comes from each project's documentation; `evyro chat --list` shows which are installed. Evidence: simulated (a stand-in ACP agent in CI). See [ACP](protocols/ACP.md).

## Switch targets beyond agent CLIs

`evyro targets` lists every AI Evyro can hand work to. See the [User guide, Part 3](user-guide/USER_GUIDE.md#part-3-switch-to-any-ai).

| Kind | Members | How work is handed over | Evidence |
|---|---|---|---|
| Coding agent | the agents above | launch with argv, session tracking | simulated |
| API key | 14 providers (`evyro targets providers`), through OpenCode by default, or Aider, Goose, Claude Code, Codex, Gemini | launch with provider, model and key in the environment for that run only | simulated (fake OpenCode in CI) |
| AI IDE | Cursor, VS Code, Windsurf, Zed, Antigravity, JetBrains | open the repo, prompt on the clipboard and in `.evyro/HANDOFF_PROMPT.md`; MCP config via `evyro integrate` | simulated (fake `cursor` in CI) |
| Chat app | ChatGPT, Claude, Gemini, Microsoft Copilot | redacted chat pack on the clipboard and in `.evyro/CHAT_PACK.md`, app or website opened | built and checked in CI with `--no-open` |

### Sign-in modes per agent

| Agent | `app_login` (subscription) | `api_key` providers |
|---|---|---|
| Claude Code | yes | anthropic, bedrock, vertex, anthropic-compatible |
| Codex | yes | openai, ollama (`--oss`) |
| Gemini CLI | yes | google, vertex |
| Cursor agent | yes | Cursor account only |
| Copilot CLI | yes | GitHub account only |
| Goose | yes | most providers (`GOOSE_PROVIDER`, `GOOSE_MODEL`) |
| OpenCode, Aider | no (key or local only) | every provider in the registry |

`app_login` removes every known API-key variable (`ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY` and others) from that run's environment, so the tool falls back to its signed-in account. Login detection checks only that a credential file or Keychain item exists, never its contents.
