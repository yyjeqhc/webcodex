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
    "list_project_tracked_files",
    "read_files",
    "search_project_texts",
    "edit_project_files",
    "read_workspace_changes",
    "run_shell",
    "observe_jobs",
    "stop_job",
    "read_tool_manifest",
];

pub(super) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 600;

pub(super) const INSTRUCTIONS: &str = "Tools execute on the Project's owning Runner, not necessarily your local machine. Use the configured Project or list_projects. Read the remote repository's AGENTS.md / CLAUDE.md before editing; local instructions may describe a different repository. run_shell waits up to 50 seconds by default; sync_wait_secs requests earlier return. Commands continue until completion or timeout_secs (default 600 seconds, subject to Runner limits), then their process tree is terminated. Pending or uncertain execution retains one Job: observe_jobs waits/reads it, stop_job cancels it. Never replay uncertain effects. Edits use a read revision or a unique exact old-text guard; whole-file writes require a revision. Discover specialized tools with read_tool_manifest, then call_runtime_tool. The client owns planning; no Workflow Session is used.";

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

/// An unfiltered catalog starts with coding tools. Exact and deliberately
/// filtered discovery can still reach the full admitted execution surface.
pub(super) fn coding_catalog_entry(name: &str) -> bool {
    use webcodex_tool_contracts::tool_definition::*;
    webcodex_tool_contracts::lookup_tool_definition(name).is_some_and(|d| {
        matches!(
            d.category,
            TOOL_CATEGORY_FILE
                | TOOL_CATEGORY_EDIT
                | TOOL_CATEGORY_EXECUTION
                | TOOL_CATEGORY_JOB
                | TOOL_CATEGORY_LSP
                | TOOL_CATEGORY_GIT
                | TOOL_CATEGORY_VALIDATION
        ) || name == "list_projects"
    })
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
        project_cli_schema(&name, &mut tool["inputSchema"]);
        if let Some(description) = description(&name) {
            tool["description"] = json!(description);
        }
        if default_project {
            let name = tool["name"].as_str().unwrap().to_string();
            project_default_schema(&name, &mut tool["inputSchema"]);
        }
        tool.as_object_mut().unwrap().remove("outputSchema");
        tool["_meta"] = json!({"anthropic/alwaysLoad": true});
        if matches!(
            name.as_str(),
            "read_files" | "search_project_texts" | "read_workspace_changes"
        ) {
            tool["_meta"]["anthropic/maxResultSizeChars"] = json!(70000);
        }
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

/// Adapter-only timing, cancellation and workflow differences; reused by exact discovery.
fn description(name: &str) -> Option<&'static str> {
    match name {
        "run_shell" => Some("Execute a shell command on the Project Runner. cwd accepts relative or in-Project absolute paths. timeout_secs defaults to 600s, bounded by Runner limits; expiration kills the process tree. sync_wait_secs shortens the initial 50s wait. Pending/uncertain results retain the same Job: observe_jobs reads/waits, stop_job cancels. Never redispatch uncertain effects. Permissions and process-tree cleanup apply."),
        "stop_job" => Some("Cancel an existing owned Job by job_id. Calling requests cancellation; host approval and Project/Job authority checks apply. Acceptance is not terminal completion: observe_jobs checks the same Job. No logs are returned."),
        "observe_jobs" => Some("Read or wait for owned Jobs. With wait_secs, wait until wake_on (default terminal) or deadline, even without tokens. change/meaningful_change waits need after_observation_token. Tokens return the latest incremental logs; page=true reads forward for explicit log recovery. Follow returned continuations without rerunning commands. Terminal status never proves uncertain effects safe to replay."),
        "call_runtime_tool" => Some("Call an admitted tool using its exact read_tool_manifest contract; prefer direct tools. Normal authorization, permissions, capability and effect checks apply. Workflow, communication and MCP App tools are unavailable. No _wc sidecars. Never replay uncertain effects."),
        _ => None,
    }
}

pub(super) const MANIFEST_DEFAULT_LIMIT: usize = 20;

fn project_cli_schema(name: &str, schema: &mut Value) {
    if name == "observe_jobs" {
        let canonical = webcodex_tool_contracts::input_schema_for_tool(name);
        for key in ["wake_on", "summary_only"] {
            schema["properties"][key] = canonical["properties"][key].clone();
        }
    }
    if name == "observe_jobs" {
        schema["properties"]["wake_on"]["description"] = json!("Wait for terminal (default), meaningful change, or any change; wait_secs bounds the wait.");
        schema["properties"]["summary_only"]["description"] = json!("Compact proven successful validation logs only; failures and ordinary commands retain logs. Default false.");
    }
    if name == "run_shell" {
        schema["properties"]["timeout_secs"]["default"] = json!(DEFAULT_COMMAND_TIMEOUT_SECS);
        schema["properties"]["timeout_secs"]["description"] = json!("Maximum process lifetime, default 600 seconds, subject to Runner limits. Expiration terminates the process tree; effects may be incomplete.");
        schema["properties"]["sync_wait_secs"]["description"] = json!("Initial wait, default 50 seconds. Can only shorten the wait; remaining execution continues as the same Job until its lifetime expires.");
        schema["properties"]["cwd"]["description"] = json!("Working directory within the Project; relative or in-Project absolute path. Defaults to the Project root.");
    }
    if let Some(properties) = schema["properties"].as_object_mut() {
        properties.remove("session_id");
        if name == "run_shell" {
            for key in ["purpose", "assertion_name", "result_expectation"] {
                properties.remove(key);
            }
        }
        if name == "stop_job" {
            properties.remove("confirm");
        }
    }
    if matches!(name, "read_files" | "search_project_texts") {
        schema["properties"]["max_result_bytes"]["default"] = json!(45000);
    }
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

pub(super) fn unavailable_argument<'a>(name: &str, arguments: &'a Value) -> Option<&'static str> {
    [
        "session_id",
        "purpose",
        "assertion_name",
        "result_expectation",
    ]
    .into_iter()
    .find(|key| arguments.get(*key).is_some() && (*key == "session_id" || name == "run_shell"))
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
            project_cli_schema(&name, schema);
        }
        if let Some(description) = description(&name) {
            value["description"] = json!(description);
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

/// Test the executable parameters of compact follow-ups independently of their
/// display spelling and the Runtime's non-argument follow-up classification.
#[cfg(test)]
pub(crate) fn parse_call_line_for_test(line: &str) -> Option<Value> {
    let line = line.trim();
    if let Ok(value) = serde_json::from_str::<Value>(line) {
        return (value["tool"].is_string() && value["arguments"].is_object())
            .then(|| json!({"tool":value["tool"], "arguments":value["arguments"]}));
    }
    let rest = line.strip_prefix("next: ")?;
    let (tool, mut rest) = rest.split_once(' ').unwrap_or((rest, ""));
    let mut arguments = serde_json::Map::new();
    while !rest.trim().is_empty() {
        let (key, payload) = rest.trim_start().split_once('=')?;
        if key.chars().any(char::is_whitespace) {
            return None;
        }
        let mut stream = serde_json::Deserializer::from_str(payload).into_iter::<Value>();
        let value = stream.next()?.ok()?;
        let used = stream.byte_offset();
        arguments.insert(key.to_string(), value);
        rest = &payload[used..];
    }
    Some(json!({"tool":tool, "arguments":arguments}))
}
