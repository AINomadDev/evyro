# Evyro user guide

Evyro keeps your coding work moving when you change AI. You can start a task in Claude Code, run out of quota, and carry on in Codex, Cursor, ChatGPT or any model behind an API key. The next AI does not start from scratch: Evyro gives it the task, what is already done, what is left, the state of the code and the tests that passed.

Everything stays on your computer. Evyro keeps one local database and a small `.evyro/` folder in each repository. It never uploads your code and never stores API keys.

> **How to read this guide.** Part 1 gets you running. Part 2 explains the ideas. Part 3 covers switching to each kind of AI. Part 4 lists everyday workflows. Part 5 covers the desktop app, and Part 6 covers problems and questions.

---

## Part 1. Get started

### 1.1 What you need

| Need | Why | How to check |
|---|---|---|
| macOS 13 or later (Linux and Windows work but are less tested) | Evyro is built for the Mac first | `sw_vers` |
| Rust 1.80 or later | To build Evyro from source | `rustc --version` |
| Git | Evyro reads branch, commits and changed files | `git --version` |
| At least one AI | Something to hand work to | `evyro targets` (after install) |

Install Rust with `curl https://sh.rustup.rs -sSf | sh` if you do not have it. The Xcode command line tools (`xcode-select --install`) give you Git.

### 1.2 Install

From the Evyro folder (for example `~/Developer/Evyro`):

```bash
cargo install --path crates/evyro-cli     # installs the `evyro` command
cargo run --release -p evyro-desktop      # opens the desktop app
```

To build a normal Mac app you can keep in Applications:

```bash
cargo install tauri-cli --version "^2"
cd crates/evyro-desktop && cargo tauri build
# The .app and .dmg are in target/release/bundle/
```

Check the install:

```bash
evyro doctor      # database, Git, agents, projects
evyro targets     # every AI Evyro can hand work to, and whether it is ready
```

### 1.3 Your first project in five minutes

```bash
cd ~/code/my-app
evyro init                                   # register the repo
evyro task add "Add rate limiting" \
  --accept "429 after 100 requests a minute" \
  --accept "Integration test covers the limit" --ready
evyro task start T-1 --agent claude-code     # Claude Code owns T-1 now
evyro run --to claude-code                   # start Claude Code with the task
```

When Claude Code stops (quota, crash, or you close it), Evyro records a checkpoint. Then switch:

```bash
evyro switch --to codex --next "Write the integration test"
```

Codex starts with a signed handoff that says what was done, what is left and what changed in Git.

---

## Part 2. Ideas you need

| Term | Meaning |
|---|---|
| **Project** | A Git repository registered with `evyro init`. |
| **Task** | A piece of work with a key like `T-1`, a status, and acceptance criteria. |
| **Acceptance criteria** | The checklist that says when a task is really done. |
| **Session** | One run of one AI on one task. Evyro tracks when it started, its process, and how it ended. |
| **Checkpoint** | A snapshot of progress: Git state plus done, remaining and next step. |
| **Handoff** | A signed continuation contract from one AI to the next. |
| **Target** | Any AI you can switch to: an agent CLI, an API model, an AI IDE or a chat app. |
| **Receipt** | Proof that a test command ran and passed on a specific state of the code. |
| **Drift** | The code changed after a handoff was written. Evyro shows what changed before the next AI accepts. |

### 2.1 Task statuses

`backlog` → `ready` → `in_progress` → `in_review` → `verified` → `done`. Tasks can also be `blocked` or `cancelled`.

> **Verified needs evidence.** A task becomes `verified` only when every acceptance criterion is met and a passing test receipt matches the current code. An AI saying "done" is not enough. You can override with `--human-override "<reason>"`, and Evyro logs the reason.

### 2.2 Who said what

Every fact in a handoff is labelled:

| Label | Means |
|---|---|
| *human-recorded* | You entered it. |
| *evyro-verified* | Evyro checked it, for example a passing test or Git state. |
| *agent-reported* | An AI said it. Treat it as a claim. |

### 2.3 Where things live

| Thing | Location |
|---|---|
| Database (all projects) | The Evyro data directory. `evyro doctor` prints it (on macOS it is under `~/Library/Application Support/`). Override with `EVYRO_HOME`. |
| Your switch targets | `targets.toml` in the Evyro config directory, also printed by `evyro doctor`. |
| Per-repository files | `.evyro/` in the repo: `project.toml`, handoff contracts, `HANDOFF_PROMPT.md`, `CHAT_PACK.md`. |
| Agent instruction files | `AGENTS.md`, `CLAUDE.md`, `GEMINI.md` with a short Evyro section. |

---

## Part 3. Switch to any AI

Evyro groups every AI into four kinds of **target**. Run `evyro targets` to see them all with their status.

| Kind | Examples | What Evyro does when you switch |
|---|---|---|
| **Coding agent** (`harness`) | Claude Code, Codex, Gemini CLI, Cursor agent, Copilot CLI, OpenCode, Goose, Aider | Starts the agent in your terminal with the task and handoff, tracks the session, checkpoints when it exits. |
| **API key** (`api`) | Any model from Anthropic, OpenAI, Google, OpenRouter, Groq, Mistral, DeepSeek, xAI, Bedrock, Vertex, or any OpenAI-compatible endpoint | Starts a coding agent (OpenCode by default) with that provider, model and key. |
| **AI IDE** (`ide`) | Cursor, VS Code (Copilot, Cline, Continue, Roo), Windsurf, Zed, Antigravity, JetBrains | Opens the repository in the IDE and puts the handoff prompt on your clipboard. |
| **Chat app** (`chat`) | ChatGPT, Claude app or claude.ai, Gemini, Microsoft Copilot | Builds a self-contained chat pack (contract, context, redacted diff), copies it and opens the app. |

Each agent target also has a **sign-in mode**:

| Sign-in mode | Use it when | What Evyro does |
|---|---|---|
| `auto` | The tool is already set up the way you like. | Starts it with your normal environment. |
| `app_login` | You pay with a subscription (Claude Pro or Max, ChatGPT Plus or Pro, Gemini, Copilot). | Removes API-key variables like `ANTHROPIC_API_KEY` for that run, so the tool uses your signed-in account instead of billing a key. |
| `api_key` | You pay per token with a key. | Reads the key from the variable or Keychain item you named, only for that run. |
| `local` | The model runs on your Mac (Ollama, LM Studio). | No key needed. |

### 3.1 Coding agents (Claude Code, Codex, Gemini CLI and others)

The built-in targets are named after the agent: `claude-code`, `codex`, `gemini`, `opencode`, `cursor`, `copilot`, `goose`, `aider`.

1. Install the agent and sign in once the normal way (for example run `claude` or `codex` and log in).
2. Check it: `evyro targets check codex`.
3. Switch: `evyro switch --to codex`.

To use a subscription and never a key, add your own target:

```bash
evyro targets add claude-sub --kind harness --harness claude-code --auth app_login
evyro targets add codex-plus --kind harness --harness codex --auth app_login
```

Agents that support `app_login`: Claude Code, Codex, Gemini CLI, Cursor agent, Copilot CLI, Goose.

> **Interactive agents need a terminal.** `evyro run` and `evyro switch` start the agent in the terminal you typed them in. For scripts or CI, add `--headless`.

### 3.2 API keys and any model

You keep the key; Evyro only remembers *where* it is.

**Step 1. Put the key somewhere safe.** Pick one:

```bash
# A. Environment variable (add to ~/.zshrc so every terminal has it)
export OPENROUTER_API_KEY="sk-or-..."

# B. macOS Keychain (nothing in files at all)
security add-generic-password -s evyro-openrouter -a evyro -w
# You are asked for the key; it goes into the Keychain.
```

**Step 2. Add a target.**

```bash
# Using an environment variable
evyro targets add sonnet-or --kind api --provider openrouter \
  --model anthropic/claude-sonnet-4.5 --key-env OPENROUTER_API_KEY

# Using the Keychain
evyro targets add gpt-key --kind api --provider openai \
  --model gpt-5 --key-keychain evyro-openai

# Claude Code with your own Anthropic key
evyro targets add claude-key --kind harness --harness claude-code \
  --auth api_key --key-env ANTHROPIC_API_KEY

# A local model
evyro targets add qwen-local --kind api --provider ollama --model qwen2.5-coder:14b

# Any OpenAI-compatible endpoint
evyro targets add my-endpoint --kind api --provider openai-compatible \
  --base-url https://llm.example.com/v1 --model my-model --key-env MY_ENDPOINT_KEY
```

API targets use OpenCode unless you pick another agent with `--harness` (Aider, Goose, Claude Code, Codex or Gemini, depending on the provider). `evyro targets providers` shows which agents work with which provider.

**Step 3. Switch.** `evyro switch --to sonnet-or`

> **Safety.** Evyro refuses a key pasted into `--key-env` (it must look like a variable name). It never writes key values to its database, handoffs, logs, exports or MCP answers. If the key is missing when you switch, Evyro stops before changing anything.

### 3.3 AI IDEs (Cursor, VS Code, Windsurf, Zed, Antigravity, JetBrains)

```bash
evyro switch --to cursor-ide
```

Evyro opens the repository in Cursor and copies the handoff prompt. Paste it into Cursor's agent or chat panel. A copy is saved at `.evyro/HANDOFF_PROMPT.md`.

For a deeper link, give the IDE's AI access to Evyro over MCP, so it can read the task and record checkpoints itself:

```bash
evyro integrate cursor-ide --write   # writes .cursor/mcp.json if missing
evyro integrate vscode --write       # writes .vscode/mcp.json if missing
evyro integrate windsurf             # prints the steps for Windsurf, Zed, Antigravity, JetBrains
```

IDE ids: `cursor-ide`, `vscode`, `windsurf`, `zed`, `antigravity`, `jetbrains`.

### 3.4 Chat apps (ChatGPT, Claude, Gemini, Copilot)

Chat apps cannot see your files, so Evyro packs everything they need into one message.

```bash
evyro switch --to chatgpt
```

Evyro writes `.evyro/CHAT_PACK.md` with the contract, the context and the current `git diff`, copies it, and opens ChatGPT. Paste it into a new chat. The chat suggests changes; you apply them and record progress.

Secrets in the diff are redacted, and `.env`, `.pem` and `.key` files are left out.

The Claude desktop app can also use Evyro directly over MCP: `evyro integrate claude-app` prints the config to add.

Chat ids: `chatgpt`, `claude-app`, `gemini-app`, `copilot-chat`.

### 3.5 Managing targets

| Goal | Command |
|---|---|
| See everything and its status | `evyro targets` |
| Only ready ones | `evyro targets list --ready` |
| Why a target is not ready | `evyro targets check <name>` |
| Add or replace your own | `evyro targets add <name> --kind ...` |
| Remove your own | `evyro targets remove <name>` |
| Providers and which agents support them | `evyro targets providers` |

Readiness checks only look for evidence: the program is on PATH, a login file exists, the variable is set, or the Keychain item exists. They never read key values. A target can show **unknown** when Evyro cannot check (for example a web chat sign-in).

### 3.6 Switch, handoff, run: which one?

| Command | Does |
|---|---|
| `evyro switch --to X` | Writes a handoff **and** starts X now. The everyday command. |
| `evyro handoff --to X` | Writes a handoff only. X picks it up later. |
| `evyro run --to X` | Starts X. If a handoff for X is waiting, X continues from it; otherwise X gets the active task. |
| `evyro run --to Y --handoff <id>` | Continue a handoff that was written for someone else. |
| `evyro resume --agent X` | Accept a handoff without starting anything (you start X yourself). |

---

## Part 4. Everyday workflows

### 4.1 Plan the work

```bash
evyro task add "Add audit log" --accept "Every write is logged" \
  --accept "Log survives restart" --priority high --ready
evyro tasks
evyro task show T-2
```

### 4.2 Work, checkpoint, verify

```bash
evyro task start T-2 --agent claude-code
evyro run --to claude-code
# ...or let the agent record checkpoints itself through MCP and hooks...
evyro checkpoint --done "Writer added" --remaining "Restart test" --next "Write restart test"
evyro verify -- cargo test             # records a receipt
evyro task check T-2 1 --receipt <receipt-id>
evyro task check T-2 2 --note "Checked by hand after restart"
evyro task update T-2 --status verified
```

### 4.3 Quota ran out halfway

1. The agent exits. Evyro checkpoints and marks the session failed.
2. `evyro status` shows where things stand.
3. `evyro switch --to codex` (or any target). If the switch fails, the handoff stays open, so you can try another AI: `evyro switch --to gemini`.

### 4.4 Somebody changed the code after the handoff

Evyro stops and shows the drift: new commits, modified files, newly changed files. Read it, then continue with `--accept-drift` or prepare a new handoff.

### 4.5 Record decisions and memory

```bash
evyro decide "Use Redis for rate limits" "Token bucket in Redis" --why "Shared across pods"
evyro memory add "Integration tests need docker compose up first"
evyro memory search redis
```

Agents see these in their context packet: `evyro context --task T-2`.

### 4.6 Connect agents over MCP (optional but useful)

| Agent | Command |
|---|---|
| Claude Code | `claude mcp add evyro -- evyro mcp` |
| Codex, Gemini, others | `evyro integrate <agent>` prints the exact step |
| Hooks (Claude Code auto-checkpoints) | `evyro integrate claude-code --hooks` |

With MCP, the agent can claim tasks, write checkpoints, run verification and prepare handoffs itself.

### 4.7 Back up, export, move machines

```bash
evyro backup                  # snapshot the database
evyro export --out my-app.json   # one project, redacted, portable
evyro import my-app.json         # on the other machine (--root to point at the repo)
```

---

## Part 5. Desktop app

The app shows the same data as the CLI. See the [Desktop guide](DESKTOP.md) for every screen. The main ones for switching:

| Where | What to do |
|---|---|
| **AI targets** | Every target grouped by kind with status and next step. **+ Add target** opens a form for agents, API keys (variable name or Keychain item), IDEs and chat apps. **Switch to** hands the work over. |
| **Handoffs** | Open a handoff, pick any AI under **Continue with**, then **Start with this AI**. Coding agents open in a new Terminal window; IDEs and chat apps open with the text on your clipboard. **Accept only** starts the session and copies the prompt, and you start the AI yourself. |
| **Overview** and **Tasks** | **Hand off** on any task writes a handoff to the AI you pick. |

If the task still belongs to another AI, **Switch to** opens the handoff form instead, so nothing is taken over silently.

---

## Part 6. Troubleshooting and questions

### 6.1 Common problems

| Message or symptom | Fix |
|---|---|
| `no registered project contains ...` | Run `evyro init` in the repo, or pass `--project <alias>`. |
| `needs an interactive terminal` | Run from Terminal, or add `--headless`. |
| `$X is not set in this environment` | Export the variable in `~/.zshrc` and open a new terminal, or use `--key-keychain`. Apps started from the Dock do not read `~/.zshrc`; the Keychain works everywhere. |
| `T-1 is owned by session ... (codex)` | Another AI owns the task. Hand off with `evyro switch --to <target>`, or end that session with `evyro recover`. |
| Target shows **needs setup** | `evyro targets check <name>` tells you the next step. |
| Clipboard or app did not open | The text is saved at `.evyro/HANDOFF_PROMPT.md` or `.evyro/CHAT_PACK.md`. |
| Session stuck as running | `evyro recover` (or **Recover** under Sessions). |
| Desktop app window is blank | Build with the default features (`cargo run -p evyro-desktop`); do not pass `--no-default-features`. |
| Want a clean test area | `EVYRO_HOME=/tmp/evyro-test evyro ...` keeps tests out of your real database. |

### 6.2 Questions

**Does Evyro send my code anywhere?** No. Evyro itself makes no network calls. The AIs you start do whatever they normally do.

**Where are my API keys?** Wherever you put them: your shell profile or the Keychain. Evyro stores only the variable name or Keychain item name.

**Can I use my Claude or ChatGPT subscription instead of a key?** Yes. Use `--auth app_login` for Claude Code, Codex, Gemini CLI, Cursor, Copilot or Goose, or use the chat app targets.

**Can two AIs work at once?** On different tasks, yes. Evyro gives each task one owner at a time, and `evyro worktrees` can give each task its own Git worktree.

**What if an AI says it finished?** That is an *agent-reported* claim. The task is only verified when the criteria are met and tests pass on the current code.

**Does it work with an AI not listed here?** Any model behind an OpenAI- or Anthropic-compatible API works through the `openai-compatible` and `anthropic-compatible` providers. Any chat app works by pasting the chat pack (`evyro switch --to chatgpt --no-open`, then copy `.evyro/CHAT_PACK.md`) or `evyro context`. Any IDE with an MCP setting can use `evyro mcp`. A coding CLI that Evyro does not know yet cannot be launched by `evyro run`; run it yourself after `evyro resume --agent <name>`. If a known agent is installed somewhere unusual, point Evyro at it with `EVYRO_AGENT_BIN_<ID>`, for example `EVYRO_AGENT_BIN_CLAUDE_CODE=/opt/bin/claude`.

### 6.3 Command cheat sheet

| Goal | Command |
|---|---|
| Register repo | `evyro init` |
| Where am I | `evyro status` |
| Add task | `evyro task add "<title>" --accept "<criterion>" --ready` |
| Claim task | `evyro task start T-1 --agent <target>` |
| Start an AI | `evyro run --to <target>` |
| Save progress | `evyro checkpoint --done "..." --next "..."` |
| Run tests with proof | `evyro verify -- <test command>` |
| Tick a criterion | `evyro task check T-1 1 --receipt <id>` |
| Switch AI now | `evyro switch --to <target>` |
| See all AIs | `evyro targets` |
| Add an AI | `evyro targets add <name> --kind harness/api/ide/chat ...` |
| Fix stale sessions | `evyro recover` |
| Health check | `evyro doctor` |

Full reference: [CLI reference](CLI_REFERENCE.md).
