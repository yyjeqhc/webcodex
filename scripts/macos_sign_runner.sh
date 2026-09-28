#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'macOS Runner signing failed: %s\n' "$*" >&2
  exit 1
}

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <webcodex-runner> <code-signing-identity|->" >&2
  exit 2
fi
[ "$(uname -s)" = Darwin ] || fail "this helper is macOS-only"

runner="$1"
identity="$2"
[ -f "$runner" ] && [ ! -L "$runner" ] && [ -x "$runner" ] || {
  fail "Runner signing input is not an executable regular file: $runner"
}
[ -n "$identity" ] || fail "Runner signing identity is empty"

tmp_plist="$(mktemp "${TMPDIR:-/tmp}/webcodex-runner-info.XXXXXX")"
cleanup() { rm -f "$tmp_plist"; }
trap cleanup EXIT INT TERM

/usr/bin/otool -X -s __TEXT __info_plist "$runner" |
  python3 -c '
import sys
out = bytearray()
for line in sys.stdin:
    fields = line.split()
    for word in fields[1:]:
        out.extend(bytes.fromhex(word)[::-1])
end = out.find(b"</plist>")
if end < 0:
    raise SystemExit("Runner __info_plist has no plist terminator")
sys.stdout.buffer.write(out[: end + len(b"</plist>")])
' > "$tmp_plist"

identifier="$(plutil -extract CFBundleIdentifier raw -o - "$tmp_plist" 2>/dev/null || true)"
[ "$identifier" = "dev.webcodex.runner" ] || {
  fail "Runner embedded identifier is not dev.webcodex.runner"
}
for key in NSScreenCaptureUsageDescription NSAccessibilityUsageDescription; do
  value="$(plutil -extract "$key" raw -o - "$tmp_plist" 2>/dev/null || true)"
  [ -n "$value" ] || fail "Runner embedded Info.plist is missing $key"
done

if [ "$identity" = "-" ]; then
  codesign --force --sign - --identifier dev.webcodex.runner "$runner"
else
  codesign --force --options runtime --timestamp --sign "$identity" --identifier dev.webcodex.runner "$runner"
fi

codesign --verify --strict --verbose=2 "$runner"
details="$(codesign -d --verbose=4 "$runner" 2>&1)"
printf '%s\n' "$details" | grep -Fxq 'Identifier=dev.webcodex.runner' || {
  fail "signed Runner did not retain dev.webcodex.runner"
}
requirement="$(codesign -d -r- "$runner" 2>&1)"

if [ "$identity" != "-" ]; then
  case "$requirement" in
    *'identifier "dev.webcodex.runner"'*) ;;
    *) fail "formal Runner designated requirement lost its stable identifier" ;;
  esac
  printf '%s\n' "$details" | grep -q '^Authority=Developer ID Application:' || {
    fail "formal Runner is not signed by a Developer ID Application identity"
  }
  team="$(printf '%s\n' "$details" | sed -n 's/^TeamIdentifier=//p' | head -n 1)"
  [ -n "$team" ] && [ "$team" != "not set" ] || fail "formal Runner has no Developer ID team"
  case "$requirement" in
    *cdhash*) fail "formal Runner designated requirement is still cdhash-bound" ;;
  esac
  case "$requirement" in
    *"anchor apple generic"*) ;;
    *) fail "formal Runner designated requirement is not Apple-anchored" ;;
  esac
fi
