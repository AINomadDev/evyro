# Feature reuse matrix

| Component | Decision | Source / reference | Status in Evyro |
|---|---|---|---|
| Continuation contract | Implement compatible functionality | Relay contract v2 (ideas: do-not-redo, HMAC) | Implemented, tested |
| Task graph, ready work, blockers | Implement compatible functionality | Beads model (create, ready, claim, close) | Implemented, tested |
| Storage | Reuse existing SDK | SQLite via rusqlite (bundled, FTS5) | Implemented |
| Git operations | Integrate existing tool | `git` CLI with explicit argv | Implemented |
| Agent launch argv | Implement compatible functionality | px0 `agent.go` dispatch patterns, agent docs | Implemented (documented, not run against real installs here) |
| MCP server | Use existing protocol | MCP spec 2025-11-25; `rmcp` evaluated | Implemented (stdio) |
| ACP | Use existing protocol | T3 Code `effect-acp`, Goose, ACP spec v1 | Implemented as a client (`evyro-acp`, own code): `evyro chat` and desktop chat threads, with Evyro sessions, Evyro's MCP server injected, project-only file access and permission prompts |
| Agent threads, model switching UI | Implement compatible functionality | T3 Code (threads), `portable-pty` + xterm.js | Implemented: agent and shell threads in the desktop app, each running `evyro run` in a PTY |
| Review workbench, PR review | Implement compatible functionality | px0 (review workflow ideas) | Line and file comments on working-tree changes in the desktop Changes view and `evyro review`; sent to running agents or as a prompt; in context packets and over MCP. PR review on GitHub not implemented |
| Fuzzy / regex search, symbols | Study architectural ideas | px0 `internal/fuzzy`, `internal/symbols`; tree-sitter | Implemented in `evyro-core::search` (own code, no px0 source copied): fuzzy file names, text and regex search, per-language pattern symbols and outlines, in the CLI, MCP and desktop. Tree-sitter not yet |
| Symbol-level retrieval | Integrate existing tool | Serena as optional MCP server | Not implemented |
| Context packing | Study ideas | Repomix | Budgeted packet implemented |
| Skills | Study ideas | obra/superpowers and Claude Code `SKILL.md` format | Implemented (own code, own skill text): `evyro-core::skills` with 15 built-in skills, global and project skills, versions, on/off, checks (secrets, size, Evyro version), import/export, `list_skills`/`get_skill` over MCP, a short list in the task context, and `evyro skills sync` into `.claude/skills`, `AGENTS.md`, `GEMINI.md` or the Copilot file without overwriting what a person wrote |
| Kanban / worktree per task | Study ideas | vibe-kanban (sunsetting) | Worktree per task implemented |
| Memory server | Study ideas | Marrow, agent-office | FTS5 memory with provenance implemented |
| Remote / mobile | Study ideas | T3 Code (Tailscale, mobile apps) | Not implemented |
| Base application | Reject fork | T3 Code, px0, Paradev | See FORK_VS_BUILD.md |
| Continue | Reject | Repository is read-only / unmaintained | |
