use super::config::RunnerPolicy;
use super::project_build;
use sha2::Digest;
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
    // Production plans supply a canonical cwd. Temporary roots may contain
    // aliases on macOS or lack the canonical Windows verbatim prefix.
    let root = root.canonicalize().unwrap();
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
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Auto,
        scope: None,
        dependency_policy: None,
    }
}

#[test]
fn project_build_rust_options_preserve_provenance_and_reject_auto_other_backend() {
    let (_temp, root, registry, policy) = fixture("Cargo.toml");
    let mut selected = request();
    selected.profile = Some("dogfood".into());
    let (plan, cwd) = project_build::plan(&policy, &registry, &selected).unwrap();
    assert_eq!(plan.process.args, ["build", "--profile", "dogfood"]);
    let operation = start_build_operation(&plan, &cwd);
    assert!(project_build::fence(&policy, &registry, &operation).is_ok());
    fs::write(root.join("Cargo.toml"), "# profile source drift\n").unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
    let (_temp, _root, registry, policy) = fixture("go.mod");
    assert!(
        matches!(project_build::plan(&policy,&registry,&selected),Err(ProjectBuildPlanningResult::Unavailable {code,..}) if code == "build_options_unsupported")
    );
}

#[cfg(feature = "runner-real-process-tests")]
#[test]
#[ignore = "real Cargo library/binary/custom profile output observation; no downloads"]
fn runner_real_process_project_build_rust_options_observe_selected_cached_artifacts() {
    use crate::webcodex_runner::config::ShellConfig;
    use crate::webcodex_runner::shell::PreparedShellProfileCache;
    use std::sync::atomic::AtomicBool;
    let (_temp, root, registry, policy) = fixture("Cargo.toml");
    fs::create_dir_all(root.join("src/bin")).unwrap();
    fs::write(root.join("Cargo.toml"),"[package]\nname='bounded-artifact-demo'\nversion='0.1.0'\nedition='2021'\n[workspace]\nmembers=['excluded']\n[profile.dogfood]\ninherits='dev'\n").unwrap();
    fs::write(root.join("src/lib.rs"), "pub fn answer()->u32 {42}\n").unwrap();
    fs::write(
        root.join("src/bin/chosen.rs"),
        "fn main(){println!(\"chosen\");}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/bin/unselected.rs"),
        "compile_error!(\"unselected must not compile\");\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("excluded/src")).unwrap();
    fs::write(
        root.join("excluded/Cargo.toml"),
        "[package]\nname='excluded'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    fs::write(
        root.join("excluded/src/lib.rs"),
        "compile_error!(\"excluded member must not compile\");\n",
    )
    .unwrap();
    // Keep fixture outputs inside its Project even when the test invoker uses
    // a shared Cargo target cache for compiling the Runner itself.
    let mut shell = ShellConfig::default();
    shell.env.insert(
        "CARGO_TARGET_DIR".into(),
        root.join("target").to_str().unwrap().into(),
    );
    let mut selected = request();
    selected.profile = Some("dogfood".into());
    selected.targets = Some(ProjectBuildTargets {
        library: true,
        binaries: vec!["chosen".into()],
    });
    selected.artifact_policy = Some(ProjectBuildArtifactPolicy::Observe);
    selected.dependency_resolution_policy = Some(ProjectDependencyResolutionPolicy::Offline);
    for index in 0..4 {
        if index == 2 {
            selected.scope = Some(ProjectBuildScope {
                packages: Vec::new(),
                all_packages: true,
                exclude_packages: vec!["excluded".into()],
            });
        }
        let (plan, cwd) = project_build::plan(&policy, &registry, &selected).unwrap();
        let operation = start_build_operation(&plan, &cwd);
        let (result, artifacts) = project_build::run_rust_build(
            1,
            &policy,
            &shell,
            &registry,
            &PreparedShellProfileCache::default(),
            &operation,
            &AtomicBool::new(false),
            &|| {},
        );
        assert_eq!(result.result.exit_code, Some(0), "{result:?}");
        assert!(result.result.error.is_none(), "{result:?}");
        let artifacts = artifacts.expect("verified Cargo outputs");
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.path.contains("chosen")),
            "{artifacts:?}"
        );
        assert!(
            artifacts
                .iter()
                .any(|artifact| artifact.path.ends_with(".rlib")),
            "{artifacts:?}"
        );
        for artifact in artifacts {
            assert!(!artifact.path.contains("excluded"));
            let bytes = fs::read(root.join(&artifact.path)).unwrap();
            assert_eq!(artifact.bytes, bytes.len() as u64);
            assert_eq!(
                artifact.sha256,
                format!("{:x}", sha2::Sha256::digest(&bytes))
            );
        }
    }
    assert!(!root.join("target/dogfood/unselected").exists());
    let mut missing = selected;
    missing.profile = Some("missing-profile".into());
    let (plan, cwd) = project_build::plan(&policy, &registry, &missing).unwrap();
    let (result, artifacts) = project_build::run_rust_build(
        1,
        &policy,
        &shell,
        &registry,
        &PreparedShellProfileCache::default(),
        &start_build_operation(&plan, &cwd),
        &AtomicBool::new(false),
        &|| {},
    );
    assert_ne!(result.result.exit_code, Some(0));
    assert!(artifacts.is_none());
}

#[test]
fn rust_build_offline_planning_is_opt_in_and_non_rust_is_rejected() {
    let (_tmp, _root, registry, policy) = fixture("Cargo.toml");
    let mut request = request();
    request.adapter = ProjectBuildAdapter::Rust;
    let (default, _) = project_build::plan(&policy, &registry, &request).unwrap();
    request.dependency_resolution_policy = Some(ProjectDependencyResolutionPolicy::Offline);
    let (offline, _) = project_build::plan(&policy, &registry, &request).unwrap();
    assert_eq!(offline.process.args, ["build", "--offline"]);
    assert_ne!(
        default.provenance.invocation_digest,
        offline.provenance.invocation_digest
    );
    assert_eq!(offline.provenance.request, request);
    assert!(offline.is_valid());
    request.dependency_policy = Some(ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    });
    let (locked, _) = project_build::plan(&policy, &registry, &request).unwrap();
    assert_eq!(locked.process.args, ["build", "--locked", "--offline"]);

    let (_tmp, _root, registry, policy) = fixture("go.mod");
    let mut non_rust = self::request();
    non_rust.dependency_resolution_policy = Some(ProjectDependencyResolutionPolicy::Offline);
    assert!(
        matches!(project_build::plan(&policy, &registry, &non_rust).unwrap_err(),
        ProjectBuildPlanningResult::Unavailable { ref code, .. }
            if code == "dependency_resolution_policy_unsupported")
    );
}

#[test]
fn project_build_runner_plans_locked_dependency_policy_for_rust_and_go() {
    for (marker, adapter, expected) in [
        (
            "Cargo.toml",
            ProjectBuildAdapter::Rust,
            vec!["build", "--locked"],
        ),
        (
            "go.mod",
            ProjectBuildAdapter::Go,
            vec!["build", "-mod=readonly", "./..."],
        ),
    ] {
        let (_tmp, _root, registry, policy) = fixture(marker);
        let request = ProjectBuildRequest {
            profile: None,
            targets: None,
            artifact_policy: None,
            dependency_resolution_policy: None,
            project_id: "demo".into(),
            cwd: None,
            adapter,
            scope: None,
            dependency_policy: Some(ProjectDependencyPolicy {
                mode: ProjectDependencyMode::Locked,
            }),
        };
        let (plan, _) = project_build::plan(&policy, &registry, &request).unwrap();
        assert_eq!(plan.provenance.request, request);
        assert_eq!(plan.process.args, expected);
        assert_eq!(
            plan.provenance.invocation_digest,
            project_build_invocation_digest(&plan.process)
        );
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
                "python" => "python -m build --no-isolation".into(),
                "node" => format!(
                    "{} {}",
                    plan.process.executable,
                    plan.process.args.join(" ")
                ),
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
        all_packages: false,
        exclude_packages: Vec::new(),
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
        all_packages: false,
        exclude_packages: Vec::new(),
    });
    let (plan, _) = project_build::plan(&policy, &registry, &go).unwrap();
    assert_eq!(plan.process.args, ["build", "./cmd/...", "./internal"]);

    let mut invalid = request();
    invalid.scope = Some(ProjectBuildScope {
        packages: vec!["not-relative".into()],
        all_packages: false,
        exclude_packages: Vec::new(),
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
fn project_build_all_packages_member_manifest_change_fails_queue_fence() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"member\", \"other\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("member/src")).unwrap();
    fs::write(
        root.join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("other/src")).unwrap();
    fs::write(
        root.join("other/Cargo.toml"),
        "[package]\nname = \"other\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let mut req = request();
    req.cwd = Some("member".into());
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: vec!["member".into()],
    });

    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    assert_eq!(
        plan.process.args,
        ["build", "--workspace", "--exclude", "member"]
    );
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    fs::write(
        root.join("member/Cargo.toml"),
        "[package]\nname = \"member\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_workspace_exclusions_reject_unknown_or_all_members_before_plan() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"a\", \"b\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    for (path, name) in [("a", "alpha"), ("b", "beta")] {
        fs::create_dir_all(root.join(path).join("src")).unwrap();
        fs::write(
            root.join(path).join("Cargo.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
        )
        .unwrap();
    }
    for excluded in [
        vec!["missing".to_string()],
        vec!["alpha".into(), "beta".into()],
    ] {
        let mut request = request();
        request.scope = Some(ProjectBuildScope {
            packages: Vec::new(),
            all_packages: true,
            exclude_packages: excluded,
        });
        let error = project_build::plan(&policy, &registry, &request).unwrap_err();
        assert!(
            format!("{error:?}").contains("build_scope_invalid"),
            "{error:?}"
        );
    }
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

#[test]
fn project_build_all_packages_target_member_change_fails_queue_fence() {
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

    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    assert_eq!(plan.process.args, ["build", "--workspace"]);
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    fs::write(
        root.join("target/member/Cargo.toml"),
        "[package]\nname='member'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[cfg(unix)]
#[test]
fn project_build_all_packages_member_symlink_retarget_fails_queue_fence() {
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

    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    fs::remove_file(root.join("selected")).unwrap();
    symlink("b", root.join("selected")).unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_all_packages_supports_single_rust_package() {
    let (_tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='single'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();

    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });

    let (plan, _) = project_build::plan(&policy, &registry, &req).unwrap();
    assert_eq!(plan.process.args, ["build", "--workspace"]);
}

#[cfg(unix)]
#[test]
fn project_build_all_packages_recursive_glob_cycle_fails_queue_fence() {
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

    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    // The queued plan must not run after its recursive membership witness
    // becomes incomplete, even though no manifest bytes have changed.
    symlink("..", root.join("a/member/back")).unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_all_packages_parent_workspace_appearance_fails_queue_fence() {
    let (tmp, root, registry, policy) = fixture("Cargo.toml");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='standalone'\nversion='0.1.0'\nedition='2021'\n",
    )
    .unwrap();
    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();

    // The new parent manifest changes Cargo's project unit without changing
    // any bytes inside the registered Project. Replanning must reject it.
    fs::write(
        tmp.path().join("Cargo.toml"),
        "[workspace]\nmembers=['remote-project']\nresolver='2'\n",
    )
    .unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
}

#[test]
fn project_build_all_packages_rejects_excluded_recipe_cwd() {
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
    let mut req = request();
    req.adapter = ProjectBuildAdapter::Rust;
    req.cwd = Some("tool".into());
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    assert!(
        matches!(project_build::plan(&policy, &registry, &req), Err(ProjectBuildPlanningResult::Unavailable { code, .. }) if code == "build_scope_unavailable")
    );
    req.cwd = Some("app".into());
    assert!(project_build::plan(&policy, &registry, &req).is_ok());
}

#[test]
fn python_packaging_plans_manifest_and_backend_input_fences() {
    let (_tmp, root, registry, policy) = fixture("pyproject.toml");
    fs::write(
        root.join("pyproject.toml"),
        "[build-system]\nrequires=[]\nbuild-backend='backend'\nbackend-path=['.']\n",
    )
    .unwrap();
    let mut req = request();
    req.adapter = ProjectBuildAdapter::Python;
    req.dependency_policy = Some(ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    });
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    assert_eq!(plan.process.args, ["-m", "build", "--no-isolation"]);
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();
    fs::write(root.join("setup.cfg"), "[metadata]\nname=demo\n").unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    let operation = start_build_operation(&plan, &cwd);
    fs::write(
        root.join("pyproject.toml"),
        "[build-system]\nrequires=['changed']\n",
    )
    .unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
    req.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    assert!(
        matches!(project_build::plan(&policy,&registry,&req),Err(ProjectBuildPlanningResult::Unavailable { code,.. }) if code == "build_scope_invalid")
    );
}

#[test]
fn python_artifact_observation_uses_existing_metadata_and_bounds() {
    use std::sync::atomic::AtomicBool;
    let (_tmp, root, registry, policy) = fixture("pyproject.toml");
    // Observation receives the canonical recipe cwd from planning, as native Jobs do.
    let (_, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    fs::create_dir(root.join("dist")).unwrap();
    fs::write(root.join("dist/demo.whl"), b"PK\x05\x06fixture").unwrap();
    fs::write(root.join("dist/demo.tar.gz"), b"\x1f\x8b\x08fixture").unwrap();
    let artifacts = project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(artifacts.len(), 2);
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact.path.ends_with(".whl"))
        .unwrap();
    assert_eq!(artifact.path, "dist/demo.whl");
    assert_eq!(artifact.bytes, 11);
    assert_eq!(artifact.mime_type.as_deref(), Some("application/zip"));
    assert_eq!(
        artifact.sha256,
        format!("{:x}", sha2::Sha256::digest(b"PK\x05\x06fixture"))
    );
    fs::write(root.join("dist/unknown.opaque"), b"unrecognized binary").unwrap();
    let unknown = project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(unknown
        .iter()
        .find(|artifact| artifact.path.ends_with(".opaque"))
        .unwrap()
        .mime_type
        .is_none());
    fs::write(root.join("dist/.env"), "must stay private").unwrap();
    assert!(project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false)
    )
    .is_err());
    fs::remove_file(root.join("dist/.env")).unwrap();
    for index in 0..16 {
        fs::write(root.join(format!("dist/{index}.whl")), "").unwrap();
    }
    assert!(project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false)
    )
    .unwrap_err()
    .contains("limit"));
}

#[cfg(unix)]
#[test]
fn python_packaging_rejects_redirected_dist_and_artifact_symlinks() {
    use std::{os::unix::fs::symlink, sync::atomic::AtomicBool};
    let (tmp, root, registry, policy) = fixture("pyproject.toml");
    // Observation receives the canonical recipe cwd from planning, as native Jobs do.
    let (_, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    symlink(tmp.path(), root.join("dist")).unwrap();
    assert!(
        matches!(project_build::plan(&policy,&registry,&request()),Err(ProjectBuildPlanningResult::Unavailable { code,.. }) if code == "build_artifact_path_invalid")
    );
    fs::remove_file(root.join("dist")).unwrap();
    fs::create_dir(root.join("dist")).unwrap();
    symlink(root.join("pyproject.toml"), root.join("dist/demo.whl")).unwrap();
    assert!(project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false)
    )
    .unwrap_err()
    .contains("path_invalid"));
}

#[test]
fn python_artifact_observation_reports_missing_directory_and_incomplete_outputs() {
    use std::sync::atomic::AtomicBool;
    let (_tmp, root, registry, policy) = fixture("pyproject.toml");
    // Observation receives the canonical recipe cwd from planning, as native Jobs do.
    let (_, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    for state in 0..3 {
        if state == 1 {
            fs::create_dir(root.join("dist")).unwrap();
        }
        if state == 2 {
            fs::write(root.join("dist/demo.whl"), b"PK\x05\x06fixture").unwrap();
        }
        assert_eq!(
            project_build::observe_python_artifacts(
                &policy,
                &registry,
                &request(),
                &cwd,
                std::time::Instant::now() + std::time::Duration::from_secs(5),
                &AtomicBool::new(false)
            )
            .unwrap_err(),
            "build_artifacts_missing"
        );
    }
}

#[cfg(unix)]
#[test]
fn python_artifact_observation_uses_planned_cwd_for_registered_project_alias() {
    use std::{os::unix::fs::symlink, sync::atomic::AtomicBool};
    let (tmp, root, registry, policy) = fixture("pyproject.toml");
    let alias = tmp.path().join("project-alias");
    symlink(&root, &alias).unwrap();
    fs::write(
        registry.join("demo.toml"),
        format!(
            "id = 'demo'\nname = 'Demo'\npath = {}\nallow_patch = true\n",
            serde_json::to_string(alias.to_str().unwrap()).unwrap()
        ),
    )
    .unwrap();
    let (_, cwd) = project_build::plan(&policy, &registry, &request()).unwrap();
    assert_eq!(cwd, root.canonicalize().unwrap());
    assert_ne!(cwd, alias);
    fs::create_dir(root.join("dist")).unwrap();
    fs::write(root.join("dist/demo.whl"), b"PK\x05\x06fixture").unwrap();
    fs::write(root.join("dist/demo.tar.gz"), b"\x1f\x8b\x08fixture").unwrap();
    let artifacts = project_build::observe_python_artifacts(
        &policy,
        &registry,
        &request(),
        &cwd,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(artifacts.len(), 2);
    assert!(artifacts
        .iter()
        .all(|artifact| artifact.path.starts_with("dist/")));
    // Alias acceptance at Project admission does not permit redirected output reads.
    fs::remove_file(root.join("dist/demo.whl")).unwrap();
    symlink(root.join("pyproject.toml"), root.join("dist/demo.whl")).unwrap();
    assert_eq!(
        project_build::observe_python_artifacts(
            &policy,
            &registry,
            &request(),
            &cwd,
            std::time::Instant::now() + std::time::Duration::from_secs(5),
            &AtomicBool::new(false),
        )
        .unwrap_err(),
        "build_artifact_path_invalid"
    );
}

#[test]
fn node_build_manifest_script_manager_scope_and_stale_fence() {
    let (_tmp, root, registry, policy) = fixture("package.json");
    fs::write(
        root.join("package.json"),
        r#"{"packageManager":"pnpm@9","scripts":{"build":"node build.js"}}"#,
    )
    .unwrap();
    let mut req = request();
    req.adapter = ProjectBuildAdapter::Node;
    let (plan, cwd) = project_build::plan(&policy, &registry, &req).unwrap();
    assert!(plan.is_valid());
    assert_eq!(plan.process.executable, "pnpm");
    assert_eq!(plan.process.args, ["run", "--silent", "build"]);
    let operation = start_build_operation(&plan, &cwd);
    project_build::fence(&policy, &registry, &operation).unwrap();
    fs::write(root.join("pnpm-lock.yaml"), "lock").unwrap();
    assert!(project_build::fence(&policy, &registry, &operation)
        .unwrap_err()
        .contains("build_plan_stale"));
    req.scope = Some(ProjectBuildScope {
        packages: vec![],
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    assert!(
        matches!(project_build::plan(&policy, &registry, &req), Err(ProjectBuildPlanningResult::Unavailable { code, .. }) if code == "build_scope_unsupported")
    );
    req.scope = None;
    req.dependency_policy = Some(ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    });
    assert!(
        matches!(project_build::plan(&policy, &registry, &req), Err(ProjectBuildPlanningResult::Unavailable { code, .. }) if code == "dependency_policy_unsupported")
    );
}
