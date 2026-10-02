use super::*;
use webcodex_core::project_build::{ProjectBuildAdapter, ProjectBuildRequest};
use webcodex_core::runner_operation::{
    RunnerInvocationMetadata, RunnerJobBuildOperation, RunnerOperation,
};

#[test]
fn project_build_rechecks_manifest_and_dependency_state_after_queueing() {
    for (marker, dependency, change_dependency, member_cwd) in [
        ("Cargo.toml", "Cargo.lock", false, false),
        ("Cargo.toml", "Cargo.lock", true, false),
        ("go.mod", "go.sum", false, false),
        ("go.mod", "go.sum", true, false),
        ("Cargo.toml", "Cargo.lock", false, true),
        ("Cargo.toml", "Cargo.lock", true, true),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let registry = temp.path().join("registry");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&registry).unwrap();
        if member_cwd {
            std::fs::create_dir_all(root.join("member")).unwrap();
            std::fs::write(
                root.join("Cargo.toml"),
                r#"[workspace]
members = ["member"]
resolver = "2"
"#,
            )
            .unwrap();
            std::fs::write(
                root.join("member/Cargo.toml"),
                r#"[package]
name = "member"
version = "0.1.0"
edition = "2021"
"#,
            )
            .unwrap();
        } else {
            std::fs::write(root.join(marker), "").unwrap();
        }
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
        let request = ProjectBuildRequest {
            project_id: "demo".into(),
            cwd: member_cwd.then(|| "member".into()),
            adapter: ProjectBuildAdapter::Auto,
            scope: None,
            dependency_policy: None,
        };
        let (plan, cwd) =
            crate::webcodex_runner::project_build::plan(&policy, &registry, &request).unwrap();
        let mut context = structured_process_context(&cwd, plan.process.args.len(), false);
        context.runtime_project_id = Some("agent:structured-agent:demo".into());
        context.project_cwd = Some(if member_cwd { "member" } else { "." }.into());
        context.purpose = Some("build".into());
        context
            .structured_execution
            .as_mut()
            .unwrap()
            .execution_source = "project_build".into();
        let operation = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
            job_id: "queued-build".into(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            process: plan.process,
            provenance: plan.provenance,
            timeout_secs: 10,
            context,
        });
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: "queued-build-request".into(),
                client_id: "structured-agent".into(),
                requested_by: "test".into(),
                created_at: chrono::Utc::now().timestamp(),
            },
            RunnerOperation::Job(operation),
        )
        .unwrap();
        let manager = JobManager::new(1);
        let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
        // Reserve the sole slot without a real child. Queue promotion is under
        // test control; no timing, inherited stdin, or helper process is needed.
        let mut blocker = retained_terminal_job("blocker", 0);
        blocker.client_id = "structured-agent".into();
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
        // The early admission fence succeeded. Invalidate that observation
        // while the build still waits for a Runner execution slot.
        let changed = if change_dependency {
            dependency
        } else {
            marker
        };
        std::fs::write(root.join(changed), "# changed while queued\n").unwrap();
        lock_unpoison(&manager.jobs).remove("blocker");
        manager.start_available_queued();
        let finished = manager.wait_for_workers(Instant::now() + Duration::from_secs(15));
        let result = lock_unpoison(&manager.jobs)
            .get("queued-build")
            .unwrap()
            .snapshot
            .clone();
        let reserved = lock_unpoison(&manager.jobs)
            .get("queued-build")
            .unwrap()
            .slot_reserved;
        manager.stop_all();
        assert!(finished, "queued build worker did not terminate");
        assert_eq!(
            result.status, "failed",
            "{marker}/{changed}/member={member_cwd}: {result:?}"
        );
        assert_eq!(
            result.command_execution_state,
            Some(ShellCommandExecutionState::NotStarted),
            "{marker}/{changed}/member={member_cwd}: {result:?}"
        );
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("build_plan_stale")),
            "{marker}/{changed}/member={member_cwd}: {result:?}"
        );
        assert!(!reserved, "rejected build retained the execution slot");
        assert!(!root.join("target").exists());
        assert!(lock_unpoison(&manager.queued).is_empty());
    }
}

#[test]
fn go_project_build_overrides_ambient_gowork_at_spawn() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let registry = root.join("registry");
    let executable_temp = crate::tests::executable_tempdir();
    let bin = executable_temp.path().to_path_buf();
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(root.join("go.mod"), "module example.test/demo\ngo 1.22\n").unwrap();
    std::fs::write(
        registry.join("demo.toml"),
        format!(
            "id = 'demo'\nname = 'Demo'\npath = {}\nallow_patch = true\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap(),
        ),
    )
    .unwrap();

    let helper = structured_process_helper();
    let fake_go = bin.join(format!("go{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(&helper.path, &fake_go).unwrap();
    let capture = root.join("build-gowork.txt");
    let mut shell = ShellConfig::default();
    shell.path_prepend.push(bin);
    shell.env.insert(
        "GOWORK".into(),
        root.join("ambient.work").to_string_lossy().into_owned(),
    );
    shell.env.insert("GO111MODULE".into(), "off".into());
    shell.env.insert(
        "WEBCODEX_TEST_CAPTURE_GO_MODULE_ENV".into(),
        capture.to_string_lossy().into_owned(),
    );

    let policy = RunnerPolicy {
        allowed_roots: vec![root.clone()],
        ..Default::default()
    };
    let request = ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Go,
        scope: None,
        dependency_policy: None,
    };
    let (plan, cwd) =
        crate::webcodex_runner::project_build::plan(&policy, &registry, &request).unwrap();
    assert_eq!(plan.provenance.backend, "go");

    let mut context = structured_process_context(&cwd, plan.process.args.len(), false);
    context.runtime_project_id = Some("agent:structured-agent:demo".into());
    context.project_cwd = Some(".".into());
    context.purpose = Some("build".into());
    context
        .structured_execution
        .as_mut()
        .unwrap()
        .execution_source = "project_build".into();
    let operation = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
        job_id: "go-single-module-build".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        process: plan.process,
        provenance: plan.provenance,
        timeout_secs: 10,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "go-single-module-build-request".into(),
            client_id: "structured-agent".into(),
            requested_by: "test".into(),
            created_at: chrono::Utc::now().timestamp(),
        },
        RunnerOperation::Job(operation),
    )
    .unwrap();

    let manager = JobManager::new(1);
    let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
    manager.enqueue(
        sink,
        PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
    );
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(15)));
    let snapshot = lock_unpoison(&manager.jobs)
        .get("go-single-module-build")
        .unwrap()
        .snapshot
        .clone();
    manager.stop_all();

    assert_eq!(snapshot.status, "completed", "{snapshot:?}");
    assert_eq!(
        std::fs::read_to_string(capture).unwrap(),
        "GOWORK=off\nGO111MODULE=on\nARGV=build\t./...\n"
    );
}
