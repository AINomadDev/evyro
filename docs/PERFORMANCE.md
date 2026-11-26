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


## Budgets checked in CI

The `perf` job in `.github/workflows/ci.yml` builds the release binary on Linux and runs `scripts/perf-check.py`. The script generates a throwaway Git repository of 5,000 small Rust files in 100 folders (one commit) and an Evyro store with 2,000 tasks, in a temporary folder with its own `EVYRO_HOME`; nothing is committed. Each command below runs once as a warm-up, then 11 times as a fresh process; the job fails when the **median** is over the budget. The run's timings appear in the job summary.

The budgets are deliberately generous, roughly 5 to 10 times what a laptop measures, so a slow shared runner does not fail the job; they catch a clear regression (an accidental full scan, a quadratic loop, a slow start-up path), not a few percent. `search text` and `search symbols` scan every file, so they have the largest budgets.

<!-- perf-budgets:start -->
| Command | Budget (ms) |
|---|---:|
| `evyro version` | 100 |
| `evyro tasks` | 250 |
| `evyro tasks --all` | 250 |
| `evyro status` | 500 |
| `evyro context` | 750 |
| `evyro search files file_4999` | 300 |
| `evyro search text perf_needle` | 1000 |
| `evyro search symbols handler_4999_7` | 1500 |
<!-- perf-budgets:end -->

Reference run (2026-11-26, Apple Silicon Mac, release build): `version` 4 ms, `tasks` 10 ms, `status` 48 ms, `context` 78 ms, `search files` 19 ms, `search text` 92 ms, `search symbols` 152 ms (medians).

### Running it locally

```sh
cargo build --release -p evyro-cli
python3 scripts/perf-check.py            # or: python3 scripts/perf-check.py path/to/evyro
```

`PERF_FILES`, `PERF_TASKS` and `PERF_RUNS` change the size of the generated repository and the number of runs (defaults 5000, 2000 and 11). Keep the defaults when comparing against the budgets: the search queries name file 4999.

### Changing a budget

Edit the table between the `perf-budgets` markers above: one row per command, the command in backticks starting with `evyro`, and the budget in milliseconds. A new row is a new check; no script change is needed. Before raising a budget because the job failed, re-run the job once (a shared runner can be slow) and run the script locally on `main` and on the branch: raise it only when the slower time is expected, and say why in the pull request.
