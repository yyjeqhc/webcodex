#!/usr/bin/env python3
"""Compatibility wrapper for the real desktop worker.

The demo command now delegates to scripts/desktop_worker.py in --once --dry-run
mode so existing docs/e2e keep working while the real worker supports a loop,
screenshot upload, and safe built-in desktop actions.
"""

from __future__ import annotations

import runpy
import sys
from pathlib import Path

worker = Path(__file__).with_name("desktop_worker.py")
sys.argv = [str(worker), "--once", "--dry-run", "--no-screenshot", *sys.argv[1:]]
runpy.run_path(str(worker), run_name="__main__")
