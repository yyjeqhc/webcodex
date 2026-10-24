//! MCP-only discovery policy. Operator configuration selects optional workflow
//! surfaces; canonical scopes, ownership, effects and Host review still apply.
use super::request_policy::McpSurface;
use crate::auth::AuthContext;
use crate::model_surface::SuggestedToolCallRoute;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const CONFIG_ENV: &str = "WEBCODEX_MCP_TOOL_EXPOSURE";
const MAX_CONFIG_BYTES: usize = 16 * 1024;
const MAX_CLIENTS: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ToolExposure {
    pub goals: bool,
    pub agent_continuation: bool,
    pub job_continuation: bool,
}

#[derive(Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct ToolExposureConfig {
    pub adaptive: ToolExposure,
    pub cli: ToolExposure,
    oauth_clients: Vec<ClientExposure>,
}

impl std::fmt::Debug for ToolExposureConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExposureConfig")
            .field("adaptive", &self.adaptive)
            .field("cli", &self.cli)
            .field("oauth_client_overrides", &self.oauth_clients.len())
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientExposure {
    client_id: String,
    adaptive: Option<ToolExposure>,
    cli: Option<ToolExposure>,
}

impl ToolExposureConfig {
    pub(crate) fn from_env() -> Result<Self, String> {
        match std::env::var(CONFIG_ENV) {
            Ok(raw) => Self::parse(&raw),
            Err(std::env::VarError::NotPresent) => Ok(Self::default()),
            Err(_) => Err(format!("{CONFIG_ENV} must be UTF-8")),
        }
    }

    pub(crate) fn parse(raw: &str) -> Result<Self, String> {
        let invalid = || {
            format!("{CONFIG_ENV} must be a closed JSON configuration (at most 16 KiB, 64 unique OAuth clients with bounded client_id); see docs/MCP.md")
        };
        if raw.len() > MAX_CONFIG_BYTES {
            return Err(invalid());
        }
        // serde structs also accept positional arrays. Configuration deliberately
        // accepts named objects only; the typed pass below also rejects duplicate keys.
        let shape: Value = serde_json::from_str(raw).map_err(|_| invalid())?;
        let surface_objects = |value: &Value| {
            ["adaptive", "cli"]
                .iter()
                .all(|key| value.get(key).is_none_or(Value::is_object))
        };
        if !shape.is_object()
            || !surface_objects(&shape)
            || shape.get("oauth_clients").is_some_and(|clients| {
                clients.as_array().is_none_or(|clients| {
                    clients
                        .iter()
                        .any(|client| !client.is_object() || !surface_objects(client))
                })
            })
        {
            return Err(invalid());
        }
        // Do not include the raw JSON or serde's user-supplied field names in errors.
        let config: Self = serde_json::from_str(raw).map_err(|_| invalid())?;
        let mut clients = std::collections::HashSet::new();
        if config.oauth_clients.len() > MAX_CLIENTS
            || config.oauth_clients.iter().any(|client| {
                client.client_id.is_empty()
                    || client.client_id.len() > 256
                    || client
                        .client_id
                        .chars()
                        .any(|c| c.is_whitespace() || c.is_control())
                    || (client.adaptive.is_none() && client.cli.is_none())
                    || !clients.insert(&client.client_id)
            })
        {
            return Err(invalid());
        }
        Ok(config)
    }

    pub(super) fn resolve(&self, surface: McpSurface, auth: Option<&AuthContext>) -> ToolExposure {
        let client = auth
            .filter(|auth| auth.is_oauth_token())
            .and_then(|auth| auth.allowed_client_id.as_deref())
            .and_then(|id| {
                self.oauth_clients
                    .iter()
                    .find(|client| client.client_id == id)
            });
        match surface {
            McpSurface::Adaptive => client
                .and_then(|client| client.adaptive)
                .unwrap_or(self.adaptive),
            McpSurface::Cli => client.and_then(|client| client.cli).unwrap_or(self.cli),
        }
    }

    pub(crate) fn status(&self) -> Value {
        // Diagnostics report defaults and the bounded override count, never client ids.
        json!({"adaptive": self.adaptive, "cli": self.cli, "oauth_client_overrides": self.oauth_clients.len()})
    }

    #[cfg(test)]
    pub(crate) fn all_enabled() -> Self {
        Self {
            adaptive: ToolExposure::ALL,
            cli: ToolExposure::ALL,
            ..Self::default()
        }
    }
}

impl ToolExposure {
    #[cfg(test)]
    pub(super) const ALL: Self = Self {
        goals: true,
        agent_continuation: true,
        job_continuation: true,
    };

    pub(super) fn allows(self, name: &str) -> bool {
        if webcodex_tool_contracts::lookup_tool_definition(name).is_some_and(|definition| {
            definition.category == webcodex_tool_contracts::tool_definition::TOOL_CATEGORY_GOAL
        }) {
            return self.goals;
        }
        if super::tools::is_agent_continuation_app_tool_name(name)
            || matches!(
                name,
                "create_agent_identity"
                    | "list_agent_identities"
                    | "update_agent_identity"
                    | "rotate_agent_continuation_endpoint"
                    | "present_agent_continuation"
                    | "bootstrap_agent_conversation"
                    | "consume_agent_wake"
                    | "detach_agent_endpoint"
                    | "wait_for_agent_events"
                    | "start_agent_task_endpoint_continuation"
            )
        {
            return self.agent_continuation;
        }
        if super::tools::is_job_terminal_continuation_app_tool_name(name)
            || matches!(
                name,
                "wait_for_job_terminal" | "present_job_terminal_continuation"
            )
        {
            return self.job_continuation;
        }
        true
    }

    pub(super) fn route(
        self,
        surface: McpSurface,
        name: &str,
        stateless: bool,
    ) -> SuggestedToolCallRoute {
        if !self.allows(name) {
            return SuggestedToolCallRoute::Unavailable;
        }
        if surface == McpSurface::Cli {
            if super::cli::is_app_tool(name) {
                SuggestedToolCallRoute::Unavailable
            } else if crate::model_surface::tool_requires_individual_host_review(name) {
                // Opted-in reviewed operations need their own CLI callable too.
                SuggestedToolCallRoute::Direct
            } else {
                super::cli::suggested_route(name, stateless)
            }
        } else {
            super::tools::mcp_suggested_tool_call_route(name, stateless)
        }
    }

    pub(super) fn is_direct(self, surface: McpSurface, name: &str) -> bool {
        self.allows(name)
            && if surface == McpSurface::Cli {
                !super::cli::is_app_tool(name)
                    && (super::cli::is_direct(name)
                        || crate::model_surface::tool_requires_individual_host_review(name))
            } else {
                crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(name)
            }
    }

    pub(super) fn allows_resource(self, uri: &str) -> bool {
        match uri {
            super::app_registry::MCP_GOAL_PLAN_UI_RESOURCE_URI => self.goals,
            super::app_registry::MCP_AGENT_CONTINUATION_UI_RESOURCE_URI => self.agent_continuation,
            super::app_registry::MCP_JOB_TERMINAL_CONTINUATION_UI_RESOURCE_URI => {
                self.job_continuation
            }
            _ => true,
        }
    }

    pub(super) fn project_manifest(
        self,
        result: &mut crate::tool_runtime::ToolResult,
        surface: McpSurface,
        stateless: bool,
    ) {
        if !result.success {
            return;
        }
        let entry = |value: &mut Value| {
            let Some(name) = value["name"]
                .as_str()
                .or_else(|| value["tool_name"].as_str())
                .map(str::to_owned)
            else {
                return;
            };
            let route = self.route(surface, &name, stateless);
            if value.get("availability").is_some() {
                let (availability, gateway) = match route {
                    SuggestedToolCallRoute::Direct => ("direct", None),
                    SuggestedToolCallRoute::Gateway(via) => ("gateway", Some(via)),
                    SuggestedToolCallRoute::Unavailable => ("unavailable", None),
                };
                value["availability"] = json!(availability);
                value["gateway_tool"] = json!(gateway);
            }
            value["route"] = if value["route"].get("primary").is_some() {
                crate::model_surface::manifest_invocation_route(&name, route)
            } else {
                match route {
                    SuggestedToolCallRoute::Direct => json!({"mode":"direct"}),
                    SuggestedToolCallRoute::Gateway(via) => json!({"mode":"gateway", "via":via}),
                    SuggestedToolCallRoute::Unavailable => json!({"mode":"unavailable"}),
                }
            };
        };
        entry(&mut result.output);
        if let Some(contract) = result
            .output
            .get_mut("contract")
            .filter(|value| value.is_object())
        {
            entry(contract);
        }
        if let Some(tools) = result.output.get_mut("tools").and_then(Value::as_array_mut) {
            for tool in tools {
                entry(tool);
            }
        }
    }
}
