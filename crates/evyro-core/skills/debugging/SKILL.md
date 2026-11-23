---
name: debugging
description: Use when a test fails, a bug is reported, or something behaves unexpectedly. Find the root cause before changing code.
version: 1.0.0
tags: [debug, debugging, bug, fix, failure, error, crash, broken, regression]
---
# Debugging

Find the cause first. A fix without a cause is a guess.

## Steps

1. **Reproduce it.** Get the exact command, input and output. If you cannot reproduce it, say so and record what you tried.
2. **Read the whole error**, including the first line of the stack trace that is in this project's code.
3. **Narrow it down.** Check recent changes (`get_git_status`), compare with a version that works, and cut the input down until the failure is small.
4. **Form one hypothesis at a time** and test it with a log line, a breakpoint or a small test.
5. **Write a test that fails because of the bug**, then fix the cause, not the symptom.
6. **Run the related tests** and record a receipt with `run_verification`.
7. **Record what you learned** with `add_memory` if it would save the next agent time.

## Avoid

- Changing several things at once.
- Catching and hiding the error.
- Calling it fixed before the reproduction passes.
