# MCP integration

`evyro mcp` runs a stdio MCP server scoped to the project of the current directory (or `--project <id>`).

## Register with agents

| Agent | Command (run inside the repo) |
|---|---|
| Claude Code | `claude mcp add evyro -- evyro mcp` |
| Codex CLI | `codex mcp add evyro -- evyro mcp` |
| Gemini CLI | `gemini mcp add evyro evyro mcp` |
| OpenCode | add `{"mcp": {"evyro": {"type": "local", "command": ["evyro", "mcp"]}}}` to `opencode.json` |
| Cursor | `.cursor/mcp.json`: `{"mcpServers": {"evyro": {"command": "evyro", "args": ["mcp"]}}}` |

`evyro integrate <agent>` prints the line for that agent. These commands are taken from each agent's documentation and have not been run against real installs in CI yet.

## Over HTTP

`evyro serve` also offers the same server at `http://<address>:7420/mcp/<project>` (Streamable HTTP, JSON replies) to devices you paired, with the device token as a bearer token. `initialize` returns an `Mcp-Session-Id` to send on later requests. Devices without the `control` scope get the read-only tools. See [Remote access](../REMOTE_ACCESS.md).

## Tools

Read-only: `get_project_context`, `get_active_task`, `list_tasks`, `get_checkpoint`, `get_handoff`, `get_git_status`, `list_sessions`, `get_project_health`, `search_memory`, `get_relevant_files`, `find_files`, `search_code`, `find_symbols`, `get_outline`, `get_review_comments`, `list_skills`, `get_skill`.

The four search tools read only files Git tracks or would track in the project (or the task's worktree when you pass `task`). They take repo-relative paths only, skip binary, large and sensitive files, and redact secrets in matched lines.

Mutating (disabled with `--read-only`, and then left out of `tools/list`): `create_task`, `claim_task`, `update_task`, `check_acceptance`, `create_checkpoint`, `prepare_handoff`, `record_decision`, `run_verification`, `add_memory`, `resolve_review_comment`.

## Safety rules

- No tool accepts a filesystem path to read. Memory file references are resolved inside the project root and `..` escapes are refused.
- `run_verification` runs only the project's configured commands by kind and index.
- `update_task` cannot reach `verified` or `done` without a matching receipt and every acceptance criterion met. There is no override over MCP.
- `check_acceptance` counts toward `verified` only when it cites a passing receipt for the same task. A bare note is stored as agent-reported and needs a human to confirm it.
- All tool output is redacted before it is returned.
- Protocol versions offered: 2025-11-25 (default), 2025-06-18, 2025-03-26, 2024-11-05.
