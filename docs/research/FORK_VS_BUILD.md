# Fork vs build

## Weighted evaluation

Scores are 1 to 5. The basis for each score is in the notes below the table. Strategy E's column scores Evyro as built on 2026-11-23, after the parity work (agent threads, review workbench, code search, ACP chat, skills, remote access). A score was raised only where the evidence in [Re-score, 2026-11-23](#re-score-2026-11-23) supports it.

| Criterion (weight) | A: Fork T3 Code | B: Fork px0 | C: Fork Paradev | D: Independent core + new shell | E: Independent core + integrate existing frontends |
|---|---:|---:|---:|---:|---:|
| Agent interoperability (20%) | 5 | 3 | 4 | 4 | 5 |
| Continuity and state reliability (20%) | 3 | 1 | 2 | 5 | 5 |
| Architecture and extensibility (15%) | 4 | 3 | 3 | 5 | 5 |
| Cross-platform feasibility (10%) | 5 | 4 | 4 | 4 | 4 |
| License and maintenance (10%) | 3 | 4 | 3 | 5 | 5 |
| Performance and resources (10%) | 4 | 5 | 3 | 5 | 5 |
| UI and developer experience (10%) | 5 | 4 | 4 | 2 | 5 |
| Testing and release quality (5%) | 4 | 3 | 3 | 4 | 4 |
| **Weighted total** | **4.10** | **3.10** | **3.20** | **4.35** | **4.85** |

## Basis

- **Interop.** T3 Code drives the most agents and speaks ACP. px0 dispatches to seven harnesses but replays transcripts for most. Strategy E gets T3-level reach by letting any frontend use Evyro through CLI + MCP, on top of Evyro's own adapters.
- **Continuity.** T3's handoff is inside its own thread model and server. px0 has no durable task state. Evyro's core was designed for this: signed contracts, receipts, drift detection, leases, crash recovery, all tested (see `crates/evyro-cli/tests/scenarios.rs`).
- **Maintenance.** Forking T3 (33k files, daily commits) or Paradev (16k files) means a permanent rebase burden; scores reflect that cost, not license quality (all are MIT).
- **Performance.** Evyro's measured numbers are in PERFORMANCE.md. T3 and Paradev ship Electron; px0's numbers are its own claims.
- **UI.** Strategy D starts at zero UI. Strategy E first leaned on T3 Code and px0 for review and agent threads. Evyro's own Tauri app now has those features too (see the re-score below), so it no longer depends on them.

## Decision

Strategy E: an independent Rust core and CLI that own continuity, exposed through MCP and a versioned local API, with a Tauri 2 desktop dashboard on top, plus optional integration with T3 Code (as an agent runner that uses Evyro's MCP server) and px0 (as a review surface). No fork. See ADR-001.

The user's note ("if an existing similar app fits, fork and extend it") was taken seriously: T3 Code is the strongest base, but its continuity model is tied to its own server and GUI, which the spec rules out ("the continuity engine must not depend on its GUI"). Integration gets T3's strengths without inheriting that coupling.

## Re-score, 2026-11-23

Goal: 5 on every criterion for Strategy E, with evidence. Each row says what the score rests on and, where it stays below 5, what is missing.

| Criterion | Before | Now | Evidence | Missing for 5 |
|---|---:|---:|---|---|
| Agent interoperability | 5 | 5 | CLI, stdio MCP and MCP over HTTP; ACP client (`evyro chat`); switch targets for agent CLIs, IDEs, chat apps and API keys; skills synced into Claude Code, Codex, Gemini and Copilot files | Nothing for the score. Still worth doing: a run with real ACP agents |
| Continuity and state reliability | 5 | 5 | Signed contracts, receipts, drift checks, leases and crash recovery, covered by 24 scenario tests | — |
| Architecture and extensibility | 5 | 5 | Separate crates for the core, MCP, ACP, terminals and remote access; chat host shared by the desktop app and `evyro serve`; versioned `/api/v1` | — |
| Cross-platform feasibility | 4 | 4 | Builds, unit tests, UI smoke and CLI end-to-end on Linux, macOS and Windows in CI; a full run on a real Mac (macOS 27.2 arm64); installers built for all three | Someone using it on Windows; the end-to-end scenario tests running on Windows (they use shell stand-ins today); installers installed on real machines |
| License and maintenance | 5 | 5 | Own code, MIT dependencies, no fork to rebase | — |
| Performance and resources | 5 | 5 | Measured numbers in PERFORMANCE.md; Tauri rather than Electron; a single small HTTP server for remote access | — |
| UI and developer experience | 4 | **5** | The desktop app now has agent threads in a terminal, chat threads with per-tool-call approval, a review workbench with line comments sent to agents, code search (files, text, symbols, outline), a skills library, a command palette, and a phone companion with approvals. The UI smoke test drives every screen on three systems (20 steps), and each feature except the Remote access card was also checked by hand in the real app under Xvfb | Not a gap for the score, but not proven yet: use by real people and with real agents, and the phone companion on a real phone |
| Testing and release quality | 4 | 4 | 73 Rust tests (unit and scenario), the UI smoke test and the CLI end-to-end script on three systems, clippy with warnings as errors, and installers with SHA-256 checksums built in CI | Signed and notarized installers; tests with real agents (Claude waits for the usage reset on 2026-12-01); CI running again (it is blocked by GitHub billing as of 2026-11-23) |

Result: Strategy E goes from 4.75 to **4.85**. Two criteria are still at 4, cross-platform and testing/release. Neither can honestly reach 5 from code alone: they need a Windows machine, code-signing certificates, and real-agent runs.

