use serde_json::{json, Value};

use crate::auth::AuthContext;
use webcodex_tool_contracts::tool_call::GitReviewScopeInput;

use super::git_review_snapshot::{
    caller_fingerprint, get_snapshot, insert_snapshot, GitReviewScope, GitReviewSnapshot,
    GitReviewSourceIdentity,
};
use super::{ToolResult, ToolRuntime};

const REVIEW_CHANGES_CONTINUATION_PREFIX: &str = "wcrc1.";

fn review_changes_failure(project: &str, reason_code: &'static str) -> ToolResult {
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

fn internal_scope_matches_input(scope: &GitReviewScope, input: &GitReviewScopeInput) -> bool {
    match (scope, input) {
        (GitReviewScope::Workspace, GitReviewScopeInput::Workspace) => true,
        (
            GitReviewScope::Committed {
                requested_base,
                requested_head,
                ..
            },
            GitReviewScopeInput::Committed {
                base_commit,
                head_commit,
            },
        ) => {
            requested_base.eq_ignore_ascii_case(base_commit)
                && requested_head.eq_ignore_ascii_case(head_commit)
        }
        _ => false,
    }
}

fn source_from_review_summary(summary: &Value) -> Option<GitReviewSourceIdentity> {
    Some(GitReviewSourceIdentity::Committed {
        requested_base: summary
            .pointer("/scope/requested_base")?
            .as_str()?
            .to_string(),
        requested_head: summary
            .pointer("/scope/requested_head")?
            .as_str()?
            .to_string(),
        merge_base: summary.pointer("/scope/merge_base")?.as_str()?.to_string(),
    })
}

fn committed_diff_matches_source(diff: &Value, source: &GitReviewSourceIdentity) -> bool {
    let GitReviewSourceIdentity::Committed {
        requested_base,
        requested_head,
        merge_base,
    } = source
    else {
        return false;
    };
    diff.pointer("/scope/requested_base").and_then(Value::as_str) == Some(requested_base.as_str())
        && diff.pointer("/scope/requested_head").and_then(Value::as_str)
            == Some(requested_head.as_str())
        && diff.pointer("/scope/merge_base").and_then(Value::as_str) == Some(merge_base.as_str())
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
            let Some(snapshot) = get_snapshot(
                snapshot_id,
                &caller,
                &resolved_project,
                session_id.as_deref(),
            ) else {
                return review_changes_failure(&project, "snapshot_unavailable");
            };
            if snapshot.projection_identity != projection
                || !internal_scope_matches_input(&snapshot.scope, &scope_input)
            {
                return review_changes_failure(&project, "continuation_scope_mismatch");
            }
            if matches!(snapshot.scope, GitReviewScope::Workspace) {
                let current = match self
                    .workspace_review_source_identity(&resolved_project)
                    .await
                {
                    Ok(value) => value,
                    Err(_) => {
                        return review_changes_failure(
                            &project,
                            "workspace_source_identity_unavailable",
                        )
                    }
                };
                if current != snapshot.source {
                    return review_changes_failure(&project, "snapshot_stale");
                }
            }

            let (base_commit, head_commit) = match &scope_input {
                GitReviewScopeInput::Workspace => (None, None),
                GitReviewScopeInput::Committed {
                    base_commit,
                    head_commit,
                } => (Some(base_commit.clone()), Some(head_commit.clone())),
            };
            let diff = self
                .git_diff_hunks_continued_with_range_and_page_bytes(
                    resolved_project.clone(),
                    paths,
                    max_hunks,
                    max_hunk_lines,
                    max_page_bytes,
                    Some(false),
                    base_commit,
                    head_commit,
                    Some(inner.to_string()),
                )
                .await;
            if !diff.success {
                return diff;
            }
            if matches!(snapshot.scope, GitReviewScope::Committed { .. })
                && !committed_diff_matches_source(&diff.output, &snapshot.source)
            {
                return review_changes_failure(&project, "snapshot_stale");
            }
            let next = preferred_diff_continuation(&diff.output)
                .and_then(|inner| encode_review_continuation(&snapshot.snapshot_id, inner));
            let mut output = json!({
                "project": project,
                "snapshot": {
                    "snapshot_id": snapshot.snapshot_id,
                    "scope": match snapshot.scope {
                        GitReviewScope::Workspace => json!({"kind": "workspace"}),
                        GitReviewScope::Committed { ref requested_base, ref requested_head, ref merge_base } => json!({
                            "kind": "committed",
                            "requested_base": requested_base,
                            "requested_head": requested_head,
                            "merge_base": merge_base,
                        }),
                    },
                    "source": snapshot.source.presentation_value(),
                    "metadata_complete": snapshot.metadata_complete,
                    "coverage_partial": snapshot.coverage_partial,
                    "reused": true,
                },
                "diff": diff.output,
                "continuation": next,
                "reason_code": Value::Null,
            });
            if let Some(next) = output["continuation"].as_str() {
                output["next_call"] = json!({
                    "tool": "review_changes",
                    "arguments": {
                        "project": project,
                        "scope": scope_input,
                        "session_id": session_id,
                        "paths": projection["paths"],
                        "max_hunks": projection["max_hunks"],
                        "max_hunk_lines": projection["max_hunk_lines"],
                        "max_page_bytes": projection["max_page_bytes"],
                        "continuation": next,
                    }
                });
            }
            return ToolResult::ok(output);
        }

        let (internal_scope, source, summary, files, signals, coverage_partial, metadata_complete, diff) =
            match scope_input.clone() {
                GitReviewScopeInput::Workspace => {
                    let before = match self
                        .workspace_review_source_identity(&resolved_project)
                        .await
                    {
                        Ok(value) => value,
                        Err(_) => {
                            return review_changes_failure(
                                &project,
                                "workspace_source_identity_unavailable",
                            )
                        }
                    };
                    let summary_result = self
                        .show_changes(
                            resolved_project.clone(),
                            session_id.clone(),
                            Some(false),
                            None,
                            None,
                            Some(0),
                        )
                        .await;
                    if !summary_result.success {
                        return summary_result;
                    }
                    let diff = self
                        .git_diff_hunks_continued_with_range_and_page_bytes(
                            resolved_project.clone(),
                            paths.clone(),
                            max_hunks,
                            max_hunk_lines,
                            max_page_bytes,
                            Some(false),
                            None,
                            None,
                            None,
                        )
                        .await;
                    if !diff.success {
                        return diff;
                    }
                    let after = match self
                        .workspace_review_source_identity(&resolved_project)
                        .await
                    {
                        Ok(value) => value,
                        Err(_) => {
                            return review_changes_failure(
                                &project,
                                "workspace_source_identity_unavailable",
                            )
                        }
                    };
                    if before != after {
                        return review_changes_failure(&project, "snapshot_stale");
                    }
                    let summary = json!({
                        "branch": summary_result.output.get("branch").cloned().unwrap_or(Value::Null),
                        "head": summary_result.output.get("head").cloned().unwrap_or(Value::Null),
                        "counts": summary_result.output.get("counts").cloned().unwrap_or(Value::Null),
                        "diff_stat": summary_result.output.get("diff_stat").cloned().unwrap_or(Value::Null),
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
                        GitReviewScope::Workspace,
                        after,
                        summary,
                        files,
                        signals,
                        partial,
                        !partial,
                        diff.output,
                    )
                }
                GitReviewScopeInput::Committed {
                    base_commit,
                    head_commit,
                } => {
                    let summary_result = self
                        .git_review_summary(
                            resolved_project.clone(),
                            base_commit.clone(),
                            head_commit.clone(),
                        )
                        .await;
                    if !summary_result.success {
                        return summary_result;
                    }
                    let Some(source) = source_from_review_summary(&summary_result.output) else {
                        return review_changes_failure(&project, "review_metadata_malformed");
                    };
                    let internal_scope = match &source {
                        GitReviewSourceIdentity::Committed {
                            requested_base,
                            requested_head,
                            merge_base,
                        } => GitReviewScope::Committed {
                            requested_base: requested_base.clone(),
                            requested_head: requested_head.clone(),
                            merge_base: merge_base.clone(),
                        },
                        GitReviewSourceIdentity::Workspace { .. } => {
                            return review_changes_failure(&project, "review_metadata_malformed");
                        }
                    };                    let diff = self
                        .git_diff_hunks_continued_with_range_and_page_bytes(
                            resolved_project.clone(),
                            paths.clone(),
                            max_hunks,
                            max_hunk_lines,
                            max_page_bytes,
                            Some(false),
                            Some(base_commit),
                            Some(head_commit),
                            None,
                        )
                        .await;
                    if !diff.success {
                        return diff;
                    }
                    if !committed_diff_matches_source(&diff.output, &source) {
                        return review_changes_failure(&project, "snapshot_stale");
                    }
                    let partial = summary_result
                        .output
                        .pointer("/coverage/partial")
                        .and_then(Value::as_bool)
                        .unwrap_or(true);
                    (
                        internal_scope,
                        source,
                        summary_result.output.get("stats").cloned().unwrap_or(Value::Null),
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
                    )
                }
            };

        let snapshot = insert_snapshot(GitReviewSnapshot::new(
            caller,
            resolved_project,
            session_id.clone(),
            internal_scope,
            source,
            projection.clone(),
            summary.clone(),
            files.clone(),
            signals.clone(),
            coverage_partial,
            metadata_complete,
        ));
        let next = preferred_diff_continuation(&diff)
            .and_then(|inner| encode_review_continuation(&snapshot.snapshot_id, inner));
        let mut output = json!({
            "project": project,
            "snapshot": {
                "snapshot_id": snapshot.snapshot_id,
                "scope": match snapshot.scope {
                    GitReviewScope::Workspace => json!({"kind": "workspace"}),
                    GitReviewScope::Committed { ref requested_base, ref requested_head, ref merge_base } => json!({
                        "kind": "committed",
                        "requested_base": requested_base,
                        "requested_head": requested_head,
                        "merge_base": merge_base,
                    }),
                },
                "source": snapshot.source.presentation_value(),
                "metadata_complete": snapshot.metadata_complete,
                "coverage_partial": snapshot.coverage_partial,
                "reused": false,
            },
            "summary": summary,
            "files": files,
            "signals": signals,
            "diff": diff,
            "continuation": next,
            "reason_code": Value::Null,
        });
        if let Some(next) = output["continuation"].as_str() {
            output["next_call"] = json!({
                "tool": "review_changes",
                "arguments": {
                    "project": project,
                    "scope": scope_input,
                    "session_id": session_id,
                    "paths": projection["paths"],
                    "max_hunks": projection["max_hunks"],
                    "max_hunk_lines": projection["max_hunk_lines"],
                    "max_page_bytes": projection["max_page_bytes"],
                    "continuation": next,
                }
            });
        }
        ToolResult::ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn committed_scope_input_matches_only_exact_requested_range() {
        let scope = GitReviewScope::Committed {
            requested_base: "a".repeat(40),
            requested_head: "b".repeat(40),
            merge_base: "c".repeat(40),
        };
        assert!(internal_scope_matches_input(
            &scope,
            &GitReviewScopeInput::Committed {
                base_commit: "A".repeat(40),
                head_commit: "B".repeat(40),
            }
        ));
        assert!(!internal_scope_matches_input(
            &scope,
            &GitReviewScopeInput::Workspace
        ));
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
