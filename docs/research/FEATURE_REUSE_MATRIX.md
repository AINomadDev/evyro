# Feature reuse matrix

| Component | Decision | Source / reference | Status in Evyro |
|---|---|---|---|
| Continuation contract | Implement compatible functionality | Relay contract v2 (ideas: do-not-redo, HMAC) | Implemented, tested |
| Task graph, ready work, blockers | Implement compatible functionality | Beads model (create, ready, claim, close) | Implemented, tested |
| Storage | Reuse existing SDK | SQLite via rusqlite (bundled, FTS5) | Implemented |
| Git operations | Integrate existing tool | `git` CLI with explicit argv | Implemented |
| Agent launch argv | Implement compatible functionality | px0 `agent.go` dispatch patterns, agent docs | Implemented (documented, not run against real installs here) |
| MCP server | Use existing protocol | MCP spec 2025-11-25; `rmcp` evaluated | Implemented (stdio) |
| ACP | Use existing protocol (later) | T3 Code `effect-acp`, Goose | Not implemented |
| Agent threads, model switching UI | Implement compatible functionality | T3 Code (threads), `portable-pty` + xterm.js | Implemented: agent and shell threads in the desktop app, each running `evyro run` in a PTY |
| Review workbench, PR review | Integrate existing tool | px0 | Planned (launch `px0 <worktree>`) |
| Fuzzy / regex search, symbols | Study architectural ideas | px0 `internal/fuzzy`, `internal/symbols`; tree-sitter | Not implemented |
| Symbol-level retrieval | Integrate existing tool | Serena as optional MCP server | Not implemented |
| Context packing | Study ideas | Repomix | Budgeted packet implemented |
| Skills | Study ideas | obra/superpowers format | Not implemented |
| Kanban / worktree per task | Study ideas | vibe-kanban (sunsetting) | Worktree per task implemented |
| Memory server | Study ideas | Marrow, agent-office | FTS5 memory with provenance implemented |
| Remote / mobile | Study ideas | T3 Code (Tailscale, mobile apps) | Not implemented |
| Base application | Reject fork | T3 Code, px0, Paradev | See FORK_VS_BUILD.md |
| Continue | Reject | Repository is read-only / unmaintained | |
