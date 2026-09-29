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

fn browser_observe_audit_projection(call: &BrowserObserveToolCall) -> Value {
    serde_json::to_value(call).unwrap_or_else(|_| {
        serde_json::json!({
            "action": call.action_name()
        })
    })
}

fn browser_act_audit_projection(call: &BrowserActToolCall) -> Value {
    match call {
        BrowserActToolCall::Launch { client_id } => serde_json::json!({
            "action": "launch",
            "client_id": client_id,
        }),
        BrowserActToolCall::NewPage {
            client_id,
            browser_id,
        } => serde_json::json!({
            "action": "new_page",
            "client_id": client_id,
            "browser_id": browser_id,
        }),
        BrowserActToolCall::Navigate {
            client_id,
            browser_id,
            page_id,
            ..
        } => serde_json::json!({
            "action": "navigate",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "url_present": true,
        }),
        BrowserActToolCall::Reload {
            client_id,
            browser_id,
            page_id,
        } => serde_json::json!({
            "action": "reload",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
        }),
        BrowserActToolCall::Click {
            client_id,
            browser_id,
            page_id,
            element_id,
        } => serde_json::json!({
            "action": "click",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "element_id": element_id,
        }),
        BrowserActToolCall::InputText {
            client_id,
            browser_id,
            page_id,
            element_id,
            text,
        } => serde_json::json!({
            "action": "input_text",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "element_id": element_id,
            "text_present": true,
            "text_bytes": text.len(),
        }),
        BrowserActToolCall::SelectOption {
            client_id,
            browser_id,
            page_id,
            element_id,
            option,
        } => serde_json::json!({
            "action": "select_option",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "element_id": element_id,
            "option_present": true,
            "option_bytes": option.len(),
        }),
        BrowserActToolCall::SetValue {
            client_id,
            browser_id,
            page_id,
            element_id,
            value,
        } => serde_json::json!({
            "action": "set_value",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "element_id": element_id,
            "value_present": true,
            "value_bytes": value.len(),
        }),
        BrowserActToolCall::UploadFile {
            client_id,
            browser_id,
            page_id,
            element_id,
            project,
            path,
        } => serde_json::json!({
            "action": "upload_file",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "element_id": element_id,
            "project": project,
            "path_present": true,
            "path_bytes": path.len(),
        }),
        BrowserActToolCall::Key {
            client_id,
            browser_id,
            page_id,
            key,
        } => serde_json::json!({
            "action": "key",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "key": key.as_str(),
        }),
        BrowserActToolCall::ClearDiagnostics {
            client_id,
            browser_id,
            page_id,
        } => serde_json::json!({
            "action": "clear_diagnostics",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
        }),
        BrowserActToolCall::ClosePage {
            client_id,
            browser_id,
            page_id,
        } => serde_json::json!({
            "action": "close_page",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
        }),
        BrowserActToolCall::CloseBrowser {
            client_id,
            browser_id,
        } => serde_json::json!({
            "action": "close_browser",
            "client_id": client_id,
            "browser_id": browser_id,
        }),
    }
}

fn computer_observe_audit_projection(call: &ComputerObserveToolCall) -> Value {
    let mut projection = serde_json::to_value(call).unwrap_or_else(|_| {
        serde_json::json!({
            "action": call.action_name()
        })
    });
    if let Some(object) = projection.as_object_mut() {
        for field in ["role", "subrole", "label"] {
            let present = object.remove(field).is_some();
            if present {
                object.insert(format!("{field}_present"), Value::Bool(true));
            }
        }
        let region_present = object.remove("region").is_some();
        if region_present {
            object.insert("region_present".to_string(), Value::Bool(true));
        }
    }
    projection
}

fn computer_control_audit_projection(call: &ComputerControlToolCall) -> Value {
    let mut projection = serde_json::to_value(call).unwrap_or_else(|_| {
        serde_json::json!({
            "action": call.action_name()
        })
    });
    if let Some(object) = projection.as_object_mut() {
        if let Some(text) = object
            .remove("text")
            .and_then(|value| value.as_str().map(str::to_owned))
        {
            object.insert("text_bytes".to_string(), Value::from(text.len()));
        }
    }
    projection
}

#[derive(Debug, Clone, Copy)]
enum StructuredValidationRequestAudit {
    CargoFmt,
    CargoCheck,
    CargoTest,
    GoTest,
}

impl StructuredValidationRequestAudit {
    fn tool_name(self) -> &'static str {
        match self {
            Self::CargoFmt => "cargo_fmt",
            Self::CargoCheck => "cargo_check",
            Self::CargoTest => "cargo_test",
            Self::GoTest => "go_test",
        }
    }
}

fn typed_structured_validation_request_audit(
    kind: StructuredValidationRequestAudit,
    arguments: &Value,
) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match kind {
        StructuredValidationRequestAudit::CargoFmt => {
            copy_keys(obj, &mut out, &["cwd", "check", "timeout_secs"]);
            insert_structured_validation_target(kind.tool_name(), obj, &mut out);
        }
        StructuredValidationRequestAudit::CargoCheck => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "cwd",
                    "all_targets",
                    "all_features",
                    "no_default_features",
                    "package",
                    "packages",
                    "timeout_secs",
                ],
            );
            out.insert(
                "features_present".to_string(),
                Value::Bool(
                    obj.get("features")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty()),
                ),
            );
            insert_structured_validation_target(kind.tool_name(), obj, &mut out);
        }
        StructuredValidationRequestAudit::CargoTest => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "cwd",
                    "all_targets",
                    "all_features",
                    "no_default_features",
                    "package",
                    "no_run",
                    "require_tests",
                    "min_tests",
                    "timeout_secs",
                ],
            );
            if obj.get("lib").and_then(Value::as_bool) == Some(true) {
                out.insert("lib".to_string(), Value::Bool(true));
            }
            out.insert(
                "filter_present".to_string(),
                Value::Bool(
                    obj.get("filter")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty()),
                ),
            );
            out.insert(
                "features_present".to_string(),
                Value::Bool(
                    obj.get("features")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty()),
                ),
            );
            insert_structured_validation_target(kind.tool_name(), obj, &mut out);
        }
        StructuredValidationRequestAudit::GoTest => {
            copy_keys(obj, &mut out, &["cwd", "timeout_secs"]);
            let packages = obj.get("packages").and_then(Value::as_array);
            out.insert(
                "packages_present".to_string(),
                Value::Bool(obj.get("packages").is_some_and(|value| !value.is_null())),
            );
            out.insert(
                "package_count".to_string(),
                Value::from(packages.map(Vec::len).unwrap_or_default()),
            );
            insert_structured_validation_target(kind.tool_name(), obj, &mut out);
        }
    }
    if let Some(sync_wait_secs) = obj
        .get("sync_wait_secs")
        .filter(|value| !value.is_null())
        .cloned()
    {
        out.insert("sync_wait_secs".to_string(), sync_wait_secs);
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy)]
enum GoalRequestAudit {
    Create,
    Prepare,
    Get,
    List,
    Update,
    AssociateAgentTask,
    AssociateWorkflowSession,
}

fn typed_goal_request_audit(kind: GoalRequestAudit, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    match kind {
        GoalRequestAudit::Create | GoalRequestAudit::Prepare => {
            if matches!(kind, GoalRequestAudit::Prepare) {
                copy_keys(obj, &mut out, &["session_id"]);
            }
            out.insert(
                "title_chars".to_string(),
                Value::from(
                    obj.get("title")
                        .and_then(Value::as_str)
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "objective_bytes".to_string(),
                Value::from(
                    obj.get("objective")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            copy_keys(obj, &mut out, &["controller_agent_id"]);
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        GoalRequestAudit::Get => copy_keys(obj, &mut out, &["goal_id"]),
        GoalRequestAudit::List => copy_keys(obj, &mut out, &["lifecycle", "offset", "limit"]),
        GoalRequestAudit::Update => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "goal_id",
                    "expected_revision",
                    "controller_agent_id",
                    "lifecycle",
                ],
            );
            out.insert(
                "title_chars".to_string(),
                Value::from(
                    obj.get("title")
                        .and_then(Value::as_str)
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "objective_bytes".to_string(),
                Value::from(
                    obj.get("objective")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "terminal_reason_bytes".to_string(),
                Value::from(
                    obj.get("terminal_reason")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        GoalRequestAudit::AssociateAgentTask => {
            copy_keys(obj, &mut out, &["goal_id", "task_id"]);
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        GoalRequestAudit::AssociateWorkflowSession => {
            copy_keys(obj, &mut out, &["goal_id", "session_id"]);
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy)]
enum AgentTaskRequestAudit {
    Create,
    List,
    Read,
    Assign,
    StartAttempt,
    StartEndpointContinuation,
    StartCodingRun,
    ReconcileCodingRun,
    HeartbeatAttempt,
    CompleteAttempt,
}

fn typed_agent_task_request_audit(kind: AgentTaskRequestAudit, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match kind {
        AgentTaskRequestAudit::Create => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "assignee_agent_id",
                    "source_conversation_id",
                    "source_message_id",
                    "referenced_project_id",
                ],
            );
            out.insert(
                "title_chars".to_string(),
                Value::from(
                    obj.get("title")
                        .and_then(Value::as_str)
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "instruction_bytes".to_string(),
                Value::from(
                    obj.get("instruction")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        AgentTaskRequestAudit::List => {
            copy_keys(obj, &mut out, &["assignee_agent_id", "offset", "limit"]);
        }
        AgentTaskRequestAudit::Read => {
            copy_keys(obj, &mut out, &["task_id"]);
        }
        AgentTaskRequestAudit::Assign => {
            copy_keys(obj, &mut out, &["task_id", "assignee_agent_id"]);
        }
        AgentTaskRequestAudit::StartAttempt => {
            copy_keys(obj, &mut out, &["task_id", "assignee_agent_id"]);
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        AgentTaskRequestAudit::StartEndpointContinuation => {
            for key in [
                "attempt_ref",
                "task_id",
                "attempt_id",
                "assignee_agent_id",
                "attempt_controller_generation",
            ] {
                if let Some(value) = obj.get(key).filter(|value| !value.is_null()) {
                    out.insert((*key).to_string(), value.clone());
                }
            }
            out.insert(
                "attempt_fence_present".to_string(),
                Value::Bool(obj.get("attempt_fence").and_then(Value::as_str).is_some()),
            );
        }
        AgentTaskRequestAudit::StartCodingRun => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "task_id",
                    "attempt_id",
                    "assignee_agent_id",
                    "attempt_controller_generation",
                    "provider_id",
                    "timeout_secs",
                ],
            );
            out.insert(
                "attempt_fence_present".to_string(),
                Value::Bool(obj.get("attempt_fence").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "config_count".to_string(),
                Value::from(
                    obj.get("config")
                        .and_then(Value::as_object)
                        .map(serde_json::Map::len)
                        .unwrap_or_default(),
                ),
            );
        }
        AgentTaskRequestAudit::ReconcileCodingRun => {
            copy_keys(obj, &mut out, &["task_id", "attempt_id"]);
        }
        AgentTaskRequestAudit::HeartbeatAttempt => {
            for key in [
                "attempt_ref",
                "task_id",
                "attempt_id",
                "assignee_agent_id",
                "attempt_controller_generation",
            ] {
                if let Some(value) = obj.get(key).filter(|value| !value.is_null()) {
                    out.insert((*key).to_string(), value.clone());
                }
            }
            out.insert(
                "attempt_fence_present".to_string(),
                Value::Bool(obj.get("attempt_fence").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "active_turn_proof_present".to_string(),
                Value::Bool(
                    obj.get("active_turn_wake_id")
                        .and_then(Value::as_str)
                        .is_some()
                        && obj
                            .get("active_turn_consume_token")
                            .and_then(Value::as_str)
                            .is_some(),
                ),
            );
        }
        AgentTaskRequestAudit::CompleteAttempt => {
            for key in [
                "attempt_ref",
                "task_id",
                "attempt_id",
                "assignee_agent_id",
                "attempt_controller_generation",
                "outcome",
            ] {
                if let Some(value) = obj.get(key).filter(|value| !value.is_null()) {
                    out.insert((*key).to_string(), value.clone());
                }
            }
            out.insert(
                "attempt_fence_present".to_string(),
                Value::Bool(obj.get("attempt_fence").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "terminal_result_bytes".to_string(),
                Value::from(
                    obj.get("terminal_result")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "terminal_reason_bytes".to_string(),
                Value::from(
                    obj.get("terminal_reason")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "completion_key_present".to_string(),
                Value::Bool(obj.get("completion_key").and_then(Value::as_str).is_some()),
            );
        }
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy)]
enum CommunicationRequestAudit {
    CreateIdentity,
    ListIdentities,
    UpdateIdentity,
    AttachEndpoint,
    DetachEndpoint,
    CreateConversation,
    ListConversations,
    ReadConversation,
    PostMessage,
    ListInbox,
    ConsumeDeliveries,
    BootstrapConversation,
    ConsumeWake,
}

fn typed_communication_request_audit(kind: CommunicationRequestAudit, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match kind {
        CommunicationRequestAudit::CreateIdentity => {
            out.insert(
                "handle_chars".to_string(),
                Value::from(
                    obj.get("handle")
                        .and_then(Value::as_str)
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "display_name_chars".to_string(),
                Value::from(
                    obj.get("display_name")
                        .and_then(Value::as_str)
                        .map(str::chars)
                        .map(Iterator::count)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "description_bytes".to_string(),
                Value::from(
                    obj.get("description")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "specialty_label_count".to_string(),
                Value::from(
                    obj.get("specialty_labels")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        CommunicationRequestAudit::ListIdentities => {
            copy_keys(obj, &mut out, &["agent_id", "offset", "limit"]);
        }
        CommunicationRequestAudit::UpdateIdentity => {
            copy_keys(obj, &mut out, &["agent_id", "expected_profile_revision"]);
            for field in ["handle", "display_name", "description", "specialty_labels"] {
                out.insert(
                    format!("{field}_present"),
                    Value::Bool(obj.get(field).is_some_and(|value| !value.is_null())),
                );
            }
            out.insert(
                "description_bytes".to_string(),
                Value::from(
                    obj.get("description")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "specialty_label_count".to_string(),
                Value::from(
                    obj.get("specialty_labels")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
        }
        CommunicationRequestAudit::AttachEndpoint => {
            copy_keys(obj, &mut out, &["agent_id", "host"]);
            out.insert(
                "client_attachment_id_present".to_string(),
                Value::Bool(
                    obj.get("client_attachment_id")
                        .is_some_and(|value| !value.is_null()),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        CommunicationRequestAudit::DetachEndpoint => {
            copy_keys(obj, &mut out, &["endpoint_id"]);
        }
        CommunicationRequestAudit::CreateConversation => {
            out.insert(
                "title_present".to_string(),
                Value::Bool(obj.get("title").is_some_and(|value| !value.is_null())),
            );
            out.insert(
                "agent_count".to_string(),
                Value::from(
                    obj.get("agent_ids")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        CommunicationRequestAudit::ListConversations => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "offset",
                    "limit",
                ],
            );
        }
        CommunicationRequestAudit::ReadConversation => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "conversation_id",
                    "agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "after_seq",
                    "limit",
                ],
            );
        }
        CommunicationRequestAudit::PostMessage => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "conversation_id",
                    "author_agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "reply_to",
                    "wake_reply_id",
                    "reply_operation_index",
                ],
            );
            out.insert(
                "body_bytes".to_string(),
                Value::from(
                    obj.get("body")
                        .and_then(Value::as_str)
                        .map(str::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "recipient_mode".to_string(),
                Value::String(
                    if obj.get("recipient_agent_ids").is_some_and(Value::is_array) {
                        "explicit".to_string()
                    } else {
                        "all_agents_except_author".to_string()
                    },
                ),
            );
            out.insert(
                "recipient_count".to_string(),
                Value::from(
                    obj.get("recipient_agent_ids")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        CommunicationRequestAudit::ListInbox => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "after_delivery_order",
                    "limit",
                ],
            );
        }
        CommunicationRequestAudit::ConsumeDeliveries => {
            copy_keys(
                obj,
                &mut out,
                &["agent_id", "endpoint_id", "expected_controller_generation"],
            );
            out.insert(
                "delivery_count".to_string(),
                Value::from(
                    obj.get("delivery_ids")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                ),
            );
        }
        CommunicationRequestAudit::BootstrapConversation => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "conversation_id",
                    "wake_id",
                ],
            );
        }
        CommunicationRequestAudit::ConsumeWake => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "agent_id",
                    "endpoint_id",
                    "expected_controller_generation",
                    "wake_id",
                ],
            );
            let consume_token_present = obj
                .get("consume_token_present")
                .and_then(Value::as_bool)
                .unwrap_or_else(|| obj.get("consume_token").and_then(Value::as_str).is_some());
            out.insert(
                "consume_token_present".to_string(),
                Value::Bool(consume_token_present),
            );
        }
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy)]
enum MemoryRequestAudit {
    Search,
    Read,
    Set,
    Delete,
    ScopeList,
    ScopePurge,
}

fn typed_memory_request_audit(kind: MemoryRequestAudit, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match kind {
        MemoryRequestAudit::Search => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "offset",
                    "limit",
                    "expected_catalog_revision",
                    "session_id",
                ],
            );
            out.insert(
                "query_present".to_string(),
                Value::Bool(
                    obj.get("query")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty()),
                ),
            );
            out.insert(
                "tag_count".to_string(),
                Value::from(
                    obj.get("tags")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0),
                ),
            );
        }
        MemoryRequestAudit::Read => {
            copy_keys(
                obj,
                &mut out,
                &["project", "memory_key", "expected_revision", "session_id"],
            );
        }
        MemoryRequestAudit::Set => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "memory_key",
                    "priority",
                    "bootstrap",
                    "expected_revision",
                    "session_id",
                ],
            );
            out.insert(
                "summary_present".to_string(),
                Value::Bool(obj.get("summary").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "body_present".to_string(),
                Value::Bool(obj.get("body").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "tag_count".to_string(),
                Value::from(
                    obj.get("tags")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0),
                ),
            );
        }
        MemoryRequestAudit::Delete => {
            copy_keys(
                obj,
                &mut out,
                &["project", "memory_key", "expected_revision", "session_id"],
            );
        }
        MemoryRequestAudit::ScopeList => {
            copy_keys(obj, &mut out, &["offset", "limit"]);
        }
        MemoryRequestAudit::ScopePurge => {
            copy_keys(
                obj,
                &mut out,
                &["memory_scope_id", "expected_catalog_revision"],
            );
        }
    }
    Value::Object(out)
}

#[derive(Debug, Clone, Copy)]
enum SkillRequestAudit {
    Versions,
    Install,
    Activate,
    RemoveRevision,
}

fn typed_skill_request_audit(kind: SkillRequestAudit, arguments: &Value) -> Value {
    let Some(obj) = arguments.as_object() else {
        return empty_audit_projection();
    };
    let mut out = serde_json::Map::new();
    if let Some(project) = obj.get("project").cloned() {
        out.insert("project".to_string(), project);
    }
    match kind {
        SkillRequestAudit::Versions => {
            copy_keys(
                obj,
                &mut out,
                &["project", "skill_key", "offset", "limit", "session_id"],
            );
        }
        SkillRequestAudit::Install => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "skill_key",
                    "expected_artifact_sha256",
                    "activate",
                    "expected_state_revision",
                    "session_id",
                ],
            );
            out.insert(
                "artifact_path_present".to_string(),
                Value::Bool(obj.get("artifact_path").and_then(Value::as_str).is_some()),
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        SkillRequestAudit::Activate => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "skill_key",
                    "package_revision",
                    "expected_state_revision",
                    "session_id",
                ],
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
        SkillRequestAudit::RemoveRevision => {
            copy_keys(
                obj,
                &mut out,
                &[
                    "project",
                    "skill_key",
                    "package_revision",
                    "expected_state_revision",
                    "session_id",
                ],
            );
            out.insert(
                "idempotency_key_present".to_string(),
                Value::Bool(obj.get("idempotency_key").and_then(Value::as_str).is_some()),
            );
        }
    }
    Value::Object(out)
}

fn bounded_test_count_assertion_audit(value: &Value) -> Option<Value> {
    let assertion = value.as_object()?;
    let minimum_tests = assertion.get("minimum_tests")?.as_u64()?;
    let actual_tests_run = match assertion.get("actual_tests_run")? {
        Value::Null => Value::Null,
        value => serde_json::json!(value.as_u64()?),
    };
    let status = assertion.get("status")?.as_str()?;
    let reason_code = assertion.get("reason_code")?.as_str()?;
    let evidence_reason_code = assertion.get("evidence_reason_code")?.as_str()?;
    if !matches!(status, "passed" | "failed" | "unproven")
        || !matches!(
            reason_code,
            "minimum_satisfied" | "minimum_not_met" | "test_count_unproven"
        )
        || !matches!(
            evidence_reason_code,
            "complete_summary"
                | "output_truncated"
                | "partial_harness_summary"
                | "no_complete_summary"
                | "incomplete_stream"
        )
    {
        return None;
    }
    Some(serde_json::json!({
        "minimum_tests": minimum_tests,
        "actual_tests_run": actual_tests_run,
        "status": status,
        "reason_code": reason_code,
        "evidence_reason_code": evidence_reason_code,
    }))
}

/// ActionAudit cannot persist the raw canonical evidence that the Session ledger
/// consumes. Narrow definition-owned execution evidence to lifecycle scalars plus
/// bounded validation counts/assertions; Session excerpt/source processing continues
/// to consume its original result.
pub fn canonical_execution_audit_result_for_tool(tool_name: &str, output: &Value) -> Value {
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(tool_name) else {
        return empty_audit_projection();
    };
    if definition.audit_policy().result
        != webcodex_tool_contracts::ToolAuditResultPolicy::CanonicalLedgerEvidence
    {
        return session_log_result_for_tool(tool_name, output);
    }
    if definition.audit_policy().execution.detail
        == webcodex_tool_contracts::ToolAuditExecutionDetail::Omit
    {
        return empty_audit_projection();
    }
    // Canonical ledger evidence is borrowed only; never clone that raw body
    // into ActionAudit. The definition still owns execution eligibility.
    let mut result = serde_json::Map::new();
    for key in [
        "command_started",
        "command_completed",
        "command_ok",
        "passed",
        "terminal",
        "promoted_to_job",
        "changed",
        "state_changed",
        "tool_failure",
    ] {
        if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if let Some(value) = output.get("exit_code").filter(|value| value.is_i64()) {
        result.insert("exit_code".to_string(), value.clone());
    }
    if let Some(kind) = output.get("failure_kind").and_then(Value::as_str) {
        if !kind.is_empty()
            && kind.len() <= 64
            && kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            result.insert("failure_kind".to_string(), Value::String(kind.to_string()));
        }
    }
    if let Some(kind) = output.get("recovery_kind").and_then(Value::as_str) {
        if webcodex_core::runtime_contract::RECOVERY_KIND_VALUES.contains(&kind) {
            result.insert("recovery_kind".to_string(), Value::String(kind.to_string()));
        }
    }
    for key in ["warnings_count", "errors_count"] {
        if let Some(value) = output.get(key).filter(|value| value.is_u64()) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if matches!(
        definition.audit_policy().execution.detail,
        webcodex_tool_contracts::ToolAuditExecutionDetail::TestCounts
            | webcodex_tool_contracts::ToolAuditExecutionDetail::TestAssertions
    ) {
        for key in ["tests_run_count", "tests_passed", "tests_failed"] {
            if let Some(value) = output.get(key).filter(|value| value.is_u64()) {
                result.insert(key.to_string(), value.clone());
            }
        }
        for key in ["tests_detected", "zero_tests_run"] {
            if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
                result.insert(key.to_string(), value.clone());
            }
        }
    }
    if definition.audit_policy().execution.detail
        == webcodex_tool_contracts::ToolAuditExecutionDetail::TestAssertions
    {
        for key in ["require_tests", "no_run"] {
            if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
                result.insert(key.to_string(), value.clone());
            }
        }
        if let Some(assertion) = output
            .get("test_count_assertion")
            .and_then(bounded_test_count_assertion_audit)
        {
            result.insert("test_count_assertion".to_string(), assertion);
        }
    }
    if let Some(state) = output.get("execution_state").and_then(Value::as_str) {
        if matches!(
            state,
            "completed"
                | "not_started"
                | "outcome_unknown"
                | "timed_out"
                | "pending"
                | "queued"
                | "running"
                | "started"
        ) {
            result.insert(
                "execution_state".to_string(),
                Value::String(state.to_string()),
            );
        }
    }
    if let Some(source) = output.get("source_state") {
        // Only closed source classifications, never the fence/epoch identity.
        if let (Some(freshness), Some(fence)) = (
            source.get("freshness").and_then(Value::as_str),
            source
                .get("observed_mutation_fence")
                .and_then(Value::as_str),
        ) {
            if matches!(freshness, "unproven" | "stale")
                && matches!(fence, "uncrossed" | "crossed" | "unknown")
            {
                result.insert(
                    "source_state".to_string(),
                    serde_json::json!({
                        "freshness": freshness, "observed_mutation_fence": fence,
                    }),
                );
            }
        }
    }
    Value::Object(result)
}

#[cfg(test)]
#[path = "tool_audit/tests/canonical_execution.rs"]
mod canonical_execution_audit_projection_tests;

pub fn session_log_result_for_tool(tool_name: &str, output: &Value) -> Value {
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(tool_name) else {
        return empty_audit_projection();
    };
    match definition.audit_policy().result {
        webcodex_tool_contracts::ToolAuditResultPolicy::CanonicalLedgerEvidence => output.clone(),
        webcodex_tool_contracts::ToolAuditResultPolicy::Fields(fields) => {
            project_declared_result_fields(fields, output)
        }
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::BrowserObservation,
        ) => browser_observation_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::BrowserControl,
        ) => browser_control_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::ComputerObservation,
        ) => computer_observation_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::ComputerControl,
        ) => computer_control_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::CodingAgentObservation,
        ) => coding_agent_observation_result_audit(output),
    }
}

fn project_declared_result_fields(
    fields: &[webcodex_tool_contracts::ToolAuditResultField],
    output: &Value,
) -> Value {
    use webcodex_tool_contracts::ToolAuditResultField;

    let mut projected = serde_json::Map::new();
    for field in fields {
        let (key, value) = match *field {
            ToolAuditResultField::Value {
                output: key,
                source,
            } => (key, output.get(source).cloned().unwrap_or(Value::Null)),
            ToolAuditResultField::Pointer {
                output: key,
                pointer,
            } => (key, output.pointer(pointer).cloned().unwrap_or(Value::Null)),
            ToolAuditResultField::ArrayLen {
                output: key,
                source,
            } => (
                key,
                output
                    .get(source)
                    .and_then(Value::as_array)
                    .map(|items| Value::from(items.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::PointerArrayLen {
                output: key,
                pointer,
            } => (
                key,
                output
                    .pointer(pointer)
                    .and_then(Value::as_array)
                    .map(|items| Value::from(items.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::StringBytes {
                output: key,
                source,
            } => (
                key,
                output
                    .get(source)
                    .and_then(Value::as_str)
                    .map(|value| Value::from(value.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::Presence {
                output: key,
                source,
            } => (key, Value::Bool(output.get(source).is_some())),
            ToolAuditResultField::StringPresent {
                output: key,
                source,
            } => (
                key,
                Value::Bool(output.get(source).and_then(Value::as_str).is_some()),
            ),
            ToolAuditResultField::PointerNonNull {
                output: key,
                pointer,
            } => (
                key,
                Value::Bool(
                    output
                        .pointer(pointer)
                        .is_some_and(|value| !value.is_null()),
                ),
            ),
        };
        projected.insert(key.to_string(), value);
    }
    Value::Object(projected)
}

fn copy_existing_audit_value(
    projected: &mut serde_json::Map<String, Value>,
    output: &Value,
    key: &'static str,
) {
    if let Some(value) = output.get(key) {
        projected.insert(key.to_string(), value.clone());
    }
}

fn browser_observation_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    for key in [
        "execution_state",
        "state_changed",
        "error_kind",
        "count",
        "total_count",
        "truncated",
        "retained_count",
        "console_retained",
        "console_count",
        "console_truncated",
        "network_retained",
        "network_count",
        "network_truncated",
        "browser_id",
        "page_id",
        "snapshot_generation",
        "node_count",
        "mime_type",
        "width",
        "height",
        "file_bytes",
        "sha256",
    ] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    for (source, target) in [
        ("targets", "target_count"),
        ("browsers", "browser_count"),
        ("pages", "page_count"),
        ("nodes", "projected_node_count"),
    ] {
        if let Some(count) = output.get(source).and_then(Value::as_array).map(Vec::len) {
            projected.insert(target.to_string(), Value::from(count));
        }
    }
    Value::Object(projected)
}

fn browser_control_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    for key in [
        "execution_state",
        "state_changed",
        "error_kind",
        "browser_id",
        "page_id",
        "page_count",
    ] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

fn computer_observation_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();

    if output.get("targets").is_some() {
        for key in ["count", "total_count", "truncated"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("windows").is_some()
        || output.get("displays").is_some()
        || output.get("applications").is_some()
    {
        for key in ["count", "truncated"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("trusted").is_some() {
        for key in ["platform", "trusted"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("nodes").is_some() {
        for key in [
            "surface_id",
            "observation_generation",
            "node_count",
            "truncated",
            "max_depth",
            "max_nodes",
        ] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("elements").is_some() {
        for key in [
            "surface_id",
            "observation_generation",
            "count",
            "scanned_nodes",
            "truncated",
        ] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("content_base64").is_some() {
        if output.get("display_id").is_some() {
            for key in [
                "display_id",
                "snapshot_generation",
                "source_width",
                "source_height",
                "width",
                "height",
                "mime_type",
                "file_bytes",
                "sha256",
                "captured_at_unix_ms",
            ] {
                copy_existing_audit_value(&mut projected, output, key);
            }
        } else {
            if let Some(surface_id) = output.pointer("/surface/surface_id") {
                projected.insert("surface_id".to_string(), surface_id.clone());
            }
            for key in [
                "source_width",
                "source_height",
                "width",
                "height",
                "mime_type",
                "file_bytes",
                "captured_at_unix_ms",
            ] {
                copy_existing_audit_value(&mut projected, output, key);
            }
            projected.insert(
                "region_present".to_string(),
                Value::Bool(output.get("region").is_some()),
            );
        }
    } else if output.get("available").is_some() {
        for key in ["available", "text_bytes", "success"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("element_id").is_some() {
        for key in ["surface_id", "element_id", "observation_generation"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    }

    for key in ["error_kind", "execution_state"] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

fn computer_control_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    let keys: &[&str] = if output.get("application_id").is_some() {
        &[
            "application_id",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("display_id").is_some() && output.get("x").is_some() {
        &[
            "display_id",
            "snapshot_generation",
            "x",
            "y",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("text_bytes").is_some() && output.get("element_id").is_none() {
        &[
            "text_bytes",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("key").is_some() {
        &["surface_id", "key", "modifiers", "success"]
    } else if output.get("element_id").is_some() && output.get("text_bytes").is_some() {
        &["surface_id", "element_id", "text_bytes", "success"]
    } else if output.get("element_id").is_some() && output.get("action").is_some() {
        &["surface_id", "element_id", "action", "success"]
    } else if output.get("element_id").is_some() {
        &["surface_id", "element_id", "success"]
    } else {
        &["surface_id", "success"]
    };
    for key in keys {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

fn coding_agent_observation_result_audit(output: &Value) -> Value {
    let mut kind_counts = serde_json::Map::new();
    let mut event_count = 0usize;
    let mut event_body_bytes = 0usize;
    if let Some(events) = output.get("events").and_then(Value::as_array) {
        event_count = events.len();
        for event in events {
            if let Some(kind) = event.get("kind").and_then(Value::as_str) {
                let count = kind_counts.get(kind).and_then(Value::as_u64).unwrap_or(0) + 1;
                kind_counts.insert(kind.to_string(), Value::from(count));
            }
            event_body_bytes = event_body_bytes.saturating_add(
                event
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0),
            );
        }
    }
    serde_json::json!({
        "run_id": output.get("run_id").cloned().unwrap_or(Value::Null),
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "provider_id": output.get("provider_id").cloned().unwrap_or(Value::Null),
        "state": output.get("state").cloned().unwrap_or(Value::Null),
        "execution_state": output.get("execution_state").cloned().unwrap_or(Value::Null),
        "event_count": event_count,
        "event_kind_counts": kind_counts,
        "event_body_bytes": event_body_bytes,
        "has_more": output.get("has_more").cloned().unwrap_or(Value::Null),
        "history_lost": output.get("history_lost").cloned().unwrap_or(Value::Null),
        "first_retained_sequence": output.get("first_retained_sequence").cloned().unwrap_or(Value::Null),
        "terminal_stop_reason": output.pointer("/terminal/stop_reason").cloned().unwrap_or(Value::Null),
        "terminal_error_code": output.pointer("/terminal/error_code").cloned().unwrap_or(Value::Null),
        "terminal_completed_at": output.pointer("/terminal/completed_at").cloned().unwrap_or(Value::Null),
        "recovery_kind": output.get("recovery_kind").cloned().unwrap_or(Value::Null),
        "error_kind": output.get("error_kind").cloned().unwrap_or(Value::Null),
    })
}

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

fn normalized_exact_git_commit_for_audit(value: &str) -> Option<String> {
    (value.len() == 40 && value.as_bytes().iter().all(u8::is_ascii_hexdigit))
        .then(|| value.to_ascii_lowercase())
}

fn insert_structured_validation_target(
    tool_name: &str,
    arguments: &serde_json::Map<String, Value>,
    out: &mut serde_json::Map<String, Value>,
) {
    let identity_kind = webcodex_tool_contracts::runtime_tool_session_evidence_policy(tool_name)
        .validation_identity;
    if let Some(identity) =
        structured_validation_target_identity(identity_kind, &Value::Object(arguments.clone()))
    {
        out.insert("validation_target_id".to_string(), Value::String(identity));
    }
}

pub use webcodex_core::validation_identity::{
    assertion_validation_identity, is_structured_validation_target_identity,
    is_validation_execution_identity, structured_validation_target_identity,
};
use webcodex_core::validation_identity::{
    GENERIC_VALIDATION_IDENTITY_PREFIX, VALIDATION_IDENTITY_HEX_LEN,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericValidationIdentity {
    pub identity: String,
    pub validation_tool: Option<&'static str>,
}

fn validation_like_purpose(purpose: Option<&str>) -> bool {
    purpose.is_some_and(is_validation_like_execution_purpose)
}

fn generic_validation_digest<'a>(
    source: &str,
    purpose: &str,
    cwd: Option<&str>,
    parts: impl IntoIterator<Item = &'a str>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-generic-validation-v1\0");
    hasher.update(source.as_bytes());
    hasher.update(b"\0");
    hasher.update(purpose.as_bytes());
    hasher.update(b"\0");
    hasher.update(cwd.unwrap_or(".").as_bytes());
    for part in parts {
        hasher.update(b"\0");
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!(
        "{GENERIC_VALIDATION_IDENTITY_PREFIX}{}",
        &digest[..VALIDATION_IDENTITY_HEX_LEN]
    )
}

fn canonical_cargo_validation_target(
    argv: &[String],
    cwd: Option<&str>,
) -> Option<(&'static str, String)> {
    let (subcommand, rest) = argv.split_first()?;
    let mut input = serde_json::Map::new();
    input.insert(
        "cwd".to_string(),
        Value::String(cwd.unwrap_or(".").to_string()),
    );
    let tool = match subcommand.as_str() {
        "fmt" => {
            let check = match rest {
                [] => false,
                [separator, check] if separator == "--" && check == "--check" => true,
                _ => return None,
            };
            input.insert("check".to_string(), Value::Bool(check));
            "cargo_fmt"
        }
        "check" | "test" => {
            let is_test = subcommand == "test";
            let mut packages = Vec::new();
            let mut features: Option<String> = None;
            let mut filter: Option<String> = None;
            let mut all_targets = false;
            let mut all_features = false;
            let mut no_default_features = false;
            let mut no_run = false;
            let mut lib = false;
            let mut index = 0;
            while index < rest.len() {
                let arg = &rest[index];
                match arg.as_str() {
                    "-p" | "--package" | "--features" => {
                        let value = rest.get(index + 1)?.clone();
                        if arg == "--features" {
                            if features.replace(value).is_some() {
                                return None;
                            }
                        } else {
                            if is_test && !packages.is_empty() {
                                return None;
                            }
                            packages.push(value);
                        }
                        index += 2;
                        continue;
                    }
                    "--all-targets" if !all_targets => all_targets = true,
                    "--lib" if is_test && !lib => lib = true,
                    "--all-features" if !all_features => all_features = true,
                    "--no-default-features" if !no_default_features => no_default_features = true,
                    "--no-run" if is_test && !no_run => no_run = true,
                    _ if arg.starts_with("--package=") && (!is_test || packages.is_empty()) => {
                        packages.push(arg.trim_start_matches("--package=").to_string());
                    }
                    _ if arg.starts_with("--features=") && features.is_none() => {
                        features = Some(arg.trim_start_matches("--features=").to_string());
                    }
                    _ if is_test && !arg.starts_with('-') && filter.is_none() => {
                        filter = Some(arg.to_string());
                    }
                    _ => return None,
                }
                index += 1;
            }
            let packages = normalize_cargo_packages(
                None,
                (!packages.is_empty()).then_some(packages.as_slice()),
            )
            .ok()?;
            let features = match features {
                Some(value) => normalize_cargo_value(&value).ok()?,
                None => None,
            };
            if is_test {
                input.insert(
                    "package".to_string(),
                    serde_json::json!(packages.and_then(|mut values| values.pop())),
                );
            } else {
                input.insert("packages".to_string(), serde_json::json!(packages));
            }
            input.insert("features".to_string(), serde_json::json!(features));
            input.insert("all_targets".to_string(), Value::Bool(all_targets));
            input.insert("all_features".to_string(), Value::Bool(all_features));
            input.insert(
                "no_default_features".to_string(),
                Value::Bool(no_default_features),
            );
            if is_test {
                let filter = match filter {
                    Some(value) => normalize_rust_test_filter(&value).ok()?,
                    None => None,
                };
                input.insert("filter".to_string(), serde_json::json!(filter));
                if lib {
                    input.insert("lib".to_string(), Value::Bool(true));
                }
                input.insert("no_run".to_string(), Value::Bool(no_run));
                "cargo_test"
            } else {
                "cargo_check"
            }
        }
        _ => return None,
    };
    let identity_kind =
        webcodex_tool_contracts::runtime_tool_session_evidence_policy(tool).validation_identity;
    let identity = structured_validation_target_identity(identity_kind, &Value::Object(input))?;
    Some((tool, identity))
}

pub fn run_process_validation_identity(
    executable: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&str>,
    purpose: Option<&str>,
) -> Option<GenericValidationIdentity> {
    if !validation_like_purpose(purpose) {
        return None;
    }
    if executable == "cargo" && stdin.is_none() {
        if let Some((validation_tool, identity)) = canonical_cargo_validation_target(args, cwd) {
            return Some(GenericValidationIdentity {
                identity,
                validation_tool: Some(validation_tool),
            });
        }
    }
    let purpose = purpose?;
    let mut parts = Vec::with_capacity(args.len() + 2);
    parts.push(executable);
    parts.extend(args.iter().map(String::as_str));
    if let Some(stdin) = stdin {
        parts.push(stdin);
    }
    Some(GenericValidationIdentity {
        identity: generic_validation_digest("run_process", purpose, cwd, parts),
        validation_tool: None,
    })
}

fn simple_script_argv(script: &str) -> Option<Vec<String>> {
    let trimmed = script.trim();
    if trimmed.is_empty()
        || trimmed.lines().count() != 1
        || trimmed.chars().any(|character| {
            matches!(
                character,
                ';' | '|' | '&' | '$' | '`' | '\\' | '\'' | '"' | '<' | '>' | '(' | ')' | '{' | '}'
            )
        })
    {
        return None;
    }
    let argv = trimmed
        .split_whitespace()
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!argv.is_empty()).then_some(argv)
}

pub fn run_script_validation_identity(
    language: &str,
    script: &str,
    args: &[String],
    stdin: Option<&str>,
    cwd: Option<&str>,
    purpose: Option<&str>,
) -> Option<GenericValidationIdentity> {
    if !validation_like_purpose(purpose) {
        return None;
    }
    if matches!(language, "sh" | "bash") && args.is_empty() && stdin.is_none() {
        if let Some(argv) = simple_script_argv(script) {
            if argv.first().is_some_and(|program| program == "cargo") {
                if let Some((validation_tool, identity)) =
                    canonical_cargo_validation_target(&argv[1..], cwd)
                {
                    return Some(GenericValidationIdentity {
                        identity,
                        validation_tool: Some(validation_tool),
                    });
                }
            }
        }
    }
    let purpose = purpose?;
    let mut parts = Vec::with_capacity(args.len() + 3);
    parts.push(language);
    parts.push(script);
    parts.extend(args.iter().map(String::as_str));
    if let Some(stdin) = stdin {
        parts.push(stdin);
    }
    Some(GenericValidationIdentity {
        identity: generic_validation_digest("run_script", purpose, cwd, parts),
        validation_tool: None,
    })
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
            Self::RunProcess {
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
                let identity = run_process_validation_identity(
                    executable,
                    args,
                    stdin.as_deref(),
                    cwd.as_deref(),
                    purpose.as_ref().map(|purpose| purpose.as_str()),
                );
                let mut value = serde_json::json!({
                    "project": project,
                    "executable_present": true,
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
                config,
                timeout_secs,
                recording_session_id: _,
            } => serde_json::json!({
                "project": project,
                "provider_id": provider_id,
                "idempotency_key_present": !idempotency_key.is_empty(),
                "instruction_bytes": instruction.len(),
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
            Self::ProjectValidate {
                project,
                cwd,
                action,
                adapter,
                scope,
                timeout_secs,
                ..
            } => serde_json::json!({
                "project": project,
                "cwd": cwd,
                "action": action,
                "adapter": adapter,
                "packages_present": scope.is_some(),
                "package_count": scope.as_ref().map(|scope| scope.packages.len()).unwrap_or_default(),
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
            #[cfg(feature = "legacy-gpt-actions")]
            Self::AttachAgentEndpoint {
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
                summary_only,
                include_diff,
                include_workspace,
                include_hygiene,
                include_handoff,
                include_validation_summary,
            } => serde_json::json!({
                "project": project,
                "session_id": session_id,
                "summary_only": summary_only,
                "include_diff": include_diff,
                "include_workspace": include_workspace,
                "include_hygiene": include_hygiene,
                "include_handoff": include_handoff,
                "include_validation_summary": include_validation_summary,
            }),
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
            Self::ListProjects {
                client_id,
                project,
                query,
                limit,
                summary_only,
            } => serde_json::json!({
                "client_id_present": client_id.is_some(),
                "project_present": project.is_some(),
                "query_present": query.is_some(),
                "query_length": query.as_deref().map(|value| value.chars().count()).unwrap_or_default(),
                "limit": limit,
                "summary_only": summary_only,
            }),
            Self::ListRunners {
                client_id,
                client_ids,
                include_projects,
                summary_only,
            } => serde_json::json!({
                "client_id_present": client_id.is_some(),
                "client_ids_count": client_ids.as_ref().map(Vec::len).unwrap_or_default(),
                "include_projects": include_projects,
                "summary_only": summary_only,
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
                category,
                intent,
                include_recommended_flows,
                include_risk_summary,
            } => serde_json::json!({
                "tool_name": tool_name,
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
                offset,
                limit,
                payload_index,
            } => serde_json::json!({
                "trace_ref": trace_ref,
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
