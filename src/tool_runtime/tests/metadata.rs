//! Metadata tests for tool_runtime.

use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::projects::ProjectsState;
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    ShellAgentResultRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

fn shared_key_auth(hash: &str) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::SharedKey,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("shared-key".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("shared-key".to_string()),
        allowed_client_id: None,
        shared_key_hash: Some(hash.to_string()),
    }
}

fn open_auth() -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OpenAnonymous,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("open".to_string()),
        scopes: vec![
            crate::auth::SCOPE_RUNTIME_READ.to_string(),
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
            crate::auth::SCOPE_JOB_RUN.to_string(),
            crate::auth::SCOPE_AGENT_REGISTER.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("open".to_string()),
        allowed_client_id: None,
        shared_key_hash: None,
    }
}

fn bootstrap_auth() -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::Bootstrap,
        user_id: None,
        username: None,
        api_key_id: None,
        api_key_name: None,
        role: Some("admin".to_string()),
        scopes: vec![crate::auth::SCOPE_ADMIN.to_string()],
        is_bootstrap: true,
        token_kind: None,
        allowed_client_id: None,
        shared_key_hash: None,
    }
}

async fn register_agent_projects_for_auth(
    runtime: &ToolRuntime,
    client_id: &str,
    auth: &crate::auth::AuthContext,
    project_id: &str,
) {
    runtime
        .shell_clients
        .register_with_auth(
            ShellClientRegisterRequest {
                client_id: client_id.to_string(),
                agent_instance_id: format!("inst-{}", client_id),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: Some(ShellClientCapabilities {
                    shell: true,
                    file_read: true,
                    file_write: true,
                    git: true,
                    jobs: true,
                    async_jobs: true,
                    async_shell_jobs: true,
                }),
                projects: Some(vec![registered_project(
                    project_id,
                    &format!("/tmp/{}", project_id),
                )]),
                agent_protocol_version: Some("polling-v1".to_string()),
                policy: None,
            },
            Some(auth),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn list_projects_returns_agent_registered_projects_without_server_config() {
    let runtime = test_runtime();
    register_agent_with_projects(
        &runtime,
        "workstation-1",
        None,
        ShellClientCapabilities::default(),
        vec![registered_project("webcodex", "/root/git/webcodex")],
    )
    .await;

    let result = runtime.dispatch(ToolCall::ListProjects).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["count"], 1);
    let projects = result.output["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["id"], "agent:workstation-1:webcodex");
    assert_eq!(projects[0]["agent_project_id"], "webcodex");
    assert_eq!(projects[0]["executor"], "agent");
    assert_eq!(projects[0]["source"], "agent_registered");
    assert!(projects[0]["capabilities"].is_object());
    assert_eq!(projects[0]["capabilities"]["git_available"], false);
    assert_eq!(projects[0]["capabilities"]["recommended_for_smoke"], false);
}

#[tokio::test]
async fn list_projects_reports_smoke_selection_capabilities() {
    let runtime = test_runtime();
    let mut test_mcp = registered_project("test-mcp", "/tmp/test-mcp");
    test_mcp.name = Some("Test MCP".to_string());
    let mut smoke = registered_project("webcodex-smoke", "/tmp/webcodex-smoke");
    smoke.name = Some("WebCodex Smoke Workspace".to_string());
    smoke.git_branch = Some("main".to_string());
    smoke.git_head = Some("abc1234".to_string());
    smoke.git_dirty = Some(false);
    register_agent_with_projects(
        &runtime,
        "special",
        None,
        ShellClientCapabilities {
            file_read: true,
            file_write: true,
            git: true,
            ..Default::default()
        },
        vec![test_mcp, smoke],
    )
    .await;

    let result = runtime.dispatch(ToolCall::ListProjects).await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["count"], 2);
    let projects = result.output["projects"].as_array().unwrap();
    let test_mcp = projects
        .iter()
        .find(|project| project["id"] == "agent:special:test-mcp")
        .expect("test-mcp project");
    let smoke = projects
        .iter()
        .find(|project| project["id"] == "agent:special:webcodex-smoke")
        .expect("webcodex-smoke project");

    assert_eq!(test_mcp["capabilities"]["safe_smoke_project"], true);
    assert_eq!(test_mcp["capabilities"]["git_available"], false);
    assert_eq!(
        test_mcp["capabilities"]["supports_cleanup_verification"],
        true
    );
    assert_eq!(test_mcp["capabilities"]["recommended_for_smoke"], false);
    assert_eq!(smoke["capabilities"]["safe_smoke_project"], true);
    assert_eq!(smoke["capabilities"]["git_available"], true);
    assert_eq!(smoke["capabilities"]["supports_artifact_smoke"], true);
    assert_eq!(smoke["capabilities"]["recommended_for_smoke"], true);
    assert_eq!(
        result.output["recommended_for_smoke"],
        json!(["agent:special:webcodex-smoke"])
    );
}

#[tokio::test]
async fn list_projects_and_dispatch_are_filtered_by_lightweight_auth_group() {
    let runtime = test_runtime();
    let shared_a = shared_key_auth("hash-a");
    let shared_b = shared_key_auth("hash-b");
    let bridge_a = oauth_bridge_auth_context("hash-a", &[crate::auth::SCOPE_PROJECT_READ]);
    let bridge_b = oauth_bridge_auth_context("hash-b", &[crate::auth::SCOPE_PROJECT_READ]);
    let open = open_auth();
    let bootstrap = bootstrap_auth();

    register_agent_projects_for_auth(&runtime, "client-a", &shared_a, "proj-a").await;
    register_agent_projects_for_auth(&runtime, "client-b", &shared_b, "proj-b").await;
    register_agent_projects_for_auth(&runtime, "client-open", &open, "proj-open").await;

    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, Some(&shared_a))
        .await;
    assert!(result.success, "{:?}", result.error);
    let ids: Vec<&str> = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["agent:client-a:proj-a"]);

    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, Some(&bridge_a))
        .await;
    assert!(result.success, "{:?}", result.error);
    let ids: Vec<&str> = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["agent:client-a:proj-a"]);

    let bridge_read = tokio::spawn({
        let runtime = runtime.clone();
        let bridge_a = bridge_a.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: "agent:client-a:proj-a".to_string(),
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bridge_a),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "client-a")
        .await
        .expect("bridge read_file should enqueue for shared-key group A");
    complete_patch_agent_request_for_instance(
        &runtime,
        "client-a",
        "inst-client-a",
        &req.request_id,
        0,
        "bridge\n",
        "",
    )
    .await;
    let result = bridge_read.await.unwrap();
    assert!(result.success, "{:?}", result.error);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "agent:client-b:proj-b".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&bridge_a),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");

    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, Some(&bridge_b))
        .await;
    assert!(result.success, "{:?}", result.error);
    let ids: Vec<&str> = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["agent:client-b:proj-b"]);

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "agent:client-a:proj-a".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&bridge_b),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");

    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, Some(&open))
        .await;
    let ids: Vec<&str> = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["agent:client-open:proj-open"]);

    let open_read = tokio::spawn({
        let runtime = runtime.clone();
        let open = open.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project: "agent:client-open:proj-open".to_string(),
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&open),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "client-open")
        .await
        .expect("open read_file should enqueue for the open agent");
    complete_patch_agent_request_for_instance(
        &runtime,
        "client-open",
        "inst-client-open",
        &req.request_id,
        0,
        "open\n",
        "",
    )
    .await;
    let result = open_read.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_ne!(result.output["error_kind"], "current_session_unavailable");

    let open_git = tokio::spawn({
        let runtime = runtime.clone();
        let open = open.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::GitStatus {
                        project: "agent:client-open:proj-open".to_string(),
                        session_id: None,
                    },
                    Some(&open),
                )
                .await
        }
    });
    let req = next_agent_request_for_client(&runtime, "client-open")
        .await
        .expect("open git_status should enqueue for the open agent");
    complete_patch_agent_request_for_instance(
        &runtime,
        "client-open",
        "inst-client-open",
        &req.request_id,
        0,
        "",
        "",
    )
    .await;
    let result = open_git.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_ne!(result.output["error_kind"], "current_session_unavailable");

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "agent:client-a:proj-a".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&open),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");

    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, Some(&bootstrap))
        .await;
    let ids: Vec<&str> = result
        .output
        .get("projects")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|project| project["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        ids,
        vec![
            "agent:client-a:proj-a",
            "agent:client-b:proj-b",
            "agent:client-open:proj-open",
        ]
    );

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "agent:client-b:proj-b".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&shared_a),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: "agent:client-open:proj-open".to_string(),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&shared_a),
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unknown_project");
}

#[tokio::test]
async fn list_projects_shows_shell_profile_resolution() {
    use crate::shell_protocol::{AgentPolicySummary, ShellProfilesSummary};
    let runtime = test_runtime();
    let summary = ShellProfilesSummary {
        default_profile: Some("rust".to_string()),
        configured_count: 1,
        prepared_cache_count: 0,
        profiles: vec![profile_summary_entry("rust", false, 2)],
    };
    let policy = AgentPolicySummary {
        allow_raw_shell: true,
        allow_cwd_anywhere: true,
        allowed_roots: Vec::new(),
        max_timeout_secs: 3600,
        max_output_bytes: 262144,
        shell_profiles: Some(summary),
    };
    let mut configured = registered_project("rust-proj", "/root/git/rust");
    configured.shell_profile = Some("rust".to_string());
    let mut missing = registered_project("bad-proj", "/root/git/bad");
    missing.shell_profile = Some("nope".to_string());
    let mut fallback = registered_project("default-proj", "/root/git/default");
    // No explicit shell_profile: should resolve to default_profile "rust".
    let _ = fallback.shell_profile.take();
    register_agent_with_shell_profiles(
        &runtime,
        "ws-1",
        Some(policy),
        vec![configured, missing, fallback],
    )
    .await;

    let result = runtime.dispatch(ToolCall::ListProjects).await;
    assert!(result.success, "{:?}", result.error);
    let projects = result.output["projects"].as_array().unwrap();
    let by_id: std::collections::HashMap<&str, &Value> = projects
        .iter()
        .map(|p| (p["agent_project_id"].as_str().unwrap(), p))
        .collect();
    // Explicit profile that is configured.
    let cfg = by_id["rust-proj"];
    assert_eq!(cfg["shell_profile"], "rust");
    assert_eq!(cfg["resolved_shell_profile"], "rust");
    assert_eq!(cfg["shell_profile_status"], "configured");
    // Explicit profile that is missing.
    let miss = by_id["bad-proj"];
    assert_eq!(miss["shell_profile"], "nope");
    assert_eq!(miss["resolved_shell_profile"], "nope");
    assert_eq!(miss["shell_profile_status"], "missing");
    // No explicit profile: resolves to default_profile "rust".
    let def = by_id["default-proj"];
    assert_eq!(def["shell_profile"], Value::Null);
    assert_eq!(def["resolved_shell_profile"], "rust");
    assert_eq!(def["shell_profile_status"], "configured");
    // Agent liveness fields are surfaced for each project.
    assert_eq!(def["agent_status"], "online");
    assert_eq!(def["connected"], true);
}

#[tokio::test]
async fn list_projects_shell_profile_status_unknown_without_summary() {
    // An older agent that did not report a shell-profiles summary (policy
    // is None): a project with a shell_profile resolves but its configured
    // state is "unknown" because the configured set cannot be checked.
    let runtime = test_runtime();
    let mut project = registered_project("proj", "/root/git/proj");
    project.shell_profile = Some("rust".to_string());
    register_agent_with_shell_profiles(&runtime, "legacy", None, vec![project]).await;

    let result = runtime.dispatch(ToolCall::ListProjects).await;
    assert!(result.success);
    let projects = result.output["projects"].as_array().unwrap();
    assert_eq!(projects[0]["resolved_shell_profile"], "rust");
    assert_eq!(projects[0]["shell_profile_status"], "unknown");
}

#[tokio::test]
async fn runtime_status_shell_profiles_summary_is_sanitized() {
    use crate::shell_protocol::{
        AgentPolicySummary, ShellProfileSummaryEntry, ShellProfilesSummary,
    };
    let registry = Arc::new(ShellClientRegistry::default());
    let secret_env_value = "DO_NOT_LEAK_THIS_ENV_VALUE";
    let secret_script = "DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY";
    let summary = ShellProfilesSummary {
        default_profile: Some("rust".to_string()),
        configured_count: 1,
        prepared_cache_count: 0,
        profiles: vec![ShellProfileSummaryEntry {
            name: "rust".to_string(),
            has_init_script: true,
            env_keys_count: 3,
            program: "sh".to_string(),
            args_count: 1,
        }],
    };
    // The summary itself never carries env values or init_script bodies;
    // the secrets below are only carried in local test variables to prove
    // they never reach the status JSON.
    let _ = (secret_env_value, secret_script);
    registry
        .register(crate::shell_protocol::ShellClientRegisterRequest {
            client_id: "profile-agent".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: Some(AgentPolicySummary {
                allow_raw_shell: true,
                allow_cwd_anywhere: false,
                allowed_roots: Vec::new(),
                max_timeout_secs: 3600,
                max_output_bytes: 262144,
                shell_profiles: Some(summary),
            }),
        })
        .await
        .unwrap();
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let client = &result.output["agents"]["clients"][0];
    let sp = &client["shell_profiles"];
    assert_eq!(sp["default_profile"], "rust");
    assert_eq!(sp["configured_count"], 1);
    assert_eq!(sp["profiles"][0]["name"], "rust");
    assert_eq!(sp["profiles"][0]["has_init_script"], true);
    assert_eq!(sp["profiles"][0]["env_keys_count"], 3);
    assert_eq!(sp["profiles"][0]["program"], "sh");
    assert_eq!(sp["profiles"][0]["args_count"], 1);
    // Sanitization: never expose init_script bodies or env values.
    let rendered = sp.to_string();
    assert!(!rendered.contains("DO_NOT_LEAK_THIS_ENV_VALUE"));
    assert!(!rendered.contains("DO_NOT_LEAK_THIS_INIT_SCRIPT_BODY"));
    assert!(sp["profiles"][0].get("init_script").is_none());
    assert!(sp["profiles"][0].get("env").is_none());
}

#[tokio::test]
async fn unique_short_agent_project_id_is_resolved_by_runtime_surface() {
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::RunShell {
                        project: "agent-proj".to_string(),
                        command: "echo hi".to_string(),
                        session_id: None,
                        timeout_secs: Some(1),
                        cwd: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_agent_request_for_instance(&runtime, "oe", "inst")
        .await
        .expect("unique short id should resolve to the owning agent");
    assert_eq!(req.cwd.as_deref(), Some("/tmp/agent-proj"));
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "oe".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some("hi\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn agent_run_shell_without_shell_capability_is_rejected() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false;
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support shell"), "{}", err);
    assert!(err.contains("agent client oe"), "{}", err);
}

#[tokio::test]
async fn agent_read_file_without_file_read_capability_is_rejected() {
    let runtime = runtime_with_agent_project("oe");
    // Default caps: shell=true, file_read=false.
    register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: agent_test_project_id("oe"),
                path: "README.md".to_string(),
                session_id: None,
                start_line: None,
                limit: None,
                with_line_numbers: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support file_read"), "{}", err);
}

#[tokio::test]
async fn agent_run_job_without_async_capability_is_rejected() {
    let runtime = runtime_with_agent_project("oe");
    // Default caps: async_jobs=false, async_shell_jobs=false.
    register_agent(&runtime, "oe", None, ShellClientCapabilities::default()).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support async shell jobs"), "{}", err);
}

#[tokio::test]
async fn agent_git_status_without_shell_or_git_is_rejected() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = false; // git stays false by default
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitStatus {
                project: agent_test_project_id("oe"),
                session_id: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("does not support shell or git"), "{}", err);
}

#[tokio::test]
async fn agent_tool_unknown_client_returns_unknown_project_error() {
    // Project points at client "ghost" which never registered.
    let runtime = runtime_with_agent_project("ghost");
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: agent_test_project_id("ghost"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("unknown_project"), "{}", err);
    assert!(err.contains("ghost"), "{}", err);
    assert_eq!(result.output["error_kind"], "unknown_project");
    assert_eq!(result.output["project"], agent_test_project_id("ghost"));
}

#[tokio::test]
async fn agent_tool_rejects_non_owner_api_key() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "oe", Some("alice"), caps).await;
    let bob = auth_context(Some("bob"), false);
    // Use run_job (async) so the test does not hang if owner check leaked.
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&bob),
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("owned by alice"), "{}", err);
    assert!(err.contains("belongs to bob"), "{}", err);
}

#[tokio::test]
async fn agent_tool_rejects_missing_auth_context() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "oe", Some("alice"), caps).await;
    // dispatch_with_auth(None): no owner can be proven for an owned agent.
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunShell {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            None,
        )
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(
        err.contains("owned by alice") || err.contains("belongs to anonymous"),
        "{}",
        err
    );
}

#[tokio::test]
async fn agent_tool_allows_owner_api_key_for_run_job() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "oe", Some("alice"), caps).await;
    let alice = auth_context(Some("alice"), false);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&alice),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert!(result.output["job_id"].is_string());
}

#[tokio::test]
async fn agent_tool_allows_bootstrap_token_for_run_job() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "oe", Some("alice"), caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id("oe"),
                command: "echo hi".to_string(),
                session_id: None,
                timeout_secs: None,
                cwd: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
}

#[tokio::test]
async fn run_codex_is_disabled_before_project_resolution() {
    // Codex delegation remains implemented underneath, but model-facing runtime
    // dispatch must reject it before project resolution or job creation.
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RunCodex {
                project: "demo".to_string(),
                prompt: "echo hi".to_string(),
                session_id: None,
                approval_mode: None,
                timeout_secs: Some(10),
                cwd: None,
                extra_args: None,
            },
            None,
        )
        .await;
    assert!(!result.success);
    assert_eq!(result.output["code"], "run_codex_disabled");
    assert!(result.error.unwrap().contains("currently disabled"));
}

#[test]
fn runtime_status_is_in_tool_specs() {
    let runtime = test_runtime();
    let names: Vec<String> = runtime
        .tool_specs()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert!(
        names.iter().any(|n| n == "runtime_status"),
        "runtime_status must be in tool_specs: {:?}",
        names
    );
}

#[test]
fn session_handoff_validation_exposure_keeps_read_only_metadata() {
    let metadata = crate::tool_runtime::metadata::lookup_tool_metadata("session_handoff_summary")
        .expect("session_handoff_summary metadata");
    assert!(metadata.read_only);
    assert!(!metadata.destructive);
    assert!(!metadata.shell_like);
    assert_eq!(metadata.oauth_scope, Some("runtime:read"));
}

#[tokio::test]
async fn tool_manifest_hides_run_codex_from_model_facing_surface() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    let tools = result.output["tools"].as_array().unwrap();
    assert!(
        !tools.iter().any(|tool| tool["name"] == "run_codex"),
        "tool_manifest tools must not include run_codex: {:?}",
        tools
    );
    let serialized = result.output.to_string();
    assert!(
        !serialized.contains("run_codex"),
        "tool_manifest output must not advertise run_codex: {}",
        serialized
    );
}

#[tokio::test]
async fn tool_manifest_reports_accepted_flattened_args_without_schemas() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["schema_version"], 1);
    assert!(result.output["count"].as_u64().unwrap() > 0);

    let tools = result.output["tools"].as_array().unwrap();
    for tool in tools {
        assert!(
            tool.get("inputSchema").is_none() && tool.get("outputSchema").is_none(),
            "tool_manifest must stay compact: {tool:?}"
        );
        assert!(
            tool["accepted_flattened_args"].is_array(),
            "tool_manifest entry must expose accepted_flattened_args: {tool:?}"
        );
        assert_eq!(tool["deprecated_or_unsupported_args"], json!([]));
    }

    let accepted = |name: &str| -> Vec<String> {
        tools
            .iter()
            .find(|tool| tool["name"] == name)
            .unwrap_or_else(|| panic!("missing manifest tool {name}"))["accepted_flattened_args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect()
    };

    for field in [
        "category",
        "include_recommended_flows",
        "include_risk_summary",
        "recording_session_id",
    ] {
        assert!(accepted("tool_manifest").contains(&field.to_string()));
    }
    for field in ["summary_only", "category", "features", "limit"] {
        assert!(accepted("list_tools").contains(&field.to_string()));
    }
    for field in [
        "project",
        "title",
        "bind_current",
        "include_runtime_status",
        "include_git",
        "include_recent_commits",
        "include_rules",
        "include_tool_manifest",
        "tool_manifest_categories",
        "tool_manifest_limit",
        "session_id",
        "recording_session_id",
    ] {
        assert!(accepted("start_coding_task").contains(&field.to_string()));
    }
    for field in [
        "session_id",
        "include_validation",
        "include_workspace",
        "include_checkpoints",
        "limit",
    ] {
        assert!(accepted("session_handoff_summary").contains(&field.to_string()));
    }
    for field in [
        "project",
        "path",
        "allow_missing",
        "session_id",
        "allow_cross_project_session",
    ] {
        assert!(accepted("read_project_artifact_metadata").contains(&field.to_string()));
    }
    for (tool, fields) in [
        (
            "artifact_upload_begin",
            vec![
                "project",
                "path",
                "expected_bytes",
                "expected_sha256",
                "mime_type",
                "overwrite",
                "session_id",
            ],
        ),
        (
            "artifact_upload_chunk",
            vec![
                "project",
                "path",
                "upload_id",
                "offset",
                "content_base64",
                "session_id",
            ],
        ),
        (
            "artifact_upload_finish",
            vec!["project", "path", "upload_id", "session_id"],
        ),
        (
            "artifact_upload_abort",
            vec!["project", "path", "upload_id", "session_id"],
        ),
    ] {
        let accepted = accepted(tool);
        for field in fields {
            assert!(
                accepted.contains(&field.to_string()),
                "{tool} missing accepted flattened arg {field}: {accepted:?}"
            );
        }
    }
}

#[tokio::test]
async fn bounded_list_tools_hides_schemas_and_finds_artifact_upload_tools() {
    let runtime = test_runtime();
    let full = runtime
        .dispatch(ToolCall::ListTools {
            category: None,
            features: None,
            summary_only: false,
            limit: None,
        })
        .await;
    assert!(full.success, "{:?}", full.error);

    let bounded = runtime
        .dispatch(ToolCall::ListTools {
            category: Some("artifact".to_string()),
            features: Some("artifact_upload".to_string()),
            summary_only: true,
            limit: Some(10),
        })
        .await;
    assert!(bounded.success, "{:?}", bounded.error);
    assert_eq!(bounded.output["total_count"], full.output["total_count"]);
    assert!(bounded.output["count"].as_u64().unwrap() > 0);
    assert_eq!(bounded.output["truncated"], false);
    let tools = bounded.output["tools"].as_array().unwrap();
    let names = bounded.output["names"].as_array().unwrap();
    for tool in [
        "artifact_upload_begin",
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        assert!(names.iter().any(|name| name == tool), "missing {tool}");
    }
    assert!(
        !names.iter().any(|name| name == "run_codex"),
        "bounded list_tools must not expose run_codex: {:?}",
        names
    );
    for tool in tools {
        assert!(tool["category"].as_str() == Some("artifact"), "{tool:?}");
        assert!(tool.get("inputSchema").is_none(), "{tool:?}");
        assert!(tool.get("outputSchema").is_none(), "{tool:?}");
    }

    let full_json = serde_json::to_string(&full.output).unwrap();
    let bounded_json = serde_json::to_string(&bounded.output).unwrap();
    assert!(
        bounded_json.len() < full_json.len() / 2,
        "bounded discovery should be substantially smaller than full list"
    );
}

#[tokio::test]
async fn bounded_list_tools_limit_reports_truncation() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ListTools {
            category: None,
            features: Some("artifact_upload".to_string()),
            summary_only: true,
            limit: Some(2),
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["count"], 2);
    assert_eq!(result.output["filtered_count"], 4);
    assert_eq!(result.output["truncated"], true);
}

#[tokio::test]
async fn tool_manifest_recommends_default_remote_coding_loop() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ToolManifest {
            category: None,
            include_recommended_flows: true,
            include_risk_summary: true,
        })
        .await;
    assert!(result.success, "{:?}", result.error);

    let flows = result.output["recommended_flows"]
        .as_array()
        .expect("tool_manifest should include recommended_flows");
    for name in [
        "discovery",
        "inspect",
        "edit",
        "validate",
        "review",
        "handoff",
    ] {
        assert!(
            flows.iter().any(|flow| flow["name"] == name),
            "recommended_flows should include {name}: {:?}",
            flows
        );
    }

    let serialized = result.output["recommended_flows"]
        .to_string()
        .to_lowercase();
    for tool in [
        "read_file",
        "search_project_text",
        "show_changes",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        "apply_text_edits",
        "apply_patch_checked",
        "cargo_check",
        "cargo_test",
        "validate_patch",
        "git_diff_hunks",
        "workspace_hygiene_check",
        "session_summary",
        "session_handoff_summary",
    ] {
        assert!(
            serialized.contains(tool),
            "recommended_flows should mention {tool}: {serialized}"
        );
    }
    assert!(
        serialized.contains("run_shell")
            && serialized.contains("escape hatch")
            && serialized.contains("not the primary validation path"),
        "run_shell should be a bounded escape hatch in recommended_flows: {serialized}"
    );
}

#[tokio::test]
async fn runtime_status_with_no_projects_returns_configured_false() {
    let runtime = test_runtime();
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success, "{:?}", result.error);
    let out = &result.output;
    assert_eq!(out["service"], "webcodex");
    assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
    assert!(out["build"].is_object());
    assert!(out["build"].get("git_commit").is_some());
    assert!(out["build"].get("git_dirty").is_some());
    assert!(out["build"].get("built_at").is_some());
    assert!(out["server_time"].is_i64());
    assert!(out["pid"].is_i64());
    assert_eq!(out["permissions"]["policy"], "dev_auto_approve");
    assert_eq!(out["permissions"]["auto_approve"], true);
    assert_eq!(out["permissions"]["human_approval_required"], false);
    assert_eq!(
        out["permissions"]["release_recommended_policy"],
        "require_approval"
    );
    // No projects.toml -> configured false, load_error present.
    assert_eq!(out["projects"]["configured"], false);
    assert_eq!(out["projects"]["count"], 0);
    assert!(out["projects"]["load_error"].is_string());
    assert_eq!(out["projects"]["server_static"]["configured"], false);
    assert_eq!(out["projects"]["server_static"]["count"], 0);
    assert_eq!(
        out["projects"]["server_static"]["warning"],
        "projects.toml not configured"
    );
    assert_eq!(out["projects"]["agent_registered"]["count"], 0);
    assert_eq!(out["projects"]["agent_registered"]["online_count"], 0);
    assert_eq!(out["projects"]["effective"]["count"], 0);
    assert_eq!(out["projects"]["effective"]["status"], "no_projects");
}

#[tokio::test]
async fn runtime_status_uses_agent_projects_as_effective_when_server_config_missing() {
    let runtime = test_runtime();
    let mut smoke = registered_project("webcodex-smoke", "/tmp/webcodex-smoke");
    smoke.git_branch = Some("main".to_string());
    smoke.git_head = Some("abc1234".to_string());
    smoke.git_dirty = Some(false);
    register_agent_with_projects(
        &runtime,
        "special",
        None,
        ShellClientCapabilities {
            file_read: true,
            file_write: true,
            git: true,
            ..Default::default()
        },
        vec![smoke],
    )
    .await;

    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success, "{:?}", result.error);
    let projects = &result.output["projects"];
    assert_eq!(projects["server_static"]["configured"], false);
    assert_eq!(projects["server_static"]["count"], 0);
    assert_eq!(projects["agent_registered"]["count"], 1);
    assert_eq!(projects["agent_registered"]["online_count"], 1);
    assert_eq!(projects["effective"]["count"], 1);
    assert_eq!(projects["effective"]["status"], "ok");
    assert_eq!(projects["count"], 1);
}

#[tokio::test]
async fn runtime_status_includes_build_metadata() {
    let runtime = test_runtime();
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success, "{:?}", result.error);
    let build = &result.output["build"];
    assert!(build.is_object());
    assert!(build.get("git_commit").is_some());
    assert!(build.get("git_dirty").is_some());
    assert!(build.get("built_at").is_some());
}

#[tokio::test]
async fn runtime_status_with_loaded_project_returns_configured_true() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_project(tmp.path(), "demo");
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success, "{:?}", result.error);
    let out = &result.output;
    assert_eq!(out["projects"]["configured"], true);
    assert_eq!(out["projects"]["count"], 1);
    assert!(out["projects"]["load_error"].is_null());
    assert_eq!(out["projects"]["server_static"]["configured"], true);
    assert_eq!(out["projects"]["server_static"]["count"], 1);
    assert_eq!(out["projects"]["effective"]["count"], 1);
    assert_eq!(out["projects"]["effective"]["status"], "ok");
}

#[tokio::test]
async fn runtime_status_does_not_expose_tokens_or_secrets() {
    let info = RuntimeInfo {
        auth_enabled: true,
        configured_public_url: Some("https://example.com".to_string()),
        quic: Some(Arc::new(std::sync::Mutex::new(
            crate::config::QuicServerConfig::default().runtime_status(),
        ))),
    };
    let runtime = runtime_with_info(info);
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let serialized = serde_json::to_string(&result.output).unwrap();
    // The summary must never contain secret-like field names.
    for forbidden in [
        "token",
        "WEBCODEX_TOKEN",
        "api_key",
        "apikey",
        "secret",
        "password",
        "authorization",
        "bearer",
    ] {
        assert!(
            !serialized
                .to_lowercase()
                .contains(&forbidden.to_lowercase()),
            "runtime_status output must not contain '{}': {}",
            forbidden,
            serialized
        );
    }
    // auth_enabled is a bool, not the token value.
    assert_eq!(result.output["auth_enabled"], true);
}

#[tokio::test]
async fn runtime_status_quic_disabled_is_non_sensitive() {
    let runtime = runtime_with_info(RuntimeInfo::default());
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    assert_eq!(result.output["quic"]["enabled"], false);
    assert_eq!(result.output["quic"]["listen"], "0.0.0.0:8443");
    assert_eq!(result.output["quic"]["alpn"], "webcodex-agent/1");
    assert_eq!(result.output["quic"]["listener_started"], false);
    assert!(result.output["quic"]["last_error"].is_null());
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("WEBCODEX_QUIC_CERT"));
    assert!(!serialized.contains("WEBCODEX_QUIC_KEY"));
    assert!(!serialized.to_ascii_lowercase().contains("token"));
}

#[tokio::test]
async fn runtime_status_quic_enabled_error_is_sanitized() {
    let quic_cfg = crate::config::QuicServerConfig {
        enabled: true,
        listen: "0.0.0.0:8443".to_string(),
        cert: PathBuf::from("/secret/certs/fullchain.pem"),
        key: PathBuf::from("/secret/certs/privkey.pem"),
        alpn: "webcodex-agent/1".to_string(),
    };
    let status = Arc::new(std::sync::Mutex::new(quic_cfg.runtime_status()));
    status
        .lock()
        .unwrap()
        .mark_error("WEBCODEX_QUIC_KEY path does not exist: /secret/certs/privkey.pem");
    let runtime = runtime_with_info(RuntimeInfo {
        auth_enabled: false,
        configured_public_url: None,
        quic: Some(status),
    });
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    assert_eq!(result.output["quic"]["enabled"], true);
    assert_eq!(result.output["quic"]["listener_started"], false);
    assert_eq!(
        result.output["quic"]["last_error"],
        "WEBCODEX_QUIC_KEY path does not exist"
    );
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("/secret/certs"));
    assert!(!serialized.contains("privkey.pem"));
}

#[tokio::test]
async fn runtime_status_quic_started_reports_listen_and_alpn() {
    let quic_cfg = crate::config::QuicServerConfig {
        enabled: true,
        listen: "127.0.0.1:9443".to_string(),
        cert: PathBuf::from("/hidden/cert.pem"),
        key: PathBuf::from("/hidden/key.pem"),
        alpn: "webcodex-agent/1".to_string(),
    };
    let status = Arc::new(std::sync::Mutex::new(quic_cfg.runtime_status()));
    status.lock().unwrap().mark_started();
    let runtime = runtime_with_info(RuntimeInfo {
        auth_enabled: false,
        configured_public_url: None,
        quic: Some(status),
    });
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    assert_eq!(result.output["quic"]["enabled"], true);
    assert_eq!(result.output["quic"]["listen"], "127.0.0.1:9443");
    assert_eq!(result.output["quic"]["alpn"], "webcodex-agent/1");
    assert_eq!(result.output["quic"]["listener_started"], true);
    assert!(result.output["quic"]["last_error"].is_null());
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("/hidden"));
}

#[tokio::test]
async fn runtime_status_auth_enabled_reflects_runtime_info() {
    let runtime = runtime_with_info(RuntimeInfo {
        auth_enabled: false,
        configured_public_url: None,
        quic: Some(Arc::new(std::sync::Mutex::new(
            crate::config::QuicServerConfig::default().runtime_status(),
        ))),
    });
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    assert_eq!(result.output["auth_enabled"], false);
    assert!(result.output["configured_public_url"].is_null());

    let runtime = runtime_with_info(RuntimeInfo {
        auth_enabled: true,
        configured_public_url: Some("https://webcodex.example.com".to_string()),
        quic: Some(Arc::new(std::sync::Mutex::new(
            crate::config::QuicServerConfig::default().runtime_status(),
        ))),
    });
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    assert_eq!(result.output["auth_enabled"], true);
    assert_eq!(
        result.output["configured_public_url"],
        "https://webcodex.example.com"
    );
}

#[test]
fn runtime_info_from_env_reads_webcodex_public_url() {
    let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
    std::env::set_var("WEBCODEX_TOKEN", "token");
    std::env::set_var("WEBCODEX_PUBLIC_URL", "https://new.example.com");

    let info = RuntimeInfo::from_env();
    assert!(info.auth_enabled);
    assert_eq!(
        info.configured_public_url.as_deref(),
        Some("https://new.example.com")
    );

    std::env::remove_var("WEBCODEX_TOKEN");
    std::env::remove_var("WEBCODEX_PUBLIC_URL");
}

#[tokio::test]
async fn runtime_status_agent_summary_includes_protocol_version() {
    use crate::shell_protocol::ShellClientRegisterRequest;
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            client_id: "agent-1".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: Some("Workstation".to_string()),
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: Some(vec![]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let agents = &result.output["agents"];
    assert_eq!(agents["count"], 1);
    assert_eq!(agents["online_count"], 1);
    assert_eq!(agents["offline_count"], 0);
    assert_eq!(agents["stale_count"], 0);
    let clients = agents["clients"].as_array().unwrap();
    assert_eq!(clients.len(), 1);
    assert_eq!(clients[0]["client_id"], "agent-1");
    assert_eq!(clients[0]["agent_protocol_version"], "polling-v1");
    assert_eq!(clients[0]["transport"], "polling");
    assert_eq!(clients[0]["connected"], true);
    assert!(clients[0]["capabilities"].is_object());
    assert_eq!(clients[0]["projects_count"], 0);
    // last_seen must be present as an integer unix timestamp (seconds).
    assert!(
        clients[0]["last_seen"].is_i64(),
        "last_seen must be an integer timestamp: {:?}",
        clients[0]["last_seen"]
    );
}

#[tokio::test]
async fn runtime_status_includes_sanitized_policy_summary() {
    use crate::shell_protocol::{AgentPolicySummary, ShellClientRegisterRequest};
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            client_id: "policy-agent".to_string(),
            agent_instance_id: "inst-p".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: Some(AgentPolicySummary {
                allow_raw_shell: true,
                allow_cwd_anywhere: false,
                allowed_roots: vec![std::path::PathBuf::from("/root")],
                max_timeout_secs: 3600,
                max_output_bytes: 262144,
                shell_profiles: None,
            }),
        })
        .await
        .unwrap();
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let clients = result.output["agents"]["clients"].as_array().unwrap();
    let policy = &clients[0]["policy"];
    assert_eq!(policy["allow_raw_shell"], true);
    assert_eq!(policy["allow_cwd_anywhere"], false);
    assert_eq!(policy["allowed_roots"], json!(["/root"]));
    assert_eq!(policy["max_timeout_secs"], 3600);
    assert_eq!(policy["max_output_bytes"], 262144);
    // Sanitization: never expose token/env/init_script.
    assert!(policy.get("token").is_none());
    assert!(policy.get("env").is_none());
    assert!(policy.get("init_script").is_none());
}

#[tokio::test]
async fn runtime_status_policy_summary_is_null_for_older_agents() {
    use crate::shell_protocol::ShellClientRegisterRequest;
    let registry = Arc::new(ShellClientRegistry::default());
    // Older agent: no policy field (None).
    registry
        .register(ShellClientRegisterRequest {
            client_id: "legacy-agent".to_string(),
            agent_instance_id: "inst-l".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let clients = result.output["agents"]["clients"].as_array().unwrap();
    // Older/minimal payload -> policy is null, not a fatal error.
    assert!(clients[0]["policy"].is_null());
}

#[tokio::test]
async fn list_agents_includes_sanitized_policy_summary() {
    use crate::shell_protocol::{AgentPolicySummary, ShellClientRegisterRequest};
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            client_id: "list-policy-agent".to_string(),
            agent_instance_id: "inst-lp".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: Some(AgentPolicySummary {
                allow_raw_shell: false,
                allow_cwd_anywhere: true,
                allowed_roots: vec![],
                max_timeout_secs: 120,
                max_output_bytes: 4096,
                shell_profiles: None,
            }),
        })
        .await
        .unwrap();
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::ListAgents).await;
    assert!(result.success);
    let agents = result.output["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    let policy = &agents[0]["policy"];
    assert_eq!(policy["allow_raw_shell"], false);
    assert_eq!(policy["allow_cwd_anywhere"], true);
    assert_eq!(policy["max_timeout_secs"], 120);
    assert_eq!(policy["max_output_bytes"], 4096);
    // No secret fields leak through listAgents either.
    assert!(policy.get("token").is_none());
    assert!(policy.get("env").is_none());
    assert!(policy.get("init_script").is_none());
}

#[tokio::test]
async fn runtime_status_marks_stale_websocket_agent_with_last_seen() {
    use crate::shell_client::TRANSPORT_WEBSOCKET;
    use crate::shell_protocol::ShellClientRegisterRequest;
    let registry = Arc::new(ShellClientRegistry::default());
    registry
        .register(ShellClientRegisterRequest {
            client_id: "ws-stale".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: Some("Stale WS".to_string()),
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: Some(vec![]),
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_transport("ws-stale", TRANSPORT_WEBSOCKET)
        .await
        .unwrap();
    // Force the agent past the 60s online window so it reads as stale.
    let stale_ts = chrono::Utc::now().timestamp() - 120;
    registry.set_last_seen_for_test("ws-stale", stale_ts).await;

    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry,
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let agents = &result.output["agents"];
    assert_eq!(agents["count"], 1);
    assert_eq!(agents["online_count"], 0);
    assert_eq!(agents["stale_count"], 1);
    assert_eq!(agents["offline_count"], 1);
    let entry = &agents["clients"][0];
    assert_eq!(entry["client_id"], "ws-stale");
    assert_eq!(entry["transport"], "websocket");
    assert_eq!(entry["status"], "stale");
    assert_eq!(entry["connected"], false);
    assert_eq!(entry["last_seen"], stale_ts);
}

#[tokio::test]
async fn runtime_status_reflects_websocket_transport_label() {
    let registry = Arc::new(ShellClientRegistry::default());
    let runtime = ToolRuntime::new(
        Arc::new(ProjectsState::failed(
            "none".to_string(),
            "test".to_string(),
        )),
        registry.clone(),
        Arc::new(CodexConfig::default()),
        Arc::new(RuntimeInfo::default()),
    );
    registry
        .register(ShellClientRegisterRequest {
            client_id: "ws-agent".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: Some("websocket-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    // Flip the transport label the same way the WebSocket handler does.
    registry
        .set_transport("ws-agent", crate::shell_client::TRANSPORT_WEBSOCKET)
        .await
        .unwrap();

    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let clients = &result.output["agents"]["clients"];
    let entry = clients
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["client_id"] == "ws-agent")
        .expect("ws-agent present");
    assert_eq!(entry["transport"], "websocket");
    assert_eq!(entry["agent_protocol_version"], "websocket-v1");
}

#[tokio::test]
async fn runtime_status_counts_local_jobs() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_project(root, "demo");
    // Write a fake local job in "running" state and register it in the
    // in-memory map so runtime_status counts it.
    let job_dir = root.join(".codex/jobs/job-active");
    fs::create_dir_all(&job_dir).unwrap();
    fs::write(job_dir.join("status"), "running").unwrap();
    let meta_json = json!({
        "job_id": "job-active",
        "project": "demo",
        "command": "sleep 10",
        "status": "running",
        "created_at": 1,
        "started_at": 1,
        "max_runtime_secs": 600,
        "executor": "local",
        "path": root.to_string_lossy(),
        "kind": "shell",
    });
    fs::write(
        job_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta_json).unwrap(),
    )
    .unwrap();
    runtime.local_jobs.lock().await.insert(
        "job-active".to_string(),
        LocalJobRecord {
            project: "demo".to_string(),
            dir: job_dir,
        },
    );
    // Also write a completed job to verify it's not counted as active.
    let done_dir = root.join(".codex/jobs/job-done");
    fs::create_dir_all(&done_dir).unwrap();
    fs::write(done_dir.join("status"), "completed").unwrap();
    fs::write(
        done_dir.join("metadata.json"),
        serde_json::to_string(&json!({
            "job_id": "job-done",
            "project": "demo",
            "command": "true",
            "status": "completed",
            "created_at": 1,
            "started_at": 1,
            "executor": "local",
            "path": root.to_string_lossy(),
            "kind": "shell",
        }))
        .unwrap(),
    )
    .unwrap();
    runtime.local_jobs.lock().await.insert(
        "job-done".to_string(),
        LocalJobRecord {
            project: "demo".to_string(),
            dir: done_dir,
        },
    );

    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success, "{:?}", result.error);
    let jobs = &result.output["jobs"];
    assert_eq!(jobs["local_known_count"], 2);
    // Only the running job is active.
    assert_eq!(jobs["active_count"], 1);
    assert_eq!(jobs["agent_known_count"], 0);
}

#[tokio::test]
async fn runtime_status_tools_summary_lists_names() {
    let runtime = test_runtime();
    let result = runtime.dispatch(ToolCall::RuntimeStatus).await;
    assert!(result.success);
    let tools = &result.output["tools"];
    let names = tools["names"].as_array().unwrap();
    assert!(names.len() > 0);
    assert!(
        names.iter().any(|n| n == "runtime_status"),
        "tools.names must include runtime_status: {:?}",
        names
    );
    assert!(
        !names.iter().any(|n| n == "run_codex"),
        "runtime_status tools.names must not include hidden run_codex: {:?}",
        names
    );
    assert_eq!(tools["count"], names.len() as i64);
}
