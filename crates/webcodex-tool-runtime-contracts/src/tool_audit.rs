//! Audit-safe argument summaries for runtime tool calls.

use serde_json::Value;
use sha2::{Digest, Sha256};
use webcodex_core::audit_preview::{command_preview, process_preview};
use webcodex_core::runner_protocol::{
    normalize_cargo_packages, normalize_cargo_value, normalize_rust_test_filter,
};
use webcodex_core::workflow_session_contract::is_validation_like_execution_purpose;
#[cfg(test)]
use webcodex_tool_contracts::tool_call::ComputerSnapshotRegion;
use webcodex_tool_contracts::tool_call::{
    BrowserActToolCall, BrowserObserveToolCall, ComputerControlToolCall, ComputerObserveToolCall,
    ToolCall,
};
#[cfg(feature = "workspace-checkpoints")]
use webcodex_tool_contracts::tool_inputs::{is_checkpoint_kind, is_checkpoint_validation_status};
use webcodex_workflow_session::SessionExecutionContext;

mod execution_identity;
mod request;
mod result;
use execution_identity::{
    insert_structured_validation_target, normalized_exact_git_commit_for_audit,
};
pub use execution_identity::{
    run_process_validation_identity, run_script_validation_identity, GenericValidationIdentity,
};
use request::*;
pub use result::{canonical_execution_audit_result_for_tool, session_log_result_for_tool};
pub use webcodex_core::validation_identity::{
    assertion_validation_identity, is_structured_validation_target_identity,
    is_validation_execution_identity, structured_validation_target_identity,
};

pub fn session_log_arguments_for_tool_request(tool_name: &str, arguments: &Value) -> Value {
    let Ok(call) = ToolCall::from_tool_name(tool_name, arguments.clone()) else {
        // Malformed requests fail closed. Raw input is never filtered, retried,
        // or used as an audit fallback.
        return empty_audit_projection();
    };
    session_log_arguments_for_typed_call(tool_name, &call)
}

pub fn session_log_arguments_for_typed_call(tool_name: &str, call: &ToolCall) -> Value {
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(tool_name) else {
        return empty_audit_projection();
    };
    let request_policy = definition.audit_policy().request;
    if !matches!(
        request_policy,
        webcodex_tool_contracts::ToolAuditRequestPolicy::Typed
            | webcodex_tool_contracts::ToolAuditRequestPolicy::TypedDropNullValues
    ) {
        return empty_audit_projection();
    }

    debug_assert_eq!(call.tool_name(), tool_name);
    let mut projected = call.session_log_arguments();
    if request_policy == webcodex_tool_contracts::ToolAuditRequestPolicy::TypedDropNullValues {
        if let Some(projected) = projected.as_object_mut() {
            projected.retain(|_, value| !value.is_null());
        }
    }
    projected
}

fn empty_audit_projection() -> Value {
    serde_json::json!({})
}

#[cfg(test)]
#[path = "tool_audit/tests/canonical_execution.rs"]
mod canonical_execution_audit_projection_tests;

fn bounded_completion_key_fingerprint(value: Option<&str>) -> Value {
    let Some(value) = value else {
        return Value::Null;
    };
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 {
        return Value::String("invalid".to_string());
    }
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.session-message-completion.v1\0");
    hasher.update(value.as_bytes());
    Value::String(format!("{:x}", hasher.finalize()))
}

fn copy_keys(
    obj: &serde_json::Map<String, Value>,
    out: &mut serde_json::Map<String, Value>,
    keys: &[&str],
) {
    for key in keys {
        if let Some(value) = obj.get(*key).cloned() {
            out.insert((*key).to_string(), value);
        }
    }
}

#[cfg(test)]
#[path = "tool_audit/tests/execution_identity.rs"]
mod execution_purpose_classification_tests;

#[cfg(test)]
#[path = "tool_audit/tests/computer_privacy.rs"]
mod computer_privacy_tests;

#[cfg(test)]
#[path = "tool_audit/tests/browser_privacy.rs"]
mod browser_privacy_tests;

/// Audit-safe projection over the canonical typed request.
///
/// This policy intentionally remains outside the structural input contract: it
/// consumes ToolCall but never reparses raw request JSON or defines accepted fields.
pub trait ToolCallAuditProjection {
    fn session_log_arguments(&self) -> Value;
}

impl ToolCallAuditProjection for ToolCall {
    fn session_log_arguments(&self) -> Value {
        match self {
            Self::OpenWebcodexWorkbench { .. } => serde_json::json!({"workbench_open":true}),
            Self::SearchWebcodexResources {
                kind,
                offset,
                limit,
                ..
            } => serde_json::json!({"kind":kind,"offset":offset,"limit":limit}),
            Self::ReadWebcodexResource { .. } => serde_json::json!({"resource_read":true}),

            #[cfg(feature = "experimental-code-mode")]
            Self::CodeModeExec {
                project,
                source,
                timeout_ms,
                ..
            }
            | Self::CodeModeExecEffectful {
                project,
                source,
                timeout_ms,
                ..
            }
            | Self::CodeModeExecMutating {
                project,
                source,
                timeout_ms,
                ..
            } => serde_json::json!({
                "project": project,
                "source_bytes": source.len(),
                "timeout_ms": timeout_ms,
            }),
            Self::JobWriteInput {
                project,
                job_id,
                input_id,
                data,
                close,
            } => serde_json::json!({
                "project":project,"job_id":job_id,"input_id_present":!input_id.is_empty(),
                "input_bytes":data.len(),"close":close,
            }),
            Self::RunProcess {
                interactive,
                project,
                executable,
                args,
                stdin,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                ..
            } => {
                let identity = if *interactive {
                    None
                } else {
                    run_process_validation_identity(
                        executable,
                        args,
                        stdin.as_deref(),
                        cwd.as_deref(),
                        purpose.as_ref().map(|purpose| purpose.as_str()),
                    )
                };
                let mut value = serde_json::json!({
                    "project": project,
                    "executable_present": true,
                    "interactive": interactive,
                    "arg_count": args.len(),
                    "stdin_present": stdin.is_some(),
                    "process_summary": process_preview(
                        executable,
                        args.iter().map(String::as_str),
                    ),
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                    "cwd": cwd,
                    "purpose": purpose,
                });
                if let Some(identity) = identity {
                    value["execution_identity"] = serde_json::json!(identity.identity);
                    if identity.validation_tool.is_some() {
                        value["validation_target_id"] = value["execution_identity"].clone();
                        value["validation_tool"] = serde_json::json!(identity.validation_tool);
                    }
                }
                value
            }
            Self::CodingAgentStart {
                project,
                provider_id,
                idempotency_key,
                instruction,
                context_session_id,
                config,
                timeout_secs,
                recording_session_id: _,
            } => serde_json::json!({
                "project": project,
                "provider_id": provider_id,
                "idempotency_key_present": !idempotency_key.is_empty(),
                "instruction_bytes": instruction.len(),
                "context_session_present": context_session_id.is_some(),
                "config_count": config.as_ref().map(std::collections::BTreeMap::len).unwrap_or_default(),
                "timeout_secs": timeout_secs,
            }),
            Self::CodingAgentObserve {
                run_id,
                after_observation_token,
                wait_secs,
            } => serde_json::json!({
                "run_id": run_id,
                "token_present": after_observation_token.is_some(),
                "wait_secs": wait_secs,
            }),
            Self::CodingAgentCancel { run_id } => serde_json::json!({
                "run_id": run_id,
            }),
            Self::RunScript {
                project,
                language,
                script,
                args,
                stdin,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                ..
            } => {
                let identity = run_script_validation_identity(
                    language.as_str(),
                    script,
                    args,
                    stdin.as_deref(),
                    cwd.as_deref(),
                    purpose.as_ref().map(|purpose| purpose.as_str()),
                );
                let mut value = serde_json::json!({
                    "project": project,
                    "language": language,
                    "script_bytes": script.len(),
                    "arg_count": args.len(),
                    "stdin_present": stdin.is_some(),
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                    "cwd": cwd,
                    "purpose": purpose,
                });
                if let Some(identity) = identity {
                    value["execution_identity"] = serde_json::json!(identity.identity);
                    if identity.validation_tool.is_some() {
                        value["validation_target_id"] = value["execution_identity"].clone();
                        value["validation_tool"] = serde_json::json!(identity.validation_tool);
                    }
                }
                value
            }
            Self::RunShell {
                project,
                command,
                timeout_secs,
                cwd,
                purpose,
                shell,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "command_summary": command_preview(command),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
                "shell": shell,
            }),
            Self::RunJob {
                project,
                command,
                timeout_secs,
                cwd,
                purpose,
                shell,
                ..
            } => serde_json::json!({
                "project": project,
                "command_present": true,
                "command_summary": command_preview(command),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
                "shell": shell,
            }),
            Self::OpenSessionShell {
                project,
                session_id,
                cwd,
                shell,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "cwd": cwd,
                "shell": shell,
            }),
            Self::SessionShellExec {
                project,
                session_id,
                shell_id,
                command,
                timeout_secs,
                purpose,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "shell_id": shell_id,
                "command_present": true,
                "command_summary": command_preview(command),
                "timeout_secs": timeout_secs,
                "purpose": purpose,
            }),
            Self::SessionShellStatus {
                project,
                session_id,
                shell_id,
            }
            | Self::CloseSessionShell {
                project,
                session_id,
                shell_id,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "shell_id": shell_id,
            }),
            Self::BrowserObserve(call) => browser_observe_audit_projection(call),
            Self::BrowserAct(call) => browser_act_audit_projection(call),
            Self::ComputerObserve(call) => computer_observe_audit_projection(call),
            Self::ComputerControl(call) => computer_control_audit_projection(call),
            Self::ComputerSaveSnapshot {
                project,
                path,
                client_id,
                surface_id,
                region,
                max_width,
                max_height,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "client_id": client_id,
                "surface_id": surface_id,
                "region_present": region.is_some(),
                "max_width": max_width,
                "max_height": max_height,
            }),
            Self::StopJob {
                project,
                job_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "job_id": job_id,
                "confirm": confirm,
            }),
            Self::ObserveJobs {
                items,
                tail_lines,
                wait_secs,
                wake_on,
                summary_only,
            } => serde_json::json!({
                "summary_only": summary_only,
                "item_count": items.len(),
                "token_count": items
                    .iter()
                    .filter(|item| item.after_observation_token.is_some())
                    .count(),
                "observation_ref_count": items
                    .iter()
                    .filter(|item| item.observation_ref.is_some())
                    .count(),
                "job_ids": items
                    .iter()
                    .filter_map(|item| (!item.job_id.is_empty()).then_some(item.job_id.as_str()))
                    .collect::<Vec<_>>(),
                "tail_lines": tail_lines,
                "wait_secs": wait_secs,
                "wake_on": wake_on,
            }),
            Self::WaitForJobReadiness {
                job_ids,
                mode,
                wait_secs,
            } => serde_json::json!({
                "mode": mode,
                "requested_jobs": job_ids.len(),
                "unique_jobs": job_ids.iter().collect::<std::collections::HashSet<_>>().len(),
                "wait_secs": wait_secs,
            }),
            Self::WaitForJobTerminal { job_id, .. } => serde_json::json!({
                "job_id": job_id,
            }),
            Self::PresentJobTerminalContinuation { wait_id }
            | Self::JobTerminalContinuationBind { wait_id, .. }
            | Self::JobTerminalContinuationState { wait_id, .. }
            | Self::JobTerminalContinuationPrepare { wait_id, .. }
            | Self::JobTerminalContinuationUnbind { wait_id, .. } => serde_json::json!({
                "wait_id": wait_id,
            }),
            Self::JobTerminalContinuationFinish {
                wait_id,
                attempt_id,
                outcome,
                ..
            } => serde_json::json!({
                "wait_id": wait_id,
                "attempt_id": attempt_id,
                "outcome": outcome,
            }),
            Self::ApplyUnifiedDiff {
                project,
                deny_sensitive_paths,
                ..
            } => serde_json::json!({
                "project": project,
                "diff_present": true,
                "deny_sensitive_paths": deny_sensitive_paths,
            }),
            Self::DeleteProjectFiles { project, paths, .. }
            | Self::GitRestorePaths { project, paths, .. }
            | Self::DiscardUntracked { project, paths, .. } => serde_json::json!({
                "project": project,
                "paths": paths,
            }),
            Self::GitCommitPaths {
                project,
                expected_head,
                paths,
                ..
            } => {
                let expected_head = normalized_exact_git_commit_for_audit(expected_head);
                serde_json::json!({
                    "project": project,
                    "paths": paths,
                    "expected_head_valid": expected_head.is_some(),
                    "expected_head": expected_head,
                    "message_present": true,
                })
            }
            Self::GitStatus { project, .. } => serde_json::json!({
                "project": project,
            }),
            Self::GitReviewSummary {
                project,
                base_commit,
                head_commit,
                ..
            } => {
                let base_commit = normalized_exact_git_commit_for_audit(base_commit);
                let head_commit = normalized_exact_git_commit_for_audit(head_commit);
                serde_json::json!({
                    "project": project,
                    "base_commit_valid": base_commit.is_some(),
                    "base_commit": base_commit,
                    "head_commit_valid": head_commit.is_some(),
                    "head_commit": head_commit,
                })
            }
            Self::ReviewChanges {
                project,
                scope,
                paths,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                continuation,
                ..
            } => serde_json::json!({
                "project": project,
                "scope": scope,
                "paths": paths,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "max_page_bytes": max_page_bytes,
                "continuation_present": continuation.is_some(),
            }),
            Self::GitLog {
                project,
                head_commit,
                limit,
                skip,
                ..
            } => {
                let head_commit = head_commit
                    .as_deref()
                    .and_then(normalized_exact_git_commit_for_audit);
                serde_json::json!({
                    "project": project,
                    "head_commit_valid": head_commit.is_some(),
                    "head_commit": head_commit,
                    "limit": limit,
                    "skip": skip,
                })
            }
            Self::GitDiffHunks {
                project,
                paths,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                cached,
                base_commit,
                head_commit,
                continuation,
                ..
            } => {
                let base_commit = base_commit
                    .as_deref()
                    .and_then(normalized_exact_git_commit_for_audit);
                let head_commit = head_commit
                    .as_deref()
                    .and_then(normalized_exact_git_commit_for_audit);
                let mut out = serde_json::json!({
                    "project": project,
                    "paths": paths,
                    "max_hunks": max_hunks,
                    "max_hunk_lines": max_hunk_lines,
                    "max_page_bytes": max_page_bytes,
                    "cached": cached,
                    "base_commit_valid": base_commit.is_some(),
                    "head_commit_valid": head_commit.is_some(),
                    "continuation_present": continuation.is_some(),
                });
                if let Some(base_commit) = base_commit {
                    out["base_commit"] = Value::String(base_commit);
                }
                if let Some(head_commit) = head_commit {
                    out["head_commit"] = Value::String(head_commit);
                }
                out
            }
            Self::ProjectBuild {
                project,
                cwd,
                adapter,
                scope,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "adapter": adapter,
                "packages_present": scope.as_ref().is_some_and(|scope| !scope.packages.is_empty()),
                "package_count": scope.as_ref().map(|scope| scope.packages.len()).unwrap_or_default(),
                "all_packages": scope.as_ref().is_some_and(|scope| scope.all_packages),
                "timeout_secs": timeout_secs,
            }),
            Self::ProjectValidate {
                project,
                cwd,
                action,
                adapter,
                scope,
                test,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "action": action,
                "adapter": adapter,
                "packages_present": scope.as_ref().is_some_and(|scope| !scope.packages.is_empty()),
                "package_count": scope.as_ref().map(|scope| scope.packages.len()).unwrap_or_default(),
                "all_packages": scope.as_ref().is_some_and(|scope| scope.all_packages),
                "test_options_present": test.is_some(),
                "filter_present": test.as_ref().is_some_and(|options| options.filter.is_some()),
                "require_tests": test.as_ref().and_then(|options| options.require_tests),
                "min_tests": test.as_ref().and_then(|options| options.min_tests),
                "timeout_secs": timeout_secs,
            }),
            Self::CargoFmt {
                project,
                cwd,
                check,
                timeout_secs,
                sync_wait_secs,
                ..
            } => typed_structured_validation_request_audit(
                StructuredValidationRequestAudit::CargoFmt,
                &serde_json::json!({
                    "project": project,
                    "cwd": cwd,
                    "check": check,
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                }),
            ),
            Self::CargoCheck {
                project,
                cwd,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                packages,
                timeout_secs,
                sync_wait_secs,
                ..
            } => typed_structured_validation_request_audit(
                StructuredValidationRequestAudit::CargoCheck,
                &serde_json::json!({
                    "project": project,
                    "cwd": cwd,
                    "all_targets": all_targets,
                    "all_features": all_features,
                    "no_default_features": no_default_features,
                    "features": features,
                    "package": package,
                    "packages": packages,
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                }),
            ),
            Self::CargoTest {
                project,
                cwd,
                filter,
                lib,
                all_targets,
                all_features,
                no_default_features,
                features,
                package,
                no_run,
                require_tests,
                min_tests,
                timeout_secs,
                sync_wait_secs,
                ..
            } => typed_structured_validation_request_audit(
                StructuredValidationRequestAudit::CargoTest,
                &serde_json::json!({
                    "project": project,
                    "cwd": cwd,
                    "filter": filter,
                    "lib": lib,
                    "all_targets": all_targets,
                    "all_features": all_features,
                    "no_default_features": no_default_features,
                    "features": features,
                    "package": package,
                    "no_run": no_run,
                    "require_tests": require_tests,
                    "min_tests": min_tests,
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                }),
            ),
            Self::GoTest {
                project,
                cwd,
                packages,
                timeout_secs,
                sync_wait_secs,
                ..
            } => typed_structured_validation_request_audit(
                StructuredValidationRequestAudit::GoTest,
                &serde_json::json!({
                    "project": project,
                    "cwd": cwd,
                    "packages": packages,
                    "timeout_secs": timeout_secs,
                    "sync_wait_secs": sync_wait_secs,
                }),
            ),
            Self::ReadFiles {
                project,
                items,
                with_line_numbers,
                ..
            } => serde_json::json!({
                "project": project,
                "items": items,
                "with_line_numbers": with_line_numbers,
            }),
            Self::PrepareGoalWorkflow {
                session_id,
                title,
                objective,
                controller_agent_id,
                idempotency_key,
                ..
            } => typed_goal_request_audit(
                GoalRequestAudit::Prepare,
                &serde_json::json!({
                    "session_id": session_id,
                    "title": title,
                    "objective": objective,
                    "controller_agent_id": controller_agent_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::CreateGoal {
                title,
                objective,
                controller_agent_id,
                idempotency_key,
                ..
            } => typed_goal_request_audit(
                GoalRequestAudit::Create,
                &serde_json::json!({
                    "title": title,
                    "objective": objective,
                    "controller_agent_id": controller_agent_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::CheckpointGoal {
                goal_id,
                expected_revision,
                completed_step_ids,
                current_step_id,
                summary,
                idempotency_key,
            } => serde_json::json!({
                "goal_id": goal_id,
                "expected_revision": expected_revision,
                "completed_step_count": completed_step_ids.len(),
                "current_step_present": current_step_id.is_some(),
                "summary_bytes": summary.len(),
                "idempotency_key_present": !idempotency_key.is_empty(),
            }),
            Self::GetGoal { goal_id } => typed_goal_request_audit(
                GoalRequestAudit::Get,
                &serde_json::json!({"goal_id": goal_id}),
            ),
            Self::PresentGoalPlan { goal_id } | Self::GoalPlanSync { goal_id } => {
                typed_goal_request_audit(
                    GoalRequestAudit::Get,
                    &serde_json::json!({"goal_id": goal_id}),
                )
            }
            Self::ListGoals {
                query: _,
                lifecycle,
                offset,
                limit,
            } => typed_goal_request_audit(
                GoalRequestAudit::List,
                &serde_json::json!({
                    "lifecycle": lifecycle,
                    "offset": offset,
                    "limit": limit,
                }),
            ),
            Self::UpdateGoal {
                goal_id,
                expected_revision,
                title,
                objective,
                controller_agent_id,
                lifecycle,
                terminal_reason,
                idempotency_key,
            } => typed_goal_request_audit(
                GoalRequestAudit::Update,
                &serde_json::json!({
                    "goal_id": goal_id,
                    "expected_revision": expected_revision,
                    "title": title,
                    "objective": objective,
                    "controller_agent_id": controller_agent_id,
                    "lifecycle": lifecycle,
                    "terminal_reason": terminal_reason,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::AssociateGoalAgentTask {
                goal_id,
                task_id,
                idempotency_key,
            } => typed_goal_request_audit(
                GoalRequestAudit::AssociateAgentTask,
                &serde_json::json!({
                    "goal_id": goal_id,
                    "task_id": task_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::AssociateGoalWorkflowSession {
                goal_id,
                session_id,
                idempotency_key,
            } => typed_goal_request_audit(
                GoalRequestAudit::AssociateWorkflowSession,
                &serde_json::json!({
                    "goal_id": goal_id,
                    "session_id": session_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::WaitForAgentEvents {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                mode,
                events,
                goal_id,
                idempotency_key,
            } => serde_json::json!({
                "agent_id": agent_id,
                "endpoint_id": endpoint_id,
                "expected_controller_generation": expected_controller_generation,
                "mode": mode.as_str(),
                "goal_id": goal_id,
                "event_count": events.len(),
                "idempotency_key_present": !idempotency_key.is_empty(),
            }),
            Self::ReadAgentWait { wait_id } | Self::AgentWaitState { wait_id } => {
                serde_json::json!({"wait_id": wait_id})
            }
            Self::CancelAgentWait {
                wait_id,
                idempotency_key,
            } => serde_json::json!({
                "wait_id": wait_id,
                "idempotency_key_present": !idempotency_key.is_empty(),
            }),
            Self::CreateAgentTask {
                title,
                instruction,
                assignee_agent_id,
                source_conversation_id,
                source_message_id,
                referenced_project_id,
                idempotency_key,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::Create,
                &serde_json::json!({
                    "title": title,
                    "instruction": instruction,
                    "assignee_agent_id": assignee_agent_id,
                    "source_conversation_id": source_conversation_id,
                    "source_message_id": source_message_id,
                    "referenced_project_id": referenced_project_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::ListAgentTasks {
                assignee_agent_id,
                offset,
                limit,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::List,
                &serde_json::json!({
                    "assignee_agent_id": assignee_agent_id,
                    "offset": offset,
                    "limit": limit,
                }),
            ),
            Self::ReadAgentTask { task_id } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::Read,
                &serde_json::json!({"task_id": task_id}),
            ),
            Self::AssignAgentTask {
                task_id,
                assignee_agent_id,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::Assign,
                &serde_json::json!({
                    "task_id": task_id,
                    "assignee_agent_id": assignee_agent_id,
                }),
            ),
            Self::StartAgentTaskAttempt {
                task_id,
                assignee_agent_id,
                idempotency_key,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::StartAttempt,
                &serde_json::json!({
                    "task_id": task_id,
                    "assignee_agent_id": assignee_agent_id,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::StartAgentTaskEndpointContinuation {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::StartEndpointContinuation,
                &serde_json::json!({
                    "attempt_ref": attempt_ref,
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                    "assignee_agent_id": assignee_agent_id,
                    "attempt_fence": attempt_fence,
                    "attempt_controller_generation": attempt_controller_generation,
                }),
            ),
            Self::StartAgentTaskCodingRun {
                project,
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                provider_id,
                config,
                timeout_secs,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::StartCodingRun,
                &serde_json::json!({
                    "project": project,
                    "attempt_ref": attempt_ref,
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                    "assignee_agent_id": assignee_agent_id,
                    "attempt_fence": attempt_fence,
                    "attempt_controller_generation": attempt_controller_generation,
                    "provider_id": provider_id,
                    "config": config,
                    "timeout_secs": timeout_secs,
                }),
            ),
            Self::ReconcileAgentTaskCodingRun {
                task_id,
                attempt_id,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::ReconcileCodingRun,
                &serde_json::json!({
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                }),
            ),
            Self::HeartbeatAgentTaskAttempt {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                active_turn_wake_id,
                active_turn_consume_token,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::HeartbeatAttempt,
                &serde_json::json!({
                    "attempt_ref": attempt_ref,
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                    "assignee_agent_id": assignee_agent_id,
                    "attempt_fence": attempt_fence,
                    "attempt_controller_generation": attempt_controller_generation,
                    "active_turn_wake_id": active_turn_wake_id,
                    "active_turn_consume_token": active_turn_consume_token,
                }),
            ),
            Self::CompleteAgentTaskAttempt {
                attempt_ref,
                task_id,
                attempt_id,
                assignee_agent_id,
                attempt_fence,
                attempt_controller_generation,
                outcome,
                terminal_result,
                terminal_reason,
                completion_key,
            } => typed_agent_task_request_audit(
                AgentTaskRequestAudit::CompleteAttempt,
                &serde_json::json!({
                    "attempt_ref": attempt_ref,
                    "task_id": task_id,
                    "attempt_id": attempt_id,
                    "assignee_agent_id": assignee_agent_id,
                    "attempt_fence": attempt_fence,
                    "attempt_controller_generation": attempt_controller_generation,
                    "outcome": outcome,
                    "terminal_result": terminal_result,
                    "terminal_reason": terminal_reason,
                    "completion_key": completion_key,
                }),
            ),
            Self::CreateAgentIdentity {
                handle,
                display_name,
                description,
                specialty_labels,
                idempotency_key,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::CreateIdentity,
                &serde_json::json!({
                    "handle": handle,
                    "display_name": display_name,
                    "description": description,
                    "specialty_labels": specialty_labels,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::ListAgentIdentities {
                agent_id,
                offset,
                limit,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ListIdentities,
                &serde_json::json!({"agent_id": agent_id, "offset": offset, "limit": limit}),
            ),
            Self::UpdateAgentIdentity {
                agent_id,
                expected_profile_revision,
                handle,
                display_name,
                description,
                specialty_labels,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::UpdateIdentity,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "expected_profile_revision": expected_profile_revision,
                    "handle": handle,
                    "display_name": display_name,
                    "description": description,
                    "specialty_labels": specialty_labels,
                }),
            ),
            Self::RotateAgentContinuationEndpoint {
                agent_id,
                host,
                client_attachment_id,
                idempotency_key,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::AttachEndpoint,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "host": host,
                    "client_attachment_id": client_attachment_id,
                    "idempotency_key": idempotency_key,
                }),
            ),

            Self::PresentAgentContinuation {
                agent_continuation_ref,
                agent_id,
                endpoint_id,
                expected_controller_generation,
            } => serde_json::json!({
                "agent_continuation_ref": agent_continuation_ref,
                "agent_id": agent_id,
                "endpoint_id": endpoint_id,
                "expected_controller_generation": expected_controller_generation,
            }),
            Self::AgentContinuationBind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                ..
            }
            | Self::AgentContinuationRecoverEndpoint {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                ..
            }
            | Self::AgentContinuationState {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                ..
            }
            | Self::AgentContinuationWakeAcquire {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                ..
            }
            | Self::AgentContinuationUnbind {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                ..
            } => serde_json::json!({
                "agent_id": agent_id,
                "endpoint_id": endpoint_id,
                "expected_controller_generation": expected_controller_generation,
            }),
            Self::AgentContinuationWakePrepare {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                attempt_id,
                ..
            } => serde_json::json!({
                "agent_id": agent_id,
                "endpoint_id": endpoint_id,
                "expected_controller_generation": expected_controller_generation,
                "wake_id": wake_id,
                "attempt_id": attempt_id,
            }),
            Self::AgentContinuationWakeFinish {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                attempt_id,
                outcome,
                ..
            } => serde_json::json!({
                "agent_id": agent_id,
                "endpoint_id": endpoint_id,
                "expected_controller_generation": expected_controller_generation,
                "wake_id": wake_id,
                "attempt_id": attempt_id,
                "outcome": outcome,
            }),
            Self::DetachAgentEndpoint { endpoint_id } => typed_communication_request_audit(
                CommunicationRequestAudit::DetachEndpoint,
                &serde_json::json!({"endpoint_id": endpoint_id}),
            ),
            Self::CreateConversation {
                title,
                agent_ids,
                idempotency_key,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::CreateConversation,
                &serde_json::json!({
                    "title": title,
                    "agent_ids": agent_ids,
                    "idempotency_key": idempotency_key,
                }),
            ),
            Self::ListConversations {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                offset,
                limit,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ListConversations,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "offset": offset,
                    "limit": limit,
                }),
            ),
            Self::ReadConversation {
                conversation_id,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_seq,
                limit,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ReadConversation,
                &serde_json::json!({
                    "conversation_id": conversation_id,
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "after_seq": after_seq,
                    "limit": limit,
                }),
            ),
            Self::PostConversationMessage {
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
            } => typed_communication_request_audit(
                CommunicationRequestAudit::PostMessage,
                &serde_json::json!({
                    "conversation_id": conversation_id,
                    "body": body,
                    "author_agent_id": author_agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "recipient_agent_ids": recipient_agent_ids,
                    "reply_to": reply_to,
                    "idempotency_key": idempotency_key,
                    "wake_reply_id": wake_reply_id,
                    "reply_operation_index": reply_operation_index,
                }),
            ),
            Self::ListAgentInbox {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                after_delivery_order,
                limit,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ListInbox,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "after_delivery_order": after_delivery_order,
                    "limit": limit,
                }),
            ),
            Self::ConsumeAgentDeliveries {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                delivery_ids,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ConsumeDeliveries,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "delivery_ids": delivery_ids,
                }),
            ),
            Self::BootstrapAgentConversation {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                conversation_id,
                wake_id,
                activation_idempotency_key,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::BootstrapConversation,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "conversation_id": conversation_id,
                    "wake_id": wake_id,
                    "activation_idempotency_key": activation_idempotency_key,
                }),
            ),
            Self::ConsumeAgentWake {
                agent_id,
                endpoint_id,
                expected_controller_generation,
                wake_id,
                consume_token,
            } => typed_communication_request_audit(
                CommunicationRequestAudit::ConsumeWake,
                &serde_json::json!({
                    "agent_id": agent_id,
                    "endpoint_id": endpoint_id,
                    "expected_controller_generation": expected_controller_generation,
                    "wake_id": wake_id,
                    "consume_token_present": !consume_token.is_empty(),
                }),
            ),
            Self::MemorySearch {
                project,
                query,
                tags,
                offset,
                limit,
                expected_catalog_revision,
                session_id,
            } => typed_memory_request_audit(
                MemoryRequestAudit::Search,
                &serde_json::json!({
                    "project": project,
                    "query": query,
                    "tags": tags,
                    "offset": offset,
                    "limit": limit,
                    "expected_catalog_revision": expected_catalog_revision,
                    "session_id": session_id,
                }),
            ),
            Self::MemoryRead {
                project,
                memory_key,
                expected_revision,
                session_id,
            } => typed_memory_request_audit(
                MemoryRequestAudit::Read,
                &serde_json::json!({
                    "project": project,
                    "memory_key": memory_key,
                    "expected_revision": expected_revision,
                    "session_id": session_id,
                }),
            ),
            Self::MemorySet {
                project,
                memory_key,
                summary,
                body,
                priority,
                bootstrap,
                tags,
                expected_revision,
                session_id,
            } => typed_memory_request_audit(
                MemoryRequestAudit::Set,
                &serde_json::json!({
                    "project": project,
                    "memory_key": memory_key,
                    "summary": summary,
                    "body": body,
                    "priority": priority,
                    "bootstrap": bootstrap,
                    "tags": tags,
                    "expected_revision": expected_revision,
                    "session_id": session_id,
                }),
            ),
            Self::MemoryDelete {
                project,
                memory_key,
                expected_revision,
                session_id,
            } => typed_memory_request_audit(
                MemoryRequestAudit::Delete,
                &serde_json::json!({
                    "project": project,
                    "memory_key": memory_key,
                    "expected_revision": expected_revision,
                    "session_id": session_id,
                }),
            ),
            Self::MemoryScopeList { offset, limit } => typed_memory_request_audit(
                MemoryRequestAudit::ScopeList,
                &serde_json::json!({"offset": offset, "limit": limit}),
            ),
            Self::MemoryScopePurge {
                memory_scope_id,
                expected_catalog_revision,
                ..
            } => typed_memory_request_audit(
                MemoryRequestAudit::ScopePurge,
                &serde_json::json!({
                    "memory_scope_id": memory_scope_id,
                    "expected_catalog_revision": expected_catalog_revision,
                }),
            ),
            Self::SkillLoad { project, name, .. } => serde_json::json!({
                "project": project,
                "name_present": !name.is_empty(),
            }),
            Self::RunSkillResource {
                project,
                skill_id,
                path,
                expected_definition_revision,
                expected_package_revision,
                args,
                timeout_secs,
                sync_wait_secs,
                cwd,
                purpose,
                ..
            } => serde_json::json!({
                "project": project,
                "skill_id": skill_id,
                "path": path,
                "expected_definition_revision": expected_definition_revision,
                "expected_package_revision": expected_package_revision,
                "arg_count": args.len(),
                "timeout_secs": timeout_secs,
                "sync_wait_secs": sync_wait_secs,
                "cwd": cwd,
                "purpose": purpose,
            }),
            Self::SkillList {
                project,
                query,
                offset,
                limit,
                expected_catalog_revision,
                ..
            } => serde_json::json!({
                "project": project,
                "query_present": query.as_ref().is_some_and(|value| !value.is_empty()),
                "offset": offset,
                "limit": limit,
                "expected_catalog_revision": expected_catalog_revision,
            }),
            Self::SkillReadFile {
                project,
                skill_id,
                path,
                start_line,
                limit,
                expected_definition_revision,
                expected_package_revision,
                ..
            } => serde_json::json!({
                "project": project,
                "skill_id": skill_id,
                "path": path,
                "start_line": start_line,
                "limit": limit,
                "expected_definition_revision": expected_definition_revision,
                "expected_package_revision": expected_package_revision,
            }),
            Self::SkillVersions {
                project,
                skill_key,
                offset,
                limit,
                session_id,
            } => typed_skill_request_audit(
                SkillRequestAudit::Versions,
                &serde_json::json!({
                    "project": project,
                    "skill_key": skill_key,
                    "offset": offset,
                    "limit": limit,
                    "session_id": session_id,
                }),
            ),
            Self::SkillInstall {
                project,
                skill_key,
                artifact_path,
                expected_artifact_sha256,
                idempotency_key,
                activate,
                expected_state_revision,
                session_id,
            } => typed_skill_request_audit(
                SkillRequestAudit::Install,
                &serde_json::json!({
                    "project": project,
                    "skill_key": skill_key,
                    "artifact_path": artifact_path,
                    "expected_artifact_sha256": expected_artifact_sha256,
                    "idempotency_key": idempotency_key,
                    "activate": activate,
                    "expected_state_revision": expected_state_revision,
                    "session_id": session_id,
                }),
            ),
            Self::SkillActivate {
                project,
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                session_id,
            } => typed_skill_request_audit(
                SkillRequestAudit::Activate,
                &serde_json::json!({
                    "project": project,
                    "skill_key": skill_key,
                    "package_revision": package_revision,
                    "expected_state_revision": expected_state_revision,
                    "idempotency_key": idempotency_key,
                    "session_id": session_id,
                }),
            ),
            Self::SkillRemoveRevision {
                project,
                skill_key,
                package_revision,
                expected_state_revision,
                idempotency_key,
                session_id,
            } => typed_skill_request_audit(
                SkillRequestAudit::RemoveRevision,
                &serde_json::json!({
                    "project": project,
                    "skill_key": skill_key,
                    "package_revision": package_revision,
                    "expected_state_revision": expected_state_revision,
                    "idempotency_key": idempotency_key,
                    "session_id": session_id,
                }),
            ),
            Self::ListProjectFiles {
                project,
                path,
                limit,
                offset,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "limit": limit,
                "offset": offset,
            }),
            Self::ProjectOverview {
                project,
                path,
                max_depth,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "max_depth": max_depth,
                "limit": limit,
            }),
            Self::SearchProjectTexts {
                project, queries, ..
            } => serde_json::json!({
                "project": project,
                "query_count": queries.len(),
                "patterns_present": !queries.is_empty(),
            }),
            Self::SearchAndRead {
                project,
                read_before,
                read_after,
                max_reads,
                with_line_numbers,
                ..
            } => serde_json::json!({
                "project": project,
                "query_present": true,
                "read_before": read_before,
                "read_after": read_after,
                "max_reads": max_reads,
                "with_line_numbers": with_line_numbers,
            }),
            Self::LspStatus { project, .. } => serde_json::json!({
                "project": project,
            }),
            Self::DocumentSymbols {
                project,
                path,
                limit,
                ..
            }
            | Self::DocumentDiagnostics {
                project,
                path,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "limit": limit,
            }),
            Self::Hover {
                project,
                path,
                line,
                column,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
            }),
            Self::WorkspaceSymbols { project, limit, .. } => serde_json::json!({
                "project": project,
                "query_present": true,
                "limit": limit,
            }),
            Self::GotoDefinition {
                project,
                path,
                line,
                column,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "limit": limit,
            }),
            Self::FindReferences {
                project,
                path,
                line,
                column,
                include_declaration,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "include_declaration": include_declaration,
                "limit": limit,
            }),
            Self::CallHierarchy {
                project,
                path,
                line,
                column,
                direction,
                depth,
                limit,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "line": line,
                "column": column,
                "direction": direction,
                "depth": depth,
                "limit": limit,
            }),
            Self::ShowChanges {
                project,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
                ..
            } => serde_json::json!({
                "project": project,
                "include_diff": include_diff,
                "max_hunks": max_hunks,
                "max_hunk_lines": max_hunk_lines,
                "session_event_limit": session_event_limit,
            }),
            Self::WriteProjectFile {
                project,
                path,
                overwrite,
                expected_read_revision,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_present": true,
                "overwrite": overwrite,
                "expected_read_revision_present": expected_read_revision.is_some(),
            }),
            Self::SaveProjectArtifact {
                project,
                path,
                mime_type,
                overwrite,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "content_base64_present": true,
                "mime_type": mime_type,
                "overwrite": overwrite,
            }),
            Self::ImportConversationFilesToProject {
                project,
                openai_file_id_refs,
                output_dir,
                targets,
                overwrite,
                session_id,
                ..
            } => serde_json::json!({
                "project": project,
                "file_count": openai_file_id_refs.len(),
                "output_dir": output_dir,
                "targets_count": targets.as_ref().map(Vec::len).unwrap_or_default(),
                "overwrite": overwrite,
                "session_id": session_id,
            }),
            Self::TransferProjectArtifact {
                source_project,
                source_path,
                destination_project,
                destination_path,
                overwrite,
            } => serde_json::json!({
                "source_project": source_project,
                "source_path": source_path,
                "destination_project": destination_project,
                "destination_path": destination_path,
                "overwrite": overwrite,
            }),
            Self::AcceptArtifactHandoff {
                grant_id,
                destination_project,
                destination_path,
                overwrite,
                idempotency_key,
            } => serde_json::json!({
                "grant_id": grant_id,
                "destination_project": destination_project,
                "destination_path": destination_path,
                "overwrite": overwrite,
                "idempotency_key_present": !idempotency_key.is_empty(),
            }),
            Self::PresentSpreadsheet { project, path } => serde_json::json!({
                "project": project,
                "path": path,
            }),
            Self::ProjectArtifact {
                project,
                path,
                action,
                allow_missing,
                offset,
                length,
                expected_sha256,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "action": action.as_str(),
                "allow_missing": allow_missing,
                "offset": offset,
                "length": length,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::ReadProjectArtifactMetadata {
                project,
                path,
                allow_missing,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "allow_missing": allow_missing,
            }),
            Self::ReadProjectArtifact {
                project,
                path,
                encoding,
                offset,
                length,
                expected_sha256,
                as_image,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "encoding": encoding,
                "offset": offset,
                "length": length,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "as_image": as_image,
            }),
            Self::ArtifactUploadBegin {
                project,
                path,
                expected_bytes,
                expected_sha256,
                mime_type,
                overwrite,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "expected_bytes": expected_bytes,
                "expected_sha256_present": expected_sha256.as_ref().is_some_and(|v| !v.is_empty()),
                "mime_type": mime_type,
                "overwrite": overwrite,
            }),
            Self::ArtifactUploadChunk {
                project,
                path,
                upload_id,
                offset,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "upload_id": upload_id,
                "offset": offset,
                "content_base64_present": true,
            }),
            Self::ArtifactUploadFinish {
                project,
                path,
                upload_id,
                ..
            }
            | Self::ArtifactUploadAbort {
                project,
                path,
                upload_id,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "upload_id": upload_id,
            }),
            Self::ApplyPatch {
                project,
                patch,
                dry_run,
                matching_mode,
                ..
            } => serde_json::json!({
                "project": project,
                "patch_present": !patch.is_empty(),
                "patch_bytes": patch.len(),
                "dry_run": dry_run,
                "matching_mode": matching_mode.map(|mode| mode.as_str()),
            }),
            Self::ApplyTextEdits {
                project,
                changes,
                dry_run,
                ..
            } => {
                let kind_list: Vec<&str> =
                    changes.iter().map(|change| change.kind.as_str()).collect();
                serde_json::json!({
                    "project": project,
                    "change_count": changes.len(),
                    "kinds": kind_list,
                    "paths": changes.iter().map(|change| change.path.as_str()).collect::<Vec<_>>(),
                    "destination_paths": changes.iter().filter_map(|change| change.to_path.as_deref()).collect::<Vec<_>>(),
                    "expected_read_revision_count": changes.iter().filter(|change| change.expected_read_revision.is_some()).count(),
                    "dry_run": dry_run,
                })
            }
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointCreate {
                project,
                title,
                note,
                include_untracked,
                kind,
                labels,
                validation,
                ..
            } => {
                let kind = kind
                    .as_deref()
                    .filter(|value| is_checkpoint_kind(value))
                    .unwrap_or(if kind.is_some() {
                        "invalid"
                    } else {
                        "snapshot"
                    });
                let validation_status = validation
                    .as_ref()
                    .and_then(|value| value.status.as_deref())
                    .filter(|value| is_checkpoint_validation_status(value))
                    .unwrap_or(
                        if validation
                            .as_ref()
                            .and_then(|value| value.status.as_deref())
                            .is_some()
                        {
                            "invalid"
                        } else {
                            "unknown"
                        },
                    );
                serde_json::json!({
                    "project": project,
                    "title": title,
                    "note_present": note.as_ref().is_some_and(|v| !v.is_empty()),
                    "include_untracked": include_untracked,
                    "kind": kind,
                    "label_count": labels.len(),
                    "validation_status": validation_status,
                })
            }
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointList { project, limit, .. } => serde_json::json!({
                "project": project,
                "limit": limit,
            }),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointShow {
                project,
                checkpoint_id,
                include_diff_stat,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "include_diff_stat": include_diff_stat,
            }),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointRestore {
                project,
                checkpoint_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "confirm": confirm,
            }),
            #[cfg(feature = "workspace-checkpoints")]
            Self::WorkspaceCheckpointDelete {
                project,
                checkpoint_id,
                confirm,
                ..
            } => serde_json::json!({
                "project": project,
                "checkpoint_id": checkpoint_id,
                "confirm": confirm,
            }),
            Self::RecordExternalObservation {
                project,
                session_id,
                ..
            }
            | Self::ListExternalObservations {
                project,
                session_id,
            } => serde_json::json!({
                "project": project, "session_id": session_id,
            }),
            Self::PostSessionMessage {
                session_id,
                kind,
                message,
                tags,
                reply_to,
                priority,
                requires_ack,
                delivery_key: _,
            } => serde_json::json!({
                "session_id": session_id,
                "kind": kind,
                "body_present": !message.is_empty(),
                "body_bytes": message.len(),
                "tags_count": tags.len(),
                "reply_to": reply_to,
                "priority": priority,
                "requires_ack": requires_ack,
            }),
            Self::PostPeerMessage {
                peer_id,
                kind,
                message,
                tags,
                priority,
                requires_ack,
                delivery_key: _,
            } => serde_json::json!({
                "peer_id": peer_id,
                "kind": kind,
                "body_present": !message.is_empty(),
                "body_bytes": message.len(),
                "tags_count": tags.len(),
                "priority": priority,
                "requires_ack": requires_ack,
            }),
            Self::ListSessionMessages {
                session_id,
                kind,
                status,
                message_id,
                reply_to,
                limit,
            } => serde_json::json!({
                "session_id": session_id,
                "kind": kind,
                "status": status,
                "message_id": message_id,
                "reply_to": reply_to,
                "limit": limit,
            }),
            Self::ObserveSessionMessages {
                session_id,
                after_observation_token,
                wait_secs,
                limit,
            } => serde_json::json!({
                "session_id": session_id,
                "token_present": after_observation_token.is_some(),
                "wait_secs": wait_secs,
                "limit": limit,
            }),
            Self::ResolveSessionMessage {
                session_id,
                message_id,
                resolution,
            } => serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "resolution_present": resolution.as_ref().is_some_and(|v| !v.is_empty()),
            }),
            Self::CompleteSessionMessage {
                session_id,
                message_id,
                answer,
                completion_key,
                expected_assignment_fence: _,
                tags,
                priority,
                ..
            } => serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
                "body_present": true,
                "body_bytes": answer.len(),
                "tags_count": tags.len(),
                "priority": priority,
                "completion_id": bounded_completion_key_fingerprint(Some(completion_key)),
                "assignment_fence_present": true,
            }),
            Self::SessionDiscussionSummary { session_id, limit } => serde_json::json!({
                "session_id": session_id,
                "limit": limit,
            }),
            Self::SessionHandoffSummary {
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                include_validation,
                diagnostic,
                limit,
            } => serde_json::json!({
                "session_id": session_id,
                "project": project,
                "include_workspace": include_workspace,
                "include_checkpoints": include_checkpoints,
                "include_validation": include_validation,
                "diagnostic": diagnostic,
                "limit": limit,
            }),
            Self::SessionHandoffState {
                project,
                session_id,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
            }),
            Self::StartSession {
                project,
                title,
                mode,
                deny_write_tools,
                deny_shell_tools,
                execution_context,
            } => serde_json::json!({
                "project": project,
                "title": title,
                "mode": mode,
                "deny_write_tools": deny_write_tools,
                "deny_shell_tools": deny_shell_tools,
                "execution_context": execution_context
                    .as_ref()
                    .map(SessionExecutionContext::audit_summary),
            }),
            Self::WorkOnProject {
                project,
                client_id,
                path,
                mode,
                base_ref,
                instruction,
                guidance_profile: _,
                session_id,
                include_extension_catalog,
            } => serde_json::json!({
                "project": project,
                "client_id": client_id,
                "path_source_requested": path.is_some(),
                "mode": mode,
                "base_ref_present": base_ref.is_some(),
                "instruction_present": true,
                "instruction_summary": command_preview(instruction),
                "include_extension_catalog": include_extension_catalog,
                "session_id": session_id,
            }),
            Self::UpdateSessionContext {
                project,
                session_id,
                execution_context,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "execution_context": execution_context.audit_summary(),
            }),
            Self::FinishCodingTask {
                project,
                session_id,
                outputs,
                summary_only,
                include_diff,
                include_workspace,
                include_hygiene,
                include_handoff,
                include_validation_summary,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "outputs": outputs,
                "summary_only": summary_only,
                "include_diff": include_diff,
                "include_workspace": include_workspace,
                "include_hygiene": include_hygiene,
                "include_handoff": include_handoff,
                "include_validation_summary": include_validation_summary,
            }),
            Self::PresentPdf { project, path }
            | Self::ReadPdfChunk { project, path, .. }
            | Self::ReadAppArtifactChunk { project, path, .. } => {
                serde_json::json!({
                    "project": project,
                    "path": path,
                })
            }
            Self::PresentWorkResult {
                project,
                session_id,
            }
            | Self::WorkResultState {
                project,
                session_id,
                ..
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
            }),
            Self::WorkResultActivityDetail {
                project,
                server_trace_id,
            } => serde_json::json!({
                "project": project,
                "server_trace_id": server_trace_id,
            }),
            Self::WorkResultSendMessage {
                project,
                session_id,
                message,
                delivery_key,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "message_chars": message.chars().count(),
                "delivery_key_present": !delivery_key.is_empty(),
            }),
            Self::ChangesFileDiff {
                project,
                session_id,
                snapshot_id,
                path,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "snapshot_id": snapshot_id,
                "path": path,
            }),
            Self::ResolveWorkspace {
                path, query, limit, ..
            } => serde_json::json!({
                "path_present":path.is_some(),"query_present":query.is_some(),"limit":limit,
            }),
            Self::UnregisterProjects {
                items,
                dry_run,
                confirm,
            } => serde_json::json!({
                "item_count":items.len(),"dry_run":dry_run,"confirm":confirm,
            }),
            Self::ListProjects {
                client_id,
                project,
                query,
                limit,
                summary_only,
                include_git_summary,
            } => serde_json::json!({
                "client_id_present": client_id.is_some(),
                "project_present": project.is_some(),
                "query_present": query.is_some(),
                "query_length": query.as_deref().map(|value| value.chars().count()).unwrap_or_default(),
                "limit": limit,
                "summary_only": summary_only,
                "include_git_summary": include_git_summary,
            }),
            Self::ListRunners {
                client_id,
                client_ids,
                include_projects,
                summary_only,
                query,
                status,
                limit,
            } => serde_json::json!({
                "client_id_present": client_id.is_some(),
                "client_ids_count": client_ids.as_ref().map(Vec::len).unwrap_or_default(),
                "include_projects": include_projects,
                "summary_only": summary_only,
                "query_present": query.is_some(), "status": status, "limit": limit,
            }),
            Self::ListJobs {
                limit,
                status,
                project,
                session_id,
            } => serde_json::json!({
                "limit": limit,
                "status": status,
                "project_present": project.is_some(),
                "session_id_present": session_id.is_some(),
            }),
            Self::ToolManifest {
                tool_name,
                query,
                limit,
                category,
                intent,
                include_recommended_flows,
                include_risk_summary,
            } => serde_json::json!({
                "tool_name": tool_name,
                "query_present": query.is_some(), "limit":limit,
                "category": category,
                "intent": intent,
                "include_recommended_flows": include_recommended_flows,
                "include_risk_summary": include_risk_summary,
            }),
            Self::ListTools {
                category,
                features,
                summary_only,
                limit,
            } => serde_json::json!({
                "category": category,
                "features": features,
                "summary_only": summary_only,
                "limit": limit,
            }),
            Self::ListSessions {
                project,
                lifecycle,
                offset,
                limit,
            } => serde_json::json!({
                "project": project,
                "lifecycle": lifecycle,
                "offset": offset,
                "limit": limit,
            }),
            Self::SessionSummary { session_id, limit } => serde_json::json!({
                "session_id": session_id,
                "limit": limit,
            }),
            Self::CloseSession { session_id } => serde_json::json!({
                "session_id": session_id,
            }),
            Self::ValidationSummary {
                project,
                session_id,
                limit,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "limit": limit,
            }),
            Self::GetSessionAssignment {
                session_id,
                message_id,
            } => serde_json::json!({
                "session_id": session_id,
                "message_id": message_id,
            }),
            Self::RunDetachedProcess {
                project,
                executable: _,
                args,
                stdin,
                timeout_secs,
                cwd,
                purpose,
                ..
            } => serde_json::json!({
                "project": project,
                "executable_present": true,
                "stdin_present": stdin.is_some(),
                "arg_count": args.len(),
                "process_summary": format!("detached process ({} args)", args.len()),
                "timeout_secs": timeout_secs,
                "cwd": cwd,
                "purpose": purpose,
            }),
            Self::JobTail {
                job_id,
                tail_lines,
                after_observation_token,
                wait_secs,
            } => serde_json::json!({
                "job_id": job_id,
                "tail_lines": tail_lines,
                "token_present": after_observation_token.is_some(),
                "wait_secs": wait_secs,
            }),
            Self::ListProjectTrackedFiles {
                project,
                path,
                globs,
                depth,
                limit,
                offset,
                ..
            } => serde_json::json!({
                "project": project,
                "path": path,
                "globs": globs,
                "depth": depth,
                "limit": limit,
                "offset": offset,
            }),

            Self::RegisterProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                overwrite,
            } => serde_json::json!({
                "client_id": client_id,
                "id": id,
                "name": name,
                "path_present": !path.is_empty(),
                "description_present": description.is_some(),
                "description_bytes": description.as_ref().map(String::len).unwrap_or_default(),
                "allow_patch": allow_patch,
                "overwrite": overwrite,
            }),
            Self::UnregisterProject {
                project,
                expected_revision,
            } => serde_json::json!({
                "project": project,
                "expected_revision": expected_revision,
            }),
            Self::CreateProject {
                client_id,
                id,
                name,
                path,
                description,
                allow_patch,
                template,
                git_init,
                adopt_existing_empty,
                overwrite,
            } => serde_json::json!({
                "client_id": client_id,
                "id": id,
                "name": name,
                "path_present": !path.is_empty(),
                "description_present": description.is_some(),
                "description_bytes": description.as_ref().map(String::len).unwrap_or_default(),
                "allow_patch": allow_patch,
                "template": template,
                "git_init": git_init,
                "adopt_existing_empty": adopt_existing_empty,
                "overwrite": overwrite,
            }),
            Self::RunnerConfigCheck { client_id } => serde_json::json!({
                "client_id": client_id,
            }),
            Self::RunnerConfigReload {
                client_id,
                expected_generation,
            } => serde_json::json!({
                "client_id": client_id,
                "expected_generation": expected_generation,
            }),
            Self::PluginTool(plugin) => serde_json::json!({
                "action": plugin.action,
                "runner_present": plugin.runner.is_some(),
                "plugin_present": plugin.plugin.is_some(),
                "tool_present": plugin.tool.is_some(),
                "binding_present": plugin.binding.is_some(),
                "arguments_present": plugin.arguments.is_some(),
                "argument_key_count": plugin.arguments.as_ref().and_then(Value::as_object).map(serde_json::Map::len).unwrap_or_default(),
            }),
            Self::SshResource(resource) => serde_json::json!({
                "action": resource.action,
                "runner_present": resource.runner.is_some(),
                "binding_present": resource.binding.is_some(),
                "name_present": resource.name.is_some(),
                "target_present": resource.target.is_some(),
                "default_cwd_present": resource.default_cwd.is_some(),
            }),
            Self::ReadToolTrace {
                trace_ref,
                query,
                offset,
                limit,
                payload_index,
            } => serde_json::json!({
                "trace_ref": trace_ref,
                "query_present": query.is_some(),
                "offset": offset,
                "limit": limit,
                "payload_index": payload_index,
            }),
            Self::RuntimeStatus {
                compact,
                summary_only,
                client_id,
            } => serde_json::json!({
                "compact": compact,
                "summary_only": summary_only,
                "client_id_present": client_id.is_some(),
            }),
            Self::CurrentWindowActivity {
                limit,
                include_nonmeaningful,
            } => serde_json::json!({
                "limit": limit,
                "include_nonmeaningful": include_nonmeaningful,
            }),
            Self::WorkspaceHygieneCheck {
                project,
                max_findings,
                include_tracked,
                ..
            } => serde_json::json!({
                "project": project,
                "max_findings": max_findings,
                "include_tracked": include_tracked,
            }),
        }
    }
}
