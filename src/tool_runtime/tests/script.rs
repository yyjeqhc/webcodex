//! Model-facing bounded typed script execution contract.

use super::super::*;
use super::support::*;
use crate::shell_protocol::{
    ShellAgentResultPayload, ShellAgentResultRequest, ShellClientCapabilities,
    ShellCommandExecutionState, ShellScriptLanguage, SCRIPT_MAX_BYTES,
};
use crate::tool_runtime::activity::{ActivityRecord, ActivityRecorder};
use crate::tool_runtime::kernel::{ToolCallContext, ToolCallRequest, ToolTransport};
use serde_json::json;
use std::sync::{Arc, Mutex};

fn script_call(
    project: String,
    session_id: Option<String>,
    language: ShellScriptLanguage,
    script: impl Into<String>,
) -> ToolCall {
    ToolCall::RunScript {
        project,
        language,
        script: script.into(),
        args: vec![
            String::new(),
            "two words".to_string(),
            "$(literal)".to_string(),
            "雪".to_string(),
        ],
        stdin: Some("independent stdin\n".to_string()),
        session_id,
        timeout_secs: Some(30),
        cwd: None,
        purpose: Some(ExecutionPurpose::Operation),
    }
}

async fn register_script_agent(
    runtime: &ToolRuntime,
    client_id: &str,
    root: &std::path::Path,
    structured_script_payload: bool,
    sandbox_inspect_commands: bool,
) -> String {
    let mut capabilities = ShellClientCapabilities::default();
    capabilities.shell = true;
    capabilities.structured_validation_argv = true;
    capabilities.structured_process_argv = true;
    capabilities.structured_script_payload = structured_script_payload;
    capabilities.sandbox_inspect_commands = sandbox_inspect_commands;
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        capabilities,
        vec![registered_project("demo", &root.to_string_lossy())],
    )
    .await;
    crate::tool_runtime::agent_project_runtime_id(client_id, "demo")
}

async fn complete_script_lifecycle(
    runtime: &ToolRuntime,
    client_id: &str,
    request_id: String,
    state: ShellCommandExecutionState,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    error: Option<&str>,
) {
    runtime
        .shell_clients
        .complete(ShellAgentResultPayload {
            result: ShellAgentResultRequest {
                client_id: client_id.to_string(),
                agent_instance_id: "inst".to_string(),
                request_id,
                exit_code,
                stdout: Some(stdout.to_string()),
                stderr: Some(stderr.to_string()),
                duration_ms: Some(9),
                error: error.map(str::to_string),
            },
            command_execution_state: Some(state),
        })
        .await
        .unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn server_local_compatibility_executes_a_temporary_script_file_directly() {
    let cwd = tempfile::tempdir().unwrap();
    let observed_path = cwd.path().join("observed-script-path");
    let marker = cwd.path().join("marker");
    let payload = crate::shell_protocol::ShellScriptPayload {
        language: ShellScriptLanguage::Sh,
        script: "printf '%s' \"$0\" > \"$1\"\nprintf '%s\\n' \"$0\" \"$2\"\n".to_string(),
        args: vec![
            observed_path.to_string_lossy().into_owned(),
            "; touch marker".to_string(),
        ],
    };
    let (exit_code, stdout, stderr, _) =
        super::super::helpers::run_script_sync_bounded_with_sandbox(
            payload,
            None,
            cwd.path().to_path_buf(),
            10,
            None,
        )
        .await
        .unwrap_or_else(|_| panic!("local compatibility script execution should return"));

    assert_eq!(exit_code, 0, "{stderr}");
    assert_eq!(stdout, "<temporary-script>\n; touch marker\n");
    assert!(
        !marker.exists(),
        "script arguments must remain literal argv"
    );
    let temporary_path = std::path::PathBuf::from(std::fs::read_to_string(observed_path).unwrap());
    assert_eq!(
        temporary_path.extension().and_then(|value| value.to_str()),
        Some("sh")
    );
    assert!(!temporary_path.starts_with(cwd.path()));
    assert!(!temporary_path.exists());
}

#[tokio::test]
async fn run_script_wire_is_typed_body_free_command_and_supports_more_than_32_kib() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_script_agent(&runtime, "script-wire", temp.path(), true, false).await;
    let mut large_script = "# typed script payload\n".repeat(1_800);
    large_script.push_str("printf 'done\\n'\n");
    assert!(large_script.len() > 32 * 1024);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let large_script = large_script.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(project, None, ShellScriptLanguage::Bash, large_script),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "script-wire")
        .await
        .expect("run_script should enqueue");
    assert_eq!(request.kind, "run_script");
    assert_eq!(request.command, "");
    assert!(request.process.is_none());
    let payload = request.script.as_ref().expect("typed script payload");
    assert_eq!(payload.language, ShellScriptLanguage::Bash);
    assert_eq!(payload.script, large_script);
    assert_eq!(
        payload.args,
        ["", "two words", "$(literal)", "雪"].map(str::to_string)
    );
    assert_eq!(request.stdin.as_deref(), Some("independent stdin\n"));
    assert!(!request.command.contains(&payload.script));

    complete_script_lifecycle(
        &runtime,
        "script-wire",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "done\n",
        "",
        None,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["execution_source"], "run_script");
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["command_started"], true);
    assert_eq!(result.output["command_completed"], true);
    assert_eq!(result.output["language"], "bash");
    assert_eq!(result.output["purpose"], "operation");
    assert_eq!(
        result.output["script_summary"],
        format!("bash script ({} bytes, 4 args)", large_script.len())
    );
}

#[tokio::test]
async fn run_script_nonzero_timeout_uncertainty_and_interpreter_absence_are_truthful() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_script_agent(&runtime, "script-lifecycle", temp.path(), true, false).await;

    let nonzero_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(project, None, ShellScriptLanguage::Sh, "exit 19"),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "script-lifecycle")
        .await
        .unwrap();
    complete_script_lifecycle(
        &runtime,
        "script-lifecycle",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(19),
        "",
        "",
        None,
    )
    .await;
    let nonzero = nonzero_task.await.unwrap();
    assert!(!nonzero.success);
    assert_eq!(nonzero.output["execution_state"], "completed");
    assert_eq!(nonzero.output["command_started"], true);
    assert_eq!(nonzero.output["command_completed"], true);
    assert_eq!(nonzero.output["exit_code"], 19);
    assert_eq!(nonzero.output["failure_kind"], "command_exit_nonzero");

    let timeout_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(project, None, ShellScriptLanguage::Sh, "sleep 10"),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "script-lifecycle")
        .await
        .unwrap();
    complete_script_lifecycle(
        &runtime,
        "script-lifecycle",
        request.request_id,
        ShellCommandExecutionState::TimedOut,
        Some(-1),
        "",
        "script timed out",
        Some("script timed out"),
    )
    .await;
    let timed_out = timeout_task.await.unwrap();
    assert_eq!(timed_out.output["execution_state"], "timed_out");
    assert_eq!(timed_out.output["command_started"], true);
    assert_eq!(timed_out.output["command_completed"], false);
    assert!(timed_out
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("do not blindly retry"));

    let uncertain_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(project, None, ShellScriptLanguage::Sh, "true"),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    next_patch_agent_request(&runtime, "script-lifecycle")
        .await
        .unwrap();
    runtime
        .shell_clients
        .reconcile_disconnect("script-lifecycle", "inst")
        .await;
    let uncertain = uncertain_task.await.unwrap();
    assert_eq!(uncertain.output["execution_state"], "outcome_unknown");
    assert_eq!(uncertain.output["command_started"], true);
    assert_eq!(uncertain.output["command_completed"], false);
    assert!(uncertain
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("Do not automatically retry"));

    let missing_runtime = test_runtime();
    let missing_project = register_script_agent(
        &missing_runtime,
        "script-interpreter",
        temp.path(),
        true,
        false,
    )
    .await;
    let missing_task = tokio::spawn({
        let runtime = missing_runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(
                        missing_project,
                        None,
                        ShellScriptLanguage::Powershell,
                        "Write-Output never",
                    ),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&missing_runtime, "script-interpreter")
        .await
        .unwrap();
    complete_script_lifecycle(
        &missing_runtime,
        "script-interpreter",
        request.request_id,
        ShellCommandExecutionState::NotStarted,
        None,
        "",
        "",
        Some(
            "interpreter_unavailable: powershell interpreter is unavailable; command was not started",
        ),
    )
    .await;
    let missing = missing_task.await.unwrap();
    assert_eq!(missing.output["execution_state"], "not_started");
    assert_eq!(missing.output["command_started"], false);
    assert_eq!(missing.output["command_completed"], false);
    assert_eq!(missing.output["failure_kind"], "interpreter_unavailable");
}

#[tokio::test]
async fn run_script_capability_and_authority_fail_before_enqueue_without_shell_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_script_agent(&runtime, "legacy-script", temp.path(), false, false).await;
    let result = runtime
        .dispatch_with_auth(
            script_call(project, None, ShellScriptLanguage::Sh, "touch marker"),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "capability_unavailable");
    assert!(result
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("no shell fallback"));
    assert!(next_patch_agent_request(&runtime, "legacy-script")
        .await
        .is_none());

    let denied_runtime = test_runtime().with_permission_evaluator(
        permissions::PermissionEvaluator::with_mode(permissions::AuthorityMode::Restricted),
    );
    let denied_project =
        register_script_agent(&denied_runtime, "denied-script", temp.path(), true, false).await;
    let denied = denied_runtime
        .dispatch_with_auth(
            script_call(
                denied_project,
                None,
                ShellScriptLanguage::Sh,
                "touch marker",
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(denied.output["execution_state"], "not_started");
    assert_eq!(denied.output["command_started"], false);
    assert_eq!(denied.output["command_completed"], false);
    assert_eq!(denied.output["failure_kind"], "permission_denied");
    assert!(next_patch_agent_request(&denied_runtime, "denied-script")
        .await
        .is_none());
}

#[derive(Default)]
struct CapturingActivity {
    commands: Mutex<Vec<Option<String>>>,
}

impl ActivityRecorder for CapturingActivity {
    fn record(&self, record: ActivityRecord<'_>) {
        self.commands
            .lock()
            .unwrap()
            .push(record.command.map(str::to_string));
    }
}

#[tokio::test]
async fn run_script_session_defaults_and_evidence_are_body_and_stdin_free() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let frontend = root.join("frontend");
    std::fs::create_dir_all(&frontend).unwrap();
    let recorder = Arc::new(CapturingActivity::default());
    let runtime = test_runtime().with_activity_recorder(recorder.clone());
    let project = register_script_agent(&runtime, "script-context", &root, true, false).await;
    let session = runtime
        .sessions
        .start_session_with_options(
            sessions::SessionCreateOptions::new(
                Some(project.clone()),
                Some("script context".to_string()),
                SessionMode::Normal,
                sessions::SessionGuards::default(),
            )
            .with_execution_context(sessions::SessionExecutionContext {
                default_cwd: Some("frontend".to_string()),
                default_shell: Some(ExecutionShell::Bash),
                resource: None,
            }),
        )
        .unwrap();
    let raw_script = "printf RAW_SCRIPT_BODY";
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(
                        project,
                        Some(session_id),
                        ShellScriptLanguage::Sh,
                        raw_script,
                    ),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "script-context")
        .await
        .expect("Session run_script should enqueue");
    assert_eq!(
        request.cwd.as_deref(),
        Some(frontend.to_string_lossy().as_ref())
    );
    assert_eq!(
        request.script.as_ref().unwrap().language,
        ShellScriptLanguage::Sh,
        "Session default_shell must not override explicit language"
    );
    complete_script_lifecycle(
        &runtime,
        "script-context",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success);
    assert_eq!(result.output["cwd"], "frontend");

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    let event = finished_event(&summary, "run_script");
    assert_eq!(event.risk_class, "job_run");
    assert!(event.shell_like);
    let started = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_started" && event.tool_name == "run_script")
        .unwrap();
    let input = started.input_summary.as_ref().unwrap();
    assert_eq!(input["language"], "sh");
    assert_eq!(input["script_bytes"], raw_script.len());
    assert_eq!(input["arg_count"], 4);
    assert_eq!(input["stdin_present"], true);
    assert!(input.get("script").is_none());
    assert!(input.get("args").is_none());
    assert!(input.get("stdin").is_none());
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains("RAW_SCRIPT_BODY"));
    assert!(!serialized.contains("independent stdin"));
    assert!(!serialized.contains("$(literal)"));

    let commands = recorder.commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    let expected_preview = format!("sh script ({} bytes, 4 args)", raw_script.len());
    assert_eq!(commands[0].as_deref(), Some(expected_preview.as_str()));
    assert!(!commands[0].as_deref().unwrap().contains("RAW_SCRIPT_BODY"));
}

#[tokio::test]
async fn run_script_ssh_read_only_closed_and_inspect_session_boundaries_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    let runtime = test_runtime();
    let project = register_script_agent(&runtime, "script-guards", &root, true, true).await;

    let ssh = runtime
        .sessions
        .start_session_with_options(
            sessions::SessionCreateOptions::new(
                Some(project.clone()),
                Some("remote script".to_string()),
                SessionMode::Normal,
                sessions::SessionGuards::default(),
            )
            .with_execution_context(sessions::SessionExecutionContext {
                default_cwd: Some("/srv/app".to_string()),
                default_shell: None,
                resource: Some("production".to_string()),
            }),
        )
        .unwrap();
    let unsupported = runtime
        .dispatch_with_auth(
            script_call(
                project.clone(),
                Some(ssh.session_id),
                ShellScriptLanguage::Sh,
                "touch marker",
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(unsupported.output["execution_state"], "not_started");
    assert_eq!(unsupported.output["command_started"], false);
    assert_eq!(unsupported.output["command_completed"], false);
    assert_eq!(unsupported.output["failure_kind"], "unsupported_resource");
    assert!(next_patch_agent_request(&runtime, "script-guards")
        .await
        .is_none());

    let mismatch = runtime.sessions.start_session(
        Some("agent:other:demo".to_string()),
        Some("mismatched script".to_string()),
    );
    let mismatched = runtime
        .dispatch_with_auth(
            script_call(
                project.clone(),
                Some(mismatch.session_id),
                ShellScriptLanguage::Sh,
                "touch marker",
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(
        mismatched.output["failure_kind"],
        "session_project_mismatch"
    );
    assert_eq!(mismatched.output["execution_state"], "not_started");
    assert_eq!(mismatched.output["command_started"], false);
    assert_eq!(mismatched.output["command_completed"], false);
    assert!(next_patch_agent_request(&runtime, "script-guards")
        .await
        .is_none());

    let read_only = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read-only script".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let denied = runtime
        .dispatch_with_auth(
            script_call(
                project.clone(),
                Some(read_only.session_id),
                ShellScriptLanguage::Sh,
                "touch marker",
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(denied.output["guard"], "deny_shell_tools");
    assert_eq!(denied.output["execution_state"], "not_started");
    assert_eq!(denied.output["command_started"], false);
    assert_eq!(denied.output["command_completed"], false);
    assert_eq!(denied.output["failure_kind"], "session_guard_denied");

    let closed = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("closed script".to_string()),
        SessionMode::Normal,
        sessions::SessionGuards::default(),
    );
    runtime.sessions.close_session(&closed.session_id).unwrap();
    let closed_result = runtime
        .dispatch_with_auth(
            script_call(
                project.clone(),
                Some(closed.session_id),
                ShellScriptLanguage::Sh,
                "true",
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert_eq!(closed_result.output["execution_state"], "not_started");
    assert_eq!(closed_result.output["command_started"], false);
    assert_eq!(closed_result.output["command_completed"], false);
    assert_eq!(closed_result.output["failure_kind"], "session_closed");

    let inspect = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("inspect script".to_string()),
        SessionMode::Inspect,
        sessions::SessionGuards::default(),
    );
    let inspect_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = inspect.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    script_call(project, Some(session_id), ShellScriptLanguage::Sh, "true"),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let request = next_patch_agent_request(&runtime, "script-guards")
        .await
        .expect("inspect script should enqueue");
    assert_eq!(
        request.sandbox.as_deref(),
        Some(crate::command_sandbox::INSPECT_SANDBOX_MODE)
    );
    complete_script_lifecycle(
        &runtime,
        "script-guards",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        "",
        "",
        None,
    )
    .await;
    assert!(inspect_task.await.unwrap().success);
}

#[tokio::test]
async fn run_script_shared_bounds_reject_before_enqueue_with_full_prestart_tuple() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_script_agent(&runtime, "script-bounds", temp.path(), true, false).await;
    let invalid_calls = [
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: String::new(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "x".repeat(SCRIPT_MAX_BYTES + 1),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "bad\0script".to_string(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "true".to_string(),
            args: vec![String::new(); 257],
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "true".to_string(),
            args: vec!["x".repeat(8_193)],
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "true".to_string(),
            args: Vec::new(),
            stdin: Some("x".repeat(65_537)),
            session_id: None,
            timeout_secs: Some(60),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "true".to_string(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(121),
            cwd: None,
            purpose: None,
        },
        ToolCall::RunScript {
            project: project.clone(),
            language: ShellScriptLanguage::Sh,
            script: "true".to_string(),
            args: Vec::new(),
            stdin: None,
            session_id: None,
            timeout_secs: Some(60),
            cwd: Some("bad\0cwd".to_string()),
            purpose: None,
        },
    ];
    for call in invalid_calls {
        let result = runtime
            .dispatch_with_auth(call, Some(&auth_context(None, true)))
            .await;
        assert_eq!(result.output["execution_state"], "not_started");
        assert_eq!(result.output["command_started"], false);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["failure_kind"], "invalid_arguments");
    }
    assert!(next_patch_agent_request(&runtime, "script-bounds")
        .await
        .is_none());
}

#[tokio::test]
async fn model_facing_run_script_session_denials_keep_phase_a_tuple() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project = register_script_agent(&runtime, "script-kernel", temp.path(), true, false).await;
    let read_only = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        Some("read-only model script".to_string()),
        SessionMode::ReadOnly,
        sessions::SessionGuards::default(),
    );
    let auth = auth_context(None, true);
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "run_script".to_string(),
                arguments: json!({
                    "project": project,
                    "language": "sh",
                    "script": "true"
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: Some(&read_only.session_id),
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
            },
        )
        .await;
    let result = outcome.result.unwrap();
    assert_eq!(result.output["execution_state"], "not_started");
    assert_eq!(result.output["command_started"], false);
    assert_eq!(result.output["command_completed"], false);
    assert_eq!(result.output["failure_kind"], "session_guard_denied");
    assert!(next_patch_agent_request(&runtime, "script-kernel")
        .await
        .is_none());
}
