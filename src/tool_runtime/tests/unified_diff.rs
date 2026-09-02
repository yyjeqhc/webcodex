//! Canonical unified-diff mutation contract tests.

use super::super::patch::{analyze_unified_diff, MAX_UNIFIED_DIFF_BYTES};
use super::super::*;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use std::collections::BTreeSet;

async fn runtime_with_unified_diff_agent(client_id: &str) -> ToolRuntime {
    let runtime = runtime_with_agent_project(client_id);
    register_agent(
        &runtime,
        client_id,
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    runtime
}

#[tokio::test]
async fn apply_unified_diff_success_uses_exact_two_commands_and_typed_stdin() {
    let client_id = "unified-success";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let marker = "UNIFIED_DIFF_SUCCESS_MARKER";
    let diff = marker_patch("UNIFIED_SUCCESS.md", marker);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let diff = diff.clone();
        async move { runtime.apply_unified_diff(project, diff, None).await }
    });

    let check = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(check.command, "git apply --check -");
    assert_eq!(check.stdin.as_deref(), Some(diff.as_str()));
    assert_eq!(check.cwd.as_deref(), Some("/tmp/agent-proj"));
    assert_safe_patch_command(&check.command, marker);
    complete_patch_agent_request(&runtime, client_id, &check.request_id, 0, "", "").await;

    let apply = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(apply.command, "git apply -");
    assert_eq!(apply.stdin.as_deref(), Some(diff.as_str()));
    assert_eq!(apply.cwd.as_deref(), Some("/tmp/agent-proj"));
    assert_safe_patch_command(&apply.command, marker);
    complete_patch_agent_request(&runtime, client_id, &apply.request_id, 0, "", "").await;

    assert!(
        probe_patch_agent_request(&runtime, client_id).await.is_none(),
        "successful canonical apply must not enqueue --stat, a duplicate --check, or post-apply shell work"
    );

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["applied"], true);
    assert_eq!(result.output["can_apply"], true);
    assert_eq!(result.output["policy_blocked"], false);
    assert_eq!(result.output["state_changed"], true);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["affected_files"][0], "UNIFIED_SUCCESS.md");
}

#[tokio::test]
async fn apply_unified_diff_non_applicable_is_domain_outcome_without_apply_dispatch() {
    let client_id = "unified-not-applicable";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let diff = marker_patch("NOT_APPLICABLE.md", "not-applicable");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let diff = diff.clone();
        async move { runtime.apply_unified_diff(project, diff, None).await }
    });
    let check = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(check.command, "git apply --check -");
    complete_patch_agent_request(
        &runtime,
        client_id,
        &check.request_id,
        1,
        "",
        "patch does not apply",
    )
    .await;

    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    let result = task.await.unwrap();
    assert!(
        result.success,
        "non-applicability is a normal domain result"
    );
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["can_apply"], false);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "not_applicable");
    assert_eq!(result.output["recovery_action"], "regenerate_unified_diff");
}

#[tokio::test]
async fn apply_unified_diff_sensitive_path_is_blocked_by_default_before_shell_dispatch() {
    let client_id = "unified-sensitive";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let diff = marker_patch(".env", "SECRET_PLACEHOLDER");

    let result = runtime.apply_unified_diff(project, diff, None).await;
    assert!(result.success);
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["can_apply"], false);
    assert_eq!(result.output["policy_blocked"], true);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["error_kind"], "policy_blocked");
    assert_eq!(result.output["recovery_action"], "review_sensitive_paths");
    assert!(result.output["warnings"][0]
        .as_str()
        .unwrap()
        .contains(".env"));
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn apply_unified_diff_timestamped_sensitive_header_is_blocked_before_shell_dispatch() {
    let client_id = "unified-sensitive-timestamp";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let diff = "--- a/.env\t2026-08-29 00:00:00 +0000\n+++ b/.env\t2026-08-29 00:00:01 +0000\n@@ -1 +1 @@\n-old\n+new\n";

    let analysis = analyze_unified_diff(diff).expect("timestamped unified diff header");
    assert_eq!(analysis.affected_files, vec![".env"]);
    assert!(analysis.has_sensitive_paths);

    let result = runtime
        .apply_unified_diff(project, diff.to_string(), None)
        .await;
    assert!(result.success);
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["can_apply"], false);
    assert_eq!(result.output["policy_blocked"], true);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["affected_files"][0], ".env");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn apply_unified_diff_rename_extended_header_cannot_bypass_sensitive_policy() {
    let client_id = "unified-sensitive-rename";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    // Git accepts this deliberately inconsistent diff --git header and follows
    // the extended rename target, so policy must inspect the extended header.
    let diff = "diff --git a/safe.txt b/safe.txt\nsimilarity index 100%\nrename from safe.txt\nrename to .env\n";

    let analysis = analyze_unified_diff(diff).expect("rename-only git diff");
    assert_eq!(analysis.affected_files, vec![".env", "safe.txt"]);
    assert!(analysis.has_sensitive_paths);

    let result = runtime
        .apply_unified_diff(project, diff.to_string(), None)
        .await;
    assert!(result.success);
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["policy_blocked"], true);
    assert_eq!(result.output["state_changed"], false);
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[test]
fn unified_diff_hunk_content_that_looks_like_file_headers_is_not_parsed_as_paths() {
    let diff = "diff --git a/HEADER_LIKE.md b/HEADER_LIKE.md\n--- a/HEADER_LIKE.md\n+++ b/HEADER_LIKE.md\n@@ -1 +1 @@\n--- .env\n+++ safe\n";
    let analysis = analyze_unified_diff(diff).expect("valid header-like hunk content");
    assert_eq!(analysis.affected_files, vec!["HEADER_LIKE.md"]);
    assert!(!analysis.has_sensitive_paths);
}

#[tokio::test]
async fn apply_unified_diff_rejects_wrapper_paths_nul_and_size_before_dispatch() {
    let client_id = "unified-input-reject";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);

    let wrapper = "*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-old\n+new\n*** End Patch\n";
    let result = runtime
        .apply_unified_diff(project.clone(), wrapper.to_string(), None)
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "unsupported_diff_format");
    assert_eq!(result.output["expected_format"], "unified_diff");
    assert_eq!(result.output["recovery_action"], "regenerate_unified_diff");
    assert_eq!(result.output["state_changed"], false);

    for diff in [
        "diff --git /etc/passwd /etc/passwd\n--- /etc/passwd\n+++ /etc/passwd\n@@ -1 +1 @@\n-a\n+b\n".to_string(),
        "diff --git a/../outside b/../outside\n--- a/../outside\n+++ b/../outside\n@@ -1 +1 @@\n-a\n+b\n".to_string(),
        "diff --git a/safe.txt b/safe.txt\nsimilarity index 100%\nrename from safe.txt\nrename to ../outside\n".to_string(),
        "diff --git a/src/x b/src/x\n--- a/src/x\n+++ b/src/x\n@@ -1 +1 @@\n-a\n+b\0\n".to_string(),
        "x".repeat(MAX_UNIFIED_DIFF_BYTES + 1),
    ] {
        let result = runtime
            .apply_unified_diff(project.clone(), diff, None)
            .await;
        assert!(!result.success);
        assert_eq!(result.output["state_changed"], false);
        assert_eq!(result.output["execution_state"], "not_started");
    }
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    let legal = "diff --git a/src/foo..bar b/src/foo..bar\n--- a/src/foo..bar\n+++ b/src/foo..bar\n@@ -1 +1 @@\n-a\n+b\n";
    let analysis = analyze_unified_diff(legal).expect("embedded '..' in a filename is legal");
    assert_eq!(analysis.affected_files, vec!["src/foo..bar"]);
}

#[tokio::test]
async fn apply_unified_diff_large_payload_still_travels_only_via_stdin() {
    let client_id = "unified-large";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let marker = "UNIFIED_LARGE_MARKER";
    let diff = large_marker_patch("UNIFIED_LARGE.md", marker);
    assert!(diff.len() > crate::shell_protocol::RAW_SHELL_COMMAND_MAX_BYTES);
    assert!(diff.len() <= MAX_UNIFIED_DIFF_BYTES);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let diff = diff.clone();
        async move { runtime.apply_unified_diff(project, diff, None).await }
    });
    for expected in ["git apply --check -", "git apply -"] {
        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        assert_eq!(request.command, expected);
        assert_eq!(request.stdin.as_deref(), Some(diff.as_str()));
        assert!(!request.command.contains(marker));
        complete_patch_agent_request(&runtime, client_id, &request.request_id, 0, "", "").await;
    }
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["applied"], true);
}

#[tokio::test]
async fn apply_unified_diff_dropped_waiter_after_apply_dispatch_is_outcome_unknown() {
    let client_id = "unified-uncertain";
    let runtime = runtime_with_unified_diff_agent(client_id).await;
    let project = agent_test_project_id(client_id);
    let diff = marker_patch("UNCERTAIN.md", "uncertain");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move { runtime.apply_unified_diff(project, diff, None).await }
    });
    let check = wait_for_patch_agent_request(&runtime, client_id).await;
    complete_patch_agent_request(&runtime, client_id, &check.request_id, 0, "", "").await;

    let apply = wait_for_patch_agent_request(&runtime, client_id).await;
    assert_eq!(apply.command, "git apply -");
    assert_eq!(
        runtime
            .shell_clients
            .cancel_request_dispatch_state(&apply.request_id)
            .await,
        Some(true),
        "test must drop the waiter only after dispatch was observed"
    );

    let result = task.await.unwrap();
    assert!(!result.success);
    assert!(result.output["applied"].is_null());
    assert_eq!(result.output["can_apply"], true);
    assert!(result.output["state_changed"].is_null());
    assert_eq!(result.output["execution_state"], "outcome_unknown");
    assert_eq!(result.output["error_kind"], "outcome_unknown");
    assert_eq!(
        result.output["recovery_action"],
        "inspect_workspace_before_retry"
    );
}

#[tokio::test]
async fn apply_unified_diff_rejects_server_configured_project_without_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_local_project(temp.path(), "local-proj");
    let diff = marker_patch("LOCAL.md", "local");
    let result = runtime
        .apply_unified_diff("local-proj".to_string(), diff, None)
        .await;
    assert!(!result.success);
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["state_changed"], false);
    assert_eq!(result.output["execution_state"], "not_started");
}

#[test]
fn apply_unified_diff_schema_matches_flat_runtime_contract_and_old_tools_are_absent() {
    use crate::tool_runtime::tool_definition::is_known_tool_name;

    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "apply_unified_diff")
        .expect("canonical unified diff spec");
    let input = spec.input_schema["properties"].as_object().unwrap();
    assert!(input.contains_key("project"));
    assert!(input.contains_key("diff"));
    assert!(input.contains_key("deny_sensitive_paths"));
    assert!(!input.contains_key("patch"));
    assert_eq!(input["deny_sensitive_paths"]["default"], true);

    let output = spec.output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("flat output properties");
    let actual: BTreeSet<&str> = output.keys().map(String::as_str).collect();
    let expected: BTreeSet<&str> = [
        "applied",
        "can_apply",
        "policy_blocked",
        "state_changed",
        "execution_state",
        "affected_files",
        "affected_files_truncated",
        "warnings",
        "warnings_truncated",
        "stderr",
        "stderr_truncated",
        "error_kind",
        "expected_format",
        "recovery_action",
        "permission",
        "recovery_kind",
        "recovery_tool",
        "session_hint",
        "trace_ref",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);

    assert!(is_known_tool_name("apply_patch"));
    assert!(specs.iter().any(|spec| spec.name == "apply_patch"));

    for removed in ["apply_patch_checked", "validate_patch"] {
        assert!(!is_known_tool_name(removed), "{removed} must be removed");
        assert!(specs.iter().all(|spec| spec.name != removed));
    }
}
