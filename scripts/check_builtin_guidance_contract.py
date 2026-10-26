#!/usr/bin/env python3
"""No-build size/shape preflight for bundled model guidance.

The Rust composition and behavior tests own semantics. This checker reads only
contributors and Rust-owned ergonomic limits; ordinary wording is not a contract.
"""

from __future__ import annotations

import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
CONTRIBUTORS = ROOT / "src/tool_runtime/guidance/contributors.rs"
CONTRACT = ROOT / "crates/webcodex-core/src/runtime_contract.rs"


class GuidancePreflightError(ValueError):
    """The static source size/shape cannot be checked without compiling."""


def _items(source: str, name: str) -> list[str]:
    anchor = f"pub(super) static {name}: GuidanceContributor ="
    if source.count(anchor) != 1:
        raise GuidancePreflightError(f"{name}: guidance contributor not uniquely found")
    section = source.split(anchor, 1)[1].split("};", 1)
    if len(section) != 2 or section[0].count("items: &[") != 1:
        raise GuidancePreflightError(f"{name}: unexpected contributor layout")
    result: list[str] = []
    for line in section[0].split("items: &[", 1)[1].splitlines():
        value = line.strip()
        if value == "],":
            if not result:
                raise GuidancePreflightError(f"{name}: empty guidance list")
            return result
        if not value:
            continue
        if not (value.startswith('"') and value.endswith('",')):
            raise GuidancePreflightError(f"{name}: unsupported Rust literal layout")
        try:
            item = json.loads(value[:-1])
        except json.JSONDecodeError as exc:
            raise GuidancePreflightError(f"{name}: unparseable guidance literal") from exc
        if not isinstance(item, str) or not item.strip():
            raise GuidancePreflightError(f"{name}: empty or non-string guidance item")
        result.append(item)
    raise GuidancePreflightError(f"{name}: unterminated guidance list")


def _limit(contract: str, name: str) -> int:
    values = re.findall(rf"(?m)^pub const {re.escape(name)}: usize = (\d+);$", contract)
    if len(values) != 1 or int(values[0]) == 0:
        raise GuidancePreflightError(f"missing or invalid canonical Rust limit {name}")
    return int(values[0])


def validate(contributors: str, contract: str) -> dict[str, int]:
    max_items = _limit(contract, "BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEMS")
    max_chars = _limit(contract, "BUILTIN_CODING_WORKFLOW_GUIDANCE_TARGET_ITEM_CHARS")
    # Group by the contributor's own target instead of another profile/name list.
    declarations = list(re.finditer(
        r"pub\(super\) static (\w+): GuidanceContributor = GuidanceContributor \{\s*"
        r"target: GuidanceTarget::([\w:()]+),\s*items: &\[",
        contributors,
    ))
    if not declarations or len(declarations) != contributors.count("GuidanceContributor ="):
        raise GuidancePreflightError("unexpected contributor declaration layout")
    targets: dict[str, list[str]] = {}
    for declaration in declarations:
        name, target = declaration.groups()
        targets.setdefault(target, []).extend(_items(contributors, name))
    for target, entries in targets.items():
        if len(entries) > max_items:
            raise GuidancePreflightError(f"{target}: {len(entries)} items exceed {max_items}")
        if any(len(value) > max_chars for value in entries):
            raise GuidancePreflightError(f"{target}: guidance item exceeds {max_chars} chars")
        if len(set(entries)) != len(entries):
            raise GuidancePreflightError(f"{target}: duplicate guidance item")
    return {target: len(entries) for target, entries in targets.items()}


def main() -> int:
    try:
        result = validate(
            CONTRIBUTORS.read_text(encoding="utf-8"),
            CONTRACT.read_text(encoding="utf-8"),
        )
    except (GuidancePreflightError, OSError, UnicodeError) as exc:
        print(f"[guidance preflight] FAIL: {exc}", file=sys.stderr)
        return 1
    print("[guidance preflight] PASS: "
          f"{len(result)} targets, {sum(result.values())} items; no Cargo build")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
