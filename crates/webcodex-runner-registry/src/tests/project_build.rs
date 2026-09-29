use super::*;
use webcodex_core::project_build::{
    canonical_project_build_process, project_build_invocation_digest, ProjectBuildAdapter,
    ProjectBuildPlan, ProjectBuildProvenance, ProjectBuildRequest,
};
use webcodex_core::runner_operation::{RunnerJobOperation, RunnerOperation};

fn build_request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Rust,
        scope: None,
    }
}

fn build_plan() -> ProjectBuildPlan {
    let request = build_request();
    let process = canonical_project_build_process("rust", &request).unwrap();
    ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request,
            backend: "rust".into(),
            recipe_root: ".".into(),
            root_digest: "1".repeat(64),
            manifest_digest: "2".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process,
    }
}

fn registration(project_build: bool) -> RunnerRegisterRequest {
    let mut capabilities =
        crate::test_support::current_runner_capabilities(RunnerCapabilities::default());
    capabilities.project_build_v1 = project_build;
    current_runner_registration(RunnerRegisterRequest {
        computer_session_availability: None,
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: "build-runner".into(),
        runner_instance_id: "inst".into(),
        runner_protocol_generation: RUNNER_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities,
        policy: None,
    })
}

#[tokio::test]
async fn project_build_plan_requires_capability_and_stays_planning_only() {
    let registry = RunnerRegistry::default();
    let access = auth_context(None, true);
    registry.register(registration(false)).await.unwrap();

    let error = registry
        .enqueue_project_build_plan("build-runner".into(), build_request(), Some(&access))
        .await
        .unwrap_err();
    assert!(error.contains("project_build_v1"), "{error}");

    registry.register(registration(true)).await.unwrap();
    let (request_id, _rx) = registry
        .enqueue_project_build_plan("build-runner".into(), build_request(), Some(&access))
        .await
        .unwrap();
    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.request_id, request_id);
    assert_eq!(polled.kind, "plan_project_build");
    assert!(matches!(
        polled.decode_operation().unwrap(),
        RunnerOperation::PlanProjectBuild(request) if request == build_request()
    ));
}

#[tokio::test]
async fn project_build_job_handoff_emits_typed_start_build() {
    let registry = RunnerRegistry::default();
    registry.register(registration(true)).await.unwrap();

    let plan = build_plan();
    let job = registry
        .start_job_with_metadata_for_access(
            ShellJobOpRequest {
                login: false,
                op: "start".into(),
                client_id: Some("build-runner".into()),
                cwd: Some("/tmp/demo".into()),
                command: Some(String::new()),
                timeout_secs: Some(60),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".into(),
            ShellJobStartMetadata {
                project_id: Some("agent:build-runner:demo".into()),
                project_cwd: Some(".".into()),
                purpose: Some("build".into()),
                shell: Some("direct_argv".into()),
                visibility: ShellJobVisibility::Public,
                structured_execution: Some(StructuredJobExecution::ProjectBuild(plan.clone())),
                ..Default::default()
            },
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(job.kind, "project_build");

    let polled = registry
        .poll(RunnerPollRequest {
            client_id: "build-runner".into(),
            runner_instance_id: "inst".into(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(polled.kind, "start_build_job");
    let RunnerOperation::Job(RunnerJobOperation::StartBuild(build)) =
        polled.decode_operation().unwrap()
    else {
        panic!("expected typed StartBuild");
    };
    assert_eq!(build.process, plan.process);
    assert_eq!(build.provenance, plan.provenance);
    assert_eq!(
        build.context.runtime_project_id.as_deref(),
        Some("agent:build-runner:demo")
    );
}
