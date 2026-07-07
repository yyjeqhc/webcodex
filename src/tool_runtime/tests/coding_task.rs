use super::support::*;
use crate::auth::AuthContext;
use crate::shell_protocol::{AgentPolicySummary, ShellClientCapabilities};
use crate::tool_runtime::metadata::lookup_tool_metadata;
use crate::tool_runtime::sessions::SessionTransport;
use crate::tool_runtime::validation_parser::VALIDATION_OUTPUT_METADATA_ABSENT_REASON;
use crate::tool_runtime::{
    is_known_tool_name, registered_tool_specs, SessionMode, ToolCall, ToolResult, ToolRuntime,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[test]
fn coding_task_tools_are_registered_in_metadata_and_openapi() {
    let specs = registered_tool_specs();
    let names: Vec<&str> = specs.iter().map(|spec| spec.name.as_str()).collect();

    for name in ["start_coding_task", "finish_coding_task"] {
        assert!(is_known_tool_name(name), "{name} missing from known names");
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
    assert!(
        start_output["properties"]["output"]["properties"]
            .as_object()
            .unwrap()
            .contains_key("startup_verdict"),
        "start_coding_task output schema should include startup_verdict"
    );
    let finish = spec_named(&specs, "finish_coding_task");
    assert_eq!(required_fields(finish), vec!["project", "session_id"]);
    let finish_props = finish.input_schema["properties"].as_object().unwrap();
    assert!(
        finish_props.contains_key("include_workspace"),
        "finish_coding_task input schema should accept include_workspace as a compatibility flag"
    );
    assert!(
        !required_fields(finish)
            .iter()
            .any(|field| field == "include_workspace"),
        "include_workspace must remain optional"
    );
    let finish_output = crate::tool_runtime::registry::output_schema_for_tool("finish_coding_task");
    let finish_output_props = finish_output["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(
        finish_output_props.contains_key("verdict"),
        "finish_coding_task output schema should include verdict"
    );
    assert!(
        finish_output_props.contains_key("finish_verdict"),
        "finish_coding_task output schema should include finish_verdict"
    );

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
        "compact_startup",
        "include_git",
        "include_recent_commits",
        "include_rules",
        "include_tool_manifest",
        "tool_manifest_categories",
        "tool_manifest_limit",
        "bind_current",
        "include_hygiene",
        "include_handoff",
        "include_workspace",
        "include_validation_summary",
        "include_validation",
        "summary_only",
        "expected_failure",
        "expected_failure_kind",
        "assertion_name",
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
                        compact_startup: false,
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
    for field in [
        "session",
        "runtime_status",
        "permissions",
        "rules",
        "git",
        "recommended_flow",
        "warnings",
        "tool_manifest",
    ] {
        assert!(
            result.output.get(field).is_some(),
            "start_coding_task output should include {field}"
        );
    }
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
        .any(|field| field == "compact_startup"));
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
                compact_startup: false,
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
async fn start_coding_task_runtime_status_defaults_to_full_output() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let runtime = test_runtime();
    let policy = AgentPolicySummary {
        allowed_roots: vec![PathBuf::from("/tmp/startup-full-allowed-root")],
        ..Default::default()
    };
    register_agent_with_shell_profiles(
        &runtime,
        "coding-full-status",
        Some(policy),
        vec![registered_project("demo", &tmp.path().to_string_lossy())],
    )
    .await;
    let auth = auth_context(None, true);
    let project = "agent:coding-full-status:demo".to_string();

    let result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_coding_task",
                json!({
                    "project": project,
                    "include_runtime_status": true,
                    "include_git": false,
                    "include_recent_commits": false,
                    "include_rules": false,
                    "include_tool_manifest": false
                }),
            )
            .unwrap(),
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    let runtime_status = &result.output["runtime_status"];
    assert!(runtime_status["tools"]["names"].is_array());
    assert!(runtime_status["configured_public_url"].is_null());
    assert_eq!(
        runtime_status["agents"]["clients"][0]["policy"]["allowed_roots"][0],
        "/tmp/startup-full-allowed-root"
    );
}

#[tokio::test]
async fn start_coding_task_compact_startup_returns_sanitized_runtime_summary() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let runtime = test_runtime();
    let policy = AgentPolicySummary {
        allowed_roots: vec![PathBuf::from("/tmp/compact-allowed-root-never-emit")],
        ..Default::default()
    };
    register_agent_with_shell_profiles(
        &runtime,
        "coding-compact-status",
        Some(policy),
        vec![registered_project("demo", &tmp.path().to_string_lossy())],
    )
    .await;
    let auth = auth_context(None, true);
    let project = "agent:coding-compact-status:demo".to_string();

    let result = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name(
                "start_coding_task",
                json!({
                    "project": project,
                    "include_runtime_status": true,
                    "compact_startup": true,
                    "include_git": false,
                    "include_recent_commits": false,
                    "include_rules": false,
                    "include_tool_manifest": false
                }),
            )
            .unwrap(),
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    let summary = &result.output["runtime_status"];
    assert_eq!(summary["compact"], true);
    for pointer in [
        "/build/version",
        "/build/git_commit",
        "/build/git_dirty",
        "/tools/count",
        "/jobs/active_count",
        "/agents/summary/online",
        "/projects/effective/status",
        "/projects/agent_registered/online_count",
    ] {
        assert!(
            summary.pointer(pointer).is_some(),
            "compact startup runtime_status should include {pointer}"
        );
    }
    assert_eq!(summary["build"]["version"], env!("CARGO_PKG_VERSION"));
    assert!(summary["build"].get("git_commit").is_some());
    assert!(summary["build"].get("git_dirty").is_some());
    assert!(summary["tools"]["count"].as_u64().unwrap() > 0);
    assert!(summary["tools"].get("names").is_none());
    assert_eq!(summary["jobs"]["active_count"], 0);
    assert!(summary["agents"]["summary"].is_object());
    assert_eq!(summary["agents"]["summary"]["count"], 1);
    assert_eq!(summary["agents"]["summary"]["online"], 1);
    assert_eq!(
        summary["agents"]["summary"]["clients"][0]["client_id"],
        "coding-compact-status"
    );
    assert_eq!(
        summary["agents"]["summary"]["clients"][0]["status"],
        "online"
    );
    assert_eq!(
        summary["agents"]["summary"]["clients"][0]["transport"],
        "polling"
    );
    assert_eq!(
        summary["agents"]["summary"]["clients"][0]["projects_count"],
        1
    );
    assert_eq!(summary["projects"]["effective"]["status"], "ok");
    assert_eq!(summary["projects"]["effective"]["count"], 1);
    assert_eq!(summary["projects"]["agent_registered"]["count"], 1);
    assert_eq!(summary["projects"]["agent_registered"]["online_count"], 1);
    assert!(summary["projects"].get("server_static").is_none());
    let verdict = &result.output["startup_verdict"];
    assert_startup_verdict_shape(verdict);
    assert_ne!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], false);
    assert_check_reason(verdict, "workspace", "workspace_not_checked");
    assert_check_reason(verdict, "tool_manifest", "tool_manifest_not_requested");
    assert_compact_verdict_safe(verdict, "startup verdict");

    let serialized = serde_json::to_string(summary).unwrap();
    for forbidden in [
        "tools.names",
        "policy",
        "allowed_roots",
        "compact-allowed-root-never-emit",
        "stdout",
        "stderr",
        "command",
        "env",
        "token",
        "secret",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "compact startup leaked {forbidden}: {serialized}"
        );
    }
}

#[tokio::test]
async fn start_coding_task_compact_startup_verdict_accepts_clean_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-start-verdict", "demo", tmp.path()).await;
    let auth = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::from_tool_name(
                        "start_coding_task",
                        json!({
                            "project": project,
                            "include_runtime_status": true,
                            "compact_startup": true,
                            "include_git": true,
                            "include_recent_commits": false,
                            "include_rules": false,
                            "include_tool_manifest": true
                        }),
                    )
                    .unwrap(),
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "coding-start-verdict")
        .await
        .expect("start_coding_task should inspect clean workspace");
    complete_agent_request_by_running_locally(&runtime, "coding-start-verdict", req).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    let verdict = &result.output["startup_verdict"];
    assert_startup_verdict_shape(verdict);
    assert_eq!(verdict["status"], "pass");
    assert_eq!(verdict["blocking"], false);
    assert_check_status(verdict, "runtime_status", "pass");
    assert_check_status(verdict, "workspace", "pass");
    assert_check_status(verdict, "jobs", "pass");
    assert_check_status(verdict, "agent", "pass");
    assert_check_status(verdict, "tool_manifest", "pass");
    assert_compact_verdict_safe(verdict, "startup clean verdict");
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
    let verdict = &result.output["startup_verdict"];
    assert_startup_verdict_shape(verdict);
    assert_eq!(verdict["status"], "warn");
    assert_check_reason(verdict, "runtime_status", "runtime_status_not_requested");
    assert_check_reason(verdict, "workspace", "workspace_not_checked");
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
    assert_eq!(manifest["truncation_reason"], "limit");
    assert_eq!(manifest["limit_applied"], true);
    assert_eq!(manifest["requested_limit"], 2);
    assert_eq!(manifest["count"], 2);
    assert_eq!(manifest["returned_count"], 2);
    assert!(
        manifest["total_count"].as_u64().unwrap() >= manifest["filtered_count"].as_u64().unwrap()
    );
    assert!(manifest["filtered_count"].as_u64().unwrap() > 2);
    assert!(!serde_json::to_string(manifest)
        .unwrap()
        .contains("ResponseTooLarge"));
    assert!(manifest["tools"]
        .as_array()
        .unwrap()
        .iter()
        .all(|tool| tool["category"] == "session"));
    let verdict = &result.output["startup_verdict"];
    assert_startup_verdict_shape(verdict);
    assert_ne!(verdict["status"], "fail");
    assert_check_reason(verdict, "tool_manifest", "truncated_by_limit");
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
                compact_startup: false,
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
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
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
    assert_eq!(result.output["permissions"]["manual_approved_count"], 0);
    assert_eq!(result.output["permissions"]["approved_count"], 0);
    assert_eq!(result.output["permissions"]["total_approved_count"], 0);
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
    assert_eq!(result.output["review_evidence"]["available"], true);
    assert_eq!(result.output["review_evidence"]["source"], "session_ledger");
    assert_eq!(result.output["review_evidence"]["total"], 1);
    assert_eq!(
        result.output["review_evidence"]["workspace_review_count"],
        1
    );
    assert_eq!(
        result.output["review_evidence"]["tools"],
        json!(["show_changes"])
    );
    assert_review_evidence_tools_safe(&result.output["review_evidence"]);
    assert!(result.output["hygiene"].is_null());
    assert!(result.output["handoff"].is_null());
    assert!(result.output["final_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "dirty_worktree"));
}

#[tokio::test]
async fn finish_coding_task_summary_only_is_compact_for_clean_project() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-finish-compact", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("compact finish".to_string()));
    let session_id = session.session_id.clone();

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
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(true),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });

    for _ in 0..200 {
        if task.is_finished() {
            break;
        }
        if let Some(req) = next_patch_agent_request(&runtime, "coding-finish-compact").await {
            complete_agent_request_by_running_locally(&runtime, "coding-finish-compact", req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    assert!(
        task.is_finished(),
        "finish_coding_task summary_only did not finish after read-only agent requests"
    );
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["summary_only"], true);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["session_id"], session_id);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["jobs"]["active_count"], 0);
    assert_eq!(result.output["jobs"]["blocking_active_count"], 0);
    assert_eq!(result.output["permissions"]["total_approved_count"], 0);
    assert_eq!(result.output["tool_failures"]["expected_count"], 0);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 0);
    assert!(result.output["tool_failures"]
        .get("expectation_mismatch_count")
        .is_some());
    assert!(result.output["tool_failures"]
        .get("unexpected_success_count")
        .is_some());
    assert_eq!(result.output["validation"]["status"], "not_run");
    assert_eq!(
        result.output["validation"]["reason"],
        "no_validation_tool_invoked"
    );
    assert_eq!(result.output["review_evidence"]["available"], true);
    assert!(
        result.output["review_evidence"]["total"].as_u64().unwrap() > 0,
        "finish summary_only should count closeout review evidence: {}",
        result.output["review_evidence"]
    );
    assert!(
        result.output["review_evidence"]["workspace_review_count"]
            .as_u64()
            .unwrap()
            > 0
            || result.output["review_evidence"]["hygiene_review_count"]
                .as_u64()
                .unwrap()
                > 0,
        "finish summary_only should count workspace or hygiene review evidence: {}",
        result.output["review_evidence"]
    );
    assert_eq!(
        result.output["review_evidence"]["tools"]
            .as_array()
            .expect("review evidence tools array")
            .first()
            .and_then(Value::as_str),
        Some("show_changes")
    );
    assert_review_evidence_tools_safe(&result.output["review_evidence"]);
    assert!(result.output["warnings"].as_array().unwrap().is_empty());
    assert!(result.output.get("verdict").is_some());
    assert!(result.output.get("finish_verdict").is_some());
    assert_eq!(result.output["finish_verdict"], result.output["verdict"]);
    assert!(result.output["suggested_next_actions"].is_array());
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "warn");
    assert_eq!(verdict["blocking"], false);
    let finish_verdict = &result.output["finish_verdict"];
    assert_workflow_verdict_shape(finish_verdict);
    assert_eq!(finish_verdict["status"], "warn");
    assert_eq!(finish_verdict["blocking"], false);
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "validation_not_run_with_review_evidence",
    );
    assert_reason_list_contains(
        finish_verdict,
        "warning_reasons",
        "validation_not_run_with_review_evidence",
    );
    assert_reason_list_not_contains(verdict, "warning_reasons", "validation_not_run");
    assert!(verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str()
            == Some(
                "no structured validation was run; review evidence is available for task-appropriate closeout"
            )));
    assert!(result.output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str()
            == Some(
                "no structured validation was run; review evidence is available for task-appropriate closeout"
            )));
    assert_verdict_actions_mirrored_at_top_level(&result.output);
    assert!(!verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str() == Some("run validation before closeout when available")));
    assert_compact_verdict_safe(verdict, "finish compact verdict");

    let serialized = serde_json::to_string(&result.output).unwrap();
    for forbidden in [
        "recent_events",
        "recent_failed_tools",
        "stdout",
        "stderr",
        "tail",
        "excerpt",
        "command",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "summary_only finish leaked {forbidden}: {serialized}"
        );
    }
    assert!(
        !serialized.contains("\"show_changes\":"),
        "summary_only finish leaked raw show_changes payload: {serialized}"
    );
}

#[tokio::test]
async fn finish_coding_task_summary_only_includes_review_evidence_for_docs_only_session() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "docs.md", "hello\n", "add docs");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-finish-docs", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("docs-only finish".to_string()));
    let session_id = session.session_id.clone();

    record_coding_task_tool_event(
        &runtime,
        &session_id,
        "replace_line_range",
        json!({
            "project": project,
            "path": "docs.md",
            "start_line": 1,
            "end_line": 1,
            "replacement": "updated docs"
        }),
        true,
        json!({}),
    );
    record_coding_task_tool_event(
        &runtime,
        &session_id,
        "search_project_text",
        json!({"project": project, "query": "docs"}),
        true,
        json!({}),
    );
    record_coding_task_tool_event(
        &runtime,
        &session_id,
        "show_changes",
        json!({"project": project, "include_diff": false}),
        true,
        json!({}),
    );

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
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(true),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..200 {
        if task.is_finished() {
            break;
        }
        if let Some(req) = next_patch_agent_request(&runtime, "coding-finish-docs").await {
            complete_agent_request_by_running_locally(&runtime, "coding-finish-docs", req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    assert!(
        task.is_finished(),
        "finish_coding_task summary_only did not finish after read-only agent requests"
    );
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["summary_only"], true);
    assert_eq!(result.output["validation"]["status"], "not_run");
    assert_eq!(
        result.output["validation"]["reason"],
        "no_validation_tool_invoked"
    );
    assert_eq!(result.output["review_evidence"]["available"], true);
    assert!(
        result.output["review_evidence"]["total"].as_u64().unwrap() >= 2,
        "finish summary_only should preserve existing and closeout review evidence: {}",
        result.output["review_evidence"]
    );
    assert_eq!(result.output["review_evidence"]["search_count"], 1);
    assert!(
        result.output["review_evidence"]["workspace_review_count"]
            .as_u64()
            .unwrap()
            >= 2,
        "finish summary_only should include manual and closeout workspace review evidence: {}",
        result.output["review_evidence"]
    );
    let tools = result.output["review_evidence"]["tools"]
        .as_array()
        .expect("review evidence tools array");
    assert!(tools
        .iter()
        .any(|tool| tool.as_str() == Some("search_project_text")));
    assert!(tools
        .iter()
        .any(|tool| tool.as_str() == Some("show_changes")));
    assert_review_evidence_tools_safe(&result.output["review_evidence"]);
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "warn");
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "validation_not_run_with_review_evidence",
    );
    assert_reason_list_not_contains(verdict, "warning_reasons", "validation_not_run");
}

#[tokio::test]
async fn finish_coding_task_summary_only_verdict_fails_for_dirty_workspace() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    fs::write(tmp.path().join("README.md"), "changed\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-finish-dirty", "demo", tmp.path()).await;
    let auth = auth_context(None, true);
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("dirty finish".to_string()));
    let session_id = session.session_id.clone();

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
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "coding-finish-dirty")
        .await
        .expect("finish_coding_task should inspect changes");
    complete_agent_request_by_running_locally(&runtime, "coding-finish-dirty", req).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], false);
    assert_eq!(result.output["verdict"]["status"], "fail");
    assert_eq!(result.output["verdict"]["blocking"], true);
    assert_workflow_verdict_shape(&result.output["verdict"]);
    assert_reason_list_contains(
        &result.output["verdict"],
        "blocking_reasons",
        "workspace_dirty",
    );
    assert_compact_verdict_safe(&result.output["verdict"], "dirty finish verdict");
}

#[tokio::test]
async fn finish_coding_task_summary_only_warns_for_resolved_historical_validation_failures() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project =
        register_agent_project_at_path(&runtime, "coding-finish-resolved", "demo", tmp.path())
            .await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("resolved validation finish".to_string()),
    );
    let session_id = session.session_id.clone();

    record_coding_task_tool_event(
        &runtime,
        &session_id,
        "cargo_test",
        json!({
            "project": project,
            "expected_failure": true,
            "expected_failure_kind": "validation_failed",
            "assertion_name": "pre-fix validation should fail"
        }),
        false,
        json!({
            "exit_code": 101,
            "failure_kind": "validation_failed"
        }),
    );
    record_coding_task_tool_event(
        &runtime,
        &session_id,
        "cargo_check",
        json!({"project": project}),
        true,
        json!({"exit_code": 0}),
    );

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
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(true),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..200 {
        if task.is_finished() {
            break;
        }
        if let Some(req) = next_patch_agent_request(&runtime, "coding-finish-resolved").await {
            complete_agent_request_by_running_locally(&runtime, "coding-finish-resolved", req)
                .await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    assert!(
        task.is_finished(),
        "finish_coding_task summary_only did not finish after read-only agent requests"
    );
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 0);
    assert_eq!(result.output["validation"]["status"], "mixed");
    assert_eq!(result.output["validation"]["latest_status"], "passed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["count"],
        1
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["resolved"],
        true
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        false
    );
    let verdict = &result.output["verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_ne!(verdict["status"], "fail");
    assert_eq!(verdict["status"], "warn");
    assert_eq!(verdict["blocking"], false);
    assert_eq!(result.output["finish_verdict"], result.output["verdict"]);
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "validation_historical_failures_resolved",
    );
    assert_reason_list_not_contains(verdict, "blocking_reasons", "validation_mixed");
    assert!(verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str()
            == Some(
                "historical validation failures were resolved by later successful validation"
            )));
    assert_compact_verdict_safe(verdict, "resolved validation finish verdict");
}

#[tokio::test]
async fn finish_coding_task_summary_only_does_not_block_resolved_cargo_fmt_failure() {
    let fixture = finish_summary_fixture("coding-finish-resolved-fmt").await;

    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "cargo_fmt",
        json!({"project": fixture.project.clone(), "check": true}),
        false,
        json!({
            "exit_code": 1,
            "failure_kind": "validation_failed"
        }),
    );
    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "cargo_fmt",
        json!({"project": fixture.project.clone(), "check": true}),
        true,
        json!({"exit_code": 0}),
    );

    let result = finish_coding_task_summary_only_with_agent(
        &fixture.runtime,
        fixture.client_id,
        fixture.project,
        fixture.session_id,
        fixture.auth,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 1);
    assert_eq!(result.output["validation"]["status"], "mixed");
    assert_eq!(result.output["validation"]["latest_status"], "passed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["resolved"],
        true
    );
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        false
    );
    let verdict = &result.output["finish_verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_ne!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], false);
    assert_eq!(result.output["finish_verdict"], result.output["verdict"]);
    assert_reason_list_not_contains(verdict, "blocking_reasons", "unexpected_tool_failures");
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "validation_historical_failures_resolved",
    );
    assert_reason_list_contains(
        verdict,
        "warning_reasons",
        "resolved_validation_like_tool_failures",
    );
}

#[tokio::test]
async fn finish_coding_task_summary_only_warns_for_cargo_test_zero_tests_success() {
    let fixture = finish_summary_fixture("coding-finish-zero-tests").await;

    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "cargo_test",
        json!({
            "project": fixture.project.clone(),
            "expected_failure": true,
            "expected_failure_kind": "validation_failed",
            "assertion_name": "negative assertion accidentally ran zero tests"
        }),
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "running 0 tests\n\n\
                test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "tests_detected": true,
            "tests_run_count": 0,
            "zero_tests_run": true
        }),
    );

    let result = finish_coding_task_summary_only_with_agent(
        &fixture.runtime,
        fixture.client_id,
        fixture.project,
        fixture.session_id,
        fixture.auth,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(
        result.output["tool_failures"]["unexpected_success_count"],
        1
    );
    assert_eq!(
        result.output["tool_failures"]["expectation_mismatch_count"],
        0
    );
    assert_eq!(result.output["validation"]["status"], "passed");
    assert_eq!(
        result.output["validation"]["cargo_test_zero_tests_run"],
        true
    );
    let verdict = &result.output["finish_verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], true);
    assert_eq!(result.output["finish_verdict"], result.output["verdict"]);
    assert_reason_list_contains(verdict, "blocking_reasons", "unexpected_successes");
    assert_reason_list_contains(verdict, "warning_reasons", "cargo_test_zero_tests");
    assert!(verdict["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action.as_str()
            == Some("cargo_test ran zero tests; verify the test filter or command")));
}

#[tokio::test]
async fn finish_coding_task_summary_only_blocks_unresolved_cargo_fmt_failure() {
    let fixture = finish_summary_fixture("coding-finish-unresolved-fmt").await;

    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "cargo_fmt",
        json!({"project": fixture.project.clone(), "check": true}),
        false,
        json!({
            "exit_code": 1,
            "failure_kind": "validation_failed"
        }),
    );

    let result = finish_coding_task_summary_only_with_agent(
        &fixture.runtime,
        fixture.client_id,
        fixture.project,
        fixture.session_id,
        fixture.auth,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 1);
    assert_eq!(result.output["validation"]["status"], "failed");
    assert_eq!(result.output["validation"]["latest_status"], "failed");
    assert_eq!(
        result.output["validation"]["historical_failures"]["unresolved"],
        true
    );
    let verdict = &result.output["finish_verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], true);
    assert_reason_list_contains(verdict, "blocking_reasons", "unexpected_tool_failures");
}

#[tokio::test]
async fn finish_coding_task_summary_only_keeps_non_validation_tool_failures_blocking() {
    let fixture = finish_summary_fixture("coding-finish-read-failure").await;

    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "read_file",
        json!({"project": fixture.project.clone(), "path": "README.md"}),
        false,
        json!({
            "error_kind": "permission_denied"
        }),
    );
    record_coding_task_tool_event(
        &fixture.runtime,
        &fixture.session_id,
        "cargo_test",
        json!({"project": fixture.project.clone()}),
        true,
        json!({"exit_code": 0}),
    );

    let result = finish_coding_task_summary_only_with_agent(
        &fixture.runtime,
        fixture.client_id,
        fixture.project,
        fixture.session_id,
        fixture.auth,
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["workspace_clean"], true);
    assert_eq!(result.output["hygiene_clean"], true);
    assert_eq!(result.output["tool_failures"]["unexpected_count"], 1);
    assert_eq!(result.output["validation"]["status"], "passed");
    assert_eq!(result.output["validation"]["latest_status"], "passed");
    let verdict = &result.output["finish_verdict"];
    assert_workflow_verdict_shape(verdict);
    assert_eq!(verdict["status"], "fail");
    assert_eq!(verdict["blocking"], true);
    assert_reason_list_contains(verdict, "blocking_reasons", "unexpected_tool_failures");
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
                compact_startup: false,
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
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
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
    assert_eq!(result.output["jobs"]["running_count"], 1);
    assert_eq!(result.output["jobs"]["stop_requested_count"], 0);
    assert_eq!(result.output["jobs"]["terminal_pending_count"], 0);
    assert_eq!(result.output["jobs"]["blocking_active_count"], 1);
    assert_eq!(result.output["jobs"]["nonblocking_active_count"], 0);
    assert_eq!(
        result.output["jobs"]["recent"][0]["job_id"],
        run.output["job_id"]
    );
    assert!(result.output["final_warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "active_jobs_present" && warning["blocking"] == true));
    assert_no_raw_validation_output_fields(&result.output["jobs"], "finish jobs summary");
    let serialized = serde_json::to_string(&result.output["jobs"]).unwrap();
    assert!(!serialized.contains("secret-job-output"));
}

#[tokio::test]
async fn finish_coding_task_treats_stop_requested_jobs_as_nonblocking() {
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
        "coding-finish-stop-pending",
        &auth,
        caps,
        vec![registered_project("demo", &project_path)],
    )
    .await;
    let project = "agent:coding-finish-stop-pending:demo".to_string();
    let start = runtime
        .dispatch_with_auth(
            ToolCall::StartCodingTask {
                project: project.clone(),
                title: Some("finish stop pending".to_string()),
                mode: SessionMode::Normal,
                deny_write_tools: false,
                deny_shell_tools: false,
                include_runtime_status: Some(false),
                compact_startup: false,
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
                command: "printf stop-pending-secret-output".to_string(),
                session_id: Some(session_id.clone()),
                timeout_secs: None,
                cwd: None,
            },
            Some(&auth),
        )
        .await;
    assert!(run.success, "{:?}", run.error);
    let job_id = run.output["job_id"].as_str().unwrap().to_string();
    let start_job = next_agent_request_for_client(&runtime, "coding-finish-stop-pending")
        .await
        .expect("run_job should enqueue a job request");
    assert_eq!(start_job.kind, "start_job");

    let stop = runtime
        .dispatch_with_auth(
            ToolCall::StopJob {
                project: project.clone(),
                job_id: job_id.clone(),
                session_id: Some(session_id.clone()),
                confirm: true,
            },
            Some(&auth),
        )
        .await;
    assert!(stop.success, "{:?}", stop.error);
    assert_eq!(stop.output["status_after"], "stop_requested");
    let stop_req = next_agent_request_for_client(&runtime, "coding-finish-stop-pending")
        .await
        .expect("stop_job should enqueue a stop request");
    assert_eq!(stop_req.kind, "stop_job");

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
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "coding-finish-stop-pending")
        .await
        .expect("finish_coding_task should inspect changes through the agent");
    assert!(req.command.contains("git status --porcelain=v1 -b"));
    let show_changes_stdout = "## main\n@@WEBCODEX_SHOW_CHANGES_SEP@@\nabc123\0abc123\0add readme\n@@WEBCODEX_SHOW_CHANGES_SEP@@\n";
    complete_patch_agent_request_for_instance(
        &runtime,
        "coding-finish-stop-pending",
        "inst-coding-finish-stop-pending",
        &req.request_id,
        0,
        show_changes_stdout,
        "",
    )
    .await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["jobs"]["active_count"], 1);
    assert_eq!(result.output["jobs"]["running_count"], 0);
    assert_eq!(result.output["jobs"]["stop_requested_count"], 1);
    assert_eq!(result.output["jobs"]["terminal_pending_count"], 1);
    assert_eq!(result.output["jobs"]["blocking_active_count"], 0);
    assert_eq!(result.output["jobs"]["nonblocking_active_count"], 1);
    assert_eq!(result.output["jobs"]["recent"][0]["job_id"], job_id);
    let final_warnings = result.output["final_warnings"].as_array().unwrap();
    assert!(final_warnings
        .iter()
        .all(|warning| warning["kind"] != "active_jobs_present"));
    assert!(final_warnings.iter().any(|warning| {
        warning["kind"] == "jobs_terminal_pending" && warning["blocking"] == false
    }));
    assert_no_raw_validation_output_fields(&result.output["jobs"], "finish jobs summary");
    let serialized = serde_json::to_string(&result.output["jobs"]).unwrap();
    assert!(!serialized.contains("stop-pending-secret-output"));
}

fn contains_string(values: &[Value], needle: &str) -> bool {
    values.iter().any(|value| value.as_str() == Some(needle))
}

fn assert_check_status(verdict: &Value, name: &str, status: &str) {
    let check = verdict["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("missing startup check {name}: {verdict}"));
    assert_eq!(check["status"], status);
}

fn assert_check_reason(verdict: &Value, name: &str, reason: &str) {
    let check = verdict["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|check| check["name"] == name)
        .unwrap_or_else(|| panic!("missing startup check {name}: {verdict}"));
    assert_eq!(check["reason"], reason);
}

fn assert_reason_list_contains(verdict: &Value, key: &str, reason: &str) {
    let reasons = verdict[key].as_array().expect("reason list");
    assert!(
        reasons.iter().any(|value| value.as_str() == Some(reason)),
        "{key} should contain {reason}: {verdict}"
    );
}

fn assert_reason_list_not_contains(verdict: &Value, key: &str, reason: &str) {
    let reasons = verdict[key].as_array().expect("reason list");
    assert!(
        !reasons.iter().any(|value| value.as_str() == Some(reason)),
        "{key} should not contain {reason}: {verdict}"
    );
}

fn assert_startup_verdict_shape(verdict: &Value) {
    assert_status_string(verdict);
    assert!(verdict["blocking"].is_boolean(), "blocking bool: {verdict}");
    let checks = verdict["checks"].as_array().expect("startup checks array");
    assert!(!checks.is_empty(), "startup checks should not be empty");
    for check in checks {
        assert!(
            check["name"].is_string(),
            "startup check name should be present: {check}"
        );
        assert_status_string(check);
        if let Some(reason) = check.get("reason") {
            assert!(reason.is_string(), "reason must be a string: {check}");
        }
    }
    assert!(
        verdict["suggested_next_actions"].is_array(),
        "suggested_next_actions array: {verdict}"
    );
}

fn assert_workflow_verdict_shape(verdict: &Value) {
    assert_status_string(verdict);
    assert!(verdict["blocking"].is_boolean(), "blocking bool: {verdict}");
    for key in [
        "blocking_reasons",
        "warning_reasons",
        "suggested_next_actions",
    ] {
        assert!(verdict[key].is_array(), "{key} array: {verdict}");
    }
}

fn assert_status_string(value: &Value) {
    let status = value["status"].as_str().expect("status string");
    assert!(
        matches!(status, "pass" | "warn" | "fail"),
        "unexpected verdict status {status}: {value}"
    );
}

fn assert_verdict_actions_mirrored_at_top_level(output: &Value) {
    let top_level = output["suggested_next_actions"]
        .as_array()
        .expect("top-level suggested_next_actions array");
    for action in output["finish_verdict"]["suggested_next_actions"]
        .as_array()
        .expect("finish verdict suggested_next_actions array")
    {
        assert!(
            top_level.iter().any(|candidate| candidate == action),
            "top-level suggested_next_actions should include final verdict action {action}: {output}"
        );
    }
}

fn assert_compact_verdict_safe(value: &Value, context: &str) {
    let serialized = serde_json::to_string(value).unwrap();
    for forbidden in [
        "stdout", "stderr", "tail", "excerpt", "command", "token", "secret", "env",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "{context} leaked {forbidden}: {serialized}"
        );
    }
}

fn assert_review_evidence_tools_safe(review_evidence: &Value) {
    let tools = review_evidence["tools"]
        .as_array()
        .expect("review_evidence.tools array");
    assert!(
        !tools.is_empty(),
        "review_evidence.tools should not be empty"
    );
    assert!(tools.len() <= 20, "review_evidence.tools should be bounded");
    for tool in tools {
        let tool = tool.as_str().expect("review evidence tool name");
        assert!(
            matches!(
                tool,
                "read_file"
                    | "list_project_files"
                    | "search_project_text"
                    | "git_diff"
                    | "git_diff_summary"
                    | "git_diff_hunks"
                    | "show_changes"
                    | "git_status"
                    | "workspace_hygiene_check"
            ),
            "unexpected review evidence tool name {tool}"
        );
        for forbidden in [
            "stdout", "stderr", "tail", "excerpt", "command", "token", "secret", "env",
        ] {
            assert!(
                !tool.contains(forbidden),
                "review_evidence.tools leaked {forbidden}: {review_evidence}"
            );
        }
    }
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

fn record_coding_task_tool_event(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    success: bool,
    output: Value,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &arguments,
    );
    let error = (!success).then_some("tool failed");
    runtime
        .sessions
        .record_tool_call_finished(start, success, &output, error, None);
}

struct FinishSummaryFixture {
    _tmp: tempfile::TempDir,
    runtime: ToolRuntime,
    project: String,
    session_id: String,
    auth: AuthContext,
    client_id: &'static str,
}

async fn finish_summary_fixture(client_id: &'static str) -> FinishSummaryFixture {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "add readme");
    let runtime = test_runtime();
    let project = register_agent_project_at_path(&runtime, client_id, "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some(client_id.to_string()));
    FinishSummaryFixture {
        _tmp: tmp,
        runtime,
        project,
        session_id: session.session_id,
        auth: auth_context(None, true),
        client_id,
    }
}

async fn finish_coding_task_summary_only_with_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    session_id: String,
    auth: AuthContext,
) -> ToolResult {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: true,
                        include_diff: Some(false),
                        include_workspace: None,
                        include_hygiene: Some(true),
                        include_handoff: Some(false),
                        include_validation_summary: Some(true),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    for _ in 0..200 {
        if task.is_finished() {
            break;
        }
        if let Some(req) = next_patch_agent_request(runtime, client_id).await {
            complete_agent_request_by_running_locally(runtime, client_id, req).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    assert!(
        task.is_finished(),
        "finish_coding_task summary_only did not finish after read-only agent requests"
    );
    task.await.unwrap()
}
