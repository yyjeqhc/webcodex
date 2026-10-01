#!/usr/bin/env python3
"""Read an explicitly selected WebCodex Workflow Session for local Codex recovery."""

import argparse
import json
from pathlib import Path
import sys
import urllib.error
import urllib.request
import unicodedata

from external_observation_hook import AdapterError, MAX_BYTES, NoRedirect, load_config, private_file



def _bounded_text(value, max_bytes, nullable=False):
    if value is None:
        return nullable
    return (isinstance(value, str)
            and len(value.encode("utf-8")) <= max_bytes
            and not any(unicodedata.category(ch) == "Cc" for ch in value))


def _valid_excerpt(value, max_bytes, nullable=False):
    return (isinstance(value, dict)
            and set(value) == {"excerpt", "truncated"}
            and _bounded_text(value.get("excerpt"), max_bytes, nullable)
            and type(value.get("truncated")) is bool)


def _valid_goal_candidate(value):
    return (isinstance(value, dict)
            and set(value) == {"goal_id", "title", "title_truncated", "lifecycle", "revision"}
            and _bounded_text(value.get("goal_id"), 128)
            and _bounded_text(value.get("title"), 256)
            and type(value.get("title_truncated")) is bool
            and value.get("lifecycle") == "active"
            and type(value.get("revision")) is int
            and value["revision"] >= 1)


def _valid_goal_detail(value):
    if (not isinstance(value, dict)
            or set(value) != {"goal_id", "title", "title_truncated", "lifecycle", "revision", "objective", "plan"}
            or not _bounded_text(value.get("goal_id"), 128)
            or not _bounded_text(value.get("title"), 256)
            or type(value.get("title_truncated")) is not bool
            or value.get("lifecycle") != "active"
            or type(value.get("revision")) is not int
            or value["revision"] < 1
            or not _valid_excerpt(value.get("objective"), 1024)):
        return False
    plan = value.get("plan")
    if not isinstance(plan, dict) or set(plan) != {"completion_conditions", "steps", "current_step_id", "checkpoint"}:
        return False

    conditions = plan.get("completion_conditions")
    if (not isinstance(conditions, dict)
            or set(conditions) != {"items", "total", "returned", "truncated", "content_truncated"}
            or not isinstance(conditions.get("items"), list)
            or len(conditions["items"]) > 8
            or not all(_bounded_text(item, 256) for item in conditions["items"])
            or type(conditions.get("total")) is not int
            or type(conditions.get("returned")) is not int
            or conditions["total"] < 0
            or conditions["returned"] != len(conditions["items"])
            or conditions["returned"] > conditions["total"]
            or type(conditions.get("truncated")) is not bool
            or conditions["truncated"] != (conditions["returned"] < conditions["total"])
            or type(conditions.get("content_truncated")) is not bool):
        return False

    steps = plan.get("steps")
    if (not isinstance(steps, dict)
            or set(steps) != {"items", "total", "returned", "truncated"}
            or not isinstance(steps.get("items"), list)
            or len(steps["items"]) > 32
            or type(steps.get("total")) is not int
            or type(steps.get("returned")) is not int
            or steps["total"] < 0
            or steps["returned"] != len(steps["items"])
            or steps["returned"] > steps["total"]
            or type(steps.get("truncated")) is not bool
            or steps["truncated"] != (steps["returned"] < steps["total"])):
        return False
    for step in steps["items"]:
        if (not isinstance(step, dict)
                or set(step) != {"id", "title", "title_truncated", "status"}
                or not _bounded_text(step.get("id"), 32)
                or not _bounded_text(step.get("title"), 128)
                or type(step.get("title_truncated")) is not bool
                or step.get("status") not in {"pending", "in_progress", "completed"}):
            return False

    current_step_id = plan.get("current_step_id")
    if current_step_id is not None and not _bounded_text(current_step_id, 32):
        return False
    checkpoint = plan.get("checkpoint")
    return (isinstance(checkpoint, dict)
            and set(checkpoint) == {"summary", "at_unix_ms"}
            and _valid_excerpt(checkpoint.get("summary"), 512, nullable=True)
            and (checkpoint.get("at_unix_ms") is None
                 or (type(checkpoint.get("at_unix_ms")) is int)))


def _validate_goal_context(value):
    if (not isinstance(value, dict)
            or set(value) != {"version", "source", "status", "reason_code", "truncated", "goal", "candidates"}
            or value.get("version") != 1
            or value.get("source") != "explicit_workflow_session_correlation"
            or value.get("status") not in {"available", "selection_required", "unavailable"}
            or type(value.get("truncated")) is not bool
            or not isinstance(value.get("candidates"), list)
            or len(value["candidates"]) > 8
            or not all(_valid_goal_candidate(candidate) for candidate in value["candidates"])):
        raise AdapterError("handoff_goal_context_invalid")

    status = value["status"]
    if status == "available":
        valid = (value.get("reason_code") is None
                 and value["truncated"] is False
                 and value["candidates"] == []
                 and _valid_goal_detail(value.get("goal")))
    elif status == "selection_required":
        ids = [candidate["goal_id"] for candidate in value["candidates"]]
        valid = (value.get("reason_code") == "multiple_active_goals"
                 and value.get("goal") is None
                 and len(value["candidates"]) >= 2
                 and len(ids) == len(set(ids)))
    else:
        valid = (value.get("reason_code") == "store_unavailable"
                 and value["truncated"] is False
                 and value.get("goal") is None
                 and value["candidates"] == [])
    if not valid:
        raise AdapterError("handoff_goal_context_invalid")


def read_handoff(config, timeout=6, current_dir=None):
    # A private operator configuration selects the exact Project and Session.
    # The current directory can only narrow that selection, never redirect it.
    if not (current_dir or Path.cwd()).resolve().is_relative_to(Path(config["project_root"])):
        raise AdapterError("outside_configured_project")
    authorization = private_file(config["authorization_file"]).decode().strip()
    if not authorization or "\n" in authorization or "\r" in authorization:
        raise AdapterError("invalid_authorization_file")
    request = urllib.request.Request(
        config["server_url"].rstrip("/") + "/api/tools/call",
        data=json.dumps({
            "tool": "get_session_handoff_state",
            "params": {
                "project": config["project"],
                "session_id": config["workflow_session_id"],
            },
        }).encode(),
        headers={"Content-Type": "application/json", "Authorization": authorization},
        method="POST",
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=timeout) as response:
        raw = response.read(MAX_BYTES + 1)
    if len(raw) > MAX_BYTES:
        raise AdapterError("oversized_handoff_response")
    result = json.loads(raw)
    if not isinstance(result, dict) or result.get("success") is not True:
        raise AdapterError("handoff_read_failed")
    output = result.get("output")
    if (not isinstance(output, dict)
            or output.get("project") != config["project"]
            or output.get("session_id") != config["workflow_session_id"]
            or not isinstance(output.get("handoff_brief"), dict)):
        raise AdapterError("handoff_identity_mismatch")
    brief = output["handoff_brief"]
    if (not isinstance(brief.get("session"), dict)
            or brief["session"].get("session_id") != config["workflow_session_id"]):
        raise AdapterError("handoff_identity_mismatch")
    if (not isinstance(brief.get("basis"), dict)
            or not isinstance(brief["basis"].get("complete"), bool)):
        raise AdapterError("handoff_basis_missing")
    external = brief.get("external_observations")
    coverage = external.get("coverage") if isinstance(external, dict) else None
    if (not isinstance(external, dict)
            or external.get("provenance") != "external_report"
            or not isinstance(coverage, dict)
            or coverage.get("complete") is not False):
        raise AdapterError("handoff_external_observations_missing")
    if brief.get("deterministic") is not True or brief.get("llm_summary") is not False:
        raise AdapterError("handoff_contract_invalid")
    goal_context = output.get("goal_context")
    if goal_context is not None:
        _validate_goal_context(goal_context)
    recovered = {
        "status": "read",
        "project": output["project"],
        "session_id": output["session_id"],
        "handoff_brief": brief,
        "recovery_note": "Historical evidence only. Check current project rules, files, Git state and unresolved work before acting; never replay an unknown operation.",
    }
    if goal_context is not None:
        recovered["goal_context"] = goal_context
    return recovered


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--config", required=True, type=Path)
    args = parser.parse_args()
    try:
        config = load_config(args.config)
        print(json.dumps(read_handoff(config), ensure_ascii=False, sort_keys=True))
        return 0
    except AdapterError as error:
        print(json.dumps({"status": "unavailable", "reason": str(error)}), file=sys.stderr)
    except (OSError, ValueError, UnicodeError, urllib.error.URLError):
        print(json.dumps({"status": "unavailable", "reason": "handoff_read_unconfirmed"}), file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
