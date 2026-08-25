#!/usr/bin/env bash
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

python3 - <<'PY'
from pathlib import Path

paths = sorted(Path("scripts").rglob("*.py"))
for path in paths:
    source = path.read_bytes()
    compile(source, str(path), "exec")
print(f"syntax-checked {len(paths)} Python files under scripts/")
PY

python3 -m unittest discover \
    --start-directory scripts/tests \
    --pattern 'test_*.py'
