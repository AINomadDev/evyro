# Backlog

Requirement references are spec section numbers. Evidence for done items is the named test in `crates/evyro-cli/tests/scenarios.rs` unless noted.

## Done (v0.1.0)

| Item | Spec | Evidence |
|---|---|---|
| Project registry, init, move detection | 9, 10 | `init_is_non_destructive`, `multi_project_isolation` |
| SQLite storage, migrations, integrity, FTS5 | 5.3 | unit tests, `export_import_and_backup_restore` |
| Task graph, state machine, deps, ready work | 19 | `task_graph_rules` |
| Sessions, leases | 15 | `parallel_worktrees` |
| Checkpoints, adaptive dedupe | 14.4, 35.7 | `auto_checkpoints_dedupe` |
| Continuation contract, signing | 14.3 | `three_agent_continuation`, `tampered_contract_is_rejected` |
| Resume validation, drift | 14.5, 35.2, 35.4 | `drift_is_detected_on_resume` |
| Crash recovery | 14.4, 37-D | `abrupt_agent_exit_is_recovered`, `recover_detects_dead_pid` |
| Verification receipts, evidence gate | 35.10, 37-R | `three_agent_continuation`, `task_graph_rules` |
| Redaction | 32.2, 37-J | `secrets_are_redacted_everywhere` |
| Backup/restore, export/import | 33, 37-L | `export_import_and_backup_restore` |
| MCP stdio server | 25 | `mcp_round_trip`, `prompt_injection_cannot_mark_verified` |
| Adapter framework + version checks | 16 | `incompatible_agent_version`, unit tests |
| Worktree per task | 21.1 | `parallel_worktrees` |
| Doctor, health | 31, 35.12 | `export_import_and_backup_restore` (doctor) |
| Acceptance criteria gate on `verified` | 35.10 | `three_agent_continuation`, `agent_claims_do_not_satisfy_criteria` |
| Failed launch reopens handoff; `task start` / `claim_task` accept open handoffs | 14.5 | `failed_launch_reopens_handoff` |
| Simpler `decide` / `memory add` syntax | 12 | `decide_and_memory_syntax` |
| Desktop MVP, Tauri 2 (overview, tasks, handoffs, sessions, decisions and memory, agents, settings) | Phase 3 | manual run on Linux under Xvfb; no automated UI tests yet |
| Switch to any AI: targets for agents, API keys (env var or Keychain), app sign-in, IDEs and chat apps; desktop AI targets screen | 15, 17 | `switch_to_any_kind_of_ai`; desktop checked by hand on Linux |
| Detailed user guide | docs | [USER_GUIDE.md](user-guide/USER_GUIDE.md) |

## Next, in dependency order

1. **Real-agent validation on macOS** (16.1, 37-B/C): run the Claude → Codex → Gemini flow with installed agents; record versions and fix argv/MCP differences. Blocks claiming any adapter as tested.
2. **CI on macOS + Windows** (39): run the existing workflow; fix path and process issues.
3. **Self-hosting** (44): register this repo, import this backlog as Evyro tasks.
4. **Desktop, next steps** (Phase 3): run on macOS, native folder picker, notifications, automated UI smoke test.
4a. **Targets, next steps**: confirm login-file locations and model mapping against real installs; model discovery per provider; a generic "custom CLI" harness with an argv template.
5. **px0 / T3 Code integration** (21.2): launch px0 on a task worktree; document T3 Code + Evyro MCP setup.
5a. **ACP, next steps**: chat is in (`evyro chat`, desktop chat threads). Next: run it with real ACP agents (Claude Agent ACP, Codex ACP, Gemini), `session/load` so a chat survives an app restart, `terminal/*`, image prompts, model and mode pickers.
6. **Codex and Gemini hooks** (14.4): automatic checkpoints where those agents expose hooks.
7. **Code search and symbols** (22): done for fuzzy file search, text and regex search, pattern-based symbols and outlines (CLI, MCP, desktop). Next: tree-sitter outlines, find references, optional Serena or LSP over MCP.
8. **Skills registry** (23).
9. **Handoff benchmark suite** (38) with real agents.
10. **Packaging** (5.5): release binaries + checksums, Homebrew tap, then installers.
11. **Remote access and mobile companion** (6.4, 32.1): Streamable HTTP with auth, after desktop is stable.
12. **Relay contract import/export, GitHub Issues sync** (19.4).
