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
    let payload: ProjectValidationRequest =
        serde_json::from_str(wire.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload, input);
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
