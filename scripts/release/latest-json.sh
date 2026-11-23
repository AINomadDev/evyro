#!/usr/bin/env bash
# Print the latest.json that installed desktop apps read to find an update.
#   scripts/release/latest-json.sh <version> <dist dir> <download base url>
# The dist dir holds the signed update files and their .sig files:
#   Evyro_<v>_universal.app.tar.gz  macOS (Apple silicon and Intel)
#   *-setup.exe                     Windows
#   *.AppImage                      Linux
# A platform whose file or signature is missing is left out, so an app on it
# is never offered an update it cannot verify.
set -euo pipefail
v="${1:?usage: latest-json.sh <version> <dist dir> <base url>}"
dist="${2:?}"; base="${3:?}"; base="${base%/}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
command -v jq >/dev/null || { echo "latest-json.sh needs jq" >&2; exit 1; }

platforms='{}'
add() { # add <platform> <file name>
  local f="$dist/$2"
  [ -f "$f" ] && [ -f "$f.sig" ] || return 0
  platforms=$(jq --arg p "$1" --arg url "$base/$2" --rawfile sig "$f.sig" \
    '. + {($p): {signature: ($sig | rtrimstr("\n")), url: $url}}' <<<"$platforms")
}
first() { (cd "$dist" && ls -1 -- $1 2>/dev/null | grep -v '\.sig$' | head -1) || true; }

mac="Evyro_${v}_universal.app.tar.gz"
add darwin-aarch64 "$mac"
add darwin-x86_64 "$mac"
win=$(first '*-setup.exe'); [ -z "$win" ] || add windows-x86_64 "$win"
lin=$(first '*.AppImage'); [ -z "$lin" ] || add linux-x86_64 "$lin"

[ "$platforms" != '{}' ] || { echo "latest-json.sh: no signed update files in $dist" >&2; exit 1; }
jq -n --arg v "$v" --arg notes "$(bash "$root/scripts/release/notes.sh" "$v")" \
  --arg date "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --argjson platforms "$platforms" \
  '{version: $v, notes: $notes, pub_date: $date, platforms: $platforms}'
