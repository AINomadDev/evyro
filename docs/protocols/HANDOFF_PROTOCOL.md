# Handoff protocol

## Lifecycle

1. **Prepare** (`evyro handoff --to <agent>` or MCP `prepare_handoff`)
   - Resolves the project and task (given, or the active in-progress task).
   - Moves the owning session to `handing_off`.
   - Takes a final checkpoint (trigger `handoff`) capturing Git state with per-file sha256.
   - Builds the contract from durable records: task, acceptance criteria, constraints, decisions, checkpoint history, receipts.
   - Redacts every string, marks sensitive files (`.env`, keys) as path+hash only.
   - Scores quality against seven explicit criteria.
   - Seals (content hash + HMAC), writes `.evyro/handoffs/<id>.json` and `.md` atomically.
   - Marks the source session `completed` (reason `handoff:<id>`), releasing the lease. Older pending handoffs for the task become `superseded`.
   - If anything fails, the source session goes back to `running`; nothing is lost.
2. **Resume** (`evyro resume --agent <agent>`, or automatically by `evyro run --agent <agent>`)
   - Verifies integrity, project id, worktree, checkpoint, commit existence, known test commands.
   - Detects drift. Drift stops here (exit 5) unless `--accept-drift`.
   - Starts the target session, takes the lease, marks the handoff `accepted`, moves the task to `in_progress` if it was not.
   - Prints the readable contract for the agent.
   - `evyro task start <task> --agent <agent>` and MCP `claim_task` go through the same checks when the task has an open handoff for that agent, so the new session always carries the contract.
   - Interactive launches (`evyro run`, `evyro switch` without `--headless`) are refused before anything changes when there is no terminal.
   - If the receiving session ends `failed` or `cancelled`, the handoff goes back to `prepared` (event `handoff.reopened`). Any work that session did shows up as drift on the next resume. A newer handoff for the same task is never overridden.
3. **Continue**: the agent checkpoints with `evyro checkpoint --done … --next …` and verifies with `evyro verify -- <cmd>`.

## Contract v1 fields

| Field | Meaning | Provenance |
|---|---|---|
| `schema_version` | `"1.0"` | |
| `contract_id`, `event_id`, `created_at`, `producer` | Identity | |
| `project`, `task` | Ids, key, title, description, acceptance criteria, constraints | task record |
| `source`, `target` | Harness, model, provider, session | session record |
| `parent_checkpoint_id` | Handoff checkpoint | |
| `objective` | From the task record | task record |
| `summary`, `completed[]`, `remaining[]`, `blockers[]`, `next_action` | Progress | `agent_reported` or `human_recorded` |
| `do_not_redo[]` | Completed items + commands already passing on this exact state | mixed |
| `decisions[]` | Recorded decisions | decision record |
| `files[]` | Changed files: path, porcelain status, sha256, size, `sensitive` | `evyro_captured` |
| `git` | Root, branch, commit, dirty, upstream, ahead/behind, fingerprint, recent commits, diff stat | `evyro_captured` |
| `verification[]` | Receipts: argv, exit code, passed, time, commit, `matches_handoff_state` | `evyro_verified` |
| `environment` | OS, arch, build/test/lint argv | project record |
| `ownership` | Lease transfer note | |
| `resume_protocol[]` | Steps the receiving agent must follow | |
| `redaction` | Applied + findings by kind | |
| `quality` | Criteria met / total, each with detail | |
| `integrity` | `hmac-sha256`, content sha256, signature | |

JSON Schema: `schemas/continuation-contract.v1.schema.json`.

## Quality criteria

Clear objective · Git reference resolves · Acceptance criteria present · Completed work recorded · Remaining steps recorded · Next action stated · Verification evidence for this exact state. The score is the count met, never a percentage.

## Fallback for agents without a launchable CLI

`evyro run --agent antigravity` (or any agent not installed) prints the resume prompt instead of launching, exits 6, and leaves the handoff pending. The agent can read `.evyro/handoffs/<id>.md`, use the MCP server, and the human runs `evyro resume --agent <id>` to transfer ownership.
