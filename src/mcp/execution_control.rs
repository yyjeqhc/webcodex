//! MCP-only schema for the opt-in execution view. Runtime/Runner contracts stay unchanged.
use serde_json::{json, Value};

pub(super) fn project_result(result: &mut crate::tool_runtime::ToolResult) {
    let original = result.output.clone();
    crate::tool_runtime::execution_control::split_result(result);
    if let Some(next) = result.output.pointer_mut("/execution/next") {
        if next["tool"] == "observe_jobs" {
            // Presentation continuity only; same Job, token, wait and posture.
            next["arguments"]["_wc"] = json!({"compact_execution":true});
        }
    }
    // Optional presentation must not widen the existing model-result budget or
    // discard diagnostics to fit. Missing execution is never proof of success.
    if !crate::json_measurement::serialized_json_len(result).is_ok_and(|bytes| {
        bytes <= webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES
    }) {
        result.output = original;
        if let Some(output) = result.output.as_object_mut() {
            output.remove("execution");
            if let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    if let Some(item) = item.as_object_mut() {
                        item.remove("execution");
                    }
                }
            }
        }
    }
}

pub(super) fn add_output_schema(tool: &mut Value, name: &str) {
    if !crate::tool_runtime::execution_control::supported(name) {
        return;
    }
    let Some(schema) = tool.get_mut("outputSchema") else {
        return;
    };
    let execution = json!({
        "type":"object", "required":["state","outcome"], "additionalProperties":false,
        "properties":{
            "state":{"type":"string"},
            "outcome":{"enum":["passed","failed","pending","unknown"]},
            "exit_code":{"type":"integer"}, "job_id":{"type":"string"},
            "observation_ref":{"type":"string"}, "observation_token":{"type":"string"},
            "next":{"type":"object","required":["tool","arguments","follow_up_kind"],
                "properties":{"tool":{"type":"string"},"arguments":{"type":"object"},
                    "follow_up_kind":{"enum":["mechanically_followable","fallback_recovery"]}},
                "additionalProperties":false}
        }
    });
    let view = json!({"type":"object", "required":["execution"],
        "properties":{"execution":execution,"details":{"type":"object"}},
        "additionalProperties":true});
    let output = if name == "observe_jobs" {
        json!({"type":"object","required":["items"],
            "properties":{"items":{"type":"array","maxItems":8,"items":view}},
            "additionalProperties":true})
    } else {
        view
    };
    let ordinary = std::mem::take(schema);
    *schema = json!({"type":"object","anyOf":[ordinary, {
        "type":"object", "required":["success","output","error"],
        "properties":{"success":{"type":"boolean"},"error":{"type":["string","null"]},"output":output},
        "additionalProperties":false
    }]});
}
