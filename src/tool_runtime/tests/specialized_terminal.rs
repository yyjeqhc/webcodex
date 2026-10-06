use super::*;

#[test]
fn specialized_terminal_preserves_raw_state_defaults_and_failure_precedence() {
    for success in [false, true] {
        for state in [
            None,
            Some(Value::Null),
            Some(json!(4)),
            Some(json!("recovering")),
            Some(json!("")),
            Some(json!("outcome_unknown")),
        ] {
            for failure in [
                None,
                Some(Value::Null),
                Some(json!(false)),
                Some(json!("preferred")),
            ] {
                let mut output = json!({"error_kind":"fallback"});
                if let Some(state) = &state {
                    output["execution_state"] = state.clone();
                }
                if let Some(failure) = &failure {
                    output["failure_kind"] = failure.clone();
                }
                let expected_state = state
                    .as_ref()
                    .and_then(Value::as_str)
                    .unwrap_or(if success { "completed" } else { "not_started" });
                let expected_failure = match &failure {
                    None => Some("fallback"),
                    Some(value) => value.as_str(),
                };
                let result = ToolResult {
                    success,
                    output,
                    error: None,
                };
                assert_eq!(
                    specialized_terminal(&result),
                    (expected_state, expected_failure)
                );
            }
        }
    }
}
