use crate::projects::ProjectConfig;
use crate::runner_http::RunnerRegistry;
use crate::tool_runtime::{RuntimeInfo, ToolRuntime, ToolSpec};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

pub(in crate::tool_runtime::tests) const CODING_WORKFLOW_FIXTURE_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(50);

pub(in crate::tool_runtime::tests) const SAMPLE_PROJECT: &str = "agent:oe:private-drop";
pub(in crate::tool_runtime::tests) fn test_runtime() -> ToolRuntime {
    ToolRuntime::new_for_tests()
}

pub(in crate::tool_runtime::tests) fn registered_tool_names() -> Vec<String> {
    crate::tool_runtime::registered_tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect()
}

pub(in crate::tool_runtime::tests) use webcodex_tool_contracts::test_support::{
    sample_tool_args, sample_tool_args_for_spec,
};

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

pub(in crate::tool_runtime::tests) fn seed_recovery_events(
    runtime: &ToolRuntime,
    session_id: &str,
    project: &str,
    count: usize,
) {
    use crate::tool_runtime::sessions::{SessionTransport, ToolCallRecorderMetadata};

    for index in 0..count {
        let start = runtime.sessions.record_tool_call_started_with_metadata(
            Some(session_id),
            SessionTransport::Mcp,
            "run_process",
            &json!({"project": project, "executable": "true"}),
            Some(project.to_string()),
            ToolCallRecorderMetadata::default(),
            crate::tool_runtime::sessions::session_tool_contract("run_process"),
        );
        let evidence = format!("event-{index:02}-{}", "x".repeat(760));
        runtime
            .sessions
            .record_tool_call_finished(
                start,
                true,
                &json!({
                    "exit_code": 0,
                    "stdout_tail": evidence,
                    "stderr_tail": "y".repeat(760),
                    "stdout_truncated": false,
                    "stderr_truncated": false,
                    "stdout_lines": 1,
                    "stderr_lines": 1,
                    "purpose": "test",
                    "command_summary": "true",
                    "cwd": ".",
                    "executor": "agent",
                    "execution_state": "completed"
                }),
                None,
                None,
            )
            .expect("seeded recovery event");
    }
}

pub(in crate::tool_runtime::tests) fn seed_large_changed_path_events(
    runtime: &ToolRuntime,
    session_id: &str,
    project: &str,
    count: usize,
) {
    use crate::tool_runtime::sessions::{SessionTransport, ToolCallRecorderMetadata};

    for index in 0..count {
        let paths = (0..8)
            .map(|path_index| {
                format!(
                    "src/recovery-{index:02}-{path_index:02}-{}",
                    "p".repeat(450)
                )
            })
            .collect::<Vec<_>>();
        let start = runtime.sessions.record_tool_call_started_with_metadata(
            Some(session_id),
            SessionTransport::Mcp,
            "delete_project_files",
            &json!({"project": project, "paths": paths}),
            Some(project.to_string()),
            ToolCallRecorderMetadata::default(),
            crate::tool_runtime::sessions::session_tool_contract("delete_project_files"),
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({"deleted_count": 8}), None, None)
            .expect("seeded large changed-path recovery event");
    }
}

pub(in crate::tool_runtime::tests) fn local_project_config(path: &str) -> ProjectConfig {
    ProjectConfig {
        path: path.to_string(),
        client_id: "local-unit-test".to_string(),
        allow_patch: true,
    }
}

pub(in crate::tool_runtime::tests) fn runtime_with_project(
    root: &Path,
    project_id: &str,
) -> ToolRuntime {
    let _ = (root, project_id);
    ToolRuntime::new(
        Arc::new(RunnerRegistry::default()),
        Arc::new(RuntimeInfo::default()),
    )
}

pub(in crate::tool_runtime::tests) fn runtime_with_info(info: RuntimeInfo) -> ToolRuntime {
    ToolRuntime::new(Arc::new(RunnerRegistry::default()), Arc::new(info))
}
