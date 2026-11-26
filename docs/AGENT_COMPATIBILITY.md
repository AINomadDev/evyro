# Agent compatibility

**Evidence levels.** *Simulated*: exercised in CI by a stand-in script with the same binary name and argv shape. *Documented*: argv and MCP setup come from the agent's docs or source. *Real*: run by hand against the real, signed-in agent (none in CI; real agents need accounts). Run `evyro agents` on your machine to see detection and version checks.

| Agent | Binary | Headless argv | Interactive argv | Instruction file | MCP | Hooks | Evidence |
|---|---|---|---|---|---|---|---|
| Claude Code | `claude` | `claude -p <prompt> --permission-mode acceptEdits --allowedTools "Bash(evyro *)"` | `claude <prompt>` | CLAUDE.md | yes | yes (`evyro integrate claude-code --hooks`) | documented + simulated |
| Codex CLI | `codex` | `codex exec --sandbox workspace-write --add-dir <evyro data dir> <prompt>` | `codex <prompt>` | AGENTS.md | yes | no (fallback: instructions + MCP) | documented + simulated + real (macOS, 2026-11-23) |
| Gemini CLI | `gemini` | `gemini -p <prompt>` (with `GEMINI_CLI_TRUST_WORKSPACE=true`) | `gemini -i <prompt>` | GEMINI.md | yes | no | documented + simulated |
| OpenCode | `opencode` | `opencode run <prompt>` | `opencode --prompt <prompt>` | AGENTS.md | yes | no | documented (source at 4332e24) |
| Cursor | `cursor-agent` | `cursor-agent -p <prompt>` | `cursor-agent <prompt>` | AGENTS.md | yes | no | macOS 2026-11-23: 2026.11.22-e9042b2 reached sign-in through `evyro run`; authenticated response pending |
| Antigravity CLI | `agy` | `agy --print <prompt>` | `agy --prompt-interactive <prompt>` | AGENTS.md | yes | yes (`agy acp`) | macOS 2026-11-23: authenticated ACP startup checked without sending a turn |
| Copilot CLI | `copilot` | `copilot -p <prompt>` | `copilot -i <prompt>` | AGENTS.md | yes | no | macOS 2026-11-23: 1.0.114 answered a diagnostic prompt; `evyro run` reached the interactive folder-trust prompt |
| Goose | `goose` | `goose run -t <prompt>` | fallback | AGENTS.md | yes | no | documented |
| Aider | `aider` | `aider --read .evyro/CURRENT_TASK.md --message <prompt>` | fallback | AGENTS.md | no | no | documented |
| Amp | `amp` | `amp -x <prompt>` (paid credits) | `amp` (reads AGENTS.md; no first prompt) | AGENTS.md | yes | no | documented |
| Qwen Code | `qwen` | `qwen -p <prompt>` | `qwen -i <prompt>` | QWEN.md | yes | no | documented |
| Kiro CLI | `kiro-cli` | `kiro-cli chat --no-interactive <prompt>` | `kiro-cli chat <prompt>` | AGENTS.md | yes | no | documented |
| Factory Droid | `droid` | `droid exec --auto low <prompt>` | `droid <prompt>` | AGENTS.md | yes | no | documented |
| Cline CLI | `cline` | `cline <prompt>` (approves its own tools) | `cline -i <prompt>` | AGENTS.md | yes | no | documented |
| Crush | `crush` | `crush run <prompt>` (approves its own tools) | `crush` (reads AGENTS.md; no first prompt) | AGENTS.md | yes | no | documented |
| Antigravity | none | fallback only | fallback only | AGENTS.md | yes (settings) | no | fallback |

All launches use explicit argv; the prompt is one argument, so task text cannot inject shell commands. Headless runs get a closed stdin, since nobody types into them and an open one makes some CLIs wait for more prompt text. Headless permissions are kept narrow: Claude Code may edit files and run `evyro` only, and Codex may write inside the workspace and Evyro's data folder only, so its own checkpoints and receipts work (its sandbox still blocks the network). `--model <id>` is passed through for agents with a model flag. `EVYRO_AGENT_BIN_<ID>` overrides the binary path (e.g. `EVYRO_AGENT_BIN_CLAUDE_CODE`).

Local models: Ollama, LM Studio, MLX (`mlx_lm.server`, port 8080), llama.cpp (`llama-server`, port 8080, also for Unsloth GGUF models) and vLLM (port 8000) work through OpenCode, Aider or Goose with `--auth local`. The new agents above have not been run with Evyro yet ("documented" means from each tool's docs).

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

## Custom agents

An agent CLI Evyro does not know can be added with one TOML file, in `agents/` under Evyro's config directory (shown by `evyro doctor`; `$EVYRO_HOME/config/agents/` when `EVYRO_HOME` is set) or in a project's `.evyro/agents/`. `evyro agents` lists it next to the built-in agents, and `evyro doctor` reports files it could not read and runs each agent's login check.

```toml
name = "my-agent"                     # letters, digits, - _ . ; not a built-in agent's name
display_name = "My Agent"             # optional
command = "my-agent"                  # on PATH, or a full path
args = ["run", "--cwd", "{cwd}", "{prompt}"]  # headless launch; must contain {prompt}
interactive_args = ["{prompt}"]       # optional; defaults to args
model_args = ["--model", "{model}"]   # optional; put right after the command when a model is chosen
instruction_file = "AGENTS.md"        # optional; the file the agent reads from the repository root
login_check = ["my-agent", "auth", "status"]  # optional; exit status 0 means signed in

[env]                                 # optional; set for the login check (and for runs, once supported)
MY_AGENT_TELEMETRY = "0"
```

Placeholders are `{prompt}`, `{cwd}` and `{model}` (only in `model_args`). No shell is used: each list item is one argument and a placeholder's value is inserted as-is, so prompt text cannot add arguments or commands. Unknown fields, unknown placeholders and a missing `{prompt}` are errors that name the file. Login checks in a project's `.evyro/agents/` are repository text, so `evyro doctor` shows them instead of running them.

Custom agents are listed and checked today; starting them with `evyro run` and showing them in the desktop app are not done yet.

## Testing a new agent

`evyro agents check <name>` is the test kit for an agent definition. Run it on a built-in agent after changing `crates/evyro-core/src/adapters.rs`, on a custom agent by name, or on a `.toml` file by path before you install it:

```sh
evyro agents check my-agent.toml            # a file you are writing
evyro agents check my-agent --smoke         # an installed custom agent, started once
evyro agents check gemini --json            # a built-in agent, as JSON
```

Each step prints `pass`, `fail`, `warn` or `skip` with the reason, and the command exits 1 when any step fails:

| Step | What it checks |
|---|---|
| definition | The file parses and validates (unknown fields, placeholders, a missing `{prompt}`), or the built-in agent exists |
| binary | The program is on PATH, or the full path exists |
| version | Built-in agents only: `--version` reads and meets the minimum Evyro supports |
| login | Custom: `login_check` exits 0. Built-in: a login file or Keychain item exists |
| argv headless / interactive / + model | The argv Evyro builds starts with the program, keeps a prompt full of quotes, `$(...)`, `;` and `{cwd}` in exactly one unchanged argument, and passes the model when one is chosen. The argv is shown with `{prompt}` and `{cwd}` in place |
| smoke | Only with `--smoke`: runs the headless command once in an empty scratch folder with a short "reply OK, change nothing" prompt and checks it exits 0 within `--timeout` seconds (180 by default). This uses the agent's account and a little of its quota |

A file in a project's `.evyro/agents/` is repository text, so its login check and smoke run are skipped there; pass the file's path to check it as your own. Automated tests must not call a real agent: the unit tests in `crates/evyro-core/src/agent_check.rs` and the `agents check` step in `scripts/cli-e2e.sh` use small shell scripts in its place.
