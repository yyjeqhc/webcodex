use super::*;
use webcodex_core::project_format::{
    ProjectFormatAdapter, ProjectFormatFileWitness, ProjectFormatPlan, ProjectFormatProfile,
    ProjectFormatRequest,
};
use webcodex_core::runner_operation::{RunnerJobOperation, RunnerOperation};

#[derive(Debug, Default)]
struct FormatReceipts(std::sync::Mutex<Vec<RetainedJobReceipt>>);

impl JobReceiptStore for FormatReceipts {
    fn upsert(&self, receipt: &RetainedJobReceipt) -> Result<(), String> {
        self.0.lock().unwrap().push(receipt.clone());
        Ok(())
    }
    fn load(&self, _now: i64) -> Result<Vec<RetainedJobReceipt>, String> {
        Ok(self.0.lock().unwrap().clone())
    }
    fn prune(&self, _now: i64) -> Result<(), String> {
        Ok(())
    }
}

async fn registry() -> RunnerRegistry {
    RunnerRegistry::with_job_receipt_store(
        Arc::new(NoopRunnerRegistryTelemetry),
        Arc::new(FormatReceipts::default()),
    )
    .await
}

async fn register(registry: &RunnerRegistry, instance: &str, base: bool, go: bool) {
    let mut registration =
        current_runner_registration(runner_registration("format-runner", instance, Vec::new()));
    registration.capabilities.project_format_v1 = base;
    registration.capabilities.project_format_go_v1 = go;
    registry.register(registration).await.unwrap();
    crate::test_support::apply_project_inventory_snapshot(
        registry,
        "format-runner",
        instance,
        vec![project_summary("demo", "/tmp/demo")],
    )
    .await;
}

fn request(adapter: ProjectFormatAdapter, file: &str) -> ProjectFormatRequest {
    ProjectFormatRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter,
        files: vec![file.into()],
    }
}

fn plan(request: ProjectFormatRequest, profile: ProjectFormatProfile) -> ProjectFormatPlan {
    ProjectFormatPlan {
        files: request
            .files
            .iter()
            .map(|path| ProjectFormatFileWitness {
                path: path.clone(),
                bytes: 20,
                sha256: "1".repeat(64),
                identity_digest: "2".repeat(64),
            })
            .collect(),
        request,
        recipe_root: ".".into(),
        root_digest: "3".repeat(64),
        manifest_digest: "4".repeat(64),
        profile,
    }
}

async fn poll(registry: &RunnerRegistry, instance: &str) -> Option<RunnerRequest> {
    registry
        .poll(RunnerPollRequest {
            client_id: "format-runner".into(),
            runner_instance_id: instance.into(),
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn project_format_go_planning_requires_additive_capability_before_dispatch() {
    for adapter in [ProjectFormatAdapter::Go, ProjectFormatAdapter::Auto] {
        for supported in [false, true] {
            let registry = registry().await;
            register(&registry, "inst", true, supported).await;
            let request = request(adapter, "main.go");
            let result = registry
                .enqueue_project_format_plan(
                    "format-runner".into(),
                    request.clone(),
                    Some(&auth_context(None, true)),
                )
                .await;
            if supported {
                let (id, instance, _rx) = result.unwrap();
                assert_eq!(instance, "inst");
                let polled = poll(&registry, "inst").await.unwrap();
                assert_eq!(polled.request_id, id);
                assert!(matches!(polled.decode_operation().unwrap(),
                    RunnerOperation::PlanProjectFormat(actual) if actual == request));
            } else {
                let error = result.unwrap_err();
                assert!(error.contains("project_format_go_v1"), "{error}");
                assert!(error.contains("Runner `format-runner`"), "{error}");
                assert!(error.contains("upgrade that Runner"), "{error}");
                assert!(poll(&registry, "inst").await.is_none());
            }
        }
    }
}

#[tokio::test]
async fn project_format_go_still_requires_base_capability() {
    let registry = registry().await;
    register(&registry, "inst", false, true).await;
    let error = registry
        .enqueue_project_format_plan(
            "format-runner".into(),
            request(ProjectFormatAdapter::Go, "main.go"),
            Some(&auth_context(None, true)),
        )
        .await
        .unwrap_err();
    assert!(error.contains("project_format_v1"), "{error}");
    assert!(poll(&registry, "inst").await.is_none());
}

#[tokio::test]
async fn project_format_legacy_runner_retains_rust_and_python_planning() {
    for (adapter, file) in [
        (ProjectFormatAdapter::Rust, "main.rs"),
        (ProjectFormatAdapter::Python, "main.py"),
    ] {
        let registry = registry().await;
        register(&registry, "inst", true, false).await;
        let (_id, _instance, _rx) = registry
            .enqueue_project_format_plan(
                "format-runner".into(),
                request(adapter, file),
                Some(&auth_context(None, true)),
            )
            .await
            .unwrap();
        assert!(poll(&registry, "inst").await.is_some());
    }
}

async fn start_format(
    registry: &RunnerRegistry,
    plan: ProjectFormatPlan,
    instance: &str,
) -> Result<ShellJobInfo, String> {
    registry
        .start_job_with_metadata_for_access(
            ShellJobOpRequest {
                login: false,
                op: "start".into(),
                client_id: Some("format-runner".into()),
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
                project_id: Some("agent:format-runner:demo".into()),
                project_cwd: Some(".".into()),
                purpose: Some("format".into()),
                shell: Some("direct_argv".into()),
                visibility: ShellJobVisibility::Public,
                structured_execution: Some(StructuredJobExecution::ProjectFormat {
                    plan,
                    runner_instance_id: instance.into(),
                }),
                ..Default::default()
            },
            None,
            None,
        )
        .await
}

#[tokio::test]
async fn project_format_go_typed_admission_gates_auto_profile_and_preserves_plan() {
    for supported in [false, true] {
        let registry = registry().await;
        register(&registry, "inst", true, supported).await;
        let plan = plan(
            request(ProjectFormatAdapter::Auto, "main.go"),
            ProjectFormatProfile::Go {},
        );
        let result = start_format(&registry, plan.clone(), "inst").await;
        if supported {
            let job = result.unwrap();
            assert_eq!(job.kind, "project_format");
            let polled = poll(&registry, "inst").await.unwrap();
            let RunnerOperation::Job(RunnerJobOperation::StartFormat(actual)) =
                polled.decode_operation().unwrap()
            else {
                panic!("expected typed StartFormat");
            };
            assert_eq!(actual.plan, plan);
        } else {
            let error = result.unwrap_err();
            assert!(error.contains("project_format_go_v1"), "{error}");
            assert!(registry.list_jobs(Some(10)).await.is_empty());
            assert!(poll(&registry, "inst").await.is_none());
        }
    }
}

#[tokio::test]
async fn project_format_go_typed_admission_keeps_exact_instance_fence() {
    let registry = registry().await;
    register(&registry, "old", true, true).await;
    let (_, planned_instance, _rx) = registry
        .enqueue_project_format_plan(
            "format-runner".into(),
            request(ProjectFormatAdapter::Go, "main.go"),
            Some(&auth_context(None, true)),
        )
        .await
        .unwrap();
    assert!(poll(&registry, "old").await.is_some());
    registry
        .set_last_seen_for_test("format-runner", now_ts() - RUNNER_ONLINE_WINDOW_SECS - 1)
        .await;
    register(&registry, "replacement", true, true).await;
    let plan = plan(
        request(ProjectFormatAdapter::Go, "main.go"),
        ProjectFormatProfile::Go {},
    );
    let error = start_format(&registry, plan, &planned_instance)
        .await
        .unwrap_err();
    assert_eq!(error, "format_runner_replaced");
    assert!(registry.list_jobs(Some(10)).await.is_empty());
    assert!(poll(&registry, "replacement").await.is_none());
}

#[tokio::test]
async fn project_format_go_typed_admission_still_requires_base_capability() {
    let registry = registry().await;
    register(&registry, "inst", false, true).await;
    let plan = plan(
        request(ProjectFormatAdapter::Go, "main.go"),
        ProjectFormatProfile::Go {},
    );
    let error = start_format(&registry, plan, "inst").await.unwrap_err();
    assert!(error.contains("project_format_v1"), "{error}");
    assert!(registry.list_jobs(Some(10)).await.is_empty());
    assert!(poll(&registry, "inst").await.is_none());
}

#[tokio::test]
async fn project_format_legacy_runner_retains_rust_and_python_typed_admission() {
    for (adapter, file, profile) in [
        (
            ProjectFormatAdapter::Rust,
            "main.rs",
            ProjectFormatProfile::Rust {
                edition: "2021".into(),
            },
        ),
        (
            ProjectFormatAdapter::Python,
            "main.py",
            ProjectFormatProfile::Python {
                target_version: "py311".into(),
            },
        ),
    ] {
        let registry = registry().await;
        register(&registry, "inst", true, false).await;
        let plan = plan(request(adapter, file), profile);
        start_format(&registry, plan.clone(), "inst").await.unwrap();
        let polled = poll(&registry, "inst").await.unwrap();
        assert!(matches!(polled.decode_operation().unwrap(),
            RunnerOperation::Job(RunnerJobOperation::StartFormat(actual)) if actual.plan == plan));
    }
}
