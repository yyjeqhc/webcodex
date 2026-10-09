use crate::project_validation::*;
use crate::runner_operation::{RunnerInvocationMetadata, RunnerOperation};
use crate::runner_protocol::RunnerRequest;

#[test]
fn project_node_check_step_identity_capability_and_metadata_are_closed() {
    use crate::runner_protocol::{
        RunnerCapabilities, ShellJobValidationMetadata, ShellJobValidationStep,
    };
    use crate::validation_identity::{
        structured_validation_target_identity, ToolValidationIdentityKind,
    };
    let old: RunnerCapabilities =
        serde_json::from_value(serde_json::json!({"project_validation_v1":true})).unwrap();
    assert!(!old.project_validation_node_script_check_v1);
    let mut ids = std::collections::HashSet::new();
    for script in ["check", "typecheck", "lint"] {
        let step = ShellJobValidationStep {
            name: "check".into(),
            program: "node".into(),
            args: vec!["--run".into(), script.into()],
            env: vec![],
        };
        assert!(step.is_canonical() && step.is_structured_node_check());
        let id = structured_validation_target_identity(
            ToolValidationIdentityKind::NodeScriptCheck,
            &serde_json::json!({"cwd":".","script":script}),
        )
        .unwrap();
        assert!(ids.insert(id.clone()));
        let mut metadata = ShellJobValidationMetadata {
            tool: "project_validate".into(),
            kind: "check".into(),
            steps: vec![step],
            effective_timeout_secs: 60,
            sync_wait_secs: 1,
            adapter: "node:script:check".into(),
            validation_target_id: Some(id),
            source_fence: None,
            minimum_tests: None,
            require_tests: None,
            no_run: None,
            project_validation: Some(ProjectValidationProvenance {
                request: ProjectValidationRequest {
                    project_id: "demo".into(),
                    cwd: None,
                    action: ProjectValidationAction::Check,
                    adapter: ProjectValidationAdapter::Node,
                    scope: None,
                    dependency_policy: None,
                    test: None,
                },
                backend: "node".into(),
                recipe_root: ".".into(),
                root_digest: "a".repeat(64),
                manifest_digest: "b".repeat(64),
                invocation_digest: "c".repeat(64),
            }),
        };
        assert!(metadata.is_valid());
        let wire = serde_json::to_string(&metadata).unwrap();
        assert!(serde_json::from_str::<ShellJobValidationMetadata>(&wire)
            .unwrap()
            .is_valid());
        metadata.steps[0].args.push("arbitrary".into());
        assert!(!metadata.is_valid());
        metadata.steps[0].args.pop();
        metadata.steps[0]
            .env
            .push(("CARGO_TARGET_DIR".into(), "/tmp".into()));
        assert!(!metadata.is_valid());
        metadata.steps[0].env.clear();
        metadata.require_tests = Some(true);
        assert!(!metadata.is_valid());
        metadata.require_tests = None;
        metadata.project_validation.as_mut().unwrap().request.action =
            ProjectValidationAction::Test;
        assert!(!metadata.is_valid());
    }
    assert!(structured_validation_target_identity(
        ToolValidationIdentityKind::NodeScriptCheck,
        &serde_json::json!({"cwd":".","script":"--eval"}),
    )
    .is_none());
    let node = structured_validation_target_identity(
        ToolValidationIdentityKind::NodeScriptCheck,
        &serde_json::json!({"cwd":".","script":"check"}),
    )
    .unwrap();
    let ruff = structured_validation_target_identity(
        ToolValidationIdentityKind::PythonRuffCheck,
        &serde_json::json!({"cwd":"."}),
    )
    .unwrap();
    assert_ne!(node, ruff);
}
#[test]
fn project_validation_protocol_is_closed_declarative_and_roundtrips() {
    let input = ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: Some("src".into()),
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Auto,
        scope: None,
        dependency_policy: None,
        test: None,
    };
    let wire = RunnerRequest::from_operation(
        RunnerInvocationMetadata {
            request_id: "request".into(),
            client_id: "runner".into(),
            requested_by: "test".into(),
            created_at: 0,
        },
        RunnerOperation::PlanProjectValidation(input.clone()),
    )
    .unwrap();
    assert_eq!(wire.kind, "plan_project_validation");
    assert!(wire.command.is_empty());
    assert!(wire.process.is_none() && wire.script.is_none() && wire.cwd.is_none());
    let wire_content = wire.content.as_deref().unwrap();
    let wire_json: serde_json::Value = serde_json::from_str(wire_content).unwrap();
    assert!(
        wire_json.get("scope").is_none(),
        "unscoped project validation must remain wire-compatible with project_validation_v1"
    );
    let payload: ProjectValidationRequest = serde_json::from_str(wire_content).unwrap();
    assert_eq!(payload, input);

    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct LegacyProjectValidationRequest {
        project_id: String,
        cwd: Option<String>,
        action: ProjectValidationAction,
        #[serde(default)]
        adapter: ProjectValidationAdapter,
    }
    let legacy: LegacyProjectValidationRequest = serde_json::from_str(wire_content).unwrap();
    assert_eq!(legacy.project_id, input.project_id);
    assert_eq!(legacy.cwd, input.cwd);
    assert_eq!(legacy.action, input.action);
    assert_eq!(legacy.adapter, input.adapter);
    for field in ["program", "args", "shell", "script", "executable", "argv"] {
        let mut json = serde_json::to_value(&input).unwrap();
        json[field] = serde_json::json!("arbitrary");
        assert!(serde_json::from_value::<ProjectValidationRequest>(json).is_err());
    }
    for cwd in [
        "/tmp",
        "../outside",
        "C:/secret",
        "\\\\host\\share",
        "a/../b",
    ] {
        let mut input = input.clone();
        input.cwd = Some(cwd.into());
        assert!(input.validate().is_err());
    }
}

#[test]
fn project_validation_test_options_are_closed_scoped_and_preserve_defaults() {
    let base = serde_json::json!({"project_id":"demo","cwd":null,"action":"test"});
    let request: ProjectValidationRequest = serde_json::from_value(base.clone()).unwrap();
    assert!(request.validate().is_ok());
    assert_eq!(request.test_requirements(), (Some(true), Some(1)));
    assert!(serde_json::to_value(&request)
        .unwrap()
        .get("test")
        .is_none());
    for (options, required, minimum) in [
        (serde_json::json!({}), true, Some(1)),
        (serde_json::json!({"require_tests":false}), false, None),
        (serde_json::json!({"min_tests":7}), true, Some(7)),
        (
            serde_json::json!({"require_tests":false,"min_tests":7,"filter":"^TestA/sub.*$"}),
            false,
            Some(7),
        ),
    ] {
        let mut value = base.clone();
        value["test"] = options;
        let request: ProjectValidationRequest = serde_json::from_value(value.clone()).unwrap();
        assert!(request.validate().is_ok());
        assert_eq!(request.test_requirements(), (Some(required), minimum));
        assert_eq!(
            serde_json::from_value::<ProjectValidationRequest>(
                serde_json::to_value(&request).unwrap()
            )
            .unwrap(),
            request
        );
        value["action"] = serde_json::json!("check");
        assert!(serde_json::from_value::<ProjectValidationRequest>(value)
            .unwrap()
            .validate()
            .is_err());
    }
    for options in [
        serde_json::json!({"min_tests":0}),
        serde_json::json!({"min_tests":1_000_001}),
        serde_json::json!({"filter":"x".repeat(201)}),
        serde_json::json!({"filter":"测".repeat(67)}),
        serde_json::json!({"filter":"bad\nfilter"}),
    ] {
        let mut value = base.clone();
        value["test"] = options;
        assert!(serde_json::from_value::<ProjectValidationRequest>(value)
            .unwrap()
            .validate()
            .is_err());
    }
    for field in ["args", "argv", "script", "no_run", "shell"] {
        let mut value = base.clone();
        value["test"] = serde_json::json!({field: true});
        assert!(serde_json::from_value::<ProjectValidationRequest>(value).is_err());
    }
    // The additive capability is absent/false on old registrations, never a
    // v2 baseline assumption; declaring it explicitly round-trips.
    let mut caps = crate::runner_protocol::RunnerCapabilities::default();
    assert!(!caps.project_validation_test_options_v1);
    assert!(serde_json::to_value(caps.clone())
        .unwrap()
        .get("project_validation_test_options_v1")
        .is_none());
    caps.project_validation_test_options_v1 = true;
    assert!(
        serde_json::from_value::<crate::runner_protocol::RunnerCapabilities>(
            serde_json::to_value(caps).unwrap()
        )
        .unwrap()
        .project_validation_test_options_v1
    );
}

#[test]
fn project_test_filters_have_canonical_argv_and_stable_native_identity() {
    use crate::runner_protocol::{normalize_go_test_filter, ShellJobValidationStep};
    use crate::validation_identity::{
        structured_validation_target_identity, ToolValidationIdentityKind,
    };
    let step = |args: &[&str]| ShellJobValidationStep {
        name: "test".into(),
        program: "go".into(),
        args: args.iter().map(|v| (*v).into()).collect(),
        env: Vec::new(),
    };
    assert!(step(&["test", "-json", "-run", "^TestA/sub.*$", "./pkg"]).is_canonical());
    assert!(step(&["test", "-json", "-run", "Test space", "./..."]).is_structured_go_test_json());
    for args in [
        vec!["test", "-json", "-run"],
        vec!["test", "-json", "-run", "A"],
        vec!["test", "-json", "-run", "", "./..."],
        vec!["test", "-json", "-run", "A", "-race", "./..."],
        vec!["test", "-json", "-run", "bad\nfilter", "./..."],
    ] {
        assert!(!step(&args).is_canonical());
    }
    assert_eq!(
        normalize_go_test_filter("  space  ").unwrap(),
        Some("  space  ".into())
    );
    assert_eq!(normalize_go_test_filter("").unwrap(), None);
    let identity = |filter: Option<&str>| {
        structured_validation_target_identity(
            ToolValidationIdentityKind::GoTest,
            &serde_json::json!({"cwd":".","filter":filter,"packages":["./..."]}),
        )
    };
    let original = structured_validation_target_identity(
        ToolValidationIdentityKind::GoTest,
        &serde_json::json!({"cwd":".","packages":["./..."]}),
    );
    assert_eq!(identity(None), original);
    assert_eq!(identity(Some("")), original);
    assert_ne!(identity(Some("A")), original);
    assert_ne!(identity(Some("A")), identity(Some("B")));
    assert_eq!(
        identity(Some("A")),
        structured_validation_target_identity(
            ToolValidationIdentityKind::GoTest,
            &serde_json::json!({"cwd":".","filter":"A","packages":["./..."],"min_tests":500})
        )
    );
}

#[test]
fn project_validation_package_scope_roundtrips_and_is_bounded() {
    let input = ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: Some("src".into()),
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Auto,
        scope: Some(ProjectValidationScope {
            packages: vec!["package-a".into(), "package-b".into()],
            all_packages: false,
        }),
        dependency_policy: None,
        test: None,
    };
    assert!(input.validate().is_ok());
    let json = serde_json::to_value(&input).unwrap();
    let decoded: ProjectValidationRequest = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, input);

    for packages in [
        Vec::<String>::new(),
        (0..9).map(|index| format!("package-{index}")).collect(),
        vec!["x".repeat(257)],
        vec!["bad\npackage".into()],
    ] {
        let mut invalid = input.clone();
        invalid.scope = Some(ProjectValidationScope {
            packages,
            all_packages: false,
        });
        assert!(invalid.validate().is_err());
    }
}

#[test]
fn project_validation_locked_policy_rejects_format_check_without_silent_ignore() {
    let request = ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: None,
        action: ProjectValidationAction::FormatCheck,
        adapter: ProjectValidationAdapter::Rust,
        scope: None,
        dependency_policy: Some(ProjectDependencyPolicy {
            mode: ProjectDependencyMode::Locked,
        }),
        test: None,
    };
    assert_eq!(
        request.validate().unwrap_err(),
        "dependency_policy requires project_validate action=check or test"
    );
}

#[test]
fn project_validation_python_argv_identity_and_metadata_are_closed() {
    use crate::runner_protocol::{
        normalize_pytest_filter, ShellJobValidationMetadata, ShellJobValidationStep,
    };
    use crate::validation_identity::{
        structured_validation_target_identity, ToolValidationIdentityKind,
    };
    let step = ShellJobValidationStep {
        name: "test".into(),
        program: "python".into(),
        args: [
            "-m",
            "pytest",
            "--color=no",
            "-rA",
            "-k",
            "name and not slow",
        ]
        .map(str::to_string)
        .to_vec(),
        env: vec![],
    };
    assert!(step.is_canonical() && step.is_structured_pytest());
    for filter in ["--collect-only", " -x", "x\ny", &"a".repeat(201)] {
        assert!(normalize_pytest_filter(filter).is_err());
    }
    assert_eq!(normalize_pytest_filter("  ").unwrap(), None);
    let identity = |filter| {
        structured_validation_target_identity(
            ToolValidationIdentityKind::PythonPytest,
            &serde_json::json!({"cwd":".","filter":filter}),
        )
        .unwrap()
    };
    assert_eq!(identity(None::<&str>), identity(Some("")));
    assert_ne!(identity(Some("name")), identity(Some("other")));
    let mut metadata = ShellJobValidationMetadata {
        tool: "project_validate".into(),
        kind: "test".into(),
        steps: vec![step],
        effective_timeout_secs: 60,
        sync_wait_secs: 1,
        adapter: "python:pytest:test".into(),
        validation_target_id: Some(identity(Some("name and not slow"))),
        source_fence: None,
        minimum_tests: Some(1),
        require_tests: Some(true),
        no_run: None,
        project_validation: Some(ProjectValidationProvenance {
            request: ProjectValidationRequest {
                project_id: "demo".into(),
                cwd: None,
                action: ProjectValidationAction::Test,
                adapter: ProjectValidationAdapter::Python,
                scope: None,
                dependency_policy: None,
                test: None,
            },
            backend: "python".into(),
            recipe_root: ".".into(),
            root_digest: "a".repeat(64),
            manifest_digest: "b".repeat(64),
            invocation_digest: "c".repeat(64),
        }),
    };
    assert!(metadata.is_valid());
    let wire =
        serde_json::to_value(&metadata.project_validation.as_ref().unwrap().request).unwrap();
    assert_eq!(wire["adapter"], "python");
    metadata.steps[0].args.push("--collect-only".into());
    assert!(!metadata.is_valid());
}

#[test]
fn project_ruff_step_identity_metadata_and_capability_are_independent() {
    use crate::runner_protocol::{
        RunnerCapabilities, ShellJobValidationMetadata, ShellJobValidationStep,
    };
    use crate::validation_identity::{
        structured_validation_target_identity, ToolValidationIdentityKind,
    };
    let capabilities: RunnerCapabilities =
        serde_json::from_value(serde_json::json!({"project_validation_python_pytest_v1":true}))
            .unwrap();
    assert!(!capabilities.project_validation_python_ruff_v1);
    let mut identities = std::collections::HashSet::new();
    for (action, kind, identity_kind) in [
        (
            ProjectValidationAction::Check,
            "check",
            ToolValidationIdentityKind::PythonRuffCheck,
        ),
        (
            ProjectValidationAction::FormatCheck,
            "format",
            ToolValidationIdentityKind::PythonRuffFormat,
        ),
    ] {
        let step = ShellJobValidationStep::python_ruff(kind).unwrap();
        assert!(step.is_canonical() && step.is_structured_ruff());
        assert_eq!(&step.args[..4], ["-I", "-B", "-m", "ruff"]);
        for removed in 0..step.args.len() {
            let mut bad = step.clone();
            bad.args.remove(removed);
            assert!(!bad.is_structured_ruff());
        }
        for extra in [
            "--fix",
            "--fix-only",
            "--output-file=report.json",
            "--config=../ruff.toml",
            "--isolated",
            "src",
        ] {
            let mut bad = step.clone();
            bad.args.push(extra.into());
            assert!(!bad.is_canonical());
        }
        let identity =
            structured_validation_target_identity(identity_kind, &serde_json::json!({"cwd":"."}))
                .unwrap();
        assert!(identities.insert(identity.clone()));
        let mut metadata = ShellJobValidationMetadata {
            tool: "project_validate".into(),
            kind: kind.into(),
            steps: vec![step],
            effective_timeout_secs: 60,
            sync_wait_secs: 1,
            adapter: identity_kind.tool_name().unwrap().into(),
            validation_target_id: Some(identity),
            source_fence: None,
            minimum_tests: None,
            require_tests: None,
            no_run: None,
            project_validation: Some(ProjectValidationProvenance {
                request: ProjectValidationRequest {
                    project_id: "demo".into(),
                    cwd: None,
                    action,
                    adapter: ProjectValidationAdapter::Python,
                    scope: None,
                    dependency_policy: None,
                    test: None,
                },
                backend: "python".into(),
                recipe_root: ".".into(),
                root_digest: "a".repeat(64),
                manifest_digest: "b".repeat(64),
                invocation_digest: "c".repeat(64),
            }),
        };
        assert!(metadata.is_valid());
        metadata.minimum_tests = Some(1);
        assert!(!metadata.is_valid());
        metadata.minimum_tests = None;
        metadata.steps[0]
            .env
            .push(("CARGO_TARGET_DIR".into(), "/tmp/cache".into()));
        assert!(!metadata.is_valid());
        metadata.steps[0].env.clear();
        metadata.adapter = "python:pytest:test".into();
        assert!(!metadata.is_valid());
        metadata.adapter = identity_kind.tool_name().unwrap().into();
        metadata.project_validation = None;
        assert!(!metadata.is_valid());
    }
}
