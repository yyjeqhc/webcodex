use super::kernel::ToolTransport;
use super::orchestration_host::{
    CanonicalOrchestrationHost, OrchestrationPolicy, OrchestrationToolResponse,
};
use super::{ResolvedProject, ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde_json::{json, Value};
use std::sync::Arc;
use webcodex_code_mode::{
    CodeModeError, CodeModeErrorKind, CodeModeExecuteRequest, CodeModeHost, CodeModeHostError,
    CodeModeHostFuture, CodeModeTerminationMode, CodeModeToolRequest, CodeModeToolResponse,
};

pub(crate) use super::orchestration_host::OrchestrationCompositionSummary as CodeModeCompositionSummary;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeModeCallableStage {
    ReadOnly,
    Validation,
    GuardedEdit,
}

impl CodeModeCallableStage {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::Validation => "validation",
            Self::GuardedEdit => "guarded_edit",
        }
    }

    pub(crate) const fn entry_tool(self) -> &'static str {
        match self {
            Self::ReadOnly => "code_mode_exec",
            Self::Validation => "code_mode_exec_effectful",
            Self::GuardedEdit => "code_mode_exec_mutating",
        }
    }
}

pub(crate) fn code_mode_callable_stage_for_entry_tool(
    tool_name: &str,
) -> Option<CodeModeCallableStage> {
    match tool_name {
        "code_mode_exec" => Some(CodeModeCallableStage::ReadOnly),
        "code_mode_exec_effectful" => Some(CodeModeCallableStage::Validation),
        "code_mode_exec_mutating" => Some(CodeModeCallableStage::GuardedEdit),
        _ => None,
    }
}

/// E1 admission is intentionally explicit. A future tool becoming read-only does
/// not opt it into Code Mode automatically.
pub(crate) const READ_ONLY_NESTED_TOOLS: &[&str] = &[
    "read_files",
    "search_project_texts",
    "project_overview",
    "list_project_tracked_files",
    "git_status",
    "git_log",
    "git_diff_hunks",
    "git_review_summary",
    "show_changes",
];

pub(crate) const E2A_NESTED_TOOLS: &[&str] = &[
    "read_files",
    "search_project_texts",
    "project_overview",
    "list_project_tracked_files",
    "git_status",
    "git_log",
    "git_diff_hunks",
    "git_review_summary",
    "show_changes",
    "cargo_check",
    "cargo_test",
];

pub(crate) const E2C_NESTED_TOOLS: &[&str] = &[
    "read_files",
    "search_project_texts",
    "project_overview",
    "list_project_tracked_files",
    "git_status",
    "git_log",
    "git_diff_hunks",
    "git_review_summary",
    "show_changes",
    "apply_text_edits",
    "cargo_check",
    "cargo_test",
];

const RECURSIVE_CODE_MODE_TOOLS: &[&str] = &[
    "code_mode_exec",
    "code_mode_exec_effectful",
    "code_mode_exec_mutating",
];

const CODE_MODE_E1_POLICY: OrchestrationPolicy = OrchestrationPolicy {
    frontend: "code_mode_v8",
    policy_name: "Code Mode E1",
    admitted_tools: READ_ONLY_NESTED_TOOLS,
    denied_tools: RECURSIVE_CODE_MODE_TOOLS,
    additional_forbidden_argument_fields: &[],
    nested_sync_wait_max_secs: None,
    max_mutation_calls: None,
    validation_after_mutation: false,
};

const CODE_MODE_E2A_POLICY: OrchestrationPolicy = OrchestrationPolicy {
    frontend: "code_mode_v8_effectful",
    policy_name: "Code Mode E2a",
    admitted_tools: E2A_NESTED_TOOLS,
    denied_tools: RECURSIVE_CODE_MODE_TOOLS,
    additional_forbidden_argument_fields: &[],
    nested_sync_wait_max_secs: Some(5),
    max_mutation_calls: None,
    validation_after_mutation: false,
};

const CODE_MODE_E2C_POLICY: OrchestrationPolicy = OrchestrationPolicy {
    frontend: "code_mode_v8_mutating",
    policy_name: "Code Mode E2c",
    admitted_tools: E2C_NESTED_TOOLS,
    denied_tools: RECURSIVE_CODE_MODE_TOOLS,
    additional_forbidden_argument_fields: &[],
    nested_sync_wait_max_secs: Some(5),
    max_mutation_calls: Some(1),
    validation_after_mutation: true,
};

pub(crate) const fn code_mode_orchestration_policy(
    stage: CodeModeCallableStage,
) -> OrchestrationPolicy {
    match stage {
        CodeModeCallableStage::ReadOnly => CODE_MODE_E1_POLICY,
        CodeModeCallableStage::Validation => CODE_MODE_E2A_POLICY,
        CodeModeCallableStage::GuardedEdit => CODE_MODE_E2C_POLICY,
    }
}

pub(crate) fn is_admitted_nested_tool(tool_name: &str) -> bool {
    CODE_MODE_E1_POLICY.is_admitted(tool_name)
}

pub(crate) const MAX_MODEL_ERROR_BYTES: usize = 16 * 1024;
const CONSEQUENTIAL_MAX_STARTED_CHILD_DRAIN_MS: u64 = 5_000;

fn bounded_model_error(message: &str) -> String {
    if message.len() <= MAX_MODEL_ERROR_BYTES {
        return message.to_string();
    }
    let suffix = "…";
    let mut end = MAX_MODEL_ERROR_BYTES.saturating_sub(suffix.len());
    while end > 0 && !message.is_char_boundary(end) {
        end -= 1;
    }
    let mut bounded = message[..end].to_string();
    bounded.push_str(suffix);
    bounded
}

fn code_mode_recovery(error: &CodeModeError, job_handoffs: usize, outcome_unknown: usize) -> Value {
    let mut actions = Vec::new();
    let primary = match error.kind {
        CodeModeErrorKind::InvalidRequest => "fix_code_mode_request",
        CodeModeErrorKind::Runtime => "fix_code_mode_source",
        CodeModeErrorKind::ChildCallFailed => match error
            .child_failure
            .as_ref()
            .map(|failure| failure.failure_kind.as_str())
        {
            Some("invalid_arguments") => "fix_child_arguments",
            Some("insufficient_scope") => "obtain_required_scope",
            Some("tool_not_admitted" | "composition_policy_denied") => {
                "remove_or_replace_child_call"
            }
            Some("mutation_budget_exceeded") => "reduce_mutation_attempts",
            Some("frontend_closed") => "fix_code_mode_source",
            _ => "inspect_child_host_failure",
        },
        CodeModeErrorKind::Timeout => "reduce_or_bound_code_mode_work",
        CodeModeErrorKind::ToolCallBudgetExceeded => "reduce_child_calls",
        CodeModeErrorKind::OutputLimitExceeded => "reduce_text_projection",
    };
    actions.push(primary);
    if job_handoffs > 0 {
        actions.push("observe_existing_job_continuations");
    }
    if outcome_unknown > 0 {
        actions.push("reconcile_effect_state_before_retry");
    }
    actions.dedup();
    json!({
        "retry_same_call_unchanged": false,
        "actions": actions,
    })
}

fn attach_effect_follow_up(output: &mut Value, job_handoffs: usize, outcome_unknown: usize) {
    let mut actions = Vec::new();
    if job_handoffs > 0 {
        actions.push("observe_existing_job_continuations");
    }
    if outcome_unknown > 0 {
        actions.push("reconcile_effect_state_before_retry");
    }
    if !actions.is_empty() {
        output["recovery"] = json!({"retry_same_call_unchanged": false, "actions": actions});
    }
}

fn code_mode_failure_output(
    error: &CodeModeError,
    message: String,
    job_handoffs: usize,
    outcome_unknown: usize,
) -> Value {
    let mut output = json!({
        "failure_kind": error.kind.as_str(),
        "message": message,
        "stats": error.stats,
        "recovery": code_mode_recovery(error, job_handoffs, outcome_unknown),
    });
    if let Some(child_failure) = error.child_failure.as_ref() {
        output["child_failure"] = json!(child_failure);
    }
    if let Some(limit) = error.limit.as_ref() {
        output["limit"] = json!(limit);
    }
    output
}

/// Thin V8 frontend adapter. Authority, Project/Session injection, canonical
/// dispatch, child evidence, and composition accounting live in the reusable
/// CanonicalOrchestrationHost rather than in the JavaScript runtime adapter.
struct V8CodeModeHost {
    orchestration: Arc<CanonicalOrchestrationHost>,
}

impl CodeModeHost for V8CodeModeHost {
    fn invoke_tool(
        &self,
        request: CodeModeToolRequest,
    ) -> CodeModeHostFuture<'_, Result<CodeModeToolResponse, CodeModeHostError>> {
        Box::pin(async move {
            self.orchestration
                .invoke_tool(request.ordinal, request.tool_name, request.arguments)
                .await
                .map(
                    |OrchestrationToolResponse {
                         success,
                         output,
                         error,
                     }| CodeModeToolResponse {
                        success,
                        output,
                        error,
                    },
                )
                .map_err(|error| {
                    let failure_kind = error.failure_kind().as_str();
                    CodeModeHostError::with_kind(failure_kind, error.into_message())
                })
        })
    }

    fn stop_accepting_calls(&self) {
        self.orchestration.stop_accepting_nested_calls();
    }
}

impl ToolRuntime {
    pub(crate) async fn code_mode_exec(
        &self,
        project: ResolvedProject,
        session_id: String,
        source: String,
        timeout_ms: Option<u64>,
        auth: Option<&AuthContext>,
        transport: super::sessions::SessionTransport,
        composition_parent_invocation_id: Option<String>,
    ) -> (ToolResult, CodeModeCompositionSummary) {
        let transport = match transport {
            super::sessions::SessionTransport::Api => ToolTransport::Api,
            super::sessions::SessionTransport::Mcp => ToolTransport::Mcp,
        };
        let policy = code_mode_orchestration_policy(CodeModeCallableStage::ReadOnly);
        let orchestration = Arc::new(CanonicalOrchestrationHost::new(
            self.clone(),
            auth,
            project.resolved_id,
            session_id,
            transport,
            composition_parent_invocation_id.clone(),
            policy,
        ));
        let host = Arc::new(V8CodeModeHost {
            orchestration: Arc::clone(&orchestration),
        });
        let input_bytes = source.len();
        let execution = webcodex_code_mode::execute(
            host as Arc<dyn CodeModeHost>,
            CodeModeExecuteRequest {
                source,
                allowed_tools: policy
                    .admitted_tools
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                timeout_ms,
            },
        )
        .await;
        let (result, stats) = match execution {
            Ok(execution) => {
                let stats = execution.stats.clone();
                (
                    ToolResult::ok(json!({
                        "content": execution.content,
                        "stats": execution.stats,
                    })),
                    stats,
                )
            }
            Err(error) => {
                let stats = error.stats.clone();
                (
                    ToolResult::err_with_output(
                        "code mode execution failed",
                        code_mode_failure_output(&error, bounded_model_error(&error.message), 0, 0),
                    ),
                    stats,
                )
            }
        };
        let composition = orchestration.composition_summary(
            stats.duration_ms,
            input_bytes,
            stats.returned_bytes,
            stats.slot_wait_ms,
        );
        super::runtime_metrics::observe_code_mode_composition(self.metrics.as_ref(), &composition);
        tracing::debug!(
            composition_parent_invocation_id = composition_parent_invocation_id
                .as_deref()
                .unwrap_or("unavailable"),
            nested_calls = composition.nested_calls,
            nested_successes = composition.nested_successes,
            nested_failures = composition.nested_failures,
            max_in_flight = composition.max_in_flight,
            duration_ms = composition.duration_ms,
            slot_wait_ms = composition.slot_wait_ms,
            input_bytes = composition.input_bytes,
            returned_bytes = composition.returned_bytes,
            nested_raw_result_bytes_total = composition.nested_raw_result_bytes_total,
            nested_tool_counts = ?composition.nested_tool_counts,
            "code_mode_composition_finished"
        );
        (result, composition)
    }

    pub(crate) async fn code_mode_exec_effectful(
        &self,
        project: ResolvedProject,
        session_id: String,
        source: String,
        timeout_ms: Option<u64>,
        auth: Option<&AuthContext>,
        transport: super::sessions::SessionTransport,
        composition_parent_invocation_id: Option<String>,
    ) -> (ToolResult, CodeModeCompositionSummary) {
        let transport = match transport {
            super::sessions::SessionTransport::Api => ToolTransport::Api,
            super::sessions::SessionTransport::Mcp => ToolTransport::Mcp,
        };
        let policy = code_mode_orchestration_policy(CodeModeCallableStage::Validation);
        let orchestration = Arc::new(CanonicalOrchestrationHost::new(
            self.clone(),
            auth,
            project.resolved_id,
            session_id,
            transport,
            composition_parent_invocation_id.clone(),
            policy,
        ));
        let host = Arc::new(V8CodeModeHost {
            orchestration: Arc::clone(&orchestration),
        });
        let input_bytes = source.len();
        let execution = webcodex_code_mode::execute_with_termination_mode(
            host as Arc<dyn CodeModeHost>,
            CodeModeExecuteRequest {
                source,
                allowed_tools: policy
                    .admitted_tools
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                timeout_ms,
            },
            CodeModeTerminationMode::DrainStartedChildren {
                max_drain_ms: CONSEQUENTIAL_MAX_STARTED_CHILD_DRAIN_MS,
            },
        )
        .await;
        let effect_receipt = orchestration.effect_receipt();
        let has_consequential_work = effect_receipt.consequential_calls > 0;
        let (result, stats) = match execution {
            Ok(execution) => {
                let stats = execution.stats.clone();
                let mut output = json!({
                    "content": execution.content,
                    "stats": execution.stats,
                });
                if has_consequential_work {
                    output["effect_receipt"] = json!(effect_receipt);
                    attach_effect_follow_up(
                        &mut output,
                        effect_receipt.job_handoffs,
                        effect_receipt.outcome_unknown,
                    );
                }
                (ToolResult::ok(output), stats)
            }
            Err(error) => {
                let stats = error.stats.clone();
                let message = if has_consequential_work {
                    bounded_model_error(&format!(
                        "{} One or more consequential child calls entered canonical execution. Do not blindly rerun the whole orchestration; inspect the effect receipt and any returned Job continuations.",
                        error.message
                    ))
                } else {
                    bounded_model_error(&error.message)
                };
                let mut output = code_mode_failure_output(
                    &error,
                    message,
                    effect_receipt.job_handoffs,
                    effect_receipt.outcome_unknown,
                );
                if has_consequential_work {
                    output["effect_receipt"] = json!(effect_receipt);
                }
                (
                    ToolResult::err_with_output("effectful code mode execution failed", output),
                    stats,
                )
            }
        };
        let composition = orchestration.composition_summary(
            stats.duration_ms,
            input_bytes,
            stats.returned_bytes,
            stats.slot_wait_ms,
        );
        super::runtime_metrics::observe_code_mode_composition(self.metrics.as_ref(), &composition);
        tracing::debug!(
            composition_parent_invocation_id = composition_parent_invocation_id
                .as_deref()
                .unwrap_or("unavailable"),
            nested_calls = composition.nested_calls,
            nested_successes = composition.nested_successes,
            nested_failures = composition.nested_failures,
            max_in_flight = composition.max_in_flight,
            duration_ms = composition.duration_ms,
            slot_wait_ms = composition.slot_wait_ms,
            input_bytes = composition.input_bytes,
            returned_bytes = composition.returned_bytes,
            nested_raw_result_bytes_total = composition.nested_raw_result_bytes_total,
            nested_tool_counts = ?composition.nested_tool_counts,
            consequential_calls = composition.consequential_calls,
            known_results = composition.known_results,
            job_handoffs = composition.job_handoffs,
            outcome_unknown = composition.outcome_unknown,
            "code_mode_effectful_composition_finished"
        );
        (result, composition)
    }
    pub(crate) async fn code_mode_exec_mutating(
        &self,
        project: ResolvedProject,
        session_id: String,
        source: String,
        timeout_ms: Option<u64>,
        auth: Option<&AuthContext>,
        transport: super::sessions::SessionTransport,
        composition_parent_invocation_id: Option<String>,
    ) -> (ToolResult, CodeModeCompositionSummary) {
        let transport = match transport {
            super::sessions::SessionTransport::Api => ToolTransport::Api,
            super::sessions::SessionTransport::Mcp => ToolTransport::Mcp,
        };
        let policy = code_mode_orchestration_policy(CodeModeCallableStage::GuardedEdit);
        let orchestration = Arc::new(CanonicalOrchestrationHost::new(
            self.clone(),
            auth,
            project.resolved_id,
            session_id,
            transport,
            composition_parent_invocation_id.clone(),
            policy,
        ));
        let host = Arc::new(V8CodeModeHost {
            orchestration: Arc::clone(&orchestration),
        });
        let input_bytes = source.len();
        let execution = webcodex_code_mode::execute_with_termination_mode(
            host as Arc<dyn CodeModeHost>,
            CodeModeExecuteRequest {
                source,
                allowed_tools: policy
                    .admitted_tools
                    .iter()
                    .map(|tool| (*tool).to_string())
                    .collect(),
                timeout_ms,
            },
            CodeModeTerminationMode::DrainStartedChildren {
                max_drain_ms: CONSEQUENTIAL_MAX_STARTED_CHILD_DRAIN_MS,
            },
        )
        .await;
        let effect_receipt = orchestration.effect_receipt();
        let has_consequential_work = effect_receipt.consequential_calls > 0;
        let (result, stats) = match execution {
            Ok(execution) => {
                let stats = execution.stats.clone();
                let mut output = json!({
                    "content": execution.content,
                    "stats": execution.stats,
                });
                if has_consequential_work {
                    output["effect_receipt"] = json!(effect_receipt);
                    attach_effect_follow_up(
                        &mut output,
                        effect_receipt.job_handoffs,
                        effect_receipt.outcome_unknown,
                    );
                }
                (ToolResult::ok(output), stats)
            }
            Err(error) => {
                let stats = error.stats.clone();
                let message = if has_consequential_work {
                    bounded_model_error(&format!(
                        "{} One or more consequential child calls entered canonical execution. Do not blindly rerun the whole orchestration; inspect the effect receipt and current workspace state before another mutation.",
                        error.message
                    ))
                } else {
                    bounded_model_error(&error.message)
                };
                let mut output = code_mode_failure_output(
                    &error,
                    message,
                    effect_receipt.job_handoffs,
                    effect_receipt.outcome_unknown,
                );
                if has_consequential_work {
                    output["effect_receipt"] = json!(effect_receipt);
                }
                (
                    ToolResult::err_with_output("mutating code mode execution failed", output),
                    stats,
                )
            }
        };
        let composition = orchestration.composition_summary(
            stats.duration_ms,
            input_bytes,
            stats.returned_bytes,
            stats.slot_wait_ms,
        );
        super::runtime_metrics::observe_code_mode_composition(self.metrics.as_ref(), &composition);
        tracing::debug!(
            composition_parent_invocation_id = composition_parent_invocation_id
                .as_deref()
                .unwrap_or("unavailable"),
            nested_calls = composition.nested_calls,
            nested_successes = composition.nested_successes,
            nested_failures = composition.nested_failures,
            max_in_flight = composition.max_in_flight,
            duration_ms = composition.duration_ms,
            slot_wait_ms = composition.slot_wait_ms,
            input_bytes = composition.input_bytes,
            returned_bytes = composition.returned_bytes,
            nested_raw_result_bytes_total = composition.nested_raw_result_bytes_total,
            nested_tool_counts = ?composition.nested_tool_counts,
            consequential_calls = composition.consequential_calls,
            known_results = composition.known_results,
            job_handoffs = composition.job_handoffs,
            outcome_unknown = composition.outcome_unknown,
            "code_mode_mutating_composition_finished"
        );
        (result, composition)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webcodex_tool_contracts::{lookup_tool_definition, ToolEffect, ToolRisk};

    #[test]
    fn e1_allowlist_remains_canonically_read_only() {
        for tool in READ_ONLY_NESTED_TOOLS {
            let definition = lookup_tool_definition(tool)
                .unwrap_or_else(|| panic!("missing canonical definition for {tool}"));
            let metadata = definition.metadata();
            assert_eq!(metadata.effect, ToolEffect::Observe, "{tool}");
            assert_eq!(metadata.risk, ToolRisk::Read, "{tool}");
            assert!(!definition.is_shell_like(), "{tool}");
            assert!(!definition.is_write_like(), "{tool}");
            assert!(!definition.requires_permission(), "{tool}");
        }
        assert!(READ_ONLY_NESTED_TOOLS.contains(&"git_review_summary"));
        assert!(READ_ONLY_NESTED_TOOLS.contains(&"show_changes"));
        assert!(!READ_ONLY_NESTED_TOOLS.contains(&"code_mode_exec"));
        assert!(!READ_ONLY_NESTED_TOOLS.contains(&"run_shell"));
    }

    #[test]
    fn callable_stage_projection_keys_reuse_exact_execution_admission() {
        assert_eq!(
            code_mode_orchestration_policy(CodeModeCallableStage::ReadOnly).admitted_tools,
            READ_ONLY_NESTED_TOOLS
        );
        assert_eq!(
            &E2A_NESTED_TOOLS[..READ_ONLY_NESTED_TOOLS.len()],
            READ_ONLY_NESTED_TOOLS
        );
        assert_eq!(
            &E2A_NESTED_TOOLS[READ_ONLY_NESTED_TOOLS.len()..],
            ["cargo_check", "cargo_test"]
        );
        assert_eq!(
            &E2C_NESTED_TOOLS[..READ_ONLY_NESTED_TOOLS.len()],
            READ_ONLY_NESTED_TOOLS
        );
        assert_eq!(
            &E2C_NESTED_TOOLS[READ_ONLY_NESTED_TOOLS.len()..],
            ["apply_text_edits", "cargo_check", "cargo_test"]
        );
        for stage in [
            CodeModeCallableStage::ReadOnly,
            CodeModeCallableStage::Validation,
            CodeModeCallableStage::GuardedEdit,
        ] {
            let policy = code_mode_orchestration_policy(stage);
            assert_eq!(
                code_mode_callable_stage_for_entry_tool(stage.entry_tool()),
                Some(stage)
            );
            assert!(policy
                .admitted_tools
                .iter()
                .all(|tool| !RECURSIVE_CODE_MODE_TOOLS.contains(tool)));
        }
    }

    #[test]
    fn nested_target_and_wrapper_fields_are_server_owned() {
        for field in [
            "project",
            "session_id",
            "recording_session_id",
            "ack_session_message_ids",
            "context_request",
            "session_message_resolution",
            "_control",
            "expected_failure",
            "expected_failure_kind",
            "result_expectation",
            "accepted_exit_codes",
            "assertion_name",
            "__webcodex_private",
        ] {
            assert!(
                super::super::orchestration_host::is_server_owned_orchestration_argument(field),
                "{field}"
            );
        }
    }
}
