use crate::config::CodexConfig;
use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::tool_runtime::{RuntimeInfo, ToolRuntime, ToolSpec};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub(in crate::tool_runtime::tests) const SAMPLE_PROJECT: &str = "agent:oe:private-drop";
pub(in crate::tool_runtime::tests) const UNIT_TOOL_FIXTURES: &[&str] = &[
    "list_tools",
    "list_projects",
    "list_agents",
    "runtime_status",
];

pub(in crate::tool_runtime::tests) fn test_runtime() -> ToolRuntime {
    ToolRuntime::new_for_tests()
}

pub(in crate::tool_runtime::tests) fn sample_tool_args(name: &str) -> Value {
    let spec = ToolRuntime::registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == name)
        .unwrap_or_else(|| panic!("missing tool spec for {name}"));
    sample_tool_args_for_spec(&spec)
}

pub(in crate::tool_runtime::tests) fn sample_tool_args_for_spec(spec: &ToolSpec) -> Value {
    let required = spec.input_schema["required"]
        .as_array()
        .unwrap_or_else(|| panic!("{} schema should list required fields", spec.name));
    if required.is_empty() && UNIT_TOOL_FIXTURES.contains(&spec.name.as_str()) {
        return Value::Null;
    }

    let args = required
        .iter()
        .map(|field| {
            let field = field
                .as_str()
                .unwrap_or_else(|| panic!("{} required field should be a string", spec.name));
            (field.to_string(), sample_field_value(field))
        })
        .collect();
    Value::Object(args)
}

pub(in crate::tool_runtime::tests) fn sample_field_value(field: &str) -> Value {
    match field {
        "project" => json!(SAMPLE_PROJECT),
        "command" => json!("true"),
        "patch" => json!("diff --git a/a b/a\n"),
        "paths" => json!(["old.txt"]),
        "path" => json!("src/lib.rs"),
        "old" | "old_text" => json!("a"),
        "new" | "new_text" => json!("b"),
        "pattern" => json!("fn main"),
        "text" => json!("// hi\n"),
        "content" => json!("fn main() {}\n"),
        "content_base64" => json!("AA=="),
        "start_line" | "end_line" | "line" | "offset" => json!(1),
        "upload_id" => json!("wc_upload_test_1"),
        "edits" => json!([{"kind": "replace_exact", "old_text": "a", "new_text": "b"}]),
        "prompt" => json!("summarize"),
        "job_id" => json!("job_123"),
        "session_id" => json!("wc_sess_existing"),
        "checkpoint_id" => json!("wc_ckpt_1234"),
        "confirm" => json!(true),
        "client_id" => json!("oe"),
        "id" => json!("private-drop"),
        "name" => json!("Private Drop"),
        "kind" => json!("note"),
        "message" => json!("hello"),
        "message_id" => json!("wc_msg_0001"),
        other => panic!("missing sample value for required field {other}"),
    }
}

pub(in crate::tool_runtime::tests) fn sample_tool_args_with_session(name: &str) -> Value {
    let mut args = sample_tool_args(name);
    let obj = args
        .as_object_mut()
        .unwrap_or_else(|| panic!("{name} does not accept object arguments"));
    obj.insert(
        "session_id".to_string(),
        Value::String("wc_sess_accessor".to_string()),
    );
    args
}

/// Build a placeholder value for a required field from its JSON Schema
/// property definition. When the property carries an `enum` constraint the
/// first allowed value is used so that serde deserialization succeeds.
pub(in crate::tool_runtime::tests) fn placeholder_from_prop(prop: &Value) -> Value {
    if let Some(vals) = prop["enum"].as_array() {
        if let Some(first) = vals.first() {
            return first.clone();
        }
    }
    let kind = prop["type"].as_str().unwrap_or("string");
    match kind {
        "integer" => json!(1),
        "array" => json!([]),
        "boolean" => json!(true),
        _ => json!("value"),
    }
}

/// Helper: fetch a ToolSpec by name from the runtime.
pub(in crate::tool_runtime::tests) fn spec_named<'a>(
    specs: &'a [ToolSpec],
    name: &str,
) -> &'a ToolSpec {
    specs
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("tool '{}' missing from specs", name))
}

/// Helper: the `required` field of a tool's input schema, as Strings.
pub(in crate::tool_runtime::tests) fn required_fields(spec: &ToolSpec) -> Vec<String> {
    spec.input_schema["required"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|v| v.as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub(in crate::tool_runtime::tests) fn local_project_config(path: &str) -> ProjectConfig {
    ProjectConfig {
        path: path.to_string(),
        executor: Executor::Local,
        client_id: None,
        allow_patch: true,
        allow_command_requests: false,
        allow_raw_command_requests: false,
        default_apply_patch_backend: None,
        allowed_checks: vec![],
        checks: None,
        commands: HashMap::new(),
        hooks: HashMap::new(),
    }
}

pub(in crate::tool_runtime::tests) fn runtime_with_project(
    root: &Path,
    project_id: &str,
) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        project_id.to_string(),
        local_project_config(&root.to_string_lossy()),
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(in crate::tool_runtime::tests) fn codex_config_with_allowlist(
    allowlist: &[&str],
) -> CodexConfig {
    CodexConfig {
        bin: "codex".to_string(),
        approval_mode: String::new(),
        default_timeout_secs: 3600,
        max_prompt_bytes: 100_000,
        allowed_extra_args: allowlist.iter().map(|s| s.to_string()).collect(),
    }
}

pub(in crate::tool_runtime::tests) fn runtime_with_codex(
    root: &Path,
    codex: CodexConfig,
) -> ToolRuntime {
    let mut projects = HashMap::new();
    projects.insert(
        "demo".to_string(),
        local_project_config(&root.to_string_lossy()),
    );
    let config = ProjectsConfig { projects };
    let state = ProjectsState::loaded(config, "test".to_string());
    ToolRuntime::new(
        Arc::new(state),
        Arc::new(ShellClientRegistry::default()),
        Arc::new(codex),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(in crate::tool_runtime::tests) fn runtime_with_info(info: RuntimeInfo) -> ToolRuntime {
    let projects = Arc::new(ProjectsState::failed(
        "projects not configured for test".to_string(),
        "test".to_string(),
    ));
    ToolRuntime::new(
        projects,
        Arc::new(ShellClientRegistry::default()),
        Arc::new(CodexConfig::default()),
        Arc::new(info),
    )
}
