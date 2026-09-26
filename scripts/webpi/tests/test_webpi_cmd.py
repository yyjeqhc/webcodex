from __future__ import annotations

import os
import subprocess
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CMD = ROOT / "webpi.cmd"


@unittest.skipUnless(os.name == "nt", "webpi.cmd is a Windows launcher")
class WebPiCmdTests(unittest.TestCase):
    def test_failed_python_command_propagates_nonzero_exit_code(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(CMD), "definitely-not-a-webpi-command"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=30,
            shell=False,
        )
        self.assertNotEqual(completed.returncode, 0)
        self.assertIn("usage: webpi.cmd", completed.stderr)
        self.assertNotIn("standalone.py", completed.stderr)


    def test_help_is_product_facing_and_guides_common_workflows(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(CMD), "--help"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=30,
            shell=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertIn("WebPi local runtime", completed.stdout)
        self.assertIn("webpi.cmd doctor", completed.stdout)
        self.assertIn("webpi.cmd run", completed.stdout)
        self.assertIn("webpi.cmd verify", completed.stdout)
        self.assertIn("webpi.cmd pi", completed.stdout)
        self.assertNotIn("standalone.py", completed.stdout)

    def test_pi_admin_trust_status_is_reachable_from_launcher(self) -> None:
        completed = subprocess.run(
            ["cmd.exe", "/d", "/c", str(CMD), "pi", "trust-status"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            timeout=30,
            shell=False,
        )
        self.assertEqual(completed.returncode, 0, completed.stderr)
        payload = __import__("json").loads(completed.stdout)
        self.assertIn("trusted", payload)
        self.assertIsInstance(payload["trusted"], bool)

    def test_windows_kill_on_close_job_reaps_assigned_child(self) -> None:
        from scripts.webpi import standalone
        import time

        child = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(60)"],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
        )
        containment = standalone._create_process_containment()
        try:
            containment.assign(child)
            containment.close()
            deadline = time.monotonic() + 5
            while child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.05)
            self.assertIsNotNone(child.poll(), "closing the supervisor job must terminate assigned children")
        finally:
            containment.close()
            if child.poll() is None:
                child.kill()
                child.wait(timeout=5)

    def test_windows_contained_spawn_reaps_startup_descendant(self) -> None:
        from scripts.webpi import standalone
        import ctypes
        import tempfile
        import time

        containment = standalone._create_process_containment()
        parent: subprocess.Popen[object] | None = None
        descendant_pid: int | None = None
        try:
            with tempfile.TemporaryDirectory() as temp_dir:
                pid_file = Path(temp_dir) / "descendant.pid"
                code = (
                    "import subprocess,sys,time; "
                    "c=subprocess.Popen([sys.executable,'-c','import time; time.sleep(60)']); "
                    "open(sys.argv[1],'w',encoding='utf-8').write(str(c.pid)); "
                    "time.sleep(60)"
                )
                parent = containment.popen(
                    [sys.executable, "-c", code, str(pid_file)],
                    cwd=ROOT,
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    creationflags=subprocess.CREATE_NEW_PROCESS_GROUP,
                )
                deadline = time.monotonic() + 5
                while not pid_file.exists() and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertTrue(pid_file.exists(), "contained child must be resumed after Job assignment")
                descendant_pid = int(pid_file.read_text(encoding="utf-8"))

                containment.close()
                deadline = time.monotonic() + 5
                while parent.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.05)
                self.assertIsNotNone(parent.poll(), "closing the Job must terminate the direct child")

                kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
                open_process = kernel32.OpenProcess
                open_process.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_ulong]
                open_process.restype = ctypes.c_void_p
                wait_for_single = kernel32.WaitForSingleObject
                wait_for_single.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
                wait_for_single.restype = ctypes.c_ulong
                descendant_stopped = False
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    handle = open_process(0x00100000, False, descendant_pid)  # SYNCHRONIZE
                    if not handle:
                        descendant_stopped = True
                        break
                    try:
                        if wait_for_single(handle, 0) == 0:  # WAIT_OBJECT_0
                            descendant_stopped = True
                            break
                    finally:
                        kernel32.CloseHandle(handle)
                    time.sleep(0.05)
                self.assertTrue(descendant_stopped, "Job containment must include descendants spawned at startup")
        finally:
            containment.close()
            if parent is not None and parent.poll() is None:
                parent.kill()
                parent.wait(timeout=5)
            if descendant_pid is not None:
                subprocess.run(
                    [r"C:\Windows\System32\taskkill.exe", "/PID", str(descendant_pid), "/F"],
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    check=False,
                )


if __name__ == "__main__":
    unittest.main()
