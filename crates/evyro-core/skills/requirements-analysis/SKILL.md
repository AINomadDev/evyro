---
name: requirements-analysis
description: Use when a task is vague, a request can be read more than one way, or acceptance criteria are missing.
version: 1.0.0
tags: [requirements, spec, acceptance, criteria, scope, feature, epic, ambiguous]
---
# Requirements analysis

Turn a request into something you can build and check.

## Steps

1. **Restate the goal** in your own words, as the user would see the result.
2. **Find the readings.** If the request can mean two things, write both down. Pick the most likely one and say which you picked.
3. **Write acceptance criteria** a test or a person can check: inputs, expected output, and what must not happen.
4. **Mark the edges:** empty input, very large input, errors, permissions, and platforms.
5. **List what is out of scope**, so it does not creep in.
6. **Record open questions** in the checkpoint's remaining items. Do not block on them if a safe default exists.

## Output

A short list of criteria, added to the task (`update_task`) or the checkpoint, so `check_acceptance` and the next agent can use them.
