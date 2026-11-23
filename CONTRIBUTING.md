# Contributing

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

- End-to-end scenarios live in `crates/evyro-cli/tests/scenarios.rs` and use simulated agent binaries on PATH. Add a scenario for every user-visible behaviour.
- Never edit a shipped migration; add `crates/evyro-core/migrations/000N_*.sql` and append it to `MIGRATIONS` in `db.rs`.
- Product name and CLI name live in `crates/evyro-core/src/brand.rs` only.
- Docs must describe what is implemented. Put future work in `docs/BACKLOG.md`.
