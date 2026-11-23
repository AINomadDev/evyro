# Fork vs build

## Weighted evaluation

Scores are 1 to 5. The basis for each score is in the notes below the table. Evyro's row scores the independent core as built today (no GUI yet).

| Criterion (weight) | A: Fork T3 Code | B: Fork px0 | C: Fork Paradev | D: Independent core + new shell | E: Independent core + integrate existing frontends |
|---|---:|---:|---:|---:|---:|
| Agent interoperability (20%) | 5 | 3 | 4 | 4 | 5 |
| Continuity and state reliability (20%) | 3 | 1 | 2 | 5 | 5 |
| Architecture and extensibility (15%) | 4 | 3 | 3 | 5 | 5 |
| Cross-platform feasibility (10%) | 5 | 4 | 4 | 4 | 4 |
| License and maintenance (10%) | 3 | 4 | 3 | 5 | 5 |
| Performance and resources (10%) | 4 | 5 | 3 | 5 | 5 |
| UI and developer experience (10%) | 5 | 4 | 4 | 2 | 4 |
| Testing and release quality (5%) | 4 | 3 | 3 | 4 | 4 |
| **Weighted total** | **4.10** | **3.10** | **3.20** | **4.35** | **4.75** |

## Basis

- **Interop.** T3 Code drives the most agents and speaks ACP. px0 dispatches to seven harnesses but replays transcripts for most. Strategy E gets T3-level reach by letting any frontend use Evyro through CLI + MCP, on top of Evyro's own adapters.
- **Continuity.** T3's handoff is inside its own thread model and server. px0 has no durable task state. Evyro's core was designed for this: signed contracts, receipts, drift detection, leases, crash recovery, all tested (see `crates/evyro-cli/tests/scenarios.rs`).
- **Maintenance.** Forking T3 (33k files, daily commits) or Paradev (16k files) means a permanent rebase burden; scores reflect that cost, not license quality (all are MIT).
- **Performance.** Evyro's measured numbers are in PERFORMANCE.md. T3 and Paradev ship Electron; px0's numbers are its own claims.
- **UI.** Strategy D starts at zero UI. Strategy E can lean on T3 Code / px0 for review and agent threads while Evyro ships a lean Tauri dashboard.

## Decision

Strategy E: an independent Rust core and CLI that own continuity, exposed through MCP and a versioned local API, with a Tauri 2 desktop dashboard on top, plus optional integration with T3 Code (as an agent runner that uses Evyro's MCP server) and px0 (as a review surface). No fork. See ADR-001.

The user's note ("if an existing similar app fits, fork and extend it") was taken seriously: T3 Code is the strongest base, but its continuity model is tied to its own server and GUI, which the spec rules out ("the continuity engine must not depend on its GUI"). Integration gets T3's strengths without inheriting that coupling.
