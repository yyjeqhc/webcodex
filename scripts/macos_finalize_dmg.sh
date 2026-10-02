#!/usr/bin/env bash
# Finalize the actual app and repack its DMG after Tauri nested signing.
set -euo pipefail
[ "$#" -eq 3 ] || { echo "usage: $0 <dmg> <mode> <identity>" >&2; exit 2; }
[ "$2" != adhoc ] || exit 0
dmg="$1"
scripts="$(cd "$(dirname "$0")" && pwd)"
scratch="$(mktemp -d "${TMPDIR:-/tmp}/webcodex-final-dmg.XXXXXX")"
mounted=0
cleanup() {
  if [ "$mounted" -eq 1 ]; then hdiutil detach "$scratch/mount" -quiet || true; fi
  rm -rf "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/mount" "$scratch/payload"
hdiutil attach "$dmg" -readonly -nobrowse -mountpoint "$scratch/mount" -quiet
mounted=1
shopt -s nullglob
apps=("$scratch/mount"/*.app)
[ "${#apps[@]}" -eq 1 ]
app="$scratch/payload/$(basename "${apps[0]}")"
ditto "${apps[0]}" "$app"
hdiutil detach "$scratch/mount" -quiet
mounted=0
bash "$scripts/macos_finalize_desktop.sh" "$app" "$2" "$3"
ln -s /Applications "$scratch/payload/Applications"
hdiutil create -quiet -volname 'WebCodex Desktop' -srcfolder "$scratch/payload" -format UDZO "$scratch/final.dmg"
cp "$scratch/final.dmg" "$dmg"
