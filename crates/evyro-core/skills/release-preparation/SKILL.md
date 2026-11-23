---
name: release-preparation
description: Use when preparing a release: version bump, changelog, final checks and packaging.
version: 1.0.0
tags: [release, version, changelog, tag, publish, packaging, ship]
---
# Release preparation

Make the release repeatable and boring.

## Checklist

1. **Everything merged** that belongs in the release, and nothing else.
2. **Full tests pass** on every supported platform (`run_verification` for each configured command).
3. **Version bumped** in every place the project keeps it.
4. **Changelog written** for users: what is new, what changed, what was fixed, and anything that breaks.
5. **Docs updated** for new commands and options.
6. **Build the release artifacts** with the project's release script, and record checksums.
7. **Smoke test** an installed artifact, not just the development build.
8. **Tag and publish only with the person's approval.** Publishing cannot be undone.
