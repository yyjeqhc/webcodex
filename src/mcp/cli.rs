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
    "work_on_project",
    "list_project_files",
    "list_project_tracked_files",
    "read_files",
    "search_project_texts",
    "search_file_context",
    "find_definition",
    "find_references",
    "edit_project_files",
    "read_workspace_changes",
    "run_process",
    "run_shell",
    "observe_jobs",
    "wait_for_job_readiness",
    "read_tool_manifest",
];

pub(super) const INSTRUCTIONS: &str = "WebCodex operates on registered Projects on their owning Runner, which may be a different machine from your local shell. Use WebCodex tools for that Project. Select an exact project id from list_projects or user configuration; never infer a Project or Session from the connection. work_on_project starts/resumes a Workflow Session and returns workspace status and bounded repository instructions; follow any read_more before dependent work. Use list_project_tracked_files for filename/glob discovery, list_project_files for one directory, and find_definition/find_references for semantic navigation when available. Batch independent reads/searches; use search_file_context when matches need source context. Keep read_revision for edit_project_files and expected_read_revision for continued reads. A pending command is the same Job: do independent work, then wait_for_job_readiness and observe_jobs; never redispatch after timeout or outcome_unknown. Unknown effects require observation. Other tools: read_tool_manifest, then call_runtime_tool with exact arguments. Results are text; failures retain isError and recovery facts. No MCP App presentation or _wc sidecars are available on this surface.";

pub(super) fn initialize_instructions(default_project: Option<&str>) -> Value {
    json!(match default_project {
        Some(project) => format!("{INSTRUCTIONS}\nConfigured default Project: {project}. It applies only to tools requiring a Project and fresh work_on_project; explicit selectors and exact Session resumes win. This is a target preference, not authorization."),
        None => INSTRUCTIONS.to_string(),
    })
}

pub(super) fn is_direct(name: &str) -> bool {
    DIRECT_TOOLS.contains(&name) && is_model_visible_tool_name(name)
}

pub(super) fn is_app_tool(name: &str) -> bool {
    crate::model_surface::tool_requires_direct_app_presentation(name)
        || name == "open_webcodex_workbench"
}

pub(super) fn suggested_route(name: &str, stateless: bool) -> SuggestedToolCallRoute {
    if is_app_tool(name) || crate::model_surface::tool_requires_individual_host_review(name) {
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
        project_work_on_project(tool, "inputSchema");
        let name = tool["name"].as_str().unwrap().to_owned();
        project_manifest_limit_schema(&name, &mut tool["inputSchema"]);
        if tool["name"] == "call_runtime_tool" {
            tool["description"] = json!("Call one admitted runtime tool using its exact read_tool_manifest contract. Prefer a direct callable when listed. Normal authorization, permissions, capability and effect checks apply. MCP App tools and _wc sidecars are unavailable. Never replay uncertain effects.");
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

/// CLI keeps the canonical bootstrap contract and adds only adapter behavior.
/// The same projection serves tools/list and exact Manifest discovery. Broad
/// Manifest entries keep their canonical selection summary.
fn project_work_on_project(value: &mut Value, schema_key: &str) {
    if value["name"] != "work_on_project" || !value[schema_key].is_object() {
        return;
    }
    let canonical = webcodex_tool_contracts::lookup_tool_definition("work_on_project")
        .expect("registered bootstrap")
        .model_spec
        .expect("model-visible bootstrap")
        .description;
    value["description"] = json!(format!(
        "{canonical} CLI returns bounded project instructions automatically. include_extension_catalog defaults false; set true for extension discovery. _wc sidecars and MCP Apps are unavailable."
    ));
    if let Some(field) = value[schema_key].pointer_mut("/properties/include_extension_catalog") {
        field["default"] = json!(false);
        field["description"] =
            json!("Include extension catalog; CLI defaults false. Set true when needed.");
    }
}

pub(super) const MANIFEST_DEFAULT_LIMIT: usize = 20;

fn project_manifest_limit_schema(name: &str, schema: &mut Value) {
    if name == "read_tool_manifest" {
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
    tool == "work_on_project"
        || webcodex_tool_contracts::input_schema_for_tool(tool)["required"]
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
    // Exact Session resume and explicit path registration have their own source
    // rules. A default must never override them, even when malformed.
    if tool == "work_on_project"
        && ["session_id", "client_id", "path"]
            .iter()
            .any(|key| arguments.contains_key(*key))
    {
        return;
    }
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
        project_work_on_project(value, "input_schema");
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
