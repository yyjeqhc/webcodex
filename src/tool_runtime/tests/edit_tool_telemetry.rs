//! Edit tool usage telemetry — dispatch integration and safety checks.

use super::super::edit_tool_telemetry::{
    clear_test_edit_tool_usage, edit_tool_surface, record_contains_sensitive_keys,
    take_test_edit_tool_usage, EditToolSurface, EDIT_TOOL_USAGE_EVENT, TELEMETRY_CATEGORY_EDIT,
};
use super::super::*;
use super::support::*;
use serde_json::json;

#[tokio::test]
async fn dispatch_records_edit_tool_usage_without_sensitive_args() {
    clear_test_edit_tool_usage();
    let runtime = test_runtime();

    // Compatibility path: fails without a registered agent/project, but selection
    // still must be counted.
    let compatibility = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceInFile {
                project: "agent:oe:missing".to_string(),
                path: "src/secret.rs".to_string(),
                old: "token=super-secret-value".to_string(),
                new: "token=other".to_string(),
                expected_replacements: None,
                allow_multiple: None,
                session_id: None,
            },
            None,
        )
        .await;
    assert!(!compatibility.success);

    // Canonical path selection.
    let canonical = runtime
        .dispatch_with_auth(
            ToolCall::ApplyTextEdits {
                project: "agent:oe:missing".to_string(),
                changes: vec![ApplyFileChangeInput {
                    kind: ApplyFileChangeKind::Edit,
                    path: "src/secret.rs".to_string(),
                    to_path: None,
                    content: None,
                    edits: vec![ApplyTextEditInput {
                        kind: ApplyTextEditKind::ReplaceExact,
                        old_text: Some("a".to_string()),
                        new_text: Some("b".to_string()),
                        anchor_text: None,
                    }],
                    expected_sha256: Some("a".repeat(64)),
                }],
                dry_run: Some(true),
                session_id: None,
            },
            None,
        )
        .await;
    assert!(!canonical.success);

    // Advanced path selection.
    let advanced = runtime
        .dispatch_with_auth(
            ToolCall::WriteProjectFile {
                project: "agent:oe:missing".to_string(),
                path: "src/secret.rs".to_string(),
                content: "fn main() { /* secret body */ }".to_string(),
                overwrite: Some(true),
                expected_sha256: None,
                expected_content_prefix: None,
                session_id: None,
            },
            None,
        )
        .await;
    assert!(!advanced.success);

    // Non-edit tools must not emit edit telemetry.
    let _ = runtime
        .dispatch_with_auth(
            ToolCall::ListTools {
                category: None,
                features: None,
                summary_only: false,
                limit: None,
            },
            None,
        )
        .await;

    let events = take_test_edit_tool_usage();
    assert_eq!(
        events.len(),
        3,
        "only edit tools should emit usage events: {events:?}"
    );

    assert_eq!(events[0].tool_name, "replace_in_file");
    assert_eq!(events[0].edit_surface, EditToolSurface::Compatibility);
    assert_eq!(events[0].category, TELEMETRY_CATEGORY_EDIT);
    assert!(!events[0].success);

    assert_eq!(events[1].tool_name, "apply_text_edits");
    assert_eq!(events[1].edit_surface, EditToolSurface::Canonical);
    assert!(!events[1].success);

    assert_eq!(events[2].tool_name, "write_project_file");
    assert_eq!(events[2].edit_surface, EditToolSurface::Advanced);
    assert!(!events[2].success);

    for event in &events {
        assert!(!record_contains_sensitive_keys(event));
        // Payload must never echo argument secrets or paths from the call.
        let serialized = format!("{event:?}");
        assert!(
            !serialized.contains("super-secret-value"),
            "telemetry must not record argument secrets: {serialized}"
        );
        assert!(
            !serialized.contains("secret body"),
            "telemetry must not record file contents: {serialized}"
        );
        assert!(
            !serialized.contains("src/secret.rs"),
            "telemetry must not record file paths: {serialized}"
        );
        assert!(
            !serialized.contains("token="),
            "telemetry must not record tokens: {serialized}"
        );
    }
}

#[tokio::test]
async fn edit_tool_usage_does_not_change_session_ledger_shape() {
    clear_test_edit_tool_usage();
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        Some("agent:oe:private-drop".to_string()),
        Some("edit telemetry".into()),
    );

    let _ = runtime
        .dispatch_with_auth(
            ToolCall::ReplaceInFile {
                project: "agent:oe:missing".to_string(),
                path: "src/x.rs".to_string(),
                old: "a".to_string(),
                new: "b".to_string(),
                expected_replacements: None,
                allow_multiple: None,
                session_id: Some(session.session_id.clone()),
            },
            None,
        )
        .await;

    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .expect("session summary");
    assert!(
        summary
            .events
            .iter()
            .any(|e| e.kind == "tool_call_started" && e.tool_name == "replace_in_file"),
        "session ledger must still record tool_call_started"
    );
    assert!(
        summary
            .events
            .iter()
            .any(|e| e.kind == "tool_call_finished" && e.tool_name == "replace_in_file"),
        "session ledger must still record tool_call_finished"
    );
    // Telemetry must remain a parallel structured-log stream — no new ledger kinds.
    assert!(
        summary.events.iter().all(|e| {
            e.kind == "tool_call_started"
                || e.kind == "tool_call_finished"
                || e.kind == "session_started"
        }),
        "edit telemetry must not inject new session ledger event kinds: {:?}",
        summary.events.iter().map(|e| &e.kind).collect::<Vec<_>>()
    );

    let usage = take_test_edit_tool_usage();
    assert_eq!(usage.len(), 1);
    assert_eq!(usage[0].tool_name, "replace_in_file");
    assert_eq!(usage[0].edit_surface, EditToolSurface::Compatibility);
}

#[test]
fn edit_surface_table_matches_canonicalization_contract() {
    // Keep the classification table aligned with the product contract used by
    // tool descriptions / discovery (canonical vs advanced vs compatibility).
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
    assert_eq!(edit_tool_surface("validate_patch"), None);
    assert_eq!(edit_tool_surface("read_file"), None);
    assert_eq!(EDIT_TOOL_USAGE_EVENT, "edit_tool_usage");
    assert_eq!(TELEMETRY_CATEGORY_EDIT, "edit");
}

#[test]
fn sample_edit_tool_args_are_not_required_by_telemetry_module() {
    // Sanity: telemetry classification is name-only; sample args (paths/content)
    // used elsewhere for schema fixtures must not be needed to classify tools.
    let _ = sample_tool_args("apply_text_edits");
    let _ = sample_tool_args("replace_in_file");
    let _ = json!({"path": "ignored-by-telemetry"});
    assert!(edit_tool_surface("apply_text_edits").is_some());
}
