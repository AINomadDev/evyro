# License compliance

## Evyro's license

Dual-licensed **MIT OR Apache-2.0**, the Rust ecosystem convention. Compatible with every dependency below.

## Code reuse

No source code from any researched repository is copied into Evyro. Ideas taken (Relay's do-not-redo list and HMAC signing, Beads' ready-work model, px0's explicit dispatch argv) are re-implemented independently. If code is ever vendored, the file must keep its original copyright header and be listed here.

## Direct Rust dependencies

| Crate | License | Use |
|---|---|---|
| rusqlite (+ bundled SQLite) | MIT (SQLite: public domain) | Storage, FTS5 |
| serde, serde_json, toml | MIT OR Apache-2.0 | Serialization |
| clap, clap_complete | MIT OR Apache-2.0 | CLI |
| portable-pty | MIT | Pseudo-terminals for agent threads (`evyro-term`) |
| @xterm/xterm 5.5.0, @xterm/addon-fit 0.10.0 (vendored in `crates/evyro-desktop/ui/vendor/xterm/`, license included) | MIT | Terminal view in the desktop app |
| sha2, hmac | MIT OR Apache-2.0 | Hashing, contract integrity |
| uuid | MIT OR Apache-2.0 | IDs |
| time | MIT OR Apache-2.0 | Timestamps |
| directories | MIT OR Apache-2.0 | Platform data dirs |
| regex | MIT OR Apache-2.0 | Redaction |
| thiserror, anyhow | MIT OR Apache-2.0 | Errors |
| tempfile (dev) | MIT OR Apache-2.0 | Tests |

Run `cargo deny check licenses` (configured in CI backlog) before each release to cover transitive crates.

## Integrations (no linking)

T3 Code (MIT), px0 (MIT), Serena (MIT at 7f7d624), and agent CLIs are invoked as separate processes or MCP servers. No distribution obligations arise from invoking them.
