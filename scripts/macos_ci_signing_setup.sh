#!/usr/bin/env bash
# Public release defaults to the persistent self-signed fallback. No key generation.
set -euo pipefail
: "${GITHUB_ENV:?}"
case "${BUILD_KIND:?}" in
  verification) mode=adhoc ;;
  release) mode="${WEBCODEX_MACOS_SIGNING_MODE:-self-signed}" ;;
  *) echo 'Unknown build kind' >&2; exit 1 ;;
esac
case "$mode" in
  adhoc)
    [ "$BUILD_KIND" = verification ] || { echo 'Public releases cannot use ad-hoc identity' >&2; exit 1; }
    printf 'WEBCODEX_MACOS_SIGNING_MODE=adhoc\nAPPLE_SIGNING_IDENTITY=-\n' >> "$GITHUB_ENV"
    exit 0 ;;
  self-signed) ;;
  developer-id)
    : "${APPLE_ID:?Developer ID notarization requires APPLE_ID}"
    : "${APPLE_PASSWORD:?Developer ID notarization requires APPLE_PASSWORD}"
    : "${APPLE_TEAM_ID:?Developer ID notarization requires APPLE_TEAM_ID}" ;;
  *) echo 'Invalid signing mode' >&2; exit 1 ;;
esac
export WEBCODEX_MACOS_SIGNING_MODE="$mode"
bash "$(dirname "$0")/macos_ci_developer_id_setup.sh"
printf 'WEBCODEX_MACOS_SIGNING_MODE=%s\n' "$mode" >> "$GITHUB_ENV"
