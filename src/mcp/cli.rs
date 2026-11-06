//! Opt-in CLI discovery and text presentation. All calls still enter the
//! canonical kernel; this adapter neither grants capabilities nor retries work.
use crate::auth::AuthContext;
use crate::model_surface::SuggestedToolCallRoute;
use crate::tool_runtime::tool_definition::is_model_visible_tool_name;
use serde_json::{json, Value};

mod text;
pub(super) use text::project_response;

// A small coding surface; other admitted tools retain exact-manifest + gateway
// discovery. Every entry must be an ordinary model-visible canonical tool.
pub(super) const DIRECT_TOOLS: &[&str] = &[
    "list_projects",
    "list_project_files",
    "read_files",
    "search_project_texts",
    "edit_project_files",
    "read_workspace_changes",
    "run_shell",
    "observe_jobs",
    "stop_job",
    "read_tool_manifest",
];

pub(super) const INSTRUCTIONS: &str = "Tools execute on the Project's owning Runner, which may differ from your local machine. Use an exact Project from configuration or list_projects. run_shell executes commands; pending/unknown execution retains one Job: observe_jobs can wait and read it, stop_job cancels it. Never redispatch uncertain effects. File edits require their read revision. Discover specialized tools with read_tool_manifest, then call_runtime_tool. The client owns planning and repository instructions; no Workflow Session is required.";

pub(super) fn initialize_instructions(default_project: Option<&str>) -> Value {
    json!(match default_project {
        Some(project) => format!("{INSTRUCTIONS}\nConfigured default Project: {project}. It supplies omitted required Project selectors; explicit selectors win. This is a target preference, not authorization."),
        None => INSTRUCTIONS.to_string(),
    })
}

pub(super) fn is_direct(name: &str) -> bool {
    DIRECT_TOOLS.contains(&name) && is_model_visible_tool_name(name)
}

pub(super) fn is_app_tool(name: &str) -> bool {
    webcodex_tool_contracts::lookup_tool_definition(name)
        .is_some_and(|definition| definition.presents_app())
}

/// Execution-only admission is shared by discovery, direct/gateway dispatch and
/// recovery routes. Categories belong to the canonical registry; no new authority.
pub(super) fn admits(name: &str) -> bool {
    use webcodex_tool_contracts::tool_definition::*;
    // These named protocol adapters own action-dependent schemas/authority and
    // are deliberately outside the ToolDefinition registry.
    if matches!(
        name,
        crate::mcp_gateway::MCP_TOOL_NAME | crate::ssh_resource_gateway::SSH_RESOURCE_TOOL_NAME
    ) {
        return true;
    }
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(name) else {
        return false;
    };
    if definition.presents_app() || definition.policy.requires_explicit_business_session {
        return false;
    }
    match definition.category {
        TOOL_CATEGORY_SESSION
        | TOOL_CATEGORY_GOAL
        | TOOL_CATEGORY_COMMUNICATION
        | TOOL_CATEGORY_SKILL
        | TOOL_CATEGORY_MEMORY
        | TOOL_CATEGORY_AGENT_TASK
        | TOOL_CATEGORY_AGENT_WAIT
        | TOOL_CATEGORY_CODING_AGENT => false,
        TOOL_CATEGORY_RUNTIME => matches!(
            name,
            "read_tool_manifest" | "get_runtime_status" | "list_runners" | "read_tool_trace"
        ),
        _ => !matches!(name, "work_on_project" | "wait_for_job_terminal"),
    }
}

pub(super) fn suggested_route(name: &str, stateless: bool) -> SuggestedToolCallRoute {
    if !admits(name) || crate::model_surface::tool_requires_individual_host_review(name) {
        SuggestedToolCallRoute::Unavailable
    } else if is_direct(name) {
        SuggestedToolCallRoute::Direct
    } else {
        match super::tools::mcp_suggested_tool_call_route(name, stateless) {
            SuggestedToolCallRoute::Direct => SuggestedToolCallRoute::Gateway(
                crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME,
            ),
            route => route,
        }
    }
}

#[cfg(test)]
pub(super) fn tools_list(auth: Option<&AuthContext>, default_project: bool) -> Value {
    tools_list_with_exposure(auth, default_project, Default::default())
}

pub(super) fn tools_list_with_exposure(
    auth: Option<&AuthContext>,
    default_project: bool,
    exposure: super::tool_exposure::ToolExposure,
) -> Value {
    let specs = webcodex_tool_contracts::registry::registered_tool_descriptors()
        .into_iter()
        .filter(|spec| exposure.is_direct(super::request_policy::McpSurface::Cli, &spec.name))
        .collect();
    let mut tools: Vec<Value> = super::tools::filter_descriptors_for_oauth(specs, auth)
        .into_iter()
        .map(|spec| super::tools::mcp_tool_descriptor_json(spec, true, false))
        .collect();
    tools.push(super::tools::adaptive_runtime_gateway_tool_json(true));
    for tool in &mut tools {
        super::discovery::compact_tool(tool);
        let name = tool["name"].as_str().unwrap().to_owned();
        project_manifest_limit_schema(&name, &mut tool["inputSchema"]);
        if tool["name"] == "call_runtime_tool" {
            tool["description"] = json!("Call one admitted runtime tool using its exact read_tool_manifest contract. Prefer a direct callable when listed. Normal authorization, permissions, capability and effect checks apply. Workflow, communication and MCP App tools are unavailable. No _wc sidecars. Never replay uncertain effects.");
        }
        if default_project {
            let name = tool["name"].as_str().unwrap().to_string();
            project_default_schema(&name, &mut tool["inputSchema"]);
        }
        tool.as_object_mut().unwrap().remove("outputSchema");
        tool.as_object_mut().unwrap().remove("_meta");
        if auth.is_some_and(AuthContext::is_oauth_token) {
            if let Some(scopes) = crate::auth::oauth_mcp_security_scheme_scopes(
                tool["name"].as_str().unwrap_or_default(),
            ) {
                tool["securitySchemes"] = json!(scopes
                    .into_iter()
                    .map(|scopes| json!({"type":"oauth2", "scopes":scopes}))
                    .collect::<Vec<_>>());
            }
        }
    }
    json!({"tools": tools})
}

pub(super) const MANIFEST_DEFAULT_LIMIT: usize = 20;

fn project_manifest_limit_schema(name: &str, schema: &mut Value) {
    if name == "read_tool_manifest" {
        schema["properties"]["include_recommended_flows"]["default"] = json!(false);
        schema["properties"]["include_recommended_flows"]["const"] = json!(false);
        schema["properties"]["include_recommended_flows"]["description"] =
            json!("Workflow recipes are unavailable on the execution-only surface.");
        schema["properties"]["limit"]["default"] = json!(MANIFEST_DEFAULT_LIMIT);
        schema["properties"]["limit"]["description"] = json!("Maximum catalog entries, clamped to 1..100; CLI defaults to 20. Narrow by query/category/intent, or use tool_name for one full input contract.");
    }
}

pub(super) fn project_can_default(tool: &str) -> bool {
    if !is_model_visible_tool_name(tool)
        && !webcodex_tool_contracts::EXACT_MANIFEST_SPECIALIST_TOOL_NAMES.contains(&tool)
    {
        return false;
    }
    webcodex_tool_contracts::input_schema_for_tool(tool)["required"]
        .as_array()
        .is_some_and(|required| required.iter().any(|key| key == "project"))
}

pub(super) fn apply_default_project(tool: &str, arguments: &mut Value, project: Option<&str>) {
    let Some(project) = project.filter(|_| project_can_default(tool)) else {
        return;
    };
    if arguments.is_null() {
        *arguments = json!({});
    }
    let Some(arguments) = arguments.as_object_mut() else {
        return;
    };
    arguments.entry("project").or_insert_with(|| json!(project));
}

pub(super) fn project_default_schema(tool: &str, schema: &mut Value) {
    if !project_can_default(tool) {
        return;
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|key| key != "project");
    }
    if let Some(property) = schema.pointer_mut("/properties/project") {
        property["description"] = json!("Exact Project selector. Omit to use this connection's explicitly configured default. Explicit selectors win; ordinary authorization still applies.");
    }
}

pub(super) fn project_manifest(
    result: &mut crate::tool_runtime::ToolResult,
    default_project: bool,
) {
    if !result.success {
        if result.output["code"] == "tool_unavailable_on_surface" {
            result.error = Some(format!(
                "tool '{}' is unavailable on the CLI surface",
                result.output["tool_name"].as_str().unwrap_or("?")
            ));
        }
        return;
    }
    fn entry(value: &mut Value, default_project: bool) {
        let Some(name) = value["name"].as_str().map(str::to_string) else {
            return;
        };
        if default_project {
            if let Some(schema) = value.get_mut("input_schema") {
                project_default_schema(&name, schema);
            }
        }
        if let Some(schema) = value.get_mut("input_schema") {
            project_manifest_limit_schema(&name, schema);
        }
        if let Some(properties) = value
            .pointer_mut("/input_schema/properties")
            .and_then(Value::as_object_mut)
        {
            properties.remove("_wc");
        }
    }
    if let Some(output) = result.output.as_object_mut() {
        output.remove("recommended_flows");
    }
    entry(&mut result.output, default_project);
    if let Some(contract) = result
        .output
        .get_mut("contract")
        .filter(|value| value.is_object())
    {
        entry(contract, default_project);
    }
    if let Some(tools) = result.output.get_mut("tools").and_then(Value::as_array_mut) {
        for tool in tools {
            entry(tool, default_project);
        }
    }
}
