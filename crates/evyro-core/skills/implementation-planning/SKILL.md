---
name: implementation-planning
description: Use before starting a change that touches more than one file or will take more than one session.
version: 1.0.0
tags: [plan, planning, steps, implementation, feature, task, epic]
---
# Implementation planning

Break the work into steps small enough to finish, test and hand off.

## Steps

1. **Read the task and its acceptance criteria** (`get_active_task`).
2. **Find every file you expect to touch** and why.
3. **Order the steps** so that each one leaves the code building and the tests passing.
4. **Give each step a check:** the test you will add or run, or the command that proves it works.
5. **Put the plan in a checkpoint** (`create_checkpoint`): the completed items, the remaining steps in order, and the next action.
6. **Work the plan one step at a time.** When reality differs from the plan, update the checkpoint instead of drifting.

## Good steps

- Name a file or a function, not "improve things".
- End with something that can be verified.
- Can be picked up by another agent from the checkpoint alone.
