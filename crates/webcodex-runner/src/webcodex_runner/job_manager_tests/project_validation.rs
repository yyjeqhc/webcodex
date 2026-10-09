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
    for (marker, dependency, change_dependency, member_cwd, ruff_action) in [
        ("Cargo.toml", "Cargo.lock", false, false, None),
        ("Cargo.toml", "Cargo.lock", true, false, None),
        ("go.mod", "go.sum", false, false, None),
        ("go.mod", "go.sum", true, false, None),
        ("pyproject.toml", "pytest.ini", false, false, None),
        ("pyproject.toml", "pytest.ini", true, false, None),
        ("Cargo.toml", "Cargo.lock", false, true, None),
        ("Cargo.toml", "Cargo.lock", true, true, None),
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
                if ruff_action.is_some() {
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
            } else if marker == "pyproject.toml" {
                ProjectValidationAction::Test
            } else {
                ProjectValidationAction::Check
            },
            adapter: ProjectValidationAdapter::Auto,
            scope: None,
            dependency_policy: None,
            test: None,
        };
        let (plan, cwd) =
            crate::webcodex_runner::validation::project::plan(&policy, &registry, &request)
                .unwrap();

        let metadata = ShellJobValidationMetadata {
            project_validation: Some(plan.provenance.clone()),
            source_fence: None,
            tool: "project_validate".into(),
            kind: request.action.kind().into(),
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
