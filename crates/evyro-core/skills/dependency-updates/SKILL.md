---
name: dependency-updates
description: Use when adding, upgrading or removing a dependency, or when a security advisory affects one.
version: 1.0.0
tags: [dependency, dependencies, upgrade, update, package, version, maintenance, advisory]
---
# Dependency updates

Change one dependency at a time and prove nothing broke.

## Steps

1. **Read the changelog** between the current and target versions. Note breaking changes.
2. **Check the package** is maintained, licensed compatibly, and actually needed.
3. **Update one dependency** (or one group that must move together).
4. **Update the lockfile** with the project's own tool, not by hand.
5. **Build and run the full tests** (`run_verification`).
6. **Fix breakages** in this change, and keep unrelated fixes out.
7. **Record the reason** for the update in the checkpoint or a decision.
