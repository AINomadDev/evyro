# Open-source research

Inspected on 2026-11-22 by shallow-cloning each repository at its default branch. Commit hashes are the HEAD that was read. "Inspected" means the source tree, license file and README were read; for T3 Code, px0, relay and OpenCode specific source files were read as noted. Nothing here is a benchmark of those projects: their performance claims are recorded as claims.

## Summary table

| Project | Commit (date) | License | Main language | Status on 2026-11-22 | Relevance to Evyro |
|---|---|---|---|---|---|
| [pingdotgg/t3code](https://github.com/pingdotgg/t3code) | 324ac49c (2026-11-22) | MIT | TypeScript (Effect), Electron 44 desktop | Very active | Best candidate frontend / control plane |
| [px0-ai/px0](https://github.com/px0-ai/px0) | b4e324b (2026-11-06) | MIT | Go single binary + JS web UI | Active | Review workbench, navigation reference |
| [ParadevAI/Paradev](https://github.com/ParadevAI/Paradev) | 640a0d9b (2026-11-22) | MIT | TypeScript, Electron 44 | Very active | Competing workspace shell |
| [OpenHands/OpenHands](https://github.com/OpenHands/OpenHands) | 3e9cf43 (2026-11-22) | MIT | TS/Python | Very active | Agent platform, not a continuity layer |
| [BloopAI/vibe-kanban](https://github.com/BloopAI/vibe-kanban) | 3f095f0 (2026-11-15) | Apache-2.0 | Rust + TS | README says it is sunsetting | Rejected as base; task/worktree ideas only |
| [anomalyco/opencode](https://github.com/anomalyco/opencode) | 4332e24 (2026-11-22) | MIT | TypeScript | Very active | Adapter target (`opencode run`, `--prompt`) |
| [aaif-goose/goose](https://github.com/aaif-goose/goose) | 36b2911 (2026-11-20) | Apache-2.0 | Rust + TS | Active | Adapter target, ACP support |
| [cline/cline](https://github.com/cline/cline) | 8075d2e (2026-11-20) | Apache-2.0 | TypeScript | Active | IDE agent, fallback path |
| [RooCodeInc/Roo-Code](https://github.com/RooCodeInc/Roo-Code) | b867ec9 (2026-05-15) | Apache-2.0 | TypeScript | No commits since May 2026 | IDE agent, fallback path |
| [continuedev/continue](https://github.com/continuedev/continue) | 5522c6f (2026-07-20) | Apache-2.0 | TypeScript | README: no longer maintained, read-only | Rejected |
| [Aider-AI/aider](https://github.com/Aider-AI/aider) | 5dc9490 (2026-05-22) | Apache-2.0 | Python | Slower activity | Adapter target (`--message`, `--read`) |
| [dbisina/relay](https://github.com/dbisina/relay) | b272286 (2026-08-12) | Apache-2.0 | Go daemon (+ Rust/egui app) | Active | Closest overlap: signed handoff contract |
| [siudajakub/agent-context-kit](https://github.com/siudajakub/agent-context-kit) | e7a086c (2026-07-12) | MIT | Markdown + shell | Small | Markdown conventions only |
| [desikai-lab/Marrow](https://github.com/desikai-lab/Marrow) | 2068e24 (2026-11-05) | MIT | Python | Active | MCP memory server; ideas only |
| [PatrickJS/shared-agent-state](https://github.com/PatrickJS/shared-agent-state) | 1a1f469 (2026-05-27) | MIT | JS | Small | File-backed state; ideas only |
| [arxdsilva/agent-office](https://github.com/arxdsilva/agent-office) | dff8e89 (2026-10-18) | MIT | TypeScript desktop | Active | Competing product (projects, memory, handoffs) |
| [steveyegge/beads](https://github.com/steveyegge/beads) | 7862410 (2026-11-20) | MIT | Go (Dolt-backed) | Very active | Task graph model reference |
| [amutix/amutix](https://github.com/amutix/amutix) | 14848e8 (2026-07-03) | MIT | TypeScript | Small | Reservations, review handoffs; ideas only |
| [coleam00/Archon](https://github.com/coleam00/Archon) | 579e404 (2026-11-22) | MIT | TypeScript | Active (v2 is a YAML workflow engine) | Out of scope now |
| [oraios/serena](https://github.com/oraios/serena) | 7f7d624 (2026-11-19) | MIT | Python | Active | Optional MCP integration for symbols |
| [yamadashy/repomix](https://github.com/yamadashy/repomix) | f0bcc08 (2026-11-21) | MIT | TypeScript | Active | Context packing reference |
| [obra/superpowers](https://github.com/obra/superpowers) | 22f0b46 (2026-11-03) | MIT | Markdown skills | Active | Skills format reference |
| [OpenHands/software-agent-sdk](https://github.com/OpenHands/software-agent-sdk) | ebec01b (2026-11-22) | MIT | Python | Very active | Not needed for continuity core |

Note: Serena was MIT at the inspected commit; re-check before embedding anything.

## T3 Code (deep dive)

What it is: "the open-source control plane for coding agents". A Node server (`apps/server`) built with Effect, clients for desktop (Electron 44), web, iOS and Android, and ~40 packages including `effect-acp`, `provider-acp`, per-provider packages (Cursor, OpenCode, Grok, Kiro, Pi, Muse, fx), source-control integrations (GitHub, GitLab, Bitbucket, Forgejo, Azure DevOps), `ssh` and `tailscale` for remote access, and node:sqlite persistence.

Findings that matter for Evyro:

- Agent coverage is the broadest of any candidate, including ACP agents. It drives the agent CLIs the user is already signed in to.
- It supports changing models mid-thread and "let Claude hand a task to Codex". That continuity lives inside T3's own thread model and server; there is no portable, harness-independent contract, no verification receipts, and no evidence gate on completion.
- Desktop is Electron, which conflicts with the Tauri 2 preference but is mature and fast in practice per its own profiling scripts (not measured here).
- MIT license; forking is legally simple. The codebase is large (33k files at depth 1) and moves daily, so a fork would carry a heavy rebase burden.

Conclusion: integrate, do not fork. T3 Code can drive agents that have Evyro's MCP server configured, so Evyro state is available inside T3 threads without coupling either codebase.

## px0 (deep dive)

What it is: a review workbench and speed-first browser IDE, shipped as a single static Go binary (Go 1.25, deps: chroma, goldmark, `px0-ai/harness`). Source files read: `agent.go`, `git.go`, `pr.go`, `review_workflow.go`, `search.go`, `internal/fuzzy`, `internal/symbols`.

Findings:

- Agent dispatch (`agent.go`) builds explicit argv per harness: Claude `--output-format stream-json --verbose --session-id/--resume`, Gemini `--output-format stream-json --resume`, Cursor `--resume`, Antigravity (`agy`) `--conversation`; other harnesses are replayed the transcript. Evyro's adapters follow the same explicit-argv rule.
- Review: PR review across GitHub/GitLab/Gitea/Bitbucket, unpushed-commit review, inline drafts that survive sessions, virtual rendering for very large files.
- Claims "< 1 ms cold start" and "~20–30 MB daemon RSS". Not independently benchmarked yet (see PERFORMANCE.md for Evyro's own numbers and method).

Conclusion: option B + C from the spec. Launch px0 as an optional review surface for a task's worktree (`px0 <worktree>`), and use its code as a reference for fuzzy search and symbol outlines. No code is copied.

## Relay (closest conceptual overlap)

Relay describes itself as "the handoff protocol for AI coding agents: a signed, portable contract". Its v2 contract (`packages/daemon-go/internal/contract/testdata/contract_v2.json`) has `taskGoal`, `nextAction`, `doNotRedo`, `plan`, `tasksRemaining`, `acceptanceAssertions`, `fileManifest` (path, sha256, modified), `decisions`, `constraints`, `baseCommitSha`, and an HMAC-SHA256 signature over canonical JSON.

Evyro adopted two ideas, re-implemented independently: an explicit `do_not_redo` list and HMAC integrity over canonical JSON. Evyro adds per-fact provenance (agent-reported vs Evyro-captured vs Evyro-verified), verification receipts tied to a repository fingerprint, a task graph, and drift detection on resume. A Relay import/export adapter is in the backlog.

## Protocols

- MCP: implemented as a stdio JSON-RPC server negotiating 2025-11-25, 2025-06-18, 2025-03-26 and 2024-11-05. The official Rust SDK (`rmcp`) was evaluated; the hand-written server was kept for now because it has no async runtime dependency and keeps cold start ~2 ms. Migration to `rmcp` is tracked in the backlog.
- ACP: not implemented in Evyro. ACP is client-to-agent; Evyro is a state layer agents and clients talk to through CLI and MCP. T3 Code and Goose already speak ACP.
- Agent hooks: Claude Code hooks (PostToolUse, PreCompact, Stop, SessionStart) are used for automatic checkpoints via `evyro integrate claude-code --hooks`.
