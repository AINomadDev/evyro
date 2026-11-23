# Changelog

All notable changes to Evyro are listed here. Versions follow [Semantic Versioning](https://semver.org). A release's notes on GitHub come from its section below (`scripts/release/notes.sh`).

## [Unreleased]

### Added

- Agent threads: run an agent or a shell in a terminal inside the desktop app.
- Review workbench: comment on lines of a diff and send the comments to a running agent.
- Code search in the CLI, over MCP and in the desktop app: file names, text and regex, symbols and outlines.
- ACP chat (`evyro chat` and desktop chat threads), with approval for each tool call.
- Skills library: 15 built-in skills, project and global skills, checks, import, export and sync into agent files.
- Remote access (`evyro serve`): pair a phone or another computer, follow the work, approve agents, chat.
- Release pipeline: a version tag publishes installers, CLI archives for macOS, Linux and Windows, `SHA256SUMS`, an SBOM and a Homebrew formula, with signed build provenance. macOS signing and notarization switch on once the certificate secrets are set.
- Desktop auto-update: the app checks for a newer release at startup and in Settings, and installs it on request after checking its signature.
- Install scripts for the CLI (`scripts/install.sh`, `scripts/install.ps1`) that check the release checksum before installing.

### Changed

- Headless Codex runs with `--sandbox workspace-write` and can write Evyro's data folder; headless Claude Code gets `--permission-mode acceptEdits` and may run only `evyro`; headless Gemini skips the folder-trust question; headless runs get a closed stdin.
- `.evyro/project.toml` accepts a command as a string, a list or a list of lists, and `evyro doctor` reports a file it cannot read.
- The desktop app can be used with the keyboard alone, and passes WCAG 2.1 AA checks (axe) on every screen in light and dark themes.

### Fixed

- npm-installed agents (`.cmd` shims) launch on Windows.
- The CLI exits quietly when a reader closes its output early.

## [0.1.0] - 2026-11-23

First version: continuity core (tasks, sessions, checkpoints, signed continuation contracts, verification receipts, drift checks, crash recovery), the `evyro` CLI, the MCP server, the Tauri desktop app, and switching to any AI (agent CLIs, API keys, IDEs, chat apps).
