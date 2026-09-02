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
                        occurrence: None,
                        line_scope: None,
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
        2,
        "only edit tools should emit usage events: {events:?}"
    );

    assert_eq!(events[0].tool_name, "apply_text_edits");
    assert_eq!(events[0].edit_surface, EditToolSurface::Canonical);
    assert_eq!(events[0].category, TELEMETRY_CATEGORY_EDIT);
    assert!(!events[0].success);

    assert_eq!(events[1].tool_name, "write_project_file");
    assert_eq!(events[1].edit_surface, EditToolSurface::Advanced);
    assert!(!events[1].success);

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
async fn kernel_generic_telemetry_and_edit_enrichment_each_emit_once() {
    clear_test_edit_tool_usage();
    let runtime = test_runtime();
    let outcome = runtime
        .call_tool_with_context(
            super::super::kernel::ToolCallRequest {
                tool_name: "write_project_file".to_string(),
                arguments: json!({
                    "project": "agent:oe:missing",
                    "path": "src/private.rs",
                    "content": "PRIVATE edit body",
                    "overwrite": true
                }),
            },
            super::super::kernel::ToolCallContext {
                transport: super::super::kernel::ToolTransport::Api,
                session_id: None,
                auth: None,
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: Default::default(),
            },
        )
        .await;
    let result = outcome.result.as_ref().expect("tool result");
    let generic = outcome
        .model_ergonomics
        .as_ref()
        .expect("one generic model-visible completion")
        .record_for_tool_result(result)
        .expect("generic record");
    assert_eq!(generic.tool_name, "write_project_file");
    assert_eq!(generic.tool_category, "edit");
    assert_eq!(generic.success, result.success);

    let edit_events = take_test_edit_tool_usage();
    assert_eq!(
        edit_events.len(),
        1,
        "edit-specific enrichment must remain one event"
    );
    assert_eq!(edit_events[0].tool_name, "write_project_file");
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
            ToolCall::WriteProjectFile {
                project: "agent:oe:missing".to_string(),
                path: "src/x.rs".to_string(),
                content: "a".to_string(),
                overwrite: None,
                expected_sha256: None,
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
            .any(|e| e.kind == "tool_call_started" && e.tool_name == "write_project_file"),
        "session ledger must still record tool_call_started"
    );
    assert!(
        summary
            .events
            .iter()
            .any(|e| e.kind == "tool_call_finished" && e.tool_name == "write_project_file"),
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
    assert_eq!(usage[0].tool_name, "write_project_file");
    assert_eq!(usage[0].edit_surface, EditToolSurface::Advanced);
}

#[test]
fn edit_surface_table_matches_canonicalization_contract() {
    // Keep the classification table aligned with the product contract used by
    // tool descriptions / discovery (canonical vs advanced).
    assert_eq!(
        edit_tool_surface("apply_text_edits"),
        Some(EditToolSurface::Canonical)
    );
    assert_eq!(
        edit_tool_surface("apply_unified_diff"),
        Some(EditToolSurface::Canonical)
    );
    assert_eq!(
        edit_tool_surface("write_project_file"),
        Some(EditToolSurface::Advanced)
    );
    // Removed legacy compatibility tools are no longer classified.
    for name in [
        "replace_in_file",
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
        "apply_patch_checked",
        "validate_patch",
    ] {
        assert_eq!(edit_tool_surface(name), None, "{name}");
    }
    assert_eq!(edit_tool_surface("read_file"), None);
    assert_eq!(EDIT_TOOL_USAGE_EVENT, "edit_tool_usage");
    assert_eq!(TELEMETRY_CATEGORY_EDIT, "edit");
}

#[test]
fn sample_edit_tool_args_are_not_required_by_telemetry_module() {
    // Sanity: telemetry classification is name-only; sample args (paths/content)
    // used elsewhere for schema fixtures must not be needed to classify tools.
    // `apply_text_edits` is the small exact guarded-edit fallback and can
    // synthesize args from its spec; telemetry classification is name-only and
    // is asserted via `edit_tool_surface` above, so no sample-args construction
    // is required beyond the canonical tools.
    let _ = sample_tool_args("apply_text_edits");
    let _ = json!({"path": "ignored-by-telemetry"});
    assert!(edit_tool_surface("apply_text_edits").is_some());
}
