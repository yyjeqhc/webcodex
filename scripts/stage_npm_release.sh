#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_PACKAGE="$ROOT/npm/webcodex"
MANIFEST=""
OUTPUT_DIR=""
ALLOW_DEVELOPMENT=0

usage() {
    cat <<'EOF'
Usage: scripts/stage_npm_release.sh --manifest <manifest.json> --output-dir <empty-dir> [--allow-development]

Create an npm publication staging tree from WebCodex source plus an OE-generated
publish-ready manifest. By default the source worktree must be clean and HEAD
must be the exact immutable v<VERSION> tag. --allow-development is only for
local/CI smoke and must never be used for npm publication.
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --manifest)
            MANIFEST="${2:-}"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="${2:-}"
            shift 2
            ;;
        --allow-development)
            ALLOW_DEVELOPMENT=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

[[ -n "$MANIFEST" ]] || { echo "--manifest is required" >&2; exit 2; }
[[ -n "$OUTPUT_DIR" ]] || { echo "--output-dir is required" >&2; exit 2; }
[[ -f "$MANIFEST" ]] || { echo "manifest not found: $MANIFEST" >&2; exit 2; }

VERSION="$(node -e 'process.stdout.write(require(process.argv[1]).version)' "$SOURCE_PACKAGE/package.json")"
MANIFEST_VERSION="$(node -e 'process.stdout.write(JSON.parse(require("fs").readFileSync(process.argv[1], "utf8")).version || "")' "$MANIFEST")"
[[ "$MANIFEST_VERSION" == "$VERSION" ]] || {
    echo "manifest version $MANIFEST_VERSION does not match package version $VERSION" >&2
    exit 1
}

if [[ "$ALLOW_DEVELOPMENT" -eq 0 ]]; then
    [[ -z "$(git -C "$ROOT" status --porcelain)" ]] || {
        echo "release staging requires a clean source worktree" >&2
        exit 1
    }
    EXPECTED_COMMIT="$(git -C "$ROOT" rev-parse "v$VERSION^{commit}" 2>/dev/null)" || {
        echo "release tag v$VERSION does not exist" >&2
        exit 1
    }
    HEAD_COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
    [[ "$HEAD_COMMIT" == "$EXPECTED_COMMIT" ]] || {
        echo "release staging HEAD $HEAD_COMMIT is not immutable tag v$VERSION ($EXPECTED_COMMIT)" >&2
        exit 1
    }
else
    echo "WARNING: development npm staging mode; do not publish this staging tree" >&2
fi

if [[ -e "$OUTPUT_DIR" ]] && [[ -n "$(find "$OUTPUT_DIR" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]; then
    echo "output directory must be empty: $OUTPUT_DIR" >&2
    exit 1
fi
mkdir -p "$OUTPUT_DIR"
STAGE_DIR="$OUTPUT_DIR/npm-package"
mkdir "$STAGE_DIR"

# Release mode copies only tracked files from the immutable tag. Development
# mode may exercise worktree edits, but never copies local credentials/builds.
if [[ "$ALLOW_DEVELOPMENT" -eq 0 ]]; then
    git -C "$ROOT" archive --format=tar HEAD:npm/webcodex | tar -C "$STAGE_DIR" -xf -
else
    tar -C "$SOURCE_PACKAGE" \
        --exclude='./manifest.json' \
        --exclude='./node_modules' \
        --exclude='./vendor' \
        --exclude='./.npmrc' \
        --exclude='./.env' \
        --exclude='./.env.*' \
        --exclude='*.tgz' \
        -cf - . | tar -C "$STAGE_DIR" -xf -
fi
rm -f "$STAGE_DIR/manifest.json"
install -m 0644 "$MANIFEST" "$STAGE_DIR/manifest.json"

node "$STAGE_DIR/test/release-manifest-check.js" "$STAGE_DIR/manifest.json"

echo "npm release staging prepared"
echo "version=$VERSION"
echo "source_commit=$(git -C "$ROOT" rev-parse HEAD)"
echo "stage_dir=$STAGE_DIR"
