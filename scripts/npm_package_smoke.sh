#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PACKAGE_DIR="${WEBCODEX_NPM_PACKAGE_DIR:-$ROOT/npm/webcodex}"
PROFILE="${WEBCODEX_NPM_SMOKE_PROFILE:-release}"
BIN_DIR="${WEBCODEX_NPM_BINARY_DIR:-}"

usage() {
    cat <<'EOF'
Usage: scripts/npm_package_smoke.sh [--package-dir DIR] [--binary-dir DIR] [--profile release|debug]

Without --binary-dir, build the three native binaries from source using --profile.
With --binary-dir, reuse that exact binary set and do not invoke Cargo. The installed
npm native files are compared byte-for-byte with the supplied binaries.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --package-dir|--binary-dir|--profile)
            [[ $# -ge 2 && -n "$2" ]] || { echo "$1 requires a value" >&2; exit 2; }
            case "$1" in
                --package-dir) PACKAGE_DIR="$2" ;;
                --binary-dir) BIN_DIR="$2" ;;
                --profile) PROFILE="$2" ;;
            esac
            shift 2
            ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

if [[ ! -f "$PACKAGE_DIR/manifest.json" ]]; then
    echo "[npm-smoke] publish-ready manifest missing from package staging: $PACKAGE_DIR/manifest.json" >&2
    echo "[npm-smoke] run scripts/stage_npm_release.sh with release metadata generated on the release control host first" >&2
    exit 2
fi

if [[ -z "$BIN_DIR" ]]; then
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
            echo "WEBCODEX_NPM_SMOKE_PROFILE/--profile must be 'release' or 'debug'" >&2
            exit 2
            ;;
    esac
    echo "[npm-smoke] building three WebCodex binaries ($PROFILE)"
    cargo "${CARGO_ARGS[@]}"
else
    [[ -d "$BIN_DIR" ]] || { echo "[npm-smoke] binary directory not found: $BIN_DIR" >&2; exit 2; }
    BIN_DIR="$(cd "$BIN_DIR" && pwd)"
    echo "[npm-smoke] reusing existing native binaries from $BIN_DIR"
fi
for name in webcodex webcodex-server webcodex-runner; do
    test -x "$BIN_DIR/$name" || { echo "[npm-smoke] missing executable $BIN_DIR/$name" >&2; exit 1; }
done

VERSION="$(node -e "process.stdout.write(require('$PACKAGE_DIR/package.json').version)")"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT
PACK_DIR="$TMP/pack"
PREFIX="$TMP/prefix"
UNPACK="$TMP/unpack"
mkdir -p "$PACK_DIR" "$PREFIX" "$UNPACK"

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
const raw = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const reports = Array.isArray(raw) ? raw : Object.values(raw || {});
if (reports.length !== 1 || !reports[0] || !Array.isArray(reports[0].files)) {
  throw new Error('unexpected npm pack --json result shape');
}
const files = reports[0].files.map((entry) => entry.path);
for (const required of ['README.md', 'bin/webcodex.js', 'bin/wrapper.js', 'install.js', 'manifest.json', 'package.json']) {
  if (!files.includes(required)) throw new Error(`npm pack missing ${required}`);
}
for (const forbidden of ['manifest.example.json', 'bin/webcodex-cli.js', 'bin/webcodex-runner.js', 'vendor/bin/webcodex-cli']) {
  if (files.includes(forbidden)) throw new Error(`npm pack contains legacy/private wrapper ${forbidden}`);
}
NODE

echo "[npm-smoke] packing and inspecting local tarball"
(cd "$PACKAGE_DIR" && npm pack --pack-destination "$PACK_DIR" --json > "$TMP/pack.json")
TARBALL_NAME="$(node - "$TMP/pack.json" <<'NODE'
const fs = require('fs');
const raw = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const reports = Array.isArray(raw) ? raw : Object.values(raw || {});
if (reports.length !== 1 || !reports[0] || typeof reports[0].filename !== 'string') {
  throw new Error('unexpected npm pack --json result shape');
}
process.stdout.write(reports[0].filename);
NODE
)"
TARBALL="$PACK_DIR/$TARBALL_NAME"
tar -xzf "$TARBALL" -C "$UNPACK"
node - "$UNPACK/package/package.json" <<'NODE'
const pkg = require(process.argv[2]);
if (JSON.stringify(pkg.bin) !== JSON.stringify({webcodex: 'bin/webcodex.js'})) {
  throw new Error(`unexpected npm bin mapping: ${JSON.stringify(pkg.bin)}`);
}
if (!pkg.scripts || pkg.scripts.postinstall !== 'node install.js') {
  throw new Error(`unexpected npm postinstall: ${JSON.stringify(pkg.scripts && pkg.scripts.postinstall)}`);
}
NODE
if find "$UNPACK/package" -type f \( -name '.env' -o -name '*token*' -o -name 'webcodex-cli*' \) | grep -q .; then
    echo "[npm-smoke] tarball contains a forbidden sensitive or legacy file" >&2
    exit 1
fi

echo "[npm-smoke] proving one-shot npx lazy bootstrap from the packed tarball"
NPX_CACHE="$TMP/npx-cache"
WEBCODEX_BINARY_DIR="$BIN_DIR" npm_config_cache="$NPX_CACHE" npm_config_ignore_scripts=true \
    npx --yes --package "$TARBALL" webcodex --version >/dev/null
NPX_PACKAGE_DIR="$(find "$NPX_CACHE/_npx" -path '*/node_modules/@yyjeqhc/webcodex' -type d -print -quit)"
test -n "$NPX_PACKAGE_DIR"
for name in webcodex webcodex-server webcodex-runner; do
    test -x "$NPX_PACKAGE_DIR/vendor/bin/$name"
done

echo "[npm-smoke] installing tarball into temporary prefix without host lifecycle policy"
npm install --global --prefix "$PREFIX" --ignore-scripts --no-audit --no-fund "$TARBALL"
INSTALLED_PACKAGE="$PREFIX/lib/node_modules/@yyjeqhc/webcodex"
WEBCODEX_BINARY_DIR="$BIN_DIR" node "$INSTALLED_PACKAGE/install.js"
NATIVE_DIR="$INSTALLED_PACKAGE/vendor/bin"
for name in webcodex webcodex-server webcodex-runner; do
    test -x "$NATIVE_DIR/$name"
    output="$("$NATIVE_DIR/$name" --version)"
    echo "[npm-smoke] $output"
    case "$output" in
        "$name $VERSION "*|"$name $VERSION") ;;
        *) echo "[npm-smoke] unexpected $name version output: $output" >&2; exit 1 ;;
    esac
    if ! cmp -s "$BIN_DIR/$name" "$NATIVE_DIR/$name"; then
        echo "[npm-smoke] installed $name is not byte-identical to the supplied native binary" >&2
        exit 1
    fi
done
test ! -e "$NATIVE_DIR/webcodex-cli"
test -x "$PREFIX/bin/webcodex"
test ! -e "$PREFIX/bin/webcodex-cli"
test ! -e "$PREFIX/bin/webcodex-runner"

"$PREFIX/bin/webcodex" --help >/dev/null
"$PREFIX/bin/webcodex" --version >/dev/null
"$PREFIX/bin/webcodex" server run --version >/dev/null
"$PREFIX/bin/webcodex" runner run --version >/dev/null

echo "[npm-smoke] local npm package smoke passed for $VERSION"
