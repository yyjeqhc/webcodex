use super::*;
use std::fs;
use webcodex_core::project_validation::*;

fn fixture(marker: &str) -> (tempfile::TempDir, PathBuf, PathBuf, RunnerPolicy) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("remote-project");
    let registry = tmp.path().join("registry");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&registry).unwrap();
    fs::write(root.join(marker), "").unwrap();
    fs::write(
        registry.join("demo.toml"),
        format!(
            "id = 'demo'\nname = 'Demo'\npath = {}\nallow_patch = true\n",
            serde_json::to_string(root.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let policy = RunnerPolicy {
        allowed_roots: vec![root.clone()],
        ..Default::default()
    };
    (tmp, root, registry, policy)
}
fn request(action: ProjectValidationAction) -> ProjectValidationRequest {
    ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: None,
        action,
        adapter: ProjectValidationAdapter::Auto,
        scope: None,
        dependency_policy: None,
        test: None,
    }
}

#[test]
fn project_validation_runner_plans_locked_dependency_policy_for_rust_and_go() {
    for (marker, adapter, expected) in [
        (
            "Cargo.toml",
            ProjectValidationAdapter::Rust,
            vec!["check", "--locked", "--all-targets"],
        ),
        (
            "go.mod",
            ProjectValidationAdapter::Go,
            vec!["vet", "-mod=readonly", "./..."],
        ),
    ] {
        let (_tmp, _root, registry, policy) = fixture(marker);
        let mut locked = request(ProjectValidationAction::Check);
        locked.adapter = adapter;
        locked.dependency_policy = Some(ProjectDependencyPolicy {
            mode: ProjectDependencyMode::Locked,
        });
        let (plan, _) = project::plan(&policy, &registry, &locked).unwrap();
        assert_eq!(plan.provenance.request, locked);
        assert_eq!(plan.step.args, expected);

        let mut default = locked.clone();
        default.dependency_policy = None;
        let (default_plan, _) = project::plan(&policy, &registry, &default).unwrap();
        assert_ne!(plan.validation_target_id, default_plan.validation_target_id);
        assert_ne!(
            plan.provenance.invocation_digest,
            default_plan.provenance.invocation_digest
        );
    }
}

#[test]
fn project_validation_runner_resolves_all_production_actions() {
    use sha2::Digest;
    use ProjectValidationAction::*;
    for (marker, action, adapter, args) in [
        (
            "Cargo.toml",
            FormatCheck,
            "cargo_fmt",
            vec!["fmt", "--", "--check"],
        ),
        (
            "Cargo.toml",
            Check,
            "cargo_check",
            vec!["check", "--all-targets"],
        ),
        ("Cargo.toml", Test, "cargo_test", vec!["test"]),
        ("go.mod", Check, "go_vet", vec!["vet", "./..."]),
        ("go.mod", Test, "go_test", vec!["test", "-json", "./..."]),
        (
            "pyproject.toml",
            Test,
            "python:pytest:test",
            vec!["-m", "pytest", "--color=no", "-rA"],
        ),
    ] {
        let (_tmp, root, registry, policy) = fixture(marker);
        let (plan, cwd) = project::plan(&policy, &registry, &request(action)).unwrap();
        assert_eq!(cwd.canonicalize().unwrap(), root.canonicalize().unwrap());
        assert_eq!(plan.adapter, adapter);
        assert_eq!(plan.step.args, args);

        let semantic_check = match action {
            FormatCheck => webcodex_validation::SemanticCheck::Format,
            Check => webcodex_validation::SemanticCheck::Check,
            Test => webcodex_validation::SemanticCheck::Test,
        };
        let resolved = webcodex_validation::resolve_project_validation_recipe(
            &root,
            None,
            None,
            &[semantic_check],
            None,
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(
            std::slice::from_ref(&plan.step),
            resolved.steps.as_slice(),
            "{adapter} gateway plan must use the resolved recipe step"
        );
        let invocation_digest = format!(
            "{:x}",
            sha2::Sha256::digest(serde_json::to_vec(&resolved.steps).unwrap())
        );
        assert_eq!(
            plan.provenance.invocation_digest, invocation_digest,
            "{adapter} provenance must bind the canonical adapter step"
        );
        assert!(plan.provenance.is_valid());
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains(root.to_str().unwrap()));
    }
}
#[test]
fn project_validation_recipe_step_and_identity_preserve_normalized_selection() {
    for (marker, backend, tool, filter, packages) in [
        (
            "Cargo.toml",
            "rust",
            "cargo_test",
            "selected",
            vec!["b", "a", "a"],
        ),
        (
            "go.mod",
            "go",
            "go_test",
            " ^TestA/sub$ ",
            vec!["./b", "./a", "./a"],
        ),
        (
            "pyproject.toml",
            "python",
            "python:pytest:test",
            " selected and not slow ",
            vec![],
        ),
    ] {
        let (_tmp, root, registry, policy) = fixture(marker);
        let mut req = request(ProjectValidationAction::Test);
        req.test = Some(ProjectValidationTestOptions {
            filter: Some(if backend == "rust" {
                format!("  {filter}  ")
            } else {
                filter.into()
            }),
            ..Default::default()
        });
        if !packages.is_empty() {
            req.scope = Some(ProjectValidationScope {
                packages: packages.into_iter().map(str::to_string).collect(),
                all_packages: false,
            });
            req.dependency_policy = Some(ProjectDependencyPolicy {
                mode: ProjectDependencyMode::Locked,
            });
        }
        let (plan, _) = project::plan(&policy, &registry, &req).unwrap();
        let resolved = webcodex_validation::resolve_project_validation_recipe(
            &root,
            None,
            None,
            &[webcodex_validation::SemanticCheck::Test],
            req.test.as_ref().unwrap().filter.as_deref(),
            req.scope
                .as_ref()
                .and_then(ProjectValidationScope::explicit_packages),
            false,
            req.dependency_policy,
        )
        .unwrap();
        assert_eq!(std::slice::from_ref(&plan.step), resolved.steps.as_slice());
        assert_eq!(plan.adapter, tool);
        assert_eq!(
            plan.provenance.invocation_digest,
            resolved.invocation_digest
        );

        req.test.as_mut().unwrap().filter = Some(filter.into());
        if backend == "rust" {
            let scope = req.scope.as_mut().unwrap();
            scope.packages.sort();
            scope.packages.dedup();
        }
        let (normalized, _) = project::plan(&policy, &registry, &req).unwrap();
        assert_eq!(plan.step, normalized.step);
        assert_eq!(plan.validation_target_id, normalized.validation_target_id);
        let operation = webcodex_validation::project_validation_operation(
            backend,
            webcodex_validation::SemanticCheck::Test,
            req.scope
                .as_ref()
                .and_then(ProjectValidationScope::explicit_packages)
                .map(<[String]>::to_vec),
            false,
        )
        .unwrap()
        .with_dependency_policy(req.dependency_policy)
        .unwrap()
        .with_test_filter(Some(filter))
        .unwrap();
        let identity = operation
            .validation_target_id(Some(&resolved.recipe_root_relative))
            .unwrap();
        let expected = if backend == "go" {
            webcodex_core::validation_identity::contextualize_structured_validation_target_identity(
                &identity,
                webcodex_core::validation_identity::StructuredValidationExecutionContext::GoProjectSingleModuleV1,
            ).unwrap()
        } else {
            identity
        };
        assert_eq!(plan.validation_target_id, expected);
    }
}

#[test]
fn go_project_validation_identity_is_single_module_domain_separated() {
    for action in [
        ProjectValidationAction::Check,
        ProjectValidationAction::Test,
    ] {
        let (_tmp, _root, registry, policy) = fixture("go.mod");
        let (plan, _) = project::plan(&policy, &registry, &request(action)).unwrap();
        let semantic = match action {
            ProjectValidationAction::Check => webcodex_validation::SemanticCheck::Check,
            ProjectValidationAction::Test => webcodex_validation::SemanticCheck::Test,
            ProjectValidationAction::FormatCheck => unreachable!(),
        };
        let operation =
            webcodex_validation::project_validation_operation("go", semantic, None, false).unwrap();
        let native_identity = operation
            .validation_target_id(Some(&plan.provenance.recipe_root))
            .unwrap();
        let expected = webcodex_core::validation_identity::contextualize_structured_validation_target_identity(
            &native_identity,
            webcodex_core::validation_identity::StructuredValidationExecutionContext::GoProjectSingleModuleV1,
        )
        .unwrap();
        assert_eq!(plan.validation_target_id, expected);
        assert_ne!(plan.validation_target_id, native_identity);
    }
}

#[test]
fn project_validation_package_scope_maps_through_canonical_operations() {
    use ProjectValidationAction::*;
    let cases = [
        (
            "Cargo.toml",
            Check,
            vec!["package-b", "package-a"],
            "cargo_check",
            vec![
                "check",
                "--all-targets",
                "-p",
                "package-a",
                "-p",
                "package-b",
            ],
        ),
        (
            "Cargo.toml",
            Test,
            vec!["package-b", "package-a"],
            "cargo_test",
            vec!["test", "-p", "package-a", "-p", "package-b"],
        ),
        (
            "go.mod",
            Check,
            vec!["./cmd/...", "./internal"],
            "go_vet",
            vec!["vet", "./cmd/...", "./internal"],
        ),
        (
            "go.mod",
            Test,
            vec!["./cmd/...", "./internal"],
            "go_test",
            vec!["test", "-json", "./cmd/...", "./internal"],
        ),
    ];
    for (marker, action, packages, adapter, expected_args) in cases {
        let (_tmp, _root, registry, policy) = fixture(marker);
        let mut req = request(action);
        req.scope = Some(ProjectValidationScope {
            packages: packages.into_iter().map(str::to_string).collect(),
            all_packages: false,
        });
        let (plan, _) = project::plan(&policy, &registry, &req).unwrap();
        assert_eq!(plan.adapter, adapter);
        assert_eq!(plan.step.args, expected_args);
        assert_eq!(plan.provenance.request, req);
        let semantic = match action {
            FormatCheck => webcodex_validation::SemanticCheck::Format,
            Check => webcodex_validation::SemanticCheck::Check,
            Test => webcodex_validation::SemanticCheck::Test,
        };
        let operation = webcodex_validation::project_validation_operation(
            plan.provenance.backend.as_str(),
            semantic,
            plan.provenance
                .request
                .scope
                .as_ref()
                .and_then(ProjectValidationScope::explicit_packages)
                .map(<[String]>::to_vec),
            plan.provenance
                .request
                .scope
                .as_ref()
                .is_some_and(ProjectValidationScope::selects_all_packages),
        )
        .unwrap();
        let native_identity = operation
            .validation_target_id(Some(&plan.provenance.recipe_root))
            .unwrap();
        let expected_identity = if plan.provenance.backend == "go" {
            webcodex_core::validation_identity::contextualize_structured_validation_target_identity(
                &native_identity,
                webcodex_core::validation_identity::StructuredValidationExecutionContext::GoProjectSingleModuleV1,
            )
            .unwrap()
        } else {
            native_identity.clone()
        };
        assert_eq!(plan.validation_target_id, expected_identity);
        if plan.provenance.backend == "go" {
            assert_ne!(plan.validation_target_id, native_identity);
        }
        assert_eq!(
            plan.step,
            operation.build_readonly_plan().unwrap().structured_step
        );
    }
}

#[test]
fn project_validation_package_scope_fails_closed_when_action_or_backend_scope_is_invalid() {
    let (_tmp, _root, registry, policy) = fixture("Cargo.toml");
    let mut format = request(ProjectValidationAction::FormatCheck);
    format.scope = Some(ProjectValidationScope {
        packages: vec!["package-a".into()],
        all_packages: false,
    });
    assert!(matches!(
        project::plan(&policy, &registry, &format),
        Err(ProjectValidationPlanningResult::Unavailable { code, .. })
            if code == "validation_scope_unsupported"
    ));

    let mut invalid_rust = request(ProjectValidationAction::Check);
    invalid_rust.scope = Some(ProjectValidationScope {
        packages: vec!["-bad".into()],
        all_packages: false,
    });
    assert!(matches!(
        project::plan(&policy, &registry, &invalid_rust),
        Err(ProjectValidationPlanningResult::Unavailable { code, .. })
            if code == "validation_scope_invalid"
    ));

    let (_tmp, _root, registry, policy) = fixture("go.mod");
    let mut invalid_go = request(ProjectValidationAction::Test);
    invalid_go.scope = Some(ProjectValidationScope {
        packages: vec!["not-relative".into()],
        all_packages: false,
    });
    assert!(matches!(
        project::plan(&policy, &registry, &invalid_go),
        Err(ProjectValidationPlanningResult::Unavailable { code, .. })
            if code == "validation_scope_invalid"
    ));
}

#[test]
fn project_validation_nearest_root_hint_and_ambiguity() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/go.mod"), "module demo\n").unwrap();
    let mut req = request(ProjectValidationAction::Check);
    req.cwd = Some("nested".into());
    let (plan, _) = project::plan(&policy, &registry, &req).unwrap();
    assert_eq!(plan.provenance.backend, "go");
    assert_eq!(plan.provenance.recipe_root, "nested");
    req.adapter = ProjectValidationAdapter::Rust;
    assert!(
        matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable { code, .. }) if code == "validation_recipe_mismatch")
    );
    req.adapter = ProjectValidationAdapter::Go;
    assert!(project::plan(&policy, &registry, &req).is_ok());
    fs::write(root.join("nested/Cargo.toml"), "").unwrap();
    req.adapter = ProjectValidationAdapter::Auto;
    assert!(
        matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable { code, .. }) if code == "validation_recipe_ambiguous")
    );
}
#[test]
fn project_validation_deferred_backends_do_not_resolve_scripts() {
    for (marker, backend) in [("package.json", "node")] {
        let (_tmp, _root, registry, policy) = fixture(marker);
        assert_eq!(
            project::plan(&policy, &registry, &request(ProjectValidationAction::Test)).unwrap_err(),
            ProjectValidationPlanningResult::Unavailable {
                code: "validation_adapter_unavailable".into(),
                detected_backend: Some(backend.into())
            }
        );
    }
    let (_tmp, _root, registry, policy) = fixture("go.mod");
    assert!(
        matches!(project::plan(&policy, &registry, &request(ProjectValidationAction::FormatCheck)), Err(ProjectValidationPlanningResult::Unavailable { code, .. }) if code == "validation_action_unsupported")
    );
}
#[test]
fn project_validation_rejects_escape_and_unknown_project() {
    let (_tmp, _root, registry, policy) = fixture("Cargo.toml");
    for cwd in ["../", "/tmp", "C:/tmp", "a/../../b", "\\\\host\\share"] {
        let mut req = request(ProjectValidationAction::Check);
        req.cwd = Some(cwd.into());
        assert!(project::plan(&policy, &registry, &req).is_err());
    }
    let mut req = request(ProjectValidationAction::Check);
    req.project_id = "missing".into();
    assert!(project::plan(&policy, &registry, &req).is_err());
}
#[test]
fn project_validation_test_options_fence_selection_and_count_policy_separately() {
    use webcodex_core::runner_protocol::ShellJobValidationMetadata;
    for (marker, filter, expected) in [
        ("Cargo.toml", " selected ", vec!["test", "selected"]),
        (
            "go.mod",
            "^TestA/sub.*$",
            vec!["test", "-json", "-run", "^TestA/sub.*$", "./..."],
        ),
    ] {
        let (_tmp, _root, registry, policy) = fixture(marker);
        let mut req = request(ProjectValidationAction::Test);
        let (plain, _) = project::plan(&policy, &registry, &req).unwrap();
        req.test = Some(ProjectValidationTestOptions {
            filter: Some(filter.into()),
            require_tests: Some(false),
            min_tests: Some(3),
        });
        let (selected, _) = project::plan(&policy, &registry, &req).unwrap();
        assert_eq!(selected.step.args, expected);
        assert_ne!(selected.validation_target_id, plain.validation_target_id);
        assert_ne!(
            selected.provenance.invocation_digest,
            plain.provenance.invocation_digest
        );
        let mut metadata = ShellJobValidationMetadata {
            project_validation: Some(selected.provenance.clone()),
            tool: "project_validate".into(),
            kind: "test".into(),
            adapter: selected.adapter,
            steps: vec![selected.step],
            effective_timeout_secs: 60,
            sync_wait_secs: 5,
            validation_target_id: Some(selected.validation_target_id.clone()),
            source_fence: None,
            minimum_tests: Some(3),
            require_tests: Some(false),
            no_run: None,
        };
        assert!(metadata.is_valid());
        assert_eq!(
            serde_json::from_str::<ShellJobValidationMetadata>(
                &serde_json::to_string(&metadata).unwrap()
            )
            .unwrap(),
            metadata
        );
        metadata.minimum_tests = Some(1);
        assert!(!metadata.is_valid());
        metadata.minimum_tests = Some(3);
        metadata.require_tests = Some(true);
        assert!(!metadata.is_valid());
        metadata.require_tests = Some(false);
        metadata.no_run = Some(true);
        assert!(!metadata.is_valid());
        // Policy changes fence a request/Job but do not pretend to alter argv.
        req.test.as_mut().unwrap().min_tests = Some(9);
        let (other_minimum, _) = project::plan(&policy, &registry, &req).unwrap();
        assert_eq!(
            other_minimum.validation_target_id,
            selected.validation_target_id
        );
        assert_eq!(
            other_minimum.provenance.invocation_digest,
            selected.provenance.invocation_digest
        );
        assert_ne!(other_minimum.provenance, selected.provenance);
        req.action = ProjectValidationAction::Check;
        assert!(project::plan(&policy, &registry, &req).is_err());
    }
}

#[test]
fn project_validation_manifest_fence_and_exact_recovery_plan() {
    use webcodex_core::runner_operation::{RunnerJobOperation, RunnerJobValidationOperation};
    use webcodex_core::runner_protocol::{ShellJobContext, ShellJobValidationMetadata};
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    let (plan, cwd) =
        project::plan(&policy, &registry, &request(ProjectValidationAction::Check)).unwrap();
    let metadata = ShellJobValidationMetadata {
        project_validation: Some(plan.provenance.clone()),
        tool: "project_validate".into(),
        kind: "check".into(),
        adapter: plan.adapter,
        steps: vec![plan.step.clone()],
        effective_timeout_secs: 60,
        sync_wait_secs: 10,
        validation_target_id: Some(plan.validation_target_id),
        source_fence: None,
        minimum_tests: None,
        require_tests: None,
        no_run: None,
    };
    assert!(metadata.is_valid());
    let encoded = serde_json::to_string(&metadata).unwrap();
    let restored: ShellJobValidationMetadata = serde_json::from_str(&encoded).unwrap();
    let op = RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
        job_id: "job".into(),
        cwd: Some(cwd.to_str().unwrap().into()),
        steps: restored.steps.clone(),
        timeout_secs: 60,
        context: ShellJobContext {
            runtime_project_id: Some("agent:runner:demo".into()),
            validation: Some(restored.clone()),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(".".into()),
            cwd: Some(cwd.to_str().unwrap().into()),
            purpose: Some("validation".into()),
            shell: None,
            command_preview: "cargo check --all-targets".into(),
            validation_steps: vec!["check".into()],
            structured_execution: None,
        },
    });
    project::fence(&policy, &registry, &op).unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    assert!(project::fence(&policy, &registry, &op)
        .unwrap_err()
        .contains("validation_plan_stale"));
    fs::remove_file(root.join("Cargo.toml")).unwrap();
    fs::write(root.join("go.mod"), "module replacement\n").unwrap();
    assert!(project::fence(&policy, &registry, &op).is_err());
    assert_eq!(restored.steps[0].program, "cargo");
    assert_eq!(restored.project_validation.unwrap().backend, "rust");
}

#[test]
fn project_validation_python_pytest_detects_explicit_and_auto_and_fences_config() {
    let (_tmp, root, registry, policy) = fixture("pyproject.toml");
    let mut req = request(ProjectValidationAction::Test);
    let (baseline, _) = project::plan(&policy, &registry, &req).unwrap();
    assert_eq!(baseline.adapter, "python:pytest:test");
    assert_eq!(baseline.provenance.backend, "python");
    req.adapter = ProjectValidationAdapter::Python;
    let (explicit, _) = project::plan(&policy, &registry, &req).unwrap();
    assert_eq!(explicit.step, baseline.step);
    assert_eq!(explicit.validation_target_id, baseline.validation_target_id);
    req.test = Some(ProjectValidationTestOptions {
        filter: Some("selected and not slow".into()),
        ..Default::default()
    });
    let (filtered, _) = project::plan(&policy, &registry, &req).unwrap();
    assert!(filtered.step.is_structured_pytest());
    assert_ne!(filtered.validation_target_id, baseline.validation_target_id);
    fs::write(root.join("pytest.ini"), "[pytest]\naddopts = -q\n").unwrap();
    let (changed, _) = project::plan(&policy, &registry, &req).unwrap();
    assert_ne!(
        changed.provenance.manifest_digest,
        filtered.provenance.manifest_digest
    );
    req.scope = Some(ProjectValidationScope {
        packages: vec!["tests".into()],
        all_packages: false,
    });
    assert!(
        matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable {code,..}) if code == "validation_scope_unsupported")
    );
    req.scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });
    assert!(
        matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable {code,..}) if code == "validation_scope_unsupported")
    );
    req.scope = None;
    for action in [
        ProjectValidationAction::Check,
        ProjectValidationAction::FormatCheck,
    ] {
        req.action = action;
        req.test = None;
        assert!(
            matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable {code,..}) if code == "validation_action_unsupported")
        );
    }
    fs::remove_file(root.join("pyproject.toml")).unwrap();
    req.action = ProjectValidationAction::Test;
    assert!(project::plan(&policy, &registry, &req)
        .unwrap()
        .0
        .step
        .is_structured_pytest());
}

fn start_project_validation_operation(
    plan: &ProjectValidationPlan,
    cwd: &std::path::Path,
) -> webcodex_core::runner_operation::RunnerJobOperation {
    use webcodex_core::runner_operation::{RunnerJobOperation, RunnerJobValidationOperation};
    use webcodex_core::runner_protocol::{ShellJobContext, ShellJobValidationMetadata};

    let kind = match plan.provenance.request.action {
        ProjectValidationAction::FormatCheck => "format",
        ProjectValidationAction::Check => "check",
        ProjectValidationAction::Test => "test",
    };
    let metadata = ShellJobValidationMetadata {
        project_validation: Some(plan.provenance.clone()),
        tool: "project_validate".into(),
        kind: kind.into(),
        adapter: plan.adapter.clone(),
        steps: vec![plan.step.clone()],
        effective_timeout_secs: 60,
        sync_wait_secs: 10,
        validation_target_id: Some(plan.validation_target_id.clone()),
        source_fence: None,
        minimum_tests: None,
        require_tests: None,
        no_run: None,
    };
    assert!(metadata.is_valid());

    RunnerJobOperation::StartValidation(RunnerJobValidationOperation {
        job_id: "job".into(),
        cwd: Some(cwd.to_str().unwrap().into()),
        steps: vec![plan.step.clone()],
        timeout_secs: 60,
        context: ShellJobContext {
            runtime_project_id: Some("agent:runner:demo".into()),
            validation: Some(metadata),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(plan.provenance.recipe_root.clone()),
            cwd: Some(cwd.to_str().unwrap().into()),
            purpose: Some("validation".into()),
            shell: None,
            command_preview: "cargo check --workspace".into(),
            validation_steps: vec!["check".into()],
            structured_execution: None,
        },
    })
}

#[test]
fn project_validation_all_packages_target_member_change_fails_queue_fence() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['target/member']\nresolver='2'\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("target/member")).unwrap();
    fs::write(
        root.join("target/member/Cargo.toml"),
        "[package]\nname='member'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();

    let mut req = request(ProjectValidationAction::Check);
    req.adapter = ProjectValidationAdapter::Rust;
    req.scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });
    let (plan, cwd) = project::plan(&policy, &registry, &req).unwrap();
    assert!(plan.step.args.iter().any(|arg| arg == "--workspace"));
    let operation = start_project_validation_operation(&plan, &cwd);
    project::fence(&policy, &registry, &operation).unwrap();

    fs::write(
        root.join("target/member/Cargo.toml"),
        "[package]\nname='member'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    assert!(project::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("validation_plan_stale"));
}

#[cfg(unix)]
#[test]
fn project_validation_all_packages_member_symlink_retarget_fails_queue_fence() {
    use std::os::unix::fs::symlink;

    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['selected']\nresolver='2'\n",
    )
    .unwrap();
    for package in ["a", "b"] {
        fs::create_dir_all(root.join(package)).unwrap();
        fs::write(
            root.join(package).join("Cargo.toml"),
            "[package]\nname='member'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
    }
    symlink("a", root.join("selected")).unwrap();

    let mut req = request(ProjectValidationAction::Check);
    req.adapter = ProjectValidationAdapter::Rust;
    req.scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });
    let (plan, cwd) = project::plan(&policy, &registry, &req).unwrap();
    let operation = start_project_validation_operation(&plan, &cwd);
    project::fence(&policy, &registry, &operation).unwrap();

    fs::remove_file(root.join("selected")).unwrap();
    symlink("b", root.join("selected")).unwrap();
    assert!(project::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("validation_plan_stale"));
}

#[cfg(unix)]
#[test]
fn project_validation_all_packages_recursive_glob_cycle_fails_queue_fence() {
    use std::os::unix::fs::symlink;

    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers=['a/**/target']\nresolver='2'\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("a/member/target")).unwrap();
    fs::write(
        root.join("a/member/target/Cargo.toml"),
        "[package]\nname='member'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();

    let mut req = request(ProjectValidationAction::Check);
    req.adapter = ProjectValidationAdapter::Rust;
    req.scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });
    let (plan, cwd) = project::plan(&policy, &registry, &req).unwrap();
    let operation = start_project_validation_operation(&plan, &cwd);
    project::fence(&policy, &registry, &operation).unwrap();

    // The queued plan must not run after its recursive membership witness
    // becomes incomplete, even though no manifest bytes have changed.
    symlink("..", root.join("a/member/back")).unwrap();
    assert!(project::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("validation_plan_stale"));
}

#[test]
fn project_validation_all_packages_parent_workspace_appearance_fails_queue_fence() {
    let (tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='standalone'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    let mut req = request(ProjectValidationAction::Check);
    req.adapter = ProjectValidationAdapter::Rust;
    req.scope = Some(ProjectValidationScope {
        packages: Vec::new(),
        all_packages: true,
    });
    let (plan, cwd) = project::plan(&policy, &registry, &req).unwrap();
    let operation = start_project_validation_operation(&plan, &cwd);
    project::fence(&policy, &registry, &operation).unwrap();

    // The new parent manifest changes Cargo's project unit without changing
    // any bytes inside the registered Project. Replanning must reject it.
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers=['remote-project']\nresolver='2'\n",
    )
    .unwrap();
    assert!(project::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("validation_plan_stale"));
}

#[test]
fn project_validation_all_packages_rejects_excluded_recipe_cwd() {
    for action in [
        ProjectValidationAction::Check,
        ProjectValidationAction::Test,
    ] {
        let (_tmp, root, registry, policy) = fixture("Cargo.toml");
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers=['app']\nexclude=['tool']\nresolver='2'\n",
        )
        .unwrap();
        for package in ["app", "tool"] {
            fs::create_dir_all(root.join(package)).unwrap();
            fs::write(
                root.join(package).join("Cargo.toml"),
                format!("[package]\nname='{package}'\nversion='0.1.0'\nedition='2021'\n"),
            )
            .unwrap();
        }
        let mut req = request(action);
        req.adapter = ProjectValidationAdapter::Rust;
        req.cwd = Some("tool".into());
        req.scope = Some(ProjectValidationScope {
            packages: Vec::new(),
            all_packages: true,
        });
        assert!(
            matches!(project::plan(&policy, &registry, &req), Err(ProjectValidationPlanningResult::Unavailable { code, .. }) if code == "validation_scope_unavailable")
        );
        req.cwd = Some("app".into());
        assert!(project::plan(&policy, &registry, &req).is_ok());
    }
}
