#!/usr/bin/env bash
# Fail unless the tag (v1.2.3 or v1.2.3-rc.1), the workspace version in
# Cargo.toml and the desktop app's version in tauri.conf.json all agree, and
# CHANGELOG.md has a section for it, and the README's Download links name it.
#   scripts/release/check-version.sh v0.2.0
set -euo pipefail
tag="${1:?usage: check-version.sh <tag>}"
want="${tag#v}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
cargo=$(awk -F'"' '/^\[workspace.package\]/{f=1;next} /^\[/{f=0} f&&/^version *=/{print $2; exit}' "$root/Cargo.toml" | head -1)
tauri=$(sed -n 's/^ *"version": *"\(.*\)",*$/\1/p' "$root/crates/evyro-desktop/tauri.conf.json" | head -1)
ok=1
[[ "$want" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]] || { echo "tag $tag is not v<major>.<minor>.<patch>[-pre]"; ok=0; }
[ "$cargo" = "$want" ] || { echo "Cargo.toml says $cargo, tag says $want"; ok=0; }
[ "$tauri" = "$want" ] || { echo "tauri.conf.json says $tauri, tag says $want"; ok=0; }
grep -q "^## \[$want\]" "$root/CHANGELOG.md" || { echo "CHANGELOG.md has no '## [$want]' section"; ok=0; }
stale=$(grep -o 'releases/download/v[0-9][^/]*/' "$root/README.md" | grep -vF "releases/download/v$want/" | sort -u || true)
grep -qF "releases/download/v$want/" "$root/README.md" && [ -z "$stale" ] \
  || { echo "README.md Download links don't all name v$want (see docs/RELEASING.md)"; ok=0; }
[ $ok = 1 ] && echo "version $want: tag, Cargo.toml, tauri.conf.json, CHANGELOG.md and README.md agree"
[ $ok = 1 ]
