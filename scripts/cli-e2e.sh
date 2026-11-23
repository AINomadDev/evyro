#!/usr/bin/env bash
# End-to-end CLI flow on any OS (Linux, macOS, Windows Git Bash), no agents.
#   bash scripts/cli-e2e.sh [path-to-evyro]
# Uses a scratch EVYRO_HOME and repository; exits non-zero on the first failure.
set -euo pipefail
EVYRO="${1:-$(cd "$(dirname "$0")/.." && pwd)/target/debug/evyro}"
[ -x "$EVYRO" ] || EVYRO="$EVYRO.exe"
TMP="$(mktemp -d)"
export EVYRO_HOME="$TMP/home" EVYRO_NO_APP_LOOKUP=1
REPO="$TMP/repo"
mkdir -p "$REPO" && cd "$REPO"
git init -q -b main && git config user.email ci@example.com && git config user.name CI
printf 'one\n' > app.txt && printf '#!/bin/sh\nexit 0\n' > test.sh && chmod +x test.sh
git add -A && git commit -qm init

ok() { echo "PASS  $*"; }
step() { local n="$1"; shift; if "$@" >/dev/null; then ok "$n"; else echo "FAIL  $n"; exit 1; fi; }
step "init" "$EVYRO" init
step "doctor" "$EVYRO" doctor
step "targets" "$EVYRO" targets
step "task add" "$EVYRO" task add "Demo" --accept "works" --ready
step "task start" "$EVYRO" task start T-1 --agent claude-code
printf 'two\n' >> app.txt
step "checkpoint" "$EVYRO" checkpoint --done "first" --next "second"
step "verify" "$EVYRO" verify -- sh ./test.sh
step "check criterion" "$EVYRO" task check T-1 1
step "verified" "$EVYRO" task update T-1 --status verified
"$EVYRO" task add "Second" --accept "pack built" --ready >/dev/null
"$EVYRO" task start T-2 --agent codex >/dev/null
step "handoff" "$EVYRO" handoff --to chatgpt --task T-2 --next "review"
step "chat hand-over" "$EVYRO" run --to chatgpt --no-open --accept-drift
step "chat pack written" test -s .evyro/CHAT_PACK.md
step "add api target" "$EVYRO" targets add or-key --kind api --provider openrouter --model x/y --key-env OPENROUTER_API_KEY
if "$EVYRO" targets add bad --kind api --provider openai --key-env sk-abcdefghijklmnopqrstuvwxyz123456 >/dev/null 2>&1; then
  echo "FAIL  pasted key was accepted"; exit 1
fi
ok "pasted key refused"
if "$EVYRO" --json handoffs | grep -q '"accepted"'; then ok "handoff accepted"; else echo "FAIL  handoff accepted"; exit 1; fi
step "export" "$EVYRO" export --out "$TMP/export.json"
step "backup" "$EVYRO" backup "$TMP/backup.db"
echo "CLI end-to-end passed."
