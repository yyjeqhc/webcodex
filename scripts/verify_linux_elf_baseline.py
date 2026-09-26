#!/usr/bin/env python3
"""Check native Linux installer ELF files against the Ubuntu 22.04 ABI floor."""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


class VerificationError(ValueError):
    pass


def version_tuple(value: str) -> tuple[int, ...]:
    try:
        return tuple(int(part) for part in value.split("."))
    except ValueError as exc:
        raise VerificationError(f"invalid GLIBC version: {value}") from exc


def verify(path: Path, architecture: str, maximum_glibc: str) -> dict[str, str]:
    if not path.is_file() or path.is_symlink():
        raise VerificationError(f"not a regular ELF file: {path}")
    header = subprocess.run(["readelf", "-h", str(path)], capture_output=True, text=True, check=False)
    if header.returncode:
        raise VerificationError(f"could not read ELF header: {path}")
    machine = re.search(r"^\s*Machine:\s*(.+)$", header.stdout, re.MULTILINE)
    expected_machine = {"x86_64": "X86-64", "aarch64": "AArch64"}.get(architecture)
    if expected_machine is None or machine is None or expected_machine.lower() not in machine.group(1).lower():
        raise VerificationError(f"unexpected ELF architecture for {path}")

    versions = subprocess.run(["readelf", "--version-info", str(path)], capture_output=True, text=True, check=False)
    if versions.returncode:
        raise VerificationError(f"could not inspect ELF symbol versions: {path}")
    glibc_versions = re.findall(r"GLIBC_([0-9]+(?:\.[0-9]+)+)", versions.stdout)
    if not glibc_versions:
        raise VerificationError(f"ELF file has no GLIBC symbol version evidence: {path}")
    maximum = max(glibc_versions, key=version_tuple)
    if version_tuple(maximum) > version_tuple(maximum_glibc):
        raise VerificationError(f"{path} requires GLIBC_{maximum}; supported maximum is GLIBC_{maximum_glibc}")

    dependencies = subprocess.run(["ldd", "-r", str(path)], capture_output=True, text=True, check=False)
    output = dependencies.stdout + dependencies.stderr
    if dependencies.returncode or "not found" in output or "undefined symbol" in output:
        raise VerificationError(f"unresolved runtime dependency or symbol in {path}: {output.strip()}")
    return {"path": str(path), "machine": machine.group(1).strip(), "maximum_glibc": maximum}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--architecture", choices=("x86_64", "aarch64"), required=True)
    parser.add_argument("--maximum-glibc", default="2.35")
    parser.add_argument("binaries", nargs="+", type=Path)
    args = parser.parse_args(argv)
    try:
        for binary in args.binaries:
            result = verify(binary, args.architecture, args.maximum_glibc)
            print(f"{result['path']}: {result['machine']}, max GLIBC_{result['maximum_glibc']}")
    except VerificationError as exc:
        print(str(exc), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
