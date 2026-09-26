from __future__ import annotations

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "verify_linux_elf_baseline.py"
SPEC = importlib.util.spec_from_file_location("verify_linux_elf_baseline", SCRIPT)
verify_elf = importlib.util.module_from_spec(SPEC)
assert SPEC and SPEC.loader
SPEC.loader.exec_module(verify_elf)


class LinuxElfBaselineTests(unittest.TestCase):
    def run_mocked(self, glibc: str = "2.35", ldd: str = "libc.so.6 => /lib/libc.so.6 (0x0)\n"):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        binary = Path(temp.name) / "binary"
        binary.write_bytes(b"ELF fixture")

        def run(args, **kwargs):
            command = args[0]
            if command == "readelf" and args[1] == "-h":
                return subprocess.CompletedProcess(args, 0, "  Machine:                           Advanced Micro Devices X86-64\n", "")
            if command == "readelf" and args[1] == "--version-info":
                return subprocess.CompletedProcess(args, 0, f"  Name: GLIBC_{glibc}\n", "")
            if command == "ldd":
                return subprocess.CompletedProcess(args, 0 if "not found" not in ldd else 1, ldd, "")
            raise AssertionError(args)

        return binary, run

    def test_accepts_glibc_235_and_resolved_native_dependencies(self):
        binary, run = self.run_mocked()
        with mock.patch.object(verify_elf.subprocess, "run", side_effect=run):
            report = verify_elf.verify(binary, "x86_64", "2.35")
        self.assertEqual(report["maximum_glibc"], "2.35")

    def test_rejects_glibc_newer_than_ubuntu_2204_baseline(self):
        binary, run = self.run_mocked(glibc="2.36")
        with mock.patch.object(verify_elf.subprocess, "run", side_effect=run):
            with self.assertRaisesRegex(verify_elf.VerificationError, "requires GLIBC_2.36"):
                verify_elf.verify(binary, "x86_64", "2.35")

    def test_rejects_unresolved_dynamic_dependency(self):
        binary, run = self.run_mocked(ldd="libwebkit2gtk-4.1.so.0 => not found\n")
        with mock.patch.object(verify_elf.subprocess, "run", side_effect=run):
            with self.assertRaisesRegex(verify_elf.VerificationError, "unresolved runtime dependency"):
                verify_elf.verify(binary, "x86_64", "2.35")

    def test_release_linux_package_compiles_and_probes_in_ubuntu_2204_container(self):
        workflow = Path(".github/workflows/release-build.yml").read_text(encoding="utf-8")
        start = workflow.index("- name: Build and package Linux installer in Ubuntu 22.04 native container")
        end = workflow.index("- name: Verify native host and prepare exact runtime inputs", start)
        linux_step = workflow[start:end]
        self.assertIn("ubuntu:22.04 bash -lc", linux_step)
        self.assertIn("Ubuntu 22.04 container is not native", linux_step)
        self.assertIn("verify_linux_elf_baseline.py", linux_step)
        self.assertIn("collect_unified_installer_inputs.py", linux_step)
        self.assertIn("libwebkit2gtk-4.1-dev", linux_step)
        self.assertIn("if: matrix.os == 'macos'", workflow[end:])


if __name__ == "__main__":
    unittest.main()
