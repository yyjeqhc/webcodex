"""Read-only WebPi authentication acceptance checks. Never log credentials or response bodies."""
from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
LOCAL_ORIGIN = "http://127.0.0.1:56542"
MAX_RESPONSE_BYTES = 2 * 1024 * 1024


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # An Authorization header must never be forwarded to a different host.
        return None


def validate_origin(value: str) -> str:
    if any(ord(character) < 32 or ord(character) == 127 for character in value):
        raise ValueError("origin contains control characters")
    parsed = urllib.parse.urlsplit(value.strip())
    if parsed.username is not None or parsed.password is not None or parsed.path not in ("", "/") or parsed.query or parsed.fragment:
        raise ValueError("use an origin without credentials, path, query or fragment")
    if not parsed.hostname or parsed.scheme not in ("https", "http"):
        raise ValueError("an HTTPS origin is required")
    if parsed.scheme == "http" and parsed.hostname not in ("127.0.0.1", "localhost", "::1"):
        raise ValueError("unencrypted authentication probes are allowed only on loopback")
    _ = parsed.port
    return urllib.parse.urlunsplit((parsed.scheme, parsed.netloc, "", "", ""))


def request_json(origin: str, route: str, payload: dict[str, Any] | None, authorization: str | None = None, timeout: float = 8.0) -> tuple[int | None, object | None, str | None]:
    headers = {"Content-Type": "application/json", "Accept": "application/json", "User-Agent": "WebPi-Security-Acceptance/1"}
    if authorization is not None:
        headers["Authorization"] = authorization
    request = urllib.request.Request(origin + route, data=None if payload is None else json.dumps(payload).encode("utf-8"), headers=headers, method="GET" if payload is None else "POST")
    try:
        response = urllib.request.build_opener(NoRedirect()).open(request, timeout=timeout)
    except urllib.error.HTTPError as error:
        response = error
    except (OSError, urllib.error.URLError):
        return None, None, "unreachable"
    with response:
        content = response.read(MAX_RESPONSE_BYTES + 1)
        if len(content) > MAX_RESPONSE_BYTES:
            return response.status, None, "response_too_large"
        try:
            value = json.loads(content)
        except (ValueError, UnicodeError):
            return response.status, None, "not_json"
        return response.status, value, None


def authentication_checks(origin: str, timeout: float = 8.0) -> list[dict[str, object]]:
    base = validate_origin(origin)
    routes = (
        ("/api/actions/runtime_status", {"compact": True}),
        ("/api/tools/call", {"tool": "runtime_status", "params": {"compact": True}}),
        ("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
    )
    credentials = (
        ("missing", None),
        ("unknown_nonmanaged_bearer", "Bearer webpi-acceptance-invalid-not-a-secret"),
        ("unknown_managed_bearer", "Bearer wc_pat_webpi_acceptance_invalid"),
        ("wrong_scheme", "Basic d2VicGk6aW52YWxpZA=="),
    )
    checks: list[dict[str, object]] = []
    for route, body in routes:
        for label, header in credentials:
            status, _, error = request_json(base, route, body, header, timeout)
            checks.append({"check": label, "route": route, "status": status, "passed": status == 401 and error is None, "error": error})
    return checks


def public_surface_checks(origin: str, timeout: float = 8.0) -> list[dict[str, object]]:
    base = validate_origin(origin)
    credentials = (
        ("missing", None),
        ("unknown_nonmanaged_bearer", "Bearer webpi-acceptance-invalid-not-a-secret"),
        ("unknown_managed_bearer", "Bearer wc_pat_webpi_acceptance_invalid"),
        ("wrong_scheme", "Basic d2VicGk6aW52YWxpZA=="),
    )
    checks: list[dict[str, object]] = []
    for label, header in credentials:
        status, _, error = request_json(base, "/api/actions/runtime_status", {"compact": True}, header, timeout)
        checks.append({"check": label, "route": "/api/actions/runtime_status", "status": status, "passed": status == 401 and error is None, "error": error})
    hidden = (
        ("/api/tools/call", {"tool": "runtime_status", "params": {"compact": True}}),
        ("/mcp", {"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}}),
        ("/admin", None),
    )
    for route, body in hidden:
        status, _, error = request_json(base, route, body, None, timeout)
        checks.append({"check": "public_surface_hidden", "route": route, "status": status, "passed": status == 404 and error is None, "error": error})
    return checks


def require_authenticated_origin(origin: str, timeout: float = 2.0) -> None:
    failures = [item for item in authentication_checks(origin, timeout) if not item["passed"]]
    status, schema, _ = request_json(validate_origin(origin), "/openapi.json", None, timeout=timeout)
    components = schema.get("components") if isinstance(schema, dict) else None
    if not (
        status == 200
        and isinstance(schema, dict)
        and isinstance(schema.get("info"), dict)
        and schema["info"].get("title") == "WebPi GPT Actions"
        and isinstance(components, dict)
        and isinstance(components.get("schemas"), dict)
    ):
        raise RuntimeError("The origin is not an import-compatible WebPi API; do not expose this runtime through a tunnel")
    if failures:
        # Status metadata only; never expose a body, token, or Authorization header.
        statuses = sorted({str(item["status"]) for item in failures})
        raise RuntimeError("WebPi authentication acceptance failed (expected HTTP 401; observed " + ", ".join(statuses) + "). Do not expose this runtime through a tunnel.")


def verify(origin: str, expected_public_origin: str | None = None, token: str | None = None) -> dict[str, object]:
    base = validate_origin(origin)
    hostname = urllib.parse.urlsplit(base).hostname
    loopback = hostname in ("127.0.0.1", "localhost", "::1")
    checks = public_surface_checks(base) if expected_public_origin is not None and not loopback else authentication_checks(base)
    status, schema, error = request_json(base, "/openapi.json", None)
    components = schema.get("components") if isinstance(schema, dict) else None
    valid_schema = status == 200 and isinstance(schema, dict) and isinstance(schema.get("paths"), dict) and str(schema.get("openapi", "")).startswith("3.")
    valid_schema = valid_schema and isinstance(schema.get("info"), dict) and schema["info"].get("title") == "WebPi GPT Actions"
    valid_schema = valid_schema and isinstance(components, dict) and isinstance(components.get("schemas"), dict)
    checks.append({"check": "public_openapi_is_schema_not_authenticated_execution", "route": "/openapi.json", "status": status, "passed": valid_schema, "error": error})
    if expected_public_origin is not None:
        expected = validate_origin(expected_public_origin)
        servers = schema.get("servers", []) if isinstance(schema, dict) else []
        urls = [entry.get("url") for entry in servers if isinstance(entry, dict)]
        checks.append({"check": "openapi_public_origin_matches", "passed": urls == [expected]})
    if token is not None:
        if not token.strip() or any(ord(character) < 32 for character in token):
            raise ValueError("configured Action credential is empty or malformed")
        status, result, error = request_json(base, "/api/actions/runtime_status", {"compact": True}, "Bearer " + token)
        output = result.get("output") if isinstance(result, dict) else None
        success = status == 200 and isinstance(result, dict) and result.get("success") is True and isinstance(output, dict) and output.get("auth_enabled") is True
        success = success and output.get("service") == "webpi"
        checks.append({"check": "configured_action_credential_executes_authenticated_runtime_read", "status": status, "passed": success, "error": error})
    return {"origin": base, "passed": all(item["passed"] for item in checks), "positive_credential_checked": token is not None, "checks": checks}


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default=LOCAL_ORIGIN)
    parser.add_argument("--expect-public-origin")
    parser.add_argument("--with-action-token", action="store_true", help="Use this checkout's local Action PAT without displaying it; only for loopback or its configured public origin")
    args = parser.parse_args(argv)
    base = validate_origin(args.base_url)
    token = None
    if args.with_action_token:
        state = ROOT / ".webpi-state"
        if state.is_symlink() or state.is_junction() or not state.resolve().is_relative_to(ROOT.resolve()):
            raise ValueError("WebPi credential state must remain inside its own checkout")
        manifest_path = state / "manifest.json"
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        allowed = {LOCAL_ORIGIN}
        if isinstance(manifest, dict) and isinstance(manifest.get("public_url"), str):
            allowed.add(validate_origin(manifest["public_url"]))
        if base not in allowed:
            raise ValueError("refusing to send the local Action credential to an unconfigured origin")
        token_path = state / "webpi-action-token"
        if token_path.is_symlink() or token_path.resolve().parent != state.resolve():
            raise ValueError("Action credential path must remain in the WebPi state directory")
        token = token_path.read_text(encoding="utf-8").strip()
    report = verify(base, args.expect_public_origin, token)
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RuntimeError):
        print(json.dumps({"passed": False, "error": "security_check_configuration_or_runtime_failure"}))
        raise SystemExit(2)
