//! Contract tests for the compact deterministic `handoff_brief` projection.

use super::support::*;
use crate::tool_runtime::continuation_feedback::{
    continuation_feedback_value, continuation_projection_hooks, continuation_validation_snapshot,
    ContinuationFeedbackInput, ContinuationToolFailureSnapshot,
};
use crate::tool_runtime::handoff_brief::{build_handoff_brief, HandoffBriefInput};
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
use std::collections::HashSet;
use std::sync::Arc;

const PROJECT: &str = "test-project";

fn store_with_limit(max_events: usize) -> SessionStore {
    SessionStore::new(16, max_events)
}

fn start_session(store: &SessionStore, title: &str) -> String {
    store
        .start_session(Some(PROJECT.to_string()), Some(title.to_string()))
        .session_id
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
fn discussion(store: &SessionStore, session_id: &str) -> SessionDiscussionSummary {
    store.discussion_summary(session_id, Some(20)).unwrap()
}

fn raw_failed_event_ids(
    summary: &crate::tool_runtime::sessions::SessionSummary,
) -> HashSet<String> {
    crate::tool_runtime::sessions::canonical_tool_call_finished_events(&summary.events)
        .into_iter()
        .filter(|event| event.status.as_deref() == Some("failed"))
        .map(|event| event.event_id.clone())
        .collect()
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
    let actionable = raw_failed_event_ids(&summary);
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
        tool_failures: ContinuationToolFailureSnapshot::new(&actionable),
    });
    build_handoff_brief(HandoffBriefInput {
        session_summary: &summary,
        discussion: guidance_available.then_some(&discussion),
        continuation_feedback: &continuation,
        workspace_requested,
        workspace,
        validation_requested,
        validation: Some(validation),
        jobs,
        external_observations: None,
        guidance_available,
        existing_suggested_actions: None,
        session_changed_during_snapshot: false,
        external_observations_changed_during_snapshot: false,
    })
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
    let handoff = spec_named(&specs, "read_session_handoff");
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
    let goal_context_schema =
        &handoff.output_schema["properties"]["output"]["properties"]["goal_context"];
    assert_eq!(
        goal_context_schema["properties"]["source"]["const"],
        "explicit_workflow_session_correlation"
    );
    assert_eq!(
        goal_context_schema["properties"]["candidates"]["maxItems"],
        8
    );
    assert!(
        finish.output_schema["properties"]["output"]["properties"]
            .get("goal_context")
            .is_none(),
        "finish keeps its existing goal_follow_up instead of duplicating recovery goal_context"
    );
    let selection_context = json!({
        "version": 1,
        "source": "explicit_workflow_session_correlation",
        "status": "selection_required",
        "reason_code": "multiple_active_goals",
        "truncated": false,
        "goal": null,
        "candidates": [
            {"goal_id": "wc_goal_a", "title": "A", "title_truncated": false, "lifecycle": "active", "revision": 1},
            {"goal_id": "wc_goal_b", "title": "B", "title_truncated": false, "lifecycle": "active", "revision": 2}
        ]
    });
    let selection_envelope = json!({
        "success": true,
        "output": {"goal_context": selection_context}
    });
    validate_schema_instance_for_test(&selection_envelope, &handoff.output_schema).unwrap();

    let mut ambiguous_with_one = selection_envelope.clone();
    ambiguous_with_one["output"]["goal_context"]["candidates"]
        .as_array_mut()
        .unwrap()
        .pop();
    assert!(
        validate_schema_instance_for_test(&ambiguous_with_one, &handoff.output_schema).is_err()
    );

    let mut available_without_goal = selection_envelope;
    available_without_goal["output"]["goal_context"]["status"] = json!("available");
    available_without_goal["output"]["goal_context"]["reason_code"] = Value::Null;
    available_without_goal["output"]["goal_context"]["candidates"] = json!([]);
    assert!(
        validate_schema_instance_for_test(&available_without_goal, &handoff.output_schema).is_err()
    );

    let external_schema = &finish_schema["properties"]["external_observations"];
    assert_eq!(
        external_schema["properties"]["provenance"]["const"],
        "external_report"
    );
    assert_eq!(
        external_schema["properties"]["coverage"]["properties"]["complete"]["const"],
        false
    );
    assert_eq!(
        external_schema["properties"]["observations"]["anyOf"][0]["maxItems"],
        5
    );
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
async fn hidden_handoff_state_is_non_recording_exact_recovery_read() {
    let root = tempfile::tempdir().unwrap();
    init_git_repo(root.path());
    // This case specifically proves the available-store/zero-Goal contract.
    // A missing Goal store has distinct explicit `unavailable` semantics and
    // is covered by the Goal workflow recovery test.
    let goal_store = tempfile::tempdir().unwrap();
    let goal_db =
        Arc::new(crate::db::Database::open(&goal_store.path().join("handoff-goals.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests().with_communication_database(goal_db);
    let auth = bootstrap_auth_context();
    let client_id = "handoff-state-hidden-read";
    let project =
        register_runner_project_at_path_with_auth(&runtime, client_id, "demo", root.path(), &auth)
            .await;
    let session = runtime.sessions.start_session(
        Some(project.clone()),
        Some("hidden handoff read".to_string()),
    );
    add_instruction_for_project(
        &runtime.sessions,
        &session.session_id,
        &project,
        "preserve exact recovery state",
    );
    let before = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();

    let result = runtime
        .dispatch_with_auth(
            ToolCall::SessionHandoffState {
                project: project.clone(),
                session_id: session.session_id.clone(),
            },
            Some(&auth),
        )
        .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["session_id"], session.session_id);
    assert!(result.output["handoff_brief"].is_object());
    assert!(
        result.output.get("goal_context").is_none(),
        "zero correlated active Goals must omit recovery goal_context"
    );
    assert_eq!(
        result.output["handoff_brief"]["external_observations"]["provenance"],
        "external_report"
    );
    let after = runtime
        .sessions
        .summary(&session.session_id, Some(200))
        .unwrap();
    assert_eq!(before.events_total, after.events_total);
    assert_eq!(before.updated_at, after.updated_at);
    assert!(probe_patch_agent_request(&runtime, client_id)
        .await
        .is_none());
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
                diagnostic: true,
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
        .all(|event| event.tool_name == "read_session_handoff"));
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
    for include_handoff in [false, true] {
        assert_finish_and_handoff_shared_external_brief(include_handoff).await;
    }
}

async fn assert_finish_and_handoff_shared_external_brief(include_handoff: bool) {
    let root = tempfile::tempdir().unwrap();
    let db_root = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::db::Database::open(&db_root.path().join("db")).unwrap());
    init_git_repo(root.path());
    commit_file(root.path(), "README.md", "hello\n", "initial");
    let runtime = ToolRuntime::new_for_tests().with_communication_database(db.clone());
    let client_id = "handoff-brief-shared";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", root.path()).await;
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), Some("shared builder".to_string()));
    db.record_external_observation(
        &session.session_id,
        &project,
        webcodex_store::ExternalObservation {
            adapter_id: "a".repeat(64),
            event_id: "b".repeat(64),
            tool: "Bash".to_string(),
            exit_code: None,
            recorded_at: 1,
        },
    )
    .unwrap();
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
                        outputs: Vec::new(),
                        project,
                        session_id,
                        summary_only: false,
                        include_diff: Some(false),
                        include_workspace: Some(false),
                        include_hygiene: Some(false),
                        include_handoff: Some(include_handoff),
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
    assert_eq!(
        finish.output["handoff_brief"]["external_observations"]["observations"][0]["event_id"],
        "b".repeat(64),
    );
    if include_handoff {
        assert_eq!(
            finish.output["handoff"]["handoff_brief"]["external_observations"],
            finish.output["handoff_brief"]["external_observations"],
        );
    }
}
