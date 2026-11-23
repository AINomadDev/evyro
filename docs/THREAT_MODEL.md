# Threat model

Status as of 2026-11-23 (v0.1.0). This page lists what Evyro protects, who it protects it from, and what is left to the person. [SECURITY.md](../SECURITY.md) has the short version and how to report a problem.

## What Evyro protects

| Asset | Where it lives |
|---|---|
| API keys and tokens | Never stored by Evyro. A target names an env var or a macOS Keychain item, read only when an agent starts |
| Source code and secrets in the repository | The person's repository. Evyro reads it to build context, search and diffs |
| Task state, checkpoints, contracts, receipts | Evyro's database in the platform data folder (`EVYRO_HOME` overrides it) |
| Contract signing key | `contract.key` in the data folder, mode 0600 on Unix |
| Remote access device tokens | Stored only as hashes in the database |

## Who and what is not trusted

1. **Agent output.** A coding agent can be wrong, or steered by text it read. It runs with the person's account, so Evyro cannot stop what the agent's own tools do. Evyro limits what the agent can do *through Evyro*.
2. **Repository content.** Files, commit messages and issue text can carry prompt injection.
3. **Other devices on the network**, when remote access is on.
4. **Exported files and handoff packs**, which may be pasted into chat apps or shared.

The person at the keyboard, the operating system and the installed agent CLIs are trusted.

## Threats and what Evyro does about them

| Threat | Mitigation | Tested by |
|---|---|---|
| An agent marks unfinished work as verified | Only Evyro creates verification receipts, by running the command itself. `verified` needs every acceptance criterion met and a passing receipt for the current code. Over MCP only configured commands can run | Scenario tests |
| An agent or a repository file widens its own permissions | Agent output cannot change permissions or MCP scope. Repository text in context packets is fenced as `UNTRUSTED` | Scenario and unit tests |
| Command injection through task text or file names | Every process starts with explicit argv, never a shell. The prompt is one argument | Unit test `argv_is_shell_free` |
| A headless agent does more than the task needs | Claude Code headless may edit files and run `evyro` only. Codex headless writes only in the workspace, temp folders and Evyro's data folder, with no network | Unit and scenario tests (argv); a real Codex run on macOS |
| A secret leaks into a handoff, export, MCP answer, search result or log | Pattern-based redaction on every way out. `.env` and key files are referenced by path and hash, never by content. Skills with a secret are refused | Unit and scenario tests |
| A key is kept on disk by Evyro | Evyro stores only the env var or Keychain item name, and refuses a pasted key | Scenario tests; Keychain check on a real Mac |
| A contract is changed after it was written | Contracts are sealed with HMAC-SHA256 and a content hash; a changed one fails validation | Scenario tests |
| Someone on the network uses remote access | `evyro serve` listens on loopback unless `--allow-lan` is given. Devices pair with a one-time code; tokens are stored as hashes; each device has a scope (`read`, `approve`, `control`); requests are rate-limited; an audit log records actions without tokens; devices can be revoked | Scenario test for remote access |
| A tampered installer or dependency | `cargo-deny` checks advisories, licenses and sources on every PR. Release builds carry an SBOM and signed build provenance (`gh attestation verify`) | CI |

## What is left to the person

- **Agents run with your account.** An agent's own shell and file tools are limited only by the agent's sandbox and permission settings, not by Evyro. Review what an agent changed before you merge it. The review workbench is there for that.
- **Redaction is pattern-based.** An unusual secret format can be missed. Keep secrets out of the repository and in env vars or the Keychain.
- **Remote access has no HTTPS of its own.** Use it over Tailscale (or `tailscale serve`) or another encrypted link, not over open Wi-Fi.
- **Installers are not code-signed yet.** macOS and Windows warn on first open. Check the SHA-256 checksum or the build attestation before you install.
- **The data folder is not encrypted.** It is protected by your operating system account, like the agents' own history.

## Not done yet

No outside security review has been done. See [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) for everything else that has not been verified.
