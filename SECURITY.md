# Security

## Model

- Local-first: no network calls, no cloud account. All state is in the platform data directory (`EVYRO_HOME` overrides).
- Agent output and repository files are untrusted. They cannot mark work verified, change permissions, or widen MCP scope. Repository text in context packets is fenced as `UNTRUSTED`.
- Verification receipts are only created by Evyro running a command itself. Over MCP only configured commands can run.
- Processes are launched with explicit argv, never through a shell.
- Secrets are redacted from contracts, context packets, exports, MCP responses and verification logs; sensitive files (`.env`, keys) are referenced by path and hash only.
- Destructive actions need confirmation: `restore --yes`, `memory delete --yes`, `worktrees remove --force` for dirty worktrees.
- Contracts are sealed with HMAC-SHA256 using a per-install key (`contract.key`, mode 0600 on Unix).

## Reporting

Please open a private security advisory on the repository rather than a public issue.
