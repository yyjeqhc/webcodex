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
        let canonical_adapter = webcodex_validation::validation_adapter_for_recipe(
            plan.provenance.backend.as_str(),
            semantic_check,
        )
        .unwrap();
        let canonical_options = if plan.provenance.backend == "rust" && action == FormatCheck {
            webcodex_validation::ValidationCommandOptions {
                check: true,
                ..Default::default()
            }
        } else {
            webcodex_validation::ValidationCommandOptions::default()
        };
        let canonical_plan = canonical_adapter
            .build_readonly_plan(canonical_options)
            .unwrap();
        assert_eq!(
            plan.step, canonical_plan.structured_step,
            "{adapter} gateway plan must use the canonical adapter step"
        );
        let invocation_digest = format!(
            "{:x}",
            sha2::Sha256::digest(
                serde_json::to_vec(&vec![canonical_plan.structured_step]).unwrap()
            )
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
    for (marker, backend) in [("package.json", "node"), ("pyproject.toml", "python")] {
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
