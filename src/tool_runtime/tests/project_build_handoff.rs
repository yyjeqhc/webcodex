use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerResultPayload, RunnerResultRequest,
    ShellCommandExecutionState,
};
use crate::tool_runtime::{ToolCall, ToolRuntime};
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildPlanningResult, ProjectBuildProvenance, ProjectBuildRequest,
    ProjectBuildScope,
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
            project_build_python_v1: true,
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
        dependency_policy: None,
        dependency_resolution_policy: None,
        timeout_secs: Some(60),
    }
}

async fn reply_plan(runtime: &ToolRuntime) -> (crate::runner_protocol::RunnerRequest, String) {
    let request = wait_for_patch_agent_request(runtime, CLIENT).await;
    assert_eq!(request.kind, "plan_project_build");
    let semantic: ProjectBuildRequest =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(semantic.project_id, "agent-proj");
    let backend = if semantic.adapter == ProjectBuildAdapter::Python {
        "python"
    } else {
        "rust"
    };
    let process = canonical_project_build_process(backend, &semantic).unwrap();
    let plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request: semantic,
            backend: backend.into(),
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
    let start = wait_for_patch_agent_request(runtime, CLIENT).await;
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
            format_mutation: None,
            build_artifacts: None,
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
async fn rust_project_build_offline_policy_is_fenced_before_runner_plan() {
    let runtime = runtime_with_agent_project(CLIENT);
    register_agent(
        &runtime,
        CLIENT,
        None,
        RunnerCapabilities {
            project_build_v1: true,
            project_dependency_resolution_offline_v1: false,
            ..Default::default()
        },
    )
    .await;
    let mut input = call();
    if let ToolCall::ProjectBuild {
        dependency_resolution_policy,
        ..
    } = &mut input
    {
        *dependency_resolution_policy =
            Some(webcodex_core::project_build::ProjectDependencyResolutionPolicy::Offline);
    }
    let result = runtime
        .dispatch_with_auth(input, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap()
        .contains("project_dependency_resolution_offline_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
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

#[tokio::test]
async fn project_build_all_packages_requires_additive_runner_capability() {
    let runtime = runtime_with_agent_project(CLIENT);
    register_agent(
        &runtime,
        CLIENT,
        None,
        RunnerCapabilities {
            project_build_v1: true,
            project_all_packages_v1: false,
            ..Default::default()
        },
    )
    .await;

    let mut scoped = call();
    let ToolCall::ProjectBuild { scope, .. } = &mut scoped else {
        unreachable!()
    };
    *scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
    });

    let result = runtime
        .dispatch_with_auth(scoped, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("project_all_packages_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, CLIENT).await.is_none());
}

#[tokio::test]
async fn python_project_build_success_projects_observed_artifacts_without_correctness_claims() {
    let runtime = setup(500).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let mut input = call();
            if let ToolCall::ProjectBuild { adapter, .. } = &mut input {
                *adapter = Some(ProjectBuildAdapter::Python);
            }
            runtime
                .dispatch_with_auth(input, Some(&auth_context(None, true)))
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime).await;
    assert_eq!(
        request.process.as_ref().unwrap().args,
        ["-m", "build", "--no-isolation"]
    );
    let artifacts = vec![webcodex_core::project_build::ProjectBuildArtifact {
        path: "dist/existing.whl".into(),
        bytes: 4,
        sha256: "a".repeat(64),
        mime_type: Some("application/zip".into()),
    }];
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: CLIENT.into(),
            runner_instance_id: "inst".into(),
            update_seq: None,
            job_id: job_id.clone(),
            request_id: Some(request.request_id),
            status: "completed".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(25),
            error: None,
            command_execution_state: Some(ShellCommandExecutionState::Completed),
            validation_progress: None,
            test_count_evidence: None,
            format_mutation: None,
            build_artifacts: Some(artifacts.clone()),
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["backend"], "python");
    assert_eq!(
        result.output["build_artifacts"],
        serde_json::to_value(artifacts).unwrap()
    );
    assert!(result.output.get("tests_detected").is_none());
    assert!(result.output.get("source_state").is_none());
}

#[tokio::test]
async fn python_project_build_missing_outputs_preserves_native_exit_and_explicit_failure() {
    let runtime = setup(500).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let mut input = call();
            if let ToolCall::ProjectBuild { adapter, .. } = &mut input {
                *adapter = Some(ProjectBuildAdapter::Python);
            }
            runtime
                .dispatch_with_auth(input, Some(&auth_context(None, true)))
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime).await;
    let update = serde_json::from_value(serde_json::json!({
        "client_id": CLIENT, "agent_instance_id": "inst", "job_id": job_id,
        "request_id": request.request_id, "status": "failed", "exit_code": 0,
        "error": "build_artifacts_missing", "command_execution_state": "completed", "finished": true
    }))
    .unwrap();
    runtime.runner_registry.update_job(update).await.unwrap();
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["exit_code"], 0);
    assert_eq!(result.output["execution_state"], "completed");
    assert_eq!(result.output["failure_kind"], "build_artifacts_missing");
    assert!(result.output.get("build_artifacts").is_none());
}

async fn observe_pending_python_build(missing_outputs: bool) -> serde_json::Value {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
    };
    let runtime = setup(1).await;
    let auth = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            let mut input = call();
            if let ToolCall::ProjectBuild { adapter, .. } = &mut input {
                *adapter = Some(ProjectBuildAdapter::Python);
            }
            runtime.dispatch_with_auth(input, Some(&auth)).await
        }
    });
    let (request, job_id) = reply_plan(&runtime).await;
    let pending = task.await.unwrap();
    assert_eq!(pending.output["execution_state"], "pending");
    let artifacts = serde_json::json!([{
        "path": "dist/demo.whl", "bytes": 4, "sha256": "a".repeat(64),
        "mime_type": "application/zip"
    }]);
    let update = serde_json::from_value(serde_json::json!({
        "client_id": CLIENT, "agent_instance_id": "inst", "job_id": job_id,
        "request_id": request.request_id,
        "status": if missing_outputs { "failed" } else { "completed" },
        "exit_code": 0,
        "error": if missing_outputs { Some("build_artifacts_missing") } else { None },
        "build_artifacts": if missing_outputs { serde_json::Value::Null } else { artifacts },
        "command_execution_state": "completed", "finished": true
    }))
    .unwrap();
    runtime.runner_registry.update_job(update).await.unwrap();
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "observe_jobs".into(),
                arguments: serde_json::json!({"items": [{"job_id": job_id}]}),
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    let result = outcome.result.unwrap();
    assert!(result.success, "{result:?}");
    let item = result.output["items"][0].clone();
    assert_eq!(item["job_id"], job_id);
    assert_eq!(item["terminal"], true);
    assert_eq!(item["exit_code"], 0);
    item
}

#[tokio::test]
async fn python_project_build_pending_observation_preserves_artifacts_in_model_result() {
    let item = observe_pending_python_build(false).await;
    assert_eq!(item["status"], "completed");
    assert_eq!(
        item["build_artifacts"][0]["path"], "dist/demo.whl",
        "{item}"
    );
    assert_eq!(item["build_artifacts"][0]["sha256"], "a".repeat(64));
}

#[tokio::test]
async fn python_project_build_pending_observation_preserves_artifact_failure_kind() {
    let item = observe_pending_python_build(true).await;
    assert_eq!(item["status"], "failed");
    assert_eq!(item["failure_kind"], "build_artifacts_missing", "{item}");
    assert!(item.get("build_artifacts").is_none());
}
