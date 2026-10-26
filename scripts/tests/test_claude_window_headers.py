"""Credential-free fixtures for the optional Claude headersHelper."""

import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

from scripts import claude_window_headers as helper


def stat_line(pid, parent, start, comm="python"):
    fields = ["S", str(parent)] + ["0"] * 17 + [str(start)]
    return "{} ({}) {}\n".format(pid, comm, " ".join(fields))


class ClaudeWindowHeadersTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.proc = self.root / "proc"
        self.machine = self.root / "machine-id"
        self.machine.write_text("0123456789abcdef0123456789abcdef\n", encoding="ascii")
        self.boot = self.proc / "sys/kernel/random/boot_id"
        self.boot.parent.mkdir(parents=True)
        self.boot.write_text("12345678-1234-4321-9876-123456789abc\n", encoding="ascii")
        self.process(300, 200, 30, "python")
        self.process(200, 100, 20, "shell (with) ) name")
        self.process(100, 1, 10, "claude")

    def process(self, pid, parent, start, comm="python"):
        path = self.proc / str(pid) / "stat"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(stat_line(pid, parent, start, comm), encoding="ascii")
        return path

    def headers(self, **kwargs):
        options = {
            "environment": {},
            "platform": "linux",
            "proc_root": self.proc,
            "machine_id_paths": (self.machine,),
            "start_pid": 300,
            "readlink": lambda path: "/opt/claude",
        }
        options.update(kwargs)
        return helper.window_headers(**options)

    def test_explicit_identity_is_exact_and_platform_independent(self):
        for platform in ("win32", "darwin", "linux"):
            for value in ("x", "Claude-window:001_test.value", "a" * 256):
                with self.subTest(platform=platform, value=value):
                    self.assertEqual(self.headers(
                        environment={helper.EXPLICIT_ID_ENV: value}, platform=platform
                    ), {helper.HEADER: value})

    def test_invalid_explicit_identity_never_falls_back(self):
        values = ("", "a" * 257, " leading", "trailing ", "new\nline",
                  "界", "a/b", "a,b", "a=b", "a\t", "a\x00")
        with mock.patch.object(helper, "_linux_process_id") as detect:
            for value in values:
                with self.subTest(value=value):
                    self.assertEqual(self.headers(environment={helper.EXPLICIT_ID_ENV: value}), {})
            detect.assert_not_called()

    def test_unsupported_platform_without_explicit_identity_is_empty(self):
        with mock.patch.object(helper, "_linux_process_id") as detect:
            for platform in ("win32", "darwin", "freebsd"):
                self.assertEqual(self.headers(platform=platform), {})
            detect.assert_not_called()

    def test_same_claude_process_survives_helper_and_shell_reconnection(self):
        first = self.headers()
        self.process(301, 201, 31)
        self.process(201, 100, 21, "sh")
        second = self.headers(start_pid=301)
        self.assertEqual(first, second)
        value = first[helper.HEADER]
        self.assertRegex(value, r"^claude-process-[0-9a-f]{64}$")
        self.assertNotIn(self.machine.read_text().strip(), value)
        self.assertNotIn(self.boot.read_text().strip(), value)

    def test_new_process_or_pid_reuse_changes_identity(self):
        first = self.headers()
        self.process(100, 1, 11, "claude")
        reused_pid = self.headers()
        self.assertNotEqual(first, reused_pid)
        self.process(101, 1, 11, "claude")
        self.process(200, 101, 20, "sh")
        self.assertNotEqual(reused_pid, self.headers())

    def test_boot_and_machine_identity_separate_processes(self):
        first = self.headers()
        self.boot.write_text("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee\n", encoding="ascii")
        rebooted = self.headers()
        self.assertNotEqual(first, rebooted)
        self.machine.write_text("f" * 32 + "\n", encoding="ascii")
        self.assertNotEqual(rebooted, self.headers())

    def test_native_executable_names_and_versioned_install(self):
        for executable in (
            "/usr/local/bin/claude", "/opt/claude-code",
            "/home/test/.local/share/claude/versions/2.1.10",
            "/home/test/.local/share/claude/versions/2.1.10-beta.2",
            "/opt/claude (deleted)",
        ):
            with self.subTest(executable=executable):
                self.assertIn(helper.HEADER, self.headers(readlink=lambda path: executable))

    def test_ambiguous_executable_and_runtime_wrapper_fail_closed(self):
        for executable in (
            "/usr/bin/node", "/usr/bin/bun", "/usr/bin/not-claude", "claude",
            "/tmp/versions/2.1.10", "/tmp/claude/versions/not-a-version",
            "/" + "c" * (helper.MAX_EXE_BYTES + 1),
        ):
            with self.subTest(executable=executable):
                self.assertEqual(self.headers(readlink=lambda path: executable), {})
        for runtime in ("node", "nodejs", "bun"):
            self.process(200, 100, 20, runtime)
            self.assertEqual(self.headers(), {})

    def test_parent_stat_with_parentheses_is_parsed_correctly(self):
        stamp = helper._read_process(self.proc, 200)
        self.assertEqual(stamp.parent_pid, 100)
        self.assertEqual(stamp.start_ticks, 20)
        self.assertEqual(stamp.comm, b"shell (with) ) name")
        self.assertIn(helper.HEADER, self.headers())

    def test_incomplete_or_malformed_stat_is_rejected(self):
        path = self.proc / "200/stat"
        for value in (
            b"", b"200 (sh) S 100\n",
            stat_line(200, 100, 20).encode().rstrip(b"\n"),
            stat_line(999, 100, 20).encode(),
            stat_line(200, -1, 20).encode(),
            stat_line(200, 100, -1).encode(),
            stat_line(200, 100, 2**64).encode(),
            stat_line(200, 100, "invalid").encode(),
            b"x" * (helper.MAX_STAT_BYTES + 1),
        ):
            with self.subTest(value=value[:80]):
                path.write_bytes(value)
                self.assertEqual(self.headers(), {})

    def test_missing_process_and_parent_cycle_are_empty(self):
        (self.proc / "200/stat").unlink()
        self.assertEqual(self.headers(), {})
        self.process(200, 300, 20, "sh")
        self.assertEqual(self.headers(), {})
        self.process(200, 200, 20, "sh")
        self.assertEqual(self.headers(), {})

    def test_parent_walk_is_bounded(self):
        for pid in range(1000, 1000 + helper.MAX_PARENT_DEPTH):
            self.process(pid, pid + 1, pid, "sh")
        self.process(1000 + helper.MAX_PARENT_DEPTH, 1, 4, "claude")
        self.assertEqual(self.headers(start_pid=1000), {})

    def test_absent_claude_ancestor_does_not_use_constant_identity(self):
        self.process(100, 0, 10, "init")
        self.assertEqual(self.headers(), {})

    def test_unreadable_executable_is_empty(self):
        def denied(path):
            raise PermissionError("private diagnostic must not escape")
        self.assertEqual(self.headers(readlink=denied), {})

    def test_pid_reuse_during_walk_is_rejected(self):
        def replace_process(path):
            self.process(100, 1, 999, "claude")
            return "/opt/claude"
        self.assertEqual(self.headers(readlink=replace_process), {})

    def test_parent_change_during_walk_is_rejected(self):
        def reparent(path):
            self.process(200, 99, 20, "sh")
            return "/opt/claude"
        self.assertEqual(self.headers(readlink=reparent), {})

    def test_process_exit_during_walk_is_rejected(self):
        def exited(path):
            (self.proc / "100/stat").unlink()
            return "/opt/claude"
        self.assertEqual(self.headers(readlink=exited), {})

    def test_executable_change_during_walk_is_rejected(self):
        links = iter(("/opt/claude", "/other/claude"))
        self.assertEqual(self.headers(readlink=lambda path: next(links)), {})

    def test_boot_change_during_walk_is_rejected(self):
        def rebooted(path):
            self.boot.write_text("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee\n", encoding="ascii")
            return "/opt/claude"
        self.assertEqual(self.headers(readlink=rebooted), {})

    def test_machine_change_during_walk_is_rejected(self):
        def changed(path):
            self.machine.write_text("f" * 32 + "\n", encoding="ascii")
            return "/opt/claude"
        self.assertEqual(self.headers(readlink=changed), {})

    def test_missing_machine_id_uses_only_documented_fallback(self):
        missing = self.root / "missing"
        self.assertIn(helper.HEADER, self.headers(machine_id_paths=(missing, self.machine)))
        self.assertEqual(self.headers(machine_id_paths=(missing,)), {})
        self.machine.write_text("malformed\n", encoding="ascii")
        self.assertEqual(self.headers(), {})

    def test_invalid_boot_and_oversized_identity_files_are_empty(self):
        for value in ("", "not-a-uuid", "x" * 129):
            self.boot.write_text(value, encoding="ascii")
            self.assertEqual(self.headers(), {})

    def test_main_failure_emits_only_empty_json(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with mock.patch.object(helper, "window_headers", side_effect=RuntimeError("private-value")):
            with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                helper.main()
        self.assertEqual(stdout.getvalue(), "{}\n")
        self.assertEqual(stderr.getvalue(), "")

    def test_cli_stdout_is_one_json_object_without_diagnostics(self):
        script = Path(helper.__file__).resolve()
        # Windows Python needs SystemRoot for its own random-number startup.
        # Keep the child credential-free instead of copying the parent environment.
        runtime_environment = {
            name: os.environ[name] for name in ("SystemRoot",) if name in os.environ
        }
        for value in ("fixture-window-0123456789abcdef", "invalid value"):
            completed = subprocess.run(
                [sys.executable, str(script)],
                env={**runtime_environment, helper.EXPLICIT_ID_ENV: value},
                capture_output=True, text=True, timeout=10, check=True,
            )
            expected = {helper.HEADER: value} if " " not in value else {}
            self.assertEqual(json.loads(completed.stdout), expected)
            self.assertEqual(len(completed.stdout.splitlines()), 1)
            self.assertEqual(completed.stderr, "")


if __name__ == "__main__":
    unittest.main()
