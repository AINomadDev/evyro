# Reference projects

Open-source projects we read on purpose, now and again later, to see what developers of AI coding tools are building and to find what Evyro lacks. This list is a watch list, not a dependency list: Evyro copies ideas, never code. Before reusing anything, check the license in [LICENSE_COMPLIANCE.md](LICENSE_COMPLIANCE.md); the Apache-2.0 projects below need a notice if code were ever taken.

First read on 2026-11-25. **Depth of that read:** each project's README, license and top-level layout, from a shallow clone. Source files were not read in depth, so a feature named below is "the project says it has this", not "we confirmed how it works". Nothing from these projects was built or run.

An earlier, broader survey is in [OPEN_SOURCE_RESEARCH.md](OPEN_SOURCE_RESEARCH.md).

## Re-checking

```bash
scripts/reference-projects.sh ~/Developer/evyro-refs   # a folder outside this repository
```

It clones or updates each project and prints the commit it moved from and to. Read what changed, then add a dated line under "Check log" and open tasks for real gaps. Do not build or run anything in those copies.

## The list

| Project | License | What it is | Read at (date) |
|---|---|---|---|
| [pingdotgg/t3code](https://github.com/pingdotgg/t3code) | MIT | Desktop, web and phone app that drives Claude Code, Codex, Cursor, OpenCode and any ACP agent. The project the others are measured against | a05a8518 (2026-11-25) |
| [maria-rcks/t1code](https://github.com/maria-rcks/t1code) | MIT | T3 Code in a terminal UI | aeb4ce0 (2026-06-03, quiet) |
| [MiaAI-Lab/m3code](https://github.com/MiaAI-Lab/m3code) | MIT | T3 Code fork built for local models, with all telemetry removed | 30d72c63 (2026-11-25) |
| [stablyai/orca](https://github.com/stablyai/orca) | MIT | Agent IDE: bots from Slack/Linear/GitHub, chat or terminal, orchestration, mobile, SSH | 095b7fda (2026-11-25) |
| [getpaseo/paseo](https://github.com/getpaseo/paseo) | Apache-2.0 | Daemon plus desktop, mobile, web and CLI clients; plugins; voice; encrypted relay | cd2e56f (2026-11-25) |
| [slopus/happy](https://github.com/slopus/happy) | MIT | Multi-model harness with end-to-end encrypted mobile apps and shared sessions | cd91953 (2026-11-24) |
| [OrchestratorInc/agent-orchestrator](https://github.com/OrchestratorInc/agent-orchestrator) | Apache-2.0 | Fleet of workers on one board; CI, review and conflicts routed back to the owning worker | 20136d2 (2026-11-25) |
| [ParadevAI/Paradev](https://github.com/ParadevAI/Paradev) | MIT | Parallel workspaces, in-app browser, automations, CLI/MCP/SDK so agents drive the app | 296f25b4 (2026-11-24) |
| [get-bb/bb](https://github.com/get-bb/bb) | MIT | "Agent IDE that builds itself"; every surface (app, web, CLI, HTTP API) drives the same threads | fb045a0 (2026-11-24) |
| [generalaction/emdash](https://github.com/generalaction/emdash) | Apache-2.0 | Parallel agents in worktrees, local or over SSH; issues from ten trackers; lifecycle hooks | c876989 (2026-11-23) |
| [Emanuele-web04/synara](https://github.com/Emanuele-web04/synara) | MIT | Local-first desktop app: threads, split views, previews, automations, external MCP | 001454e (2026-11-25) |
| [coollabsio/jean](https://github.com/coollabsio/jean) | Apache-2.0 | Tauri desktop plus headless server; Plan/Build/Yolo modes, "magic commands", issue-to-worktree bot | c2d9b7d (2026-11-24) |
| [monorepo-labs/dray](https://github.com/monorepo-labs/dray) | Apache-2.0 | Tauri 2 (Rust and React) desktop app, the same stack as Evyro. Worktree, diff, PR, issue and browser panels per session; a `dray` CLI over a local socket lets one agent create, list and message others; iPhone client; Linux server mode; local dictation | 83306bf (2026-11-25) |

**dray** was added on 2026-11-25 after the link was supplied. It is the closest in stack to Evyro (Tauri 2 with a Rust core).

## What they have that Evyro does not (yet)

Only gaps are listed. Everything these projects do that Evyro already covers (parallel worktrees, task switching, handoffs, a phone companion, MCP, diff review comments, receipts) is left out. Each gap has a task in the tracker, under the epic T-108 "Learn from reference projects" (T-109 to T-127).

| Gap | Seen in | Why it matters to Evyro |
|---|---|---|
| A watchdog for a stuck or looping agent: repeated text, long silence, no first token | m3code | Evyro knows when an AI's process is gone, not when it is alive and useless. Its answer should be the switch flow, not a kill |
| Plan / Build / Auto modes with plan approval before edits | jean | A person approves the plan of a risky task, then lets the AI run |
| One AI splits a task and starts workers | orca, agent-orchestrator, paseo, paradev, dray (agents message each other through a local socket CLI) | `evyro work` starts one AI; there is no way for an AI to hand pieces to others through MCP |
| A board that puts what needs you on top | agent-orchestrator | Evyro's thread sidebar groups this way; the task board does not |
| Failing CI and review comments go back to the AI that owns the branch | orca, agent-orchestrator | Evyro does not watch a pull request's checks yet (T-61 and T-62 only plan to show them). Sending the failure to the AI that owns the branch is the useful half |
| Run a task on an SSH host | emdash, orca, jean, m3code | Evyro's phone page reaches the machine the app runs on; it cannot start an AI on another host |
| Scheduled and triggered runs | paradev, synara, jean | A nightly dependency review or a run on a new issue |
| More issue trackers: Linear, Jira, Sentry, and others | emdash, jean, orca | T-60 covers GitHub and GitLab only |
| Resolve merge conflicts, draft commit and PR text, investigate an alert, write release notes | jean, orca | These are repeated chores; each is a short prompt with the right context |
| Hooks in each AI's own config so status and resume work for AIs other than Claude Code | emdash | Evyro installs hooks for Claude Code only |
| Open the worktree in an editor or terminal in one click | jean, paradev | Small and expected |
| A push notification to the paired phone when an AI needs you | happy, orca, t3code, paseo, agent-orchestrator | Evyro's phone page is pull only |
| A page that lists every network call Evyro makes | m3code | A cheap trust signal; Evyro already sends no telemetry |
| A "local models only" setting that never falls back to a cloud AI | m3code | Matters for teams that cannot send code out |
| Install through winget, scoop, npx and a one-line script | t3code, agent-orchestrator, bb | Evyro has a Homebrew script only |
| A short recap on each thread and side notes | jean, synara | Handoffs carry state; a person returning to a thread has to read it |
| A small always-on panel showing which AI needs you (dray's "Island" on the Mac) | dray |
| An in-app browser with click-to-edit | paradev, synara, dray | Larger and later |
| Voice control, shared multiplayer sessions | paseo, happy | Interesting; not now |

## Deliberately not copied

- **Cloud agents and hosted workspaces** (orca, agent-orchestrator). Evyro is local first; the code stays on the person's machines.
- **Telemetry for growth.** m3code removed all of it. Evyro's tracked opt-in counters (T-96) stay opt-in with a preview, or are dropped.
- **Token-saving plugins that rewrite an AI's output or commands** (jean's RTK and Caveman). Out of scope for a continuity tool.

## Check log

| Date | What was read | Result |
|---|---|---|
| 2026-11-25 | READMEs, licenses and layout of the projects above | Gaps above, tasks added to the tracker |
| 2026-11-25 | dray, after the link was supplied | One more gap (always-on panel); one more source for task splitting |
