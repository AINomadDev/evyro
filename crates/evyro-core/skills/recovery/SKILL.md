---
name: recovery
description: Use when resuming after a crash, a killed session, a failed launch, or when the repository is not in the state the last checkpoint describes.
version: 1.0.0
tags: [recovery, recover, crash, resume, failed, interrupted, drift, restore]
---
# Recovery

Work out what really happened before continuing.

## Steps

1. **Read the latest checkpoint** (`get_checkpoint`) and the active task (`get_active_task`).
2. **Compare with the repository** (`get_git_status`): which files changed since the checkpoint, and are any half-written?
3. **Run the tests** to see what state the code is in.
4. **Decide per file:** keep finished work, finish or revert partial edits. Do not discard changes you do not understand. Ask, or record them in the checkpoint.
5. **Create a new checkpoint** that describes the real state.
6. **Continue from the next action**, or update it if it no longer makes sense.

## Never

- Reset or delete work to get a clean state without the person's approval.
- Claim earlier steps were verified unless a receipt says so.
