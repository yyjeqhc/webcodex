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
        self.assertIn("usage: standalone.py", completed.stderr)


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


if __name__ == "__main__":
    unittest.main()
