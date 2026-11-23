#!/bin/sh
# Self-hosting: register this repository in Evyro and load the next backlog items as tasks.
set -e
cd "$(dirname "$0")/.."
evyro init --no-agent-files -q
add() { evyro task add "$1" --kind "$2" --priority "$3" --accept "$4" --json | sed -n 's/.*"key": "\(T-[0-9]*\)".*/\1/p' | head -1; }
A=$(add "Validate Claude -> Codex -> Gemini with real agents on macOS" integration 0 "real run recorded with agent versions")
B=$(add "Run CI on macOS and Windows" release 0 "ci.yml green on all three OSes")
C=$(add "Desktop MVP (Tauri 2) dashboard over evyro-core" feature 1 "dashboard shows real projects, tasks, sessions")
D=$(add "Launch px0 review on a task worktree" integration 2 "evyro review T-n opens px0 on the worktree")
E=$(add "Codex and Gemini automatic checkpoint hooks" integration 2 "checkpoints created without manual commands")
F=$(add "Fuzzy file search, regex search, symbol outline" feature 2 "search returns results under 50 ms on a 10k-file repo")
G=$(add "Handoff benchmark suite with real agents" spike 2 "four strategies measured on 3 tasks")
H=$(add "Release binaries, checksums, Homebrew tap" release 2 "brew install works on Apple Silicon")
evyro task dep "$C" --on "$B" -q
evyro task dep "$D" --on "$C" -q
evyro task dep "$G" --on "$A" -q
evyro task dep "$H" --on "$B" -q
evyro decide "Strategy E" --decision "Independent Rust core; integrate T3 Code and px0 instead of forking" --why "see docs/research/FORK_VS_BUILD.md" -q
evyro tasks --ready
