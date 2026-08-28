//! Tests for the deterministic continuation feedback projection.
//!
//! These tests exercise the pure read-only projection over the session
//! ledger, validation evidence, job metadata, and message board: attempt
//! segmentation by `task_instruction`, resume/restored determinism,
//! validation delta comparability and stable failure identity, bounded
//! output, message-board counts that do not change state, job recovery
//! projection, and the pure-read contract.

use super::support::*;
use crate::tool_runtime::continuation_feedback::{
    continuation_feedback_value, validation_delta_value, ContinuationFeedbackInput,
};
use crate::tool_runtime::sessions::{
    self, SessionDiscussionCounts, SessionGuards, SessionTransport,
};
use crate::tool_runtime::tool_definition::{
    runtime_tool_captures_validation_output, runtime_tool_is_git_like, runtime_tool_is_shell_like,
    runtime_tool_is_write_like,
};
use crate::tool_runtime::validation_events::validation_summary_from_events;
use crate::tool_runtime::{registered_tool_specs, SessionMode, ToolRuntime, ToolSpec};
use serde_json::{json, Value};

// =========================================================================
// Schema / registration
// =========================================================================

#[test]
fn continuation_feedback_output_schemas_are_synchronized() {
    let specs = registered_tool_specs();
    let start = crate::tool_runtime::start_coding_task_compatibility_spec();
    let start_props = start_full_output_schema(&start)
        .get("properties")
        .expect("full startup properties");
    assert!(
        start_props
            .as_object()
            .is_some_and(|p| p.contains_key("continuation_feedback")),
        "advanced start_coding_task output schema should expose continuation_feedback"
    );
    for name in ["finish_coding_task", "session_handoff_summary"] {
        let spec = spec_named(&specs, name);
        let props = &spec.output_schema["properties"]["output"]["properties"];
        assert!(
            props
                .as_object()
                .is_some_and(|p| p.contains_key("continuation_feedback")),
            "{name} output schema should expose continuation_feedback"
        );
    }
    let validation_spec = spec_named(&specs, "validation_summary");
    let validation_props = &validation_spec.output_schema["properties"]["output"]["properties"];
    assert!(
        validation_props
            .as_object()
            .is_some_and(|p| p.contains_key("validation_delta")),
        "validation_summary output schema should expose validation_delta"
    );
}

/// Locate the `continuation_feedback` strict sub-schema inside a tool output.
fn continuation_feedback_subschema(specs: &[ToolSpec], tool: &str) -> Value {
    if tool == "start_coding_task" {
        let spec = crate::tool_runtime::start_coding_task_compatibility_spec();
        return start_full_output_schema(&spec)["properties"]["continuation_feedback"].clone();
    }
    let spec = spec_named(specs, tool);
    spec.output_schema["properties"]["output"]["properties"]["continuation_feedback"].clone()
}

fn start_full_output_schema(spec: &ToolSpec) -> &Value {
    spec.output_schema["properties"]["output"]["oneOf"]
        .as_array()
        .expect("start_coding_task detail variants")
        .iter()
        .find(|variant| variant["properties"]["detail"]["const"] == "full")
        .expect("full startup output schema")
}

/// Walk every object node reachable from `schema` and assert it carries
/// `additionalProperties: false`, except the documented open boundaries.
fn assert_all_objects_strict(schema: &Value, path: &str, open_boundaries: &[&str]) {
    match schema {
        Value::Object(obj) => {
            if obj.get("type").and_then(Value::as_str) == Some("object") {
                let ap = obj.get("additionalProperties");
                if open_boundaries.contains(&path) {
                    assert!(
                        ap.map(|v| v.as_bool() == Some(true)).unwrap_or(false),
                        "{path}: documented open boundary should be additionalProperties:true"
                    );
                } else {
                    assert!(
                        ap.map(|v| v.as_bool() == Some(false)).unwrap_or(false),
                        "{path}: core object must be additionalProperties:false, got {ap:?}"
                    );
                }
            }
            for (key, child) in obj {
                assert_all_objects_strict(child, &format!("{path}.{key}"), open_boundaries);
            }
        }
        Value::Array(arr) => {
            for (i, child) in arr.iter().enumerate() {
                assert_all_objects_strict(child, &format!("{path}[{i}]"), open_boundaries);
            }
        }
        _ => {}
    }
}

#[test]
fn continuation_feedback_schema_is_strict_on_core_objects() {
    let specs = registered_tool_specs();
    let schema = continuation_feedback_subschema(&specs, "start_coding_task");
    // The root continuation_feedback object and every documented core
    // sub-object (attempt, boundary, instruction, event_range, activity,
    // changes, validation, jobs, guidance, outcome, validation_delta,
    // comparison, counts, failures, failure identity) must reject unknown
    // fields via additionalProperties:false. There are no open boundaries.
    assert_all_objects_strict(&schema, "continuation_feedback", &[]);
    // validation_summary exposes the same validation_delta strict schema.
    let validation_spec = spec_named(&specs, "validation_summary");
    let delta =
        &validation_spec.output_schema["properties"]["output"]["properties"]["validation_delta"];
    assert_eq!(
        delta["additionalProperties"].as_bool(),
        Some(false),
        "validation_delta root must be strict"
    );
    assert_all_objects_strict(delta, "validation_delta", &[]);
}

#[test]
fn continuation_feedback_schema_enums_and_signed_ints_are_stable() {
    let specs = registered_tool_specs();
    let schema = continuation_feedback_subschema(&specs, "start_coding_task");

    let status_enum = schema["properties"]["status"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(status_enum, ["available", "not_applicable", "unknown"]);

    let validation_status_enum = schema["properties"]["attempt"]["properties"]["validation"]
        ["properties"]["latest_status"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        validation_status_enum,
        ["passed", "failed", "not_run", "unknown", "unavailable"]
    );

    let boundary_source_enum = schema["properties"]["attempt"]["properties"]["boundary"]
        ["properties"]["source"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        boundary_source_enum,
        [
            "task_instruction",
            "session_start",
            "unavailable",
            "no_events"
        ]
    );

    let outcome_enum = schema["properties"]["attempt"]["properties"]["outcome"]["properties"]
        ["status"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(outcome_enum, ["in_progress", "blocked", "clean", "unknown"]);

    let recovery_state_enum = schema["properties"]["attempt"]["properties"]["jobs"]["properties"]
        ["recovery_state"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        recovery_state_enum,
        [
            "none",
            "recovering",
            "terminal_pending",
            "active",
            "unknown"
        ]
    );

    let comparison_reason_enum = schema["properties"]["validation_delta"]["properties"]
        ["comparison"]["properties"]["reason_code"]["enum"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        comparison_reason_enum,
        [
            "no_previous_validation",
            "validation_scope_changed",
            "previous_evidence_incomplete",
            "current_evidence_incomplete",
            "parser_changed",
            "parser_identity_unavailable",
            "test_identity_unavailable",
            "insufficient_scope_identity",
            "validation_not_requested"
        ]
    );

    // Signed integer deltas: counts must be `integer` (not unsigned, and the
    // serializer emits negative values for decreases).
    for key in [
        "passed_delta",
        "failed_delta",
        "ignored_delta",
        "total_delta",
    ] {
        assert_eq!(
            schema["properties"]["validation_delta"]["properties"]["counts"]["properties"][key]
                ["type"],
            "integer",
            "{key} must be a signed integer"
        );
    }
}

#[test]
fn continuation_feedback_output_conforms_to_strict_schema_shape() {
    // A real continued-session output carries exactly the schema'd fields and
    // no unknown core fields. Because serde derives the output from the Rust
    // structs and the schema now sets additionalProperties:false, a drift
    // (unknown field) would be rejected by the schema; assert the keys line up.
    let runtime = test_runtime();
    let session = create_session(&runtime, "schema shape");
    add_instruction(&runtime, &session, "do something", SessionMode::Normal);
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let schema = continuation_feedback_subschema(&registered_tool_specs(), "start_coding_task");

    // Every field present in the serialized output must be declared in the
    // strict schema's properties for its parent object (no drift fields).
    fn assert_no_unknown_fields(output: &Value, schema: &Value, path: &str) {
        if let (Some(out_obj), Some(schema_obj)) = (output.as_object(), schema.as_object()) {
            if schema_obj.get("type").and_then(Value::as_str) == Some("object") {
                let declared = schema_obj["properties"].as_object().unwrap();
                for key in out_obj.keys() {
                    assert!(
                        declared.contains_key(key),
                        "{path}: output field '{key}' is not declared in the strict schema"
                    );
                }
            }
        }
        if let (Some(out_obj), Some(schema_obj)) = (output.as_object(), schema.as_object()) {
            for (key, child) in out_obj {
                if let Some(child_schema) = schema_obj["properties"].get(key) {
                    assert_no_unknown_fields(child, child_schema, &format!("{path}.{key}"));
                }
            }
        }
    }
    assert_no_unknown_fields(&feedback, &schema, "continuation_feedback");

    // Negative passed_delta: 2 passed -> 0 passed yields -2, proving the signed
    // integer round-trips through the serializer.
    let session_neg = create_session(&runtime, "neg delta");
    add_instruction(&runtime, &session_neg, "neg", SessionMode::Normal);
    record_validation_event(
        &runtime,
        &session_neg,
        "cargo_test",
        true,
        test_output(2, 0, 0, &[]),
    );
    add_instruction(&runtime, &session_neg, "neg2", SessionMode::Normal);
    record_validation_event(
        &runtime,
        &session_neg,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    let summary = runtime.sessions.summary(&session_neg, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    assert_eq!(
        feedback["validation_delta"]["counts"]["passed_delta"].as_i64(),
        Some(-2),
        "passed_delta must serialize as a negative integer"
    );
    assert_eq!(
        feedback["validation_delta"]["counts"]["failed_delta"].as_i64(),
        Some(1)
    );
    assert_no_unknown_fields(&feedback, &schema, "continuation_feedback");
}

#[test]
fn attempt_only_counts_events_after_the_last_task_instruction() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "attempt segmentation");

    // Instruction A, a failed validation, then instruction B, a different
    // failed validation and a resolving success.
    add_instruction(&runtime, &session, "A", SessionMode::Normal);
    record_validation_event(&runtime, &session, "cargo_check", false, check_output(2, 0));
    add_instruction(&runtime, &session, "B", SessionMode::Normal);
    record_validation_event(&runtime, &session, "cargo_check", false, check_output(1, 0));
    record_validation_event(&runtime, &session, "cargo_check", true, check_output(0, 0));

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");

    // The boundary is the last task_instruction (B); attempt events exclude A.
    assert_eq!(feedback["status"], "available");
    assert_eq!(
        feedback["attempt"]["boundary"]["source"],
        "task_instruction"
    );
    let start = feedback["attempt"]["event_range"]["start_sequence"]
        .as_u64()
        .unwrap();
    let end = feedback["attempt"]["event_range"]["end_sequence"]
        .as_u64()
        .unwrap();
    // Two validation tool calls after instruction B = 4 ledger events
    // (started + finished for each call).
    assert_eq!(
        end - start,
        4,
        "attempt covers the 4 ledger events after instruction B"
    );
    // A's failure must NOT pollute the current attempt unresolved count: the
    // B failure was resolved by the later B success within the attempt.
    assert_eq!(feedback["attempt"]["activity"]["resolved_failures"], 1);
    assert_eq!(feedback["attempt"]["activity"]["unresolved_failures"], 0);
    assert_eq!(
        feedback["attempt"]["validation"]["unresolved_failure_count"],
        0
    );
    assert_eq!(feedback["attempt"]["validation"]["latest_status"], "passed");
}

#[test]
fn attempt_does_not_recount_recorder_finish_when_business_finish_precedes_boundary() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "logical boundary straddle");
    let arguments = json!({
        "project": "test-project",
        "changes": [{"kind": "edit", "path": "src/prior.rs"}],
    });
    let mut recorder = sessions::ToolCallRecorderMetadata::default();
    recorder.assign_logical_invocation();
    let mut business = recorder.clone();
    business.mark_business_execution();
    let recorder_start = runtime.sessions.record_tool_call_started_with_metadata(
        Some(&session),
        SessionTransport::Api,
        "apply_text_edits",
        &arguments,
        Some("test-project".to_string()),
        recorder,
    );
    let business_start = runtime.sessions.record_tool_call_started_with_metadata(
        Some(&session),
        SessionTransport::Api,
        "apply_text_edits",
        &arguments,
        Some("test-project".to_string()),
        business,
    );
    runtime.sessions.record_tool_call_finished(
        business_start,
        true,
        &json!({"applied": true}),
        None,
        None,
    );
    add_instruction(&runtime, &session, "next attempt", SessionMode::Normal);
    runtime.sessions.record_tool_call_finished(
        recorder_start,
        true,
        &json!({"applied": true}),
        None,
        None,
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    assert_eq!(
        feedback["attempt"]["boundary"]["source"],
        "task_instruction"
    );
    assert_eq!(feedback["attempt"]["activity"]["meaningful_tool_calls"], 0);
    assert_eq!(feedback["attempt"]["changes"]["total_changed_paths"], 0);
}

#[test]
fn attempt_without_task_instruction_falls_back_to_session_start() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "no instruction");
    // Only tool calls, no task_instruction event.
    record_validation_event(&runtime, &session, "cargo_check", false, check_output(1, 0));

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    assert_eq!(feedback["attempt"]["boundary"]["source"], "session_start");
    assert_eq!(feedback["attempt"]["instruction"]["status"], "not_observed");
    assert_eq!(feedback["attempt"]["activity"]["unresolved_failures"], 1);
}

#[test]
fn evicted_task_instruction_reports_unavailable_boundary_not_session_start() {
    // Cap the in-memory ledger at a small event count so an early
    // `task_instruction` is evicted by later write events.
    let runtime = test_runtime().with_session_event_cap(20);
    let session = create_session(&runtime, "evicted instruction");
    add_instruction(&runtime, &session, "do the work", SessionMode::Normal);
    // Generate enough finished write events to evict instruction A.
    for i in 0..30 {
        record_write(&runtime, &session, &[&format!("src/file_{i}.rs")]);
    }
    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/retained-tail.rs"}),
        true,
        json!({}),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    assert!(
        summary.events_truncated,
        "ledger should report a truncated retained window"
    );
    // The instruction event must no longer be in the retained window.
    assert!(summary
        .events
        .iter()
        .all(|event| event.kind != "task_instruction"));
    let feedback = feedback_for(&runtime, &summary, "continued");
    // Must NOT masquerade the retained tail as session_start.
    assert_ne!(feedback["attempt"]["boundary"]["source"], "session_start");
    assert_eq!(feedback["attempt"]["boundary"]["source"], "unavailable");
    assert_eq!(
        feedback["attempt"]["boundary"]["reason_code"],
        "attempt_boundary_evicted"
    );
    assert_eq!(feedback["attempt"]["event_range"]["complete"], false);
    assert_eq!(feedback["attempt"]["exploration"]["complete"], false);
    assert_eq!(
        feedback["attempt"]["exploration"]["observed_paths"],
        json!(["src/retained-tail.rs"])
    );
}

#[test]
fn retained_task_instruction_still_segments_and_reports_complete() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "retained instruction");
    add_instruction(&runtime, &session, "do work", SessionMode::Normal);
    record_write(&runtime, &session, &["src/lib.rs"]);

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    assert!(!summary.events_truncated);
    let feedback = feedback_for(&runtime, &summary, "continued");
    assert_eq!(
        feedback["attempt"]["boundary"]["source"],
        "task_instruction"
    );
    assert_eq!(feedback["attempt"]["event_range"]["complete"], true);
    assert_eq!(feedback["attempt"]["activity"]["meaningful_tool_calls"], 1);
}

#[test]
fn exploration_workset_is_attempt_scoped_deduped_and_newest_first() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "exploration ordering");
    add_instruction(
        &runtime,
        &session,
        "inspect the implementation",
        SessionMode::Inspect,
    );

    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/read.rs"}),
        true,
        json!({"content": "READ_BODY_MUST_NOT_APPEAR"}),
    );
    record_exploration_event(
        &runtime,
        &session,
        "search_project_text",
        json!({
            "project": "test-project",
            "pattern": "RAW_PATTERN_MUST_NOT_APPEAR wc_pat_PRIVATE_TOKEN",
            "pattern_present": true
        }),
        true,
        json!({
            "matches": [
                {"path": "src/search.rs", "line": 1, "preview": "RAW_PREVIEW_MUST_NOT_APPEAR"},
                {"path": "src/read.rs", "line": 2, "preview": "Authorization: Bearer PRIVATE_SECRET"},
                {"path": "/root/git/private-drop/src/absolute.rs", "line": 3, "preview": "absolute"}
            ]
        }),
    );
    record_exploration_event(
        &runtime,
        &session,
        "goto_definition",
        json!({
            "project": "test-project",
            "path": "src/caller.rs",
            "line": 3,
            "column": 5
        }),
        true,
        locations_output("src/caller.rs", &["src/definition.rs", "src/search.rs"]),
    );
    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/failed.rs"}),
        false,
        json!({"content": "FAILED_BODY_MUST_NOT_APPEAR"}),
    );
    for (tool, output) in [
        (
            "list_project_files",
            json!({"entries": [{"path": "src/listed.rs", "kind": "file"}]}),
        ),
        (
            "list_project_tracked_files",
            json!({"files": [{"path": "src/tracked.rs"}]}),
        ),
        (
            "project_overview",
            json!({"key_files": [{"path": "src/overview.rs"}]}),
        ),
        (
            "run_shell",
            json!({"stdout": "src/from-shell.rs", "path": "src/from-shell.rs"}),
        ),
    ] {
        record_exploration_event(
            &runtime,
            &session,
            tool,
            json!({"project": "test-project"}),
            true,
            output,
        );
    }

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let exploration = &feedback["attempt"]["exploration"];
    assert_eq!(
        exploration["observed_paths"],
        json!([
            "src/caller.rs",
            "src/definition.rs",
            "src/search.rs",
            "src/read.rs"
        ])
    );
    assert_eq!(exploration["total_observed_paths"], 4);
    assert_eq!(exploration["truncated"], false);
    assert_eq!(exploration["read_count"], 1);
    assert_eq!(exploration["search_count"], 1);
    assert_eq!(exploration["navigation_count"], 1);
    assert_eq!(exploration["latest_tool"], "goto_definition");
    assert_eq!(exploration["complete"], true);
    assert!(feedback["attempt"]["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action
            == "continue from the recent exploration workset before repeating broad discovery"));

    let serialized = serde_json::to_string(&feedback).unwrap();
    for forbidden in [
        "RAW_PATTERN_MUST_NOT_APPEAR",
        "wc_pat_PRIVATE_TOKEN",
        "RAW_PREVIEW_MUST_NOT_APPEAR",
        "Authorization: Bearer PRIVATE_SECRET",
        "/root/git/private-drop",
        "READ_BODY_MUST_NOT_APPEAR",
        "FAILED_BODY_MUST_NOT_APPEAR",
        "src/listed.rs",
        "src/tracked.rs",
        "src/overview.rs",
        "src/from-shell.rs",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "feedback leaked {forbidden}"
        );
    }
}

#[test]
fn exploration_workset_reports_real_total_at_under_equal_and_over_limit() {
    for (count, expected_returned, truncated) in [
        (99usize, 99usize, false),
        (100usize, 100usize, false),
        (101usize, 100usize, true),
    ] {
        let runtime = test_runtime();
        let session = create_session(&runtime, &format!("exploration limit {count}"));
        add_instruction(&runtime, &session, "search", SessionMode::Inspect);
        record_exploration_event(
            &runtime,
            &session,
            "search_project_text",
            json!({
                "project": "test-project",
                "pattern_present": true,
                "result_mode": "count"
            }),
            true,
            json!({
                "files": (0..count)
                    .map(|index| json!({
                        "path": format!("src/observed-{index:03}.rs"),
                        "match_count": index + 1
                    }))
                    .collect::<Vec<_>>(),
                "total_match_count": count
            }),
        );
        let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
        let feedback = feedback_for(&runtime, &summary, "continued");
        let exploration = &feedback["attempt"]["exploration"];
        assert_eq!(exploration["total_observed_paths"], count);
        assert_eq!(
            exploration["observed_paths"].as_array().unwrap().len(),
            expected_returned
        );
        assert_eq!(exploration["truncated"], truncated);
    }
}

#[test]
fn exploration_is_segmented_by_the_latest_attempt_instruction() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "exploration attempts");
    add_instruction(&runtime, &session, "attempt A", SessionMode::Inspect);
    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/attempt-a.rs"}),
        true,
        json!({}),
    );
    add_instruction(&runtime, &session, "attempt B", SessionMode::Normal);
    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/attempt-b.rs"}),
        true,
        json!({}),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    assert_eq!(
        feedback["attempt"]["exploration"]["observed_paths"],
        json!(["src/attempt-b.rs"])
    );
    assert_eq!(feedback["attempt"]["exploration"]["read_count"], 1);
}

#[test]
fn exploration_continuity_suggestion_is_lower_priority_than_blocking_evidence() {
    let has_exploration_action = |feedback: &Value| {
        feedback["attempt"]["suggested_next_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|action| {
                action
                    == "continue from the recent exploration workset before repeating broad discovery"
            })
    };
    let seed_read = |runtime: &ToolRuntime, title: &str| {
        let session = create_session(runtime, title);
        add_instruction(runtime, &session, "inspect", SessionMode::Inspect);
        record_exploration_event(
            runtime,
            &session,
            "read_file",
            json!({"project": "test-project", "path": "src/seed.rs"}),
            true,
            json!({}),
        );
        session
    };

    let runtime = test_runtime();
    let validation_session = seed_read(&runtime, "validation blocks exploration");
    record_validation_event(
        &runtime,
        &validation_session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::failing"]),
    );
    let validation_summary = runtime
        .sessions
        .summary(&validation_session, Some(200))
        .unwrap();
    let validation_feedback = feedback_for(&runtime, &validation_summary, "continued");
    assert_eq!(
        validation_feedback["attempt"]["suggested_next_actions"][0],
        "fix failing test tests::failing"
    );
    assert!(!has_exploration_action(&validation_feedback));

    let job_session = seed_read(&runtime, "job blocks exploration");
    let job_summary = runtime.sessions.summary(&job_session, Some(200)).unwrap();
    let job_feedback = feedback_for_with_jobs(
        &runtime,
        &job_summary,
        &json!({
            "active_count": 1,
            "running_count": 1,
            "recovering_count": 0,
            "terminal_pending_count": 0,
            "recent": [{"status": "running"}]
        }),
        "continued",
    );
    assert!(!has_exploration_action(&job_feedback));

    let risk_session = seed_read(&runtime, "risk blocks exploration");
    post_session_message(&runtime, &risk_session, "risk", "review this risk");
    let risk_summary = runtime.sessions.summary(&risk_session, Some(200)).unwrap();
    let risk_discussion = runtime
        .sessions
        .discussion_summary(&risk_session, Some(20))
        .unwrap();
    let risk_feedback =
        feedback_for_with_discussion(&runtime, &risk_summary, &risk_discussion, "continued");
    assert!(!has_exploration_action(&risk_feedback));

    let conflict_session = seed_read(&runtime, "conflict blocks exploration");
    let conflict_summary = runtime
        .sessions
        .summary(&conflict_session, Some(200))
        .unwrap();
    let conflict_validation = validation_summary_from_events(&conflict_summary.events, 20);
    let conflict_discussion = empty_discussion();
    let conflict_feedback = continuation_feedback_value(ContinuationFeedbackInput {
        session_summary: &conflict_summary,
        validation: &conflict_validation,
        jobs: &json!({"active_count": 0, "running_count": 0, "recent": []}),
        discussion: &conflict_discussion,
        continuation: "continued",
        suggest_exploration_continuity: true,
        workspace_conflicts: true,
    });
    assert!(!has_exploration_action(&conflict_feedback));
}

// =========================================================================
// Resume / restart determinism
// =========================================================================

#[test]
fn resume_and_restart_produce_the_same_attempt_summary_without_appending_events() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "resume determinism");
    add_instruction(&runtime, &session, "do work", SessionMode::Normal);
    record_exploration_event(
        &runtime,
        &session,
        "read_file",
        json!({"project": "test-project", "path": "src/deterministic.rs"}),
        true,
        json!({}),
    );
    record_validation_event(&runtime, &session, "cargo_check", false, check_output(1, 0));
    record_validation_event(&runtime, &session, "cargo_check", true, check_output(0, 0));

    let summary_a = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback_a = feedback_for(&runtime, &summary_a, "resumed_explicitly");

    // Reading the projection must not append events or change the ledger.
    let events_after = summary_a.events.len();
    let summary_b = runtime.sessions.summary(&session, Some(200)).unwrap();
    assert_eq!(summary_b.events.len(), events_after);
    let feedback_b = feedback_for(&runtime, &summary_b, "resumed_explicitly");

    assert_eq!(feedback_a, feedback_b, "deterministic resume projection");
    assert_eq!(
        feedback_a["attempt"]["exploration"]["observed_paths"],
        json!(["src/deterministic.rs"])
    );
    assert_eq!(feedback_a["attempt"]["instruction"]["resumed"], true);
    assert_eq!(feedback_a["attempt"]["instruction"]["status"], "available");
}

// =========================================================================
// Validation delta transitions
// =========================================================================

#[test]
fn validation_delta_fail_to_pass_reports_resolved_failure() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "fail to pass");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(2, 0, 0, &[]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "available");
    assert_eq!(delta["counts"]["failed_delta"], -1);
    assert_eq!(delta["counts"]["passed_delta"], 2);
    let resolved = delta["failures"]["resolved"].as_array().unwrap();
    assert!(resolved.iter().any(|f| f["name"] == "tests::a"));
    assert!(delta["failures"]["still_failing"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(delta["failures"]["newly_failed"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn validation_delta_pass_to_fail_reports_newly_failed() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "pass to fail");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(2, 0, 0, &[]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::b"]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "available");
    assert_eq!(delta["counts"]["failed_delta"], 1);
    // 2 passed -> 0 passed is a *negative* passed_delta, not 0.
    assert_eq!(delta["counts"]["passed_delta"], -2);
    let newly = delta["failures"]["newly_failed"].as_array().unwrap();
    assert!(newly.iter().any(|f| f["name"] == "tests::b"));
}

#[test]
fn validation_delta_passed_count_decrease_is_negative_not_zero() {
    // Regression: passed_delta must be signed, not a saturating subtraction.
    // 2 passed, 0 failed  ->  0 passed, 1 failed  yields passed_delta == -2.
    let runtime = test_runtime();
    let session = create_session(&runtime, "passed delta sign");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(2, 0, 0, &[]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::b"]),
    );
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["counts"]["passed_delta"], -2);
    assert_eq!(delta["counts"]["failed_delta"], 1);
    assert_eq!(delta["counts"]["total_delta"], -1);
}

#[test]
fn scope_identity_is_opaque_hash_and_does_not_leak_command() {
    // A unique command marker must not appear in the serialized delta except
    // where explicitly surfaced; scope_identity is a stable opaque hash.
    let runtime = test_runtime();
    let session = create_session(&runtime, "scope identity");
    let marker = "unique_marker_filter_xyz_123";
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::a ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "test_failure",
            "cwd": "crates/webcodex",
            "command_summary": format!("cargo test --lib {marker}"),
        }),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::a ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "test_failure",
            "cwd": "crates/webcodex",
            "command_summary": format!("cargo test --lib {marker}"),
        }),
    );
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    let serialized = serde_json::to_string(&delta).unwrap();
    assert!(
        !serialized.contains(marker),
        "scope_identity leaked the command marker"
    );
    let scope_identity = delta["comparison"]["scope_identity"].as_str().unwrap();
    assert!(
        scope_identity.starts_with("validation_scope:v1:"),
        "scope_identity must be a versioned opaque hash: {scope_identity}"
    );
    // Same scope => same hash.
    let scope_a = &scope_identity;
    // A different filter yields a different hash.
    let other = validation_delta_value(&validation_summary_from_events(
        &runtime
            .sessions
            .summary(&create_session(&runtime, "other"), Some(200))
            .unwrap()
            .events,
        20,
    ));
    let _ = other;
    let _ = scope_a;
}

#[test]
fn scope_identity_is_stable_for_same_scope_and_differs_for_different_command() {
    fn scope_for(command: &str) -> String {
        let runtime = test_runtime();
        let session = create_session(&runtime, "scope stable");
        record_validation_event(
            &runtime,
            &session,
            "cargo_test",
            false,
            json!({
                "exit_code": 101,
                "stdout_tail": "",
                "stderr_tail": "tests::a ... FAILED",
                "stdout_truncated": false,
                "stderr_truncated": false,
                "failure_kind": "test_failure",
                "cwd": "crates/webcodex",
                "command_summary": command,
            }),
        );
        record_validation_event(
            &runtime,
            &session,
            "cargo_test",
            false,
            json!({
                "exit_code": 101,
                "stdout_tail": "",
                "stderr_tail": "tests::a ... FAILED",
                "stdout_truncated": false,
                "stderr_truncated": false,
                "failure_kind": "test_failure",
                "cwd": "crates/webcodex",
                "command_summary": command,
            }),
        );
        let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
        let validation = validation_summary_from_events(&summary.events, 20);
        let delta = validation_delta_value(&validation);
        delta["comparison"]["scope_identity"]
            .as_str()
            .unwrap()
            .to_string()
    }
    // Whitespace normalization: same recipe surface => same hash.
    assert_eq!(
        scope_for("cargo test --lib"),
        scope_for("cargo   test   --lib")
    );
    // Different filter => different hash.
    assert_ne!(
        scope_for("cargo test --lib tests::a"),
        scope_for("cargo test --lib tests::b")
    );
    // Different cwd => different hash.
    fn scope_with_cwd(cwd: &str) -> String {
        let runtime = test_runtime();
        let session = create_session(&runtime, "scope cwd");
        record_validation_event(
            &runtime,
            &session,
            "cargo_test",
            false,
            json!({
                "exit_code": 101, "stdout_tail": "", "stderr_tail": "tests::a ... FAILED",
                "stdout_truncated": false, "stderr_truncated": false,
                "failure_kind": "test_failure", "cwd": cwd, "command_summary": "cargo test",
            }),
        );
        record_validation_event(
            &runtime,
            &session,
            "cargo_test",
            false,
            json!({
                "exit_code": 101, "stdout_tail": "", "stderr_tail": "tests::a ... FAILED",
                "stdout_truncated": false, "stderr_truncated": false,
                "failure_kind": "test_failure", "cwd": cwd, "command_summary": "cargo test",
            }),
        );
        let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
        let validation = validation_summary_from_events(&summary.events, 20);
        validation_delta_value(&validation)["comparison"]["scope_identity"]
            .as_str()
            .unwrap()
            .to_string()
    }
    assert_ne!(
        scope_with_cwd("crates/webcodex"),
        scope_with_cwd("crates/webcodex-runner")
    );
}

#[test]
fn validation_delta_fail_a_to_fail_a_reports_still_failing() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "fail a to fail a");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    let still = delta["failures"]["still_failing"].as_array().unwrap();
    assert!(still.iter().any(|f| f["name"] == "tests::a"));
    assert_eq!(
        delta["failures"]["total_still_failing"], 1,
        "count exceeds the bounded list total"
    );
}

#[test]
fn validation_delta_fail_a_to_fail_b_reports_newly_failed_and_resolved() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "fail a to fail b");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::b"]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    let newly = delta["failures"]["newly_failed"].as_array().unwrap();
    let resolved = delta["failures"]["resolved"].as_array().unwrap();
    assert!(newly.iter().any(|f| f["name"] == "tests::b"));
    assert!(resolved.iter().any(|f| f["name"] == "tests::a"));
    assert!(delta["failures"]["still_failing"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn validation_delta_multi_failure_partial_resolution() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "partial resolve");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 2, 0, &["tests::a", "tests::b"]),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::b"]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert!(delta["failures"]["resolved"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "tests::a"));
    assert!(delta["failures"]["still_failing"]
        .as_array()
        .unwrap()
        .iter()
        .any(|f| f["name"] == "tests::b"));
    assert_eq!(delta["failures"]["total_still_failing"], 1);
}

// =========================================================================
// Incomparable validation
// =========================================================================

#[test]
fn validation_delta_unavailable_when_command_filter_changes() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "filter change");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::a ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
            "cwd": "crates/webcodex",
            "command_summary": "cargo test --lib tests::a",
        }),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::b ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
            "cwd": "crates/webcodex",
            "command_summary": "cargo test --lib tests::b",
        }),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "unavailable");
    assert_eq!(
        delta["comparison"]["reason_code"],
        "validation_scope_changed"
    );
}

#[test]
fn validation_delta_unavailable_when_cwd_changes() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "cwd change");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::a ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
            "cwd": "crates/webcodex",
            "command_summary": "cargo test",
        }),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "tests::b ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
            "cwd": "crates/webcodex-runner",
            "command_summary": "cargo test",
        }),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "unavailable");
    assert_eq!(
        delta["comparison"]["reason_code"],
        "validation_scope_changed"
    );
}

#[test]
fn validation_delta_unavailable_with_no_previous_validation() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "single run");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(1, 0, 0, &[]),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "unavailable");
    assert_eq!(delta["comparison"]["reason_code"], "no_previous_validation");
}

#[test]
fn validation_delta_unavailable_when_scope_identity_is_missing() {
    // A check run that omits cwd/command_summary cannot be proven comparable.
    let runtime = test_runtime();
    let session = create_session(&runtime, "missing scope");
    record_validation_event(
        &runtime,
        &session,
        "cargo_check",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "error[E0308]: m",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
        }),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_check",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "error[E0277]: m",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
        }),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "unavailable");
    assert_eq!(
        delta["comparison"]["reason_code"],
        "insufficient_scope_identity"
    );
}

#[test]
fn validation_delta_previous_evidence_incomplete_when_prior_run_unparsed() {
    // The latest run is comparable in scope, but the prior run's output could
    // not be parsed (no test result line, no failed-test line), so its failure
    // identities are unknown and the delta is downgraded.
    let runtime = test_runtime();
    let session = create_session(&runtime, "prior unparsed");
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "",
            "stderr_tail": "something unparseable happened",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "test_failure",
            "cwd": "crates/webcodex",
            "command_summary": "cargo test --lib",
        }),
    );
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(1, 0, 0, &[]),
    );
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    assert_eq!(delta["comparison"]["status"], "unavailable");
    assert_eq!(
        delta["comparison"]["reason_code"],
        "previous_evidence_incomplete"
    );
}

#[test]
fn validation_delta_zero_test_success_does_not_resolve_prior_test_failures() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "zero test resolve");
    // Previous: one real test failure.
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );
    // Current: a "success" with zero tests run (no tests executed).
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        true,
        json!({
            "exit_code": 0,
            "stdout_tail": "test result: ok. 0 passed; 0 failed; 0 ignored",
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": serde_json::Value::Null,
            "cwd": "crates/webcodex",
            "command_summary": "cargo test --lib",
        }),
    );
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let validation = validation_summary_from_events(&summary.events, 20);
    let delta = validation_delta_value(&validation);
    // The scope is comparable, so comparison status is available, but the
    // failure identity diff must be unavailable (no false "resolved").
    assert_eq!(delta["comparison"]["status"], "available");
    assert_eq!(delta["failures"]["identity_status"], "unavailable");
    assert_eq!(
        delta["failures"]["identity_reason_code"],
        "test_identity_unavailable"
    );
    assert!(
        delta["failures"]["resolved"].as_array().unwrap().is_empty(),
        "zero-test run must not falsely resolve prior failures"
    );
}

// =========================================================================
// Bounded output
// =========================================================================

#[test]
fn changed_paths_are_deduped_bounded_and_report_total_and_truncated() {
    // Real finished write events only. `closeout_work_projection` reads
    // changed paths from `tool_call_finished` events, dedups them into a
    // BTreeSet (deterministic sorted order), and `continuation_feedback`
    // further truncates to MAX_CHANGED_PATHS (100) with truncated=true when
    // the deduped total exceeds it. total_changed_paths is the real deduped
    // count (kept <= 200 so the work projection's own cap does not distort it).
    let runtime = test_runtime();

    // Over the limit: 150 distinct paths + duplicates, all finished.
    // Each write emits a started+finished pair (2 events), so batch 2 paths
    // per write to keep the total event count inside the 200-event retained
    // window; otherwise the oldest writes would be evicted and the deduped
    // total would silently shrink. The ledger extraction (changed_paths_for_tool)
    // reads `changes[].path` from each finished event — the same path the
    // production closeout_work_projection reads.
    let session = create_session(&runtime, "many paths");
    add_instruction(&runtime, &session, "edit many", SessionMode::Normal);
    let distinct: Vec<String> = (0..150).map(|i| format!("src/file_{i}.rs")).collect();
    // 75 writes, each carrying 2 distinct paths; every 3rd write repeats an
    // earlier path so the projection must collapse duplicates to one entry.
    for i in 0..75 {
        let p0 = distinct[i * 2].as_str();
        let p1 = distinct[i * 2 + 1].as_str();
        let batch: Vec<&str> = if i > 0 && i % 3 == 0 {
            // repeat an earlier path alongside the two new ones
            vec![p0, p1, distinct[(i - 1) * 2].as_str()]
        } else {
            vec![p0, p1]
        };
        record_write(&runtime, &session, &batch);
    }
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    assert!(summary.events.len() <= 200);
    let feedback = feedback_for(&runtime, &summary, "continued");
    let changes = &feedback["attempt"]["changes"];
    let total = changes["total_changed_paths"].as_u64().unwrap();
    let truncated = changes["truncated"].as_bool().unwrap();
    let returned: Vec<String> = changes["changed_paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(total, 150, "total is the real deduped count");
    assert!(truncated, "over the limit => truncated=true");
    assert_eq!(returned.len(), 100, "capped at MAX_CHANGED_PATHS");
    // Deterministic sorted order (BTreeSet): returned prefix is sorted.
    let mut sorted_distinct = distinct.clone();
    sorted_distinct.sort();
    assert_eq!(returned, sorted_distinct[..100].to_vec());
    // No duplicate appears twice in the returned list.
    let mut seen = std::collections::HashSet::new();
    for path in &returned {
        assert!(
            seen.insert(path.clone()),
            "duplicate path in output: {path}"
        );
    }

    // Under the limit: nothing truncated, all returned.
    let session_under = create_session(&runtime, "under limit");
    add_instruction(&runtime, &session_under, "few edits", SessionMode::Normal);
    for path in &["src/a.rs", "src/b.rs", "src/a.rs"] {
        record_write(&runtime, &session_under, std::slice::from_ref(path));
    }
    let summary = runtime.sessions.summary(&session_under, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let changes = &feedback["attempt"]["changes"];
    assert_eq!(changes["total_changed_paths"].as_u64().unwrap(), 2);
    assert!(!changes["truncated"].as_bool().unwrap());
    assert_eq!(changes["changed_paths"].as_array().unwrap().len(), 2);

    // Exactly at the limit: not truncated, len == MAX. 50 writes x 2 paths
    // = 100 distinct paths, ~102 events, well inside the window.
    let session_at = create_session(&runtime, "at limit");
    add_instruction(&runtime, &session_at, "exactly limit", SessionMode::Normal);
    for i in 0..50 {
        let p0 = format!("src/exact_{i}a.rs");
        let p1 = format!("src/exact_{i}b.rs");
        record_write(&runtime, &session_at, &[p0.as_str(), p1.as_str()]);
    }
    let summary = runtime.sessions.summary(&session_at, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let changes = &feedback["attempt"]["changes"];
    assert_eq!(changes["total_changed_paths"].as_u64().unwrap(), 100);
    assert!(
        !changes["truncated"].as_bool().unwrap(),
        "exactly at the limit is not truncated"
    );
    assert_eq!(changes["changed_paths"].as_array().unwrap().len(), 100);
}

#[test]
fn continuation_feedback_output_is_bounded_in_size() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "bounded size");
    add_instruction(&runtime, &session, "bounded", SessionMode::Normal);
    for _ in 0..40 {
        record_validation_event(
            &runtime,
            &session,
            "cargo_test",
            false,
            test_output(0, 1, 0, &["tests::x"]),
        );
    }
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let serialized = serde_json::to_string(&feedback).unwrap();
    // The projection stays bounded even with many validation events.
    assert!(
        serialized.len() < 32_000,
        "continuation feedback must stay bounded: {} bytes",
        serialized.len()
    );
    // suggested_next_actions is bounded.
    assert!(
        feedback["attempt"]["suggested_next_actions"]
            .as_array()
            .map(|a| a.len() <= 8)
            .unwrap_or(true),
        "suggested_next_actions bounded"
    );
}

// =========================================================================
// Message board
// =========================================================================

#[test]
fn guidance_counts_open_messages_without_changing_status() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "guidance board");
    add_instruction(&runtime, &session, "with guidance", SessionMode::Normal);
    post_session_message(&runtime, &session, "guidance", "review the new boundary");
    post_session_message(&runtime, &session, "risk", "risk A");
    post_session_message(&runtime, &session, "todo", "todo A");

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let discussion = runtime
        .sessions
        .discussion_summary(&session, Some(20))
        .unwrap();
    let feedback = feedback_for_with_discussion(&runtime, &summary, &discussion, "continued");
    assert_eq!(feedback["attempt"]["guidance"]["open_count"], 1);
    assert_eq!(feedback["attempt"]["guidance"]["open_risk_count"], 1);
    assert_eq!(feedback["attempt"]["guidance"]["open_todo_count"], 1);
    assert_eq!(feedback["attempt"]["guidance"]["latest_open_kind"], "todo");

    // Reading the projection must not resolve or mark-read anything.
    let after = runtime
        .sessions
        .discussion_summary(&session, Some(20))
        .unwrap();
    assert_eq!(after.counts.guidance, 1);
    assert_eq!(after.counts.risk, 1);
    assert_eq!(after.counts.todo, 1);
}

#[test]
fn resolved_guidance_is_not_counted_as_open() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "resolved guidance");
    add_instruction(&runtime, &session, "resolve", SessionMode::Normal);
    post_session_message(&runtime, &session, "guidance", "resolved guidance");
    // Resolve the only guidance message.
    let listed = runtime
        .sessions
        .list_messages(
            &session,
            sessions::ListSessionMessagesFilter {
                kind: Some(sessions::SessionMessageKind::Guidance),
                status: None,
                message_id: None,
                reply_to: None,
                limit: None,
            },
        )
        .unwrap();
    let message_id = listed.first().unwrap().message_id.clone();
    runtime
        .sessions
        .resolve_message(&session, &message_id, Some("done".to_string()))
        .unwrap();

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let discussion = runtime
        .sessions
        .discussion_summary(&session, Some(20))
        .unwrap();
    let feedback = feedback_for_with_discussion(&runtime, &summary, &discussion, "continued");
    assert_eq!(feedback["attempt"]["guidance"]["open_count"], 0);
}

// =========================================================================
// Jobs
// =========================================================================

#[test]
fn jobs_block_reports_recovering_not_healthy_running() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "jobs recovering");
    add_instruction(&runtime, &session, "jobs", SessionMode::Normal);
    let jobs = json!({
        "active_count": 1,
        "running_count": 1,
        "recovering_count": 1,
        "terminal_pending_count": 0,
        "truncated": false,
        "recent": [{"job_id": "j1", "status": "recovering"}],
    });
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for_with_jobs(&runtime, &summary, &jobs, "continued");
    assert_eq!(feedback["attempt"]["jobs"]["recovering_count"], 1);
    assert_eq!(feedback["attempt"]["jobs"]["recovery_state"], "recovering");
    // recovering must not masquerade as healthy running.
    assert_ne!(feedback["attempt"]["jobs"]["recovery_state"], "healthy");
    assert_ne!(feedback["attempt"]["jobs"]["recovery_state"], "active");
}

#[test]
fn jobs_block_terminal_pending_does_not_become_healthy_running() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "jobs terminal pending");
    add_instruction(&runtime, &session, "jobs", SessionMode::Normal);
    let jobs = json!({
        "active_count": 1,
        "running_count": 0,
        "recovering_count": 0,
        "terminal_pending_count": 1,
        "truncated": false,
        "recent": [{"job_id": "j1", "status": "stop_requested"}],
    });
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for_with_jobs(&runtime, &summary, &jobs, "continued");
    assert_eq!(feedback["attempt"]["jobs"]["terminal_pending_count"], 1);
    assert_eq!(
        feedback["attempt"]["jobs"]["recovery_state"],
        "terminal_pending"
    );
}

#[test]
fn jobs_block_none_when_no_active_jobs() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "no jobs");
    add_instruction(&runtime, &session, "jobs", SessionMode::Normal);
    let jobs = json!({
        "active_count": 0,
        "running_count": 0,
        "recovering_count": 0,
        "terminal_pending_count": 0,
        "truncated": false,
        "recent": [],
    });
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for_with_jobs(&runtime, &summary, &jobs, "continued");
    assert_eq!(feedback["attempt"]["jobs"]["recovery_state"], "none");
    assert_eq!(
        feedback["attempt"]["jobs"]["latest_job_status"],
        "not_observed"
    );
}

#[test]
fn jobs_block_recovering_hidden_beyond_truncated_recent_still_reported() {
    // Regression: counts come from the aggregate, not the truncated `recent`
    // list. A recovering job hidden beyond the recent window must still drive
    // recovery_state = recovering (never masquerade as healthy/active).
    let runtime = test_runtime();
    let session = create_session(&runtime, "jobs truncated recent");
    add_instruction(&runtime, &session, "jobs", SessionMode::Normal);
    let jobs = json!({
        "active_count": 5,
        "running_count": 5,
        "recovering_count": 1,
        "terminal_pending_count": 0,
        "truncated": true,
        "recent": [{"job_id": "j1", "status": "running"}, {"job_id": "j2", "status": "running"}],
    });
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for_with_jobs(&runtime, &summary, &jobs, "continued");
    assert_eq!(feedback["attempt"]["jobs"]["recovering_count"], 1);
    assert_eq!(feedback["attempt"]["jobs"]["recent_truncated"], true);
    assert_eq!(feedback["attempt"]["jobs"]["recovery_state"], "recovering");
}

// =========================================================================
// Real entry integration: start_coding_task pre-instruction attempt
// =========================================================================

use crate::tool_runtime::tests::reconnect::dispatch_start_coding_task_in_window;
use crate::tool_runtime::{StartupDetail, ToolCall};

fn coding_call(project: &str, instruction: &str, resume: Option<&str>) -> ToolCall {
    ToolCall::StartCodingTask {
        project: project.to_string(),
        client_id: None,
        path: None,
        temporary_project_name: None,
        title: Some(instruction.to_string()),
        mode: SessionMode::Normal,
        deny_write_tools: false,
        deny_shell_tools: false,
        // These integration assertions exercise the complete underlying
        // continuation_feedback block retained by full diagnostics. Standard
        // uses the separately tested bounded model-facing projection.
        detail: StartupDetail::Full,
        resume_session_id: resume.map(str::to_string),
        execution_context: None,
    }
}

#[tokio::test]
async fn start_coding_task_continuation_describes_previous_attempt_not_empty_new_one() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "continuation-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);

    // First start_coding_task creates the session with instruction A.
    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "continuation-agent",
        coding_call(&project, "instruction A", None),
        Some(&auth),
        "continuation-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    let session_id = first.output["session"]["session_id"]
        .as_str()
        .unwrap()
        .to_string();

    // Record real work under attempt A: a write tool call and a validation run.
    record_write(&runtime, &session_id, &["src/lib.rs"]);
    record_validation_event(
        &runtime,
        &session_id,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );

    // Capture event count *after* A's work, before the continuation call.
    let events_before_continuation = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();

    // Second start_coding_task continues with instruction B on the same session.
    let second = dispatch_start_coding_task_in_window(
        &runtime,
        "continuation-agent",
        coding_call(&project, "instruction B", Some(&session_id)),
        Some(&auth),
        "continuation-window",
    )
    .await;
    assert!(second.success, "{:?}", second.error);
    assert_eq!(
        second.output["session"]["continuation"],
        "resumed_explicitly"
    );

    let feedback = &second.output["continuation_feedback"];
    // The feedback must describe A's attempt (real work), not the empty new B attempt.
    assert_eq!(feedback["status"], "available");
    assert_eq!(
        feedback["attempt"]["boundary"]["source"],
        "task_instruction"
    );
    assert_eq!(feedback["attempt"]["instruction"]["status"], "available");
    assert_eq!(
        feedback["attempt"]["instruction"]["excerpt"],
        "instruction A"
    );
    // Instruction B must be appended exactly once.
    let summary = runtime.sessions.summary(&session_id, Some(200)).unwrap();
    let instructions: Vec<&str> = summary
        .events
        .iter()
        .filter(|event| event.kind == "task_instruction")
        .map(|event| event.instruction.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(
        instructions
            .iter()
            .filter(|i| **i == "instruction B")
            .count(),
        1,
        "instruction B appended exactly once"
    );
    assert_eq!(
        instructions
            .iter()
            .filter(|i| **i == "instruction A")
            .count(),
        1,
        "instruction A still present exactly once"
    );

    // Reading the continuation feedback must not append any further events
    // beyond the one legitimate instruction append for B itself.
    let events_after = summary.events.len();
    // B's continuation appended exactly one task_instruction event.
    assert_eq!(
        events_after,
        events_before_continuation + 1,
        "continuation read appended only the B instruction"
    );
}

#[tokio::test]
async fn start_coding_task_fresh_session_continuation_is_not_applicable() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project = register_agent_project_at_path(&runtime, "fresh-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);

    let first = dispatch_start_coding_task_in_window(
        &runtime,
        "fresh-agent",
        coding_call(&project, "fresh start", None),
        Some(&auth),
        "fresh-window",
    )
    .await;
    assert!(first.success, "{:?}", first.error);
    assert_eq!(first.output["session"]["continuation"], "created");
    let feedback = &first.output["continuation_feedback"];
    assert_eq!(feedback["status"], "not_applicable");
    assert_eq!(feedback["reason_code"], "fresh_session");
}

// =========================================================================
// Real entry integration: session_handoff_summary
// =========================================================================

fn handoff_call(session_id: &str) -> ToolCall {
    ToolCall::SessionHandoffSummary {
        session_id: session_id.to_string(),
        project: None,
        include_workspace: None,
        include_checkpoints: None,
        include_validation: None,
        summary_only: false,
        limit: None,
    }
}

#[tokio::test]
async fn handoff_default_limit_does_not_shrink_continuation_attempt_boundary() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "handoff display limit");
    add_instruction(&runtime, &session, "instruction A", SessionMode::Normal);
    // Produce 30+ events so the default display limit (20) cannot see instruction A.
    for i in 0..35 {
        record_write(&runtime, &session, &[&format!("src/file_{i}.rs")]);
    }

    // Call the real session_handoff_summary with the default display limit.
    let result = runtime.dispatch(handoff_call(&session)).await;
    assert!(result.success, "{:?}", result.error);
    // The display `events` count is bounded by the caller limit.
    let display_events = result.output["counts"]["events"].as_u64().unwrap_or(0);
    assert!(
        display_events <= 20,
        "display events bounded by default limit, got {display_events}"
    );
    // The continuation feedback still locates instruction A as the attempt
    // boundary (it uses an independent 200-event evidence snapshot).
    let feedback = &result.output["continuation_feedback"];
    assert_eq!(feedback["status"], "available");
    assert_eq!(
        feedback["attempt"]["boundary"]["source"],
        "task_instruction"
    );
    assert_eq!(feedback["attempt"]["activity"]["meaningful_tool_calls"], 35);
}

#[tokio::test]
async fn handoff_summary_only_and_include_validation_false_shape() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "handoff summary only");
    add_instruction(&runtime, &session, "instruction A", SessionMode::Normal);
    record_write(&runtime, &session, &["src/lib.rs"]);
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
    );

    // summary_only=true, include_validation=false.
    let result = runtime
        .dispatch(ToolCall::SessionHandoffSummary {
            session_id: session.clone(),
            project: None,
            include_workspace: None,
            include_checkpoints: None,
            include_validation: Some(false),
            summary_only: true,
            limit: None,
        })
        .await;
    assert!(result.success, "{:?}", result.error);
    // summary_only must not expose raw command/output fields.
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(
        !serialized.contains("command_summary") && !serialized.contains("stdout_tail"),
        "summary_only leaked raw validation/command fields"
    );
    let feedback = &result.output["continuation_feedback"];
    assert_eq!(feedback["status"], "available");
    // Validation in the feedback is explicitly unavailable, not `not_run`.
    assert_eq!(
        feedback["attempt"]["validation"]["latest_status"],
        "unavailable"
    );
    assert_eq!(
        feedback["attempt"]["validation"]["delta_reason_code"],
        "validation_not_requested"
    );
}

#[tokio::test]
async fn handoff_guidance_read_does_not_resolve_messages() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "handoff guidance read");
    add_instruction(&runtime, &session, "with guidance", SessionMode::Normal);
    post_session_message(&runtime, &session, "guidance", "review the boundary");
    post_session_message(&runtime, &session, "todo", "finish tests");

    let result = runtime.dispatch(handoff_call(&session)).await;
    assert!(result.success, "{:?}", result.error);
    let feedback = &result.output["continuation_feedback"];
    assert_eq!(feedback["attempt"]["guidance"]["open_count"], 1);
    assert_eq!(feedback["attempt"]["guidance"]["open_todo_count"], 1);

    // Reading guidance via handoff must not resolve or change counts.
    let after = runtime
        .sessions
        .discussion_summary(&session, Some(20))
        .unwrap();
    assert_eq!(after.counts.guidance, 1);
    assert_eq!(after.counts.todo, 1);
}

#[test]
fn continuation_feedback_does_not_leak_raw_output_or_commands() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "no leak");
    add_instruction(&runtime, &session, "secret command", SessionMode::Normal);
    record_validation_event(
        &runtime,
        &session,
        "cargo_test",
        false,
        json!({
            "exit_code": 101,
            "stdout_tail": "SENSITIVE_TOKEN=abc123",
            "stderr_tail": "tests::a ... FAILED",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": "validation_failed",
            "cwd": "crates/webcodex",
            "command_summary": "cargo test --lib",
        }),
    );

    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "continued");
    let serialized = serde_json::to_string(&feedback).unwrap();
    assert!(
        !serialized.contains("SENSITIVE_TOKEN"),
        "continuation feedback leaked a sensitive token"
    );
    assert!(
        !serialized.contains("stdout_tail") && !serialized.contains("stderr_tail"),
        "continuation feedback leaked raw output field names"
    );
    assert!(
        !serialized.contains("command_summary"),
        "continuation feedback leaked command text"
    );
}

#[test]
fn fresh_session_reports_not_applicable() {
    let runtime = test_runtime();
    let session = create_session(&runtime, "fresh");
    let summary = runtime.sessions.summary(&session, Some(200)).unwrap();
    let feedback = feedback_for(&runtime, &summary, "created");
    assert_eq!(feedback["status"], "not_applicable");
    assert_eq!(feedback["reason_code"], "fresh_session");
    assert_eq!(
        feedback["attempt"]["exploration"]["observed_paths"],
        json!([])
    );
    assert_eq!(
        feedback["attempt"]["exploration"]["latest_tool"],
        Value::Null
    );
}

// =========================================================================
// Real entry integration: validation_summary
// =========================================================================

#[tokio::test]
async fn validation_summary_surfaces_validation_delta_without_shell_or_new_events() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "vsummary-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);

    // Create a real session scoped to the registered project, then record
    // prior + current validation evidence directly in the ledger (no shell,
    // no agent request).
    let session = runtime
        .sessions
        .start_session_with_options(sessions::SessionCreateOptions::new(
            Some(project.clone()),
            Some("validation delta".to_string()),
            SessionMode::Normal,
            SessionGuards::default(),
        ))
        .unwrap()
        .session_id;
    // Use a Normal-mode instruction so the attempt boundary is task_instruction.
    add_instruction_for(
        &runtime,
        &session,
        "fix tests",
        SessionMode::Normal,
        &project,
    );
    // Prior comparable run: 2 passed, 0 failed.
    record_validation_event_for(
        &runtime,
        &session,
        "cargo_test",
        true,
        test_output(2, 0, 0, &[]),
        &project,
    );
    // Current run: 0 passed, 1 failed. passed_delta must be negative (-2).
    record_validation_event_for(
        &runtime,
        &session,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
        &project,
    );

    let events_before = runtime
        .sessions
        .summary(&session, Some(200))
        .unwrap()
        .events
        .len();

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ValidationSummary {
                project: project.clone(),
                session_id: session.clone(),
                limit: None,
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);

    // The real tool output exposes validation_delta at the top level.
    let delta = &result.output["validation_delta"];
    assert_eq!(delta["comparison"]["status"], "available");
    assert_eq!(
        delta["counts"]["passed_delta"].as_i64(),
        Some(-2),
        "negative passed_delta"
    );
    assert_eq!(delta["counts"]["failed_delta"].as_i64(), Some(1));

    // The delta itself never fabricates a new verdict and never leaks raw
    // command text or raw output. (The validation *summary* block may carry a
    // derived command_summary; the delta projection must not.)
    let delta_serialized = serde_json::to_string(delta).unwrap();
    assert!(
        !delta_serialized.contains("cargo test --lib"),
        "validation_delta leaked command text"
    );
    assert!(
        !delta_serialized.contains("stdout_tail") && !delta_serialized.contains("stderr_tail"),
        "validation_delta leaked raw output"
    );

    // Reading validation_summary must not append ledger events and must not
    // enqueue an agent/runner request.
    let events_after = runtime
        .sessions
        .summary(&session, Some(200))
        .unwrap()
        .events
        .len();
    assert_eq!(
        events_after, events_before,
        "validation_summary appended events"
    );
    assert!(
        probe_patch_agent_request(&runtime, "vsummary-agent")
            .await
            .is_none(),
        "validation_summary enqueued an agent request"
    );
}

// =========================================================================
// Real entry integration: finish_coding_task
// =========================================================================

#[tokio::test]
async fn finish_coding_task_continuation_matches_handoff_attempt_without_rerunning_validation() {
    let dir = tempfile::tempdir().unwrap();
    init_git_repo(dir.path());
    let runtime = ToolRuntime::new_for_tests();
    let project =
        register_agent_project_at_path(&runtime, "finish-agent", "demo", dir.path()).await;
    let auth = auth_context(None, true);
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("finish continuation".to_string()),
    );
    let session_id = session.session_id.clone();
    add_instruction_for(
        &runtime,
        &session_id,
        "instruction A",
        SessionMode::Normal,
        &project,
    );
    record_write_for(&runtime, &session_id, &project, &["src/lib.rs"]);
    record_validation_event_for(
        &runtime,
        &session_id,
        "cargo_test",
        false,
        test_output(0, 1, 0, &["tests::a"]),
        &project,
    );

    let handoff = runtime
        .session_handoff_summary(
            session_id.clone(),
            Some(project.clone()),
            Some(false),
            Some(false),
            Some(false),
            false,
            Some(20),
            Some(&auth),
        )
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    let handoff_feedback = handoff.output["continuation_feedback"].clone();
    assert_eq!(handoff_feedback["status"], "available");

    let events_before_finish = runtime
        .sessions
        .summary(&session_id, Some(200))
        .unwrap()
        .events
        .len();
    let finish = runtime
        .dispatch_with_auth(
            ToolCall::FinishCodingTask {
                project: project.clone(),
                session_id: session_id.clone(),
                summary_only: false,
                include_diff: Some(false),
                include_workspace: Some(false),
                include_hygiene: Some(false),
                include_handoff: Some(false),
                include_validation_summary: Some(false),
            },
            Some(&auth),
        )
        .await;
    assert!(finish.success, "{:?}", finish.error);
    let finish_feedback = &finish.output["continuation_feedback"];

    for pointer in [
        "/status",
        "/attempt/boundary",
        "/attempt/instruction",
        "/attempt/validation/latest_status",
        "/attempt/changes/total_changed_paths",
    ] {
        assert_eq!(
            finish_feedback.pointer(pointer),
            handoff_feedback.pointer(pointer),
            "finish and handoff must project the same established attempt at {pointer}"
        );
    }

    let summary = runtime.sessions.summary(&session_id, Some(200)).unwrap();
    assert!(summary.events.len() >= events_before_finish);
    assert_eq!(
        summary
            .events
            .iter()
            .filter(|event| event.kind == "tool_call_finished" && event.tool_name == "cargo_test")
            .count(),
        1,
        "finish_coding_task must not re-run validation"
    );
    assert!(
        probe_patch_agent_request(&runtime, "finish-agent")
            .await
            .is_none(),
        "finish_coding_task enqueued an agent request"
    );
}

// =========================================================================
// Meaningful-tool-call classification is consistent with tool policy
// =========================================================================

#[test]
fn meaningful_tool_classification_excludes_status_and_manifest_queries() {
    // Pure status/manifest/summary queries must never count as work progress.
    for name in [
        "runtime_status",
        "tool_manifest",
        "list_tools",
        "session_summary",
    ] {
        assert!(
            !is_meaningful(name),
            "{name} must not count as a meaningful tool call"
        );
    }
    // Write / shell / git / validation tools do count.
    for name in ["apply_text_edits", "run_shell", "git_diff", "cargo_test"] {
        assert!(is_meaningful(name), "{name} should count as meaningful");
    }
}

fn is_meaningful(name: &str) -> bool {
    runtime_tool_is_write_like(name)
        || runtime_tool_is_shell_like(name)
        || runtime_tool_is_git_like(name)
        || runtime_tool_captures_validation_output(name)
}

// =========================================================================
// Helpers
// =========================================================================

fn create_session(runtime: &ToolRuntime, title: &str) -> String {
    runtime
        .sessions
        .start_session(Some("test-project".to_string()), Some(title.to_string()))
        .session_id
}

fn add_instruction(runtime: &ToolRuntime, session_id: &str, instruction: &str, mode: SessionMode) {
    runtime
        .sessions
        .ensure_coding_session(sessions::CodingSessionRequest {
            project: "test-project".to_string(),
            authority_fingerprint: sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                .to_string(),
            resume_session_id: Some(session_id.to_string()),
            instruction: Some(instruction.to_string()),
            mode,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap();
}

fn record_validation_event(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    success: bool,
    output: Value,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &json!({"project": "test-project"}),
    );
    runtime.sessions.record_tool_call_finished(
        start,
        success,
        &output,
        (!success).then_some("validation failed"),
        None,
    );
}

/// Record a finished write tool call (`apply_text_edits`) whose `changes` carry
/// the given changed paths. The ledger records the deduped changed paths from
/// the start event and surfaces them on the finished event, the same extraction
/// the production `closeout_work_projection` reads.
fn record_write(runtime: &ToolRuntime, session_id: &str, paths: &[&str]) {
    record_write_for(runtime, session_id, "test-project", paths);
}

fn record_write_for(runtime: &ToolRuntime, session_id: &str, project: &str, paths: &[&str]) {
    let changes: Vec<Value> = paths
        .iter()
        .map(|path| json!({"kind": "edit", "path": path}))
        .collect();
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        "apply_text_edits",
        &json!({
            "project": project,
            "changes": changes,
        }),
    );
    runtime
        .sessions
        .record_tool_call_finished(start, true, &json!({"applied": true}), None, None);
}

fn record_exploration_event(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    arguments: Value,
    success: bool,
    output: Value,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &arguments,
    );
    runtime.sessions.record_tool_call_finished(
        start,
        success,
        &output,
        (!success).then_some("exploration failed"),
        None,
    );
}

fn locations_output(path: &str, locations: &[&str]) -> Value {
    json!({
        "project": "test-project",
        "path": path,
        "query_position": {"line": 3, "column": 5},
        "locations": locations
            .iter()
            .enumerate()
            .map(|(index, path)| json!({
                "path": path,
                "range": {
                    "start": {"line": index + 1, "column": 1},
                    "end": {"line": index + 1, "column": 2}
                }
            }))
            .collect::<Vec<_>>(),
        "total_results": locations.len(),
        "returned_count": locations.len(),
        "truncated": false,
        "external_results_omitted": 0,
        "invalid_results_omitted": 0
    })
}

/// Project-scoped variants of the helpers above for real entry tests that
/// dispatch tools against a registered agent project (e.g. validation_summary).
fn add_instruction_for(
    runtime: &ToolRuntime,
    session_id: &str,
    instruction: &str,
    mode: SessionMode,
    project: &str,
) {
    runtime
        .sessions
        .ensure_coding_session(sessions::CodingSessionRequest {
            project: project.to_string(),
            authority_fingerprint: sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                .to_string(),
            resume_session_id: Some(session_id.to_string()),
            instruction: Some(instruction.to_string()),
            mode,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap();
}

fn record_validation_event_for(
    runtime: &ToolRuntime,
    session_id: &str,
    tool_name: &str,
    success: bool,
    output: Value,
    project: &str,
) {
    let start = runtime.sessions.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        tool_name,
        &json!({"project": project}),
    );
    runtime.sessions.record_tool_call_finished(
        start,
        success,
        &output,
        (!success).then_some("validation failed"),
        None,
    );
}

fn post_session_message(runtime: &ToolRuntime, session_id: &str, kind: &str, message: &str) {
    use crate::tool_runtime::sessions::{
        PostSessionMessageInput, SessionMessageKind, SessionMessagePriority,
    };
    let kind = match kind {
        "note" => SessionMessageKind::Note,
        "proposal" => SessionMessageKind::Proposal,
        "question" => SessionMessageKind::Question,
        "answer" => SessionMessageKind::Answer,
        "decision" => SessionMessageKind::Decision,
        "risk" => SessionMessageKind::Risk,
        "progress" => SessionMessageKind::Progress,
        "guidance" => SessionMessageKind::Guidance,
        "todo" => SessionMessageKind::Todo,
        _ => panic!("unknown message kind: {kind}"),
    };
    runtime
        .sessions
        .post_message(PostSessionMessageInput {
            session_id: session_id.to_string(),
            kind,
            message: message.to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();
}

fn check_output(errors: usize, passed: usize) -> Value {
    let mut stderr = String::new();
    for i in 0..errors {
        stderr.push_str(&format!("error[E0308]: message {i}\n --> src/f{i}:1:1\n"));
    }
    let stdout = if passed > 0 {
        format!("test result: ok. {passed} passed; 0 failed")
    } else {
        String::new()
    };
    json!({
        "exit_code": if errors > 0 { 101 } else { 0 },
        "stdout_tail": stdout,
        "stderr_tail": stderr,
        "stdout_truncated": false,
        "stderr_truncated": false,
        "failure_kind": if errors > 0 { json!("validation_failed") } else { Value::Null },
        "cwd": "crates/webcodex",
        "command_summary": "cargo check",
    })
}

/// Build a cargo `test` validation output. The parser combines stdout and
/// stderr line-by-line, so the single `test result:` summary line and the
/// `test <name> ... FAILED` lines are placed in stdout only to avoid
/// double-counting. `failed_names` are the bare test module paths (e.g.
/// `tests::a`); they are rendered as `test <name> ... FAILED` so the existing
/// parser captures stable failure identities.
fn test_output(passed: u64, failed: u64, ignored: u64, failed_names: &[&str]) -> Value {
    let mut stdout = String::new();
    for name in failed_names {
        stdout.push_str(&format!("test {name} ... FAILED\n"));
    }
    if failed == 0 {
        stdout.push_str(&format!(
            "test result: ok. {passed} passed; 0 failed; {ignored} ignored"
        ));
    } else {
        stdout.push_str(&format!(
            "test result: FAILED. {passed} passed; {failed} failed; {ignored} ignored"
        ));
    }
    json!({
        "exit_code": if failed > 0 { 101 } else { 0 },
        "stdout_tail": stdout,
        "stderr_tail": "",
        "stdout_truncated": false,
        "stderr_truncated": false,
        "failure_kind": if failed > 0 { json!("test_failure") } else { Value::Null },
        "cwd": "crates/webcodex",
        "command_summary": "cargo test --lib",
    })
}

fn empty_discussion() -> sessions::SessionDiscussionSummary {
    sessions::SessionDiscussionSummary {
        counts: SessionDiscussionCounts {
            total: 0,
            open: 0,
            resolved: 0,
            guidance: 0,
            progress: 0,
            risk: 0,
            todo: 0,
            question: 0,
            answer: 0,
            decision: 0,
            open_guidance: 0,
            open_questions: 0,
            open_risks: 0,
            open_todos: 0,
        },
        open_guidance: Vec::new(),
        open_questions: Vec::new(),
        open_risks: Vec::new(),
        open_todos: Vec::new(),
        high_priority_open_todos: Vec::new(),
        recent_answers: Vec::new(),
        recent_completions: Vec::new(),
        recent_progress: Vec::new(),
        recent_decisions: Vec::new(),
    }
}

fn feedback_for(
    runtime: &ToolRuntime,
    summary: &sessions::SessionSummary,
    continuation: &'static str,
) -> Value {
    let discussion = runtime
        .sessions
        .discussion_summary(&summary.session_id, Some(20))
        .unwrap_or_else(|_| empty_discussion());
    feedback_for_with_discussion(runtime, summary, &discussion, continuation)
}

fn feedback_for_with_discussion(
    _runtime: &ToolRuntime,
    summary: &sessions::SessionSummary,
    discussion: &sessions::SessionDiscussionSummary,
    continuation: &'static str,
) -> Value {
    let validation = validation_summary_from_events(&summary.events, 20);
    let jobs = json!({"active_count": 0, "terminal_pending_count": 0, "recent": []});
    continuation_feedback_value(ContinuationFeedbackInput {
        session_summary: summary,
        validation: &validation,
        jobs: &jobs,
        discussion,
        continuation,
        suggest_exploration_continuity: true,
        workspace_conflicts: false,
    })
}

fn feedback_for_with_jobs(
    _runtime: &ToolRuntime,
    summary: &sessions::SessionSummary,
    jobs: &Value,
    continuation: &'static str,
) -> Value {
    let validation = validation_summary_from_events(&summary.events, 20);
    let discussion = empty_discussion();
    continuation_feedback_value(ContinuationFeedbackInput {
        session_summary: summary,
        validation: &validation,
        jobs,
        discussion: &discussion,
        continuation,
        suggest_exploration_continuity: true,
        workspace_conflicts: false,
    })
}
