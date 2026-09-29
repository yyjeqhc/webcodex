#!/usr/bin/env python3
"""Run one CI command once; preserve its exit status and emit bounded test facts.

Raw child output goes to the normal CI log, never to the uploaded JSON report.
No retry, test selection, concurrency override or third-party dependency.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

ANSI = re.compile(r"\x1b\[[0-9;]*[A-Za-z]")
TEST = re.compile(r"^test ([A-Za-z0-9_:]+) \.\.\. (ok|FAILED|ignored)(?:,.*)?$")
SUMMARY = re.compile(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;.*finished in ([0-9.]+)s")
SIGNAL = re.compile(r"\(signal: (\d+), ([A-Z][A-Z0-9]+):")
COMPILE = re.compile(r"^\s*Finished `(?:test|dev)` profile .* in (.+)$")

class TestFacts:
    def __init__(self) -> None:
        self.tests: list[dict] = []
        self.summaries: list[dict] = []
        self.signals: list[dict] = []
        self.compile_error = False
        self.truncated = False
        self.build_durations: list[str] = []

    def feed(self, line: str) -> None:
        line = ANSI.sub("", line).strip()
        if match := TEST.fullmatch(line):
            if len(self.tests) < 20000:
                self.tests.append({"name": match[1], "status": match[2]})
            else:
                self.truncated = True
        if match := SUMMARY.match(line):
            if len(self.summaries) < 1000:
                self.summaries.append({"status": match[1], "passed": int(match[2]),
                    "failed": int(match[3]), "ignored": int(match[4]), "seconds": float(match[5])})
            else:
                self.truncated = True
        if match := SIGNAL.search(line):
            if len(self.signals) < 16:
                self.signals.append({"number": int(match[1]), "name": match[2]})
        if line.startswith("error: could not compile") or re.match(r"^error\[E\d+\]", line):
            self.compile_error = True
        if match := COMPILE.match(line):
            # Accept timing tokens only, never a path/command accidentally parsed
            # from an arbitrary compiler or fixture line.
            if re.fullmatch(r"[0-9.hms ]{1,40}", match[1]) and len(self.build_durations) < 100:
                self.build_durations.append(match[1])

    def report(self, code: int, seconds: float, metadata: dict) -> dict:
        failed = any(test["status"] == "FAILED" for test in self.tests) or any(s["failed"] for s in self.summaries)
        outcome = ("signal" if self.signals or code < 0 else "compile_failure" if self.compile_error
                   else "test_failure" if code and failed else "command_failure" if code
                   else "passed" if self.summaries else "no_test_summary")
        return {"version": 1, **metadata, "exit_code": code, "outcome": outcome,
            "elapsed_seconds": round(seconds, 3), "build_durations": self.build_durations,
            "suite_summaries": self.summaries, "tests": self.tests,
            "signals": self.signals, "truncated": self.truncated,
            "attribution": "completed_test_lines_do_not_identify_a_crash_culprit",
            "raw_output_included": False, "attempts_executed": 1}


def metadata() -> dict:
    result = {"platform": sys.platform, "architecture": platform.machine()}
    for key in ["GITHUB_SHA", "GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "CI_RUST_SHARD"]:
        value = os.environ.get(key, "")
        if re.fullmatch(r"[A-Za-z0-9_.-]{1,80}", value):
            result[key.lower()] = value
    return result


def run(command: list[str], report_path: Path) -> int:
    facts = TestFacts()
    start = time.monotonic()
    try:
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
    except OSError:
        code = 127  # do not put executable paths or environment in the report
    else:
        assert child.stdout is not None
        with child.stdout:
            while line := child.stdout.readline(65536):
                sys.stdout.write(line)
                sys.stdout.flush()
                facts.feed(line)
        code = child.wait()
    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(facts.report(code, time.monotonic() - start, metadata()),
        ensure_ascii=True, indent=2) + "\n", encoding="utf-8")
    return code if code >= 0 else 128 - code


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("one explicit command is required")
    return run(command, args.report)

if __name__ == "__main__":
    raise SystemExit(main())
