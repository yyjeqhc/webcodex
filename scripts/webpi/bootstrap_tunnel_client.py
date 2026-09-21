from __future__ import annotations

import json
import os
import shutil
import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

from tunnel_client_runtime import WINDOWS_AMD64, ensure_local_tunnel_client, verify_local_binary


ROOT = Path(__file__).resolve().parents[2]


def source_candidates() -> list[Path]:
    candidates: list[Path] = []
    from_path = shutil.which("tunnel-client")
    if from_path:
        candidates.append(Path(from_path))
    local_app_data = os.environ.get("LOCALAPPDATA")
    if local_app_data:
        candidates.append(
            Path(local_app_data)
            / "WebCodex"
            / "tools"
            / "tunnel-client"
            / WINDOWS_AMD64.version
            / WINDOWS_AMD64.target
            / WINDOWS_AMD64.member_name
        )
    return candidates


def main() -> int:
    installed = ensure_local_tunnel_client(
        ROOT,
        source_candidates=source_candidates(),
        asset=WINDOWS_AMD64,
    )
    if not verify_local_binary(installed, WINDOWS_AMD64):
        raise RuntimeError("WebPi tunnel-client bootstrap completed without a valid pinned binary")
    print(
        json.dumps(
            {
                "status": "ready",
                "version": WINDOWS_AMD64.version,
                "target": WINDOWS_AMD64.target,
                "path": str(installed),
                "sha256": WINDOWS_AMD64.binary_sha256,
            },
            ensure_ascii=False,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
