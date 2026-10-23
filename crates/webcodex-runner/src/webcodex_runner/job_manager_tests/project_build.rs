use super::*;
use webcodex_core::project_build::{ProjectBuildAdapter, ProjectBuildRequest};
use webcodex_core::runner_operation::{
    RunnerInvocationMetadata, RunnerJobBuildOperation, RunnerOperation,
};

#[test]
#[ignore = "native Cargo exclusion smoke; run explicitly on the task candidate"]
fn native_cargo_lifecycle_jobs_skip_an_intentionally_broken_workspace_member() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("workspace");
    for package in ["healthy", "broken"] {
        std::fs::create_dir_all(root.join(package).join("src")).unwrap();
        std::fs::write(
            root.join(package).join("Cargo.toml"),
            format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
        )
        .unwrap();
        std::fs::write(
            root.join(package).join("src/lib.rs"),
            "pub fn value() -> u8 { 1 }\n",
        )
        .unwrap();
    }
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"healthy\", \"broken\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("broken/build.rs"),
        "fn main() { panic!(\"excluded member build script ran\"); }\n",
    )
    .unwrap();
    std::fs::write(root.join("broken/src/lib.rs"), "pub fn broken() {}\n").unwrap();
    std::fs::write(
        root.join("healthy/src/lib.rs"),
        "pub fn value() -> u8 { 1 }\n#[cfg(test)] mod tests { #[test] fn healthy() { assert_eq!(super::value(), 1); } }\n",
    )
    .unwrap();

    let manager = JobManager::new(1);
    let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
    let cargo_jobs = [
        ("build", vec!["build", "--workspace", "--exclude", "broken"]),
        (
            "check",
            vec![
                "check",
                "--all-targets",
                "--workspace",
                "--exclude",
                "broken",
            ],
        ),
        (
            "lint",
            vec![
                "clippy",
                "--all-targets",
                "--workspace",
                "--exclude",
                "broken",
                "--",
                "-D",
                "clippy::correctness",
            ],
        ),
        ("test", vec!["test", "--workspace", "--exclude", "broken"]),
    ];
    for (name, args) in cargo_jobs {
        let job_id = format!("native-exclusion-{name}");
        enqueue_structured_process_job(
            &manager,
            sink.clone(),
            &root,
            &job_id,
            Path::new("cargo"),
            args.into_iter().map(str::to_string).collect(),
            None,
            180,
            None,
        );
        assert!(
            manager.wait_for_workers(Instant::now() + Duration::from_secs(190)),
            "native Cargo {name} worker did not finish"
        );
        let snapshot = lock_unpoison(&manager.jobs)
            .get(&job_id)
            .expect("native Cargo Job retained")
            .snapshot
            .clone();
        assert_eq!(snapshot.status, "completed", "{name}: {snapshot:?}");
        assert_eq!(snapshot.exit_code, Some(0), "{name}: {snapshot:?}");
    }
    manager.stop_all();
}

#[test]
fn node_project_build_admission_accepts_runner_selected_package_manager() {
    for manager in ["npm", "pnpm", "yarn", "bun"] {
        for adapter in [ProjectBuildAdapter::Node, ProjectBuildAdapter::Auto] {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path().join("project");
            let registry = temp.path().join("registry");
            std::fs::create_dir_all(&root).unwrap();
            std::fs::create_dir_all(&registry).unwrap();
            std::fs::write(
                root.join("package.json"),
                format!(
                    r#"{{"packageManager":"{manager}@1","scripts":{{"build":"node build.js"}}}}"#
                ),
            )
            .unwrap();
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
                profile: None,
                targets: None,
                artifact_policy: None,
                project_id: "demo".into(),
                cwd: None,
                adapter,
                scope: None,
                dependency_policy: None,
                dependency_resolution_policy: None,
            };
            let (plan, cwd) =
                crate::webcodex_runner::project_build::plan(&policy, &registry, &request).unwrap();
            assert!(
                plan.is_valid(),
                "{manager}/{adapter:?}: invalid Runner plan"
            );
            assert_eq!(plan.process.executable, manager);
            assert_eq!(
                plan.process.args,
                if manager == "yarn" {
                    vec!["run", "build"]
                } else {
                    vec!["run", "--silent", "build"]
                },
                "{manager}/{adapter:?}"
            );
            let mut context = structured_process_context(&cwd, plan.process.args.len(), false);
            context.runtime_project_id = Some("agent:structured-agent:demo".into());
            context.project_cwd = Some(".".into());
            context.purpose = Some("build".into());
            context
                .structured_execution
                .as_mut()
                .unwrap()
                .execution_source = "project_build".into();
            // A different allowed manager is a valid command shape, but it is
            // not authorized unless the Runner's current manifest selected it.
            let other_manager = if manager == "npm" { "pnpm" } else { "npm" };
            let other_process =
                webcodex_core::project_build::canonical_project_build_process_with_manager(
                    "node",
                    &request,
                    other_manager,
                )
                .unwrap();
            let mut other_provenance = plan.provenance.clone();
            other_provenance.invocation_digest =
                webcodex_core::project_build::project_build_invocation_digest(&other_process);
            let spoofed = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
                job_id: "spoofed-node-build".into(),
                cwd: Some(cwd.to_string_lossy().into_owned()),
                process: other_process,
                provenance: other_provenance,
                timeout_secs: 10,
                context: context.clone(),
            });
            assert!(
                crate::webcodex_runner::project_build::fence(&policy, &registry, &spoofed)
                    .unwrap_err()
                    .contains("build_plan_stale"),
                "{manager}/{adapter:?}: manager substitution bypassed source provenance"
            );
            let operation = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
                job_id: "node-build-admission".into(),
                cwd: Some(cwd.to_string_lossy().into_owned()),
                process: plan.process,
                provenance: plan.provenance,
                timeout_secs: 10,
                context: context.clone(),
            });
            let wire = RunnerRequest::from_operation(
                RunnerInvocationMetadata {
                    request_id: "node-build-admission-request".into(),
                    client_id: "structured-agent".into(),
                    requested_by: "test".into(),
                    created_at: chrono::Utc::now().timestamp(),
                },
                RunnerOperation::Job(operation),
            )
            .unwrap();
            assert!(
                validate_runner_job_context(&context, &wire, "structured-agent").is_ok(),
                "{manager}/{adapter:?}: Runner rejected its own canonical Node build plan"
            );
            let manager_jobs = JobManager::new(1);
            let (sink, _rx) = structured_test_sink("structured-agent", "structured-instance");
            let mut blocker = retained_terminal_job("blocker", 0);
            blocker.client_id = "structured-agent".into();
            blocker.snapshot.status = "running".into();
            blocker.snapshot.ended_at = None;
            blocker.slot_reserved = true;
            lock_unpoison(&manager_jobs.jobs).insert("blocker".into(), blocker);
            manager_jobs.enqueue(
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
            assert_eq!(
                lock_unpoison(&manager_jobs.queued).len(),
                1,
                "{manager}/{adapter:?}: valid Node build was not admitted to the queue"
            );
            assert_eq!(manager_jobs.worker_count(), 0);
            manager_jobs.stop_all();
        }
    }
}

#[test]
fn project_build_rechecks_manifest_and_dependency_state_after_queueing() {
    for (marker, dependency, change_dependency, member_cwd) in [
        ("Cargo.toml", "Cargo.lock", false, false),
        ("Cargo.toml", "Cargo.lock", true, false),
        ("go.mod", "go.sum", false, false),
        ("go.mod", "go.sum", true, false),
        ("package.json", "package-lock.json", false, false),
        ("package.json", "package-lock.json", true, false),
        ("pyproject.toml", "setup.cfg", false, false),
        ("pyproject.toml", "setup.cfg", true, false),
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
            std::fs::write(
                root.join(marker),
                if marker == "package.json" {
                    r#"{"packageManager":"npm@1","scripts":{"build":"node build.js"}}"#
                } else {
                    ""
                },
            )
            .unwrap();
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
            profile: None,
            targets: None,
            artifact_policy: None,
            dependency_resolution_policy: None,
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
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
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

fn enqueue_python_build_fixture(
    shell: ShellConfig,
    manifest: &str,
    backend: Option<&str>,
    timeout: u64,
) -> (tempfile::TempDir, JobManager) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let registry = temp.path().join("registry");
    std::fs::create_dir_all(root.join("src/demo")).unwrap();
    std::fs::create_dir_all(root.join("dist")).unwrap();
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(root.join("src/demo/__init__.py"), "answer = 42\n").unwrap();
    // A previous artifact stays an observation; its presence never proves this
    // Job built a fresh artifact or that native packaging succeeded.
    std::fs::write(root.join("dist/previous.txt"), "existing output\n").unwrap();
    std::fs::write(root.join("pyproject.toml"), manifest).unwrap();
    if let Some(body) = backend {
        std::fs::write(root.join("backend.py"), body).unwrap();
    }
    std::fs::write(
        registry.join("demo.toml"),
        format!(
            "id='demo'\nname='Demo'\npath={}\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let policy = RunnerPolicy {
        allowed_roots: vec![root],
        ..Default::default()
    };
    let request = ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Python,
        scope: None,
        dependency_policy: Some(webcodex_core::project_build::ProjectDependencyPolicy {
            mode: webcodex_core::project_build::ProjectDependencyMode::Locked,
        }),
    };
    let (plan, cwd) =
        crate::webcodex_runner::project_build::plan(&policy, &registry, &request).unwrap();
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
        job_id: "python-build".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        process: plan.process,
        provenance: plan.provenance,
        timeout_secs: timeout,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "python-build-request".into(),
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
    (temp, manager)
}

#[test]
fn python_project_build_missing_interpreter_is_definitely_not_started() {
    let bin = tempfile::tempdir().unwrap();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("PATH".into(), bin.path().to_string_lossy().into_owned());
    let (_temp, manager) = enqueue_python_build_fixture(
        shell,
        "[build-system]\nrequires=[]\nbuild-backend='backend'\nbackend-path=['.']\n",
        Some("raise RuntimeError('must never run')\n"),
        10,
    );
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.status, "failed", "{snapshot:?}");
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert_eq!(snapshot.error.as_deref(), Some("build_tool_unavailable"));
    assert!(snapshot.build_artifacts.is_none());
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires provisioned WEBCODEX_TEST_BUILD_PYTHON with build/setuptools/wheel; never installs"]
fn runner_real_process_python_project_build_fixtures() {
    let python =
        std::env::var("WEBCODEX_TEST_BUILD_PYTHON").expect("set existing packaging interpreter");
    let mut shell = ShellConfig::default();
    shell.program = python.clone();
    let manifest="[build-system]\nrequires=['setuptools','wheel']\nbuild-backend='setuptools.build_meta'\n[project]\nname='webcodex-fixture-demo'\nversion='0.1.0'\n";
    let (temp, manager) = enqueue_python_build_fixture(shell.clone(), manifest, None, 30);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(40)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.status, "completed", "{snapshot:?}");
    assert_eq!(snapshot.job_id, "python-build");
    assert_eq!(snapshot.request_id, "python-build-request");
    let artifacts = snapshot.build_artifacts.unwrap();
    assert_eq!(artifacts.len(), 3, "{artifacts:?}");
    assert!(artifacts
        .iter()
        .any(|a| a.path.ends_with(".tar.gz") && a.bytes > 0));
    assert!(artifacts.iter().any(|a| a.path == "dist/previous.txt"));
    let wheel = artifacts.iter().find(|a| a.path.ends_with(".whl")).unwrap();
    let output=Command::new(&python).args(["-c","import sys,zipfile;z=zipfile.ZipFile(sys.argv[1]);assert z.read('demo/__init__.py')==b'answer = 42\\n';assert any(p.endswith('.dist-info/METADATA') for p in z.namelist())"])
        .arg(temp.path().join("project").join(&wheel.path)).stdin(Stdio::null()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        snapshot.stderr.tail.contains("Building wheel from sdist"),
        "{}",
        snapshot.stdout.tail
    );
    // Exit zero alone cannot invent missing artifact observations. A fixture
    // frontend with no outputs exercises this boundary without parsing stdout.
    let module = tempfile::tempdir().unwrap();
    std::fs::write(
        module.path().join("build.py"),
        "print('native success without outputs')\n",
    )
    .unwrap();
    let mut no_outputs = shell.clone();
    no_outputs.env.insert(
        "PYTHONPATH".into(),
        module.path().to_string_lossy().into_owned(),
    );
    let (_temp, manager) = enqueue_python_build_fixture(no_outputs, manifest, None, 15);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(20)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.exit_code, Some(0));
    assert_eq!(snapshot.status, "failed");
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::Completed)
    );
    assert_eq!(snapshot.error.as_deref(), Some("build_artifacts_missing"));
    assert!(snapshot.build_artifacts.is_none());

    // Preparation/probe can run project code. Drift after the queue fence must
    // still fail before the actual -m build invocation, even with valid tooling.
    let customization = tempfile::tempdir().unwrap();
    std::fs::write(customization.path().join("sitecustomize.py"), "from pathlib import Path\nwith Path('pyproject.toml').open('a') as manifest: manifest.write('# probe drift\\n')\nwith Path('probe-ran').open('a') as marker: marker.write('probe\\n')\n").unwrap();
    let mut drifting = shell.clone();
    drifting.env.insert(
        "PYTHONPATH".into(),
        customization.path().to_string_lossy().into_owned(),
    );
    let (temp, manager) = enqueue_python_build_fixture(drifting, manifest, None, 15);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(20)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted),
        "{snapshot:?}"
    );
    assert!(snapshot
        .error
        .as_deref()
        .unwrap()
        .contains("build_plan_stale"));
    assert_eq!(
        std::fs::read_to_string(temp.path().join("project/probe-ran")).unwrap(),
        "probe\n"
    );
    assert_eq!(
        std::fs::read_dir(temp.path().join("project/dist"))
            .unwrap()
            .count(),
        1
    );
    // No-isolation retains the frontend's dependency checks: an unavailable
    // requirement fails instead of installing or suppressing dependency checks.
    let missing = manifest.replace(
        "'setuptools','wheel'",
        "'webcodex-no-such-build-dependency==9999'",
    );
    let (_temp, manager) = enqueue_python_build_fixture(shell.clone(), &missing, None, 15);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(20)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.status, "failed", "{snapshot:?}");
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::Completed)
    );
    assert!(snapshot.build_artifacts.is_none());
    assert!(
        snapshot
            .stderr
            .tail
            .contains("webcodex-no-such-build-dependency"),
        "{snapshot:?}"
    );
    let custom = "[build-system]\nrequires=[]\nbuild-backend='backend'\nbackend-path=['.']\n";
    let (_temp,manager)=enqueue_python_build_fixture(shell.clone(),custom,Some("def get_requires_for_build_sdist(config_settings=None):\n    raise RuntimeError('intentional backend failure')\n"),15);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(20)));
    let snapshot = lock_unpoison(&manager.jobs)["python-build"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.status, "failed", "{snapshot:?}");
    assert!(snapshot.build_artifacts.is_none());
    assert!(
        snapshot.stderr.tail.contains("intentional backend failure"),
        "{snapshot:?}"
    );
    let sleeping="from pathlib import Path\nimport time\ndef get_requires_for_build_sdist(config_settings=None):\n    with Path('started').open('a') as marker: marker.write('started\\n')\n    time.sleep(30)\n    return []\n";
    for cancel in [true, false] {
        let (temp, manager) = enqueue_python_build_fixture(
            shell.clone(),
            custom,
            Some(sleeping),
            if cancel { 15 } else { 2 },
        );
        assert!(wait_until(Duration::from_secs(5), || temp
            .path()
            .join("project/started")
            .exists()));
        if cancel {
            manager.stop("python-build").unwrap();
        }
        assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
        let snapshot = lock_unpoison(&manager.jobs)["python-build"]
            .snapshot
            .clone();
        manager.stop_all();
        assert_eq!(
            snapshot.status,
            if cancel { "stopped" } else { "timeout" },
            "{snapshot:?}"
        );
        assert_eq!(
            std::fs::read_to_string(temp.path().join("project/started")).unwrap(),
            "started\n"
        );
        assert!(snapshot.build_artifacts.is_none());
        assert_eq!(snapshot.job_id, "python-build");
        assert_eq!(snapshot.request_id, "python-build-request");
    }
}

#[test]
#[ignore = "real-process Node/npm build fixture; requires installed Node and npm"]
fn node_project_build_real_declared_script_and_no_adapter_installation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(
        root.join("package.json"),
        r#"{"packageManager":"npm@11","scripts":{"build":"node build.cjs"}}"#,
    )
    .unwrap();
    std::fs::write(root.join("build.cjs"), "const fs=require('fs');fs.writeFileSync('output.txt', [process.env.COREPACK_ENABLE_NETWORK,process.env.COREPACK_ENV_FILE,process.env.COREPACK_ENABLE_AUTO_PIN].join(','));").unwrap();
    std::fs::write(
        registry.join("demo.toml"),
        format!(
            "id='demo'\nname='Demo'\npath={}\nallow_patch=true\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("COREPACK_ENABLE_NETWORK".into(), "1".into());
    shell
        .env
        .insert("COREPACK_ENABLE_AUTO_PIN".into(), "1".into());
    let policy = RunnerPolicy {
        allowed_roots: vec![root.clone()],
        ..Default::default()
    };
    let request = ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Node,
        scope: None,
        dependency_policy: None,
    };
    let (plan, cwd) =
        crate::webcodex_runner::project_build::plan(&policy, &registry, &request).unwrap();
    assert_eq!(plan.provenance.backend, "node");

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
        job_id: "node-build".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        process: plan.process,
        provenance: plan.provenance,
        timeout_secs: 10,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "node-build-request".into(),
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
        .get("node-build")
        .unwrap()
        .snapshot
        .clone();
    manager.stop_all();

    assert_eq!(snapshot.status, "completed", "{snapshot:?}");
    assert_eq!(
        std::fs::read_to_string(root.join("output.txt")).unwrap(),
        "0,0,0"
    );
    assert!(!root.join("node_modules").exists());
    assert!(!root.join("package-lock.json").exists());
}
