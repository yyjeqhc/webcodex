//! Dispatch tests for tool_runtime.

use super::super::cargo::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentPollRequest, ShellAgentResultRequest, ShellClientCapabilities,
    ShellClientRegisterRequest,
};
use serde_json::json;

#[test]
fn cargo_runtime_tools_are_known_and_parse() {
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(is_known_tool_name(name), "{name} missing");
    }
    assert!(matches!(
        ToolCall::from_tool_name(
            "cargo_fmt",
            json!({"project":"agent:oe:webcodex","check":true,"cwd":"crates/app"})
        )
        .unwrap(),
        ToolCall::CargoFmt {
            check: Some(true),
            ..
        }
    ));
    assert!(matches!(
        ToolCall::from_tool_name("cargo_check", json!({"project":"agent:oe:webcodex"})).unwrap(),
        ToolCall::CargoCheck {
            all_targets: None,
            ..
        }
    ));
    assert!(matches!(
        ToolCall::from_tool_name(
            "cargo_test",
            json!({"project":"agent:oe:webcodex","filter":"tool_runtime"})
        )
        .unwrap(),
        ToolCall::CargoTest { filter: Some(filter), .. } if filter == "tool_runtime"
    ));
}

#[test]
fn cargo_command_builders_use_expected_defaults_and_escaping() {
    assert_eq!(cargo_fmt_command(true), "cargo fmt -- --check");
    assert_eq!(
        cargo_check_command(None, None, None, None, None).unwrap(),
        "cargo check --all-targets"
    );
    assert_eq!(
        cargo_test_command(
            Some("tool_runtime".to_string()),
            None,
            None,
            None,
            None,
            None,
            None
        )
        .unwrap(),
        "cargo test 'tool_runtime'"
    );
    assert!(cargo_check_command(None, None, None, Some("feat\0x".to_string()), None).is_err());
}

#[tokio::test]
async fn cargo_tools_reject_unsafe_cwd_before_project_dispatch() {
    let runtime = test_runtime();
    let fmt = runtime
        .cargo_fmt(
            "agent:oe:webcodex".to_string(),
            Some("../outside".to_string()),
            None,
            None,
        )
        .await;
    assert!(!fmt.success);
    assert!(fmt.error.unwrap().contains("parent traversal"));

    let check = runtime
        .cargo_check(
            "agent:oe:webcodex".to_string(),
            Some("/tmp".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!check.success);
    assert!(check.error.unwrap().contains("project-relative"));

    let test = runtime
        .cargo_test(
            "agent:oe:webcodex".to_string(),
            Some("src\0bad".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!test.success);
    assert!(test.error.unwrap().contains("NUL"));
}

#[tokio::test]
async fn cargo_check_failure_includes_stderr_tail_or_guidance() {
    let runtime = runtime_with_agent_project("cargo-checker");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-checker", None, caps).await;
    let project = agent_test_project_id("cargo-checker");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_check(project, None, None, None, None, None, None, Some(60))
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-checker")
        .await
        .expect("cargo_check should enqueue a cargo command");
    assert_eq!(req.command, "cargo check --all-targets");
    complete_patch_agent_request(
        &runtime,
        "cargo-checker",
        &req.request_id,
        101,
        "",
        "error: simulated compile failure\n",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or("");
    assert!(error.contains("cargo command failed"));
    assert!(error.contains("command was started"));
    assert!(error.contains("stdout_tail/stderr_tail"));
    assert!(error.contains("narrower cargo filter"));
    assert_eq!(result.output["passed"], false);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert!(result.output["stderr_tail"]
        .as_str()
        .unwrap_or("")
        .contains("simulated compile failure"));
}

#[tokio::test]
async fn cargo_test_failure_includes_stderr_tail_or_guidance() {
    let runtime = runtime_with_agent_project("cargo-tester");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-tester", None, caps).await;
    let project = agent_test_project_id("cargo-tester");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_test(
                project,
                None,
                Some("failing".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(60),
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-tester")
        .await
        .expect("cargo_test should enqueue a cargo command");
    assert_eq!(req.command, "cargo test 'failing'");
    complete_patch_agent_request(
        &runtime,
        "cargo-tester",
        &req.request_id,
        101,
        "test result: FAILED. 0 passed; 1 failed\ncargo-test-stdout-tail\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or("");
    assert!(error.contains("cargo command failed"));
    assert!(error.contains("command was started"));
    assert!(error.contains("stdout_tail/stderr_tail"));
    assert_eq!(result.output["passed"], false);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert!(result.output["stdout_tail"]
        .as_str()
        .unwrap_or("")
        .contains("cargo-test-stdout-tail"));
}

#[tokio::test]
async fn cargo_test_output_includes_bounded_failed_test_diagnostics() {
    let runtime = runtime_with_agent_project("cargo-diag");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-diag", None, caps).await;
    let project = agent_test_project_id("cargo-diag");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_test(
                project,
                None,
                Some("multi_fail".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(60),
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-diag")
        .await
        .expect("cargo_test should enqueue a cargo command");
    assert_eq!(req.command, "cargo test 'multi_fail'");
    complete_patch_agent_request(
        &runtime,
        "cargo-diag",
        &req.request_id,
        101,
        "running 10 tests\n\
test tests::first_failure ... FAILED\n\
test tests::second_failure ... FAILED\n\
test tests::third_failure ... FAILED\n\
\n\
failures:\n\
\n\
---- tests::first_failure stdout ----\n\
thread 'tests::first_failure' panicked at 'TOKEN=secret-value'\n\
assertion failed: left == right\n\
\n\
test result: FAILED. 7 passed; 3 failed; 1 ignored; 0 measured; 0 filtered out\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["passed"], false);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert_eq!(result.output["tests_passed"], 7);
    assert_eq!(result.output["tests_failed"], 3);
    assert_eq!(result.output["tests_detected"], true);
    assert_eq!(result.output["tests_run_count"], 10);
    assert_eq!(result.output["zero_tests_run"], false);
    assert!(result.output["stdout_truncated"].is_boolean());
    assert!(result.output["stderr_truncated"].is_boolean());

    let diagnostics = &result.output["diagnostics"];
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["parser"], "minimal_bounded_tail_parser");
    assert_eq!(diagnostics["diagnostic_count"], 3);
    assert_eq!(diagnostics["test_summary"]["passed"], 7);
    assert_eq!(diagnostics["test_summary"]["failed"], 3);
    assert_eq!(diagnostics["test_summary"]["ignored"], 1);
    assert_eq!(
        diagnostics["failed_tests"],
        json!([
            "tests::first_failure",
            "tests::second_failure",
            "tests::third_failure"
        ])
    );
    assert_eq!(diagnostics["first_failed_test"], "tests::first_failure");
    assert_eq!(diagnostics["failed_tests_truncated"], false);
    assert_eq!(diagnostics["truncated"], false);

    let diagnostics_json = diagnostics.to_string();
    for raw in ["TOKEN=secret-value", "assertion failed", "left == right"] {
        assert!(
            !diagnostics_json.contains(raw),
            "cargo_test diagnostics must not include unsafe text {raw:?}: {diagnostics_json}"
        );
    }
}

#[tokio::test]
async fn cargo_test_passing_output_includes_empty_failed_tests_diagnostics() {
    let runtime = runtime_with_agent_project("cargo-pass-diag");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-pass-diag", None, caps).await;
    let project = agent_test_project_id("cargo-pass-diag");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_test(
                project,
                None,
                Some("all_ok".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(60),
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-pass-diag")
        .await
        .expect("cargo_test should enqueue a cargo command");
    complete_patch_agent_request(
        &runtime,
        "cargo-pass-diag",
        &req.request_id,
        0,
        "running 12 tests\n\
test result: ok. 12 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["passed"], true);
    assert_eq!(result.output["tests_passed"], 12);
    assert_eq!(result.output["tests_failed"], 0);
    assert_eq!(result.output["tests_detected"], true);
    assert_eq!(result.output["tests_run_count"], 12);
    assert_eq!(result.output["zero_tests_run"], false);

    let diagnostics = &result.output["diagnostics"];
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["diagnostic_count"], 0);
    assert_eq!(diagnostics["failed_tests"], json!([]));
    assert_eq!(diagnostics["failed_tests_truncated"], false);
    assert_eq!(diagnostics["test_summary"]["passed"], 12);
    assert_eq!(diagnostics["test_summary"]["failed"], 0);
}

#[tokio::test]
async fn cargo_test_multi_harness_counts_match_diagnostics_summary() {
    let runtime = runtime_with_agent_project("cargo-multi-harness");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-multi-harness", None, caps).await;
    let project = agent_test_project_id("cargo-multi-harness");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_test(
                project,
                None,
                Some("multi_harness".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(60),
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-multi-harness")
        .await
        .expect("cargo_test should enqueue a cargo command");
    complete_patch_agent_request(
        &runtime,
        "cargo-multi-harness",
        &req.request_id,
        101,
        "running 2 tests\n\
test result: ok. 2 passed; 0 failed; 1 ignored\n\
running 4 tests\n\
test tests::broken ... FAILED\n\
test result: FAILED. 3 passed; 1 failed; 0 ignored\n\
running 0 tests\n\
test result: ok. 0 passed; 0 failed; 2 ignored\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    // Top-level counts (full combined output) and diagnostics (bounded tails)
    // must agree when every summary is still present in the tails.
    assert_eq!(result.output["tests_passed"], 5);
    assert_eq!(result.output["tests_failed"], 1);
    let diagnostics = &result.output["diagnostics"];
    assert_eq!(diagnostics["available"], true);
    assert_eq!(diagnostics["test_summary"]["passed"], 5);
    assert_eq!(diagnostics["test_summary"]["failed"], 1);
    assert_eq!(diagnostics["test_summary"]["ignored"], 3);
    assert_eq!(diagnostics["diagnostic_count"], 1);
    assert_eq!(diagnostics["failed_tests"], json!(["tests::broken"]));
    assert_eq!(diagnostics["first_failed_test"], "tests::broken");
    assert_eq!(diagnostics["failed_tests_truncated"], false);
}

#[tokio::test]
async fn cargo_test_agent_timeout_is_not_validation_failed() {
    let runtime = runtime_with_agent_project("cargo-timeout");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-timeout", None, caps).await;
    let project = agent_test_project_id("cargo-timeout");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_test(
                project,
                None,
                Some("slow".to_string()),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(60),
            )
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-timeout")
        .await
        .expect("cargo_test should enqueue a cargo command");
    assert_eq!(req.command, "cargo test 'slow'");
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "cargo-timeout".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(-1),
            stdout: Some("partial cargo output\n".to_string()),
            stderr: Some("Command timed out after 60 seconds".to_string()),
            duration_ms: Some(60_000),
            error: Some("command timed out".to_string()),
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "timeout");
}

#[tokio::test]
async fn cargo_fmt_failure_includes_stderr_tail_or_guidance() {
    let runtime = runtime_with_agent_project("cargo-formatter");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "cargo-formatter", None, caps).await;
    let project = agent_test_project_id("cargo-formatter");
    let runtime_for_task = runtime.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .cargo_fmt(project, None, Some(true), Some(60))
            .await
    });
    let req = next_patch_agent_request(&runtime, "cargo-formatter")
        .await
        .expect("cargo_fmt should enqueue a cargo command");
    assert_eq!(req.command, "cargo fmt -- --check");
    complete_patch_agent_request(
        &runtime,
        "cargo-formatter",
        &req.request_id,
        1,
        "Diff in src/lib.rs\n",
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    let error = result.error.as_deref().unwrap_or("");
    assert!(error.contains("cargo command failed"));
    assert!(error.contains("command was started"));
    assert!(error.contains("stdout_tail/stderr_tail"));
    assert_eq!(result.output["passed"], false);
    assert_eq!(result.output["failure_kind"], "validation_failed");
    assert!(result.output["stdout_tail"].is_string());
    assert!(result.output["stderr_tail"].is_string());
}

#[tokio::test]
async fn apply_patch_agent_does_not_require_server_local_project_root() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    runtime
        .shell_clients
        .register(ShellClientRegisterRequest {
            client_id: "patcher".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            capabilities: Some(caps),
            projects: Some(vec![registered_project(
                "agent-proj",
                "/definitely/not/on/server/webcodex-agent-only",
            )]),
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();

    let project = agent_test_project_id("patcher");
    let patch = "diff --git a/REMOTE_ONLY.md b/REMOTE_ONLY.md\n\
new file mode 100644\n\
--- /dev/null\n\
+++ b/REMOTE_ONLY.md\n\
@@ -0,0 +1 @@\n\
+remote\n"
        .to_string();
    let runtime_for_task = runtime.clone();
    let apply_task =
        tokio::spawn(async move { runtime_for_task.apply_patch(project, patch).await });

    let mut check_req = None;
    for _ in 0..10 {
        check_req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "patcher".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if check_req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let check_req = check_req.expect("apply_patch should enqueue git apply --check for the agent");
    assert_eq!(check_req.command, "git apply --check - && echo OK");
    assert!(check_req
        .stdin
        .as_deref()
        .unwrap_or("")
        .contains("REMOTE_ONLY.md"));
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "patcher".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: check_req.request_id,
            exit_code: Some(0),
            stdout: Some("OK\n".to_string()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let mut apply_req = None;
    for _ in 0..10 {
        apply_req = runtime
            .shell_clients
            .poll(ShellAgentPollRequest {
                client_id: "patcher".to_string(),
                agent_instance_id: "inst".to_string(),
                projects: None,
            })
            .await
            .unwrap();
        if apply_req.is_some() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let apply_req = apply_req.expect("apply_patch should enqueue git apply for the agent");
    assert_eq!(apply_req.command, "git apply -");
    assert!(apply_req
        .stdin
        .as_deref()
        .unwrap_or("")
        .contains("REMOTE_ONLY.md"));
    runtime
        .shell_clients
        .complete(ShellAgentResultRequest {
            client_id: "patcher".to_string(),
            agent_instance_id: "inst".to_string(),
            request_id: apply_req.request_id,
            exit_code: Some(0),
            stdout: Some(String::new()),
            stderr: Some(String::new()),
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = apply_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["success"], true);
    assert!(result.output["changed_files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some("REMOTE_ONLY.md")));
}

#[tokio::test]
async fn apply_patch_agent_command_excludes_patch_content_and_uses_stdin_and_cwd() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "patcher", None, caps).await;

    let project = agent_test_project_id("patcher");
    let marker = "ZZZ_PATCH_MARKER_APPLY_ZZZ";
    let patch = marker_patch("APPLY_MARKER.md", marker);
    let runtime_for_task = runtime.clone();
    let patch_for_apply = patch.clone();
    let apply_task =
        tokio::spawn(async move { runtime_for_task.apply_patch(project, patch_for_apply).await });

    // 1) preflight check: `git apply --check - && echo OK`
    let check_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("apply_patch should enqueue a git apply --check request");
    assert_safe_patch_command(&check_req.command, marker);
    assert_eq!(check_req.command, "git apply --check - && echo OK");
    assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
    assert_eq!(check_req.cwd.as_deref(), Some("/tmp/agent-proj"));
    complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "OK\n", "").await;

    // 2) apply: `git apply -`
    let apply_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("apply_patch should enqueue a git apply request");
    assert_safe_patch_command(&apply_req.command, marker);
    assert_eq!(apply_req.command, "git apply -");
    assert_eq!(apply_req.stdin.as_deref(), Some(patch.as_str()));
    assert_eq!(apply_req.cwd.as_deref(), Some("/tmp/agent-proj"));
    complete_patch_agent_request(&runtime, "patcher", &apply_req.request_id, 0, "", "").await;

    let result = apply_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["success"], true);
}

#[tokio::test]
async fn apply_patch_rejects_nul_byte_patch() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "patcher", None, caps).await;
    let project = agent_test_project_id("patcher");
    let patch = "diff --git a/A b/A\n--- a/A\n+++ b/A\n@@ -1 +1 @@\n-a\n\0+b\n";
    let result = runtime.apply_patch(project, patch.to_string()).await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
}

#[tokio::test]
async fn apply_patch_checked_does_not_apply_when_check_fails() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "patcher", None, caps).await;

    let project = agent_test_project_id("patcher");
    let marker = "ZZZ_PATCH_MARKER_CHECKFAIL_ZZZ";
    let patch = marker_patch("CHECKFAIL_PROBE.md", marker);
    let runtime_for_task = runtime.clone();
    let patch_for_task = patch.clone();
    let checked_task = tokio::spawn(async move {
        runtime_for_task
            .apply_patch_checked(project, patch_for_task, Some(true))
            .await
    });

    // 1) validate preflight check: fails (exit 1) -> can_apply=false.
    let check_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("apply_patch_checked should enqueue a validate check request");
    assert_safe_patch_command(&check_req.command, marker);
    assert_eq!(check_req.command, "git apply --check -");
    assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
    complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 1, "", "bad").await;

    // 2) validate stat summary still runs (read-only, regardless of can_apply).
    let stat_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("validate_patch should enqueue a git apply --stat request");
    assert_safe_patch_command(&stat_req.command, marker);
    assert_eq!(stat_req.command, "git apply --stat -");
    complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "").await;

    // 3) No apply step must be enqueued because the preflight failed.
    let leaked_apply = next_patch_agent_request(&runtime, "patcher").await;
    assert!(
        leaked_apply.is_none(),
        "apply_patch_checked must not apply when the check fails (got: {:?})",
        leaked_apply.map(|r| r.command)
    );

    let result = checked_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["applied"], false);
    assert_eq!(result.output["validate"]["can_apply"], false);
}

#[tokio::test]
async fn apply_patch_checked_applies_large_patch_over_command_limit_via_stdin() {
    let runtime = runtime_with_agent_project("patcher");
    let mut caps = ShellClientCapabilities::default();
    caps.shell = true;
    register_agent(&runtime, "patcher", None, caps).await;

    let project = agent_test_project_id("patcher");
    let marker = "ZZZ_PATCH_MARKER_LARGE_CHECKED_ZZZ";
    let patch = large_marker_patch("LARGE_CHECKED_PROBE.md", marker);
    // Prove the patch exceeds the agent shell command length limit; it must
    // still validate + apply because it travels over stdin, not the command.
    assert!(patch.len() > 8_000, "patch must exceed command limit");
    assert!(patch.len() <= MAX_VALIDATE_PATCH_BYTES);

    let runtime_for_task = runtime.clone();
    let patch_for_task = patch.clone();
    let checked_task = tokio::spawn(async move {
        runtime_for_task
            .apply_patch_checked(project, patch_for_task, Some(true))
            .await
    });

    // 1) validate check.
    let check_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("validate check request");
    assert_safe_patch_command(&check_req.command, marker);
    assert_eq!(check_req.stdin.as_deref(), Some(patch.as_str()));
    complete_patch_agent_request(&runtime, "patcher", &check_req.request_id, 0, "", "").await;

    // 2) validate stat.
    let stat_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("validate stat request");
    assert_safe_patch_command(&stat_req.command, marker);
    complete_patch_agent_request(&runtime, "patcher", &stat_req.request_id, 0, "stat", "").await;

    // 3) apply preflight check.
    let apply_check_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("apply check request");
    assert_safe_patch_command(&apply_check_req.command, marker);
    assert_eq!(apply_check_req.command, "git apply --check - && echo OK");
    assert_eq!(apply_check_req.stdin.as_deref(), Some(patch.as_str()));
    complete_patch_agent_request(
        &runtime,
        "patcher",
        &apply_check_req.request_id,
        0,
        "OK\n",
        "",
    )
    .await;

    // 4) apply.
    let apply_req = next_patch_agent_request(&runtime, "patcher")
        .await
        .expect("apply request");
    assert_safe_patch_command(&apply_req.command, marker);
    assert_eq!(apply_req.command, "git apply -");
    assert_eq!(apply_req.stdin.as_deref(), Some(patch.as_str()));
    complete_patch_agent_request(&runtime, "patcher", &apply_req.request_id, 0, "", "").await;

    // 5) post-apply git_diff_summary (drain + complete generically).
    if let Some(diff_req) = next_patch_agent_request(&runtime, "patcher").await {
        complete_patch_agent_request(&runtime, "patcher", &diff_req.request_id, 0, "", "").await;
    }

    let result = checked_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["applied"], true);
    assert_eq!(result.output["validate"]["can_apply"], true);
}

#[tokio::test]
async fn patch_tools_reject_server_configured_project() {
    // A server-configured (local) project must NOT be a runtime surface for
    // any patch tool: the server never reads/writes its filesystem directly.
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_local_project(tmp.path(), "local-proj");
    let patch = marker_patch("LOCAL_PROBE.md", "marker");

    let apply = runtime
        .apply_patch("local-proj".to_string(), patch.clone())
        .await;
    assert!(!apply.success);
    let apply_err = apply.error.unwrap();
    assert!(
        apply_err.contains("agent-registered")
            || apply_err.contains("server-configured")
            || apply_err.contains("Unknown project")
            || apply_err.contains("unknown_project"),
        "apply_patch should reject a server-configured project: {}",
        apply_err
    );

    let checked = runtime
        .apply_patch_checked("local-proj".to_string(), patch.clone(), Some(true))
        .await;
    assert!(!checked.success);
    let checked_err = checked.error.unwrap();
    assert!(
        checked_err.contains("agent-registered")
            || checked_err.contains("server-configured")
            || checked_err.contains("Unknown project")
            || checked_err.contains("unknown_project"),
        "apply_patch_checked should reject a server-configured project: {}",
        checked_err
    );

    let validate = runtime
        .validate_patch("local-proj".to_string(), patch.clone(), None)
        .await;
    assert!(!validate.success);
    let validate_err = validate.error.unwrap();
    assert!(
        validate_err.contains("agent-registered")
            || validate_err.contains("server-configured")
            || validate_err.contains("Unknown project")
            || validate_err.contains("unknown_project"),
        "validate_patch should reject a server-configured project: {}",
        validate_err
    );
}

#[test]
fn cleanup_paths_reject_sensitive_and_project_root() {
    let root = vec![".".to_string()];
    assert!(validate_limited_cleanup_paths(&root, true).is_err());
    let sensitive = vec!["agent.toml".to_string()];
    assert!(validate_limited_cleanup_paths(&sensitive, true).is_err());
    let safe = vec!["tmp_web_codex_smoke.txt".to_string()];
    assert_eq!(
        validate_limited_cleanup_paths(&safe, true).unwrap(),
        vec!["tmp_web_codex_smoke.txt".to_string()]
    );
}

#[test]
fn project_management_tools_require_expected_fields() {
    for spec in registered_tool_specs() {
        if spec.name == "register_project" || spec.name == "create_project" {
            let required = spec.input_schema["required"]
                .as_array()
                .unwrap_or_else(|| panic!("{} must have required array", spec.name));
            for field in ["client_id", "id", "name", "path"] {
                assert!(
                    required.iter().any(|v| v == field),
                    "{} input_schema must require '{}'",
                    spec.name,
                    field
                );
            }
        }
    }
}

#[tokio::test]
async fn dispatch_register_project_rejects_unknown_client_id() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::RegisterProject {
            client_id: "no-such-agent".to_string(),
            id: "my-project".to_string(),
            name: "My Project".to_string(),
            path: "/root/git/my-project".to_string(),
            description: None,
            allow_patch: true,
            overwrite: false,
        })
        .await;
    assert!(!result.success);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("unknown agent"),
        "register_project should reject unknown client_id: {:?}",
        result.error
    );
}

#[tokio::test]
async fn dispatch_create_project_rejects_unknown_client_id() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::CreateProject {
            client_id: "no-such-agent".to_string(),
            id: "hello".to_string(),
            name: "Hello".to_string(),
            path: "/root/git/hello".to_string(),
            description: None,
            allow_patch: true,
            template: None,
            git_init: false,
            allow_existing_empty: false,
            overwrite: false,
        })
        .await;
    assert!(!result.success);
    assert!(
        result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("unknown agent"),
        "create_project should reject unknown client_id: {:?}",
        result.error
    );
}

#[tokio::test]
async fn dispatch_register_project_rejects_unsafe_id() {
    let runtime = test_runtime();
    for bad_id in ["", "a/b", "a\\b", "..", "a..b", "a\0b"] {
        let result = runtime
            .dispatch(ToolCall::RegisterProject {
                client_id: "oe".to_string(),
                id: bad_id.to_string(),
                name: "Test".to_string(),
                path: "/root/git/test".to_string(),
                description: None,
                allow_patch: true,
                overwrite: false,
            })
            .await;
        assert!(
            !result.success,
            "register_project should reject unsafe id '{:?}'",
            bad_id
        );
    }
}

#[tokio::test]
async fn dispatch_create_project_rejects_relative_path() {
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::CreateProject {
            client_id: "oe".to_string(),
            id: "hello".to_string(),
            name: "Hello".to_string(),
            path: "relative/path".to_string(),
            description: None,
            allow_patch: true,
            template: None,
            git_init: false,
            allow_existing_empty: false,
            overwrite: false,
        })
        .await;
    assert!(!result.success);
    assert!(
        result.error.as_deref().unwrap_or("").contains("absolute"),
        "create_project should reject relative path: {:?}",
        result.error
    );
}
