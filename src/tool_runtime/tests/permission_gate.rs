//! Phase 2 permission pre-exec gate: single evaluation, mode matrix, mutation side effects.

use super::super::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::permissions::{EffectivePermissionConfig, PermissionEvaluator, PermissionMode};
use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn write_tool_call(
    project: String,
    path: &str,
    content: &str,
    session_id: Option<String>,
) -> ToolCall {
    ToolCall::WriteProjectFile {
        project,
        path: path.to_string(),
        content: content.to_string(),
        session_id,
        overwrite: Some(true),
        expected_sha256: None,
        expected_content_prefix: None,
    }
}

fn runtime_with_mode(client_id: &str, mode: PermissionMode) -> ToolRuntime {
    runtime_with_agent_project(client_id)
        .with_permission_evaluator(PermissionEvaluator::with_mode(mode))
}

fn runtime_with_config(client_id: &str, config: EffectivePermissionConfig) -> ToolRuntime {
    runtime_with_agent_project(client_id)
        .with_permission_evaluator(PermissionEvaluator::with_config(config))
}

async fn register_write_agent(runtime: &ToolRuntime, client_id: &str) {
    register_agent(
        runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_write: true,
            shell: true,
            git: true,
            ..Default::default()
        },
    )
    .await;
}

/// Complete a successful agent write so the mutation path can finish.
async fn complete_write_ok(runtime: &ToolRuntime, client_id: &str, path: &str) {
    let req = next_patch_agent_request(runtime, client_id)
        .await
        .expect("mutation tool should enqueue agent request under allowing modes");
    assert_eq!(req.kind, "file_write_project_file");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("file-op payload")).unwrap();
    assert_eq!(payload["path"], path);
    complete_patch_agent_request(
        runtime,
        client_id,
        &req.request_id,
        0,
        &format!(r#"{{"path":"{path}","bytes_written":4,"sha256":"abc","changed":true}}"#),
        "",
    )
    .await;
}

#[tokio::test]
async fn dev_auto_approve_evaluates_once_executes_and_attaches_same_decision() {
    let counter = Arc::new(AtomicUsize::new(0));
    let client_id = "perm-auto-once";
    let runtime = runtime_with_agent_project(client_id).with_permission_evaluator(
        PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
            .with_eval_counter(counter.clone()),
    );
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    write_tool_call(project, "src/once.txt", "hi\n", Some(session_id)),
                    Some(&bootstrap),
                )
                .await
        }
    });
    complete_write_ok(&runtime, client_id, "src/once.txt").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(counter.load(Ordering::SeqCst), 1, "evaluator must run once");
    assert_eq!(result.output["permission"]["status"], "auto_approved");
    assert_eq!(result.output["permission"]["policy"], "dev_auto_approve");
    assert_eq!(result.output["permission"]["reason"], "dev_auto_approve");
    let request_id = result.output["permission"]["request_id"]
        .as_str()
        .expect("request_id");
    assert!(request_id.starts_with("wc_perm_"), "{request_id}");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "write_project_file");
    let ledger_perm = event.permission.as_ref().expect("ledger permission");
    assert_eq!(ledger_perm.request_id, request_id);
    assert_eq!(ledger_perm.status, "auto_approved");
}

#[tokio::test]
async fn audit_only_allows_mutation_and_attaches_audit_decision() {
    let client_id = "perm-audit";
    let runtime = runtime_with_mode(client_id, PermissionMode::AuditOnly);
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let bootstrap = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    write_tool_call(project, "src/audit.txt", "ok\n", None),
                    Some(&bootstrap),
                )
                .await
        }
    });
    complete_write_ok(&runtime, client_id, "src/audit.txt").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["permission"]["status"], "audit_only_allowed");
    assert_eq!(result.output["permission"]["policy"], "audit_only");
    assert_eq!(result.output["permission"]["risk"], "write");
}

#[tokio::test]
async fn require_approval_blocks_mutation_before_agent_enqueue() {
    let client_id = "perm-require";
    let runtime = runtime_with_mode(client_id, PermissionMode::RequireApproval);
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            write_tool_call(
                project,
                "src/blocked.txt",
                "must-not-write\n",
                Some(session.session_id.clone()),
            ),
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "permission_denied");
    assert_eq!(result.output["failure_kind"], "permission_denied");
    assert_eq!(result.output["permission"]["status"], "denied");
    assert_eq!(
        result.output["permission"]["reason"],
        "require_approval_not_implemented"
    );
    assert_ne!(result.output["permission"]["status"], "auto_approved");
    let err = result.error.as_deref().unwrap();
    assert!(err.contains("require_approval"), "{err}");
    assert!(err.contains("not implemented"), "{err}");

    assert!(
        next_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "require_approval must not enqueue mutation"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "write_project_file");
    assert_eq!(event.status.as_deref(), Some("failed"));
    let perm = event.permission.as_ref().expect("permission on ledger");
    assert_eq!(perm.status, "denied");
    assert_eq!(perm.reason, "require_approval_not_implemented");
}

#[tokio::test]
async fn invalid_mode_blocks_mutation_and_does_not_auto_approve() {
    let client_id = "perm-invalid";
    let runtime = runtime_with_config(
        client_id,
        EffectivePermissionConfig::from_raw(Some("totally_bogus_mode")),
    );
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let bootstrap = auth_context(None, true);

    let result = runtime
        .dispatch_with_auth(
            write_tool_call(project, "src/invalid-mode.txt", "nope\n", None),
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "permission_denied");
    assert_eq!(result.output["permission"]["status"], "denied");
    assert_ne!(result.output["permission"]["status"], "auto_approved");
    assert!(
        result.output["permission"]["reason"]
            .as_str()
            .unwrap()
            .contains("invalid_permission_mode"),
        "{:?}",
        result.output["permission"]["reason"]
    );
    let err = result.error.as_deref().unwrap();
    assert!(
        err.contains("WEBCODEX_PERMISSION_MODE") || err.contains("invalid"),
        "{err}"
    );
    assert!(
        next_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "invalid mode must not enqueue mutation"
    );
}

#[tokio::test]
async fn hard_policy_deny_still_suppresses_permission_attach() {
    let client_id = "perm-hard-deny";
    let runtime = runtime_with_mode(client_id, PermissionMode::DevAutoApprove);
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    // Sensitive / policy-rejected path is hard safety inside the tool; must
    // not attach auto_approved over a hard deny.
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ArtifactUploadBegin {
                project,
                path: "artifacts/smoke/raw.bin".to_string(),
                session_id: Some(session.session_id.clone()),
                expected_bytes: Some(1),
                expected_sha256: None,
                mime_type: Some("application/octet-stream".to_string()),
                overwrite: Some(false),
            },
            Some(&bootstrap),
        )
        .await;

    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "policy_rejected");
    assert_eq!(result.output["error_kind"], "policy_rejected");
    assert!(
        result.output.get("permission").is_none(),
        "hard deny must not carry permission auto-approve: {:?}",
        result.output.get("permission")
    );
    assert!(
        next_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "policy rejection must happen before enqueue"
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "artifact_upload_begin");
    assert!(event.permission.is_none());
}

#[tokio::test]
async fn read_only_tool_skips_permission_decision() {
    let client_id = "perm-readonly";
    let runtime = runtime_with_mode(client_id, PermissionMode::RequireApproval);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let bootstrap = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFile {
                        project,
                        path: "README.md".to_string(),
                        session_id: None,
                        start_line: None,
                        limit: Some(1),
                        with_line_numbers: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, client_id)
        .await
        .expect("read_file should still execute under require_approval");
    complete_patch_agent_request(&runtime, client_id, &req.request_id, 0, "hello\n", "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("permission").is_none(),
        "read-only tools must not invent permission records: {:?}",
        result.output.get("permission")
    );
}

#[tokio::test]
async fn kernel_path_does_not_double_evaluate_or_duplicate_request_id() {
    let counter = Arc::new(AtomicUsize::new(0));
    let client_id = "perm-kernel-once";
    let runtime = runtime_with_agent_project(client_id).with_permission_evaluator(
        PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
            .with_eval_counter(counter.clone()),
    );
    register_write_agent(&runtime, client_id).await;
    let project = agent_test_project_id(client_id);
    let recording = runtime.sessions.start_session(Some(project.clone()), None);
    let bootstrap = auth_context(None, true);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let recording_id = recording.session_id.clone();
        let bootstrap = bootstrap.clone();
        async move {
            runtime
                .call_tool_with_context(
                    ToolCallRequest {
                        tool_name: "write_project_file".to_string(),
                        arguments: json!({
                            "project": project,
                            "path": "src/kernel-once.txt",
                            "content": "x\n",
                            "overwrite": true,
                        }),
                    },
                    ToolCallContext {
                        transport: ToolTransport::Api,
                        session_id: Some(&recording_id),
                        auth: Some(&bootstrap),
                        record_oauth_scope_denials: true,
                    },
                )
                .await
        }
    });
    complete_write_ok(&runtime, client_id, "src/kernel-once.txt").await;
    let outcome = task.await.unwrap();
    assert!(
        outcome.success,
        "{:?}",
        outcome.result.as_ref().and_then(|r| r.error.clone())
    );
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "kernel+dispatch must evaluate permission exactly once"
    );

    let result = outcome.result.expect("tool result");
    let request_id = result.output["permission"]["request_id"]
        .as_str()
        .expect("permission.request_id")
        .to_string();
    assert!(request_id.starts_with("wc_perm_"));
    assert_eq!(result.output["permission"]["status"], "auto_approved");

    let summary = runtime
        .sessions
        .summary(&recording.session_id, Some(40))
        .unwrap();
    let mut seen_ids = Vec::new();
    for event in &summary.events {
        if let Some(perm) = event.permission.as_ref() {
            seen_ids.push(perm.request_id.clone());
        }
    }
    assert!(
        !seen_ids.is_empty(),
        "outer recording session should reuse attached decision"
    );
    for id in &seen_ids {
        assert_eq!(
            id, &request_id,
            "all ledger permission request ids must match the single decision"
        );
    }
    let unique: std::collections::BTreeSet<_> = seen_ids.iter().collect();
    assert_eq!(
        unique.len(),
        1,
        "duplicate permission request ids: {seen_ids:?}"
    );
}
