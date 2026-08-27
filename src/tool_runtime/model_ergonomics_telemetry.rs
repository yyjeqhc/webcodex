//! Privacy-bounded telemetry projection for model-visible runtime tool calls.
//!
//! This module owns no persistence. The shared tool kernel measures invocation
//! latency and transports finalize the record from the exact model-facing
//! `ToolResult` projection before attaching it to the existing Action Audit row.

use super::tool_definition::model_visible_tool_definitions;
use super::{ToolResult, RECOVERY_KIND_VALUES};
use serde::Serialize;
use serde_json::Value;
#[cfg(test)]
use std::time::Duration;
use std::time::Instant;

const MAX_STRUCTURED_KIND_BYTES: usize = 64;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContextAckShape {
    Unsupported,
    Missing,
    Revision,
    Invalid,
}

#[derive(Debug)]
pub(crate) struct ModelErgonomicsTimer {
    tool_name: &'static str,
    tool_category: &'static str,
    started: Instant,
    context_ack_shape: ContextAckShape,
    finish_summary_only: Option<bool>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ModelErgonomicsCompletion {
    tool_name: &'static str,
    tool_category: &'static str,
    duration_ms: u64,
    context_ack_shape: ContextAckShape,
    finish_summary_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct ModelErgonomicsRecord {
    pub(crate) schema_version: u8,
    pub(crate) tool_name: &'static str,
    pub(crate) tool_category: &'static str,
    pub(crate) success: bool,
    pub(crate) duration_ms: u64,
    pub(crate) serialized_result_bytes: Option<u64>,
    pub(crate) error_kind: Option<String>,
    pub(crate) failure_kind: Option<String>,
    pub(crate) recovery_kind: Option<String>,
    pub(crate) execution_state: Option<String>,
    pub(crate) context_continuity_eligible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context_ack_present: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context_continuity_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_recovery_event_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_recovery_truncated: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) session_history_lost: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) finish_summary_only: Option<bool>,
}

impl ModelErgonomicsTimer {
    pub(crate) fn start(tool_name: &str) -> Option<Self> {
        Self::start_with_protocol(tool_name, &Value::Null, false)
    }

    pub(crate) fn start_with_protocol(
        tool_name: &str,
        arguments: &Value,
        context_continuity_capable: bool,
    ) -> Option<Self> {
        let definition =
            model_visible_tool_definitions().find(|definition| definition.name == tool_name)?;
        let context_ack_shape = if context_continuity_capable {
            match arguments.as_object().and_then(|object| {
                object.get(super::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD)
            }) {
                None => ContextAckShape::Missing,
                Some(value) if value.as_u64().is_some() => ContextAckShape::Revision,
                Some(_) => ContextAckShape::Invalid,
            }
        } else {
            ContextAckShape::Unsupported
        };
        let finish_summary_only = (tool_name == "finish_coding_task").then(|| {
            arguments
                .get("summary_only")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        });
        Some(Self {
            tool_name: definition.name,
            tool_category: definition.category,
            started: Instant::now(),
            context_ack_shape,
            finish_summary_only,
        })
    }

    pub(crate) fn finish(self) -> ModelErgonomicsCompletion {
        let elapsed = self.started.elapsed();
        ModelErgonomicsCompletion {
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            duration_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            context_ack_shape: self.context_ack_shape,
            finish_summary_only: self.finish_summary_only,
        }
    }

    #[cfg(test)]
    fn finish_after(self, elapsed: Duration) -> ModelErgonomicsCompletion {
        ModelErgonomicsCompletion {
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            duration_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
            context_ack_shape: self.context_ack_shape,
            finish_summary_only: self.finish_summary_only,
        }
    }
}

impl ModelErgonomicsCompletion {
    /// Finalize telemetry from the exact `ToolResult` value rendered by the API
    /// transport. Serialization failure drops telemetry rather than affecting
    /// the tool outcome.
    pub(crate) fn record_for_tool_result(
        &self,
        result: &ToolResult,
    ) -> Option<ModelErgonomicsRecord> {
        let serialized_result_bytes = serde_json::to_vec(result).ok()?.len();
        Some(self.record_from_parts(
            result.success,
            &result.output,
            Some(serialized_result_bytes),
        ))
    }

    /// Finalize telemetry from MCP `structuredContent`, which is the final
    /// model-facing ToolResult projection after MCP-only image/resource framing.
    /// The MCP content blocks and JSON-RPC envelope are intentionally excluded.
    pub(crate) fn record_for_structured_content(
        &self,
        structured_content: &Value,
    ) -> Option<ModelErgonomicsRecord> {
        let success = structured_content.get("success")?.as_bool()?;
        let output = structured_content.get("output")?;
        let serialized_result_bytes = serde_json::to_vec(structured_content).ok()?.len();
        Some(self.record_from_parts(success, output, Some(serialized_result_bytes)))
    }

    /// Record an invocation rejected after the kernel recognized a model-visible
    /// tool but before any ToolResult existed. The byte field is intentionally
    /// null rather than measuring a transport-specific error envelope.
    pub(crate) fn record_for_pre_result_failure(
        &self,
        error_kind: &'static str,
    ) -> ModelErgonomicsRecord {
        let mut record = self.record_from_parts(false, &Value::Null, None);
        record.error_kind = Some(error_kind.to_string());
        record
    }

    fn record_from_parts(
        &self,
        success: bool,
        output: &Value,
        serialized_result_bytes: Option<usize>,
    ) -> ModelErgonomicsRecord {
        let (error_kind, failure_kind, recovery_kind) = if success {
            (None, None, None)
        } else {
            (
                structured_kind(output, "error_kind"),
                structured_kind(output, "failure_kind"),
                recovery_kind(output),
            )
        };
        let continuity = continuity_facts(self.context_ack_shape, output);
        ModelErgonomicsRecord {
            schema_version: 2,
            tool_name: self.tool_name,
            tool_category: self.tool_category,
            success,
            duration_ms: self.duration_ms,
            serialized_result_bytes: serialized_result_bytes
                .map(|bytes| bytes.min(u64::MAX as usize) as u64),
            error_kind,
            failure_kind,
            recovery_kind,
            execution_state: execution_state(output),
            context_continuity_eligible: continuity.eligible,
            context_ack_present: continuity.ack_present,
            context_continuity_status: continuity.status,
            session_recovery_event_count: continuity.recovery_event_count,
            session_recovery_truncated: continuity.recovery_truncated,
            session_history_lost: continuity.history_lost,
            finish_summary_only: self.finish_summary_only,
        }
    }
}

#[derive(Debug)]
struct ContinuityFacts {
    eligible: bool,
    ack_present: Option<bool>,
    status: Option<String>,
    recovery_event_count: Option<u64>,
    recovery_truncated: Option<bool>,
    history_lost: Option<bool>,
}

fn continuity_facts(ack_shape: ContextAckShape, output: &Value) -> ContinuityFacts {
    let eligible = !matches!(ack_shape, ContextAckShape::Unsupported);
    if !eligible {
        return ContinuityFacts {
            eligible: false,
            ack_present: None,
            status: None,
            recovery_event_count: None,
            recovery_truncated: None,
            history_lost: None,
        };
    }
    let ack_present = Some(!matches!(ack_shape, ContextAckShape::Missing));
    if output
        .get("session_context_revision")
        .and_then(Value::as_u64)
        .is_none()
    {
        return ContinuityFacts {
            eligible: true,
            ack_present,
            status: None,
            recovery_event_count: None,
            recovery_truncated: None,
            history_lost: None,
        };
    }
    let detailed_status = output
        .pointer("/session_continuity/status")
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "exact" | "unacknowledged" | "behind" | "invalid"));
    let status = detailed_status.map(str::to_string).or_else(|| {
        let status = match ack_shape {
            ContextAckShape::Missing => "unacknowledged",
            ContextAckShape::Revision => "exact",
            ContextAckShape::Invalid => "invalid",
            ContextAckShape::Unsupported => return None,
        };
        Some(status.to_string())
    });
    let recovery = output.get("session_recovery");
    let recovery_event_count = recovery
        .and_then(|value| value.get("model_facing_events"))
        .and_then(Value::as_array)
        .map(|events| events.len().min(u64::MAX as usize) as u64)
        .or_else(|| status.as_ref().map(|_| 0));
    let recovery_truncated = recovery
        .and_then(|value| value.get("truncated"))
        .and_then(Value::as_bool)
        .or_else(|| status.as_ref().map(|_| false));
    let history_lost = output
        .pointer("/session_continuity/history_lost")
        .and_then(Value::as_bool)
        .or_else(|| {
            recovery
                .and_then(|value| value.get("history_lost"))
                .and_then(Value::as_bool)
        })
        .or_else(|| status.as_ref().map(|_| false));
    ContinuityFacts {
        eligible: true,
        ack_present,
        status,
        recovery_event_count,
        recovery_truncated,
        history_lost,
    }
}

fn structured_kind(output: &Value, field: &str) -> Option<String> {
    let value = output.get(field)?.as_str()?.trim();
    if value.is_empty()
        || value.len() > MAX_STRUCTURED_KIND_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return None;
    }
    Some(value.to_string())
}

fn recovery_kind(output: &Value) -> Option<String> {
    let value = output.get("recovery_kind")?.as_str()?;
    RECOVERY_KIND_VALUES
        .contains(&value)
        .then(|| value.to_string())
}

fn execution_state(output: &Value) -> Option<String> {
    let value = output.get("execution_state")?.as_str()?;
    matches!(
        value,
        "not_started"
            | "pending"
            | "running"
            | "started"
            | "outcome_unknown"
            | "completed"
            | "cancelled"
            | "timed_out"
    )
    .then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn completion(tool_name: &str, duration_ms: u64) -> ModelErgonomicsCompletion {
        ModelErgonomicsTimer::start(tool_name)
            .expect("model-visible tool")
            .finish_after(Duration::from_millis(duration_ms))
    }

    #[test]
    fn success_record_uses_exact_utf8_tool_result_bytes() {
        let result = ToolResult::ok(json!({"text": "中文", "count": 2}));
        let record = completion("tool_manifest", 7)
            .record_for_tool_result(&result)
            .unwrap();
        let expected = serde_json::to_vec(&result).unwrap().len() as u64;
        let chars = serde_json::to_string(&result).unwrap().chars().count() as u64;
        assert_eq!(record.serialized_result_bytes, Some(expected));
        assert!(
            expected > chars,
            "UTF-8 multibyte text must count bytes, not chars"
        );
        assert_eq!(record.duration_ms, 7);
        assert!(record.success);
        assert_eq!(record.tool_name, "tool_manifest");
        assert_eq!(record.tool_category, "runtime");
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
    }

    #[test]
    fn failure_record_consumes_only_structured_recovery_fields() {
        let private = "PRIVATE error prose /tmp/secret.rs token=abc search needle";
        let result = ToolResult::err_with_output(
            private,
            json!({
                "error_kind": "stale_surface",
                "failure_kind": "not_started",
                "recovery_kind": "reobserve",
                "execution_state": "not_started",
                "command": private,
                "path": "/tmp/secret.rs",
                "query": "search needle",
                "message": private
            }),
        );
        let record = completion("computer_control", 3)
            .record_for_tool_result(&result)
            .unwrap();
        assert!(!record.success);
        assert_eq!(record.error_kind.as_deref(), Some("stale_surface"));
        assert_eq!(record.failure_kind.as_deref(), Some("not_started"));
        assert_eq!(record.recovery_kind.as_deref(), Some("reobserve"));
        assert_eq!(record.execution_state.as_deref(), Some("not_started"));
        let telemetry = serde_json::to_string(&record).unwrap();
        for forbidden in [private, "/tmp/secret.rs", "search needle", "token=abc"] {
            assert!(
                !telemetry.contains(forbidden),
                "telemetry leaked {forbidden}: {telemetry}"
            );
        }
    }

    #[test]
    fn success_never_invents_failure_or_recovery_metadata() {
        let result = ToolResult::ok(json!({
            "error_kind": "stale_surface",
            "failure_kind": "outcome_unknown",
            "recovery_kind": "retry_same"
        }));
        let record = completion("tool_manifest", 0)
            .record_for_tool_result(&result)
            .unwrap();
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
    }

    #[test]
    fn invalid_structured_kinds_are_not_promoted_to_telemetry() {
        let result = ToolResult::err_with_output(
            "private prose",
            json!({
                "error_kind": "PRIVATE arbitrary text / path",
                "failure_kind": "x".repeat(MAX_STRUCTURED_KIND_BYTES + 1),
                "recovery_kind": "blind_retry",
                "execution_state": "maybe"
            }),
        );
        let record = completion("tool_manifest", 0)
            .record_for_tool_result(&result)
            .unwrap();
        assert_eq!(record.error_kind, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
        assert_eq!(record.execution_state, None);
    }

    #[test]
    fn pre_result_failure_counts_invocation_without_fabricating_tool_result_bytes() {
        let record = completion("read_file", 2).record_for_pre_result_failure("invalid_arguments");
        assert!(!record.success);
        assert_eq!(record.error_kind.as_deref(), Some("invalid_arguments"));
        assert_eq!(record.serialized_result_bytes, None);
        assert_eq!(record.failure_kind, None);
        assert_eq!(record.recovery_kind, None);
        assert_eq!(record.execution_state, None);
    }

    #[test]
    fn protocol_telemetry_distinguishes_unsupported_missing_exact_and_recovery_states() {
        let unsupported = ModelErgonomicsTimer::start_with_protocol("read_file", &json!({}), false)
            .unwrap()
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(json!({"session_context_revision": 1})))
            .unwrap();
        assert!(!unsupported.context_continuity_eligible);
        assert_eq!(unsupported.context_ack_present, None);
        assert_eq!(unsupported.context_continuity_status, None);

        let missing = ModelErgonomicsTimer::start_with_protocol("read_file", &json!({}), true)
            .unwrap()
            .finish_after(Duration::ZERO)
            .record_for_tool_result(&ToolResult::ok(json!({"session_context_revision": 1})))
            .unwrap();
        assert!(missing.context_continuity_eligible);
        assert_eq!(missing.context_ack_present, Some(false));
        assert_eq!(
            missing.context_continuity_status.as_deref(),
            Some("unacknowledged")
        );
        assert_eq!(missing.session_recovery_event_count, Some(0));

        let ack_field =
            super::super::sessions::TOOL_CALL_ACK_SESSION_CONTEXT_REVISION_INTERNAL_FIELD;
        let exact =
            ModelErgonomicsTimer::start_with_protocol("read_file", &json!({ack_field: 1}), true)
                .unwrap()
                .finish_after(Duration::ZERO)
                .record_for_tool_result(&ToolResult::ok(json!({"session_context_revision": 2})))
                .unwrap();
        assert_eq!(exact.context_ack_present, Some(true));
        assert_eq!(exact.context_continuity_status.as_deref(), Some("exact"));

        let behind = ModelErgonomicsTimer::start_with_protocol(
            "read_file",
            &json!({ack_field: 1}),
            true,
        )
        .unwrap()
        .finish_after(Duration::ZERO)
        .record_for_tool_result(&ToolResult::ok(json!({
            "session_context_revision": 3,
            "session_continuity": {"status": "behind", "history_lost": false},
            "session_recovery": {"model_facing_events": [{"tool_name": "read_file"}], "truncated": false, "history_lost": false}
        })))
        .unwrap();
        assert_eq!(behind.context_ack_present, Some(true));
        assert_eq!(behind.context_continuity_status.as_deref(), Some("behind"));
        assert_eq!(behind.session_recovery_event_count, Some(1));
        assert_eq!(behind.session_history_lost, Some(false));

        let invalid = ModelErgonomicsTimer::start_with_protocol(
            "read_file",
            &json!({ack_field: "bad"}),
            true,
        )
        .unwrap()
        .finish_after(Duration::ZERO)
        .record_for_tool_result(&ToolResult::ok(json!({
            "session_context_revision": 4,
            "session_continuity": {"status": "invalid", "history_lost": false},
            "session_recovery": {
                "model_facing_events": [],
                "truncated": false,
                "history_lost": false,
                "current_handoff": {"work_performed": []}
            }
        })))
        .unwrap();
        assert_eq!(invalid.context_ack_present, Some(true));
        assert_eq!(
            invalid.context_continuity_status.as_deref(),
            Some("invalid")
        );
        assert_eq!(invalid.session_recovery_event_count, Some(0));
        assert_eq!(invalid.session_history_lost, Some(false));
    }

    #[test]
    fn finish_summary_only_is_taken_from_request_metadata_without_body_capture() {
        for (arguments, expected) in [(json!({"summary_only": true}), true), (json!({}), false)] {
            let record =
                ModelErgonomicsTimer::start_with_protocol("finish_coding_task", &arguments, false)
                    .unwrap()
                    .finish_after(Duration::ZERO)
                    .record_for_tool_result(&ToolResult::ok(json!({"private_body": "do-not-copy"})))
                    .unwrap();
            assert_eq!(record.schema_version, 2);
            assert_eq!(record.finish_summary_only, Some(expected));
            assert!(record.serialized_result_bytes.is_some());
            let serialized = serde_json::to_string(&record).unwrap();
            assert!(!serialized.contains("do-not-copy"));
            assert!(!serialized.contains("session_id"));
            assert!(!serialized.contains("ack_session_context_revision"));
        }
    }

    #[test]
    fn hidden_tools_do_not_start_generic_model_usage_telemetry() {
        assert!(ModelErgonomicsTimer::start("start_coding_task").is_none());
        assert!(ModelErgonomicsTimer::start("definitely_internal_helper").is_none());
    }

    #[test]
    fn every_registered_model_visible_tool_has_generic_telemetry_identity() {
        let specs = super::super::registered_tool_specs();
        assert!(!specs.is_empty());
        for spec in specs {
            let timer = ModelErgonomicsTimer::start(&spec.name)
                .unwrap_or_else(|| panic!("{} bypasses generic telemetry identity", spec.name));
            assert_ne!(
                timer.tool_category, "other",
                "{} has no bounded category",
                spec.name
            );
        }
    }
}
