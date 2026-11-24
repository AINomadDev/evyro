## What changed and why

## How it was checked

- [ ] `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` pass
- [ ] `bash scripts/cli-e2e.sh` passes (if the CLI changed)
- [ ] `node crates/evyro-desktop/tests/ui-smoke.cjs` passes (if the desktop UI changed)
- [ ] Docs updated (user guide, CLI reference or CHANGELOG) if behavior changed

## Not verified

Say what you could not test (for example an OS or a real agent).
