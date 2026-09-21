from __future__ import annotations

import hashlib
import json
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STATE = ROOT / ".webpi-state"
MANIFEST = STATE / "manifest.json"


def load_manifest() -> dict[str, object]:
    value = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError("WebPi manifest is malformed")
    return value


def load_token(path: Path) -> str:
    raw = path.read_text(encoding="utf-8").strip()
    if raw.startswith("{"):
        value = json.loads(raw)
        for key in ("token", "access_token", "api_token"):
            token = value.get(key)
            if isinstance(token, str) and token:
                return token
    if not raw:
        raise RuntimeError("WebPi user token file is empty")
    return raw


def unwrap(value: object) -> object:
    if isinstance(value, dict) and "output" in value:
        return value["output"]
    return value


def post(base: str, token: str, operation: str, payload: dict[str, object]) -> object:
    request = urllib.request.Request(
        f"{base}/api/actions/{operation}",
        data=json.dumps(payload).encode("utf-8"),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"{operation} HTTP {error.code}: {body[:2000]}") from error


def main() -> None:
    manifest = load_manifest()
    base = str(manifest["server_url"])
    token_path = manifest.get("action_token_file") or manifest.get("user_token_file")
    if not isinstance(token_path, str) or not token_path:
        raise RuntimeError("WebPi manifest has no Action token path")
    token = load_token(Path(token_path))
    client_id = str(manifest["client_id"])

    with urllib.request.urlopen(f"{base}/openapi.json", timeout=10) as response:
        openapi = json.loads(response.read().decode("utf-8"))
    operation_ids = {
        operation.get("operationId")
        for methods in openapi.get("paths", {}).values()
        if isinstance(methods, dict)
        for operation in methods.values()
        if isinstance(operation, dict)
    }
    required = {
        "work_on_project",
        "read_files",
        "apply_text_edits",
        "run_process",
        "observe_jobs",
        "show_changes",
        "plugin_tool",
    }
    missing = sorted(required - operation_ids)
    if missing:
        raise RuntimeError(f"WebPi OpenAPI is missing direct coding actions: {missing}")

    work_raw = post(
        base,
        token,
        "work_on_project",
        {
            "client_id": client_id,
            "path": str(ROOT),
            "mode": "checkout",
            "instruction": "WebPi standalone end-to-end smoke; read-only except dry-run edit validation.",
            "include_project_instructions": False,
            "include_workflow_guidance": False,
        },
    )
    work = unwrap(work_raw)
    if not isinstance(work, dict):
        raise RuntimeError("work_on_project returned an unexpected payload")
    project = work.get("project") or work.get("resolved_project")
    session_id = work.get("session_id")
    if not isinstance(project, str) or not project:
        raise RuntimeError("work_on_project did not return a project id")
    if not isinstance(session_id, str) or not session_id.startswith("wc_sess_"):
        raise RuntimeError("work_on_project did not return a Workflow Session")

    read_raw = post(
        base,
        token,
        "read_files",
        {
            "project": project,
            "items": [{"path": "docs/WEBPI.md", "start_line": 1, "limit": 40}],
            "with_line_numbers": False,
            "session_id": session_id,
        },
    )
    read_value = unwrap(read_raw)
    if not isinstance(read_value, dict):
        raise RuntimeError("read_files returned an unexpected payload")
    items = read_value.get("items")
    if not isinstance(items, list) or not items or not isinstance(items[0], dict):
        raise RuntimeError("read_files did not return an item")
    first = items[0]
    file_output = first.get("output")
    if not isinstance(file_output, dict):
        raise RuntimeError("read_files item has no output")
    text = file_output.get("text")
    read_revision = file_output.get("read_revision")
    if not isinstance(text, str) or "# WebPi" not in text:
        raise RuntimeError("read_files did not read the WebPi documentation")
    if not isinstance(read_revision, int) or read_revision < 1:
        raise RuntimeError("read_files did not return a read_revision stale-write fence")
    before_sha = hashlib.sha256((ROOT / "docs" / "WEBPI.md").read_bytes()).hexdigest()

    dry_raw = post(
        base,
        token,
        "apply_text_edits",
        {
            "project": project,
            "dry_run": True,
            "session_id": session_id,
            "changes": [
                {
                    "kind": "edit",
                    "path": "docs/WEBPI.md",
                    "expected_read_revision": read_revision,
                    "edits": [
                        {
                            "kind": "replace_exact",
                            "old_text": "# WebPi",
                            "new_text": "# WebPi standalone-smoke",
                        }
                    ],
                }
            ],
        },
    )
    dry = unwrap(dry_raw)
    if not isinstance(dry, dict) or dry.get("dry_run") is not True:
        raise RuntimeError("apply_text_edits dry-run did not complete")

    listed = unwrap(
        post(
            base,
            token,
            "plugin_tool",
            {"action": "list", "runner": client_id, "plugin": "pi-bridge"},
        )
    )
    if (
        not isinstance(listed, dict)
        or not isinstance(listed.get("toolCount"), int)
        or int(listed["toolCount"]) < 16
    ):
        raise RuntimeError(f"pi-bridge parity catalog was not ready: {listed!r}")

    described = unwrap(
        post(
            base,
            token,
            "plugin_tool",
            {
                "action": "describe",
                "runner": client_id,
                "plugin": "pi-bridge",
                "tool": "pi_read",
            },
        )
    )
    if not isinstance(described, dict) or not isinstance(described.get("binding"), str):
        raise RuntimeError("pi_read describe did not return a binding")

    called = unwrap(
        post(
            base,
            token,
            "plugin_tool",
            {
                "action": "call",
                "binding": described["binding"],
                "arguments": {"path": "docs/WEBPI.md", "limit": 40},
            },
        )
    )
    if not isinstance(called, dict):
        raise RuntimeError(f"pi_read call returned an unexpected payload: {called!r}")
    structured = called.get("structuredContent", called)
    if not isinstance(structured, dict) or structured.get("engine") != "pi":
        raise RuntimeError(f"pi_read call did not execute through Pi: {called!r}")
    if "# WebPi" not in str(structured.get("text", "")):
        raise RuntimeError("Pi read result did not contain expected WebPi documentation")

    parity_described = unwrap(
        post(
            base,
            token,
            "plugin_tool",
            {
                "action": "describe",
                "runner": client_id,
                "plugin": "pi-bridge",
                "tool": "pi_capability_report",
            },
        )
    )
    if not isinstance(parity_described, dict) or not isinstance(
        parity_described.get("binding"), str
    ):
        raise RuntimeError("pi_capability_report describe did not return a binding")

    parity_called = unwrap(
        post(
            base,
            token,
            "plugin_tool",
            {
                "action": "call",
                "binding": parity_described["binding"],
                "arguments": {},
            },
        )
    )
    if not isinstance(parity_called, dict):
        raise RuntimeError(
            f"pi_capability_report returned an unexpected payload: {parity_called!r}"
        )
    parity = parity_called.get("structuredContent", parity_called)
    if not isinstance(parity, dict):
        raise RuntimeError("pi_capability_report has no structured content")
    if parity.get("architecture") != "web-gpt-primary-no-second-pi-model-loop":
        raise RuntimeError(f"unexpected WebPi/Pi architecture contract: {parity!r}")
    if not str(parity.get("piVersion", "")).startswith("0.85."):
        raise RuntimeError(f"unexpected pinned Pi version: {parity!r}")

    current_sha = hashlib.sha256((ROOT / "docs" / "WEBPI.md").read_bytes()).hexdigest()
    if current_sha != before_sha:
        raise RuntimeError("dry-run unexpectedly changed docs/WEBPI.md")

    print(
        json.dumps(
            {
                "ok": True,
                "server_url": base,
                "runner": client_id,
                "project": project,
                "session": session_id,
                "direct_action_count_checked": len(required),
                "dry_run_edit": True,
                "pi_bridge_tools": listed.get("toolCount"),
                "pi_engine": structured.get("engine"),
                "pi_parity_architecture": parity.get("architecture"),
                "pi_version": parity.get("piVersion"),
            },
            ensure_ascii=False,
        )
    )


if __name__ == "__main__":
    main()
