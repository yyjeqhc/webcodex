from __future__ import annotations

import shutil
from pathlib import Path


class RelocationError(RuntimeError):
    pass


IGNORED_DIRECTORY_NAMES = {
    ".webpi-state",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
}


def _is_within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


RUNTIME_BINARY_NAMES = (
    "webpi.exe",
    "webpi-server.exe",
    "webpi-runner.exe",
)


def _ignore_factory(source_root: Path):
    def ignore(directory: str, names: list[str]) -> set[str]:
        ignored = {name for name in names if name in IGNORED_DIRECTORY_NAMES}
        if Path(directory).resolve() == source_root and "target" in names:
            ignored.add("target")
        return ignored

    return ignore


def _copy_runtime_binaries(source_root: Path, target_root: Path) -> list[str]:
    source_dogfood = source_root / "target" / "dogfood"
    if not source_dogfood.is_dir():
        return []
    target_dogfood = target_root / "target" / "dogfood"
    copied: list[str] = []
    for name in RUNTIME_BINARY_NAMES:
        source_binary = source_dogfood / name
        if not source_binary.is_file():
            continue
        target_dogfood.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source_binary, target_dogfood / name)
        copied.append(name)
    return copied


def copy_webpi_tree(source: Path, target: Path) -> dict[str, object]:
    source_resolved = source.resolve()
    target_resolved = target.resolve(strict=False)
    if not source_resolved.is_dir():
        raise RelocationError(f"WebPi source is not a directory: {source_resolved}")
    if target.exists():
        raise RelocationError(f"WebPi relocation target already exists: {target_resolved}")
    if source_resolved == target_resolved or _is_within(target_resolved, source_resolved):
        raise RelocationError("WebPi relocation target must be outside the source tree")

    target_resolved.parent.mkdir(parents=True, exist_ok=True)
    try:
        shutil.copytree(
            source_resolved,
            target_resolved,
            symlinks=True,
            ignore=_ignore_factory(source_resolved),
        )
        copied_binaries = _copy_runtime_binaries(source_resolved, target_resolved)
    except Exception:
        if target_resolved.exists():
            shutil.rmtree(target_resolved, ignore_errors=True)
        raise

    return {
        "source": str(source_resolved),
        "target": str(target_resolved),
        "state_copied": False,
        "git_preserved": (target_resolved / ".git").exists(),
        "runtime_preserved": (target_resolved / ".webpi-runtime").exists(),
        "build_preserved": bool(copied_binaries),
        "runtime_binaries": copied_binaries,
    }
