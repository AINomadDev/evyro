# Fork vs build

## Weighted evaluation

Scores are 1 to 5. The basis for each score is in the notes below the table. Strategy E's column scores Evyro as built on 2026-11-23, after the parity work (agent threads, review workbench, code search, ACP chat, skills, remote access). A score was raised only where the evidence in the re-scores below supports it. The latest is [Second re-score](#second-re-score-2026-11-23).

| Criterion (weight) | A: Fork T3 Code | B: Fork px0 | C: Fork Paradev | D: Independent core + new shell | E: Independent core + integrate existing frontends |
|---|---:|---:|---:|---:|---:|
| Agent interoperability (20%) | 5 | 3 | 4 | 4 | 5 |
| Continuity and state reliability (20%) | 3 | 1 | 2 | 5 | 5 |
| Architecture and extensibility (15%) | 4 | 3 | 3 | 5 | 5 |
| Cross-platform feasibility (10%) | 5 | 4 | 4 | 4 | 5 |
| License and maintenance (10%) | 3 | 4 | 3 | 5 | 5 |
| Performance and resources (10%) | 4 | 5 | 3 | 5 | 5 |
| UI and developer experience (10%) | 5 | 4 | 4 | 2 | 5 |
| Testing and release quality (5%) | 4 | 3 | 3 | 4 | 4 |
| **Weighted total** | **4.10** | **3.10** | **3.20** | **4.35** | **4.95** |

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


## Second re-score, 2026-11-23

This round rests on PRs #10 (Windows scenario tests), #11 (release checks and accessibility) and #12 (fixes from the first real-agent runs). The scores assume those three are merged.

| Criterion | Before | Now | New evidence | Missing for 5 |
|---|---:|---:|---|---|
| Cross-platform feasibility | 4 | **5** | The end-to-end scenario tests now run on Windows too, with the agents launched through `.cmd` shims the way npm installs them there. That found and fixed a real bug: Windows refused prompts with line breaks for npm-installed agents. In CI, every installer is installed and started on its own system: the .deb and AppImage on Ubuntu, the .dmg on macOS, and the silent NSIS setup on Windows. The .dmg was also installed and opened on a real Mac | Not a gap for the score, but not proven yet: a person using Evyro on a Windows machine |
| Testing and release quality | 4 | 4 | CI is running again and green on all three systems. `cargo-deny` checks advisories, licenses and sources on every PR. Release builds carry an SBOM and signed build provenance (GitHub attestations, checkable with `gh attestation verify`). Installers are installed and started in CI. The first real-agent runs on a Mac found 5 launch bugs, which are fixed in PR #12. The real Codex CLI finished a task across two sessions and a handoff, and its verification passed | Code-signed and notarized installers, which need an Apple Developer ID and a Windows signing certificate. A real Claude Code run, after the usage reset on 2026-12-01 |

Result: Strategy E goes from 4.85 to **4.95**. Only testing and release is still at 4. It needs signing certificates and a real Claude run, and neither can come from code.

## New criteria (unweighted)

These were added on 2026-11-23. They score Strategy E only and are kept out of the weighted total, so earlier totals stay comparable.

| Criterion | Score | Evidence | Missing for 5 |
|---|---:|---|---|
| Security and privacy | 4 | Local-first, with no network calls and no account. Evyro never stores keys: it keeps only env var or Keychain item names. Secrets are redacted from contracts, context, exports, MCP output, search results and logs, and sensitive files are kept out. Agents launch with explicit argv, never a shell. Headless agents get narrow permissions (Claude Code may edit and run `evyro`; Codex may write inside the workspace only). Remote access pairs with one-time codes, stores tokens only as hashes, and has scopes, a rate limit and an audit log. CI audits dependencies, and releases carry an SBOM and signed provenance | Private vulnerability reporting, which SECURITY.md points to, is off on the repository, and so are secret scanning and Dependabot alerts. There is no written threat model and no outside review. Redaction is pattern-based |
| Docs and onboarding | 4 | A README with a quick start and a five-minute tour, a user guide, a full CLI reference, a desktop guide, and guides for remote access, skills, ACP and agent compatibility. `evyro doctor` explains what is wrong and how to fix it, and now reports a broken project.toml. The desktop app has a first-run setup, covered by the UI smoke test | No new user has followed the quick start yet. Nothing checks that the CLI reference matches `--help`. CONTRIBUTING is a stub |
| Accessibility | 4 | axe-core checks WCAG 2.1 A and AA on all 10 screens in light and dark themes, on three systems, and CI fails on any serious or critical problem. Contrast was fixed for text, buttons and the terminal (minimum contrast 4.5). Every screen can be reached from the command palette and keyboard shortcuts | No hand test with a screen reader (VoiceOver, NVDA). No recorded keyboard-only run. axe problems rated moderate or minor do not fail CI |
