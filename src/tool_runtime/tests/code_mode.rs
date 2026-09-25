//! Feature-gated integration evidence for Experimental Code Mode E1.

use super::support::*;
use crate::runner_protocol::RunnerCapabilities;
use crate::tool_runtime::kernel::{
    HostFileImportTrust, ToolCallContext, ToolCallOutcome, ToolCallRequest, ToolTransport,
};
use crate::tool_runtime::orchestration_host::{
    CanonicalOrchestrationHost, OrchestrationHostFailureKind, OrchestrationPolicy,
};
use crate::tool_runtime::structured_execution::STRUCTURED_EXECUTION_SYNC_WAIT_SECS;
use crate::tool_runtime::{ObserveJobsItem, ObserveJobsWakeOn, ToolRuntime};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task::JoinHandle;

#[derive(Debug)]
struct ObservedRunnerRequest {
    login: bool,
    client_id: String,
    cwd: Option<String>,
}

fn spawn_code_mode_call(
    runtime: &ToolRuntime,
    tool_name: &'static str,
    project: String,
    session_id: String,
    source: String,
    timeout_ms: u64,
) -> JoinHandle<ToolCallOutcome> {
    let runtime = runtime.clone();
    tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: tool_name.to_string(),
                    arguments: json!({
                        "project": project,
                        "session_id": session_id,
                        "source": source,
                        "timeout_ms": timeout_ms,
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session_id),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await
    })
}

async fn e2a_validation_runtime(client_id: &str) -> (ToolRuntime, String, String) {
    let runtime =
        runtime_with_agent_project(client_id).with_validation_sync_wait(Duration::from_millis(20));
    register_agent(
        &runtime,
        client_id,
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Code Mode E2a integration".to_string()),
    );
    (runtime, project, session.session_id)
}

async fn call_code_mode_with_local_runners(
    runtime: &ToolRuntime,
    clients: &[&str],
    project: &str,
    session_id: &str,
    source: &str,
) -> (ToolCallOutcome, Vec<ObservedRunnerRequest>) {
    let runtime_for_task = runtime.clone();
    let project = project.to_string();
    let session_id_owned = session_id.to_string();
    let source = source.to_string();
    let task = tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        runtime_for_task
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "code_mode_exec".to_string(),
                    arguments: json!({
                        "project": project,
                        "session_id": session_id_owned,
                        "source": source,
                        "timeout_ms": 5_000,
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session_id_owned),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    let mut observed = Vec::new();
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "Code Mode integration call did not finish within the test deadline"
        );
        let mut made_progress = false;
        for client_id in clients {
            let request = runtime
                .runner_registry
                .poll(crate::runner_protocol::RunnerPollRequest {
                    client_id: (*client_id).to_string(),
                    runner_instance_id: "inst".to_string(),
                })
                .await
                .unwrap();
            let Some(request) = request else {
                continue;
            };
            made_progress = true;
            observed.push(ObservedRunnerRequest {
                login: false,
                client_id: (*client_id).to_string(),
                cwd: request.cwd.clone(),
            });
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        }
        if !made_progress {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    (task.await.unwrap(), observed)
}

fn init_git_repo(path: &std::path::Path) {
    let output = std::process::Command::new("git")
        .arg("init")
        .arg("--quiet")
        .arg(path)
        .output()
        .expect("git init");
    assert!(output.status.success(), "git init failed: {output:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e1_still_rejects_structured_validation_before_runner_dispatch() {
    let client_id = "code-mode-e1-validation-denied";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let outcome = spawn_code_mode_call(
        &runtime,
        "code_mode_exec",
        project,
        session_id,
        "await tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600});".to_string(),
        5_000,
    )
    .await
    .unwrap();
    let result = outcome.result.expect("outer E1 ToolResult");
    assert!(!result.success, "E1 must reject cargo_check: {result:?}");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_cargo_check_handoff_preserves_same_canonical_job_and_sparse_receipt() {
    let client_id = "code-mode-e2a-check-handoff";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project.clone(),
        session_id.clone(),
        r#"
        const check = await tools.cargo_check({
            sync_wait_secs: 99,
            timeout_secs: 600
        });
        text({job_id: check.output?.job_id ?? null, terminal: check.output?.terminal ?? null});
        "#
        .to_string(),
        5_000,
    );

    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    let request_json = serde_json::to_value(&request).unwrap();
    assert_eq!(
        request_json["job_context"]["validation"]["sync_wait_secs"], 5,
        "E2a clamps only the synchronous Job-handoff preference"
    );
    assert_eq!(
        request_json["job_context"]["validation"]["effective_timeout_secs"],
        600
    );
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "Checking e2a v0.1.0\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(outcome.success, "{outcome:?}");
    let result = outcome.result.expect("outer E2a ToolResult");
    assert!(result.success, "{result:?}");
    let receipt = &result.output["effect_receipt"];
    assert_eq!(receipt["consequential_calls"], 1);
    assert_eq!(receipt["known_results"], 0);
    assert_eq!(receipt["job_handoffs"], 1);
    assert_eq!(receipt["outcome_unknown"], 0);
    assert_eq!(receipt["children"][0]["tool"], "cargo_check");
    assert_eq!(receipt["children"][0]["outcome"], "job_handoff");
    assert_eq!(receipt["children"][0]["job_id"], job_id);
    assert_eq!(
        receipt["children"][0]["continuation"]["tool"],
        "observe_jobs"
    );
    let continuation_text = receipt["children"][0]["continuation"].to_string();
    assert!(continuation_text.contains(&job_id));
    assert!(!receipt.to_string().contains("Checking e2a"));

    let baseline = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: job_id.clone(),
                after_observation_token: None,
                observation_ref: None,
            }],
            20,
            None,
            ObserveJobsWakeOn::Change,
            Some(&bootstrap_auth_context()),
        )
        .await;
    assert!(baseline.success, "{:?}", baseline.error);
    assert_eq!(baseline.output["items"][0]["job_id"], job_id);
    assert_eq!(baseline.output["items"][0]["output"]["status"], "running");
    assert!(
        probe_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "Job observation must not restart validation"
    );

    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished check\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let terminal = runtime
        .observe_jobs_for_auth(
            vec![ObserveJobsItem {
                job_id: job_id.clone(),
                after_observation_token: None,
                observation_ref: None,
            }],
            20,
            None,
            ObserveJobsWakeOn::Change,
            Some(&bootstrap_auth_context()),
        )
        .await;
    assert!(terminal.success, "{:?}", terminal.error);
    assert_eq!(terminal.output["items"][0]["job_id"], job_id);
    assert_eq!(terminal.output["items"][0]["output"]["status"], "completed");

    let summary = runtime.sessions.summary(&session_id, Some(100)).unwrap();
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(serialized.contains("cargo_check"));
    assert!(serialized.contains(&job_id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_omitted_sync_wait_uses_canonical_validation_default() {
    let client_id = "code-mode-e2a-default-handoff";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session_id,
        r#"
        const check = await tools.cargo_check({timeout_secs: 600});
        text({job_id: check.output?.job_id ?? null});
        "#
        .to_string(),
        5_000,
    );

    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    let request_json = serde_json::to_value(&request).unwrap();
    assert_eq!(
        request_json["job_context"]["validation"]["sync_wait_secs"],
        STRUCTURED_EXECUTION_SYNC_WAIT_SECS,
        "E2a omission must reach the canonical validation budget resolver"
    );
    assert_eq!(
        request_json["job_context"]["validation"]["effective_timeout_secs"],
        600
    );
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "Checking default e2a v0.1.0\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(outcome.success, "{outcome:?}");
    let result = outcome.result.expect("outer E2a ToolResult");
    assert!(result.success, "{result:?}");
    let receipt = &result.output["effect_receipt"];
    assert_eq!(receipt["consequential_calls"], 1);
    assert_eq!(receipt["job_handoffs"], 1);
    assert_eq!(receipt["outcome_unknown"], 0);
    assert_eq!(receipt["children"][0]["tool"], "cargo_check");
    assert_eq!(receipt["children"][0]["outcome"], "job_handoff");
    assert_eq!(receipt["children"][0]["job_id"], job_id);
    assert_eq!(
        receipt["children"][0]["continuation"]["tool"],
        "observe_jobs"
    );
    assert!(
        probe_patch_agent_request(&runtime, client_id)
            .await
            .is_none(),
        "E2a default handoff must not start a replacement validation"
    );

    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished default e2a check\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let terminal = runtime
        .job_status_for_auth(job_id.clone(), false, None)
        .await;
    assert!(terminal.success, "{:?}", terminal.error);
    assert_eq!(terminal.output["job_id"], job_id);
    assert_eq!(terminal.output["status"], "completed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_failed_cargo_test_is_known_result_not_outcome_unknown() {
    let client_id = "code-mode-e2a-known-failure";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session_id,
        r#"
        const test = await tools.cargo_test({sync_wait_secs: 5, timeout_secs: 600});
        text({child_success: test.success});
        "#
        .to_string(),
        5_000,
    );
    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "failed",
            "test example ... FAILED\n",
            "",
            Some(101),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(
        outcome.success,
        "frontend itself should finish: {outcome:?}"
    );
    let result = outcome.result.expect("outer E2a ToolResult");
    assert!(
        result.success,
        "frontend business result remains known: {result:?}"
    );
    assert_eq!(result.output["effect_receipt"]["consequential_calls"], 1);
    assert_eq!(result.output["effect_receipt"]["known_results"], 1);
    assert_eq!(result.output["effect_receipt"]["job_handoffs"], 0);
    assert_eq!(result.output["effect_receipt"]["outcome_unknown"], 0);
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["outcome"],
        "known_result"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_promise_all_validators_handoff_sequentially_then_jobs_remain_independent() {
    let client_id = "code-mode-e2a-two-validations";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session_id,
        r#"
        const [check, test] = await Promise.all([
            tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600}),
            tools.cargo_test({sync_wait_secs: 1, timeout_secs: 600})
        ]);
        text({check_job: check.output?.job_id ?? null, test_job: test.output?.job_id ?? null});
        "#
        .to_string(),
        5_000,
    );

    let (first_request, first_job) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    let first_request_json = serde_json::to_value(&first_request).unwrap();
    let first_tool = first_request_json["job_context"]["validation"]["tool"]
        .as_str()
        .expect("first structured validation tool");
    let first_step = if first_tool == "cargo_test" {
        "test"
    } else {
        "check"
    };
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &first_request.request_id,
            &first_job,
            "running",
            "first validation running\n",
            "",
            None,
            super::validation_handoff::running_progress(first_step),
            false,
        ))
        .await
        .unwrap();

    let (second_request, second_job) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    assert_ne!(
        first_job, second_job,
        "each canonical validator owns one Job"
    );
    let second_request_json = serde_json::to_value(&second_request).unwrap();
    let second_tool = second_request_json["job_context"]["validation"]["tool"]
        .as_str()
        .expect("second structured validation tool");
    let second_step = if second_tool == "cargo_test" {
        "test"
    } else {
        "check"
    };
    assert_ne!(
        first_tool, second_tool,
        "Promise.all must dispatch both requested validators"
    );
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &second_request.request_id,
            &second_job,
            "running",
            "second validation running\n",
            "",
            None,
            super::validation_handoff::running_progress(second_step),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(outcome.success, "{outcome:?}");
    let result = outcome.result.expect("outer E2a ToolResult");
    assert!(result.success, "{result:?}");
    let receipt = &result.output["effect_receipt"];
    assert_eq!(receipt["consequential_calls"], 2);
    assert_eq!(
        receipt["job_handoffs"], 2,
        "unexpected receipt/result: {result:?}"
    );
    assert_eq!(receipt["known_results"], 0);
    assert_eq!(receipt["outcome_unknown"], 0);
    let mut receipt_jobs = receipt["children"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|child| child["job_id"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    receipt_jobs.sort();
    let mut expected_jobs = vec![first_job.clone(), second_job.clone()];
    expected_jobs.sort();
    assert_eq!(receipt_jobs, expected_jobs);

    let baseline = runtime
        .observe_jobs_for_auth(
            vec![
                ObserveJobsItem {
                    job_id: first_job.clone(),
                    after_observation_token: None,
                    observation_ref: None,
                },
                ObserveJobsItem {
                    job_id: second_job.clone(),
                    after_observation_token: None,
                    observation_ref: None,
                },
            ],
            20,
            None,
            ObserveJobsWakeOn::AllTerminal,
            Some(&bootstrap_auth_context()),
        )
        .await;
    assert!(baseline.success, "{:?}", baseline.error);
    assert_eq!(baseline.output["items"][0]["output"]["status"], "running");
    assert_eq!(baseline.output["items"][1]["output"]["status"], "running");

    for (request, job) in [(&first_request, &first_job), (&second_request, &second_job)] {
        runtime
            .runner_registry
            .update_job(super::validation_handoff::cargo_test_update(
                client_id,
                &request.request_id,
                job,
                "completed",
                "validation completed\n",
                "",
                Some(0),
                super::validation_handoff::completed_progress(),
                true,
            ))
            .await
            .unwrap();
    }
    let terminal = runtime
        .observe_jobs_for_auth(
            vec![
                ObserveJobsItem {
                    job_id: first_job.clone(),
                    after_observation_token: None,
                    observation_ref: None,
                },
                ObserveJobsItem {
                    job_id: second_job.clone(),
                    after_observation_token: None,
                    observation_ref: None,
                },
            ],
            20,
            None,
            ObserveJobsWakeOn::AllTerminal,
            Some(&bootstrap_auth_context()),
        )
        .await;
    assert!(terminal.success, "{:?}", terminal.error);
    assert_eq!(terminal.output["items"][0]["output"]["status"], "completed");
    assert_eq!(terminal.output["items"][1]["output"]["status"], "completed");
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_js_error_after_job_handoff_preserves_effect_receipt_and_no_retry_claim() {
    let client_id = "code-mode-e2a-after-child-error";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session_id,
        r#"
        const check = await tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600});
        throw new Error("E2A_AFTER_CHILD");
        "#
        .to_string(),
        5_000,
    );
    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "check running\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(!outcome.success);
    let result = outcome.result.expect("outer failed E2a ToolResult");
    assert!(!result.success);
    assert_eq!(result.output["effect_receipt"]["job_handoffs"], 1);
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["job_id"],
        job_id
    );
    let message = result.output["message"].as_str().unwrap_or_default();
    assert!(message.contains("E2A_AFTER_CHILD"));
    assert!(message.contains("Do not blindly rerun the whole orchestration"));
    assert!(!message.contains("retry_same"));
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    let recovery_actions = result.output["recovery"]["actions"].as_array().unwrap();
    assert!(recovery_actions.contains(&json!("fix_code_mode_source")));
    assert!(recovery_actions.contains(&json!("observe_existing_job_continuations")));
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["continuation"]["tool"],
        "observe_jobs"
    );
    assert!(!result.output["recovery"].to_string().contains(&job_id));

    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "done\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_cpu_timeout_after_child_dispatch_preserves_started_job_truth() {
    let client_id = "code-mode-e2a-timeout-after-child";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let task = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session_id,
        r#"
        const child = tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600});
        while (true) {}
        "#
        .to_string(),
        300,
    );
    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "check running before frontend timeout\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(!outcome.success);
    let result = outcome.result.expect("outer timeout ToolResult");
    assert_eq!(result.output["failure_kind"], "timeout");
    assert_eq!(result.output["effect_receipt"]["consequential_calls"], 1);
    assert_eq!(result.output["effect_receipt"]["job_handoffs"], 1);
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["job_id"],
        job_id
    );
    assert!(result.output["message"]
        .as_str()
        .unwrap_or_default()
        .contains("Do not blindly rerun the whole orchestration"));
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    let recovery_actions = result.output["recovery"]["actions"].as_array().unwrap();
    assert!(recovery_actions.contains(&json!("reduce_or_bound_code_mode_work")));
    assert!(recovery_actions.contains(&json!("observe_existing_job_continuations")));
    assert_eq!(
        result.output["effect_receipt"]["children"][0]["continuation"]["tool"],
        "observe_jobs"
    );
    assert!(!result.output["recovery"].to_string().contains(&job_id));
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());

    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "done\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_denies_mutation_shell_recursion_and_invalid_validator_before_business_dispatch() {
    let client_id = "code-mode-e2a-denied-effects";
    let (runtime, project, _) = e2a_validation_runtime(client_id).await;
    for (label, source) in [
        (
            "apply_text_edits",
            "await tools.apply_text_edits({changes: []});",
        ),
        (
            "run_shell",
            "await tools.run_shell({command: 'echo forbidden'});",
        ),
        (
            "recursive_e1",
            "await tools.code_mode_exec({source: `text('nested')`});",
        ),
        (
            "recursive_e2a",
            "await tools.code_mode_exec_effectful({source: `text('nested')`});",
        ),
    ] {
        let session = runtime
            .sessions
            .start_session(Some(project.clone()), Some(format!("deny {label}")));
        let outcome = spawn_code_mode_call(
            &runtime,
            "code_mode_exec_effectful",
            project.clone(),
            session.session_id,
            source.to_string(),
            2_000,
        )
        .await
        .unwrap();
        assert!(!outcome.success, "{label} unexpectedly succeeded");
        assert!(probe_patch_agent_request(&runtime, client_id)
            .await
            .is_none());
    }

    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("invalid sync wait".to_string()));
    let outcome = spawn_code_mode_call(
        &runtime,
        "code_mode_exec_effectful",
        project,
        session.session_id,
        r#"
        const check = await tools.cargo_check({sync_wait_secs: 0, timeout_secs: 600});
        text({success: check.success, state: check.output?.execution_state ?? null});
        "#
        .to_string(),
        2_000,
    )
    .await
    .unwrap();
    assert!(
        !outcome.success,
        "canonical parser rejection must fail the frontend call: {outcome:?}"
    );
    let result = outcome.result.expect("invalid child result");
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], "child_call_failed");
    assert_eq!(result.output["child_failure"]["ordinal"], 1);
    assert_eq!(result.output["child_failure"]["tool"], "cargo_check");
    assert_eq!(
        result.output["child_failure"]["failure_kind"],
        "invalid_arguments"
    );
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    assert!(result.output["recovery"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("fix_child_arguments")));
    assert!(
        result.output.get("effect_receipt").is_none(),
        "prestart rejection is not an effect"
    );
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_parent_and_child_complete_without_retired_continuity_overlays() {
    use crate::tool_runtime::kernel::{ToolInvocationMetadata, ToolProtocolCapabilities};

    let client_id = "code-mode-e2a-session-continuity";
    let (runtime, project, session_id) = e2a_validation_runtime(client_id).await;
    let runtime_for_call = runtime.clone();
    let project_for_call = project.clone();
    let session_for_call = session_id.clone();
    let task = tokio::spawn(async move {
        let auth = bootstrap_auth_context();
        runtime_for_call
            .call_tool_with_invocation_metadata(
                ToolCallRequest {
                    tool_name: "code_mode_exec_effectful".to_string(),
                    arguments: json!({
                        "project": project_for_call,
                        "session_id": session_for_call,
                        "source": "const check = await tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600}); text({job_id: check.output?.job_id ?? null});",
                        "timeout_ms": 5_000,
                    }),
                },
                ToolCallContext {
                    transport: ToolTransport::Mcp,
                    session_id: Some(&session_for_call),
                    auth: Some(&auth),
                    window: None,
                    record_oauth_scope_denials: true,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
                ToolInvocationMetadata::default(),
                ToolProtocolCapabilities {

                    ..Default::default()
                },
            )
            .await
    });

    let (request, job_id) =
        super::validation_handoff::poll_start_validation_job(&runtime, client_id).await;
    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "running",
            "Checking continuity v0.1.0\n",
            "",
            None,
            super::validation_handoff::running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let outcome = task.await.unwrap();
    assert!(outcome.success, "{outcome:?}");
    let result = outcome.result.expect("E2a result");
    assert!(result.success, "{result:?}");
    assert!(result.output.get("session_context_revision").is_none());
    assert!(result.output.get("session_continuity").is_none());
    assert_eq!(result.output["effect_receipt"]["job_handoffs"], 1);

    runtime
        .runner_registry
        .update_job(super::validation_handoff::cargo_test_update(
            client_id,
            &request.request_id,
            &job_id,
            "completed",
            "Finished continuity check\n",
            "",
            Some(0),
            super::validation_handoff::completed_progress(),
            true,
        ))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e2a_outer_job_run_scope_denial_starts_no_validation_process() {
    let client_id = "code-mode-e2a-scope-denied";
    let shared_key_hash = "code-mode-e2a-scope-shared-key";
    let runtime = test_runtime().with_validation_sync_wait(Duration::from_millis(20));
    let auth = oauth_bridge_auth_context(
        shared_key_hash,
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_AGENT_REGISTER,
        ],
    );
    register_agent_projects_for_auth(
        &runtime,
        client_id,
        &auth,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
        vec![registered_project(
            "agent-proj",
            "/tmp/code-mode-e2a-scope-denied",
        )],
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("Code Mode E2a scope denial".to_string()),
    );
    let session_id = session.session_id;
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "code_mode_exec_effectful".to_string(),
                arguments: json!({
                    "project": project,
                    "session_id": session_id,
                    "source": "await tools.cargo_check({sync_wait_secs: 1, timeout_secs: 600});",
                    "timeout_ms": 2_000,
                }),
            },
            ToolCallContext {
                transport: ToolTransport::Mcp,
                session_id: Some(&session_id),
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(!outcome.success);
    assert!(
        matches!(
            outcome.error_status,
            Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InsufficientScope { .. })
        ),
        "expected canonical scope denial before outer E2a execution: {outcome:?}"
    );
    assert!(probe_agent_request_for_client(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn canonical_orchestration_host_distinguishes_child_scope_denial_from_invalid_arguments() {
    let client_id = "orchestration-host-scope-denied";
    let shared_key_hash = "orchestration-host-scope-shared-key";
    let runtime = test_runtime().with_validation_sync_wait(Duration::from_millis(20));
    let auth = oauth_bridge_auth_context(
        shared_key_hash,
        &[
            crate::auth::SCOPE_RUNTIME_READ,
            crate::auth::SCOPE_PROJECT_READ,
            crate::auth::SCOPE_AGENT_REGISTER,
        ],
    );
    register_agent_projects_for_auth(
        &runtime,
        client_id,
        &auth,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
        vec![registered_project(
            "agent-proj",
            "/tmp/orchestration-host-scope-denied",
        )],
    )
    .await;
    let project = agent_test_project_id(client_id);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("orchestration child scope denial".to_string()),
    );
    let policy = OrchestrationPolicy {
        frontend: "test_structured_plan",
        policy_name: "test structured plan",
        admitted_tools: &["cargo_check"],
        denied_tools: &[],
        additional_forbidden_argument_fields: &[],
        nested_sync_wait_max_secs: Some(5),
        max_mutation_calls: None,
        validation_after_mutation: false,
    };
    let host = CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&auth),
        project,
        session.session_id,
        ToolTransport::Mcp,
        Some("test-parent".to_string()),
        policy,
    );

    let error = host
        .invoke_tool(
            1,
            "cargo_check".to_string(),
            json!({"sync_wait_secs": 1, "timeout_secs": 600}),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.failure_kind(),
        OrchestrationHostFailureKind::InsufficientScope
    );
    assert_eq!(error.failure_kind().as_str(), "insufficient_scope");
    let composition = host.composition_summary(1, 1, 0, 0);
    assert_eq!(composition.nested_calls, 1);
    assert_eq!(composition.nested_failures, 1);
    assert_eq!(composition.max_in_flight, 1);
    assert!(probe_agent_request_for_client(&runtime, client_id)
        .await
        .is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn canonical_orchestration_host_runs_without_the_v8_frontend() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("README.md"), "frontend-independent host\n").unwrap();

    let runtime = test_runtime();
    let client_id = "orchestration-host-direct";
    let exact_project =
        register_runner_project_at_path(&runtime, client_id, "demo", tmp.path()).await;
    let session = runtime.sessions.start_session(
        Some(exact_project.clone()),
        Some("orchestration host direct test".to_string()),
    );
    let auth = bootstrap_auth_context();
    let policy = OrchestrationPolicy {
        frontend: "test_structured_plan",
        policy_name: "test structured plan",
        admitted_tools: &["read_files"],
        denied_tools: &[],
        additional_forbidden_argument_fields: &[],
        nested_sync_wait_max_secs: None,
        max_mutation_calls: None,
        validation_after_mutation: false,
    };
    let host = Arc::new(CanonicalOrchestrationHost::new(
        runtime.clone(),
        Some(&auth),
        exact_project.clone(),
        session.session_id.clone(),
        ToolTransport::Mcp,
        Some("test-parent".to_string()),
        policy,
    ));
    host.assert_scheduling_policy_fences_for_test().await;
    let host_for_task = Arc::clone(&host);
    let task = tokio::spawn(async move {
        host_for_task
            .invoke_tool(
                1,
                "read_files".to_string(),
                json!({"items": [{"path": "README.md", "start_line": 1, "limit": 20}]}),
            )
            .await
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    while !task.is_finished() {
        assert!(
            Instant::now() < deadline,
            "direct orchestration host call did not finish within the test deadline"
        );
        let request = runtime
            .runner_registry
            .poll(crate::runner_protocol::RunnerPollRequest {
                client_id: client_id.to_string(),
                runner_instance_id: "inst".to_string(),
            })
            .await
            .unwrap();
        if let Some(request) = request {
            complete_agent_request_by_running_locally(&runtime, client_id, request).await;
        } else {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    let response = task.await.unwrap().expect("canonical nested read");
    assert!(response.success, "{response:?}");
    let composition = host.composition_summary(17, 77, 123, 5);
    assert_eq!(composition.nested_calls, 1);
    assert_eq!(composition.nested_successes, 1);
    assert_eq!(composition.nested_failures, 0);
    assert_eq!(composition.max_in_flight, 1);
    assert_eq!(composition.duration_ms, 17);
    assert_eq!(composition.slot_wait_ms, 5);
    assert_eq!(composition.input_bytes, 77);
    assert_eq!(composition.returned_bytes, 123);
    assert!(composition.nested_raw_result_bytes_total > 0);
    assert_eq!(composition.nested_tool_counts.get("read_files"), Some(&1));

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .expect("session summary");
    let read_start = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_started"
                && event.tool_name == "read_files"
                && event.logical_invocation_role.as_deref() == Some("business")
        })
        .expect("canonical child business evidence");
    assert_eq!(read_start.session_id, session.session_id);
    assert_eq!(
        read_start.resolved_project.as_deref(),
        Some(exact_project.as_str())
    );
    let input_summary = read_start
        .input_summary
        .as_ref()
        .expect("canonical child input summary");
    assert_eq!(input_summary["project"], exact_project);
    assert!(summary
        .events
        .iter()
        .all(|event| event.tool_name != "code_mode_exec"));
}

#[tokio::test]
async fn canonical_orchestration_host_rejects_server_owned_metadata_without_frontend_help() {
    let runtime = test_runtime();
    let session = runtime
        .sessions
        .start_session(None, Some("orchestration metadata guard".to_string()));
    let policy = OrchestrationPolicy {
        frontend: "test_structured_plan",
        policy_name: "test structured plan",
        admitted_tools: &["read_files"],
        denied_tools: &[],
        additional_forbidden_argument_fields: &[],
        nested_sync_wait_max_secs: None,
        max_mutation_calls: None,
        validation_after_mutation: false,
    };
    let host = CanonicalOrchestrationHost::new(
        runtime,
        None,
        "agent:unused:demo".to_string(),
        session.session_id,
        ToolTransport::Mcp,
        Some("test-parent".to_string()),
        policy,
    );

    for (attempt_index, (field, value)) in [
        ("project", json!("agent:other:demo")),
        ("session_id", json!("wc_sess_0000000000000000")),
        ("recording_session_id", json!("wc_sess_0000000000000000")),
        ("ack_session_message_ids", json!([])),
        ("ack_ref", json!("wc_ack1_fixture")),
        ("context_request", json!(["webcodex.workflow"])),
        (
            "session_message_resolution",
            json!({"message_id": "wc_msg_0000000000000000", "resolution": "handled"}),
        ),
        ("expected_failure", json!(true)),
        ("expected_failure_kind", json!("anything")),
        ("result_expectation", json!("failure")),
        ("accepted_exit_codes", json!([0, 1])),
        ("assertion_name", json!("nested-assertion")),
        ("__webcodex_private", json!(true)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut arguments = serde_json::Map::new();
        arguments.insert(field.to_string(), value);
        arguments.insert("items".to_string(), json!([{"path": "README.md"}]));
        let error = host
            .invoke_tool(
                attempt_index + 1,
                "read_files".to_string(),
                Value::Object(arguments),
            )
            .await
            .expect_err("server-owned nested metadata must fail before canonical dispatch");
        assert!(error.into_message().contains(field), "{field}");
    }
    let composition = host.composition_summary(0, 0, 0, 0);
    assert_eq!(composition.nested_calls, 12);
    assert_eq!(composition.nested_failures, 12);
    assert_eq!(composition.max_in_flight, 0);
    assert_eq!(composition.nested_tool_counts.get("read_files"), Some(&12));

    let error = host
        .invoke_tool(13, "cargo_check".to_string(), json!({}))
        .await
        .expect_err("frontend-unadmitted tools must fail before composition accounting");
    assert_eq!(
        error.failure_kind(),
        OrchestrationHostFailureKind::ToolNotAdmitted
    );
    let composition = host.composition_summary(0, 0, 0, 0);
    assert_eq!(composition.nested_calls, 12);
    assert!(composition.nested_tool_counts.get("cargo_check").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_binds_exact_project_and_session_through_real_canonical_reads() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    std::fs::write(
        tmp.path().join("README.md"),
        "WebCodex Code Mode integration fixture\nToolRuntime\n",
    )
    .unwrap();

    let runtime = test_runtime();
    let client_id = "code-mode-exact";
    let exact_project =
        register_runner_project_at_path(&runtime, client_id, "demo", tmp.path()).await;
    let session = runtime.sessions.start_session(
        Some(exact_project.clone()),
        Some("code mode integration".to_string()),
    );

    // The outer caller deliberately uses the project shorthand. The root resolver
    // must bind it once and nested calls must receive the exact resolved id.
    let source = r#"
        const [status, hits] = await Promise.all([
            tools.git_status({}),
            tools.search_project_texts({
                queries: [{
                    pattern: "ToolRuntime",
                    pattern_mode: "literal",
                    result_mode: "files_with_matches",
                    limit: 10
                }]
            })
        ]);
        const files = await tools.read_files({
            items: [{path: "README.md", start_line: 1, limit: 20}]
        });
        text({
            status_success: status.success,
            search_success: hits.success,
            read_success: files.success
        });
    "#;
    let (outcome, observed) = call_code_mode_with_local_runners(
        &runtime,
        &[client_id],
        "demo",
        &session.session_id,
        source,
    )
    .await;
    assert!(outcome.success, "{outcome:?}");
    let composition = outcome
        .correlation
        .code_mode_composition
        .clone()
        .expect("outer Code Mode composition diagnostic");
    assert_eq!(composition.nested_calls, 3);
    assert_eq!(composition.input_bytes, source.len());
    assert_eq!(composition.nested_successes, 3);
    assert_eq!(composition.nested_failures, 0);
    assert!(composition.max_in_flight >= 2);
    let mut nested_tools = composition
        .nested_tool_counts
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    nested_tools.sort_unstable();
    assert_eq!(
        nested_tools,
        ["git_status", "read_files", "search_project_texts"]
    );
    assert_eq!(composition.nested_tool_counts.values().sum::<usize>(), 3);
    assert!(composition.nested_raw_result_bytes_total > composition.returned_bytes);
    assert!(composition.slot_wait_ms <= composition.duration_ms);
    assert!(composition
        .nested_tool_counts
        .keys()
        .all(|tool| super::super::code_mode::is_admitted_nested_tool(tool)));
    let diagnostic = serde_json::to_string(&composition).unwrap();
    for private in [
        "ToolRuntime",
        "README.md",
        tmp.path().to_string_lossy().as_ref(),
    ] {
        assert!(
            !diagnostic.contains(private),
            "composition diagnostic leaked nested private text: {diagnostic}"
        );
    }
    let result = outcome.result.expect("code_mode_exec ToolResult");
    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("content").is_some(), "{result:?}");
    assert!(result.output.get("stats").is_some(), "{result:?}");
    assert!(result.output.get("nested_results").is_none(), "{result:?}");
    assert!(result.output.get("tool_results").is_none(), "{result:?}");
    let mut stats_keys = result.output["stats"]
        .as_object()
        .expect("sparse Code Mode stats")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    stats_keys.sort_unstable();
    assert_eq!(
        stats_keys,
        [
            "duration_ms",
            "max_in_flight",
            "returned_bytes",
            "tool_calls"
        ]
    );
    assert!(result.output.get("code_mode_composition").is_none());
    let emitted: Value = serde_json::from_str(
        result.output["content"][0]
            .as_str()
            .expect("one text() emission"),
    )
    .unwrap();
    assert_eq!(emitted["status_success"], true);
    assert_eq!(emitted["search_success"], true);
    assert_eq!(emitted["read_success"], true);
    assert_eq!(result.output["stats"]["tool_calls"], 3);
    assert!(result.output["stats"]["max_in_flight"].as_u64().unwrap() >= 2);
    assert!(
        observed
            .iter()
            .all(|request| request.client_id == client_id),
        "{observed:?}"
    );
    assert!(
        observed
            .iter()
            .all(|request| request.cwd.as_deref() == Some(tmp.path().to_string_lossy().as_ref())),
        "{observed:?}"
    );

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(40))
        .expect("session summary");
    for tool_name in [
        "code_mode_exec",
        "git_status",
        "search_project_texts",
        "read_files",
    ] {
        let started = summary
            .events
            .iter()
            .find(|event| {
                event.kind == "tool_call_started"
                    && event.tool_name == tool_name
                    && event.logical_invocation_role.as_deref() == Some("business")
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing {tool_name} business start event: {:?}",
                    summary.events
                )
            });
        assert_eq!(started.session_id, session.session_id, "{tool_name}");
        assert_eq!(
            started.resolved_project.as_deref(),
            Some(exact_project.as_str()),
            "{tool_name}"
        );
        if tool_name != "code_mode_exec" {
            let input_summary = started
                .input_summary
                .as_ref()
                .unwrap_or_else(|| panic!("missing {tool_name} input summary"));
            assert_eq!(
                input_summary["project"], exact_project,
                "nested {tool_name} must receive the exact resolved Project id"
            );
        }
    }
    let business_invocation_ids = summary
        .events
        .iter()
        .filter(|event| {
            event.kind == "tool_call_started"
                && event.logical_invocation_role.as_deref() == Some("business")
                && [
                    "code_mode_exec",
                    "git_status",
                    "search_project_texts",
                    "read_files",
                ]
                .contains(&event.tool_name.as_str())
        })
        .filter_map(|event| event.logical_invocation_id.as_deref())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        business_invocation_ids.len(),
        4,
        "outer and each canonical child must retain independent invocation evidence"
    );
    let outer_start = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_started"
                && event.tool_name == "code_mode_exec"
                && event.logical_invocation_role.as_deref() == Some("business")
        })
        .unwrap();
    let audit = outer_start
        .input_summary
        .as_ref()
        .expect("outer audit summary");
    assert_eq!(audit["project"], "demo");
    assert_eq!(audit["source_bytes"], source.len());
    assert!(!audit.to_string().contains("ToolRuntime"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_rejects_nested_target_override_before_runner_dispatch() {
    let root_a = tempfile::tempdir().unwrap();
    let root_b = tempfile::tempdir().unwrap();
    std::fs::write(root_a.path().join("README.md"), "alpha\n").unwrap();
    std::fs::write(root_b.path().join("README.md"), "bravo\n").unwrap();
    let runtime = test_runtime();
    let project_a =
        register_runner_project_at_path(&runtime, "code-mode-a", "alpha", root_a.path()).await;
    let project_b =
        register_runner_project_at_path(&runtime, "code-mode-b", "bravo", root_b.path()).await;
    let session = runtime.sessions.start_session(
        Some(project_a.clone()),
        Some("code mode target guard".to_string()),
    );
    let source = format!(
        "await tools.read_files({{project: {project_b:?}, items: [{{path: 'README.md'}}]}});"
    );
    let (outcome, observed) = call_code_mode_with_local_runners(
        &runtime,
        &["code-mode-a", "code-mode-b"],
        "alpha",
        &session.session_id,
        &source,
    )
    .await;
    let result = outcome.result.expect("outer ToolResult");
    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("code mode execution failed"));
    assert!(
        result.output["message"]
            .as_str()
            .unwrap_or_default()
            .contains("server-owned field `project`"),
        "{result:?}"
    );
    assert!(
        observed.is_empty(),
        "override dispatched Runner work: {observed:?}"
    );
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .expect("session summary");
    assert!(!summary
        .events
        .iter()
        .any(|event| event.tool_name == "read_files"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_failure_detail_is_bounded_without_persisting_source_derived_text() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let client_id = "code-mode-error-privacy";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", tmp.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project), Some("code mode error privacy".to_string()));
    let (outcome, observed) = call_code_mode_with_local_runners(
        &runtime,
        &[client_id],
        "demo",
        &session.session_id,
        "throw('PRIVATE_RUNTIME_DETAIL_'.repeat(2000));",
    )
    .await;
    assert!(observed.is_empty());
    let result = outcome.result.expect("outer ToolResult");
    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("code mode execution failed"));
    let detail = result.output["message"]
        .as_str()
        .expect("bounded runtime detail");
    assert!(detail.starts_with("PRIVATE_RUNTIME_DETAIL_"));
    assert!(detail.len() <= super::super::code_mode::MAX_MODEL_ERROR_BYTES);
    assert_eq!(result.output["failure_kind"], "runtime_error");
    assert!(result.output.get("child_failure").is_none());
    assert_eq!(
        result.output["recovery"]["retry_same_call_unchanged"],
        false
    );
    assert!(result.output["recovery"]["actions"]
        .as_array()
        .unwrap()
        .contains(&json!("fix_code_mode_source")));

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .expect("session summary");
    let finished = summary
        .events
        .iter()
        .find(|event| {
            event.kind == "tool_call_finished"
                && event.tool_name == "code_mode_exec"
                && event.logical_invocation_role.as_deref() == Some("business")
        })
        .expect("business finish event");
    assert_eq!(
        finished.error_message_summary.as_deref(),
        Some("code mode execution failed")
    );
    assert!(!format!("{finished:?}").contains("PRIVATE_RUNTIME_DETAIL_"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn code_mode_does_not_admit_effectful_or_recursive_tools() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "code-mode-effects", "demo", tmp.path()).await;
    for (label, source) in [
        (
            "run_shell",
            "await tools.run_shell({command: 'echo forbidden'});",
        ),
        (
            "recursive",
            "await tools.code_mode_exec({source: `text('nested')`});",
        ),
        (
            "wait_for_job_terminal",
            "await tools.wait_for_job_terminal({job_id: 'wc_job_forbidden', idempotency_key: 'forbidden'});",
        ),
        (
            "present_job_terminal_continuation",
            "await tools.present_job_terminal_continuation({wait_id: 'wc_job_wait_q6urq6urq6urq6ur'});",
        ),
        (
            "job_terminal_continuation_bind",
            "await tools.job_terminal_continuation_bind({wait_id: 'wc_job_wait_q6urq6urq6urq6ur', binding_id: 'wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg'});",
        ),
        (
            "job_terminal_continuation_state",
            "await tools.job_terminal_continuation_state({wait_id: 'wc_job_wait_q6urq6urq6urq6ur', binding_id: 'wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg'});",
        ),
        (
            "job_terminal_continuation_prepare",
            "await tools.job_terminal_continuation_prepare({wait_id: 'wc_job_wait_q6urq6urq6urq6ur', binding_id: 'wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg'});",
        ),
        (
            "job_terminal_continuation_finish",
            "await tools.job_terminal_continuation_finish({wait_id: 'wc_job_wait_q6urq6urq6urq6ur', binding_id: 'wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg', attempt_id: 'wc_job_delivery_ZmZmZmZmZmZmZmZm', outcome: 'dispatch_accepted'});",
        ),
        (
            "job_terminal_continuation_unbind",
            "await tools.job_terminal_continuation_unbind({wait_id: 'wc_job_wait_q6urq6urq6urq6ur', binding_id: 'wc_host_binding_qqqqqqqqqqqqqqqqqqqqqg'});",
        ),
        (
            "observe_jobs",
            "await tools.observe_jobs({items: [{job_id: 'wc_job_forbidden'}]});",
        ),
    ] {
        let session = runtime
            .sessions
            .start_session(Some(project.clone()), Some(format!("code mode {label}")));
        let (outcome, observed) = call_code_mode_with_local_runners(
            &runtime,
            &["code-mode-effects"],
            "demo",
            &session.session_id,
            source,
        )
        .await;
        let result = outcome.result.expect("outer ToolResult");
        assert!(!result.success, "{label}: {result:?}");
        assert!(
            observed.is_empty(),
            "{label} dispatched Runner work: {observed:?}"
        );
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(20))
            .expect("session summary");
        assert!(summary.events.iter().all(|event| {
            event.tool_name != "run_shell"
                && !(event.tool_name == "code_mode_exec"
                    && event
                        .input_summary
                        .as_ref()
                        .and_then(|value| value.get("source"))
                        .is_some())
        }));
        let outer_starts = summary
            .events
            .iter()
            .filter(|event| {
                event.tool_name == "code_mode_exec" && event.kind == "tool_call_started"
            })
            .collect::<Vec<_>>();
        assert_eq!(outer_starts.len(), 2, "{label}: {outer_starts:?}");
        assert!(
            outer_starts.iter().all(|event| {
                event
                    .input_summary
                    .as_ref()
                    .and_then(|value| value.get("source"))
                    .is_none()
            }),
            "{label}: JavaScript source entered durable audit evidence"
        );
        assert!(
            outer_starts
                .iter()
                .any(|event| { event.logical_invocation_role.as_deref() == Some("recorder") }),
            "{label}"
        );
        assert!(
            outer_starts
                .iter()
                .any(|event| { event.logical_invocation_role.as_deref() == Some("business") }),
            "{label}"
        );
    }
}
