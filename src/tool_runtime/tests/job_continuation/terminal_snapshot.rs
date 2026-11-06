use super::*;
use std::sync::Arc;
use tokio::sync::Notify;

async fn paused_terminal(
    tool: &'static str,
    observation_failure: bool,
    unknown: bool,
    stdout: &str,
) -> (
    ToolRuntime,
    RunnerRequest,
    tokio::task::JoinHandle<ToolResult>,
    Arc<Notify>,
) {
    let client = "terminal-snapshot";
    let runtime = runtime_with_agent_project(client)
        .with_structured_execution_sync_wait(Duration::from_secs(2))
        .with_validation_sync_wait(Duration::from_secs(2));
    register_agent(
        &runtime,
        client,
        None,
        RunnerCapabilities {
            structured_cargo_test_lib: true,
            ..Default::default()
        },
    )
    .await;
    let mut arguments = match tool {
        "run_process" => json!({"executable": "printf", "args": ["terminal"]}),
        "cargo_check" => json!({"all_targets": false}),
        "cargo_test" => json!({"lib": true}),
        _ => unreachable!(),
    };
    arguments["project"] = json!(agent_test_project_id(client));
    arguments["timeout_secs"] = json!(120);
    arguments["sync_wait_secs"] = json!(2);
    let (reached, release) = runtime
        .runner_registry
        .pause_next_hidden_handoff_failure_for_test(observation_failure);
    let mut task = tokio::spawn({
        let runtime = runtime.clone();
        async move { invoke(&runtime, tool, arguments, &auth_context(None, true)).await }
    });
    let request = tokio::select! {
        result = &mut task => panic!("{tool} returned before admission: {result:?}"),
        request = wait_for_patch_agent_request(&runtime, client) => request,
    };
    let mut terminal = cargo_test_update(
        client,
        &request.request_id,
        request.job_id.as_deref().unwrap(),
        if unknown { "lost" } else { "completed" },
        stdout,
        "",
        (!unknown).then_some(0),
        completed_progress(),
        true,
    );
    if tool == "run_process" {
        terminal.validation_progress = None;
        terminal.activity = None;
        terminal.command_execution_state = Some(if unknown {
            ShellCommandExecutionState::OutcomeUnknown
        } else {
            ShellCommandExecutionState::Completed
        });
    }
    runtime.runner_registry.update_job(terminal).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), reached.notified())
        .await
        .expect("terminal observation/publication boundary");
    (runtime, request, task, release)
}

#[tokio::test]
async fn cancellation_during_terminal_log_read_cleans_up_the_original_hidden_job() {
    for tool in ["run_process", "cargo_check", "cargo_test"] {
        let (runtime, request, task, _release) = paused_terminal(tool, true, false, "done\n").await;
        let job_id = request.job_id.as_deref().unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        runtime
            .runner_registry
            .process_hidden_cleanup_intents()
            .await;
        assert!(runtime
            .runner_registry
            .get_hidden_job_for_auth(None, job_id)
            .await
            .is_err());
        assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
        assert!(probe_patch_agent_request(&runtime, &request.client_id)
            .await
            .is_none());
    }
}

#[tokio::test]
async fn validation_terminal_log_recovery_preserves_the_200_line_evidence_bound() {
    let stdout = (0..250)
        .map(|i| format!("line-{i:03}\n"))
        .collect::<String>();
    let (runtime, request, task, release) =
        paused_terminal("cargo_check", true, false, &stdout).await;
    release.notify_one();
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["stdout_truncated"], true);
    let tail = result.output["stdout_tail"].as_str().unwrap();
    assert_eq!(tail.lines().count(), 200);
    assert!(!tail.contains("line-000"));
    assert!(tail.contains("line-249"));
    assert!(result.output.get("continuation").is_none());
    assert!(runtime
        .runner_registry
        .get_hidden_job_for_auth(None, request.job_id.as_deref().unwrap())
        .await
        .is_err());
    assert!(probe_patch_agent_request(&runtime, &request.client_id)
        .await
        .is_none());
}

#[tokio::test]
async fn cancellation_during_uncertain_terminal_publication_retains_recovery_evidence() {
    let (runtime, request, task, _release) =
        paused_terminal("run_process", false, true, "partial\n").await;
    let job_id = request.job_id.as_deref().unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(!runtime
        .runner_registry
        .has_hidden_cleanup_intent_for_test(job_id));
    runtime
        .runner_registry
        .process_hidden_cleanup_intents()
        .await;
    let retained = runtime
        .runner_registry
        .get_hidden_job_for_auth(None, job_id)
        .await
        .unwrap();
    assert_eq!(retained.job_id, job_id);
    assert_eq!(
        retained.command_execution_state,
        Some(ShellCommandExecutionState::OutcomeUnknown)
    );
    assert!(runtime
        .runner_registry
        .get_job_for_auth(None, job_id)
        .await
        .is_err());
    assert!(probe_patch_agent_request(&runtime, &request.client_id)
        .await
        .is_none());
}
