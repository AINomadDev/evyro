# Releasing Evyro

A version tag publishes a release. CI builds and checks everything, then creates the GitHub Release. Nobody uploads files by hand.

## Cut a release

1. Set the version in two places: `version` under `[workspace.package]` in `Cargo.toml`, and `version` in `crates/evyro-desktop/tauri.conf.json`. Run `cargo check` so `Cargo.lock` follows.
2. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD` and start a new empty `## [Unreleased]` above it.
3. Point the README's Download links at the new version: `sed -i.bak 's/OLD_VERSION/X.Y.Z/g' README.md && rm README.md.bak`, with OLD_VERSION being the version the links name now. The version check fails if the README links don't name the tag.
4. Run `bash scripts/release/check-version.sh vX.Y.Z`, then open a PR with these changes and merge it once CI is green.
5. Tag the merge commit on `main` and push the tag:

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
| publish | Gathers everything, writes `latest.json` for the desktop updater (when the update key is set), writes one `SHA256SUMS`, generates the Homebrew formula, and creates the GitHub Release with the notes from `CHANGELOG.md` |

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

## Desktop auto-update

Installed desktop apps check `https://github.com/AINomadDev/evyro/releases/latest/download/latest.json` and offer a newer version. Every update file is signed with Evyro's update key. The app holds the public key and refuses a file whose signature does not match. This key is separate from the Apple certificate.

Set it up once:

1. Make the key on your own computer: `npx @tauri-apps/cli@2.12.6 signer generate -w ~/.tauri/evyro-update.key`. Choose a password. Keep the private key file and the password somewhere safe, such as your password manager. A lost key means installed apps can never update again.
2. Save two Actions secrets: `TAURI_SIGNING_PRIVATE_KEY` (the contents of `~/.tauri/evyro-update.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Put the public key (`~/.tauri/evyro-update.key.pub`) in `crates/evyro-desktop/tauri.conf.json` under `plugins.updater.pubkey`, in place of `UPDATE_PUBLIC_KEY_PENDING`, and merge that change.

With the secret set, the bundle job also builds the signed update files (`.app.tar.gz` on macOS, the `-setup.exe` on Windows and the `.AppImage` on Linux, each with a `.sig`), and publish writes `latest.json` listing them.

A few rules follow from this:

- A build that still has `UPDATE_PUBLIC_KEY_PENDING` never offers an update.
- `releases/latest` skips pre-releases, so a release candidate is never offered to installed apps.
- Linux `.deb` installs don't update themselves; only the AppImage does.

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
