---
name: agent-handoff
description: Use when passing a task to another agent or model, or when your session must end before the task is finished.
version: 1.0.0
tags: [handoff, handover, switch, continue, agent, transfer, continuation]
---
# Agent handoff

Give the next agent everything it needs and nothing it has to guess.

## Steps

1. **Bring the work to a clean point:** the code builds, or the checkpoint says clearly why not.
2. **Run the tests** with `run_verification` so the next agent has real results.
3. **Create a final checkpoint** (see context-checkpointing).
4. **Call `prepare_handoff`** with the target agent, a summary, the completed and remaining items, and the next action.
5. **Stop changing files** once the handoff is prepared. The contract records the state of the repository.

## When you receive a handoff

1. Read it with `get_handoff` and check the repository matches it.
2. If the files changed since, Evyro reports drift: review the changes before continuing.
3. Continue from `next_action`, and do not redo completed work without a reason.
