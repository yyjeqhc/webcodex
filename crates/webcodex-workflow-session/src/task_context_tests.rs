use crate::task_context::*;
use crate::ContinuationToolFailureSnapshot;
use crate::{SessionEvent, SessionSummary};
use crate::{SessionPathHint, SessionStore, SessionToolContract, SessionTransport};
use serde_json::{json, Value};
use std::collections::HashSet;

fn fixture() -> (SessionSummary, SessionEvent) {
    let store = SessionStore::new(4, 200);
    let session = store.start_session(Some("project".into()), Some("task".into()));
    let started = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_test",
        &json!({"project":"project"}),
        SessionToolContract {
            risk_class: "execute",
            read_like: false,
            write_like: false,
            shell_like: true,
            git_like: false,
            change_summary_like: false,
            project_write: false,
            path_hint: SessionPathHint::None,
        },
    );
    store.record_tool_call_finished(started, false, &json!({"exit_code":1}), None, None);
    let mut summary = store.summary(&session.session_id, Some(200)).unwrap();
    let mut event = summary
        .events
        .iter()
        .find(|event| event.kind == "tool_call_finished")
        .unwrap()
        .clone();
    event.resolved_project = Some("project".into());
    event.input_summary =
        Some(json!({"execution_identity":"target:aaaaaaaaaaaaaaaaaaaaaaaa","command":"redacted"}));
    event.validation_output_summary =
        Some(json!({"canonical_failure":{"kind":"test","name":"tests::fails"}}));
    event.exit_code = Some(1);
    event.failure_kind = Some("exit_code".into());
    summary.events.clear();
    (summary, event)
}

fn append(summary: &mut SessionSummary, template: &SessionEvent, index: usize) {
    let mut event = template.clone();
    event.event_id = format!("evt_{index}");
    event.call_id = Some(format!("wc_call_{index}"));
    event.logical_invocation_id = None;
    event.logical_invocation_role = None;
    let mut start = event.clone();
    start.kind = "tool_call_started".into();
    start.event_id = format!("evt_start_{index}");
    start.status = None;
    start.validation_output_summary = None;
    event.input_summary = None;
    summary.events.push(start);
    summary.events.push(event);
}

fn finished_mut(summary: &mut SessionSummary, index: usize) -> &mut SessionEvent {
    summary
        .events
        .iter_mut()
        .filter(|event| event.kind == "tool_call_finished")
        .nth(index)
        .unwrap()
}

fn started_mut(summary: &mut SessionSummary, index: usize) -> &mut SessionEvent {
    summary
        .events
        .iter_mut()
        .filter(|event| event.kind == "tool_call_started")
        .nth(index)
        .unwrap()
}

fn canonical_test_identity(event: &SessionEvent) -> Option<Value> {
    event
        .validation_output_summary
        .as_ref()?
        .get("canonical_failure")
        .cloned()
}

fn context(summary: &SessionSummary) -> Value {
    let actionable = summary
        .events
        .iter()
        .filter(|event| event.status.as_deref() == Some("failed"))
        .map(|event| event.event_id.clone())
        .collect::<HashSet<_>>();
    task_context_value(
        summary,
        None,
        &Value::Null,
        Some(ContinuationToolFailureSnapshot::new(&actionable)),
        Some(canonical_test_identity),
    )
}

#[test]
fn task_context_three_canonical_unexpected_failures_are_advisory_only() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let value = context(&summary);
    assert_eq!(value["stall"]["status"], "suspected");
    assert_eq!(
        value["stall"]["event_ids"],
        json!(["evt_2", "evt_1", "evt_0"])
    );
    assert_eq!(value["stall"]["advisory_only"], true);
    assert!(value.get("progress").is_none());
    assert_eq!(value["scope"]["session_id"], summary.session_id);
    assert_eq!(
        value["continuation_entry"]["session_id"],
        summary.session_id
    );
}

#[test]
fn task_context_redacted_argv_and_assertion_names_never_prove_operation_identity() {
    let (mut summary, mut event) = fixture();
    for identity in [
        None,
        Some("assertion:aaaaaaaaaaaaaaaaaaaaaaaa"),
        Some("command:bad"),
    ] {
        summary.events.clear();
        event.input_summary =
            Some(json!({"command":"same redacted argv","execution_identity":identity}));
        for index in 0..3 {
            append(&mut summary, &event, index);
        }
        assert_eq!(context(&summary)["stall"]["status"], "unavailable");
    }
}

#[test]
fn task_context_different_targets_or_new_failure_evidence_break_the_run() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    started_mut(&mut summary, 1).input_summary =
        Some(json!({"execution_identity":"target:bbbbbbbbbbbbbbbbbbbbbbbb","command":"redacted"}));
    assert_eq!(context(&summary)["stall"]["status"], "not_observed");
    started_mut(&mut summary, 1).input_summary = event.input_summary.clone();
    finished_mut(&mut summary, 1).validation_output_summary =
        Some(json!({"canonical_failure":{"kind":"test","name":"different_test"}}));
    assert_eq!(context(&summary)["stall"]["status"], "not_observed");
}

#[test]
fn task_context_expected_or_reconciled_failures_do_not_trigger_stall() {
    let (mut summary, mut event) = fixture();
    for expected in [true, false] {
        summary.events.clear();
        event.expected_failure = Some(expected);
        event.failure_expectation_result = Some("matched_expected_result".into());
        for index in 0..3 {
            append(&mut summary, &event, index);
        }
        assert_eq!(context(&summary)["stall"]["status"], "not_observed");
    }
    let empty = HashSet::new();
    let value = task_context_value(
        &summary,
        None,
        &Value::Null,
        Some(ContinuationToolFailureSnapshot::new(&empty)),
        Some(canonical_test_identity),
    );
    assert_eq!(value["stall"]["status"], "not_observed");
}

#[test]
fn task_context_fresh_read_resets_failure_run_but_poll_heartbeat_and_pagination_do_not() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    for (index, tool) in ["get_job", "agent_heartbeat", "read_files"]
        .iter()
        .enumerate()
    {
        let mut polling = event.clone();
        polling.tool_name = (*tool).into();
        polling.status = Some("succeeded".into());
        if *tool == "read_files" {
            polling.input_summary = Some(json!({"offset":10}));
        }
        append(&mut summary, &polling, index + 10);
    }
    assert_eq!(context(&summary)["stall"]["status"], "suspected");
    let mut read = event.clone();
    read.tool_name = "read_files".into();
    read.status = Some("succeeded".into());
    read.input_summary = Some(json!({"path":"src/new.rs"}));
    append(&mut summary, &read, 20);
    assert_eq!(context(&summary)["stall"]["status"], "not_observed");
}

#[test]
fn task_context_recovery_and_closeout_projections_do_not_change_failure_evidence() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let before = context(&summary);
    for (index, tool) in [
        "read_session_handoff",
        "read_validation_summary",
        "finish_coding_task",
    ]
    .iter()
    .enumerate()
    {
        let mut projection = event.clone();
        projection.tool_name = (*tool).into();
        projection.status = Some("succeeded".into());
        append(&mut summary, &projection, index + 10);
    }
    assert_eq!(context(&summary), before);
    let mut review = event.clone();
    review.tool_name = "read_workspace_changes".into();
    review.status = Some("succeeded".into());
    append(&mut summary, &review, 20);
    assert_eq!(context(&summary)["stall"]["status"], "not_observed");
}

#[test]
fn task_context_replayed_ids_and_recorder_business_duplicates_cannot_make_three_operations() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let replay = finished_mut(&mut summary, 2).call_id.clone();
    finished_mut(&mut summary, 1).call_id = replay;
    assert_eq!(context(&summary)["stall"]["status"], "unavailable");
    summary.events.clear();
    for index in 0..4 {
        append(&mut summary, &event, index);
    }
    for pair in 0..2 {
        for (index, role) in [(pair * 2, "recorder"), (pair * 2 + 1, "business")] {
            for event in summary.events.iter_mut().filter(|event| {
                event.call_id.as_deref() == Some(format!("wc_call_{index}").as_str())
            }) {
                event.logical_invocation_id =
                    Some(format!("wc_inv_{}", "a".repeat(31) + &pair.to_string()));
                event.logical_invocation_role = Some(role.into());
            }
        }
    }
    assert_ne!(context(&summary)["stall"]["status"], "suspected");
    assert_eq!(
        context(&summary)["stall"]["event_ids"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn task_context_structured_unknown_failure_kinds_require_reconciliation() {
    for field in ["failure_kind", "error_kind", "actual_failure_kind"] {
        let (mut summary, mut event) = fixture();
        event.failure_kind = None;
        event.error_kind = None;
        event.actual_failure_kind = None;
        event.effect_evidence = None;
        event.validation_output_summary = None;
        match field {
            "failure_kind" => event.failure_kind = Some("outcome_unknown".into()),
            "error_kind" => event.error_kind = Some("outcome_unknown".into()),
            _ => event.actual_failure_kind = Some("outcome_unknown".into()),
        }
        append(&mut summary, &event, 0);
        assert_eq!(
            context(&summary)["stall"]["status"],
            "reconcile_required",
            "{field}"
        );
    }
}

#[test]
fn task_context_successful_job_handoff_does_not_remain_an_unknown_outcome() {
    let store = SessionStore::new(4, 200);
    let session = store.start_session(Some("project".into()), Some("task".into()));
    let contract = SessionToolContract {
        risk_class: "execute",
        read_like: false,
        write_like: false,
        shell_like: true,
        git_like: false,
        change_summary_like: false,
        project_write: false,
        path_hint: SessionPathHint::None,
    };
    let started = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "cargo_test",
        &json!({"project":"project"}),
        contract,
    );
    store.record_tool_call_finished(
        started,
        true,
        &json!({
            "job_id":"job-task-context", "execution_state":"started",
            "command_started":true, "command_completed":false,
            "promoted_to_job":true, "terminal":false,
        }),
        None,
        None,
    );
    let accepted = store.summary(&session.session_id, Some(200)).unwrap();
    // Pending execution belongs to the existing Job lifecycle projection;
    // the immutable acceptance event must not invent outcome uncertainty.
    assert_ne!(context(&accepted)["stall"]["status"], "reconcile_required");
    let accepted_at = accepted.events.last().unwrap().timestamp;
    assert!(store.record_validation_job_terminal(
        &session.session_id,
        "job-task-context",
        &["job-task-context"],
        "cargo_test",
        contract,
        Some("project".into()),
        "target:aaaaaaaaaaaaaaaaaaaaaaaa",
        None,
        "completed",
        Some(0),
        Some(true),
        Some(accepted_at),
        Some(accepted_at + 1),
        Some(1000),
        Some(json!({"execution_state":"completed"})),
    ));
    let completed = store.summary(&session.session_id, Some(200)).unwrap();
    assert_ne!(context(&completed)["stall"]["status"], "reconcile_required");

    for variant in ["unknown_state", "unknown_kind", "failed_handoff", "no_job"] {
        let mut uncertain = accepted.clone();
        let event = finished_mut(&mut uncertain, 0);
        match variant {
            "unknown_state" => {
                event.effect_evidence.as_mut().unwrap().execution_state =
                    Some("outcome_unknown".into())
            }
            "unknown_kind" => event.failure_kind = Some("outcome_unknown".into()),
            "failed_handoff" => event.status = Some("failed".into()),
            _ => event.job_id = None,
        }
        assert_eq!(
            context(&uncertain)["stall"]["status"],
            "reconcile_required",
            "{variant}"
        );
    }
}

#[test]
fn task_context_unknown_execution_requires_reconciliation_even_after_a_read() {
    let (mut summary, mut event) = fixture();
    event.effect_evidence = Some(crate::model::ToolEffectEventEvidence {
        state_changed: None,
        command_started: Some(true),
        command_completed: None,
        execution_state: Some("outcome_unknown".into()),
    });
    append(&mut summary, &event, 0);
    event.effect_evidence = None;
    event.status = Some("succeeded".into());
    event.tool_name = "read_files".into();
    append(&mut summary, &event, 1);
    let value = context(&summary);
    assert_eq!(value["stall"]["status"], "reconcile_required");
    assert_eq!(value["stall"]["event_ids"], json!(["evt_0"]));
    assert_eq!(value["stall"]["recommendation"], RECONCILE_ACTION);
}

#[test]
fn task_context_exact_scope_boundary_and_retention_gaps_are_conservative() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let mut boundary = event.clone();
    boundary.kind = "task_instruction".into();
    boundary.event_id = "evt_boundary".into();
    summary.events.insert(4, boundary);
    assert_eq!(context(&summary)["stall"]["status"], "not_observed");
    assert_eq!(
        context(&summary)["scope"]["instruction_event_id"],
        "evt_boundary"
    );
    summary
        .events
        .retain(|event| event.kind != "task_instruction");
    summary.events_truncated = true;
    assert_eq!(context(&summary)["stall"]["status"], "unavailable");
    summary.events_truncated = false;
    finished_mut(&mut summary, 1).session_id = "other".into();
    finished_mut(&mut summary, 2).resolved_project = Some("other".into());
    assert_eq!(context(&summary)["stall"]["event_ids"], json!(["evt_0"]));
}

#[test]
fn task_context_bounds_files_locations_and_preserves_safe_source_references() {
    let (mut summary, mut event) = fixture();
    event.status = Some("succeeded".into());
    event.observed_paths = (0..12).map(|index| format!("src/{index}.rs")).collect();
    event.changed_paths = vec!["/private/root/secret.rs".into(), "../outside.rs".into()];
    append(&mut summary, &event, 0);
    let failures = json!([{"kind":"diagnostic","name":"E0308","file":"src/main.rs","line":17},
        {"kind":"test","name":"test::expected","file":"/private/root/secret.rs"},
        {"kind":"test","name":"/private/leak"}]);
    let value = task_context_value(&summary, None, &failures, None, None);
    assert_eq!(value["relevant_files"]["returned"], 6);
    assert_eq!(value["relevant_files"]["truncated"], true);
    assert_eq!(value["failure_locations"]["returned"], 2);
    assert_eq!(value["failure_locations"]["truncated"], true);
    assert_eq!(value["failure_locations"]["items"][0]["line"], 17);
    assert_eq!(
        value["failure_locations"]["items"][0]["source_ref"],
        "continuation_feedback#/attempt/validation/open_failures/0"
    );
    assert!(!value.to_string().contains("private"));
    assert!(crate::handoff_brief_size(&value) <= TASK_CONTEXT_MAX_BYTES);
}

#[test]
fn task_context_only_latest_twenty_relevant_events_can_require_reconciliation() {
    let (mut summary, mut event) = fixture();
    event.effect_evidence = Some(crate::model::ToolEffectEventEvidence {
        state_changed: None,
        command_started: Some(true),
        command_completed: None,
        execution_state: Some("outcome_unknown".into()),
    });
    append(&mut summary, &event, 0);
    event.effect_evidence = None;
    event.status = Some("succeeded".into());
    for index in 1..25 {
        append(&mut summary, &event, index);
    }
    let value = context(&summary);
    assert_eq!(value["stall"]["scanned_events"], 20);
    assert_eq!(value["stall"]["status"], "not_observed");
}
#[test]
fn task_context_unknown_outcome_suppresses_execution_suggestions_in_both_briefs() {
    let (mut summary, mut event) = fixture();
    event.effect_evidence = Some(crate::model::ToolEffectEventEvidence {
        state_changed: None,
        command_started: Some(true),
        command_completed: None,
        execution_state: Some("outcome_unknown".into()),
    });
    append(&mut summary, &event, 0);
    let notes_store = SessionStore::new(1, 10);
    let notes_session = notes_store.start_session(Some("project".into()), None);
    let discussion = notes_store
        .discussion_summary(&notes_session.session_id, Some(20))
        .unwrap();
    let validation =
        json!({"status":"not_run","available":false,"current_evidence":{"status":"not_run"}});
    let evidence = validation["current_evidence"].clone();
    let current = json!({"latest":null,"unresolved_failures":{"events":[]}});
    let jobs = json!({"active_count":0,"running_count":0,"recovering_count":0,"terminal_pending_count":0,"recent":[],"truncated":false});
    let actionable = HashSet::from(["evt_0".to_string()]);
    let feedback = crate::continuation_feedback_value(crate::ContinuationFeedbackInput {
        session_summary: &summary,
        validation: &validation,
        jobs: crate::ContinuationJobSnapshot::from_json(&jobs),
        discussion: &discussion,
        continuation: "continued",
        suggest_exploration_continuity: false,
        workspace_conflicts: false,
        hooks: crate::ContinuationProjectionHooks::new(|_| true),
        current_validation: crate::ContinuationValidationSnapshot::new(&evidence, &current),
        tool_failures: ContinuationToolFailureSnapshot::new(&actionable),
    });
    assert_eq!(
        feedback["attempt"]["suggested_next_actions"],
        json!([RECONCILE_ACTION])
    );
    let mut input = crate::HandoffBriefInput {
        session_summary: &summary,
        discussion: Some(&discussion),
        continuation_feedback: &feedback,
        workspace_requested: false,
        workspace: None,
        validation_requested: true,
        validation: Some(&validation),
        jobs: Some(&jobs),
        external_observations: None,
        guidance_available: true,
        session_changed_during_snapshot: false,
        external_observations_changed_during_snapshot: false,
        existing_suggested_actions: None,
    };
    let brief = crate::build_handoff_brief(input);
    assert_eq!(brief["next_actions"], json!([RECONCILE_ACTION]));
    assert!(crate::handoff_brief_size(&brief) <= crate::HANDOFF_BRIEF_HARD_MAX_BYTES);
    input = crate::HandoffBriefInput {
        session_changed_during_snapshot: true,
        ..crate::HandoffBriefInput {
            session_summary: &summary,
            discussion: Some(&discussion),
            continuation_feedback: &feedback,
            workspace_requested: false,
            workspace: None,
            validation_requested: true,
            validation: Some(&validation),
            jobs: Some(&jobs),
            external_observations: None,
            guidance_available: true,
            session_changed_during_snapshot: false,
            external_observations_changed_during_snapshot: false,
            existing_suggested_actions: None,
        }
    };
    let raced = crate::build_handoff_brief(input);
    assert_eq!(raced["task"]["stall"]["status"], "unavailable");
    assert_eq!(raced["task"]["stall"]["reason_code"], "snapshot_changed");
    assert_eq!(raced["progress"]["state"], "insufficient_evidence");
    assert_eq!(
        raced["next_actions"],
        json!(["re-read read_session_handoff for the exact Session before dependent work"])
    );
}

#[test]
fn task_context_handoff_rejects_context_from_a_different_task_boundary() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let old = context(&summary);
    let mut boundary = event.clone();
    boundary.kind = "task_instruction".into();
    boundary.event_id = "evt_new_task".into();
    summary.events.push(boundary);
    let feedback = json!({"status":"available","attempt":{"task_context":old}});
    let brief = crate::build_handoff_brief(crate::HandoffBriefInput {
        session_summary: &summary,
        discussion: None,
        continuation_feedback: &feedback,
        workspace_requested: false,
        workspace: None,
        validation_requested: false,
        validation: None,
        jobs: None,
        external_observations: None,
        guidance_available: false,
        session_changed_during_snapshot: false,
        external_observations_changed_during_snapshot: false,
        existing_suggested_actions: None,
    });
    assert_eq!(
        brief["task"]["scope"]["instruction_event_id"],
        "evt_new_task"
    );
    assert_eq!(brief["task"]["stall"]["status"], "unavailable");
    assert_eq!(brief["task"]["stall"]["event_ids"], json!([]));
    assert!(!brief["next_actions"].to_string().contains(STALL_ACTION));
}

#[test]
fn task_context_bounds_untrusted_scope_metadata_without_panicking_or_claiming_exact_scope() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    summary.project = Some("界\"\\".repeat(5000));
    let value = context(&summary);
    assert!(value["scope"]["project"].is_null());
    assert_eq!(value["scope"]["complete"], false);
    assert_eq!(value["stall"]["status"], "unavailable");
    assert!(crate::handoff_brief_size(&value) <= TASK_CONTEXT_MAX_BYTES);
    summary.session_id = "x".repeat(10000);
    let value = context(&summary);
    assert!(value["scope"]["session_id"].is_null());
    assert!(value["continuation_entry"]["session_id"].is_null());
    assert!(crate::handoff_brief_size(&value) <= TASK_CONTEXT_MAX_BYTES);
}

#[test]
fn task_context_persisted_session_restores_exact_identity_notes_paths_and_continuation_entry() {
    let directory = tempfile::tempdir().unwrap();
    let ledger = directory.path().join("sessions.json");
    let store = SessionStore::with_persistence(&ledger, 4, 200);
    let session = store.start_session(
        Some("project".into()),
        Some("recover this exact task".into()),
    );
    let read = store.record_tool_call_started(
        Some(&session.session_id),
        SessionTransport::Api,
        "read_files",
        &json!({"project":"project","items":[{"path":"src/recovery.rs"}]}),
        SessionToolContract {
            risk_class: "read",
            read_like: true,
            write_like: false,
            shell_like: false,
            git_like: false,
            change_summary_like: false,
            project_write: false,
            path_hint: SessionPathHint::None,
        },
    );
    store.record_tool_call_finished(read,true,&json!({"items":[{"index":0,"success":true,"output":{"path":"src/recovery.rs","content":"private body"}}]}),None,None);
    let identity = webcodex_core::validation_identity::structured_validation_target_identity(
        webcodex_core::validation_identity::ToolValidationIdentityKind::CargoTest,
        &json!({"cwd":".","filter":"tests::same"}),
    )
    .unwrap();
    for index in 0..3 {
        let started = store.record_tool_call_started(
            Some(&session.session_id),
            SessionTransport::Api,
            "cargo_test",
            &json!({"project":"project","execution_identity":identity}),
            SessionToolContract {
                risk_class: "execute",
                read_like: false,
                write_like: false,
                shell_like: true,
                git_like: false,
                change_summary_like: false,
                project_write: false,
                path_hint: SessionPathHint::None,
            },
        );
        store.record_tool_call_finished(started,false,&json!({"exit_code":101,"stdout_tail":format!("test tests::same ... FAILED\n{index} elapsed\n"),"stderr_tail":""}),None,None);
    }
    for (kind, message) in [
        (
            crate::SessionMessageKind::Decision,
            "Keep the current design",
        ),
        (
            crate::SessionMessageKind::Progress,
            "The review remains pending",
        ),
    ] {
        store
            .post_message(crate::PostSessionMessageInput {
                session_id: session.session_id.clone(),
                kind,
                message: message.into(),
                tags: Vec::new(),
                reply_to: None,
                priority: crate::SessionMessagePriority::Normal,
            })
            .unwrap();
    }
    let before = store.summary(&session.session_id, Some(200)).unwrap();
    let discussion = store
        .discussion_summary(&session.session_id, Some(20))
        .unwrap();
    fn persisted_failure_identity(event: &SessionEvent) -> Option<Value> {
        let output = event.validation_output_summary.as_ref()?;
        let diagnostics = webcodex_core::validation_evidence::parse_cargo_test_diagnostics(
            output.get("stdout_tail_excerpt")?.as_str()?,
            output.get("stderr_tail_excerpt")?.as_str()?,
            false,
        );
        (!diagnostics.failed_test_details.is_empty())
            .then(|| json!(diagnostics.failed_test_details))
    }
    let actionable = before
        .events
        .iter()
        .filter(|event| event.status.as_deref() == Some("failed"))
        .map(|event| event.event_id.clone())
        .collect::<HashSet<_>>();
    let context_before = task_context_value(
        &before,
        Some(&discussion),
        &Value::Null,
        Some(ContinuationToolFailureSnapshot::new(&actionable)),
        Some(persisted_failure_identity),
    );
    assert_eq!(context_before["stall"]["status"], "suspected");
    store.flush_persistence();
    drop(store);
    let restored = SessionStore::with_persistence(&ledger, 4, 200);
    let after = restored.summary(&session.session_id, Some(200)).unwrap();
    let restored_discussion = restored
        .discussion_summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(
        before
            .events
            .iter()
            .map(|event| (&event.event_id, &event.input_summary))
            .collect::<Vec<_>>(),
        after
            .events
            .iter()
            .map(|event| (&event.event_id, &event.input_summary))
            .collect::<Vec<_>>()
    );
    let context_after = task_context_value(
        &after,
        Some(&restored_discussion),
        &Value::Null,
        Some(ContinuationToolFailureSnapshot::new(&actionable)),
        Some(persisted_failure_identity),
    );
    assert_eq!(context_before, context_after);
    assert_eq!(
        context_after["relevant_files"]["items"][0]["path"],
        "src/recovery.rs"
    );
    assert!(!context_after.to_string().contains("private body"));
    assert_eq!(context_after["decisions"]["returned"], 1);
    assert_eq!(context_after["recent_progress"]["returned"], 1);
    assert_eq!(
        context_after["continuation_entry"]["session_id"],
        session.session_id
    );
    assert_eq!(
        context_after["next_steps"]["items"][0]["source_refs"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        after.events.len(),
        before.events.len(),
        "recovery projection must not append progress"
    );
}

#[test]
fn task_context_mismatched_negative_expectation_is_still_an_unexpected_failure() {
    let (mut summary, mut event) = fixture();
    event.expected_failure = Some(true);
    event.failure_expectation_result = Some("expectation_mismatch".into());
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    assert_eq!(context(&summary)["stall"]["status"], "suspected");
}

#[test]
fn task_context_nested_pagination_does_not_become_new_progress() {
    let (mut summary, event) = fixture();
    for index in 0..3 {
        append(&mut summary, &event, index);
    }
    let mut read = event.clone();
    read.status = Some("succeeded".into());
    read.tool_name = "search_project_texts".into();
    read.input_summary = Some(json!({"queries":[{"offset":100,"pattern":"redacted"}]}));
    append(&mut summary, &read, 3);
    assert_eq!(context(&summary)["stall"]["status"], "suspected");
}

#[test]
fn task_context_unicode_notes_respect_serialized_budget_and_explicit_truncation() {
    let store = SessionStore::new(4, 200);
    let session = store.start_session(Some("project".into()), None);
    for kind in [
        crate::SessionMessageKind::Decision,
        crate::SessionMessageKind::Progress,
    ] {
        for _ in 0..5 {
            store
                .post_message(crate::PostSessionMessageInput {
                    session_id: session.session_id.clone(),
                    kind,
                    message: "界🦀\"".repeat(500),
                    tags: Vec::new(),
                    reply_to: None,
                    priority: crate::SessionMessagePriority::Normal,
                })
                .unwrap();
        }
    }
    let summary = store.summary(&session.session_id, Some(200)).unwrap();
    let discussion = store
        .discussion_summary(&session.session_id, Some(20))
        .unwrap();
    let value = task_context_value(&summary, Some(&discussion), &Value::Null, None, None);
    assert!(crate::handoff_brief_size(&value) <= TASK_CONTEXT_MAX_BYTES);
    assert!(
        value["decisions"]["truncated"] == true || value["recent_progress"]["truncated"] == true
    );
    assert_eq!(value["decisions"]["total"], 5);
    assert_eq!(value["recent_progress"]["total"], 5);
    assert_eq!(
        summary.events.len(),
        store
            .summary(&session.session_id, Some(200))
            .unwrap()
            .events
            .len()
    );
}

#[test]
fn task_context_missing_ambiguous_or_cross_boundary_start_identity_is_unavailable() {
    for invalid in [
        "missing",
        "duplicate",
        "wrong_session",
        "wrong_project",
        "wrong_tool",
        "wrong_role",
        "cross_boundary",
    ] {
        let (mut summary, event) = fixture();
        for index in 0..3 {
            append(&mut summary, &event, index);
        }
        match invalid {
            "missing" => {
                summary
                    .events
                    .retain(|event| event.kind != "tool_call_started");
            }
            "duplicate" => {
                let duplicate = started_mut(&mut summary, 1).clone();
                summary.events.insert(0, duplicate);
            }
            "wrong_session" => started_mut(&mut summary, 1).session_id = "other".into(),
            "wrong_project" => started_mut(&mut summary, 1).resolved_project = Some("other".into()),
            "wrong_tool" => started_mut(&mut summary, 1).tool_name = "cargo_check".into(),
            "wrong_role" => {
                started_mut(&mut summary, 1).logical_invocation_role = Some("recorder".into())
            }
            "cross_boundary" => {
                let mut boundary = event.clone();
                boundary.kind = "task_instruction".into();
                boundary.event_id = "evt_boundary".into();
                summary.events.insert(1, boundary);
            }
            _ => unreachable!(),
        }
        assert_eq!(
            context(&summary)["stall"]["status"],
            "unavailable",
            "{invalid}"
        );
    }
}
