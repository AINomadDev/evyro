# Contributing

## Set up

You need Rust 1.80 or newer and git. The desktop app also needs the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your system, and the UI smoke test needs Node 18 or newer.

```bash
git clone https://github.com/AINomadDev/evyro && cd evyro
cargo build -p evyro-cli        # the `evyro` CLI, at target/debug/evyro
cargo run -p evyro-desktop      # the desktop app
```

While you work, point `EVYRO_HOME` at a scratch folder (`export EVYRO_HOME=$(mktemp -d)`) so your real projects and history are never touched.

## Checks

Run these before opening a pull request. CI runs the same checks on Linux, macOS and Windows.

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
bash scripts/cli-e2e.sh
cargo build -p evyro-cli && node crates/evyro-desktop/tests/ui-smoke.cjs
```

## Where things live

| Path | What it holds |
|---|---|
| `crates/evyro-core` | Continuity engine: database, tasks, sessions, checkpoints, contracts, handoffs, adapters, redaction |
| `crates/evyro-cli` | The `evyro` CLI. End-to-end scenarios are in `tests/scenarios.rs` |
| `crates/evyro-mcp` | MCP server (stdio and HTTP) |
| `crates/evyro-acp` | ACP client used by `evyro chat` |
| `crates/evyro-term` | Terminals for agent threads |
| `crates/evyro-remote` | `evyro serve` and the phone companion |
| `crates/evyro-desktop` | Tauri 2 app. The UI is plain JS in `ui/`; the smoke test is `tests/ui-smoke.cjs` |
| `docs/` | User guide, CLI reference, design notes and known limitations |

## Rules

- Add a scenario test for every user-visible behaviour. Scenarios use simulated agent binaries on PATH; never call a real agent from a test.
- Never edit a shipped migration. Add `crates/evyro-core/migrations/000N_*.sql` and append it to `MIGRATIONS` in `db.rs`.
- The product name and CLI name live in `crates/evyro-core/src/brand.rs` only.
- Never write a secret into a contract, export, log, MCP response or test fixture, and never store an API key: keep only the env var or Keychain item name.
- Launch processes with explicit argv, never through a shell.
- Docs must describe what is implemented. Put future work in `docs/BACKLOG.md`, and record what is untested in `docs/KNOWN_LIMITATIONS.md`.
- `docs/user-guide/CLI_REFERENCE.md` is checked against `--help` by a test. After changing a command, run `EVYRO_UPDATE_DOCS=1 cargo test -p evyro-cli --test cli_reference` to refresh it.

## Security issues

Please report them privately, as described in [SECURITY.md](SECURITY.md), not in a public issue.
