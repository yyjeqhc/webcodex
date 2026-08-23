use serde_json::{json, Value};
use std::path::Path;

use super::helpers::{
    command_rejected_message, explicit_shell_dispatch_command, is_safe_job_id,
    normalize_local_status, project_relative_agent_cwd, project_relative_cwd, resolve_agent_cwd,
    resolve_local_cwd, shell_escape_simple, validate_raw_shell_command_length, MAX_LOCAL_LOG_LINES,
};
use super::local_jobs::{
    retain_inspect_job_until_terminal, LocalJobKiller, LocalJobLogSnapshot, LocalJobRecord,
    TerminateOutcome, ACTIVE_JOB_STATUSES, ACTIVE_LOCAL_STATUSES,
};
use super::tool_result::{RecoveryKind, RecoveryTool, ToolResult};
use super::{ExecutionPurpose, ExecutionShell, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_client::{command_preview, ShellJobStartMetadata, COMMAND_PREVIEW_MAX_CHARS};
use crate::shell_protocol::{ShellJobInfo, ShellJobOpRequest, ShellJobValidationStep};

pub(crate) fn is_blocking_active_job_status(status: &str) -> bool {
    matches!(
        status,
        "queued" | "running" | "started" | "agent_queued" | "recovering"
    )
}

pub(crate) fn is_stop_pending_job_status(status: &str) -> bool {
    status == "stop_requested"
}

pub(crate) fn is_terminal_job_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "failed" | "stopped" | "lost" | "timeout" | "timed_out" | "cancelled"
    )
}

fn detected_job_summary(
    command_summary: Option<&str>,
    purpose: Option<&str>,
    status: &str,
    exit_code: Option<i64>,
    stdout: &str,
    stderr: &str,
) -> Value {
    let normalized = command_summary
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let kind = if normalized.starts_with("cargo test") {
        "test"
    } else if normalized.starts_with("cargo check") {
        "check"
    } else if normalized.starts_with("cargo fmt") {
        "format"
    } else if normalized.starts_with("cargo build") {
        "build"
    } else {
        match purpose {
            Some("other") | None => "operation",
            Some(purpose) => purpose,
        }
    };
    let outcome = if !is_terminal_job_status(status) {
        "in_progress"
    } else if status == "completed" && exit_code == Some(0) {
        "passed"
    } else if matches!(status, "timeout" | "timed_out") {
        "timed_out"
    } else if matches!(status, "stopped" | "cancelled") {
        "cancelled"
    } else {
        "failed"
    };
    let mut detected = json!({
        "kind": kind,
        "outcome": outcome,
    });
    if kind == "test" {
        let combined = format!("{stdout}\n{stderr}");
        let metadata = super::cargo::parse_cargo_test_run_metadata(&combined);
        let (passed, failed) = super::cargo::parse_cargo_test_counts(&combined);
        detected["tests_detected"] = json!(metadata.tests_detected);
        detected["tests_run_count"] = json!(metadata.tests_run_count);
        detected["zero_tests_run"] = json!(metadata.zero_tests_run);
        detected["tests_passed"] = json!(passed);
        detected["tests_failed"] = json!(failed);
    }
    detected
}

#[derive(Debug, Clone)]
pub(crate) struct StructuredValidationEvidence {
    pub(crate) diagnostics: Option<super::validation_parser::ValidationDiagnostics>,
    pub(crate) tests_detected: Option<bool>,
    pub(crate) tests_run_count: Option<u64>,
    pub(crate) tests_passed: Option<u64>,
    pub(crate) tests_failed: Option<u64>,
    pub(crate) zero_tests_run: Option<bool>,
    pub(crate) warnings_count: Option<u64>,
    pub(crate) errors_count: Option<u64>,
}

pub(crate) fn structured_validation_evidence(
    tool: &str,
    kind: &str,
    stdout: &str,
    stderr: &str,
    truncated: bool,
) -> StructuredValidationEvidence {
    let combined = format!("{stdout}\n{stderr}");
    let diagnostics = matches!(kind, "check" | "test")
        .then(|| super::validation_profile::validation_adapter_for_tool(tool))
        .flatten()
        .map(|adapter| adapter.parse(stdout, stderr, truncated));
    let mut evidence = StructuredValidationEvidence {
        diagnostics,
        tests_detected: None,
        tests_run_count: None,
        tests_passed: None,
        tests_failed: None,
        zero_tests_run: None,
        warnings_count: None,
        errors_count: None,
    };
    match kind {
        "test" if tool == "go_test" => {
            let test_summary = evidence
                .diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.test_summary.as_ref());
            evidence.tests_detected = Some(test_summary.is_some());
            if !truncated {
                evidence.tests_passed = test_summary.and_then(|summary| summary.passed);
                evidence.tests_failed = test_summary.and_then(|summary| summary.failed);
                evidence.tests_run_count = test_summary.map(|summary| {
                    summary
                        .passed
                        .unwrap_or(0)
                        .saturating_add(summary.failed.unwrap_or(0))
                        .saturating_add(summary.ignored.unwrap_or(0))
                });
                evidence.zero_tests_run = evidence.tests_run_count.map(|count| count == 0);
            }
        }
        "test" => {
            let metadata = super::cargo::parse_cargo_test_run_metadata(&combined);
            let (tests_passed, tests_failed) = super::cargo::parse_cargo_test_counts(&combined);
            evidence.tests_detected = Some(metadata.tests_detected);
            if !truncated {
                evidence.tests_run_count = metadata.tests_run_count;
                evidence.tests_passed = tests_passed;
                evidence.tests_failed = tests_failed;
                evidence.zero_tests_run = metadata.zero_tests_run;
            }
        }
        "check" if !truncated => {
            evidence.warnings_count =
                Some(super::cargo::count_rustc_diagnostics(&combined, "warning:") as u64);
            evidence.errors_count =
                Some(super::cargo::count_rustc_diagnostics(&combined, "error:") as u64);
        }
        _ => {}
    }
    evidence
}

pub(crate) fn validation_job_projection(
    tool: Option<&str>,
    kind: Option<&str>,
    status: &str,
    exit_code: Option<i64>,
    stdout: &str,
    stderr: &str,
    truncated: bool,
) -> Option<Value> {
    let tool = tool?;
    let kind = kind.unwrap_or(match tool {
        "cargo_test" | "go_test" => "test",
        "cargo_fmt" => "format",
        _ => "check",
    });
    if !is_terminal_job_status(status) {
        return Some(json!({
            "tool": tool,
            "kind": kind,
            "state": if status == "queued" || status == "agent_queued" { "pending" } else { "running" },
        }));
    }
    if matches!(
        status,
        "timeout" | "timed_out" | "stopped" | "cancelled" | "lost"
    ) {
        let evidence = structured_validation_evidence(tool, kind, stdout, stderr, true);
        let mut value = json!({
            "tool": tool,
            "kind": kind,
            "state": match status {
                "timeout" | "timed_out" => "timed_out",
                "stopped" | "cancelled" => "cancelled",
                _ => "lost",
            },
            "passed": Value::Null,
            "truncated": true,
        });
        if let Some(diagnostics) = evidence.diagnostics {
            value["diagnostics"] = json!(diagnostics);
        }
        match kind {
            "test" => {
                value["tests_detected"] = json!(evidence.tests_detected);
                value["tests_run_count"] = Value::Null;
                value["tests_passed"] = Value::Null;
                value["tests_failed"] = Value::Null;
                value["zero_tests_run"] = Value::Null;
            }
            "check" => {
                value["warnings_count"] = Value::Null;
                value["errors_count"] = Value::Null;
            }
            _ => {}
        }
        return Some(value);
    }
    let passed = status == "completed" && exit_code == Some(0);
    let evidence = structured_validation_evidence(tool, kind, stdout, stderr, truncated);
    let mut value = json!({
        "tool": tool,
        "kind": kind,
        "state": "completed",
        "passed": passed,
        "truncated": truncated,
    });
    if let Some(diagnostics) = evidence.diagnostics {
        value["diagnostics"] = json!(diagnostics);
    }
    match kind {
        "test" => {
            value["tests_detected"] = json!(evidence.tests_detected);
            value["tests_run_count"] = json!(evidence.tests_run_count);
            value["tests_passed"] = json!(evidence.tests_passed);
            value["tests_failed"] = json!(evidence.tests_failed);
            value["zero_tests_run"] = json!(evidence.zero_tests_run);
        }
        "check" => {
            value["warnings_count"] = json!(evidence.warnings_count);
            value["errors_count"] = json!(evidence.errors_count);
        }
        _ => {}
    }
    Some(value)
}

fn local_validation_identity(meta: &Value) -> (Option<&str>, Option<&str>) {
    (
        meta.get("validation_tool").and_then(Value::as_str),
        meta.get("validation_kind").and_then(Value::as_str),
    )
}

fn is_lifecycle_active_status(status: &str) -> bool {
    is_blocking_active_job_status(status) || is_stop_pending_job_status(status)
}

fn add_job_lifecycle_fields(
    output: &mut Value,
    status: &str,
    recovery_state: Option<&str>,
    recovery_reason_code: Option<&str>,
) {
    let blocking_active = is_blocking_active_job_status(status);
    let terminal_pending = is_stop_pending_job_status(status);
    output["active"] = json!(blocking_active || terminal_pending);
    output["blocking_active"] = json!(blocking_active);
    output["terminal"] = json!(is_terminal_job_status(status));
    output["terminal_pending"] = json!(terminal_pending);
    if let Some(text) = recovery_reason_text(recovery_state, recovery_reason_code) {
        output["recovery_reason"] = json!(text);
    }
}

/// Map the bounded `recovery_state` / `recovery_reason_code` pair to a stable,
/// human-readable `recovery_reason` string for the Console/API projection.
///
/// The text is derived only from the bounded reason codes and the recovery
/// state — never from raw backend error strings, tokens, command payloads,
/// environment, filesystem paths, transport connection ids, raw inventory, or
/// internal notifier/request-channel state. Unknown reason codes fall back to
/// a generic form that echoes only the code (safe to surface, not sensitive).
pub(crate) fn recovery_reason_text(
    recovery_state: Option<&str>,
    recovery_reason_code: Option<&str>,
) -> Option<String> {
    match (recovery_state, recovery_reason_code) {
        (Some("recovering"), _) => {
            Some("server is waiting for the same runner instance to reconnect".to_string())
        }
        (Some("reconciled"), _) => Some("reconciled after runner reconnect".to_string()),
        (Some("lost_after_reconcile"), Some(code)) => Some(match code {
            "runner_recovery_deadline_exceeded" => {
                "lost: runner did not reconnect before the recovery deadline".to_string()
            }
            "runner_inventory_missing" => {
                "lost: runner reconnect did not report this job in its inventory".to_string()
            }
            "runner_instance_replaced" => {
                "lost: runner instance was replaced by a newer process".to_string()
            }
            _ => format!("lost after reconciliation ({code})"),
        }),
        (Some("lost_after_reconcile"), None) => Some("lost after reconciliation".to_string()),
        // Jobs lost without entering the reconciliation path keep their original
        // reason code (e.g. legacy disconnect) and have recovery_state == None.
        (_, Some("legacy_runner_disconnected")) => {
            Some("lost: legacy runner disconnected without reconciliation support".to_string())
        }
        (_, Some("runner_transport_disconnected")) => {
            Some("lost: runner transport disconnected".to_string())
        }
        (_, Some("runner_transport_stale")) => {
            Some("lost: runner transport went stale while the job was running".to_string())
        }
        (_, Some("runner_request_not_dispatched")) => {
            Some("lost: runner did not dispatch the job request".to_string())
        }
        (_, Some(code)) => Some(format!("recovery ({code})")),
        (Some(state), None) => Some(format!("recovery ({state})")),
        (None, None) => None,
    }
}

fn command_preview_truncated(preview: &str) -> bool {
    preview.chars().count() > COMMAND_PREVIEW_MAX_CHARS
}

fn add_command_preview_metadata(output: &mut Value, preview: String) {
    output["command_preview_truncated"] = json!(command_preview_truncated(&preview));
    output["command_preview_max_chars"] = json!(COMMAND_PREVIEW_MAX_CHARS);
    output["command_preview_bounded"] = json!(true);
    output["command_preview"] = Value::String(preview);
}

fn job_id_for_log(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("<unknown>")
        .to_string()
}

fn local_read_trim(record: &LocalJobRecord, name: &str) -> Option<String> {
    record
        .read_text(name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn local_read_log_pair(
    record: LocalJobRecord,
    offset: Option<usize>,
    stdout_len: u64,
    stderr_len: u64,
) -> (LocalJobLogSnapshot, LocalJobLogSnapshot) {
    tokio::task::spawn_blocking(move || {
        (
            record.read_log_snapshot_at("stdout.log", offset, stdout_len),
            record.read_log_snapshot_at("stderr.log", offset, stderr_len),
        )
    })
    .await
    .ok()
    .and_then(|(stdout, stderr)| Some((stdout?, stderr?)))
    .unwrap_or_else(|| {
        let empty = || LocalJobLogSnapshot {
            retained_text: String::new(),
            total_lines: 0,
            first_retained_line: 1,
            truncated: false,
        };
        (empty(), empty())
    })
}

fn local_runtime_deadline(meta: &Value) -> Option<tokio::time::Instant> {
    let started_at = meta.get("started_at").and_then(Value::as_i64)?;
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64)?;
    let remaining = started_at
        .saturating_add(max_runtime_secs)
        .saturating_sub(chrono::Utc::now().timestamp());
    Some(tokio::time::Instant::now() + tokio::time::Duration::from_secs(remaining.max(0) as u64))
}

fn agent_log_stream_incomplete(
    runner_truncated: bool,
    retained_from_line: Option<usize>,
    returned_lines: usize,
    next_line: usize,
) -> bool {
    runner_truncated
        || retained_from_line.is_some_and(|line| line > 1)
        || returned_lines < next_line.saturating_sub(1)
}

/// Build a bounded job summary `Value` for an agent-known job. Never includes
/// stdout/stderr bodies.
pub(crate) fn agent_job_summary_value(job: &ShellJobInfo) -> Value {
    json!({
        "job_id": job.job_id,
        "kind": job.kind,
        "status": job.status,
        "project": job.project_id,
        "session_id": job.session_id,
        "ssh_resource": job.ssh_resource,
        "executor": "agent",
        "client_id": job.client_id,
        "created_at": job.created_at,
        "started_at": job.started_at,
        "ended_at": job.ended_at,
        "duration_ms": job.duration_ms,
        "elapsed_secs": job.elapsed_secs,
        "exit_code": job.exit_code,
        "command_execution_state": job.command_execution_state,
        "structured_execution": job.structured_execution,
        "recovery_state": job.recovery_state,
        "recovered_after_server_restart": job.recovered_after_server_restart,
        "reconciled_at": job.reconciled_at,
        "recovery_reason_code": job.recovery_reason_code,
        "last_update_seq": job.last_update_seq,
        "recovery_reason": recovery_reason_text(
            job.recovery_state.as_deref(),
            job.recovery_reason_code.as_deref(),
        ),
    })
}

/// Build a bounded job summary `Value` for a local on-disk job by reading
/// lightweight metadata/status files. Returns `None` when a status filter is
/// set and the job does not match. Never includes stdout/stderr bodies.
pub(crate) fn local_job_summary_value(
    job_id: &str,
    record: &LocalJobRecord,
    status_filter: &Option<String>,
) -> Option<Value> {
    let meta = record.read_json("metadata.json");
    let raw_status = local_read_trim(record, "status").unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if let Some(filter) = status_filter {
        if &status != filter {
            return None;
        }
    }
    let exit_code = local_read_trim(record, "exit_code").and_then(|v| v.parse::<i32>().ok());
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let ended_at = local_read_trim(record, "finished_at").and_then(|v| v.parse::<i64>().ok());
    let kind = meta
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("shell")
        .to_string();
    Some(json!({
        "job_id": job_id,
        "kind": kind,
        "status": status,
        "project": record.project,
        "session_id": meta.get("session_id").cloned().unwrap_or(Value::Null),
        "executor": "local",
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": ended_at,
        "exit_code": exit_code,
    }))
}

pub(crate) fn local_job_status(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
    include_command_preview: bool,
) -> ToolResult {
    // Reclaim overtime jobs before reading status: this persists a terminal
    // `lost` status (and terminates the process group) so callers see a
    // consistent terminal state and we don't leak processes.
    let timeout_note = enforce_local_job_timeout(record, killer);
    let observation = match record.observe() {
        Ok(observation) => observation,
        Err(error) => return ToolResult::err(error),
    };
    let observation_token = match observation.token(job_id) {
        Ok(token) => token,
        Err(error) => return ToolResult::err(error),
    };
    let meta = record.read_json("metadata.json");
    let status = normalize_local_status(&observation.status);
    let exit_code = if observation.terminal() {
        local_read_trim(record, "exit_code").and_then(|value| value.parse::<i32>().ok())
    } else {
        None
    };
    let created_at = meta
        .get("created_at")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    let started_at = meta.get("started_at").and_then(Value::as_i64);
    let finished_at = if observation.terminal() {
        local_read_trim(record, "finished_at").and_then(|value| value.parse::<i64>().ok())
    } else {
        None
    };
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64);
    let elapsed_secs = started_at.map(|started| {
        finished_at
            .unwrap_or_else(|| chrono::Utc::now().timestamp())
            .saturating_sub(started) as u64
    });
    let mut output = json!({
        "job_id": job_id,
        "project": record.project,
        "session_id": meta.get("session_id").cloned().unwrap_or(Value::Null),
        "status": status,
        "exit_code": exit_code,
        "created_at": created_at,
        "started_at": started_at,
        "ended_at": finished_at,
        "elapsed_secs": elapsed_secs,
        "max_runtime_secs": max_runtime_secs,
        "executor": "local",
        "observation_token": observation_token,
        "kind": meta.get("kind").cloned().unwrap_or_else(|| Value::String("shell".to_string())),
        "command_preview_included": include_command_preview,
    });
    let (validation_tool, validation_kind) = local_validation_identity(&meta);
    let stdout = record.read_log_lines("stdout.log", None, Some(MAX_LOCAL_LOG_LINES));
    let stderr = record.read_log_lines("stderr.log", None, Some(MAX_LOCAL_LOG_LINES));
    if let Some(mut validation) = validation_job_projection(
        validation_tool,
        validation_kind,
        &status,
        exit_code.map(i64::from),
        &stdout.0,
        &stderr.0,
        stdout.3 || stderr.3,
    ) {
        if let Some(target_id) = meta.get("validation_target_id").and_then(Value::as_str) {
            validation["validation_target_id"] = json!(target_id);
        }
        output["validation"] = validation;
    }
    add_job_lifecycle_fields(&mut output, &status, None, None);
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    if include_command_preview {
        if let Some(command) = meta.get("command").and_then(Value::as_str) {
            add_command_preview_metadata(&mut output, command_preview(command));
        }
    }
    ToolResult::ok(output)
}

pub(crate) async fn local_job_log(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
    offset: Option<usize>,
    tail_lines: Option<usize>,
    after_observation_token: Option<String>,
    wait_secs: Option<u64>,
) -> ToolResult {
    let after = match after_observation_token
        .as_deref()
        .map(|value| {
            crate::job_observation::JobObservationToken::parse_bound(
                value,
                crate::job_observation::JobObservationExecutor::Local,
                job_id,
            )
            .map_err(|error| error.to_string())
        })
        .transpose()
    {
        Ok(token) => token,
        Err(error) => return invalid_job_observation_result("invalid_observation_token", error),
    };
    let mut timeout_note = enforce_local_job_timeout(record, killer);
    let meta = record.read_json("metadata.json");
    let mut observation = match record.observe() {
        Ok(observation) => observation,
        Err(error) => return ToolResult::err(error),
    };
    let wait_deadline =
        wait_secs.map(|secs| tokio::time::Instant::now() + tokio::time::Duration::from_secs(secs));
    let runtime_deadline = local_runtime_deadline(&meta);
    let changed_now = after.as_ref().is_some_and(|token| {
        token.epoch != observation.epoch || token.revision != observation.revision
    });
    let mut wait_outcome = if changed_now {
        "immediate"
    } else if observation.terminal() {
        "terminal"
    } else {
        "immediate"
    };
    let mut changed = changed_now;
    let mut waited_ms = 0u64;

    if wait_secs.is_some() && after.is_some() && !observation.terminal() && !changed {
        loop {
            let now = tokio::time::Instant::now();
            let poll_deadline = now + tokio::time::Duration::from_millis(200);
            let next_deadline = [wait_deadline, runtime_deadline, Some(poll_deadline)]
                .into_iter()
                .flatten()
                .min()
                .unwrap();
            let wait_started = tokio::time::Instant::now();
            tokio::time::sleep_until(next_deadline).await;
            waited_ms = waited_ms.saturating_add(wait_started.elapsed().as_millis() as u64);
            let now = tokio::time::Instant::now();
            if runtime_deadline.is_some_and(|deadline| now >= deadline) {
                timeout_note = enforce_local_job_timeout(record, killer).or(timeout_note);
            }
            observation = match record.observe() {
                Ok(observation) => observation,
                Err(error) => return ToolResult::err(error),
            };
            changed = after.as_ref().is_some_and(|token| {
                token.epoch != observation.epoch || token.revision != observation.revision
            });
            if observation.terminal() {
                wait_outcome = "terminal";
                break;
            }
            if changed {
                wait_outcome = "updated";
                break;
            }
            if wait_deadline.is_some_and(|deadline| now >= deadline) {
                wait_outcome = "timeout";
                break;
            }
        }
    }
    let final_status = normalize_local_status(&observation.status);
    let final_terminal = observation.terminal();
    if final_terminal && !changed_now && wait_outcome != "updated" {
        wait_outcome = "terminal";
    }
    if wait_outcome == "timeout" {
        changed = false;
    }
    let frozen_stdout_len = observation.stdout_len;
    let frozen_stderr_len = observation.stderr_len;
    let final_exit_code = if final_terminal {
        local_read_trim(record, "exit_code").and_then(|value| value.parse::<i32>().ok())
    } else {
        None
    };
    let explicit_paging = offset.is_some();
    let (stdout_snapshot, stderr_snapshot) = local_read_log_pair(
        record.clone(),
        if explicit_paging { offset } else { None },
        frozen_stdout_len,
        frozen_stderr_len,
    )
    .await;
    let (analysis_stdout_snapshot, analysis_stderr_snapshot) = if explicit_paging {
        local_read_log_pair(record.clone(), None, frozen_stdout_len, frozen_stderr_len).await
    } else {
        (stdout_snapshot.clone(), stderr_snapshot.clone())
    };
    let analysis_stdout = crate::job_observation::project_log_stream(
        &analysis_stdout_snapshot.retained_text,
        analysis_stdout_snapshot.first_retained_line,
        analysis_stdout_snapshot.total_lines.saturating_add(1),
        analysis_stdout_snapshot.truncated,
        Some(MAX_LOCAL_LOG_LINES),
        crate::job_observation::JobLogSelectionMode::Baseline,
        false,
    );
    let analysis_stderr = crate::job_observation::project_log_stream(
        &analysis_stderr_snapshot.retained_text,
        analysis_stderr_snapshot.first_retained_line,
        analysis_stderr_snapshot.total_lines.saturating_add(1),
        analysis_stderr_snapshot.truncated,
        Some(MAX_LOCAL_LOG_LINES),
        crate::job_observation::JobLogSelectionMode::Baseline,
        false,
    );
    let (
        stdout,
        stderr,
        log_delta_status,
        observation_token,
        stdout_delta_reset,
        stderr_delta_reset,
    ) = if explicit_paging {
        let stdout = stdout_snapshot.read_lines(offset, tail_lines);
        let stderr = stderr_snapshot.read_lines(offset, tail_lines);
        let stdout = crate::job_observation::JobLogStreamProjection {
            returned_lines: stdout.0.lines().count(),
            text: stdout.0,
            next_line: stdout.1,
            total_lines: stdout.2,
            first_retained_line: stdout_snapshot.first_retained_line,
            truncated: stdout.3,
            delta_reset: false,
        };
        let stderr = crate::job_observation::JobLogStreamProjection {
            returned_lines: stderr.0.lines().count(),
            text: stderr.0,
            next_line: stderr.1,
            total_lines: stderr.2,
            first_retained_line: stderr_snapshot.first_retained_line,
            truncated: stderr.3,
            delta_reset: false,
        };
        let observation_token = match observation.token(job_id) {
            Ok(token) => token,
            Err(error) => return ToolResult::err(error),
        };
        (
            stdout,
            stderr,
            crate::job_observation::JobLogDeltaStatus::Baseline,
            observation_token,
            false,
            false,
        )
    } else {
        let automatic_tail_lines = tail_lines.or(Some(super::helpers::DEFAULT_JOB_LOG_TAIL_LINES));
        let epoch_matches = after
            .as_ref()
            .is_none_or(|token| token.epoch == observation.epoch);
        let stdout_mode = match after.as_ref() {
            None => crate::job_observation::JobLogSelectionMode::Baseline,
            Some(token) if token.is_legacy() || !epoch_matches => {
                crate::job_observation::JobLogSelectionMode::Reset
            }
            Some(token) => crate::job_observation::JobLogSelectionMode::Delta {
                cursor: token
                    .stdout_cursor
                    .expect("cursor-aware token has stdout cursor"),
            },
        };
        let stderr_mode = match after.as_ref() {
            None => crate::job_observation::JobLogSelectionMode::Baseline,
            Some(token) if token.is_legacy() || !epoch_matches => {
                crate::job_observation::JobLogSelectionMode::Reset
            }
            Some(token) => crate::job_observation::JobLogSelectionMode::Delta {
                cursor: token
                    .stderr_cursor
                    .expect("cursor-aware token has stderr cursor"),
            },
        };
        let stdout = crate::job_observation::project_log_stream(
            &stdout_snapshot.retained_text,
            stdout_snapshot.first_retained_line,
            stdout_snapshot.total_lines.saturating_add(1),
            stdout_snapshot.truncated,
            automatic_tail_lines,
            stdout_mode,
            false,
        );
        let stderr = crate::job_observation::project_log_stream(
            &stderr_snapshot.retained_text,
            stderr_snapshot.first_retained_line,
            stderr_snapshot.total_lines.saturating_add(1),
            stderr_snapshot.truncated,
            automatic_tail_lines,
            stderr_mode,
            false,
        );
        let log_delta_status =
            crate::job_observation::combined_delta_status(stdout_mode, &stdout, &stderr);
        let observation_token =
            match observation.token_with_cursors(job_id, stdout.next_line, stderr.next_line) {
                Ok(token) => token,
                Err(error) => return ToolResult::err(error),
            };
        let stdout_delta_reset = stdout.delta_reset;
        let stderr_delta_reset = stderr.delta_reset;
        (
            stdout,
            stderr,
            log_delta_status,
            observation_token,
            stdout_delta_reset,
            stderr_delta_reset,
        )
    };
    let purpose = meta
        .get("purpose")
        .and_then(Value::as_str)
        .unwrap_or("other");
    let command_summary = meta
        .get("command")
        .and_then(Value::as_str)
        .map(command_preview)
        .unwrap_or_default();
    let detected_summary = detected_job_summary(
        Some(&command_summary),
        Some(purpose),
        &final_status,
        final_exit_code.map(i64::from),
        &analysis_stdout.text,
        &analysis_stderr.text,
    );
    let (validation_tool, validation_kind) = local_validation_identity(&meta);
    let mut validation = validation_job_projection(
        validation_tool,
        validation_kind,
        &final_status,
        final_exit_code.map(i64::from),
        &analysis_stdout.text,
        &analysis_stderr.text,
        analysis_stdout.truncated || analysis_stderr.truncated,
    );
    if let (Some(validation), Some(target_id)) = (
        validation.as_mut(),
        meta.get("validation_target_id").and_then(Value::as_str),
    ) {
        validation["validation_target_id"] = json!(target_id);
    }
    let mut output = json!({
        "job_id": job_id, "status": final_status, "exit_code": final_exit_code,
        "session_id": meta.get("session_id").cloned().unwrap_or(Value::Null),
        "stdout_tail": stdout.text, "stderr_tail": stderr.text,
        "stdout_lines": stdout.total_lines, "stderr_lines": stderr.total_lines,
        "stdout_returned_lines": stdout.returned_lines,
        "stderr_returned_lines": stderr.returned_lines,
        "stdout_truncated": stdout.truncated, "stderr_truncated": stderr.truncated,
        "stdout_retained_from_line": stdout.first_retained_line,
        "stderr_retained_from_line": stderr.first_retained_line,
        "earlier_stdout_unavailable": stdout_snapshot.first_retained_line > 1
            || stdout_snapshot.truncated,
        "earlier_stderr_unavailable": stderr_snapshot.first_retained_line > 1
            || stderr_snapshot.truncated,
        "cursor": { "stdout": stdout.next_line, "stderr": stderr.next_line },
        "observation_token": observation_token,
        "log_delta_status": log_delta_status.as_str(),
        "stdout_delta_reset": stdout_delta_reset,
        "stderr_delta_reset": stderr_delta_reset,
        "wait_outcome": wait_outcome, "waited_ms": waited_ms,
        "changed": changed, "terminal": final_terminal, "executor": "local",
        "cwd": meta.get("cwd").cloned().unwrap_or_else(|| json!(".")),
        "shell": meta.get("shell").cloned().unwrap_or_else(|| json!("bash")),
        "purpose": purpose,
        "command_summary": command_summary,
        "detected_summary": detected_summary,
        "validation": validation,
    });
    if let Some(note) = timeout_note {
        output["note"] = Value::String(note);
    }
    ToolResult::ok(output)
}

/// Resolve the process-group id to signal for a local job. Prefers an explicit
/// `process_group_id` in metadata (written by current spawn code); falls back
/// to the `pid` file, which under `setsid` is equal to the pgid. Returns
/// `None` when neither is recorded (e.g. very old metadata predating pid
/// tracking) — in that case we never guess at a pid to kill.
pub(crate) fn resolve_job_pgid(meta: &Value, record: &LocalJobRecord) -> Option<i64> {
    meta.get("process_group_id")
        .and_then(Value::as_i64)
        .or_else(|| local_read_trim(record, "pid").and_then(|s| s.parse::<i64>().ok()))
}

/// If a local job is still `running` but has exceeded `max_runtime_secs`,
/// terminate its process group and persist a terminal `lost` status. Returns a
/// short human-readable note when a timeout was enforced, or `None` if the job
/// is not running or not over time.
///
/// Safety: the pid/pgid come only from this job's own on-disk files (written by
/// us at spawn time via `setsid`). We never kill based on caller-supplied pids.
/// If no pid/pgid is recorded, we only mark the job `lost` — never guess. Kill
/// failures never panic; a conservative `lost` status is persisted regardless.
pub(crate) fn enforce_local_job_timeout(
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> Option<String> {
    let meta = record.read_json("metadata.json");
    let raw_status = local_read_trim(record, "status").unwrap_or_default();
    if normalize_local_status(&raw_status) != "running" {
        return None;
    }
    let started_at = meta.get("started_at").and_then(Value::as_i64)?;
    let max_runtime_secs = meta.get("max_runtime_secs").and_then(Value::as_i64)?;
    // The wrapper writes `finished_at` before `status`. If it exists, the job
    // just finished (or was already reclaimed) — do not double-reclaim.
    if local_read_trim(record, "finished_at").is_some() {
        return None;
    }
    let now = chrono::Utc::now().timestamp();
    if now < started_at.saturating_add(max_runtime_secs) {
        return None;
    }
    // Over time. Reclaim the process group if we recorded one.
    let pgid = resolve_job_pgid(&meta, record);
    let note = match pgid {
        Some(pgid) => {
            let pid = local_read_trim(record, "pid")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!(
                        "timed out after {}s; process group {} terminated ({})",
                        max_runtime_secs, pgid, sig
                    )
                }
                TerminateOutcome::AlreadyGone => format!(
                    "timed out after {}s; process group {} already exited; marked lost",
                    max_runtime_secs, pgid
                ),
            }
        }
        None => format!(
            "timed out after {}s; no pid/process_group_id on record; marked lost",
            max_runtime_secs
        ),
    };
    // Persist terminal state so subsequent reads are consistent and we don't
    // repeatedly attempt to kill. The wrapper shell was part of the group and
    // is now gone, so it will not write its own status/finished_at.
    if let Err(e) = std::fs::write(record.dir.join("finished_at"), now.to_string()) {
        tracing::warn!(
            job_id = %job_id_for_log(&record.dir),
            error = %e,
            "failed to write timed-out local job finished_at"
        );
    }
    if let Err(e) = std::fs::write(record.dir.join("status"), "lost") {
        tracing::warn!(
            job_id = %job_id_for_log(&record.dir),
            error = %e,
            "failed to write timed-out local job status"
        );
    }
    Some(note)
}

/// Stop a local job by terminating its process group and persisting a
/// `stopped` status. Only acts on active jobs; terminal jobs are left alone.
/// Like `enforce_local_job_timeout`, the pid/pgid come only from the job's own
/// on-disk files, and missing pid/pgid yields a conservative `stopped` marker
/// without guessing. Kill failures never panic.
pub(crate) fn stop_local_job(
    job_id: &str,
    record: &LocalJobRecord,
    killer: &dyn LocalJobKiller,
) -> ToolResult {
    let meta = record.read_json("metadata.json");
    let raw_status = local_read_trim(record, "status").unwrap_or_default();
    let status = normalize_local_status(&raw_status);
    if !ACTIVE_LOCAL_STATUSES.contains(&status.as_str()) {
        return ToolResult::ok(json!({
            "job_id": job_id,
            "project": record.project,
            "status": status,
            "note": "job already terminal; not stopped again",
        }));
    }
    let now = chrono::Utc::now().timestamp();
    let note = match resolve_job_pgid(&meta, record) {
        Some(pgid) => {
            let pid = local_read_trim(record, "pid")
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(pgid);
            let outcome = killer.terminate_group(pid, pgid);
            match outcome {
                TerminateOutcome::Terminated {
                    pgid,
                    escalated_to_kill,
                } => {
                    let sig = if escalated_to_kill {
                        "SIGKILL"
                    } else {
                        "SIGTERM"
                    };
                    format!("stopped; process group {} terminated ({})", pgid, sig)
                }
                TerminateOutcome::AlreadyGone => {
                    format!("stopped; process group {} already exited", pgid)
                }
            }
        }
        None => "stopped; no pid/process_group_id on record; marked stopped".to_string(),
    };
    if let Err(e) = std::fs::write(record.dir.join("finished_at"), now.to_string()) {
        tracing::warn!(
            job_id,
            error = %e,
            "failed to write stopped local job finished_at"
        );
    }
    if let Err(e) = std::fs::write(record.dir.join("status"), "stopped") {
        tracing::warn!(
            job_id,
            error = %e,
            "failed to write stopped local job status"
        );
    }
    ToolResult::ok(json!({
        "job_id": job_id,
        "project": record.project,
        "status": "stopped",
        "note": note,
    }))
}

fn local_job_status_string(record: &LocalJobRecord) -> String {
    let raw_status = local_read_trim(record, "status").unwrap_or_default();
    normalize_local_status(&raw_status)
}

fn local_job_session_id(record: &LocalJobRecord) -> Option<String> {
    record
        .read_json("metadata.json")
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn invalid_job_observation_result(error_kind: &str, message: String) -> ToolResult {
    ToolResult::err_with_output(
        message,
        json!({
            "error_kind": error_kind,
            "failure_kind": "invalid_arguments",
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

fn unknown_job_observation_result(job_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("unknown job: {}", job_id),
        json!({
            "error_kind": "unknown_job",
            "failure_kind": "job_not_found",
            "job_id": job_id,
            "state_changed": false,
        }),
    )
    .with_recovery(RecoveryKind::Reobserve, Some(RecoveryTool::ListJobs))
}

fn agent_job_log_error_result(job_id: &str, error: String) -> ToolResult {
    if error.starts_with("invalid after_observation_token:") {
        invalid_job_observation_result("invalid_observation_token", error)
    } else {
        unknown_job_observation_result(job_id)
    }
}

fn confirmation_required_result(project: &str, job_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        "confirmation_required: stop_job requires confirm=true".to_string(),
        json!({
            "error_kind": "confirmation_required",
            "failure_kind": "confirmation_required",
            "project": project,
            "job_id": job_id,
            "already_finished": false,
            "already_stop_requested": false,
            "stop_request_accepted": false,
            "target_was_active_at_request": false,
            "terminal": false,
            "terminal_pending": false,
            "final_status": Value::Null,
            "stop_effect": "confirmation_required",
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::UserAction, None)
}

fn job_not_found_result(project: &str, job_id: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!("job_not_found: {}", job_id),
        json!({
            "error_kind": "job_not_found",
            "failure_kind": "job_not_found",
            "project": project,
            "job_id": job_id,
            "already_finished": false,
            "already_stop_requested": false,
            "stop_request_accepted": false,
            "target_was_active_at_request": false,
            "terminal": false,
            "terminal_pending": false,
            "final_status": Value::Null,
            "stop_effect": "not_found",
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::Reobserve, Some(RecoveryTool::ListJobs))
}

fn job_project_mismatch_result(
    request_project: &str,
    job_project: &str,
    job_id: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "job_project_mismatch: job {} belongs to project {} but request used {}",
            job_id, job_project, request_project
        ),
        json!({
            "error_kind": "job_project_mismatch",
            "failure_kind": "job_project_mismatch",
            "project": request_project,
            "job_project": job_project,
            "job_id": job_id,
            "already_finished": false,
            "already_stop_requested": false,
            "stop_request_accepted": false,
            "target_was_active_at_request": false,
            "terminal": false,
            "terminal_pending": false,
            "final_status": Value::Null,
            "stop_effect": "forbidden",
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

fn job_stop_forbidden_result(
    request_project: &str,
    job_id: &str,
    request_session_id: Option<&str>,
    job_session_id: Option<&str>,
) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "job_stop_forbidden: job {} is bound to a different session",
            job_id
        ),
        json!({
            "error_kind": "job_stop_forbidden",
            "failure_kind": "job_stop_forbidden",
            "project": request_project,
            "job_id": job_id,
            "request_session_id": request_session_id,
            "job_session_id": job_session_id,
            "already_finished": false,
            "already_stop_requested": false,
            "stop_request_accepted": false,
            "target_was_active_at_request": false,
            "terminal": false,
            "terminal_pending": false,
            "final_status": Value::Null,
            "stop_effect": "forbidden",
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::FixInput, None)
}

fn job_session_unknown_warning() -> Value {
    json!({
        "kind": "job_session_unknown",
        "warning_kind": "job_session_unknown",
        "message": "job has no recorded session_id; stop authorized by project boundary only",
    })
}

fn job_recovering_stop_result(project: &str, job: &ShellJobInfo) -> ToolResult {
    ToolResult::err_with_output(
        "runner_unavailable_recovering: the runner must reconcile this job before it can be stopped"
            .to_string(),
        json!({
            "error_kind": "runner_unavailable_recovering",
            "failure_kind": "runner_unavailable_recovering",
            "project": project,
            "job_id": job.job_id,
            "status_before": "recovering",
            "status_after": "recovering",
            "recovery_state": job.recovery_state,
            "recovery_reason_code": job.recovery_reason_code,
            "recovery_reason": recovery_reason_text(
                job.recovery_state.as_deref(),
                job.recovery_reason_code.as_deref(),
            ),
            "already_finished": false,
            "already_stop_requested": false,
            "stop_request_accepted": false,
            "target_was_active_at_request": true,
            "terminal": false,
            "terminal_pending": false,
            "final_status": Value::Null,
            "stop_effect": "runner_unavailable",
            "command_started": false,
        }),
    )
    .with_recovery(RecoveryKind::Wait, None)
}

fn ownership_basis_for_stop(
    request_project: &str,
    job_id: &str,
    request_session_id: Option<&str>,
    job_session_id: Option<&str>,
) -> Result<(&'static str, Vec<Value>), ToolResult> {
    match job_session_id {
        Some(job_session_id) if Some(job_session_id) == request_session_id => {
            Ok(("project_and_session", Vec::new()))
        }
        Some(job_session_id) => Err(job_stop_forbidden_result(
            request_project,
            job_id,
            request_session_id,
            Some(job_session_id),
        )),
        None => Ok((
            "unknown_session_project_only",
            vec![job_session_unknown_warning()],
        )),
    }
}

fn stop_job_output(
    project: &str,
    job_id: &str,
    status_before: &str,
    status_after: &str,
    stopped: bool,
    already_finished: bool,
    ownership_basis: &str,
    warnings: Vec<Value>,
) -> Value {
    let already_stop_requested = is_stop_pending_job_status(status_before) && !already_finished;
    let terminal = is_terminal_job_status(status_after);
    let terminal_pending = is_stop_pending_job_status(status_after);
    let stop_request_accepted = !already_finished && !already_stop_requested && stopped;
    let stop_effect = if already_finished {
        "already_finished"
    } else if already_stop_requested {
        "already_stop_requested"
    } else if terminal {
        "stopped"
    } else {
        "requested"
    };
    let mut output = json!({
        "already_finished": already_finished,
        "already_stop_requested": already_stop_requested,
        "stop_request_accepted": stop_request_accepted,
        "target_was_active_at_request": is_lifecycle_active_status(status_before),
        "terminal": terminal,
        "terminal_pending": terminal_pending,
        "final_status": if terminal { json!(status_after) } else { Value::Null },
        "stop_effect": stop_effect,
        "job_id": job_id,
        "project": project,
        "status_before": status_before,
        "status_after": status_after,
        "command_started": false,
        "ownership_basis": ownership_basis,
    });
    if !warnings.is_empty() {
        output["warning_kind"] = warnings
            .first()
            .and_then(|warning| warning.get("warning_kind"))
            .cloned()
            .unwrap_or(Value::Null);
        output["warnings"] = Value::Array(warnings);
    }
    output
}

fn active_job_brief(summary: &Value) -> Value {
    json!({
        "job_id": summary.get("job_id").cloned().unwrap_or(Value::Null),
        "kind": summary.get("kind").cloned().unwrap_or_else(|| json!("shell")),
        "status": summary.get("status").cloned().unwrap_or(Value::Null),
        "project": summary.get("project").cloned().unwrap_or(Value::Null),
        "started_at": summary.get("started_at").cloned().unwrap_or(Value::Null),
        "created_at": summary.get("created_at").cloned().unwrap_or(Value::Null),
        "executor": summary.get("executor").cloned().unwrap_or(Value::Null),
    })
}

pub(crate) fn local_jobs_visible_to_auth(auth: Option<&AuthContext>) -> bool {
    !auth
        .map(|auth| auth.is_lightweight() || auth.is_oauth_shared_key_subject())
        .unwrap_or(false)
}

impl ToolRuntime {
    pub(crate) async fn run_job_for_auth(
        &self,
        project: String,
        command: String,
        session_id: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
        validation_steps: Vec<ShellJobValidationStep>,
        sandbox: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.run_job_for_auth_with_contract(
            project,
            command,
            session_id,
            timeout_secs,
            cwd,
            validation_steps,
            sandbox,
            auth,
            None,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_job_for_auth_with_contract(
        &self,
        project: String,
        command: String,
        session_id: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
        validation_steps: Vec<ShellJobValidationStep>,
        sandbox: Option<String>,
        auth: Option<&AuthContext>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
    ) -> ToolResult {
        self.run_job_for_auth_with_contract_with_ssh_resource(
            project,
            command,
            session_id,
            timeout_secs,
            cwd,
            validation_steps,
            sandbox,
            auth,
            purpose,
            shell,
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn run_job_for_auth_with_contract_with_ssh_resource(
        &self,
        project: String,
        command: String,
        session_id: Option<String>,
        timeout_secs: Option<i64>,
        cwd: Option<String>,
        validation_steps: Vec<ShellJobValidationStep>,
        sandbox: Option<String>,
        auth: Option<&AuthContext>,
        purpose: Option<ExecutionPurpose>,
        shell: Option<ExecutionShell>,
        ssh_resource: Option<&str>,
    ) -> ToolResult {
        if let Err(error) = validate_raw_shell_command_length(&command) {
            return ToolResult::err(command_rejected_message(
                error,
                "use run_script for larger shell program text or stdin/files/artifacts for large data.",
            ));
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(e) => return ToolResult::err(command_rejected_message(
                e.to_message(),
                "verify the project id with list_projects, then retry with a registered project.",
            )),
        };
        let project_id = resolved.resolved_id.clone();
        let proj = resolved.config;
        if ssh_resource.is_some() && !proj.is_agent() {
            return ToolResult::err(
                "ssh_resource_requires_agent_project: SSH resources require a project owned by a connected Runner"
                    .to_string(),
            );
        }
        let max_runtime = timeout_secs.unwrap_or(3600).clamp(1, 604800);
        let declared_purpose = purpose.unwrap_or_default();
        let command_summary = command_preview(&command);
        if proj.is_agent() {
            let client_id = match proj.agent_client_id() {
                Ok(id) => id.to_string(),
                Err(e) => {
                    return ToolResult::err(command_rejected_message(
                        e,
                        "refresh the agent project registry with list_projects, then retry.",
                    ))
                }
            };
            let remote = ssh_resource.is_some();
            if remote && !validation_steps.is_empty() {
                return ToolResult::err(
                    "ssh_resource_unsupported_for_request: SSH resources do not support structured validation jobs"
                        .to_string(),
                );
            }
            if remote && session_id.is_none() {
                return ToolResult::err(
                    "ssh_session_required: an SSH resource requires a Workflow Session id; command was not started"
                        .to_string(),
                );
            }
            let (effective_cwd, resolved_cwd) = if remote {
                let remote_cwd = cwd
                    .as_deref()
                    .map(str::trim)
                    .filter(|cwd| !cwd.is_empty())
                    .map(str::to_string);
                if remote_cwd
                    .as_deref()
                    .is_some_and(|cwd| cwd.len() > 4096 || cwd.chars().any(char::is_control))
                {
                    return ToolResult::err(command_rejected_message(
                        "ssh_remote_cwd_invalid: cwd must be a bounded remote path without control characters",
                        "choose a valid remote path, or omit cwd to use the SSH resource default.",
                    ));
                }
                let display = remote_cwd.clone().unwrap_or_else(|| ".".to_string());
                (remote_cwd, display)
            } else {
                let cwd = match resolve_agent_cwd(&proj, cwd.as_deref()) {
                    Ok(cwd) => cwd,
                    Err(error) => {
                        return ToolResult::err(command_rejected_message(
                            error,
                            "choose '.', an existing project-relative cwd, or a path inside the registered project root.",
                        ))
                    }
                };
                let display =
                    project_relative_agent_cwd(&proj, &cwd).unwrap_or_else(|_| ".".to_string());
                (Some(cwd), display)
            };
            let actual_shell = shell.map(ExecutionShell::as_str).unwrap_or(if remote {
                "remote"
            } else {
                "configured"
            });
            let dispatched_command =
                match shell {
                    Some(shell) => match explicit_shell_dispatch_command(&command, shell.as_str()) {
                        Ok(command) => command,
                        Err(error) => return ToolResult::err(command_rejected_message(
                            error,
                            "use run_script for large or quote-dense explicit-shell program text.",
                        )),
                    },
                    None => command.clone(),
                };
            match self
                .shell_clients
                .start_job_with_metadata_for_auth(
                    ShellJobOpRequest {
                        op: "start".to_string(),
                        client_id: Some(client_id),
                        cwd: effective_cwd,
                        command: Some(dispatched_command),
                        timeout_secs: Some(max_runtime as u64),
                        job_id: None,
                        since_stdout_line: None,
                        since_stderr_line: None,
                        tail_lines: None,
                        limit: None,
                        codex: None,
                    },
                    "tool_runtime".to_string(),
                    ShellJobStartMetadata {
                        project_id: Some(project_id.clone()),
                        session_id: session_id.clone(),
                        ssh_resource: ssh_resource.map(str::to_string),
                        project_cwd: Some(resolved_cwd.clone()),
                        purpose: Some(declared_purpose.as_str().to_string()),
                        shell: Some(actual_shell.to_string()),
                        validation_steps,
                        validation: None,
                        visibility: crate::shell_client::ShellJobVisibility::Public,
                        sandbox,
                        structured_execution: None,
                        stdin: None,
                        detached_idempotency_key: None,
                    },
                    auth,
                )
                .await
            {
                Ok(job) => ToolResult::ok(json!({
                    "job_id": job.job_id,
                    "kind": job.kind,
                    "status": job.status,
                    "project": project_id,
                    "execution_source": "run_job",
                    "purpose": declared_purpose.as_str(),
                    "command_summary": command_summary,
                    "cwd": resolved_cwd,
                    "shell": actual_shell,
                    "executor": "agent",
                    "ssh_resource": ssh_resource,
                    "execution_state": "started",
                    "created_at": job.created_at,
                    "observation_token": job.observation_token,
                    "last_update_seq": job.last_update_seq,
                    "stdout_tail": "",
                    "stderr_tail": "",
                    "stdout_lines": 0,
                    "stderr_lines": 0,
                    "stdout_truncated": false,
                    "stderr_truncated": false,
                })),
                Err(e) => ToolResult::err(command_rejected_message(
                    e,
                    "confirm the agent is connected and async jobs are allowed, then retry or use run_shell for short commands.",
                )),
            }
        } else {
            if !validation_steps.is_empty() {
                return ToolResult::err(
                    "structured validation jobs require an agent-backed project".to_string(),
                );
            }
            let root = proj.root();
            let cwd_path = match resolve_local_cwd(&proj, cwd.as_deref()) {
                Ok(path) => path,
                Err(error) => {
                    return ToolResult::err(command_rejected_message(
                        error,
                        "choose '.', an existing project-relative cwd, or a path inside the project root.",
                    ))
                }
            };
            let resolved_cwd =
                project_relative_cwd(&proj, &cwd_path).unwrap_or_else(|_| ".".to_string());
            // Preserve the existing local async-job command language (bash)
            // when omitted; explicit sh/bash selects the requested language.
            let actual_shell = shell.map(ExecutionShell::as_str).unwrap_or("bash");
            let job_id = uuid::Uuid::new_v4().to_string();
            let inspect_scratch = match sandbox.as_deref() {
                None => None,
                Some(crate::command_sandbox::INSPECT_SANDBOX_MODE) => {
                    match crate::command_sandbox::InspectScratch::create() {
                        Ok(scratch) => Some(scratch),
                        Err(error) => {
                            return ToolResult::err(format!("inspect sandbox unavailable: {error}"))
                        }
                    }
                }
                Some(other) => return ToolResult::err(format!("unknown sandbox mode '{other}'")),
            };
            let dir = inspect_scratch
                .as_ref()
                .map(|scratch| scratch.path().join("job"))
                .unwrap_or_else(|| root.join(format!(".codex/jobs/{}", job_id)));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                return ToolResult::err(format!("Failed to create job dir: {}", e));
            }
            let now = chrono::Utc::now().timestamp();
            let mut meta = json!({
                "job_id": job_id,
                "project": project_id.clone(),
                "command": command,
                "status": "running",
                "created_at": now,
                "started_at": now,
                "max_runtime_secs": max_runtime,
                "executor": "local",
                "path": proj.path.clone(),
                "kind": "shell",
                "purpose": declared_purpose.as_str(),
                "cwd": resolved_cwd,
                "shell": actual_shell,
            });
            if let Some(session_id) = session_id.as_ref() {
                meta["session_id"] = json!(session_id);
            }
            if let Err(e) = std::fs::write(
                dir.join("metadata.json"),
                serde_json::to_string_pretty(&meta).unwrap_or_default(),
            ) {
                return ToolResult::err(format!("Failed to write metadata: {}", e));
            }
            let cmd_content = format!("#!/usr/bin/env {actual_shell}\n{command}\n");
            if let Err(e) = std::fs::write(dir.join("command.sh"), &cmd_content) {
                return ToolResult::err(format!("Failed to write command.sh: {}", e));
            }
            if let Err(e) = std::fs::write(dir.join("status"), "running") {
                return ToolResult::err(format!("Failed to write initial status: {e}"));
            }
            if let Err(e) = std::fs::write(dir.join("stdout.log"), b"") {
                return ToolResult::err(format!("Failed to create stdout.log: {e}"));
            }
            if let Err(e) = std::fs::write(dir.join("stderr.log"), b"") {
                return ToolResult::err(format!("Failed to create stderr.log: {e}"));
            }
            let (record, initial_observation) =
                match LocalJobRecord::initialize(project_id.clone(), dir.clone()) {
                    Ok(value) => value,
                    Err(error) => return ToolResult::err(error),
                };
            let initial_observation_token = match initial_observation.token(&job_id) {
                Ok(token) => token,
                Err(error) => return ToolResult::err(error),
            };
            let terminal_snapshot = record.terminal_snapshot_handle();
            let dir_s = dir.to_string_lossy().to_string();
            let wrapper = format!(
                "{1} {0}/command.sh > {0}/stdout.log 2> {0}/stderr.log; code=$?; echo $code > {0}/exit_code; finished=$(date +%s); echo $finished > {0}/finished_at; if [ $code -eq 0 ]; then echo completed > {0}/status; else echo failed > {0}/status; fi",
                shell_escape_simple(&dir_s),
                actual_shell,
            );
            let mut job_command = std::process::Command::new("setsid");
            job_command
                .arg("sh")
                .arg("-c")
                .arg(wrapper)
                .current_dir(&cwd_path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if let Some(scratch) = inspect_scratch.as_ref() {
                if let Err(error) =
                    crate::command_sandbox::sandbox_command_inspect(&mut job_command, scratch)
                {
                    return ToolResult::err(format!("inspect sandbox unavailable: {error}"));
                }
            }
            match job_command.spawn() {
                Ok(child) => {
                    // `setsid` makes the child a session + process-group
                    // leader, so child.id() is both the leader pid and the
                    // process-group id. Record the pgid so timeout/stop can
                    // signal the whole subtree (`kill -<pgid>`).
                    let pgid = child.id() as i64;
                    if let Err(e) = std::fs::write(dir.join("pid"), child.id().to_string()) {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to write local job pid"
                        );
                    }
                    meta["process_group_id"] = json!(pgid);
                    if let Err(e) = std::fs::write(
                        dir.join("metadata.json"),
                        serde_json::to_string_pretty(&meta).unwrap_or_default(),
                    ) {
                        tracing::warn!(
                            job_id = %job_id,
                            error = %e,
                            "failed to update local job metadata with process group"
                        );
                    }
                    self.local_jobs.lock().await.insert(job_id.clone(), record);
                    if let Some(scratch) = inspect_scratch {
                        retain_inspect_job_until_terminal(dir, terminal_snapshot, scratch, child);
                    }
                    ToolResult::ok(json!({
                        "job_id": job_id,
                        "kind": "shell",
                        "status": "running",
                        "project": project_id,
                        "execution_source": "run_job",
                        "purpose": declared_purpose.as_str(),
                        "command_summary": command_summary,
                        "cwd": resolved_cwd,
                        "shell": actual_shell,
                        "executor": "local",
                        "execution_state": "started",
                        "observation_token": initial_observation_token,
                        "created_at": now,
                        "stdout_tail": "",
                        "stderr_tail": "",
                        "stdout_lines": 0,
                        "stderr_lines": 0,
                        "stdout_truncated": false,
                        "stderr_truncated": false,
                    }))
                }
                Err(e) => ToolResult::err(format!("Failed to spawn job: {}", e)),
            }
        }
    }

    #[cfg(test)]
    pub(crate) async fn job_status(&self, job_id: String) -> ToolResult {
        self.job_status_for_auth(job_id, false, None).await
    }

    pub(crate) async fn job_status_for_auth(
        &self,
        job_id: String,
        include_command_preview: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            if !record.is_public() || !local_jobs_visible_to_auth(auth) {
                return unknown_job_observation_result(&job_id);
            }
            return local_job_status(&job_id, &record, killer, include_command_preview);
        }
        // Fall through to agent-backed jobs. If the agent registry does not
        // know this job either, attempt local recovery from on-disk metadata
        // so jobs started before a server restart remain queryable.
        if self
            .shell_clients
            .get_job_for_auth(auth, &job_id)
            .await
            .is_err()
        {
            if let Some(record) = self.recover_local_job(&job_id).await {
                if !local_jobs_visible_to_auth(auth) {
                    return unknown_job_observation_result(&job_id);
                }
                return local_job_status(&job_id, &record, killer, include_command_preview);
            }
            return unknown_job_observation_result(&job_id);
        }
        match self.shell_clients.get_job_for_auth(auth, &job_id).await {
            Ok(job) => {
                let mut output = json!({
                    "job_id": job.job_id,
                    "project": job.project_id,
                    "session_id": job.session_id,
                    "ssh_resource": job.ssh_resource,
                    "status": job.status,
                    "exit_code": job.exit_code,
                    "started_at": job.started_at,
                    "ended_at": job.ended_at,
                    "duration_ms": job.duration_ms,
                    "elapsed_secs": job.elapsed_secs,
                    "client_id": job.client_id,
                    "error": job.error,
                    "command_execution_state": job.command_execution_state,
                    "structured_execution": job.structured_execution,
                    "recovery_state": job.recovery_state,
                    "recovered_after_server_restart": job.recovered_after_server_restart,
                    "reconciled_at": job.reconciled_at,
                    "recovery_reason_code": job.recovery_reason_code,
                    "observation_token": job.observation_token,
                    "last_update_seq": job.last_update_seq,
                    "stdout_retained_from_line": job.stdout_retained_from_line,
                    "stderr_retained_from_line": job.stderr_retained_from_line,
                    "stdout_log_truncated": job.stdout_log_truncated,
                    "stderr_log_truncated": job.stderr_log_truncated,
                    "command_preview_included": include_command_preview,
                });
                let status = output["status"].as_str().unwrap_or_default().to_string();
                add_job_lifecycle_fields(
                    &mut output,
                    &status,
                    job.recovery_state.as_deref(),
                    job.recovery_reason_code.as_deref(),
                );
                if include_command_preview {
                    add_command_preview_metadata(&mut output, job.command_preview.clone());
                }
                let validation_metadata = job.validation.as_ref();
                let tool = validation_metadata.map(|metadata| metadata.tool.as_str());
                let kind = validation_metadata.map(|metadata| metadata.kind.as_str());
                if tool.is_some() {
                    let logs = self
                        .shell_clients
                        .job_log_for_auth(auth, &job_id, None, None, Some(500), None, None)
                        .await;
                    let (stdout, stderr, truncated) = match logs {
                        Ok((logged_job, stdout, stderr, next_stdout_line, next_stderr_line, _)) => {
                            let stdout = stdout.unwrap_or_default();
                            let stderr = stderr.unwrap_or_default();
                            let truncated = agent_log_stream_incomplete(
                                logged_job.stdout_log_truncated,
                                logged_job.stdout_retained_from_line,
                                stdout.lines().count(),
                                next_stdout_line,
                            ) || agent_log_stream_incomplete(
                                logged_job.stderr_log_truncated,
                                logged_job.stderr_retained_from_line,
                                stderr.lines().count(),
                                next_stderr_line,
                            );
                            (stdout, stderr, truncated)
                        }
                        Err(_) => (String::new(), String::new(), true),
                    };
                    if let Some(mut validation) = validation_job_projection(
                        tool,
                        kind,
                        &status,
                        job.exit_code.map(i64::from),
                        &stdout,
                        &stderr,
                        truncated,
                    ) {
                        if let Some(target_id) = validation_metadata
                            .and_then(|metadata| metadata.validation_target_id.as_deref())
                        {
                            validation["validation_target_id"] = json!(target_id);
                        }
                        output["validation"] = validation;
                    }
                }
                ToolResult::ok(output)
            }
            Err(_) => unknown_job_observation_result(&job_id),
        }
    }

    #[cfg(test)]
    pub(crate) async fn job_log(
        &self,
        job_id: String,
        offset: Option<usize>,
        tail_lines: Option<usize>,
    ) -> ToolResult {
        self.job_log_for_auth(job_id, offset, tail_lines, None, None, None)
            .await
    }

    /// Validate the bounded-wait arguments before any execution. Rejects
    /// out-of-range `wait_secs` up front. Observation-token syntax and Job
    /// binding are validated before execution or waiting by the selected executor.
    fn validate_job_log_wait(wait_secs: Option<u64>) -> Result<(), String> {
        if let Some(secs) = wait_secs {
            if secs == 0 || secs > 60 {
                return Err(format!(
                    "invalid wait_secs: must be between 1 and 60, got {}",
                    secs
                ));
            }
        }
        Ok(())
    }

    pub(crate) async fn job_log_for_auth(
        &self,
        job_id: String,
        offset: Option<usize>,
        tail_lines: Option<usize>,
        auth: Option<&AuthContext>,
        after_observation_token: Option<String>,
        wait_secs: Option<u64>,
    ) -> ToolResult {
        if let Err(message) = Self::validate_job_log_wait(wait_secs) {
            return invalid_job_observation_result("invalid_wait_secs", message);
        }
        let tail_lines = if offset.is_none() && tail_lines.is_none() {
            Some(super::helpers::DEFAULT_JOB_LOG_TAIL_LINES)
        } else {
            tail_lines
        };
        let killer = self.job_killer.as_ref();
        if let Some(record) = self.local_jobs.lock().await.get(&job_id).cloned() {
            if !record.is_public() || !local_jobs_visible_to_auth(auth) {
                return unknown_job_observation_result(&job_id);
            }
            return local_job_log(
                &job_id,
                &record,
                killer,
                offset,
                tail_lines,
                after_observation_token,
                wait_secs,
            )
            .await;
        }
        if self
            .shell_clients
            .get_job_for_auth(auth, &job_id)
            .await
            .is_err()
        {
            if let Some(record) = self.recover_local_job(&job_id).await {
                if !local_jobs_visible_to_auth(auth) {
                    return unknown_job_observation_result(&job_id);
                }
                return local_job_log(
                    &job_id,
                    &record,
                    killer,
                    offset,
                    tail_lines,
                    after_observation_token,
                    wait_secs,
                )
                .await;
            }
            return unknown_job_observation_result(&job_id);
        }
        match self
            .shell_clients
            .job_log_for_auth(
                auth,
                &job_id,
                offset,
                None,
                tail_lines,
                after_observation_token.as_deref(),
                wait_secs,
            )
            .await
        {
            Ok((job, stdout, stderr, next_stdout_line, next_stderr_line, wait)) => {
                let stdout = stdout.unwrap_or_default();
                let stderr = stderr.unwrap_or_default();
                let command_summary = job.command_preview.clone();
                let purpose = job.purpose.clone().unwrap_or_else(|| "other".to_string());
                let detected_summary = detected_job_summary(
                    Some(&command_summary),
                    Some(&purpose),
                    &job.status,
                    job.exit_code.map(i64::from),
                    &wait.analysis_stdout,
                    &wait.analysis_stderr,
                );
                let validation_tool = job
                    .validation
                    .as_ref()
                    .map(|metadata| metadata.tool.as_str());
                let validation_kind = job
                    .validation
                    .as_ref()
                    .map(|metadata| metadata.kind.as_str());
                let mut validation = validation_job_projection(
                    validation_tool,
                    validation_kind,
                    &job.status,
                    job.exit_code.map(i64::from),
                    &wait.analysis_stdout,
                    &wait.analysis_stderr,
                    wait.analysis_truncated,
                );
                if let (Some(validation), Some(target_id)) = (
                    validation.as_mut(),
                    job.validation
                        .as_ref()
                        .and_then(|metadata| metadata.validation_target_id.as_deref()),
                ) {
                    validation["validation_target_id"] = json!(target_id);
                }
                ToolResult::ok(json!({
                    "job_id": job.job_id,
                    "status": job.status,
                    "exit_code": job.exit_code,
                    "command_execution_state": job.command_execution_state,
                    "structured_execution": job.structured_execution,
                    "stdout_tail": stdout,
                    "stderr_tail": stderr,
                    "stdout_lines": next_stdout_line.saturating_sub(1),
                    "stderr_lines": next_stderr_line.saturating_sub(1),
                    "stdout_returned_lines": wait.stdout_returned_lines,
                    "stderr_returned_lines": wait.stderr_returned_lines,
                    "stdout_truncated": wait.stdout_truncated,
                    "stderr_truncated": wait.stderr_truncated,
                    "stdout_retained_from_line": job.stdout_retained_from_line,
                    "stderr_retained_from_line": job.stderr_retained_from_line,
                    "earlier_stdout_unavailable": job
                        .stdout_retained_from_line
                        .is_some_and(|line| line > 1)
                        || job.stdout_log_truncated,
                    "earlier_stderr_unavailable": job
                        .stderr_retained_from_line
                        .is_some_and(|line| line > 1)
                        || job.stderr_log_truncated,
                    "recovery_state": job.recovery_state,
                    "recovery_reason_code": job.recovery_reason_code,
                    "recovery_reason": recovery_reason_text(
                        job.recovery_state.as_deref(),
                        job.recovery_reason_code.as_deref(),
                    ),
                    "observation_token": job.observation_token,
                    "log_delta_status": wait.log_delta_status.as_str(),
                    "stdout_delta_reset": wait.stdout_delta_reset,
                    "stderr_delta_reset": wait.stderr_delta_reset,
                    "last_update_seq": job.last_update_seq,
                    "cursor": {
                        "stdout": next_stdout_line,
                        "stderr": next_stderr_line,
                    },
                    "wait_outcome": wait.wait_outcome.as_str(),
                    "waited_ms": wait.waited_ms,
                    "changed": wait.changed,
                    "terminal": wait.terminal,
                    "executor": "agent",
                    "session_id": job.session_id,
                    "ssh_resource": job.ssh_resource,
                    "cwd": job.project_cwd,
                    "shell": job.shell,
                    "purpose": purpose,
                    "command_summary": command_summary,
                    "detected_summary": detected_summary,
                    "validation": validation,
                }))
            }
            Err(error) => agent_job_log_error_result(&job_id, error),
        }
    }

    #[cfg(test)]
    pub(crate) async fn list_jobs_for_auth(
        &self,
        limit: Option<usize>,
        status: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.list_jobs_for_auth_with_filters(limit, status, None, None, auth)
            .await
    }

    pub(crate) async fn list_jobs_for_auth_with_filters(
        &self,
        limit: Option<usize>,
        status: Option<String>,
        project: Option<String>,
        session_id: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let max = limit.unwrap_or(20).clamp(1, 100);
        let status_filter = status
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let project_filter = match project {
            Some(value) => {
                let value = value.trim();
                if value.is_empty() || value.chars().count() > 512 {
                    return invalid_job_observation_result(
                        "invalid_project_filter",
                        "invalid_project_filter: project must contain 1..=512 characters"
                            .to_string(),
                    );
                }
                Some(value.to_string())
            }
            None => None,
        };
        let session_filter = match session_id {
            Some(value) => {
                let value = value.trim();
                if value.is_empty() || value.chars().count() > 128 {
                    return invalid_job_observation_result(
                        "invalid_session_filter",
                        "invalid_session_filter: session_id must contain 1..=128 characters"
                            .to_string(),
                    );
                }
                Some(value.to_string())
            }
            None => None,
        };

        // Authorization/visibility is applied by the registry first. Focused
        // filters only reduce that already-visible set, and limit is applied
        // after every filter so exact project/session targets cannot be hidden
        // behind unrelated recent Jobs.
        let agent_jobs = self.shell_clients.list_all_jobs_for_auth(auth).await;
        let mut summaries: Vec<Value> = agent_jobs
            .iter()
            .filter(|job| {
                status_filter
                    .as_ref()
                    .map(|status| status == &job.status)
                    .unwrap_or(true)
                    && project_filter
                        .as_deref()
                        .map(|project| job.project_id.as_deref() == Some(project))
                        .unwrap_or(true)
                    && session_filter
                        .as_deref()
                        .map(|session_id| job.session_id.as_deref() == Some(session_id))
                        .unwrap_or(true)
            })
            .map(agent_job_summary_value)
            .collect();

        let local_records: Vec<(String, LocalJobRecord)> = if local_jobs_visible_to_auth(auth) {
            let local_jobs_map = self.local_jobs.lock().await;
            local_jobs_map
                .iter()
                .filter(|(_, record)| record.is_public())
                .filter(|(_, record)| {
                    project_filter
                        .as_deref()
                        .map(|project| record.project == project)
                        .unwrap_or(true)
                })
                .map(|(job_id, record)| (job_id.clone(), record.clone()))
                .collect()
        } else {
            Vec::new()
        };
        for (job_id, record) in &local_records {
            if session_filter.as_deref().is_some_and(|session_id| {
                local_job_session_id(record).as_deref() != Some(session_id)
            }) {
                continue;
            }
            if let Some(summary) = local_job_summary_value(job_id, record, &status_filter) {
                summaries.push(summary);
            }
        }
        summaries.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["created_at"].as_i64().unwrap_or(0))
                .then_with(|| {
                    a["job_id"]
                        .as_str()
                        .unwrap_or_default()
                        .cmp(b["job_id"].as_str().unwrap_or_default())
                })
        });
        let matched_count = summaries.len();
        let truncated = matched_count > max;
        summaries.truncate(max);
        ToolResult::ok(json!({
            "jobs": summaries,
            "count": summaries.len(),
            "matched_count": matched_count,
            "truncated": truncated,
        }))
    }

    pub(crate) async fn validation_job_candidates_for_sessions(
        &self,
        project: &str,
        session_ids: &[String],
        auth: Option<&AuthContext>,
    ) -> std::collections::HashMap<String, Vec<Value>> {
        let requested = session_ids
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let mut grouped = std::collections::HashMap::<String, Vec<Value>>::new();
        if requested.is_empty() {
            return grouped;
        }
        for job in self.shell_clients.list_all_jobs_for_auth(auth).await.iter() {
            let Some(session_id) = job.session_id.as_deref() else {
                continue;
            };
            if job.project_id.as_deref() == Some(project)
                && requested.contains(session_id)
                && job.validation.is_some()
            {
                grouped
                    .entry(session_id.to_string())
                    .or_default()
                    .push(agent_job_summary_value(job));
            }
        }
        if local_jobs_visible_to_auth(auth) {
            let local_jobs_map = self.local_jobs.lock().await;
            for (job_id, record) in local_jobs_map
                .iter()
                .filter(|(_, record)| record.is_public() && record.project == project)
            {
                let Some(session_id) = local_job_session_id(record) else {
                    continue;
                };
                if !requested.contains(session_id.as_str()) {
                    continue;
                }
                if let Some(summary) = local_job_summary_value(job_id, record, &None) {
                    if summary.get("validation").is_some_and(Value::is_object) {
                        grouped.entry(session_id).or_default().push(summary);
                    }
                }
            }
        }
        for summaries in grouped.values_mut() {
            summaries.sort_by(|a, b| {
                b["created_at"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&a["created_at"].as_i64().unwrap_or(0))
                    .then_with(|| {
                        a["job_id"]
                            .as_str()
                            .unwrap_or_default()
                            .cmp(b["job_id"].as_str().unwrap_or_default())
                    })
            });
        }
        grouped
    }

    /// `job_tail`: bounded stdout/stderr tails for a job. Reuses the bounded
    /// `job_log` path with a tail-focused default so the console never reads
    /// full logs by default.
    #[cfg(test)]
    pub(crate) async fn job_tail(&self, job_id: String, tail_lines: Option<usize>) -> ToolResult {
        self.job_tail_for_auth(job_id, tail_lines, None, None, None)
            .await
    }

    pub(crate) async fn job_tail_for_auth(
        &self,
        job_id: String,
        tail_lines: Option<usize>,
        auth: Option<&AuthContext>,
        after_observation_token: Option<String>,
        wait_secs: Option<u64>,
    ) -> ToolResult {
        let tail = tail_lines.unwrap_or(200).clamp(1, 500);
        self.job_log_for_auth(
            job_id,
            None,
            Some(tail),
            auth,
            after_observation_token,
            wait_secs,
        )
        .await
    }

    /// Model-facing `stop_job`: requires confirm=true, verifies project/session
    /// ownership, and never exposes stdout/stderr.
    pub(crate) async fn stop_job_model_facing(
        &self,
        project: String,
        job_id: String,
        session_id: Option<String>,
        confirm: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if !confirm {
            return confirmation_required_result(&project, &job_id);
        }
        if !is_safe_job_id(&job_id) {
            return job_not_found_result(&project, &job_id);
        }

        let cached = {
            let jobs = self.local_jobs.lock().await;
            jobs.get(&job_id).cloned()
        };
        if let Some(record) = match cached {
            Some(record) => Some(record),
            None => self.recover_local_job(&job_id).await,
        } {
            if !record.is_public() || !local_jobs_visible_to_auth(auth) {
                return job_not_found_result(&project, &job_id);
            }
            let request_project = self
                .resolve_project_input_for_auth(&project, auth)
                .await
                .map(|resolved| resolved.resolved_id)
                .unwrap_or_else(|_| project.trim().to_string());
            if record.project != request_project {
                return job_project_mismatch_result(&request_project, &record.project, &job_id);
            }
            let job_session_id = local_job_session_id(&record);
            let (ownership_basis, warnings) = match ownership_basis_for_stop(
                &request_project,
                &job_id,
                session_id.as_deref(),
                job_session_id.as_deref(),
            ) {
                Ok(value) => value,
                Err(result) => return result,
            };
            let status_before = local_job_status_string(&record);
            if is_stop_pending_job_status(&status_before) {
                return ToolResult::ok(stop_job_output(
                    &request_project,
                    &job_id,
                    &status_before,
                    &status_before,
                    true,
                    false,
                    ownership_basis,
                    warnings,
                ));
            }
            if !ACTIVE_LOCAL_STATUSES.contains(&status_before.as_str()) {
                return ToolResult::ok(stop_job_output(
                    &request_project,
                    &job_id,
                    &status_before,
                    &status_before,
                    false,
                    true,
                    ownership_basis,
                    warnings,
                ));
            }
            let stop_result = stop_local_job(&job_id, &record, self.job_killer.as_ref());
            if !stop_result.success {
                return stop_result;
            }
            let status_after = stop_result
                .output
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("stopped")
                .to_string();
            return ToolResult::ok(stop_job_output(
                &request_project,
                &job_id,
                &status_before,
                &status_after,
                true,
                false,
                ownership_basis,
                warnings,
            ));
        }

        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let request_project = resolved.resolved_id;
        let job = match self.shell_clients.get_job_for_auth(auth, &job_id).await {
            Ok(job) => job,
            Err(_) => return job_not_found_result(&request_project, &job_id),
        };
        let Some(job_project) = job
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
        else {
            return job_stop_forbidden_result(
                &request_project,
                &job_id,
                session_id.as_deref(),
                job.session_id.as_deref(),
            );
        };
        if job_project != request_project {
            return job_project_mismatch_result(&request_project, job_project, &job_id);
        }
        let (ownership_basis, warnings) = match ownership_basis_for_stop(
            &request_project,
            &job_id,
            session_id.as_deref(),
            job.session_id.as_deref(),
        ) {
            Ok(value) => value,
            Err(result) => return result,
        };
        let status_before = job.status.clone();
        if status_before == "recovering" {
            return job_recovering_stop_result(&request_project, &job);
        }
        if is_stop_pending_job_status(&status_before) {
            return ToolResult::ok(stop_job_output(
                &request_project,
                &job_id,
                &status_before,
                &status_before,
                true,
                false,
                ownership_basis,
                warnings,
            ));
        }
        if !ACTIVE_JOB_STATUSES.contains(&status_before.as_str()) {
            return ToolResult::ok(stop_job_output(
                &request_project,
                &job_id,
                &status_before,
                &status_before,
                false,
                true,
                ownership_basis,
                warnings,
            ));
        }
        let stopped = match self
            .shell_clients
            .stop_job_for_auth(auth, &job_id, "tool_runtime".to_string())
            .await
        {
            Ok(job) => job,
            Err(error) if error.contains("runner_unavailable_recovering") => {
                let recovering = self
                    .shell_clients
                    .get_job_for_auth(auth, &job_id)
                    .await
                    .unwrap_or(job);
                return job_recovering_stop_result(&request_project, &recovering);
            }
            Err(_) => return job_not_found_result(&request_project, &job_id),
        };
        ToolResult::ok(stop_job_output(
            &request_project,
            &job_id,
            &status_before,
            &stopped.status,
            true,
            false,
            ownership_basis,
            warnings,
        ))
    }

    /// Bounded active job summary for finish/handoff. Never returns stdout,
    /// stderr, tails, command text, or command previews.
    pub(crate) async fn active_jobs_summary(
        &self,
        project: Option<&str>,
        auth: Option<&AuthContext>,
        limit: usize,
    ) -> Value {
        let max = limit.clamp(1, 20);
        let mut active = Vec::new();
        for job in self.shell_clients.list_jobs_for_auth(auth, Some(100)).await {
            if !ACTIVE_JOB_STATUSES.contains(&job.status.as_str()) {
                continue;
            }
            if let Some(project) = project {
                if job.project_id.as_deref() != Some(project) {
                    continue;
                }
            }
            active.push(agent_job_summary_value(&job));
        }

        if local_jobs_visible_to_auth(auth) {
            let local_records: Vec<(String, LocalJobRecord)> = {
                let local_jobs_map = self.local_jobs.lock().await;
                local_jobs_map
                    .iter()
                    .map(|(job_id, record)| (job_id.clone(), record.clone()))
                    .collect()
            };
            for (job_id, record) in local_records {
                if let Some(project) = project {
                    if record.project != project {
                        continue;
                    }
                }
                let status = local_job_status_string(&record);
                if !ACTIVE_JOB_STATUSES.contains(&status.as_str()) {
                    continue;
                }
                if let Some(summary) = local_job_summary_value(&job_id, &record, &None) {
                    active.push(summary);
                }
            }
        }

        active.sort_by(|a, b| {
            b["created_at"]
                .as_i64()
                .unwrap_or(0)
                .cmp(&a["created_at"].as_i64().unwrap_or(0))
        });
        let running_count = active
            .iter()
            .filter(|summary| {
                summary
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(is_blocking_active_job_status)
            })
            .count();
        // `recovering` jobs are a subset of running/blocking-active jobs that the
        // runner must reconcile before their output can be trusted. Counted over
        // the full active vector (not the truncated `recent` list) so the count is
        // reliable regardless of how many recent jobs are surfaced.
        let recovering_count = active
            .iter()
            .filter(|summary| summary.get("status").and_then(Value::as_str) == Some("recovering"))
            .count();
        let stop_requested_count = active
            .iter()
            .filter(|summary| {
                summary
                    .get("status")
                    .and_then(Value::as_str)
                    .is_some_and(is_stop_pending_job_status)
            })
            .count();
        let terminal_pending_count = stop_requested_count;
        let blocking_active_count = running_count;
        let nonblocking_active_count = terminal_pending_count;
        let active_count = blocking_active_count + nonblocking_active_count;
        let recent: Vec<Value> = active.iter().take(max).map(active_job_brief).collect();
        let mut warnings = Vec::new();
        if blocking_active_count > 0 {
            warnings.push(json!({
                "kind": "active_jobs_present",
                "blocking": true,
                "active_count": active_count,
                "blocking_active_count": blocking_active_count,
                "message": format!(
                    "{} blocking active job{} still running",
                    blocking_active_count,
                    if blocking_active_count == 1 { "" } else { "s" }
                ),
            }));
        }
        if terminal_pending_count > 0 {
            warnings.push(json!({
                "kind": "jobs_terminal_pending",
                "blocking": false,
                "stop_requested_count": stop_requested_count,
                "terminal_pending_count": terminal_pending_count,
                "message": format!(
                    "{} job{} stop_requested and waiting for terminal status",
                    terminal_pending_count,
                    if terminal_pending_count == 1 { " is" } else { "s are" }
                ),
            }));
        }
        json!({
            "active_count": active_count,
            "running_count": running_count,
            "recovering_count": recovering_count,
            "stop_requested_count": stop_requested_count,
            "terminal_pending_count": terminal_pending_count,
            "blocking_active_count": blocking_active_count,
            "nonblocking_active_count": nonblocking_active_count,
            "recent": recent,
            "recent_limit": max,
            "truncated": active_count > max,
            "warnings": warnings,
        })
    }

    /// Stop a local job by terminating its process group and marking it
    /// `stopped`.
    ///
    /// This is an internal lifecycle method intended as the implementation
    /// backing a future explicit stop API; it is deliberately **not** exposed
    /// as a GPT Actions / MCP write tool, to avoid surfacing an arbitrary kill
    /// surface to remote callers. Only jobs we created and recorded (in-memory
    /// or recoverable on disk) can be stopped, and the pid/pgid come
    /// exclusively from the job's own on-disk files — never from caller input.
    pub async fn stop_job(&self, job_id: String) -> ToolResult {
        if !is_safe_job_id(&job_id) {
            return ToolResult::err("invalid job id");
        }
        let cached = {
            let jobs = self.local_jobs.lock().await;
            jobs.get(&job_id).cloned()
        };
        let record = match cached {
            Some(r) => r,
            None => match self.recover_local_job(&job_id).await {
                Some(r) => r,
                None => return ToolResult::err(format!("unknown job: {}", job_id)),
            },
        };
        stop_local_job(&job_id, &record, self.job_killer.as_ref())
    }

    /// On-disk local job recovery used to scan server-configured project roots.
    /// The runtime no longer has a server-side project map, so only in-memory
    /// local jobs from the current process can be queried or stopped.
    pub(crate) async fn recover_local_job(&self, job_id: &str) -> Option<LocalJobRecord> {
        if !is_safe_job_id(job_id) {
            return None;
        }
        None
    }
}

#[cfg(test)]
mod recovery_projection_tests {
    use super::{
        confirmation_required_result, job_not_found_result, job_project_mismatch_result,
        job_recovering_stop_result, job_stop_forbidden_result, recovery_reason_text,
        validation_job_projection,
    };
    use crate::shell_protocol::ShellJobInfo;
    use serde_json::json;

    #[test]
    fn recovery_reason_text_recovering_explains_wait() {
        let text = recovery_reason_text(Some("recovering"), Some("runner_transport_disconnected"));
        assert_eq!(
            text.as_deref(),
            Some("server is waiting for the same runner instance to reconnect")
        );
        // recovering state is described regardless of the specific reason code.
        let text2 = recovery_reason_text(Some("recovering"), None);
        assert_eq!(
            text2.as_deref(),
            Some("server is waiting for the same runner instance to reconnect")
        );
    }

    #[test]
    fn recovery_reason_text_lost_after_reconcile_codes_are_distinct() {
        let deadline = recovery_reason_text(
            Some("lost_after_reconcile"),
            Some("runner_recovery_deadline_exceeded"),
        );
        let missing = recovery_reason_text(
            Some("lost_after_reconcile"),
            Some("runner_inventory_missing"),
        );
        let replaced = recovery_reason_text(
            Some("lost_after_reconcile"),
            Some("runner_instance_replaced"),
        );
        assert_eq!(
            deadline.as_deref(),
            Some("lost: runner did not reconnect before the recovery deadline")
        );
        assert_eq!(
            missing.as_deref(),
            Some("lost: runner reconnect did not report this job in its inventory")
        );
        assert_eq!(
            replaced.as_deref(),
            Some("lost: runner instance was replaced by a newer process")
        );
        // The three reasons must produce three distinct human strings so the
        // Console can tell them apart.
        let texts = [deadline, missing, replaced]
            .into_iter()
            .filter_map(|opt| opt.as_deref().map(str::to_string))
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(texts.len(), 3, "distinct recoverable loss reasons");
    }

    #[test]
    fn recovery_reason_text_legacy_runner_disconnect() {
        let text = recovery_reason_text(None, Some("legacy_runner_disconnected"));
        assert_eq!(
            text.as_deref(),
            Some("lost: legacy runner disconnected without reconciliation support")
        );
    }

    #[test]
    fn recovery_reason_text_unknown_code_falls_back_safely() {
        let text = recovery_reason_text(Some("lost_after_reconcile"), Some("some_new_code"));
        assert!(
            text.as_deref().unwrap().contains("some_new_code"),
            "unknown code is echoed for debuggability"
        );
        assert!(
            !text.as_deref().unwrap().contains("token"),
            "no sensitive leak"
        );
        let other = recovery_reason_text(None, Some("unknown_reason"));
        assert!(other.as_deref().unwrap().contains("unknown_reason"));
    }

    #[test]
    fn recovery_reason_text_none_when_no_state_or_code() {
        assert_eq!(recovery_reason_text(None, None), None);
    }

    #[test]
    fn job_control_failures_expose_bounded_recovery_without_changing_lifecycle_fields() {
        let confirmation = confirmation_required_result("agent:special:demo", "job-1");
        assert_eq!(confirmation.output["error_kind"], "confirmation_required");
        assert_eq!(confirmation.output["failure_kind"], "confirmation_required");
        assert_eq!(confirmation.output["command_started"], false);
        assert_eq!(confirmation.output["recovery_kind"], "user_action");

        let missing = job_not_found_result("agent:special:demo", "job-missing");
        assert_eq!(missing.output["failure_kind"], "job_not_found");
        assert_eq!(missing.output["recovery_kind"], "reobserve");
        assert_eq!(missing.output["recovery_tool"], "list_jobs");

        let mismatch =
            job_project_mismatch_result("agent:special:demo", "agent:special:other", "job-2");
        assert_eq!(mismatch.output["failure_kind"], "job_project_mismatch");
        assert_eq!(mismatch.output["recovery_kind"], "fix_input");

        let forbidden = job_stop_forbidden_result(
            "agent:special:demo",
            "job-3",
            Some("wc_sess_request"),
            Some("wc_sess_owner"),
        );
        assert_eq!(forbidden.output["failure_kind"], "job_stop_forbidden");
        assert_eq!(forbidden.output["recovery_kind"], "fix_input");

        let recovering: ShellJobInfo = serde_json::from_value(json!({
            "job_id": "job-recovering",
            "client_id": "special",
            "command_preview": "",
            "status": "recovering",
            "created_at": 1,
            "recovery_state": "recovering",
            "recovery_reason_code": "runner_transport_disconnected"
        }))
        .unwrap();
        let wait = job_recovering_stop_result("agent:special:demo", &recovering);
        assert_eq!(wait.output["error_kind"], "runner_unavailable_recovering");
        assert_eq!(wait.output["failure_kind"], "runner_unavailable_recovering");
        assert_eq!(wait.output["command_started"], false);
        assert_eq!(wait.output["stop_effect"], "runner_unavailable");
        assert_eq!(wait.output["recovery_kind"], "wait");
        assert!(wait.output.get("recovery_tool").is_none());
    }

    #[test]
    fn validation_projection_reports_terminal_cargo_test_counts_and_diagnostics() {
        let success = validation_job_projection(
            Some("cargo_test"),
            Some("test"),
            "completed",
            Some(0),
            "running 3 tests\n\ntest result: ok. 3 passed; 0 failed; 0 ignored\n",
            "",
            false,
        )
        .unwrap();
        assert_eq!(success["passed"], true);
        assert_eq!(success["tests_detected"], true);
        assert_eq!(success["tests_run_count"], 3);
        assert_eq!(success["tests_passed"], 3);
        assert_eq!(success["tests_failed"], 0);
        assert_eq!(success["diagnostics"]["truncated"], false);

        let compile_error = validation_job_projection(
            Some("cargo_test"),
            Some("test"),
            "failed",
            Some(101),
            "",
            "error[E0308]: mismatched types\n --> src/lib.rs:1:1\n",
            false,
        )
        .unwrap();
        assert_eq!(compile_error["passed"], false);
        assert_eq!(compile_error["tests_detected"], false);
        assert!(compile_error["tests_run_count"].is_null());
        assert_eq!(compile_error["diagnostics"]["available"], true);
    }

    #[test]
    fn validation_projection_reports_check_counts_and_never_fakes_truncated_counts() {
        let complete = validation_job_projection(
            Some("cargo_check"),
            Some("check"),
            "failed",
            Some(101),
            "",
            "warning: unused import\nerror[E0308]: mismatched types\n",
            false,
        )
        .unwrap();
        assert_eq!(complete["warnings_count"], 1);
        assert_eq!(complete["errors_count"], 1);

        let truncated = validation_job_projection(
            Some("cargo_check"),
            Some("check"),
            "failed",
            Some(101),
            "",
            "error[E0308]: mismatched types\n",
            true,
        )
        .unwrap();
        assert!(truncated["warnings_count"].is_null());
        assert!(truncated["errors_count"].is_null());
        assert_eq!(truncated["diagnostics"]["truncated"], true);
    }

    #[test]
    fn validation_projection_keeps_lifecycle_terminals_distinct_from_validation_failure() {
        let fmt = validation_job_projection(
            Some("cargo_fmt"),
            Some("format"),
            "failed",
            Some(1),
            "Diff in src/lib.rs\n",
            "",
            false,
        )
        .unwrap();
        assert_eq!(fmt["passed"], false);
        assert!(fmt.get("diagnostics").is_none());

        for (status, state) in [
            ("timeout", "timed_out"),
            ("stopped", "cancelled"),
            ("lost", "lost"),
        ] {
            let value = validation_job_projection(
                Some("cargo_test"),
                Some("test"),
                status,
                None,
                "running 1 test\n",
                "",
                false,
            )
            .unwrap();
            assert_eq!(value["state"], state);
            assert!(value["passed"].is_null());
            assert!(value["tests_run_count"].is_null());
            assert!(value["tests_passed"].is_null());
            assert!(value["tests_failed"].is_null());
        }
    }

    #[test]
    fn agent_job_summary_includes_recovery_reason() {
        use crate::shell_protocol::ShellJobInfo;
        let job = ShellJobInfo {
            job_id: "job-1".to_string(),
            request_id: Some("req-1".to_string()),
            client_id: "oe".to_string(),
            kind: "shell".to_string(),
            project_id: None,
            session_id: None,
            ssh_resource: None,
            cwd: None,
            project_cwd: None,
            purpose: None,
            shell: None,
            command_preview: String::new(),
            status: "lost".to_string(),
            created_at: 1,
            started_at: Some(2),
            ended_at: Some(3),
            exit_code: None,
            duration_ms: None,
            elapsed_secs: Some(1),
            error: Some("runner did not reconcile".to_string()),
            command_execution_state: None,
            structured_execution: None,
            codex: None,
            result: None,
            validation_progress: None,
            validation: None,
            recovery_state: Some("lost_after_reconcile".to_string()),
            recovered_after_server_restart: true,
            reconciled_at: Some(3),
            recovery_reason_code: Some("runner_recovery_deadline_exceeded".to_string()),
            observation_token: Some("wjob1:a:job-1:0123456789abcdef:4".to_string()),
            last_update_seq: Some(4),
            stdout_retained_from_line: Some(1),
            stderr_retained_from_line: Some(1),
            stdout_log_truncated: false,
            stderr_log_truncated: false,
        };
        let value = super::agent_job_summary_value(&job);
        assert_eq!(
            value["recovery_reason_code"],
            json!("runner_recovery_deadline_exceeded")
        );
        assert_eq!(
            value["recovery_reason"],
            json!("lost: runner did not reconnect before the recovery deadline")
        );
        // Never expose the raw error string or command payload via the summary.
        assert!(
            value.get("error").is_none(),
            "summary must not surface raw error"
        );
        assert!(
            value.get("command").is_none(),
            "summary must not surface command"
        );
    }
}
