#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'macOS Desktop identity verification failed: %s\n' "$*" >&2
  exit 1
}

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <WebCodex Desktop.app> <adhoc|developer-id>" >&2
  exit 2
fi
[ "$(uname -s)" = Darwin ] || fail "this helper is macOS-only"

app="$1"
signing_mode="$2"
case "$signing_mode" in
  adhoc|developer-id) ;;
  *) fail "invalid signing mode: $signing_mode" ;;
esac

[ -d "$app" ] || fail "Desktop app is missing: $app"
plist="$app/Contents/Info.plist"
runner="$app/Contents/Resources/webcodex-runtime/webcodex-runner"
[ -f "$plist" ] || fail "Desktop Info.plist is missing"
[ -x "$runner" ] || fail "bundled WebCodex Runner is missing"

for key in NSScreenCaptureUsageDescription NSAccessibilityUsageDescription; do
  value="$(plutil -extract "$key" raw -o - "$plist" 2>/dev/null || true)"
  [ -n "$value" ] || fail "Desktop Info.plist is missing $key"
done

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
[ "$(plutil -extract CFBundleIdentifier raw -o - "$tmp_plist" 2>/dev/null || true)" = "dev.webcodex.runner" ] || {
  fail "bundled Runner embedded identifier is not dev.webcodex.runner"
}
for key in NSScreenCaptureUsageDescription NSAccessibilityUsageDescription; do
  value="$(plutil -extract "$key" raw -o - "$tmp_plist" 2>/dev/null || true)"
  [ -n "$value" ] || fail "bundled Runner embedded Info.plist is missing $key"
done

codesign --verify --strict --verbose=2 "$runner"
runner_details="$(codesign -d --verbose=4 "$runner" 2>&1)"
printf '%s\n' "$runner_details" | grep -Fxq 'Identifier=dev.webcodex.runner' || {
  fail "bundled Runner code identity is not dev.webcodex.runner"
}
runner_requirement="$(codesign -d -r- "$runner" 2>&1)"

codesign --verify --deep --strict --verbose=2 "$app"

if [ "$signing_mode" = developer-id ]; then
  case "$runner_requirement" in
    *'identifier "dev.webcodex.runner"'*) ;;
    *) fail "Developer ID Runner designated requirement lost its stable identifier" ;;
  esac
  printf '%s\n' "$runner_details" | grep -q '^Authority=Developer ID Application:' || {
    fail "bundled Runner is not Developer ID signed"
  }
  runner_team="$(printf '%s\n' "$runner_details" | sed -n 's/^TeamIdentifier=//p' | head -n 1)"
  [ -n "$runner_team" ] && [ "$runner_team" != "not set" ] || fail "bundled Runner has no Developer ID team"
  case "$runner_requirement" in
    *cdhash*) fail "Developer ID Runner requirement is cdhash-bound" ;;
  esac
  case "$runner_requirement" in
    *"anchor apple generic"*) ;;
    *) fail "Developer ID Runner requirement is not Apple-anchored" ;;
  esac

  app_details="$(codesign -d --verbose=4 "$app" 2>&1)"
  printf '%s\n' "$app_details" | grep -q '^Authority=Developer ID Application:' || {
    fail "Desktop app is not Developer ID signed"
  }
  app_team="$(printf '%s\n' "$app_details" | sed -n 's/^TeamIdentifier=//p' | head -n 1)"
  [ "$app_team" = "$runner_team" ] || fail "Desktop and Runner use different Developer ID teams"
fi
