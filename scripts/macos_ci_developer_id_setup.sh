#!/usr/bin/env bash
set -euo pipefail
# Certificate bytes must be private from creation, including failed imports.
umask 077

fail() {
  printf 'macOS signing setup failed: %s\n' "$*" >&2
  exit 1
}

[ "$(uname -s)" = Darwin ] || fail "this helper is macOS-only"
: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"
: "${APPLE_CERTIFICATE:?APPLE_CERTIFICATE is required}"
: "${APPLE_CERTIFICATE_PASSWORD:?APPLE_CERTIFICATE_PASSWORD is required}"
: "${APPLE_SIGNING_IDENTITY:?APPLE_SIGNING_IDENTITY is required}"

case "$APPLE_SIGNING_IDENTITY" in *$'\n'*|*$'\r'*|"-") fail "invalid identity name" ;; esac
mode="${WEBCODEX_MACOS_SIGNING_MODE:-developer-id}"
case "$mode" in
  developer-id)
    case "$APPLE_SIGNING_IDENTITY" in
      "Developer ID Application:"*) ;;
      *) fail "APPLE_SIGNING_IDENTITY must name a Developer ID Application identity" ;;
    esac ;;
  self-signed)
    : "${WEBCODEX_MACOS_CERTIFICATE_SHA1:?persistent certificate SHA1 is required}"
    [[ "$WEBCODEX_MACOS_CERTIFICATE_SHA1" != *[!A-Fa-f0-9]* ]] && [ "${#WEBCODEX_MACOS_CERTIFICATE_SHA1}" -eq 40 ] || fail "invalid certificate SHA1"
    ;;
  *) fail "CI import requires self-signed or developer-id" ;;
esac

keychain="$RUNNER_TEMP/webcodex-developer-id.keychain-db"
certificate="$RUNNER_TEMP/webcodex-developer-id.p12"
# Check before installing cleanup: never delete another attempt's scratch state.
[ ! -e "$keychain" ] && [ ! -L "$keychain" ] && [ ! -e "$certificate" ] && [ ! -L "$certificate" ] || {
  fail "signing scratch paths already exist"
}
keychain_password="$(openssl rand -hex 32)"
setup_complete=0
keychain_created=0
cleanup() {
  rm -f "$certificate"
  if [ "$setup_complete" -ne 1 ] && [ "$keychain_created" -eq 1 ]; then
    security delete-keychain "$keychain" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

printf '%s' "$APPLE_CERTIFICATE" | openssl base64 -d -A > "$certificate"
chmod 600 "$certificate"

security create-keychain -p "$keychain_password" "$keychain"
keychain_created=1
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

# Explicit self-signed requirements trust the pinned certificate, not the host
# trust store. Enumerate matching identities without requiring Apple/root trust.
valid_only=(-v)
if [ "$mode" = self-signed ]; then valid_only=(); fi
matches="$(
  security find-identity "${valid_only[@]}" -p codesigning "$keychain" |
    grep -F "\"$APPLE_SIGNING_IDENTITY\"" | awk '!seen[$2]++' || true
)"
count="$(printf '%s\n' "$matches" | awk 'NF { n += 1 } END { print n + 0 }')"
[ "$count" -eq 1 ] || fail "expected exactly one configured signing identity, found $count"

hash="$(printf '%s\n' "$matches" | awk 'NF {print $2}')"
if [ "$mode" = self-signed ]; then
  [ "$(printf '%s' "$hash" | tr A-F a-f)" = "$(printf '%s' "$WEBCODEX_MACOS_CERTIFICATE_SHA1" | tr A-F a-f)" ] || fail "imported certificate differs from persistent identity pin"
fi

{
  printf 'APPLE_SIGNING_IDENTITY=%s\n' "$APPLE_SIGNING_IDENTITY"
  printf 'WEBCODEX_SIGNING_KEYCHAIN=%s\n' "$keychain"
} >> "$GITHUB_ENV"

setup_complete=1
printf 'Configured one persistent signing identity in an ephemeral keychain.\n'
