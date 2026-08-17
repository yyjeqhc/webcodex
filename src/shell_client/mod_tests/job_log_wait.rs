use super::*;

// ============================================================================
// Bounded `job_log`/`job_tail` waits
// ============================================================================
//
// These tests drive `job_log_for_auth` with `after_observation_token`/`wait_secs`
// against a sequenced agent-backed job. A sequenced agent (`job_state_reconciliation`)
// advances `last_update_seq` via `update_job`, so waiters observe real sequence
// advancement, log growth, and terminal transitions.

fn sequenced_job_capabilities() -> ShellClientCapabilities {
    ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        jobs: true,
        job_state_reconciliation: true,
        structured_validation_argv: true,
        ..Default::default()
    }
}

fn wait_job_update(
    instance: &str,
    job_id: &str,
    sequence: u64,
    status: &str,
    stdout_chunk: Option<&str>,
    finished: bool,
) -> ShellAgentJobUpdateRequest {
    ShellAgentJobUpdateRequest {
        client_id: "oe".to_string(),
        agent_instance_id: instance.to_string(),
        job_id: job_id.to_string(),
        request_id: None,
        update_seq: Some(sequence),
        status: status.to_string(),
        stdout_chunk: stdout_chunk.map(str::to_string),
        stderr_chunk: None,
        stdout_tail: None,
        stderr_tail: None,
        log_snapshot: None,
        exit_code: finished.then_some(0),
        duration_ms: finished.then_some(2_000),
        error: None,
        command_execution_state: None,
        validation_progress: None,
        finished,
    }
}

async fn register_sequenced(registry: &ShellClientRegistry, instance: &str) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: Some(crate::shell_protocol::ShellJobInventory {
                active_complete: true,
                jobs: Vec::new(),
            }),
            client_id: "oe".to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(sequenced_job_capabilities()),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
}

async fn start_wait_job(registry: &ShellClientRegistry) -> crate::shell_protocol::ShellJobInfo {
    register_sequenced(registry, "inst-wait").await;
    registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("oe".to_string()),
                cwd: Some("/tmp".to_string()),
                command: Some("printf 'line one\\n'".to_string()),
                timeout_secs: Some(30),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn job_log_wait_observation_update_between_calls_is_immediate() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    let token0 = job.observation_token.clone().unwrap();
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            Some("hi\n"),
            false,
        ))
        .await
        .unwrap();
    let started = tokio::time::Instant::now();
    let (info, stdout, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&token0), Some(5))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Immediate);
    assert!(wait.changed);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
    assert_eq!(stdout.as_deref(), Some("hi\n"));
    assert_ne!(info.observation_token.as_deref(), Some(token0.as_str()));
    assert_eq!(info.last_update_seq, Some(1));
}

#[tokio::test]
async fn job_log_wait_server_transition_between_calls_is_immediate() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            None,
            false,
        ))
        .await
        .unwrap();
    let token0 = registry
        .get_job(&job.job_id)
        .await
        .unwrap()
        .observation_token
        .unwrap();
    registry
        .stop_job(&job.job_id, "tester".to_string())
        .await
        .unwrap();
    let (info, _, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&token0), Some(5))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Immediate);
    assert!(wait.changed);
    assert_eq!(info.status, "stop_requested");
}

#[tokio::test]
async fn job_log_wait_epoch_mismatch_refreshes_immediately() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    let parsed = crate::job_observation::JobObservationToken::parse(
        job.observation_token.as_deref().unwrap(),
    )
    .unwrap();
    let stale = crate::job_observation::JobObservationToken::new(
        crate::job_observation::JobObservationExecutor::Agent,
        job.job_id.clone(),
        "ffffffffffffffffffffffffffffffff",
        parsed.revision,
    )
    .unwrap()
    .encode();
    let started = tokio::time::Instant::now();
    let (_, _, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&stale), Some(5))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Immediate);
    assert!(wait.changed);
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn job_log_wait_rejects_wrong_job_malformed_and_oversized_tokens() {
    let registry = ShellClientRegistry::default();
    let first = start_wait_job(&registry).await;
    let second = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".into(),
                client_id: Some("oe".into()),
                cwd: Some("/tmp".into()),
                command: Some("true".into()),
                timeout_secs: Some(30),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".into(),
        )
        .await
        .unwrap();
    let wrong = registry
        .job_log_for_auth(
            None,
            &second.job_id,
            None,
            None,
            None,
            first.observation_token.as_deref(),
            Some(1),
        )
        .await
        .unwrap_err();
    assert!(wrong.contains("different Job"));
    let malformed = registry
        .job_log_for_auth(None, &first.job_id, None, None, None, Some("bad"), Some(1))
        .await
        .unwrap_err();
    assert!(malformed.contains("malformed"));
    let oversized = "x".repeat(crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN + 1);
    let oversized = registry
        .job_log_for_auth(
            None,
            &first.job_id,
            None,
            None,
            None,
            Some(&oversized),
            Some(1),
        )
        .await
        .unwrap_err();
    assert!(oversized.contains("exceeds 192"));
}

#[tokio::test]
async fn job_log_wait_same_token_times_out_without_changed() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    let token = job.observation_token.clone().unwrap();
    let (_, _, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&token), Some(1))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Timeout);
    assert!(!wait.changed);
    assert!(!wait.terminal);
}

#[tokio::test]
async fn job_log_wait_wakes_all_waiters_without_holding_registry_mutex() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    let token = job.observation_token.clone().unwrap();
    let mut tasks = Vec::new();
    for _ in 0..2 {
        let registry = registry.clone();
        let job_id = job.job_id.clone();
        let token = token.clone();
        tasks.push(tokio::spawn(async move {
            registry
                .job_log_for_auth(None, &job_id, None, None, None, Some(&token), Some(5))
                .await
                .unwrap()
                .5
        }));
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            Some("one\n"),
            false,
        ))
        .await
        .unwrap();
    for task in tasks {
        let wait = task.await.unwrap();
        assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Updated);
        assert!(wait.changed);
    }
}

#[tokio::test]
async fn job_log_wait_stale_or_replayed_sequence_does_not_change_token() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            Some("one\n"),
            false,
        ))
        .await
        .unwrap();
    let token = registry
        .get_job(&job.job_id)
        .await
        .unwrap()
        .observation_token
        .unwrap();
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            Some("replay\n"),
            false,
        ))
        .await
        .unwrap();
    let current = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(current.observation_token.as_deref(), Some(token.as_str()));
    assert_eq!(current.last_update_seq, Some(1));
}

#[tokio::test]
async fn job_log_wait_sequenced_update_changes_token_even_when_tail_is_same() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".into(),
            agent_instance_id: "inst-wait".into(),
            update_seq: Some(1),
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: Some("same\n".into()),
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    let token = registry
        .get_job(&job.job_id)
        .await
        .unwrap()
        .observation_token
        .unwrap();
    registry
        .update_job(ShellAgentJobUpdateRequest {
            client_id: "oe".into(),
            agent_instance_id: "inst-wait".into(),
            update_seq: Some(2),
            job_id: job.job_id.clone(),
            request_id: None,
            status: "running".into(),
            stdout_chunk: None,
            stderr_chunk: None,
            stdout_tail: Some("same\n".into()),
            stderr_tail: None,
            log_snapshot: None,
            exit_code: None,
            duration_ms: None,
            error: None,
            command_execution_state: None,
            validation_progress: None,
            finished: false,
        })
        .await
        .unwrap();
    let current = registry.get_job(&job.job_id).await.unwrap();
    assert_ne!(
        current.observation_token.as_deref(),
        Some(token.as_str()),
        "Runner sequence itself remains a public diagnostic field and changed"
    );
}

#[tokio::test]
async fn job_log_wait_recovery_transition_between_calls_is_immediate() {
    let registry = ShellClientRegistry::default();
    let job = start_wait_job(&registry).await;
    registry
        .update_job(wait_job_update(
            "inst-wait",
            &job.job_id,
            1,
            "running",
            None,
            false,
        ))
        .await
        .unwrap();
    let token = registry
        .get_job(&job.job_id)
        .await
        .unwrap()
        .observation_token
        .unwrap();
    registry.reconcile_disconnect("oe", "inst-wait").await;
    let (info, _, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&token), Some(5))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Immediate);
    assert!(wait.changed);
    assert_eq!(info.status, "recovering");
    assert_eq!(info.recovery_state.as_deref(), Some("recovering"));
}

#[tokio::test]
async fn job_log_wait_legacy_update_between_calls_and_noop_replacement() {
    let registry = ShellClientRegistry::default();
    let capabilities = ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        jobs: true,
        ..Default::default()
    };
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "legacy".to_string(),
            agent_instance_id: "legacy-inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(capabilities),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap();
    let job = registry
        .start_job(
            ShellJobOpRequest {
                op: "start".to_string(),
                client_id: Some("legacy".to_string()),
                cwd: Some("/tmp".to_string()),
                command: Some("printf legacy".to_string()),
                timeout_secs: Some(30),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "tester".to_string(),
        )
        .await
        .unwrap();
    let token0 = job.observation_token.clone().unwrap();
    let update = || ShellAgentJobUpdateRequest {
        client_id: "legacy".to_string(),
        agent_instance_id: "legacy-inst".to_string(),
        update_seq: None,
        job_id: job.job_id.clone(),
        request_id: None,
        status: "running".to_string(),
        stdout_chunk: None,
        stderr_chunk: None,
        stdout_tail: Some("legacy output\n".to_string()),
        stderr_tail: None,
        log_snapshot: None,
        exit_code: None,
        duration_ms: None,
        error: None,
        command_execution_state: None,
        validation_progress: None,
        finished: false,
    };
    registry.update_job(update()).await.unwrap();
    let (info, stdout, _, _, _, wait) = registry
        .job_log_for_auth(None, &job.job_id, None, None, None, Some(&token0), Some(5))
        .await
        .unwrap();
    assert_eq!(wait.wait_outcome, JobLogWaitOutcome::Immediate);
    assert!(wait.changed);
    assert_eq!(stdout.as_deref(), Some("legacy output\n"));
    let token1 = info.observation_token.unwrap();
    registry.update_job(update()).await.unwrap();
    let current = registry.get_job(&job.job_id).await.unwrap();
    assert_eq!(current.observation_token.as_deref(), Some(token1.as_str()));
    assert_eq!(current.last_update_seq, Some(0));
}
