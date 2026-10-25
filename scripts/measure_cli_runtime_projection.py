#!/usr/bin/env python3
"""Compare two compiled CLI projections of the same captured native Runtime output.

This starts no Server or Runner. Native captures come from e2e_cli_runtime_output.py.
Results are UTF-8 bytes and Unicode scalar counts, not Host tokens or latency.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

MAX_CAPTURE_BYTES = 2 * 1024 * 1024
TEST = "mcp::tests::cli::text_projection::cli_runtime_replay_captured_native_response"
PREFIX = "runtime_cli_replay "


def check(condition, message):
    if not condition:
        raise RuntimeError(message)


def digest(path):
    sha = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            sha.update(chunk)
    return sha.hexdigest()


def capture_metadata(path):
    check(path.is_file(), f"capture must be a regular file: {path}")
    with path.open("rb") as source:
        raw = source.read(MAX_CAPTURE_BYTES + 1)
    check(len(raw) <= MAX_CAPTURE_BYTES, f"capture exceeds 2 MiB: {path}")
    value = json.loads(raw)
    check(isinstance(value, dict) and value.get("format") == "webcodex.cli_runtime_capture.v1",
          f"unsupported native capture: {path}")
    check(value.get("tool") in {"list_projects", "list_workspace_symbols"},
          f"unsupported captured tool: {path}")
    return {"tool": value["tool"], "sha256": hashlib.sha256(raw).hexdigest(),
            "source": value.get("source"), "input_bytes": len(raw)}


def replay(binary, capture, destination):
    env = {key: value for key, value in os.environ.items() if not key.startswith("WEBCODEX_")}
    env["WEBCODEX_CLI_RUNTIME_CAPTURE"] = str(capture)
    env["WEBCODEX_CLI_RUNTIME_REPLAY_OUTPUT"] = str(destination)
    result = subprocess.run(
        [str(binary), "--ignored", "--exact", TEST, "--nocapture", "--test-threads=1"],
        env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT, timeout=45, check=False,
    )
    check(len(result.stdout) <= 64 * 1024, "projection replay log exceeded 64 KiB")
    log = result.stdout.decode("utf-8")
    destination.with_suffix(".log").write_text(log, encoding="utf-8")
    check(result.returncode == 0, f"projection replay failed; see {destination.with_suffix('.log')}")
    records = [json.loads(line.split(PREFIX, 1)[1]) for line in log.splitlines()
               if PREFIX in line]
    check(len(records) == 1, "ignored replay test did not execute exactly once")
    measurement = records[0]
    check(destination.is_file(), "replay did not produce exact CLI text")
    raw = destination.read_bytes()
    text = raw.decode("utf-8")
    check(len(raw) == measurement["text_utf8_bytes"] and len(text) == measurement["text_chars"],
          "written CLI text disagrees with replay measurement")
    return measurement


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before-test-bin", required=True, type=Path,
                        help="preserved test binary built before production projection changes")
    parser.add_argument("--after-test-bin", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path,
                        help="new directory; exact text and ledger are never overwritten")
    parser.add_argument("captures", nargs="+", type=Path)
    args = parser.parse_args()
    binaries = {"before": args.before_test_bin.resolve(), "after": args.after_test_bin.resolve()}
    for name, binary in binaries.items():
        check(binary.is_file() and os.access(binary, os.X_OK), f"{name} test binary is not executable")
    binary_evidence = {name: {"path": str(path), "sha256": digest(path)}
                       for name, path in binaries.items()}
    captures = [path.resolve() for path in args.captures]
    metadata = [capture_metadata(path) for path in captures]
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rows = []
    for index, (capture, source) in enumerate(zip(captures, metadata)):
        measurements = {
            name: replay(binary, capture, output / f"{index:02d}_{source['tool']}.{name}.txt")
            for name, binary in binaries.items()
        }
        check(digest(capture) == source["sha256"], "native capture changed during replay")
        before, after = measurements["before"], measurements["after"]
        check(before["tool"] == after["tool"] == source["tool"], "projection changed canonical tool")
        check(before["is_error"] == after["is_error"], "projection changed failure semantics")
        row = {
            "tool": source["tool"], "capture": str(capture), "capture_evidence": source,
            **measurements,
            "text_utf8_bytes_delta": after["text_utf8_bytes"] - before["text_utf8_bytes"],
            "text_chars_delta": after["text_chars"] - before["text_chars"],
            "text_utf8_reduction_percent": (
                round(100 * (before["text_utf8_bytes"] - after["text_utf8_bytes"])
                      / before["text_utf8_bytes"], 2) if before["text_utf8_bytes"] else None),
        }
        rows.append(row)
        print(json.dumps(row, ensure_ascii=False, sort_keys=True))
    for name, binary in binaries.items():
        check(digest(binary) == binary_evidence[name]["sha256"], "test binary changed during replay")
    ledger = {
        "measurement": "same native Runtime output; two compiled CLI projections; no Host tokens or end-to-end timing",
        "test": TEST,
        "test_binaries": binary_evidence,
        "samples": rows,
    }
    (output / "measurements.json").write_text(
        json.dumps(ledger, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
