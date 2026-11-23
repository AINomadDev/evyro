# ACP (Agent Client Protocol)

[ACP](https://agentclientprotocol.com) is an open protocol between a client, such as an editor, and a coding agent. Over ACP the agent sends its reply, its plan and each tool call as structured messages, and it asks the client before it runs a tool. Evyro is an **ACP client**. `evyro chat` in the terminal and **chat threads** in the desktop app both use it.

## What Evyro adds on top of a plain ACP client

- **The chat is an Evyro session.**
  - Starting a chat claims the task, or picks up the handoff waiting for that agent, exactly like `evyro run`.
  - The first message is Evyro's task context or the handoff's resume prompt.
  - Evyro records a checkpoint after every turn, skipping turns where nothing changed. It records another when the chat ends. Each checkpoint holds the agent's last reply as the summary and the files it wrote as completed items. Both are stored as *agent-reported*.
  - Ending the chat closes the session. If the agent crashes, the session is marked failed, and any handoff it accepted opens again.
- **The agent gets Evyro's MCP server.** In `session/new`, Evyro passes itself as a stdio MCP server, scoped to the project and the session. From inside the chat, the agent can read the task, record checkpoints and decisions, run the configured tests, and prepare a handoff. Use `--no-mcp` to leave it out.
- **Files stay inside the project.** Evyro answers `fs/read_text_file` and `fs/write_text_file` itself, and only for paths inside the session's directory (the repository, or the task's worktree). It refuses:
  - paths outside that directory, after resolving `..` and symlinks;
  - Git's own files;
  - files that may hold secrets, such as `.env`, keys and certificates.

  Every read, write and refusal is shown in the chat.
- **You approve tool calls.**
  - Each `session/request_permission` is shown with the agent's own options.
  - In the terminal, `--approve` sets a policy:
    - `ask`: the default;
    - `reads`: allow reading, searching and thinking, and ask about the rest;
    - `all`;
    - `none`.
  - Without a terminal to ask in, Evyro refuses.
  - Cancelling a turn answers every open request as cancelled, as the protocol requires.

## Starting an agent in ACP mode

Evyro uses each agent's documented ACP command. It never downloads an agent to start it.

| Evyro target | Command | Install the ACP side |
|---|---|---|
| `claude-code` | `claude-agent-acp` | `npm install -g @agentclientprotocol/claude-agent-acp` |
| `codex` | `codex-acp` | `npm install -g @agentclientprotocol/codex-acp` |
| `gemini` | `gemini --experimental-acp` | `npm install -g @google/gemini-cli` |
| `goose` | `goose acp` | Goose |
| `opencode` | `opencode acp` | OpenCode |
| `copilot` | `copilot --acp` | `npm install -g @github/copilot` |
| `cursor` | `cursor-agent acp` | Cursor CLI |

How these are used:

- `evyro chat --list` shows which of these are found on this machine.
- API-key targets chat through the agent they run on. Their key goes into that agent's environment for the run only, as with `evyro run`.
- To use another command, either:
  - set `EVYRO_ACP_BIN_<TARGET>` to the program, for example `EVYRO_ACP_BIN_CODEX=/opt/codex-acp`; or
  - pass `--command <program> [args]` to `evyro chat`.
- The desktop app takes `PATH` from your login shell when it starts. Agents installed with npm, Homebrew or cargo are then found even when the app is opened from the Dock.

## Protocol coverage (ACP v1)

| Area | Evyro |
|---|---|
| `initialize` | Protocol version 1. Advertises `fs.readTextFile` and `fs.writeTextFile`, with `terminal: false`. Refuses agents that answer with another version. |
| `session/new` | `cwd` is the repository or the task worktree. `mcpServers` holds Evyro's stdio server. |
| `session/prompt` | Text content blocks. |
| `session/update` | Shows these updates: `agent_message_chunk`, `agent_thought_chunk` (folded), `tool_call`, `tool_call_update`, and `plan`. Tool call content with `diff` is shown as a diff, and text content as text. Other updates are ignored. |
| `session/request_permission` | Shown to you, with the agent's options. |
| `session/cancel` | **Stop turn** in the app. |
| `fs/read_text_file`, `fs/write_text_file` | Inside the project only (see above). |
| `terminal/*` | Not offered. Agents run their commands themselves, and report them as tool calls. |
| `authenticate` | Not supported. Sign in to the agent on its own, for example `claude` and `/login` once. |
| `session/load`, `session/set_mode`, images, audio | Not supported yet. |

## Evidence

- **Tests in CI on Linux and macOS:**
  - `crates/evyro-acp/tests/client.rs` covers a turn with a plan, tool calls, a permission request, and file reads and writes. It also checks refusal of `.env` and of paths outside the project, rejection, cancellation, and a dead agent.
  - The scenario test `acp_chat_runs_under_an_evyro_session` covers `evyro chat` end to end: the MCP server reaches the agent, `--approve all` and `--approve none` behave as set, a handoff is picked up, the checkpoint lists the edited file, and an agent with no ACP command is refused before any state changes.
  - The agent in these tests is `crates/evyro-acp/tests/fake_acp_agent.py`. It exits on any message that breaks the parts of ACP v1 it checks.
- **Schema check (by hand, 2026-11-23):** every message Evyro sent in a two-turn chat validated against the official ACP v1 JSON schema, from `@agentclientprotocol/sdk` 2.9.1 (12 of 12). The messages were three requests, three responses and six error answers.
- **Real app (by hand, Linux under Xvfb, 2026-11-23):** a chat thread started the stand-in agent and showed its plan, tool calls and refusals. A permission prompt was allowed, the file was written, and ending the chat closed the session with a checkpoint.
- **Not yet verified:** a real ACP agent. No run with Claude Agent ACP, Codex ACP, Gemini CLI, Goose, OpenCode or Copilot has been recorded. The commands above come from each project's documentation.
