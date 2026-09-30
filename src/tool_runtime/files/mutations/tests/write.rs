use super::*;

#[test]
fn write_project_file_effect_payload_requires_complete_consistent_state() {
    for payload in [
        r#"{"changed":false,"execution_state":"completed","error":"missing state_changed"}"#,
        r#"{"changed":false,"state_changed":true,"execution_state":"completed","error":"contradictory effect"}"#,
        r#"{"changed":false,"state_changed":false,"execution_state":"outcome_unknown","error":"uncertain"}"#,
        r#"{"changed":false,"execution_state":"completed"}"#,
    ] {
        let result = write_project_file_agent_stdout_result(payload);
        assert!(!result.success);
        assert_eq!(result.output["execution_state"], "outcome_unknown");
        assert!(result.output["state_changed"].is_null());
        assert!(result.output.get("error").is_none());
    }

    let rolled_back = write_project_file_agent_stdout_result(
        r#"{"changed":false,"state_changed":false,"execution_state":"completed","error":"write failed and parent creation was rolled back"}"#,
    );
    assert!(!rolled_back.success);
    assert_eq!(rolled_back.output["changed"], false);
    assert_eq!(rolled_back.output["state_changed"], false);
    assert_eq!(rolled_back.output["execution_state"], "completed");
}
