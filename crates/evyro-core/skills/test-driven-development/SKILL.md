---
name: test-driven-development
description: Use when adding a feature or fixing a bug where a test can show the behaviour before the code exists.
version: 1.0.0
tags: [test, tdd, testing, feature, bug, regression, unit]
---
# Test-driven development

Write the test that fails, then the code that makes it pass.

## The loop

1. **Write one test** for the next small piece of behaviour. Use the project's existing test style and helpers.
2. **Run it and watch it fail** for the reason you expect. A test that passes before the change proves nothing.
3. **Write the least code** that makes it pass.
4. **Run the test again**, then the related tests (`run_verification` with `kind: test`).
5. **Clean up** names and duplication while the tests stay green.
6. **Repeat** for the next behaviour.

## Rules

- For a bug, the first test reproduces the bug.
- Never weaken, skip or delete a test to make it pass.
- Report test results as they are. If a run did not happen, say so.
