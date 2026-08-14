#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP="$(mktemp -d)"
cleanup() { rm -rf "$TMP"; }
trap cleanup EXIT

cp -a "$ROOT/npm/webcodex" "$TMP/npm-package"
mkdir -p "$TMP/bin" "$TMP/fake-path"
VERSION="$(node -p "require('$ROOT/npm/webcodex/package.json').version")"

python3 - "$TMP/npm-package/manifest.json" "$VERSION" <<'PY'
import json
import sys

path, version = sys.argv[1:]
platforms = ("linux-x64", "linux-arm64", "darwin-arm64", "win32-x64", "win32-arm64")
manifest = {
    "version": version,
    "binaries": ["webcodex", "webcodex-server", "webcodex-runner"],
    "artifacts": {
        platform: {
            "url": (
                f"https://github.com/yyjeqhc/webcodex/releases/download/v{version}/"
                f"webcodex-v{version}-{platform}.tar.gz"
            ),
            "sha256": "a" * 64,
        }
        for platform in platforms
    },
}
with open(path, "w", encoding="utf-8") as handle:
    json.dump(manifest, handle, indent=2)
    handle.write("\n")
PY

for name in webcodex webcodex-server webcodex-runner; do
    cat > "$TMP/bin/$name" <<EOF
#!/bin/sh
if [ "\${1:-}" = "--help" ]; then
    echo "$name help"
    exit 0
fi
case " \$* " in
    *" --version "*)
        echo "$name $VERSION (commit fixturefixture, dirty=false, built_at=1)"
        exit 0
        ;;
esac
echo "$name fixture"
EOF
    chmod +x "$TMP/bin/$name"
done

cat > "$TMP/fake-path/cargo" <<'EOF'
#!/bin/sh
echo "cargo must not be invoked in --binary-dir release smoke" >&2
exit 77
EOF
chmod +x "$TMP/fake-path/cargo"

PATH="$TMP/fake-path:$PATH" bash "$ROOT/scripts/npm_package_smoke.sh" \
    --package-dir "$TMP/npm-package" \
    --binary-dir "$TMP/bin"

echo "npm existing-binary release smoke self-test passed"
