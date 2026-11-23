# Performance

## Method

Release build (`cargo build --release`, thin LTO, stripped). Each command run 30 times as a fresh process from Python `subprocess.run`, output discarded; median and max wall time reported. Repository: fresh Git repo with one commit and 20 tasks. Peak RSS is the largest `ru_maxrss` among all child processes.

Machine: cloud Linux VM, 4 vCPU Intel Xeon @ 2.30 GHz. **Not** an Apple Silicon Mac; re-run `scripts/bench.sh` there before quoting numbers for macOS.

## Results (2026-11-22, v0.1.0)

| Command | Median | Max |
|---|---:|---:|
| `evyro version` | 2.2 ms | 3.2 ms |
| `evyro tasks` | 3.0 ms | 4.7 ms |
| `evyro status` (includes `git status`) | 12.4 ms | 14.5 ms |
| `evyro context` (includes `git status`) | 15.1 ms | 20.8 ms |
| `evyro checkpoint --auto` (hook path) | 14.4 ms | 20.1 ms |

| Metric | Value |
|---|---:|
| Binary size (Linux x86_64) | 6.8 MB |
| Peak RSS of any command | 13.9 MB |
| Idle CPU | 0 (no daemon; nothing runs between commands) |

Most of the `status`/`context`/`checkpoint` time is spawning `git`. These numbers are for Evyro only; px0's and T3 Code's published figures were not reproduced here.
