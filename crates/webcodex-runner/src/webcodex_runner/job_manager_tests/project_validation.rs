use super::*;
use crate::webcodex_runner::ShellProfileConfig;
use webcodex_core::project_validation::{
    ProjectValidationAction, ProjectValidationAdapter, ProjectValidationRequest,
};
use webcodex_core::runner_operation::{
    RunnerInvocationMetadata, RunnerJobValidationOperation, RunnerOperation,
};
use webcodex_core::runner_protocol::{ShellJobContext, ShellJobValidationMetadata};

#[test]
fn project_validation_rechecks_manifest_and_dependency_state_after_queueing() {
    for (marker, dependency, change_dependency, member_cwd, ruff_action) in [
        ("Cargo.toml", "Cargo.lock", false, false, None),
        ("Cargo.toml", "Cargo.lock", true, false, None),
        ("go.mod", "go.sum", false, false, None),
        ("go.mod", "go.sum", true, false, None),
        ("package.json", "package-lock.json", false, false, None),
        ("package.json", "package-lock.json", true, false, None),
        ("pyproject.toml", "pytest.ini", false, false, None),
        ("pyproject.toml", "pytest.ini", true, false, None),
        ("Cargo.toml", "Cargo.lock", false, true, None),
        ("Cargo.toml", "Cargo.lock", true, true, None),
        (
            "Cargo.toml",
            "Cargo.lock",
            false,
            false,
            Some(ProjectValidationAction::Lint),
        ),
        (
            "Cargo.toml",
            "Cargo.lock",
            true,
            false,
            Some(ProjectValidationAction::Lint),
        ),
        (
            "Cargo.toml",
            "Cargo.lock",
            true,
            true,
            Some(ProjectValidationAction::Lint),
        ),
        (
            "pyproject.toml",
            "unused",
            false,
            false,
            Some(ProjectValidationAction::Check),
        ),
        (
            "pyproject.toml",
            "unused",
            false,
            false,
            Some(ProjectValidationAction::FormatCheck),
        ),
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
                    r#"{"scripts":{"test":"vitest run"}}"#
                } else if marker == "pyproject.toml" && ruff_action.is_some() {
                    "[tool.ruff]\ntarget-version='py311'\n"
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
        let request = ProjectValidationRequest {
            project_id: "demo".into(),
            cwd: member_cwd.then(|| "member".into()),
            action: if let Some(action) = ruff_action {
                action
            } else if matches!(marker, "pyproject.toml" | "package.json") {
                ProjectValidationAction::Test
            } else {
                ProjectValidationAction::Check
            },
            adapter: ProjectValidationAdapter::Auto,
            scope: None,
            dependency_policy: None,
            dependency_resolution_policy: None,
            test: None,
        };
        let (plan, cwd) =
            crate::webcodex_runner::validation::project::plan(&policy, &registry, &request)
                .unwrap();

        let metadata = ShellJobValidationMetadata {
            project_validation: Some(plan.provenance.clone()),
            source_fence: None,
            tool: "project_validate".into(),
            kind: request
                .action
                .kind_for_backend(&plan.provenance.backend)
                .into(),
            steps: vec![plan.step.clone()],
            effective_timeout_secs: 60,
            sync_wait_secs: 10,
            adapter: plan.adapter.clone(),
            validation_target_id: Some(plan.validation_target_id.clone()),
            minimum_tests: request.test_requirements().1,
            require_tests: request.test_requirements().0,
            no_run: None,
        };
        assert!(metadata.is_valid());

        let context = ShellJobContext {
            runtime_project_id: Some("agent:validation-agent:demo".into()),
            validation: Some(metadata),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(if member_cwd { "member" } else { "." }.into()),
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
                .is_some_and(|error| error.contains("validation_plan_stale")),
            "{marker}/{changed}/member={member_cwd}: {result:?}"
        );
        assert!(!reserved, "rejected validation retained the execution slot");
        assert!(lock_unpoison(&manager.queued).is_empty());
    }
}

#[test]
fn go_project_validation_overrides_ambient_gowork_but_direct_go_test_does_not() {
    for project_gateway in [true, false] {
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
        let capture = root.join(if project_gateway {
            "project-validate-gowork.txt"
        } else {
            "direct-go-test-gowork.txt"
        });
        let ambient = root.join("ambient.work").to_string_lossy().into_owned();
        let mut shell = ShellConfig::default();
        shell.path_prepend.push(bin);
        shell.env.insert("GOWORK".into(), ambient.clone());
        shell.env.insert("GO111MODULE".into(), "off".into());
        shell.env.insert(
            "WEBCODEX_TEST_CAPTURE_GO_MODULE_ENV".into(),
            capture.to_string_lossy().into_owned(),
        );
        if !project_gateway {
            shell
                .env
                .insert("WEBCODEX_TEST_GO_JSON_PASS".into(), "1".into());
        }

        let policy = RunnerPolicy {
            allowed_roots: vec![root.clone()],
            ..Default::default()
        };
        let (step, metadata, cwd) = if project_gateway {
            let request = ProjectValidationRequest {
                project_id: "demo".into(),
                cwd: None,
                action: ProjectValidationAction::Check,
                adapter: ProjectValidationAdapter::Go,
                scope: None,
                dependency_policy: None,
                dependency_resolution_policy: None,
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
                adapter: plan.adapter,
                validation_target_id: Some(plan.validation_target_id),
                minimum_tests: None,
                require_tests: None,
                no_run: None,
            };
            (plan.step, metadata, cwd)
        } else {
            let step = ShellJobValidationStep {
                name: "test".into(),
                program: "go".into(),
                args: vec!["test".into(), "-json".into(), "./...".into()],
                env: Vec::new(),
            };
            let metadata = ShellJobValidationMetadata {
                project_validation: None,
                source_fence: None,
                tool: "go_test".into(),
                kind: "test".into(),
                steps: vec![step.clone()],
                effective_timeout_secs: 60,
                sync_wait_secs: 10,
                adapter: "go_test".into(),
                validation_target_id: Some("target:3123456789abcdef01234567".into()),
                minimum_tests: None,
                require_tests: None,
                no_run: None,
            };
            (step, metadata, root.clone())
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
            command_preview: "go validation".into(),
            validation_steps: vec![step.name.clone()],
            structured_execution: None,
        };
        let job_id = if project_gateway {
            "go-single-module-validation"
        } else {
            "direct-go-test"
        };
        let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
            job_id: job_id.into(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            steps: vec![step],
            timeout_secs: 60,
            context,
        });
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: format!("{job_id}-request"),
                client_id: "validation-agent".into(),
                requested_by: "test".into(),
                created_at: chrono::Utc::now().timestamp(),
            },
            RunnerOperation::Job(operation),
        )
        .unwrap();

        let manager = JobManager::new(1);
        let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
        manager.enqueue(
            sink,
            PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
        );
        assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(15)));
        let snapshot = lock_unpoison(&manager.jobs)
            .get(job_id)
            .unwrap()
            .snapshot
            .clone();
        manager.stop_all();

        assert_eq!(snapshot.status, "completed", "{snapshot:?}");
        let observed = std::fs::read_to_string(capture).unwrap();
        let expected = if project_gateway {
            "GOWORK=off\nGO111MODULE=on\nARGV=vet\t./...\n".to_string()
        } else {
            format!("GOWORK={ambient}\nGO111MODULE=off\nARGV=test\t-json\t./...\n")
        };
        assert_eq!(observed, expected);
    }
}

fn enqueue_python_project_fixture(
    shell: ShellConfig,
    body: &str,
    filter: Option<&str>,
) -> (tempfile::TempDir, JobManager) {
    enqueue_python_validation_fixture(shell, body, filter, ProjectValidationAction::Test)
}

fn enqueue_python_validation_fixture(
    shell: ShellConfig,
    body: &str,
    filter: Option<&str>,
    action: ProjectValidationAction,
) -> (tempfile::TempDir, JobManager) {
    use webcodex_core::project_validation::ProjectValidationTestOptions;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let registry = temp.path().join("registry");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::write(
        root.join("pyproject.toml"),
        "[tool.pytest.ini_options]\n[tool.ruff]\ntarget-version='py311'\n",
    )
    .unwrap();
    std::fs::write(root.join("test_example.py"), body).unwrap();
    std::fs::write(
        registry.join("demo.toml"),
        format!(
            "id='demo'\nname='Demo'\npath={}\nallow_patch=true\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
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
        action,
        adapter: ProjectValidationAdapter::Python,
        scope: None,
        dependency_policy: None,
        dependency_resolution_policy: None,
        test: filter.map(|filter| ProjectValidationTestOptions {
            filter: Some(filter.into()),
            ..Default::default()
        }),
    };
    let (plan, cwd) =
        crate::webcodex_runner::validation::project::plan(&policy, &registry, &request).unwrap();
    let metadata = ShellJobValidationMetadata {
        project_validation: Some(plan.provenance),
        tool: "project_validate".into(),
        kind: action.kind().into(),
        steps: vec![plan.step.clone()],
        effective_timeout_secs: 60,
        sync_wait_secs: 1,
        adapter: plan.adapter,
        validation_target_id: Some(plan.validation_target_id),
        source_fence: None,
        minimum_tests: request.test_requirements().1,
        require_tests: request.test_requirements().0,
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
        purpose: Some(
            webcodex_validation::execution_purpose_for_validation_kind(action.kind())
                .as_str()
                .into(),
        ),
        shell: None,
        command_preview: "Python validation".into(),
        validation_steps: vec![plan.step.name.clone()],
        structured_execution: None,
    };
    let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
        job_id: "pytest-fixture-job".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        steps: vec![plan.step],
        timeout_secs: 60,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "pytest-fixture-request".into(),
            client_id: "validation-agent".into(),
            requested_by: "test".into(),
            created_at: chrono::Utc::now().timestamp(),
        },
        RunnerOperation::Job(operation),
    )
    .unwrap();
    let manager = JobManager::new(1);
    let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
    manager.enqueue(
        sink,
        PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
    );
    (temp, manager)
}

#[test]
fn project_validation_python_pytest_missing_interpreter_starts_no_tests() {
    let bin = tempfile::tempdir().unwrap();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("PATH".into(), bin.path().to_string_lossy().into_owned());
    let (_temp, manager) =
        enqueue_python_project_fixture(shell, "raise RuntimeError('must not run')\n", None);
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
    let snapshot = lock_unpoison(&manager.jobs)["pytest-fixture-job"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert_eq!(
        snapshot.error.as_deref(),
        Some(VALIDATION_TOOL_UNAVAILABLE_CODE)
    );
    assert_eq!(
        snapshot.context.validation.unwrap().adapter,
        "python:pytest:test"
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires explicitly supplied existing Python 3 environment with pytest; never installs"]
fn runner_real_process_project_validation_python_pytest_fixtures() {
    let interpreter = std::env::var("WEBCODEX_TEST_PYTEST_PYTHON")
        .expect("set an existing fixture interpreter path");
    for (body,filter,exit,count) in [
        ("def test_ok():\n    assert True\n",None,0,Some(1)),
        ("def test_bad():\n    assert False\n",None,1,Some(1)),
        ("def test_ok():\n    assert True\n",Some("absent"),5,Some(0)),
        ("import pytest\n@pytest.mark.skip(reason='fixture')\ndef test_skip():\n    assert True\n",None,0,Some(0)),
        ("raise RuntimeError('collection error fixture')\n",None,2,None),
    ] {
        let mut shell=ShellConfig::default(); shell.program=interpreter.clone();
        let (_temp,manager)=enqueue_python_project_fixture(shell,body,filter);
        assert!(manager.wait_for_workers(Instant::now()+Duration::from_secs(15)));
        let snapshot=lock_unpoison(&manager.jobs)["pytest-fixture-job"].snapshot.clone();manager.stop_all();
        assert_eq!(snapshot.exit_code,Some(exit),"{snapshot:?}");
        assert_eq!(snapshot.status, if exit == 0 { "completed" } else { "failed" }, "{snapshot:?}");
        let diagnostics=webcodex_core::validation_evidence::parse_pytest_diagnostics(&snapshot.stdout.tail,snapshot.stdout.truncated);
        let observed=diagnostics.test_summary.as_ref().map(|summary| summary.passed.unwrap()+summary.failed.unwrap());
        // Collection errors are native failures; the final "Interrupted" line
        // cannot provide completed-test evidence.
        assert_eq!(observed,count,"{diagnostics:?}");
        assert_eq!(snapshot.context.validation.unwrap().tool,"project_validate");
    }
    // A long pytest test remains one cancellable managed validation process.
    let mut shell = ShellConfig::default();
    shell.program = interpreter.clone();
    let (temp,manager)=enqueue_python_project_fixture(shell,
        "from pathlib import Path\nimport time\ndef test_wait():\n    Path('started').write_text('ready')\n    time.sleep(30)\n",None);
    assert!(wait_until(Duration::from_secs(5), || temp
        .path()
        .join("project/started")
        .exists()));
    manager.stop("pytest-fixture-job").unwrap();
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
    let snapshot = lock_unpoison(&manager.jobs)["pytest-fixture-job"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.status, "stopped");
    assert!(
        webcodex_core::validation_evidence::parse_pytest_diagnostics(&snapshot.stdout.tail, true)
            .test_summary
            .is_none()
    );

    // Tooling deliberately supplied through a profile PYTHONPATH is probed with
    // the same module search environment that actual -m execution receives.
    let module = tempfile::tempdir().unwrap();
    std::fs::write(
        module.path().join("pytest.py"),
        "print('1 passed in 0.01s')\n",
    )
    .unwrap();
    let mut shell = ShellConfig::default();
    shell.program = interpreter;
    shell.env.insert(
        "PYTHONPATH".into(),
        module.path().to_string_lossy().into_owned(),
    );
    let (_temp, manager) = enqueue_python_project_fixture(
        shell,
        "raise RuntimeError('fixture module owns execution')\n",
        None,
    );
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
    let snapshot = lock_unpoison(&manager.jobs)["pytest-fixture-job"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(snapshot.exit_code, Some(0), "{snapshot:?}");
    assert_eq!(snapshot.stdout.tail.trim(), "1 passed in 0.01s");
}

#[test]
fn project_validation_ruff_missing_interpreter_is_definitely_not_started() {
    for action in [
        ProjectValidationAction::Check,
        ProjectValidationAction::FormatCheck,
    ] {
        let bin = tempfile::tempdir().unwrap();
        let mut shell = ShellConfig::default();
        shell
            .env
            .insert("PATH".into(), bin.path().to_string_lossy().into_owned());
        let (_temp, manager) = enqueue_python_validation_fixture(
            shell,
            "raise RuntimeError('must not run')\n",
            None,
            action,
        );
        assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(10)));
        let snapshot = lock_unpoison(&manager.jobs)["pytest-fixture-job"]
            .snapshot
            .clone();
        manager.stop_all();
        assert_eq!(
            snapshot.command_execution_state,
            Some(ShellCommandExecutionState::NotStarted)
        );
        assert_eq!(
            snapshot.error.as_deref(),
            Some(VALIDATION_TOOL_UNAVAILABLE_CODE)
        );
        assert_eq!(
            snapshot.context.validation.unwrap().adapter,
            format!("python:ruff:{}", action.kind())
        );
    }
}

#[test]
fn rust_clippy_activity_keeps_lint_step_identity() {
    let step = ShellJobValidationStep {
        name: "lint".into(),
        program: "cargo".into(),
        args: ["clippy", "--all-targets", "--", "-D", "clippy::correctness"]
            .map(str::to_string)
            .to_vec(),
        env: vec![],
    };
    let activity = validation_step_activity(&step);
    assert_eq!(activity.phase, ShellJobActivityPhase::ValidationCheck);
    assert_eq!(
        cargo_activity_from_stderr(&step, "Finished dev profile\n"),
        Some(activity)
    );
    assert_eq!(step.name, "lint");
}

#[test]
fn project_validation_rechecks_provenance_after_shell_profile_preparation() {
    // A profile initializer runs *after* the worker's queue-time fence.
    // Its filesystem effects must not authorize a stale native validation.
    for (backend, marker, change_source) in [
        ("rust", "Cargo.lock", true),
        ("rust", "Cargo.lock", false),
        ("go", "go.mod", true),
        ("go", "go.mod", false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        let registry = temp.path().join("registry");
        let executable_temp = crate::tests::executable_tempdir();
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&registry).unwrap();
        let (adapter, manifest) = if backend == "rust" {
            std::fs::write(
                root.join("Cargo.toml"),
                "[package]\nname='demo'\nversion='0.1.0'\nedition='2021'\n",
            )
            .unwrap();
            (ProjectValidationAdapter::Rust, "version = 3\n")
        } else {
            (
                ProjectValidationAdapter::Go,
                "module example.test/demo\ngo 1.22\n",
            )
        };
        std::fs::write(root.join(marker), manifest).unwrap();
        std::fs::write(
            registry.join("demo.toml"),
            format!(
                "id='demo'\nname='Demo'\npath={}\nallow_patch=true\n",
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
            adapter,
            scope: None,
            dependency_policy: None,
            dependency_resolution_policy: None,
            test: None,
        };
        let (plan, cwd) =
            crate::webcodex_runner::validation::project::plan(&policy, &registry, &request)
                .unwrap();
        let metadata = ShellJobValidationMetadata {
            project_validation: Some(plan.provenance),
            source_fence: None,
            tool: "project_validate".into(),
            kind: "check".into(),
            steps: vec![plan.step.clone()],
            effective_timeout_secs: 30,
            sync_wait_secs: 1,
            adapter: plan.adapter,
            validation_target_id: Some(plan.validation_target_id),
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
            validation_steps: vec!["check".into()],
            structured_execution: None,
        };
        let job_id = format!("profile-fence-{backend}-{change_source}");
        let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
            job_id: job_id.clone(),
            cwd: Some(cwd.to_string_lossy().into_owned()),
            steps: vec![plan.step],
            timeout_secs: 30,
            context,
        });
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: format!("{job_id}-request"),
                client_id: "validation-agent".into(),
                requested_by: "test".into(),
                created_at: chrono::Utc::now().timestamp(),
            },
            RunnerOperation::Job(operation),
        )
        .unwrap();

        let helper = structured_process_helper();
        let fake_tool = executable_temp.path().join(format!(
            "{}{}",
            if backend == "rust" { "cargo" } else { "go" },
            std::env::consts::EXE_SUFFIX
        ));
        std::fs::copy(&helper.path, fake_tool).unwrap();
        let capture = root.join("native-tool-invoked.txt");
        let mut shell = ShellConfig::default();
        shell
            .path_prepend
            .push(executable_temp.path().to_path_buf());
        if backend == "go" {
            shell.env.insert(
                "WEBCODEX_TEST_CAPTURE_GO_MODULE_ENV".into(),
                capture.to_string_lossy().into_owned(),
            );
        }
        let init_script = if change_source {
            if cfg!(windows) {
                format!("Set-Content -LiteralPath '{marker}' -Value 'profile changed this input'")
            } else {
                format!("printf 'profile changed this input\\n' > '{marker}'")
            }
        } else if cfg!(windows) {
            "$null = 1".to_string()
        } else {
            ":".to_string()
        };
        shell.default_profile = Some("source-test".into());
        shell.profiles.insert(
            "source-test".into(),
            ShellProfileConfig {
                init_script: Some(init_script),
                ..Default::default()
            },
        );
        let manager = JobManager::new(1);
        let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
        manager.enqueue(
            sink,
            PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
        );
        assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(15)));
        let snapshot = lock_unpoison(&manager.jobs)[&job_id].snapshot.clone();
        manager.stop_all();
        if change_source {
            assert_eq!(snapshot.status, "failed", "{backend}: {snapshot:?}");
            assert_eq!(
                snapshot.command_execution_state,
                Some(ShellCommandExecutionState::NotStarted),
                "{backend}: {snapshot:?}"
            );
            assert!(
                snapshot
                    .error
                    .as_deref()
                    .is_some_and(|error| error.contains("validation_plan_stale")),
                "{backend}: {snapshot:?}"
            );
            if backend == "go" {
                let captured = std::fs::read_to_string(&capture).unwrap_or_default();
                assert!(
                    !captured.contains("ARGV=vet"),
                    "stale Go vet subprocess was launched: {captured}"
                );
            }
        } else {
            assert_eq!(snapshot.status, "completed", "{backend}: {snapshot:?}");
            assert_eq!(snapshot.exit_code, Some(0), "{backend}: {snapshot:?}");
            if backend == "go" {
                assert!(
                    std::fs::read_to_string(&capture)
                        .unwrap()
                        .contains("ARGV=vet"),
                    "fresh Go validation must run"
                );
            } else {
                assert!(snapshot.stdout.tail.contains("JOB_STDIN_EOF"));
            }
        }
    }
}

#[test]
fn project_validation_missing_clippy_keeps_prestart_progress() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    let registry = temp.path().join("registry");
    let bin = temp.path().join("bin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&registry).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='missing_clippy_fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
    )
    .unwrap();
    std::fs::write(
        registry.join("demo.toml"),
        format!(
            "id='demo'\nname='Demo'\npath={}\nallow_patch=true\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
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
        action: ProjectValidationAction::Lint,
        adapter: ProjectValidationAdapter::Rust,
        scope: None,
        dependency_policy: None,
        dependency_resolution_policy: None,
        test: None,
    };
    let (plan, cwd) =
        crate::webcodex_runner::validation::project::plan(&policy, &registry, &request).unwrap();
    let metadata = ShellJobValidationMetadata {
        project_validation: Some(plan.provenance),
        source_fence: None,
        tool: "project_validate".into(),
        kind: "lint".into(),
        steps: vec![plan.step.clone()],
        effective_timeout_secs: 30,
        sync_wait_secs: 8,
        adapter: plan.adapter,
        validation_target_id: Some(plan.validation_target_id),
        minimum_tests: None,
        require_tests: None,
        no_run: None,
    };
    let context = ShellJobContext {
        runtime_project_id: Some("agent:validation-agent:demo".into()),
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: Some(".".into()),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        purpose: Some("validation".into()),
        shell: Some("configured".into()),
        command_preview: "validation: lint".into(),
        validation_steps: vec![plan.step.name.clone()],
        validation: Some(metadata),
        structured_execution: None,
    };
    let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
        job_id: "clippy-missing-job".into(),
        cwd: Some(cwd.to_string_lossy().into_owned()),
        steps: vec![plan.step],
        timeout_secs: 30,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "clippy-missing-request".into(),
            client_id: "validation-agent".into(),
            requested_by: "test".into(),
            created_at: chrono::Utc::now().timestamp(),
        },
        RunnerOperation::Job(operation),
    )
    .unwrap();
    let mut shell = ShellConfig::default();
    shell
        .env
        .insert("PATH".into(), bin.to_string_lossy().into_owned());
    let manager = JobManager::new(1);
    let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
    manager.enqueue(
        sink,
        PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
    );
    assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(12)));
    let snapshot = lock_unpoison(&manager.jobs)["clippy-missing-job"]
        .snapshot
        .clone();
    manager.stop_all();
    assert_eq!(
        snapshot.command_execution_state,
        Some(ShellCommandExecutionState::NotStarted)
    );
    assert_eq!(snapshot.exit_code, None);
    assert_eq!(
        snapshot.error.as_deref(),
        Some(VALIDATION_TOOL_UNAVAILABLE_CODE)
    );
    let progress = snapshot
        .validation_progress
        .expect("prestart tool failure must retain canonical validation progress");
    assert_eq!(progress.completed, 0);
    assert!(progress.current_step.is_none());
    assert!(progress.failed_step.is_none());
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "requires installed Cargo Clippy; run explicitly with --ignored"]
fn runner_real_process_job_manager_clippy_nested_project_default_and_offline() {
    use webcodex_core::project_validation::ProjectDependencyResolutionPolicy;
    let located = Command::new("rustup")
        .env("RUSTUP_AUTO_INSTALL", "0")
        .args(["which", "cargo-clippy"])
        .stdin(std::process::Stdio::null())
        .output()
        .expect("installed rustup and Clippy required");
    assert!(located.status.success());
    let clippy = PathBuf::from(String::from_utf8(located.stdout).unwrap().trim());
    let toolchain = clippy.parent().unwrap();
    let path = std::env::join_paths(std::iter::once(toolchain.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();
    for offline in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("registered-project");
        let cwd = root.join("clippy-fixture/cached");
        let registry = temp.path().join("registry");
        let cargo_home = temp.path().join("cargo-home");
        for dir in [&cwd.join("src"), &registry, &cargo_home] {
            std::fs::create_dir_all(dir).unwrap();
        }
        std::fs::write(
            cwd.join("Cargo.toml"),
            "[package]\nname='clippy_job_fixture'\nversion='0.1.0'\nedition='2021'\n[workspace]\n",
        )
        .unwrap();
        std::fs::write(
            cwd.join("src/lib.rs"),
            "pub fn value() -> u32 { return 1; }\n",
        )
        .unwrap();
        let lock = Command::new(toolchain.join("cargo"))
            .args(["generate-lockfile", "--offline"])
            .current_dir(&cwd)
            .env("PATH", &path)
            .env("CARGO_HOME", &cargo_home)
            .env("RUSTUP_AUTO_INSTALL", "0")
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(lock.status.success(), "{lock:?}");
        std::fs::write(
            registry.join("demo.toml"),
            format!(
                "id='demo'\nname='Demo'\npath={}\nallow_patch=true\n",
                serde_json::to_string(root.to_str().unwrap()).unwrap()
            ),
        )
        .unwrap();
        let policy = RunnerPolicy {
            allowed_roots: vec![root.clone()],
            ..Default::default()
        };
        let request = ProjectValidationRequest {
            project_id: "demo".into(),
            cwd: Some("clippy-fixture/cached".into()),
            action: ProjectValidationAction::Lint,
            adapter: ProjectValidationAdapter::Rust,
            scope: None,
            dependency_policy: None,
            dependency_resolution_policy: offline
                .then_some(ProjectDependencyResolutionPolicy::Offline),
            test: None,
        };
        let (plan, resolved_cwd) =
            crate::webcodex_runner::validation::project::plan(&policy, &registry, &request)
                .unwrap();
        let metadata = ShellJobValidationMetadata {
            tool: "project_validate".into(),
            kind: "lint".into(),
            adapter: plan.adapter.clone(),
            steps: vec![plan.step.clone()],
            effective_timeout_secs: 30,
            sync_wait_secs: 10,
            validation_target_id: Some(plan.validation_target_id),
            source_fence: Some(webcodex_core::validation_source::ValidationSourceFence {
                epoch: "a".repeat(32),
                generation: 0,
                quiescent: true,
            }),
            project_validation: Some(plan.provenance),
            minimum_tests: None,
            require_tests: None,
            no_run: None,
        };
        assert!(metadata.is_valid());
        let operation = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
            job_id: "clippy-native-job".into(),
            cwd: Some(resolved_cwd.to_string_lossy().into_owned()),
            steps: vec![plan.step],
            timeout_secs: 30,
            context: ShellJobContext {
                runtime_project_id: Some("agent:validation-agent:demo".into()),
                workflow_session_id: None,
                ssh_resource: None,
                project_cwd: request.cwd.clone(),
                cwd: Some(resolved_cwd.to_string_lossy().into_owned()),
                purpose: Some("validation".into()),
                shell: Some("configured".into()),
                command_preview: "validation: lint".into(),
                validation_steps: vec!["lint".into()],
                validation: Some(metadata),
                structured_execution: None,
            },
        });
        let wire = RunnerRequest::from_operation(
            RunnerInvocationMetadata {
                request_id: "clippy-native-request".into(),
                client_id: "validation-agent".into(),
                requested_by: "test".into(),
                created_at: chrono::Utc::now().timestamp(),
            },
            RunnerOperation::Job(operation),
        )
        .unwrap();
        let wire: RunnerRequest =
            serde_json::from_slice(&serde_json::to_vec(&wire).unwrap()).unwrap();
        let decoded = wire.decode_operation();
        assert!(
            decoded.is_ok(),
            "actual wire JSON must preserve typed validation: {decoded:?}"
        );
        let mut shell = ShellConfig::default();
        shell
            .env
            .insert("PATH".into(), path.to_string_lossy().into_owned());
        shell.env.insert(
            "CARGO_HOME".into(),
            cargo_home.to_string_lossy().into_owned(),
        );
        shell.env.insert(
            "CARGO_TARGET_DIR".into(),
            temp.path().join("target").to_string_lossy().into_owned(),
        );
        let manager = JobManager::new(1);
        let (sink, _rx) = structured_test_sink("validation-agent", "validation-instance");
        manager.enqueue(
            sink,
            PendingJobStart::from_wire(1, policy, shell, SshConfig::default(), registry, wire),
        );
        assert!(manager.wait_for_workers(Instant::now() + Duration::from_secs(40)));
        let inventory = manager.inventory();
        assert_eq!(inventory.jobs.len(), 1, "{inventory:?}");
        let job = &inventory.jobs[0];
        assert_eq!(job.status, "completed", "offline={offline}: {job:?}");
        assert_eq!(job.exit_code, Some(0), "{job:?}");
        assert!(
            job.stderr.tail.contains("clippy::needless_return"),
            "{job:?}"
        );
        assert_eq!(
            job.context.validation.as_ref().unwrap().adapter,
            "cargo_clippy"
        );
    }
}
