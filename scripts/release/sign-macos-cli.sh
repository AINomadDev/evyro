#!/usr/bin/env bash
# CI helper: sign the evyro CLI binary with the Developer ID certificate and
# send it to Apple's notary service. Needs the APPLE_* variables that
# apple-env.sh exports. A bare binary cannot be stapled; Gatekeeper looks up
# the notarization online the first time it runs.
#   scripts/release/sign-macos-cli.sh target/aarch64-apple-darwin/release/evyro
set -euo pipefail
bin="${1:?usage: sign-macos-cli.sh <binary>}"
kc="$RUNNER_TEMP/evyro-signing.keychain-db"
kcpass="$(openssl rand -hex 16)"
cert="$RUNNER_TEMP/cert.p12"
trap 'rm -f "$cert"; security delete-keychain "$kc" 2>/dev/null || true' EXIT

echo "$APPLE_CERTIFICATE" | base64 --decode > "$cert"
security create-keychain -p "$kcpass" "$kc"
security set-keychain-settings -lut 21600 "$kc"
security unlock-keychain -p "$kcpass" "$kc"
security import "$cert" -k "$kc" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple: -s -k "$kcpass" "$kc" >/dev/null
security list-keychains -d user -s "$kc" $(security list-keychains -d user | tr -d '"')

codesign --force --timestamp --options runtime --keychain "$kc" --sign "$APPLE_SIGNING_IDENTITY" "$bin"
codesign --verify --strict --verbose=2 "$bin"

zip="$RUNNER_TEMP/evyro-notarize.zip"
ditto -c -k --keepParent "$bin" "$zip"
xcrun notarytool submit "$zip" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
echo "signed and notarized $bin"
