//! Model-facing adapter for Runner-owned native Tool Plugins.
//!
//! Plugin processes and their executable configuration never cross the Runner
//! boundary.  This module resolves only caller-visible logical identities,
//! stores bounded describe observations, and dispatches the closed typed Plugin
//! gateway with opaque exact Runner/provider/schema bindings.

pub(crate) use webcodex_core::plugin::*;

use crate::auth::{AuthContext, SCOPE_PLUGIN_LOCAL};
use crate::tool_runtime::ToolRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

pub(crate) const PLUGIN_TOOL_NAME: &str = "plugin_tool";
const MAX_PLUGIN_BINDINGS: usize = 512;
const GATEWAY_WAIT_TIMEOUT: Duration = Duration::from_secs(125);

#[derive(Debug, Clone)]
struct PluginBinding {
    client_id: String,
    runner_instance_id: String,
    provider_id: String,
    provider_instance_id: String,
    tool_name: String,
    schema: PluginSchemaObservation,
}

#[derive(Default)]
struct BindingStore {
    values: HashMap<String, PluginBinding>,
    order: VecDeque<String>,
}

#[derive(Default)]
pub(crate) struct PluginGatewayRuntime {
    bindings: Mutex<BindingStore>,
}

impl PluginGatewayRuntime {
    fn remember(&self, value: PluginBinding) -> String {
        let mut store = self
            .bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let binding = loop {
            let candidate = format!("wc_pbind_{}", uuid::Uuid::new_v4().simple());
            if !store.values.contains_key(&candidate) {
                break candidate;
            }
        };
        store.order.push_back(binding.clone());
        store.values.insert(binding.clone(), value);
        while store.values.len() > MAX_PLUGIN_BINDINGS {
            let Some(oldest) = store.order.pop_front() else {
                break;
            };
            store.values.remove(&oldest);
        }
        binding
    }

    fn binding(&self, binding: &str) -> Option<PluginBinding> {
        self.bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values
            .get(binding)
            .cloned()
    }

    fn forget(&self, binding: &str) {
        let mut store = self
            .bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        store.values.remove(binding);
        store.order.retain(|candidate| candidate != binding);
    }

    #[cfg(test)]
    pub(crate) fn binding_count(&self) -> usize {
        self.bindings
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values
            .len()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedPluginRunner {
    pub(crate) client_id: String,
    pub(crate) runner_instance_id: String,
    pub(crate) display_name: Option<String>,
    pub(crate) startup_providers: Vec<StartupPluginProvider>,
}

/// One exact startup Tool binding projected from the current caller-visible
/// frozen Runner registrations.  The adapter recomputes uniqueness from these
/// candidates for every list/call; it is not a Server-side Plugin cache.
#[derive(Debug, Clone)]
pub(crate) struct StartupPluginToolCandidate {
    pub(crate) client_id: String,
    pub(crate) runner_instance_id: String,
    pub(crate) provider_id: String,
    pub(crate) provider_instance_id: String,
    pub(crate) tool: PluginTool,
}

#[derive(Debug)]
pub(crate) struct GatewayError {
    code: String,
    message: String,
    recovery: Option<&'static str>,
    dispatch_state: Option<PluginDispatchState>,
}

impl GatewayError {
    fn local(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recovery: None,
            dispatch_state: Some(PluginDispatchState::NotStarted),
        }
    }

    fn recovery(mut self, recovery: &'static str) -> Self {
        self.recovery = Some(recovery);
        self
    }
}

#[derive(Debug)]
enum GatewaySuccess {
    Metadata(Value),
    ToolResult(PluginToolResult),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginToolArguments {
    action: String,
    #[serde(default)]
    runner: Option<String>,
    #[serde(default)]
    plugin: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    binding: Option<String>,
    #[serde(default)]
    arguments: Option<Value>,
}

pub(crate) fn authorized(auth: Option<&AuthContext>) -> bool {
    auth.is_some_and(|auth| auth.has_scope(SCOPE_PLUGIN_LOCAL))
}

pub(crate) fn tool_spec() -> Value {
    json!({
        "name": PLUGIN_TOOL_NAME,
        "description": "Develop and call Runner-local WebCodex native Tool Plugins. Canonical discovery is list -> list(runner) -> list(runner, plugin) -> describe -> call: provider-level list observes only the current committed/effective exact provider and returns bounded tool names/titles without creating a binding. Prefer action=check before reload while developing: check rereads runner.toml, starts one disposable candidate, initializes and lists tools, never calls tools/call, and returns bounded WebCodex-generated diagnostics without mutating committed Plugin state. Then use reload -> list(runner, plugin) -> describe -> call; restart the Runner only for startup first-class promotion. list never checks, reloads, or mutates Plugin state. describe creates the opaque exact binding; call accepts only that binding plus arguments. Stale Runner/provider observations never retarget or replay, and Plugin execution configuration stays Runner-local.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["list", "check", "reload", "describe", "call"]
                },
                "runner": {
                    "type": "string",
                    "description": "Exact caller-visible Runner client id. Optional for list; required for list(plugin), check, reload, and describe; not accepted for call."
                },
                "plugin": {
                    "type": "string",
                    "description": "Logical Plugin provider id. Optional only for list when runner is also present; required for check and describe; not accepted for reload or call."
                },
                "tool": {
                    "type": "string",
                    "description": "Logical Plugin tool name. Required only for describe; not accepted for call."
                },
                "binding": {
                    "type": "string",
                    "pattern": "^wc_pbind_[0-9a-f]{32}$",
                    "description": "Opaque exact-observation handle returned by action=describe. Required for call. It does not grant authorization."
                },
                "arguments": {
                    "type": "object",
                    "description": "Required for action=call. Arguments must match the schema returned by the describe that created binding."
                }
            },
            "required": ["action"],
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": false
        }
    })
}

pub(crate) async fn call(
    runtime: &ToolRuntime,
    arguments: Value,
    auth: Option<&AuthContext>,
) -> Value {
    if !authorized(auth) {
        return gateway_error_result(GatewayError::local(
            "insufficient_scope",
            "local native Plugin access requires the plugin:local scope",
        ));
    }
    let parsed: PluginToolArguments = match serde_json::from_value(arguments) {
        Ok(parsed) => parsed,
        Err(_) => {
            return gateway_error_result(GatewayError::local(
                "invalid_arguments",
                "plugin_tool arguments are invalid",
            ))
        }
    };
    let result = match parsed.action.as_str() {
        "list" => list(runtime, parsed, auth)
            .await
            .map(GatewaySuccess::Metadata),
        "check" => check(runtime, parsed, auth)
            .await
            .map(GatewaySuccess::Metadata),
        "reload" => reload(runtime, parsed, auth)
            .await
            .map(GatewaySuccess::Metadata),
        "describe" => describe(runtime, parsed, auth)
            .await
            .map(GatewaySuccess::Metadata),
        "call" => call_plugin(runtime, parsed, auth)
            .await
            .map(GatewaySuccess::ToolResult),
        _ => Err(GatewayError::local(
            "invalid_action",
            "action must be one of list, check, reload, describe, or call",
        )),
    };
    render_gateway_result(result)
}

async fn list(
    runtime: &ToolRuntime,
    args: PluginToolArguments,
    auth: Option<&AuthContext>,
) -> Result<Value, GatewayError> {
    if args.tool.is_some() || args.binding.is_some() || args.arguments.is_some() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=list accepts only optional runner and plugin",
        ));
    }
    if args.plugin.is_some() && args.runner.is_none() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=list requires runner when plugin is provided",
        ));
    }
    if let Some(runner) = args.runner.as_deref() {
        let runner = resolve_runner(runtime, runner, auth).await?;
        if let Some(plugin) = args.plugin.as_deref() {
            let plugin = required_provider(Some(plugin))?;
            let (provider, tools, restart_required) =
                observe_effective_provider_tools(runtime, &runner, plugin, auth).await?;
            let mut value = sanitize_provider_view_for_runner(&runner, provider);
            value["runner"] = Value::String(runner.client_id.clone());
            value["toolCount"] = Value::from(tools.len());
            value["tools"] = Value::Array(
                tools
                    .into_iter()
                    .map(sanitize_tool_summary)
                    .collect::<Vec<_>>(),
            );
            value["firstClassRestartRequired"] = Value::Bool(restart_required);
            return Ok(value);
        }
        let response =
            execute_exact(runtime, &runner, PluginGatewayRequest::ProvidersList, auth).await?;
        let (providers, restart_required) = response_providers(response)?;
        return Ok(json!({
            "runner": runner.client_id,
            "plugins": providers
                .into_iter()
                .map(|provider| sanitize_provider_view_for_runner(&runner, provider))
                .collect::<Vec<_>>(),
            "firstClassRestartRequired": restart_required
        }));
    }

    let runners = visible_plugin_runners(runtime, auth)
        .await
        .into_iter()
        .map(|runner| {
            let mut value = json!({"runner": runner.client_id});
            if let Some(name) = runner.display_name {
                value["name"] = Value::String(name);
            }
            value
        })
        .collect::<Vec<_>>();
    Ok(json!({"runners": runners}))
}

async fn check(
    runtime: &ToolRuntime,
    args: PluginToolArguments,
    auth: Option<&AuthContext>,
) -> Result<Value, GatewayError> {
    if args.tool.is_some() || args.binding.is_some() || args.arguments.is_some() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=check accepts only runner and plugin",
        ));
    }
    let runner_id = required_runner(args.runner.as_deref())?;
    let plugin_id = required_provider(args.plugin.as_deref())?;
    let runner = resolve_runner(runtime, runner_id, auth).await?;
    let response = execute_exact(
        runtime,
        &runner,
        PluginGatewayRequest::Check {
            provider_id: plugin_id.to_string(),
        },
        auth,
    )
    .await?;
    if let Some(error) = response.error {
        return Err(response_error(response.dispatch_state, error));
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::Checked { report }) => {
            Ok(sanitize_check_report(&runner.client_id, report))
        }
        _ => Err(GatewayError::local(
            "invalid_plugin_response",
            "Runner returned an unexpected Plugin check response",
        )),
    }
}

async fn reload(
    runtime: &ToolRuntime,
    args: PluginToolArguments,
    auth: Option<&AuthContext>,
) -> Result<Value, GatewayError> {
    if args.plugin.is_some()
        || args.tool.is_some()
        || args.binding.is_some()
        || args.arguments.is_some()
    {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=reload accepts only runner",
        ));
    }
    let runner_id = required_runner(args.runner.as_deref())?;
    let runner = resolve_runner(runtime, runner_id, auth).await?;
    let response = execute_exact(runtime, &runner, PluginGatewayRequest::Reload, auth).await?;
    if let Some(error) = response.error {
        return Err(response_error(response.dispatch_state, error));
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::Reloaded {
            providers,
            failures,
            first_class_restart_required,
        }) => Ok(json!({
            "runner": runner.client_id,
            "plugins": providers
                .into_iter()
                .map(|provider| sanitize_provider_view_for_runner(&runner, provider))
                .collect::<Vec<_>>(),
            "failures": failures,
            "firstClassRestartRequired": first_class_restart_required
        })),
        _ => Err(GatewayError::local(
            "invalid_plugin_response",
            "Runner returned an unexpected Plugin reload response",
        )),
    }
}

async fn describe(
    runtime: &ToolRuntime,
    args: PluginToolArguments,
    auth: Option<&AuthContext>,
) -> Result<Value, GatewayError> {
    if args.binding.is_some() || args.arguments.is_some() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=describe accepts only runner, plugin, and tool",
        ));
    }
    let runner_id = required_runner(args.runner.as_deref())?;
    let plugin_id = required_provider(args.plugin.as_deref())?;
    let tool_name = required_tool(args.tool.as_deref())?;
    let runner = resolve_runner(runtime, runner_id, auth).await?;
    let (provider, tools, _) =
        observe_effective_provider_tools(runtime, &runner, plugin_id, auth).await?;
    let tool = tools
        .into_iter()
        .find(|candidate| candidate.name == tool_name)
        .ok_or_else(|| {
            GatewayError::local(
                "tool_not_found",
                "the requested tool is not present on the current effective Plugin provider",
            )
        })?;
    let binding = runtime.plugin_gateway.remember(PluginBinding {
        client_id: runner.client_id.clone(),
        runner_instance_id: runner.runner_instance_id.clone(),
        provider_id: provider.provider_id.clone(),
        provider_instance_id: provider.provider_instance_id.clone(),
        tool_name: tool.name.clone(),
        schema: tool.schema_observation(),
    });
    Ok(json!({
        "runner": runner.client_id,
        "plugin": provider.provider_id,
        "pluginName": provider.name,
        "tool": tool,
        "binding": binding
    }))
}

async fn observe_effective_provider_tools(
    runtime: &ToolRuntime,
    runner: &ResolvedPluginRunner,
    plugin_id: &str,
    auth: Option<&AuthContext>,
) -> Result<(PluginProviderView, Vec<PluginTool>, bool), GatewayError> {
    let providers_response =
        execute_exact(runtime, runner, PluginGatewayRequest::ProvidersList, auth).await?;
    let (providers, restart_required) = match response_providers(providers_response) {
        Ok(value) => value,
        Err(mut error) if matches!(error.code.as_str(), "stale_runner" | "runner_unavailable") => {
            error.code = "plugin_replaced".to_string();
            error.recovery =
                Some("Re-list the Plugin. WebCodex did not retarget or replay the operation.");
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    let provider = providers
        .into_iter()
        .find(|provider| provider.provider_id == plugin_id)
        .ok_or_else(|| {
            GatewayError::local(
                "plugin_unavailable",
                "the requested Plugin is not currently available on the exact Runner",
            )
        })?;

    let response = execute_exact(
        runtime,
        runner,
        PluginGatewayRequest::ToolsList {
            plane: PluginPlane::Effective,
            provider_id: provider.provider_id.clone(),
            provider_instance_id: provider.provider_instance_id.clone(),
        },
        auth,
    )
    .await?;
    let tools = match response_tools(response) {
        Ok(tools) => tools,
        Err(mut error)
            if matches!(
                error.code.as_str(),
                "stale_runner"
                    | "runner_unavailable"
                    | "stale_plugin_provider"
                    | "plugin_provider_unavailable"
            ) =>
        {
            error.code = "plugin_replaced".to_string();
            error.recovery =
                Some("Re-list the Plugin. WebCodex did not retarget or replay the operation.");
            return Err(error);
        }
        Err(error) => return Err(error),
    };
    Ok((provider, tools, restart_required))
}

async fn call_plugin(
    runtime: &ToolRuntime,
    args: PluginToolArguments,
    auth: Option<&AuthContext>,
) -> Result<PluginToolResult, GatewayError> {
    if args.runner.is_some() || args.plugin.is_some() || args.tool.is_some() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=call accepts only binding and arguments",
        ));
    }
    let binding_id = required_binding(args.binding.as_deref())?.to_string();
    let arguments = args.arguments.ok_or_else(|| {
        GatewayError::local("invalid_arguments", "action=call requires arguments")
    })?;
    if !arguments.is_object() {
        return Err(GatewayError::local(
            "invalid_arguments",
            "action=call arguments must be a JSON object",
        ));
    }
    validate_json_value(&arguments, PLUGIN_MAX_ARGUMENT_BYTES, "tool arguments").map_err(|_| {
        GatewayError::local("invalid_arguments", "tool arguments exceed Plugin bounds")
    })?;
    let observed = runtime
        .plugin_gateway
        .binding(&binding_id)
        .ok_or_else(describe_required_error)?;

    // A binding identifies the observation; it does not grant authority. Resolve
    // the logical Runner against the current credential again, then require the
    // exact Runner instance that produced the describe observation.
    let runner = resolve_runner(runtime, &observed.client_id, auth)
        .await
        .map_err(|_| describe_required_error())?;
    if runner.runner_instance_id != observed.runner_instance_id {
        runtime.plugin_gateway.forget(&binding_id);
        return Err(GatewayError::local(
            "plugin_replaced",
            "the exact Runner instance described by this binding is no longer current",
        )
        .recovery("Re-describe this Plugin tool. WebCodex did not retarget or replay the call."));
    }

    // Deliberately do not re-list/re-resolve the provider here. The exact
    // provider instance, tool name, and schema from this binding are the only
    // legal dispatch target.
    let response = match execute_exact(
        runtime,
        &runner,
        PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Effective,
            provider_id: observed.provider_id.clone(),
            provider_instance_id: observed.provider_instance_id.clone(),
            name: observed.tool_name.clone(),
            arguments,
            expected_schema: observed.schema,
        },
        auth,
    )
    .await
    {
        Ok(response) => response,
        Err(mut error) => {
            if error.code == "plugin_replaced" {
                runtime.plugin_gateway.forget(&binding_id);
                error.recovery = Some(
                    "Re-describe this Plugin tool. WebCodex did not retarget or replay the call.",
                );
            }
            return Err(error);
        }
    };
    let state = response.dispatch_state;
    if let Some(error) = response.error {
        if matches!(
            error.code.as_str(),
            "plugin_schema_changed"
                | "plugin_tool_unavailable"
                | "stale_plugin_provider"
                | "plugin_provider_unavailable"
        ) {
            runtime.plugin_gateway.forget(&binding_id);
        }
        let mut error = response_error(state, error);
        if matches!(
            error.code.as_str(),
            "plugin_schema_changed" | "plugin_tool_unavailable"
        ) {
            error.recovery = Some(
                "Re-describe this Plugin tool before calling again; WebCodex did not retarget or replay the call.",
            );
        }
        if error.code == "stale_plugin_provider" || error.code == "plugin_provider_unavailable" {
            error.code = "plugin_replaced".to_string();
            error.recovery = Some(
                "The exact Plugin provider instance changed or retired. Re-describe it; WebCodex did not retarget or replay the call.",
            );
        }
        return Err(error);
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::ToolResult { result }) => Ok(result),
        _ => Err(GatewayError::local(
            "invalid_plugin_result",
            "Runner returned an unexpected Plugin tool result",
        )),
    }
}

pub(crate) async fn visible_plugin_runners(
    runtime: &ToolRuntime,
    auth: Option<&AuthContext>,
) -> Vec<ResolvedPluginRunner> {
    let access = crate::runner_http::runner_access_from_auth(auth);
    let mut runners = Vec::new();
    for runner in runtime
        .runner_registry
        .list_runners_for_auth(access.as_ref())
        .await
    {
        if !runner.connected
            || !runner.capabilities.native_tool_plugins
            || runtime
                .runner_registry
                .assert_runner_access(access.as_ref(), &runner.client_id)
                .await
                .is_err()
        {
            continue;
        }
        runners.push(ResolvedPluginRunner {
            client_id: runner.client_id,
            runner_instance_id: runner.runner_instance_id,
            display_name: runner.display_name,
            startup_providers: runner
                .policy
                .and_then(|policy| policy.plugin_providers)
                .unwrap_or_default(),
        });
    }
    runners
}

/// Return exact startup Tool candidates from sanitized immutable registration
/// inventory.  This helper intentionally does not require `plugin:local`: call
/// dispatch uses it before the scope check so a spoofed direct Plugin name is
/// rejected as forbidden instead of falling through to an unrelated unknown
/// static tool path. Runner visibility policy remains authoritative.
pub(crate) async fn startup_tool_candidates(
    runtime: &ToolRuntime,
    auth: Option<&AuthContext>,
) -> Vec<StartupPluginToolCandidate> {
    let access = crate::runner_http::runner_access_from_auth(auth);
    let mut candidates = Vec::new();
    for runner in runtime
        .runner_registry
        .list_runners_for_auth(access.as_ref())
        .await
    {
        if !runner.connected
            || !runner.capabilities.native_tool_plugins
            || runtime
                .runner_registry
                .assert_runner_access(access.as_ref(), &runner.client_id)
                .await
                .is_err()
        {
            continue;
        }
        let Some(providers) = runner
            .policy
            .as_ref()
            .and_then(|policy| policy.plugin_providers.as_ref())
        else {
            continue;
        };
        for provider in providers {
            if provider.status != "ready" {
                continue;
            }
            // Failed/secondary-only startup providers have no admitted direct
            // ToolSpecs. Do not infer process health or fetch tools remotely.
            for tool in &provider.tools {
                candidates.push(StartupPluginToolCandidate {
                    client_id: runner.client_id.clone(),
                    runner_instance_id: runner.runner_instance_id.clone(),
                    provider_id: provider.provider_id.clone(),
                    provider_instance_id: provider.provider_instance_id.clone(),
                    tool: tool.clone(),
                });
            }
        }
    }
    candidates
}

pub(crate) async fn call_startup_direct(
    runtime: &ToolRuntime,
    candidate: &StartupPluginToolCandidate,
    arguments: Value,
    auth: Option<&AuthContext>,
) -> Value {
    if !authorized(auth) {
        return gateway_error_result(GatewayError::local(
            "insufficient_scope",
            "direct native Plugin calls require the plugin:local scope",
        ));
    }
    if !arguments.is_object() {
        return gateway_error_result(GatewayError::local(
            "invalid_arguments",
            "direct Plugin arguments must be a JSON object",
        ));
    }
    if validate_json_value(&arguments, PLUGIN_MAX_ARGUMENT_BYTES, "tool arguments").is_err() {
        return gateway_error_result(GatewayError::local(
            "invalid_arguments",
            "direct Plugin arguments exceed Plugin bounds",
        ));
    }
    let runner = ResolvedPluginRunner {
        client_id: candidate.client_id.clone(),
        runner_instance_id: candidate.runner_instance_id.clone(),
        display_name: None,
        startup_providers: Vec::new(),
    };
    let response = match execute_exact(
        runtime,
        &runner,
        PluginGatewayRequest::ToolsCall {
            plane: PluginPlane::Startup,
            provider_id: candidate.provider_id.clone(),
            provider_instance_id: candidate.provider_instance_id.clone(),
            name: candidate.tool.name.clone(),
            arguments,
            expected_schema: candidate.tool.schema_observation(),
        },
        auth,
    )
    .await
    {
        Ok(response) => response,
        Err(error) => return gateway_error_result(error),
    };
    if let Some(error) = response.error {
        return gateway_error_result(response_error(response.dispatch_state, error));
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::ToolResult { result }) => serde_json::to_value(result)
            .unwrap_or_else(|_| {
                gateway_error_result(GatewayError::local(
                    "invalid_plugin_result",
                    "Plugin result could not be encoded",
                ))
            }),
        _ => gateway_error_result(GatewayError::local(
            "invalid_plugin_result",
            "Runner returned an unexpected direct Plugin tool result",
        )),
    }
}

pub(crate) async fn resolve_runner(
    runtime: &ToolRuntime,
    runner_id: &str,
    auth: Option<&AuthContext>,
) -> Result<ResolvedPluginRunner, GatewayError> {
    if runner_id.trim().is_empty()
        || runner_id.len() > 128
        || runner_id.chars().any(char::is_control)
    {
        return Err(GatewayError::local(
            "invalid_runner",
            "runner is not a valid exact Runner client id",
        ));
    }
    visible_plugin_runners(runtime, auth)
        .await
        .into_iter()
        .find(|runner| runner.client_id == runner_id)
        .ok_or_else(|| {
            GatewayError::local(
                "runner_unavailable",
                "the exact Runner is not currently available to this credential",
            )
        })
}

pub(crate) async fn execute_exact(
    runtime: &ToolRuntime,
    runner: &ResolvedPluginRunner,
    request: PluginGatewayRequest,
    auth: Option<&AuthContext>,
) -> Result<PluginGatewayResponse, GatewayError> {
    let access = crate::runner_http::runner_access_from_auth(auth);
    let (request_id, receiver) = runtime
        .runner_registry
        .enqueue_plugin_gateway(
            &runner.client_id,
            &runner.runner_instance_id,
            request,
            access.as_ref(),
            PLUGIN_TOOL_NAME.to_string(),
        )
        .await
        .map_err(|message| {
            let (code, public_message) = if message.contains("stale")
                || message.contains("exact Runner")
                || message.contains("offline")
            {
                (
                    "plugin_replaced",
                    "the exact Runner or Plugin provider changed or became unavailable before dispatch",
                )
            } else {
                (
                    "plugin_gateway_unavailable",
                    "the Plugin gateway request could not be queued",
                )
            };
            GatewayError::local(code, public_message).recovery(
                "Re-list or re-describe the Plugin. WebCodex did not retarget or replay this operation.",
            )
        })?;

    match tokio::time::timeout(GATEWAY_WAIT_TIMEOUT, receiver).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(_)) | Err(_) => {
            let dispatched = runtime
                .runner_registry
                .cancel_request_dispatch_state(&request_id)
                .await;
            let state = if dispatched == Some(false) {
                PluginDispatchState::NotStarted
            } else {
                PluginDispatchState::OutcomeUnknown
            };
            Err(GatewayError {
                code: "plugin_gateway_timeout".to_string(),
                message: if state == PluginDispatchState::NotStarted {
                    "Plugin gateway request timed out before Runner dispatch".to_string()
                } else {
                    "Plugin gateway request timed out after dispatch may have begun; do not retry automatically"
                        .to_string()
                },
                recovery: None,
                dispatch_state: Some(state),
            })
        }
    }
}

fn response_providers(
    response: PluginGatewayResponse,
) -> Result<(Vec<PluginProviderView>, bool), GatewayError> {
    if let Some(error) = response.error {
        return Err(response_error(response.dispatch_state, error));
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::Providers {
            providers,
            first_class_restart_required,
        }) => Ok((providers, first_class_restart_required)),
        _ => Err(GatewayError::local(
            "invalid_plugin_response",
            "Runner returned an unexpected Plugin provider response",
        )),
    }
}

fn response_tools(response: PluginGatewayResponse) -> Result<Vec<PluginTool>, GatewayError> {
    if let Some(error) = response.error {
        return Err(response_error(response.dispatch_state, error));
    }
    match response.payload {
        Some(PluginGatewayResponsePayload::Tools { tools }) => Ok(tools),
        _ => Err(GatewayError::local(
            "invalid_plugin_response",
            "Runner returned an unexpected Plugin tools/list response",
        )),
    }
}

fn response_error(state: PluginDispatchState, error: PluginGatewayError) -> GatewayError {
    GatewayError {
        code: error.code,
        message: error.message,
        recovery: None,
        dispatch_state: Some(state),
    }
}

fn sanitize_check_report(runner: &str, report: PluginCheckReport) -> Value {
    let mut value = json!({
        "runner": runner,
        "plugin": report.provider_id,
        "ready": report.ready,
        "phase": check_phase_name(report.phase),
        "toolCount": report.tool_count,
        "tools": report
            .tools
            .into_iter()
            .map(|tool| {
                let mut summary = json!({"name": tool.name});
                if let Some(title) = tool.title {
                    summary["title"] = Value::String(title);
                }
                summary
            })
            .collect::<Vec<_>>()
    });
    if let Some(code) = report.code {
        value["code"] = Value::String(code);
    }
    if let Some(detail) = report.detail {
        value["detail"] = Value::String(detail);
    }
    if let Some(diagnostic) = report.diagnostic {
        let mut summary = json!({"code": diagnostic.code});
        if let Some(tool) = diagnostic.tool {
            summary["tool"] = Value::String(tool);
        }
        if let Some(field) = diagnostic.field {
            summary["field"] = Value::String(field);
        }
        value["diagnostic"] = summary;
    }
    if let Some(shape) = report.startup_tool_shape {
        let mut startup_shape = json!({"eligible": shape.eligible});
        if let Some(code) = shape.code {
            startup_shape["code"] = Value::String(code);
        }
        if let Some(tool) = shape.tool {
            startup_shape["tool"] = Value::String(tool);
        }
        if let Some(field) = shape.field {
            startup_shape["field"] = Value::String(field);
        }
        value["startupToolShape"] = startup_shape;
    }
    value
}

fn check_phase_name(phase: PluginCheckPhase) -> &'static str {
    match phase {
        PluginCheckPhase::Config => "config",
        PluginCheckPhase::Environment => "environment",
        PluginCheckPhase::Executable => "executable",
        PluginCheckPhase::Spawn => "spawn",
        PluginCheckPhase::Stdio => "stdio",
        PluginCheckPhase::Initialize => "initialize",
        PluginCheckPhase::ToolsList => "tools_list",
        PluginCheckPhase::Validation => "validation",
        PluginCheckPhase::Ready => "ready",
    }
}

fn sanitize_tool_summary(tool: PluginTool) -> Value {
    let mut summary = json!({"name": tool.name});
    if let Some(title) = tool.title {
        summary["title"] = Value::String(title);
    }
    summary
}

fn sanitize_provider_view_for_runner(
    runner: &ResolvedPluginRunner,
    provider: PluginProviderView,
) -> Value {
    let startup_admission = runner
        .startup_providers
        .iter()
        .find(|startup| startup.provider_id == provider.provider_id);
    let mut value = json!({
        "plugin": provider.provider_id,
        "name": provider.name,
        "status": provider.status,
        "errorCode": provider.error_code,
        "source": match provider.plane {
            PluginPlane::Startup => "startup",
            PluginPlane::Effective => "dynamic"
        },
        "startupDirectToolCount": provider.startup_direct_tool_count
    });
    if let Some(startup) = startup_admission {
        let admission = match startup.status.as_str() {
            "ready" => Some("direct"),
            "ready_secondary" => Some("secondary"),
            "failed" => Some("failed"),
            _ => None,
        };
        if let Some(admission) = admission {
            value["startupAdmission"] = Value::String(admission.to_string());
            if let Some(code) = startup.error_code.as_ref() {
                value["startupAdmissionCode"] = Value::String(code.clone());
            }
        }
    }
    value
}

fn required_runner(value: Option<&str>) -> Result<&str, GatewayError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| GatewayError::local("invalid_arguments", "action requires runner"))
}

fn required_provider(value: Option<&str>) -> Result<&str, GatewayError> {
    let value =
        value.ok_or_else(|| GatewayError::local("invalid_arguments", "action requires plugin"))?;
    validate_provider_id(value)
        .map_err(|_| GatewayError::local("invalid_arguments", "plugin id is invalid"))?;
    Ok(value)
}

fn required_tool(value: Option<&str>) -> Result<&str, GatewayError> {
    let value =
        value.ok_or_else(|| GatewayError::local("invalid_arguments", "action requires tool"))?;
    validate_tool_name(value)
        .map_err(|_| GatewayError::local("invalid_arguments", "tool name is invalid"))?;
    Ok(value)
}

fn required_binding(value: Option<&str>) -> Result<&str, GatewayError> {
    let value = value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| GatewayError::local("invalid_arguments", "action=call requires binding"))?;
    let Some(random) = value.strip_prefix("wc_pbind_") else {
        return Err(GatewayError::local(
            "invalid_arguments",
            "binding is not a valid opaque Plugin binding",
        ));
    };
    if random.len() != 32
        || !random
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(GatewayError::local(
            "invalid_arguments",
            "binding is not a valid opaque Plugin binding",
        ));
    }
    Ok(value)
}

fn describe_required_error() -> GatewayError {
    GatewayError::local(
        "describe_required",
        "this Plugin binding is unavailable to the current credential or is no longer retained",
    )
    .recovery(
        "Call plugin_tool with action=describe for the intended runner, plugin, and tool, then call with the returned binding. WebCodex did not retarget or replay the call.",
    )
}

fn render_gateway_result(result: Result<GatewaySuccess, GatewayError>) -> Value {
    match result {
        Ok(GatewaySuccess::Metadata(value)) => gateway_success_result(value),
        Ok(GatewaySuccess::ToolResult(result)) => {
            serde_json::to_value(result).unwrap_or_else(|_| {
                gateway_error_result(GatewayError::local(
                    "invalid_plugin_result",
                    "Plugin result could not be encoded",
                ))
            })
        }
        Err(error) => gateway_error_result(error),
    }
}

fn gateway_success_result(value: Value) -> Value {
    let text = serde_json::to_string(&value).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": value,
        "isError": false
    })
}

fn gateway_error_result(error: GatewayError) -> Value {
    let dispatch_state = error.dispatch_state.map(dispatch_state_name);
    let mut structured = json!({
        "error": {"code": error.code, "message": error.message}
    });
    if let Some(state) = dispatch_state {
        structured["dispatchState"] = Value::String(state.to_string());
    }
    if let Some(recovery) = error.recovery {
        structured["recovery"] = Value::String(recovery.to_string());
    }
    let text = serde_json::to_string(&structured).unwrap_or_else(|_| "{}".to_string());
    json!({
        "content": [{"type": "text", "text": text}],
        "structuredContent": structured,
        "isError": true
    })
}

fn dispatch_state_name(state: PluginDispatchState) -> &'static str {
    match state {
        PluginDispatchState::NotStarted => "not_started",
        PluginDispatchState::OutcomeUnknown => "outcome_unknown",
        PluginDispatchState::Completed => "completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_binding(provider: &str, tool: &str) -> PluginBinding {
        PluginBinding {
            client_id: "runner-a".to_string(),
            runner_instance_id: "runner-instance-a".to_string(),
            provider_id: provider.to_string(),
            provider_instance_id: format!("{provider}-instance"),
            tool_name: tool.to_string(),
            schema: PluginSchemaObservation {
                input_schema: json!({"type":"object"}),
                output_schema: None,
                annotations: None,
            },
        }
    }

    #[test]
    fn plugin_bindings_are_independent_and_evicted_fifo() {
        let runtime = PluginGatewayRuntime::default();
        let first = runtime.remember(test_binding("provider-a", "tool-a"));
        let second = runtime.remember(test_binding("provider-b", "tool-b"));
        assert_ne!(first, second);
        assert!(first.starts_with("wc_pbind_"));
        assert!(second.starts_with("wc_pbind_"));
        assert_eq!(runtime.binding(&first).unwrap().tool_name, "tool-a");
        assert_eq!(runtime.binding(&second).unwrap().tool_name, "tool-b");

        runtime.forget(&first);
        assert!(runtime.binding(&first).is_none());
        assert_eq!(
            runtime.binding(&second).unwrap().provider_id,
            "provider-b",
            "forgetting one exact binding must not disturb another provider/tool binding"
        );

        let oldest = runtime.remember(test_binding("oldest", "tool"));
        for index in 0..MAX_PLUGIN_BINDINGS {
            runtime.remember(test_binding(&format!("provider-{index}"), "tool"));
        }
        assert!(
            runtime.binding(&oldest).is_none(),
            "binding store must remain bounded with deterministic oldest eviction"
        );
    }

    #[test]
    fn fixed_plugin_tool_catalog_hides_runtime_identity_and_provider_defined_output_schema() {
        let spec = tool_spec();
        let encoded = serde_json::to_string(&spec).unwrap();
        assert_eq!(spec["name"], PLUGIN_TOOL_NAME);
        assert!(spec.get("outputSchema").is_none());
        assert!(spec["inputSchema"]["properties"]["binding"].is_object());
        assert!(spec["inputSchema"]["properties"]["plugin"].is_object());
        assert!(spec["inputSchema"]["properties"]["runner"].is_object());
        assert!(spec["inputSchema"]["properties"]["action"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| action == "check"));
        assert_eq!(
            spec["inputSchema"]["properties"]["binding"]["pattern"],
            "^wc_pbind_[0-9a-f]{32}$"
        );
        assert!(spec["description"]
            .as_str()
            .unwrap()
            .contains("call accepts only that binding plus arguments"));
        assert!(!encoded.contains("provider_instance_id"));
        assert!(!encoded.contains("runner_instance_id"));
        assert!(!encoded.contains("revision"));
        assert!(!encoded.contains("command"));
        assert!(!encoded.contains("cwd"));
        assert!(!encoded.contains("\"env\""));
    }
}
