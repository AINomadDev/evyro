---
name: security-review
description: Use when code handles input from users or the network, secrets, files, authentication, permissions, or runs commands.
version: 1.0.0
tags: [security, review, secret, auth, permission, injection, vulnerability, input]
---
# Security review

Look for ways the code can be made to do something it should not.

## Check

1. **Input:** every value from a user, file, network or agent is validated before use. Look for injection into SQL, shell commands, paths, HTML and prompts.
2. **Paths:** joined paths cannot escape their root (`..`, absolute paths, symlinks).
3. **Commands:** no shell string built from input. Arguments are passed as a list.
4. **Secrets:** none in code, logs, errors, commits, checkpoints or handoffs. Keys come from the environment or the system keychain.
5. **Permissions:** each action checks who is allowed to do it, on the server side.
6. **Dependencies:** new ones are maintained and needed.
7. **Errors:** failures are refused safely and do not reveal internals.

## Report

For each finding: where it is, how it could be used, how serious it is, and the fix. Do not mark a finding resolved without a test or a clear reason.
