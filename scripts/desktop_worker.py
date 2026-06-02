#!/usr/bin/env python3
"""Private Drop desktop worker.

This worker is designed to run in a logged-in desktop session on Windows/macOS/Linux.
It talks to Private Drop only through the public desktop task API:

1. claim_next pending task
2. execute a small, safe built-in action set
3. optionally capture and upload a screenshot
4. report completed/failed/needs_input back to the task

It intentionally does not run arbitrary shell from task instructions. The first
prototype action is URL opening, which is enough to demonstrate: "say a task in
ChatGPT/Web -> a desktop opens the requested page -> screenshot/status returns".
"""

from __future__ import annotations

import argparse
import json
import mimetypes
import os
import platform
import re
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

TASK_POLL_PATH = "/api/desktop/tasks/claim_next"
TASK_EVENT_PATH = "/api/desktop/tasks/{task_id}/event"
FILE_UPLOAD_PATH = "/api/files?channel=desktop"

URL_RE = re.compile(r"https?://[^\s'\"<>]+", re.IGNORECASE)
TEXT_RE = re.compile(r"(?ims)^(?:type|text|message)\s*:\s*(.+?)(?=\n(?:open|url|press_enter|send|wait|type|text|message|wechat_to|wechat_message|wechat_send)\s*:|\Z)")
WECHAT_TO_RE = re.compile(r"(?im)^wechat_to\s*:\s*(.+?)\s*$")
WECHAT_MESSAGE_RE = re.compile(r"(?ims)^wechat_message\s*:\s*(.+?)(?=\n(?:wechat_send|wechat_to|open|url|press_enter|send|wait|type|text|message)\s*:|\Z)")


@dataclass
class WorkerConfig:
    base: str
    token: str
    worker: str
    poll_interval: float
    once: bool
    idle_heartbeat: int
    screenshot: bool
    screenshot_delay: float
    dry_run: bool
    max_tasks: int | None


class ApiError(RuntimeError):
    pass


def request_json(
    base: str,
    token: str,
    method: str,
    path: str,
    body: dict[str, Any] | None = None,
) -> dict[str, Any]:
    data = None if body is None else json.dumps(body).encode("utf-8")
    req = urllib.request.Request(
        base.rstrip("/") + path,
        data=data,
        method=method,
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
            "User-Agent": "private-drop-desktop-worker/0.1",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=45) as resp:
            return json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise ApiError(f"HTTP {exc.code} for {path}: {detail}") from exc
    except urllib.error.URLError as exc:
        raise ApiError(f"Network error for {path}: {exc}") from exc


def upload_file(base: str, token: str, path: Path) -> str:
    boundary = f"----private-drop-{uuid4().hex}"
    mime = mimetypes.guess_type(str(path))[0] or "application/octet-stream"
    filename = path.name
    body_prefix = (
        f"--{boundary}\r\n"
        f'Content-Disposition: form-data; name="file"; filename="{filename}"\r\n'
        f"Content-Type: {mime}\r\n\r\n"
    ).encode("utf-8")
    body_suffix = f"\r\n--{boundary}--\r\n".encode("utf-8")
    data = body_prefix + path.read_bytes() + body_suffix
    req = urllib.request.Request(
        base.rstrip("/") + FILE_UPLOAD_PATH,
        data=data,
        method="POST",
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": f"multipart/form-data; boundary={boundary}",
            "Content-Length": str(len(data)),
            "User-Agent": "private-drop-desktop-worker/0.1",
        },
    )
    try:
        with urllib.request.urlopen(req, timeout=60) as resp:
            payload = json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise ApiError(f"HTTP {exc.code} uploading screenshot: {detail}") from exc
    file_id = payload.get("id")
    if not file_id:
        raise ApiError(f"Upload response missing id: {payload}")
    return base.rstrip("/") + f"/api/files/{file_id}"


def claim_next(config: WorkerConfig) -> dict[str, Any] | None:
    payload = request_json(
        config.base,
        config.token,
        "POST",
        TASK_POLL_PATH,
        {"worker": config.worker},
    )
    if not payload.get("success"):
        raise ApiError(f"claim_next failed: {payload}")
    task = payload.get("task")
    if task is None:
        return None
    return task


def update_task(
    config: WorkerConfig,
    task_id: str,
    status: str,
    message: str,
    screenshot_url: str | None = None,
) -> None:
    body: dict[str, Any] = {
        "status": status,
        "worker": config.worker,
        "message": message,
    }
    if screenshot_url:
        body["screenshot_url"] = screenshot_url
    payload = request_json(
        config.base,
        config.token,
        "POST",
        TASK_EVENT_PATH.format(task_id=urllib.parse.quote(task_id, safe="")),
        body,
    )
    if not payload.get("success"):
        raise ApiError(f"task event update failed: {payload}")


def extract_url(instructions: str) -> str | None:
    match = URL_RE.search(instructions)
    if not match:
        return None
    url = match.group(0).rstrip(".,;)")
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        return None
    return url


def open_url(url: str, dry_run: bool) -> str:
    if dry_run:
        return f"dry-run: would open {url}"
    system = platform.system().lower()
    if system == "windows":
        os.startfile(url)  # type: ignore[attr-defined]
    elif system == "darwin":
        subprocess.run(["open", url], check=True)
    else:
        subprocess.run(["xdg-open", url], check=True)
    return f"opened {url}"


def extract_text_to_type(instructions: str) -> str | None:
    match = TEXT_RE.search(instructions)
    if not match:
        return None
    text = match.group(1).strip()
    return text or None


def should_press_enter(instructions: str) -> bool:
    lowered = instructions.lower()
    return any(token in lowered for token in ["press_enter: true", "send: true", "press enter", "hit enter"])


def paste_text(text: str, press_enter: bool, dry_run: bool) -> str:
    if dry_run:
        suffix = " and press Enter" if press_enter else ""
        return f"dry-run: would paste {len(text)} chars{suffix}"
    system = platform.system().lower()
    if system == "windows":
        windows_paste_text(text, press_enter)
    elif system == "darwin":
        subprocess.run(["pbcopy"], input=text.encode("utf-8"), check=True)
        subprocess.run(["osascript", "-e", 'tell application "System Events" to keystroke "v" using command down'], check=True)
        if press_enter:
            subprocess.run(["osascript", "-e", 'tell application "System Events" to key code 36'], check=True)
    else:
        subprocess.run(["sh", "-lc", "command -v xclip >/dev/null && xclip -selection clipboard || xsel -ib"], input=text.encode("utf-8"), check=True)
        subprocess.run(["xdotool", "key", "ctrl+v"], check=True)
        if press_enter:
            subprocess.run(["xdotool", "key", "Return"], check=True)
    suffix = " and pressed Enter" if press_enter else ""
    return f"pasted {len(text)} chars{suffix}"


def windows_paste_text(text: str, press_enter: bool) -> None:
    ps = r"""
param([string]$Text,[bool]$PressEnter)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Clipboard]::SetText($Text)
Start-Sleep -Milliseconds 250
[System.Windows.Forms.SendKeys]::SendWait('^v')
if ($PressEnter) { Start-Sleep -Milliseconds 100; [System.Windows.Forms.SendKeys]::SendWait('{ENTER}') }
"""
    subprocess.run(
        ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps, "-Text", text, "-PressEnter", str(press_enter).lower()],
        check=True,
    )


def extract_wechat_action(instructions: str) -> tuple[str, str, bool] | None:
    to_match = WECHAT_TO_RE.search(instructions)
    msg_match = WECHAT_MESSAGE_RE.search(instructions)
    if not to_match or not msg_match:
        return None
    contact = to_match.group(1).strip()
    message = msg_match.group(1).strip()
    if not contact or not message:
        return None
    send = any(token in instructions.lower() for token in ["wechat_send: true", "send: true"])
    return contact, message, send


def send_wechat_message(contact: str, message: str, send: bool, dry_run: bool) -> str:
    allow_send = os.environ.get("WECHAT_ALLOW_SEND", "false").lower() in {"1", "true", "yes"}
    effective_send = send and allow_send
    search_x = int(os.environ.get("WECHAT_SEARCH_X", "145"))
    search_y = int(os.environ.get("WECHAT_SEARCH_Y", "45"))
    if dry_run:
        suffix = " and send" if effective_send else " as draft"
        return f"dry-run: would draft WeChat message to {contact}{suffix}; click search at {search_x},{search_y}"
    if platform.system().lower() != "windows":
        raise RuntimeError("wechat action currently supports Windows workers only")
    app_path = os.environ.get("WECHAT_EXE_PATH")
    ps = r"""
param([string]$Contact,[string]$Message,[bool]$Send,[string]$AppPath,[int]$SearchX,[int]$SearchY)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
public class NativeWin32 {
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
}
"@
function Click-Point([int]$x,[int]$y) {
  [NativeWin32]::SetCursorPos($x,$y) | Out-Null
  Start-Sleep -Milliseconds 80
  [NativeWin32]::mouse_event(0x0002,0,0,0,[UIntPtr]::Zero)
  Start-Sleep -Milliseconds 80
  [NativeWin32]::mouse_event(0x0004,0,0,0,[UIntPtr]::Zero)
}
$ws = New-Object -ComObject WScript.Shell
$activated = $ws.AppActivate('微信') -or $ws.AppActivate('WeChat')
if (-not $activated -and $AppPath) {
  Start-Process $AppPath
  Start-Sleep -Seconds 3
  $activated = $ws.AppActivate('微信') -or $ws.AppActivate('WeChat')
}
if (-not $activated) {
  throw 'WeChat window is not active. Open WeChat manually or set WECHAT_EXE_PATH; refusing to type into an unknown focused window.'
}
Start-Sleep -Milliseconds 700
$hwnd = [NativeWin32]::GetForegroundWindow()
$rect = New-Object RECT
if (-not [NativeWin32]::GetWindowRect($hwnd, [ref]$rect)) {
  throw 'Cannot read WeChat window rectangle; refusing to type.'
}
# Click the contact search box in the left sidebar. Coordinates are relative to the WeChat window.
Click-Point ($rect.Left + $SearchX) ($rect.Top + $SearchY)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.Clipboard]::SetText($Contact)
[System.Windows.Forms.SendKeys]::SendWait('^a')
Start-Sleep -Milliseconds 100
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Seconds 1
[System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
Start-Sleep -Seconds 1
[System.Windows.Forms.Clipboard]::SetText($Message)
[System.Windows.Forms.SendKeys]::SendWait('^v')
if ($Send) { Start-Sleep -Milliseconds 300; [System.Windows.Forms.SendKeys]::SendWait('{ENTER}') }
"""
    subprocess.run(
        [
            "powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps,
            "-Contact", contact, "-Message", message, "-Send", str(effective_send).lower(),
            "-AppPath", app_path or "", "-SearchX", str(search_x), "-SearchY", str(search_y),
        ],
        check=True,
    )
    if send and not allow_send:
        return f"wechat message drafted to {contact}; auto-send blocked by WECHAT_ALLOW_SEND=false"
    suffix = "sent" if effective_send else "drafted"
    return f"wechat message {suffix} to {contact}"


def capture_screenshot(enabled: bool, delay: float) -> Path | None:
    if not enabled:
        return None
    if delay > 0:
        time.sleep(delay)
    out = Path(tempfile.gettempdir()) / f"private-drop-screenshot-{uuid4().hex}.png"
    system = platform.system().lower()
    try:
        if system == "windows":
            ps = f"""
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
$graphics = [System.Drawing.Graphics]::FromImage($bmp)
$graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
$bmp.Save('{str(out)}', [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bmp.Dispose()
"""
            subprocess.run(
                ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
        elif system == "darwin":
            subprocess.run(["screencapture", "-x", str(out)], check=True)
        else:
            # Optional Linux fallback when ImageMagick is installed.
            subprocess.run(["import", "-window", "root", str(out)], check=True)
    except Exception:
        return None
    return out if out.exists() and out.stat().st_size > 0 else None


def execute_task(config: WorkerConfig, task: dict[str, Any]) -> tuple[str, str, Path | None]:
    title = str(task.get("title") or "")
    instructions = str(task.get("instructions") or "")
    combined = f"{title}\n{instructions}"
    wechat_action = extract_wechat_action(combined)
    url = extract_url(combined)
    text_to_type = extract_text_to_type(combined)
    press_enter = should_press_enter(combined)
    messages: list[str] = []

    if wechat_action:
        contact, wechat_message, wechat_send = wechat_action
        messages.append(send_wechat_message(contact, wechat_message, wechat_send, config.dry_run))
    else:
        if url:
            messages.append(open_url(url, config.dry_run))
            if not config.dry_run:
                time.sleep(max(config.screenshot_delay, 1.0))
        if text_to_type:
            messages.append(paste_text(text_to_type, press_enter, config.dry_run))

    if not messages:
        messages.append(
            "No safe built-in action matched. Add an http(s) URL, type:/text:/message: block, or wechat_to:/wechat_message:."
        )
        return "needs_input", "; ".join(messages), None

    screenshot_path = capture_screenshot(config.screenshot and not config.dry_run, config.screenshot_delay)
    if screenshot_path:
        messages.append(f"captured screenshot {screenshot_path.name}")
    elif config.screenshot and not config.dry_run:
        messages.append("screenshot capture unavailable")
    return "completed", "; ".join(messages), screenshot_path


def run_once(config: WorkerConfig) -> bool:
    task = claim_next(config)
    if task is None:
        return False
    task_id = str(task["id"])
    print(f"Claimed task: {task_id} - {task.get('title', '')}", flush=True)
    try:
        status, message, screenshot_path = execute_task(config, task)
        screenshot_url = None
        if screenshot_path is not None:
            try:
                screenshot_url = upload_file(config.base, config.token, screenshot_path)
                message = f"{message}; uploaded screenshot"
            finally:
                try:
                    screenshot_path.unlink(missing_ok=True)
                except Exception:
                    pass
        update_task(config, task_id, status, message, screenshot_url)
        print(f"Updated task: {task_id} -> {status}", flush=True)
    except Exception as exc:  # Keep worker alive; report task failure.
        update_task(config, task_id, "failed", f"worker error: {exc}")
        print(f"Failed task: {task_id}: {exc}", file=sys.stderr, flush=True)
    return True


def parse_args() -> WorkerConfig:
    parser = argparse.ArgumentParser(description="Private Drop desktop worker")
    parser.add_argument("--base", default=os.environ.get("PRIVATE_DROP_URL", "http://127.0.0.1:8080"))
    parser.add_argument("--token", default=os.environ.get("DROP_TOKEN"))
    parser.add_argument("--worker", default=os.environ.get("DESKTOP_WORKER_ID", platform.node() or "desktop-worker"))
    parser.add_argument("--poll-interval", type=float, default=float(os.environ.get("DESKTOP_WORKER_POLL_INTERVAL", "5")))
    parser.add_argument("--idle-heartbeat", type=int, default=int(os.environ.get("DESKTOP_WORKER_IDLE_HEARTBEAT", "12")))
    parser.add_argument("--once", action="store_true", help="process at most one task then exit")
    parser.add_argument("--max-tasks", type=int, default=None, help="process up to N claimed tasks then exit")
    parser.add_argument("--no-screenshot", action="store_true")
    parser.add_argument("--screenshot-delay", type=float, default=float(os.environ.get("DESKTOP_WORKER_SCREENSHOT_DELAY", "2")))
    parser.add_argument("--dry-run", action="store_true", help="do not open apps or capture screenshots")
    args = parser.parse_args()
    if not args.token:
        raise SystemExit("DROP_TOKEN or --token is required")
    return WorkerConfig(
        base=args.base,
        token=args.token,
        worker=args.worker,
        poll_interval=max(args.poll_interval, 0.5),
        once=args.once,
        idle_heartbeat=max(args.idle_heartbeat, 1),
        screenshot=not args.no_screenshot,
        screenshot_delay=max(args.screenshot_delay, 0.0),
        dry_run=args.dry_run,
        max_tasks=args.max_tasks,
    )


def main() -> int:
    config = parse_args()
    print(f"Desktop worker {config.worker} polling {config.base}", flush=True)
    processed = 0
    idle = 0
    while True:
        did_work = run_once(config)
        if did_work:
            processed += 1
            idle = 0
            if config.once:
                return 0
            if config.max_tasks is not None and processed >= config.max_tasks:
                return 0
        else:
            idle += 1
            if config.once:
                print("No pending desktop tasks.", flush=True)
                return 0
            if idle % config.idle_heartbeat == 0:
                print("No pending desktop tasks.", flush=True)
            time.sleep(config.poll_interval)


if __name__ == "__main__":
    sys.exit(main())
