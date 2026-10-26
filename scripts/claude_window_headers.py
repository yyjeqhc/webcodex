#!/usr/bin/env python3
"""Optional Claude Code headersHelper; stdout is always one JSON header object.

No credentials, command lines, process environments, or Claude state files are
read. Explicit IDs work on every platform. Automatic identity is limited to a
reliably recognized native Claude ancestor on Linux. See
docs/CLAUDE_WINDOW_IDENTITY.md for configuration and process-level semantics.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import sys
from dataclasses import dataclass
from typing import Callable, Mapping, Optional, Sequence


HEADER = "X-WebCodex-Client-Window"
EXPLICIT_ID_ENV = "WEBCODEX_CLIENT_WINDOW_ID"
MAX_PARENT_DEPTH = 32
MAX_STAT_BYTES = 4096
MAX_EXE_BYTES = 4096
OPAQUE_ID = re.compile(r"[A-Za-z0-9._:-]{1,256}", re.ASCII)
MACHINE_ID = re.compile(rb"[0-9a-fA-F]{32}")
BOOT_ID = re.compile(rb"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}")
NATIVE_VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9._-]+)?", re.ASCII)
CLAUDE_NAMES = frozenset((b"claude", b"claude-code"))
RUNTIME_NAMES = frozenset((b"node", b"nodejs", b"bun"))


@dataclass(frozen=True)
class ProcessStamp:
    pid: int
    parent_pid: int
    start_ticks: int
    comm: bytes


def _read_limited(path: Path, limit: int) -> bytes:
    with path.open("rb") as stream:
        value = stream.read(limit + 1)
    if len(value) > limit:
        raise ValueError("bounded identity input exceeded")
    return value


def _unsigned(value: bytes, maximum: int) -> int:
    if not value or len(value) > 20 or not value.isdigit():
        raise ValueError("invalid process number")
    number = int(value)
    if number > maximum:
        raise ValueError("process number exceeded")
    return number


def _read_process(proc_root: Path, pid: int) -> ProcessStamp:
    if not 0 < pid <= 2**31 - 1:
        raise ValueError("invalid process id")
    value = _read_limited(proc_root / str(pid) / "stat", MAX_STAT_BYTES)
    # comm can itself contain spaces and parentheses. The final ') ' delimits
    # the numeric fields; splitting the whole stat line on spaces is incorrect.
    prefix, separator, rest = value.partition(b" (")
    if not separator or prefix != str(pid).encode("ascii") or not value.endswith(b"\n"):
        raise ValueError("incomplete process stat")
    comm, separator, suffix = rest.rpartition(b") ")
    fields = suffix.split()
    if not separator or len(fields) < 20 or len(fields[0]) != 1:
        raise ValueError("incomplete process stat")
    return ProcessStamp(
        pid,
        _unsigned(fields[1], 2**31 - 1),
        _unsigned(fields[19], 2**64 - 1),
        comm,
    )


def _machine_identity(paths: Sequence[Path]) -> bytes:
    for path in paths:
        try:
            value = _read_limited(path, 128).strip()
        except FileNotFoundError:
            continue
        if MACHINE_ID.fullmatch(value) is None:
            raise ValueError("invalid machine identity")
        return value.lower()
    raise ValueError("machine identity unavailable")


def _boot_identity(proc_root: Path) -> bytes:
    value = _read_limited(proc_root / "sys/kernel/random/boot_id", 128).strip()
    if BOOT_ID.fullmatch(value) is None:
        raise ValueError("invalid boot identity")
    return value.lower()


def _native_claude_executable(stamp: ProcessStamp, executable: str) -> bool:
    if stamp.comm not in CLAUDE_NAMES or len(os.fsencode(executable)) > MAX_EXE_BYTES:
        return False
    # Linux marks a running executable this way after an update unlinks it.
    path = PurePosixPath(executable.removesuffix(" (deleted)"))
    if not path.is_absolute():
        return False
    if path.name in ("claude", "claude-code"):
        return True
    # The native installer may resolve the claude symlink to a versioned binary.
    return (
        path.parent.name == "versions"
        and path.parent.parent.name == "claude"
        and NATIVE_VERSION.fullmatch(path.name) is not None
    )


def _linux_process_id(
    proc_root: Path,
    machine_id_paths: Sequence[Path],
    start_pid: int,
    readlink: Callable[[Path], str],
) -> Optional[str]:
    machine = _machine_identity(machine_id_paths)
    boot = _boot_identity(proc_root)
    chain = []
    visited = set()
    pid = start_pid
    for _ in range(MAX_PARENT_DEPTH):
        if pid <= 0 or pid in visited:
            return None
        visited.add(pid)
        stamp = _read_process(proc_root, pid)
        chain.append(stamp)
        # Never walk through an ambiguous npm/runtime wrapper to attribute an
        # inner client to some outer native Claude process.
        if stamp.comm in RUNTIME_NAMES:
            return None
        if stamp.comm in CLAUDE_NAMES:
            executable = readlink(proc_root / str(pid) / "exe")
            if not _native_claude_executable(stamp, executable):
                return None
            # Parent changes, PID reuse, process exit, binary replacement and
            # boot/machine changes observed during the walk all fail closed.
            if any(_read_process(proc_root, item.pid) != item for item in chain):
                return None
            if readlink(proc_root / str(pid) / "exe") != executable:
                return None
            if _boot_identity(proc_root) != boot or _machine_identity(machine_id_paths) != machine:
                return None
            material = json.dumps(
                [
                    "webcodex.claude-process-window.v1",
                    machine.decode("ascii"),
                    boot.decode("ascii"),
                    stamp.pid,
                    stamp.start_ticks,
                ],
                separators=(",", ":"),
            ).encode("ascii")
            return "claude-process-" + hashlib.sha256(material).hexdigest()
        pid = stamp.parent_pid
    return None


def window_headers(
    environment: Optional[Mapping[str, str]] = None,
    *,
    platform: Optional[str] = None,
    proc_root: Path = Path("/proc"),
    machine_id_paths: Sequence[Path] = (Path("/etc/machine-id"), Path("/var/lib/dbus/machine-id")),
    start_pid: Optional[int] = None,
    readlink: Optional[Callable[[Path], str]] = None,
) -> dict:
    environment = os.environ if environment is None else environment
    # Presence is deliberate even when empty/invalid: never silently switch an
    # explicitly configured identity to a process-derived identity.
    if EXPLICIT_ID_ENV in environment:
        value = environment[EXPLICIT_ID_ENV]
        return {HEADER: value} if OPAQUE_ID.fullmatch(value) is not None else {}
    if (sys.platform if platform is None else platform) != "linux":
        return {}
    try:
        identity = _linux_process_id(
            proc_root,
            machine_id_paths,
            os.getpid() if start_pid is None else start_pid,
            os.readlink if readlink is None else readlink,
        )
    except (OSError, ValueError, UnicodeError):
        return {}
    return {HEADER: identity} if identity is not None else {}


def main() -> None:
    try:
        headers = window_headers()
    except Exception:
        # headersHelper output must remain parseable, and failures must not leak
        # paths, machine facts, environment values or exception text to stderr.
        headers = {}
    sys.stdout.write(json.dumps(headers, separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
