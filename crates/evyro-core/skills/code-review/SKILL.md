---
name: code-review
description: Use when reviewing a diff, your own before handing off, or another agent's work, or when addressing review comments.
version: 1.0.0
tags: [review, code-review, diff, pull, request, feedback, comments]
---
# Code review

Read the change as the next person who has to maintain it.

## Look at

1. **Correctness:** does it do what the task asks? Check edge cases, errors and empty inputs.
2. **Tests:** is new behaviour covered, and do the tests check results rather than just run code?
3. **Clarity:** names, size of functions, and comments where the reason is not obvious.
4. **Consistency** with the project's existing patterns and rules.
5. **Safety:** secrets, input handling, and anything the security-review skill covers.
6. **Scope:** unrelated changes belong in another task.

## With Evyro

- Read open comments with `get_review_comments`, address each one, then `resolve_review_comment`.
- Say plainly what you checked and what you did not.
