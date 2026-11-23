#!/bin/sh
# Install the `evyro` CLI from a GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/AINomadDev/evyro/main/scripts/install.sh | sh
#
# Options (environment variables):
#   EVYRO_VERSION   a version such as 0.2.0 (default: the latest release)
#   EVYRO_BIN_DIR   where to put the binary (default: ~/.local/bin)
#   EVYRO_DOWNLOAD_BASE  a mirror holding SHA256SUMS and the archives
#                   (https:// or file://; used by the release tests)
#
# The archive is checked against the release's SHA256SUMS before anything is
# installed. Nothing runs as root and nothing outside EVYRO_BIN_DIR changes.
set -eu

REPO="AINomadDev/evyro"
BIN_DIR="${EVYRO_BIN_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "evyro-install: $*" >&2; }
die() { say "$*"; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "needs '$1', which is not installed"; }

need uname
need tar
if command -v curl >/dev/null 2>&1; then
  fetch() { curl -fsSL --proto '=https' --tlsv1.2 -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  fetch() { wget -q --https-only -O "$2" "$1"; }
else
  die "needs curl or wget"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sha() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "needs sha256sum or shasum"
fi

case "$(uname -s)" in
  Darwin) os="apple-darwin" ;;
  Linux) os="unknown-linux-gnu" ;;
  *) die "unsupported system $(uname -s); on Windows use scripts/install.ps1" ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) die "unsupported processor $(uname -m)" ;;
esac
target="$arch-$os"

if [ -n "${EVYRO_VERSION:-}" ]; then
  version="${EVYRO_VERSION#v}"
  base="https://github.com/$REPO/releases/download/v$version"
else
  base="https://github.com/$REPO/releases/latest/download"
  version="latest"
fi

if [ -n "${EVYRO_DOWNLOAD_BASE:-}" ]; then
  base="${EVYRO_DOWNLOAD_BASE%/}"
fi
case "$base" in
  file://*) fetch() { cp "${1#file://}" "$2"; } ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

# Before the first stable release, releases/latest has nothing in it: fall
# back to the newest release, pre-releases included.
if [ "$version" = "latest" ] && [ -z "${EVYRO_DOWNLOAD_BASE:-}" ] && ! fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" 2>/dev/null; then
  fetch "https://api.github.com/repos/$REPO/releases?per_page=1" "$tmp/releases.json" || die "could not list the releases of $REPO"
  tag="$(grep -m1 '"tag_name"' "$tmp/releases.json" | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')"
  [ -n "$tag" ] || die "$REPO has no releases yet"
  version="${tag#v} (pre-release)"
  base="https://github.com/$REPO/releases/download/$tag"
fi

say "downloading the $version release for $target"
[ -s "$tmp/SHA256SUMS" ] || fetch "$base/SHA256SUMS" "$tmp/SHA256SUMS" || die "could not download SHA256SUMS from $base"
archive="$(grep -E "evyro-[^ ]*-$target\.tar\.gz\$" "$tmp/SHA256SUMS" | awk '{print $2}' | sed 's|^\*||' | head -1)"
[ -n "$archive" ] || die "this release has no CLI for $target"
fetch "$base/$archive" "$tmp/$archive" || die "could not download $archive"

want="$(grep -E " \*?$archive\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)"
got="$(sha "$tmp/$archive")"
[ "$want" = "$got" ] || die "checksum mismatch for $archive (expected $want, got $got); nothing was installed"
say "checksum ok"

tar -xzf "$tmp/$archive" -C "$tmp"
src="$(find "$tmp" -type f -name evyro | head -1)"
[ -n "$src" ] || die "the archive has no evyro binary"
mkdir -p "$BIN_DIR"
install -m 0755 "$src" "$BIN_DIR/evyro" 2>/dev/null || { cp "$src" "$BIN_DIR/evyro" && chmod 0755 "$BIN_DIR/evyro"; }
say "installed $("$BIN_DIR/evyro" --version 2>/dev/null || echo evyro) to $BIN_DIR/evyro"

case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) say "add $BIN_DIR to your PATH, for example: echo 'export PATH=\"$BIN_DIR:\$PATH\"' >> ~/.zshrc" ;;
esac
