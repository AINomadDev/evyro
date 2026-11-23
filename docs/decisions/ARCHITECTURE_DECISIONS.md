# Architecture decisions

## ADR-001: Independent Rust core, integrate existing frontends (Strategy E)

- **Status:** accepted, 2026-11-22
- **Context:** The spec requires the continuity engine to be independent of any GUI. T3 Code is the strongest existing app (MIT, broadest agent support) but its continuity lives in its own server and thread model. px0 is a strong review surface but has no task state.
- **Decision:** Build `evyro-core` (Rust library) + `evyro` CLI + `evyro mcp` server. Desktop will be Tauri 2 over the same core. Integrate T3 Code and px0 as optional surfaces; do not fork.
- **Consequences:** Evyro works from any terminal or agent today. The desktop UI is pending (Phase 3). Agent thread UI and PR review come from integrations rather than being rebuilt.
- **Evidence:** docs/research/FORK_VS_BUILD.md (weighted scores).

## ADR-002: Few crates, split later

- **Status:** accepted
- **Decision:** Three crates (`evyro-core`, `evyro-mcp`, `evyro-cli`) instead of the 19 sketched in the spec. Modules inside `evyro-core` map to the spec's components (projects, tasks, sessions, checkpoints, handoff, context, memory, verify, adapters, git, redact, backup).
- **Why:** Avoid premature fragmentation (spec section 8). Split when the desktop app or daemon needs a separate dependency boundary.

## ADR-003: SQLite as the operational store, Git CLI for repository facts

- **Status:** accepted
- **Decision:** One global SQLite database (WAL, foreign keys, explicit migrations, FTS5) in the platform data dir; `EVYRO_HOME` overrides. Git facts come from the `git` CLI with explicit argv rather than libgit2.
- **Why:** No C library to cross-compile beyond bundled SQLite; behaviour matches what agents see; linked worktrees work for free.

## ADR-004: Provenance and evidence gates

- **Status:** accepted
- **Decision:** Every fact in a contract carries provenance: `evyro_captured` (Git), `evyro_verified` (receipt), `agent_reported`, `human_recorded`. A task can move to `verified` only if every acceptance criterion is met, the latest receipt for it passed, *and* its repository fingerprint equals the current state. Criteria are met by a human check or by an agent citing a passing receipt; an agent's bare claim does not count (added 2026-11-22 after Pratik found `verify -- true` could verify a task with unmet criteria). Only a human (`--human-override "<reason>"`, CLI only) can bypass, and the reason is logged. MCP clients can never bypass.
- **Why:** Spec: "verified state over agent claims", Scenario R.

## ADR-005: Signed continuation contract

- **Status:** accepted
- **Decision:** Contract JSON v1 sealed with SHA-256 content hash + HMAC-SHA256 using a per-install key (`<data>/contract.key`, mode 0600). Resume verifies both; a modified contract is refused.
- **Why:** Prevent a previous agent (or anything in the repo) from editing "completed" claims after the fact.

## ADR-006: Drift blocks resume by default

- **Status:** accepted
- **Decision:** `evyro resume` compares the handoff checkpoint with the live repo (branch, HEAD relation, per-file content hashes). Any drift returns exit code 5 and a report; continuing needs `--accept-drift`, which is logged.
- **Why:** Spec 14.5: controlled reconciliation, never blind overwrite.

## ADR-007: Hand-written MCP server for now

- **Status:** accepted, revisit at Phase 3
- **Decision:** Implement MCP stdio JSON-RPC directly (initialize, tools/list, tools/call, ping) with protocol negotiation. Tools are scoped to the project the server started in and take task keys, never filesystem paths. Verification over MCP can only run configured commands.
- **Why:** Keeps the CLI synchronous with ~2 ms startup. The official `rmcp` SDK needs Tokio; adopt it when Streamable HTTP is needed for remote/mobile.

## ADR-008: Evyro bookkeeping is excluded from drift

- **Status:** accepted
- **Decision:** Paths under `.evyro/` are excluded from Git snapshots and fingerprints. Handoff files and CURRENT_TASK.md are git-ignored by `.evyro/.gitignore`; `project.toml` and `CONTEXT.md` are meant to be committed.
- **Why:** Writing a checkpoint must not itself look like drift.

## ADR-009: Desktop app is a thin Tauri 2 shell with a no-build UI

- **Status:** accepted (2026-11-22)
- **Decision:** `crates/evyro-desktop` exposes core operations as Tauri commands that open the same SQLite store as the CLI. The UI is plain HTML, CSS and JavaScript in `ui/`, with no bundler or npm dependencies. All untrusted text is inserted with `textContent`. The window does not launch agents or run arbitrary commands; it runs only the project's configured verification commands, like the MCP server.
- **Why:** One source of truth, no second state model, and nothing to install beyond Rust. A framework (React, Svelte) can replace `ui/` later without touching the commands.
- **Trade-off:** No component library or type checking in the UI yet. Revisit once the screens settle.

## ADR-010: Timestamps are fixed-width

- **Status:** accepted (2026-11-22)
- **Decision:** Stored timestamps always have three millisecond digits (`…:20.050Z`).
- **Why:** Records are ordered by timestamp strings. The trimmed RFC 3339 form (`20.05Z`, `20Z`) sorts wrong and caused occasional mis-ordered checkpoints in tests.

## ADR-011: Switch targets describe every AI, and keys are referenced, never stored

- **Status:** accepted (2026-11-23)
- **Decision:** Anything Evyro can hand work to is a *target* with a kind (`harness`, `api`, `ide`, `chat`) and a sign-in mode (`auto`, `app_login`, `api_key`, `local`). Built-in targets cover the known agents, IDEs and chat apps; people add their own in `targets.toml`. A target holds the *name* of an environment variable or macOS Keychain item, and the key is read only when a run starts, into that child process's environment. `app_login` removes the API-key variables for that run so the tool uses the signed-in account. IDE and chat targets do not launch an agent: Evyro writes the hand-over text to `.evyro/`, copies it and opens the app.
- **Why:** People pay for AI in different ways (subscriptions, keys, local models) and use it in different places (terminal agents, IDEs, chat). One concept keeps `switch` the same for all of them. Referencing keys keeps Evyro out of the secret-storage business and keeps keys out of the database, contracts and exports.
- **Trade-off:** Readiness checks are evidence of setup, not proof that a sign-in or key works. Chat targets depend on the person to apply changes.

