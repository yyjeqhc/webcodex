use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use crate::tool_runtime::metadata::lookup_tool_metadata;
use crate::tool_runtime::validation_parser::VALIDATION_OUTPUT_METADATA_ABSENT_REASON;
use crate::tool_runtime::{SessionMode, ToolCall, KNOWN_TOOL_NAMES};
use serde_json::{json, Value};
use std::fs;

#[test]
fn coding_task_tools_are_registered_in_metadata_and_openapi() {
    let runtime = test_runtime();
    let specs = runtime.tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();

    for name in ["start_coding_task", "finish_coding_task"] {
        assert!(
            KNOWN_TOOL_NAMES.contains(&name),
            "{name} missing from known names"
        );
        assert!(names.contains(&name), "{name} missing from tool specs");

        let metadata = lookup_tool_metadata(name).expect("metadata");
        assert!(metadata.read_only);
        assert!(!metadata.destructive);
        assert!(!metadata.shell_like);
        assert_eq!(metadata.oauth_scope, Some("runtime:read"));

        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .expect("tool spec");
        assert_eq!(spec.annotations["readOnlyHint"], true);
        assert_eq!(spec.annotations["destructiveHint"], false);
        assert_eq!(spec.annotations["openWorldHint"], false);
    }

    let start = spec_named(&specs, "start_coding_task");
    assert_eq!(required_fields(start), vec!["project"]);
    assert!(start.input_schema["properties"]
        .as_object()
        .unwrap()
        .contains_key("include_tool_manifest"));
    assert!(start.input_schema["properties"]
        .as_object()
        .unwrap()
        .contains_key("tool_manifest_categories"));
    assert!(start.input_schema["properties"]
        .as_object()
        .unwrap()
        .contains_key("tool_manifest_limit"));
    let start_output = crate::tool_runtime::registry::output_schema_for_tool("start_coding_task");
    assert!(
        start_output["properties"]["output"]["properties"]
            .as_object()
            .unwrap()
            .contains_key("permissions"),
        "start_coding_task output schema should include permissions"
    );
    let finish = spec_named(&specs, "finish_coding_task");
    assert_eq!(required_fields(finish), vec!["project", "session_id"]);

    let openapi = crate::openapi::build_openapi_spec();
    let tool_call = &openapi["components"]["schemas"]["ToolCallRequest"];
    let tool_desc = tool_call["properties"]["tool"]["description"]
        .as_str()
        .unwrap();
    assert!(tool_desc.contains("start_coding_task"));
    assert!(tool_desc.contains("finish_coding_task"));
    let properties = tool_call["properties"].as_object().unwrap();
    for field in [
        "include_runtime_status",
        "include_git",
        "include_recent_commits",
        "include_rules",
        "include_tool_manifest",
        "tool_manifest_categories",
        "tool_manifest_limit",
        "bind_current",
        "include_hygiene",
        "include_handoff",
        "include_validation_summary",
        "include_validation",
    ] {
        assert!(
            properties.contains_key(field),
            "ToolCallRequest missing flattened field {field}"
        );
    }
    let operation_count: usize = openapi["paths"]
        .as_object()
        .unwrap()
        .values()
        .map(|methods| methods.as_object().unwrap().len())
        .sum();
    assert_eq!(operation_count, 25, "no dedicated OpenAPI operations added");
}

#[tokio::test]
async fn start_coding_task_returns_session_and_does_not_bind_current_by_default() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(
        tmp.path(),
        "AGENTS.md",
        "# Rules\n\nUse focused tests.\n",
        "add instructions",
    );
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-start", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::StartCodingTask {
                        project,
                        title: Some("implement deterministic aggregate".to_string()),
                        mode: SessionMode::Normal,
                        deny_write_tools: false,
                        deny_shell_tools: false,
                        include_runtime_status: Some(false),
                        include_git: Some(true),
                        include_recent_commits: Some(true),
                        include_rules: Some(true),
                        include_tool_manifest: Some(true),
                        tool_manifest_categories: None,
                        tool_manifest_limit: None,
                        bind_current: false,
                    },
                    Some(&auth),
                )
                .await
        }
    });

    let rules_req = next_patch_agent_request(&runtime, "coding-start")
        .await
        .expect("start_coding_task should load rules through the agent");
    assert_eq!(rules_req.kind, "file_read");
    complete_patch_agent_request(
        &runtime,
        "coding-start",
        &rules_req.request_id,
        0,
        "# Rules\n\nUse focused tests.\n",
        "",
    )
    .await;

    let status_req = next_patch_agent_request(&runtime, "coding-start")
        .await
        .expect("start_coding_task should inspect git status through the agent");
    assert!(status_req.command.contains("git status --porcelain=v1 -b"));
    let show_changes_stdout =
        "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add readme\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
    complete_patch_agent_request(
        &runtime,
        "coding-start",
        &status_req.request_id,
        0,
        show_changes_stdout,
        "",
    )
    .await;

    let log_req = next_patch_agent_request(&runtime, "coding-start")
        .await
        .expect("start_coding_task should inspect recent commits through the agent");
    assert!(log_req.command.contains("git log"));
    let commit_stdout = "0123456789012345678901234567890123456789\u{1f}0123456\u{1f}HEAD -> main\u{1f}WebCodex Test\u{1f}test@example.com\u{1f}2026-01-01T00:00:00+00:00\u{1f}add readme\u{1e}";
    complete_patch_agent_request(
        &runtime,
        "coding-start",
        &log_req.request_id,
        0,
        commit_stdout,
        "",
    )
    .await;

    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    let session_id = result.output["session"]["session_id"].as_str().unwrap();
    assert!(session_id.starts_with("wc_sess_"));
    assert_eq!(
        result.output["session"]["explicit_session_id_recommended"],
        true
    );
    assert_eq!(
        result.output["session"]["current_binding"]["bound"], false,
        "start_coding_task must not bind current by default"
    );
    assert_eq!(
        result.output["session"]["current_binding"]["process_local_in_memory"],
        true
    );
    assert_eq!(result.output["permissions"]["policy"], "dev_auto_approve");
    assert_eq!(result.output["permissions"]["auto_approve"], true);
    assert_eq!(
        result.output["permissions"]["human_approval_required"],
        false
    );

    let current = runtime
        .dispatch(ToolCall::CurrentSession {
            project: project.clone(),
        })
        .await;
    assert!(current.success, "{:?}", current.error);
    assert_eq!(current.output["found"], false);

    let inspect = result.output["recommended_flow"]["inspect"]
        .as_array()
        .unwrap();
    assert!(contains_string(inspect, "read_file"));
    assert!(contains_string(inspect, "search_project_text"));
    assert!(contains_string(inspect, "show_changes"));
    let edit = result.output["recommended_flow"]["edit"]
        .as_array()
        .unwrap();
    assert!(contains_string(edit, "replace_line_range"));
    assert!(contains_string(edit, "insert_at_line"));
    assert!(contains_string(edit, "delete_line_range"));
    assert!(contains_string(edit, "apply_text_edits"));

    assert_eq!(result.output["rules"]["present"], true);
    assert_eq!(result.output["rules"]["sources"][0]["path"], "AGENTS.md");
    let manifest = &result.output["tool_manifest"];
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["filtered"], false);
    assert_eq!(manifest["categories_requested"], Value::Null);
    assert_eq!(manifest["limit"], Value::Null);
    assert_eq!(manifest["truncated"], false);
    assert!(manifest["count"].as_u64().unwrap() > 0);
    let start_tool = manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "start_coding_task")
        .expect("start_coding_task manifest entry");
    assert!(start_tool["accepted_flattened_args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "include_tool_manifest"));
    assert!(start_tool["accepted_flattened_args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "tool_manifest_categories"));
    assert!(start_tool["accepted_flattened_args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "tool_manifest_limit"));
    assert!(start_tool.get("inputSchema").is_none());
    assert!(start_tool.get("outputSchema").is_none());
    assert_eq!(result.output["git"]["clean"], true);
    assert!(
        result.output["git"]["recent_commits"]
            .as_array()
            .unwrap()
            .len()
            >= 1
    );
}

#[tokio::test]
async fn start_coding_task_can_omit_compact_tool_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-no-manifest", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::StartCodingTask {
                project,
                title: Some("small startup payload".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                include_runtime_status: Some(false),
                include_git: Some(false),
                include_recent_commits: Some(false),
                include_rules: Some(false),
                include_tool_manifest: Some(false),
                tool_manifest_categories: None,
                tool_manifest_limit: None,
                bind_current: false,
            },
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("tool_manifest").is_none(),
        "include_tool_manifest=false should omit compact manifest"
    );
}

#[tokio::test]
async fn start_coding_task_filters_compact_tool_manifest_by_categories() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-filter", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_coding_task",
                json!({
                    "project": project,
                    "include_runtime_status": false,
                    "include_git": false,
                    "include_recent_commits": false,
                    "include_rules": false,
                    "include_tool_manifest": true,
                    "tool_manifest_categories": ["workflow", "session"]
                }),
            )
            .unwrap(),
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    let manifest = &result.output["tool_manifest"];
    assert_eq!(manifest["filtered"], true);
    assert_eq!(
        manifest["categories_requested"],
        json!(["workflow", "session"])
    );
    assert_eq!(manifest["limit"], Value::Null);
    assert_eq!(manifest["truncated"], false);
    let tools = manifest["tools"].as_array().unwrap();
    assert!(tools.iter().any(|tool| tool["name"] == "start_coding_task"));
    assert!(tools.iter().any(|tool| tool["name"] == "session_summary"));
    assert!(tools
        .iter()
        .all(|tool| matches!(tool["category"].as_str(), Some("workflow" | "session"))));
    assert!(tools
        .iter()
        .all(|tool| tool.get("inputSchema").is_none() && tool.get("outputSchema").is_none()));
    assert!(tools
        .iter()
        .all(|tool| tool["accepted_flattened_args"].is_array()));
}

#[tokio::test]
async fn start_coding_task_manifest_limit_truncates_filtered_entries() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-limit", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_coding_task",
                json!({
                    "project": project,
                    "include_runtime_status": false,
                    "include_git": false,
                    "include_recent_commits": false,
                    "include_rules": false,
                    "include_tool_manifest": true,
                    "tool_manifest_categories": ["session"],
                    "tool_manifest_limit": 2
                }),
            )
            .unwrap(),
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    let manifest = &result.output["tool_manifest"];
    assert_eq!(manifest["filtered"], true);
    assert_eq!(manifest["limit"], 2);
    assert_eq!(manifest["truncated"], true);
    assert_eq!(manifest["count"], 2);
    assert!(manifest["filtered_count"].as_u64().unwrap() > 2);
    assert!(manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["category"] == "session"));
}

#[tokio::test]
async fn finish_coding_task_requires_explicit_session_and_returns_structured_fields() {
    let missing_session =
        ToolCall::from_tool_name("finish_coding_task", json!({"project": "demo"})).unwrap_err();
    assert!(missing_session.contains("session_id"));

    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-finish", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let start = runtime
        .dispatch_with_auth(
            ToolCall::StartCodingTask {
                project: project.clone(),
                title: Some("finish contract".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                include_runtime_status: Some(false),
                include_git: Some(false),
                include_recent_commits: Some(false),
                include_rules: Some(false),
                include_tool_manifest: Some(false),
                tool_manifest_categories: None,
                tool_manifest_limit: None,
                bind_current: false,
            },
            Some(&auth),
        )
        .await;
    assert!(start.success, "{:?}", start.error);
    let session_id = start.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        include_diff: Some(false),
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "coding-finish")
        .await
        .expect("finish_coding_task should inspect changes through the agent");
    assert!(req.command.contains("git status --porcelain=v1 -b"));
    let show_changes_stdout = "## main\n M README.md\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add readme\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n README.md | 1 +\n 1 file changed, 1 insertion(+)\n";
    complete_patch_agent_request(
        &runtime,
        "coding-finish",
        &req.request_id,
        0,
        show_changes_stdout,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["deterministic"], true);
    assert_eq!(result.output["llm_summary"], false);
    assert_eq!(result.output["workspace"]["clean"], false);
    assert_eq!(result.output["changes"]["hunks_truncated"], false);
    assert!(result.output["changes"]["show_changes"].is_object());
    let validation = &result.output["validation"];
    assert_eq!(validation["available"], false);
    assert_eq!(validation["status"], "not_run");
    assert_eq!(validation["reason"], "no_validation_tool_invoked");
    assert_eq!(validation["source"], "session_ledger");
    assert_eq!(validation["events_total"], 0);
    assert!(validation["events"].as_array().unwrap().is_empty());
    assert_eq!(result.output["permissions"]["policy"], "dev_auto_approve");
    assert_eq!(result.output["permissions"]["required_count"], 0);
    assert_eq!(result.output["permissions"]["auto_approved_count"], 0);
    assert!(result.output["permissions"]["recent"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(validation["parser"]["available"], false);
    assert_eq!(
        validation["parser"]["reason"],
        VALIDATION_OUTPUT_METADATA_ABSENT_REASON
    );
    assert_no_raw_validation_output_fields(validation, "finish validation summary");
    assert!(validation.get("observed_commands").is_none());
    assert!(result.output["hygiene"].is_null());
    assert!(result.output["handoff"].is_null());
    assert!(result.output["final_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "dirty_worktree"));
}

#[tokio::test]
async fn finish_coding_task_includes_active_jobs_warning_without_logs() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    caps.git = true;
    caps.async_shell_jobs = true;
    let project_path = tmp.path().to_string_lossy().to_string();
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        "coding-finish-jobs",
        &auth,
        caps,
        vec![registered_project("demo", &project_path)],
    )
    .await;
    let project = "agent:coding-finish-jobs:demo".to_string();
    let start = runtime
        .dispatch_with_auth(
            ToolCall::StartCodingTask {
                project: project.clone(),
                title: Some("finish active jobs".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                include_runtime_status: Some(false),
                include_git: Some(false),
                include_recent_commits: Some(false),
                include_rules: Some(false),
                include_tool_manifest: Some(false),
                tool_manifest_categories: None,
                tool_manifest_limit: None,
                bind_current: false,
            },
            Some(&auth),
        )
        .await;
    assert!(start.success, "{:?}", start.error);
    let session_id = start.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    let run = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "printf secret-job-output".to_string(),
                session_id: Some(session_id.clone()),
                timeout_secs: None,
                cwd: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let queued_job = next_agent_request_for_client(&runtime, "coding-finish-jobs")
        .await
        .expect("run_job should enqueue a job request");
    assert_eq!(queued_job.kind, "start_job");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        include_diff: Some(false),
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "coding-finish-jobs")
        .await
        .expect("finish_coding_task should inspect changes through the agent");
    assert!(req.command.contains("git status --porcelain=v1 -b"));
    let show_changes_stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add readme\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
    complete_patch_agent_request_for_instance(
        &runtime,
        "coding-finish-jobs",
        "inst-coding-finish-jobs",
        &req.request_id,
        0,
        show_changes_stdout,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["jobs"]["active_count"], 1);
    assert_eq!(
        result.output["jobs"]["recent"][0]["job_id"],
        run.output["job_id"]
    );
    assert!(result.output["final_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "active_jobs_present"));
    assert_no_raw_validation_output_fields(&result.output["jobs"], "finish jobs summary");
    let serialized = serde_json::to_string(&result.output["jobs"]).unwrap();
    assert!(!serialized.contains("secret-job-output"));
}

fn contains_string(values: &[Value], needle: &str) -> bool {
    values.iter().any(|value| value.as_str() == Some(needle))
}

fn json_contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.contains_key(key) || map.values().any(|value| json_contains_key(value, key))
        }
        Value::Array(values) => values.iter().any(|value| json_contains_key(value, key)),
        _ => false,
    }
}

fn assert_no_raw_validation_output_fields(value: &Value, context: &str) {
    for key in [
        "stdout",
        "stderr",
        "stdout_tail",
        "stderr_tail",
        "stdout_tail_excerpt",
        "stderr_tail_excerpt",
        "validation_output_summary",
    ] {
        assert!(
            !json_contains_key(value, key),
            "{context} must not include {key}: {value}"
        );
    }
}
