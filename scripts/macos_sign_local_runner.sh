#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE="${1:-$ROOT/target/dogfood/webcodex-runner}"
DEST="${2:-${WEBCODEX_MACOS_LOCAL_RUNNER_PATH:-$HOME/.local/lib/webcodex-dev/webcodex-runner}}"
IDENTITY_NAME="${WEBCODEX_MACOS_LOCAL_SIGNING_IDENTITY:-WebCodex Local Development}"
IDENTIFIER="${WEBCODEX_MACOS_LOCAL_RUNNER_IDENTIFIER:-dev.webcodex.runner.local}"

fail() {
    printf 'macOS local Runner signing failed: %s\n' "$*" >&2
    exit 1
}

if [ "$(uname -s)" != "Darwin" ]; then
    fail "this helper is macOS-only"
fi
if [ ! -f "$SOURCE" ] || [ ! -x "$SOURCE" ]; then
    fail "missing executable Runner binary: $SOURCE"
fi
if [ -z "$IDENTIFIER" ] || [[ "$IDENTIFIER" == *[!A-Za-z0-9._-]* ]]; then
    fail "WEBCODEX_MACOS_LOCAL_RUNNER_IDENTIFIER must contain only letters, digits, '.', '_' or '-'"
fi

identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
matches="$(printf '%s\n' "$identities" | awk -v needle="\"$IDENTITY_NAME\"" 'index($0, needle) { print $2 }')"
match_count="$(printf '%s\n' "$matches" | awk 'NF { count += 1 } END { print count + 0 }')"
if [ "$match_count" -ne 1 ]; then
    cat >&2 <<EOF
A single valid code-signing identity named "$IDENTITY_NAME" is required.
Create it once in Keychain Access:
  Certificate Assistant -> Create a Certificate
  Name: $IDENTITY_NAME
  Identity Type: Self Signed Root
  Certificate Type: Code Signing
  Enable "Let me override defaults", choose a unique serial number,
  fill the requested certificate information, then accept the remaining defaults.
Keep this certificate and its private key; recreating it creates a new code identity.
EOF
    fail "found $match_count matching valid code-signing identities"
fi
identity_hash="$(printf '%s\n' "$matches" | awk 'NF { print; exit }')"
if [ "${#identity_hash}" -ne 40 ] || [[ "$identity_hash" == *[!A-Fa-f0-9]* ]]; then
    fail "unexpected code-signing identity hash"
fi

mkdir -p "$(dirname "$DEST")"
tmp="$(mktemp "$(dirname "$DEST")/.webcodex-runner.sign.XXXXXX")"
cleanup() { rm -f "$tmp"; }
trap cleanup EXIT
install -m 0755 "$SOURCE" "$tmp"

requirement="designated => anchor H\"$identity_hash\" and identifier \"$IDENTIFIER\""
codesign --force \
    --sign "$identity_hash" \
    --identifier "$IDENTIFIER" \
    --requirements "=$requirement" \
    --timestamp=none \
    "$tmp"
codesign --verify --strict --verbose=2 "$tmp"
actual_requirement="$(codesign -d -r- "$tmp" 2>&1)"
actual_requirement_lower="$(printf '%s' "$actual_requirement" | tr '[:upper:]' '[:lower:]')"
identity_hash_lower="$(printf '%s' "$identity_hash" | tr '[:upper:]' '[:lower:]')"
case "$actual_requirement" in
    *"identifier \"$IDENTIFIER\""*) ;;
    *) fail "signed Runner does not carry the expected identifier" ;;
esac
case "$actual_requirement_lower" in
    *"certificate root = h\"$identity_hash_lower\""*|*"anchor h\"$identity_hash_lower\""*) ;;
    *) fail "signed Runner designated requirement is not anchored to the selected local identity" ;;
esac
case "$actual_requirement_lower" in
    *cdhash*) fail "designated requirement is still bound to one binary cdhash" ;;
esac

mv -f "$tmp" "$DEST"
trap - EXIT
codesign --verify --strict --verbose=2 "$DEST"
printf 'Installed signed local Runner:\n  %s\n' "$DEST"
printf 'Signing identity: %s\n' "$IDENTITY_NAME"
printf 'Designated requirement: %s\n' "$requirement"
