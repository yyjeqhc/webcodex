#!/usr/bin/env python3
"""Install the development Chrome Native Messaging manifest for the current user.

This does not load/enable an extension, alter Chrome preferences, restart Chrome,
copy a Chrome profile, or grant debugger consent. Load the unpacked extension and
share a tab using Chrome's own UI after running this explicitly.
"""
from __future__ import annotations
import argparse
import hashlib
import base64
import json
import os
from pathlib import Path
import stat
import sys
import tempfile

HOST = 'com.webcodex.browser_bridge'
EXPECTED_ID = 'pjhnlafbcbgcgjpkfiomhjnhpnaohgcg'

def manifest_for(binary: Path, extension: Path) -> dict:
    metadata = json.loads((extension / 'manifest.json').read_text(encoding='utf-8'))
    digest = hashlib.sha256(base64.b64decode(metadata['key'], validate=True)).hexdigest()[:32]
    extension_id = ''.join(chr(ord('a') + int(n, 16)) for n in digest)
    if extension_id != EXPECTED_ID:
        raise ValueError('extension identity does not match the built-in native caller')
    binary = binary.resolve(strict=True)
    info = binary.stat()
    if not stat.S_ISREG(info.st_mode) or (os.name != 'nt' and (info.st_uid != os.getuid() or info.st_mode & 0o022)):
        raise ValueError('native host binary must be a regular, current-user-owned, non-writable-by-others file')
    return {'name': HOST, 'description': 'WebCodex explicit local Chrome tab bridge',
            'path': str(binary), 'type': 'stdio',
            'allowed_origins': [f'chrome-extension://{extension_id}/']}

def destination() -> Path:
    home = Path.home()
    if sys.platform == 'darwin':
        return home / 'Library/Application Support/Google/Chrome/NativeMessagingHosts' / f'{HOST}.json'
    if os.name == 'nt':
        return Path(os.environ['LOCALAPPDATA']) / 'webcodex/native-messaging' / f'{HOST}.json'
    return home / '.config/google-chrome/NativeMessagingHosts' / f'{HOST}.json'

def install(binary: Path, extension: Path) -> Path:
    data = manifest_for(binary, extension)
    target = destination()
    for ancestor in target.parents:
        if ancestor.is_symlink():
            raise ValueError('native host installation directory must not use symbolic links')
    target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if target.is_symlink():
        raise ValueError('native host manifest must not be a symbolic link')
    content = json.dumps(data, indent=2) + '\n'
    if target.exists() and target.read_text(encoding='utf-8') != content:
        backup = target.with_suffix('.json.previous')
        if backup.exists():
            raise ValueError('inspect the existing native host backup before replacing the manifest')
        backup.write_bytes(target.read_bytes())
        if os.name != 'nt': backup.chmod(0o600)
    fd, temporary = tempfile.mkstemp(prefix='.webcodex-native-', dir=target.parent)
    try:
        with os.fdopen(fd, 'w', encoding='utf-8') as output:
            output.write(content); output.flush(); os.fsync(output.fileno())
        os.replace(temporary, target)
    finally:
        Path(temporary).unlink(missing_ok=True)
    if os.name == 'nt':
        import winreg
        with winreg.CreateKey(winreg.HKEY_CURRENT_USER, rf'Software\Google\Chrome\NativeMessagingHosts\{HOST}') as key:
            winreg.SetValueEx(key, '', 0, winreg.REG_SZ, str(target))
    return target

def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=Path, help='Exact built webcodex-browser-bridge executable')
    parser.add_argument('--extension', type=Path, default=Path(__file__).resolve().parents[1] / 'extensions/browser-bridge')
    parser.add_argument('--check', action='store_true', help='Validate without changing the installed native manifest')
    args = parser.parse_args()
    try:
        if args.check:
            manifest_for(args.binary, args.extension)
            print('Native host binary and extension identity are valid; installation unchanged.')
        else:
            print(f'Installed current-user native manifest: {install(args.binary, args.extension)}')
            print(f'Load unpacked extension in Chrome: {args.extension.resolve()}')
            print('No Chrome profile or running Chrome process was changed.')
    except (OSError, KeyError, ValueError) as error:
        print(f'Browser bridge installation failed: {error}', file=sys.stderr)
        return 1
    return 0
if __name__ == '__main__':
    raise SystemExit(main())
