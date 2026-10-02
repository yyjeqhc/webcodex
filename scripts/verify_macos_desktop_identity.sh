#!/usr/bin/env bash
set -euo pipefail

fail() {
  printf 'macOS Desktop identity verification failed: %s\n' "$*" >&2
  exit 1
}

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <WebCodex Desktop.app> <adhoc|self-signed|developer-id>" >&2
  exit 2
fi
[ "$(uname -s)" = Darwin ] || fail "this helper is macOS-only"

app="$1"
signing_mode="$2"
case "$signing_mode" in
  adhoc|self-signed|developer-id) ;;
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

app_details="$(codesign -d --verbose=4 "$app" 2>&1)"
printf '%s\n' "$app_details" | grep -Fxq 'Identifier=dev.webcodex.desktop' || fail "Desktop identifier mismatch"
app_requirement="$(codesign -d -r- "$app" 2>&1)"
if [ "$signing_mode" = self-signed ]; then
  hash="${WEBCODEX_MACOS_CERTIFICATE_SHA1:?self-signed verification requires the persistent certificate SHA1}"
  [[ "$hash" != *[!A-Fa-f0-9]* ]] && [ "${#hash}" -eq 40 ] || fail "invalid certificate SHA1"
  for kind in runner app; do
    if [ "$kind" = runner ]; then
      code="$runner"; identifier=dev.webcodex.runner; requirement="$runner_requirement"
    else
      code="$app"; identifier=dev.webcodex.desktop; requirement="$app_requirement"
    fi
    expected="anchor H\"$hash\" and identifier \"$identifier\""
    codesign --verify --strict -R="$expected" "$code"
    lower="$(printf '%s' "$requirement" | tr '[:upper:]' '[:lower:]')"
    hash_lower="$(printf '%s' "$hash" | tr A-F a-f)"
    case "$lower" in *cdhash*) fail "self-signed requirement is cdhash-bound" ;; esac
    case "$lower" in
      *"certificate root = h\"$hash_lower\""*|*"anchor h\"$hash_lower\""*) ;;
      *) fail "self-signed requirement does not pin the persistent certificate" ;;
    esac
    case "$requirement" in *"identifier \"$identifier\""*) ;; *) fail "missing stable identifier requirement" ;; esac
  done
fi
if [ "$signing_mode" = developer-id ]; then
  case "$app_requirement" in *cdhash*) fail "Desktop requirement is cdhash-bound" ;; esac
  case "$app_requirement" in *'identifier "dev.webcodex.desktop"'*) ;; *) fail "Desktop requirement lost identifier" ;; esac
  case "$app_requirement" in *"anchor apple generic"*) ;; *) fail "Desktop requirement is not Apple-anchored" ;; esac
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
