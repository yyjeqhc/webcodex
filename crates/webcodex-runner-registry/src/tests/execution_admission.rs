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
