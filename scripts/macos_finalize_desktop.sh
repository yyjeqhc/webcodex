#!/usr/bin/env bash
# Restore final nested Runner identity after Tauri, then seal the outer app.
set -euo pipefail
[ "$#" -eq 3 ] || { echo "usage: $0 <app> <mode> <identity>" >&2; exit 2; }
app="$1"; mode="$2"; identity="$3"
scripts="$(cd "$(dirname "$0")" && pwd)"
if [ "$mode" != adhoc ]; then
  bash "$scripts/macos_sign_runner.sh" "$app/Contents/Resources/webcodex-runtime/webcodex-runner" "$identity" "$mode"
  if [ "$mode" = self-signed ]; then
    bash "$scripts/macos_sign_self_signed.sh" "$app" dev.webcodex.desktop "$identity"
  else
    codesign --force --options runtime --timestamp --sign "$identity" --identifier dev.webcodex.desktop "$app"
  fi
fi
bash "$scripts/verify_macos_desktop_identity.sh" "$app" "$mode"
if [ "$mode" = developer-id ]; then
  : "${APPLE_ID:?}" "${APPLE_PASSWORD:?}" "${APPLE_TEAM_ID:?}"
  scratch="$(mktemp -d "${TMPDIR:-/tmp}/webcodex-notarize.XXXXXX")"
  trap 'rm -rf "$scratch"' EXIT
  ditto -c -k --keepParent "$app" "$scratch/app.zip"
  xcrun notarytool submit "$scratch/app.zip" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" --wait
  xcrun stapler staple "$app"
  xcrun stapler validate "$app"
  spctl --assess --type execute "$app"
fi
