//! Contract tests for the compact deterministic `handoff_brief` projection.

use super::support::*;
use crate::tool_runtime::continuation_feedback::{
    continuation_feedback_value, continuation_projection_hooks, continuation_validation_snapshot,
    ContinuationFeedbackInput,
};
use crate::tool_runtime::handoff_brief::{
    build_handoff_brief, handoff_brief_size, HandoffBriefInput, HANDOFF_BRIEF_HARD_MAX_BYTES,
    HANDOFF_CHANGED_PATHS_MAX_ITEMS, HANDOFF_INSTRUCTION_MAX_CHARS, HANDOFF_NEXT_ACTIONS_MAX_ITEMS,
    HANDOFF_OPEN_FAILURES_MAX_ITEMS, HANDOFF_RECENT_FILES_MAX_ITEMS,
};
use crate::tool_runtime::sessions::{
    CodingSessionRequest, PostSessionMessageInput, SessionDiscussionSummary, SessionGuards,
    SessionMessageKind, SessionMessagePriority, SessionStore, SessionTransport,
};
use crate::tool_runtime::startup_brief::validate_schema_instance_for_test;
use crate::tool_runtime::validation_events::{
    current_validation_evidence_for_session, validation_summary_from_events,
};
use crate::tool_runtime::{registered_tool_specs, SessionMode, ToolCall, ToolRuntime};
use serde_json::{json, Value};

const PROJECT: &str = "test-project";

fn store_with_limit(max_events: usize) -> SessionStore {
    SessionStore::new(16, max_events)
}

fn start_session(store: &SessionStore, title: &str) -> String {
    store
        .start_session(Some(PROJECT.to_string()), Some(title.to_string()))
        .session_id
}

fn add_instruction(store: &SessionStore, session_id: &str, instruction: &str) {
    add_instruction_for_project(store, session_id, PROJECT, instruction);
}

fn add_instruction_for_project(
    store: &SessionStore,
    session_id: &str,
    project: &str,
    instruction: &str,
) {
    store
        .ensure_coding_session(CodingSessionRequest {
            project: project.to_string(),
            authority_fingerprint:
                crate::tool_runtime::sessions::TEST_ONLY_PROJECT_SESSION_AUTHORITY_FINGERPRINT
                    .to_string(),
            resume_session_id: Some(session_id.to_string()),
            instruction: Some(instruction.to_string()),
            mode: SessionMode::Normal,
            guards: SessionGuards::default(),
            execution_context: None,
            project_instructions: None,
            transport: SessionTransport::Api,
            context_refreshed: true,
            write_scope_verified: true,
        })
        .unwrap();
}

fn jobs_summary(running: u64, recovering: u64, terminal_pending: u64) -> Value {
    json!({
        "active_count": running + terminal_pending,
        "running_count": running,
        "recovering_count": recovering,
        "terminal_pending_count": terminal_pending,
        "blocking_active_count": running,
        "nonblocking_active_count": terminal_pending,
        "recent": [],
        "truncated": false,
    })
}

fn empty_jobs() -> Value {
    jobs_summary(0, 0, 0)
}

fn clean_workspace() -> Value {
    json!({
        "git_available": true,
        "clean": true,
        "branch": "main",
        "head": {
            "commit": "0123456789abcdef0123456789abcdef01234567",
        },
        "counts": {
            "modified": 0,
            "added": 0,
            "deleted": 0,
            "renamed": 0,
            "copied": 0,
            "untracked": 0,
            "conflicted": 0,
        },
    })
}

fn dirty_workspace(conflicted: u64) -> Value {
    json!({
        "git_available": true,
        "clean": false,
        "branch": "main",
        "head": {
            "commit": "0123456789abcdef0123456789abcdef01234567",
        },
        "counts": {
            "modified": 1,
            "added": 0,
            "deleted": 0,
            "renamed": 0,
            "copied": 0,
            "untracked": 0,
            "conflicted": conflicted,
        },
    })
}

fn passed_validation() -> Value {
    json!({
        "available": true,
        "status": "passed",
        "latest_status": "passed",
        "unresolved_failures": {"count": 0, "events": []},
        "events": [],
    })
}

fn discussion(store: &SessionStore, session_id: &str) -> SessionDiscussionSummary {
    store.discussion_summary(session_id, Some(20)).unwrap()
}

#[allow(clippy::too_many_arguments)]
fn brief_for(
    store: &SessionStore,
    session_id: &str,
    workspace_requested: bool,
    workspace: Option<&Value>,
    validation_requested: bool,
    validation_override: Option<&Value>,
    jobs: Option<&Value>,
    guidance_available: bool,
) -> Value {
    let summary = store.summary(session_id, Some(200)).unwrap();
    let computed_validation = validation_summary_from_events(&summary.events, 20);
    let current_validation = current_validation_evidence_for_session(&summary, 20);
    let validation = validation_override.unwrap_or(&computed_validation);
    let validation_not_requested = json!({
        "available": false,
        "not_requested": true,
    });
    let feedback_validation = if validation_requested {
        validation
    } else {
        &validation_not_requested
    };
    let discussion = discussion(store, session_id);
    let null_jobs = Value::Null;
    let feedback_jobs = jobs.unwrap_or(&null_jobs);
    let continuation = continuation_feedback_value(ContinuationFeedbackInput {
        session_summary: &summary,
        validation: feedback_validation,
        jobs: feedback_jobs,
        discussion: &discussion,
        continuation: "continued",
        suggest_exploration_continuity: false,
        workspace_conflicts: workspace
            .and_then(|value| value.pointer("/counts/conflicted"))
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0,
        hooks: continuation_projection_hooks(),
        current_validation: continuation_validation_snapshot(&current_validation),
    });
    build_handoff_brief(HandoffBriefInput {
        session_summary: &summary,
        continuation_feedback: &continuation,
        workspace_requested,
        workspace,
        validation_requested,
        validation: Some(validation),
        jobs,
        guidance_available,
        existing_suggested_actions: None,
    })
}

fn record_write(store: &SessionStore, session_id: &str, path: &str) {
    let start = store.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        "apply_text_edits",
        &json!({
            "project": PROJECT,
            "changes": [{
                "kind": "edit",
                "path": path,
                "edits": [{
                    "kind": "replace_exact",
                    "old_text": "old",
                    "new_text": "new",
                }],
            }],
        }),
        crate::tool_runtime::sessions::session_tool_contract("apply_text_edits"),
    );
    store.record_tool_call_finished(start, true, &json!({"changed": true}), None, None);
}

fn record_read(store: &SessionStore, session_id: &str, path: &str) {
    let start = store.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        "read_file",
        &json!({
            "project": PROJECT,
            "path": path,
        }),
        crate::tool_runtime::sessions::session_tool_contract("read_file"),
    );
    store.record_tool_call_finished(
        start,
        true,
        &json!({
            "path": path,
            "content": "not retained as exploration evidence",
        }),
        None,
        None,
    );
}

fn record_test_validation(
    store: &SessionStore,
    session_id: &str,
    passed: u64,
    failed_names: &[String],
) {
    let mut stdout = String::new();
    for name in failed_names {
        stdout.push_str(&format!("test {name} ... FAILED\n"));
    }
    if failed_names.is_empty() {
        stdout.push_str(&format!(
            "test result: ok. {passed} passed; 0 failed; 0 ignored"
        ));
    } else {
        stdout.push_str(&format!(
            "test result: FAILED. {passed} passed; {} failed; 0 ignored",
            failed_names.len()
        ));
    }
    let failed = !failed_names.is_empty();
    let start = store.record_tool_call_started(
        Some(session_id),
        SessionTransport::Api,
        "cargo_test",
        &json!({"project": PROJECT}),
        crate::tool_runtime::sessions::session_tool_contract("cargo_test"),
    );
    store.record_tool_call_finished(
        start,
        !failed,
        &json!({
            "exit_code": if failed { 101 } else { 0 },
            "stdout_tail": stdout,
            "stderr_tail": "",
            "stdout_truncated": false,
            "stderr_truncated": false,
            "failure_kind": if failed { json!("test_failure") } else { Value::Null },
            "cwd": ".",
            "command_summary": "cargo test --lib",
        }),
        failed.then_some("validation failed"),
        None,
    );
}

#[test]
fn handoff_brief_fresh_active_session_is_stable_and_ready() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "implement deterministic handoff");
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    let first = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    let second = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );

    assert_eq!(first, second);
    assert_eq!(first["version"], 1);
    assert_eq!(first["session"]["lifecycle"], "active");
    assert_eq!(
        first["task"]["root_instruction"]["excerpt"],
        "implement deterministic handoff"
    );
    assert!(first["task"]["latest_instruction"]["excerpt"].is_null());
    assert_eq!(first["workspace"]["status"], "available");
    assert_eq!(first["validation"]["status"], "not_run");
    assert_eq!(first["progress"]["state"], "ready_to_continue");
    assert_eq!(first["basis"]["complete"], true);
    assert_eq!(first["deterministic"], true);
    assert_eq!(first["llm_summary"], false);
}

#[test]
fn handoff_brief_root_and_latest_instructions_are_distinct_and_bounded() {
    let store = store_with_limit(200);
    let long_root = format!("root \"quoted\" \\\\ {}", "界".repeat(800));
    let session_id = start_session(&store, &long_root);
    let long_latest = format!("latest \"quoted\" \\\\ {}", "新".repeat(800));
    add_instruction(&store, &session_id, &long_latest);
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    let root = brief["task"]["root_instruction"]["excerpt"]
        .as_str()
        .unwrap();
    let latest = brief["task"]["latest_instruction"]["excerpt"]
        .as_str()
        .unwrap();

    assert!(root.starts_with("root \"quoted\""));
    assert!(latest.starts_with("latest \"quoted\""));
    assert_eq!(
        root,
        long_root
            .chars()
            .take(HANDOFF_INSTRUCTION_MAX_CHARS)
            .collect::<String>()
    );
    assert_eq!(
        latest,
        long_latest
            .chars()
            .take(HANDOFF_INSTRUCTION_MAX_CHARS)
            .collect::<String>()
    );
    assert!(root.chars().count() <= HANDOFF_INSTRUCTION_MAX_CHARS);
    assert!(latest.chars().count() <= HANDOFF_INSTRUCTION_MAX_CHARS);
    assert_eq!(brief["task"]["root_instruction"]["truncated"], true);
    assert_eq!(brief["task"]["latest_instruction"]["truncated"], true);

    let equal_id = start_session(&store, "same instruction");
    add_instruction(&store, &equal_id, "same instruction");
    let equal = brief_for(
        &store,
        &equal_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    assert_eq!(
        equal["task"]["root_instruction"],
        equal["task"]["latest_instruction"]
    );
}

#[test]
fn handoff_brief_progress_state_uses_only_proven_blockers() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "state matrix");
    let clean = clean_workspace();
    let dirty = dirty_workspace(0);
    let conflict = dirty_workspace(1);
    let empty = empty_jobs();
    let passed = passed_validation();

    let ready = brief_for(
        &store,
        &session_id,
        true,
        Some(&clean),
        true,
        None,
        Some(&empty),
        true,
    );
    assert_eq!(ready["progress"]["state"], "ready_to_continue");

    let dirty_passed = brief_for(
        &store,
        &session_id,
        true,
        Some(&dirty),
        true,
        Some(&passed),
        Some(&empty),
        true,
    );
    assert_eq!(dirty_passed["workspace"]["dirty"], true);
    assert_eq!(dirty_passed["progress"]["state"], "ready_to_continue");

    let needs_validation = brief_for(
        &store,
        &session_id,
        true,
        Some(&dirty),
        true,
        None,
        Some(&empty),
        true,
    );
    assert_eq!(needs_validation["progress"]["state"], "needs_validation");

    let conflicted = brief_for(
        &store,
        &session_id,
        true,
        Some(&conflict),
        true,
        Some(&passed),
        Some(&empty),
        true,
    );
    assert_eq!(conflicted["progress"]["state"], "blocked");
    assert_eq!(conflicted["attention"]["workspace_conflict"], true);

    let blocking_jobs = jobs_summary(1, 0, 0);
    let blocked = brief_for(
        &store,
        &session_id,
        true,
        Some(&clean),
        true,
        Some(&passed),
        Some(&blocking_jobs),
        true,
    );
    assert_eq!(blocked["progress"]["state"], "blocked");

    let recovering_jobs = jobs_summary(1, 1, 0);
    let recovering = brief_for(
        &store,
        &session_id,
        true,
        Some(&clean),
        true,
        Some(&passed),
        Some(&recovering_jobs),
        true,
    );
    assert_eq!(recovering["progress"]["state"], "blocked");

    let terminal_pending_jobs = jobs_summary(0, 0, 1);
    let terminal_pending = brief_for(
        &store,
        &session_id,
        true,
        Some(&clean),
        true,
        Some(&passed),
        Some(&terminal_pending_jobs),
        true,
    );
    assert_eq!(terminal_pending["progress"]["state"], "ready_to_continue");
    assert_eq!(terminal_pending["attention"]["terminal_pending_jobs"], 1);

    store
        .post_message(PostSessionMessageInput {
            session_id: session_id.clone(),
            kind: SessionMessageKind::Question,
            message: "clarify a nonblocking detail".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::Normal,
        })
        .unwrap();
    let open_question = brief_for(
        &store,
        &session_id,
        true,
        Some(&clean),
        true,
        Some(&passed),
        Some(&empty),
        true,
    );
    assert_eq!(open_question["progress"]["state"], "ready_to_continue");
    assert_eq!(open_question["attention"]["open_questions"], 1);
}

#[test]
fn handoff_brief_unresolved_validation_failure_is_blocking_and_bounded() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "fix failing tests");
    add_instruction(&store, &session_id, "run focused validation");
    let failed_names = (0..8)
        .map(|index| format!("tests::handoff_failure_{index}"))
        .collect::<Vec<_>>();
    record_test_validation(&store, &session_id, 0, &failed_names);
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );

    assert_eq!(brief["validation"]["status"], "failed");
    assert_eq!(brief["progress"]["state"], "blocked");
    assert_eq!(
        brief["validation"]["open_failures"]["items"]
            .as_array()
            .unwrap()
            .len(),
        HANDOFF_OPEN_FAILURES_MAX_ITEMS
    );
    assert_eq!(brief["validation"]["open_failures"]["total"], 8);
    assert_eq!(
        brief["validation"]["open_failures"]["returned"],
        HANDOFF_OPEN_FAILURES_MAX_ITEMS
    );
    assert_eq!(brief["validation"]["open_failures"]["truncated"], true);
}

#[test]
fn handoff_brief_caps_changed_paths_and_recent_files_without_mutating_exploration() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "bounded workset");
    add_instruction(&store, &session_id, "edit and inspect");
    for index in 0..15 {
        record_write(&store, &session_id, &format!("src/change_{index:02}.rs"));
    }
    for index in 0..10 {
        record_read(&store, &session_id, &format!("src/recent_{index:02}.rs"));
    }
    let before = store.summary(&session_id, Some(200)).unwrap();
    let workspace = dirty_workspace(0);
    let jobs = empty_jobs();

    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    let after = store.summary(&session_id, Some(200)).unwrap();

    assert_eq!(before.events_total, after.events_total);
    assert_eq!(before.updated_at, after.updated_at);
    assert_eq!(
        brief["progress"]["changes"]["items"]
            .as_array()
            .unwrap()
            .len(),
        HANDOFF_CHANGED_PATHS_MAX_ITEMS
    );
    assert_eq!(brief["progress"]["changes"]["total"], 15);
    assert_eq!(brief["progress"]["changes"]["truncated"], true);
    let recent = brief["progress"]["recent_files"]["items"]
        .as_array()
        .unwrap();
    assert_eq!(recent.len(), HANDOFF_RECENT_FILES_MAX_ITEMS);
    assert_eq!(recent[0], "src/recent_09.rs");
    assert_eq!(recent[7], "src/recent_02.rs");
    assert_eq!(brief["progress"]["recent_files"]["total"], 10);
    assert_eq!(brief["progress"]["recent_files"]["truncated"], true);
}

#[test]
fn handoff_brief_closed_and_insufficient_evidence_states_are_explicit() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "close me");
    store.close_session(&session_id).unwrap();
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    let closed = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    assert_eq!(closed["progress"]["state"], "closed");

    let active_id = start_session(&store, "missing evidence");
    let insufficient = brief_for(
        &store,
        &active_id,
        true,
        None,
        true,
        Some(&json!({"available": false, "status": "unknown"})),
        None,
        false,
    );
    assert_eq!(insufficient["progress"]["state"], "insufficient_evidence");
    assert_eq!(insufficient["basis"]["complete"], false);
    assert_eq!(
        insufficient["basis"]["reason_codes"],
        json!([
            "guidance_unavailable",
            "job_summary_unavailable",
            "validation_unavailable",
            "workspace_unavailable",
        ])
    );
}

#[test]
fn handoff_brief_not_requested_and_unavailable_statuses_use_fixed_reasons() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "projection controls");
    let jobs = empty_jobs();

    let omitted = brief_for(
        &store,
        &session_id,
        false,
        None,
        false,
        None,
        Some(&jobs),
        true,
    );
    assert_eq!(omitted["workspace"]["status"], "not_requested");
    assert_eq!(
        omitted["workspace"]["reason_code"],
        "workspace_not_requested"
    );
    assert_eq!(omitted["validation"]["status"], "not_requested");
    assert_eq!(
        omitted["validation"]["reason_code"],
        "validation_not_requested"
    );
    assert_eq!(omitted["validation"]["open_failures"]["total"], 0);
    assert_eq!(
        omitted["basis"]["reason_codes"],
        json!(["validation_not_requested", "workspace_not_requested"])
    );

    let unavailable_workspace = json!({
        "git_available": false,
        "clean": true,
        "git_error": "sensitive internal error must not escape",
    });
    let unavailable = brief_for(
        &store,
        &session_id,
        true,
        Some(&unavailable_workspace),
        true,
        Some(&json!({"available": false, "status": "unknown"})),
        Some(&jobs),
        true,
    );
    assert_eq!(unavailable["workspace"]["status"], "unavailable");
    assert_eq!(
        unavailable["workspace"]["reason_code"],
        "workspace_unavailable"
    );
    assert_eq!(unavailable["validation"]["status"], "unavailable");
    assert!(!unavailable.to_string().contains("sensitive internal error"));
}

#[test]
fn handoff_brief_attempt_boundary_eviction_marks_basis_incomplete() {
    let store = store_with_limit(6);
    let session_id = start_session(&store, "evicted attempt");
    add_instruction(&store, &session_id, "boundary that will be evicted");
    for index in 0..4 {
        record_write(&store, &session_id, &format!("src/evicted_{index}.rs"));
    }
    let summary = store.summary(&session_id, Some(200)).unwrap();
    assert!(summary.events_truncated);
    assert!(summary
        .events
        .iter()
        .all(|event| event.kind != "task_instruction"));
    let workspace = dirty_workspace(0);
    let jobs = empty_jobs();

    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );

    assert_eq!(brief["basis"]["complete"], false);
    assert!(brief["basis"]["reason_codes"]
        .as_array()
        .unwrap()
        .contains(&json!("attempt_boundary_evicted")));
}

#[test]
fn handoff_brief_next_action_priority_is_stable() {
    let store = store_with_limit(200);
    let session_id = start_session(&store, "priority");
    add_instruction(&store, &session_id, "resolve blockers");
    record_test_validation(
        &store,
        &session_id,
        0,
        &["tests::priority_failure".to_string()],
    );
    store
        .post_message(PostSessionMessageInput {
            session_id: session_id.clone(),
            kind: SessionMessageKind::Risk,
            message: "review the risky migration".to_string(),
            tags: Vec::new(),
            reply_to: None,
            priority: SessionMessagePriority::High,
        })
        .unwrap();
    let workspace = dirty_workspace(1);
    let jobs = jobs_summary(1, 1, 0);

    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    assert_eq!(brief["progress"]["state"], "blocked");
    assert_eq!(
        brief["next_actions"],
        json!([
            "resolve workspace conflicts before continuing",
            "await recovering jobs before relying on their output",
            "resolve the latest validation failure",
            "review open risk guidance before continuing",
        ])
    );
    assert!(brief["next_actions"].as_array().unwrap().len() <= HANDOFF_NEXT_ACTIONS_MAX_ITEMS);
}

fn mutate_feedback_to_worst_case(feedback: &mut Value) {
    let long_path = |prefix: &str, index: usize| {
        let suffix = "x".repeat(470);
        format!("{prefix}/{index:03}_{suffix}.rs")
    };
    let changes = (0..100)
        .map(|index| json!(long_path("src/quoted_\\segment", index)))
        .collect::<Vec<_>>();
    let recent = (0..100)
        .map(|index| json!(long_path("src/recent_\\segment", index)))
        .collect::<Vec<_>>();
    let failures = (0..20)
        .map(|index| {
            json!({
                "kind": "test",
                "name": format!("tests::{}", format!("failure_{index}_").repeat(18)),
            })
        })
        .collect::<Vec<_>>();
    feedback["status"] = json!("available");
    feedback["attempt"]["changes"] = json!({
        "changed_paths": changes,
        "total_changed_paths": 100,
        "truncated": false,
    });
    feedback["attempt"]["exploration"] = json!({
        "observed_paths": recent,
        "total_observed_paths": 100,
        "truncated": false,
        "read_count": 100,
        "search_count": 0,
        "navigation_count": 0,
        "latest_tool": "read_file",
        "complete": true,
    });
    feedback["attempt"]["validation"] = json!({
        "latest_status": "failed",
        "unresolved_failure_count": 20,
        "open_failures": failures,
        "total_open_failures": 20,
        "failures_truncated": false,
        "delta_available": false,
    });
}

#[test]
fn handoff_brief_hard_limit_uses_actual_escaped_json_bytes() {
    let store = store_with_limit(200);
    let root = format!("root {} {}", "\"\\\\\n\t".repeat(180), "界".repeat(600));
    let session_id = start_session(&store, &root);
    let latest = format!("latest {} {}", "\"\\\\\n\t".repeat(180), "新".repeat(600));
    add_instruction(&store, &session_id, &latest);
    let summary = store.summary(&session_id, Some(200)).unwrap();
    let validation = passed_validation();
    let current_validation = current_validation_evidence_for_session(&summary, 20);
    let discussion = discussion(&store, &session_id);
    let jobs = empty_jobs();
    let mut feedback = continuation_feedback_value(ContinuationFeedbackInput {
        session_summary: &summary,
        validation: &validation,
        jobs: &jobs,
        discussion: &discussion,
        continuation: "continued",
        suggest_exploration_continuity: false,
        workspace_conflicts: false,
        hooks: continuation_projection_hooks(),
        current_validation: continuation_validation_snapshot(&current_validation),
    });
    mutate_feedback_to_worst_case(&mut feedback);
    let mut workspace = dirty_workspace(0);
    workspace["branch"] = json!(format!("feature/{}", "b".repeat(240)));

    let brief = build_handoff_brief(HandoffBriefInput {
        session_summary: &summary,
        continuation_feedback: &feedback,
        workspace_requested: true,
        workspace: Some(&workspace),
        validation_requested: true,
        validation: Some(&json!({
            "available": true,
            "status": "failed",
            "latest_status": "failed",
            "unresolved_failures": {"count": 20, "events": []},
        })),
        jobs: Some(&jobs),
        guidance_available: true,
        existing_suggested_actions: None,
    });
    let bytes = handoff_brief_size(&brief);
    eprintln!("worst_case_handoff_brief_bytes={bytes}");

    assert!(bytes < HANDOFF_BRIEF_HARD_MAX_BYTES, "{bytes}");
    assert_eq!(brief["progress"]["recent_files"]["truncated"], true);
    assert_eq!(brief["progress"]["changes"]["truncated"], true);
    assert_eq!(brief["validation"]["open_failures"]["truncated"], true);
    assert_eq!(brief["version"], 1);
    assert!(brief["session"].is_object());
    assert!(brief["progress"]["state"].is_string());
    assert!(brief["attention"].is_object());
    assert!(brief["basis"].is_object());
}

#[test]
fn handoff_brief_redacts_instruction_credentials() {
    let store = store_with_limit(200);
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    for credential in [
        "wc_pat_test_secret_value",
        "Bearer test_bearer_value",
        "client_secret=test_client_secret_value",
    ] {
        let session_id = start_session(&store, &format!("do not expose {credential}"));
        let brief = brief_for(
            &store,
            &session_id,
            true,
            Some(&workspace),
            true,
            None,
            Some(&jobs),
            true,
        );
        let serialized = brief.to_string();
        assert!(
            !serialized.contains(credential),
            "handoff brief leaked {credential}: {serialized}"
        );
        assert_eq!(brief["task"]["root_instruction"]["excerpt"], "[redacted]");
        assert_eq!(brief["task"]["root_instruction"]["truncated"], true);
    }

    let latest_secret = "wc_oat_latest_secret_value";
    let session_id = start_session(&store, "safe root instruction");
    add_instruction(
        &store,
        &session_id,
        &format!("latest instruction includes {latest_secret}"),
    );
    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    assert!(!brief.to_string().contains(latest_secret));
    assert_eq!(brief["task"]["latest_instruction"]["excerpt"], "[redacted]");
}

#[test]
fn handoff_brief_preserves_instruction_paths_commands_and_code_deterministically() {
    let store = store_with_limit(200);
    let root_instruction = r#"Continue the handoff task with its useful local context.
Inspect /root/git/private-drop/src/tool_runtime/handoff_brief.rs, C:\repo\private-drop\src, \\server\share\repo, and file:///root/git/private-drop.
Run the focused check:
```shell
cargo test -p webcodex --lib handoff_brief
```
Keep the inline `git diff --check` result with the task."#;
    let latest_instruction = r#"Continue from /home/user/repo and C:/repo/private-drop.
$ cargo check -p webcodex --all-targets
PS> cargo test -p webcodex --lib handoff
Keep `file:///root/git/private-drop/docs/agent/session-model.md` and the normal task description."#;
    let session_id = start_session(&store, root_instruction);
    add_instruction(&store, &session_id, latest_instruction);
    let workspace = clean_workspace();
    let jobs = empty_jobs();

    let first = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    let second = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    assert_eq!(first, second, "the projection must be deterministic");

    let root = first["task"]["root_instruction"]["excerpt"]
        .as_str()
        .unwrap();
    let latest = first["task"]["latest_instruction"]["excerpt"]
        .as_str()
        .unwrap();
    assert_eq!(root, root_instruction);
    assert_eq!(latest, latest_instruction);
    assert_eq!(first["task"]["root_instruction"]["truncated"], false);
    assert_eq!(first["task"]["latest_instruction"]["truncated"], false);
    assert!(root.chars().count() <= HANDOFF_INSTRUCTION_MAX_CHARS);
    assert!(latest.chars().count() <= HANDOFF_INSTRUCTION_MAX_CHARS);
}

fn assert_all_objects_strict(schema: &Value, path: &str) {
    match schema {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("object") {
                assert_eq!(
                    object.get("additionalProperties").and_then(Value::as_bool),
                    Some(false),
                    "{path} must reject unknown fields"
                );
            }
            for (key, value) in object {
                assert_all_objects_strict(value, &format!("{path}.{key}"));
            }
        }
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                assert_all_objects_strict(value, &format!("{path}[{index}]"));
            }
        }
        _ => {}
    }
}

#[test]
fn handoff_brief_schema_is_shared_strict_and_absent_from_startup() {
    let specs = registered_tool_specs();
    let finish = spec_named(&specs, "finish_coding_task");
    let handoff = spec_named(&specs, "session_handoff_summary");
    let work = spec_named(&specs, "work_on_project");
    let finish_schema =
        &finish.output_schema["properties"]["output"]["properties"]["handoff_brief"];
    let handoff_schema =
        &handoff.output_schema["properties"]["output"]["properties"]["handoff_brief"];

    let mut finish_shape = finish_schema.clone();
    let mut handoff_shape = handoff_schema.clone();
    finish_shape.as_object_mut().unwrap().remove("description");
    handoff_shape.as_object_mut().unwrap().remove("description");
    assert_eq!(finish_shape, handoff_shape);
    assert_all_objects_strict(finish_schema, "handoff_brief");
    let truncated_description = finish_schema["properties"]["task"]["properties"]
        ["root_instruction"]["properties"]["truncated"]["description"]
        .as_str()
        .unwrap();
    assert!(truncated_description.contains("600-character"));
    assert!(truncated_description.contains("credential redaction"));
    assert!(work
        .output_schema
        .to_string()
        .find("\"handoff_brief\"")
        .is_none());
    let openapi = crate::openapi::build_openapi_spec();
    assert_eq!(
        &openapi["components"]["schemas"]["HandoffBrief"], handoff_schema,
        "REST/OpenAPI and GPT Actions must reuse the MCP/runtime handoff schema"
    );
    assert_eq!(
        openapi["components"]["schemas"]["ToolResult"]["properties"]["output"]["oneOf"][0]
            ["properties"]["handoff_brief"]["$ref"],
        "#/components/schemas/HandoffBrief"
    );

    let store = store_with_limit(200);
    let session_id = start_session(&store, "schema");
    let workspace = clean_workspace();
    let jobs = empty_jobs();
    let brief = brief_for(
        &store,
        &session_id,
        true,
        Some(&workspace),
        true,
        None,
        Some(&jobs),
        true,
    );
    let envelope = json!({
        "success": true,
        "output": {
            "handoff_brief": brief,
        },
    });
    validate_schema_instance_for_test(&envelope, &finish.output_schema).unwrap();

    let mut unknown = envelope;
    unknown["output"]["handoff_brief"]["unknown_field"] = json!(true);
    assert!(validate_schema_instance_for_test(&unknown, &finish.output_schema).is_err());
}

#[tokio::test]
async fn internal_handoff_projection_does_not_append_events_or_enqueue_agent_requests() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "handoff-brief-no-probe";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("handoff read".to_string()));
    let before = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();

    let result = runtime
        .session_handoff_summary(
            session.session_id.clone(),
            Some(project),
            Some(false),
            Some(false),
            Some(false),
            true,
            Some(20),
            None,
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert!(result.output["handoff_brief"].is_object());
    assert_eq!(
        result.output["handoff_brief"]["workspace"]["status"],
        "not_requested"
    );
    assert_eq!(
        result.output["handoff_brief"]["validation"]["status"],
        "not_requested"
    );
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    let after = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    assert_eq!(before.events_total, after.events_total);
    assert_eq!(before.updated_at, after.updated_at);
}

#[tokio::test]
async fn public_handoff_dispatch_records_only_standard_telemetry_and_preserves_guidance() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "sentinel.txt", "unchanged\n", "initial");
    let runtime = ToolRuntime::new_for_tests();
    let auth = bootstrap_auth_context();
    let client_id = "handoff-brief-public-dispatch";
    let project =
        register_runner_project_at_path_with_auth(&runtime, client_id, "demo", root.path(), &auth)
            .await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("public dispatch".to_string()));
    add_instruction_for_project(
        &runtime.sessions,
        &session.session_id,
        &project,
        "preserve the current task instruction",
    );
    for (kind, message) in [
        (SessionMessageKind::Guidance, "keep guidance open"),
        (SessionMessageKind::Question, "keep question open"),
        (SessionMessageKind::Todo, "keep todo open"),
        (SessionMessageKind::Risk, "keep risk open"),
    ] {
        runtime
            .sessions
            .post_message(PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind,
                message: message.to_string(),
                tags: Vec::new(),
                reply_to: None,
                priority: SessionMessagePriority::Normal,
            })
            .unwrap();
    }

    let before = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    let before_discussion = serde_json::to_value(
        runtime
            .sessions
            .discussion_summary(&session.session_id, Some(20))
            .unwrap(),
    )
    .unwrap();
    let before_instruction_count = before
        .events
        .iter()
        .filter(|event| event.kind == "task_instruction")
        .count();
    let before_file = std::fs::read(root.path().join("sentinel.txt")).unwrap();
    let (before_status_code, before_status, before_status_error, _) =
        crate::tool_runtime::helpers::run_command_sync("git status --porcelain", root.path(), 30);
    assert_eq!(
        before_status_code, 0,
        "fixture git status failed: {before_status_error}"
    );

    let result = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffSummary {
                session_id: session.session_id.clone(),
                project: Some(project),
                include_workspace: Some(false),
                include_checkpoints: Some(false),
                include_validation: Some(false),
                summary_only: true,
                limit: Some(20),
            },
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert!(result.output["handoff_brief"].is_object());
    let after = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    assert_eq!(after.events_total, before.events_total + 2);
    let new_events = &after.events[before.events.len()..];
    assert_eq!(
        new_events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>(),
        vec!["tool_call_started", "tool_call_finished"]
    );
    assert!(new_events
        .iter()
        .all(|event| event.tool_name == "session_handoff_summary"));
    assert!(new_events.iter().all(|event| {
        event.read_like
            && !event.write_like
            && !event.shell_like
            && !event.git_like
            && event.changed_paths.is_empty()
            && event.observed_paths.is_empty()
    }));
    assert_eq!(
        after
            .events
            .iter()
            .filter(|event| event.kind == "task_instruction")
            .count(),
        before_instruction_count
    );

    let after_discussion = serde_json::to_value(
        runtime
            .sessions
            .discussion_summary(&session.session_id, Some(20))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(before_discussion, after_discussion);
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
    assert_eq!(
        std::fs::read(root.path().join("sentinel.txt")).unwrap(),
        before_file
    );
    let (after_status_code, after_status, after_status_error, _) =
        crate::tool_runtime::helpers::run_command_sync("git status --porcelain", root.path(), 30);
    assert_eq!(
        after_status_code, 0,
        "fixture git status failed: {after_status_error}"
    );
    assert_eq!(before_status, after_status);
}

#[tokio::test]
async fn finish_and_handoff_surfaces_return_the_same_brief_for_the_same_snapshot() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "hello\n", "initial");
    let runtime = ToolRuntime::new_for_tests();
    let client_id = "handoff-brief-shared";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("shared builder".to_string()));
    let auth = bootstrap_auth_context();

    let finish_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::FinishCodingTask {
                        project,
                        session_id,
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: Some(false),
                        include_hygiene: Some(false),
                        include_handoff: Some(false),
                        include_validation_summary: Some(false),
                    },
                    Some(&auth),
                )
                .await
        }
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !finish_task.is_finished() {
        assert!(
            std::time::Instant::now() < deadline,
            "finish_coding_task did not finish within 10 seconds while proving shared handoff brief"
        );
        if let Some(request) = probe_patch_agent_request(&runtime, client_id).await {
            complete_agent_request_by_running_locally(&runtime, client_id, request).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    let finish = finish_task.await.unwrap();
    assert!(finish.success, "{:?}", finish.error);

    let handoff = runtime
        .session_handoff_summary(
            session.session_id,
            Some(project),
            Some(false),
            Some(false),
            Some(false),
            true,
            Some(20),
            Some(&auth),
        )
        .await;
    assert!(handoff.success, "{:?}", handoff.error);
    assert_eq!(
        finish.output["handoff_brief"],
        handoff.output["handoff_brief"]
    );
}
