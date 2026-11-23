---
name: context-checkpointing
description: Use during any long task, before stopping, and after each meaningful step, so another agent can continue from where you are.
version: 1.0.0
tags: [checkpoint, context, progress, save, continuity, session, resume]
---
# Context checkpointing

Save progress so that work survives a crash, a context limit or a switch to another agent.

## When

- After each step that changes files or makes a decision.
- Before anything risky or long-running.
- Before you stop, for any reason.

## How

Call `create_checkpoint` with:

- **summary:** one or two sentences on where things stand;
- **completed:** what is done, each item checkable;
- **remaining:** what is left, in order;
- **next_action:** the very next concrete step;
- **files:** the files you changed or that matter next.

## Rules

- Write what is true, not what you hope. Unverified work is marked as unverified.
- Never put secrets in a checkpoint.
- Record decisions separately with `record_decision`.
