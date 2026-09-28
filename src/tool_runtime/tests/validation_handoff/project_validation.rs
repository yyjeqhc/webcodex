use super::*;
use webcodex_core::project_validation::*;

async fn setup(grace_ms: u64) -> ToolRuntime {
    let runtime = runtime_with_agent_project("project-validation")
        .with_validation_sync_wait(std::time::Duration::from_millis(grace_ms));
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            structured_cargo_check_packages: true,
            project_validation_v1: true,
            project_validation_package_scope_v1: true,
            structured_go_test_json: true,
            structured_go_test_tool: true,
            structured_cargo_test_count_assertion: true,
            structured_cargo_test_execution_policy: true,
            ..Default::default()
        },
    )
    .await;
    runtime
}
async fn reply_plan(
    runtime: &ToolRuntime,
    backend: &str,
    action: ProjectValidationAction,
) -> (crate::runner_protocol::RunnerRequest, String) {
    let request = wait_for_runner_request(runtime, "project-validation").await;
    assert_eq!(request.kind, "plan_project_validation");
    assert!(request.command.is_empty() && request.cwd.is_none());
    let semantic: ProjectValidationRequest =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(semantic.project_id, "agent-proj");
    let check = match action {
        ProjectValidationAction::FormatCheck => webcodex_validation::SemanticCheck::Format,
        ProjectValidationAction::Check => webcodex_validation::SemanticCheck::Check,
        ProjectValidationAction::Test => webcodex_validation::SemanticCheck::Test,
    };
    let operation = webcodex_validation::project_validation_operation(
        backend,
        check,
        semantic.scope.as_ref().map(|scope| scope.packages.clone()),
    )
    .unwrap();
    let adapter = operation.adapter();
    let step = operation.build_readonly_plan().unwrap().structured_step;
    let validation_target_id = operation.validation_target_id(Some(".")).unwrap();
    let plan = ProjectValidationPlan {
        provenance: ProjectValidationProvenance {
            request: semantic,
            backend: backend.into(),
            recipe_root: ".".into(),
            root_digest: "a".repeat(64),
            manifest_digest: "b".repeat(64),
            invocation_digest: "c".repeat(64),
        },
        adapter: adapter.tool_identity().into(),
        step,
        validation_target_id,
    };
    complete_sync_shell_lifecycle(
        runtime,
        "project-validation",
        request.request_id,
        ShellCommandExecutionState::Completed,
        Some(0),
        &serde_json::to_string(&ProjectValidationPlanningResult::Ready { plan }).unwrap(),
        "",
        None,
    )
    .await;
    poll_start_validation_job(runtime, "project-validation").await
}
fn call(action: ProjectValidationAction, session_id: Option<String>) -> ToolCall {
    call_with_scope(action, session_id, None)
}

fn call_with_scope(
    action: ProjectValidationAction,
    session_id: Option<String>,
    packages: Option<Vec<String>>,
) -> ToolCall {
    ToolCall::ProjectValidate {
        project: agent_test_project_id("project-validation"),
        session_id,
        cwd: None,
        action,
        adapter: None,
        scope: packages.map(|packages| ProjectValidationScope { packages }),
        timeout_secs: Some(60),
    }
}
#[tokio::test]
async fn project_validation_fast_rust_check_records_resolved_evidence() {
    let runtime = setup(500).await;
    let session = runtime
        .sessions
        .start_session(Some(agent_test_project_id("project-validation")), None);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let id = session.session_id.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    call(ProjectValidationAction::Check, Some(id)),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    // Registered path /tmp/agent-proj need not exist on the Server. Only the
    // Runner's planning response supplies the recipe; Server never probes it.
    let (request, job_id) = reply_plan(&runtime, "rust", ProjectValidationAction::Check).await;
    let metadata = request
        .job_context
        .as_ref()
        .unwrap()
        .validation
        .as_ref()
        .unwrap();
    assert_eq!(metadata.tool, "project_validate");
    assert_eq!(metadata.adapter, "cargo_check");
    assert!(metadata.is_valid());
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "completed",
            "",
            "Finished dev profile\n",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result);
    // project_validate currently participates in the potential-mutation fence.
    // Unknown source evidence must retain the rich receipt, even on exit 0.
    assert_eq!(
        result.output["source_state"]["observed_mutation_fence"],
        "unknown"
    );
    assert_eq!(result.output["backend"], "rust");
    assert_eq!(result.output["action"], "check");
    assert!(result.output.get("diagnostics").is_some());
    assert_eq!(result.output["adapter"], "cargo_check");
    assert_model_cargo_result_matches_schema("project_validate", &result);
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(50))
        .unwrap();
    let validation = validation_summary_for_session(&summary);
    assert_eq!(validation["status"], "passed", "{validation}");
    assert_eq!(
        validation["latest"]["identity"],
        "target:f9d553ae448eadebf9aaae0e"
    );
}
#[tokio::test]
async fn project_validation_pending_returns_same_job_and_unproven_go_count_fails_closed() {
    let runtime = setup(1).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    call(ProjectValidationAction::Test, None),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime, "go", ProjectValidationAction::Test).await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result);
    assert_eq!(result.output["execution_state"], "pending");
    assert_eq!(
        result.output["continuation"]["arguments"]["items"][0]["job_id"],
        job_id
    );
    assert_model_cargo_result_matches_schema("project_validate", &result);
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "completed",
            "{\"Action\":\"pass\",\"Package\":\"example/pkg\"}\n",
            "",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let status = runtime.job_status_for_auth(job_id, false, None).await;
    assert_eq!(status.output["validation"]["passed"], false, "{status:?}");
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["status"],
        "unproven"
    );
    assert_eq!(
        status.output["validation"]["test_count_assertion"]["reason_code"],
        "test_count_unproven"
    );
}
#[tokio::test]
async fn project_validation_capability_mismatch_starts_nothing() {
    let runtime = runtime_with_agent_project("project-validation");
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities::default(),
    )
    .await;
    let result = runtime
        .dispatch_with_auth(
            call(ProjectValidationAction::Check, None),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("project_validation_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
}

#[tokio::test]
async fn project_validation_scoped_rust_check_preserves_scope_in_job_metadata() {
    let runtime = setup(500).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    call_with_scope(
                        ProjectValidationAction::Check,
                        None,
                        Some(vec!["package-b".into(), "package-a".into()]),
                    ),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime, "rust", ProjectValidationAction::Check).await;
    let metadata = request
        .job_context
        .as_ref()
        .unwrap()
        .validation
        .as_ref()
        .unwrap();
    assert_eq!(metadata.tool, "project_validate");
    assert_eq!(metadata.adapter, "cargo_check");
    assert_eq!(
        metadata.steps[0].args,
        [
            "check",
            "--all-targets",
            "-p",
            "package-a",
            "-p",
            "package-b"
        ]
    );
    assert_eq!(
        metadata
            .project_validation
            .as_ref()
            .unwrap()
            .request
            .scope
            .as_ref()
            .unwrap()
            .packages,
        ["package-b", "package-a"]
    );
    assert_ne!(
        metadata.validation_target_id.as_deref(),
        Some("target:f9d553ae448eadebf9aaae0e")
    );
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "completed",
            "",
            "Finished dev profile\n",
            Some(0),
            completed_progress(),
            true,
        ))
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
}

#[tokio::test]
async fn project_validation_package_scope_requires_additive_runner_capability() {
    let runtime = runtime_with_agent_project("project-validation");
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            project_validation_v1: true,
            project_validation_package_scope_v1: false,
            ..Default::default()
        },
    )
    .await;
    let result = runtime
        .dispatch_with_auth(
            call_with_scope(
                ProjectValidationAction::Check,
                None,
                Some(vec!["package-a".into()]),
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap()
        .contains("project_validation_package_scope_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
}
