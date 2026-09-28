use crate::project_validation::*;
use crate::runner_operation::{RunnerInvocationMetadata, RunnerOperation};
use crate::runner_protocol::RunnerRequest;
#[test]
fn project_validation_protocol_is_closed_declarative_and_roundtrips() {
    let input = ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: Some("src".into()),
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Auto,
        scope: None,
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
fn project_validation_package_scope_roundtrips_and_is_bounded() {
    let input = ProjectValidationRequest {
        project_id: "demo".into(),
        cwd: Some("src".into()),
        action: ProjectValidationAction::Check,
        adapter: ProjectValidationAdapter::Auto,
        scope: Some(ProjectValidationScope {
            packages: vec!["package-a".into(), "package-b".into()],
        }),
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
        invalid.scope = Some(ProjectValidationScope { packages });
        assert!(invalid.validate().is_err());
    }
}
