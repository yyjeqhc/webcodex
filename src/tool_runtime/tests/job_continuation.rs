//! Same-execution recovery through real dispatch, deterministic registry faults,
//! canonical Runner updates, model projection, and parser-ready follow-ups.
use super::support::*;
use super::validation_handoff::{cargo_test_update, completed_progress, running_progress};
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerCapabilities, RunnerRequest, ShellCommandExecutionState};
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
};
use crate::tool_runtime::{ToolResult, ToolRuntime};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Race {
    Public,
    Terminal,
    Cleanup,
}

async fn invoke(
    runtime: &ToolRuntime,
    tool: &str,
    arguments: Value,
    auth: &AuthContext,
) -> ToolResult {
    Box::pin(runtime.call_tool_with_context(
        ToolCallRequest {
            tool_name: tool.to_string(),
            arguments,
        },
        ToolCallContext {
            transport: ToolTransport::Mcp,
            session_id: None,
            auth: Some(auth),
            window: None,
            record_oauth_scope_denials: true,
            host_file_import_trust: HostFileImportTrust::Untrusted,
        },
    ))
    .await
    .result
    .expect("canonical model result")
}

async fn update(runtime: &ToolRuntime, request: &RunnerRequest, tool: &str, terminal: bool) {
    let mut update = cargo_test_update(
        &request.client_id,
        &request.request_id,
        request.job_id.as_deref().unwrap(),
        if terminal { "completed" } else { "running" },
        if terminal {
            "test result: ok. 3 passed; 0 failed; 0 ignored\n"
        } else {
            "PRIVATE_STDOUT\n"
        },
        if terminal { "" } else { "PRIVATE_STDERR\n" },
        terminal.then_some(0),
        if terminal {
            completed_progress()
        } else {
            running_progress(if tool == "cargo_check" {
                "check"
            } else {
                "test"
            })
        },
        terminal,
    );
    if !tool.starts_with("cargo_") {
        update.validation_progress = None;
        update.activity = None;
        update.command_execution_state = terminal.then_some(ShellCommandExecutionState::Completed);
    }
    runtime.runner_registry.update_job(update).await.unwrap();
}

async fn exercise(tool: &'static str, observation_failure: bool, race: Race) {
    let client = "handoff-fixture";
    let runtime = runtime_with_agent_project(client)
        .with_structured_execution_sync_wait(Duration::from_millis(20))
        .with_validation_sync_wait(Duration::from_millis(20));
    register_agent(
        &runtime,
        client,
        None,
        RunnerCapabilities {
            shell: true,
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_cargo_test_lib: true,
            structured_process_argv: true,
            structured_script_payload: true,
            ..Default::default()
        },
    )
    .await;
    let auth = auth_context(None, true);
    let project = agent_test_project_id(client);
    let session = runtime.sessions.start_session(Some(project.clone()), None);
    let mut arguments = match tool {
        "run_shell" => json!({"command": "printf PRIVATE_COMMAND"}),
        "run_process" => json!({"executable": "printf", "args": ["PRIVATE_COMMAND"]}),
        "run_script" => json!({"language": "sh", "script": "printf PRIVATE_COMMAND"}),
        "cargo_check" => json!({"all_targets": false}),
        "cargo_test" => json!({"lib": true}),
        _ => unreachable!(),
    };
    arguments["project"] = json!(project);
    arguments["session_id"] = json!(session.session_id);
    arguments["timeout_secs"] = json!(120);
    arguments["sync_wait_secs"] = json!(1);
    let (reached, release) = runtime
        .runner_registry
        .pause_next_hidden_handoff_failure_for_test(observation_failure);
    let mut task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { invoke(&runtime, tool, arguments, &auth).await }
    });
    let request = tokio::select! {
        result = &mut task => panic!("{tool}: initiating call returned before durable admission: {result:?}"),
        request = wait_for_patch_agent_request(&runtime, client) => request,
    };
    assert!(
        matches!(
            request.kind.as_str(),
            "start_job" | "start_process_job" | "start_script_job" | "start_validation_job"
        ),
        "{}",
        request.kind
    );
    let job_id = request.job_id.clone().expect("one durable identity");
    update(&runtime, &request, tool, false).await;
    tokio::time::timeout(Duration::from_secs(3), reached.notified())
        .await
        .expect("deterministic handoff boundary");
    let foreign = shared_key_auth_context("foreign-handoff-fixture");
    let foreign_access = crate::runner_http::runner_access_from_auth(Some(&foreign));
    assert!(
        runtime
            .runner_registry
            .promote_hidden_job(foreign_access.as_ref(), &job_id)
            .await
            .is_err(),
        "recovery promotion must recheck caller authority atomically"
    );
    if race == Race::Terminal {
        update(&runtime, &request, tool, true).await;
    } else if race == Race::Cleanup {
        assert!(
            !observation_failure,
            "cleanup must still own a hidden record"
        );
        runtime.runner_registry.record_hidden_cleanup_intent(
            job_id.clone(),
            crate::runner_http::runner_access_from_auth(Some(&auth)),
        );
        runtime
            .runner_registry
            .process_hidden_cleanup_intents()
            .await;
    }
    release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    let initial_summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    assert_eq!(
        initial_summary.counts.tool_calls, 1,
        "handoff/recovery cannot become a second execution"
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool(tool);
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .unwrap_or_else(|error| panic!("{tool}: {error}; {}", result.output));
    if race == Race::Terminal {
        assert!(result.success, "{:?}", result.error);
        assert!(!result.output["job_id"].is_string());
        assert!(result.output.get("continuation").is_none());
        assert!(runtime
            .runner_registry
            .get_hidden_job_for_auth(None, &job_id)
            .await
            .is_err());
    } else {
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert_eq!(result.output["failure_kind"], "outcome_unknown");
        assert_eq!(result.output["command_started"], true);
        assert_eq!(result.output["command_completed"], false);
        assert_eq!(result.output["terminal"], false);
        let serialized = serde_json::to_string(&result).unwrap();
        for forbidden in [
            "PRIVATE_COMMAND",
            "PRIVATE_STDOUT",
            "PRIVATE_STDERR",
            "retry_same_call_unchanged",
            "stdout_tail",
            "stderr_tail",
            "command_summary",
            "runner_instance_id",
        ] {
            assert!(
                !serialized.contains(forbidden),
                "{tool} leaked {forbidden}: {serialized}"
            );
        }
        if race == Race::Public {
            assert_eq!(result.output["job_id"], job_id);
            assert_eq!(
                result.output["continuation"]["arguments"]["items"][0]["job_id"],
                job_id
            );
            assert!(runtime
                .runner_registry
                .get_job_for_auth(
                    crate::runner_http::runner_access_from_auth(Some(&auth)).as_ref(),
                    &job_id
                )
                .await
                .is_ok());
            assert!(!runtime
                .runner_registry
                .has_hidden_cleanup_intent_for_test(&job_id));
            if tool.starts_with("cargo_") {
                let pending = runtime
                    .validation_summary_for_session_with_jobs(&initial_summary, 50, Some(&auth))
                    .await;
                assert_ne!(pending["status"], "passed");
                assert!(pending.to_string().contains("outcome_unknown"), "{pending}");
            }
            update(&runtime, &request, tool, true).await;
            let next = &result.output["continuation"];
            let observed = invoke(
                &runtime,
                next["tool"].as_str().unwrap(),
                next["arguments"].clone(),
                &auth,
            )
            .await;
            assert!(
                observed.success,
                "exact returned continuation must parse and observe: {:?}",
                observed.error
            );
            if tool.starts_with("cargo_") {
                let summary = runtime
                    .sessions
                    .summary(&session.session_id, Some(50))
                    .unwrap();
                let reconciled = runtime
                    .validation_summary_for_session_with_jobs(&summary, 50, Some(&auth))
                    .await;
                assert_eq!(reconciled["status"], "passed", "{reconciled}");
            }
        } else {
            assert!(
                !serialized.contains(&job_id),
                "private identity must not leak even in prose"
            );
            assert!(result.output["job_id"].is_null());
            assert!(result.output.get("continuation").is_none());
            assert_eq!(
                result.output["suggested_call"],
                json!({"follow_up_kind": "fallback_recovery", "tool": "list_jobs", "arguments": {"project": project}})
            );
            let next = &result.output["suggested_call"];
            webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(next)
                .expect("private Job identity recovery must pass list_jobs registered inputSchema");
            let listed = invoke(
                &runtime,
                next["tool"].as_str().unwrap(),
                next["arguments"].clone(),
                &auth,
            )
            .await;
            assert!(listed.success, "{:?}", listed.error);
            assert!(!listed.output.to_string().contains(&job_id));
            let stop = wait_for_patch_agent_request(&runtime, client).await;
            assert_eq!(
                stop.kind, "stop_job",
                "only cleanup, never replacement launch"
            );
            assert_eq!(stop.job_id.as_deref(), Some(job_id.as_str()));
            update(&runtime, &request, tool, true).await;
            runtime
                .runner_registry
                .process_hidden_cleanup_intents()
                .await;
        }
    }
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "recovery must not redispatch"
    );
}

#[tokio::test]
async fn run_shell_handoff_recovery_preserves_public_identity_without_redispatch() {
    for observation in [false, true] {
        exercise("run_shell", observation, Race::Public).await;
    }
}

#[tokio::test]
async fn typed_process_and_script_share_same_execution_recovery() {
    for tool in ["run_process", "run_script"] {
        for observation in [false, true] {
            exercise(tool, observation, Race::Public).await;
        }
    }
}

#[tokio::test]
async fn validation_handoff_recovery_stays_actionable_until_same_job_reconciles() {
    for tool in ["cargo_check", "cargo_test"] {
        for observation in [false, true] {
            exercise(tool, observation, Race::Public).await;
        }
    }
}

#[tokio::test]
async fn terminal_race_returns_initiating_result_not_hidden_continuation() {
    for tool in ["run_shell", "cargo_check", "cargo_test"] {
        exercise(tool, false, Race::Terminal).await;
    }
}

#[tokio::test]
async fn cleanup_pending_handoff_fails_closed_with_parser_ready_inventory_recovery() {
    for tool in ["run_shell", "cargo_check", "cargo_test"] {
        exercise(tool, false, Race::Cleanup).await;
    }
}
