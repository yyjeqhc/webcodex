"""Bounded, body-free SQLite evidence for one explicitly selected continuation.

This reads durable metadata only. It cannot establish browser/Host state or
whether a model turn happened. No schema migration or recovery runs here.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import re
import sqlite3
import stat
import time

MAX_ROWS = 128
MAX_STEPS = 64
MAX_OUTPUT_BYTES = 128 * 1024
QUERY_SECONDS = 3


class EvidenceError(ValueError):
    """A closed diagnostic code; never includes paths, SQL, or stored text."""


def _require(condition, reason="invalid_metadata"):
    if not condition:
        raise EvidenceError(reason)


def _integer(value, *, nullable=False, minimum=0):
    _require((nullable and value is None) or
             (type(value) is int and minimum <= value <= 2**63 - 1))
    return value


def _identifier(value, prefix, *, nullable=False):
    _require((nullable and value is None) or
             (isinstance(value, str) and
              re.fullmatch(re.escape(prefix) + r"[A-Za-z0-9_-]{16}", value) is not None))
    return value


def _state(value, allowed):
    _require(isinstance(value, str) and value in allowed)


def _timestamps(row, required, optional=()):
    for field in required:
        _integer(row[field])
    for field in optional:
        _integer(row[field], nullable=True)


def _owner(row):
    kind, digest = row.pop("owner_principal_kind"), row.pop("owner_principal_digest")
    _require(isinstance(kind, str) and re.fullmatch(r"[A-Za-z0-9_:.\-]{1,64}", kind) is not None)
    _require(isinstance(digest, str) and re.fullmatch(r"wc_commprincipal_[a-f0-9]{64}", digest) is not None)
    return kind, digest


def _rows(connection, sql, parameters=(), *, bounded=False):
    rows = [dict(row) for row in connection.execute(sql, parameters)]
    if bounded:
        for row in rows:
            _require(row.pop("_total") <= MAX_ROWS, "row_limit")
    return rows


def _one(connection, sql, parameters):
    rows = _rows(connection, sql, parameters)
    _require(len(rows) == 1, "exact_identity_missing")
    return rows[0]


def _file_identity(path):
    # Reject symlinked directory components too: resolving them would conceal
    # which database the operator actually selected.
    for component in (path, *path.parents):
        info = component.lstat()
        _require(not stat.S_ISLNK(info.st_mode), "unsafe_database_file")
    info = path.lstat()
    _require(stat.S_ISREG(info.st_mode) and info.st_uid == os.getuid(), "unsafe_database_file")
    return [info.st_dev, info.st_ino]


def _schema(connection):
    rows = _rows(connection, """SELECT name, type, rootpage FROM sqlite_schema
        WHERE name IN ('wc_goals','wc_goal_correlations','wc_agent_identities',
        'wc_agent_endpoints','wc_agent_attention_events','wc_agent_wakes',
        'wc_agent_wake_attempts') LIMIT 8""")
    _require(len(rows) == 7 and all(row["type"] == "table" and row["rootpage"] > 0
                                  for row in rows), "unsupported_schema")


def _read(connection, goal_id, agent_id, session_id, now_ms):
    _schema(connection)
    goal = _one(connection, """SELECT controller_agent_id, owner_principal_kind,
        owner_principal_digest, lifecycle, revision, created_at_unix_ms,
        updated_at_unix_ms, terminal_at_unix_ms,
        CASE WHEN length(CAST(plan_json AS BLOB)) <= 262144 THEN
          json_extract(plan_json,'$.checkpoint_at_unix_ms') END AS checkpoint_at_unix_ms,
        CASE WHEN length(CAST(plan_json AS BLOB)) <= 262144 THEN
          json_type(plan_json,'$.checkpoint_at_unix_ms') END AS checkpoint_type,
        CASE WHEN length(CAST(plan_json AS BLOB)) <= 262144 THEN
          json_array_length(plan_json,'$.steps') END AS step_count,
        CASE WHEN length(CAST(plan_json AS BLOB)) <= 262144 THEN
          json_type(plan_json,'$.steps') END AS steps_type
        FROM wc_goals WHERE goal_id=? LIMIT 2""", (goal_id,))
    _require(goal.pop("controller_agent_id") == agent_id, "controller_mismatch")
    owner = _owner(goal)
    agent = _one(connection, """SELECT owner_principal_kind, owner_principal_digest,
        current_controller_generation FROM wc_agent_identities WHERE agent_id=? LIMIT 2""", (agent_id,))
    _require(_owner(agent) == owner, "owner_mismatch")
    _integer(agent["current_controller_generation"])
    correlations = _rows(connection, """SELECT goal_id, reference_id FROM wc_goal_correlations
        WHERE kind='workflow_session' AND (goal_id=? OR reference_id=?) LIMIT 2""", (goal_id, session_id))
    _require(correlations == [{"goal_id": goal_id, "reference_id": session_id}], "session_correlation_not_unique")
    _state(goal["lifecycle"], {"active", "completed", "cancelled"})
    _integer(goal["revision"], minimum=1)
    _timestamps(goal, ("created_at_unix_ms", "updated_at_unix_ms"),
                ("terminal_at_unix_ms", "checkpoint_at_unix_ms"))
    _require((goal["lifecycle"] == "active") == (goal["terminal_at_unix_ms"] is None))
    _require(goal["updated_at_unix_ms"] >= goal["created_at_unix_ms"])
    _require(goal.pop("checkpoint_type") in {"integer", "null"})
    _require(goal.pop("steps_type") == "array")
    step_count = _integer(goal.pop("step_count"))
    _require(step_count <= MAX_STEPS, "step_limit")
    steps = _rows(connection, """SELECT json_extract(s.value,'$.id') AS id,
        json_extract(s.value,'$.status') AS status,
        json_extract(s.value,'$.updated_at_unix_ms') AS updated_at_unix_ms,
        json_type(s.value,'$.updated_at_unix_ms') AS timestamp_type
        FROM wc_goals g, json_each(g.plan_json,'$.steps') s WHERE g.goal_id=? LIMIT 64""", (goal_id,))
    _require(len(steps) == step_count)
    seen_steps = set()
    for step in steps:
        _require(isinstance(step["id"], str) and
                 re.fullmatch(r"[A-Za-z0-9_-]{1,32}", step["id"]) is not None and
                 step["id"] not in seen_steps)
        seen_steps.add(step["id"])
        _state(step["status"], {"pending", "in_progress", "completed"})
        _require(step.pop("timestamp_type") == "integer")
        _integer(step["updated_at_unix_ms"])
    goal["steps"] = steps
    endpoints = _rows(connection, """SELECT endpoint_id, controller_generation,
        lifecycle, wake_capable, attached_at_unix_ms, last_seen_at_unix_ms,
        lease_expires_at_unix_ms, expired_at_unix_ms, detached_at_unix_ms,
        attachment_principal_kind AS owner_principal_kind,
        attachment_principal_digest AS owner_principal_digest, COUNT(*) OVER() AS _total
        FROM wc_agent_endpoints WHERE agent_id=?
        ORDER BY attached_at_unix_ms, endpoint_id LIMIT 128""", (agent_id,), bounded=True)
    endpoint_map = {}
    for endpoint in endpoints:
        _identifier(endpoint["endpoint_id"], "wc_endpoint_")
        _require(endpoint["endpoint_id"] not in endpoint_map)
        endpoint_map[endpoint["endpoint_id"]] = endpoint
        _require(_owner(endpoint) == owner, "owner_mismatch")
        _integer(endpoint["controller_generation"], minimum=1)
        _require(endpoint["controller_generation"] <= agent["current_controller_generation"], "generation_mismatch")
        _state(endpoint["lifecycle"], {"attached", "expired", "detached"})
        _require(type(endpoint["wake_capable"]) is int and endpoint["wake_capable"] in (0, 1))
        _timestamps(endpoint, ("attached_at_unix_ms", "last_seen_at_unix_ms", "lease_expires_at_unix_ms"),
                    ("expired_at_unix_ms", "detached_at_unix_ms"))
        _require(endpoint["last_seen_at_unix_ms"] >= endpoint["attached_at_unix_ms"])
        if endpoint["lifecycle"] == "attached":
            _require(endpoint["expired_at_unix_ms"] is None and endpoint["detached_at_unix_ms"] is None)
            _require(endpoint["controller_generation"] == agent["current_controller_generation"], "generation_mismatch")
        else:
            _require(endpoint[endpoint["lifecycle"] + "_at_unix_ms"] is not None)
    _require(sum(row["lifecycle"] == "attached" for row in endpoints) <= 1, "endpoint_not_unique")
    attention = _rows(connection, """SELECT event_id, target_agent_id, workflow_session_id,
        owner_principal_kind, owner_principal_digest, last_meaningful_activity_at_ms,
        last_seen_at_ms, goal_revision, created_at_unix_ms,
        task_id IS NULL AND task_attempt_id IS NULL AND terminal_task_state IS NULL AS source_valid,
        COUNT(*) OVER() AS _total FROM wc_agent_attention_events
        WHERE goal_id=? AND kind='goal_workflow_stalled'
        ORDER BY created_at_unix_ms, event_id LIMIT 128""", (goal_id,), bounded=True)
    attention_map = {}
    for event in attention:
        _identifier(event["event_id"], "wc_attention_event_")
        _require(event["event_id"] not in attention_map)
        attention_map[event["event_id"]] = event
        _require(event.pop("target_agent_id") == agent_id and
                 event.pop("workflow_session_id") == session_id, "attention_identity_mismatch")
        _require(_owner(event) == owner, "owner_mismatch")
        _require(event.pop("source_valid") == 1, "attention_identity_mismatch")
        _timestamps(event, ("last_meaningful_activity_at_ms", "last_seen_at_ms", "created_at_unix_ms"))
        _integer(event["goal_revision"], minimum=1)
        _require(event["goal_revision"] <= goal["revision"])
        _require(event["last_meaningful_activity_at_ms"] < event["last_seen_at_ms"] <= event["created_at_unix_ms"])
    wakes = _rows(connection, """SELECT w.wake_id,w.source_event_id,w.target_agent_id,w.trigger_kind,
        w.state,w.revision,w.created_at_unix_ms,w.updated_at_unix_ms,w.consumed_at_unix_ms,
        w.claimed_attempt_id,w.claimed_endpoint_id,w.claimed_controller_generation,
        w.claim_lease_expires_at_unix_ms,w.consumed_by_endpoint_id,w.consumed_controller_generation,
        COUNT(*) OVER() AS _total FROM wc_agent_wakes w JOIN wc_agent_attention_events e
        ON e.event_id=w.source_event_id WHERE e.goal_id=? AND e.kind='goal_workflow_stalled'
        ORDER BY w.created_at_unix_ms,w.wake_id LIMIT 128""", (goal_id,), bounded=True)
    attempts = _rows(connection, """SELECT a.attempt_id,a.wake_id,a.endpoint_id,a.controller_generation,
        a.state,a.claimed_at_unix_ms,a.claim_lease_expires_at_unix_ms,a.prepared_at_unix_ms,
        a.delivered_at_unix_ms,a.delivery_unknown_at_unix_ms,a.consumed_at_unix_ms,a.revoked_at_unix_ms,
        COUNT(*) OVER() AS _total FROM wc_agent_wake_attempts a JOIN wc_agent_wakes w ON w.wake_id=a.wake_id
        JOIN wc_agent_attention_events e ON e.event_id=w.source_event_id
        WHERE e.goal_id=? AND e.kind='goal_workflow_stalled'
        ORDER BY a.claimed_at_unix_ms,a.attempt_id LIMIT 128""", (goal_id,), bounded=True)
    _validate_delivery(wakes, attempts, endpoint_map, attention_map, agent_id)
    return {"schema_version": 1, "observed_at_ms": now_ms,
            "selectors": {"goal_id": goal_id, "agent_id": agent_id, "session_id": session_id},
            "goal": goal, "agent": agent, "endpoints": endpoints, "attention": attention,
            "wakes": wakes, "attempts": attempts}


def _validate_delivery(wakes, attempts, endpoints, attention, agent_id):
    wake_map, attempt_map, sources = {}, {}, set()
    for wake in wakes:
        _identifier(wake["wake_id"], "wc_wake_")
        _identifier(wake["source_event_id"], "wc_attention_event_")
        _require(wake["wake_id"] not in wake_map and wake["source_event_id"] not in sources)
        wake_map[wake["wake_id"]] = wake
        sources.add(wake["source_event_id"])
        _require(wake["source_event_id"] in attention, "wake_identity_mismatch")
        _require(wake.pop("target_agent_id") == agent_id and
                 wake.pop("trigger_kind") == "attention_event", "wake_identity_mismatch")
        _state(wake["state"], {"pending", "claimed", "prepared", "delivered", "delivery_unknown", "consumed", "retired"})
        _integer(wake["revision"], minimum=1)
        _timestamps(wake, ("created_at_unix_ms", "updated_at_unix_ms"),
                    ("claim_lease_expires_at_unix_ms", "consumed_at_unix_ms"))
        _require(wake["created_at_unix_ms"] >= attention[wake["source_event_id"]]["created_at_unix_ms"])
        _require(wake["updated_at_unix_ms"] >= wake["created_at_unix_ms"])
        for field, prefix in (("claimed_attempt_id", "wc_wake_attempt_"), ("claimed_endpoint_id", "wc_endpoint_"),
                              ("consumed_by_endpoint_id", "wc_endpoint_")):
            _identifier(wake[field], prefix, nullable=True)
        for field in ("claimed_controller_generation", "consumed_controller_generation"):
            _integer(wake[field], nullable=True, minimum=1)
    for attempt in attempts:
        _identifier(attempt["attempt_id"], "wc_wake_attempt_")
        _identifier(attempt["wake_id"], "wc_wake_")
        _identifier(attempt["endpoint_id"], "wc_endpoint_")
        _require(attempt["attempt_id"] not in attempt_map)
        attempt_map[attempt["attempt_id"]] = attempt
        _require(attempt["wake_id"] in wake_map, "attempt_identity_mismatch")
        endpoint = endpoints.get(attempt["endpoint_id"])
        _integer(attempt["controller_generation"], minimum=1)
        _require(endpoint is not None and endpoint["controller_generation"] == attempt["controller_generation"], "generation_mismatch")
        _state(attempt["state"], {"claimed", "prepared", "delivered", "delivery_unknown", "consumed", "revoked"})
        _timestamps(attempt, ("claimed_at_unix_ms", "claim_lease_expires_at_unix_ms"),
                    ("prepared_at_unix_ms", "delivered_at_unix_ms", "delivery_unknown_at_unix_ms", "consumed_at_unix_ms", "revoked_at_unix_ms"))
        claimed = attempt["claimed_at_unix_ms"]
        _require(claimed >= wake_map[attempt["wake_id"]]["created_at_unix_ms"])
        _require(attempt["claim_lease_expires_at_unix_ms"] >= claimed, "attempt_fence_mismatch")
        for field in ("prepared_at_unix_ms", "delivered_at_unix_ms", "delivery_unknown_at_unix_ms", "consumed_at_unix_ms", "revoked_at_unix_ms"):
            _require(attempt[field] is None or attempt[field] >= claimed, "attempt_fence_mismatch")
        if attempt["state"] in {"claimed", "revoked"}:
            _require(all(attempt[field] is None for field in ("prepared_at_unix_ms", "delivered_at_unix_ms",
                         "delivery_unknown_at_unix_ms", "consumed_at_unix_ms")), "attempt_fence_mismatch")
        if attempt["state"] == "prepared":
            _require(attempt["delivered_at_unix_ms"] is None and
                     attempt["delivery_unknown_at_unix_ms"] is None, "attempt_fence_mismatch")
        if attempt["state"] in {"prepared", "delivered", "delivery_unknown", "consumed"}:
            _require(attempt["prepared_at_unix_ms"] is not None, "attempt_fence_mismatch")
        for state_name, field in (("delivered", "delivered_at_unix_ms"), ("delivery_unknown", "delivery_unknown_at_unix_ms"),
                                  ("consumed", "consumed_at_unix_ms"), ("revoked", "revoked_at_unix_ms")):
            if attempt["state"] == state_name:
                _require(attempt[field] is not None, "attempt_fence_mismatch")
        for field in ("delivered_at_unix_ms", "delivery_unknown_at_unix_ms", "consumed_at_unix_ms"):
            if attempt[field] is not None:
                _require(attempt["prepared_at_unix_ms"] is not None and attempt[field] >= attempt["prepared_at_unix_ms"], "attempt_fence_mismatch")
        _require((attempt["state"] == "consumed") == (attempt["consumed_at_unix_ms"] is not None), "attempt_fence_mismatch")
        _require((attempt["state"] == "revoked") == (attempt["revoked_at_unix_ms"] is not None), "attempt_fence_mismatch")
    for wake in wakes:
        claim_fields = ("claimed_attempt_id", "claimed_endpoint_id", "claimed_controller_generation")
        if wake["state"] in {"pending", "retired"}:
            _require(all(wake[field] is None for field in claim_fields) and
                     wake["claim_lease_expires_at_unix_ms"] is None, "wake_fence_mismatch")
        else:
            attempt = attempt_map.get(wake["claimed_attempt_id"])
            _require(attempt is not None and attempt["wake_id"] == wake["wake_id"] and
                     attempt["endpoint_id"] == wake["claimed_endpoint_id"] and
                     attempt["controller_generation"] == wake["claimed_controller_generation"] and
                     attempt["state"] == wake["state"], "wake_fence_mismatch")
            if wake["state"] == "claimed":
                _require(wake["claim_lease_expires_at_unix_ms"] == attempt["claim_lease_expires_at_unix_ms"], "wake_fence_mismatch")
            else:
                _require(wake["claim_lease_expires_at_unix_ms"] is None, "wake_fence_mismatch")
        consumed_fields = ("consumed_at_unix_ms", "consumed_by_endpoint_id", "consumed_controller_generation")
        if wake["state"] == "consumed":
            _require(wake["consumed_by_endpoint_id"] == attempt["endpoint_id"] and
                     wake["consumed_controller_generation"] == attempt["controller_generation"] and
                     wake["consumed_at_unix_ms"] == attempt["consumed_at_unix_ms"] and
                     wake["claim_lease_expires_at_unix_ms"] is None, "consume_identity_mismatch")
        else:
            _require(all(wake[field] is None for field in consumed_fields), "consume_identity_mismatch")
    _require(sources == set(attention), "attention_wake_missing")
    for attempt in attempts:
        if attempt["state"] != "revoked":
            _require(wake_map[attempt["wake_id"]]["claimed_attempt_id"] == attempt["attempt_id"], "attempt_identity_mismatch")


def snapshot(database: Path, *, goal_id: str, agent_id: str, session_id: str, now_ms: int) -> dict:
    """Read one consistent snapshot or raise a body-free EvidenceError code."""
    _identifier(goal_id, "wc_goal_")
    _identifier(agent_id, "wc_dagent_")
    _identifier(session_id, "wc_sess_")
    _integer(now_ms)
    connection = None
    try:
        path = Path(os.path.abspath(database))
        identity = _file_identity(path)
        deadline = time.monotonic() + QUERY_SECONDS
        connection = sqlite3.connect(path.as_uri() + "?mode=ro", uri=True, timeout=0.1)
        connection.row_factory = sqlite3.Row
        connection.setlimit(sqlite3.SQLITE_LIMIT_LENGTH, 1024 * 1024)
        connection.set_progress_handler(lambda: int(time.monotonic() >= deadline), 1000)
        connection.execute("PRAGMA query_only=ON")
        connection.execute("PRAGMA trusted_schema=OFF")
        connection.execute("BEGIN")
        result = _read(connection, goal_id, agent_id, session_id, now_ms)
        connection.rollback()
        _require(time.monotonic() < deadline, "query_budget_exceeded")
        _require(_file_identity(path) == identity, "database_replaced")
        result["db_identity"] = identity
        _require(len(json.dumps(result, separators=(",", ":"), ensure_ascii=True).encode()) <= MAX_OUTPUT_BYTES, "output_limit")
        return result
    except EvidenceError:
        raise
    except sqlite3.Error as error:
        if getattr(error, "sqlite_errorcode", None) == sqlite3.SQLITE_INTERRUPT:
            raise EvidenceError("query_budget_exceeded") from None
        raise EvidenceError("database_or_schema_unavailable") from None
    except (OSError, ValueError, TypeError, KeyError, OverflowError):
        raise EvidenceError("database_unavailable_or_invalid") from None
    finally:
        if connection is not None:
            connection.close()
