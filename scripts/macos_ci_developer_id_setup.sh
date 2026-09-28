#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'macOS Developer ID setup failed: %s\n' "$*" >&2
  exit 1
}

[ "$(uname -s)" = Darwin ] || fail "this helper is macOS-only"
: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"
: "${APPLE_CERTIFICATE:?APPLE_CERTIFICATE is required}"
: "${APPLE_CERTIFICATE_PASSWORD:?APPLE_CERTIFICATE_PASSWORD is required}"
: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is required}"

case "$APPLE_SIGNING_IDENTITY" in
  "Developer ID Application:"*) ;;
  *) fail "APPLE_SIGNING_IDENTITY must name a Developer ID Application identity" ;;
esac

keychain="$RUNNER_TEMP/webcodex-developer-id.keychain-db"
certificate="$RUNNER_TEMP/webcodex-developer-id.p12"
keychain_password="$(openssl rand -hex 32)"

rm -f "$keychain" "$certificate"
printf '%s' "$APPLE_CERTIFICATE" | openssl base64 -d -A > "$certificate"
chmod 600 "$certificate"

security create-keychain -p "$keychain_password" "$keychain"
security set-keychain-settings -lut 7200 "$keychain"
security unlock-keychain -p "$keychain_password" "$keychain"
security import "$certificate" \
  -k "$keychain" \
  -P "$APPLE_CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign \
  -T /usr/bin/security >/dev/null
rm -f "$certificate"
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$keychain_password" \
  "$keychain" >/dev/null
security list-keychains -d user -s "$keychain"
security default-keychain -d user -s "$keychain"

matches="$(
  security find-identity -v -p codesigning "$keychain" |
    grep -F "\"$APPLE_SIGNING_IDENTITY\"" || true
)"
count="$(printf '%s\n' "$matches" | awk 'NF { n += 1 } END { print n + 0 }')"
[ "$count" -eq 1 ] || fail "expected exactly one configured Developer ID Application identity, found $count"

{
  printf 'APPLE_SIGNING_IDENTITY=%s\n' "$APPLE_SIGNING_IDENTITY"
  printf 'WEBCODEX_SIGNING_KEYCHAIN=%s\n' "$keychain"
} >> "$GITHUB_ENV"

printf 'Configured one Developer ID Application signing identity in an ephemeral keychain.\n'
