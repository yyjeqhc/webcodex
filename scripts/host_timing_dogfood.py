#!/usr/bin/env python3
"""Read-only, aggregate-only Host timing dogfood report from existing ActionAudit.

No production telemetry, raw payload export, or inferred model turns. Cohort
configuration is operator-declared and deliberately not presented as verified.
"""

from __future__ import annotations

import argparse
from collections import Counter
from datetime import datetime, timedelta, timezone
import json
import math
from pathlib import Path
import re
import sqlite3
import sys

MAX_ROWS = 100_000
STRUCTURED = frozenset({
    "run_process", "run_script", "run_shell", "run_skill_resource",
    "cargo_check", "cargo_test", "cargo_fmt", "go_test",
    "project_build", "project_validate",
})
OBSERVATION = frozenset({"observe_jobs", "job_tail"})
READINESS = "wait_for_job_readiness"
COHORT_FIELDS = frozenset({
    "schema_version", "cohort", "case_id", "base_revision", "host_profile",
    "host_budget_secs", "sync_wait_secs", "continuation_wait_secs",
})


class ReportError(ValueError):
    pass


def _validate_policy(value: object) -> dict:
    if (not isinstance(value, dict) or set(value) != COHORT_FIELDS
            or type(value.get("schema_version")) is not int or value["schema_version"] != 1):
        raise ReportError("cohort configuration must use the exact schema v1 fields")
    for field in ("cohort", "case_id"):
        if not isinstance(value[field], str) or not re.fullmatch(r"[a-z0-9][a-z0-9_-]{0,47}", value[field]):
            raise ReportError(f"{field} must be a bounded non-identifying slug")
    if not isinstance(value["base_revision"], str) or not re.fullmatch(r"[0-9a-f]{40}", value["base_revision"]):
        raise ReportError("base_revision must be a full 40-character Git SHA")
    if value["host_profile"] != "host_code_mode":
        raise ReportError("only host_code_mode paired experiments are supported")
    for field, maximum in (("host_budget_secs", 3600), ("sync_wait_secs", 60), ("continuation_wait_secs", 100)):
        v = value[field]
        if type(v) is not int or not 1 <= v <= maximum:
            raise ReportError(f"{field} must be a bounded positive integer")
    if value["sync_wait_secs"] > max(1, value["host_budget_secs"] - 5):
        raise ReportError("sync_wait_secs exceeds declared safe Host budget")
    if value["continuation_wait_secs"] > max(1, value["host_budget_secs"] - 5):
        raise ReportError("continuation_wait_secs exceeds declared safe Host budget")
    return value


def _load_policy(path: Path) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, ValueError) as exc:
        raise ReportError("cannot read cohort configuration JSON") from exc
    return _validate_policy(value)


def _timestamp(raw: str) -> int:
    try:
        value = datetime.fromisoformat(raw.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ReportError("timestamps must be ISO 8601 with explicit UTC timezone") from exc
    if value.tzinfo is None or value.utcoffset() != timedelta(0):
        raise ReportError("timestamps must use UTC (Z or +00:00)")
    return int(value.timestamp())


def _percentile(values: list[int], fraction: float) -> int | None:
    if not values:
        return None
    ordered = sorted(values)
    return ordered[math.ceil(fraction * len(ordered)) - 1]


def _query(db: Path, *, project: str, start: int, end: int) -> list[tuple]:
    if not db.is_file():
        raise ReportError("audit database is not a file")
    try:
        connection = sqlite3.connect(f"{db.resolve().as_uri()}?mode=ro", uri=True)
        try:
            connection.execute("PRAGMA query_only=ON")
            rows = connection.execute(
                """SELECT action_name, operation, duration_ms, summary_json, window_meaningful
                   FROM action_events
                   WHERE started_at >= ? AND started_at < ? AND project = ?
                   ORDER BY started_at, event_id LIMIT ?""",
                (start, end, project, MAX_ROWS + 1),
            ).fetchall()
        finally:
            connection.close()
    except sqlite3.Error as exc:
        raise ReportError("could not query ActionAudit read-only") from exc
    if len(rows) > MAX_ROWS:
        raise ReportError("selection exceeds the 100000-event limit; split the time range")
    return rows


def summarize(db: Path, policy: dict, project: str, start: int, end: int) -> dict:
    if not project or len(project) > 160:
        raise ReportError("an exact, bounded --project is required")
    if not 0 < end - start <= 7 * 86400:
        raise ReportError("select a positive UTC range of at most 7 days")
    rows = _query(db, project=project, start=start, end=end)
    counts = Counter()
    structured_ms: list[int] = []
    observation_ms: list[int] = []
    readiness_ms: list[int] = []
    unknown_states = 0
    unproven = 0
    for action, operation, duration, raw, meaningful in rows:
        if action != "toolsCall" or not isinstance(operation, str):
            continue
        counts["outer_tool_calls"] += 1
        counts["meaningful_outer_calls_proxy"] += int(meaningful == 1)
        try:
            summary = json.loads(raw) if isinstance(raw, str) else None
        except ValueError:
            summary = None
        record = summary.get("model_ergonomics") if isinstance(summary, dict) else None
        if (not isinstance(record, dict) or record.get("tool_name") != operation
                or type(record.get("schema_version")) is not int
                or record["schema_version"] < 1
                or type(record.get("success")) is not bool):
            unproven += 1
            continue
        counts["canonical_calls"] += 1
        state = record.get("execution_state")
        if operation in STRUCTURED:
            counts["structured_calls"] += 1
            if type(duration) is int and duration >= 0:
                structured_ms.append(duration)
            if state in ("pending", "running"):
                counts["handoff_state_calls"] += 1
            elif state == "completed":
                counts["in_call_completed_state_calls"] += 1
            else:
                counts["structured_state_unclassified"] += 1
        if operation in OBSERVATION:
            counts["observation_calls"] += 1
            if type(duration) is int and duration >= 0:
                observation_ms.append(duration)
        if operation == READINESS:
            counts["readiness_calls"] += 1
            telemetry = record.get("readiness")
            waited = telemetry.get("waited_ms") if isinstance(telemetry, dict) else None
            if type(waited) is int and waited >= 0:
                readiness_ms.append(waited)
            else:
                counts["readiness_wait_unavailable"] += 1
        if state == "outcome_unknown" or record.get("error_kind") == "dispatch_hard_timeout":
            unknown_states += 1
    return {
        "schema_version": 1,
        "kind": "host_timing_dogfood_summary",
        "cohort": policy,
        "cohort_evidence": "operator_declared_not_verified_by_audit",
        "duration_basis": "legacy_action_audit_pre_response_handoff_not_host_latency",
        "selection": {"start_utc": datetime.fromtimestamp(start, tz=timezone.utc).isoformat(),
                      "end_utc": datetime.fromtimestamp(end, tz=timezone.utc).isoformat(),
                      "project_filter_applied": True, "events_selected": len(rows)},
        "counts": {
            **{key: counts[key] for key in (
                "outer_tool_calls", "canonical_calls", "meaningful_outer_calls_proxy",
                "structured_calls", "handoff_state_calls", "in_call_completed_state_calls",
                "structured_state_unclassified", "observation_calls", "readiness_calls",
                "readiness_wait_unavailable",
            )},
            "canonical_record_unavailable": unproven,
            "outcome_unknown_calls": unknown_states,
        },
        "timing_ms": {
            "structured_request_p50": _percentile(structured_ms, .5),
            "structured_request_p90": _percentile(structured_ms, .9),
            "observation_request_p50": _percentile(observation_ms, .5),
            "observation_request_p90": _percentile(observation_ms, .9),
            "readiness_wait_p50": _percentile(readiness_ms, .5),
            "readiness_wait_p90": _percentile(readiness_ms, .9),
        },
        "availability": {
            "structured_request_duration_samples": len(structured_ms),
            "observation_request_duration_samples": len(observation_ms),
            "readiness_wait_samples": len(readiness_ms),
        },
        "task_wall_time_ms": None,
        "model_turns": None,
        "final_job_outcomes": None,
        "correctness": None,
        "interpretation": "Legacy ActionAudit duration may end before response handoff; not Host-visible latency. Handed-off MCP calls do not prove final Job success; no causal or model-turn inference.",
    }


COUNT_FIELDS = (
    "outer_tool_calls", "canonical_calls", "meaningful_outer_calls_proxy",
    "structured_calls", "handoff_state_calls", "in_call_completed_state_calls",
    "structured_state_unclassified", "observation_calls", "readiness_calls",
    "readiness_wait_unavailable", "canonical_record_unavailable", "outcome_unknown_calls",
)
COMPARE_METRICS = (
    "structured_calls", "handoff_state_calls", "observation_calls",
    "readiness_calls", "outcome_unknown_calls",
)
DURATION_SAMPLE_FIELDS = (
    "structured_request_duration_samples", "observation_request_duration_samples",
    "readiness_wait_samples",
)
DURATION_FIELDS = (
    "structured_request_p50", "structured_request_p90", "observation_request_p50",
    "observation_request_p90", "readiness_wait_p50", "readiness_wait_p90",
)


def _validate_comparison_summary(value: object) -> dict:
    # Strictly validate the shape before dereferencing/arithmetic. Parsed JSON
    # from disk is untrusted and must not be able to fabricate negative deltas.
    if (not isinstance(value, dict) or type(value.get("schema_version")) is not int
            or value["schema_version"] != 1
            or value.get("kind") != "host_timing_dogfood_summary"):
        raise ReportError("both inputs must be host_timing_dogfood_summary v1")
    _validate_policy(value.get("cohort"))
    if (value.get("cohort_evidence") != "operator_declared_not_verified_by_audit"
            or value.get("duration_basis") not in (
                None, "legacy_action_audit_pre_response_handoff_not_host_latency",
            )):
        raise ReportError("unrecognized comparison evidence provenance")
    for name in ("model_turns", "task_wall_time_ms", "final_job_outcomes", "correctness"):
        if name not in value or value[name] is not None:
            raise ReportError("comparison requires unproven task outcomes to remain null")
    counts = value.get("counts")
    if not isinstance(counts, dict) or any(
        type(counts.get(key)) is not int or not 0 <= counts[key] <= MAX_ROWS
        for key in COUNT_FIELDS
    ):
        raise ReportError("summary counters must be nonnegative integers")
    if (counts["canonical_calls"] + counts["canonical_record_unavailable"] != counts["outer_tool_calls"]
            or counts["meaningful_outer_calls_proxy"] > counts["outer_tool_calls"]
            or counts["structured_calls"] + counts["observation_calls"] + counts["readiness_calls"] > counts["canonical_calls"]
            or (counts["handoff_state_calls"] + counts["in_call_completed_state_calls"]
                + counts["structured_state_unclassified"]) != counts["structured_calls"]
            or counts["readiness_wait_unavailable"] > counts["readiness_calls"]
            or counts["outcome_unknown_calls"] > counts["canonical_calls"]):
        raise ReportError("summary counter relationships are inconsistent")
    selection = value.get("selection")
    if (not isinstance(selection, dict)
            or selection.get("project_filter_applied") is not True
            or type(selection.get("events_selected")) is not int
            or not counts["outer_tool_calls"] <= selection["events_selected"] <= MAX_ROWS):
        raise ReportError("summary selection bounds are inconsistent")
    availability = value.get("availability")
    timing = value.get("timing_ms")
    if (not isinstance(availability, dict) or not isinstance(timing, dict)
            or not all(key in timing for key in DURATION_FIELDS)
            or any(type(availability.get(key)) is not int or availability[key] < 0
                   for key in DURATION_SAMPLE_FIELDS)
            or any(timing.get(key) is not None and (
                type(timing[key]) is not int or timing[key] < 0
            ) for key in DURATION_FIELDS)):
        raise ReportError("summary timing and sample counts are invalid")
    if any(
        availability[sample] > counts[category]
        for sample, category in zip(DURATION_SAMPLE_FIELDS, (
            "structured_calls", "observation_calls", "readiness_calls"
        ))
    ):
        raise ReportError("summary duration samples exceed observed calls")
    for sample, p50, p90 in zip(DURATION_SAMPLE_FIELDS,
            DURATION_FIELDS[::2], DURATION_FIELDS[1::2]):
        if ((availability[sample] == 0) != (timing.get(p50) is None)
                or (availability[sample] == 0) != (timing.get(p90) is None)
                or (timing[p50] is not None and timing[p50] > timing[p90])):
            raise ReportError("summary duration quantiles contradict sample availability")
    return value


def compare(left: dict, right: dict) -> dict:
    left = _validate_comparison_summary(left)
    right = _validate_comparison_summary(right)
    a, b = left["cohort"], right["cohort"]
    comparable = all(a.get(key) == b.get(key) for key in (
        "case_id", "base_revision", "host_profile", "host_budget_secs", "continuation_wait_secs",
    )) and a.get("sync_wait_secs") != b.get("sync_wait_secs") and a.get("cohort") != b.get("cohort")
    metrics = COMPARE_METRICS
    return {
        "schema_version": 1, "kind": "host_timing_dogfood_comparison",
        "comparable_configuration": comparable,
        "reason": None if comparable else "cohort case/base/profile/budget/continuation must match; only sync ceiling and cohort name may differ",
        "descriptive_deltas": {
            metric: right["counts"][metric] - left["counts"][metric] if comparable else None
            for metric in metrics
        },
        "final_job_outcomes_comparable": False,
        "causal_claim_supported": False,
        "notes": "Operator-declared configuration, possibly different workloads; use paired correctness evidence before decisions.",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("summarize")
    p.add_argument("--audit-db", type=Path, required=True)
    p.add_argument("--policy", type=Path, required=True)
    p.add_argument("--project", required=True)
    p.add_argument("--since-utc", required=True)
    p.add_argument("--until-utc", required=True)
    p.add_argument("--output", type=Path)
    c = sub.add_parser("compare")
    c.add_argument("--baseline", type=Path, required=True)
    c.add_argument("--candidate", type=Path, required=True)
    c.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "summarize":
            result = summarize(args.audit_db, _load_policy(args.policy), args.project,
                               _timestamp(args.since_utc), _timestamp(args.until_utc))
        else:
            result = compare(json.loads(args.baseline.read_text(encoding="utf-8")),
                             json.loads(args.candidate.read_text(encoding="utf-8")))
        output = json.dumps(result, sort_keys=True, indent=2, ensure_ascii=False) + "\n"
        if args.output:
            args.output.write_text(output, encoding="utf-8")
        else:
            sys.stdout.write(output)
        return 0
    except ReportError as exc:
        # Every ReportError carries only a static, bounded diagnostic.
        print(f"host_timing_dogfood: {exc}", file=sys.stderr)
        return 2
    except (OSError, ValueError, KeyError, TypeError):
        # Never echo source paths, raw event contents or configuration values.
        print("host_timing_dogfood: invalid or unavailable input", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
