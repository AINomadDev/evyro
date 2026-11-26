# Security

## Model

- Local-first: no network calls, no cloud account. All state is in the platform data directory (`EVYRO_HOME` overrides).
- Agent output and repository files are untrusted. They cannot mark work verified, change permissions, or widen MCP scope. Repository text in context packets is fenced as `UNTRUSTED`.
- Verification receipts are only created by Evyro running a command itself. Over MCP only configured commands can run.
- Processes are launched with explicit argv, never through a shell.
- Secrets are redacted from contracts, context packets, exports, MCP responses and verification logs; sensitive files (`.env`, keys) are referenced by path and hash only.
- Saved thread output and chat transcripts are written with secrets redacted (best effort: a secret split across screen updates can be missed) and readable only by you (0700/0600 on Unix). The live view is unchanged. You choose how many ended threads and chats to keep (Settings, Saved history) and can delete them all at any time.
- Destructive actions need confirmation: `restore --yes`, `memory delete --yes`, `worktrees remove --force` for dirty worktrees.
- Contracts are sealed with HMAC-SHA256 using a per-install key (`contract.key`, mode 0600 on Unix).

## Dependencies

- Every pull request runs `cargo deny check advisories licenses bans sources` (the `audit` job in CI, configured in `deny.toml`).
- Once a month Dependabot opens one grouped update pull request for Rust crates and one for GitHub Actions (`.github/dependabot.yml`). Security advisories open their own pull requests straight away.
- On the 1st of each month the `dependency-review` workflow re-runs `cargo deny` on `main`, so a new advisory shows up even when no pull request is open. It can also be run by hand from the Actions tab. When it fails, update the dependency, or, if it cannot move yet, add the advisory to `ignore` in `deny.toml` with the reason.
- Known exception: glib 0.18 (GHSA-wrw7-89jp-8q8g, unsound `VariantStrIter`) is pinned by Tauri's Linux GTK3 stack and the fix needs glib 0.20. Evyro does not use `VariantStrIter`, so the Dependabot alert is dismissed. Revisit when Tauri moves to GTK4.

## Reporting

Please open a private security advisory on the repository rather than a public issue.

See [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) for the assets, trust boundaries and each threat with its mitigation.
