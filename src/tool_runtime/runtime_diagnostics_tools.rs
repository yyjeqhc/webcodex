use super::{ToolResult, ToolRuntime};
use serde_json::json;
use webcodex_core::runtime_diagnostics::{
    self, DiagnosticSeverity, RuntimeDiagnosticFilter, RUNTIME_DIAGNOSTIC_MAX_QUERY,
};

fn safe_filter_token(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_-.:".contains(&byte))
}

impl ToolRuntime {
    pub(crate) fn runtime_diagnostics(
        &self,
        severity: Option<String>,
        component: Option<String>,
        correlation_id: Option<String>,
        since: Option<i64>,
        until: Option<i64>,
        limit: Option<usize>,
    ) -> ToolResult {
        let severity = match severity.as_deref() {
            Some(value) => match DiagnosticSeverity::parse(value) {
                Some(value) => Some(value),
                None => {
                    return ToolResult::err_with_output(
                        "severity must be one of info, warn, error",
                        json!({
                            "error_kind": "invalid_diagnostic_severity",
                            "state_changed": false,
                        }),
                    );
                }
            },
            None => None,
        };
        if component
            .as_deref()
            .is_some_and(|value| !safe_filter_token(value))
        {
            return ToolResult::err_with_output(
                "component must be a bounded diagnostic token",
                json!({
                    "error_kind": "invalid_diagnostic_component",
                    "state_changed": false,
                }),
            );
        }
        if correlation_id
            .as_deref()
            .is_some_and(|value| !safe_filter_token(value))
        {
            return ToolResult::err_with_output(
                "correlation_id must be a bounded diagnostic token",
                json!({
                    "error_kind": "invalid_diagnostic_correlation_id",
                    "state_changed": false,
                }),
            );
        }
        if matches!((since, until), (Some(lower), Some(upper)) if lower > upper) {
            return ToolResult::err_with_output(
                "since must be less than or equal to until",
                json!({
                    "error_kind": "invalid_diagnostic_time_range",
                    "state_changed": false,
                }),
            );
        }
        let limit = limit.unwrap_or(50).clamp(1, RUNTIME_DIAGNOSTIC_MAX_QUERY);
        let events = runtime_diagnostics::snapshot(RuntimeDiagnosticFilter {
            severity,
            component: component.as_deref(),
            correlation_id: correlation_id.as_deref(),
            since,
            until,
            limit,
        });
        let stats = runtime_diagnostics::stats();
        ToolResult::ok(json!({
            "capacity": runtime_diagnostics::RUNTIME_DIAGNOSTIC_CAPACITY,
            "buffered_count": stats.buffered_count,
            "dropped_count": stats.dropped_count,
            "oldest_sequence": stats.oldest_sequence,
            "newest_sequence": stats.newest_sequence,
            "info_count": stats.info_count,
            "warn_count": stats.warn_count,
            "error_count": stats.error_count,
            "newest_warn_sequence": stats.newest_warn_sequence,
            "newest_error_sequence": stats.newest_error_sequence,
            "returned_count": events.len(),
            "filters": {
                "severity": severity.map(DiagnosticSeverity::as_wire),
                "component": component,
                "correlation_id": correlation_id,
                "since": since,
                "until": until,
                "limit": limit,
            },
            "events": events,
            "state_changed": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_filter_accepts_only_bounded_tokens() {
        assert!(safe_filter_token("runner_registry"));
        assert!(safe_filter_token("service.lifecycle-v1"));
        assert!(safe_filter_token("wc_job_123"));
        assert!(!safe_filter_token(""));
        assert!(!safe_filter_token("Bearer secret value"));
        assert!(!safe_filter_token(&"x".repeat(129)));
    }

    #[test]
    fn diagnostics_query_rejects_unsafe_correlation_and_reversed_time_range() {
        let runtime = ToolRuntime::new_for_tests();
        let unsafe_id = runtime.runtime_diagnostics(
            None,
            None,
            Some("Bearer secret".to_string()),
            None,
            None,
            Some(10),
        );
        assert!(!unsafe_id.success);
        assert_eq!(
            unsafe_id.output["error_kind"],
            "invalid_diagnostic_correlation_id"
        );

        let reversed = runtime.runtime_diagnostics(None, None, None, Some(20), Some(10), Some(10));
        assert!(!reversed.success);
        assert_eq!(
            reversed.output["error_kind"],
            "invalid_diagnostic_time_range"
        );
    }
}
