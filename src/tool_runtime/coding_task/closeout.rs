//! Workflow closeout observations, decisions, and bounded finish projection.

use super::projection::{
    append_workspace_warnings, merged_suggested_next_actions, resolved_project_payload,
    workspace_payload_from_show_changes,
};
use super::*;

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn finish_coding_task(
        &self,
        project: String,
        session_id: String,
        summary_only: bool,
        include_diff: Option<bool>,
        include_workspace: Option<bool>,
        include_hygiene: Option<bool>,
        include_handoff: Option<bool>,
        include_validation_summary: Option<bool>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        // summary_only never returns raw change provenance, so generating bounded
        // diff bodies cannot make the compact result more decision-complete. Keep
        // full closeout behavior unchanged while allowing the common compact path
        // to reuse exact/current review snapshots without fabricating a canonical
        // show_changes payload from git_diff_hunks output.
        let include_diff = !summary_only && include_diff.unwrap_or(true);
        let include_workspace = include_workspace.unwrap_or(true);
        let include_hygiene = include_hygiene.unwrap_or(true);
        let include_handoff = include_handoff.unwrap_or(true);
        let include_validation_summary = include_validation_summary.unwrap_or(true);

        if let Err(result) = self
            .authorize_session_target(&session_id, "finish_coding_task", auth)
            .await
        {
            return result;
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(err) => return err.into_tool_result(),
        };
        let session_summary = match self
            .sessions
            .summary(&session_id, Some(FINISH_SESSION_EVENT_LIMIT))
        {
            Some(summary) => summary,
            None => return unknown_session_result(&session_id),
        };
        let session_project = session_summary
            .project
            .clone()
            .unwrap_or_else(|| "<projectless>".to_string());
        if session_summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
            return session_project_mismatch_result(
                &session_id,
                "finish_coding_task",
                &SessionProjectMismatch {
                    session_project,
                    request_project: resolved.resolved_id.clone(),
                },
            );
        }
        let mut final_warnings = Vec::new();

        let mut review_snapshot_reuse = json!({
            "status": "miss",
            "reason_code": "snapshot_unavailable",
        });
        let reusable_snapshot = if summary_only {
            review_caller_fingerprint(auth).ok().and_then(|caller| {
                self.latest_workspace_review_snapshot(
                    &caller,
                    &resolved.resolved_id,
                    Some(&session_id),
                )
            })
        } else {
            review_snapshot_reuse["reason_code"] =
                json!("full_output_requires_canonical_show_changes");
            None
        };
        let reusable_snapshot = if let Some(snapshot) = reusable_snapshot {
            if !workspace_snapshot_complete_for_closeout(&snapshot, false) {
                review_snapshot_reuse["reason_code"] = json!("snapshot_projection_incomplete");
                None
            } else {
                match self
                    .workspace_review_source_identity(&resolved.resolved_id)
                    .await
                {
                    Ok(current) if current == snapshot.source => {
                        review_snapshot_reuse = json!({
                            "status": "hit",
                            "reason_code": Value::Null,
                            "snapshot_id": snapshot.snapshot_id,
                        });
                        Some(snapshot)
                    }
                    Ok(_) => {
                        review_snapshot_reuse["reason_code"] = json!("snapshot_stale");
                        None
                    }
                    Err(_) => {
                        review_snapshot_reuse["reason_code"] =
                            json!("snapshot_freshness_unavailable");
                        None
                    }
                }
            }
        } else {
            None
        };

        let changes_result = if let Some(snapshot) = reusable_snapshot.as_ref() {
            ToolResult::ok(closeout_workspace_observation_from_review_snapshot(
                snapshot,
            ))
        } else {
            let show_changes_call = ToolCall::ShowChanges {
                project: resolved.resolved_id.clone(),
                session_id: Some(session_id.clone()),
                include_diff: Some(include_diff),
                max_hunks: None,
                max_hunk_lines: None,
                session_event_limit: Some(50),
            };
            let show_changes_start = self.sessions.record_tool_call_started_with_options(
                Some(&session_id),
                SessionTransport::Api,
                show_changes_call.tool_name(),
                &show_changes_call.session_log_arguments(),
                Some(resolved.resolved_id.clone()),
                crate::tool_runtime::sessions::session_tool_contract(show_changes_call.tool_name()),
            );
            let result = self
                .show_changes(
                    resolved.resolved_id.clone(),
                    Some(session_id.clone()),
                    Some(include_diff),
                    None,
                    None,
                    Some(50),
                )
                .await;
            self.sessions.record_tool_call_finished(
                show_changes_start,
                result.success,
                &result.output,
                result.error.as_deref(),
                None,
            );
            result
        };
        tracing::debug!(
            target: "webcodex::git_review",
            closeout_review_snapshot_reuse = %review_snapshot_reuse["status"],
            closeout_review_snapshot_reason = %review_snapshot_reuse["reason_code"],
            "finish_coding_task Git review snapshot reuse"
        );
        if !changes_result.success {
            final_warnings.push(json!({
                "kind": "show_changes_failed",
                "message": changes_result.error,
            }));
        }
        let workspace = workspace_payload_from_show_changes(&changes_result.output);
        append_workspace_warnings(&workspace, &mut final_warnings);
        let permissions = permission_summary_from_events(
            &session_summary.events,
            crate::tool_runtime::permissions::DEFAULT_PERMISSION_RECENT_LIMIT,
        );

        let hygiene = if include_hygiene {
            let hygiene_call = ToolCall::WorkspaceHygieneCheck {
                project: resolved.resolved_id.clone(),
                max_findings: None,
                include_tracked: None,
                session_id: Some(session_id.clone()),
            };
            let hygiene_start = self.sessions.record_tool_call_started_with_options(
                Some(&session_id),
                SessionTransport::Api,
                hygiene_call.tool_name(),
                &hygiene_call.session_log_arguments(),
                Some(resolved.resolved_id.clone()),
                crate::tool_runtime::sessions::session_tool_contract(hygiene_call.tool_name()),
            );
            let result = self
                .workspace_hygiene_check(
                    resolved.resolved_id.clone(),
                    None,
                    None,
                    Some(session_id.clone()),
                )
                .await;
            self.sessions.record_tool_call_finished(
                hygiene_start,
                result.success,
                &result.output,
                result.error.as_deref(),
                None,
            );
            if !result.success {
                final_warnings.push(json!({
                    "kind": "workspace_hygiene_failed",
                    "message": result.error,
                }));
            }
            result.output
        } else {
            Value::Null
        };
        append_hygiene_warnings(&hygiene, &mut final_warnings);

        let jobs = self
            .active_jobs_summary(Some(&resolved.resolved_id), Some(&session_id), auth, 10)
            .await;
        if let Some(warnings) = jobs.get("warnings").and_then(Value::as_array) {
            final_warnings.extend(warnings.iter().cloned());
        }

        let handoff = if include_handoff {
            let result = self
                .session_handoff_summary(
                    session_id.clone(),
                    Some(resolved.resolved_id.clone()),
                    Some(include_workspace),
                    Some(true),
                    Some(include_validation_summary),
                    true,
                    Some(20),
                    auth,
                )
                .await;
            if !result.success {
                final_warnings.push(json!({
                    "kind": "session_handoff_failed",
                    "message": result.error,
                }));
            }
            result.output
        } else {
            Value::Null
        };
        // Reconcile from the freshest post-inspection ledger snapshot. Validation
        // materialization may itself append authoritative terminal evidence, so
        // refresh once more after deriving validation before classifying tool
        // failure actionability.
        let closeout_pre_validation_summary = self
            .sessions
            .summary(&session_id, Some(FINISH_SESSION_EVENT_LIMIT))
            .unwrap_or_else(|| session_summary.clone());
        let validation = if include_validation_summary {
            self.validation_summary_for_session_with_jobs(
                &closeout_pre_validation_summary,
                10,
                auth,
            )
            .await
        } else {
            skipped_validation_summary()
        };
        let closeout_session_summary = self
            .sessions
            .summary(&session_id, Some(FINISH_SESSION_EVENT_LIMIT))
            .unwrap_or(closeout_pre_validation_summary);
        let work_result_presentation = match self
            .final_changes_presentation_needed(&resolved.resolved_id, &closeout_session_summary)
            .await
        {
            Ok(true) => Some(json!({
                "suggested_call": crate::tool_runtime::SuggestedToolCall::fallback_recovery(
                    "present_work_result",
                    json!({"project": resolved.resolved_id.clone()}),
                ).to_value()
            })),
            Ok(false) => None,
            Err(message) => {
                final_warnings.push(json!({
                    "kind": "work_result_presentation_probe_failed",
                    "message": message,
                }));
                None
            }
        };
        let projection_closeout_session_summary =
            self.refresh_validation_source_summary(&closeout_session_summary);
        let review_evidence =
            review_evidence_summary_for_session(&projection_closeout_session_summary);
        let (work_performed, changed_paths) =
            closeout_work_projection(&projection_closeout_session_summary.events);

        // Continuation feedback reuses the same attempt summary and validation
        // delta projections as start/handoff. It is a read-only projection over
        // the existing closeout summary, validation, and job metadata; it never
        // re-runs validation, mutates the ledger, or replaces the closeout
        // verdict.
        let (discussion, guidance_available) = self.discussion_snapshot(&session_id);
        let continuation_validation = if include_validation_summary {
            validation.clone()
        } else {
            json!({ "available": false, "not_requested": true })
        };
        let continuation_current_validation =
            crate::tool_runtime::validation_events::current_validation_evidence_for_session(
                &projection_closeout_session_summary,
                20,
            );
        let raw_tool_failures =
            tool_failure_summary_from_events(&projection_closeout_session_summary.events, 10);
        let reconciliation = reconcile_closeout_evidence(
            &raw_tool_failures,
            &projection_closeout_session_summary,
            &validation,
        );
        let continuation_feedback = if closeout_session_summary.events.is_empty() {
            not_applicable_continuation_feedback_value("empty_session")
        } else {
            continuation_feedback_value(ContinuationFeedbackInput {
                session_summary: &projection_closeout_session_summary,
                validation: &continuation_validation,
                jobs: &jobs,
                discussion: &discussion,
                continuation: "continued",
                suggest_exploration_continuity: false,
                workspace_conflicts: workspace
                    .pointer("/counts/conflicted")
                    .and_then(Value::as_u64)
                    .unwrap_or(0)
                    > 0,
                hooks: continuation_projection_hooks(),
                current_validation: continuation_validation_snapshot(
                    &continuation_current_validation,
                ),
                tool_failures: ContinuationToolFailureSnapshot::new(
                    &reconciliation.actionable_unexpected_event_ids,
                ),
            })
        };

        // Reuse the handoff's exact read when present. An omitted or failed
        // handoff still gets a single local report read for its own brief.
        let external_observations = handoff
            .pointer("/handoff_brief/external_observations")
            .filter(|value| value.is_object())
            .cloned()
            .unwrap_or_else(|| {
                self.handoff_external_observations(
                    &session_id,
                    projection_closeout_session_summary.project.as_deref(),
                )
            });
        // When a nested handoff supplied the first snapshot, this comparison also
        // spans the rest of closeout. With include_handoff=false it still fences
        // the local read against a concurrent accepted external report.
        let external_observations_changed_during_snapshot = external_observations
            != self.handoff_external_observations(
                &session_id,
                projection_closeout_session_summary.project.as_deref(),
            );

        let mut output = json!({
            "project": project,
            "resolved_project": resolved_project_payload(&resolved),
            "session_id": session_id,
            "workspace": workspace,
            "changes": {
                "show_changes": changes_result.output,
                "review_snapshot_reuse": review_snapshot_reuse,
                "hunks_truncated": changes_result.output
                    .get("hunks_truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "validation": reconciliation.validation,
            "continuation_feedback": continuation_feedback,
            "permissions": permissions,
            "tool_failures": reconciliation.tool_failures,
            "review_evidence": review_evidence,
            "work_performed": work_performed,
            "changed_paths": changed_paths,
            "hygiene": hygiene,
            "handoff": handoff,
            "jobs": jobs,
            "deterministic": true,
            "llm_summary": false,
            "final_warnings": final_warnings,
        });
        if let Some(presentation) = work_result_presentation {
            output["presentation"] = presentation;
        }
        output["suggested_next_actions"] = json!(finish_suggested_next_actions(&output));
        output["handoff_brief"] = build_handoff_brief(HandoffBriefInput {
            session_summary: &projection_closeout_session_summary,
            discussion: guidance_available.then_some(&discussion),
            continuation_feedback: output.get("continuation_feedback").unwrap_or(&Value::Null),
            workspace_requested: include_workspace,
            workspace: output.get("workspace"),
            validation_requested: include_validation_summary,
            validation: output.get("validation"),
            jobs: output.get("jobs"),
            external_observations: Some(&external_observations),
            guidance_available,
            existing_suggested_actions: output.get("suggested_next_actions"),
            session_changed_during_snapshot: false,
            external_observations_changed_during_snapshot,
        });
        if let Some(follow_up) = self.active_goal_context_for_session(auth, &session_id) {
            output["goal_follow_up"] = follow_up;
        }
        let mut decision = finish_decision_output(&output);
        if decision
            .pointer("/task_outcome/blocking")
            .and_then(Value::as_bool)
            == Some(false)
            && output.get("presentation").is_some()
        {
            if let Err(result) = self
                .seal_work_result_changes_for_closeout(
                    &resolved.resolved_id,
                    &closeout_session_summary,
                    auth,
                )
                .await
            {
                let message = result.error.unwrap_or_else(|| {
                    "Final changes could not be sealed at coding closeout".to_string()
                });
                output["final_warnings"]
                    .as_array_mut()
                    .expect("finish final_warnings must remain an array")
                    .push(json!({
                        "kind": "work_result_seal_failed",
                        "message": message,
                    }));
                decision = finish_decision_output(&output);
            }
        }
        if summary_only {
            return ToolResult::ok(compact_finish_output(&decision));
        }
        for field in [
            "facts",
            "hard_blockers",
            "advisories",
            "task_outcome",
            "evidence_history",
            "evidence_integrity",
            "informational_notes",
        ] {
            output[field] = decision.get(field).cloned().unwrap_or(Value::Null);
        }
        output["suggested_next_actions"] = decision["suggested_next_actions"].clone();
        ToolResult::ok(output)
    }

    pub(super) fn discussion_snapshot(
        &self,
        session_id: &str,
    ) -> (sessions::SessionDiscussionSummary, bool) {
        match self.sessions.discussion_summary(session_id, Some(20)) {
            Ok(discussion) => (discussion, true),
            Err(_) => (
                sessions::SessionDiscussionSummary {
                    counts: sessions::SessionDiscussionCounts {
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
                },
                false,
            ),
        }
    }
}
fn closeout_workspace_observation_from_review_snapshot(snapshot: &GitReviewSnapshot) -> Value {
    // This is an internal compact-closeout projection, deliberately not a
    // show_changes result. Full closeout always executes canonical show_changes so
    // its public nested contract never changes shape on a snapshot reuse hit.
    json!({
        "clean": snapshot.summary.get("clean").cloned().unwrap_or(Value::Null),
        "git_available": snapshot.summary.get("git_available").cloned().unwrap_or(Value::Null),
        "non_git_project": snapshot.summary.get("non_git_project").cloned().unwrap_or(Value::Null),
        "counts": snapshot.summary.get("counts").cloned().unwrap_or_else(|| json!({})),
        "warnings": snapshot.summary.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "review_snapshot_id": snapshot.snapshot_id,
        "review_snapshot_reused": true,
    })
}

pub(super) fn finish_decision_output(output: &Value) -> Value {
    let hygiene_checked = output
        .get("hygiene")
        .is_some_and(|hygiene| !hygiene.is_null());
    let workspace_clean = output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .cloned()
        .unwrap_or(Value::Null);
    let workspace_conflicts = output
        .pointer("/workspace/counts/conflicted")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hygiene_clean = output
        .get("hygiene")
        .and_then(|hygiene| hygiene.get("clean"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let hygiene_secret_like_paths = output
        .pointer("/hygiene/counts/secret_like_paths")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let hygiene_truncated = output
        .pointer("/hygiene/truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut decision = json!({
        "workspace_clean": workspace_clean,
        "workspace_conflicts": workspace_conflicts,
        "hygiene_clean": hygiene_clean,
        "hygiene_secret_like_paths": hygiene_secret_like_paths,
        "hygiene_truncated": hygiene_truncated,
        "jobs": compact_jobs(output.get("jobs").unwrap_or(&Value::Null)),
        "tool_failures": compact_tool_failures(output.get("tool_failures").unwrap_or(&Value::Null)),
        "validation": compact_validation(output.get("validation").unwrap_or(&Value::Null)),
        "review_evidence": compact_review_evidence(output.get("review_evidence").unwrap_or(&Value::Null)),
        "work_performed": output.get("work_performed").cloned().unwrap_or_else(|| json!([])),
        "changed_paths": output.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
        "warnings": output.get("final_warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    if let Some(presentation) = output.get("presentation") {
        decision["presentation"] = presentation.clone();
    }
    if let Some(follow_up) = output.get("goal_follow_up") {
        decision["goal_follow_up"] = follow_up.clone();
    }
    apply_compact_workflow_outcomes(&mut decision, true, Some(hygiene_checked));
    let verdict = decision
        .get("verdict")
        .cloned()
        .unwrap_or_else(|| json!({}));
    decision["suggested_next_actions"] = json!(merged_suggested_next_actions(&decision, &verdict));
    decision
        .as_object_mut()
        .expect("finish decision output is an object")
        .remove("verdict");
    decision
}

pub(super) fn compact_finish_output(decision: &Value) -> Value {
    let mut output = json!({
        "summary_only": true,
        "workspace_clean": decision.get("workspace_clean").cloned().unwrap_or(Value::Null),
        "workspace_conflicts": decision.get("workspace_conflicts").cloned().unwrap_or(json!(0)),
        "hygiene_clean": decision.get("hygiene_clean").cloned().unwrap_or(json!(true)),
        "hygiene_secret_like_paths": decision.get("hygiene_secret_like_paths").cloned().unwrap_or(json!(0)),
        "hygiene_truncated": decision.get("hygiene_truncated").cloned().unwrap_or(json!(false)),
        "jobs": decision.get("jobs").cloned().unwrap_or_else(|| compact_jobs(&Value::Null)),
        "validation": compact_finish_validation(decision.get("validation").unwrap_or(&Value::Null)),
        "tool_failures": decision.get("tool_failures").cloned().unwrap_or_else(|| compact_tool_failures(&Value::Null)),
        "task_outcome": decision.get("task_outcome").cloned().unwrap_or(Value::Null),
        "evidence_integrity": decision.get("evidence_integrity").cloned().unwrap_or(Value::Null),
        "warnings": decision.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": decision.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    if let Some(presentation) = decision.get("presentation") {
        output["presentation"] = presentation.clone();
    }
    if let Some(follow_up) = decision.get("goal_follow_up") {
        output["goal_follow_up"] = follow_up.clone();
    }
    output
}

fn compact_finish_validation(validation: &Value) -> Value {
    let current = validation.get("current_evidence").unwrap_or(&Value::Null);
    json!({
        "status": validation.get("status").cloned().unwrap_or_else(|| json!("not_run")),
        "reason": validation.get("reason").cloned().unwrap_or(Value::Null),
        "latest_status": validation.get("latest_status").cloned().unwrap_or_else(|| json!("unknown")),
        "successes": validation.get("successes").and_then(Value::as_u64).unwrap_or(0),
        "failures": validation.get("failures").and_then(Value::as_u64).unwrap_or(0),
        "resolved_failure_count": validation.pointer("/resolved_failures/count").and_then(Value::as_u64).unwrap_or(0),
        "unresolved_failure_count": validation.pointer("/unresolved_failures/count").and_then(Value::as_u64).unwrap_or(0),
        "evidence_gap_count": validation.pointer("/evidence_gaps/count").and_then(Value::as_u64).unwrap_or(0),
        "current_status": current.get("status").cloned().unwrap_or_else(|| json!("unknown")),
        "current_reason": current.get("reason").cloned().unwrap_or(Value::Null),
        "current_validation_events": current.get("events_total").and_then(Value::as_u64).unwrap_or(0),
        "current_successes": current.get("successes").and_then(Value::as_u64).unwrap_or(0),
        "current_failures": current.get("failures").and_then(Value::as_u64).unwrap_or(0),
        "current_resolved_failure_count": current.get("resolved_failure_count").and_then(Value::as_u64).unwrap_or(0),
        "current_unresolved_failure_count": current.get("unresolved_failure_count").and_then(Value::as_u64).unwrap_or(0),
        "current_evidence_gap_event_count": current.get("evidence_gap_event_count").and_then(Value::as_u64).unwrap_or(0),
        "stale_failure_count": current.get("stale_failure_count").and_then(Value::as_u64).unwrap_or(0),
        "cargo_test_zero_tests_run": validation_has_cargo_test_zero_tests(validation),
    })
}

pub(super) fn finish_suggested_next_actions(output: &Value) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };
    let tool_failures = output.get("tool_failures").unwrap_or(&Value::Null);
    let expectation_mismatch_count = tool_failures
        .get("expectation_mismatch_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let unexpected_success_count = tool_failures
        .get("unexpected_success_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    if actionable_unexpected_failure_count(tool_failures) > 0 {
        push(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    }
    if expectation_mismatch_count > 0 {
        push(
            &mut actions,
            "review result expectation mismatches before proceeding",
        );
    }
    if unexpected_success_count > 0 {
        push(
            &mut actions,
            "review failure expectations that unexpectedly succeeded",
        );
    }
    if output
        .get("workspace")
        .and_then(|workspace| workspace.get("clean"))
        .and_then(Value::as_bool)
        == Some(false)
    {
        if output
            .pointer("/changes/show_changes/diff_review_handoff/next_call/tool")
            .and_then(Value::as_str)
            == Some("git_diff_hunks")
        {
            push(
                &mut actions,
                "continue the review with review_changes when its continuation is available",
            );
        } else {
            push(&mut actions, "review workspace changes with review_changes");
        }
    }
    if output
        .get("jobs")
        .and_then(|jobs| jobs.get("blocking_active_count"))
        .and_then(Value::as_u64)
        .unwrap_or(0)
        > 0
    {
        push(&mut actions, "stop or await blocking active jobs");
    }
    if validation_has_cargo_test_zero_tests(output.get("validation").unwrap_or(&Value::Null)) {
        push(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
    }
    actions
}

fn append_hygiene_warnings(hygiene: &Value, warnings: &mut Vec<Value>) {
    let finding_count = hygiene
        .get("counts")
        .and_then(|counts| counts.get("findings"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    if finding_count > 0 {
        warnings.push(json!({
            "kind": "workspace_hygiene_findings",
            "findings": finding_count,
            "message": "workspace hygiene findings should be reviewed",
        }));
    }
}

const FINISH_SESSION_EVENT_LIMIT: usize = 200;
