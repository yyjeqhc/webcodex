use super::config::RunnerPolicy;
use super::project_build;
use std::fs;
use std::path::PathBuf;
use webcodex_core::project_build::*;
use webcodex_core::runner_operation::{RunnerJobBuildOperation, RunnerJobOperation};
use webcodex_core::runner_protocol::{ShellJobContext, ShellJobStructuredExecutionMetadata};

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

fn request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Auto,
        scope: None,
    }
}

fn start_build_operation(plan: &ProjectBuildPlan, cwd: &std::path::Path) -> RunnerJobOperation {
    let structured = ShellJobStructuredExecutionMetadata {
        execution_source: "project_build".into(),
        language: None,
        script_bytes: None,
        arg_count: plan.process.args.len(),
        stdin_present: false,
        validation_identity: None,
        assertion_name: None,
        validation_tool: None,
    };
    RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
        job_id: "job".into(),
        cwd: Some(cwd.to_str().unwrap().into()),
        process: plan.process.clone(),
        provenance: plan.provenance.clone(),
        timeout_secs: 60,
        context: ShellJobContext {
            runtime_project_id: Some("agent:runner:demo".into()),
            workflow_session_id: None,
            ssh_resource: None,
            project_cwd: Some(plan.provenance.recipe_root.clone()),
            cwd: Some(cwd.to_str().unwrap().into()),
            purpose: Some("build".into()),
            shell: Some("direct_argv".into()),
            command_preview: match plan.provenance.backend.as_str() {
                "rust" => "cargo build".into(),
                "go" => "go build".into(),
                _ => unreachable!(),
            },
            validation_steps: Vec::new(),
            validation: None,
            structured_execution: Some(structured),
        },
    })
}

#[test]
fn project_build_runner_plans_rust_and_go_canonical_argv() {
    for (marker, backend, executable, args) in [
        ("Cargo.toml", "rust", "cargo", vec!["build"]),
        ("go.mod", "go", "go", vec!["build", "./..."]),
    ] {
        let (_tmp, root, registry, policy) = fixture(marker);
        let (plan, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
        assert_eq!(cwd.canonicalize().unwrap(), root.canonicalize().unwrap());
        assert_eq!(plan.provenance.backend, backend);
        assert_eq!(plan.process.executable, executable);
        assert_eq!(plan.process.args, args);
        assert!(plan.is_valid());
        assert!(!serde_json::to_string(&plan)
            .unwrap()
            .contains(root.to_str().unwrap()));
    }
}

#[test]
fn project_build_package_scope_maps_to_bounded_backend_argv() {
    let (_tmp, _root, registry, policy) = fixture("Cargo.toml");
    let mut rust = request();
    rust.scope = Some(ProjectBuildScope {
        packages: vec!["package-b".into(), "package-a".into(), "package-a".into()],
    });
    let (plan, _) = project_build::plan(&policy, &registry, &rust).unwrap();
    assert_eq!(
        plan.process.args,
        ["build", "-p", "package-a", "-p", "package-b"]
    );

    let (_tmp, _root, registry, policy) = fixture("go.mod");
    let mut go = request();
    go.scope = Some(ProjectBuildScope {
        packages: vec!["./cmd/...".into(), "./internal".into()],
    });
    let (plan, _) = project_build::plan(&policy, &registry, &go).unwrap();
    assert_eq!(plan.process.args, ["build", "./cmd/...", "./internal"]);

    let mut invalid = request();
    invalid.scope = Some(ProjectBuildScope {
        packages: vec!["not-relative".into()],
    });
    assert!(matches!(
        project_build::plan(&policy, &registry, &invalid),
        Err(ProjectBuildPlanningResult::Unavailable { code, .. })
            if code == "build_scope_invalid"
    ));
}

#[test]
fn project_build_backend_selection_is_nearest_explicit_and_fail_closed() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::create_dir(root.join("nested")).unwrap();
    fs::write(root.join("nested/go.mod"), "module demo\n").unwrap();
    let mut req = request();
    req.cwd = Some("nested".into());
    let (plan, _) = project_build::plan(&policy, &registry, &req).unwrap();
    assert_eq!(plan.provenance.backend, "go");

    req.adapter = ProjectBuildAdapter::Rust;
    assert!(matches!(
        project_build::plan(&policy, &registry, &req),
        Err(ProjectBuildPlanningResult::Unavailable { code, .. })
            if code == "build_recipe_mismatch"
    ));
    req.adapter = ProjectBuildAdapter::Go;
    assert!(project_build::plan(&policy, &registry, &req).is_ok());

    fs::write(root.join("nested/Cargo.toml"), "").unwrap();
    req.adapter = ProjectBuildAdapter::Auto;
    assert!(matches!(
        project_build::plan(&policy, &registry, &req),
        Err(ProjectBuildPlanningResult::Unavailable { code, .. })
            if code == "build_recipe_ambiguous"
    ));

    for (marker, backend) in [("package.json", "node"), ("pyproject.toml", "python")] {
        let (_tmp, _root, registry, policy) = fixture(marker);
        assert_eq!(
            project_build::plan(&policy, &registry, &request()).unwrap_err(),
            ProjectBuildPlanningResult::Unavailable {
                code: "build_adapter_unavailable".into(),
                detected_backend: Some(backend.into()),
            }
        );
    }
}

#[test]
fn project_build_admission_fence_replans_manifest_lock_and_invocation_truth() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
    let (plan, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    fs::write(root.join("src.txt"), "source-only change\n").unwrap();
    project_build::fence(&policy, &registry, &operation).unwrap();

    fs::write(root.join("Cargo.lock"), "version = 3\n").unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));

    fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\n").unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_fence_rejects_process_drift() {
    let (_tmp, _root, registry, policy) = fixture("go.mod");
    let (plan, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    let mut operation = start_build_operation(&plan, &cwd);
    let RunnerJobOperation::StartBuild(ref mut build) = operation else {
        unreachable!()
    };
    build.process.args.push("./extra".into());
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_fence_rejects_runtime_project_identity_drift() {
    let (_tmp, _root, registry, policy) = fixture("Cargo.toml");
    let (plan, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    let mut operation = start_build_operation(&plan, &cwd);
    let RunnerJobOperation::StartBuild(ref mut build) = operation else {
        unreachable!()
    };
    build.context.runtime_project_id = Some("agent:runner:other".into());
    assert_eq!(
        project_build::fence(&policy, &registry, &operation).unwrap_err(),
        "build project identity mismatch"
    );
}
