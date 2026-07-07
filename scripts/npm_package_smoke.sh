#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_DIR="$ROOT/npm/webcodex"
PROFILE="${WEBCODEX_NPM_SMOKE_PROFILE:-release}"

case "$PROFILE" in
    release)
        CARGO_ARGS=(build --release --bins)
        BIN_DIR="$ROOT/target/release"
        ;;
    debug)
        CARGO_ARGS=(build --bins)
        BIN_DIR="$ROOT/target/debug"
        ;;
    *)
        echo "WEBCODEX_NPM_SMOKE_PROFILE must be 'release' or 'debug'" >&2
        exit 2
        ;;
esac

VERSION="$(node -e "process.stdout.write(require('$PACKAGE_DIR/package.json').version)")"
TMP="$(mktemp -d)"
cleanup() {
    rm -rf "$TMP"
}
trap cleanup EXIT

PACK_DIR="$TMP/pack"
PREFIX="$TMP/prefix"
mkdir -p "$PACK_DIR" "$PREFIX"

echo "[npm-smoke] building WebCodex binaries ($PROFILE)"
cargo "${CARGO_ARGS[@]}"

echo "[npm-smoke] running npm self-test"
npm --prefix "$PACKAGE_DIR" test

echo "[npm-smoke] packing npm tarball"
TARBALL="$(cd "$PACKAGE_DIR" && npm pack --pack-destination "$PACK_DIR")"
TARBALL="$PACK_DIR/$TARBALL"

echo "[npm-smoke] installing tarball into temporary prefix"
WEBCODEX_BINARY_DIR="$BIN_DIR" npm install --global --prefix "$PREFIX" "$TARBALL"

for name in webcodex webcodex-cli webcodex-agent; do
    output="$("$PREFIX/bin/$name" --version)"
    echo "[npm-smoke] $output"
    case "$output" in
        "$name $VERSION "*)
            ;;
        *)
            echo "[npm-smoke] unexpected $name version output: $output" >&2
            exit 1
            ;;
    esac
done

"$PREFIX/bin/webcodex" -h >/dev/null
"$PREFIX/bin/webcodex-cli" -h >/dev/null
"$PREFIX/bin/webcodex-agent" -h >/dev/null

echo "[npm-smoke] local npm package smoke passed for $VERSION"
