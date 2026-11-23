#!/usr/bin/env bash
# CI helper: pass the Apple signing secrets on to later steps, or build
# unsigned when they are not set. Tauri signs and notarizes the app when
# APPLE_CERTIFICATE and the rest are in the environment; an empty value would
# make it fail, so nothing is exported unless every secret is present.
# Secret values stay masked in the logs.
set -euo pipefail
vars=(APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID)
if [ -z "${APPLE_CERTIFICATE:-}" ]; then
  echo "No APPLE_CERTIFICATE secret: this build is not signed (see docs/RELEASING.md)."
  exit 0
fi
missing=()
for v in "${vars[@]}"; do [ -n "${!v:-}" ] || missing+=("$v"); done
if [ ${#missing[@]} -gt 0 ]; then
  echo "APPLE_CERTIFICATE is set but these secrets are missing: ${missing[*]}"
  exit 1
fi
for v in "${vars[@]}"; do
  d="EOF_$(openssl rand -hex 8)"
  { echo "$v<<$d"; echo "${!v}"; echo "$d"; } >> "$GITHUB_ENV"
done
echo "EVYRO_SIGNED=1" >> "$GITHUB_ENV"
echo "Signing as the identity in APPLE_SIGNING_IDENTITY; notarizing with team $APPLE_TEAM_ID."
