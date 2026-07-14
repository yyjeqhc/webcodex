//! Edit tool usage telemetry (phase 1).
//!
//! Emits always-on structured logs for edit-surface tool calls so operators can
//! measure whether models prefer canonical edit tools over compatibility paths.
//!
//! Design constraints:
//! - No new database tables, Action Audit columns, session ledger fields, or
//!   OpenAPI/MCP/schema changes.
//! - Never log arguments, file paths, file contents, patches, secrets, or tokens.
//! - Reuses existing `tracing` infrastructure (same family as `tool_request_trace`).
//! - Does not change tool execution semantics, permissions, or session behavior.

use super::ToolResult;
use std::time::Instant;

/// High-level tool family for this telemetry stream.
pub(crate) const TELEMETRY_CATEGORY_EDIT: &str = "edit";

/// Event name written to structured logs / metrics pipelines.
pub(crate) const EDIT_TOOL_USAGE_EVENT: &str = "edit_tool_usage";

/// How a specific edit tool sits on the preferred-vs-legacy surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditToolSurface {
    /// Preferred precise/local or checked multi-file paths.
    Canonical,
    /// Valid but non-preferred specialized paths (whole-file write, raw patch).
    Advanced,
    /// Retained for existing workflows; prefer canonical for new work.
    Compatibility,
}

impl EditToolSurface {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Advanced => "advanced",
            Self::Compatibility => "compatibility",
        }
    }
}

/// Classify an edit tool, or `None` when the tool is outside the edit surface
/// tracked by this phase-1 telemetry.
pub(crate) fn edit_tool_surface(tool_name: &str) -> Option<EditToolSurface> {
    match tool_name {
        "apply_text_edits" | "apply_patch_checked" => Some(EditToolSurface::Canonical),
        "write_project_file" | "apply_patch" => Some(EditToolSurface::Advanced),
        "replace_in_file"
        | "replace_exact_block"
        | "insert_before_pattern"
        | "insert_after_pattern"
        | "replace_line_range"
        | "insert_at_line"
        | "delete_line_range" => Some(EditToolSurface::Compatibility),
        _ => None,
    }
}

/// Safe, argument-free usage record for one edit tool invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EditToolUsageRecord {
    pub(crate) tool_name: &'static str,
    pub(crate) category: &'static str,
    pub(crate) edit_surface: EditToolSurface,
    pub(crate) success: bool,
    pub(crate) duration_ms: u64,
    /// Optional coarse error classification (never free-form user content).
    pub(crate) error_kind: Option<&'static str>,
}

/// Start a usage timer when `tool_name` is an edit-surface tool.
pub(crate) fn start_edit_tool_usage(tool_name: &'static str) -> Option<EditToolUsageGuard> {
    let edit_surface = edit_tool_surface(tool_name)?;
    Some(EditToolUsageGuard {
        tool_name,
        edit_surface,
        started: Instant::now(),
        finished: false,
    })
}

/// RAII guard: records one structured log line when finished (or on drop if
/// the dispatch path aborts without an explicit finish).
pub(crate) struct EditToolUsageGuard {
    tool_name: &'static str,
    edit_surface: EditToolSurface,
    started: Instant,
    finished: bool,
}

impl EditToolUsageGuard {
    pub(crate) fn finish_with_result(&mut self, result: &ToolResult) {
        if self.finished {
            return;
        }
        self.finished = true;
        let record = EditToolUsageRecord {
            tool_name: self.tool_name,
            category: TELEMETRY_CATEGORY_EDIT,
            edit_surface: self.edit_surface,
            success: result.success,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            error_kind: safe_error_kind(result),
        };
        emit_edit_tool_usage(&record);
    }

    fn finish_incomplete(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let record = EditToolUsageRecord {
            tool_name: self.tool_name,
            category: TELEMETRY_CATEGORY_EDIT,
            edit_surface: self.edit_surface,
            success: false,
            duration_ms: self.started.elapsed().as_millis().min(u64::MAX as u128) as u64,
            error_kind: Some("incomplete"),
        };
        emit_edit_tool_usage(&record);
    }
}

impl Drop for EditToolUsageGuard {
    fn drop(&mut self) {
        self.finish_incomplete();
    }
}

/// Build a log-safe record and emit it. Public to unit tests as pure field set.
pub(crate) fn emit_edit_tool_usage(record: &EditToolUsageRecord) {
    #[cfg(test)]
    test_sink::push(record.clone());

    tracing::info!(
        event = EDIT_TOOL_USAGE_EVENT,
        tool_name = record.tool_name,
        category = record.category,
        edit_surface = record.edit_surface.as_str(),
        success = record.success,
        duration_ms = record.duration_ms,
        error_kind = record.error_kind.unwrap_or("-"),
        "{EDIT_TOOL_USAGE_EVENT}"
    );
}

/// Extract only coarse, allowlisted error kinds from tool results.
///
/// Never returns free-form error messages (which may include paths or snippets).
fn safe_error_kind(result: &ToolResult) -> Option<&'static str> {
    if result.success {
        return None;
    }
    let raw = result
        .output
        .get("error_kind")
        .and_then(|v| v.as_str())
        .or_else(|| result.output.get("failure_kind").and_then(|v| v.as_str()))
        .or_else(|| result.output.get("code").and_then(|v| v.as_str()));
    raw.and_then(sanitize_error_kind).or(Some("runtime_error"))
}

fn sanitize_error_kind(kind: &str) -> Option<&'static str> {
    // Allowlist only; anything else collapses to a coarse bucket so free-form
    // codes/messages cannot leak into telemetry fields.
    match kind {
        "invalid_arguments" => Some("invalid_arguments"),
        "insufficient_scope" => Some("insufficient_scope"),
        "session_guard_denied" => Some("session_guard_denied"),
        "session_closed" => Some("session_closed"),
        "session_project_mismatch" => Some("session_project_mismatch"),
        "tool_disabled" => Some("tool_disabled"),
        "permission_denied" | "permission_execution_denied" => Some("permission_denied"),
        "policy_rejected" => Some("policy_rejected"),
        "confirmation_required" => Some("confirmation_required"),
        "agent_offline" | "agent_unavailable" => Some("agent_unavailable"),
        "timeout" => Some("timeout"),
        "not_found" => Some("not_found"),
        "runtime_error" => Some("runtime_error"),
        "incomplete" => Some("incomplete"),
        _ => None,
    }
}

/// True when a telemetry field map (or serialized log payload) would be unsafe.
/// Used by tests to assert absence of sensitive keys/values.
#[cfg(test)]
pub(crate) fn record_contains_sensitive_keys(record: &EditToolUsageRecord) -> bool {
    // Structural: only the known safe fields exist on the record type.
    // Also reject if any string field accidentally embeds path/content markers
    // from test fixtures that should never appear in telemetry.
    let surface = record.edit_surface.as_str();
    let kind = record.error_kind.unwrap_or("");
    let haystacks = [record.tool_name, record.category, surface, kind];
    for h in haystacks {
        if h.contains('/') || h.contains('\\') || h.contains('\n') {
            return true;
        }
        for banned in [
            "content",
            "patch",
            "old_text",
            "new_text",
            "arguments",
            "secret",
            "token",
            "password",
        ] {
            // tool_name may legitimately contain none of these; category/surface
            // are fixed. error_kind allowlist is fixed. This is a safety net.
            if h.contains(banned) && h != record.tool_name {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod test_sink {
    use super::EditToolUsageRecord;
    use std::cell::RefCell;

    thread_local! {
        static EVENTS: RefCell<Vec<EditToolUsageRecord>> = const { RefCell::new(Vec::new()) };
    }

    pub(crate) fn push(record: EditToolUsageRecord) {
        EVENTS.with(|events| events.borrow_mut().push(record));
    }

    pub(crate) fn take() -> Vec<EditToolUsageRecord> {
        EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }

    pub(crate) fn clear() {
        let _ = take();
    }
}

#[cfg(test)]
pub(crate) use test_sink::{
    clear as clear_test_edit_tool_usage, take as take_test_edit_tool_usage,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifies_canonical_advanced_and_compatibility_edit_tools() {
        assert_eq!(
            edit_tool_surface("apply_text_edits"),
            Some(EditToolSurface::Canonical)
        );
        assert_eq!(
            edit_tool_surface("apply_patch_checked"),
            Some(EditToolSurface::Canonical)
        );
        assert_eq!(
            edit_tool_surface("write_project_file"),
            Some(EditToolSurface::Advanced)
        );
        assert_eq!(
            edit_tool_surface("apply_patch"),
            Some(EditToolSurface::Advanced)
        );
        for name in [
            "replace_in_file",
            "replace_exact_block",
            "insert_before_pattern",
            "insert_after_pattern",
            "replace_line_range",
            "insert_at_line",
            "delete_line_range",
        ] {
            assert_eq!(
                edit_tool_surface(name),
                Some(EditToolSurface::Compatibility),
                "{name}"
            );
        }
    }

    #[test]
    fn non_edit_tools_are_not_tracked() {
        for name in [
            "read_file",
            "run_shell",
            "validate_patch",
            "list_tools",
            "save_project_artifact",
            "git_status",
            "cargo_check",
        ] {
            assert_eq!(edit_tool_surface(name), None, "{name}");
        }
    }

    #[test]
    fn telemetry_record_has_no_sensitive_fields() {
        let record = EditToolUsageRecord {
            tool_name: "replace_in_file",
            category: TELEMETRY_CATEGORY_EDIT,
            edit_surface: EditToolSurface::Compatibility,
            success: false,
            duration_ms: 12,
            error_kind: Some("runtime_error"),
        };
        assert!(!record_contains_sensitive_keys(&record));
        assert_eq!(record.category, "edit");
        assert_eq!(record.edit_surface.as_str(), "compatibility");
        assert_eq!(record.tool_name, "replace_in_file");
    }

    #[test]
    fn safe_error_kind_never_returns_freeform_messages() {
        let result = ToolResult::err_with_output(
            "failed to write /secret/path/with secrets token=abc",
            json!({
                "path": "/secret/path",
                "content": "user source code here",
                "patch": "@@ -1 +1 @@\n-secret",
                "error_kind": "policy_rejected",
            }),
        );
        assert_eq!(safe_error_kind(&result), Some("policy_rejected"));

        let unknown = ToolResult::err_with_output(
            "boom /tmp/foo",
            json!({ "error_kind": "something_custom_with_/path" }),
        );
        // Unknown kinds collapse to runtime_error rather than echoing free-form.
        assert_eq!(safe_error_kind(&unknown), Some("runtime_error"));
    }

    #[test]
    fn guard_emits_one_event_with_correct_name_and_surface() {
        clear_test_edit_tool_usage();
        let mut guard = start_edit_tool_usage("apply_text_edits").expect("edit tool");
        guard.finish_with_result(&ToolResult::ok(json!({ "ok": true })));
        drop(guard);
        let events = take_test_edit_tool_usage();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name, "apply_text_edits");
        assert_eq!(events[0].category, "edit");
        assert_eq!(events[0].edit_surface, EditToolSurface::Canonical);
        assert!(events[0].success);
        assert!(events[0].error_kind.is_none());
        assert!(!record_contains_sensitive_keys(&events[0]));
    }

    #[test]
    fn start_returns_none_for_non_edit_tools() {
        assert!(start_edit_tool_usage("read_file").is_none());
    }

    #[test]
    fn incomplete_drop_emits_failure_once() {
        clear_test_edit_tool_usage();
        {
            let _guard = start_edit_tool_usage("replace_in_file").expect("edit tool");
            // drop without finish
        }
        let events = take_test_edit_tool_usage();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].tool_name, "replace_in_file");
        assert_eq!(events[0].edit_surface, EditToolSurface::Compatibility);
        assert!(!events[0].success);
        assert_eq!(events[0].error_kind, Some("incomplete"));
    }
}
