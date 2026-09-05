use serde_json::{json, Value};

fn client_id_schema(description: &str) -> Value {
    json!({
        "type": "string",
        "minLength": 1,
        "maxLength": 128,
        "description": description,
    })
}

pub fn runner_config_check_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "client_id": client_id_schema("Exact caller-visible Runner client_id whose startup-bound runner.toml candidate is checked. No filesystem path is accepted.")
        },
        "required": ["client_id"],
        "additionalProperties": false,
    })
}

pub fn runner_config_reload_input_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "client_id": client_id_schema("Exact caller-visible Runner client_id whose startup-bound runner.toml candidate is activated. No filesystem path is accepted."),
            "expected_generation": {
                "type": "integer",
                "minimum": 1,
                "description": "Optimistic active-config generation fence previously observed from runner_config_check, runtime_status, or list_runners. A mismatch is rejected before candidate validation or mutation."
            }
        },
        "required": ["client_id", "expected_generation"],
        "additionalProperties": false,
    })
}
