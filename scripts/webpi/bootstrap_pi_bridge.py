from __future__ import annotations

import hashlib
import os
import shutil
import subprocess
import sys
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
RUNTIME = ROOT / ".webpi-runtime"
NODE_HOME = RUNTIME / "node"
NODE_VERSION = "24.21.0"
BASE = f"https://nodejs.org/dist/v{NODE_VERSION}"
ZIP_NAME = f"node-v{NODE_VERSION}-win-x64.zip"
ZIP_PATH = RUNTIME / ZIP_NAME
SHASUMS = RUNTIME / "SHASUMS256.txt"
PLUGIN = ROOT / "plugins" / "pi-bridge"
SDK = ROOT / "npm" / "plugin-sdk"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def download(url: str, dest: Path) -> None:
    print(f"download: {url}", file=sys.stderr)
    with urllib.request.urlopen(url, timeout=120) as response, dest.open("wb") as handle:
        shutil.copyfileobj(response, handle)


def ensure_node() -> Path:
    node = NODE_HOME / "node.exe"
    if node.exists():
        return node

    RUNTIME.mkdir(parents=True, exist_ok=True)
    download(f"{BASE}/SHASUMS256.txt", SHASUMS)
    expected = None
    for line in SHASUMS.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[-1] == ZIP_NAME:
            expected = parts[0].lower()
            break
    if expected is None:
        raise RuntimeError(f"{ZIP_NAME} missing from official SHASUMS256.txt")

    if not ZIP_PATH.exists():
        download(f"{BASE}/{ZIP_NAME}", ZIP_PATH)
    actual = sha256(ZIP_PATH)
    if actual != expected:
        raise RuntimeError(f"Node archive checksum mismatch: {actual} != {expected}")

    extracted = RUNTIME / f"node-v{NODE_VERSION}-win-x64"
    if extracted.exists():
        shutil.rmtree(extracted)
    with zipfile.ZipFile(ZIP_PATH) as archive:
        archive.extractall(RUNTIME)
    if NODE_HOME.exists():
        shutil.rmtree(NODE_HOME)
    extracted.rename(NODE_HOME)
    return node


def run(command: list[str], cwd: Path) -> None:
    env = os.environ.copy()
    env["PATH"] = str(NODE_HOME) + os.pathsep + env.get("PATH", "")
    print("run:", " ".join(command), file=sys.stderr)
    subprocess.run(command, cwd=cwd, env=env, check=True)


def main() -> None:
    node = ensure_node()
    npm = NODE_HOME / "npm.cmd"
    run([str(npm), "install", "--ignore-scripts", "--package-lock=false"], SDK)
    run([str(npm), "run", "build"], SDK)
    shutil.rmtree(
        PLUGIN / "node_modules" / "@yyjeqhc" / "webcodex-plugin-sdk",
        ignore_errors=True,
    )
    run([str(npm), "install", "--ignore-scripts", "--package-lock=false"], PLUGIN)
    run([str(npm), "test"], PLUGIN)
    print(f"WEBPI_NODE={node}")
    print(f"WEBPI_PI_BRIDGE={PLUGIN / 'dist' / 'plugin.js'}")


if __name__ == "__main__":
    main()
