use super::*;
use crate::tool_runtime::return_timing::ToolReturnTimingPolicy;
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
            project_go_single_module_v1: true,
            project_validation_package_scope_v1: true,
            project_all_packages_v1: true,
            project_validation_test_options_v1: true,
            project_validation_python_pytest_v1: true,
            structured_go_test_json: true,
            structured_go_test_tool: true,
            structured_go_test_packages: true,
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
    super::reply_project_validation_plan(
        runtime,
        "project-validation",
        "agent-proj",
        backend,
        action,
    )
    .await
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
        scope: packages.map(|packages| ProjectValidationScope {
            packages,
            all_packages: false,
        }),
        dependency_policy: None,
        test: None,
        timeout_secs: Some(60),
    }
}
#[tokio::test]
async fn project_validation_dispatcher_applies_trusted_handoff_cap() {
    let runtime = setup(1).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_cargo_tool(
                    call(ProjectValidationAction::Check, None),
                    None,
                    Some(&auth_context(None, true)),
                    Some(4),
                )
                .await
        }
    });
    let (request, job_id) = reply_plan(&runtime, "rust", ProjectValidationAction::Check).await;
    let validation = request
        .job_context
        .as_ref()
        .and_then(|context| context.validation.as_ref())
        .expect("project validation metadata");
    assert_eq!(validation.sync_wait_secs, 4);
    assert_eq!(
        validation.effective_timeout_secs, 60,
        "trusted return policy must not shorten execution lifetime"
    );
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "running",
            "Checking project validation handoff\n",
            "",
            None,
            running_progress("check"),
            false,
        ))
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert!(
        matches!(
            result.output["execution_state"].as_str(),
            Some("queued" | "running")
        ),
        "handoff may race the Runner running update: {result:?}"
    );
    assert_eq!(result.output["sync_wait_secs"], 4);
    assert_eq!(result.output["effective_timeout_secs"], 60);
    assert_eq!(
        result.output["continuation"]["arguments"]["items"][0]["job_id"],
        job_id
    );
}

#[tokio::test]
async fn project_validation_governance_preserves_trusted_handoff_cap() {
    let runtime = setup(1).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let auth = auth_context(None, true);
            let (result, _, _) = runtime
                .dispatch_with_auth_transport_options_and_metadata_with_recording_mode_and_context_with_result_projection(
                    call(ProjectValidationAction::Check, None),
                    Some(&auth),
                    crate::tool_runtime::sessions::SessionTransport::Api,
                    Default::default(),
                    None,
                    true,
                    Vec::new(),
                    Default::default(),
                    Default::default(),
                    ToolReturnTimingPolicy::handoff_max_secs(4),
                )
                .await;
            result
        }
    });

    let (request, job_id) = reply_plan(&runtime, "rust", ProjectValidationAction::Check).await;
    let validation = request
        .job_context
        .as_ref()
        .and_then(|context| context.validation.as_ref())
        .expect("project validation metadata");
    assert_eq!(
        validation.sync_wait_secs, 4,
        "trusted return policy must reach project_validate through governance and routing"
    );
    assert_eq!(
        validation.effective_timeout_secs, 60,
        "return timing must not shorten project validation execution lifetime"
    );

    runtime
        .runner_registry
        .update_job(cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "running",
            "Checking project validation governance handoff\n",
            "",
            None,
            running_progress("check"),
            false,
        ))
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["sync_wait_secs"], 4);
    assert_eq!(result.output["effective_timeout_secs"], 60);
    assert_eq!(
        result.output["continuation"]["arguments"]["items"][0]["job_id"],
        job_id
    );
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
    assert_eq!(
        result.output["source_state"]["observed_mutation_fence"],
        "uncrossed"
    );
    assert_eq!(result.output["source_state"]["freshness"], "unproven");
    assert!(result.output.get("backend").is_none());
    assert!(result.output.get("action").is_none());
    assert!(result.output.get("diagnostics").is_none());
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
            project_all_packages_v1: false,
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

#[tokio::test]
async fn project_validation_all_packages_requires_additive_runner_capability() {
    let runtime = runtime_with_agent_project("project-validation");
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            project_validation_v1: true,
            project_validation_package_scope_v1: true,
            project_all_packages_v1: false,
            ..Default::default()
        },
    )
    .await;

    let mut call = call(ProjectValidationAction::Check, None);
    let ToolCall::ProjectValidate { scope, .. } = &mut call else {
        unreachable!()
    };
    *scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });

    let result = runtime
        .dispatch_with_auth(call, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("project_all_packages_v1"));
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
}

fn call_with_test(test: ProjectValidationTestOptions) -> ToolCall {
    let mut call = call(ProjectValidationAction::Test, None);
    if let ToolCall::ProjectValidate { test: options, .. } = &mut call {
        *options = Some(test);
    }
    call
}

#[tokio::test]
async fn project_validation_test_options_reject_before_any_plan_on_old_runner() {
    let runtime = runtime_with_agent_project("project-validation");
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            project_validation_v1: true,
            project_validation_package_scope_v1: true,
            project_all_packages_v1: true,
            ..Default::default()
        },
    )
    .await;
    for test in [
        ProjectValidationTestOptions::default(),
        ProjectValidationTestOptions {
            filter: Some("selected".into()),
            ..Default::default()
        },
        ProjectValidationTestOptions {
            require_tests: Some(false),
            ..Default::default()
        },
        ProjectValidationTestOptions {
            min_tests: Some(3),
            ..Default::default()
        },
    ] {
        let result = runtime
            .dispatch_with_auth(call_with_test(test), Some(&auth_context(None, true)))
            .await;
        assert!(!result.success);
        assert!(
            result
                .error
                .as_deref()
                .unwrap()
                .contains("project_validation_test_options_v1"),
            "{result:?}"
        );
        assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
        assert!(probe_patch_agent_request(&runtime, "project-validation")
            .await
            .is_none());
    }
}

#[tokio::test]
async fn project_validation_filtered_test_count_policy_survives_same_job_handoff() {
    for (backend, filter, stdout, minimum, required, expected) in [
        ("rust", "selected", "running 2 tests\ntest selected_one ... ok\ntest selected_two ... ok\ntest result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n", Some(3), true, false),
        ("go", "^Test", "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestOne\"}\n{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Test\":\"TestOne\"}\n{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestTwo\"}\n{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Test\":\"TestTwo\"}\n{\"Action\":\"pass\",\"Package\":\"example/pkg\"}\n", Some(3), false, false),
        ("rust", "absent", "running 0 tests\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out\n", None, false, true),
        ("rust", "selected", "running 1 test\ntest selected_one ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n", Some(1), true, true),
    ] {
        let runtime = setup(1).await;
        let task = tokio::spawn({ let runtime = runtime.clone(); async move {
            runtime.dispatch_with_auth(call_with_test(ProjectValidationTestOptions {
                filter:Some(filter.into()), require_tests:Some(required), min_tests:minimum,
            }), Some(&auth_context(None,true))).await
        }});
        let (request, job_id) = reply_plan(&runtime, backend, ProjectValidationAction::Test).await;
        let metadata = request.job_context.as_ref().unwrap().validation.as_ref().unwrap();
        assert_eq!(metadata.minimum_tests, minimum);
        assert_eq!(metadata.require_tests, Some(required));
        assert!(metadata.steps[0].args.iter().any(|arg| arg==filter));
        let encoded = serde_json::to_vec(metadata).unwrap();
        assert_eq!(serde_json::from_slice::<crate::runner_protocol::ShellJobValidationMetadata>(&encoded).unwrap(), *metadata);
        let pending = task.await.unwrap();
        assert_eq!(pending.output["continuation"]["arguments"]["items"][0]["job_id"], job_id);
        runtime.runner_registry.update_job(cargo_test_update("project-validation", &request.request_id, &job_id,
            "completed", stdout, "", Some(0), completed_progress(), true)).await.unwrap();
        let status = runtime.job_status_for_auth(job_id, false, None).await;
        assert_eq!(status.output["validation"]["passed"], expected, "backend={backend} {status:?}");
        assert!(probe_patch_agent_request(&runtime, "project-validation").await.is_none());
    }
}

#[tokio::test]
async fn project_validation_rejects_test_policy_on_non_test_before_planning() {
    let runtime = setup(1).await;
    let mut call = call_with_test(ProjectValidationTestOptions::default());
    if let ToolCall::ProjectValidate { action, .. } = &mut call {
        *action = ProjectValidationAction::Check;
    }
    let result = runtime
        .dispatch_with_auth(call, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result.error.as_deref().unwrap().contains("action=test"));
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
}

#[tokio::test]
async fn project_validation_readonly_adapters_preserve_source_fence_through_handoff() {
    for (backend, action, adapter, stdout) in [
        ("rust", ProjectValidationAction::FormatCheck, "cargo_fmt", ""),
        ("rust", ProjectValidationAction::Test, "cargo_test", "running 1 test\ntest example ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"),
        ("go", ProjectValidationAction::Check, "go_vet", ""),
        ("go", ProjectValidationAction::Test, "go_test", "{\"Action\":\"run\",\"Package\":\"example/pkg\",\"Test\":\"TestOne\"}\n{\"Action\":\"pass\",\"Package\":\"example/pkg\",\"Test\":\"TestOne\"}\n{\"Action\":\"pass\",\"Package\":\"example/pkg\"}\n"),
        ("python", ProjectValidationAction::Test, "python:pytest:test", "1 passed in 0.01s\n"),
    ] {
        for external_writer in [false, true] {
            let runtime = setup(1).await;
            let project = agent_test_project_id("project-validation");
            let session = runtime.sessions.start_session(Some(project.clone()), None);
            let before = runtime.validation_sources.capture(&project).unwrap();
            let writer = external_writer.then(|| runtime.validation_sources.begin(&project).unwrap());
            let task = tokio::spawn({
                let runtime = runtime.clone();
                let id = session.session_id.clone();
                async move { runtime.dispatch_with_auth(call(action, Some(id)), Some(&auth_context(None, true))).await }
            });
            let (request, job_id) = reply_plan(&runtime, backend, action).await;
            let metadata = request.job_context.as_ref().unwrap().validation.as_ref().unwrap();
            assert_eq!(metadata.adapter, adapter);
            assert_eq!(metadata.source_fence.as_ref().unwrap().quiescent, !external_writer);
            let pending = task.await.unwrap();
            assert_eq!(pending.output["execution_state"], "pending");
            assert_eq!(pending.output["continuation"]["arguments"]["items"][0]["job_id"], job_id);
            runtime.runner_registry.update_job(cargo_test_update(
                "project-validation", &request.request_id, &job_id, "completed", stdout, "", Some(0), completed_progress(), true,
            )).await.unwrap();
            let status = runtime.job_status_for_auth(job_id.clone(), false, None).await;
            assert_eq!(status.output["validation"]["source_state"]["observed_mutation_fence"], if external_writer { "unknown" } else { "uncrossed" }, "{status:?}");
            assert_eq!(status.output["validation"]["passed"], true, "{status:?}");
            let summary = runtime.sessions.summary(&session.session_id, None).unwrap();
            let _ = runtime.validation_summary_for_session_with_jobs(&summary, 50, Some(&auth_context(None, true))).await;
            let reconciled = runtime.sessions.summary(&session.session_id, None).unwrap();
            let terminal = reconciled.events.iter().filter(|event| event.kind == "validation_job_terminal" && event.job_id.as_deref() == Some(job_id.as_str())).collect::<Vec<_>>();
            assert_eq!(terminal.len(), 1, "adapter={adapter}");
            assert_eq!(terminal[0].tool_name, "project_validate");
            assert_eq!(terminal[0].validation_output_summary.as_ref().unwrap()["adapter"], adapter);
            let encoded = serde_json::to_value(terminal[0]).unwrap();
            assert!(encoded.to_string().contains(if external_writer { "unknown" } else { "uncrossed" }), "{encoded}");
            let after = runtime.validation_sources.capture(&project).unwrap();
            assert_eq!(after.generation, before.generation + u64::from(external_writer));
            assert!(probe_patch_agent_request(&runtime, "project-validation").await.is_none());
            drop(writer);
        }
    }
}

#[tokio::test]
async fn project_validation_python_pytest_success_and_count_failures_record_session_evidence() {
    for (stdout, exit, options, expected) in [
        ("==== 2 passed in 0.01s ====\n", 0, None, true),
        ("==== 1 failed, 1 passed in 0.01s ====\n", 1, None, false),
        ("no tests ran in 0.01s\n", 5, None, false),
        ("==== 2 skipped in 0.01s ====\n", 0, None, false),
        (
            "==== 2 skipped in 0.01s ====\n",
            0,
            Some(ProjectValidationTestOptions {
                require_tests: Some(false),
                ..Default::default()
            }),
            true,
        ),
        (
            "==== 2 passed in 0.01s ====\n",
            0,
            Some(ProjectValidationTestOptions {
                min_tests: Some(3),
                ..Default::default()
            }),
            false,
        ),
        ("progress without summary\n", 0, None, false),
        (
            "progress without summary\n",
            0,
            Some(ProjectValidationTestOptions {
                require_tests: Some(false),
                ..Default::default()
            }),
            true,
        ),
        ("==== 1 error in 0.01s ====\n", 2, None, false),
        ("==== 1 failed, 1 passed in 0.01s ====\n", 0, None, false),
        ("==== 1 passed in 0.01s ====\n", 5, None, false),
    ] {
        let runtime = setup(500).await;
        let session = runtime
            .sessions
            .start_session(Some(agent_test_project_id("project-validation")), None);
        let mut tool = call(
            ProjectValidationAction::Test,
            Some(session.session_id.clone()),
        );
        if let ToolCall::ProjectValidate { adapter, test, .. } = &mut tool {
            *adapter = Some(ProjectValidationAdapter::Python);
            *test = options;
        }
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(tool, Some(&auth_context(None, true)))
                    .await
            }
        });
        let (request, job_id) = reply_plan(&runtime, "python", ProjectValidationAction::Test).await;
        let metadata = request
            .job_context
            .as_ref()
            .unwrap()
            .validation
            .as_ref()
            .unwrap();
        assert!(metadata.is_valid());
        assert_eq!(metadata.adapter, "python:pytest:test");
        runtime
            .runner_registry
            .update_job(cargo_test_update(
                "project-validation",
                &request.request_id,
                &job_id,
                if exit == 0 { "completed" } else { "failed" },
                stdout,
                "",
                Some(exit),
                completed_progress(),
                true,
            ))
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert_eq!(result.success, expected, "{result:?}");
        assert_eq!(result.output["backend"], "python");
        assert_eq!(result.output["adapter"], "python:pytest:test");
        if exit != 0 {
            assert_eq!(result.output["exit_code"], exit);
        }
        assert_model_cargo_result_matches_schema("project_validate", &result);
        let summary = runtime
            .sessions
            .summary(&session.session_id, Some(50))
            .unwrap();
        let validation = validation_summary_for_session(&summary);
        assert_eq!(
            validation["status"],
            if exit != 0 {
                "failed"
            } else if expected {
                "passed"
            } else {
                "inconclusive"
            },
            "{validation}"
        );
    }
}

#[tokio::test]
async fn project_validation_python_pytest_handoff_observes_same_job_and_cancelled_evidence() {
    for cancel in [false, true] {
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
        let (request, job_id) = reply_plan(&runtime, "python", ProjectValidationAction::Test).await;
        let result = task.await.unwrap();
        assert_eq!(result.output["execution_state"], "pending");
        assert_eq!(
            result.output["continuation"]["arguments"]["items"][0]["job_id"],
            job_id
        );
        assert_model_cargo_result_matches_schema("project_validate", &result);
        runtime
            .runner_registry
            .update_job(cargo_test_update(
                "project-validation",
                &request.request_id,
                &job_id,
                if cancel { "stopped" } else { "completed" },
                "1 passed in 0.01s\n",
                "",
                Some(if cancel { -1 } else { 0 }),
                completed_progress(),
                true,
            ))
            .await
            .unwrap();
        let status = runtime.job_status_for_auth(job_id, false, None).await;
        if cancel {
            assert_eq!(status.output["validation"]["state"], "cancelled");
            assert!(status.output["validation"]["tests_run_count"].is_null());
        } else {
            assert_eq!(status.output["validation"]["passed"], true);
            assert_eq!(status.output["validation"]["tests_run_count"], 1);
        }
        assert!(probe_patch_agent_request(&runtime, "project-validation")
            .await
            .is_none());
    }
}

#[tokio::test]
async fn project_validation_python_pytest_old_runner_rejects_before_planning() {
    let runtime = runtime_with_agent_project("project-validation");
    register_agent(
        &runtime,
        "project-validation",
        None,
        RunnerCapabilities {
            project_validation_v1: true,
            ..Default::default()
        },
    )
    .await;
    let mut tool = call(ProjectValidationAction::Test, None);
    if let ToolCall::ProjectValidate { adapter, .. } = &mut tool {
        *adapter = Some(ProjectValidationAdapter::Python);
    }
    let result = runtime
        .dispatch_with_auth(tool, Some(&auth_context(None, true)))
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .unwrap()
        .contains("project_validation_python_pytest_v1"));
    assert!(probe_patch_agent_request(&runtime, "project-validation")
        .await
        .is_none());
    assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
}

#[tokio::test]
async fn project_validation_all_packages_dispatches_and_completes_same_wire_job() {
    for action in [
        ProjectValidationAction::Check,
        ProjectValidationAction::Test,
    ] {
        let runtime = setup(1).await;
        let mut request_call = call(action, None);
        let ToolCall::ProjectValidate { scope, .. } = &mut request_call else {
            unreachable!()
        };
        *scope = Some(ProjectValidationScope {
            packages: Vec::new(),
            all_packages: true,
        });
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(request_call, Some(&auth_context(None, true)))
                    .await
            }
        });
        let (request, job_id) = reply_plan(&runtime, "rust", action).await;
        assert_eq!(request.kind, "start_validation_job");
        assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
        assert!(request.decode_operation().is_ok());
        let metadata = request
            .job_context
            .as_ref()
            .unwrap()
            .validation
            .as_ref()
            .unwrap();
        assert!(metadata.is_valid());
        assert!(metadata.steps[0]
            .args
            .iter()
            .any(|arg| arg == "--workspace"));
        assert!(
            metadata
                .project_validation
                .as_ref()
                .unwrap()
                .request
                .scope
                .as_ref()
                .unwrap()
                .all_packages
        );
        let pending = task.await.unwrap();
        assert_eq!(
            pending.output["continuation"]["arguments"]["items"][0]["job_id"],
            job_id
        );
        let mut running = cargo_test_update(
            "project-validation",
            &request.request_id,
            &job_id,
            "running",
            "",
            "",
            None,
            running_progress(action.kind()),
            false,
        );
        running.activity = Some(ShellJobActivity {
            state: ShellJobActivityState::Working,
            phase: ShellJobActivityPhase::CargoCompiling,
            source: ShellJobActivitySource::CargoOutput,
        });
        runtime.runner_registry.update_job(running).await.unwrap();
        let stdout = if action == ProjectValidationAction::Test {
            "running 1 test\ntest selected ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n"
        } else {
            ""
        };
        runtime
            .runner_registry
            .update_job(cargo_test_update(
                "project-validation",
                &request.request_id,
                &job_id,
                "completed",
                stdout,
                "",
                Some(0),
                completed_progress(),
                true,
            ))
            .await
            .unwrap();
        let status = runtime.job_status_for_auth(job_id, false, None).await;
        assert_eq!(status.output["validation"]["passed"], true, "{status:?}");
        assert!(probe_patch_agent_request(&runtime, "project-validation")
            .await
            .is_none());
    }
}

#[tokio::test]
async fn project_validation_mismatched_evidence_profile_starts_nothing() {
    for (backend, identity) in [
        ("rust", "cargo_test"),
        ("rust", "go_vet"),
        ("go", "cargo_check"),
        ("python", "cargo_test"),
        ("rust", "unknown_check"),
    ] {
        let runtime = setup(1).await;
        let task = tokio::spawn({
            let runtime = runtime.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        call(ProjectValidationAction::Check, None),
                        Some(&auth_context(None, true)),
                    )
                    .await
            }
        });
        let request = wait_for_runner_request(&runtime, "project-validation").await;
        assert_eq!(request.kind, "plan_project_validation");
        let semantic: ProjectValidationRequest =
            serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
        let operation = webcodex_validation::project_validation_operation(
            "rust",
            webcodex_validation::SemanticCheck::Check,
            None,
            false,
        )
        .unwrap();
        let plan = ProjectValidationPlan {
            provenance: ProjectValidationProvenance {
                request: semantic,
                backend: backend.into(),
                recipe_root: ".".into(),
                root_digest: "a".repeat(64),
                manifest_digest: "b".repeat(64),
                invocation_digest: "c".repeat(64),
            },
            adapter: identity.into(),
            step: operation.build_readonly_plan().unwrap().structured_step,
            validation_target_id: operation.validation_target_id(Some(".")).unwrap(),
        };
        complete_sync_shell_lifecycle(
            &runtime,
            "project-validation",
            request.request_id,
            ShellCommandExecutionState::Completed,
            Some(0),
            &serde_json::to_string(&ProjectValidationPlanningResult::Ready { plan }).unwrap(),
            "",
            None,
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success, "{backend}/{identity}");
        assert!(runtime.runner_registry.list_jobs(Some(10)).await.is_empty());
        assert!(probe_patch_agent_request(&runtime, "project-validation")
            .await
            .is_none());
    }
}
