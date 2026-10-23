use crate::project_build::*;
use crate::runner_operation::{
    RunnerInvocationMetadata, RunnerJobBuildOperation, RunnerJobOperation, RunnerOperation,
};
use crate::runner_protocol::{RunnerRequest, ShellJobContext, ShellJobStructuredExecutionMetadata};

fn request() -> ProjectBuildRequest {
    ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: Some("src".into()),
        adapter: ProjectBuildAdapter::Auto,
        scope: None,
        dependency_policy: None,
    }
}

fn digest(fill: char) -> String {
    std::iter::repeat_n(fill, 64).collect()
}

#[test]
fn project_build_rust_options_are_typed_bounded_and_bound_to_canonical_plan() {
    let mut selected = request();
    selected.adapter = ProjectBuildAdapter::Rust;
    let default = canonical_project_build_process("rust", &selected).unwrap();
    assert_eq!(default.args, ["build"]);
    let wire = serde_json::to_value(&selected).unwrap();
    for key in ["profile", "targets", "artifact_policy"] {
        assert!(wire.get(key).is_none());
    }
    selected.profile = Some("dogfood".into());
    selected.targets = Some(ProjectBuildTargets {
        library: true,
        binaries: vec!["z".into(), "a".into(), "a".into()],
    });
    selected.artifact_policy = Some(ProjectBuildArtifactPolicy::Observe);
    let process = canonical_project_build_process("rust", &selected).unwrap();
    assert_eq!(
        process.args,
        [
            "build",
            "--profile",
            "dogfood",
            "--lib",
            "--bin",
            "a",
            "--bin",
            "z",
            "--message-format=json-render-diagnostics"
        ]
    );
    assert_ne!(
        project_build_invocation_digest(&process),
        project_build_invocation_digest(&default)
    );
    for backend in ["node", "python", "go"] {
        assert_eq!(
            canonical_project_build_process(backend, &selected).unwrap_err(),
            "build_options_unsupported"
        );
    }
    for name in ["", "--release", "../prod", "a b", "x\n", "a/b"] {
        let mut bad = selected.clone();
        bad.profile = Some(name.into());
        assert!(bad.validate().is_err());
    }
    for targets in [
        ProjectBuildTargets::default(),
        ProjectBuildTargets {
            library: false,
            binaries: vec!["x".repeat(257)],
        },
        ProjectBuildTargets {
            library: false,
            binaries: vec!["x".into(); 9],
        },
    ] {
        let mut bad = selected.clone();
        bad.targets = Some(targets);
        assert!(bad.validate().is_err());
    }
    let mut bad = serde_json::to_value(&selected).unwrap();
    bad["targets"]["args"] = serde_json::json!(["--all-targets"]);
    assert!(serde_json::from_value::<ProjectBuildRequest>(bad).is_err());
}

#[test]
fn project_build_protocol_is_closed_declarative_and_roundtrips() {
    let input = request();
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "request".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::PlanProjectBuild(input.clone()),
    )
    .unwrap();
    assert_eq!(wire.kind, "plan_project_build");
    assert!(wire.command.is_empty());
    assert!(wire.process.is_none() && wire.script.is_none() && wire.cwd.is_none());
    let payload: ProjectBuildRequest =
        serde_json::from_str(wire.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload, input);

    for field in ["program", "args", "shell", "script", "executable", "argv"] {
        let mut json = serde_json::to_value(&input).unwrap();
        json[field] = serde_json::json!("arbitrary");
        assert!(serde_json::from_value::<ProjectBuildRequest>(json).is_err());
    }
    for cwd in [
        "/tmp",
        "../outside",
        "C:/secret",
        "\\\\host\\share",
        "a/../b",
    ] {
        let mut invalid = input.clone();
        invalid.cwd = Some(cwd.into());
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn project_build_scope_and_canonical_processes_are_bounded() {
    let mut rust = request();
    rust.scope = Some(ProjectBuildScope {
        packages: vec!["package-b".into(), "package-a".into(), "package-a".into()],
        all_packages: false,
        exclude_packages: Vec::new(),
    });
    assert_eq!(
        canonical_project_build_process("rust", &rust).unwrap().args,
        ["build", "-p", "package-a", "-p", "package-b"]
    );

    let mut go = request();
    go.scope = Some(ProjectBuildScope {
        packages: vec!["./cmd/...".into(), "./internal".into()],
        all_packages: false,
        exclude_packages: Vec::new(),
    });
    assert_eq!(
        canonical_project_build_process("go", &go).unwrap().args,
        ["build", "./cmd/...", "./internal"]
    );

    for packages in [
        Vec::<String>::new(),
        (0..9).map(|index| format!("package-{index}")).collect(),
        vec!["x".repeat(257)],
        vec!["bad\npackage".into()],
    ] {
        let mut invalid = request();
        invalid.scope = Some(ProjectBuildScope {
            packages,
            all_packages: false,
            exclude_packages: Vec::new(),
        });
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn project_build_all_packages_maps_to_portable_native_scope() {
    for (backend, expected) in [
        ("rust", vec!["build", "--workspace"]),
        ("go", vec!["build", "./..."]),
    ] {
        let mut request = request();
        request.scope = Some(ProjectBuildScope {
            packages: Vec::new(),
            all_packages: true,
            exclude_packages: Vec::new(),
        });
        assert_eq!(
            canonical_project_build_process(backend, &request)
                .unwrap()
                .args,
            expected
        );
    }
}

#[test]
fn project_build_workspace_exclusions_are_canonical_and_rust_only() {
    let mut request = request();
    request.scope = Some(ProjectBuildScope {
        packages: Vec::new(),
        all_packages: true,
        exclude_packages: vec!["zeta".into(), "alpha".into(), "alpha".into()],
    });
    assert_eq!(
        canonical_project_build_process("rust", &request)
            .unwrap()
            .args,
        [
            "build",
            "--workspace",
            "--exclude",
            "alpha",
            "--exclude",
            "zeta"
        ]
    );
    assert!(canonical_project_build_process("go", &request)
        .unwrap_err()
        .contains("workspace_exclusions_unsupported"));
    request.scope.as_mut().unwrap().exclude_packages = vec!["crate-*".into()];
    assert!(canonical_project_build_process("rust", &request).is_err());
}

#[test]
fn start_build_wire_roundtrip_preserves_typed_process_and_provenance() {
    let request = ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Rust,
        scope: None,
        dependency_policy: None,
    };
    let process = canonical_project_build_process("rust", &request).unwrap();
    let provenance = ProjectBuildProvenance {
        request,
        backend: "rust".into(),
        recipe_root: ".".into(),
        root_digest: digest('1'),
        manifest_digest: digest('2'),
        invocation_digest: project_build_invocation_digest(&process),
    };
    let structured = ShellJobStructuredExecutionMetadata {
        execution_source: "project_build".into(),
        language: None,
        script_bytes: None,
        arg_count: process.args.len(),
        stdin_present: false,
        validation_identity: None,
        assertion_name: None,
        validation_tool: None,
    };
    assert!(structured.is_valid());
    let context = ShellJobContext {
        runtime_project_id: Some("agent:runner:demo".into()),
        workflow_session_id: None,
        ssh_resource: None,
        project_cwd: Some(".".into()),
        cwd: Some("/tmp/demo".into()),
        purpose: Some("build".into()),
        shell: Some("direct_argv".into()),
        command_preview: "cargo build".into(),
        validation_steps: Vec::new(),
        validation: None,
        structured_execution: Some(structured),
    };
    let operation = RunnerJobOperation::StartBuild(RunnerJobBuildOperation {
        job_id: "job".into(),
        cwd: context.cwd.clone(),
        process: process.clone(),
        provenance: provenance.clone(),
        timeout_secs: 60,
        context,
    });
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "request".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::Job(operation.clone()),
    )
    .unwrap();
    assert_eq!(wire.kind, "start_build_job");
    assert_eq!(wire.process, Some(process));
    assert!(wire.command.is_empty() && wire.stdin.is_none());
    let decoded = wire.decode_operation().unwrap();
    let RunnerOperation::Job(RunnerJobOperation::StartBuild(decoded)) = decoded else {
        panic!("expected typed StartBuild");
    };
    assert_eq!(decoded.provenance, provenance);
    assert_eq!(
        RunnerJobOperation::StartBuild(decoded).expected_structured_execution(),
        operation.expected_structured_execution()
    );

    let mut process_drift = wire.clone();
    process_drift
        .process
        .as_mut()
        .unwrap()
        .args
        .push("--release".into());
    assert!(process_drift
        .decode_operation()
        .unwrap_err()
        .contains("does not match build provenance"));

    let mut recovery_drift = wire;
    recovery_drift
        .job_context
        .as_mut()
        .unwrap()
        .structured_execution
        .as_mut()
        .unwrap()
        .execution_source = "run_process".into();
    assert!(recovery_drift
        .decode_operation()
        .unwrap_err()
        .contains("recovery metadata does not match typed operation"));
}

#[test]
fn offline_build_policy_preserves_defaults_and_fences_canonical_execution() {
    let mut request = request();
    let original = canonical_project_build_process("rust", &request).unwrap();
    assert_eq!(original.args, ["build"]);
    assert!(serde_json::to_value(&request)
        .unwrap()
        .get("dependency_resolution_policy")
        .is_none());
    request.adapter = ProjectBuildAdapter::Rust;
    request.dependency_resolution_policy = Some(ProjectDependencyResolutionPolicy::Offline);
    assert!(request.validate().is_ok());
    assert_eq!(
        serde_json::to_value(&request).unwrap()["dependency_resolution_policy"],
        "offline"
    );
    let offline = canonical_project_build_process("rust", &request).unwrap();
    assert_eq!(offline.args, ["build", "--offline"]);
    assert_ne!(
        project_build_invocation_digest(&original),
        project_build_invocation_digest(&offline)
    );
    let mut plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request: request.clone(),
            backend: "rust".into(),
            recipe_root: ".".into(),
            root_digest: digest('a'),
            manifest_digest: digest('b'),
            invocation_digest: project_build_invocation_digest(&offline),
        },
        process: offline,
    };
    assert!(plan.is_valid());
    plan.provenance.request.dependency_resolution_policy = None;
    assert!(
        !plan.is_valid(),
        "offline argv cannot be replayed as an online request"
    );
    request.dependency_policy = Some(ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    });
    assert_eq!(
        canonical_project_build_process("rust", &request)
            .unwrap()
            .args,
        ["build", "--locked", "--offline"]
    );
    request.adapter = ProjectBuildAdapter::Auto;
    for backend in ["go", "node", "python"] {
        assert!(canonical_project_build_process(backend, &request).is_err());
    }
    request.adapter = ProjectBuildAdapter::Go;
    assert!(request.validate().is_err());
    let mut invalid = serde_json::to_value(&request).unwrap();
    invalid["dependency_resolution_policy"] = serde_json::json!("online");
    assert!(serde_json::from_value::<ProjectBuildRequest>(invalid).is_err());
}

#[test]
fn project_build_locked_dependency_policy_maps_to_native_readonly_flags() {
    let locked = ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    };
    for (backend, expected) in [
        ("rust", vec!["build", "--locked"]),
        ("go", vec!["build", "-mod=readonly", "./..."]),
    ] {
        let request = ProjectBuildRequest {
            profile: None,
            targets: None,
            artifact_policy: None,
            dependency_resolution_policy: None,
            project_id: "demo".into(),
            cwd: None,
            adapter: ProjectBuildAdapter::Auto,
            scope: None,
            dependency_policy: Some(locked),
        };
        let process = canonical_project_build_process(backend, &request).unwrap();
        assert_eq!(process.args, expected);
    }
}

#[test]
fn project_build_default_dependency_policy_preserves_historical_argv() {
    let request = ProjectBuildRequest {
        profile: None,
        targets: None,
        artifact_policy: None,
        dependency_resolution_policy: None,
        project_id: "demo".into(),
        cwd: None,
        adapter: ProjectBuildAdapter::Auto,
        scope: None,
        dependency_policy: None,
    };
    assert_eq!(
        canonical_project_build_process("rust", &request)
            .unwrap()
            .args,
        vec!["build"]
    );
    assert_eq!(
        canonical_project_build_process("go", &request)
            .unwrap()
            .args,
        vec!["build", "./..."]
    );
}

#[test]
fn python_build_is_canonical_no_install_and_rejects_scope_or_argv_drift() {
    let mut request = request();
    request.adapter = ProjectBuildAdapter::Python;
    let process = canonical_project_build_process("python", &request).unwrap();
    assert_eq!(process.executable, "python");
    assert_eq!(process.args, ["-m", "build", "--no-isolation"]);
    request.dependency_policy = Some(ProjectDependencyPolicy {
        mode: ProjectDependencyMode::Locked,
    });
    assert_eq!(
        canonical_project_build_process("python", &request).unwrap(),
        process
    );
    let mut plan = ProjectBuildPlan {
        provenance: ProjectBuildProvenance {
            request: request.clone(),
            backend: "python".into(),
            recipe_root: ".".into(),
            root_digest: "a".repeat(64),
            manifest_digest: "b".repeat(64),
            invocation_digest: project_build_invocation_digest(&process),
        },
        process: process.clone(),
    };
    assert!(plan.is_valid());
    plan.process.args.push("--skip-dependency-check".into());
    plan.provenance.invocation_digest = project_build_invocation_digest(&plan.process);
    assert!(
        !plan.is_valid(),
        "even a matching digest cannot authorize noncanonical flags"
    );
    request.scope = Some(ProjectBuildScope {
        packages: vec![],
        all_packages: true,
        exclude_packages: Vec::new(),
    });
    assert!(canonical_project_build_process("python", &request).is_err());
    let caps: crate::runner_protocol::RunnerCapabilities = serde_json::from_str("{}").unwrap();
    assert!(!caps.project_build_python_v1);
}

#[test]
fn build_artifact_observations_are_bounded_private_and_never_validation_evidence() {
    let artifact = ProjectBuildArtifact {
        path: "dist/demo.whl".into(),
        bytes: 100,
        sha256: "a".repeat(64),
        mime_type: Some("application/zip".into()),
    };
    assert!(validate_build_artifacts(&[artifact.clone()]));
    let mut generated = artifact.clone();
    generated.path = "target/dogfood/demo".into();
    assert!(!validate_build_artifacts(&[generated.clone()]));
    assert!(validate_cargo_build_artifacts(&[generated]));
    assert!(!validate_build_artifacts(&vec![
        artifact.clone();
        PROJECT_BUILD_ARTIFACTS_MAX
            + 1
    ]));
    for path in [
        "../dist/demo.whl",
        "/tmp/demo.whl",
        "dist/../../secret",
        "secrets/id_rsa",
        "dist/.env",
        "target/secrets/demo",
        "target/.env",
        "target/keys/server.pem",
        "node_modules/pkg/output",
    ] {
        let mut invalid = artifact.clone();
        invalid.path = path.into();
        assert!(!validate_build_artifacts(&[invalid.clone()]), "{path}");
        assert!(!validate_cargo_build_artifacts(&[invalid]), "{path}");
    }
    use crate::runner_protocol::ShellCommandExecutionState::Completed;
    assert!(validate_build_artifact_report(
        "project_build",
        "completed",
        Some(0),
        Some(Completed),
        Some(&[artifact.clone()])
    )
    .is_ok());
    assert!(validate_build_artifact_report(
        "run_process",
        "completed",
        Some(0),
        Some(Completed),
        Some(&[artifact.clone()])
    )
    .is_err());
    assert!(validate_build_artifact_report(
        "project_build",
        "running",
        None,
        None,
        Some(&[artifact])
    )
    .is_err());
}

#[test]
fn node_build_canonical_manager_process_rejects_extra_authority() {
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
    for manager in ["npm", "pnpm", "yarn", "bun"] {
        let process =
            canonical_project_build_process_with_manager("node", &request, manager).unwrap();
        assert_eq!(process.executable, manager);
        assert_eq!(
            process.args,
            if manager == "yarn" {
                vec!["run", "build"]
            } else {
                vec!["run", "--silent", "build"]
            }
        );
    }
    assert!(canonical_project_build_process_with_manager("node", &request, "npx").is_err());
    let old: crate::runner_protocol::RunnerCapabilities =
        serde_json::from_value(serde_json::json!({})).unwrap();
    assert!(!old.project_build_node_v1 && !old.project_validation_node_json_v1);
}

#[test]
fn node_build_provenance_rejects_explicit_backend_mismatch() {
    let mut provenance = ProjectBuildProvenance {
        request: ProjectBuildRequest {
            profile: None,
            targets: None,
            artifact_policy: None,
            dependency_resolution_policy: None,
            project_id: "demo".into(),
            cwd: None,
            adapter: ProjectBuildAdapter::Node,
            scope: None,
            dependency_policy: None,
        },
        backend: "node".into(),
        recipe_root: ".".into(),
        root_digest: "a".repeat(64),
        manifest_digest: "b".repeat(64),
        invocation_digest: "c".repeat(64),
    };
    assert!(provenance.is_valid());
    for adapter in [
        ProjectBuildAdapter::Rust,
        ProjectBuildAdapter::Go,
        ProjectBuildAdapter::Python,
    ] {
        provenance.request.adapter = adapter;
        assert!(!provenance.is_valid());
    }
    provenance.request.adapter = ProjectBuildAdapter::Node;
    provenance.backend = "go".into();
    assert!(!provenance.is_valid());
}
