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

### 4.6 Review what the AI changed

Read the diff, leave comments on lines, and send them to the agent. In the desktop app, open **Changes**: click a line number (or the **+** at the end of a line) to comment, then **Send comments to agent**. Pick a running agent thread and the comments are typed into it as one message, or copy them as a prompt for any AI. New files the agent created show up as added, so you can comment on them too.

From the command line:

```bash
evyro review comment src/limit.rs:42 "Read the limit from config"
evyro review comment README.md "Document the new limit"   # whole-file comment
evyro review                     # comments still waiting (open or sent)
evyro review prompt --mark-sent  # one prompt to paste into any AI
evyro review resolve 4f9a02c1    # by the short id shown in the list
```

Comments that are not resolved are part of every context packet and handoff context, so the next AI sees them even if you switch. Agents connected over MCP read them with `get_review_comments` and close them with `resolve_review_comment`. Snippets are redacted like everything else Evyro stores.

### 4.7 Connect agents over MCP (optional but useful)

| Agent | Command |
|---|---|
| Claude Code | `claude mcp add evyro -- evyro mcp` |
| Codex, Gemini, others | `evyro integrate <agent>` prints the exact step |
| Hooks (Claude Code auto-checkpoints) | `evyro integrate claude-code --hooks` |

With MCP, the agent can claim tasks, write checkpoints, run verification and prepare handoffs itself.

### 4.8 Back up, export, move machines

```bash
evyro backup                  # snapshot the database
evyro export --out my-app.json   # one project, redacted, portable
evyro import my-app.json         # on the other machine (--root to point at the repo)
```

### 4.9 Find code fast

Search the repository by file name, by text or regex, or by symbol (a function, type or class). Search only looks at files Git tracks or would track, so anything in `.gitignore` stays out. It also skips binary files, files over 1 MB and files that may hold secrets (`.env`, keys). If a line it shows contains a secret, that part reads `[REDACTED]`.

```bash
evyro search files ratelim             # fuzzy file names: finds src/net/rate_limiter.rs
evyro search text "allow_request"      # every line that contains it
evyro search text --regex 'fn \w+_request' --path src/net
evyro search text Token --case          # match upper and lower case exactly
evyro search symbols RateLimiter        # where it is defined
evyro search outline src/net/rate_limiter.rs
```

In the desktop app, press **⌘ P** (Ctrl P on Windows and Linux) to go to a file, **⌘ ⇧ F** to search the code, or **⌘ ⇧ O** to go to a symbol. **Tab** switches between the three. Pick a result to open the file with line numbers and an outline. Click a symbol in the outline to jump to it. Agents connected over MCP get the same search as the tools `find_files`, `search_code`, `find_symbols` and `get_outline`, which also work with `--read-only`.

Symbols come from patterns for each language: Rust, TypeScript and JavaScript, Python, Go, Java, Kotlin, C#, Scala, Swift, Dart, Ruby, C and C++, plus Markdown headings. They are quick and need nothing installed, but they are not a full parser. An unusual declaration can be missed.

### 4.10 Chat with an agent instead of a terminal

Agents that speak ACP (Agent Client Protocol) can run as a **chat**: you see the agent's reply, its plan and each tool call, and the agent asks you before it edits a file or runs a command. It is still an Evyro session, so the task, checkpoints and handoffs work as usual, and the agent gets Evyro's MCP tools.

```bash
evyro chat --list                       # which agents can chat on this machine
evyro chat --to codex --task T-1        # type messages; an empty line ends the chat
evyro chat --to claude-code --approve reads "Start with the failing test"
```

`--approve` decides what runs without asking: `ask` (the default), `reads` (reading and searching only), `all` or `none`. In the desktop app, **+ Chat** on the Threads screen starts one; permission requests appear as buttons in the conversation.

Each agent needs its ACP side installed once, for example `npm install -g @agentclientprotocol/claude-agent-acp` for Claude Code or `npm install -g @agentclientprotocol/codex-acp` for Codex. `evyro chat --list` prints the command for each. Evyro shares only files inside the project with a chat agent, never `.env` or keys. See [ACP](../protocols/ACP.md) for the details.

### 4.11 Give agents skills

Skills are short instructions for one kind of work, such as debugging, test-driven development, a security review or a handoff. Evyro has 15 built in. You can add your own for all your projects, or for one repository in `.evyro/skills`. When an agent starts a task, it sees only the names of the skills that fit, and it loads a skill's text when it needs it (MCP `get_skill`).

```bash
evyro skills                                  # every skill, and whether it is on
evyro skills list --for "fix the crash on save"   # which ones fit a task
evyro skills show debugging
evyro skills disable performance-optimization # in this project (--global for all)
evyro skills new api-conventions --description "Use when adding an HTTP endpoint"
evyro skills import ./team-skills             # copies SKILL.md files in, checked for secrets
evyro skills sync --for claude-code           # writes .claude/skills/ for Claude Code
evyro skills sync --for codex --append        # adds a skills list to AGENTS.md
```

Evyro never overwrites instructions someone wrote. A sync leaves your own `.claude/skills` files alone. It changes `AGENTS.md` only between its own markers, or adds them at the end when you pass `--append`. In the desktop app, the **Skills** tab does all of this. See [Skills](../SKILLS_SYSTEM.md) for the file format and the details.

### 4.12 Follow and approve from your phone

Start remote access, pair your phone once, and you can see the tasks, the changes and every chat from it, and allow or reject an agent's requests there. It is off until you start it.

```bash
evyro remote pair --name "My phone"    # prints a one-time code, valid 10 minutes
evyro serve --host 100.x.y.z           # your Tailscale address; 127.0.0.1 is the default
```

Open `http://100.x.y.z:7420/` on the phone and type the code. In the desktop app, Settings → **Remote access** does the same, and the phone then sees the chats open in the window. Pick the lowest access that does the job: `read` looks only, `approve` (the default) also answers permission requests, `control` also starts and messages chats. `evyro remote revoke "My phone"` cuts a device off at once. See [Remote access](../REMOTE_ACCESS.md) for the security model, Tailscale and the API.

---

## Part 5. Desktop app

The app shows the same data as the CLI. See the [Desktop guide](DESKTOP.md) for every screen. The main ones for switching:

| Where | What to do |
|---|---|
| **AI targets** | Every target grouped by kind with status and next step. **+ Add target** opens a form for agents, API keys (variable name or Keychain item), IDEs and chat apps. **Switch to** hands the work over. |
| **Handoffs** | Open a handoff, pick any AI under **Continue with**, then **Start with this AI**. Coding agents start in a thread inside the window; IDEs and chat apps open with the text on your clipboard. **Accept only** starts the session and copies the prompt, and you start the AI yourself. |
| **Threads** | Every agent you started from the app, each in its own terminal, or as a chat (**+ Chat**, for agents that speak ACP) where you approve each tool call. Type into it as in any terminal. **+ Agent** starts another AI on a task, **+ Shell** opens a shell in the repo, **Hand off** passes a running agent's task on. |
| **Overview** and **Tasks** | **Hand off** on any task writes a handoff to the AI you pick. |
| **Search** (⌘ P) | Go to a file, search the code or jump to a symbol, then read the file with its outline. |

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
| Evyro crashed | It saved a report in the `logs` folder of its data folder. `evyro bug-report --out report.md` gathers it with the versions and doctor checks, secrets redacted. Read the file, then attach it to an issue. |
| Something went wrong after an upgrade | Before Evyro upgrades its database it saves a copy in the `backups` folder of its data folder (the newest 5 are kept). `evyro restore <copy> --yes` puts it back. |

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
| Find a file or code | `evyro search files <name>` / `evyro search text <text>` |
| Comment on a change | `evyro review comment <path>:<line> "..."` |
| Send review comments | `evyro review prompt --mark-sent` |
| Chat with an agent | `evyro chat --to <agent> --task T-1` |
| Skills for a task | `evyro skills list --for "<task>"` |
| Health check | `evyro doctor` |

Full reference: [CLI reference](CLI_REFERENCE.md).
