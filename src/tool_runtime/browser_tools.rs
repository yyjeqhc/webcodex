use super::sessions::SessionTransport;
use super::specialized::{
    SpecializedGovernanceDenial, SpecializedOperationPolicy, SpecializedSource,
};
use super::tool_call::{BrowserActToolCall, BrowserObserveToolCall};
use super::{SuggestedToolCall, ToolCall, ToolResult, ToolRuntime};
use crate::auth::{
    AuthContext, SCOPE_BROWSER_CONTROL, SCOPE_BROWSER_LAUNCH, SCOPE_BROWSER_READ,
    SCOPE_PROJECT_READ,
};
use crate::runner_http::RunnerFeature;
use serde_json::{json, Value};
use std::time::Duration;

const BROWSER_WAIT_SECS: u64 = 30;
const MAX_BROWSER_TARGETS: usize = 64;
const MAX_BROWSER_PAGES: usize = 32;

fn browser_snapshot_payload(
    browser_id: &str,
    page_id: &str,
    mode: &str,
    max_nodes: Option<usize>,
    max_depth: Option<u32>,
) -> Value {
    let mut payload = json!({
        "browser_id": browser_id,
        "page_id": page_id,
    });
    if mode != "auto" {
        payload["mode"] = json!(mode);
    }
    if let Some(max_nodes) = max_nodes {
        payload["max_nodes"] = json!(max_nodes);
    }
    if let Some(max_depth) = max_depth {
        payload["max_depth"] = json!(max_depth);
    }
    payload
}

fn browser_diagnostics_payload(
    browser_id: &str,
    page_id: &str,
    include_all_console: bool,
    include_all_network: bool,
    since_cursor: Option<u64>,
) -> Value {
    let mut payload = json!({
        "browser_id": browser_id,
        "page_id": page_id,
        "include_all_console": include_all_console,
        "include_all_network": include_all_network,
    });
    if let Some(since_cursor) = since_cursor {
        payload["since_cursor"] = json!(since_cursor);
    }
    payload
}

fn browser_observe_policy(call: &BrowserObserveToolCall) -> SpecializedOperationPolicy {
    SpecializedOperationPolicy::read(
        SpecializedSource::Browser,
        call.action_name(),
        SCOPE_BROWSER_READ,
    )
}

fn browser_act_policy(call: &BrowserActToolCall) -> SpecializedOperationPolicy {
    match call {
        BrowserActToolCall::Launch { .. } => SpecializedOperationPolicy::consequential(
            SpecializedSource::Browser,
            call.action_name(),
            SCOPE_BROWSER_LAUNCH,
            "browser_control",
        ),
        BrowserActToolCall::UploadFile { .. } => SpecializedOperationPolicy::consequential_all(
            SpecializedSource::Browser,
            call.action_name(),
            &[SCOPE_BROWSER_CONTROL, SCOPE_PROJECT_READ],
            "browser_file_upload",
        ),
        _ => SpecializedOperationPolicy::consequential(
            SpecializedSource::Browser,
            call.action_name(),
            SCOPE_BROWSER_CONTROL,
            "browser_control",
        ),
    }
}

fn browser_specialized_terminal(result: &ToolResult) -> (&str, Option<&str>) {
    let dispatch_certainty = result
        .output
        .get("execution_state")
        .and_then(Value::as_str)
        .unwrap_or(if result.success {
            "completed"
        } else {
            "not_started"
        });
    let failure_kind = result
        .output
        .get("failure_kind")
        .or_else(|| result.output.get("error_kind"))
        .and_then(Value::as_str);
    (dispatch_certainty, failure_kind)
}

impl ToolRuntime {
    pub(crate) async fn invoke_browser_observe_gateway(
        &self,
        call: BrowserObserveToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> Result<ToolResult, SpecializedGovernanceDenial> {
        let policy = browser_observe_policy(&call);
        let identity = json!({"action": call.action_name()});
        let permit = self
            .govern_specialized_invocation(
                "browser_observe",
                policy,
                transport,
                recording_session_id,
                auth,
                &identity,
            )
            .await?;
        let result = self
            .dispatch_browser_tool(ToolCall::BrowserObserve(call), auth)
            .await;
        let (dispatch_certainty, failure_kind) = browser_specialized_terminal(&result);
        self.finish_specialized_invocation(
            permit,
            result.success,
            dispatch_certainty,
            failure_kind,
        );
        Ok(result)
    }

    pub(crate) async fn invoke_browser_act_gateway(
        &self,
        call: BrowserActToolCall,
        recording_session_id: Option<&str>,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> Result<ToolResult, SpecializedGovernanceDenial> {
        let policy = browser_act_policy(&call);
        let identity = json!({"action": call.action_name()});
        let permit = self
            .govern_specialized_invocation(
                "browser_act",
                policy,
                transport,
                recording_session_id,
                auth,
                &identity,
            )
            .await?;
        let result = self
            .dispatch_browser_tool(ToolCall::BrowserAct(call), auth)
            .await;
        let (dispatch_certainty, failure_kind) = browser_specialized_terminal(&result);
        self.finish_specialized_invocation(
            permit,
            result.success,
            dispatch_certainty,
            failure_kind,
        );
        Ok(result)
    }

    pub(super) async fn dispatch_browser_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::BrowserObserve(BrowserObserveToolCall::Targets) => {
                self.browser_list_targets(auth).await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Browsers { client_id }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_list_browsers",
                    json!({}),
                    auth,
                    false,
                    BrowserRecoveryContext::browsers(&client_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Pages {
                client_id,
                browser_id,
                limit,
            }) => {
                let limit = limit
                    .unwrap_or(MAX_BROWSER_PAGES)
                    .clamp(1, MAX_BROWSER_PAGES);
                self.dispatch_browser_request(
                    &client_id,
                    "browser_list_pages",
                    json!({"browser_id": browser_id, "limit": limit}),
                    auth,
                    false,
                    BrowserRecoveryContext::pages(&client_id, &browser_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Snapshot {
                client_id,
                browser_id,
                page_id,
                mode,
                max_nodes,
                max_depth,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_snapshot",
                    browser_snapshot_payload(
                        &browser_id,
                        &page_id,
                        mode.as_str(),
                        max_nodes,
                        max_depth,
                    ),
                    auth,
                    false,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Console {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_console",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    false,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Network {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_network",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    false,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Diagnostics {
                client_id,
                browser_id,
                page_id,
                include_all_console,
                include_all_network,
                since_cursor,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_diagnostics",
                    browser_diagnostics_payload(
                        &browser_id,
                        &page_id,
                        include_all_console,
                        include_all_network,
                        since_cursor,
                    ),
                    auth,
                    false,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserObserve(BrowserObserveToolCall::Screenshot {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_screenshot",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    false,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::Launch { client_id }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_launch",
                    json!({}),
                    auth,
                    true,
                    BrowserRecoveryContext::browsers(&client_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::NewPage {
                client_id,
                browser_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_new_page",
                    json!({"browser_id": browser_id}),
                    auth,
                    true,
                    BrowserRecoveryContext::pages(&client_id, &browser_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::Navigate {
                client_id,
                browser_id,
                page_id,
                url,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_navigate",
                    json!({"browser_id": browser_id, "page_id": page_id, "url": url}),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::Reload {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_reload",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::Click {
                client_id,
                browser_id,
                page_id,
                element_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_click",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "element_id": element_id
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::InputText {
                client_id,
                browser_id,
                page_id,
                element_id,
                text,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_input_text",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "element_id": element_id,
                        "text": text
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::SelectOption {
                client_id,
                browser_id,
                page_id,
                element_id,
                option,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_select_option",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "element_id": element_id,
                        "option": option
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::SetValue {
                client_id,
                browser_id,
                page_id,
                element_id,
                value,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_set_value",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "element_id": element_id,
                        "value": value
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::UploadFile {
                client_id,
                browser_id,
                page_id,
                element_id,
                project,
                path,
            }) => {
                let resolved = match self.resolve_project_for_auth(&project, auth).await {
                    Ok(resolved) => resolved,
                    Err(_) => {
                        return browser_error(
                            "project_access_denied",
                            "caller cannot access the upload source project",
                            "not_started",
                            false,
                            None,
                        )
                    }
                };
                if resolved.client_id != client_id {
                    return browser_error(
                        "project_runner_mismatch",
                        "upload source project does not belong to the target Browser Runner",
                        "not_started",
                        false,
                        None,
                    );
                }
                self.dispatch_browser_request(
                    &client_id,
                    "browser_upload_file",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "element_id": element_id,
                        "project_root": resolved.path,
                        "path": path
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::Key {
                client_id,
                browser_id,
                page_id,
                key,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_key",
                    json!({
                        "browser_id": browser_id,
                        "page_id": page_id,
                        "key": key.as_str()
                    }),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::ClearDiagnostics {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_clear_diagnostics",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    true,
                    BrowserRecoveryContext::snapshot(&client_id, &browser_id, &page_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::ClosePage {
                client_id,
                browser_id,
                page_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_close_page",
                    json!({"browser_id": browser_id, "page_id": page_id}),
                    auth,
                    true,
                    BrowserRecoveryContext::pages(&client_id, &browser_id),
                )
                .await
            }
            ToolCall::BrowserAct(BrowserActToolCall::CloseBrowser {
                client_id,
                browser_id,
            }) => {
                self.dispatch_browser_request(
                    &client_id,
                    "browser_close",
                    json!({"browser_id": browser_id}),
                    auth,
                    true,
                    BrowserRecoveryContext::browsers(&client_id),
                )
                .await
            }
            _ => browser_error(
                "invalid_request",
                "unsupported Browser request",
                "not_started",
                false,
                None,
            ),
        }
    }

    async fn browser_list_targets(&self, auth: Option<&AuthContext>) -> ToolResult {
        let clients = self
            .runner_registry
            .list_runner_semantic_views_for_auth(
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await;
        let mut total_count = 0usize;
        let mut targets = Vec::new();
        for client in clients {
            let browser_observe = client.supports(RunnerFeature::BrowserObserve);
            let browser_control = client.supports(RunnerFeature::BrowserControl);
            let browser_launch = client.supports(RunnerFeature::BrowserLaunch);
            if !browser_observe && !browser_control && !browser_launch {
                continue;
            }
            total_count = total_count.saturating_add(1);
            if targets.len() >= MAX_BROWSER_TARGETS {
                continue;
            }
            let view = client.view;
            targets.push(json!({
                "client_id": view.client_id,
                "display_name": view.display_name,
                "connected": view.connected,
                "capabilities": {
                    "browser_observe": browser_observe,
                    "browser_control": browser_control,
                    "browser_launch": browser_launch,
                }
            }));
        }
        let count = targets.len();
        ToolResult::ok(json!({
            "execution_state": "completed",
            "state_changed": false,
            "targets": targets,
            "count": count,
            "total_count": total_count,
            "truncated": total_count > count,
        }))
    }

    async fn dispatch_browser_request(
        &self,
        client_id: &str,
        kind: &'static str,
        payload: Value,
        auth: Option<&AuthContext>,
        effect: bool,
        recovery: BrowserRecoveryContext,
    ) -> ToolResult {
        if client_id.is_empty() || client_id.len() > 128 {
            return browser_error(
                "invalid_client",
                "client_id is invalid",
                "not_started",
                false,
                None,
            );
        }
        let required_feature = match kind {
            "browser_list_browsers"
            | "browser_list_pages"
            | "browser_snapshot"
            | "browser_screenshot"
            | "browser_console"
            | "browser_network"
            | "browser_diagnostics" => RunnerFeature::BrowserObserve,
            "browser_launch" => RunnerFeature::BrowserLaunch,
            "browser_new_page"
            | "browser_navigate"
            | "browser_reload"
            | "browser_click"
            | "browser_input_text"
            | "browser_select_option"
            | "browser_set_value"
            | "browser_upload_file"
            | "browser_key"
            | "browser_close_page"
            | "browser_clear_diagnostics"
            | "browser_close" => RunnerFeature::BrowserControl,
            _ => {
                return browser_error(
                    "invalid_request",
                    "unsupported Browser request kind",
                    "not_started",
                    false,
                    None,
                )
            }
        };
        let client = match self
            .runner_registry
            .get_runner_semantic_view_checked_for_auth(
                client_id,
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
            )
            .await
        {
            Ok(client) => client,
            Err(_) => {
                return browser_error(
                    "client_access_denied",
                    "caller cannot access the target Browser Runner",
                    "not_started",
                    false,
                    None,
                )
            }
        };
        if !client.supports(required_feature) {
            return browser_error(
                "capability_unavailable",
                &format!(
                    "target Runner does not advertise {}",
                    required_feature.as_wire_name()
                ),
                "not_started",
                false,
                None,
            );
        }
        let payload = match serde_json::to_string(&payload) {
            Ok(payload) => payload,
            Err(_) => {
                return browser_error(
                    "invalid_request",
                    "could not encode Browser request",
                    "not_started",
                    false,
                    None,
                )
            }
        };
        let requested_by = crate::runner_http::requested_by_from_auth(auth);
        let (request_id, receiver) = match self
            .runner_registry
            .enqueue_browser(
                client_id.to_string(),
                kind,
                payload,
                requested_by,
                crate::runner_http::runner_access_from_auth(auth).as_ref(),
                BROWSER_WAIT_SECS,
            )
            .await
        {
            Ok(value) => value,
            Err(error) => {
                return browser_error(
                    "dispatch_denied",
                    &format!("Browser request was not dispatched: {error}"),
                    "not_started",
                    false,
                    None,
                )
            }
        };
        let response = match tokio::time::timeout(
            Duration::from_secs(BROWSER_WAIT_SECS + 2),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) => response,
            Ok(Err(_)) if effect => {
                let dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                return browser_delivery_failure(
                    "Runner response channel closed before a terminal Browser effect result",
                    dispatched,
                    recovery,
                );
            }
            Err(_) if effect => {
                let dispatched = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                return browser_delivery_failure(
                    "Runner did not return a terminal Browser effect result in time",
                    dispatched,
                    recovery,
                );
            }
            Ok(Err(_)) => {
                return browser_error(
                    "runner_disconnected",
                    "Runner response channel closed",
                    "not_started",
                    false,
                    None,
                )
            }
            Err(_) => {
                return browser_error(
                    "runner_timeout",
                    "Runner did not return Browser observation in time",
                    "not_started",
                    false,
                    None,
                )
            }
        };
        if let Some(error) = response.error.as_deref() {
            if effect {
                return browser_delivery_failure(error, response.request_dispatched, recovery);
            }
            return browser_error("runner_error", error, "not_started", false, None);
        }
        if response.exit_code != Some(0) {
            if effect {
                return browser_delivery_failure(
                    "Runner Browser effect ended without a structured terminal result",
                    response.request_dispatched,
                    recovery,
                );
            }
            return browser_error(
                "runner_error",
                "Runner Browser observation failed",
                "not_started",
                false,
                None,
            );
        }
        let envelope: Value = match response
            .stdout
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
        {
            Ok(Some(value)) => value,
            _ if effect => {
                return browser_delivery_failure(
                    "Runner returned invalid Browser JSON after possible effect dispatch",
                    response.request_dispatched,
                    recovery,
                )
            }
            _ => {
                return browser_error(
                    "invalid_runner_response",
                    "Runner returned invalid Browser JSON",
                    "not_started",
                    false,
                    None,
                )
            }
        };
        if envelope.get("ok").and_then(Value::as_bool) == Some(true) {
            let Some(mut result) = envelope.get("result").cloned() else {
                return if effect {
                    browser_delivery_failure(
                        "Runner omitted Browser effect result after dispatch",
                        response.request_dispatched,
                        recovery,
                    )
                } else {
                    browser_error(
                        "invalid_runner_response",
                        "Runner omitted Browser observation result",
                        "not_started",
                        false,
                        None,
                    )
                };
            };
            let Some(object) = result.as_object_mut() else {
                return browser_error(
                    "invalid_runner_response",
                    "Runner Browser result must be an object or array-bearing object",
                    if effect {
                        "outcome_unknown"
                    } else {
                        "not_started"
                    },
                    false,
                    effect.then(|| {
                        recovery.to_recovery("reconcile Browser state before any further effect")
                    }),
                );
            };
            object.insert(
                "execution_state".to_string(),
                Value::String("completed".to_string()),
            );
            object.insert("state_changed".to_string(), Value::Bool(effect));
            return ToolResult::ok(result);
        }
        let error = envelope.get("error").cloned().unwrap_or_else(|| json!({}));
        let kind = error
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or("browser_error");
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Browser operation failed");
        let execution_state = envelope
            .get("execution_state")
            .or_else(|| error.get("execution_state"))
            .and_then(Value::as_str)
            .unwrap_or(if effect {
                "outcome_unknown"
            } else {
                "not_started"
            });
        let action = error.get("recovery_action").and_then(Value::as_str);
        let recovery_value = if execution_state == "outcome_unknown"
            || kind.starts_with("stale_")
            || action.is_some()
        {
            let reason = if execution_state == "outcome_unknown" {
                "effect outcome is uncertain; observe current Browser state and do not blindly retry"
            } else if kind.starts_with("stale_") {
                "Browser identity is stale; observe current state before acting"
            } else if execution_state == "completed" {
                "Browser effect completed but its model-facing state could not be reconciled; observe current state before any further effect"
            } else {
                "observe current Browser state before acting"
            };
            Some(recovery.for_action(action).to_recovery(reason))
        } else {
            None
        };
        browser_error(
            kind,
            message,
            execution_state,
            execution_state == "completed" && effect,
            recovery_value,
        )
    }
}

#[derive(Clone)]
struct BrowserRecoveryContext {
    client_id: String,
    browser_id: Option<String>,
    page_id: Option<String>,
    preferred: &'static str,
}

impl BrowserRecoveryContext {
    fn browsers(client_id: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            browser_id: None,
            page_id: None,
            preferred: "browsers",
        }
    }

    fn pages(client_id: &str, browser_id: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            browser_id: Some(browser_id.to_string()),
            page_id: None,
            preferred: "pages",
        }
    }

    fn snapshot(client_id: &str, browser_id: &str, page_id: &str) -> Self {
        Self {
            client_id: client_id.to_string(),
            browser_id: Some(browser_id.to_string()),
            page_id: Some(page_id.to_string()),
            preferred: "snapshot",
        }
    }

    fn for_action(mut self, action: Option<&str>) -> Self {
        self.preferred = match action {
            Some("browsers") => "browsers",
            Some("pages") => "pages",
            Some("snapshot") => "snapshot",
            _ => self.preferred,
        };
        self
    }

    fn suggested_call(&self) -> SuggestedToolCall {
        let arguments = match self.preferred {
            "browsers" => json!({
                "action": "browsers",
                "client_id": self.client_id,
            }),
            "pages" if self.browser_id.is_some() => json!({
                "action": "pages",
                "client_id": self.client_id,
                "browser_id": self.browser_id,
            }),
            "snapshot" if self.browser_id.is_some() && self.page_id.is_some() => json!({
                "action": "snapshot",
                "client_id": self.client_id,
                "browser_id": self.browser_id,
                "page_id": self.page_id,
            }),
            _ if self.browser_id.is_some() => json!({
                "action": "pages",
                "client_id": self.client_id,
                "browser_id": self.browser_id,
            }),
            _ => json!({
                "action": "browsers",
                "client_id": self.client_id,
            }),
        };
        SuggestedToolCall::new("browser_observe", arguments)
    }

    fn to_recovery(&self, reason: &str) -> Value {
        json!({
            "reason": reason,
            "suggested_call": self.suggested_call().to_value(),
        })
    }
}

fn browser_delivery_failure(
    message: &str,
    request_dispatched: Option<bool>,
    recovery: BrowserRecoveryContext,
) -> ToolResult {
    if request_dispatched == Some(false) {
        browser_error("not_started", message, "not_started", false, None)
    } else {
        browser_error(
            "outcome_unknown",
            message,
            "outcome_unknown",
            false,
            Some(recovery.to_recovery(
                "effect may have reached the Browser; reconcile with browser_observe and never blindly retry",
            )),
        )
    }
}

fn browser_error(
    kind: &str,
    message: &str,
    execution_state: &str,
    state_changed: bool,
    recovery: Option<Value>,
) -> ToolResult {
    let mut output = json!({
        "execution_state": execution_state,
        "state_changed": state_changed,
        "error_kind": kind,
        "message": message,
    });
    if let Some(recovery) = recovery {
        output["recovery"] = recovery;
    }
    ToolResult::err_with_output(message, output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enhanced_browser_payloads_preserve_legacy_default_wire_shape() {
        let snapshot = browser_snapshot_payload(
            "browser_abcdefghijklmnop",
            "page_abcdefghijklmnop",
            "auto",
            None,
            None,
        );
        assert_eq!(snapshot["browser_id"], "browser_abcdefghijklmnop");
        assert_eq!(snapshot["page_id"], "page_abcdefghijklmnop");
        assert!(snapshot.get("mode").is_none());
        assert!(snapshot.get("max_nodes").is_none());
        assert!(snapshot.get("max_depth").is_none());

        let enhanced = browser_snapshot_payload(
            "browser_abcdefghijklmnop",
            "page_abcdefghijklmnop",
            "interactive",
            Some(48),
            Some(10),
        );
        assert_eq!(enhanced["mode"], "interactive");
        assert_eq!(enhanced["max_nodes"], 48);
        assert_eq!(enhanced["max_depth"], 10);

        let diagnostics = browser_diagnostics_payload(
            "browser_abcdefghijklmnop",
            "page_abcdefghijklmnop",
            false,
            false,
            None,
        );
        assert!(diagnostics.get("since_cursor").is_none());
        let delta = browser_diagnostics_payload(
            "browser_abcdefghijklmnop",
            "page_abcdefghijklmnop",
            false,
            false,
            Some(42),
        );
        assert_eq!(delta["since_cursor"], 42);
    }

    #[test]
    fn browser_policy_matrix_is_action_sensitive_and_never_shell_like() {
        use crate::tool_runtime::specialized::{
            SpecializedAuthorityRequirement, SpecializedEffect,
        };
        let observe = browser_observe_policy(&BrowserObserveToolCall::Targets);
        assert_eq!(observe.source, SpecializedSource::Browser);
        assert_eq!(observe.effect, SpecializedEffect::Read);
        assert_eq!(
            observe.authority,
            SpecializedAuthorityRequirement::Scope(SCOPE_BROWSER_READ)
        );
        assert_eq!(
            observe.authority.first_missing(None),
            Some(SCOPE_BROWSER_READ)
        );
        assert!(!observe.write_like);
        assert!(!observe.shell_like);

        let launch = browser_act_policy(&BrowserActToolCall::Launch {
            client_id: "msi".to_string(),
        });
        assert_eq!(
            launch.authority,
            SpecializedAuthorityRequirement::Scope(SCOPE_BROWSER_LAUNCH)
        );
        assert_eq!(
            launch.authority.first_missing(None),
            Some(SCOPE_BROWSER_LAUNCH)
        );
        assert_eq!(launch.effect, SpecializedEffect::Management);
        assert!(launch.write_like);
        assert!(!launch.shell_like);
        assert_eq!(launch.risk, "browser_control");

        let navigate = browser_act_policy(&BrowserActToolCall::Navigate {
            client_id: "msi".to_string(),
            browser_id: "browser_abcdefghijklmnop".to_string(),
            page_id: "page_abcdefghijklmnop".to_string(),
            url: "https://example.test/".to_string(),
        });
        assert_eq!(
            navigate.authority,
            SpecializedAuthorityRequirement::Scope(SCOPE_BROWSER_CONTROL)
        );
        assert_eq!(
            navigate.authority.first_missing(None),
            Some(SCOPE_BROWSER_CONTROL)
        );
        assert_eq!(navigate.effect, SpecializedEffect::Management);
        assert!(navigate.write_like);
        assert!(!navigate.shell_like);

        let upload = browser_act_policy(&BrowserActToolCall::UploadFile {
            client_id: "msi".to_string(),
            browser_id: "browser_abcdefghijklmnop".to_string(),
            page_id: "page_abcdefghijklmnop".to_string(),
            element_id: "element_abcdefghijklmnop".to_string(),
            project: "agent:msi:resume".to_string(),
            path: "resume.pdf".to_string(),
        });
        assert_eq!(
            upload.authority,
            SpecializedAuthorityRequirement::All(&[SCOPE_BROWSER_CONTROL, SCOPE_PROJECT_READ])
        );
        assert_eq!(upload.risk, "browser_file_upload");
        assert!(upload.write_like);
        assert!(!upload.shell_like);
    }

    #[test]
    fn uncertain_effect_never_suggests_browser_act_retry() {
        let result = browser_delivery_failure(
            "timeout",
            Some(true),
            BrowserRecoveryContext::snapshot("runner", "browser_x", "page_x"),
        );
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert_eq!(
            result.output["recovery"]["suggested_call"]["tool"],
            "browser_observe"
        );
        assert_eq!(
            result.output["recovery"]["suggested_call"]["arguments"]["action"],
            "snapshot"
        );
    }

    #[test]
    fn known_undispatched_effect_is_not_started_without_retry_hint() {
        let result = browser_delivery_failure(
            "cancelled before dispatch",
            Some(false),
            BrowserRecoveryContext::browsers("runner"),
        );
        assert_eq!(result.output["execution_state"], "not_started");
        assert!(result.output.get("recovery").is_none());
    }
}
