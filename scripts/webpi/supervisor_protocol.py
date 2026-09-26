from __future__ import annotations

import hashlib
import hmac
import json
import os
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = 1
REQUEST_FILE = "request.json"
REQUEST_LOCK_FILE = "request.lock"
TOKEN_FILE = "supervisor.key"
MAX_ENVELOPE_BYTES = 16 * 1024
MAX_CLOCK_SKEW_MS = 60_000
MIN_EXECUTE_AFTER_MS = 500
MAX_EXECUTE_AFTER_MS = 10_000
TOKEN_HEX_BYTES = 32
REQUIRED_DEPLOY_ARTIFACTS = ("webpi.exe", "webpi-server.exe", "webpi-runner.exe")
MAX_CANDIDATE_ID_CHARS = 64
MAX_BACKUP_ID_CHARS = 128


class SupervisorProtocolError(RuntimeError):
    pass


@dataclass(frozen=True)
class RestartRequest:
    receipt_id: str
    receipt_revision: int
    requested_at_ms: int
    execute_after_ms: int


@dataclass(frozen=True)
class DeployArtifact:
    name: str
    sha256: str
    size_bytes: int


@dataclass(frozen=True)
class DeployRequest:
    receipt_id: str
    receipt_revision: int
    requested_at_ms: int
    execute_after_ms: int
    candidate_id: str
    artifacts: tuple[DeployArtifact, ...]


@dataclass(frozen=True)
class RollbackRequest:
    receipt_id: str
    receipt_revision: int
    requested_at_ms: int
    execute_after_ms: int
    backup_id: str


def _valid_token(token: str) -> bool:
    return len(token) == TOKEN_HEX_BYTES * 2 and all(ch in "0123456789abcdefABCDEF" for ch in token)


def _mac(token: str, payload: str) -> str:
    if not _valid_token(token):
        raise SupervisorProtocolError("invalid supervisor token")
    return hmac.new(token.encode("ascii"), payload.encode("utf-8"), hashlib.sha256).hexdigest()


def load_or_create_token(control_dir: Path) -> str:
    control_dir.mkdir(parents=True, exist_ok=True)
    path = control_dir / TOKEN_FILE
    try:
        token = path.read_text(encoding="ascii").strip()
    except FileNotFoundError:
        token = os.urandom(TOKEN_HEX_BYTES).hex()
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        except FileExistsError:
            token = path.read_text(encoding="ascii").strip()
        else:
            with os.fdopen(fd, "w", encoding="ascii", newline="\n") as handle:
                handle.write(token + "\n")
                handle.flush()
                os.fsync(handle.fileno())
    if not _valid_token(token):
        raise SupervisorProtocolError("invalid persisted supervisor token")
    try:
        path.chmod(0o600)
    except OSError:
        pass
    return token


def envelope(payload: dict[str, Any], token: str) -> dict[str, str]:
    encoded = json.dumps(payload, separators=(",", ":"), sort_keys=True)
    return {"payload": encoded, "mac": _mac(token, encoded)}


def verify_envelope(value: object, token: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != {"payload", "mac"}:
        raise SupervisorProtocolError("invalid supervisor envelope")
    payload = value.get("payload")
    mac = value.get("mac")
    if not isinstance(payload, str) or not isinstance(mac, str) or len(payload.encode("utf-8")) > MAX_ENVELOPE_BYTES:
        raise SupervisorProtocolError("invalid supervisor envelope fields")
    if not hmac.compare_digest(_mac(token, payload), mac.lower()):
        raise SupervisorProtocolError("invalid supervisor envelope mac")
    decoded = json.loads(payload)
    if not isinstance(decoded, dict):
        raise SupervisorProtocolError("supervisor payload must be an object")
    return decoded


def _safe_single_component(value: str, max_chars: int) -> bool:
    if not value or len(value) > max_chars or not value[0].isalnum():
        return False
    return all(ch.isalnum() or ch in "._-" for ch in value)


def _validate_common_request(payload: dict[str, Any], *, now_ms: int | None) -> tuple[str, int, int, int]:
    receipt_id = payload.get("receipt_id")
    revision = payload.get("receipt_revision")
    requested = payload.get("requested_at_ms")
    delay = payload.get("execute_after_ms")
    if not isinstance(receipt_id, str) or not receipt_id.startswith("wc_deploy_") or len(receipt_id) > 96:
        raise SupervisorProtocolError("invalid deployment receipt id")
    if not isinstance(revision, int) or isinstance(revision, bool) or revision < 1:
        raise SupervisorProtocolError("invalid deployment receipt revision")
    if not isinstance(requested, int) or isinstance(requested, bool) or requested < 1:
        raise SupervisorProtocolError("invalid supervisor request timestamp")
    if not isinstance(delay, int) or isinstance(delay, bool) or not MIN_EXECUTE_AFTER_MS <= delay <= MAX_EXECUTE_AFTER_MS:
        raise SupervisorProtocolError("invalid supervisor execute delay")
    now = int(time.time() * 1000) if now_ms is None else now_ms
    if abs(now - requested) > MAX_CLOCK_SKEW_MS:
        raise SupervisorProtocolError("stale supervisor request")
    return receipt_id, revision, requested, delay

def parse_restart_request(value: object, token: str, *, now_ms: int | None = None) -> RestartRequest:
    payload = verify_envelope(value, token)
    allowed = {"version", "action", "receipt_id", "receipt_revision", "requested_at_ms", "execute_after_ms"}
    if set(payload) != allowed:
        raise SupervisorProtocolError("unsupported supervisor restart payload fields")
    if payload.get("version") != PROTOCOL_VERSION or payload.get("action") != "restart":
        raise SupervisorProtocolError("unsupported supervisor action or version")
    receipt_id, revision, requested, delay = _validate_common_request(payload, now_ms=now_ms)
    return RestartRequest(receipt_id, revision, requested, delay)


def parse_deploy_request(value: object, token: str, *, now_ms: int | None = None) -> DeployRequest:
    payload = verify_envelope(value, token)
    allowed = {
        "version", "action", "receipt_id", "receipt_revision", "requested_at_ms",
        "execute_after_ms", "candidate_id", "artifacts"
    }
    if set(payload) != allowed:
        raise SupervisorProtocolError("unsupported supervisor deploy payload fields")
    if payload.get("version") != PROTOCOL_VERSION or payload.get("action") != "deploy":
        raise SupervisorProtocolError("unsupported supervisor action or version")
    receipt_id, revision, requested, delay = _validate_common_request(payload, now_ms=now_ms)
    candidate_id = payload.get("candidate_id")
    if not isinstance(candidate_id, str) or not _safe_single_component(candidate_id, MAX_CANDIDATE_ID_CHARS):
        raise SupervisorProtocolError("invalid deployment candidate id")
    artifacts_value = payload.get("artifacts")
    if not isinstance(artifacts_value, list) or len(artifacts_value) != len(REQUIRED_DEPLOY_ARTIFACTS):
        raise SupervisorProtocolError("invalid deployment artifact set")
    artifacts: list[DeployArtifact] = []
    names: set[str] = set()
    for item in artifacts_value:
        if not isinstance(item, dict) or set(item) != {"name", "sha256", "size_bytes"}:
            raise SupervisorProtocolError("invalid deployment artifact")
        name = item.get("name")
        digest = item.get("sha256")
        size = item.get("size_bytes")
        if not isinstance(name, str) or name not in REQUIRED_DEPLOY_ARTIFACTS or name in names:
            raise SupervisorProtocolError("invalid deployment artifact name")
        if not isinstance(digest, str) or len(digest) != 64 or any(ch not in "0123456789abcdefABCDEF" for ch in digest):
            raise SupervisorProtocolError("invalid deployment artifact digest")
        if not isinstance(size, int) or isinstance(size, bool) or size <= 0:
            raise SupervisorProtocolError("invalid deployment artifact size")
        names.add(name)
        artifacts.append(DeployArtifact(name, digest.lower(), size))
    if names != set(REQUIRED_DEPLOY_ARTIFACTS):
        raise SupervisorProtocolError("deployment artifact set is incomplete")
    return DeployRequest(receipt_id, revision, requested, delay, candidate_id, tuple(artifacts))


def parse_rollback_request(value: object, token: str, *, now_ms: int | None = None) -> RollbackRequest:
    payload = verify_envelope(value, token)
    allowed = {
        "version", "action", "receipt_id", "receipt_revision", "requested_at_ms",
        "execute_after_ms", "backup_id"
    }
    if set(payload) != allowed:
        raise SupervisorProtocolError("unsupported supervisor rollback payload fields")
    if payload.get("version") != PROTOCOL_VERSION or payload.get("action") != "rollback":
        raise SupervisorProtocolError("unsupported supervisor action or version")
    receipt_id, revision, requested, delay = _validate_common_request(payload, now_ms=now_ms)
    backup_id = payload.get("backup_id")
    if not isinstance(backup_id, str) or not _safe_single_component(backup_id, MAX_BACKUP_ID_CHARS):
        raise SupervisorProtocolError("invalid rollback backup id")
    return RollbackRequest(receipt_id, revision, requested, delay, backup_id)


def result_envelope(
    request: RestartRequest | DeployRequest | RollbackRequest,
    token: str,
    *,
    status: str,
    error_code: str | None = None,
    backup_id: str | None = None,
    completed_at_ms: int | None = None,
) -> dict[str, str]:
    if isinstance(request, DeployRequest):
        action = "deploy"
        allowed_statuses = {"succeeded", "failed", "rolled_back", "outcome_unknown"}
    elif isinstance(request, RollbackRequest):
        action = "rollback"
        allowed_statuses = {"succeeded", "failed", "outcome_unknown"}
    else:
        action = "restart"
        allowed_statuses = {"succeeded", "failed"}
    if status not in allowed_statuses:
        raise SupervisorProtocolError("invalid supervisor result status")
    if error_code is not None and (
        not error_code
        or len(error_code) > 128
        or any(not (ch.isalnum() or ch in "_-.:" ) for ch in error_code)
    ):
        raise SupervisorProtocolError("invalid supervisor result error code")
    if backup_id is not None and (
        not isinstance(request, (DeployRequest, RollbackRequest))
        or not _safe_single_component(backup_id, MAX_BACKUP_ID_CHARS)
    ):
        raise SupervisorProtocolError("invalid supervisor backup id")
    payload: dict[str, Any] = {
        "version": PROTOCOL_VERSION,
        "action": action,
        "receipt_id": request.receipt_id,
        "receipt_revision": request.receipt_revision,
        "status": status,
        "completed_at_ms": int(time.time() * 1000) if completed_at_ms is None else completed_at_ms,
        "error_code": error_code,
    }
    if backup_id is not None:
        payload["backup_id"] = backup_id
    return envelope(payload, token)


def atomic_write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    data = json.dumps(value, separators=(",", ":"), sort_keys=True).encode("utf-8")
    if len(data) > MAX_ENVELOPE_BYTES:
        raise SupervisorProtocolError("supervisor envelope exceeds size limit")
    try:
        with temp.open("xb") as handle:
            handle.write(data)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temp, path)
    finally:
        try:
            temp.unlink()
        except FileNotFoundError:
            pass


def read_supervisor_request(
    control_dir: Path,
    token: str,
    *,
    now_ms: int | None = None,
) -> RestartRequest | DeployRequest | RollbackRequest | None:
    path = control_dir / REQUEST_FILE
    try:
        raw = path.read_bytes()
    except FileNotFoundError:
        return None
    if len(raw) > MAX_ENVELOPE_BYTES:
        raise SupervisorProtocolError("supervisor request exceeds size limit")
    value = json.loads(raw)
    payload = verify_envelope(value, token)
    action = payload.get("action")
    if action == "restart":
        return parse_restart_request(value, token, now_ms=now_ms)
    if action == "deploy":
        return parse_deploy_request(value, token, now_ms=now_ms)
    if action == "rollback":
        return parse_rollback_request(value, token, now_ms=now_ms)
    raise SupervisorProtocolError("unsupported supervisor action")


def read_restart_request(control_dir: Path, token: str, *, now_ms: int | None = None) -> RestartRequest | None:
    request = read_supervisor_request(control_dir, token, now_ms=now_ms)
    if request is None:
        return None
    if not isinstance(request, RestartRequest):
        raise SupervisorProtocolError("supervisor request is not restart")
    return request


def result_path(control_dir: Path, receipt_id: str) -> Path:
    if not receipt_id.startswith("wc_deploy_") or len(receipt_id) > 96 or not all(ch.isalnum() or ch == "_" for ch in receipt_id):
        raise SupervisorProtocolError("invalid deployment receipt id")
    return control_dir / f"result-{receipt_id}.json"
