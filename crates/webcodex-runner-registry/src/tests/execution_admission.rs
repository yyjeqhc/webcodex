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
    execution: Option<StructuredJobExecution>,
    access: Option<&RunnerAccess>,
) -> ExecutionAdmissionError {
    registry
        .start_job_with_metadata_for_access_typed(
            ShellJobOpRequest {
                op: "start".into(),
                client_id: Some(client_id.into()),
                cwd: None,
                command: Some(if execution.is_some() { "" } else { "echo test" }.into()),
                login: false,
                timeout_secs: Some(30),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "test".into(),
            ShellJobStartMetadata {
                structured_execution: execution,
                ..Default::default()
            },
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
            Some(StructuredJobExecution::Process(process_payload())),
            Some(StructuredJobExecution::Script(script_payload())),
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
    for (step, validation, expected) in [
        (invalid_step, None, "invalid structured validation plan"),
        (
            valid_step.clone(),
            Some(ShellJobValidationMetadata {
                tool: "invalid".into(),
                kind: "check".into(),
                steps: vec![valid_step.clone()],
                effective_timeout_secs: 30,
                sync_wait_secs: 0,
                adapter: "invalid".into(),
                validation_target_id: None,
                minimum_tests: None,
                require_tests: None,
                no_run: None,
                project_validation: None,
                source_fence: None,
            }),
            "invalid structured validation metadata",
        ),
        (
            valid_step,
            None,
            "typed structured Job starts require command=\"\" and no validation plan",
        ),
    ] {
        let error = registry
            .start_job_with_metadata_for_access_typed(
                shell_job_start_request("missing", "also conflicts with a process start", Some(30)),
                "test".into(),
                ShellJobStartMetadata {
                    validation_steps: vec![step],
                    validation,
                    structured_execution: Some(StructuredJobExecution::Process(ShellProcessArgv {
                        executable: String::new(),
                        args: Vec::new(),
                    })),
                    ..Default::default()
                },
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
    let metadata = ShellJobStartMetadata {
        session_id: Some("wc_sess_plan_test".into()),
        ssh_resource: Some("build-host".into()),
        validation_steps: vec![step.clone()],
        ..Default::default()
    };
    let error = registry
        .start_job_with_metadata(
            shell_job_start_request("plan-ssh", "validation", Some(30)),
            "test".into(),
            metadata.clone(),
        )
        .await
        .unwrap_err();
    assert!(
        error.contains("project_validation_python_ruff_v1"),
        "{error}"
    );
    assert!(registry.inner.read().await.jobs_by_id.is_empty());

    registration.capabilities.project_validation_python_ruff_v1 = true;
    registry.register(registration).await.unwrap();
    let job = registry
        .start_job_with_metadata(
            shell_job_start_request("plan-ssh", "validation", Some(30)),
            "test".into(),
            metadata,
        )
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
    let registry = RunnerRegistry::default();
    let mut registration = runner_registration("plan-fields", "instance", Vec::new());
    for feature in [
        RunnerFeature::AsyncJobs,
        RunnerFeature::StructuredExecutionJobs,
        RunnerFeature::DetachedProcessJobs,
    ] {
        registration.capabilities.set(feature, true);
    }
    registry.register(registration).await.unwrap();
    for (execution, key, expected_source, expected_assertion, expected_stdin) in [
        (None, None, None, None, false),
        (
            Some(StructuredJobExecution::Process(process_payload())),
            None,
            Some("run_process"),
            Some("assertion"),
            true,
        ),
        (
            Some(StructuredJobExecution::DetachedProcess(process_payload())),
            Some("plan-detached-key"),
            Some("run_detached_process"),
            None,
            true,
        ),
    ] {
        let command = if execution.is_some() { "" } else { "echo test" };
        let job = registry
            .start_job_with_metadata(
                shell_job_start_request("plan-fields", command, Some(30)),
                "test".into(),
                ShellJobStartMetadata {
                    structured_execution: execution,
                    stdin: Some("input".into()),
                    validation_identity: Some("validation-id".into()),
                    validation_tool: Some("tool".into()),
                    assertion_name: Some("assertion".into()),
                    detached_idempotency_key: key.map(str::to_string),
                    ..Default::default()
                },
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
            .expect("one queued start");
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
                    Some("validation-id")
                );
                assert_eq!(metadata.validation_tool.as_deref(), Some("tool"));
                assert_eq!(metadata.assertion_name.as_deref(), expected_assertion);
            }
            other => panic!("incorrect execution metadata: {other:?}"),
        }
    }
    for (stdin, identity, tool, assertion) in [
        (Some("input"), None, None, None),
        (None, Some("identity"), None, None),
        (None, None, Some("tool"), None),
        (None, None, None, Some("assertion")),
    ] {
        let error = registry
            .start_job_with_metadata(
                shell_job_start_request("plan-fields", "", Some(30)),
                "test".into(),
                ShellJobStartMetadata {
                    structured_execution: Some(StructuredJobExecution::InteractiveProcess(
                        process_payload(),
                    )),
                    stdin: stdin.map(str::to_string),
                    validation_identity: identity.map(str::to_string),
                    validation_tool: tool.map(str::to_string),
                    assertion_name: assertion.map(str::to_string),
                    ..Default::default()
                },
            )
            .await
            .unwrap_err();
        assert_eq!(
            error,
            "interactive process requires subsequent keyed input and is not validation evidence"
        );
    }
    assert_eq!(registry.inner.read().await.jobs_by_id.len(), 3);
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
    let metadata = ShellJobStartMetadata {
        structured_execution: Some(StructuredJobExecution::DetachedProcess(process_payload())),
        detached_idempotency_key: Some("same-intent".into()),
        ..Default::default()
    };
    registry
        .start_job_with_metadata(
            shell_job_start_request("plan-replay", "", Some(30)),
            "test".into(),
            metadata.clone(),
        )
        .await
        .unwrap();
    registration
        .capabilities
        .set(RunnerFeature::DetachedProcessJobs, false);
    registry.register(registration).await.unwrap();
    let error = registry
        .start_job_with_metadata(
            shell_job_start_request("plan-replay", "", Some(30)),
            "test".into(),
            metadata,
        )
        .await
        .unwrap_err();
    assert!(error.contains("detached_process_jobs"), "{error}");
    assert_eq!(registry.inner.read().await.jobs_by_id.len(), 1);
}
