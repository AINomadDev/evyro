# Developing Evyro with Evyro

Evyro tracks its own work. This page is the working agreement for building it with several AIs at once.

## Rules that keep AIs from colliding

1. **One AI per task.** A task has one owner at a time (a lease). A second AI that tries to take it is refused.
2. **One AI per set of files.** Give a task its files with `--file` (a folder or a file). Evyro refuses a second task whose files overlap one another AI holds in the same checkout.
3. **A worktree only when it is needed.** An AI works in the main checkout unless another AI is there and the two cannot be shown to touch different files. Evyro decides this for you; see below.
4. **Only Evyro verifies.** A task is `verified` only after its criteria are met and `evyro verify` passes for the current code.

## Day to day: nothing to set up

```bash
evyro work                      # starts the most important ready task on a free AI
evyro work --agent codex        # same, with an AI you choose
evyro work --dry-run            # show what would start
```

`evyro work` (and **Work on next task** in the desktop) does the rest itself:

- picks the most important ready task that no one holds and whose files no other AI is using,
- picks an installed AI that isn't working on anything,
- decides where it works, without you creating anything (see below),
- starts the AI from the task's context and records its session and checkpoints.

You can still name things: `evyro run --agent codex --task T-27`, or `evyro task start`. The worktree rule applies to all of them.

```bash
evyro checkpoint --done "..." --next "..."       # save progress often
evyro verify -- cargo test --workspace --locked  # proof, not a claim
```

Start with the epics in the tracker: each epic's subtasks are written to be independent, so different AIs can take different ones.

## Tasks that need a person

Some work cannot be done by an AI: pairing a real phone, signing with a certificate, signing in to an account. Mark it so Evyro never hands it to one:

```bash
evyro task add "Sign the Windows installer" --needs-person "a code-signing certificate"
evyro task update T-22 --needs-person "a code-signing certificate"
```

`evyro work` and **Work on next task** skip these tasks however important they are, and `evyro tasks` marks them "(needs a person)". Naming one explicitly (`evyro run --task T-22`) still works.

## Where an AI works (no worktree unless needed)

Evyro does not make a worktree for every task. Before an AI starts, it picks the first rule that applies:

1. **The task already has a worktree** (unfinished work, a switch, a restart): continue there. A worktree whose folder was deleted is restored on its branch with its commits.
2. **The task continues an unfinished parent task** whose worktree holds pending work and nobody is using it: continue in that worktree.
3. **No other AI is working in the main checkout:** work in the main checkout.
4. **Another AI is there, and both tasks declare files that do not overlap** (`--file`): share the main checkout.
5. **Another AI is there and the files overlap** (or the other AI's uncommitted changes touch them): refused. One AI works on a set of files at a time; finish or switch the other task.
6. **Another AI is there and independence can't be shown** (a task has no declared files): the task gets its own worktree and branch (`evyro/t-n`), and Evyro says why. Uncommitted changes in the main checkout are not in it.

Declare a task's files when you add it (`evyro task add ... --file src/area`) and unrelated tasks can run side by side in one checkout with no worktrees at all.

Tidy up finished work with `evyro worktrees prune` (add `--dry-run` to look first). It removes only worktrees of finished tasks that are clean, merged and not in use, and says why it kept the others.

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
