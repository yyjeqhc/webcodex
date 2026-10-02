#!/usr/bin/env bash
# Sign one code object with an existing, pinned self-signed certificate.
set -euo pipefail
[ "$#" -eq 3 ] || { echo "usage: $0 <code> <identifier> <identity>" >&2; exit 2; }
code="$1"
identifier="$2"
identity="$3"
[[ "$identifier" != *[!A-Za-z0-9._-]* ]] && [ -n "$identifier" ]
matches="$(security find-identity -p codesigning | awk -v name="\"$identity\"" -v hash="$identity" 'index($0, name) || $2 == hash { if (!seen[$2]++) print $2 }')"
[ "$(printf '%s\n' "$matches" | awk 'NF {n++} END {print n+0}')" -eq 1 ] || { echo 'Expected one existing self-signed identity' >&2; exit 1; }
hash="$matches"
[[ "$hash" != *[!A-Fa-f0-9]* ]] && [ "${#hash}" -eq 40 ]
# CI pins this independently of the P12, so replacing the secret certificate
# cannot silently create a new release/TCC identity.
if [ -n "${WEBCODEX_MACOS_CERTIFICATE_SHA1:-}" ]; then
  [ "$(printf '%s' "$hash" | tr A-F a-f)" = "$(printf '%s' "$WEBCODEX_MACOS_CERTIFICATE_SHA1" | tr A-F a-f)" ]
fi
requirement="anchor H\"$hash\" and identifier \"$identifier\""
codesign --force --sign "$hash" --identifier "$identifier" \
  --requirements "=designated => $requirement" --timestamp=none "$code"
codesign --verify --strict -R="$requirement" "$code"
actual="$(codesign -d -r- "$code" 2>&1)"
lower="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"
hash_lower="$(printf '%s' "$hash" | tr A-F a-f)"
case "$lower" in *cdhash*) echo 'Self-signed requirement is cdhash-bound' >&2; exit 1 ;; esac
case "$lower" in
  *"certificate root = h\"$hash_lower\""*|*"anchor h\"$hash_lower\""*) ;;
  *) echo 'Self-signed requirement lost its certificate anchor' >&2; exit 1 ;;
esac
case "$actual" in *"identifier \"$identifier\""*) ;; *) echo 'Self-signed requirement lost its identifier' >&2; exit 1 ;; esac
