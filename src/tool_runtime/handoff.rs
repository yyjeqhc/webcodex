//! `read_session_handoff` — read-only structured handoff for degraded or
//! contaminated execution context recovery (GPT long-task window routed to a
//! degraded/contaminated context, context pollution, or continuing in a fresh
//! window), multi-agent, and multi-window scenarios.
//!
//! Aggregates session info, message-board state, recent progress/decisions,
//! open todos/risks/questions/guidance, recent failed tool calls, and optional
//! workspace + checkpoint metadata. Never calls an LLM; never generates
//! natural-language summaries. Output is always bounded and never includes
//! full diffs, file contents, stdout/stderr bodies, validation commands,
//! secrets, tokens, or raw session input payloads.

use super::closeout_evidence::{CloseoutSessionEvidence, CLOSEOUT_SESSION_EVENT_LIMIT};
use super::closeout_facts::{
    CloseoutFacts, Observation, ValidationFacts, ValidationStatus, WorkspaceFacts,
};
use super::closeout_projection::{install_closeout_decision, workspace_observation};
use super::continuation_feedback::{
    continuation_feedback_value, continuation_projection_hooks, ContinuationFeedbackInput,
    ContinuationToolFailureSnapshot,
};
use super::handoff_brief::{build_handoff_brief, HandoffBriefInput};
use super::jobs::ActiveJobsSummary;
use super::permissions::permission_summary_from_events;
use super::session_context::{session_project_mismatch_result, SessionProjectMismatch};
use super::sessions::canonical_tool_call_finished_events;
use super::sessions::{SessionDiscussionCounts, SessionDiscussionSummary, SessionMessage};
use super::tool_result::ToolResult;
use super::ToolRuntime;
use crate::auth::AuthContext;
use serde_json::{json, Value};
use std::collections::HashSet;

pub(crate) use webcodex_workflow_session::closeout_work_projection;

pub(super) const DEFAULT_HANDOFF_LIMIT: usize = 20;
pub(super) use super::closeout_facts::UNPROVEN_SOURCE_REVIEW_ACTION;
const MAX_HANDOFF_LIMIT: usize = 100;
const MAX_RECENT_FAILED_TOOLS: usize = 10;
const MAX_RECENT_PROGRESS: usize = 10;
const MAX_RECENT_DECISIONS: usize = 10;
const MAX_OPEN_ITEMS: usize = 20;
#[cfg(feature = "workspace-checkpoints")]
const MAX_RECENT_CHECKPOINTS: usize = 10;
const HANDOFF_MESSAGE_CHARS: usize = 240;

pub(crate) use super::closeout_facts::VALIDATION_IDENTITY_REUSE_ACTION;

fn workspace_continuity_projection(
    workspace: &Value,
    session_changed_paths: &[Value],
    history_complete: bool,
) -> Value {
    let session_paths = session_changed_paths
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let current_paths = workspace
        .get("files")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("path").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    let current_total = workspace
        .get("files_total")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(current_paths.len());
    let files_returned = workspace
        .get("files_returned")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(current_paths.len());
    let files_truncated = workspace
        .get("files_truncated")
        .and_then(Value::as_bool)
        .unwrap_or(current_total != files_returned);
    let git_available = workspace
        .get("git_available")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let clean = workspace
        .get("clean")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let overlap_count = current_paths.intersection(&session_paths).count();
    let unattributed_paths_count = current_paths.len().saturating_sub(overlap_count);
    let complete_workspace_paths =
        git_available && !files_truncated && current_total == current_paths.len();

    let status = if !history_complete || !complete_workspace_paths {
        "unproven"
    } else if current_paths.is_empty() {
        if clean {
            "clean"
        } else {
            "unproven"
        }
    } else if clean {
        "unproven"
    } else if overlap_count == current_paths.len() {
        "consistent_with_session_history"
    } else if overlap_count > 0 {
        "partially_attributed"
    } else {
        "unattributed"
    };

    json!({
        "status": status,
        "current_dirty_paths_count": current_paths.len(),
        "session_changed_paths_count": session_paths.len(),
        "overlap_count": overlap_count,
        "unattributed_paths_count": unattributed_paths_count,
    })
}

#[cfg(test)]
pub(crate) fn workspace_continuity_projection_for_test(
    workspace: &Value,
    session_changed_paths: &[Value],
    history_complete: bool,
) -> Value {
    workspace_continuity_projection(workspace, session_changed_paths, history_complete)
}

pub(crate) struct HandoffObservation {
    pub(crate) result: ToolResult,
    pub(crate) evidence: CloseoutSessionEvidence,
    pub(crate) jobs: ActiveJobsSummary,
    pub(crate) external_observations: Value,
}

impl ToolRuntime {
    pub(crate) async fn session_handoff_summary(
        &self,
        session_id: String,
        project: Option<String>,
        include_workspace: Option<bool>,
        include_checkpoints: Option<bool>,
        include_validation: Option<bool>,
        diagnostic: bool,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match self
            .session_handoff_observation(
                session_id,
                project,
                include_workspace,
                include_checkpoints,
                include_validation,
                diagnostic,
                limit,
                auth,
            )
            .await
        {
            Ok(observation) => observation.result,
            Err(result) => result,
        }
    }

    pub(crate) async fn session_handoff_observation(
        &self,
        session_id: String,
        project: Option<String>,
        include_workspace: Option<bool>,
        include_checkpoints: Option<bool>,
        include_validation: Option<bool>,
        diagnostic: bool,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> Result<HandoffObservation, ToolResult> {
        let limit = limit
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_HANDOFF_LIMIT)
            .min(MAX_HANDOFF_LIMIT);
        let include_workspace = include_workspace.unwrap_or(true);
        let include_checkpoints = include_checkpoints.unwrap_or(true);
        #[cfg(not(feature = "workspace-checkpoints"))]
        let _ = include_checkpoints;
        let include_validation = include_validation.unwrap_or(true);

        let authorized_target = match self
            .authorize_session_target(&session_id, "read_session_handoff", auth)
            .await
        {
            Ok(resolved) => resolved,
            Err(result) => return Err(result),
        };
        let session_project = self
            .sessions
            .session_project(&session_id)
            .expect("authorized Workflow Session must still exist");
        let requested_project = if let Some(request_project) = project
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
        {
            let requested = match self
                .resolve_project_input_for_auth(request_project, auth)
                .await
            {
                Ok(resolved) => resolved,
                Err(err) => return Err(err.into_tool_result()),
            };
            if let Some(session_project) = session_project.as_deref() {
                if session_project != requested.resolved_id {
                    return Err(session_project_mismatch_result(
                        &session_id,
                        "read_session_handoff",
                        &SessionProjectMismatch {
                            session_project: session_project.to_string(),
                            request_project: requested.resolved_id,
                        },
                    ));
                }
            }
            Some(requested)
        } else {
            None
        };

        // The exact authorized business Session owns recovery. Recorder identity
        // never selects its Project or changes its evidence. Local/dev authority
        // deliberately returns no resolved target, so independently resolve the
        // already-authorized stored Project instead of treating it as unscoped.
        let project = if let Some(requested) = requested_project {
            Some(requested.resolved_id)
        } else if let Some(target) = authorized_target {
            Some(target.resolved_id)
        } else {
            // The Session project was canonicalized and authority-fenced when the
            // Session was created. Concrete workspace/Job reads keep their own
            // normal project resolution and authorization; do not add a second
            // registry-availability precondition merely to choose the recovery
            // target here.
            session_project
        };
        let observed_revision = self.sessions.handoff_revision(&session_id);

        // --- session basic info + display-bounded events ---
        let summary = match self.sessions.summary(&session_id, Some(limit)) {
            Some(summary) => summary,
            None => {
                return Err(super::session_context::absent_workflow_session_result(
                    &self.sessions,
                    &session_id,
                    auth,
                ));
            }
        };
        // Canonical closeout evidence must not depend on the caller's display
        // limit. Reuse one fixed bounded Session snapshot for validation,
        // tool-failure actionability, review evidence, and outcome projection.
        let closeout_session = self
            .sessions
            .summary(&session_id, Some(CLOSEOUT_SESSION_EVENT_LIMIT))
            .unwrap_or_else(|| summary.clone());
        // External observations live in an intentionally separate evidence plane,
        // so capture their own bounded snapshot inside the handoff window. The
        // final comparison below detects accepted reports that arrive while the
        // remaining workspace/Job/validation snapshots are assembled.
        let external_observations =
            self.handoff_external_observations(&session_id, summary.project.as_deref());

        // --- message board state ---
        let (discussion, guidance_available) =
            match self.sessions.discussion_summary(&session_id, Some(limit)) {
                Ok(discussion) => (discussion, true),
                Err(_) => {
                    // UnknownSession is already caught by summary() above; any
                    // other error is treated as an empty board.
                    (
                        SessionDiscussionSummary {
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
                        },
                        false,
                    )
                }
            };

        // --- recent failed tool calls (from finished events) ---
        let recent_failed_tools: Vec<Value> = canonical_tool_call_finished_events(&summary.events)
            .into_iter()
            .filter(|event| event.status.as_deref() == Some("failed"))
            .rev()
            .take(MAX_RECENT_FAILED_TOOLS)
            .map(|event| {
                json!({
                    "tool_name": event.tool_name,
                    "error_kind": event.error_kind,
                    "failure_kind": event.failure_kind,
                    "created_at": event.timestamp,
                    "write_like": event.write_like,
                    "job_like": event.shell_like,
                })
            })
            .collect();

        let failed_tool_calls = recent_failed_tools.len();

        let open_todos = bound_messages(&discussion.open_todos, MAX_OPEN_ITEMS);
        let high_priority_open_todos =
            bound_messages(&discussion.high_priority_open_todos, MAX_OPEN_ITEMS);
        let open_risks = bound_messages(&discussion.open_risks, MAX_OPEN_ITEMS);
        let open_questions = bound_messages(&discussion.open_questions, MAX_OPEN_ITEMS);
        let open_guidance = bound_messages(&discussion.open_guidance, MAX_OPEN_ITEMS);
        let recent_answers = bound_messages(&discussion.recent_answers, MAX_RECENT_PROGRESS);
        let recent_completions = discussion
            .recent_completions
            .iter()
            .take(MAX_RECENT_PROGRESS)
            .cloned()
            .collect::<Vec<_>>();
        let recent_progress = bound_messages(&discussion.recent_progress, MAX_RECENT_PROGRESS);
        let recent_decisions = bound_messages(&discussion.recent_decisions, MAX_RECENT_DECISIONS);

        let counts = json!({
            "events": summary.events.len(),
            "failed_tool_calls": failed_tool_calls,
            "messages": discussion.counts.total,
            "open_todos": discussion.counts.open_todos,
            "open_risks": discussion.counts.open_risks,
            "open_questions": discussion.counts.open_questions,
            "open_guidance": discussion.counts.open_guidance,
        });

        let mut warnings = Vec::new();
        let jobs_project = match project
            .as_deref()
            .map(str::trim)
            .filter(|project| !project.is_empty())
        {
            Some(project) => self
                .resolve_project_input_for_auth(project, auth)
                .await
                .map(|resolved| resolved.resolved_id)
                .unwrap_or_else(|_| project.to_string()),
            None => summary.project.clone().unwrap_or_default(),
        };
        let jobs_project = (!jobs_project.is_empty()).then_some(jobs_project);
        let job_snapshot = self
            .active_jobs_snapshot(jobs_project.as_deref(), Some(&summary.session_id), auth, 10)
            .await;
        let jobs = job_snapshot.to_value();
        if let Some(job_warnings) = jobs.get("warnings").and_then(Value::as_array) {
            warnings.extend(job_warnings.iter().cloned());
        }

        let session_ref = self.session_reference_for_id(&summary.session_id, auth);
        let mut output = json!({
            "session_id": summary.session_id,
            "project": summary.project,
            "title": summary.title,
            "mode": summary.mode,
            "guards": summary.guards,
            "execution_context": summary.execution_context,
            "lifecycle": summary.lifecycle,
            "created_at": summary.created_at,
            "updated_at": summary.updated_at,
            "counts": counts,
            "permissions": permission_summary_from_events(&summary.events, DEFAULT_HANDOFF_LIMIT),
            "open_todos": open_todos,
            "high_priority_open_todos": high_priority_open_todos,
            "recent_answers": recent_answers,
            "recent_completions": recent_completions,
            "open_risks": open_risks,
            "open_questions": open_questions,
            "open_guidance": open_guidance,
            "recent_progress": recent_progress,
            "recent_decisions": recent_decisions,
            "recent_failed_tools": recent_failed_tools,
            "jobs": jobs,
            "warnings": warnings,
        });

        if let Some(session_ref) = session_ref.as_deref() {
            output["session_ref"] = json!(session_ref);
        }

        // Keep observation success independent of the bounded workspace projection.
        let mut closeout_workspace = Observation::NotChecked;
        // --- optional workspace summary ---
        let has_project = project
            .as_deref()
            .map(|p| !p.trim().is_empty())
            .unwrap_or(false);
        if has_project && include_workspace {
            let project = project.clone().unwrap_or_default();
            let (continuity_changed_paths, history_complete) = self
                .sessions
                .retained_changed_path_evidence(&session_id)
                .map(|(paths, complete)| {
                    (
                        paths.into_iter().map(Value::String).collect::<Vec<_>>(),
                        complete,
                    )
                })
                .unwrap_or_else(|| (Vec::new(), false));
            let (workspace, continuity, observation) = self
                .handoff_workspace_summary(&project, &continuity_changed_paths, history_complete)
                .await;
            closeout_workspace = observation;
            output["workspace"] = workspace;
            output["workspace_continuity"] = continuity;
        }

        // --- optional checkpoint candidates ---
        #[cfg(feature = "workspace-checkpoints")]
        if has_project && include_checkpoints {
            let project = project.clone().unwrap_or_default();
            let checkpoints = self.handoff_checkpoint_summary(&project, limit).await;
            output["checkpoints"] = checkpoints;
        }

        // --- optional ledger-derived validation summary ---
        // Validation shares the independent fixed closeout snapshot above, so
        // caller display limits cannot shrink attempt-boundary detection or any
        // canonical closeout classification. When validation is not requested,
        // continuation feedback reports it as explicitly unavailable rather than
        // masquerading as `not_run`.
        let evidence = self
            .observe_closeout_session_evidence(
                closeout_session,
                observed_revision,
                include_validation,
                DEFAULT_HANDOFF_LIMIT,
                auth,
            )
            .await;
        let projection_closeout_session = &evidence.summary;
        let feedback_validation = if include_validation {
            evidence.validation.to_value()
        } else {
            json!({"available": false, "not_requested": true})
        };
        let tool_failures = evidence.failures.with_display_events(&summary.events);
        output["expected_failed_tool_calls"] = output_recent(&tool_failures, "recent_expected");
        output["unexpected_failed_tool_calls"] = output_recent(&tool_failures, "recent_unexpected");
        output["expectation_mismatches"] = output_recent(&tool_failures, "recent_mismatches");
        output["unexpected_success_tool_calls"] =
            output_recent(&tool_failures, "recent_unexpected_successes");
        output["tool_failures"] = tool_failures;
        output["review_evidence"] = evidence.review.to_value();
        if include_validation {
            output["validation"] = feedback_validation.clone();
        }
        let (work_performed, changed_paths) = closeout_work_projection(&summary.events);
        output["work_performed"] = work_performed;
        output["changed_paths"] = changed_paths;
        let mut facts = evidence.facts(closeout_workspace, None, job_snapshot.facts());
        if !include_validation {
            facts.validation = ValidationFacts {
                status: Some(ValidationStatus::NotRun),
                ..Default::default()
            };
        }

        // Continuation feedback: a read-only attempt-summary + validation-delta
        // projection reused across handoff, finish, and start. Built from the
        // independent bounded evidence snapshot, validation value, and job
        // metadata already gathered here; never re-runs validation, mutates the
        // ledger, refreshes activity, or consumes guidance.
        output["continuation_feedback"] = continuation_feedback_value(ContinuationFeedbackInput {
            session_summary: &projection_closeout_session,
            validation: &feedback_validation,
            jobs: job_snapshot.continuation_snapshot(),
            discussion: &discussion,
            continuation: "continued",
            suggest_exploration_continuity: false,
            workspace_conflicts: facts.workspace_conflicts().is_some_and(|count| count > 0),
            hooks: continuation_projection_hooks(),
            current_validation: evidence
                .validation
                .continuation_snapshot(include_validation),
            tool_failures: ContinuationToolFailureSnapshot::new(
                &evidence.failures.actionable_unexpected_event_ids,
            ),
        });

        let external_observations_changed_during_snapshot = external_observations
            != self.handoff_external_observations(
                &session_id,
                projection_closeout_session.project.as_deref(),
            );
        let session_changed_during_snapshot = observed_revision.is_none()
            || observed_revision != self.sessions.handoff_revision(&session_id);
        if session_changed_during_snapshot {
            if let Some(continuity) = output
                .get_mut("workspace_continuity")
                .and_then(Value::as_object_mut)
            {
                continuity.insert("status".to_string(), json!("unproven"));
            }
        }

        // --- bounded suggested next actions ---
        output["suggested_next_actions"] =
            json!(handoff_suggested_next_actions(&output, &facts, &discussion));
        if let Some(goal_context) = self.recovery_goal_context_for_session(auth, &session_id) {
            output["goal_context"] = goal_context;
        }
        output["handoff_brief"] = build_handoff_brief(HandoffBriefInput {
            session_summary: &projection_closeout_session,
            discussion: guidance_available.then_some(&discussion),
            continuation_feedback: output.get("continuation_feedback").unwrap_or(&Value::Null),
            workspace_requested: include_workspace,
            workspace: output.get("workspace"),
            validation_requested: include_validation,
            validation: Some(&feedback_validation),
            jobs: output.get("jobs"),
            external_observations: Some(&external_observations),
            guidance_available,
            session_changed_during_snapshot,
            external_observations_changed_during_snapshot,
            existing_suggested_actions: output.get("suggested_next_actions"),
        });

        let retained_outputs = super::task_outputs::retained_task_outputs(
            &projection_closeout_session,
        )
        .and_then(|value| {
            serde_json::from_value::<webcodex_core::task_outputs::TaskOutputs>(value).ok()
        });
        output["acceptance"] = super::acceptance_evidence::acceptance_evidence(
            &facts,
            super::acceptance_evidence::AcceptanceContext {
                outputs: retained_outputs.as_ref(),
                outputs_historical: true,
                history_partial: projection_closeout_session.events_truncated
                    || projection_closeout_session.retention_truncated,
                snapshot_changed: session_changed_during_snapshot
                    || external_observations_changed_during_snapshot,
            },
        );
        if !diagnostic {
            let mut handoff = json!({
                "session_id": output["session_id"],
                "project": output["project"],
                "handoff_brief": output["handoff_brief"],
                "acceptance": output["acceptance"],
            });
            if let Some(session_ref) = output.get("session_ref") {
                handoff["session_ref"] = session_ref.clone();
            }
            if let Some(workspace_continuity) = output.get("workspace_continuity") {
                handoff["workspace_continuity"] = workspace_continuity.clone();
            }
            if let Some(goal_context) = output.get("goal_context") {
                handoff["goal_context"] = goal_context.clone();
            }
            return Ok(HandoffObservation {
                result: ToolResult::ok(handoff),
                evidence,
                jobs: job_snapshot,
                external_observations,
            });
        }
        let snapshot_changed =
            session_changed_during_snapshot || external_observations_changed_during_snapshot;
        if snapshot_changed {
            // Keep the diagnostic actions aligned with the bounded brief. The
            // captured facts remain useful history, but cannot authorize a next
            // task step until the exact Session has been observed again.
            let recovery_actions = output["handoff_brief"]["next_actions"].clone();
            output["suggested_next_actions"] = recovery_actions.clone();
            if let Some(actions) =
                output.pointer_mut("/continuation_feedback/attempt/suggested_next_actions")
            {
                *actions = recovery_actions;
            }
        }
        let mut compact = compact_handoff_output(&output, &facts);
        if snapshot_changed {
            // These optional aggregate completion conclusions require a coherent
            // snapshot. Preserve facts, blockers, history and integrity without
            // presenting the stale closeout decision as a current verdict.
            let compact = compact.as_object_mut().expect("handoff projection object");
            compact.remove("verdict");
            compact.remove("task_outcome");
        }
        for (key, value) in compact.as_object().unwrap() {
            if !include_validation && key == "validation" {
                continue;
            }
            output
                .as_object_mut()
                .unwrap()
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        Ok(HandoffObservation {
            result: ToolResult::ok(output),
            evidence,
            jobs: job_snapshot,
            external_observations,
        })
    }

    /// Build a bounded workspace summary reusing the read-only `read_workspace_changes`
    /// git inspection path. Returns only clean/branch/head/counts/warnings/
    /// suggested_next_actions — never hunks, full diffs, or file contents.
    async fn handoff_workspace_summary(
        &self,
        project: &str,
        session_changed_paths: &[Value],
        history_complete: bool,
    ) -> (Value, Value, Observation<WorkspaceFacts>) {
        let show_result = self
            .show_changes(project.to_string(), None, Some(false), None, None, None)
            .await;
        if !show_result.success {
            // Non-git project or git failure: do not fail the whole handoff.
            // Surface a structured warning instead.
            let mut warnings: Vec<Value> = show_result
                .output
                .get("warnings")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            warnings.push(json!({
                "kind": "git_unavailable",
                "message": "git-backed workspace inspection unavailable; project may not be a git repository",
            }));
            return (
                json!({
                    "project": project,
                    "git_available": false,
                    "non_git_project": show_result.output.get("non_git_project").cloned().unwrap_or(json!(false)),
                    "clean": true,
                    "branch": null,
                    "head": null,
                    "changed_files_count": 0,
                    "counts": {},
                    "warnings": json!(warnings),
                    "suggested_next_actions": [],
                }),
                workspace_continuity_projection(&show_result.output, session_changed_paths, false),
                Observation::Failed,
            );
        }
        let counts = show_result
            .output
            .get("counts")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let changed_files_count = counts
            .as_object()
            .map(|obj| {
                let modified = obj.get("modified").and_then(Value::as_u64).unwrap_or(0);
                let added = obj.get("added").and_then(Value::as_u64).unwrap_or(0);
                let deleted = obj.get("deleted").and_then(Value::as_u64).unwrap_or(0);
                let renamed = obj.get("renamed").and_then(Value::as_u64).unwrap_or(0);
                let copied = obj.get("copied").and_then(Value::as_u64).unwrap_or(0);
                let untracked = obj.get("untracked").and_then(Value::as_u64).unwrap_or(0);
                let conflicted = obj.get("conflicted").and_then(Value::as_u64).unwrap_or(0);
                modified + added + deleted + renamed + copied + untracked + conflicted
            })
            .unwrap_or(0);

        // Carry warnings from show_changes and add a handoff-specific warning
        // when git is unavailable so the receiver immediately sees the gap.
        let mut warnings: Vec<Value> = show_result
            .output
            .get("warnings")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let git_available = show_result
            .output
            .get("git_available")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if !git_available {
            warnings.push(json!({
                "kind": "git_unavailable",
                "message": "git-backed workspace inspection unavailable; project may not be a git repository",
            }));
        }

        let observation = workspace_observation(&show_result.output, true, true);
        let continuity = workspace_continuity_projection(
            &show_result.output,
            session_changed_paths,
            history_complete,
        );
        (
            json!({
                "project": project,
                "git_available": json!(git_available),
                "non_git_project": show_result.output.get("non_git_project").cloned().unwrap_or(json!(false)),
                "clean": show_result.output.get("clean").cloned().unwrap_or(json!(true)),
                "branch": show_result.output.get("branch").cloned().unwrap_or(Value::Null),
                "head": show_result.output.get("head").cloned().unwrap_or(Value::Null),
                "changed_files_count": changed_files_count,
                "counts": counts,
                "warnings": json!(warnings),
                "suggested_next_actions": show_result.output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
            }),
            continuity,
            observation,
        )
    }

    /// Build a bounded checkpoint summary using the read-only
    /// `list_workspace_checkpoints` path. Returns the latest
    /// `last_known_good` checkpoint (preferring `validation_status == passed`)
    /// and a bounded recent list. Never returns validation.commands or diffs.
    #[cfg(feature = "workspace-checkpoints")]
    async fn handoff_checkpoint_summary(&self, project: &str, limit: usize) -> Value {
        let list_result = self
            .workspace_checkpoint_list(project.to_string(), Some(limit))
            .await;
        if !list_result.success {
            return json!({
                "latest_last_known_good": Value::Null,
                "recent": [],
            });
        }
        let checkpoints = list_result.output["checkpoints"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        // Find the latest last_known_good, preferring validation_status == passed.
        let mut latest_lkg: Option<&Value> = None;
        for checkpoint in &checkpoints {
            let kind = checkpoint.get("kind").and_then(Value::as_str).unwrap_or("");
            if kind != "last_known_good" {
                continue;
            }
            let candidate_status = checkpoint
                .get("validation_status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let is_passed = candidate_status == "passed";
            match &latest_lkg {
                None => {
                    latest_lkg = Some(checkpoint);
                }
                Some(current) => {
                    let current_status = current
                        .get("validation_status")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let current_passed = current_status == "passed";
                    let candidate_time = checkpoint
                        .get("created_at")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    let current_time = current
                        .get("created_at")
                        .and_then(Value::as_i64)
                        .unwrap_or(0);
                    // Prefer passed; among same pass status prefer newer.
                    if (is_passed && !current_passed)
                        || (is_passed == current_passed && candidate_time > current_time)
                    {
                        latest_lkg = Some(checkpoint);
                    }
                }
            }
        }

        let latest_lkg_value = latest_lkg
            .map(|checkpoint| {
                json!({
                    "checkpoint_id": checkpoint.get("checkpoint_id").cloned().unwrap_or(Value::Null),
                    "kind": checkpoint.get("kind").cloned().unwrap_or(Value::Null),
                    "labels": checkpoint.get("labels").cloned().unwrap_or_else(|| json!([])),
                    "validation_status": checkpoint.get("validation_status").cloned().unwrap_or(json!("unknown")),
                    "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
                    "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
                })
            })
            .unwrap_or(Value::Null);

        let recent: Vec<Value> = checkpoints
            .iter()
            .take(MAX_RECENT_CHECKPOINTS)
            .map(|checkpoint| {
                json!({
                    "checkpoint_id": checkpoint.get("checkpoint_id").cloned().unwrap_or(Value::Null),
                    "kind": checkpoint.get("kind").cloned().unwrap_or(Value::Null),
                    "validation_status": checkpoint.get("validation_status").cloned().unwrap_or(json!("unknown")),
                    "created_at": checkpoint.get("created_at").cloned().unwrap_or(Value::Null),
                    "title": checkpoint.get("title").cloned().unwrap_or(Value::Null),
                })
            })
            .collect();

        json!({
            "latest_last_known_good": latest_lkg_value,
            "recent": recent,
        })
    }
}

/// Bound a list of session messages for handoff output: limit count and
/// truncate message bodies. Never includes raw full bodies.
fn bound_messages(messages: &[SessionMessage], max_items: usize) -> Vec<Value> {
    messages
        .iter()
        .take(max_items)
        .map(|message| {
            json!({
                "message_id": message.message_id,
                "created_at": message.created_at,
                "kind": message.kind,
                "status": message.status,
                "priority": message.priority,
                "message": bound_chars(&message.message, HANDOFF_MESSAGE_CHARS),
                "tags": message.tags,
                "reply_to": message.reply_to,
                "author_session_id": message.author_session_id,
                "resolved_at": message.resolved_at,
                "resolved_by_message_id": message.resolved_by_message_id,
            })
        })
        .collect()
}

fn bound_chars(value: &str, max_chars: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in value.chars().enumerate() {
        if idx >= max_chars {
            out.push_str("...");
            return out;
        }
        out.push(ch);
    }
    out
}

fn output_recent(tool_failures: &Value, key: &str) -> Value {
    tool_failures.get(key).cloned().unwrap_or_else(|| json!([]))
}

fn compact_handoff_output(output: &Value, facts: &CloseoutFacts) -> Value {
    let workspace_clean = facts.workspace_clean();
    let workspace_conflicts = facts.workspace_conflicts().unwrap_or(0);
    let mut compact = json!({
        "diagnostic": true,
        "project": output.get("project").cloned().unwrap_or(Value::Null),
        "session_id": output.get("session_id").cloned().unwrap_or(Value::Null),
        "workspace_clean": workspace_clean,
        "workspace_conflicts": workspace_conflicts,
        "hygiene_clean": true,
        "jobs": compact_jobs(output.get("jobs").unwrap_or(&Value::Null)),
        "permissions": compact_permissions(output.get("permissions").unwrap_or(&Value::Null)),
        "tool_failures": compact_tool_failures(output.get("tool_failures").unwrap_or(&Value::Null)),
        "validation": compact_validation(output.get("validation").unwrap_or(&Value::Null)),
        "review_evidence": compact_review_evidence(output.get("review_evidence").unwrap_or(&Value::Null)),
        "collaboration": {
            "open_todo_count": output.pointer("/counts/open_todos").and_then(Value::as_u64).unwrap_or(0),
            "high_priority_open_todos": output.get("high_priority_open_todos").cloned().unwrap_or_else(|| json!([])),
            "recent_answers": output.get("recent_answers").cloned().unwrap_or_else(|| json!([])),
            "recent_completions": output.get("recent_completions").cloned().unwrap_or_else(|| json!([])),
        },
        "work_performed": output.get("work_performed").cloned().unwrap_or_else(|| json!([])),
        "changed_paths": output.get("changed_paths").cloned().unwrap_or_else(|| json!([])),
        "continuation_feedback": output
            .get("continuation_feedback")
            .cloned()
            .unwrap_or_else(|| json!({
                "status": "not_applicable",
                "reason_code": "no_continuation_evidence",
                "deterministic": true,
                "llm_summary": false,
            })),
        "handoff_brief": output.get("handoff_brief").cloned().unwrap_or(Value::Null),
        "warnings": output.get("warnings").cloned().unwrap_or_else(|| json!([])),
        "suggested_next_actions": output.get("suggested_next_actions").cloned().unwrap_or_else(|| json!([])),
    });
    let initial_actions = string_array(output.get("suggested_next_actions"));
    install_closeout_decision(&mut compact, facts, &initial_actions);
    compact
}

pub(crate) fn compact_jobs(jobs: &Value) -> Value {
    let mut compact = json!({
        "active_count": jobs.get("active_count").and_then(Value::as_u64).unwrap_or(0),
        "blocking_active_count": jobs.get("blocking_active_count").and_then(Value::as_u64).unwrap_or(0),
        "nonblocking_active_count": jobs.get("nonblocking_active_count").and_then(Value::as_u64).unwrap_or(0),
        "terminal_pending_count": jobs.get("terminal_pending_count").and_then(Value::as_u64).unwrap_or(0),
        "warnings": jobs.get("warnings").cloned().unwrap_or_else(|| json!([])),
    });
    if jobs.get("active_job").is_some_and(Value::is_object) {
        compact["active_job"] = jobs["active_job"].clone();
    }
    compact
}

pub(crate) fn compact_permissions(permissions: &Value) -> Value {
    json!({
        "required_count": permissions.get("required_count").and_then(Value::as_u64).unwrap_or(0),
        "manual_approved_count": permissions.get("manual_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "auto_approved_count": permissions.get("auto_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "total_approved_count": permissions.get("total_approved_count").and_then(Value::as_u64).unwrap_or(0),
        "hard_denied_count": permissions.get("hard_denied_count").and_then(Value::as_u64).unwrap_or(0),
    })
}

pub(crate) fn compact_tool_failures(tool_failures: &Value) -> Value {
    json!({
        "expected_count": tool_failures.get("expected_count").and_then(Value::as_u64).unwrap_or(0),
        "unexpected_count": tool_failures.get("unexpected_count").and_then(Value::as_u64).unwrap_or(0),
        "non_actionable_unexpected_count": tool_failures.get("non_actionable_unexpected_count").and_then(Value::as_u64).unwrap_or(0),
        "actionable_unexpected_count": actionable_unexpected_failure_count(tool_failures),
        "expectation_mismatch_count": tool_failures.get("expectation_mismatch_count").and_then(Value::as_u64).unwrap_or(0),
        "unexpected_success_count": tool_failures.get("unexpected_success_count").and_then(Value::as_u64).unwrap_or(0),
    })
}

pub(crate) fn compact_validation(validation: &Value) -> Value {
    if validation.is_object() {
        return validation.clone();
    }
    json!({
        "status": validation.get("status").cloned().unwrap_or_else(|| json!("not_run")),
        "reason": validation.get("reason").cloned().unwrap_or_else(|| json!("no_validation_tool_invoked")),
        "latest_status": validation
            .get("latest_status")
            .cloned()
            .unwrap_or_else(|| compact_validation_latest_status_fallback(validation)),
        "historical_failures": validation
            .get("historical_failures")
            .cloned()
            .unwrap_or_else(compact_validation_historical_failures_fallback),
        "cargo_test_zero_tests_run": validation_has_cargo_test_zero_tests(validation),
    })
}

pub(crate) fn validation_has_cargo_test_zero_tests(validation: &Value) -> bool {
    validation.get("latest_status").and_then(Value::as_str) == Some("inconclusive")
        && validation
            .get("cargo_test_zero_tests_run")
            .and_then(Value::as_bool)
            == Some(true)
}

pub(crate) fn compact_review_evidence(review_evidence: &Value) -> Value {
    let tools: Vec<Value> = review_evidence
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(Value::as_str)
                .take(20)
                .map(|tool| json!(tool))
                .collect()
        })
        .unwrap_or_default();

    json!({
        "available": review_evidence.get("available").and_then(Value::as_bool).unwrap_or(false),
        "total": review_evidence.get("total").and_then(Value::as_u64).unwrap_or(0),
        "read_only_inspection_count": review_evidence
            .get("read_only_inspection_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "search_count": review_evidence
            .get("search_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "diff_review_count": review_evidence
            .get("diff_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "workspace_review_count": review_evidence
            .get("workspace_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "hygiene_review_count": review_evidence
            .get("hygiene_review_count")
            .and_then(Value::as_u64)
            .unwrap_or(0),
        "tools": tools,
    })
}

fn compact_validation_latest_status_fallback(validation: &Value) -> Value {
    let latest_status = match validation.get("status").and_then(Value::as_str) {
        Some("passed") => "passed",
        Some("failed") => "failed",
        Some("inconclusive") => "inconclusive",
        Some("not_run") => "not_run",
        _ => "unknown",
    };
    json!(latest_status)
}

fn compact_validation_historical_failures_fallback() -> Value {
    json!({
        "count": 0,
        "resolved": false,
        "unresolved": false,
    })
}

pub(crate) fn actionable_unexpected_failure_count(tool_failures: &Value) -> u64 {
    tool_failures
        .get("actionable_unexpected_count")
        .and_then(Value::as_u64)
        .unwrap_or_else(|| count_field(tool_failures, "unexpected_count"))
}

fn count_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Build a bounded list of suggested next actions based on the handoff state.
fn handoff_suggested_next_actions(
    output: &Value,
    facts: &CloseoutFacts,
    discussion: &SessionDiscussionSummary,
) -> Vec<String> {
    let mut actions = Vec::new();
    let push = |actions: &mut Vec<String>, action: &str| {
        if !actions.iter().any(|existing| existing == action) {
            actions.push(action.to_string());
        }
    };

    if facts.failures.actionable_unexpected_count > 0 {
        push(
            &mut actions,
            "review unexpected failed tool calls before proceeding",
        );
    }
    if facts.failures.expectation_mismatch_count > 0 {
        push(
            &mut actions,
            "review result expectation mismatches before proceeding",
        );
    }
    if facts.failures.unexpected_success_count > 0 {
        push(
            &mut actions,
            "review failure expectations that unexpectedly succeeded",
        );
    }
    if discussion.counts.open_todos > 0 {
        push(&mut actions, "address open todos");
    }
    if discussion.counts.open_risks > 0 {
        push(&mut actions, "mitigate open risks");
    }
    if discussion.counts.open_questions > 0 {
        push(&mut actions, "answer open questions");
    }
    if facts
        .workspace
        .observed()
        .is_some_and(|workspace| !workspace.non_git && workspace.clean == Some(false))
    {
        push(
            &mut actions,
            "review workspace changes with read_workspace_changes",
        );
    }
    if let Some(checkpoints) = output.get("checkpoints") {
        let lkg_is_null = checkpoints
            .get("latest_last_known_good")
            .is_none_or(Value::is_null);
        if lkg_is_null {
            push(
                &mut actions,
                "consider creating a last_known_good checkpoint",
            );
        }
    }
    if facts.validation.current_unresolved_failure_count > 0 {
        push(&mut actions, VALIDATION_IDENTITY_REUSE_ACTION);
    }
    if facts.validation.cargo_test_zero_tests {
        push(
            &mut actions,
            "cargo_test ran zero tests; verify the test filter or command",
        );
    }
    if actions.is_empty() {
        push(
            &mut actions,
            "session is ready for handoff; proceed with the next task step",
        );
    }
    actions
}
