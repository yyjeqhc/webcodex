import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('bridge_installer', Path(__file__).resolve().parents[1] / 'install_browser_bridge.py')
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class BackupTests(unittest.TestCase):
    def test_history_is_preserved_and_identical_backup_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory).resolve() / 'host.json'
            legacy = target.with_suffix('.json.previous')
            legacy.write_bytes(b'oldest')
            first = installer.preserve_backup(target, b'first')
            second = installer.preserve_backup(target, b'second')
            self.assertEqual(installer.preserve_backup(target, b'first'), first)
            self.assertEqual(first.read_bytes(), b'first')
            self.assertEqual(second.read_bytes(), b'second')
            self.assertEqual(legacy.read_bytes(), b'oldest')

    def test_conflicting_backup_and_symlink_fail_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory).resolve() / 'host.json'
            backup = installer.preserve_backup(target, b'first')
            backup.write_bytes(b'other')
            with self.assertRaises(ValueError):
                installer.preserve_backup(target, b'first')
            backup.unlink()
            target.write_bytes(b'first')
            backup.symlink_to(target)
            with self.assertRaises(ValueError):
                installer.preserve_backup(target, b'first')
            self.assertEqual(target.read_bytes(), b'first')

    def test_failed_backup_never_replaces_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory).resolve() / 'host.json'
            target.write_bytes(b'old manifest')
            with patch.object(installer, 'destination', return_value=target), patch.object(installer, 'manifest_for', return_value={'name':'fixture'}), patch.object(installer.os, 'fsync', side_effect=OSError('fixture')):
                with self.assertRaises(OSError):
                    installer.install(Path('binary'), Path('extension'))
            self.assertEqual(target.read_bytes(), b'old manifest')
            self.assertEqual(list(target.parent.glob('*.previous.*')), [])

    def test_oversized_backup_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                installer.preserve_backup(Path(directory).resolve() / 'host.json', b'x' * 65537)
            self.assertEqual(list(Path(directory).iterdir()), [])

if __name__ == '__main__':
    unittest.main()
