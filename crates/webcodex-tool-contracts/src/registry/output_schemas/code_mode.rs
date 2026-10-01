use serde_json::{json, Value};

use super::common::{array_schema, schema_type, wrapped_output_schema};

fn stats_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bounded orchestration evidence for this one-shot Code Mode execution.",
        "additionalProperties": false,
        "properties": {
            "tool_calls": {"type": "integer", "minimum": 0, "maximum": 32},
            "max_in_flight": {"type": "integer", "minimum": 0, "maximum": 8},
            "duration_ms": {"type": "integer", "minimum": 0},
            "returned_bytes": {"type": "integer", "minimum": 0, "maximum": 65536}
        },
        "required": ["tool_calls", "max_in_flight", "duration_ms", "returned_bytes"]
    })
}

fn content_schema() -> Value {
    let mut schema = array_schema(
        schema_type("string", "One bounded text(value) emission."),
        "Only text(value) emissions selected by the JavaScript orchestration. Nested raw ToolResults are not copied here automatically.",
    );
    schema["maxItems"] = json!(256);
    schema
}

fn effect_receipt_schema() -> Value {
    json!({
        "type": "object",
        "description": "Sparse correctness receipt for consequential canonical child calls that actually crossed the orchestration dispatch boundary.",
        "additionalProperties": false,
        "properties": {
            "consequential_calls": {"type": "integer", "minimum": 0, "maximum": 32},
            "known_results": {"type": "integer", "minimum": 0, "maximum": 32},
            "job_handoffs": {"type": "integer", "minimum": 0, "maximum": 32},
            "outcome_unknown": {"type": "integer", "minimum": 0, "maximum": 32},
            "children": {
                "type": "array",
                "maxItems": 32,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "ordinal": {"type": "integer", "minimum": 1, "maximum": 32},
                        "tool": {"type": "string", "maxLength": 128},
                        "outcome": {"type": "string", "enum": ["known_result", "job_handoff", "outcome_unknown"]},
                        "success": {"type": "boolean", "description": "Canonical business success for a known result; false is a known failure, not outcome_unknown. Never proves current source."},
                        "source_state": super::common::validation_source_state_schema(),
                        "state_changed": {"type": "boolean", "description": "Authoritative canonical state-change truth, present only for a known mutation result."},
                        "job_id": {"type": "string", "description": "Canonical Job identity, present only for a normal same-execution Job handoff."},
                        "continuation": {"type": "object", "description": "Parser-ready canonical Job continuation, present only when returned by the child ToolResult."}
                    },
                    "required": ["ordinal", "tool", "outcome"]
                }
            }
        },
        "required": ["consequential_calls", "known_results", "job_handoffs", "outcome_unknown", "children"]
    })
}

fn child_failure_schema() -> Value {
    json!({
        "type": "object",
        "description": "Bounded identity for one nested call attempt that failed before a canonical business ToolResult existed. The ordinal is diagnostic only, not retry or durable execution identity.",
        "additionalProperties": false,
        "properties": {
            "ordinal": {"type": "integer", "minimum": 1, "maximum": 32},
            "tool": {"type": "string", "maxLength": 128},
            "failure_kind": {
                "type": "string",
                "enum": [
                    "invalid_arguments",
                    "insufficient_scope",
                    "tool_not_admitted",
                    "composition_policy_denied",
                    "frontend_closed",
                    "mutation_budget_exceeded",
                    "host_failure",
                    "host_task_failure"
                ]
            },
            "message": {"type": "string", "maxLength": 2048}
        },
        "required": ["ordinal", "tool", "failure_kind", "message"]
    })
}

fn limit_schema() -> Value {
    json!({
        "type": "object",
        "description": "Actual Code Mode frontend hard-limit evidence when the runtime can prove it.",
        "additionalProperties": false,
        "properties": {
            "kind": {"type": "string", "enum": ["nested_tool_calls", "text_output_bytes", "text_output_items"]},
            "allowed": {"type": "integer", "minimum": 0},
            "current": {"type": "integer", "minimum": 0},
            "attempted": {"type": "integer", "minimum": 0}
        },
        "required": ["kind", "allowed", "current", "attempted"]
    })
}

fn recovery_schema() -> Value {
    json!({
        "type": "object",
        "description": "Small deterministic recovery projection. Job identity is intentionally absent: existing Job handoffs remain canonical only in effect_receipt.children.",
        "additionalProperties": false,
        "properties": {
            "retry_same_call_unchanged": {"type": "boolean", "description": "False for Code Mode frontend failures; callers must apply the listed recovery action or reconcile existing effects first."},
            "actions": {
                "type": "array",
                "maxItems": 3,
                "items": {
                    "type": "string",
                    "enum": [
                        "fix_code_mode_request",
                        "fix_code_mode_source",
                        "fix_child_arguments",
                        "obtain_required_scope",
                        "remove_or_replace_child_call",
                        "reduce_mutation_attempts",
                        "inspect_child_host_failure",
                        "reduce_or_bound_code_mode_work",
                        "reduce_child_calls",
                        "reduce_text_projection",
                        "observe_existing_job_continuations",
                        "reconcile_effect_state_before_retry"
                    ]
                }
            }
        },
        "required": ["retry_same_call_unchanged", "actions"]
    })
}

fn bounded_failure_message_schema() -> Value {
    let mut schema = schema_type(
        "string",
        "Model-facing bounded frontend failure detail. When consequential children were dispatched, the message warns against blindly rerunning the whole JavaScript program.",
    );
    schema["maxLength"] = json!(16_384);
    schema
}

fn failure_kind_schema() -> Value {
    json!({
        "type": "string",
        "enum": [
            "invalid_request",
            "runtime_error",
            "child_call_failed",
            "timeout",
            "tool_call_budget_exceeded",
            "output_limit_exceeded"
        ],
        "description": "Present on a bounded Code Mode runtime/host failure. Ordinary nested ToolResult business failures remain JavaScript values and do not become this field."
    })
}

pub(super) fn output_schema_for_tool(name: &str) -> Option<Value> {
    match name {
        "execute_code_mode" => Some(wrapped_output_schema(vec![
            ("content", content_schema()),
            ("stats", stats_schema()),
            ("message", bounded_failure_message_schema()),
            ("failure_kind", failure_kind_schema()),
            ("child_failure", child_failure_schema()),
            ("limit", limit_schema()),
            ("recovery", recovery_schema()),
        ])),
        "execute_effectful_code_mode" | "execute_mutating_code_mode" => {
            Some(wrapped_output_schema(vec![
                ("content", content_schema()),
                ("stats", stats_schema()),
                ("effect_receipt", effect_receipt_schema()),
                ("message", bounded_failure_message_schema()),
                ("failure_kind", failure_kind_schema()),
                ("child_failure", child_failure_schema()),
                ("limit", limit_schema()),
                ("recovery", recovery_schema()),
            ]))
        }
        _ => None,
    }
}
