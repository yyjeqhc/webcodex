#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_DIR="${WEBCODEX_NPM_PACKAGE_DIR:-$ROOT/npm/webcodex}"
PROFILE="${WEBCODEX_NPM_SMOKE_PROFILE:-release}"

if [[ ! -f "$PACKAGE_DIR/manifest.json" ]]; then
    echo "[npm-smoke] publish-ready manifest missing from package staging: $PACKAGE_DIR/manifest.json" >&2
    echo "[npm-smoke] run scripts/stage_npm_release.sh with OE-generated release metadata first" >&2
    exit 2
fi

case "$PROFILE" in
    release)
        CARGO_ARGS=(build --release -p webcodex-cli --bin webcodex -p webcodex --bin webcodex-server -p webcodex-runner --bin webcodex-runner)
        BIN_DIR="$ROOT/target/release"
        ;;
    debug)
        CARGO_ARGS=(build -p webcodex-cli --bin webcodex -p webcodex --bin webcodex-server -p webcodex-runner --bin webcodex-runner)
        BIN_DIR="$ROOT/target/debug"
        ;;
    *)
        echo "WEBCODEX_NPM_SMOKE_PROFILE must be 'release' or 'debug'" >&2
        exit 2
        ;;
esac

VERSION="$(node -e "process.stdout.write(require('$PACKAGE_DIR/package.json').version)")"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT
PACK_DIR="$TMP/pack"
PREFIX="$TMP/prefix"
UNPACK="$TMP/unpack"
mkdir -p "$PACK_DIR" "$PREFIX" "$UNPACK"

echo "[npm-smoke] building three WebCodex binaries ($PROFILE)"
cargo "${CARGO_ARGS[@]}"

echo "[npm-smoke] running npm installer/wrapper tests"
npm --prefix "$PACKAGE_DIR" test

echo "[npm-smoke] confirming release manifest permits publishing"
npm --prefix "$PACKAGE_DIR" run prepublishOnly

echo "[npm-smoke] confirming placeholder example blocks publishing"
if node "$PACKAGE_DIR/test/release-manifest-check.js" "$PACKAGE_DIR/manifest.example.json" >"$TMP/publish-guard.log" 2>&1; then
    echo "[npm-smoke] placeholder release manifest unexpectedly passed publish guard" >&2
    exit 1
fi
if ! grep -Eq 'must not be a placeholder|64 lowercase hexadecimal' "$TMP/publish-guard.log"; then
    echo "[npm-smoke] publish guard failed without a recognized bounded manifest diagnostic" >&2
    cat "$TMP/publish-guard.log" >&2
    exit 1
fi

echo "[npm-smoke] checking dry-run package contents"
(cd "$PACKAGE_DIR" && npm pack --dry-run --json > "$TMP/pack-dry-run.json")
node - "$TMP/pack-dry-run.json" <<'NODE'
const fs = require('fs');
const report = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'))[0];
const files = report.files.map((entry) => entry.path);
for (const required of ['README.md', 'bin/webcodex.js', 'bin/wrapper.js', 'install.js', 'manifest.json', 'package.json']) {
  if (!files.includes(required)) throw new Error(`npm pack missing ${required}`);
}
for (const forbidden of ['manifest.example.json', 'bin/webcodex-cli.js', 'bin/webcodex-runner.js', 'vendor/bin/webcodex-cli']) {
  if (files.includes(forbidden)) throw new Error(`npm pack contains legacy/private wrapper ${forbidden}`);
}
NODE

echo "[npm-smoke] packing and inspecting local tarball"
(cd "$PACKAGE_DIR" && npm pack --pack-destination "$PACK_DIR" --json > "$TMP/pack.json")
TARBALL_NAME="$(node -e 'const r=require(process.argv[1]); process.stdout.write(r[0].filename)' "$TMP/pack.json")"
TARBALL="$PACK_DIR/$TARBALL_NAME"
tar -xzf "$TARBALL" -C "$UNPACK"
node - "$UNPACK/package/package.json" <<'NODE'
const pkg = require(process.argv[2]);
if (JSON.stringify(pkg.bin) !== JSON.stringify({webcodex: 'bin/webcodex.js'})) {
  throw new Error(`unexpected npm bin mapping: ${JSON.stringify(pkg.bin)}`);
}
NODE
if find "$UNPACK/package" -type f \( -name '.env' -o -name '*token*' -o -name 'webcodex-cli*' \) | grep -q .; then
    echo "[npm-smoke] tarball contains a forbidden sensitive or legacy file" >&2
    exit 1
fi

echo "[npm-smoke] installing tarball into temporary prefix"
WEBCODEX_BINARY_DIR="$BIN_DIR" npm install --global --prefix "$PREFIX" "$TARBALL"
NATIVE_DIR="$PREFIX/lib/node_modules/@yyjeqhc/webcodex/vendor/bin"
for name in webcodex webcodex-server webcodex-runner; do
    test -x "$NATIVE_DIR/$name"
    output="$("$NATIVE_DIR/$name" --version)"
    echo "[npm-smoke] $output"
    case "$output" in
        "$name $VERSION "*|"$name $VERSION") ;;
        *) echo "[npm-smoke] unexpected $name version output: $output" >&2; exit 1 ;;
    esac
done
test ! -e "$NATIVE_DIR/webcodex-cli"
test -x "$PREFIX/bin/webcodex"
test ! -e "$PREFIX/bin/webcodex-cli"
test ! -e "$PREFIX/bin/webcodex-runner"

"$PREFIX/bin/webcodex" --help >/dev/null
"$PREFIX/bin/webcodex" --version >/dev/null
"$PREFIX/bin/webcodex" server run --version >/dev/null
"$PREFIX/bin/webcodex" agent run --version >/dev/null

echo "[npm-smoke] local npm package smoke passed for $VERSION"
