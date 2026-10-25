#!/usr/bin/env python3
"""Opt-in, read-only continuation observation. Never drives a Host or a model."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import time

try:
    from scripts.continuation_snapshot import EvidenceError, snapshot
except ModuleNotFoundError:
    from continuation_snapshot import EvidenceError, snapshot

MAX_BYTES = 128 * 1024
ROLES = ("server", "runner", "desktop", "tunnel")


def require(condition, code):
    if not condition:
        raise EvidenceError(code)


def private_directory(path):
    """No parent symlink traversal for operator receipts or source records."""
    path = Path(path).absolute()
    fd = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY)
    try:
        for part in path.parts[1:]:
            child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = child
        info = os.fstat(fd)
        require(info.st_uid == os.getuid() and not info.st_mode & 0o077, "unsafe_directory")
        return fd
    except BaseException:
        os.close(fd)
        raise


def source_record(path, expected):
    """Validate a coordinator's source attestation, not live process identity."""
    require(bool(re.fullmatch(r"[0-9a-f]{40}", expected)), "invalid_source")
    path = Path(path).absolute()
    parent = private_directory(path.parent)
    try:
        fd = os.open(path.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=parent)
    finally:
        os.close(parent)
    with os.fdopen(fd, "rb") as stream:
        before = os.fstat(stream.fileno())
        require(stat.S_ISREG(before.st_mode) and before.st_uid == os.getuid()
                and before.st_nlink == 1 and not before.st_mode & 0o077, "unsafe_source_record")
        raw = stream.read(MAX_BYTES + 1)
        after = os.fstat(stream.fileno())
        require(len(raw) <= MAX_BYTES, "source_record_limit")
        require((before.st_size, before.st_mtime_ns, before.st_ctime_ns) ==
                (after.st_size, after.st_mtime_ns, after.st_ctime_ns), "source_record_changed")
    value = json.loads(raw)
    require(isinstance(value, dict) and value.get("source") == expected, "source_mismatch")
    components = value.get("components")
    require(isinstance(components, dict), "source_components_missing")
    for role in ROLES:
        component = components.get(role)
        require(isinstance(component, dict) and component.get("source") == expected
                and isinstance(component.get("sha256"), str)
                and bool(re.fullmatch(r"[0-9a-f]{64}", component["sha256"])), "source_component_mismatch")
    return {"source": expected, "record_sha256": hashlib.sha256(raw).hexdigest(),
            "file_identity": [before.st_dev, before.st_ino],
            "verification": "coordinator_attested_not_live_process_probe"}


def summarize(value):
    """Facts from one consistent snapshot; no browser-cause or success oracle."""
    now = value["observed_at_ms"]
    generation = value["agent"]["current_controller_generation"]
    current = [e for e in value["endpoints"] if e["controller_generation"] == generation]
    require(len(current) <= 1, "ambiguous_current_endpoint")
    carrier = "absent"
    if current:
        endpoint = current[0]
        carrier = endpoint["lifecycle"]
        if carrier == "attached":
            expiry = endpoint.get("lease_expires_at_unix_ms")
            carrier = "lease_unavailable" if expiry is None else (
                "lease_expired" if expiry <= now else "lease_current_at_snapshot")
    attention = {a["event_id"]: a for a in value["attention"]}
    epochs = [a["last_meaningful_activity_at_ms"] for a in value["attention"]]
    reasons = []
    if len(epochs) != len(set(epochs)):
        reasons.append("multiple_attention_same_epoch")
    wakes = []
    for wake in value["wakes"]:
        attempts = [a for a in value["attempts"] if a["wake_id"] == wake["wake_id"]]
        event = attention[wake["source_event_id"]]
        prepared = [a for a in attempts if a.get("prepared_at_unix_ms") is not None]
        unknown = any(a.get("delivery_unknown_at_unix_ms") is not None for a in attempts)
        state = wake["state"]
        selected = next((a for a in attempts if a["attempt_id"] == wake.get("claimed_attempt_id")), {})
        stage = "exact_consume_recorded" if state == "consumed" else (
            "delivery_unknown_do_not_resend" if unknown else (
                "host_delivery_recorded" if any(a.get("delivered_at_unix_ms") is not None for a in attempts)
                else "prepared_without_delivery_proof" if prepared else state))
        deltas = {}
        for label, start, end in (
            ("meaningful_to_wake_ms", event.get("last_meaningful_activity_at_ms"), wake.get("created_at_unix_ms")),
            ("wake_to_consume_ms", wake.get("created_at_unix_ms"), wake.get("consumed_at_unix_ms")),
            ("wake_to_prepare_ms", wake.get("created_at_unix_ms"), selected.get("prepared_at_unix_ms")),
            ("prepare_to_delivery_ms", selected.get("prepared_at_unix_ms"), selected.get("delivered_at_unix_ms")),
            ("delivery_to_consume_ms", selected.get("delivered_at_unix_ms"), selected.get("consumed_at_unix_ms")),
        ):
            deltas[label] = end - start if start is not None and end is not None else None
            if deltas[label] is not None and deltas[label] < 0:
                reasons.append("timestamp_order_invalid")
        if len(prepared) > 1:
            reasons.append("multiple_prepared_attempts_requires_inspection")
        wakes.append({"wake_id": wake["wake_id"], "stage": stage, "attempt_count": len(attempts),
                      "prepared_attempt_count": len(prepared), "intervals": deltas})
    return {"goal_lifecycle": value["goal"]["lifecycle"], "carrier": carrier,
            "attention_count": len(attention), "wakes": wakes, "findings": sorted(set(reasons)),
            "host_fresh_turn": "not_observed", "unattended": "not_observed",
            "browser_interruption_cause": "unknown", "model_tokens_and_cost": None}


class Receipts:
    """Exclusive, bounded writes beneath an already-private operator directory."""
    def __init__(self, directory):
        self.fd = private_directory(directory)
        with os.scandir(self.fd) as entries:
            if next(entries, None) is not None:
                os.close(self.fd)
                raise EvidenceError("output_not_empty")
        self.index = 0

    def write(self, stage, **fields):
        payload = json.dumps({"schema_version": 1, "stage": stage, **fields}, sort_keys=True).encode() + b"\n"
        require(len(payload) <= 2 * MAX_BYTES, "receipt_limit")
        name = f"{self.index:04d}-{stage}.json"
        fd = os.open(name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=self.fd)
        self.index += 1
        with os.fdopen(fd, "wb") as stream:
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())

    def close(self):
        os.close(self.fd)


def capture(args, *, read_snapshot=snapshot, clock=time.time, monotonic=time.monotonic, sleep=time.sleep):
    """Attach only to exact resources which already exist; never wait for creation."""
    require(type(args.watch_seconds) is int and 0 <= args.watch_seconds <= 900, "invalid_watch_budget")
    require(type(args.interval_seconds) is int and 1 <= args.interval_seconds <= 30, "invalid_poll_interval")
    receipts = Receipts(args.output)
    try:
        source = source_record(args.source_record, args.expected_source)
        start_wall, start_mono = clock(), monotonic()
        initial = read_snapshot(args.database, goal_id=args.goal_id, agent_id=args.agent_id,
                                session_id=args.session_id, now_ms=int(start_wall * 1000))
        require(source_record(args.source_record, args.expected_source) == source, "source_record_changed")
        # Already checkpointed work is retained as retrospective, never backdated.
        prospective = (args.watch_seconds > 0 and initial["goal"]["lifecycle"] == "active"
                       and initial["goal"].get("checkpoint_at_unix_ms") is None
                       and not initial["attention"] and not initial["wakes"])
        coverage = "before_first_checkpoint" if prospective else "late_or_snapshot_only"
        receipts.write("start", source=source, coverage=coverage, snapshot=initial,
                       assessment=summarize(initial), requested_watch_seconds=args.watch_seconds)
        previous = initial
        samples = 1
        deadline = start_mono + args.watch_seconds
        # Time and count bound independently; a broken clock cannot produce an infinite collector.
        max_polls = (args.watch_seconds + args.interval_seconds - 1) // args.interval_seconds
        for _ in range(max_polls):
            remaining = deadline - monotonic()
            if remaining <= 0:
                break
            sleep(min(args.interval_seconds, remaining))
            wall, mono = clock(), monotonic()
            require(mono >= start_mono and abs((wall - start_wall) - (mono - start_mono)) <= 2,
                    "clock_changed")
            require(source_record(args.source_record, args.expected_source) == source, "source_record_changed")
            current = read_snapshot(args.database, goal_id=args.goal_id, agent_id=args.agent_id,
                                    session_id=args.session_id, now_ms=int(wall * 1000))
            require(source_record(args.source_record, args.expected_source) == source, "source_record_changed")
            require(current["db_identity"] == initial["db_identity"], "database_replaced")
            require(current["selectors"] == initial["selectors"], "selectors_changed")
            require(current["goal"]["revision"] >= previous["goal"]["revision"], "goal_revision_regressed")
            require(current["agent"]["current_controller_generation"] >=
                    previous["agent"]["current_controller_generation"], "generation_regressed")
            receipts.write("sample", snapshot=current, assessment=summarize(current))
            samples += 1
            previous = current
        if args.watch_seconds:
            require(monotonic() >= deadline, "poll_budget_exhausted")
        receipts.write("end", coverage=coverage, elapsed_seconds=monotonic() - start_mono,
                       assessment=summarize(previous), collection_finished=True,
                       sqlite_snapshot_count=samples,
                       host_acceptance="requires_separate_attributed_observation")
        return 0
    except KeyboardInterrupt:
        receipts.write("incomplete", reason="operator_cancelled")
        return 130
    except Exception as error:
        # Exception strings from SQLite/files/JSON may contain private contents.
        code = str(error) if isinstance(error, EvidenceError) else "capture_unavailable"
        if not re.fullmatch(r"[a-z0-9_]{1,80}", code):
            code = "capture_unavailable"
        receipts.write("incomplete", reason=code)
        return 1
    finally:
        receipts.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("database", "source-record", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    for name in ("goal-id", "agent-id", "session-id", "expected-source"):
        parser.add_argument("--" + name, required=True)
    parser.add_argument("--watch-seconds", type=int, default=0, help="0: retrospective snapshot; maximum 900")
    parser.add_argument("--interval-seconds", type=int, default=5)
    args = parser.parse_args()
    try:
        code = capture(args)
    except (EvidenceError, OSError):
        code = 1
    print("Collection complete; Host acceptance is separate." if code == 0 else
          "Collection incomplete; retain existing receipts.")
    return code


if __name__ == "__main__":
    raise SystemExit(main())
