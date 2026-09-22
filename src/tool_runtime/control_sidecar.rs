//! Explicit, bounded control mutations carried by an ordinary model invocation.
//! Payloads parse as a closed sum; canonical ToolCall parsing and kernel dispatch
//! remain the sole business implementation and authority path.

use super::kernel::{
    check_runtime_tool_scope, ToolCallContext, ToolCallErrorStatus, ToolCallOutcome,
    ToolCallRequest, ToolInvocationMetadata, ToolProtocolCapabilities, ToolTransport,
};
use super::{ToolCall, ToolResult, ToolRuntime};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use webcodex_core::workflow_session_contract::{
    SessionExecutionContext, SessionMessageKind, SessionMessagePriority,
};

pub(crate) const CONTROL_FIELD: &str = "_control";
pub(crate) const MAX_CONTROL_COMMUNICATION_MESSAGES: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ControlSidecars {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) before: Option<BeforeControl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) communication: Option<CommunicationControl>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) after_success: Option<AfterSuccessControl>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct CommunicationControl {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) before: Vec<CommunicationMessage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) after_success: Vec<CommunicationMessage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommunicationControlWire {
    #[serde(default)]
    before: Vec<CommunicationMessage>,
    #[serde(default)]
    after_success: Vec<CommunicationMessage>,
}

impl<'de> Deserialize<'de> for CommunicationControl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = CommunicationControlWire::deserialize(deserializer)?;
        if wire.before.is_empty() && wire.after_success.is_empty() {
            return Err(serde::de::Error::custom(
                "communication requires at least one before or after_success message",
            ));
        }
        Ok(Self {
            before: wire.before,
            after_success: wire.after_success,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum CommunicationMessage {
    SessionMessage {
        session_id: String,
        kind: SessionMessageKind,
        message: String,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        reply_to: Option<String>,
        #[serde(default)]
        priority: SessionMessagePriority,
        #[serde(default)]
        requires_ack: bool,
        delivery_key: String,
    },
    PeerMessage {
        peer_id: String,
        kind: SessionMessageKind,
        message: String,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        priority: SessionMessagePriority,
        #[serde(default)]
        requires_ack: bool,
        delivery_key: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BeforeControl {
    GoalProgress {
        goal_id: String,
        expected_revision: i64,
        #[serde(default)]
        completed_step_ids: Vec<String>,
        #[serde(default)]
        current_step_id: Option<String>,
        summary: String,
        idempotency_key: String,
    },
    WakeConsume {
        agent_id: String,
        endpoint_id: String,
        expected_controller_generation: i64,
        wake_id: String,
        consume_token: String,
    },
    AttemptHeartbeat {
        task_id: String,
        attempt_id: String,
        assignee_agent_id: String,
        attempt_fence: String,
        attempt_controller_generation: i64,
        #[serde(default)]
        active_turn_wake_id: Option<String>,
        #[serde(default)]
        active_turn_consume_token: Option<String>,
    },
    SessionContextUpdate {
        project: String,
        session_id: String,
        execution_context: SessionExecutionContext,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AfterSuccessControl {
    GoalCompletion {
        goal_id: String,
        expected_revision: i64,
        #[serde(default)]
        terminal_reason: Option<String>,
        idempotency_key: String,
    },
    SessionClose {
        session_id: String,
    },
    TodoCompletion {
        session_id: String,
        message_id: String,
        answer: String,
        completion_key: String,
        expected_assignment_fence: String,
        #[serde(default)]
        tags: Vec<String>,
        #[serde(default)]
        priority: SessionMessagePriority,
    },
}

impl BeforeControl {
    fn kind(&self) -> &'static str {
        match self {
            Self::GoalProgress { .. } => "goal_progress",
            Self::WakeConsume { .. } => "wake_consume",
            Self::AttemptHeartbeat { .. } => "attempt_heartbeat",
            Self::SessionContextUpdate { .. } => "session_context_update",
        }
    }
}

impl AfterSuccessControl {
    fn kind(&self) -> &'static str {
        match self {
            Self::GoalCompletion { .. } => "goal_completion",
            Self::SessionClose { .. } => "session_close",
            Self::TodoCompletion { .. } => "todo_completion",
        }
    }
}

impl CommunicationMessage {
    fn kind(&self) -> &'static str {
        match self {
            Self::SessionMessage { .. } => "session_message",
            Self::PeerMessage { .. } => "peer_message",
        }
    }
}

fn canonical_tool(kind: &str) -> &'static str {
    match kind {
        "goal_progress" => "checkpoint_goal",
        "wake_consume" => "consume_agent_wake",
        "attempt_heartbeat" => "heartbeat_agent_task_attempt",
        "session_context_update" => "update_session_context",
        "goal_completion" => "update_goal",
        "session_close" => "close_session",
        "todo_completion" => "complete_session_message",
        "session_message" => "post_session_message",
        "peer_message" => "post_peer_message",
        _ => unreachable!("closed control kind"),
    }
}

/// Specialized action gateways have their own pre-dispatch path. They are not
/// admitted in v1; neither are App-only/ModelHidden tools or nested adapters.
pub(crate) fn supports_control_sidecars(tool: &str) -> bool {
    super::tool_definition::is_model_visible_tool_name(tool)
        && !matches!(
            tool,
            "plugin_tool"
                | "ssh_resource"
                | "browser_observe"
                | "browser_act"
                | "computer_observe"
                | "computer_control"
        )
}

pub(crate) fn strip_control_sidecars(
    arguments: &mut Value,
    tool: &str,
    admitted: bool,
) -> Result<Option<ControlSidecars>, &'static str> {
    let Some(value) = arguments
        .as_object_mut()
        .and_then(|args| args.remove(CONTROL_FIELD))
    else {
        return Ok(None);
    };
    if !admitted || !supports_control_sidecars(tool) {
        return Err("_control is unavailable on this tool or adapter");
    }
    // Do not interpolate serde errors: unknown fields/variants can contain
    // private continuation material supplied by a malformed caller.
    serde_json::from_value(value).map(Some).map_err(|_| {
        "_control requires closed before/communication/after_success objects with bounded canonical operations"
    })
}

fn phase_schema(kinds: &[&str]) -> Value {
    let mut properties = serde_json::Map::new();
    for kind in kinds {
        let mut schema = webcodex_tool_contracts::input_schema_for_tool(canonical_tool(kind));
        if *kind == "goal_completion" {
            let fields = [
                "goal_id",
                "expected_revision",
                "idempotency_key",
                "terminal_reason",
            ];
            schema["properties"]
                .as_object_mut()
                .unwrap()
                .retain(|field, _| fields.contains(&field.as_str()));
            schema["required"]
                .as_array_mut()
                .unwrap()
                .retain(|field| fields.contains(&field.as_str().unwrap()));
        }
        if matches!(*kind, "session_message" | "peer_message") {
            let required = schema["required"].as_array_mut().unwrap();
            if !required.iter().any(|field| field == "delivery_key") {
                required.push(json!("delivery_key"));
            }
        }
        properties.insert((*kind).to_string(), schema);
    }
    json!({"type": "object", "additionalProperties": false,
        "minProperties": 1, "maxProperties": 1, "properties": properties})
}

fn communication_schema() -> Value {
    let phase = json!({
        "type": "array",
        "minItems": 1,
        "maxItems": MAX_CONTROL_COMMUNICATION_MESSAGES,
        "items": phase_schema(&["session_message", "peer_message"]),
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "minProperties": 1,
        "properties": {
            "before": phase.clone(),
            "after_success": phase,
        },
        "allOf": [
            {
                "if": {"required": ["before"], "properties": {"before": {"minItems": 2}}},
                "then": {"properties": {"after_success": {"maxItems": 0}}}
            },
            {
                "if": {"required": ["after_success"], "properties": {"after_success": {"minItems": 2}}},
                "then": {"properties": {"before": {"maxItems": 0}}}
            }
        ]
    })
}

/// Project canonical payload schemas; no new per-tool business parameters.
pub(crate) fn input_schema(tool: &str) -> Value {
    let after = if tool == "finish_coding_task" || tool == "call_runtime_tool" {
        vec!["goal_completion", "session_close", "todo_completion"]
    } else {
        vec!["todo_completion"]
    };
    json!({
        "type": "object", "additionalProperties": false,
        "description": "Optional piggyback optimization; omit without an explicit transition or communication. At most one mutation per phase, independently plus at most two replay-safe communication messages total. before records facts already true; mutation failure proves main definitely_not_started, while communication failure is reported independently. after_success runs only after known success. Goal/session close require finish_coding_task. session_context_update currently fails closed. Standalone tools remain valid.",
        "properties": {
            "before": phase_schema(&["goal_progress", "wake_consume", "attempt_heartbeat", "session_context_update"]),
            "communication": communication_schema(),
            "after_success": phase_schema(&after)
        }
    })
}

pub(crate) fn output_schema() -> Value {
    let phase = json!({"type": "object", "additionalProperties": false,
        "properties": {
            "kind": {"type": "string", "enum": ["goal_progress", "wake_consume", "attempt_heartbeat", "session_context_update", "goal_completion", "session_close", "todo_completion", "session_message", "peer_message"]},
            "success": {"type": "boolean"},
            "execution_state": {"type": "string", "enum": ["succeeded", "failed", "definitely_not_started", "outcome_unknown"]},
            "state_changed": {"type": ["boolean", "null"]},
            "replayed": {"type": "boolean"},
            "revision": {"type": "integer"},
            "error_kind": {"type": "string", "maxLength": 96},
            "message_id": {"type": "string", "maxLength": 160}
        }, "required": ["kind", "success", "execution_state", "state_changed", "replayed"]});
    json!({"type": "object", "additionalProperties": false,
        "description": "Per-phase truth. Top-level success continues to describe main only. After a post failure, recover the canonical mutation separately; no cross-domain transaction or main-call replay guarantee.",
        "properties": {
            "main": {"type": "object", "additionalProperties": false, "properties": {
                "success": {"type": "boolean"},
                "execution_state": {"type": "string", "enum": ["succeeded", "failed", "definitely_not_started", "started", "outcome_unknown"]},
                "state_changed": {"type": ["boolean", "null"]}
            }, "required": ["success", "execution_state", "state_changed"]},
            "before": phase.clone(),
            "communication": {"type": "object", "additionalProperties": false,
                "properties": {
                    "before": {"type": "array", "maxItems": MAX_CONTROL_COMMUNICATION_MESSAGES, "items": phase.clone()},
                    "after_success": {"type": "array", "maxItems": MAX_CONTROL_COMMUNICATION_MESSAGES, "items": phase.clone()}
                }},
            "after_success": phase
        }, "required": ["main"]})
}

fn request(kind: &str, value: impl Serialize) -> Result<ToolCallRequest, &'static str> {
    let mut value = serde_json::to_value(value).expect("typed sidecar serialization");
    let mut arguments = value.as_object_mut().unwrap().remove(kind).unwrap();
    if kind == "goal_completion" {
        arguments["lifecycle"] = json!("completed");
    }
    let tool_name = canonical_tool(kind).to_string();
    // Eagerly parse BOTH phases before any mutation, using the canonical parser.
    ToolCall::from_tool_name(&tool_name, arguments.clone())
        .map_err(|_| "invalid_control_arguments")?;
    Ok(ToolCallRequest {
        tool_name,
        arguments,
    })
}

fn communication_request(value: &CommunicationMessage) -> Result<ToolCallRequest, &'static str> {
    request(value.kind(), value)
}

fn not_started(kind: &str, reason: &'static str) -> Value {
    json!({"kind": kind, "success": false, "execution_state": "definitely_not_started",
        "state_changed": false, "replayed": false, "error_kind": reason})
}

fn rejection(kind: &'static str) -> ToolCallOutcome {
    rejection_result(ToolResult::err_with_output(
        kind,
        json!({"error_kind": kind}),
    ))
}

fn rejection_result(result: ToolResult) -> ToolCallOutcome {
    ToolCallOutcome {
        success: false,
        result: Some(result),
        error_status: None,
        project: None,
        model_ergonomics: None,
        correlation: Default::default(),
    }
}

/// Request-local observation only. It is never an idempotency ledger or a
/// cross-domain transaction. Every sidecar owns its existing canonical replay.
pub(crate) struct ControlExecution {
    sidecars: ControlSidecars,
    before: Option<Value>,
    communication_before: Vec<Value>,
    communication_after_success: Vec<Value>,
    after_success: Option<Value>,
    pub(crate) main_dispatched: bool,
    main: Option<Value>,
}

impl ControlExecution {
    pub(crate) fn new(sidecars: ControlSidecars) -> Self {
        Self {
            sidecars,
            before: None,
            communication_before: Vec::new(),
            communication_after_success: Vec::new(),
            after_success: None,
            main_dispatched: false,
            main: None,
        }
    }

    pub(crate) fn validate(
        &self,
        tool: &str,
        context: ToolCallContext<'_>,
        capabilities: ToolProtocolCapabilities,
        has_resolution: bool,
    ) -> Result<(), ToolCallOutcome> {
        if !capabilities.control_sidecars
            || context.transport != ToolTransport::Mcp
            || !supports_control_sidecars(tool)
        {
            return Err(rejection("control_sidecar_not_admitted"));
        }
        if has_resolution && self.sidecars.before.is_some() {
            return Err(rejection("multiple_before_mutations"));
        }
        if tool != "finish_coding_task"
            && matches!(
                self.sidecars.after_success,
                Some(
                    AfterSuccessControl::GoalCompletion { .. }
                        | AfterSuccessControl::SessionClose { .. }
                )
            )
        {
            return Err(rejection("control_requires_finish_coding_task"));
        }
        let communication = self.sidecars.communication.as_ref();
        let communication_count = communication.map_or(0, |communication| {
            communication.before.len() + communication.after_success.len()
        });
        if communication_count > MAX_CONTROL_COMMUNICATION_MESSAGES {
            return Err(rejection("control_communication_limit_exceeded"));
        }
        if communication.is_some_and(|communication| {
            communication.before.is_empty() && communication.after_success.is_empty()
        }) {
            return Err(rejection("control_communication_empty"));
        }
        let before = self
            .sidecars
            .before
            .as_ref()
            .map(|v| request(v.kind(), v))
            .transpose();
        let after = self
            .sidecars
            .after_success
            .as_ref()
            .map(|v| request(v.kind(), v))
            .transpose();
        for request in [before, after] {
            let request = request.map_err(rejection)?;
            if let Some(request) = request {
                if check_runtime_tool_scope(context.auth, &request.tool_name).is_err() {
                    return Err(rejection("control_insufficient_scope"));
                }
            }
        }
        if let Some(communication) = communication {
            for value in communication
                .before
                .iter()
                .chain(&communication.after_success)
            {
                let request = communication_request(value).map_err(rejection)?;
                if check_runtime_tool_scope(context.auth, &request.tool_name).is_err() {
                    return Err(rejection("control_insufficient_scope"));
                }
            }
        }
        Ok(())
    }

    pub(crate) async fn before(
        &mut self,
        runtime: &ToolRuntime,
        call: &ToolCall,
        context: ToolCallContext<'_>,
    ) -> Result<(), ToolCallOutcome> {
        // Authorize main business targets before committing an independent fact.
        // Its canonical dispatch still owns permissions and lifecycle guards.
        if let Some(session_id) = call.session_id() {
            runtime
                .authorize_session_target(session_id, call.tool_name(), context.auth)
                .await
                .map_err(rejection_result)?;
        }
        if let Some(project) = call.project() {
            runtime
                .resolve_project_input_for_auth(project, context.auth)
                .await
                .map_err(|error| rejection_result(error.into_tool_result()))?;
        }
        let Some(value) = &self.sidecars.before else {
            self.execute_communication_before(runtime, context).await;
            return Ok(());
        };
        let kind = value.kind();
        if matches!(value, BeforeControl::SessionContextUpdate { .. }) {
            self.before = Some(not_started(
                kind,
                "session_context_replay_contract_required",
            ));
            return Err(rejection("session_context_replay_contract_required"));
        }
        let projection = execute(
            runtime,
            request(kind, value).map_err(rejection)?,
            kind,
            context,
        )
        .await;
        let success = projection["success"] == true;
        self.before = Some(projection);
        if success {
            self.execute_communication_before(runtime, context).await;
            Ok(())
        } else {
            Err(rejection("control_before_failed"))
        }
    }

    async fn execute_communication_before(
        &mut self,
        runtime: &ToolRuntime,
        context: ToolCallContext<'_>,
    ) {
        let values = self
            .sidecars
            .communication
            .as_ref()
            .map(|communication| communication.before.clone())
            .unwrap_or_default();
        for value in values {
            self.communication_before.push(
                execute(
                    runtime,
                    communication_request(&value).expect("prevalidated communication"),
                    value.kind(),
                    context,
                )
                .await,
            );
        }
    }

    pub(crate) async fn after(
        &mut self,
        runtime: &ToolRuntime,
        tool: &str,
        result: &ToolResult,
        context: ToolCallContext<'_>,
    ) {
        let state = main_execution_state(result, self.main_dispatched);
        self.main = Some(main_projection(result, self.main_dispatched));
        let reason = if !result.success {
            Some("main_failed")
        } else if state != "succeeded" {
            Some("main_not_completed")
        } else if tool == "finish_coding_task"
            && result.output.pointer("/task_outcome/blocking") != Some(&Value::Bool(false))
        {
            Some("closeout_blocking")
        } else {
            None
        };
        if let Some(value) = &self.sidecars.after_success {
            let kind = value.kind();
            self.after_success = Some(if let Some(reason) = reason {
                not_started(kind, reason)
            } else {
                execute(
                    runtime,
                    request(kind, value).expect("prevalidated sidecar"),
                    kind,
                    context,
                )
                .await
            });
        }
        let values = self
            .sidecars
            .communication
            .as_ref()
            .map(|communication| communication.after_success.clone())
            .unwrap_or_default();
        for value in values {
            self.communication_after_success
                .push(if let Some(reason) = reason {
                    not_started(value.kind(), reason)
                } else {
                    execute(
                        runtime,
                        communication_request(&value).expect("prevalidated communication"),
                        value.kind(),
                        context,
                    )
                    .await
                });
        }
    }

    pub(crate) fn decorate(self, outcome: &mut ToolCallOutcome) {
        // Control-bearing requests always have a structured result, even when
        // the main request was denied before dispatch. No authority text/payload
        // from a failed nested call is copied into the projection.
        if outcome.result.is_none() {
            let kind = match outcome.error_status.take() {
                Some(ToolCallErrorStatus::InsufficientScope { .. }) => "insufficient_scope",
                _ => "invalid_arguments",
            };
            outcome.result = Some(ToolResult::err_with_output(
                kind,
                json!({"error_kind": kind}),
            ));
        }
        let result = outcome.result.as_mut().unwrap();
        let main = self
            .main
            .unwrap_or_else(|| main_projection(result, self.main_dispatched));
        let mut control = json!({"main": main});
        if let Some(value) = self.sidecars.before {
            control["before"] = self
                .before
                .unwrap_or_else(|| not_started(value.kind(), "request_rejected"));
        }
        if let Some(value) = self.sidecars.after_success {
            control["after_success"] = self
                .after_success
                .unwrap_or_else(|| not_started(value.kind(), "main_not_started"));
        }
        if let Some(communication) = self.sidecars.communication {
            let mut projection = serde_json::Map::new();
            if !communication.before.is_empty() {
                projection.insert(
                    "before".to_string(),
                    Value::Array(if self.communication_before.is_empty() {
                        communication
                            .before
                            .iter()
                            .map(|value| not_started(value.kind(), "request_rejected"))
                            .collect()
                    } else {
                        self.communication_before
                    }),
                );
            }
            if !communication.after_success.is_empty() {
                projection.insert(
                    "after_success".to_string(),
                    Value::Array(if self.communication_after_success.is_empty() {
                        communication
                            .after_success
                            .iter()
                            .map(|value| not_started(value.kind(), "main_not_started"))
                            .collect()
                    } else {
                        self.communication_after_success
                    }),
                );
            }
            control["communication"] = Value::Object(projection);
        }
        if !result.output.is_object() {
            result.output = json!({});
        }
        result.output["control"] = control;
    }
}

fn main_projection(result: &ToolResult, dispatched: bool) -> Value {
    let state = main_execution_state(result, dispatched);
    let changed = match state {
        "definitely_not_started" => Some(false),
        "outcome_unknown" => None,
        _ => result.output["state_changed"].as_bool(),
    };
    json!({"success": result.success, "execution_state": state, "state_changed": changed})
}

fn main_execution_state(result: &ToolResult, dispatched: bool) -> &'static str {
    if !dispatched
        || result.output["execution_state"] == "not_started"
        || result.output["dispatch_certainty"] == "not_started"
        || result.output["error_kind"] == "permission_denied"
    {
        "definitely_not_started"
    } else if result.output["execution_state"] == "outcome_unknown"
        || result.output["failure_kind"] == "outcome_unknown"
    {
        "outcome_unknown"
    } else if matches!(
        result.output["execution_state"].as_str(),
        Some("queued" | "running" | "started" | "pending")
    ) || result.output["terminal"] == false
        || result.output["promoted_to_job"] == true
    {
        "started"
    } else if result.success {
        "succeeded"
    } else {
        "failed"
    }
}

async fn execute(
    runtime: &ToolRuntime,
    request: ToolCallRequest,
    kind: &str,
    context: ToolCallContext<'_>,
) -> Value {
    // The canonical kernel rechecks scopes, recorder/business authority,
    // permission/Session guards and then calls the SAME domain service. No
    // sidecars or capabilities propagate into this nested invocation.
    let outcome = runtime
        .call_tool_with_invocation_metadata(
            request,
            context,
            ToolInvocationMetadata::default(),
            ToolProtocolCapabilities::default(),
        )
        .await;
    let Some(result) = outcome.result else {
        return not_started(
            kind,
            match outcome.error_status {
                Some(ToolCallErrorStatus::InsufficientScope { .. }) => "insufficient_scope",
                _ => "invalid_arguments",
            },
        );
    };
    project_result(kind, &result)
}

fn project_result(kind: &str, result: &ToolResult) -> Value {
    let output = &result.output;
    let replayed = output["replayed"]
        .as_bool()
        .or_else(|| output["already_closed"].as_bool())
        .or_else(|| output["already_consumed"].as_bool())
        .unwrap_or(false);
    let prestart = !result.success
        && (output["error_kind"] == "permission_denied"
            || output["execution_state"] == "not_started"
            || output["dispatch_certainty"] == "not_started");
    let changed = output["state_changed"].as_bool().or_else(|| {
        if prestart {
            Some(false)
        } else {
            result.success.then_some(
                !replayed
                    || output["persistent_shells_closed"]
                        .as_u64()
                        .is_some_and(|count| count > 0),
            )
        }
    });
    let error_kind = output["error_kind"]
        .as_str()
        .or_else(|| output["failure_kind"].as_str())
        .filter(|kind| {
            kind.len() <= 96
                && kind
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
        });
    let unknown = !result.success
        && (changed.is_none()
            || output["execution_state"] == "outcome_unknown"
            || output["failure_kind"] == "outcome_unknown"
            || error_kind.is_some_and(|kind| kind.ends_with("serialization_failed")));
    let mut projection = json!({"kind": kind, "success": result.success,
        "execution_state": if result.success { "succeeded" } else if prestart { "definitely_not_started" } else if unknown { "outcome_unknown" } else { "failed" },
        "state_changed": if unknown { None } else { changed }, "replayed": replayed});
    if let Some(revision) = output
        .pointer("/goal/summary/revision")
        .and_then(Value::as_i64)
        .or_else(|| output["current_revision"].as_i64())
    {
        projection["revision"] = json!(revision);
    }
    if let Some(message_id) = output["message_id"]
        .as_str()
        .filter(|message_id| message_id.len() <= 160 && message_id.starts_with("wc_msg_"))
    {
        projection["message_id"] = json!(message_id);
    }
    if !result.success {
        projection["error_kind"] = json!(error_kind.unwrap_or("control_mutation_failed"));
    }
    projection
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_sidecars_projection_bounds_privacy_and_uncertainty() {
        let result = ToolResult::err_with_output(
            "PRIVATE",
            json!({
                "error_kind": "goal_result_serialization_failed", "state_changed": false,
                "consume_token": "PRIVATE", "attempt_fence": "PRIVATE", "answer": "PRIVATE".repeat(8000)
            }),
        );
        let projection = project_result("goal_completion", &result);
        assert_eq!(projection["execution_state"], "outcome_unknown");
        assert!(projection["state_changed"].is_null());
        assert!(!projection.to_string().contains("PRIVATE"));
        assert!(projection.to_string().len() < 512);
        let delivery = ToolResult::err_with_output(
            "persistence uncertain",
            json!({
                "error_kind": "message_delivery_persistence_uncertain",
                "failure_kind": "outcome_unknown",
                "state_changed": true,
            }),
        );
        let delivery_projection = project_result("session_message", &delivery);
        assert_eq!(delivery_projection["execution_state"], "outcome_unknown");
        assert!(delivery_projection["state_changed"].is_null());
        let schema = output_schema();
        let control = json!({"main": {"success": true, "execution_state": "succeeded", "state_changed": null}, "after_success": projection});
        webcodex_tool_contracts::test_support::validate_schema_instance(&control, &schema).unwrap();
    }
}
