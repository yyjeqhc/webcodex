//! Request-field allowlists. Raw inputs are never an audit fallback.
use super::*;

pub(super) fn browser_observe_audit_projection(call: &BrowserObserveToolCall) -> Value {
    serde_json::to_value(call).unwrap_or_else(|_| {
        serde_json::json!({
            "action": call.action_name()
        })
    })
}

pub(super) fn browser_act_audit_projection(call: &BrowserActToolCall) -> Value {
    match call {
        BrowserActToolCall::Batch {
            client_id,
            browser_id,
            page_id,
            operations,
        } => serde_json::json!({
            "action": "batch",
            "client_id": client_id,
            "browser_id": browser_id,
            "page_id": page_id,
            "operation_count": operations.len(),
        }),
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

pub(super) fn computer_observe_audit_projection(call: &ComputerObserveToolCall) -> Value {
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

pub(super) fn computer_control_audit_projection(call: &ComputerControlToolCall) -> Value {
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
pub(super) enum StructuredValidationRequestAudit {
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

pub(super) fn typed_structured_validation_request_audit(
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
pub(super) enum GoalRequestAudit {
    Create,
    Prepare,
    Get,
    List,
    Update,
    AssociateAgentTask,
    AssociateWorkflowSession,
}

pub(super) fn typed_goal_request_audit(kind: GoalRequestAudit, arguments: &Value) -> Value {
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
pub(super) enum AgentTaskRequestAudit {
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

pub(super) fn typed_agent_task_request_audit(
    kind: AgentTaskRequestAudit,
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
            for key in [
                "project",
                "attempt_ref",
                "task_id",
                "attempt_id",
                "assignee_agent_id",
                "attempt_controller_generation",
                "provider_id",
                "timeout_secs",
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
pub(super) enum CommunicationRequestAudit {
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

pub(super) fn typed_communication_request_audit(
    kind: CommunicationRequestAudit,
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
pub(super) enum MemoryRequestAudit {
    Search,
    Read,
    Set,
    Delete,
    ScopeList,
    ScopePurge,
}

pub(super) fn typed_memory_request_audit(kind: MemoryRequestAudit, arguments: &Value) -> Value {
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
pub(super) enum SkillRequestAudit {
    Versions,
    Install,
    Activate,
    RemoveRevision,
}

pub(super) fn typed_skill_request_audit(kind: SkillRequestAudit, arguments: &Value) -> Value {
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
