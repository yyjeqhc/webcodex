//! Runtime tool dispatch and session/permission guard flow.

use super::edit_tool_telemetry;
use super::session_context::{
    add_session_hint, session_guard_denied_result, session_lifecycle_denied_result,
    session_project_mismatch_result, SessionProjectMismatch,
};
use super::{permissions, session_context, sessions, ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use crate::tool_runtime::project_resolution::{ProjectResolverError, ResolvedProject};
use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanonicalProjectOutput {
    Canonical,
    Requested,
}

/// Select only inner adapters that still perform an auth-less legacy Project
/// lookup. The outer dispatcher has already resolved and authorized the caller's
/// selector under the current principal; these adapters need that canonical id
/// for execution. The output mode is decided in the same match so adding another
/// legacy adapter cannot silently change whether its `project` field echoes the
/// caller selector or the canonical execution target.
fn canonical_execution_project_binding(
    call: &mut ToolCall,
) -> Option<(&mut String, CanonicalProjectOutput)> {
    match call {
        ToolCall::GitDiffHunks { project, .. }
        | ToolCall::GitReviewSummary { project, .. }
        | ToolCall::ShowChanges { project, .. }
        | ToolCall::WorkspaceHygieneCheck { project, .. } => {
            Some((project, CanonicalProjectOutput::Requested))
        }
        #[cfg(feature = "workspace-checkpoints")]
        ToolCall::WorkspaceCheckpointCreate { project, .. }
        | ToolCall::WorkspaceCheckpointList { project, .. }
        | ToolCall::WorkspaceCheckpointShow { project, .. }
        | ToolCall::WorkspaceCheckpointRestore { project, .. }
        | ToolCall::WorkspaceCheckpointDelete { project, .. } => {
            Some((project, CanonicalProjectOutput::Requested))
        }
        ToolCall::RunProcess { project, .. }
        | ToolCall::RunDetachedProcess { project, .. }
        | ToolCall::StartAgentTaskCodingRun { project, .. }
        | ToolCall::RunScript { project, .. }
        | ToolCall::RunShell { project, .. }
        | ToolCall::OpenSessionShell { project, .. }
        | ToolCall::SessionShellExec { project, .. }
        | ToolCall::SessionShellStatus { project, .. }
        | ToolCall::CloseSessionShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::ApplyUnifiedDiff { project, .. }
        | ToolCall::DeleteProjectFiles { project, .. }
        | ToolCall::GitRestorePaths { project, .. }
        | ToolCall::DiscardUntracked { project, .. }
        | ToolCall::GitCommitPaths { project, .. }
        | ToolCall::GitStatus { project, .. }
        | ToolCall::GitLog { project, .. }
        | ToolCall::CargoFmt { project, .. }
        | ToolCall::CargoCheck { project, .. }
        | ToolCall::CargoTest { project, .. }
        | ToolCall::GoTest { project, .. }
        | ToolCall::ListProjectFiles { project, .. }
        | ToolCall::ListProjectTrackedFiles { project, .. }
        | ToolCall::ProjectOverview { project, .. }
        | ToolCall::WriteProjectFile { project, .. }
        | ToolCall::SaveProjectArtifact { project, .. }
        | ToolCall::ProjectArtifact { project, .. }
        | ToolCall::ReadProjectArtifactMetadata { project, .. }
        | ToolCall::ReadProjectArtifact { project, .. }
        | ToolCall::ArtifactUploadBegin { project, .. }
        | ToolCall::ArtifactUploadChunk { project, .. }
        | ToolCall::ArtifactUploadFinish { project, .. }
        | ToolCall::ArtifactUploadAbort { project, .. }
        | ToolCall::ApplyTextEdits { project, .. }
        | ToolCall::LspStatus { project, .. }
        | ToolCall::DocumentSymbols { project, .. }
        | ToolCall::DocumentDiagnostics { project, .. }
        | ToolCall::Hover { project, .. }
        | ToolCall::WorkspaceSymbols { project, .. }
        | ToolCall::GotoDefinition { project, .. }
        | ToolCall::FindReferences { project, .. }
        | ToolCall::CallHierarchy { project, .. } => {
            Some((project, CanonicalProjectOutput::Canonical))
        }
        _ => None,
    }
}

/// Add the Phase A lifecycle tuple to a definite pre-execution structured
/// execution denial without changing generic denial helpers used by unrelated
/// tools.
pub(super) fn decorate_structured_execution_prestart_denial(
    tool_name: &str,
    result: &mut ToolResult,
    fallback_failure_kind: &'static str,
) {
    let structured_execution = matches!(
        tool_name,
        "run_process" | "run_detached_process" | "run_script" | "run_skill_resource"
    );
    let structured_mutation = tool_name == "apply_text_edits";
    if !structured_execution && !structured_mutation {
        return;
    }
    let mut output = match std::mem::take(&mut result.output) {
        Value::Object(output) => output,
        other => {
            let mut output = serde_json::Map::new();
            output.insert("value".to_string(), other);
            output
        }
    };
    let failure_kind = output
        .get("failure_kind")
        .and_then(Value::as_str)
        .or_else(|| output.get("error_kind").and_then(Value::as_str))
        .or_else(|| output.get("code").and_then(Value::as_str))
        .unwrap_or(fallback_failure_kind)
        .to_string();
    output.insert(
        "execution_state".to_string(),
        Value::String("not_started".to_string()),
    );
    output.insert("failure_kind".to_string(), Value::String(failure_kind));
    output.insert("tool_failure".to_string(), Value::Bool(true));
    if structured_mutation {
        // These Runtime gates precede business mutation dispatch, so the
        // canonical mutation result can authoritatively prove no state changed.
        output.insert("state_changed".to_string(), Value::Bool(false));
    } else {
        output.insert("command_started".to_string(), Value::Bool(false));
        output.insert("command_completed".to_string(), Value::Bool(false));
        output.insert("command_ok".to_string(), Value::Bool(false));
        output.insert("exit_code".to_string(), Value::Null);
    }
    result.output = Value::Object(output);
}

fn is_structured_validation_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "cargo_fmt" | "cargo_check" | "cargo_test" | "go_test"
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
        "run_process" | "run_script" | "run_skill_resource"
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
        "run_process" | "run_skill_resource" => "process_summary",
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
        "run_process" | "run_script" | "run_skill_resource"
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
            "run_process" | "run_skill_resource" => "process_summary",
            "run_script" => "script_summary",
            _ => unreachable!("structured failure sparsifier is tool-gated"),
        });
    }
}

fn add_run_process_expectation_projection(
    tool_name: &str,
    expectation: &sessions::ToolCallExpectation,
    result: &mut ToolResult,
) {
    if tool_name != "run_process" {
        return;
    }
    if result.output.get("expectation_satisfied").is_some() {
        return;
    }
    let Some(expectation_satisfied) = sessions::public_result_expectation_satisfied(
        result.success,
        expectation,
        &result.output,
        result.error.as_deref(),
        None,
    ) else {
        return;
    };
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    output.insert(
        "expectation_satisfied".to_string(),
        Value::Bool(expectation_satisfied),
    );
}

fn apply_text_edits_model_projection(result: &mut ToolResult) {
    if !result.success {
        return;
    }
    let Some(files) = result.output.get_mut("files").and_then(Value::as_array_mut) else {
        return;
    };
    for file in files {
        let Some(file) = file.as_object_mut() else {
            continue;
        };
        file.remove("old_sha256");
        file.remove("new_sha256");
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
    JobHandoff,
    AgentWait,
    ApplyTextEdits,
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
            ToolCall::ApplyTextEdits { .. } => ModelFacingProjection::ApplyTextEdits,
            ToolCall::RunJob { .. }
            | ToolCall::RunProcess { .. }
            | ToolCall::RunSkillResource { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }
            | ToolCall::RunDetachedProcess { .. }
            | ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. }
            | ToolCall::GoTest { .. } => ModelFacingProjection::JobHandoff,
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
    /// output into the final model-facing read/search shape. Session/audit
    /// recorders must run before this method.
    pub(super) fn project(self, result: &mut ToolResult) {
        match self.projection {
            ModelFacingProjection::None => {}
            ModelFacingProjection::AgentWait => {
                super::agent_wait::agent_wait_model_projection(result)
            }
            ModelFacingProjection::ApplyTextEdits => apply_text_edits_model_projection(result),
            ModelFacingProjection::JobHandoff => {
                super::jobs::sparsify_job_handoff_model_result(result)
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
            let complete_batch =
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
            for item in items {
                let Some(item) = item.as_object_mut() else {
                    continue;
                };
                if item.get("success").and_then(Value::as_bool) != Some(true)
                    || !item.get("error").is_some_and(Value::is_null)
                {
                    continue;
                }
                let Some(index) = item.get("index").and_then(Value::as_u64) else {
                    continue;
                };
                let Some(search_output) = item.get_mut("output").and_then(Value::as_object_mut)
                else {
                    continue;
                };
                sparsify_search_output_for_model(
                    search_output,
                    default_timeouts
                        .get(index as usize)
                        .copied()
                        .unwrap_or(false),
                    true,
                );
            }
            if complete_batch {
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

/// Snapshot of the activity-relevant request facts, captured before the
/// `ToolCall` is moved into execution.
struct WorkspaceActivityContext {
    tool: &'static str,
    project: Option<String>,
    client: Option<String>,
    command: Option<String>,
    paths: Vec<String>,
}

impl ToolRuntime {
    /// Main dispatch — call from MCP handler or GPT Actions handler.
    ///
    /// This no-auth convenience defaults the caller context to `None`, which
    /// means Runner-backed tools are rejected (no owner can be proven). HTTP
    /// wrappers should prefer `dispatch_with_auth` so the depot `AuthContext`
    /// is forwarded. Tests use this wrapper for local-executor projects.
    #[cfg(test)]
    pub async fn dispatch(&self, call: ToolCall) -> ToolResult {
        self.dispatch_with_auth(call, None).await
    }

    /// Dispatch carrying the caller's auth context. Runner-backed tools enforce
    /// the owner boundary and capability requirements through
    /// `authorize_runner_tool`; local-executor tools are unaffected. Wrappers
    /// stay thin: they only forward the depot `AuthContext` here.
    pub fn dispatch_with_auth<'a>(
        &'a self,
        call: ToolCall,
        auth: Option<&'a AuthContext>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ToolResult> + Send + 'a>> {
        // The canonical dispatcher has grown into a large multi-specialist future. Keep that
        // state on the heap at the public dispatch boundary so direct callers do not need a
        // multi-megabyte stack frame merely to enter ToolRuntime.
        Box::pin(async move {
            self.dispatch_with_auth_transport(call, auth, sessions::SessionTransport::Api)
                .await
        })
    }

    pub(crate) async fn dispatch_with_auth_transport(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options(call, auth, transport)
            .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata(
            call,
            auth,
            transport,
            sessions::ToolCallRecorderMetadata::default(),
        )
        .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_window(
            call,
            auth,
            transport,
            recorder_metadata,
            None,
        )
        .await
    }

    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_window(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_recording_mode(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
    ) -> ToolResult {
        self.dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            inner_model_facing_recording,
            Vec::new(),
            super::context_projection::ContextMaterialCapabilities::default(),
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: super::context_projection::ContextMaterialCapabilities,
    ) -> ToolResult {
        let (mut result, projection, _) = self
            .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
            call,
            auth,
            transport,
            recorder_metadata,
            window,
            inner_model_facing_recording,
            context_request,
            material_capabilities,
            super::kernel::ToolProtocolCapabilities::default(),
        )
        .await;
        projection.project(&mut result);
        result
    }

    /// Kernel-only companion that returns the terminal model-facing projection
    /// plan after the same authoritative Project resolution used for execution.
    /// The returned ToolResult is still canonical with respect to domain-local
    /// budgeting and sparse projection so an outer recorder can consume it first.
    fn context_guidance_profile(call: &ToolCall) -> CodingGuidanceProfile {
        match call {
            ToolCall::WorkOnProject {
                guidance_profile, ..
            } => *guidance_profile,
            _ => CodingGuidanceProfile::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) async fn dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: super::context_projection::ContextMaterialCapabilities,
        protocol_capabilities: super::kernel::ToolProtocolCapabilities,
    ) -> (
        ToolResult,
        ModelFacingProjectionPlan,
        super::window_activity::ToolCallCorrelation,
    ) {
        let mut result_projection = ModelFacingProjectionPlan::capture(&call);
        let context_guidance_profile = Self::context_guidance_profile(&call);
        let immediate_tool_name = call.tool_name();
        let immediate_expectation = recorder_metadata.expectation.clone();
        let mut correlation = super::window_activity::ToolCallCorrelation::default();
        // Edit usage telemetry retains only fixed safe classifications. For
        // apply_patch it captures the requested matching enum before the call is
        // moved, never the patch/path/content arguments.
        let mut edit_usage = edit_tool_telemetry::start_edit_tool_usage_for_call(&call);
        let mut result = self
            .dispatch_with_auth_transport_options_and_metadata_inner(
                call,
                auth,
                transport,
                recorder_metadata.clone(),
                window,
                inner_model_facing_recording,
                context_request.clone(),
                material_capabilities,
                protocol_capabilities,
                &mut correlation,
                &mut result_projection,
            )
            .await;
        // Early project/session/auth failures can return before the normal
        add_run_process_expectation_projection(
            immediate_tool_name,
            &immediate_expectation,
            &mut result,
        );
        // Early project/session/auth failures can return before the normal
        // resolved-project sidecar hook. Preserve the main ToolResult while still
        // answering the explicit sidecar request conservatively: static material
        // remains available, but project-scoped material must not guess a target.
        if !context_request.is_empty() && result.output.get("context_projection").is_none() {
            self.add_requested_context_projection_with_guidance(
                &mut result,
                &context_request,
                None,
                auth,
                material_capabilities,
                context_guidance_profile,
                window,
            )
            .await;
        }
        if let Some(guard) = edit_usage.as_mut() {
            guard.finish_with_result(&result);
        }
        (result, result_projection, correlation)
    }

    /// Everything the activity ledger needs from a call, captured before the
    /// call value is moved into execution. `None` for non-mutating tools.
    fn capture_workspace_activity_context(
        call: &ToolCall,
        resolved_project: Option<&str>,
    ) -> Option<WorkspaceActivityContext> {
        let tool = call.tool_name();
        let mutating = super::tool_definition::runtime_tool_is_write_like(tool)
            || super::tool_definition::runtime_tool_is_shell_like(tool);
        if !mutating {
            return None;
        }
        let sanitized = call.session_log_arguments();
        let project = resolved_project.or_else(|| call.project());
        Some(WorkspaceActivityContext {
            tool,
            project: project.map(str::to_string),
            client: project
                .and_then(super::activity::runner_client_from_project)
                .map(str::to_string),
            command: match call {
                ToolCall::RunProcess {
                    executable, args, ..
                } => Some(crate::runner_http::process_preview(
                    executable,
                    args.iter().map(String::as_str),
                )),
                ToolCall::RunSkillResource {
                    skill_id,
                    path,
                    args,
                    ..
                } => Some(format!(
                    "trusted skill resource {skill_id}:{path} ({} args)",
                    args.len()
                )),
                ToolCall::RunDetachedProcess { args, .. } => {
                    Some(format!("detached process ({} args)", args.len()))
                }
                ToolCall::RunScript {
                    language,
                    script,
                    args,
                    ..
                } => Some(crate::runner_http::script_preview(
                    language.as_str(),
                    script.len(),
                    args.len(),
                )),
                _ => call.command_text().map(str::to_string),
            },
            paths: super::activity::paths_from_sanitized_arguments(&sanitized, 16),
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn record_dispatch_session_result(
        &self,
        result: &mut ToolResult,
        session_id: &str,
        start: Option<sessions::ToolCallStart>,
        tool_name: &str,
        error_kind: Option<&str>,
        model_facing: bool,
        ack_observation: Option<&sessions::SessionAckObservation>,
        ack_requested: bool,
    ) {
        let success = result.success;
        let error = result.error.clone();
        if model_facing {
            let session_output =
                super::tool_audit::session_log_result_for_tool(tool_name, &result.output);
            self.sessions.record_tool_call_finished(
                start,
                success,
                &session_output,
                error.as_deref(),
                error_kind,
            );
            add_session_hint(result, &self.sessions, session_id);
            if let Some(ack) = ack_observation {
                session_context::add_session_attention_projection(
                    result,
                    &self.sessions,
                    session_id,
                    ack,
                    ack_requested,
                );
            }
        } else {
            self.sessions.record_tool_call_finished(
                start,
                success,
                &result.output,
                error.as_deref(),
                error_kind,
            );
            add_session_hint(result, &self.sessions, session_id);
        }
    }

    async fn dispatch_with_auth_transport_options_and_metadata_inner(
        &self,
        mut call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        mut recorder_metadata: sessions::ToolCallRecorderMetadata,
        window: Option<&crate::client_window::ClientWindow>,
        inner_model_facing_recording: bool,
        context_request: Vec<String>,
        material_capabilities: super::context_projection::ContextMaterialCapabilities,
        protocol_capabilities: super::kernel::ToolProtocolCapabilities,
        correlation: &mut super::window_activity::ToolCallCorrelation,
        result_projection: &mut ModelFacingProjectionPlan,
    ) -> ToolResult {
        call = call
            .with_coding_agent_recording_session_id(recorder_metadata.recording_session_id.clone());
        if let ToolCall::PluginTool(plugin) = call {
            return match crate::plugin_gateway::invoke(
                self,
                plugin,
                recorder_metadata.recording_session_id.as_deref(),
                auth,
                transport,
            )
            .await
            {
                Ok(invocation) => invocation.to_tool_result(),
                Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Scope {
                    required_scope,
                    description,
                }) => ToolResult::err_with_output(
                    description,
                    serde_json::json!({
                        "failure_kind": "insufficient_scope",
                        "required_scope": required_scope,
                        "dispatch_certainty": "not_started",
                    }),
                ),
                Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Tool(
                    result,
                )) => result,
            };
        }
        if let ToolCall::SshResource(ssh_resource) = call {
            return match crate::ssh_resource_gateway::invoke(
                self,
                ssh_resource,
                recorder_metadata.recording_session_id.as_deref(),
                auth,
                transport,
            )
            .await
            {
                Ok(invocation) => invocation.to_tool_result(),
                Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Scope {
                    required_scope,
                    description,
                }) => ToolResult::err_with_output(
                    description,
                    serde_json::json!({
                        "failure_kind": "insufficient_scope",
                        "required_scope": required_scope,
                        "dispatch_certainty": "not_started",
                    }),
                ),
                Err(crate::tool_runtime::specialized::SpecializedGovernanceDenial::Tool(
                    result,
                )) => result,
            };
        }
        // Kernel requests arrive with the same trusted logical identity already
        // used by the outer recorder. Mark only this concrete ledger path as the
        // authoritative business role; direct/internal dispatch without a kernel
        // identity remains uncorrelated legacy-style evidence.
        recorder_metadata.mark_business_execution();
        let project_resolution = match call.project() {
            Some(project) => Some(self.resolve_project_input_for_auth(project, auth).await),
            None => None,
        };
        let resolved_project = project_resolution
            .as_ref()
            .and_then(|resolution| resolution.as_ref().ok());
        result_projection.bind_resolved_project(resolved_project);
        // Preserve the canonical project for activity attribution before the
        // session recorder consumes the resolved value below. Short aliases
        // must not turn a real Runner execution into a client-less row.
        let activity_project = resolved_project
            .as_ref()
            .map(|resolved| resolved.resolved_id.clone());
        correlation.resolved_project = activity_project.clone();
        if let (Some(project), Some(trace_id)) = (
            activity_project.as_deref(),
            crate::tool_request_trace::current_active_trace_id(),
        ) {
            self.window_activity.update(&trace_id, None, Some(project));
        }
        let context_projection_project = if context_request.is_empty() {
            None
        } else {
            resolved_project.cloned()
        };
        let context_guidance_profile = Self::context_guidance_profile(&call);
        // work_on_project.session_id is explicit coding-resume business input,
        // never a generic tool recorder. Its implementation delegates exact
        // Session/project/lifecycle/authority handling to the coding workflow
        // engine.
        let defer_work_session = matches!(&call, ToolCall::WorkOnProject { .. });
        // session_handoff_summary.session_id remains business input for direct
        // internal dispatch. When the kernel already has an explicit outer
        // recording Session, suppress this inner recorder so worker W reading
        // coordinator C records the tool execution only in W.
        let suppress_handoff_business_recorder = !inner_model_facing_recording
            && matches!(&call, ToolCall::SessionHandoffSummary { .. });
        let session_id = if defer_work_session || suppress_handoff_business_recorder {
            None
        } else {
            call.session_id().map(str::to_string)
        };
        if let Some(session_id) = session_id.as_deref() {
            // Direct/internal dispatch may derive a recorder from explicit
            // business session_id (notably the handoff compatibility path).
            // Fence that exact Session before lifecycle/guard inheritance,
            // project mismatch logic, or any ledger mutation.
            if let Err(mut result) = self
                .authorize_session_target(session_id, call.tool_name(), auth)
                .await
            {
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_authority_denied",
                );
                return result;
            }
        }
        let inner_ack_requested =
            inner_model_facing_recording && !recorder_metadata.ack_session_message_ids.is_empty();
        let inner_ack_observation = if inner_model_facing_recording {
            session_id.as_deref().map(|session_id| {
                session_context::observe_session_attention_acks(
                    &self.sessions,
                    session_id,
                    &recorder_metadata.ack_session_message_ids,
                )
            })
        } else {
            None
        };
        let session_contract = super::sessions::session_tool_contract(call.tool_name());
        let session_project_mismatch = session_id.as_deref().and_then(|session_id| {
            match (
                self.sessions.session_project(session_id),
                resolved_project.as_ref(),
            ) {
                (Some(Some(session_project)), Some(resolved))
                    if session_project != resolved.resolved_id =>
                {
                    Some(SessionProjectMismatch {
                        session_project,
                        request_project: resolved.resolved_id.clone(),
                    })
                }
                _ => None,
            }
        });
        if let (Some(session_id), Some(mismatch)) =
            (session_id.as_deref(), session_project_mismatch.as_ref())
        {
            let session_start = self.sessions.record_tool_call_started_with_metadata(
                Some(session_id),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                Some(mismatch.request_project.clone()),
                recorder_metadata.clone(),
                session_contract,
            );
            let mut result =
                session_project_mismatch_result(session_id, call.tool_name(), mismatch);
            decorate_structured_execution_prestart_denial(
                call.tool_name(),
                &mut result,
                session_context::SESSION_PROJECT_MISMATCH_KIND,
            );
            self.record_dispatch_session_result(
                &mut result,
                session_id,
                session_start,
                call.tool_name(),
                Some(session_context::SESSION_PROJECT_MISMATCH_KIND),
                inner_model_facing_recording,
                inner_ack_observation.as_ref(),
                inner_ack_requested,
            )
            .await;
            return result;
        }
        // Inherit execution defaults only after exact project matching has
        // been established. Explicit per-call cwd/shell fields remain authoritative.
        let mut ssh_resource = None;
        if session_project_mismatch.is_none() {
            if let (Some(session_id), Some(resolved)) =
                (session_id.as_deref(), resolved_project.as_ref())
            {
                if let Some(execution_context) = self
                    .sessions
                    .execution_context_for_project(session_id, &resolved.resolved_id)
                {
                    if matches!(
                        &call,
                        ToolCall::RunProcess { .. }
                            | ToolCall::RunSkillResource { .. }
                            | ToolCall::RunDetachedProcess { .. }
                            | ToolCall::RunScript { .. }
                            | ToolCall::RunShell { .. }
                            | ToolCall::RunJob { .. }
                            | ToolCall::OpenSessionShell { .. }
                            | ToolCall::CargoFmt { .. }
                            | ToolCall::CargoCheck { .. }
                            | ToolCall::CargoTest { .. }
                            | ToolCall::GoTest { .. }
                    ) {
                        ssh_resource = execution_context.resource.clone();
                    }
                    call = call.with_session_execution_context(&execution_context);
                }
            }
        }
        if let Some(session_id) = session_id.as_deref() {
            // Lifecycle denial is orthogonal to mode/guards and wins first.
            if let Some(denial) =
                self.sessions
                    .lifecycle_denial(session_id, call.tool_name(), session_contract)
            {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                    session_contract,
                );
                let mut result =
                    session_lifecycle_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_lifecycle_denied",
                );
                let error_kind = result
                    .output
                    .get("error_kind")
                    .and_then(Value::as_str)
                    .unwrap_or("session_closed")
                    .to_string();
                self.record_dispatch_session_result(
                    &mut result,
                    session_id,
                    session_start,
                    call.tool_name(),
                    Some(error_kind.as_str()),
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
                return result;
            }
            if let Some(denial) = self.sessions.guard_denial(session_id, session_contract) {
                let session_start = self.sessions.record_tool_call_started_with_metadata(
                    Some(session_id),
                    transport,
                    call.tool_name(),
                    &call.session_log_arguments(),
                    None,
                    recorder_metadata.clone(),
                    session_contract,
                );
                let mut result = session_guard_denied_result(session_id, call.tool_name(), denial);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "session_guard_denied",
                );
                self.record_dispatch_session_result(
                    &mut result,
                    session_id,
                    session_start,
                    call.tool_name(),
                    Some("session_guard_denied"),
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
                return result;
            }
        }
        let mut session_start = if session_id.is_some() {
            let resolved_project = resolved_project.map(|resolved| resolved.resolved_id.clone());
            self.sessions.record_tool_call_started_with_metadata(
                session_id.as_deref(),
                transport,
                call.tool_name(),
                &call.session_log_arguments(),
                resolved_project,
                recorder_metadata.clone(),
                session_contract,
            )
        } else {
            None
        };
        if let Err(err) = self
            .authorize_runner_tool(
                &call,
                ssh_resource.as_deref(),
                auth,
                project_resolution.as_ref(),
            )
            .await
        {
            let mut err = err;
            let failure_kind =
                super::process::classify_process_failure(err.error.as_deref().unwrap_or_default());
            decorate_structured_execution_prestart_denial(call.tool_name(), &mut err, failure_kind);
            if let Some(session_id) = session_id.as_deref() {
                self.record_dispatch_session_result(
                    &mut err,
                    session_id,
                    session_start,
                    call.tool_name(),
                    None,
                    inner_model_facing_recording,
                    inner_ack_observation.as_ref(),
                    inner_ack_requested,
                )
                .await;
            }
            return err;
        }
        // Authoritative single evaluation (kernel must not re-evaluate).
        // Order: session/auth guards above → permission gate → mutation below.
        // Path/sensitive hard checks still run inside tools; hard-deny filter
        // suppresses permission attach so soft policy never overrides them.
        let permission = super::permissions::evaluate_permission_for_tool(
            &self.permission_evaluator,
            call.tool_name(),
            activity_project.as_deref().or_else(|| call.project()),
        );
        if let Some(decision) = permission.as_ref() {
            if !decision.allows_execution() {
                let mut result = permissions::permission_execution_denied_result(decision);
                decorate_structured_execution_prestart_denial(
                    call.tool_name(),
                    &mut result,
                    "permission_denied",
                );
                if let Some(start) = session_start.as_mut() {
                    self.sessions
                        .record_permission_decision(start, decision.clone());
                }
                permissions::add_permission_to_result(&mut result, decision);
                if let Some(session_id) = session_id.as_deref() {
                    self.record_dispatch_session_result(
                        &mut result,
                        session_id,
                        session_start,
                        call.tool_name(),
                        None,
                        inner_model_facing_recording,
                        inner_ack_observation.as_ref(),
                        inner_ack_requested,
                    )
                    .await;
                }
                return result;
            }
        }
        let activity_context =
            Self::capture_workspace_activity_context(&call, activity_project.as_deref());
        let validation_assertion_name = recorder_metadata.expectation.assertion_name.as_deref();
        let logical_invocation_id = recorder_metadata.logical_invocation_id.as_deref();
        let tool_name = call.tool_name();
        let trusted_recording_session_id = recorder_metadata
            .recording_session_authorized
            .then(|| recorder_metadata.recording_session_id.as_deref())
            .flatten();
        let trusted_recording_session_project = recorder_metadata
            .recording_session_authorized
            .then(|| recorder_metadata.recording_session_project.as_deref())
            .flatten();
        let shell_recovery = self
            .process_shell_recovery_call(
                &call,
                &recorder_metadata.expectation,
                ssh_resource.as_deref(),
                project_resolution
                    .as_ref()
                    .and_then(|resolved| resolved.as_ref().ok()),
            )
            .await;
        let source_mutation = if super::validation_source::observes_potential_mutation(&call) {
            activity_project
                .as_deref()
                .and_then(|project| self.validation_sources.begin(project))
        } else {
            None
        };
        // Resolve model-facing Project selectors exactly once under the current
        // authenticated principal, then bind only legacy inner adapters that still
        // perform auth-less ProjectConfig lookups to the authorized canonical id.
        // Do not rewrite every Project-bearing ToolCall: work_on_project preserves
        // the caller selector in its output, Work Result deliberately requires an
        // exact canonical input, and newer adapters consume the retained
        // ResolvedProject or re-resolve with the current AuthContext themselves.
        let mut requested_project_output = None;
        if let Some(resolved) = project_resolution
            .as_ref()
            .and_then(|resolution| resolution.as_ref().ok())
        {
            if let Some((project, output_mode)) = canonical_execution_project_binding(&mut call) {
                if output_mode == CanonicalProjectOutput::Requested {
                    requested_project_output = Some(project.clone());
                }
                project.clone_from(&resolved.resolved_id);
            }
        }
        let read_scope = super::read_cache::ReadScope::new(
            auth,
            session_id.as_deref().or(trusted_recording_session_id),
        );
        let mut result = super::read_cache::READ_SCOPE
            .scope(
                read_scope,
                self.dispatch_authorized_inner(
                    call,
                    auth,
                    transport,
                    window,
                    ssh_resource.as_deref(),
                    validation_assertion_name,
                    project_resolution,
                    trusted_recording_session_id,
                    trusted_recording_session_project,
                    logical_invocation_id,
                    protocol_capabilities,
                    correlation,
                ),
            )
            .await;
        if let Some(requested_project) = requested_project_output {
            if result.output.get("project").is_some() {
                result.output["project"] = serde_json::Value::String(requested_project);
            }
        }
        if let Some(observation) = source_mutation {
            observation.finish(&result);
        }
        if !result.success
            && result.output["command_started"] == false
            && result.output["execution_state"] == "not_started"
            && result.output["failure_kind"] == "invalid_arguments"
            && result.error.as_deref().is_some_and(|error| {
                error.contains("run_process does not accept shell command modes")
            })
        {
            if let Some(suggested_call) = shell_recovery {
                result.output["suggested_call"] = suggested_call;
            }
        }
        let permission = permission.filter(|_| {
            !permissions::is_hard_denied_output(&result.output, result.error.as_deref())
        });
        if let Some(permission) = permission.as_ref() {
            if let Some(start) = session_start.as_mut() {
                self.sessions
                    .record_permission_decision(start, permission.clone());
            }
            permissions::add_permission_to_result(&mut result, permission);
        }
        if let Some(session_id) = session_id.as_deref() {
            self.record_dispatch_session_result(
                &mut result,
                session_id,
                session_start,
                tool_name,
                None,
                inner_model_facing_recording,
                inner_ack_observation.as_ref(),
                inner_ack_requested,
            )
            .await;
        }
        add_run_process_expectation_projection(
            tool_name,
            &recorder_metadata.expectation,
            &mut result,
        );
        if let Some(context) = activity_context {
            self.activity.record(super::activity::ActivityRecord {
                tool: context.tool,
                project: context.project.as_deref(),
                surface: transport.as_str(),
                client: context.client.as_deref(),
                success: result.success,
                session_id: session_id.as_deref(),
                command: context.command.as_deref(),
                paths: context.paths,
                error_summary: result.error.as_deref(),
                // Derived from the verified caller here, not looked up later
                // from whoever holds this client id at read time.
                scope: super::activity::activity_scope_from_auth(auth),
            });
        }
        if result.success
            && webcodex_tool_contracts::runtime_tool_activity_interaction(tool_name).is_meaningful()
        {
            if let Ok((principal_kind, principal_id)) =
                super::session_context::runtime_observation_principal(auth)
            {
                self.observations.record_successful_tool_call(
                    super::observations::ToolCallObservation {
                        principal_kind,
                        principal_id,
                        project: activity_project.clone(),
                        surface: transport.as_str().to_string(),
                        session_id: session_id.clone(),
                        tool: tool_name.to_string(),
                        observed_at: chrono::Utc::now().timestamp(),
                    },
                );
            }
        }
        self.add_requested_context_projection_with_guidance(
            &mut result,
            &context_request,
            context_projection_project.as_ref(),
            auth,
            material_capabilities,
            context_guidance_profile,
            window,
        )
        .await;
        sparsify_terminal_structured_execution_success(tool_name, &mut result);
        sparsify_structured_validation_runtime_metadata(tool_name, &mut result);
        result
    }

    async fn dispatch_authorized_inner(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: sessions::SessionTransport,
        window: Option<&crate::client_window::ClientWindow>,
        ssh_resource: Option<&str>,
        validation_assertion_name: Option<&str>,
        project_resolution: Option<Result<ResolvedProject, ProjectResolverError>>,
        trusted_recording_session_id: Option<&str>,
        trusted_recording_session_project: Option<&str>,
        _logical_invocation_id: Option<&str>,
        protocol_capabilities: super::kernel::ToolProtocolCapabilities,
        correlation: &mut super::window_activity::ToolCallCorrelation,
    ) -> ToolResult {
        match call {
            call @ (ToolCall::ListTools { .. }
            | ToolCall::ListRunners { .. }
            | ToolCall::RuntimeStatus { .. }
            | ToolCall::ReadToolTrace { .. }
            | ToolCall::ToolManifest { .. }) => {
                self.dispatch_discovery_tool(call, auth, protocol_capabilities)
                    .await
            }

            call @ (ToolCall::RunnerConfigCheck { .. } | ToolCall::RunnerConfigReload { .. }) => {
                self.dispatch_runner_config_tool(call, auth).await
            }

            ToolCall::PluginTool(_) => {
                unreachable!(
                    "plugin_tool is dispatched before generic static ToolDefinition policy"
                )
            }

            ToolCall::SshResource(_) => {
                unreachable!(
                    "ssh_resource is dispatched before generic static ToolDefinition policy"
                )
            }

            ToolCall::PostPeerMessage {
                peer_id,
                kind,
                message,
                tags,
                priority,
                requires_ack,
            } => self.post_peer_message_tool(
                peer_id,
                kind,
                message,
                tags,
                priority,
                requires_ack,
                auth,
                window,
                trusted_recording_session_id,
                trusted_recording_session_project,
            ),

            call @ (ToolCall::StartSession { .. }
            | ToolCall::SessionSummary { .. }
            | ToolCall::UpdateSessionContext { .. }
            | ToolCall::CloseSession { .. }
            | ToolCall::ValidationSummary { .. }
            | ToolCall::PostSessionMessage { .. }
            | ToolCall::ListSessionMessages { .. }
            | ToolCall::GetSessionAssignment { .. }
            | ToolCall::ObserveSessionMessages { .. }
            | ToolCall::ResolveSessionMessage { .. }
            | ToolCall::CompleteSessionMessage { .. }
            | ToolCall::SessionDiscussionSummary { .. }) => {
                self.dispatch_session_tool(call, auth, transport).await
            }

            call @ (ToolCall::WorkOnProject { .. } | ToolCall::FinishCodingTask { .. }) => {
                // Startup/closeout aggregation retains relatively large typed workflow state.
                // Keep that future off the shared dispatch future so unrelated tool calls do
                // not inherit its stack cost as the coding startup contract evolves.
                Box::pin(self.dispatch_coding_task_tool(
                    call,
                    auth,
                    transport,
                    trusted_recording_session_id,
                    trusted_recording_session_project,
                    correlation,
                ))
                .await
            }

            ToolCall::PresentWorkResult {
                project,
                session_id,
            } => self.present_work_result(project, session_id, auth).await,

            ToolCall::WorkResultState {
                project,
                session_id,
            } => self.work_result_state(project, session_id, auth).await,

            ToolCall::ChangesFileDiff {
                project,
                session_id,
                snapshot_id,
                path,
            } => {
                self.changes_file_diff(project, session_id, snapshot_id, path, auth)
                    .await
            }

            call @ ToolCall::SessionHandoffSummary { .. } => {
                self.dispatch_handoff_tool(call, auth).await
            }

            #[cfg(feature = "workspace-checkpoints")]
            call @ (ToolCall::WorkspaceCheckpointCreate { .. }
            | ToolCall::WorkspaceCheckpointList { .. }
            | ToolCall::WorkspaceCheckpointShow { .. }
            | ToolCall::WorkspaceCheckpointRestore { .. }
            | ToolCall::WorkspaceCheckpointDelete { .. }) => {
                self.dispatch_workspace_checkpoint_tool(call).await
            }

            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExec {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("code_mode_exec requires a resolved Project"),
                };
                let (result, composition) = self
                    .code_mode_exec(
                        project,
                        session_id,
                        source,
                        timeout_ms,
                        auth,
                        transport,
                        _logical_invocation_id.map(str::to_string),
                    )
                    .await;
                correlation.code_mode_composition = Some(composition);
                result
            }

            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExecEffectful {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err(
                            "code_mode_exec_effectful requires a resolved Project",
                        )
                    }
                };
                let (result, composition) = self
                    .code_mode_exec_effectful(
                        project,
                        session_id,
                        source,
                        timeout_ms,
                        auth,
                        transport,
                        _logical_invocation_id.map(str::to_string),
                    )
                    .await;
                correlation.code_mode_composition = Some(composition);
                result
            }

            #[cfg(feature = "experimental-code-mode")]
            ToolCall::CodeModeExecMutating {
                project: _,
                session_id,
                source,
                timeout_ms,
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err(
                            "code_mode_exec_mutating requires a resolved Project",
                        )
                    }
                };
                let (result, composition) = self
                    .code_mode_exec_mutating(
                        project,
                        session_id,
                        source,
                        timeout_ms,
                        auth,
                        transport,
                        _logical_invocation_id.map(str::to_string),
                    )
                    .await;
                correlation.code_mode_composition = Some(composition);
                result
            }

            ToolCall::BrowserObserve(_) | ToolCall::BrowserAct(_) => ToolResult::err(
                "Browser gateways must pass action-sensitive specialized governance".to_string(),
            ),
            ToolCall::ComputerObserve(_) | ToolCall::ComputerControl(_) => ToolResult::err(
                "Computer gateways must pass action-sensitive specialized governance".to_string(),
            ),
            call @ ToolCall::ComputerSaveSnapshot { .. } => {
                self.dispatch_computer_tool(call, auth).await
            }

            call @ (ToolCall::ListProjects { .. }
            | ToolCall::RegisterProject { .. }
            | ToolCall::UnregisterProject { .. }
            | ToolCall::CreateProject { .. }) => self.dispatch_project_tool(call, auth).await,

            ToolCall::CodingAgentStart {
                project,
                provider_id,
                idempotency_key,
                instruction,
                config,
                timeout_secs,
                recording_session_id,
            } => {
                Box::pin(self.coding_agent_start(
                    project,
                    provider_id,
                    idempotency_key,
                    instruction,
                    config,
                    timeout_secs,
                    recording_session_id,
                    auth,
                ))
                .await
            }
            ToolCall::CodingAgentObserve {
                run_id,
                after_observation_token,
                wait_secs,
            } => {
                self.coding_agent_observe(run_id, after_observation_token, wait_secs, auth)
                    .await
            }
            ToolCall::CodingAgentCancel { run_id } => self.coding_agent_cancel(run_id, auth).await,

            call @ (ToolCall::RunProcess { .. }
            | ToolCall::RunDetachedProcess { .. }
            | ToolCall::RunScript { .. }
            | ToolCall::RunShell { .. }) => {
                self.dispatch_shell_tool(call, ssh_resource, validation_assertion_name, auth)
                    .await
            }

            call @ (ToolCall::OpenSessionShell { .. }
            | ToolCall::SessionShellExec { .. }
            | ToolCall::SessionShellStatus { .. }
            | ToolCall::CloseSessionShell { .. }) => {
                self.dispatch_session_shell_tool(call, ssh_resource).await
            }

            call @ (ToolCall::ApplyPatch { .. } | ToolCall::ApplyUnifiedDiff { .. }) => {
                self.dispatch_patch_tool(call).await
            }

            ToolCall::RunSkillResource {
                skill_id,
                path,
                expected_definition_revision,
                expected_package_revision,
                args,
                session_id,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err("run_skill_resource requires a resolved Project")
                    }
                };
                self.run_skill_resource(
                    &project,
                    skill_id,
                    path,
                    expected_definition_revision,
                    expected_package_revision,
                    args,
                    cwd,
                    timeout_secs,
                    sync_wait_secs,
                    purpose,
                    session_id,
                    auth,
                )
                .await
            }

            ToolCall::SkillLoad { name, .. } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_load requires a resolved Project"),
                };
                self.skill_load(&project, name, auth).await
            }

            ToolCall::SkillList {
                query,
                offset,
                limit,
                expected_catalog_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_list requires a resolved Project"),
                };
                self.skill_list(
                    &project,
                    query,
                    offset,
                    limit,
                    expected_catalog_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillReadFile {
                skill_id,
                path,
                start_line,
                limit,
                expected_definition_revision,
                expected_package_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_read_file requires a resolved Project"),
                };
                self.skill_read_file(
                    &project,
                    skill_id,
                    path,
                    start_line,
                    limit,
                    expected_definition_revision,
                    expected_package_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillVersions {
                skill_key,
                offset,
                limit,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_versions requires a resolved Project"),
                };
                self.skill_versions(&project, skill_key, offset, limit, auth)
                    .await
            }

            ToolCall::SkillInstall {
                skill_key,
                artifact_path,
                expected_artifact_sha256,
                idempotency_key,
                activate,
                expected_state_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_install requires a resolved Project"),
                };
                self.skill_install(
                    &project,
                    skill_key,
                    artifact_path,
                    expected_artifact_sha256,
                    idempotency_key,
                    activate.unwrap_or(false),
                    expected_state_revision,
                    auth,
                )
                .await
            }

            ToolCall::SkillActivate {
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("skill_activate requires a resolved Project"),
                };
                self.skill_activate(
                    &project,
                    skill_key,
                    package_revision,
                    expected_state_revision,
                    idempotency_key,
                    auth,
                )
                .await
            }

            ToolCall::SkillRemoveRevision {
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => {
                        return ToolResult::err("skill_remove_revision requires a resolved Project")
                    }
                };
                self.skill_remove_revision(
                    &project,
                    skill_key,
                    package_revision,
                    expected_state_revision,
                    idempotency_key,
                    auth,
                )
                .await
            }

            ToolCall::CreateGoal {
                title,
                objective,
                controller_agent_id,
                completion_conditions,
                steps,
                idempotency_key,
            } => self.create_goal_with_plan(
                auth,
                crate::db::NewGoal {
                    title,
                    objective,
                    controller_agent_id,
                    completion_conditions,
                    idempotency_key,
                    steps: steps
                        .into_iter()
                        .map(|step| crate::db::NewGoalStep {
                            id: step.id,
                            title: step.title,
                        })
                        .collect(),
                },
            ),

            ToolCall::CheckpointGoal {
                goal_id,
                expected_revision,
                completed_step_ids,
                current_step_id,
                summary,
                idempotency_key,
            } => self.checkpoint_goal(
                auth,
                goal_id,
                expected_revision,
                crate::db::GoalCheckpoint {
                    completed_step_ids,
                    current_step_id,
                    summary,
                },
                idempotency_key,
            ),

            ToolCall::GetGoal { goal_id } => self.get_goal(auth, goal_id),

            ToolCall::PresentGoalPlan { goal_id } => self.present_goal_plan(auth, goal_id).await,

            ToolCall::GoalPlanSync { goal_id } => {
                self.goal_plan_sync_for_window(auth, window, goal_id).await
            }

            ToolCall::ListGoals {
                lifecycle,
                offset,
                limit,
            } => self.list_goals(
                auth,
                lifecycle.map(|value| value.as_str().to_string()),
                offset,
                limit,
            ),

            ToolCall::UpdateGoal {
                goal_id,
                expected_revision,
                title,
                objective,
                controller_agent_id,
                lifecycle,
                terminal_reason,
                idempotency_key,
            } => self.update_goal_with_controller(
                auth,
                goal_id,
                expected_revision,
                title,
                objective,
                controller_agent_id,
                lifecycle.map(|value| value.as_str().to_string()),
                terminal_reason,
                idempotency_key,
            ),

            ToolCall::AssociateGoalAgentTask {
                goal_id,
                task_id,
                idempotency_key,
            } => self.associate_goal_agent_task(auth, goal_id, task_id, idempotency_key),

            ToolCall::AssociateGoalWorkflowSession {
                goal_id,
                session_id,
                idempotency_key,
            } => {
                self.associate_goal_workflow_session(auth, goal_id, session_id, idempotency_key)
                    .await
            }

            ToolCall::WaitForAgentEvents {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                mode,
                goal_id,
                events,
                idempotency_key,
            } => self.wait_for_agent_events(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                mode,
                goal_id,
                events,
                idempotency_key,
            ),

            ToolCall::ReadAgentWait { wait_id } => self.read_agent_wait(auth, wait_id),

            ToolCall::CancelAgentWait {
                wait_id,
                idempotency_key,
            } => self.cancel_agent_wait(auth, wait_id, idempotency_key),

            ToolCall::AgentWaitState { wait_id } => self.agent_wait_state(auth, wait_id),

            ToolCall::CreateAgentTask {
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            } => self.create_agent_task(
                auth,
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            ),

            ToolCall::ListAgentTasks {
                assignee_agent_id,
                offset,
                limit,
            } => self.list_agent_tasks(auth, assignee_agent_id, offset, limit),

            ToolCall::ReadAgentTask { task_id } => self.read_agent_task(auth, task_id),

            ToolCall::AssignAgentTask {
                task_id,
                assignee_agent_id,
            } => self.assign_agent_task(auth, task_id, assignee_agent_id),

            ToolCall::StartAgentTaskAttempt {
                task_id,
                assignee_agent_id,
                idempotency_key,
            } => self.start_agent_task_attempt(auth, task_id, assignee_agent_id, idempotency_key),

            ToolCall::StartAgentTaskEndpointContinuation {
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            } => self.start_agent_task_endpoint_continuation(
                auth,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            ),

            ToolCall::StartAgentTaskCodingRun {
                project,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                provider_id,
                config,
                timeout_secs,
            } => {
                // Keep this relatively large orchestration future off the shared dispatch
                // future's inline state so unrelated tool calls do not inherit its stack cost.
                Box::pin(self.start_agent_task_coding_run(
                    auth,
                    project,
                    task_id,
                    attempt_id,
                    assignee_agent_id,
                    attempt_fence,
                    attempt_controller_generation,
                    provider_id,
                    config,
                    timeout_secs,
                ))
                .await
            }

            ToolCall::ReconcileAgentTaskCodingRun {
                task_id,
                attempt_id,
            } => Box::pin(self.reconcile_agent_task_coding_run(auth, task_id, attempt_id)).await,

            ToolCall::HeartbeatAgentTaskAttempt {
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                active_turn_wake_id,
                active_turn_consume_token,
            } => self.heartbeat_agent_task_attempt(
                auth,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                active_turn_wake_id,
                active_turn_consume_token,
            ),

            ToolCall::CompleteAgentTaskAttempt {
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                outcome,
                terminal_result,
                terminal_reason,
                completion_key,
            } => self.complete_agent_task_attempt(
                auth,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                outcome,
                terminal_result,
                terminal_reason,
                completion_key,
            ),

            ToolCall::CreateAgentIdentity {
                handle,
                display_name,
                description,
                specialty_labels,
                idempotency_key,
            } => self.create_agent_identity(
                auth,
                handle,
                display_name,
                description,
                specialty_labels,
                idempotency_key,
            ),

            ToolCall::ListAgentIdentities {
                agent_id,
                offset,
                limit,
            } => self.list_agent_identities(auth, agent_id, offset, limit),

            ToolCall::UpdateAgentIdentity {
                agent_id,
                expected_profile_revision,
                handle,
                display_name,
                description,
                specialty_labels,
            } => self.update_agent_identity(
                auth,
                agent_id,
                expected_profile_revision,
                handle,
                display_name,
                description,
                specialty_labels,
            ),

            ToolCall::RotateAgentContinuationEndpoint {
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            }
            | ToolCall::AttachAgentEndpoint {
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            } => self.attach_agent_endpoint(
                auth,
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            ),

            ToolCall::PresentAgentContinuation {
                agent_id,
                endpoint_id,
                expected_controller_generation,
            } => self.present_agent_continuation(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
            ),

            ToolCall::AgentContinuationBind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_bind_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationRecoverEndpoint {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_recover_endpoint_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationState {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_state_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationWakeAcquire {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_wake_acquire_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::AgentContinuationWakePrepare {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
            } => self.agent_continuation_wake_prepare_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
            ),

            ToolCall::AgentContinuationWakeFinish {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
                outcome,
            } => self.agent_continuation_wake_finish_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
                wake_id,
                attempt_id,
                outcome,
            ),

            ToolCall::AgentContinuationUnbind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            } => self.agent_continuation_unbind_for_window(
                auth,
                window,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                binding_id,
            ),

            ToolCall::PresentJobTerminalContinuation { wait_id } => {
                self.present_job_terminal_continuation(auth, wait_id).await
            }

            ToolCall::JobTerminalContinuationBind {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_bind_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationState {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_state_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationPrepare {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_prepare_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::JobTerminalContinuationFinish {
                wait_id,
                binding_id,
                attempt_id,
                outcome,
            } => {
                self.job_terminal_continuation_finish_for_window(
                    auth, window, wait_id, binding_id, attempt_id, outcome,
                )
                .await
            }

            ToolCall::JobTerminalContinuationUnbind {
                wait_id,
                binding_id,
            } => {
                self.job_terminal_continuation_unbind_for_window(auth, window, wait_id, binding_id)
                    .await
            }

            ToolCall::DetachAgentEndpoint { endpoint_id } => {
                self.detach_agent_endpoint(auth, endpoint_id)
            }

            ToolCall::CreateConversation {
                title,
                agent_ids,
                idempotency_key,
            } => self.create_conversation(auth, title, agent_ids, idempotency_key),

            ToolCall::ListConversations {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                offset,
                limit,
            } => self.list_conversations(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                offset,
                limit,
            ),

            ToolCall::ReadConversation {
                conversation_id,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_seq,
                limit,
            } => self.read_conversation(
                auth,
                conversation_id,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_seq,
                limit,
            ),

            ToolCall::PostConversationMessage {
                conversation_id,
                body,
                author_agent_id,
                endpoint_id,
                expected_controller_generation,
                recipient_agent_ids,
                reply_to,
                idempotency_key,
                wake_reply_id,
                reply_operation_index,
            } => self.post_conversation_message(
                auth,
                conversation_id,
                body,
                author_agent_id,
                endpoint_id,
                expected_controller_generation,
                recipient_agent_ids,
                reply_to,
                idempotency_key,
                wake_reply_id,
                reply_operation_index,
            ),

            ToolCall::ListAgentInbox {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_delivery_order,
                limit,
            } => self.list_agent_inbox(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_delivery_order,
                limit,
            ),

            ToolCall::ConsumeAgentDeliveries {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                delivery_ids,
            } => self.consume_agent_deliveries(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                delivery_ids,
            ),

            ToolCall::BootstrapAgentConversation {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                conversation_id,
                wake_id,
                activation_idempotency_key,
            } => self.bootstrap_agent_conversation(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                conversation_id,
                wake_id,
                activation_idempotency_key,
            ),

            ToolCall::ConsumeAgentWake {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                consume_token,
            } => self.consume_agent_wake(
                auth,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                consume_token,
            ),

            ToolCall::MemorySearch {
                query,
                tags,
                offset,
                limit,
                expected_catalog_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_search requires a resolved Project"),
                };
                self.memory_search(
                    &project,
                    query,
                    tags,
                    offset,
                    limit,
                    expected_catalog_revision,
                )
            }

            ToolCall::MemoryRead {
                memory_key,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_read requires a resolved Project"),
                };
                self.memory_read(&project, memory_key, expected_revision)
            }

            ToolCall::MemorySet {
                memory_key,
                summary,
                body,
                priority,
                bootstrap,
                tags,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_set requires a resolved Project"),
                };
                self.memory_set(
                    &project,
                    memory_key,
                    summary,
                    body,
                    priority,
                    bootstrap,
                    tags,
                    expected_revision,
                    auth,
                )
            }

            ToolCall::MemoryDelete {
                memory_key,
                expected_revision,
                ..
            } => {
                let project = match project_resolution {
                    Some(Ok(project)) => project,
                    Some(Err(error)) => return error.into_tool_result(),
                    None => return ToolResult::err("memory_delete requires a resolved Project"),
                };
                self.memory_delete(&project, memory_key, expected_revision)
            }

            ToolCall::MemoryScopeList { offset, limit } => {
                self.memory_scope_list(auth, offset, limit).await
            }

            ToolCall::MemoryScopePurge {
                memory_scope_id,
                expected_catalog_revision,
                confirm,
            } => {
                self.memory_scope_purge(auth, memory_scope_id, expected_catalog_revision, confirm)
                    .await
            }

            call @ ToolCall::ImportConversationFilesToProject { .. } => {
                self.dispatch_conversation_import_tool(call, auth, transport)
                    .await
            }

            call @ (ToolCall::DeleteProjectFiles { .. }
            | ToolCall::ReadFiles { .. }
            | ToolCall::ListProjectFiles { .. }
            | ToolCall::ListProjectTrackedFiles { .. }
            | ToolCall::ProjectOverview { .. }
            | ToolCall::SearchProjectTexts { .. }
            | ToolCall::SearchAndRead { .. }
            | ToolCall::WriteProjectFile { .. }
            | ToolCall::SaveProjectArtifact { .. }
            | ToolCall::TransferProjectArtifact { .. }
            | ToolCall::ProjectArtifact { .. }
            | ToolCall::ReadProjectArtifactMetadata { .. }
            | ToolCall::ReadProjectArtifact { .. }
            | ToolCall::ArtifactUploadBegin { .. }
            | ToolCall::ArtifactUploadChunk { .. }
            | ToolCall::ArtifactUploadFinish { .. }
            | ToolCall::ArtifactUploadAbort { .. }
            | ToolCall::ApplyTextEdits { .. }) => {
                self.dispatch_file_tool(call, transport, project_resolution, auth)
                    .await
            }

            call @ (ToolCall::GitRestorePaths { .. }
            | ToolCall::DiscardUntracked { .. }
            | ToolCall::GitCommitPaths { .. }
            | ToolCall::GitStatus { .. }
            | ToolCall::GitDiffHunks { .. }
            | ToolCall::GitReviewSummary { .. }
            | ToolCall::GitLog { .. }
            | ToolCall::ShowChanges { .. }) => self.dispatch_git_tool(call).await,

            call @ (ToolCall::CargoFmt { .. }
            | ToolCall::CargoCheck { .. }
            | ToolCall::CargoTest { .. }
            | ToolCall::GoTest { .. }) => self.dispatch_cargo_tool(call, ssh_resource, auth).await,

            call @ (ToolCall::RunJob { .. }
            | ToolCall::StopJob { .. }
            | ToolCall::ObserveJobs { .. }
            | ToolCall::WaitForJobTerminal { .. }
            | ToolCall::ListJobs { .. }
            | ToolCall::JobTail { .. }) => self.dispatch_job_tool(call, auth, ssh_resource).await,

            call @ ToolCall::WorkspaceHygieneCheck { .. } => self.dispatch_hygiene_tool(call).await,

            call @ (ToolCall::LspStatus { .. }
            | ToolCall::DocumentSymbols { .. }
            | ToolCall::DocumentDiagnostics { .. }
            | ToolCall::Hover { .. }
            | ToolCall::WorkspaceSymbols { .. }
            | ToolCall::GotoDefinition { .. }
            | ToolCall::FindReferences { .. }
            | ToolCall::CallHierarchy { .. }) => self.dispatch_lsp_tool(call).await,
        }
    }
}

#[cfg(test)]
mod structured_execution_sparse_projection_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn project_execution_binding_preserves_specialized_selector_semantics() {
        let mut shell = ToolCall::RunShell {
            project: "~p7".to_string(),
            command: "true".to_string(),
            session_id: None,
            timeout_secs: None,
            sync_wait_secs: None,
            cwd: None,
            purpose: None,
            shell: None,
        };
        let (shell_project, shell_output) = canonical_execution_project_binding(&mut shell)
            .expect("legacy shell execution needs canonical binding");
        assert_eq!(shell_output, CanonicalProjectOutput::Canonical);
        shell_project.clone_from(&"agent:special:webcodex".to_string());
        assert_eq!(shell.project(), Some("agent:special:webcodex"));

        let mut hygiene = ToolCall::WorkspaceHygieneCheck {
            project: "~p7".to_string(),
            max_findings: None,
            include_tracked: None,
            session_id: None,
        };
        let (_, hygiene_output) = canonical_execution_project_binding(&mut hygiene).unwrap();
        assert_eq!(hygiene_output, CanonicalProjectOutput::Requested);

        let mut show_changes = ToolCall::ShowChanges {
            project: "~p7".to_string(),
            session_id: None,
            include_diff: Some(false),
            max_hunks: None,
            max_hunk_lines: None,
            session_event_limit: None,
        };
        let (_, show_changes_output) =
            canonical_execution_project_binding(&mut show_changes).unwrap();
        assert_eq!(show_changes_output, CanonicalProjectOutput::Requested);

        let mut work_on_project = ToolCall::WorkOnProject {
            project: "~p7".to_string(),
            client_id: None,
            path: None,
            mode: None,
            base_ref: None,
            instruction: "inspect".to_string(),
            guidance_profile: CodingGuidanceProfile::default(),
            include_extension_catalog: false,
            session_id: None,
        };
        assert!(canonical_execution_project_binding(&mut work_on_project).is_none());
        assert_eq!(work_on_project.project(), Some("~p7"));

        let mut work_result = ToolCall::WorkResultState {
            project: "demo".to_string(),
            session_id: "wc_sess_x".to_string(),
        };
        assert!(canonical_execution_project_binding(&mut work_result).is_none());
        assert_eq!(work_result.project(), Some("demo"));
    }

    fn terminal_process_result(execution_source: &str) -> ToolResult {
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
