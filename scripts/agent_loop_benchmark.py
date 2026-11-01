#!/usr/bin/env python3
"""Run reproducible paired Agent Loop benchmark samples over fresh Git worktrees.

The script is orchestration only. A caller-supplied Host driver performs the real
Direct/Code Mode task. This runner owns fresh worktrees, bounded annotation,
existing agent_loop_report summarization/comparison, fixture-side oracles, sample
status retention, and cleanup of its own temporary resources.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import signal
import sqlite3
import stat
import shutil
import shlex
import subprocess
import sys
import tempfile
import time
import threading
from pathlib import Path
from typing import Any

try:
    from scripts import agent_loop_report as report
    from scripts import agent_temporal_evidence as temporal
    from scripts.e2e_generic_agent_ws import GenericFixture, GENERIC_CASES, pressure_script, mixed_note_script, encoded_csv_script
    from scripts.agent_knowledge_fixture import KnowledgeFixture, CASE_IDS as KNOWLEDGE_CASES
    from scripts import agent_knowledge_evidence as knowledge
except ModuleNotFoundError:
    import agent_loop_report as report
    import agent_temporal_evidence as temporal
    from e2e_generic_agent_ws import GenericFixture, GENERIC_CASES, pressure_script, mixed_note_script, encoded_csv_script
    from agent_knowledge_fixture import KnowledgeFixture, CASE_IDS as KNOWLEDGE_CASES
    import agent_knowledge_evidence as knowledge

STATUS_VALUES = frozenset(("pass", "fail", "partial", "unsupported"))
DEFAULT_CASES = ("readonly_review", "guarded_multi_file_edit", "long_validation_handoff")
DEFAULT_SELECTION_CASES = (
    "selection_single_read",
    "selection_batch_read",
    "selection_cross_tool_review",
    "selection_compound_source_context",
)
VARIANTS = ("direct", "code_mode")
# Selected, already-persisted metadata only. Model turns and raw payloads cannot
# be reconstructed from Agent Loop summaries.
PAIRED_DELTA_METRICS = (
    "task_wall_time_ms",
    "outer_calls.total",
    "results.serialized_tool_result_bytes.total",
    "repair_turns.total",
)
MAX_DRIVER_RECEIPT_BYTES = 64 * 1024
MAX_DRIVER_RECEIPT_DEPTH = 128
MAX_FIXTURE_ORACLE_BYTES = 1024 * 1024
MAX_CASE_MANIFEST_BYTES = report.MAX_CASE_MANIFEST_BYTES
MAX_GIT_PATH_BYTES = 64 * 1024
MAX_GIT_PATHS = 512
CODE_MODE_TOOL_BY_SURFACE = {
    "read_only": "execute_code_mode",
    "validation": "execute_effectful_code_mode",
    "guarded_edit": "execute_mutating_code_mode",
}
MAX_DRIVER_SECS = 30 * 60


class BenchmarkError(ValueError):
    pass


class DriverTimeoutError(BenchmarkError):
    pass


class DriverPlatformUnsupported(BenchmarkError):
    pass


def _stable_json(value: Any) -> str:
    return json.dumps(value, indent=2, sort_keys=True, ensure_ascii=False) + "\n"


def _run(argv: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        argv,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )

def _terminate_driver_tree(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    if process.poll() is None:
        process.kill()
        process.wait()


def _run_driver(argv: list[str], *, cwd: Path, env: dict[str, str]) -> subprocess.CompletedProcess[bytes]:
    if os.name == "nt":
        raise DriverPlatformUnsupported(
            "paired benchmark Host driver requires POSIX process-group ownership"
        )

    try:
        process = subprocess.Popen(
            argv,
            cwd=cwd,
            env=env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except OSError as exc:
        raise BenchmarkError("could not start benchmark Host driver") from exc

    try:
        returncode = process.wait(timeout=MAX_DRIVER_SECS)
    except subprocess.TimeoutExpired as exc:
        _terminate_driver_tree(process)
        raise DriverTimeoutError("benchmark Host driver exceeded the 30 minute limit") from exc
    except BaseException:
        _terminate_driver_tree(process)
        raise
    _terminate_driver_tree(process)
    return subprocess.CompletedProcess(argv, returncode)


def _require_exact_revision(repo: Path, revision: str) -> str:
    completed = _run(["git", "rev-parse", "--verify", f"{revision}^{{commit}}"], cwd=repo)
    if completed.returncode != 0:
        raise BenchmarkError(f"could not resolve base revision: {revision}")
    resolved = completed.stdout.strip()
    if not report._is_exact_git_revision(resolved):
        raise BenchmarkError("resolved base revision is not an exact 40-hex commit")
    return resolved


def _git_root(path: Path) -> Path:
    completed = _run(["git", "rev-parse", "--show-toplevel"], cwd=path)
    if completed.returncode != 0:
        raise BenchmarkError("benchmark repo must be inside a Git worktree")
    return Path(completed.stdout.strip()).resolve()


def _head_revision(repo: Path) -> str:
    completed = _run(["git", "rev-parse", "HEAD"], cwd=repo)
    if completed.returncode != 0:
        raise BenchmarkError("could not resolve current checkout HEAD")
    head = completed.stdout.strip()
    if not report._is_exact_git_revision(head):
        raise BenchmarkError("current checkout HEAD is not an exact 40-hex commit")
    return head


def _read_manifest_at_revision(repo: Path, revision: str) -> str:
    object_name = f"{revision}:scripts/agent_loop_cases.json"
    size_result = _run(["git", "cat-file", "-s", object_name], cwd=repo)
    if size_result.returncode != 0:
        raise BenchmarkError("could not read benchmark case manifest at base revision")
    try:
        size = int(size_result.stdout.strip())
    except ValueError as exc:
        raise BenchmarkError("benchmark case manifest size is invalid") from exc
    if size < 0 or size > MAX_CASE_MANIFEST_BYTES:
        raise BenchmarkError("benchmark case manifest exceeds the 1 MiB limit")

    completed = subprocess.run(
        ["git", "show", object_name],
        cwd=repo,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if completed.returncode != 0 or len(completed.stdout) != size:
        raise BenchmarkError("could not read benchmark case manifest at base revision")
    try:
        return completed.stdout.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise BenchmarkError("benchmark case manifest must be UTF-8 JSON") from exc


def _case(manifest: dict[str, Any], case_id: str) -> dict[str, Any]:
    return report._case_by_id(manifest, case_id)


def _surface(case: dict[str, Any], variant: str) -> str:
    if variant == report.TOOL_SELECTION_VARIANT:
        if case.get("experiment") != "tool_selection":
            raise BenchmarkError("free selection requires an explicit tool_selection case")
        return report.TOOL_SELECTION_VARIANT
    if variant == "direct":
        return "direct"
    if variant == "host_code_mode":
        return report.HOST_CODE_MODE_SURFACE
    surface = case.get("code_mode_surface")
    canonical = report._canonical_code_mode_surface(surface)
    if canonical is None:
        raise BenchmarkError(f"case {case['id']} has no usable Code Mode surface")
    return canonical


def _worktree_add(repo: Path, path: Path, base_revision: str) -> None:
    completed = _run(["git", "worktree", "add", "--detach", str(path), base_revision], cwd=repo)
    if completed.returncode == 0:
        return

    add_error = completed.stderr.strip() or "git worktree add failed"
    try:
        _worktree_remove(repo, path)
    except BenchmarkError as cleanup_error:
        raise BenchmarkError(f"{add_error}; owned worktree rollback failed: {cleanup_error}") from cleanup_error
    raise BenchmarkError(add_error)


def _worktree_remove(repo: Path, path: Path) -> None:
    completed = _run(["git", "worktree", "remove", "--force", str(path)], cwd=repo)
    if completed.returncode == 0:
        return

    if path.exists():
        shutil.rmtree(path, ignore_errors=True)
    retry = _run(["git", "worktree", "remove", "--force", str(path)], cwd=repo)
    if retry.returncode == 0:
        return
    if not path.exists() and "is not a working tree" in retry.stderr:
        return
    raise BenchmarkError(retry.stderr.strip() or "failed to remove owned benchmark worktree")


def _workspace_status(workspace: Path) -> bool:
    try:
        process = subprocess.Popen(
            ["git", "status", "--porcelain", "--untracked-files=all"],
            cwd=workspace,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except OSError as exc:
        raise BenchmarkError("could not start git status") from exc

    assert process.stdout is not None
    with process.stdout:
        first = process.stdout.read(1)
    if first and process.poll() is None:
        process.kill()
    returncode = process.wait()
    if first:
        return True
    if returncode != 0:
        raise BenchmarkError("git status failed")
    return False


def _bounded_git_paths(
    argv: list[str],
    *,
    cwd: Path,
) -> tuple[bool, bool, list[bytes]]:
    try:
        process = subprocess.Popen(
            ["git", *argv],
            cwd=cwd,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
        )
    except OSError as exc:
        raise BenchmarkError("could not start bounded Git path query") from exc

    assert process.stdout is not None
    with process.stdout:
        raw = process.stdout.read(MAX_GIT_PATH_BYTES + 1)
    byte_overflow = len(raw) > MAX_GIT_PATH_BYTES
    if byte_overflow and process.poll() is None:
        process.kill()
    returncode = process.wait()
    if byte_overflow:
        return returncode == 0, True, []

    paths = [item for item in raw.split(b"\0") if item]
    if len(paths) > MAX_GIT_PATHS:
        return returncode == 0, True, []
    return returncode == 0, False, paths


def _fixture_oracle(
    case: dict[str, Any],
    workspace: Path,
    base_revision: str = "HEAD",
) -> dict[str, Any]:
    spec = (case.get("correctness") or {}).get("fixture_oracle")
    if not isinstance(spec, dict):
        return {"available": False, "passed": None, "checks": [], "reason": "case has no fixture_oracle"}

    checks: list[dict[str, Any]] = []
    for rel_path, expected in sorted((spec.get("files") or {}).items()):
        target = workspace / rel_path
        try:
            before = target.lstat()
        except OSError:
            checks.append({"path": rel_path, "passed": False, "reason": "file unavailable"})
            continue
        if not stat.S_ISREG(before.st_mode):
            checks.append({"path": rel_path, "passed": False, "reason": "file is not regular"})
            continue
        flags = os.O_RDONLY
        if hasattr(os, "O_NOFOLLOW"):
            flags |= os.O_NOFOLLOW
        try:
            fd = os.open(target, flags)
        except OSError:
            checks.append({"path": rel_path, "passed": False, "reason": "file unavailable"})
            continue
        try:
            opened = os.fstat(fd)
            if (
                not stat.S_ISREG(opened.st_mode)
                or opened.st_dev != before.st_dev
                or opened.st_ino != before.st_ino
            ):
                checks.append({"path": rel_path, "passed": False, "reason": "file identity changed"})
                continue
            with os.fdopen(fd, "rb", closefd=True) as handle:
                fd = -1
                raw = handle.read(MAX_FIXTURE_ORACLE_BYTES + 1)
        finally:
            if fd >= 0:
                os.close(fd)
        if len(raw) > MAX_FIXTURE_ORACLE_BYTES:
            checks.append({"path": rel_path, "passed": False, "reason": "file exceeds oracle byte limit"})
            continue
        try:
            text = raw.decode("utf-8")
        except UnicodeDecodeError:
            checks.append({"path": rel_path, "passed": False, "reason": "file is not UTF-8"})
            continue
        expected_text = expected.get("expected_text")
        if isinstance(expected_text, str):
            passed = text == expected_text
        else:
            required = expected.get("required_text") or []
            forbidden = expected.get("forbidden_text") or []
            passed = all(item in text for item in required) and all(item not in text for item in forbidden)
        checks.append({"path": rel_path, "passed": passed, "reason": None if passed else "text oracle mismatch"})

    expected_changed = sorted(spec.get("changed_files") or [])
    if expected_changed:
        tracked_ok, tracked_overflow, tracked_paths = _bounded_git_paths(
            ["diff", "--name-only", "-z", base_revision, "--"],
            cwd=workspace,
        )
        untracked_ok, untracked_overflow, untracked_paths = _bounded_git_paths(
            ["ls-files", "-z", "--others", "--exclude-standard"],
            cwd=workspace,
        )
        overflow = tracked_overflow or untracked_overflow
        changed_raw = sorted(set(tracked_paths) | set(untracked_paths)) if not overflow else []
        expected_raw = sorted(item.encode("utf-8") for item in expected_changed)
        changed = [
            item.decode("utf-8", errors="backslashreplace")
            for item in changed_raw
        ]
        checks.append(
            {
                "kind": "changed_files",
                "passed": tracked_ok and untracked_ok and not overflow and changed_raw == expected_raw,
                "expected": expected_changed,
                "actual": None if overflow else changed,
                "reason": (
                    "changed path listing exceeds oracle limit"
                    if overflow
                    else "changed path query failed"
                    if not tracked_ok or not untracked_ok
                    else None
                ),
            }
        )

    passed = bool(checks) and all(check.get("passed") is True for check in checks)
    return {"available": True, "passed": passed, "checks": checks, "reason": None}


def _load_driver_receipt(path: Path) -> dict[str, Any]:
    try:
        before = path.lstat()
    except OSError as exc:
        raise BenchmarkError("driver did not create its result receipt") from exc
    if not stat.S_ISREG(before.st_mode):
        raise BenchmarkError("driver result must be a regular file")
    if before.st_size > MAX_DRIVER_RECEIPT_BYTES:
        raise BenchmarkError("driver result exceeds the 64 KiB receipt limit")

    flags = os.O_RDONLY
    if hasattr(os, "O_NOFOLLOW"):
        flags |= os.O_NOFOLLOW
    try:
        fd = os.open(path, flags)
    except OSError as exc:
        raise BenchmarkError("driver result must be a readable regular file") from exc
    try:
        opened = os.fstat(fd)
        if (
            not stat.S_ISREG(opened.st_mode)
            or opened.st_dev != before.st_dev
            or opened.st_ino != before.st_ino
        ):
            raise BenchmarkError("driver result changed identity before read")
        with os.fdopen(fd, "rb", closefd=True) as handle:
            fd = -1
            raw = handle.read(MAX_DRIVER_RECEIPT_BYTES + 1)
    finally:
        if fd >= 0:
            os.close(fd)
    if len(raw) > MAX_DRIVER_RECEIPT_BYTES:
        raise BenchmarkError("driver result exceeds the 64 KiB receipt limit")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError as exc:
        raise BenchmarkError("driver result must be UTF-8 JSON") from exc
    try:
        value = json.loads(text)
    except json.JSONDecodeError as exc:
        raise BenchmarkError(f"driver result is not valid JSON: {exc}") from exc
    except (RecursionError, ValueError) as exc:
        raise BenchmarkError("driver result is not valid bounded JSON") from exc
    pending = [(value, 0)]
    while pending:
        item, depth = pending.pop()
        if depth > MAX_DRIVER_RECEIPT_DEPTH:
            raise BenchmarkError("driver result is not valid bounded JSON")
        if isinstance(item, dict):
            pending.extend((child, depth + 1) for child in item.values())
        elif isinstance(item, list):
            pending.extend((child, depth + 1) for child in item)
    if not isinstance(value, dict):
        raise BenchmarkError("driver result must be a JSON object")
    status = value.get("status")
    if not isinstance(status, str) or status not in STATUS_VALUES:
        raise BenchmarkError("driver result status must be pass, fail, partial, or unsupported")
    return value


def _annotation(
    case: dict[str, Any],
    *,
    variant: str,
    surface: str,
    base_revision: str,
    receipt: dict[str, Any],
    oracle: dict[str, Any],
) -> dict[str, Any]:
    correctness = receipt.get("correctness")
    if correctness is not None and not isinstance(correctness, dict):
        raise BenchmarkError("driver correctness must be an object when present")
    correctness = dict(correctness or {})

    if oracle["available"]:
        if oracle["passed"] is not True:
            correctness["task_verdict"] = "fail"
        elif correctness.get("task_verdict") is None and case.get("experiment") != "tool_selection":
            correctness["task_verdict"] = "pass"
        if case["validation"]["required"] is False and correctness.get("validation_verdict") is None:
            correctness["validation_verdict"] = "not_required"

    if correctness:
        correctness.setdefault("task_verdict", "unavailable")
        correctness.setdefault(
            "validation_verdict",
            "unavailable" if case["validation"]["required"] else "not_required",
        )

    value: dict[str, Any] = {
        "schema_version": report.RUN_ANNOTATION_SCHEMA_VERSION,
        "case_id": case["id"],
        "variant": variant,
        "surface": surface,
        "base_revision": base_revision,
        "case_fingerprint": report._case_fingerprint(case),
    }
    for field in ("repair_turns", "task_timing"):
        if receipt.get(field) is not None:
            value[field] = receipt[field]
    if correctness:
        value["correctness"] = correctness
    return report.validate_run_annotation(value)


def _summary_for_receipt(
    case: dict[str, Any],
    *,
    variant: str,
    surface: str,
    base_revision: str,
    receipt: dict[str, Any],
    annotation_path: Path,
    case_manifest: Path,
    workspace: Path,
) -> dict[str, Any] | None:
    if receipt["status"] == "unsupported":
        return None
    audit_db = receipt.get("audit_db")
    workflow_session_id = receipt.get("workflow_session_id")
    trace_root = receipt.get("trace_root")
    if not isinstance(audit_db, str) or not audit_db:
        raise BenchmarkError("non-unsupported driver result requires audit_db")
    if not isinstance(workflow_session_id, str) or not workflow_session_id:
        raise BenchmarkError("non-unsupported driver result requires workflow_session_id")

    audit_path = Path(audit_db)
    if not audit_path.is_absolute():
        audit_path = workspace / audit_path
    trace_path: Path | None = None
    if isinstance(trace_root, str) and trace_root:
        trace_path = Path(trace_root)
        if not trace_path.is_absolute():
            trace_path = workspace / trace_path
    return report.summarize(
        trace_root=trace_path,
        audit_db=audit_path,
        workflow_session_id=workflow_session_id,
        case_manifest=case_manifest,
        case_id=case["id"],
        variant=variant,
        surface=surface,
        base_revision=base_revision,
        run_annotation=annotation_path,
    )


def _tool_count(summary: dict[str, Any], variant: str, tool_name: str) -> int | None:
    if variant in ("direct", "host_code_mode"):
        value = report._get_path(summary, f"canonical_calls.by_name.{tool_name}")
        return value if isinstance(value, int) else 0
    if variant == "code_mode":
        available = report._get_path(summary, "availability.code_mode_composition.available")
        if available is not True:
            return None
        value = report._get_path(summary, f"composition.nested_tool_counts.{tool_name}")
        return value if isinstance(value, int) else 0
    return None


def _runtime_evidence_proven(
    summary: dict[str, Any],
    variant: str,
    surface: str | None = None,
) -> bool:
    outer_total = report._get_path(summary, "outer_calls.total")
    if not isinstance(outer_total, int) or isinstance(outer_total, bool) or outer_total <= 0:
        return False

    composition_available = (
        report._get_path(summary, "availability.code_mode_composition.available") is True
    )
    if variant == report.TOOL_SELECTION_VARIANT:
        # Any admitted mix is valid here. The original forced-lane gate below
        # still rejects mixtures; this experiment measures choice separately.
        canonical_total = report._get_path(summary, "canonical_calls.total")
        return (
            surface == report.TOOL_SELECTION_VARIANT
            and composition_available
            and type(canonical_total) is int
            and canonical_total == outer_total
        )
    if variant == "direct":
        canonical_total = report._get_path(summary, "canonical_calls.total")
        return (
            composition_available
            and isinstance(canonical_total, int)
            and not isinstance(canonical_total, bool)
            and canonical_total > 0
        )

    if variant == "host_code_mode":
        # Canonical calls prove the runtime lane, not JavaScript-cell/model-turn
        # identity. The caller separately records its actual Host configuration.
        canonical_total = report._get_path(summary, "canonical_calls.total")
        counts = report._get_path(summary, "canonical_calls.by_name")
        return (surface == report.HOST_CODE_MODE_SURFACE and composition_available
                and type(canonical_total) is int and canonical_total == outer_total
                and isinstance(counts, dict)
                and not any(counts.get(name, 0) for name in report.CODE_MODE_TOOLS))

    if variant == "code_mode":
        expected_tool = CODE_MODE_TOOL_BY_SURFACE.get(surface or "")
        outer_code_mode = report._get_path(summary, "composition.outer_code_mode_calls")
        outer_by_name = report._get_path(summary, "tools.outer_by_name")
        if (
            not composition_available
            or expected_tool is None
            or not isinstance(outer_code_mode, int)
            or isinstance(outer_code_mode, bool)
            or outer_code_mode <= 0
            or not isinstance(outer_by_name, dict)
        ):
            return False
        code_mode_counts = {
            name: count
            for name in report.CODE_MODE_TOOLS
            if isinstance((count := outer_by_name.get(name, 0)), int)
            and not isinstance(count, bool)
            and count > 0
        }
        if (
            code_mode_counts.get(expected_tool) != outer_code_mode
            or sum(code_mode_counts.values()) != outer_code_mode
        ):
            return False
        if surface == "read_only":
            if any(
                not isinstance(count, int)
                or isinstance(count, bool)
                or count < 0
                for count in outer_by_name.values()
            ):
                return False
            bootstrap_count = outer_by_name.get("work_on_project", 0)
            manifest_count = outer_by_name.get("read_tool_manifest", 0)
            allowed = {expected_tool, "work_on_project", "read_tool_manifest"}
            unexpected = {
                name: count
                for name, count in outer_by_name.items()
                if name not in allowed and count > 0
            }
            return (
                bootstrap_count == 1
                and manifest_count in (0, 1)
                and not unexpected
                and outer_total
                == outer_code_mode + bootstrap_count + manifest_count
            )
        return True
    return False


def _long_handoff_terminal_proven(summary: dict[str, Any], variant: str) -> bool:
    if _tool_count(summary, variant, "cargo_test") != 1:
        return False

    pending = report._get_path(summary, "job_convergence.pending_handoff_count")
    known_followups = report._get_path(summary, "job_convergence.pending_followup_known_count")
    terminal_samples = report._get_path(summary, "job_convergence.pending_to_terminal_ms.samples")
    if (
        not isinstance(pending, int)
        or isinstance(pending, bool)
        or pending != 1
        or not isinstance(known_followups, int)
        or isinstance(known_followups, bool)
        or known_followups < 1
        or not isinstance(terminal_samples, int)
        or isinstance(terminal_samples, bool)
        or terminal_samples < 1
    ):
        return False

    if variant in ("direct", "host_code_mode"):
        origin = "cargo_test"
        return (
            report._get_path(
                summary, f"job_convergence.pending_handoff_by_origin_tool.{origin}"
            )
            == 1
            and report._get_path(
                summary, f"job_convergence.pending_followup_known_by_origin_tool.{origin}"
            )
            == 1
            and report._get_path(
                summary,
                f"job_convergence.pending_to_terminal_ms_by_origin_tool.{origin}.samples",
            )
            == 1
            and report._get_path(
                summary, f"job_convergence.selected_terminal_by_origin_tool.{origin}"
            )
            == 1
        )
    if variant == "code_mode":
        origin = "execute_effectful_code_mode"
        return (
            report._get_path(summary, "composition.job_handoffs.total") == 1
            and report._get_path(summary, "composition.consequential_calls.total") == 1
            and report._get_path(summary, "composition.known_results.total") == 0
            and report._get_path(summary, "composition.outcome_unknown.total") == 0
            and report._get_path(
                summary, f"job_convergence.pending_handoff_by_origin_tool.{origin}"
            )
            == 1
            and report._get_path(
                summary, f"job_convergence.pending_followup_known_by_origin_tool.{origin}"
            )
            == 1
            and report._get_path(
                summary,
                f"job_convergence.pending_to_terminal_ms_by_origin_tool.{origin}.samples",
            )
            == 1
            and report._get_path(
                summary, f"job_convergence.selected_terminal_by_origin_tool.{origin}"
            )
            == 1
        )
    return False


def _guarded_multi_file_contract_proven(summary: dict[str, Any], variant: str) -> bool:
    edit_count = _tool_count(summary, variant, "edit_project_files")
    read_count = _tool_count(summary, variant, "read_files")
    search_counts = [
        count
        for name in ("search_project_texts", "search_and_read")
        if (count := _tool_count(summary, variant, name)) is not None
    ]
    outer_failed = report._get_path(summary, "outer_calls.failed")
    outer_unknown = report._get_path(summary, "outer_calls.timeout_or_unknown")
    if (
        not isinstance(outer_failed, int)
        or isinstance(outer_failed, bool)
        or outer_failed != 0
        or not isinstance(outer_unknown, int)
        or isinstance(outer_unknown, bool)
        or outer_unknown != 0
    ):
        return False

    if variant in ("direct", "host_code_mode"):
        if report._get_path(summary, "availability.edit_outcomes.available") is not True:
            return False
        if report._get_path(
            summary,
            "edit_outcomes.by_tool.edit_project_files.applied",
        ) != 1:
            return False

    if variant == "code_mode":
        child_failed = report._get_path(summary, "child_calls.failed")
        consequential = report._get_path(summary, "composition.consequential_calls.total")
        known_results = report._get_path(summary, "composition.known_results.total")
        job_handoffs = report._get_path(summary, "composition.job_handoffs.total")
        outcome_unknown = report._get_path(summary, "composition.outcome_unknown.total")
        mutation_changed = report._get_path(
            summary, "composition.mutation_state_changed.total"
        )
        mutation_no_change = report._get_path(
            summary, "composition.mutation_no_change.total"
        )
        if (
            not isinstance(child_failed, int)
            or isinstance(child_failed, bool)
            or child_failed != 0
            or consequential != 1
            or known_results != 1
            or job_handoffs != 0
            or outcome_unknown != 0
            or mutation_changed != 1
            or mutation_no_change != 0
        ):
            return False

    return (
        edit_count == 1
        and read_count is not None
        and read_count >= 2
        and sum(search_counts) >= 1
    )


class _GenericCoordinator:
    """Own the fixture-side external write, never a runtime operation or receipt."""
    def __init__(self, fixture: GenericFixture, directory: Path):
        self.fixture, self.directory = fixture, directory
        self.stop = threading.Event()
        self.error = False
        self.thread = None

    def start(self):
        self.directory.mkdir()
        if self.fixture.case_id != "generic_stale_read_revision":
            return
        def coordinate():
            try:
                while not self.stop.wait(0.01):
                    if not self.fixture.external_written and (self.directory / "first-read").is_file():
                        self.fixture.append_external()
                        (self.directory / "external-written").write_text("ready\n")
                    if self.fixture.external_written and not self.fixture.rejection_checked and (self.directory / "stale-rejected").is_file():
                        self.fixture.check_rejected_bytes()
                        (self.directory / "rejection-checked").write_text("ready\n")
            except (OSError, ValueError):
                self.error = True
                (self.directory / "coordination-failed").write_text("failed\n")
        self.thread = threading.Thread(target=coordinate, name="fixture-stale-coordinator", daemon=True)
        self.thread.start()

    def close(self):
        self.stop.set()
        if self.thread is not None:
            self.thread.join(timeout=2)
            if self.thread.is_alive():
                raise BenchmarkError("fixture coordinator did not stop")


def _generic_temporal_evidence_proven(case, summary, variant, evidence=None, fixture_identity=None):
    if case["id"] not in temporal.RUNTIME_CASES:
        return False
    identity = (summary or {}).get("benchmark") or {}
    return (isinstance(evidence, dict) and type(evidence.get("schema_version")) is int and evidence.get("schema_version") == temporal.PROOF_SCHEMA_VERSION
            and isinstance(fixture_identity, str) and re.fullmatch(r"[0-9a-f]{64}", fixture_identity) is not None
            and evidence.get("fixture_identity") == fixture_identity
            and evidence.get("available") is True and evidence.get("passed") is True
            and evidence.get("case_id") == case["id"]
            and evidence.get("case_fingerprint") == report._case_fingerprint(case)
            and evidence.get("base_revision") == identity.get("base_revision")
            and isinstance(evidence.get("checks"), dict)
            and set(evidence["checks"]) == temporal.CHECK_NAMES[case["id"]]
            and all(value is True for value in evidence["checks"].values()))


class _KnowledgeCoordinator:
    """Only the operator asks; actual captured first reads admit one update."""

    def __init__(self, fixture, directory, capture_root):
        self.fixture, self.directory, self.capture_root = fixture, directory, capture_root
        self.error = None
        self.advance = None
        self._stop = threading.Event()
        self.thread = threading.Thread(target=self._run, daemon=True)

    def start(self):
        self.directory.mkdir(mode=0o700)
        self.thread.start()

    def _run(self):
        while not self._stop.wait(0.1):
            request = self.directory / "advance-request.json"
            if not request.exists() or self.advance is not None or self.error is not None:
                continue
            try:
                value = json.loads(temporal._read(request, 4096))
                if not isinstance(value, dict) or set(value) != {"audit_db", "workflow_session_id", "trace_id"}:
                    raise ValueError("invalid checkpoint request")
                self.advance = knowledge.advance_after_read(capture_root=self.capture_root,
                    audit_db=Path(value["audit_db"]), workflow_session_id=value["workflow_session_id"],
                    fixture=self.fixture, trace_id=value["trace_id"])
                target = self.directory / "advanced.json"
                with target.open("x", encoding="utf-8") as handle:
                    handle.write(_stable_json(self.advance))
                target.chmod(0o600)
            except (OSError, ValueError, TypeError, KeyError, sqlite3.Error) as error:
                self.error = type(error).__name__
                target = self.directory / "coordination-failed"
                target.touch(mode=0o600, exist_ok=False)

    def close(self):
        self._stop.set()
        self.thread.join(timeout=5)
        if self.thread.is_alive():
            raise BenchmarkError("knowledge fixture coordinator did not stop")


def _knowledge_evidence_proven(case, summary, evidence, fixture_identity):
    return (case.get("target") == knowledge.TARGET and case["id"] in KNOWLEDGE_CASES
        and isinstance(summary, dict) and isinstance(evidence, dict)
        and type(evidence.get("schema_version")) is int and evidence["schema_version"] == 1
        and isinstance(fixture_identity, str) and re.fullmatch(r"[0-9a-f]{64}", fixture_identity) is not None
        and evidence.get("fixture_identity") == fixture_identity
        and evidence.get("case_id") == case["id"]
        and evidence.get("case_fingerprint") == report._case_fingerprint(case)
        and evidence.get("base_revision") == (summary.get("benchmark") or {}).get("base_revision")
        and evidence.get("available") is True and evidence.get("passed") is True
        and type(evidence.get("canonical_call_count")) is int and evidence["canonical_call_count"] > 1
        and isinstance(evidence.get("checks"), dict) and set(evidence["checks"]) == knowledge.CHECK_NAMES
        and all(value is True for value in evidence["checks"].values()))


def _knowledge_host_identity(receipt, base_revision):
    """Keep only pinned safe metadata; a digest does not attest a remote model."""
    if not isinstance(base_revision, str) or re.fullmatch(r"[0-9a-f]{40}", base_revision) is None:
        return None
    digest, runtime = receipt.get("host_config_digest"), receipt.get("runtime_identity")
    if not isinstance(digest, str) or re.fullmatch(r"[0-9a-f]{64}", digest) is None:
        return None
    if not isinstance(runtime, dict) or set(runtime) != {"build", "binary_sha256"}:
        return None
    build, hashes = runtime["build"], runtime["binary_sha256"]
    if (not isinstance(build, dict) or set(build) != {"version", "commit_prefix", "dirty", "built_at"}
            or not isinstance(build["version"], str) or re.fullmatch(r"[\w.+-]{1,64}", build["version"]) is None
            or not isinstance(build["commit_prefix"], str) or re.fullmatch(r"[0-9a-f]{12,40}", build["commit_prefix"]) is None
            or not base_revision.startswith(build["commit_prefix"]) or build["dirty"] is not False
            or type(build["built_at"]) is not int or build["built_at"] <= 0
            or not isinstance(hashes, dict) or set(hashes) != {"webcodex-server", "webcodex-runner"}
            or any(not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None for value in hashes.values())):
        return None
    return {"model_config_digest": digest, "runtime_identity": runtime}


def _run_sample(
    repo: Path,
    root: Path,
    driver_argv: list[str],
    case: dict[str, Any],
    *,
    variant: str,
    pair_index: int,
    ordinal: int,
    base_revision: str,
    case_manifest: Path,
    host_cell_policy: str | None = None,
) -> dict[str, Any]:
    if host_cell_policy is not None and (variant != "host_code_mode" or
        host_cell_policy not in ("host_single_call", "host_composition_allowed")):
        raise BenchmarkError("Host cell policy requires an explicit Host lane")
    label = f"{case['id']}-p{pair_index + 1}-{ordinal + 1}-{variant}"
    workspace = root / "worktrees" / label
    receipt_path = root / "receipts" / f"{label}.json"
    annotation_path = root / "annotations" / f"{label}.json"
    receipt_path.unlink(missing_ok=True)
    annotation_path.unlink(missing_ok=True)
    surface: str | None = None
    worktree_added = False
    generic = case.get("target") == "generic_agent_disposable_fixture"
    knowledge_case = case.get("target") == knowledge.TARGET
    coordinator = None
    fixture = None
    fixture_token = None
    capture_root = None
    completed: subprocess.CompletedProcess[bytes] | None = None
    try:
        surface = _surface(case, variant)
        if generic:
            workspace.mkdir(parents=True)
            fixture = GenericFixture(workspace, case["id"]).prepare()
            fixture_token = temporal.fixture_identity(workspace)
            coordinator = _GenericCoordinator(fixture, root / "receipts" / (label + "-coordination"))
            coordinator.start()
        elif knowledge_case:
            fixture = KnowledgeFixture(workspace, case["id"]).prepare()
            fixture_token = temporal.fixture_identity(workspace)
            capture_root = root / "receipts" / (label + "-capture")
            temporal.prepare_capture(capture_root, case["id"], base_revision,
                report._case_fingerprint(case), fixture_root=workspace)
            coordinator = _KnowledgeCoordinator(fixture, root / "receipts" / (label + "-coordination"), capture_root)
            coordinator.start()
        else:
            _worktree_add(repo, workspace, base_revision)
            worktree_added = True
            if _workspace_status(workspace):
                raise BenchmarkError("fresh benchmark worktree is not clean")
        env = dict(os.environ)
        env.pop("WEBCODEX_BENCH_HOST_CELL_POLICY", None)
        env.update(
            {
                "WEBCODEX_BENCH_WORKSPACE": str(workspace),
                "WEBCODEX_BENCH_CASE_ID": case["id"],
                "WEBCODEX_BENCH_VARIANT": variant,
                "WEBCODEX_BENCH_EXPERIMENT": case.get("experiment", "forced_lanes"),
                "WEBCODEX_BENCH_SURFACE": surface,
                "WEBCODEX_BENCH_BASE_REVISION": base_revision,
                "WEBCODEX_BENCH_CASE_FINGERPRINT": report._case_fingerprint(case),
                "WEBCODEX_BENCH_PROMPT": case["prompt"],
                "WEBCODEX_BENCH_DRIVER_RESULT": str(receipt_path),
            }
        )
        if host_cell_policy is not None:
            env["WEBCODEX_BENCH_HOST_CELL_POLICY"] = host_cell_policy
        if knowledge_case:
            env.update({"WEBCODEX_BENCH_TARGET": knowledge.TARGET,
                "WEBCODEX_BENCH_FIXTURE_COORDINATION": str(coordinator.directory),
                "WEBCODEX_BENCH_TEMPORAL_TRACE_ROOT": str(capture_root)})
        if generic:
            env.update({"WEBCODEX_BENCH_TARGET": "generic_agent_disposable_fixture",
                "WEBCODEX_BENCH_FIXTURE_COORDINATION": str(coordinator.directory),
                "WEBCODEX_BENCH_FIXTURE_ENVIRONMENT": json.dumps({"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC", "PYTHONHASHSEED": "0", "PYTHONNOUSERSITE": "1"}),
                "WEBCODEX_BENCH_FIXTURE_COMMAND": pressure_script() if case["id"] == "generic_failure_repair" else ""})
            if case["id"] == "generic_pending_independent_join":
                env["WEBCODEX_BENCH_FIXTURE_COMMAND"] = temporal.PENDING_SCRIPT
            if case["id"] == "generic_mixed_files_conflict":
                env["WEBCODEX_BENCH_FIXTURE_COMMAND"] = mixed_note_script()
            if case["id"] == "generic_encoded_csv":
                env["WEBCODEX_BENCH_FIXTURE_COMMAND"] = encoded_csv_script()
            if case["id"] in temporal.RUNTIME_CASES:
                capture_root = root / "receipts" / (label + "-capture")
                temporal.prepare_capture(capture_root, case["id"], base_revision, report._case_fingerprint(case), fixture_root=workspace)
                env["WEBCODEX_BENCH_TEMPORAL_TRACE_ROOT"] = str(capture_root)
            env.update({"LANG": "C.UTF-8", "LC_ALL": "C.UTF-8", "TZ": "UTC", "PYTHONHASHSEED": "0", "PYTHONNOUSERSITE": "1"})
            env.pop("PYTHONPATH", None)
            env.pop("PYTHONHOME", None)
        started = time.time_ns() // 1_000_000
        completed = _run_driver(driver_argv, cwd=workspace, env=env)
        ended = time.time_ns() // 1_000_000
        if not receipt_path.exists():
            return {
                "case_id": case["id"],
                "variant": variant,
                "surface": surface,
                "status": "fail",
                "driver_exit_code": completed.returncode,
                "reason_code": "driver_receipt_missing",
                "fixture_oracle": {"available": False, "passed": None, "checks": [], "reason": "driver receipt missing"},
                "summary": None,
            }

        receipt = _load_driver_receipt(receipt_path)
        if "task_timing" not in receipt:
            receipt["task_timing"] = {"started_at_ms": started, "ended_at_ms": ended}
        if coordinator is not None:
            coordinator.close()
        head_changed = not (generic or knowledge_case) and _head_revision(workspace) != base_revision
        oracle = fixture.oracle() if fixture is not None else _fixture_oracle(case, workspace, base_revision)
        if coordinator is not None and coordinator.error:
            oracle = {"available": True, "passed": False, "checks": [], "reason": "fixture coordination failed"}
        if generic or knowledge_case:
            oracle["fixture_identity"] = fixture_token
        knowledge_evidence = None
        host_identity = None
        if knowledge_case:
            if receipt["status"] == "unsupported":
                return {"case_id": case["id"], "variant": variant, "surface": surface,
                    "status": "unsupported", "driver_exit_code": completed.returncode,
                    "fixture_oracle": oracle, "summary": None}
            if any(not isinstance(receipt.get(key), str) or not receipt[key]
                   for key in ("audit_db", "workflow_session_id")):
                raise BenchmarkError("knowledge sample requires exact Audit and Session selectors")
            knowledge_evidence = knowledge.verify(capture_root=capture_root,
                audit_db=Path(receipt["audit_db"]), workflow_session_id=receipt["workflow_session_id"],
                fixture=fixture, base_revision=base_revision, fingerprint=report._case_fingerprint(case),
                answer=receipt.get("answer"), advance=coordinator.advance)
            host_identity = _knowledge_host_identity(receipt, base_revision)
            receipt = dict(receipt)
            receipt["correctness"] = {"task_verdict": "pass" if knowledge_evidence["passed"] is True
                else "fail" if knowledge_evidence["available"] else "unavailable",
                "validation_verdict": "not_required"}
        annotation = _annotation(
            case,
            variant=variant,
            surface=surface,
            base_revision=base_revision,
            receipt=receipt,
            oracle=oracle,
        )
        annotation_path.write_text(_stable_json(annotation), encoding="utf-8")
        summary = _summary_for_receipt(
            case,
            variant=variant,
            surface=surface,
            base_revision=base_revision,
            receipt=receipt,
            annotation_path=annotation_path,
            case_manifest=case_manifest,
            workspace=workspace,
        )
        temporal_evidence = None
        if capture_root is not None and summary is not None and generic:
            audit_path = Path(receipt["audit_db"])
            temporal_evidence = temporal.verify(
                capture_root=capture_root, audit_db=audit_path if audit_path.is_absolute() else workspace / audit_path,
                workflow_session_id=receipt["workflow_session_id"], case_id=case["id"],
                base_revision=base_revision, fingerprint=report._case_fingerprint(case), fixture_root=workspace)
        effective_status = receipt["status"]
        reason_code: str | None = None
        if knowledge_case:
            if knowledge_evidence["available"] is not True:
                effective_status = "partial"
                reason_code = "knowledge_evidence_unavailable"
            elif knowledge_evidence["passed"] is not True:
                effective_status = "fail"
                reason_code = "knowledge_case_failed"
            elif host_identity is None:
                effective_status = "partial"
                reason_code = "knowledge_host_identity_unavailable"
        if head_changed:
            effective_status = "fail"
            reason_code = "workspace_head_changed"
        if completed.returncode != 0 and effective_status in ("pass", "partial"):
            effective_status = "fail"
        if oracle["available"] and oracle["passed"] is not True and effective_status in ("pass", "partial"):
            effective_status = "fail"
        if (
            (case.get("correctness") or {}).get("workspace_must_remain_clean") is True
            and _workspace_status(workspace)
            and effective_status in ("pass", "partial")
        ):
            effective_status = "fail"
        if summary is not None and effective_status == "pass":
            correctness = summary.get("correctness") or {}
            task_verdict = correctness.get("task_verdict")
            validation_verdict = correctness.get("validation_verdict")
            if task_verdict == "fail" or validation_verdict == "fail":
                effective_status = "fail"
            elif task_verdict != "pass":
                effective_status = "partial"
            elif case["validation"]["required"] and validation_verdict != "pass":
                effective_status = "partial"
            elif not case["validation"]["required"] and validation_verdict not in ("pass", "not_required"):
                effective_status = "partial"

        if (
            summary is not None
            and effective_status == "pass"
            and not knowledge_case and not _runtime_evidence_proven(summary, variant, surface)
        ):
            effective_status = "partial"

        if (
            case["id"] == "long_validation_handoff"
            and summary is not None
            and effective_status == "pass"
            and not _long_handoff_terminal_proven(summary, variant)
        ):
            effective_status = "partial"

        if (
            case["id"] == "guarded_multi_file_edit"
            and summary is not None
            and effective_status == "pass"
            and not _guarded_multi_file_contract_proven(summary, variant)
        ):
            effective_status = "partial"
        if generic and effective_status == "pass" and not _generic_temporal_evidence_proven(case, summary, variant, temporal_evidence, fixture_token):
            contradicted = temporal_evidence is not None and temporal_evidence.get("available") is True and temporal_evidence.get("passed") is False
            effective_status = "fail" if contradicted else "partial"
            reason_code = "generic_temporal_semantics_failed" if contradicted else "generic_temporal_evidence_unavailable"
        if knowledge_case and effective_status == "pass" and not _knowledge_evidence_proven(case, summary, knowledge_evidence, fixture_token):
            effective_status = "partial"
            reason_code = "knowledge_evidence_unavailable"
        return {
            "case_id": case["id"],
            "variant": variant,
            "surface": surface,
            "status": effective_status,
            "driver_exit_code": completed.returncode,
            **({"reason_code": reason_code} if reason_code is not None else {}),
            "fixture_oracle": oracle,
            **({"temporal_evidence": temporal_evidence} if temporal_evidence is not None else {}),
            **({"knowledge_evidence": knowledge_evidence} if knowledge_evidence is not None else {}),
            **({"host_identity": host_identity} if knowledge_case else {}),
            "summary": summary,
        }
    except DriverPlatformUnsupported:
        return {
            "case_id": case["id"],
            "variant": variant,
            "surface": surface,
            "status": "unsupported",
            "driver_exit_code": None,
            "reason_code": "driver_platform_unsupported",
            "fixture_oracle": {"available": False, "passed": None, "checks": [], "reason": "driver platform unsupported"},
            "summary": None,
        }
    except DriverTimeoutError:
        return {
            "case_id": case["id"],
            "variant": variant,
            "surface": surface,
            "status": "fail",
            "driver_exit_code": None,
            "reason_code": "driver_timeout",
            "fixture_oracle": {"available": False, "passed": None, "checks": [], "reason": "driver timed out"},
            "summary": None,
        }
    except (BenchmarkError, report.ReportError):
        return {
            "case_id": case["id"],
            "variant": variant,
            "surface": surface,
            "status": "fail",
            "driver_exit_code": completed.returncode if completed is not None else None,
            "reason_code": "sample_contract_error",
            "fixture_oracle": {"available": False, "passed": None, "checks": [], "reason": "sample failed before oracle"},
            "summary": None,
        }
    finally:
        if coordinator is not None:
            coordinator.close()
        if capture_root is not None:
            # Only the benchmark-created sibling of the fixture is removable;
            # receipt-supplied capture directories are never followed/deleted.
            shutil.rmtree(capture_root)
        if coordinator is not None:
            shutil.rmtree(coordinator.directory, ignore_errors=True)
        if worktree_added:
            _worktree_remove(repo, workspace)
        elif (generic or knowledge_case) and workspace.exists():
            shutil.rmtree(workspace)


def _comparison(direct: dict[str, Any], code_mode: dict[str, Any]) -> dict[str, Any] | None:
    if direct.get("status") != "pass" or code_mode.get("status") != "pass":
        return None
    if direct.get("summary") is None or code_mode.get("summary") is None:
        return None
    return report.compare_reports(direct["summary"], code_mode["summary"])


def _paired_delta_summary(pairs: list[dict[str, Any]]) -> dict[str, Any]:
    """Describe deltas only from correctness-qualified Direct/Code Mode pairs.

    A metric with incomplete evidence retains its observed count and percentiles,
    but never reports a complete total. The summary does not imply model turns,
    complete network payload bytes, or a statistically proven speedup.
    """
    qualified = [
        comparison
        for pair in pairs
        if isinstance((comparison := pair.get("comparison")), dict)
        and isinstance(comparison.get("throughput_compatibility"), dict)
        and comparison["throughput_compatibility"].get("comparable") is True
    ]
    metrics = {}
    for name in PAIRED_DELTA_METRICS:
        deltas: list[int] = []
        for comparison in qualified:
            rows = comparison.get("metrics")
            if not isinstance(rows, list):
                continue
            matches = [
                item for item in rows
                if isinstance(item, dict) and item.get("metric") == name
            ]
            if len(matches) != 1:
                continue
            value = matches[0]
            baseline, candidate, delta = (
                value.get("baseline"), value.get("candidate"), value.get("delta")
            )
            if (
                value.get("comparable") is True
                and type(baseline) is int
                and type(candidate) is int
                and type(delta) is int
                and delta == candidate - baseline
            ):
                deltas.append(delta)
        distribution = report._metric_distribution(
            deltas, missing=len(qualified) - len(deltas)
        )
        if not qualified:
            distribution["total"] = None
        distribution["complete"] = bool(qualified) and distribution["missing"] == 0
        metrics[name] = distribution
    return {
        "qualified_pairs": len(qualified),
        "excluded_pairs": len(pairs) - len(qualified),
        "candidate_minus_direct": metrics,
    }


def _pair_order(pair_index: int, candidate: str = "code_mode") -> tuple[str, str]:
    variants = ("direct", candidate)
    return variants if pair_index % 2 == 0 else tuple(reversed(variants))


def run_benchmark(
    *,
    repo: Path,
    base_revision: str,
    driver_argv: list[str],
    case_ids: list[str],
    pairs: int,
    candidate: str = "code_mode",
) -> dict[str, Any]:
    if candidate not in ("code_mode", "host_code_mode"):
        raise BenchmarkError("candidate must be code_mode or host_code_mode")
    if pairs < 1:
        raise BenchmarkError("pairs must be at least 1")
    if len(set(case_ids)) != len(case_ids):
        raise BenchmarkError("duplicate case ids are not allowed in one benchmark run")
    repo = _git_root(repo)
    resolved = _require_exact_revision(repo, base_revision)
    if _head_revision(repo) != resolved:
        raise BenchmarkError(
            "base revision must equal the current checkout HEAD; check out the target commit before benchmarking"
        )
    temp_owner = tempfile.TemporaryDirectory(prefix="webcodex-agent-loop-bench-")
    try:
        root = Path(temp_owner.name)
        for name in ("worktrees", "receipts", "annotations"):
            (root / name).mkdir(parents=True, exist_ok=True)
        case_manifest = root / "agent_loop_cases.json"
        case_manifest.write_text(_read_manifest_at_revision(repo, resolved), encoding="utf-8")
        manifest = report.load_case_manifest(case_manifest)
        selected = [_case(manifest, case_id) for case_id in case_ids]
        if any(case.get("experiment", "forced_lanes") != "forced_lanes" for case in selected):
            raise BenchmarkError("tool_selection cases require --experiment tool_selection")
        unsupported_targets = sorted(
            {str(case.get("target")) for case in selected if case.get("target") not in ("webcodex_repository", "generic_agent_disposable_fixture")}
        )
        if any(case.get("target") == "generic_agent_disposable_fixture" and case["id"] not in GENERIC_CASES for case in selected):
            raise BenchmarkError("unknown generic fixture case")
        if unsupported_targets:
            raise BenchmarkError(
                "paired runner supports only repository/disposable generic targets; "
                f"unsupported target(s): {', '.join(unsupported_targets)}"
            )
    except BaseException:
        temp_owner.cleanup()
        raise

    cases_out: list[dict[str, Any]] = []
    try:
        for case in selected:
            pair_outputs: list[dict[str, Any]] = []
            for pair_index in range(pairs):
                samples: list[dict[str, Any]] = []
                order = _pair_order(pair_index, candidate)
                for ordinal, variant in enumerate(order):
                    samples.append(
                        _run_sample(
                            repo,
                            root,
                            driver_argv,
                            case,
                            variant=variant,
                            pair_index=pair_index,
                            ordinal=ordinal,
                            base_revision=resolved,
                            case_manifest=case_manifest,
                        )
                    )
                by_variant = {sample["variant"]: sample for sample in samples}
                pair_outputs.append(
                    {
                        "pair_index": pair_index,
                        "order": list(order),
                        "samples": samples,
                        "comparison": _comparison(by_variant["direct"], by_variant[candidate]),
                    }
                )
            cases_out.append({
                "case_id": case["id"],
                "pairs": pair_outputs,
                "paired_delta_summary": _paired_delta_summary(pair_outputs),
            })

        statuses = [
            sample["status"]
            for case in cases_out
            for pair in case["pairs"]
            for sample in pair["samples"]
        ]
        return {
            "schema_version": 1,
            "kind": "agent_loop_benchmark",
            "candidate_variant": candidate,
            "base_revision": resolved,
            "pair_count": pairs,
            "case_ids": case_ids,
            "sample_count": len(statuses),
            "status_counts": {status: statuses.count(status) for status in sorted(STATUS_VALUES)},
            "cases": cases_out,
            "notes": [
                "each sample ran in a fresh detached Git worktree or ordinary-directory generic fixture",
                "pair order alternates direct/candidate then candidate/direct",
                "driver-reported unsupported, partial, and failed samples remain in the result",
                "comparisons reuse agent_loop_report exact base/case fingerprint/correctness gates",
                "outer-call metrics remain proxies; exact model round trips stay unavailable without Host turn evidence",
                "paired_delta_summary includes only correctness-qualified pairs; missing metrics are never counted as zero",
                "paired p50/p95 deltas are descriptive nearest-rank statistics, not confidence intervals or a speedup claim",
            ],
        }
    finally:
        if temp_owner is not None:
            temp_owner.cleanup()


def run_selection_benchmark(
    *,
    repo: Path,
    base_revision: str,
    driver_argv: list[str],
    case_ids: list[str],
    samples: int,
) -> dict[str, Any]:
    """Capture unrestricted tool choices without weakening forced-lane gates.

    The caller-owned driver still supplies actual Host observations and an
    independent correctness verdict. No driver or model is simulated here.
    """
    if samples < 1 or samples > 100:
        raise BenchmarkError("selection samples must be 1..100")
    if not case_ids or len(set(case_ids)) != len(case_ids):
        raise BenchmarkError("selection requires unique non-empty case ids")
    repo = _git_root(repo)
    resolved = _require_exact_revision(repo, base_revision)
    if _head_revision(repo) != resolved:
        raise BenchmarkError("base revision must equal the current checkout HEAD")
    with tempfile.TemporaryDirectory(prefix="webcodex-agent-selection-bench-") as tmp:
        root = Path(tmp)
        for name in ("worktrees", "receipts", "annotations"):
            (root / name).mkdir()
        case_manifest = root / "agent_loop_cases.json"
        case_manifest.write_text(_read_manifest_at_revision(repo, resolved), encoding="utf-8")
        selected = [_case(report.load_case_manifest(case_manifest), case_id) for case_id in case_ids]
        if any(case.get("experiment") != "tool_selection" for case in selected):
            raise BenchmarkError("selection runner requires explicit tool_selection cases")
        if any(case.get("target") not in ("webcodex_repository", knowledge.TARGET) for case in selected):
            raise BenchmarkError("selection runner target is unsupported")
        if any(case.get("target") == knowledge.TARGET and case["id"] not in KNOWLEDGE_CASES for case in selected):
            raise BenchmarkError("unknown knowledge fixture case")
        cases_out = []
        for case in selected:
            observations = [
                _run_sample(
                    repo, root, driver_argv, case,
                    variant=report.TOOL_SELECTION_VARIANT,
                    pair_index=index, ordinal=0,
                    base_revision=resolved, case_manifest=case_manifest,
                )
                for index in range(samples)
            ]
            cases_out.append({"case_id": case["id"], "samples": observations})
        statuses = [sample["status"] for case in cases_out for sample in case["samples"]]
        return {
            "schema_version": 1,
            "kind": "agent_loop_selection_benchmark",
            "experiment": "tool_selection",
            "base_revision": resolved,
            "samples_per_case": samples,
            "case_ids": case_ids,
            "sample_count": len(statuses),
            "status_counts": {status: statuses.count(status) for status in sorted(STATUS_VALUES)},
            "cases": cases_out,
            "notes": [
                "repository cases use fresh detached worktrees; explicit knowledge cases use independent Git fixtures",
                "tool selection follows each frozen case prompt; defaults retain unrestricted selection",
                "native batching is an invocation shape, not another execution surface",
                "scripted MCP calls are runtime tests, not real ChatGPT Web or model-choice evidence",
                "no optimal sequence, winner, exact model-turn count, or speedup is inferred",
            ],
        }


def _human_summary(result: dict[str, Any]) -> str:
    counts = result["status_counts"]
    if result.get("experiment") == "tool_selection":
        return "\n".join([
            f"Agent Loop tool selection {result['base_revision'][:8]}",
            f"cases={len(result['case_ids'])} samples={result['sample_count']}",
            "status " + " ".join(f"{key}={counts[key]}" for key in sorted(counts)),
        ])
    lines = [
        f"Agent Loop benchmark {result['base_revision'][:8]}",
        f"cases={len(result['case_ids'])} pairs={result['pair_count']} samples={result['sample_count']}",
        "status " + " ".join(f"{key}={counts[key]}" for key in sorted(counts)),
    ]
    for case in result["cases"]:
        orders = ["/".join(pair["order"]) for pair in case["pairs"]]
        comparable = sum(
            1
            for pair in case["pairs"]
            if isinstance(pair.get("comparison"), dict)
            and (pair["comparison"].get("throughput_compatibility") or {}).get("comparable") is True
        )
        summary = case["paired_delta_summary"]
        wall = summary["candidate_minus_direct"]["task_wall_time_ms"]
        wall_p50 = wall["p50"] if wall["complete"] else "unavailable"
        lines.append(
            f"{case['case_id']}: orders={','.join(orders)} "
            f"comparable_pairs={comparable}/{len(orders)} "
            f"paired_wall_p50_delta_ms={wall_p50}"
        )
    return "\n".join(lines)


def _strict_passed(result, requested_cases, repetitions, manifest=None):
    """Acceptance requires the exact requested matrix and real required evidence."""
    if not requested_cases or repetitions < 1:
        return False
    manifest = manifest or report.load_case_manifest(report.DEFAULT_CASE_MANIFEST)
    try:
        definitions = {case_id: _case(manifest, case_id) for case_id in requested_cases}
    except report.ReportError:
        return False
    cases = result.get("cases", [])
    if len(cases) != len(requested_cases) or {case.get("case_id") for case in cases} != set(requested_cases):
        return False
    selection = result.get("experiment") == "tool_selection"
    candidate = result.get("candidate_variant", "code_mode")
    if candidate not in ("code_mode", "host_code_mode"):
        return False
    knowledge_identities = set()
    for case in cases:
        definition = definitions[case["case_id"]]
        if selection:
            samples = case.get("samples", [])
            if len(samples) != repetitions or any(sample.get("variant") != report.TOOL_SELECTION_VARIANT for sample in samples):
                return False
        else:
            pairs = case.get("pairs", [])
            if len(pairs) != repetitions or {p.get("pair_index") for p in pairs} != set(range(repetitions)):
                return False
            if any(len(p.get("samples", [])) != 2 or {s.get("variant") for s in p["samples"]} != {"direct", candidate} for p in pairs):
                return False
            samples = [sample for pair in pairs for sample in pair["samples"]]
        for sample in samples:
            summary = sample.get("summary")
            if sample.get("case_id") != case["case_id"] or sample.get("status") != "pass" or not isinstance(summary, dict):
                return False
            correctness = summary.get("correctness") or {}
            if correctness.get("task_verdict") != "pass" or correctness.get("validation_verdict") not in ("pass", "not_required"):
                return False
            if definition["validation"]["required"] and correctness.get("validation_verdict") != "pass":
                return False
            knowledge_case = definition.get("target") == knowledge.TARGET
            if not knowledge_case and not _runtime_evidence_proven(summary, sample.get("variant"), sample.get("surface")):
                return False
            oracle = sample.get("fixture_oracle") or {}
            if not isinstance(oracle, dict):
                return False
            if knowledge_case:
                identity = sample.get("host_identity")
                if (not isinstance(identity, dict) or set(identity) != {"model_config_digest", "runtime_identity"}
                        or _knowledge_host_identity({"host_config_digest": identity.get("model_config_digest"),
                            "runtime_identity": identity.get("runtime_identity")},
                            (summary.get("benchmark") or {}).get("base_revision", "")) is None):
                    return False
                knowledge_identities.add(_stable_json(identity))
                if (sample.get("variant") != report.TOOL_SELECTION_VARIANT
                        or sample.get("surface") != report.TOOL_SELECTION_VARIANT
                        or oracle.get("available") is not True or oracle.get("passed") is not True
                        or not _knowledge_evidence_proven(definition, summary, sample.get("knowledge_evidence"), oracle.get("fixture_identity"))):
                    return False
            elif definition.get("target") == "generic_agent_disposable_fixture":
                if (oracle.get("available") is not True or oracle.get("passed") is not True
                    or not _generic_temporal_evidence_proven(definition, summary, sample.get("variant"), sample.get("temporal_evidence"), oracle.get("fixture_identity"))):
                    return False
            elif oracle.get("available") and oracle.get("passed") is not True:
                return False
    return len(knowledge_identities) <= 1


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--base-revision", required=True)
    parser.add_argument("--driver", required=True, help="Host driver command, parsed with shlex; no shell is invoked")
    parser.add_argument("--case-id", action="append", dest="case_ids")
    parser.add_argument("--pairs", type=int, default=2)
    parser.add_argument("--candidate", choices=("code_mode", "host_code_mode"), default="code_mode",
                        help="paired candidate lane; existing nested Code Mode remains the default")
    parser.add_argument("--experiment", choices=("forced_lanes", "tool_selection"), default="forced_lanes")
    parser.add_argument("--samples", type=int, default=2, help="repetitions per case in the tool_selection experiment")
    parser.add_argument("--strict", action="store_true", help="fail unless every requested case/repetition passes with required runtime evidence")
    parser.add_argument("--output", type=Path)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = _parser().parse_args(argv)
    try:
        driver_argv = shlex.split(args.driver)
        if not driver_argv:
            raise BenchmarkError("--driver must name an executable")
        if args.experiment == "tool_selection":
            result = run_selection_benchmark(
                repo=args.repo.resolve(), base_revision=args.base_revision,
                driver_argv=driver_argv,
                case_ids=args.case_ids or list(DEFAULT_SELECTION_CASES), samples=args.samples,
            )
        else:
            result = run_benchmark(
                repo=args.repo.resolve(), base_revision=args.base_revision,
                driver_argv=driver_argv,
                case_ids=args.case_ids or list(DEFAULT_CASES), pairs=args.pairs,
                candidate=args.candidate,
            )
        if args.output:
            args.output.parent.mkdir(parents=True, exist_ok=True)
            args.output.write_text(_stable_json(result), encoding="utf-8")
        print(_human_summary(result))
        if not args.output:
            print(_stable_json(result), end="")
        if args.strict:
            requested = args.case_ids or list(DEFAULT_SELECTION_CASES if args.experiment == "tool_selection" else DEFAULT_CASES)
            repetitions = args.samples if args.experiment == "tool_selection" else args.pairs
            manifest = report.validate_case_manifest(json.loads(_read_manifest_at_revision(_git_root(args.repo), result["base_revision"])))
            return 0 if _strict_passed(result, requested, repetitions, manifest) else 1
        return 0 if result["status_counts"]["fail"] == 0 else 1
    except (BenchmarkError, report.ReportError) as exc:
        print(f"agent_loop_benchmark: {exc}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
