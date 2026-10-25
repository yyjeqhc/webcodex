use super::support::*;
use crate::runner_http::RunnerRegistry;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerPollRequest, RunnerResultPayload,
    RunnerResultRequest, ShellCommandExecutionState,
};
use crate::tool_runtime::{RuntimeInfo, ToolCall, ToolRuntime};
use serde_json::json;
use std::sync::Arc;
use webcodex_core::project_format::{
    ProjectFormatAdapter, ProjectFormatFileWitness, ProjectFormatMutationReport, ProjectFormatPlan,
    ProjectFormatPlanningResult, ProjectFormatProfile, ProjectFormatRequest,
};
use webcodex_runner_registry::{JobReceiptStore, NoopRunnerRegistryTelemetry, RetainedJobReceipt};

#[tokio::test]
async fn project_format_session_shell_guard_rejection_matches_registered_schema() {
    let runtime = setup(1, true).await;
    let project = agent_test_project_id(CLIENT);
    let session = runtime.sessions.start_session_with_guards(
        Some(project.clone()),
        None,
        webcodex_core::workflow_session_contract::SessionMode::Normal,
        crate::tool_runtime::sessions::SessionGuards {
            deny_write_tools: false,
            deny_shell_tools: true,
        },
    );
    let call = ToolCall::ProjectFormat {
        project,
        session_id: Some(session.session_id.clone()),
        cwd: None,
        adapter: Some(ProjectFormatAdapter::Rust),
        files: vec!["src/main.rs".into()],
        timeout_secs: Some(60),
    };
    let result = runtime
        .dispatch_with_auth(call, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], json!("session_guard_denied"));
    assert_eq!(result.output["guard"], json!("deny_shell_tools"));
    assert_eq!(result.output["command_started"], json!(false));
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
    let schema = crate::tool_runtime::registry::output_schema_for_tool("project_format");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .expect("the real kernel shell-guard rejection must match project_format output schema");
}

#[test]
fn project_format_retain_utf8_failure_codes() {
    assert!(super::super::project_format::valid_failure_code(
        "format_invalid_utf8"
    ));
    assert!(super::super::project_format::valid_failure_code(
        "format_output_invalid_utf8"
    ));
}

const CLIENT: &str = "project-format";

#[derive(Debug, Default)]
struct MemoryReceiptStore {
    rows: std::sync::Mutex<Vec<RetainedJobReceipt>>,
    fail_writes: std::sync::atomic::AtomicBool,
}

impl JobReceiptStore for MemoryReceiptStore {
    fn upsert(&self, receipt: &RetainedJobReceipt) -> Result<(), String> {
        if self.fail_writes.load(std::sync::atomic::Ordering::SeqCst) {
            return Err("injected receipt write failure".into());
        }
        let mut rows = self.rows.lock().unwrap();
        if !rows
            .iter()
            .any(|row| row.snapshot.job_id == receipt.snapshot.job_id)
        {
            rows.push(receipt.clone());
        }
        Ok(())
    }
    fn load(&self, _now: i64) -> Result<Vec<RetainedJobReceipt>, String> {
        Ok(self.rows.lock().unwrap().clone())
    }
    fn prune(&self, _now: i64) -> Result<(), String> {
        Ok(())
    }
}

async fn setup(grace_ms: u64, with_store: bool) -> ToolRuntime {
    let registry = if with_store {
        RunnerRegistry::with_job_receipt_store(
            Arc::new(NoopRunnerRegistryTelemetry),
            Arc::new(MemoryReceiptStore::default()),
        )
        .await
    } else {
        RunnerRegistry::default()
    };
    setup_with_registry(grace_ms, registry).await
}

async fn setup_with_registry(grace_ms: u64, registry: RunnerRegistry) -> ToolRuntime {
    let runtime = ToolRuntime::new(Arc::new(registry), Arc::new(RuntimeInfo::default()))
        .with_structured_execution_sync_wait(std::time::Duration::from_millis(grace_ms));
    register_agent(
        &runtime,
        CLIENT,
        None,
        RunnerCapabilities {
            project_format_v1: true,
            ..Default::default()
        },
    )
    .await;
    runtime
}

fn call() -> ToolCall {
    ToolCall::ProjectFormat {
        project: agent_test_project_id(CLIENT),
        session_id: None,
        cwd: None,
        adapter: Some(ProjectFormatAdapter::Rust),
        files: vec!["src/main.rs".into()],
        timeout_secs: Some(60),
    }
}

async fn reply_plan(runtime: &ToolRuntime) -> (crate::runner_protocol::RunnerRequest, String) {
    let request = wait_for_patch_agent_request(runtime, CLIENT).await;
    assert_eq!(request.kind, "plan_project_format");
    let semantic: ProjectFormatRequest =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(semantic.project_id, "agent-proj");
    let plan = ProjectFormatPlan {
        request: semantic,
        recipe_root: ".".into(),
        root_digest: "a".repeat(64),
        manifest_digest: "b".repeat(64),
        profile: ProjectFormatProfile::Rust {
            edition: "2021".into(),
        },
        files: vec![ProjectFormatFileWitness {
            path: "src/main.rs".into(),
            bytes: 15,
            sha256: "c".repeat(64),
            identity_digest: "d".repeat(64),
        }],
    };
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                exit_code: Some(0),
                stdout: Some(
                    serde_json::to_string(&ProjectFormatPlanningResult::Ready { plan }).unwrap(),
                ),
                stderr: Some(String::new()),
                duration_ms: Some(5),
                ..runner_result_request(CLIENT.into(), "inst".into(), request.request_id)
            },
            command_execution_state: Some(ShellCommandExecutionState::Completed),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await
        .unwrap();
    let start = wait_for_patch_agent_request(runtime, CLIENT).await;
    assert_eq!(start.kind, "start_format_job");
    let job_id = start.job_id.clone().expect("start_format_job job id");
    (start, job_id)
}

#[allow(clippy::too_many_arguments)]
async fn finish_format_job(
    runtime: &ToolRuntime,
    request: &crate::runner_protocol::RunnerRequest,
    job_id: &str,
    status: &str,
    state: ShellCommandExecutionState,
    exit_code: Option<i32>,
    error: Option<&str>,
    mutation: Option<ProjectFormatMutationReport>,
) {
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            request_id: Some(request.request_id.clone()),
            exit_code,
            duration_ms: Some(25),
            error: error.map(str::to_string),
            command_execution_state: Some(state),
            format_mutation: mutation,
            ..runner_job_update(
                CLIENT.into(),
                "inst".into(),
                job_id.into(),
                status.into(),
                true,
            )
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn project_format_fast_success_commits_and_returns_state_changed_true() {
    let runtime = setup(500, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let (request, job_id) = reply_plan(&runtime).await;
    let operation = request.decode_operation().unwrap();
    let webcodex_core::runner_operation::RunnerOperation::Job(
        webcodex_core::runner_operation::RunnerJobOperation::StartFormat(format_op),
    ) = operation
    else {
        panic!("expected typed StartFormat");
    };
    assert_eq!(format_op.plan.recipe_root, ".");
    assert_eq!(
        format_op
            .context
            .structured_execution
            .as_ref()
            .unwrap()
            .execution_source,
        "project_format"
    );

    finish_format_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        None,
        Some(ProjectFormatMutationReport::Changed),
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "expected Ok result: {:?}", result.output);
    assert_eq!(result.output["state_changed"], json!(true));
}

#[tokio::test]
async fn project_format_fast_success_noop_returns_state_changed_false() {
    let runtime = setup(500, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let (request, job_id) = reply_plan(&runtime).await;
    finish_format_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        None,
        Some(ProjectFormatMutationReport::Unchanged),
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "expected Ok result: {:?}", result.output);
    assert_eq!(result.output["state_changed"], json!(false));
}

#[tokio::test]
async fn project_format_pending_preserves_original_job_and_pending_lifecycle() {
    let runtime = setup(1, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let (_request, job_id) = reply_plan(&runtime).await;
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .expect("pending handoff must be bounded")
        .unwrap();
    assert!(
        result.success,
        "pending is not terminal success: {:?}",
        result.output
    );
    assert_eq!(result.output["execution_state"], json!("pending"));
    assert_eq!(result.output["state_changed"], json!(null));
    assert_eq!(result.output["job_id"], json!(job_id));
    assert_eq!(result.output["continuation"]["tool"], json!("observe_jobs"));
    assert_eq!(
        result.output["continuation"]["arguments"]["items"][0]["job_id"],
        json!(job_id)
    );
}

#[tokio::test]
async fn project_format_pending_observation_does_not_report_failure() {
    let runtime = setup(1, true).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call(), Some(&auth)).await }
    });
    let (_request, job_id) = reply_plan(&runtime).await;
    let pending = task.await.unwrap();
    assert_eq!(pending.output["execution_state"], json!("pending"));
    let detailed = runtime
        .job_log_for_auth(job_id.clone(), None, Some(20), Some(&auth), None, None)
        .await;
    assert!(detailed.success);
    assert_eq!(detailed.output["terminal"], json!(false));
    assert_eq!(detailed.output["state_changed"], json!(null));
    assert_eq!(
        detailed.output["failure_kind"],
        json!(null),
        "an active Job is not a known failure: {}",
        detailed.output["status"]
    );
    let call =
        ToolCall::from_tool_name("observe_jobs", json!({"items": [{"job_id": job_id}]})).unwrap();
    let mut observed = runtime.dispatch_with_auth(call, Some(&auth)).await;
    assert_eq!(
        observed.output["items"][0]["output"]["failure_kind"],
        json!(null)
    );
    super::super::observe_jobs::sparsify_observe_jobs_model_result(&mut observed);
    assert_eq!(observed.output["items"][0]["failure_kind"], json!(null));
}

#[tokio::test]
async fn project_format_active_running_and_stop_requested_observation_does_not_report_failure() {
    for status in ["running", "stop_requested"] {
        let runtime = setup(1, true).await;
        let auth = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move { runtime.dispatch_with_auth(call(), Some(&auth)).await }
        });
        let (request, job_id) = reply_plan(&runtime).await;
        let pending = task.await.unwrap();
        assert_eq!(pending.output["execution_state"], json!("pending"));

        runtime
            .runner_registry
            .update_job(RunnerJobUpdateRequest {
                request_id: Some(request.request_id.clone()),
                ..runner_job_update(
                    CLIENT.into(),
                    "inst".into(),
                    job_id.clone(),
                    status.into(),
                    false,
                )
            })
            .await
            .unwrap();

        let detailed = runtime
            .job_log_for_auth(job_id.clone(), None, Some(20), Some(&auth), None, None)
            .await;
        assert!(detailed.success);
        assert_eq!(detailed.output["terminal"], json!(false));
        assert_eq!(detailed.output["state_changed"], json!(null));
        assert_eq!(
            detailed.output["failure_kind"],
            json!(null),
            "an active Job is not a known failure: {}",
            detailed.output["status"]
        );

        let call = ToolCall::from_tool_name("observe_jobs", json!({"items": [{"job_id": job_id}]}))
            .unwrap();
        let mut observed = runtime.dispatch_with_auth(call, Some(&auth)).await;
        assert_eq!(
            observed.output["items"][0]["output"]["failure_kind"],
            json!(null)
        );
        super::super::observe_jobs::sparsify_observe_jobs_model_result(&mut observed);
        assert_eq!(observed.output["items"][0]["failure_kind"], json!(null));
    }
}

#[tokio::test]
async fn project_format_continuation_observes_authoritative_mutation_and_failure() {
    for (status, state, exit_code, error, mutation, changed, failure_kind) in [
        (
            "completed",
            ShellCommandExecutionState::Completed,
            Some(0),
            None,
            ProjectFormatMutationReport::Changed,
            json!(true),
            json!(null),
        ),
        (
            "completed",
            ShellCommandExecutionState::Completed,
            Some(0),
            None,
            ProjectFormatMutationReport::Unchanged,
            json!(false),
            json!(null),
        ),
        (
            "failed",
            ShellCommandExecutionState::OutcomeUnknown,
            Some(1),
            Some("format_write_conflict"),
            ProjectFormatMutationReport::Unknown,
            json!(null),
            json!("format_write_conflict"),
        ),
    ] {
        let runtime = setup(1, true).await;
        let auth = auth_context(None, true);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move { runtime.dispatch_with_auth(call(), Some(&auth)).await }
        });
        let (request, job_id) = reply_plan(&runtime).await;
        let pending = task.await.unwrap();
        assert!(pending.success);
        assert_eq!(pending.output["execution_state"], json!("pending"));
        finish_format_job(
            &runtime,
            &request,
            &job_id,
            status,
            state,
            exit_code,
            error,
            Some(mutation),
        )
        .await;

        let detailed = runtime
            .job_log_for_auth(job_id.clone(), None, Some(20), Some(&auth), None, None)
            .await;
        assert!(detailed.success, "{detailed:?}");
        assert_eq!(detailed.output["state_changed"], changed);
        assert_eq!(detailed.output["failure_kind"], failure_kind);

        for summary_only in [false, true] {
            let args = json!({
                "items": [{"job_id": job_id}],
                "summary_only": summary_only,
            });
            let call = ToolCall::from_tool_name("observe_jobs", args).unwrap();
            let mut observed = runtime.dispatch_with_auth(call, Some(&auth)).await;
            assert!(observed.success, "{observed:?}");
            assert_eq!(
                observed.output["items"][0]["output"]["state_changed"],
                changed
            );
            assert_eq!(
                observed.output["items"][0]["output"]["failure_kind"],
                failure_kind
            );
            let canonical_item = observed.output["items"][0].clone();
            super::super::observe_jobs::sparsify_observe_jobs_model_result(&mut observed);
            if canonical_item["output"]["command_execution_state"] == "outcome_unknown" {
                // Unknown mutation outcomes retain the complete canonical evidence.
                assert_eq!(observed.output["items"][0], canonical_item);
                assert_eq!(
                    observed.output["items"][0]["output"]["state_changed"],
                    changed
                );
                assert_eq!(
                    observed.output["items"][0]["output"]["failure_kind"],
                    failure_kind
                );
            } else {
                assert!(observed.output["items"][0].get("output").is_none());
                assert_eq!(observed.output["items"][0]["state_changed"], changed);
                assert_eq!(observed.output["items"][0]["failure_kind"], failure_kind);
            }
        }
    }
}

#[tokio::test]
async fn project_format_sparse_observation_has_single_mutation_evidence_path() {
    let runtime = setup(1, true).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call(), Some(&auth)).await }
    });
    let (request, job_id) = reply_plan(&runtime).await;
    assert_eq!(
        task.await.unwrap().output["execution_state"],
        json!("pending")
    );
    finish_format_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        None,
        Some(ProjectFormatMutationReport::Changed),
    )
    .await;

    let call =
        ToolCall::from_tool_name("observe_jobs", json!({"items": [{"job_id": job_id}]})).unwrap();
    let mut observed = runtime.dispatch_with_auth(call, Some(&auth)).await;
    super::super::observe_jobs::sparsify_observe_jobs_model_result(&mut observed);
    let item = &observed.output["items"][0];
    assert_eq!(item["state_changed"], json!(true));
    assert_eq!(item["failure_kind"], json!(null));
    assert!(
        item.get("output").is_none(),
        "sparse observation must expose one canonical mutation evidence path"
    );
}

#[tokio::test]
async fn project_format_missing_capability_rejects_without_dispatch() {
    let registry = RunnerRegistry::with_job_receipt_store(
        Arc::new(NoopRunnerRegistryTelemetry),
        Arc::new(MemoryReceiptStore::default()),
    )
    .await;
    let runtime = ToolRuntime::new(Arc::new(registry), Arc::new(RuntimeInfo::default()));
    register_agent(
        &runtime,
        CLIENT,
        None,
        RunnerCapabilities {
            project_format_v1: false,
            ..Default::default()
        },
    )
    .await;

    let result = runtime
        .dispatch_with_auth(call(), Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(
        result.output["failure_kind"],
        json!("capability_unavailable")
    );
    let schema = crate::tool_runtime::registry::output_schema_for_tool("project_format");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(&result).unwrap(),
        &schema,
    )
    .expect("real Runner capability denial must match project_format output schema");
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
}

#[tokio::test]
async fn project_format_runner_replaced_rejects_without_executing() {
    let runtime = setup(500, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, CLIENT).await;
    assert_eq!(request.kind, "plan_project_format");
    let semantic: ProjectFormatRequest =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    let plan = ProjectFormatPlan {
        request: semantic,
        recipe_root: ".".into(),
        root_digest: "a".repeat(64),
        manifest_digest: "b".repeat(64),
        profile: ProjectFormatProfile::Rust {
            edition: "2021".into(),
        },
        files: vec![ProjectFormatFileWitness {
            path: "src/main.rs".into(),
            bytes: 15,
            sha256: "c".repeat(64),
            identity_digest: "d".repeat(64),
        }],
    };

    // Simulate runner restart/replacement with new runner_instance_id before completing plan
    register_agent_with_instance(
        &runtime,
        CLIENT,
        "inst-2",
        None,
        RunnerCapabilities {
            project_format_v1: true,
            ..Default::default()
        },
    )
    .await;

    let complete_result = runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                exit_code: Some(0),
                stdout: Some(
                    serde_json::to_string(&ProjectFormatPlanningResult::Ready { plan }).unwrap(),
                ),
                stderr: Some(String::new()),
                duration_ms: Some(5),
                ..runner_result_request(CLIENT.into(), "inst".into(), request.request_id)
            },
            command_execution_state: Some(ShellCommandExecutionState::Completed),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await;
    assert!(
        complete_result.is_err(),
        "expected stale complete rejection"
    );
    assert!(
        complete_result
            .as_ref()
            .unwrap_err()
            .contains("stale or replaced"),
        "expected stale or replaced error, got: {:?}",
        complete_result
    );

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.output["failure_kind"],
        json!("format_runner_replaced")
    );
    let replacement_request = runtime
        .runner_registry
        .poll(RunnerPollRequest {
            client_id: CLIENT.to_string(),
            runner_instance_id: "inst-2".to_string(),
        })
        .await
        .unwrap();
    assert!(
        replacement_request.is_none(),
        "no StartFormat dispatch to replacement Runner"
    );
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn project_format_failed_job_returns_failure_kind_and_continuation() {
    let runtime = setup(500, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let (request, job_id) = reply_plan(&runtime).await;
    finish_format_job(
        &runtime,
        &request,
        &job_id,
        "failed",
        ShellCommandExecutionState::OutcomeUnknown,
        Some(1),
        Some("format_write_conflict"),
        Some(ProjectFormatMutationReport::Unknown),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(
        result.output["failure_kind"],
        json!("format_write_conflict")
    );
    assert_eq!(result.output["execution_state"], json!("outcome_unknown"));
    assert_eq!(result.output["state_changed"], json!(null));
    assert_eq!(result.output["job_id"], json!(job_id));
    assert!(result.output.get("continuation").is_some());
}

#[tokio::test]
async fn project_format_contradictory_completed_error_mutation_fails_closed() {
    let runtime = setup(500, true).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });

    let (request, job_id) = reply_plan(&runtime).await;
    finish_format_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        Some("format_write_conflict"),
        Some(ProjectFormatMutationReport::Changed),
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["execution_state"], json!("outcome_unknown"));
}

#[tokio::test]
async fn project_format_invalid_arguments_reject_without_dispatch() {
    let runtime = setup(500, true).await;

    // 0 files
    let zero_files = ToolCall::ProjectFormat {
        project: agent_test_project_id(CLIENT),
        session_id: None,
        cwd: None,
        adapter: Some(ProjectFormatAdapter::Rust),
        files: vec![],
        timeout_secs: Some(60),
    };
    let result = runtime
        .dispatch_with_auth(zero_files, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], json!("invalid_arguments"));

    // 9 files (max is 8)
    let too_many_files = ToolCall::ProjectFormat {
        project: agent_test_project_id(CLIENT),
        session_id: None,
        cwd: None,
        adapter: Some(ProjectFormatAdapter::Rust),
        files: (0..9).map(|i| format!("src/file_{i}.rs")).collect(),
        timeout_secs: Some(60),
    };
    let result = runtime
        .dispatch_with_auth(too_many_files, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], json!("invalid_arguments"));

    // Invalid timeout
    let invalid_timeout = ToolCall::ProjectFormat {
        project: agent_test_project_id(CLIENT),
        session_id: None,
        cwd: None,
        adapter: Some(ProjectFormatAdapter::Rust),
        files: vec!["src/main.rs".into()],
        timeout_secs: Some(0),
    };
    let result = runtime
        .dispatch_with_auth(invalid_timeout, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(result.output["failure_kind"], json!("invalid_arguments"));

    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
}

#[tokio::test]
async fn project_format_receipt_failure_stays_unknown_and_recovery_never_redispatches() {
    use std::sync::atomic::Ordering;
    for report in [
        ProjectFormatMutationReport::Changed,
        ProjectFormatMutationReport::Unchanged,
    ] {
        let store = Arc::new(MemoryReceiptStore::default());
        store.fail_writes.store(true, Ordering::SeqCst);
        let registry = RunnerRegistry::with_job_receipt_store(
            Arc::new(NoopRunnerRegistryTelemetry),
            store.clone(),
        )
        .await;
        let runtime = setup_with_registry(1, registry).await;
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                    .await
            }
        });
        let (request, job_id) = reply_plan(&runtime).await;
        assert_eq!(task.await.unwrap().output["execution_state"], "pending");
        finish_format_job(
            &runtime,
            &request,
            &job_id,
            "completed",
            ShellCommandExecutionState::Completed,
            Some(0),
            None,
            Some(report),
        )
        .await;
        let unknown = runtime.runner_registry.get_job(&job_id).await.unwrap();
        assert_eq!(
            unknown.format_mutation.unwrap_or_default().state_changed(),
            None
        );
        assert!(store.rows.lock().unwrap().is_empty());
        assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());

        store.fail_writes.store(false, Ordering::SeqCst);
        // Only the existing persistence candidate is retried at unlock. The
        // first view can precede that commit; the next view sees its durable fact.
        runtime.runner_registry.get_job(&job_id).await.unwrap();
        let known = runtime.runner_registry.get_job(&job_id).await.unwrap();
        assert_eq!(known.format_mutation, Some(report));
        assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
        assert_eq!(store.rows.lock().unwrap().len(), 1);

        let restored = RunnerRegistry::with_job_receipt_store(
            Arc::new(NoopRunnerRegistryTelemetry),
            store.clone(),
        )
        .await;
        let historical = restored.get_job(&job_id).await.unwrap();
        assert_eq!(historical.format_mutation, Some(report));
        assert_eq!(historical.status, "completed");
        assert!(restored.list_runners().await.is_empty());
    }
}

#[tokio::test]
async fn project_format_without_receipt_store_fails_closed() {
    let runtime = setup(500, false).await;
    let result = runtime
        .dispatch_with_auth(call(), Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert_eq!(
        result.output["failure_kind"],
        json!("format_execution_unavailable")
    );
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
}
