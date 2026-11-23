#!/usr/bin/env bash
# Write a Homebrew formula for the evyro CLI from a release's SHA256SUMS.
#   scripts/release/homebrew.sh 0.2.0 dist/SHA256SUMS > evyro.rb
set -euo pipefail
v="${1:?usage: homebrew.sh <version> <SHA256SUMS>}"
sums="${2:?usage: homebrew.sh <version> <SHA256SUMS>}"
sha() {
  local s
  s=$(grep -E " \*?evyro-$v-$1\.tar\.gz\$" "$sums" | cut -d' ' -f1)
  [ -n "$s" ] || { echo "no archive for $1 in $sums" >&2; exit 1; }
  echo "$s"
}
url="https://github.com/AINomadDev/evyro/releases/download/v$v"
cat <<RB
class Evyro < Formula
  desc "Keep AI coding work continuous across agents"
  homepage "https://github.com/AINomadDev/evyro"
  version "$v"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "$url/evyro-$v-aarch64-apple-darwin.tar.gz"
      sha256 "$(sha aarch64-apple-darwin)"
    end
    on_intel do
      url "$url/evyro-$v-x86_64-apple-darwin.tar.gz"
      sha256 "$(sha x86_64-apple-darwin)"
    end
  end

  on_linux do
    on_arm do
      url "$url/evyro-$v-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "$(sha aarch64-unknown-linux-gnu)"
    end
    on_intel do
      url "$url/evyro-$v-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$(sha x86_64-unknown-linux-gnu)"
    end
  end

  def install
    bin.install "evyro"
    generate_completions_from_executable(bin/"evyro", "completions")
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/evyro --version")
  end
end
RB
