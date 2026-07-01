//! Dispatch tests for tool_runtime.

use super::super::cargo::*;
use super::super::codex::*;
use super::super::files::*;
use super::super::git::*;
use super::super::helpers::*;
use super::super::patch::*;
use super::super::types::*;
use super::super::*;
use super::support::*;
use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
use crate::shell_client::ShellClientRegistry;
use crate::shell_protocol::{
    AgentPolicySummary, ShellAgentPollRequest, ShellAgentProjectSummary, ShellAgentResultRequest,
    ShellAgentShellRequest, ShellClientCapabilities, ShellClientRegisterRequest,
};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn cargo_runtime_tools_are_known_and_parse() {
    for name in ["cargo_fmt", "cargo_check", "cargo_test"] {
        assert!(KNOWN_TOOL_NAMES.contains(&name), "{name} missing");
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
    assert!(result.output["stdout_tail"]
        .as_str()
        .unwrap_or("")
        .contains("cargo-test-stdout-tail"));
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
    assert!(result.output["stdout_tail"].is_string());
    assert!(result.output["stderr_tail"].is_string());
}

#[test]
fn build_codex_command_uses_default_bin_and_approval_mode() {
    let codex = CodexConfig::default();
    let cmd = build_codex_command(&codex, "fix tests", None, None).unwrap();
    // Default approval_mode is disabled (empty), so --approval-mode is not
    // emitted. This keeps the runtime compatible with Codex CLI builds
    // that do not support the flag.
    assert!(
        !cmd.contains("--approval-mode"),
        "default command must not include --approval-mode, got: {}",
        cmd
    );
    assert!(cmd.starts_with("'codex' "));
    assert!(cmd.ends_with("'fix tests'"));
}

#[test]
fn build_codex_command_uses_configured_bin_and_approval_mode() {
    let codex = CodexConfig {
        bin: "/usr/local/bin/codex".to_string(),
        approval_mode: "suggest".to_string(),
        default_timeout_secs: 3600,
        max_prompt_bytes: 100_000,
        allowed_extra_args: vec![],
    };
    let cmd = build_codex_command(&codex, "hello", None, None).unwrap();
    assert!(cmd.starts_with("'/usr/local/bin/codex' --approval-mode 'suggest' "));
}

#[test]
fn build_codex_command_config_suggest_emits_flag() {
    // CODEX_APPROVAL_MODE=suggest should include --approval-mode suggest.
    let codex = CodexConfig {
        approval_mode: "suggest".to_string(),
        ..CodexConfig::default()
    };
    let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
    assert!(cmd.contains("--approval-mode 'suggest'"));
}

#[test]
fn build_codex_command_config_none_omits_flag() {
    // CODEX_APPROVAL_MODE=none must not emit --approval-mode.
    for value in ["none", "off", "disabled", "NONE", "Off"] {
        let codex = CodexConfig {
            approval_mode: value.to_string(),
            ..CodexConfig::default()
        };
        let cmd = build_codex_command(&codex, "hi", None, None).unwrap();
        assert!(
            !cmd.contains("--approval-mode"),
            "CODEX_APPROVAL_MODE={:?} should omit --approval-mode, got: {}",
            value,
            cmd
        );
    }
}

#[test]
fn build_codex_command_request_approval_mode_overrides_config() {
    // A config with suggest is overridden by an explicit request value.
    let codex = CodexConfig {
        approval_mode: "suggest".to_string(),
        ..CodexConfig::default()
    };
    let cmd = build_codex_command(&codex, "hi", Some("full-auto"), None).unwrap();
    assert!(cmd.contains("--approval-mode 'full-auto'"));
    assert!(!cmd.contains("'suggest'"));
}

#[test]
fn build_codex_command_request_approval_mode_none_omits_flag() {
    // request approval_mode=none overrides a non-empty config and omits the
    // flag entirely.
    let codex = CodexConfig {
        approval_mode: "suggest".to_string(),
        ..CodexConfig::default()
    };
    for value in ["none", "off", "disabled", ""] {
        let cmd = build_codex_command(&codex, "hi", Some(value), None).unwrap();
        assert!(
            !cmd.contains("--approval-mode"),
            "request approval_mode={:?} should omit --approval-mode, got: {}",
            value,
            cmd
        );
    }
}

#[test]
fn build_codex_command_request_approval_mode_blank_omits_flag() {
    // A blank request value means disabled (not "fall back to config").
    let codex = CodexConfig {
        approval_mode: "suggest".to_string(),
        ..CodexConfig::default()
    };
    let cmd = build_codex_command(&codex, "hi", Some("   "), None).unwrap();
    assert!(!cmd.contains("--approval-mode"));
}

#[test]
fn build_codex_command_shell_escapes_prompt() {
    let codex = CodexConfig::default();
    let cmd = build_codex_command(&codex, "rm -rf /'; echo pwned", None, None).unwrap();
    // The single quote in the prompt must be escaped with '\'\'',
    // preventing the trailing "; echo pwned" from running as a command.
    assert!(cmd.contains("'\\''"));
    // The whole prompt is wrapped in single quotes, so the semicolon is
    // literal, not a command separator.
    assert!(cmd.contains("'; echo pwned'"));
}

#[test]
fn build_codex_command_rejects_empty_prompt_via_validate() {
    // build_codex_command itself does not check emptiness (run_codex does),
    // but an empty prompt still gets escaped. Verify it doesn't panic.
    let codex = CodexConfig::default();
    let cmd = build_codex_command(&codex, "", None, None).unwrap();
    // Empty prompt produces a trailing ''.
    assert!(cmd.ends_with(" ''"));
}

#[test]
fn build_codex_command_rejects_extra_args_by_default() {
    let codex = CodexConfig::default(); // empty allowlist
    let err =
        build_codex_command(&codex, "hi", None, Some(vec!["--verbose".to_string()])).unwrap_err();
    assert!(err.contains("allowlist"));
    assert!(err.contains("--verbose"));
}

#[test]
fn build_codex_command_allows_allowlisted_extra_args() {
    let codex = codex_config_with_allowlist(&["--verbose", "--json"]);
    let cmd = build_codex_command(
        &codex,
        "hi",
        None,
        Some(vec!["--verbose".to_string(), "--json".to_string()]),
    )
    .unwrap();
    assert!(cmd.contains("'--verbose'"));
    assert!(cmd.contains("'--json'"));
}

#[test]
fn build_codex_command_rejects_non_allowlisted_extra_args() {
    let codex = codex_config_with_allowlist(&["--verbose"]);
    let err =
        build_codex_command(&codex, "hi", None, Some(vec!["--danger".to_string()])).unwrap_err();
    assert!(err.contains("allowlist"));
    assert!(err.contains("--danger"));
}

#[test]
fn build_codex_command_rejects_nul_in_extra_arg() {
    let codex = codex_config_with_allowlist(&["--verbose"]);
    let err =
        build_codex_command(&codex, "hi", None, Some(vec!["--ver\0bose".to_string()])).unwrap_err();
    assert!(err.contains("NUL"));
}

#[test]
fn build_codex_command_rejects_too_many_extra_args() {
    let allowed: Vec<String> = (0..40).map(|i| format!("--a{}", i)).collect();
    let codex = CodexConfig {
        allowed_extra_args: allowed.clone(),
        ..CodexConfig::default()
    };
    let too_many: Vec<String> = allowed;
    let err = build_codex_command(&codex, "hi", None, Some(too_many)).unwrap_err();
    assert!(err.contains("at most 32"));
}

#[tokio::test]
async fn run_codex_rejects_empty_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
    let result = runtime
        .run_codex(
            "demo".to_string(),
            "   ".to_string(),
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("empty"));
}

#[tokio::test]
async fn run_codex_rejects_nul_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
    let result = runtime
        .run_codex(
            "demo".to_string(),
            "fix\0tests".to_string(),
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
}

#[tokio::test]
async fn run_codex_rejects_oversized_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let codex = CodexConfig {
        max_prompt_bytes: 16,
        ..CodexConfig::default()
    };
    let runtime = runtime_with_codex(tmp.path(), codex);
    let big = "x".repeat(100);
    let result = runtime
        .run_codex("demo".to_string(), big, None, None, None, None)
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("too large"));
    assert!(err.contains("16"));
}

#[tokio::test]
async fn run_codex_rejects_nul_approval_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
    let result = runtime
        .run_codex(
            "demo".to_string(),
            "fix tests".to_string(),
            Some("full\0auto".to_string()),
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
}

#[tokio::test]
async fn run_codex_rejects_extra_args_without_allowlist() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = runtime_with_codex(tmp.path(), CodexConfig::default());
    let result = runtime
        .run_codex(
            "demo".to_string(),
            "fix tests".to_string(),
            None,
            None,
            None,
            Some(vec!["--verbose".to_string()]),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("allowlist"));
}

#[tokio::test]
async fn run_codex_agent_output_contains_structured_fields() {
    let runtime = runtime_with_agent_project("oe");
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "oe", None, caps).await;
    let project = agent_test_project_id("oe");
    let result = runtime
        .run_codex(
            project.clone(),
            "echo hello".to_string(),
            None,
            Some(10),
            None,
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert!(result.output["job_id"].is_string());
    assert_eq!(result.output["kind"], "codex");
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["status_endpoint"], "/api/jobs/status");
    assert_eq!(result.output["log_endpoint"], "/api/jobs/log");
    assert!(
        runtime.local_jobs.lock().await.is_empty(),
        "agent-backed Codex jobs must not create server-local job metadata"
    );
}

#[tokio::test]
async fn run_codex_rejects_server_configured_project() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let runtime = runtime_with_codex(root, CodexConfig::default());
    let result = runtime
        .run_codex(
            "demo".to_string(),
            "echo hello".to_string(),
            None,
            Some(10),
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("unknown_project"));
    assert!(runtime.local_jobs.lock().await.is_empty());
}

#[tokio::test]
async fn run_codex_agent_uses_configured_command_builder() {
    let codex = CodexConfig {
        default_timeout_secs: 42,
        approval_mode: "suggest".to_string(),
        ..CodexConfig::default()
    };
    let mut runtime = runtime_with_agent_project("oe");
    runtime.codex = Arc::new(codex);
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "oe", None, caps).await;
    let result = runtime
        .run_codex(
            agent_test_project_id("oe"),
            "echo hi".to_string(),
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    let jobs = runtime.shell_clients.list_jobs(None).await;
    assert_eq!(jobs.len(), 1);
    // The configured approval_mode flows through build_codex_command into
    // the agent job's command preview.
    assert!(
        jobs[0]
            .command_preview
            .contains("--approval-mode 'suggest'"),
        "{}",
        jobs[0].command_preview
    );
}

#[tokio::test]
async fn run_codex_agent_omits_approval_mode_when_disabled() {
    // Default (disabled) approval_mode must not emit --approval-mode.
    let codex = CodexConfig::default();
    let mut runtime = runtime_with_agent_project("om");
    runtime.codex = Arc::new(codex);
    let mut caps = ShellClientCapabilities::default();
    caps.async_shell_jobs = true;
    register_agent(&runtime, "om", None, caps).await;
    let result = runtime
        .run_codex(
            agent_test_project_id("om"),
            "echo hi".to_string(),
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    let jobs = runtime.shell_clients.list_jobs(None).await;
    assert_eq!(jobs.len(), 1);
    assert!(
        !jobs[0].command_preview.contains("--approval-mode"),
        "disabled approval_mode must omit --approval-mode, got: {}",
        jobs[0].command_preview
    );
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
    let runtime = test_runtime();
    for spec in runtime.tool_specs() {
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
