use super::*;
use crate::jobs::PendingRequestEnqueueError;

fn process_payload() -> ShellProcessArgv {
    ShellProcessArgv {
        executable: "echo".into(),
        args: vec!["test".into()],
    }
}

fn script_payload() -> ShellScriptPayload {
    ShellScriptPayload {
        language: ShellScriptLanguage::Sh,
        script: "echo test".into(),
        args: Vec::new(),
    }
}

async fn rejected_job_start(
    registry: &RunnerRegistry,
    client_id: &str,
    execution: Option<JobStartExecution>,
    access: Option<&RunnerAccess>,
) -> ExecutionAdmissionError {
    let execution =
        execution.unwrap_or_else(|| JobStartRequest::shell(client_id, "echo test").execution);
    registry
        .start_job(
            JobStartRequest {
                timeout_secs: 30,
                ..JobStartRequest::new(client_id, execution)
            },
            "test".into(),
            access,
            None,
        )
        .await
        .expect_err("request must be rejected before admission")
}

#[tokio::test]
async fn unknown_runner_execution_admission_is_identifier_independent() {
    let registry = RunnerRegistry::default();
    for client_id in [
        "demo",
        "offline-demo",
        "permission-denied-demo",
        "capability_unavailable-demo",
        "interpreter_unavailable-demo",
    ] {
        let mut errors = vec![
            registry
                .enqueue_process_typed(
                    client_id.into(),
                    None,
                    process_payload(),
                    None,
                    30,
                    30,
                    "test".into(),
                )
                .await
                .err()
                .expect("unknown process Runner"),
            registry
                .enqueue_script_typed(
                    client_id.into(),
                    None,
                    script_payload(),
                    None,
                    30,
                    30,
                    "test".into(),
                )
                .await
                .err()
                .expect("unknown script Runner"),
            registry
                .enqueue_run_with_ssh_typed(
                    ShellRunRequest {
                        client_id: client_id.into(),
                        cwd: None,
                        command: "echo test".into(),
                        login: false,
                        stdin: None,
                        timeout_secs: 30,
                        wait_timeout_secs: 30,
                    },
                    "test".into(),
                    None,
                    None,
                    None,
                )
                .await
                .err()
                .expect("unknown shell Runner"),
        ];
        for execution in [
            None,
            Some(JobStartExecution::Process {
                process: process_payload(),
                stdin: None,
                correlation: JobValidationCorrelation::default(),
            }),
            Some(JobStartExecution::Script {
                script: script_payload(),
                stdin: None,
                correlation: JobValidationCorrelation::default(),
            }),
        ] {
            errors.push(rejected_job_start(&registry, client_id, execution, None).await);
        }
        for error in errors {
            assert_eq!(error.failure_kind(), "agent_offline", "{error:?}");
            assert_eq!(
                error.to_string(),
                format!("unknown shell client: {client_id}")
            );
            assert_eq!(error.recovery_job_id(), None);
            assert!(!error.is_idempotency_conflict());
        }
    }
    let inner = registry.inner.read().await;
    assert!(inner.pending_by_id.is_empty());
    assert!(inner.jobs_by_id.is_empty());
}

#[tokio::test]
async fn execution_admission_keeps_inaccessible_runner_indistinguishable_from_unknown() {
    let registry = RunnerRegistry::default();
    let client_id = "permission-denied-offline-demo";
    let access = auth_context(Some("bob"), false);
    let unknown = rejected_job_start(&registry, client_id, None, Some(&access)).await;
    let mut registration = runner_registration(client_id, "instance", Vec::new());
    registration.owner = Some("alice".into());
    registry.register(registration).await.unwrap();
    let pending_before = registry.inner.read().await.pending_by_id.len();
    let hidden = rejected_job_start(&registry, client_id, None, Some(&access)).await;
    assert_eq!(hidden, unknown);
    assert_eq!(hidden.failure_kind(), "agent_offline");
    let inner = registry.inner.read().await;
    assert_eq!(inner.pending_by_id.len(), pending_before);
    assert!(inner.jobs_by_id.is_empty());
}

#[test]
fn execution_admission_classification_ignores_display_text() {
    for message in [
        "ordinary diagnostic",
        "offline permission denied interpreter_unavailable capability_unavailable timeout",
        "arbitrary replacement wording",
    ] {
        for (error, expected) in [
            (
                ExecutionAdmissionError::invalid_request(message),
                "invalid_arguments",
            ),
            (
                ExecutionAdmissionError::capability_unavailable(message),
                "capability_unavailable",
            ),
            (
                ExecutionAdmissionError::permission_denied(message),
                "permission_denied",
            ),
            (
                ExecutionAdmissionError::target_changed(message),
                "runtime_error",
            ),
            (
                ExecutionAdmissionError::unavailable(message),
                "runtime_error",
            ),
            (
                ExecutionAdmissionError::idempotency_conflict(message),
                "idempotency_conflict",
            ),
            (
                PendingRequestEnqueueError::InvalidOperation {
                    message: message.into(),
                }
                .into(),
                "invalid_arguments",
            ),
        ] {
            assert_eq!(error.failure_kind(), expected);
            assert_eq!(error.recovery_job_id(), None);
        }
    }
    for (error, expected) in [
        (
            PendingRequestEnqueueError::RunnerOffline {
                client_id: "permission-denied".into(),
            },
            "agent_offline",
        ),
        (
            PendingRequestEnqueueError::QueueFull {
                client_id: "offline".into(),
                limit: 3,
            },
            "runtime_error",
        ),
        (PendingRequestEnqueueError::Maintenance, "runtime_error"),
    ] {
        assert_eq!(
            ExecutionAdmissionError::from(error).failure_kind(),
            expected
        );
    }
}

#[test]
fn detached_recovery_identity_is_carried_as_data_not_extracted_from_text() {
    let job_id = "wc_job_permission-denied-offline";
    let recovered = ExecutionAdmissionError::idempotency_recovery(job_id.into());
    assert_eq!(recovered.recovery_job_id(), Some(job_id));
    assert_eq!(recovered.failure_kind(), "idempotency_recovery_required");
    assert_eq!(
        recovered.to_string(),
        format!("{DETACHED_IDEMPOTENCY_RECOVERY_PREFIX}{job_id}")
    );
    let lookalike = ExecutionAdmissionError::unavailable(recovered.to_string());
    assert_eq!(lookalike.recovery_job_id(), None);
    assert_eq!(lookalike.failure_kind(), "runtime_error");
    assert!(!lookalike.is_idempotency_conflict());
}

#[tokio::test]
async fn job_start_plan_preserves_validation_error_precedence() {
    let registry = RunnerRegistry::default();
    let valid_step = ShellJobValidationStep {
        name: "check".into(),
        program: "cargo".into(),
        args: vec!["check".into()],
        env: Vec::new(),
    };
    let mut invalid_step = valid_step.clone();
    invalid_step.program = "arbitrary-program".into();
    // Invalid step content still precedes invalid adapter metadata, and both
    // precede scope validation. Conflicting execution shapes are unrepresentable.
    for (steps, expected) in [
        (vec![invalid_step], "invalid structured validation plan"),
        (vec![valid_step], "invalid structured validation metadata"),
        (Vec::new(), "invalid structured validation metadata"),
    ] {
        let metadata = ShellJobValidationMetadata {
            tool: "invalid".into(),
            kind: "check".into(),
            steps,
            effective_timeout_secs: 30,
            sync_wait_secs: 0,
            adapter: "invalid".into(),
            validation_target_id: None,
            minimum_tests: None,
            require_tests: None,
            no_run: None,
            project_validation: None,
            source_fence: None,
        };
        let error = registry
            .start_job(
                JobStartRequest {
                    cwd: Some("invalid\0cwd".into()),
                    timeout_secs: 0,
                    ..JobStartRequest::new(
                        "missing",
                        JobStartExecution::Validation {
                            plan: JobValidationPlan::Metadata(metadata),
                            ssh_resource: None,
                        },
                    )
                },
                "test".into(),
                None,
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.failure_kind(), "invalid_arguments");
        assert_eq!(error.to_string(), expected);
    }
    let inner = registry.inner.read().await;
    assert!(inner.pending_by_id.is_empty());
    assert!(inner.jobs_by_id.is_empty());
}

#[tokio::test]
async fn job_start_plan_keeps_ssh_validation_and_step_capability_admission() {
    let registry = RunnerRegistry::default();
    let mut registration = runner_registration("plan-ssh", "instance", Vec::new());
    for feature in [
        RunnerFeature::AsyncJobs,
        RunnerFeature::StructuredValidationArgv,
        RunnerFeature::SshShell,
    ] {
        registration.capabilities.set(feature, true);
    }
    registration.capabilities.project_validation_python_ruff_v1 = false;
    registry.register(registration.clone()).await.unwrap();
    let step = ShellJobValidationStep::python_ruff("check").unwrap();
    let request = JobStartRequest {
        timeout_secs: 30,
        context: JobStartContext {
            session_id: Some("wc_sess_plan_test".into()),
            ..Default::default()
        },
        ..JobStartRequest::new(
            "plan-ssh",
            JobStartExecution::Validation {
                plan: JobValidationPlan::Steps(vec![step.clone()]),
                ssh_resource: Some("build-host".into()),
            },
        )
    };
    let error = registry
        .start_job(request.clone(), "test".into(), None, None)
        .await
        .unwrap_err();
    assert_eq!(error.failure_kind(), "capability_unavailable");
    assert!(
        error
            .to_string()
            .contains("project_validation_python_ruff_v1"),
        "{error}"
    );
    assert!(registry.inner.read().await.jobs_by_id.is_empty());

    registration.capabilities.project_validation_python_ruff_v1 = true;
    registry.register(registration).await.unwrap();
    let job = registry
        .start_job(request, "test".into(), None, None)
        .await
        .unwrap();
    let request = registry
        .poll(RunnerPollRequest {
            client_id: "plan-ssh".into(),
            runner_instance_id: "instance".into(),
        })
        .await
        .unwrap()
        .expect("SSH validation must be queued");
    assert_eq!(request.kind, "start_validation_job");
    assert_eq!(
        serde_json::from_str::<Vec<ShellJobValidationStep>>(&request.command).unwrap(),
        vec![step]
    );
    assert_eq!(request.job_id.as_deref(), Some(job.job_id.as_str()));
    let context = request.job_context.unwrap();
    assert_eq!(context.ssh_resource.as_deref(), Some("build-host"));
    assert!(context.structured_execution.is_none());
}

#[tokio::test]
async fn job_start_plan_preserves_shape_specific_stdin_and_correlation() {
    let mut registration = runner_registration("plan-fields", "instance", Vec::new());
    for feature in [
        RunnerFeature::AsyncJobs,
        RunnerFeature::StructuredExecutionJobs,
        RunnerFeature::DetachedProcessJobs,
        RunnerFeature::JobProcessInput,
    ] {
        registration.capabilities.set(feature, true);
    }
    let initiator = DetachedInitiatorIdentity::internal();
    for (execution, expected_source, expected_assertion, expected_stdin, correlated) in [
        (
            JobStartRequest::shell("plan-fields", "echo test").execution,
            None,
            None,
            false,
            false,
        ),
        (
            JobStartExecution::Process {
                process: process_payload(),
                stdin: Some("input".into()),
                correlation: JobValidationCorrelation {
                    validation_identity: Some("validation-id".into()),
                    validation_tool: Some("tool".into()),
                    assertion_name: Some("assertion".into()),
                },
            },
            Some("run_process"),
            Some("assertion"),
            true,
            true,
        ),
        (
            JobStartExecution::DetachedProcess {
                process: process_payload(),
                stdin: Some("input".into()),
                idempotency_key: "plan-detached-key".into(),
                validation_identity: Some("validation-id".into()),
                validation_tool: Some("tool".into()),
            },
            Some("run_detached_process"),
            None,
            true,
            true,
        ),
        (
            JobStartExecution::InteractiveProcess(process_payload()),
            Some("run_process_interactive"),
            None,
            false,
            false,
        ),
    ] {
        // Each shape owns its fixture lifecycle; active Jobs from a previous
        // case must not consume this case's default Runner concurrency slots.
        let registry = RunnerRegistry::default();
        registry.register(registration.clone()).await.unwrap();
        crate::test_support::apply_project_inventory_snapshot(
            &registry,
            "plan-fields",
            "instance",
            vec![project_summary("demo", "/tmp/demo")],
        )
        .await;
        let job = registry
            .start_job(
                JobStartRequest {
                    timeout_secs: 30,
                    context: JobStartContext {
                        project_id: Some("agent:plan-fields:demo".into()),
                        ..Default::default()
                    },
                    ..JobStartRequest::new("plan-fields", execution)
                },
                "test".into(),
                None,
                Some(&initiator),
            )
            .await
            .unwrap();
        let request = registry
            .poll(RunnerPollRequest {
                client_id: "plan-fields".into(),
                runner_instance_id: "instance".into(),
            })
            .await
            .unwrap()
            .unwrap_or_else(|| {
                panic!(
                    "expected queued start for {expected_source:?}, kind={}",
                    job.kind
                )
            });
        assert_eq!(request.job_id.as_deref(), Some(job.job_id.as_str()));
        assert_eq!(request.stdin.is_some(), expected_stdin);
        let context = request.job_context.unwrap();
        match (expected_source, context.structured_execution) {
            (None, None) => {}
            (Some(source), Some(metadata)) => {
                assert_eq!(metadata.execution_source, source);
                assert_eq!(metadata.stdin_present, expected_stdin);
                assert_eq!(metadata.arg_count, 1);
                assert_eq!(
                    metadata.validation_identity.as_deref(),
                    correlated.then_some("validation-id")
                );
                assert_eq!(
                    metadata.validation_tool.as_deref(),
                    correlated.then_some("tool")
                );
                assert_eq!(metadata.assertion_name.as_deref(), expected_assertion);
            }
            other => panic!("incorrect execution metadata: {other:?}"),
        }
        assert_eq!(registry.inner.read().await.jobs_by_id.len(), 1);
    }
}

#[tokio::test]
async fn job_start_plan_checks_current_capabilities_before_detached_replay() {
    let registry = RunnerRegistry::default();
    let mut registration = runner_registration("plan-replay", "instance", Vec::new());
    for feature in [
        RunnerFeature::AsyncJobs,
        RunnerFeature::StructuredExecutionJobs,
        RunnerFeature::DetachedProcessJobs,
    ] {
        registration.capabilities.set(feature, true);
    }
    registry.register(registration.clone()).await.unwrap();
    let request = JobStartRequest {
        timeout_secs: 30,
        ..JobStartRequest::new(
            "plan-replay",
            JobStartExecution::DetachedProcess {
                process: process_payload(),
                stdin: None,
                idempotency_key: "same-intent".into(),
                validation_identity: None,
                validation_tool: None,
            },
        )
    };
    let initiator = DetachedInitiatorIdentity::internal();
    registry
        .start_job(request.clone(), "test".into(), None, Some(&initiator))
        .await
        .unwrap();
    registration
        .capabilities
        .set(RunnerFeature::DetachedProcessJobs, false);
    registry.register(registration).await.unwrap();
    let error = registry
        .start_job(request, "test".into(), None, Some(&initiator))
        .await
        .unwrap_err();
    assert_eq!(error.failure_kind(), "capability_unavailable");
    assert!(
        error.to_string().contains("detached_process_jobs"),
        "{error}"
    );
    assert_eq!(registry.inner.read().await.jobs_by_id.len(), 1);
}

#[test]
fn format_admission_details_are_independent_of_display_text() {
    let unavailable = ExecutionAdmissionError::format_execution_unavailable();
    assert_eq!(
        unavailable.reason_code(),
        Some("format_execution_unavailable")
    );
    assert_eq!(unavailable.failure_kind(), "runtime_error");
    let replaced = ExecutionAdmissionError::format_runner_replaced();
    assert_eq!(replaced.reason_code(), Some("format_runner_replaced"));
    let lookalike = ExecutionAdmissionError::unavailable("format_runner_replaced");
    assert_eq!(lookalike.reason_code(), None);
    let missing_async = ExecutionAdmissionError::async_jobs_unavailable("format-runner");
    assert!(missing_async.is_async_jobs_unavailable());
    assert_eq!(missing_async.failure_kind(), "capability_unavailable");
    assert!(
        !ExecutionAdmissionError::capability_unavailable(missing_async.to_string())
            .is_async_jobs_unavailable()
    );
}
