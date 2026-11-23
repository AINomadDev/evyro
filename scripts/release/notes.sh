#!/usr/bin/env bash
# Print the CHANGELOG.md section for a version, for the release notes.
#   scripts/release/notes.sh 0.2.0
set -euo pipefail
v="${1:?usage: notes.sh <version>}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
awk -v v="$v" '
  $0 ~ "^## \\[" v "\\]" { on = 1; next }
  on && /^## \[/ { exit }
  on { print }
' "$root/CHANGELOG.md"
