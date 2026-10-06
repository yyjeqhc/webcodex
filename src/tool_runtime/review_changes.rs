use serde_json::{json, Value};
use std::time::Instant;

use crate::auth::AuthContext;
use webcodex_tool_contracts::tool_call::GitReviewScopeInput;

use super::git_review_snapshot::{caller_fingerprint, GitReviewSnapshot};
use super::{ToolResult, ToolRuntime};

const REVIEW_CHANGES_CONTINUATION_PREFIX: &str = "wcrc1.";

pub(in crate::tool_runtime) fn review_changes_failure(
    project: &str,
    reason_code: &'static str,
) -> ToolResult {
    tracing::debug!(
        target: "webcodex::git_review",
        operation = "review_changes",
        outcome = "failure",
        project,
        reason_code,
        snapshot_stale = reason_code == "snapshot_stale",
        "Git review workflow failed"
    );
    ToolResult::err_with_output(
        format!("review_changes failed: {reason_code}"),
        json!({
            "project": project,
            "reason_code": reason_code,
            "snapshot": null,
            "diff": null,
            "continuation": null,
        }),
    )
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn trace_review_changes_success(
    output: &Value,
    started: Instant,
    scope_kind: &'static str,
    page_kind: &'static str,
    snapshot_reused: bool,
    metadata_duration_ms: u64,
    diff_page_duration_ms: u64,
    git_internal_observation_count: u64,
) {
    let response_bytes = crate::json_measurement::serialized_json_len(output)
        .map(|bytes| u64::try_from(bytes).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let files_count = output
        .get("files")
        .and_then(Value::as_array)
        .map(|files| u64::try_from(files.len()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let diff = output.get("diff").unwrap_or(&Value::Null);
    let hunk_count = diff.get("hunk_count").and_then(Value::as_u64).unwrap_or(0);
    let truncated = diff
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || output
            .pointer("/snapshot/coverage_partial")
            .and_then(Value::as_bool)
            .unwrap_or(false);
    tracing::debug!(
        target: "webcodex::git_review",
        operation = "review_changes",
        outcome = "success",
        scope = scope_kind,
        page = page_kind,
        snapshot_reused,
        response_bytes,
        elapsed_ms = elapsed_ms(started),
        metadata_duration_ms,
        diff_page_duration_ms,
        git_internal_observation_count,
        files_count,
        hunk_count,
        truncated,
        has_continuation = output.get("continuation").is_some_and(|value| !value.is_null()),
        "Git review workflow completed"
    );
}

fn projection_identity(
    scope: &GitReviewScopeInput,
    paths: &Option<Vec<String>>,
    max_hunks: Option<usize>,
    max_hunk_lines: Option<usize>,
    max_page_bytes: Option<usize>,
) -> Value {
    json!({
        "scope": scope,
        "paths": paths,
        "max_hunks": max_hunks,
        "max_hunk_lines": max_hunk_lines,
        "max_page_bytes": max_page_bytes,
    })
}

fn review_next_call(
    project: &str,
    session_id: Option<&str>,
    projection: &Value,
    continuation: &str,
) -> Value {
    let mut arguments = projection
        .as_object()
        .expect("review projection is an object")
        .clone();
    arguments.insert("project".to_string(), json!(project));
    arguments.insert("session_id".to_string(), json!(session_id));
    arguments.insert("continuation".to_string(), json!(continuation));
    // Optional inputs are omitted, not nullable in the published tool schema.
    // Preserve explicit zero/empty values and the exact snapshot-bound scope.
    arguments.retain(|_, value| !value.is_null());
    super::SuggestedToolCall::mechanically_followable("review_changes", Value::Object(arguments))
        .to_value()
}

fn encode_review_continuation(snapshot_id: &str, inner: &str) -> Option<String> {
    let value = format!("{REVIEW_CHANGES_CONTINUATION_PREFIX}{snapshot_id}.{inner}");
    (value.len() <= 384).then_some(value)
}

fn decode_review_continuation(value: &str) -> Option<(&str, &str)> {
    let value = value.strip_prefix(REVIEW_CHANGES_CONTINUATION_PREFIX)?;
    let (snapshot_id, inner) = value.split_once('.')?;
    if snapshot_id.is_empty() || inner.is_empty() || inner.contains(char::is_whitespace) {
        return None;
    }
    Some((snapshot_id, inner))
}

fn preferred_diff_continuation(diff: &Value) -> Option<&str> {
    diff.pointer("/recovery/current_hunk/next_call/arguments/continuation")
        .and_then(Value::as_str)
        .or_else(|| {
            diff.pointer("/recovery/later_hunks/next_call/arguments/continuation")
                .and_then(Value::as_str)
        })
}

fn project_review_diff(mut diff: Value, has_continuation: bool, workspace: bool) -> Value {
    if let Some(page) = diff.as_object_mut() {
        page.remove("recovery");
        page.insert("has_more".into(), json!(has_continuation));
        if workspace {
            page.insert("basis".into(), json!("head_to_frozen_workspace"));
        }
    }
    diff
}

impl ToolRuntime {
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn review_changes(
        &self,
        project: String,
        scope_input: GitReviewScopeInput,
        session_id: Option<String>,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        max_page_bytes: Option<usize>,
        continuation: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let request_started = Instant::now();
        let page_kind = if continuation.is_some() {
            "continuation"
        } else {
            "initial"
        };
        let scope_kind = match &scope_input {
            GitReviewScopeInput::Workspace => "workspace",
            GitReviewScopeInput::Committed { .. } => "committed",
        };
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let resolved_project = resolved.resolved_id;
        let caller = match caller_fingerprint(auth) {
            Ok(value) => value,
            Err(_) => return review_changes_failure(&project, "snapshot_authority_unavailable"),
        };
        let projection = projection_identity(
            &scope_input,
            &paths,
            max_hunks,
            max_hunk_lines,
            max_page_bytes,
        );

        if let Some(continuation) = continuation {
            let Some((snapshot_id, inner)) = decode_review_continuation(&continuation) else {
                return review_changes_failure(&project, "invalid_continuation");
            };
            let Some(snapshot) = self.presentation.review_snapshot(
                snapshot_id,
                &caller,
                &resolved_project,
                session_id.as_deref(),
            ) else {
                return review_changes_failure(&project, "snapshot_unavailable");
            };
            if snapshot.projection_identity != projection {
                return review_changes_failure(&project, "continuation_scope_mismatch");
            }
            let diff_started = Instant::now();
            let diff = self
                .git_review_diff_page(
                    resolved_project.clone(),
                    paths,
                    max_hunks,
                    max_hunk_lines,
                    max_page_bytes,
                    Some(inner.to_string()),
                    &snapshot.source,
                )
                .await;
            let diff_page_duration_ms = elapsed_ms(diff_started);
            if !diff.success {
                return diff;
            }
            let next = preferred_diff_continuation(&diff.output)
                .and_then(|inner| encode_review_continuation(&snapshot.snapshot_id, inner));
            let mut output = json!({
                "project": project,
                "snapshot": {
                    "snapshot_id": snapshot.snapshot_id,
                    "scope": snapshot.source.scope_value(),
                    "source": snapshot.source.presentation_value(),
                    "metadata_complete": snapshot.metadata_complete,
                    "coverage_partial": snapshot.coverage_partial,
                    "reused": true,
                },
                "diff": project_review_diff(diff.output, next.is_some(), snapshot.source.is_workspace()),
                "continuation": next,
                "reason_code": Value::Null,
            });
            if let Some(next) = output["continuation"].as_str() {
                output["next_call"] =
                    review_next_call(&project, session_id.as_deref(), &projection, next);
            }
            trace_review_changes_success(
                &output,
                request_started,
                scope_kind,
                page_kind,
                true,
                0,
                diff_page_duration_ms,
                1,
            );
            return ToolResult::ok(output);
        }

        let (
            source,
            summary,
            files,
            signals,
            coverage_partial,
            metadata_complete,
            diff,
            metadata_duration_ms,
            diff_page_duration_ms,
            git_internal_observation_count,
        ) = match scope_input.clone() {
            GitReviewScopeInput::Workspace => {
                let metadata_started = Instant::now();
                let (before, summary_result) = match self
                    .workspace_review_metadata(&resolved_project, session_id.as_deref())
                    .await
                {
                    Ok(value) => value,
                    Err(error) => return error,
                };
                let metadata_duration_ms = elapsed_ms(metadata_started);

                let diff_started = Instant::now();
                let diff = self
                    .git_review_diff_page(
                        resolved_project.clone(),
                        paths.clone(),
                        max_hunks,
                        max_hunk_lines,
                        max_page_bytes,
                        None,
                        &before,
                    )
                    .await;
                let diff_page_duration_ms = elapsed_ms(diff_started);
                if !diff.success {
                    return diff;
                }

                let summary = json!({
                    "branch": summary_result.output.get("branch").cloned().unwrap_or(Value::Null),
                    "head": summary_result.output.get("head").cloned().unwrap_or(Value::Null),
                    "counts": summary_result.output.get("counts").cloned().unwrap_or(Value::Null),
                    "diff_stat": summary_result.output.get("diff_stat").cloned().unwrap_or(Value::Null),
                    "clean": summary_result.output.get("clean").cloned().unwrap_or(Value::Null),
                    "git_available": summary_result.output.get("git_available").cloned().unwrap_or(Value::Null),
                    "non_git_project": summary_result.output.get("non_git_project").cloned().unwrap_or(Value::Null),
                    "warnings": summary_result.output.get("warnings").cloned().unwrap_or_else(|| json!([])),
                });
                let files = summary_result
                    .output
                    .get("files")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                let signals = summary_result
                    .output
                    .pointer("/session/signals")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                let partial = summary_result
                    .output
                    .get("files_truncated")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                (
                    before,
                    summary,
                    files,
                    signals,
                    partial,
                    !partial,
                    diff.output,
                    metadata_duration_ms,
                    diff_page_duration_ms,
                    2,
                )
            }
            GitReviewScopeInput::Committed {
                base_commit,
                head_commit,
            } => {
                let metadata_started = Instant::now();
                let scope = match self
                    .resolve_committed_git_scope(&resolved_project, &base_commit, &head_commit)
                    .await
                {
                    Ok(scope) => scope,
                    Err(reason) => return review_changes_failure(&project, reason),
                };
                let summary_result = self
                    .git_review_summary_for_scope(
                        resolved_project.clone(),
                        resolved_project.clone(),
                        &scope,
                    )
                    .await;
                if !summary_result.success {
                    return summary_result;
                }
                let metadata_duration_ms = elapsed_ms(metadata_started);
                let symbol_observation_attempted = summary_result
                    .output
                    .get("files")
                    .and_then(Value::as_array)
                    .is_some_and(|files| {
                        files.iter().any(|file| {
                            matches!(
                                file.get("symbol_inspection").and_then(Value::as_str),
                                Some("inspected" | "unavailable")
                            )
                        })
                    });
                let source = super::git_review_snapshot::committed_source_identity(&scope);
                let diff_started = Instant::now();
                let diff = self
                    .git_review_diff_page(
                        resolved_project.clone(),
                        paths.clone(),
                        max_hunks,
                        max_hunk_lines,
                        max_page_bytes,
                        None,
                        &source,
                    )
                    .await;
                let diff_page_duration_ms = elapsed_ms(diff_started);
                if !diff.success {
                    return diff;
                }
                let partial = summary_result
                    .output
                    .pointer("/coverage/partial")
                    .and_then(Value::as_bool)
                    .unwrap_or(true);
                (
                    source,
                    summary_result
                        .output
                        .get("stats")
                        .cloned()
                        .unwrap_or(Value::Null),
                    summary_result
                        .output
                        .get("files")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    summary_result
                        .output
                        .get("signals")
                        .cloned()
                        .unwrap_or_else(|| json!([])),
                    partial,
                    !summary_result
                        .output
                        .get("truncated")
                        .and_then(Value::as_bool)
                        .unwrap_or(true),
                    diff.output,
                    metadata_duration_ms,
                    diff_page_duration_ms,
                    3 + u64::from(symbol_observation_attempted),
                )
            }
        };
        let snapshot = self
            .presentation
            .insert_review_snapshot(GitReviewSnapshot::new(
                caller,
                resolved_project,
                session_id.clone(),
                source,
                projection.clone(),
                summary.clone(),
                signals.clone(),
                diff.clone(),
                coverage_partial,
                metadata_complete,
            ));
        let next = preferred_diff_continuation(&diff)
            .and_then(|inner| encode_review_continuation(&snapshot.snapshot_id, inner));
        let mut output = json!({
            "project": project,
            "snapshot": {
                "snapshot_id": snapshot.snapshot_id,
                "scope": snapshot.source.scope_value(),
                "source": snapshot.source.presentation_value(),
                "metadata_complete": snapshot.metadata_complete,
                "coverage_partial": snapshot.coverage_partial,
                "reused": false,
            },
            "summary": summary,
            "files": files,
            "signals": snapshot.signals.clone(),
            "diff": project_review_diff(diff, next.is_some(), snapshot.source.is_workspace()),
            "continuation": next,
            "reason_code": Value::Null,
        });
        if let Some(next) = output["continuation"].as_str() {
            output["next_call"] =
                review_next_call(&project, session_id.as_deref(), &projection, next);
        }
        trace_review_changes_success(
            &output,
            request_started,
            scope_kind,
            page_kind,
            false,
            metadata_duration_ms,
            diff_page_duration_ms,
            git_internal_observation_count,
        );
        ToolResult::ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn review_snapshot_next_call_preserves_explicit_optional_values() {
        let projection = projection_identity(
            &GitReviewScopeInput::Workspace,
            &Some(vec![]),
            Some(0),
            Some(0),
            Some(0),
        );
        let next = review_next_call("project", Some("wc_sess_test"), &projection, "exact-token");
        let mut expected = projection;
        expected["project"] = json!("project");
        expected["session_id"] = json!("wc_sess_test");
        expected["continuation"] = json!("exact-token");
        assert_eq!(next["arguments"], expected);
        assert_eq!(next["follow_up_kind"], "mechanically_followable");
        webcodex_tool_contracts::test_support::validate_generated_tool_call_against_registered_input_schema(&next)
            .expect("review_changes next_call must pass the registered inputSchema");
    }

    #[test]
    fn continuation_round_trip_is_snapshot_bound() {
        let snapshot = format!("wc_grs_{}", "a".repeat(64));
        let inner = "wcdh2.payload";
        let encoded = encode_review_continuation(&snapshot, inner).unwrap();
        assert_eq!(
            decode_review_continuation(&encoded),
            Some((snapshot.as_str(), inner))
        );
        assert!(decode_review_continuation("wcrc1.missing-inner").is_none());
        assert!(decode_review_continuation("wcrc2.snapshot.inner").is_none());
    }

    #[test]
    fn review_projection_identity_keeps_exact_scope_and_inputs() {
        let scope = GitReviewScopeInput::Committed {
            base_commit: "a".repeat(40),
            head_commit: "b".repeat(40),
        };
        let expected = projection_identity(&scope, &None, None, None, None);
        assert_ne!(
            expected,
            projection_identity(&GitReviewScopeInput::Workspace, &None, None, None, None)
        );
        assert_ne!(
            expected,
            projection_identity(&scope, &Some(vec!["other.rs".into()]), None, None, None)
        );
    }

    #[test]
    fn preferred_diff_continuation_prefers_current_hunk_fragment() {
        let value = json!({
            "recovery": {
                "current_hunk": {
                    "next_call": {"arguments": {"continuation": "fragment"}}
                },
                "later_hunks": {
                    "next_call": {"arguments": {"continuation": "later"}}
                }
            }
        });
        assert_eq!(preferred_diff_continuation(&value), Some("fragment"));
    }
}
