#!/usr/bin/env bash
# Evyro macOS smoke test.
#
#   cd ~/Developer/Evyro && bash scripts/mac-smoke-test.sh
#
# Builds and tests the workspace, then exercises the CLI against a throwaway
# database (EVYRO_HOME=/tmp/evyro-mac-test) and a scratch repository, so your
# real Evyro data is never touched. It never uses API keys and never starts an
# AI agent. It may open ChatGPT in your browser and put text on the clipboard.
# Everything is logged to /tmp/evyro-mac-test.log; share that file.

set -u
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
LOG=/tmp/evyro-mac-test.log
export EVYRO_HOME=/tmp/evyro-mac-test
REPO=/tmp/evyro-mac-repo
EVYRO="$ROOT/target/debug/evyro"
PASS=0
FAIL=0

rm -rf "$EVYRO_HOME" "$REPO"
: >"$LOG"
log() { printf '%s\n' "$*" | tee -a "$LOG"; }
step() {
  local name="$1"
  shift
  log ""
  log "=== $name"
  log "\$ $*"
  if "$@" >>"$LOG" 2>&1; then
    log "PASS  $name"
    PASS=$((PASS + 1))
  else
    log "FAIL  $name (exit $?)"
    FAIL=$((FAIL + 1))
  fi
}
# Steps whose non-zero exit is the expected answer (a refusal).
expect_fail() {
  local name="$1"
  shift
  log ""
  log "=== $name (expected to be refused)"
  log "\$ $*"
  if "$@" >>"$LOG" 2>&1; then
    log "FAIL  $name: it was not refused"
    FAIL=$((FAIL + 1))
  else
    log "PASS  $name"
    PASS=$((PASS + 1))
  fi
}
ev() { (cd "$REPO" && "$EVYRO" "$@"); }

log "Evyro macOS smoke test, $(date)"
log "$(sw_vers 2>/dev/null | tr '\n' ' ') $(uname -m)"
log "$(rustc --version 2>&1) | $(cargo --version 2>&1) | $(git --version 2>&1)"

cd "$ROOT" || exit 1
step "build workspace" cargo build --workspace --all-targets
step "build desktop app" cargo build -p evyro-desktop
step "tests" cargo test --workspace --exclude evyro-desktop
step "clippy" cargo clippy --workspace --all-targets -- -D warnings
step "format" cargo fmt --all --check

if [ ! -x "$EVYRO" ]; then
  log "evyro binary missing; stopping."
  exit 1
fi

mkdir -p "$REPO"
(cd "$REPO" && git init -q && git config user.email t@example.com && git config user.name test &&
  printf '#!/bin/sh\nexit 0\n' >test.sh && chmod +x test.sh && echo hi >README.md &&
  git add -A && git commit -qm init)

step "init" ev init
step "doctor" ev doctor
step "agents" ev agents
step "targets" ev targets
step "providers" ev targets providers
for t in claude-code codex gemini cursor-ide vscode chatgpt claude-app; do
  log ""
  log "=== targets check $t (status shown, any state is fine)"
  ev targets check "$t" >>"$LOG" 2>&1
done

step "task add" ev task add "Demo" --accept "works" --ready
step "task start" ev task start T-1 --agent claude-code
step "checkpoint" ev checkpoint --done "first part" --next "second part"
step "verify" ev verify -- ./test.sh
step "check criterion" ev task check T-1 1
step "mark verified" ev task update T-1 --status verified

step "add api target (env var)" ev targets add or-key --kind api --provider openrouter --model anthropic/claude-sonnet-4.5 --key-env OPENROUTER_API_KEY
step "add api target (Keychain)" ev targets add kc --kind api --provider openai --model gpt-5 --key-keychain evyro-test-nonexistent
log ""
log "=== targets check kc (Keychain item does not exist: expect 'missing', and no password prompt)"
ev targets check kc >>"$LOG" 2>&1
expect_fail "pasted key is refused" ev targets add bad --kind api --provider openai --key-env sk-abcdefghijklmnopqrstuvwxyz123456
step "add subscription target" ev targets add claude-sub --kind harness --harness claude-code --auth app_login

step "task 2" ev task add "Chat pack demo" --accept "pack built" --ready
step "handoff to chatgpt" ev handoff --to chatgpt --task T-2
step "chat pack without opening" ev run --to chatgpt --no-open --accept-drift
step "chat pack file exists" test -s "$REPO/.evyro/CHAT_PACK.md"

step "task 3" ev task add "Clipboard demo" --ready
log ""
log "=== real hand-over to ChatGPT (copies to clipboard, opens the browser)"
ev run --to chatgpt --task T-3 >>"$LOG" 2>&1
log "clipboard starts with: $(pbpaste 2>/dev/null | head -c 80 | tr '\n' ' ')"

for ide in cursor-ide vscode; do
  if ev targets check "$ide" 2>/dev/null | grep -q "state: ready"; then
    step "task for $ide" ev task add "IDE demo $ide" --ready
    key=$(ev --json tasks 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin)[-1]["key"])' 2>/dev/null)
    log ""
    log "=== open $ide on $key"
    ev run --to "$ide" --task "$key" >>"$LOG" 2>&1
    break
  fi
done

step "sessions" ev sessions
step "handoffs" ev handoffs
step "status" ev status

log ""
log "=== desktop app (15 seconds)"
"$ROOT/target/debug/evyro-desktop" >>"$LOG" 2>&1 &
APP=$!
sleep 15
if kill -0 "$APP" 2>/dev/null; then
  log "PASS  desktop app is running"
  PASS=$((PASS + 1))
  screencapture -x /tmp/evyro-desktop.png 2>/dev/null && log "screenshot: /tmp/evyro-desktop.png"
  kill "$APP" 2>/dev/null
else
  log "FAIL  desktop app exited early"
  FAIL=$((FAIL + 1))
fi

log ""
log "Done: $PASS passed, $FAIL failed. Full log: $LOG"
