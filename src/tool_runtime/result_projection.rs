//! Request-scoped model result projection, after canonical evidence capture.
//! No dispatch, authorization, Session selection or retry belongs here.
use super::{ResolvedProject, ToolCall, ToolResult};
use serde_json::Value;

fn is_structured_validation_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "project_validate" | "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test"
    )
}

fn sparsify_terminal_structured_validation_success(tool_name: &str, result: &mut ToolResult) {
    if !is_structured_validation_tool(tool_name) || !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    let terminal_success = output.get("execution_state").and_then(Value::as_str)
        == Some("completed")
        && output.get("command_started").and_then(Value::as_bool) == Some(true)
        && output.get("command_completed").and_then(Value::as_bool) == Some(true)
        && output.get("passed").and_then(Value::as_bool) == Some(true)
        && output.get("promoted_to_job").and_then(Value::as_bool) == Some(false)
        && output.get("terminal").and_then(Value::as_bool) == Some(true)
        && output.get("job_id").map(Value::is_null).unwrap_or(true)
        && output.get("job_status").map(Value::is_null).unwrap_or(true)
        && output
            .get("observation_token")
            .map(Value::is_null)
            .unwrap_or(true);
    if !terminal_success {
        return;
    }

    for key in [
        "project",
        "command_summary",
        "cwd",
        "shell",
        "executor",
        "execution_source",
        "purpose",
        "execution_state",
        "exit_code",
        "duration_ms",
        "passed",
        "command_started",
        "command_completed",
        "promoted_to_job",
        "terminal",
        "job_id",
        "job_status",
        "observation_token",
        "effective_timeout_secs",
        "sync_wait_secs",
    ] {
        output.remove(key);
    }
    output.remove("async_handoff_available");
    if output.get("failure_kind").is_some_and(Value::is_null) {
        output.remove("failure_kind");
    }
    for key in ["stdout_tail", "stderr_tail"] {
        if output.get(key).and_then(Value::as_str) == Some("") {
            output.remove(key);
        }
    }
    for key in ["stdout_lines", "stderr_lines"] {
        if output.get(key).and_then(Value::as_u64) == Some(0) {
            output.remove(key);
        }
    }
    for key in ["stdout_truncated", "stderr_truncated"] {
        if output.get(key).and_then(Value::as_bool) == Some(false) {
            output.remove(key);
        }
    }
}

fn sparsify_structured_validation_runtime_metadata(tool_name: &str, result: &mut ToolResult) {
    if !is_structured_validation_tool(tool_name) {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    for key in ["execution_source", "purpose", "executor", "shell"] {
        output.remove(key);
    }
    if result.success
        && matches!(
            output.get("execution_state").and_then(Value::as_str),
            Some("queued" | "running" | "started" | "pending")
        )
    {
        for key in [
            "project",
            "cwd",
            "terminal",
            "command_started",
            "command_completed",
            "sync_wait_secs",
        ] {
            output.remove(key);
        }
        for key in ["stdout_tail", "stderr_tail"] {
            if output.get(key).and_then(Value::as_str) == Some("") {
                output.remove(key);
            }
        }
        for key in ["stdout_lines", "stderr_lines"] {
            if output.get(key).and_then(Value::as_u64) == Some(0) {
                output.remove(key);
            }
        }
        for key in ["stdout_truncated", "stderr_truncated"] {
            if output.get(key).and_then(Value::as_bool) == Some(false) {
                output.remove(key);
            }
        }
    }
}

/// Remove facts that are fully implied by a successful synchronous terminal
/// structured execution, but only after the complete ToolResult has already
/// been recorded into the Session ledger. Failure/uncertain/Job projections
/// remain explicit because they participate in retry and reconciliation safety.
fn sparsify_terminal_structured_execution_success(tool_name: &str, result: &mut ToolResult) {
    if is_structured_validation_tool(tool_name) {
        sparsify_terminal_structured_validation_success(tool_name, result);
        return;
    }
    if !matches!(
        tool_name,
        "project_build" | "run_process" | "run_script" | "run_skill_resource"
    ) || !result.success
    {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    let terminal_success = output.get("execution_state").and_then(Value::as_str)
        == Some("completed")
        && output.get("command_started").and_then(Value::as_bool) == Some(true)
        && output.get("command_completed").and_then(Value::as_bool) == Some(true)
        && output.get("command_ok").and_then(Value::as_bool) == Some(true)
        && output.get("promoted_to_job").and_then(Value::as_bool) == Some(false)
        && output.get("terminal").and_then(Value::as_bool) == Some(true)
        && output.get("job_id").map(Value::is_null).unwrap_or(true)
        && output.get("job_status").map(Value::is_null).unwrap_or(true)
        && output
            .get("observation_token")
            .map(Value::is_null)
            .unwrap_or(true);
    if !terminal_success {
        return;
    }

    for key in [
        "promoted_to_job",
        "terminal",
        "job_id",
        "job_status",
        "observation_token",
        "effective_timeout_secs",
        "sync_wait_secs",
        "execution_state",
        "command_started",
        "command_completed",
        "command_ok",
        "exit_code",
        "duration_ms",
        "purpose",
        "cwd",
        "executor",
    ] {
        output.remove(key);
    }
    if output
        .get("async_handoff_available")
        .and_then(Value::as_bool)
        == Some(true)
    {
        output.remove("async_handoff_available");
    }
    if output.get("failure_kind").is_some_and(Value::is_null) {
        output.remove("failure_kind");
    }
    if output.get("tool_failure").and_then(Value::as_bool) == Some(false) {
        output.remove("tool_failure");
    }
    for key in ["stdout_tail", "stderr_tail"] {
        if output.get(key).and_then(Value::as_str) == Some("") {
            output.remove(key);
        }
    }
    for key in ["stdout_lines", "stderr_lines"] {
        if output.get(key).and_then(Value::as_u64) == Some(0) {
            output.remove(key);
        }
    }
    for key in ["stdout_truncated", "stderr_truncated"] {
        if output.get(key).and_then(Value::as_bool) == Some(false) {
            output.remove(key);
        }
    }

    let summary_key = match tool_name {
        "project_build" | "run_process" | "run_skill_resource" => "process_summary",
        "run_script" => "script_summary",
        _ => unreachable!("structured execution sparsifier is tool-gated"),
    };
    let canonical_source =
        output.get("execution_source").and_then(Value::as_str) == Some(tool_name);
    let canonical_summary = output
        .get(summary_key)
        .and_then(Value::as_str)
        .is_some_and(|summary| !summary.is_empty());
    if canonical_source && canonical_summary {
        output.remove(summary_key);
        output.remove("execution_source");
    }
}

/// Shell context is runtime-selected, so it is not redundant with the request.
/// Only a proven ordinary synchronous terminal result may lose lifecycle facts.
fn sparsify_terminal_shell_success(result: &mut ToolResult) {
    if !result.success || result.error.is_some() {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    if output.get("execution_state").and_then(Value::as_str) != Some("completed")
        || [
            "command_started",
            "command_completed",
            "command_ok",
            "terminal",
        ]
        .iter()
        .any(|key| output.get(*key).and_then(Value::as_bool) != Some(true))
        || output.get("promoted_to_job").and_then(Value::as_bool) != Some(false)
        || output.get("exit_code").and_then(Value::as_i64) != Some(0)
        || output.get("tool_failure").and_then(Value::as_bool) != Some(false)
        || output.get("executor").and_then(Value::as_str) != Some("agent")
        || output.get("execution_source").and_then(Value::as_str) != Some("run_shell")
        || [
            "requested_surface",
            "job_id",
            "job_status",
            "observation_token",
            "continuation",
            "activity",
            "failure_kind",
            "recovery",
            "recovery_kind",
            "recovery_state",
            "recovery_reason_code",
            "recovered_after_server_restart",
            "suggested_call",
            "recovery_reason",
            "reconciled_at",
            "observation_error",
            "reconciliation",
        ]
        .iter()
        .any(|key| output.get(*key).is_some_and(|value| !value.is_null()))
    {
        return;
    }
    for key in [
        "execution_state",
        "command_started",
        "command_completed",
        "command_ok",
        "exit_code",
        "promoted_to_job",
        "terminal",
        "job_id",
        "job_status",
        "observation_token",
        "effective_timeout_secs",
        "sync_wait_secs",
        "duration_ms",
        "failure_kind",
        "tool_failure",
        "executor",
        "execution_source",
    ] {
        output.remove(key);
    }
    // Availability cannot change continuation once this execution is terminal.
    if output
        .get("async_handoff_available")
        .is_some_and(Value::is_boolean)
    {
        output.remove("async_handoff_available");
    }
    for key in ["stdout_tail", "stderr_tail"] {
        if output.get(key).and_then(Value::as_str) == Some("") {
            output.remove(key);
        }
    }
    for key in ["stdout_lines", "stderr_lines"] {
        if output.get(key).and_then(Value::as_u64) == Some(0) {
            output.remove(key);
        }
    }
    for key in ["stdout_truncated", "stderr_truncated"] {
        if output.get(key).and_then(Value::as_bool) == Some(false) {
            output.remove(key);
        }
    }
}

/// Strip successful wrapper/audit facts only after every authority and Session
/// recorder that needs them has consumed the canonical ToolResult.
/// Failure projection is handled separately and preserves every fact required
/// for retry, escalation, uncertainty, Job handoff, and reconciliation.
pub(super) fn sparsify_success_model_result_metadata(tool_name: &str, result: &mut ToolResult) {
    if !result.success {
        return;
    }
    super::git::sparsify_complete_git_review_success(tool_name, result);
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    output.remove("permission");
    output.remove("session_recorded");
    output.remove("session_event_id");
}

/// Strip model-irrelevant audit/wrapper facts from failures only after the
/// canonical ToolResult has been consumed by permission and Session recorders.
/// Retry/recovery, uncertainty, process output, exit state, Job handoff, and
/// authorization-denial evidence remain explicit.
pub(super) fn sparsify_failure_model_result_metadata(tool_name: &str, result: &mut ToolResult) {
    if result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    if output
        .get("permission")
        .and_then(|permission| permission.get("status"))
        .and_then(Value::as_str)
        == Some("auto_approved")
    {
        output.remove("permission");
    }
    output.remove("session_recorded");
    output.remove("session_event_id");
    if matches!(
        tool_name,
        "project_build" | "run_process" | "run_script" | "run_skill_resource"
    ) {
        for key in [
            "executor",
            "duration_ms",
            "purpose",
            "cwd",
            "execution_source",
        ] {
            output.remove(key);
        }
        output.remove(match tool_name {
            "project_build" | "run_process" | "run_skill_resource" => "process_summary",
            "run_script" => "script_summary",
            _ => unreachable!("structured failure sparsifier is tool-gated"),
        });
    }
}

fn apply_text_edits_model_projection(result: &mut ToolResult, change_count: usize, dry_run: bool) {
    if !result.success {
        return;
    }
    // The mutation handler has already validated the Runner protocol and consumed
    // hashes for revisions. Only a proven actual completion can omit effect echoes.
    let output = &mut result.output;
    let compact = !dry_run
        && output.get("dry_run").and_then(Value::as_bool) == Some(false)
        && output.get("execution_state").and_then(Value::as_str) == Some("completed")
        && output.get("changed").and_then(Value::as_bool).is_some()
        && output.get("state_changed") == output.get("changed")
        && output.get("would_change") == output.get("changed")
        && output.get("applied_count").and_then(Value::as_u64) == Some(change_count as u64)
        && output
            .get("planned_count")
            .is_none_or(|count| count.as_u64() == Some(change_count as u64));
    let Some(files) = output.get_mut("files").and_then(Value::as_array_mut) else {
        return;
    };
    for file in files {
        let Some(file) = file.as_object_mut() else {
            continue;
        };
        file.remove("old_sha256");
        file.remove("new_sha256");
    }
    if compact {
        let output = output.as_object_mut().expect("validated edit output");
        for key in [
            "dry_run",
            "execution_state",
            "state_changed",
            "would_change",
            "applied_count",
            "planned_count",
        ] {
            output.remove(key);
        }
    }
}

enum SearchModelProjection {
    None,
    Batch {
        project: String,
        queries: Vec<super::SearchProjectTextsQuery>,
        session_id: Option<String>,
        default_timeouts: Vec<bool>,
        max_result_bytes: Option<usize>,
    },
}

impl SearchModelProjection {
    fn capture(call: &ToolCall) -> Self {
        match call {
            ToolCall::SearchProjectTexts {
                project,
                queries,
                session_id,
                max_result_bytes,
            } => Self::Batch {
                project: project.clone(),
                queries: queries.clone(),
                session_id: session_id.clone(),
                default_timeouts: queries
                    .iter()
                    .map(|query| caller_uses_default_search_timeout(&query.timeout_secs))
                    .collect(),
                max_result_bytes: max_result_bytes.map(|bytes| {
                    super::search_project_texts::normalized_result_budget(Some(bytes))
                }),
            },
            _ => Self::None,
        }
    }
}

enum ModelFacingProjection {
    None,
    Execution {
        tool_name: &'static str,
        validation_policy: ValidationSuccessPolicy,
    },
    AgentWait,
    JobReadiness,
    ApplyTextEdits {
        change_count: usize,
        dry_run: bool,
    },
    Read(super::read_files::ReadModelProjection),
    Search(SearchModelProjection),
}

/// Request facts needed only after canonical execution/recording has finished.
/// Captured once before the ToolCall is moved, then consumed exactly once by the
/// terminal model-facing projection stage. The concrete projection stays private
/// so callers cannot branch on tool-specific result policy.
pub(super) struct ModelFacingProjectionPlan {
    projection: ModelFacingProjection,
}

impl ModelFacingProjectionPlan {
    pub(super) fn capture(call: &ToolCall) -> Self {
        let projection = match call {
            ToolCall::WaitForAgentEvents { .. }
            | ToolCall::ReadAgentWait { .. }
            | ToolCall::CancelAgentWait { .. } => ModelFacingProjection::AgentWait,
            ToolCall::WaitForJobReadiness { .. } => ModelFacingProjection::JobReadiness,
            ToolCall::ApplyTextEdits {
                changes, dry_run, ..
            } => ModelFacingProjection::ApplyTextEdits {
                change_count: changes.len(),
                dry_run: dry_run.unwrap_or(false),
            },
            ToolCall::CargoTest {
                require_tests,
                no_run,
                min_tests,
                ..
            } => ModelFacingProjection::Execution {
                tool_name: call.tool_name(),
                validation_policy: ValidationSuccessPolicy {
                    require_tests: *require_tests,
                    no_run: *no_run,
                    min_tests: *min_tests,
                },
            },
            ToolCall::ProjectBuild { .. }
            | ToolCall::RunProcess { .. }
            | ToolCall::RunSkillResource { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }
            | ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::ProjectValidate { .. }
            | ToolCall::GoTest { .. } => ModelFacingProjection::Execution {
                tool_name: call.tool_name(),
                validation_policy: ValidationSuccessPolicy::default(),
            },
            ToolCall::ReadFiles { .. } => {
                ModelFacingProjection::Read(super::read_files::ReadModelProjection::capture(call))
            }
            ToolCall::SearchProjectTexts { .. } => {
                ModelFacingProjection::Search(SearchModelProjection::capture(call))
            }
            _ => ModelFacingProjection::None,
        };
        Self { projection }
    }

    pub(super) fn bind_resolved_project(&mut self, resolved: Option<&ResolvedProject>) {
        if let ModelFacingProjection::Read(projection) = &mut self.projection {
            projection.bind_resolved_project(resolved);
        }
        let Some(resolved) = resolved else {
            return;
        };
        if let ModelFacingProjection::Search(SearchModelProjection::Batch { project, .. }) =
            &mut self.projection
        {
            *project = resolved.resolved_id.clone();
        }
    }

    /// Consume the plan at the only stage allowed to turn canonical execution
    /// output into the final model-facing shape. Session/audit
    /// recorders must run before this method.
    pub(super) fn project(self, result: &mut ToolResult) {
        match self.projection {
            ModelFacingProjection::None => {}
            ModelFacingProjection::JobReadiness => {
                super::observe_jobs::sparsify_job_readiness_model_result(result)
            }
            ModelFacingProjection::AgentWait => {
                super::agent_wait::agent_wait_model_projection(result)
            }
            ModelFacingProjection::ApplyTextEdits {
                change_count,
                dry_run,
            } => apply_text_edits_model_projection(result, change_count, dry_run),
            ModelFacingProjection::Execution {
                tool_name,
                validation_policy,
            } => {
                sparsify_structured_validation_success_evidence(
                    tool_name,
                    validation_policy,
                    result,
                );
                if tool_name == "run_shell" {
                    sparsify_terminal_shell_success(result);
                }
                sparsify_terminal_structured_execution_success(tool_name, result);
                sparsify_structured_validation_runtime_metadata(tool_name, result);
                super::jobs::sparsify_job_handoff_model_result(result);
            }
            ModelFacingProjection::Read(projection) => {
                let super::read_files::ReadModelProjection::Batch {
                    max_result_bytes, ..
                } = &projection
                else {
                    return;
                };
                super::read_files::apply_model_facing_output_budget(
                    result,
                    *max_result_bytes,
                    &projection,
                );
                super::read_files::enforce_final_model_facing_hard_cap(result, &projection);
                super::read_files::add_actionable_read_continuations(&projection, result);
                sparsify_complete_read_success("read_files", result);
            }
            ModelFacingProjection::Search(projection) => {
                if let SearchModelProjection::Batch {
                    project,
                    queries,
                    session_id,
                    default_timeouts,
                    max_result_bytes,
                } = &projection
                {
                    super::search_project_texts::apply_model_facing_output_budget(
                        result,
                        default_timeouts,
                        *max_result_bytes,
                        project,
                        queries,
                        session_id.as_deref(),
                    );
                    super::search_project_texts::enforce_final_model_facing_hard_cap(
                        result,
                        default_timeouts,
                        project,
                        queries,
                        session_id.as_deref(),
                        *max_result_bytes,
                    );
                }
                sparsify_search_success_for_model(&projection, result);
                if let SearchModelProjection::Batch {
                    project,
                    queries,
                    session_id,
                    max_result_bytes,
                    ..
                } = &projection
                {
                    super::search_project_texts::add_actionable_search_continuation(
                        result,
                        project,
                        queries,
                        session_id.as_deref(),
                        *max_result_bytes,
                    );
                }
            }
        }
    }
}

fn caller_uses_default_search_timeout(timeout_secs: &Option<i64>) -> bool {
    timeout_secs
        .as_ref()
        .copied()
        .unwrap_or(super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64)
        == super::files::DEFAULT_SEARCH_TIMEOUT_SECS as i64
}

fn sparsify_search_match_items(output: &mut serde_json::Map<String, Value>) {
    let Some(matches) = output.get_mut("matches").and_then(Value::as_array_mut) else {
        return;
    };
    for item in matches {
        let Some(item) = item.as_object_mut() else {
            continue;
        };
        for key in ["context_before", "context_after"] {
            if item
                .get(key)
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                item.remove(key);
            }
        }
        let path = item.get("path").and_then(Value::as_str).map(str::to_string);
        let Some(read_hint) = item.get_mut("read_hint").and_then(Value::as_object_mut) else {
            continue;
        };
        if path
            .as_deref()
            .is_some_and(|path| read_hint.get("path").and_then(Value::as_str) == Some(path))
        {
            read_hint.remove("path");
        }
    }
}

/// Project successful text-search presentation after Session/event consumers
/// have seen canonical evidence. Complete rg results keep only mode-relevant
/// records and explicit non-default controls. Fallback/truncated successes retain
/// diagnostic metadata; match items still drop only empty context and duplicate
/// read-hint paths. A default batch timeout may shrink under the shared absolute
/// deadline without making that derived value model-relevant.
pub(crate) fn sparsify_search_output_for_model(
    output: &mut serde_json::Map<String, Value>,
    default_timeout: bool,
    allow_batch_deadline_reduction: bool,
) -> bool {
    sparsify_search_match_items(output);
    let exit_code = output.get("exit_code").and_then(Value::as_i64);
    let result_mode = output
        .get("result_mode")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mode_consistent = match result_mode.as_deref() {
        Some("matches") => output
            .get("matches")
            .and_then(Value::as_array)
            .is_some_and(|matches| {
                output.get("count").and_then(Value::as_u64) == Some(matches.len() as u64)
            }),
        Some("files_with_matches") => {
            output
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(|files| {
                    output.get("returned_file_count").and_then(Value::as_u64)
                        == Some(files.len() as u64)
                })
        }
        Some("count") => output
            .get("files")
            .and_then(Value::as_array)
            .is_some_and(|files| {
                output.get("returned_file_count").and_then(Value::as_u64)
                    == Some(files.len() as u64)
                    && output.get("count_complete").and_then(Value::as_bool) == Some(true)
                    && output.get("returned_match_count").and_then(Value::as_u64)
                        == output.get("total_matches").and_then(Value::as_u64)
            }),
        _ => false,
    };
    let ordinary_complete = output.get("backend").and_then(Value::as_str) == Some("rg")
        && matches!(
            output.get("pattern_mode").and_then(Value::as_str),
            Some("regex" | "literal")
        )
        && mode_consistent
        && output.get("truncated").and_then(Value::as_bool) == Some(false)
        && output.get("truncation_reason").is_some_and(Value::is_null)
        && matches!(exit_code, Some(0 | 1));
    if !ordinary_complete {
        return false;
    }

    for key in [
        "project",
        "pattern",
        "backend",
        "exit_code",
        "truncated",
        "truncation_reason",
    ] {
        output.remove(key);
    }
    if output.get("pattern_mode").and_then(Value::as_str) == Some("regex") {
        output.remove("pattern_mode");
    }
    match result_mode.as_deref() {
        Some("matches") => {
            output.remove("result_mode");
            output.remove("count");
        }
        Some("files_with_matches") => {
            output.remove("returned_file_count");
        }
        Some("count") => {
            output.remove("returned_file_count");
            output.remove("returned_match_count");
            output.remove("count_complete");
            if output
                .get("files")
                .and_then(Value::as_array)
                .is_some_and(Vec::is_empty)
            {
                output.remove("files");
            }
        }
        _ => {}
    }
    for key in ["context_before", "context_after"] {
        if output.get(key).and_then(Value::as_u64) == Some(0) {
            output.remove(key);
        }
    }
    let effective_timeout = output.get("effective_timeout_secs").and_then(Value::as_u64);
    let timeout_is_boring_default = default_timeout
        && if allow_batch_deadline_reduction {
            effective_timeout.is_some_and(|timeout| {
                (1..=super::files::DEFAULT_SEARCH_TIMEOUT_SECS).contains(&timeout)
            })
        } else {
            effective_timeout == Some(super::files::DEFAULT_SEARCH_TIMEOUT_SECS)
        };
    if timeout_is_boring_default {
        output.remove("effective_timeout_secs");
    }
    if output.get("path").and_then(Value::as_str) == Some(".") {
        output.remove("path");
    }
    true
}

fn sparsify_search_success_for_model(projection: &SearchModelProjection, result: &mut ToolResult) {
    if !result.success {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    match projection {
        SearchModelProjection::Batch {
            default_timeouts, ..
        } => {
            let mut complete_batch =
                output
                    .get("items")
                    .and_then(Value::as_array)
                    .is_some_and(|items| {
                        let item_count = items.len() as u64;
                        item_count > 0
                            && items.iter().all(|item| {
                                item.get("success").and_then(Value::as_bool) == Some(true)
                                    && item.get("error").is_some_and(Value::is_null)
                            })
                            && output.get("requested_count").and_then(Value::as_u64)
                                == Some(item_count)
                            && output.get("returned_count").and_then(Value::as_u64)
                                == Some(item_count)
                            && output.get("succeeded_count").and_then(Value::as_u64)
                                == Some(item_count)
                            && output.get("failed_count").and_then(Value::as_u64) == Some(0)
                            && output.get("output_truncated").and_then(Value::as_bool)
                                == Some(false)
                            && output.get("next_index").is_none_or(Value::is_null)
                    });
            let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
                return;
            };
            for item in items.iter_mut() {
                let Some(item) = item.as_object_mut() else {
                    complete_batch = false;
                    continue;
                };
                if item.get("success").and_then(Value::as_bool) != Some(true)
                    || !item.get("error").is_some_and(Value::is_null)
                {
                    complete_batch = false;
                    continue;
                }
                let Some(index) = item.get("index").and_then(Value::as_u64) else {
                    complete_batch = false;
                    continue;
                };
                let Some(search_output) = item.get_mut("output").and_then(Value::as_object_mut)
                else {
                    complete_batch = false;
                    continue;
                };
                complete_batch &= sparsify_search_output_for_model(
                    search_output,
                    default_timeouts
                        .get(index as usize)
                        .copied()
                        .unwrap_or(false),
                    true,
                );
            }
            if complete_batch {
                for item in items {
                    let item = item
                        .as_object_mut()
                        .expect("validated successful search item");
                    item.remove("success");
                    item.remove("error");
                }

                for key in [
                    "project",
                    "requested_count",
                    "returned_count",
                    "succeeded_count",
                    "failed_count",
                    "output_truncated",
                    "next_index",
                ] {
                    output.remove(key);
                }
            }
        }
        SearchModelProjection::None => {}
    }
}

pub(crate) fn sparsify_search_batch_success_for_model(
    default_timeouts: &[bool],
    result: &mut ToolResult,
) {
    sparsify_search_success_for_model(
        &SearchModelProjection::Batch {
            project: String::new(),
            queries: Vec::new(),
            session_id: None,
            default_timeouts: default_timeouts.to_vec(),
            max_result_bytes: None,
        },
        result,
    );
}

/// Remove range bookkeeping only when the returned text is provably the complete
/// file. `read_revision` remains the model-facing snapshot identity and
/// `total_lines` remains content-shape evidence. Partial reads and every real continuation keep the canonical full
/// range tuple. In a batch, the outer item path remains the navigation identity,
/// so an identical inner path is redundant.
pub(crate) fn sparsify_complete_file_read_output(
    output: &mut serde_json::Map<String, Value>,
    duplicate_outer_path: Option<&str>,
) -> bool {
    let format = output.get("format").and_then(Value::as_str);
    let valid_format = matches!(format, Some("plain" | "numbered"));
    let plain_format = format == Some("plain");
    let Some(inner_path) = output.get("path").and_then(Value::as_str) else {
        return false;
    };
    if duplicate_outer_path.is_some_and(|path| path != inner_path) {
        return false;
    }
    let Some(total_lines) = output.get("total_lines").and_then(Value::as_u64) else {
        return false;
    };
    let Some(returned_lines) = output.get("returned_lines").and_then(Value::as_u64) else {
        return false;
    };
    let default_limit =
        webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit as u64;
    let end_line_matches = if total_lines == 0 {
        output.get("end_line").is_some_and(Value::is_null)
    } else {
        output.get("end_line").and_then(Value::as_u64) == Some(total_lines)
    };
    let complete_file = output.get("text").and_then(Value::as_str).is_some()
        && valid_format
        && output
            .get("sha256")
            .and_then(Value::as_str)
            .is_some_and(|sha| {
                sha.len() == 64
                    && sha
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            })
        && output
            .get("read_revision")
            .and_then(Value::as_u64)
            .is_some()
        && output.get("start_line").and_then(Value::as_u64) == Some(1)
        && output.get("limit").and_then(Value::as_u64) == Some(default_limit)
        && returned_lines == total_lines
        && returned_lines <= default_limit
        && end_line_matches
        && output.get("has_more").and_then(Value::as_bool) == Some(false)
        && output.get("next_start_line").is_some_and(Value::is_null);
    if !complete_file {
        return false;
    }

    // The digest has already served canonical snapshot registration. The model
    // uses the bounded read_revision handle for this exact snapshot.
    output.remove("sha256");
    for key in [
        "start_line",
        "limit",
        "returned_lines",
        "end_line",
        "has_more",
        "next_start_line",
    ] {
        output.remove(key);
    }
    if plain_format {
        output.remove("format");
    }
    if duplicate_outer_path.is_some() {
        output.remove("path");
    }
    true
}

pub(crate) fn sparsify_complete_read_success(tool_name: &str, result: &mut ToolResult) {
    if !result.success || tool_name != "read_files" {
        return;
    }
    let Some(output) = result.output.as_object_mut() else {
        return;
    };

    let complete_batch = output
        .get("items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            let item_count = items.len() as u64;
            item_count > 0
                && items.iter().all(|item| {
                    item.get("success").and_then(Value::as_bool) == Some(true)
                        && item.get("error").is_some_and(Value::is_null)
                })
                && output.get("requested_count").and_then(Value::as_u64) == Some(item_count)
                && output.get("returned_count").and_then(Value::as_u64) == Some(item_count)
                && output.get("succeeded_count").and_then(Value::as_u64) == Some(item_count)
                && output.get("failed_count").and_then(Value::as_u64) == Some(0)
                && output.get("output_truncated").and_then(Value::as_bool) == Some(false)
                && output.get("next_index").is_some_and(Value::is_null)
        });
    let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
        return;
    };
    let mut every_item_complete = complete_batch;
    for item in items {
        let Some(item) = item.as_object_mut() else {
            every_item_complete = false;
            continue;
        };
        if item.get("success").and_then(Value::as_bool) != Some(true)
            || !item.get("error").is_some_and(Value::is_null)
        {
            every_item_complete = false;
            continue;
        }
        let Some(outer_path) = item.get("path").and_then(Value::as_str).map(str::to_string) else {
            every_item_complete = false;
            continue;
        };
        let Some(read_output) = item.get_mut("output").and_then(Value::as_object_mut) else {
            every_item_complete = false;
            continue;
        };
        if !sparsify_complete_file_read_output(read_output, Some(&outer_path)) {
            every_item_complete = false;
        }
    }
    if every_item_complete {
        for item in output
            .get_mut("items")
            .and_then(Value::as_array_mut)
            .expect("validated read items")
        {
            let item = item
                .as_object_mut()
                .expect("validated successful read item");
            item.remove("success");
            item.remove("error");
        }

        for key in [
            "project",
            "requested_count",
            "returned_count",
            "succeeded_count",
            "failed_count",
            "output_truncated",
            "next_index",
        ] {
            output.remove(key);
        }
    }
    // The output-level call is the sole machine representation of follow-up
    // positions. `read_revision` is the sole model-facing snapshot identity;
    // the underlying digest remains canonical/internal evidence only.
    output.remove("next_index");
    if let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) {
        for item in items {
            if let Some(read) = item.get_mut("output").and_then(Value::as_object_mut) {
                read.remove("next_start_line");
                read.remove("budget_next_limit");
                if read.get("read_revision").and_then(Value::as_u64).is_some() {
                    read.remove("sha256");
                }
            }
        }
    }
}

#[cfg(test)]
mod structured_execution_sparse_projection_tests {
    use super::*;
    use serde_json::json;

    pub(super) fn terminal_process_result(execution_source: &str) -> ToolResult {
        ToolResult::ok(json!({
            "duration_ms": 1,
            "exit_code": 0,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false,
            "command_started": true,
            "command_completed": true,
            "command_ok": true,
            "failure_kind": null,
            "tool_failure": false,
            "purpose": "diagnostic",
            "process_summary": "tool --arg",
            "cwd": ".",
            "executor": "agent",
            "execution_source": execution_source,
            "execution_state": "completed",
            "promoted_to_job": false,
            "terminal": true,
            "job_id": null,
            "job_status": null,
            "observation_token": null,
            "effective_timeout_secs": 60,
            "sync_wait_secs": 10,
            "async_handoff_available": true
        }))
    }

    #[test]
    fn terminal_execution_only_omits_summary_and_source_for_exact_canonical_source() {
        let mut canonical = terminal_process_result("run_process");
        sparsify_terminal_structured_execution_success("run_process", &mut canonical);
        for omitted in [
            "process_summary",
            "execution_source",
            "execution_state",
            "command_started",
            "command_completed",
            "command_ok",
            "exit_code",
            "duration_ms",
            "purpose",
            "cwd",
            "executor",
        ] {
            assert!(canonical.output.get(omitted).is_none(), "{omitted}");
        }

        let mut alternate = terminal_process_result("alternate_process_source");
        sparsify_terminal_structured_execution_success("run_process", &mut alternate);
        assert_eq!(alternate.output["process_summary"], "tool --arg");
        assert_eq!(
            alternate.output["execution_source"],
            "alternate_process_source"
        );
        assert!(alternate.output.get("cwd").is_none());
        assert!(alternate.output.get("executor").is_none());
        assert!(alternate.output.get("execution_state").is_none());
    }

    #[test]
    fn project_build_terminal_success_keeps_backend_and_drops_execution_noise() {
        let mut result = terminal_process_result("project_build");
        result.output["backend"] = json!("rust");
        result.output["purpose"] = json!("build");
        result.output["process_summary"] = json!("cargo build -p demo");
        sparsify_terminal_structured_execution_success("project_build", &mut result);
        assert_eq!(result.output, json!({"backend": "rust"}));
    }

    #[test]
    fn project_build_failure_projection_keeps_decision_facts_without_process_echo() {
        let mut result = ToolResult::err_with_output(
            "build failed",
            json!({
                "execution_source": "project_build",
                "purpose": "build",
                "process_summary": "cargo build -p private-package",
                "cwd": "private/path",
                "executor": "agent",
                "backend": "rust",
                "execution_state": "completed",
                "command_started": true,
                "command_completed": true,
                "command_ok": false,
                "exit_code": 101,
                "failure_kind": "command_exit_nonzero",
                "tool_failure": false,
                "stderr_tail": "compiler error"
            }),
        );
        sparsify_failure_model_result_metadata("project_build", &mut result);
        assert_eq!(result.output["backend"], "rust");
        assert_eq!(result.output["failure_kind"], "command_exit_nonzero");
        assert_eq!(result.output["stderr_tail"], "compiler error");
        for omitted in [
            "execution_source",
            "purpose",
            "process_summary",
            "cwd",
            "executor",
        ] {
            assert!(result.output.get(omitted).is_none(), "{omitted}");
        }
    }

    #[test]
    fn terminal_validation_success_keeps_only_independent_mutation_truth() {
        let mut result = ToolResult::ok(json!({
            "project": "agent:test:webcodex",
            "command_summary": "cargo fmt",
            "cwd": ".",
            "shell": "configured",
            "executor": "agent",
            "execution_source": "cargo_fmt",
            "purpose": "format",
            "execution_state": "completed",
            "exit_code": 0,
            "duration_ms": 5,
            "stdout_tail": "",
            "stderr_tail": "",
            "stdout_lines": 0,
            "stderr_lines": 0,
            "stdout_truncated": false,
            "stderr_truncated": false,
            "command_started": true,
            "command_completed": true,
            "passed": true,
            "failure_kind": null,
            "promoted_to_job": false,
            "terminal": true,
            "job_id": null,
            "job_status": null,
            "observation_token": null,
            "effective_timeout_secs": 60,
            "sync_wait_secs": 60,
            "async_handoff_available": false,
            "changed": true,
            "state_changed": true
        }));

        sparsify_terminal_structured_execution_success("cargo_fmt", &mut result);
        sparsify_structured_validation_runtime_metadata("cargo_fmt", &mut result);

        assert_eq!(
            result.output,
            json!({"changed": true, "state_changed": true})
        );
    }

    #[test]
    fn failure_projection_removes_audit_noise_but_preserves_decision_relevant_facts() {
        let mut result = ToolResult::err_with_output(
            "process exited 17",
            json!({
                "permission": {"status": "auto_approved", "request_id": "wc_perm_private"},
                "session_recorded": true,
                "session_event_id": "evt_private",
                "executor": "agent",
                "duration_ms": 123,
                "purpose": "diagnostic",
                "cwd": "src/private",
                "execution_source": "run_process",
                "process_summary": "private-command --secret value",
                "failure_kind": "command_exit_nonzero",
                "execution_state": "completed",
                "command_started": true,
                "command_completed": true,
                "command_ok": false,
                "exit_code": 17,
                "stderr_tail": "compiler error",
                "stderr_truncated": false,
                "job_id": "job_keep",
                "observation_token": "token_keep",
                "recovery": {"kind": "inspect_output"}
            }),
        );
        result.output["expectation_satisfied"] = json!(true);
        sparsify_failure_model_result_metadata("run_process", &mut result);

        for omitted in [
            "permission",
            "session_recorded",
            "session_event_id",
            "executor",
            "duration_ms",
            "purpose",
            "cwd",
            "execution_source",
            "process_summary",
        ] {
            assert!(
                result.output.get(omitted).is_none(),
                "{omitted}: {}",
                result.output
            );
        }
        for retained in [
            "failure_kind",
            "execution_state",
            "command_started",
            "command_completed",
            "command_ok",
            "exit_code",
            "stderr_tail",
            "stderr_truncated",
            "job_id",
            "observation_token",
            "recovery",
            "expectation_satisfied",
        ] {
            assert!(
                result.output.get(retained).is_some(),
                "{retained}: {}",
                result.output
            );
        }
    }

    #[test]
    fn failure_projection_keeps_permission_denials_and_unknown_outcomes() {
        let mut result = ToolResult::err_with_output(
            "permission denied",
            json!({
                "permission": {
                    "status": "denied",
                    "reason": "restricted_requires_human_authorization"
                },
                "failure_kind": "outcome_unknown",
                "execution_state": "outcome_unknown",
                "command_started": true,
                "command_completed": false
            }),
        );
        sparsify_failure_model_result_metadata("run_process", &mut result);
        assert_eq!(result.output["permission"]["status"], "denied");
        assert_eq!(result.output["failure_kind"], "outcome_unknown");
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], false);
    }
}

#[cfg(test)]
mod sparse_read_projection_tests {
    use super::*;
    use serde_json::json;

    fn complete_batch_item(outer_path: Option<&str>, inner_path: &str) -> Value {
        let default_limit =
            webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
        let mut item = json!({
            "index": 0,
            "success": true,
            "output": {
                "text": "one",
                "format": "plain",
                "path": inner_path,
                "sha256": "a".repeat(64),
                "start_line": 1,
                "limit": default_limit,
                "total_lines": 1,
                "returned_lines": 1,
                "end_line": 1,
                "has_more": false,
                "next_start_line": null
            },
            "error": null
        });
        if let Some(path) = outer_path {
            item["path"] = json!(path);
        }
        item
    }

    #[test]
    fn sparse_read_batch_requires_exact_outer_path_identity() {
        for item in [
            complete_batch_item(None, "a.rs"),
            complete_batch_item(Some("other.rs"), "a.rs"),
        ] {
            let mut result = ToolResult::ok(json!({
                "project": "demo",
                "requested_count": 1,
                "returned_count": 1,
                "succeeded_count": 1,
                "failed_count": 0,
                "items": [item],
                "output_truncated": false,
                "next_index": null
            }));

            sparsify_complete_read_success("read_files", &mut result);

            assert_eq!(result.output["requested_count"], 1);
            assert_eq!(result.output["items"][0]["output"]["path"], "a.rs");
            assert_eq!(result.output["items"][0]["output"]["start_line"], 1);
        }
    }

    #[test]
    fn sparse_read_batch_requires_read_revision_before_hiding_digest() {
        let mut without_revision = complete_batch_item(Some("a.rs"), "a.rs");
        let mut result = ToolResult::ok(json!({
            "project": "demo",
            "requested_count": 1,
            "returned_count": 1,
            "succeeded_count": 1,
            "failed_count": 0,
            "items": [without_revision.clone()],
            "output_truncated": false,
            "next_index": null
        }));
        sparsify_complete_read_success("read_files", &mut result);
        assert_eq!(result.output["requested_count"], 1);
        assert!(result.output["items"][0]["output"]["sha256"]
            .as_str()
            .is_some());

        without_revision["output"]["read_revision"] = json!(42);
        let mut result = ToolResult::ok(json!({
            "project": "demo",
            "requested_count": 1,
            "returned_count": 1,
            "succeeded_count": 1,
            "failed_count": 0,
            "items": [without_revision],
            "output_truncated": false,
            "next_index": null
        }));
        sparsify_complete_read_success("read_files", &mut result);
        assert!(result.output.get("requested_count").is_none());
        assert_eq!(result.output["items"][0]["output"]["read_revision"], 42);
        assert!(result.output["items"][0]["output"].get("sha256").is_none());
    }
}

#[cfg(test)]
#[path = "tests/execution_projection.rs"]
mod execution_projection_tests;

#[path = "validation_success_projection.rs"]
mod validation_success_projection;
use validation_success_projection::{
    sparsify_structured_validation_success_evidence, ValidationSuccessPolicy,
};
