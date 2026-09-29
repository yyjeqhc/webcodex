use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerResultPayload, RunnerResultRequest,
    ShellCommandExecutionState,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildPlanningResult, ProjectBuildProvenance, ProjectBuildRequest,
};

const CLIENT: &str = "project-build";

async fn setup(grace_ms: u64) -> ToolRuntime {
    let runtime = runtime_with_agent_project(CLIENT)
        .with_structured_execution_sync_wait(std::time::Duration::from_millis(grace_ms));
    register_agent(
        &runtime,
        CLIENT,
        None,
        RunnerCapabilities {
            project_build_v1: true,
            ..Default::default()
        },
    )
    .await;
    runtime
}

fn call() -> ToolCall {
    ToolCall::ProjectBuild {
        project: agent_test_project_id(CLIENT),
        session_id: None,
        cwd: None,
        adapter: Some(ProjectBuildAdapter::Rust),
        scope: None,
        timeout_secs: Some(60),
    }
}

async fn wait_for_runner_request(runtime: &ToolRuntime) -> crate::runner_protocol::RunnerRequest {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(request) = probe_patch_agent_request(runtime, CLIENT).await {
            return request;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Runner request was not enqueued within 10 seconds"
        );
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
}

async fn reply_plan(runtime: &ToolRuntime) -> (crate::runner_protocol::RunnerRequest, String) {
    let request = wait_for_runner_request(runtime).await;
    assert_eq!(request.kind, "plan_project_build");
    let semantic: ProjectBuildRequest =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(semantic.project_id, "agent-proj");
    let process = canonical_project_build_process("rust", &semantic).unwrap();
    let plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request: semantic,
            backend: "rust".into(),
            recipe_root: ".".into(),
            root_digest: "a".repeat(64),
            manifest_digest: "b".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    };
    runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                client_id: CLIENT.into(),
                runner_instance_id: "inst".into(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(
                    serde_json::to_string(&ProjectBuildPlanningResult::Ready { plan }).unwrap(),
                ),
                stderr: Some(String::new()),
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(5),
                error: None,
            },
            command_execution_state: Some(ShellCommandExecutionState::Completed),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await
        .unwrap();
    let start = wait_for_runner_request(runtime).await;
    assert_eq!(start.kind, "start_build_job");
    let job_id = start.job_id.clone().expect("start_build_job job id");
    (start, job_id)
}

async fn finish_build_job(
    runtime: &ToolRuntime,
    request: &crate::runner_protocol::RunnerRequest,
    job_id: &str,
    status: &str,
    state: ShellCommandExecutionState,
    exit_code: Option<i32>,
    stderr: Option<&str>,
) {
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: CLIENT.into(),
            runner_instance_id: "inst".into(),
            update_seq: None,
            job_id: job_id.into(),
            request_id: Some(request.request_id.clone()),
            status: status.into(),
            stdout_chunk: None,
            stderr_chunk: stderr.map(str::to_string),
            log_snapshot: None,
            exit_code,
            duration_ms: Some(25),
            error: None,
            command_execution_state: Some(state),
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn project_build_fast_success_uses_typed_start_build_and_sparse_late_projection() {
    let runtime = setup(500).await;
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
        webcodex_core::runner_operation::RunnerJobOperation::StartBuild(build),
    ) = operation
    else {
        panic!("expected typed StartBuild");
    };
    assert_eq!(build.process.executable, "cargo");
    assert_eq!(build.process.args, ["build"]);
    assert_eq!(
        build
            .context
            .structured_execution
            .as_ref()
            .unwrap()
            .execution_source,
        "project_build"
    );

    finish_build_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        None,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["backend"], "rust");
    assert!(result.output.get("execution_source").is_none());
    assert!(result.output.get("process_summary").is_none());
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn project_build_pending_projection_returns_the_same_durable_job() {
    let runtime = setup(1).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(call(), Some(&auth_context(None, true)))
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime).await;
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["execution_state"], "pending");
    assert!(
        result.output.get("backend").is_none(),
        "late pending projection must keep only recovery authority, not build details"
    );
    assert_eq!(
        result.output["continuation"]["arguments"]["items"][0]["job_id"],
        job_id
    );

    finish_build_job(
        &runtime,
        &request,
        &job_id,
        "completed",
        ShellCommandExecutionState::Completed,
        Some(0),
        None,
    )
    .await;
    let job = runtime.runner_registry.get_job(&job_id).await.unwrap();
    assert_eq!(job.kind, "project_build");
}

#[tokio::test]
async fn project_build_missing_capability_starts_nothing() {
    let runtime = runtime_with_agent_project(CLIENT);
    register_agent(&runtime, CLIENT, None, RunnerCapabilities::default()).await;
    let result = runtime
        .dispatch_with_auth(call(), Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("project_build_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
}
