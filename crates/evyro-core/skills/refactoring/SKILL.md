---
name: refactoring
description: Use when changing the structure of code without changing what it does: renames, extractions, removing duplication or tech debt.
version: 1.0.0
tags: [refactor, refactoring, cleanup, tech_debt, rename, extract, duplication, simplify]
---
# Refactoring

Change the shape of the code, not its behaviour.

## Steps

1. **Make sure tests cover the code** you are about to change. Add characterisation tests if they do not.
2. **Run the tests first** so you know they pass before you start.
3. **Take small steps:** one rename, one extraction or one move at a time.
4. **Run the tests after each step.** If they fail, undo the last step rather than debugging a big change.
5. **Keep behaviour changes out.** If you find a bug, note it in the checkpoint and fix it separately.
6. **Check the callers** with `find_symbols` or `search_code` after renames and moves.

## Done when

- The tests pass with no changes to their expectations.
- The diff is easy to review step by step.
