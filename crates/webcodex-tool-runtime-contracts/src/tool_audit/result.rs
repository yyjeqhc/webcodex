//! Bounded result allowlists and canonical execution evidence.
use super::*;

pub(super) fn bounded_test_count_assertion_audit(value: &Value) -> Option<Value> {
    let assertion = value.as_object()?;
    let minimum_tests = assertion.get("minimum_tests")?.as_u64()?;
    let actual_tests_run = match assertion.get("actual_tests_run")? {
        Value::Null => Value::Null,
        value => serde_json::json!(value.as_u64()?),
    };
    let status = assertion.get("status")?.as_str()?;
    let reason_code = assertion.get("reason_code")?.as_str()?;
    let evidence_reason_code = assertion.get("evidence_reason_code")?.as_str()?;
    if !matches!(status, "passed" | "failed" | "unproven")
        || !matches!(
            reason_code,
            "minimum_satisfied" | "minimum_not_met" | "test_count_unproven"
        )
        || !matches!(
            evidence_reason_code,
            "complete_summary"
                | "output_truncated"
                | "partial_harness_summary"
                | "no_complete_summary"
                | "incomplete_stream"
        )
    {
        return None;
    }
    Some(serde_json::json!({
        "minimum_tests": minimum_tests,
        "actual_tests_run": actual_tests_run,
        "status": status,
        "reason_code": reason_code,
        "evidence_reason_code": evidence_reason_code,
    }))
}

/// ActionAudit cannot persist the raw canonical evidence that the Session ledger
/// consumes. Narrow definition-owned execution evidence to lifecycle scalars plus
/// bounded validation counts/assertions; Session excerpt/source processing continues
/// to consume its original result.
pub fn canonical_execution_audit_result_for_tool(tool_name: &str, output: &Value) -> Value {
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(tool_name) else {
        return empty_audit_projection();
    };
    if definition.audit_policy().result
        != webcodex_tool_contracts::ToolAuditResultPolicy::CanonicalLedgerEvidence
    {
        return session_log_result_for_tool(tool_name, output);
    }
    if definition.audit_policy().execution.detail
        == webcodex_tool_contracts::ToolAuditExecutionDetail::Omit
    {
        return empty_audit_projection();
    }
    // Canonical ledger evidence is borrowed only; never clone that raw body
    // into ActionAudit. The definition still owns execution eligibility.
    let mut result = serde_json::Map::new();
    for key in [
        "command_started",
        "command_completed",
        "command_ok",
        "passed",
        "terminal",
        "promoted_to_job",
        "changed",
        "state_changed",
        "tool_failure",
    ] {
        if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if let Some(value) = output.get("exit_code").filter(|value| value.is_i64()) {
        result.insert("exit_code".to_string(), value.clone());
    }
    if let Some(kind) = output.get("failure_kind").and_then(Value::as_str) {
        if !kind.is_empty()
            && kind.len() <= 64
            && kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            result.insert("failure_kind".to_string(), Value::String(kind.to_string()));
        }
    }
    if let Some(kind) = output.get("recovery_kind").and_then(Value::as_str) {
        if webcodex_core::runtime_contract::RECOVERY_KIND_VALUES.contains(&kind) {
            result.insert("recovery_kind".to_string(), Value::String(kind.to_string()));
        }
    }
    for key in ["warnings_count", "errors_count"] {
        if let Some(value) = output.get(key).filter(|value| value.is_u64()) {
            result.insert(key.to_string(), value.clone());
        }
    }
    if matches!(
        definition.audit_policy().execution.detail,
        webcodex_tool_contracts::ToolAuditExecutionDetail::TestCounts
            | webcodex_tool_contracts::ToolAuditExecutionDetail::TestAssertions
    ) {
        for key in ["tests_run_count", "tests_passed", "tests_failed"] {
            if let Some(value) = output.get(key).filter(|value| value.is_u64()) {
                result.insert(key.to_string(), value.clone());
            }
        }
        for key in ["tests_detected", "zero_tests_run"] {
            if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
                result.insert(key.to_string(), value.clone());
            }
        }
    }
    if definition.audit_policy().execution.detail
        == webcodex_tool_contracts::ToolAuditExecutionDetail::TestAssertions
    {
        for key in ["require_tests", "no_run"] {
            if let Some(value) = output.get(key).filter(|value| value.is_boolean()) {
                result.insert(key.to_string(), value.clone());
            }
        }
        if let Some(assertion) = output
            .get("test_count_assertion")
            .and_then(bounded_test_count_assertion_audit)
        {
            result.insert("test_count_assertion".to_string(), assertion);
        }
    }
    if let Some(state) = output.get("execution_state").and_then(Value::as_str) {
        if matches!(
            state,
            "completed"
                | "not_started"
                | "outcome_unknown"
                | "timed_out"
                | "pending"
                | "queued"
                | "running"
                | "started"
        ) {
            result.insert(
                "execution_state".to_string(),
                Value::String(state.to_string()),
            );
        }
    }
    if let Some(source) = output.get("source_state") {
        // Only closed source classifications, never the fence/epoch identity.
        if let (Some(freshness), Some(fence)) = (
            source.get("freshness").and_then(Value::as_str),
            source
                .get("observed_mutation_fence")
                .and_then(Value::as_str),
        ) {
            if matches!(freshness, "unproven" | "stale")
                && matches!(fence, "uncrossed" | "crossed" | "unknown")
            {
                result.insert(
                    "source_state".to_string(),
                    serde_json::json!({
                        "freshness": freshness, "observed_mutation_fence": fence,
                    }),
                );
            }
        }
    }
    Value::Object(result)
}

pub fn session_log_result_for_tool(tool_name: &str, output: &Value) -> Value {
    let Some(definition) = webcodex_tool_contracts::lookup_tool_definition(tool_name) else {
        return empty_audit_projection();
    };
    match definition.audit_policy().result {
        webcodex_tool_contracts::ToolAuditResultPolicy::CanonicalLedgerEvidence => output.clone(),
        webcodex_tool_contracts::ToolAuditResultPolicy::Fields(fields) => {
            project_declared_result_fields(fields, output)
        }
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::BrowserObservation,
        ) => browser_observation_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::BrowserControl,
        ) => browser_control_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::ComputerObservation,
        ) => computer_observation_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::ComputerControl,
        ) => computer_control_result_audit(output),
        webcodex_tool_contracts::ToolAuditResultPolicy::Semantic(
            webcodex_tool_contracts::ToolAuditSemanticResultPolicy::CodingAgentObservation,
        ) => coding_agent_observation_result_audit(output),
    }
}

pub(super) fn project_declared_result_fields(
    fields: &[webcodex_tool_contracts::ToolAuditResultField],
    output: &Value,
) -> Value {
    use webcodex_tool_contracts::ToolAuditResultField;

    let mut projected = serde_json::Map::new();
    for field in fields {
        let (key, value) = match *field {
            ToolAuditResultField::Value {
                output: key,
                source,
            } => (key, output.get(source).cloned().unwrap_or(Value::Null)),
            ToolAuditResultField::Pointer {
                output: key,
                pointer,
            } => (key, output.pointer(pointer).cloned().unwrap_or(Value::Null)),
            ToolAuditResultField::ArrayLen {
                output: key,
                source,
            } => (
                key,
                output
                    .get(source)
                    .and_then(Value::as_array)
                    .map(|items| Value::from(items.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::PointerArrayLen {
                output: key,
                pointer,
            } => (
                key,
                output
                    .pointer(pointer)
                    .and_then(Value::as_array)
                    .map(|items| Value::from(items.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::StringBytes {
                output: key,
                source,
            } => (
                key,
                output
                    .get(source)
                    .and_then(Value::as_str)
                    .map(|value| Value::from(value.len()))
                    .unwrap_or(Value::Null),
            ),
            ToolAuditResultField::Presence {
                output: key,
                source,
            } => (key, Value::Bool(output.get(source).is_some())),
            ToolAuditResultField::StringPresent {
                output: key,
                source,
            } => (
                key,
                Value::Bool(output.get(source).and_then(Value::as_str).is_some()),
            ),
            ToolAuditResultField::PointerNonNull {
                output: key,
                pointer,
            } => (
                key,
                Value::Bool(
                    output
                        .pointer(pointer)
                        .is_some_and(|value| !value.is_null()),
                ),
            ),
        };
        projected.insert(key.to_string(), value);
    }
    Value::Object(projected)
}

pub(super) fn copy_existing_audit_value(
    projected: &mut serde_json::Map<String, Value>,
    output: &Value,
    key: &'static str,
) {
    if let Some(value) = output.get(key) {
        projected.insert(key.to_string(), value.clone());
    }
}

pub(super) fn browser_observation_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    for key in [
        "execution_state",
        "state_changed",
        "error_kind",
        "count",
        "total_count",
        "truncated",
        "retained_count",
        "console_retained",
        "console_count",
        "console_truncated",
        "network_retained",
        "network_count",
        "network_truncated",
        "browser_id",
        "page_id",
        "snapshot_generation",
        "node_count",
        "mime_type",
        "width",
        "height",
        "file_bytes",
        "sha256",
    ] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    for (source, target) in [
        ("targets", "target_count"),
        ("browsers", "browser_count"),
        ("pages", "page_count"),
        ("nodes", "projected_node_count"),
    ] {
        if let Some(count) = output.get(source).and_then(Value::as_array).map(Vec::len) {
            projected.insert(target.to_string(), Value::from(count));
        }
    }
    Value::Object(projected)
}

pub(super) fn browser_control_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    for key in [
        "execution_state",
        "state_changed",
        "error_kind",
        "browser_id",
        "page_id",
        "page_count",
    ] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

pub(super) fn computer_observation_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();

    if output.get("targets").is_some() {
        for key in ["count", "total_count", "truncated"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("windows").is_some()
        || output.get("displays").is_some()
        || output.get("applications").is_some()
    {
        for key in ["count", "truncated"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("trusted").is_some() {
        for key in ["platform", "trusted"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("nodes").is_some() {
        for key in [
            "surface_id",
            "observation_generation",
            "node_count",
            "truncated",
            "max_depth",
            "max_nodes",
        ] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("elements").is_some() {
        for key in [
            "surface_id",
            "observation_generation",
            "count",
            "scanned_nodes",
            "truncated",
        ] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("content_base64").is_some() {
        if output.get("display_id").is_some() {
            for key in [
                "display_id",
                "snapshot_generation",
                "source_width",
                "source_height",
                "width",
                "height",
                "mime_type",
                "file_bytes",
                "sha256",
                "captured_at_unix_ms",
            ] {
                copy_existing_audit_value(&mut projected, output, key);
            }
        } else {
            if let Some(surface_id) = output.pointer("/surface/surface_id") {
                projected.insert("surface_id".to_string(), surface_id.clone());
            }
            for key in [
                "source_width",
                "source_height",
                "width",
                "height",
                "mime_type",
                "file_bytes",
                "captured_at_unix_ms",
            ] {
                copy_existing_audit_value(&mut projected, output, key);
            }
            projected.insert(
                "region_present".to_string(),
                Value::Bool(output.get("region").is_some()),
            );
        }
    } else if output.get("available").is_some() {
        for key in ["available", "text_bytes", "success"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    } else if output.get("element_id").is_some() {
        for key in ["surface_id", "element_id", "observation_generation"] {
            copy_existing_audit_value(&mut projected, output, key);
        }
    }

    for key in ["error_kind", "execution_state"] {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

pub(super) fn computer_control_result_audit(output: &Value) -> Value {
    let mut projected = serde_json::Map::new();
    let keys: &[&str] = if output.get("application_id").is_some() {
        &[
            "application_id",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("display_id").is_some() && output.get("x").is_some() {
        &[
            "display_id",
            "snapshot_generation",
            "x",
            "y",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("text_bytes").is_some() && output.get("element_id").is_none() {
        &[
            "text_bytes",
            "success",
            "error_kind",
            "execution_state",
            "state_changed",
        ]
    } else if output.get("key").is_some() {
        &["surface_id", "key", "modifiers", "success"]
    } else if output.get("element_id").is_some() && output.get("text_bytes").is_some() {
        &["surface_id", "element_id", "text_bytes", "success"]
    } else if output.get("element_id").is_some() && output.get("action").is_some() {
        &["surface_id", "element_id", "action", "success"]
    } else if output.get("element_id").is_some() {
        &["surface_id", "element_id", "success"]
    } else {
        &["surface_id", "success"]
    };
    for key in keys {
        copy_existing_audit_value(&mut projected, output, key);
    }
    Value::Object(projected)
}

pub(super) fn coding_agent_observation_result_audit(output: &Value) -> Value {
    let mut kind_counts = serde_json::Map::new();
    let mut event_count = 0usize;
    let mut event_body_bytes = 0usize;
    if let Some(events) = output.get("events").and_then(Value::as_array) {
        event_count = events.len();
        for event in events {
            if let Some(kind) = event.get("kind").and_then(Value::as_str) {
                let count = kind_counts.get(kind).and_then(Value::as_u64).unwrap_or(0) + 1;
                kind_counts.insert(kind.to_string(), Value::from(count));
            }
            event_body_bytes = event_body_bytes.saturating_add(
                event
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::len)
                    .unwrap_or(0),
            );
        }
    }
    serde_json::json!({
        "run_id": output.get("run_id").cloned().unwrap_or(Value::Null),
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "provider_id": output.get("provider_id").cloned().unwrap_or(Value::Null),
        "state": output.get("state").cloned().unwrap_or(Value::Null),
        "execution_state": output.get("execution_state").cloned().unwrap_or(Value::Null),
        "event_count": event_count,
        "event_kind_counts": kind_counts,
        "event_body_bytes": event_body_bytes,
        "has_more": output.get("has_more").cloned().unwrap_or(Value::Null),
        "history_lost": output.get("history_lost").cloned().unwrap_or(Value::Null),
        "first_retained_sequence": output.get("first_retained_sequence").cloned().unwrap_or(Value::Null),
        "terminal_stop_reason": output.pointer("/terminal/stop_reason").cloned().unwrap_or(Value::Null),
        "terminal_error_code": output.pointer("/terminal/error_code").cloned().unwrap_or(Value::Null),
        "terminal_completed_at": output.pointer("/terminal/completed_at").cloned().unwrap_or(Value::Null),
        "recovery_kind": output.get("recovery_kind").cloned().unwrap_or(Value::Null),
        "error_kind": output.get("error_kind").cloned().unwrap_or(Value::Null),
    })
}
