#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

if ! command -v python3 >/dev/null 2>&1; then
    printf '[workspace-boundary][FAIL] python3 is required\n' >&2
    exit 2
fi

if [ "${1:-}" = "--self-test" ]; then
    if [ "$#" -ne 1 ]; then
        printf 'usage: bash scripts/workspace_boundary_check.sh [--self-test]\n' >&2
        exit 2
    fi
    cd "$PROJECT_DIR"
    PYTHONDONTWRITEBYTECODE=1 \
        exec python3 -m unittest scripts.tests.test_workspace_boundary_check
fi

if [ "$#" -ne 0 ]; then
    printf 'usage: bash scripts/workspace_boundary_check.sh [--self-test]\n' >&2
    exit 2
fi

if ! command -v cargo >/dev/null 2>&1; then
    printf '[workspace-boundary][FAIL] cargo is required\n' >&2
    exit 2
fi

CHECK_TEMP_DIR="$(mktemp -d)"
METADATA_FILE="$CHECK_TEMP_DIR/metadata.json"
cleanup() {
    rm -f "$METADATA_FILE"
    rmdir "$CHECK_TEMP_DIR"
}
trap cleanup EXIT

cd "$PROJECT_DIR"
cargo metadata --format-version 1 >"$METADATA_FILE"
PYTHONDONTWRITEBYTECODE=1 python3 "$SCRIPT_DIR/workspace_boundary_check.py" \
    --root "$PROJECT_DIR" \
    --metadata "$METADATA_FILE"
