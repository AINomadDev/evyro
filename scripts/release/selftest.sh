#!/usr/bin/env bash
# Checks the release scripts without a real release: the version check, the
# release notes, the Homebrew formula and the install script against a local
# mirror, including refusing an archive whose checksum does not match.
#   cargo build -p evyro-cli && bash scripts/release/selftest.sh
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
v=$(awk -F'"' '/^\[workspace.package\]/{f=1;next} /^\[/{f=0} f&&/^version *=/{print $2; exit}' "$root/Cargo.toml" | head -1)
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT

bash "$root/scripts/release/check-version.sh" "v$v"
if bash "$root/scripts/release/check-version.sh" "v9.9.9" >/dev/null; then echo "version check passed a wrong tag"; exit 1; fi
[ -n "$(bash "$root/scripts/release/notes.sh" "$v")" ] || { echo "no release notes for $v"; exit 1; }

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) t=x86_64-unknown-linux-gnu ;; Linux-aarch64) t=aarch64-unknown-linux-gnu ;;
  Darwin-arm64) t=aarch64-apple-darwin ;; Darwin-x86_64) t=x86_64-apple-darwin ;;
  *) echo "selftest: skipping the install check on $(uname -s)"; exit 0 ;;
esac
mkdir -p "$tmp/mirror" "$tmp/evyro-$v-$t"
cp "$root/target/debug/evyro" "$tmp/evyro-$v-$t/"
(cd "$tmp" && tar -czf "mirror/evyro-$v-$t.tar.gz" "evyro-$v-$t")
for x in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
  [ -f "$tmp/mirror/evyro-$v-$x.tar.gz" ] || echo placeholder > "$tmp/mirror/evyro-$v-$x.tar.gz"
done
(cd "$tmp/mirror" && { sha256sum -- * 2>/dev/null || shasum -a 256 -- *; } > SHA256SUMS)

bash "$root/scripts/release/homebrew.sh" "$v" "$tmp/mirror/SHA256SUMS" > "$tmp/evyro.rb"
grep -q "evyro-$v-$t.tar.gz" "$tmp/evyro.rb"
if command -v ruby >/dev/null; then ruby -c "$tmp/evyro.rb" >/dev/null; fi

EVYRO_VERSION="$v" EVYRO_DOWNLOAD_BASE="file://$tmp/mirror" EVYRO_BIN_DIR="$tmp/bin" sh "$root/scripts/install.sh"
"$tmp/bin/evyro" --version | grep -qF "$v"

echo tampered >> "$tmp/mirror/evyro-$v-$t.tar.gz"
if EVYRO_DOWNLOAD_BASE="file://$tmp/mirror" EVYRO_BIN_DIR="$tmp/bin2" sh "$root/scripts/install.sh" 2>/dev/null; then
  echo "install.sh installed an archive with a wrong checksum"; exit 1
fi
[ ! -e "$tmp/bin2/evyro" ] || { echo "install.sh left a binary behind after a checksum failure"; exit 1; }
echo "release scripts: ok"
