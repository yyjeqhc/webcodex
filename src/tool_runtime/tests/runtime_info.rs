use super::*;

#[test]
fn concurrency_counts_use_only_canonical_running_and_queued_statuses() {
    for status in ["running", "started"] {
        assert!(job_status_is_running(status), "{status}");
        assert!(!job_status_is_runner_queued(status), "{status}");
    }
    for status in ["queued", "agent_queued"] {
        assert!(job_status_is_runner_queued(status), "{status}");
        assert!(!job_status_is_running(status), "{status}");
    }
    for status in [
        "stop_requested",
        "recovering",
        "completed",
        "failed",
        "stopped",
        "lost",
        "timeout",
        "timed_out",
        "cancelled",
    ] {
        assert!(!job_status_is_running(status), "{status}");
        assert!(!job_status_is_runner_queued(status), "{status}");
    }
}

#[test]
fn runner_concurrency_counts_jobs_across_projects_for_one_client() {
    fn job(job_id: &str, client_id: &str, project_id: &str, status: &str) -> ShellJobInfo {
        ShellJobInfo {
            job_id: job_id.to_string(),
            request_id: None,
            client_id: client_id.to_string(),
            kind: "shell".to_string(),
            project_id: Some(project_id.to_string()),
            session_id: None,
            ssh_resource: None,
            cwd: None,
            project_cwd: None,
            purpose: None,
            shell: None,
            command_preview: "test".to_string(),
            status: status.to_string(),
            created_at: 0,
            started_at: None,
            ended_at: None,
            exit_code: None,
            duration_ms: None,
            elapsed_secs: None,
            error: None,
            command_execution_state: None,
            structured_execution: None,
            codex: None,
            result: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            validation: None,
            recovery_state: None,
            recovered_after_server_restart: false,
            reconciled_at: None,
            recovery_reason_code: None,
            observation_token: None,
            last_update_seq: None,
            stdout_retained_from_line: None,
            stderr_retained_from_line: None,
            stdout_log_truncated: false,
            stderr_log_truncated: false,
        }
    }

    let client = RunnerView {
        client_id: "shared-runner".to_string(),
        runner_instance_id: "shared-instance".to_string(),
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        status: "online".to_string(),
        connected: true,
        last_seen: 0,
        capabilities: Default::default(),
        coding_agent_providers: None,
        pending_requests: 0,
        projects: Vec::new(),
        project_inventory: None,
        runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
        transport: "websocket".to_string(),
        policy: None,
        registered_at: 0,
        connected_at: 0,
        disconnected_at: None,
        process_started_at: None,
        build: None,
        job_concurrency_limit: Some(2),
    };
    // The same compatibility reduction backs sparse and full projections.
    for count in [0, 1, 8] {
        let mut clients = vec![client.clone(); count];
        for (index, runner) in clients.iter_mut().enumerate() {
            runner.build = Some(crate::runner_protocol::RunnerBuildInfo {
                version: Some(if index == 0 { "1.0" } else { "0.1" }.into()),
                git_commit: Some(if index == 0 { "aaaaaaaa" } else { "bbbbbbbb" }.into()),
                git_dirty: Some(false),
                built_at: None,
                target: None,
                architecture: None,
            });
            if index > 0 {
                runner.connected = false;
                runner.status = "stale".into();
            }
        }
        let full = version_compatibility_against(
            &clients,
            "1.0",
            Some("aaaaaaaa"),
            Some(false),
            Value::Null,
            true,
        );
        let sparse = version_compatibility_against(
            &clients,
            "1.0",
            Some("aaaaaaaa"),
            Some(false),
            Value::Null,
            false,
        );
        for key in [
            "protocol_compatibility",
            "build_alignment",
            "source_alignment",
        ] {
            assert_eq!(full[key], sparse[key], "{count} {key}");
        }
        assert!(sparse.get("runners").is_none());
        assert_eq!(sparse["mixed_builds_present"], count > 1);
    }
    let jobs = vec![
        job(
            "project-a-running",
            "shared-runner",
            "agent:shared:a",
            "running",
        ),
        job(
            "project-b-queued",
            "shared-runner",
            "agent:shared:b",
            "agent_queued",
        ),
        job(
            "recovering",
            "shared-runner",
            "agent:shared:a",
            "recovering",
        ),
        {
            let mut lost = job("lost", "shared-runner", "agent:shared:a", "lost");
            lost.recovery_reason_code = Some("runner_inventory_missing".into());
            lost
        },
        job(
            "foreign-running",
            "other-runner",
            "agent:other:c",
            "running",
        ),
    ];

    assert_eq!(active_jobs_for_client(&jobs, "shared-runner"), 3);
    let counts = runtime_job_counts(&jobs);
    assert_eq!(counts["active_count"], 4);
    assert_eq!(counts["recovering_count"], 1);
    assert_eq!(counts["lost_after_reconcile_count"], 1);
    let sparse = sparse_job_counts(&counts);
    assert_eq!(sparse["running_count"], 2);
    assert_eq!(sparse["queued_count"], 1);
    assert_eq!(sparse["recovering_count"], 1);
    assert_eq!(sparse["lost_after_reconcile_count"], 1);
    assert!(serde_json::to_vec(&sparse).unwrap().len() < 130);
    assert_eq!(
        job_concurrency_for_client(&client, &jobs),
        json!({"limit": 2, "running": 1, "queued": 1})
    );
}

#[test]
fn compact_runtime_status_keeps_minimum_job_state_counts() {
    let compact = compact_runtime_status(&json!({
        "jobs": {
            "active_count": 5,
            "running_count": 2,
            "queued_count": 1,
        }
    }));
    assert_eq!(
        compact["jobs"],
        json!({
            "active_count": 5,
            "running_count": 2,
            "queued_count": 1,
        })
    );
}
