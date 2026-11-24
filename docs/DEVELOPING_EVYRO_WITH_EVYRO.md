# Developing Evyro with Evyro

Evyro tracks its own work. This page is the working agreement for building it with several AIs at once.

## Rules that keep AIs from colliding

1. **One AI per task.** A task has one owner at a time (a lease). A second AI that tries to take it is refused.
2. **One AI per set of files.** Give a task its files with `--file` (a folder or a file). Evyro refuses a second task whose files overlap one another AI holds in the same checkout.
3. **Parallel work gets its own checkout.** Two AIs never share a working copy: `evyro worktrees add --task T-n` creates a branch and folder per task.
4. **Only Evyro verifies.** A task is `verified` only after its criteria are met and `evyro verify` passes for the current code.

## Day to day

```bash
evyro tasks --ready                              # what can start
evyro task start T-32 --agent claude-code        # claim a task
evyro run --agent codex --task T-27              # or launch an agent on it
evyro checkpoint --done "..." --next "..."       # save progress often
evyro verify -- cargo test --workspace --locked  # proof, not a claim
```

Start with the epics in the tracker: each epic's subtasks are written to be independent, so different AIs can take different ones.

## When an AI runs out of usage

Do not wait for the limit to reset, and do not start a second AI on the same task by hand. Switch:

```bash
evyro switch --to codex --task T-32 --yes
evyro switch --to claude-code --model <smaller-model> --task T-32 --yes   # same AI, other model
```

`--yes` is your confirmation that the current AI's usage has run out or is about to; without it Evyro asks. Then Evyro:

1. ends the current AI's session and frees the task,
2. stops its process,
3. carries the latest checkpoint (summary, done, remaining, next step) into a signed handoff,
4. starts the new AI from that handoff.

In the desktop app the low-usage banner offers **Switch to <AI>…**, and a pane that was refused because another AI holds the task offers **Switch T-n here…**. Both ask for the same confirmation, let you pick a model, stop the old AI and keep its saved output.

Work done since the last checkpoint stays in the files but is not described to the next AI, so checkpoint often, and use the banner's checkpoint draft when usage is low.

## Before you open a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
bash scripts/cli-e2e.sh
node crates/evyro-desktop/tests/ui-smoke.cjs      # if the desktop UI changed
```

Record the run with `evyro verify` so the receipt is on the task.
