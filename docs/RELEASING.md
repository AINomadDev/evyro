# Releasing Evyro

A version tag publishes a release. CI builds and checks everything, then creates the GitHub Release. Nobody uploads files by hand.

## Cut a release

1. Set the version in two places: `version` under `[workspace.package]` in `Cargo.toml`, and `version` in `crates/evyro-desktop/tauri.conf.json`. Run `cargo check` so `Cargo.lock` follows.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and start a new empty `## [Unreleased]` above it.
3. Run `bash scripts/release/check-version.sh vX.Y.Z`, then open a PR with these changes and merge it once CI is green.
4. Tag the merge commit on `main` and push the tag:

   ```bash
   git tag vX.Y.Z && git push origin vX.Y.Z
   ```

   A tag with a pre-release part, such as `v0.2.0-rc.1`, is published as a pre-release and leaves the Homebrew tap alone.

## What the tag runs

| Job | What it does |
|---|---|
| lint, audit, test, cli-e2e, ui-smoke | The usual checks, on Linux, macOS and Windows |
| bundle | Builds the desktop installers (universal `.dmg`, NSIS `-setup.exe`, `.deb`, `.AppImage`), installs each one on a clean runner and checks the app stays running, makes an SBOM, and attests build provenance. On macOS it signs and notarizes when the Apple secrets are set |
| release-build | Checks the tag against `Cargo.toml`, `tauri.conf.json` and `CHANGELOG.md`, builds the CLI for 5 targets (macOS arm64 and x86_64, Linux x86_64 and arm64, Windows x86_64), signs and notarizes the macOS binaries when the secrets are set, archives each with the licenses, checks the archive runs, and attests provenance |
| publish | Gathers everything, writes one `SHA256SUMS`, generates the Homebrew formula, and creates the GitHub Release with the notes from `CHANGELOG.md` |

## macOS signing and notarization

Signing switches on when the repository has these Actions secrets (**Settings › Secrets and variables › Actions**). Without them the build is unsigned, and macOS warns on first open.

| Secret | Where it comes from |
|---|---|
| `APPLE_CERTIFICATE` | Your **Developer ID Application** certificate exported from Keychain Access as a `.p12`, then base64: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The password you chose when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate's name, e.g. `Developer ID Application: Pratik Thakare (TEAMID1234)` (`security find-identity -v -p codesigning`) |
| `APPLE_ID` | The Apple ID email of the developer account |
| `APPLE_PASSWORD` | An app-specific password from [account.apple.com](https://account.apple.com) › Sign-In and Security › App-Specific Passwords (not your Apple ID password) |
| `APPLE_TEAM_ID` | The 10-character Team ID from [developer.apple.com/account](https://developer.apple.com/account) › Membership |

To get the certificate: enroll in the [Apple Developer Program](https://developer.apple.com/programs/), then in Xcode › Settings › Accounts › Manage Certificates, add a **Developer ID Application** certificate. Export it from Keychain Access (My Certificates, right-click › Export) as `.p12`.

You can set the secrets from a terminal with `gh secret set APPLE_CERTIFICATE --repo AINomadDev/evyro < cert.b64` and so on. Never commit any of these values or paste them into an issue.

When the secrets are set, the bundle job checks the installed app with `codesign --verify`, `spctl --assess` and `xcrun stapler validate`, and fails if any of them do.

## Windows

Windows builds are not signed yet, so SmartScreen warns on first run. Users can check a download against `SHA256SUMS` or with `gh attestation verify`.

## Homebrew

Every release carries `evyro.rb`. To publish it to a tap:

1. Create the repository `AINomadDev/homebrew-tap`.
2. Create a fine-grained token with **Contents: read and write** on that repository only, and save it as the `HOMEBREW_TAP_TOKEN` Actions secret here.

Then each non-pre-release tag updates `Formula/evyro.rb` in the tap, and people install with `brew install AINomadDev/tap/evyro`.

## Checking a release

```bash
sha256sum -c SHA256SUMS --ignore-missing          # the files you downloaded
gh attestation verify Evyro_X.Y.Z_universal.dmg --repo AINomadDev/evyro
```

`bash scripts/release/selftest.sh` checks the release scripts locally (it runs in CI on Linux and macOS).
