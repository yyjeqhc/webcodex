use super::*;
use webcodex_core::project_validation::{
    ProjectValidationAction, ProjectValidationAdapter, ProjectValidationRequest,
};
use webcodex_core::runner_operation::{
    RunnerInvocationMetadata, RunnerJobValidationOperation, RunnerOperation,
};
use webcodex_core::runner_protocol::{ShellJobContext, ShellJobValidationMetadata};

#[test]
fn project_validation_rechecks_manifest_and_dependency_state_after_queueing() {
    for (marker, dependency, change_dependency) in [
        ("Cargo.toml", "Cargo.lock", false),
        ("Cargo.toml", "Cargo.lock", true),
        ("go.mod", "go.sum", false),
        ("go.mod", "go.sum", true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let registry = temp.path().join("registry");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&registry).unwrap();
        std::fs::write(root.join(marker), "").unwrap();
        std::fs::write(root.join(dependency), "").unwrap();
        std::fs::write(
            registry.join("demo.toml"),
            format!(
                "id = 'demo'\nname = 'Demo'\npath = {}\nallow_patch = true\n",
                serde_json::to_string(root.to_str().unwrap()).unwrap(),
            ),
        )
        .unwrap();

        let policy = RunnerPolicy {
            allowed_roots: vec![root.clone()],
            ..Default::default()
        };
        let request = ProjectValidationRequest {
            project_id: "demo".into(),
            cwd: None,
            action: ProjectValidationAction::Check,
            adapter: ProjectValidationAdapter::Auto,
            scope: None,
            test: None,
        };
        let (plan, cwd) =
            crate::webcodex_runner::validation::project::plan(&policy, &registry, &request)
                .unwrap();

        let metadata = ShellJobValidationMetadata {
            project_validation: Some(plan.provenance.clone()),
            source_fence: None,
            tool: "project_validate".into(),
            kind: "check".into(),
            steps: vec![plan.step.clone()],
            effective_timeout_secs: 60,
            sync_wait_secs: 10,
            adapter: plan.adapter.clone(),
            validation_target_id: Some(plan.validation_target_id.clone()),
            minimum_tests: None,
            require_tests: None,
            no_run: None,
        };
        assert!(metadata.is_valid());

        let context = ShellJobContext {
            runtime_project_id: Some("agent:validation-agent:demo".into()),
            validation: Some(metadata),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(".".into()),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            purpose: Some("validation".into()),
            shell: None,
            command_preview: "project validation".into(),
            validation_steps: vec![plan.step.name.clone()],
            structured_execution: None,
        };
        let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
            job_id: "queued-validation".into(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            steps: vec![plan.step],
            timeout_secs: 60,
            context,
        });
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: "queued-validation-request".into(),
                client_id: "validation-agent".into(),
                requested_by: "test".into(),
                created_at: chrono::Utc::now().timestamp(),
            },
            RunnerOperation::Job(operation),
        )
        .unwrap();

        let manager = JobManager::new(1);
        let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
        // Reserve the sole slot without a real child. Queue promotion remains
        // deterministic and the stale validation must fail before tool lookup
        // or native process creation.
        let mut blocker = retained_terminal_job("blocker", 0);
        blocker.client_id = "validation-agent".into();
        blocker.snapshot.status = "running".into();
        blocker.snapshot.ended_at = None;
        blocker.slot_reserved = true;
        lock_unpoison(&manager.jobs).insert("blocker".into(), blocker);

        manager.enqueue(
            sink,
            PendingJobStart::from_wire(
                1,
                policy,
                ShellConfig::default(),
                SshConfig::default(),
                registry,
                wire,
            ),
        );
        assert_eq!(lock_unpoison(&manager.queued).len(), 1);
        assert_eq!(manager.worker_count(), 0);

        // The admission fence succeeded. Invalidate the retained project plan
        // while the validation still waits for a Runner execution slot.
        let changed = if change_dependency {
            dependency
        } else {
            marker
        };
        std::fs::write(root.join(changed), "# changed while queued\n").unwrap();

        lock_unpoison(&manager.jobs).remove("blocker");
        manager.start_available_queued();

        let result = lock_unpoison(&manager.jobs)
            .get("queued-validation")
            .unwrap()
            .snapshot
            .clone();
        let reserved = lock_unpoison(&manager.jobs)
            .get("queued-validation")
            .unwrap()
            .slot_reserved;
        manager.stop_all();

        assert_eq!(result.status, "failed", "{marker}/{changed}: {result:?}");
        assert_eq!(
            result.command_execution_state,
            Some(ShellCommandExecutionState::NotStarted),
            "{marker}/{changed}: {result:?}"
        );
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("validation_plan_stale")),
            "{marker}/{changed}: {result:?}"
        );
        assert!(!reserved, "rejected validation retained the execution slot");
        assert!(lock_unpoison(&manager.queued).is_empty());
    }
}
