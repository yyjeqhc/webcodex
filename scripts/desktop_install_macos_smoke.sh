#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 --dmg <path> --version <version> --source-sha <40hex> --built-at <unix> --platform <darwin-x64|darwin-arm64> --stage-metadata <path> --signing-mode <adhoc|developer-id> [--evidence <path>]" >&2
  exit 2
}

dmg=""
version=""
source_sha=""
built_at=""
platform=""
stage_metadata=""
signing_mode=""
evidence=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dmg) [ "$#" -ge 2 ] || usage; dmg="$2"; shift 2 ;;
    --version) [ "$#" -ge 2 ] || usage; version="$2"; shift 2 ;;
    --source-sha) [ "$#" -ge 2 ] || usage; source_sha="$2"; shift 2 ;;
    --built-at) [ "$#" -ge 2 ] || usage; built_at="$2"; shift 2 ;;
    --platform) [ "$#" -ge 2 ] || usage; platform="$2"; shift 2 ;;
    --stage-metadata) [ "$#" -ge 2 ] || usage; stage_metadata="$2"; shift 2 ;;
    --signing-mode) [ "$#" -ge 2 ] || usage; signing_mode="$2"; shift 2 ;;
    --evidence) [ "$#" -ge 2 ] || usage; evidence="$2"; shift 2 ;;
    *) usage ;;
  esac
done

[ -n "$dmg" ] && [ -n "$version" ] && [ -n "$source_sha" ] && [ -n "$built_at" ] \
  && [ -n "$platform" ] && [ -n "$stage_metadata" ] && [ -n "$signing_mode" ] || usage
[[ "$source_sha" =~ ^[0-9A-Fa-f]{40}$ ]] || { echo "invalid source SHA" >&2; exit 1; }
[[ "$built_at" =~ ^[1-9][0-9]*$ ]] || { echo "invalid built_at" >&2; exit 1; }
case "$platform" in
  darwin-x64) expected_host=x86_64; expected_arch=x86_64 ;;
  darwin-arm64) expected_host=arm64; expected_arch=arm64 ;;
  *) echo "unsupported Desktop platform: $platform" >&2; exit 1 ;;
esac
case "$signing_mode" in adhoc|developer-id) ;; *) echo "invalid signing mode" >&2; exit 1 ;; esac
[ "$(uname -m)" = "$expected_host" ] || { echo "Desktop smoke requires native $expected_host host" >&2; exit 1; }
[ -f "$dmg" ] && [ ! -L "$dmg" ] || { echo "DMG is missing or not a regular file: $dmg" >&2; exit 1; }
[ -f "$stage_metadata" ] && [ ! -L "$stage_metadata" ] || { echo "stage metadata is missing: $stage_metadata" >&2; exit 1; }

temp_root="$(mktemp -d "${TMPDIR:-/tmp}/webcodex-desktop-macos-smoke.XXXXXX")"
mount_point="$temp_root/mount"
mkdir "$mount_point"
attached=0
cleanup() {
  if [ "$attached" -eq 1 ]; then
    hdiutil detach "$mount_point" -quiet >/dev/null 2>&1 || true
  fi
  rm -rf -- "$temp_root"
}
trap cleanup EXIT INT TERM

hdiutil attach -readonly -nobrowse -mountpoint "$mount_point" "$dmg" >/dev/null
attached=1
app="$mount_point/WebCodex Desktop.app"
[ -d "$app/Contents" ] || { echo "WebCodex Desktop.app is missing from DMG" >&2; exit 1; }
runtime_dir="$app/Contents/Resources/webcodex-runtime"
[ -d "$runtime_dir" ] || { echo "bundled WebCodex runtime directory is missing" >&2; exit 1; }

python3 - "$stage_metadata" "$version" "$source_sha" "$built_at" "$platform" "$signing_mode" <<'PY'
import json
import re
import sys
from pathlib import Path

path, version, source, built_at, platform, signing_mode = sys.argv[1:]
value = json.loads(Path(path).read_text(encoding="utf-8"))
required = {"schema_version", "platform", "version", "source_sha", "built_at", "signing_mode", "resource_dir", "provenance", "files"}
if set(value) != required or value.get("schema_version") != 2:
    raise SystemExit("unexpected Desktop staging metadata schema")
if value.get("version") != version or value.get("source_sha") != source.lower():
    raise SystemExit("Desktop staging metadata release identity mismatch")
if value.get("built_at") != int(built_at) or value.get("platform") != platform or value.get("signing_mode") != signing_mode:
    raise SystemExit("Desktop staging metadata platform/signing identity mismatch")
if value.get("provenance") != "same_unsigned_runtime_input_before_platform_signing":
    raise SystemExit("Desktop staging metadata provenance mismatch")
files = value.get("files")
if not isinstance(files, dict) or set(files) != {"webcodex", "webcodex-server", "webcodex-runner"}:
    raise SystemExit("Desktop staging metadata runtime set mismatch")
for name, item in files.items():
    if not isinstance(item, dict) or set(item) != {"filename", "size", "source_sha256", "staged_unsigned_sha256"}:
        raise SystemExit(f"malformed staged runtime metadata: {name}")
    if item["filename"] != name or not isinstance(item["size"], int) or item["size"] <= 0:
        raise SystemExit(f"invalid staged runtime file metadata: {name}")
    for key in ("source_sha256", "staged_unsigned_sha256"):
        if not isinstance(item[key], str) or not re.fullmatch(r"[0-9a-f]{64}", item[key]):
            raise SystemExit(f"invalid staged runtime digest: {name}")
    if item["source_sha256"] != item["staged_unsigned_sha256"]:
        raise SystemExit(f"unsigned source/staged digest mismatch: {name}")
PY

short_source="$(printf '%s' "${source_sha:0:12}" | tr '[:upper:]' '[:lower:]')"
for name in webcodex webcodex-server webcodex-runner; do
  binary="$runtime_dir/$name"
  [ -f "$binary" ] && [ ! -L "$binary" ] && [ -x "$binary" ] || { echo "bundled runtime missing: $name" >&2; exit 1; }
  actual="$("$binary" --version | head -n 1)"
  expected="$name $version (commit $short_source, dirty=false, built_at=$built_at)"
  [ "$actual" = "$expected" ] || { echo "unexpected bundled runtime identity for $name" >&2; exit 1; }
  actual_arch="$(/usr/bin/lipo -archs "$binary")"
  [ "$actual_arch" = "$expected_arch" ] || { echo "unexpected bundled runtime architecture for $name: $actual_arch" >&2; exit 1; }
done

codesign --verify --deep --strict --verbose=2 "$app"
notarized=false
if [ "$signing_mode" = developer-id ]; then
  spctl --assess --type execute --verbose=2 "$app"
  xcrun stapler validate "$app"
  notarized=true
fi

if [ -n "$evidence" ]; then
  mkdir -p "$(dirname "$evidence")"
  python3 - "$stage_metadata" "$runtime_dir" "$dmg" "$platform" "$signing_mode" "$notarized" "$evidence" <<'PY'
import hashlib
import json
import sys
from pathlib import Path

metadata_path, runtime_dir, dmg_path, platform, signing_mode, notarized, evidence_path = sys.argv[1:]
metadata = json.loads(Path(metadata_path).read_text(encoding="utf-8"))

def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()

runtime = Path(runtime_dir)
files = {}
for name, staged in metadata["files"].items():
    bundled = runtime / name
    files[name] = {
        "unsigned_input_sha256": staged["staged_unsigned_sha256"],
        "bundled_signed_sha256": digest(bundled),
    }
payload = {
    "schema_version": 1,
    "platform": platform,
    "signing_mode": signing_mode,
    "notarized": notarized == "true",
    "dmg_sha256": digest(Path(dmg_path)),
    "runtime": files,
}
Path(evidence_path).write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
PY
fi

echo "macOS Desktop DMG smoke passed: $platform signing=$signing_mode notarized=$notarized"
