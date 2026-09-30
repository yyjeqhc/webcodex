//! Bounded startup observations over existing runtime and Session owners.

use super::projection::changed_files_count_from_counts;
use super::*;

impl ToolRuntime {
    /// Build the bounded continuation feedback projection for coding startup.
    ///
    /// Pure read-only: validation is derived from the session ledger plus
    /// process-local source-fence re-observation (no Job-status enrichment), jobs
    /// come from the bounded `active_jobs_summary` metadata, and guidance is read
    /// from the message board without marking anything read or resolved. No shell,
    /// file reads, Runner requests, or ledger mutation.
    pub(super) async fn startup_continuation_feedback(
        &self,
        summary: &sessions::SessionSummary,
        pre_instruction_summary: Option<&sessions::SessionSummary>,
        continuation_kind: &'static str,
        jobs: &Value,
        workspace_conflicts: bool,
    ) -> Value {
        // Fresh new session: nothing to continue from.
        if continuation_kind == "created" {
            return not_applicable_continuation_feedback_value("fresh_session");
        }
        // For a reused/resumed/restored session, project over the snapshot taken
        // *before* this new `task_instruction` was appended, so the feedback
        // describes the previous attempt's work rather than the empty new
        // attempt. The returned session itself still contains the new
        // instruction; only the projection uses the pre-instruction window.
        // Guidance and job state are read from the live session id at the same
        // instant; neither is mutated.
        let projection_summary = pre_instruction_summary.unwrap_or(summary);
        if projection_summary.events.is_empty() {
            return not_applicable_continuation_feedback_value("empty_session");
        }
        let projection_summary = self.refresh_validation_source_summary(projection_summary);
        let validation = crate::tool_runtime::validation_events::validation_summary_from_events(
            &projection_summary.events,
            20,
        );
        let current_validation =
            crate::tool_runtime::validation_events::current_validation_evidence_for_session(
                &projection_summary,
                20,
            );
        let raw_tool_failures = tool_failure_summary_from_events(&projection_summary.events, 10);
        let reconciliation =
            reconcile_closeout_evidence(&raw_tool_failures, &projection_summary, &validation);
        let (discussion, _) = self.discussion_snapshot(&summary.session_id);
        continuation_feedback_value(ContinuationFeedbackInput {
            session_summary: &projection_summary,
            validation: &validation,
            jobs,
            discussion: &discussion,
            continuation: continuation_kind,
            suggest_exploration_continuity: true,
            workspace_conflicts,
            hooks: continuation_projection_hooks(),
            current_validation: continuation_validation_snapshot(&current_validation),
            tool_failures: ContinuationToolFailureSnapshot::new(
                &reconciliation.actionable_unexpected_event_ids,
            ),
        })
    }

    pub(super) async fn capture_coding_git_baseline_tree(
        &self,
        project: &str,
        git: &Value,
        warnings: &mut Vec<Value>,
    ) -> Option<String> {
        if git.get("available").and_then(Value::as_bool) != Some(true) {
            return None;
        }

        let observed_head = git
            .pointer("/head/commit")
            .and_then(Value::as_str)
            .filter(|value| valid_git_object_id(value))
            .map(str::to_string);
        let head_commit = if let Some(commit) = observed_head {
            Some(commit)
        } else {
            let probe = self
                .run_internal_process_sync(
                    project.to_string(),
                    "git".to_string(),
                    vec![
                        "rev-parse".to_string(),
                        "--verify".to_string(),
                        "HEAD".to_string(),
                    ],
                    30,
                )
                .await;
            if probe.success {
                process_stdout_git_object_id(&probe)
            } else {
                None
            }
        };

        if let Some(commit) = head_commit {
            let result = self
                .run_internal_process_sync(
                    project.to_string(),
                    "git".to_string(),
                    vec![
                        "rev-parse".to_string(),
                        "--verify".to_string(),
                        format!("{commit}^{{tree}}"),
                    ],
                    30,
                )
                .await;
            if let Some(tree) = result
                .success
                .then(|| process_stdout_git_object_id(&result))
                .flatten()
            {
                return Some(tree);
            }
            warnings.push(json!({
                "kind": "git_baseline_unavailable",
                "message": "Git HEAD was observed but its baseline tree could not be resolved",
            }));
            return None;
        }

        // A Git repository with no resolvable HEAD is eligible for the unborn
        // baseline only when HEAD is still a valid symbolic ref. This keeps a
        // generic Git/read failure from being mistaken for an empty repository.
        let symbolic_head = self
            .run_internal_process_sync(
                project.to_string(),
                "git".to_string(),
                vec![
                    "symbolic-ref".to_string(),
                    "-q".to_string(),
                    "HEAD".to_string(),
                ],
                30,
            )
            .await;
        if !symbolic_head.success {
            warnings.push(json!({
                "kind": "git_baseline_unavailable",
                "message": "Git baseline could not distinguish an unborn HEAD from an unavailable HEAD",
            }));
            return None;
        }

        // Ask this exact repository to materialize its own empty tree. Do not
        // hard-code the SHA-1 empty-tree id: SHA-256 repositories use a different
        // object id. This writes only the immutable empty tree object.
        let empty_tree = self
            .run_internal_process_sync(
                project.to_string(),
                "git".to_string(),
                vec!["mktree".to_string()],
                30,
            )
            .await;
        if let Some(tree) = empty_tree
            .success
            .then(|| process_stdout_git_object_id(&empty_tree))
            .flatten()
        {
            return Some(tree);
        }
        warnings.push(json!({
            "kind": "git_baseline_unavailable",
            "message": "Unborn Git repository could not create its repository-native empty tree baseline",
        }));
        None
    }

    pub(super) async fn coding_startup_git_summary(
        &self,
        project: &str,
        include_recent_commits: bool,
    ) -> (Value, Vec<Value>) {
        let mut warnings = Vec::new();
        let mut output = json!({
            "available": false,
            "branch": Value::Null,
            "head": Value::Null,
            "clean": Value::Null,
            "changed_files_count": 0,
            "counts": {},
            "recent_commits": [],
            "warnings": [],
        });

        {
            let result = self
                .show_changes(project.to_string(), None, Some(false), None, None, None)
                .await;
            if !result.success {
                warnings.push(json!({
                    "kind": "git_status_unavailable",
                    "message": result.error,
                }));
            }
            output["available"] = json!(result
                .output
                .get("git_available")
                .and_then(Value::as_bool)
                .unwrap_or(result.success));
            output["branch"] = result.output.get("branch").cloned().unwrap_or(Value::Null);
            output["head"] = result.output.get("head").cloned().unwrap_or(Value::Null);
            output["clean"] = result.output.get("clean").cloned().unwrap_or(Value::Null);
            output["upstream_status"] = result
                .output
                .get("upstream_status")
                .cloned()
                .unwrap_or_else(|| json!("unobserved"));
            output["upstream_reason_code"] = result
                .output
                .get("upstream_reason_code")
                .cloned()
                .unwrap_or(Value::Null);
            output["upstream"] = result
                .output
                .get("upstream")
                .cloned()
                .unwrap_or(Value::Null);
            output["ahead"] = result.output.get("ahead").cloned().unwrap_or(Value::Null);
            output["behind"] = result.output.get("behind").cloned().unwrap_or(Value::Null);
            output["non_git_project"] = result
                .output
                .get("non_git_project")
                .cloned()
                .unwrap_or(json!(false));
            output["counts"] = result
                .output
                .get("counts")
                .cloned()
                .unwrap_or_else(|| json!({}));
            output["changed_files_count"] =
                json!(changed_files_count_from_counts(&output["counts"]));
            output["warnings"] = result
                .output
                .get("warnings")
                .cloned()
                .unwrap_or_else(|| json!([]));
            output["show_changes"] = result.output;
        }

        if include_recent_commits {
            let result = self
                .git_log(project.to_string(), None, Some(5), None, None)
                .await;
            if result.success {
                output["recent_commits"] = result
                    .output
                    .get("commits")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                output["recent_commits_truncated"] = result
                    .output
                    .get("truncated")
                    .cloned()
                    .unwrap_or(json!(false));
            } else {
                warnings.push(json!({
                    "kind": "recent_commits_unavailable",
                    "message": result.error,
                }));
                output["recent_commits"] = json!([]);
                output["recent_commits_truncated"] = json!(false);
            }
        } else if let Some(object) = output.as_object_mut() {
            object.remove("recent_commits");
        }

        (output, warnings)
    }

    /// Deterministic repository structure overview for the coding startup
    /// brief. Reuses the existing `project_overview` implementation and keeps
    /// every safety property: directory entries, file types, and the git
    /// tracked index only; no file bodies, no project code execution, no
    /// symlink following, no protected/sensitive/build/cache paths, and only
    /// project-relative paths are returned.
    ///
    /// The overview is routed to the owning Runner
    /// via the `file_project_overview` op with a short startup probe timeout.
    /// On timeout the request is cancelled. An optional overview failure never
    /// fails the already-legal coding task: it returns a deterministic
    /// unavailable marker and the caller surfaces a
    /// `repository_overview_unavailable` warning without leaking raw errors,
    /// absolute paths, or Runner output.
    pub(super) async fn repository_overview_for_startup(
        &self,
        resolved: &ResolvedProject,
        auth: Option<&AuthContext>,
    ) -> Value {
        let client_id = resolved.config.client_id.as_str();
        let access = crate::runner_http::runner_access_from_auth(auth);
        // The owning runner must support the structured file capability.
        if !self
            .runner_registry
            .runner_supports_for_auth(client_id, RUNNER_CAPABILITY_FILE_READ, access.as_ref())
            .await
            .unwrap_or(false)
        {
            return repository_overview_unavailable();
        }
        // Short startup probe budget, much tighter than the standalone
        // `project_overview` tool's 30s wait.
        let probe_wait_timeout = self.repository_overview_probe_timeout.as_secs().max(1);
        let (request_id, receiver) = match self
            .runner_registry
            .enqueue_file_op(
                ShellFileOpRequest {
                    op: "project_overview".to_string(),
                    client_id: client_id.to_string(),
                    path: ".".to_string(),
                    cwd: Some(resolved.config.path.clone()),
                    content: Some(
                        json!({
                            "max_depth": STARTUP_OVERVIEW_REQUEST_MAX_DEPTH,
                            "limit": STARTUP_OVERVIEW_REQUEST_LIMIT,
                        })
                        .to_string(),
                    ),
                    max_bytes: None,
                    old_text: None,
                    pattern: None,
                    expected_sha256: None,
                    expected_prefix: None,
                    start_line: None,
                    end_line: None,
                    line: None,
                    create_dirs: false,
                    wait_timeout_secs: probe_wait_timeout,
                },
                "coding_startup".to_string(),
            )
            .await
        {
            Ok(enqueued) => enqueued,
            Err(_) => return repository_overview_unavailable(),
        };
        match tokio::time::timeout(
            Duration::from_secs(probe_wait_timeout.saturating_add(2)),
            receiver,
        )
        .await
        {
            Ok(Ok(response)) if response.exit_code == Some(0) && response.error.is_none() => {
                // The Runner response is untrusted: it must parse and pass the
                // shared project-overview contract validation against the fixed
                // request bounds (root / depth 2 / limit 120). A malformed,
                // boundary-mismatched, or schema-violating payload fails closed
                // to an unavailable marker without leaking raw stdout, errors,
                // or absolute paths. The normalized result keeps only the
                // formal contract fields.
                match serde_json::from_str::<Value>(response.stdout.as_deref().unwrap_or_default())
                {
                    Ok(parsed) => match validate_project_overview_for_startup(&parsed) {
                        Ok(mut overview) => {
                            if let Some(object) = overview.as_object_mut() {
                                object.insert("status".to_string(), json!("available"));
                                object.insert("reason_code".to_string(), Value::Null);
                                object.insert(
                                    "project".to_string(),
                                    json!(resolved.resolved_id.clone()),
                                );
                            }
                            overview
                        }
                        Err(_) => repository_overview_unavailable(),
                    },
                    Err(_) => repository_overview_unavailable(),
                }
            }
            Ok(Ok(_)) => repository_overview_unavailable(),
            Ok(Err(_)) => {
                self.runner_registry.cancel_request(&request_id).await;
                repository_overview_unavailable()
            }
            Err(_) => {
                self.runner_registry.cancel_request(&request_id).await;
                repository_overview_unavailable()
            }
        }
    }
}
/// Deterministic unavailable marker for the startup repository overview. Never
/// carries raw errors, absolute paths, or Runner output.
pub(super) fn repository_overview_unavailable() -> Value {
    json!({
        "status": "unavailable",
        "reason_code": "unsupported_or_unavailable",
    })
}

/// Compact marker for the ordinary work_on_project profile. This is an
/// intentional omission, not a failed repository probe, so it must not produce
/// an unavailable warning or lower readiness.
pub(super) fn repository_overview_not_requested() -> Value {
    json!({
        "status": "unavailable",
        "reason_code": REPOSITORY_OVERVIEW_NOT_REQUESTED_REASON,
    })
}

/// Fixed startup overview request bounds. The overview is always scoped to the
/// project root with depth 2 and limit 120; a Runner response that reports a
/// different `path`, `max_depth`, or `limit` is malformed and fails closed.
const STARTUP_OVERVIEW_REQUEST_PATH: &str = "";
const STARTUP_OVERVIEW_REQUEST_MAX_DEPTH: usize = 2;
const STARTUP_OVERVIEW_REQUEST_LIMIT: usize = 120;

/// Validate a startup overview payload against the fixed request bounds using
/// the shared contract entry. Returns the normalized formal-contract payload
/// on success.
fn validate_project_overview_for_startup(payload: &Value) -> Result<Value, String> {
    crate::project_overview::validate_project_overview(
        payload,
        STARTUP_OVERVIEW_REQUEST_PATH,
        STARTUP_OVERVIEW_REQUEST_MAX_DEPTH,
        STARTUP_OVERVIEW_REQUEST_LIMIT,
    )
}

fn valid_git_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn process_stdout_git_object_id(result: &ToolResult) -> Option<String> {
    let value = result.output.get("stdout_tail")?.as_str()?.trim();
    valid_git_object_id(value).then(|| value.to_string())
}
