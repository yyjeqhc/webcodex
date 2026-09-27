//! Runtime dispatch adapters for git-oriented tool calls.

use super::{ToolCall, ToolResult, ToolRuntime};
use crate::auth::AuthContext;

impl ToolRuntime {
    pub(crate) async fn dispatch_git_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        match call {
            ToolCall::GitRestorePaths {
                project,
                paths,
                session_id: _,
            } => self.git_restore_paths(project, paths).await,
            ToolCall::DiscardUntracked {
                project,
                paths,
                session_id: _,
            } => self.discard_untracked(project, paths).await,
            ToolCall::GitCommitPaths {
                project,
                expected_head,
                paths,
                message,
                session_id: _,
            } => {
                self.git_commit_paths(project, expected_head, paths, message)
                    .await
            }
            ToolCall::GitStatus {
                project,
                session_id: _,
            } => self.git_status(project).await,
            ToolCall::GitDiffHunks {
                project,
                session_id: _,
                paths,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                cached,
                base_commit,
                head_commit,
                continuation,
            } => {
                self.git_diff_hunks_continued_with_range_and_page_bytes(
                    project,
                    paths,
                    max_hunks,
                    max_hunk_lines,
                    max_page_bytes,
                    cached,
                    base_commit,
                    head_commit,
                    continuation,
                )
                .await
            }
            ToolCall::GitLog {
                project,
                head_commit,
                limit,
                skip,
                session_id,
            } => {
                self.git_log(project, head_commit, limit, skip, session_id)
                    .await
            }
            ToolCall::GitReviewSummary {
                project,
                base_commit,
                head_commit,
                session_id: _,
            } => {
                self.git_review_summary(project, base_commit, head_commit)
                    .await
            }
            ToolCall::ReviewChanges {
                project,
                scope,
                session_id,
                paths,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                continuation,
            } => {
                self.review_changes(
                    project,
                    scope,
                    session_id,
                    paths,
                    max_hunks,
                    max_hunk_lines,
                    max_page_bytes,
                    continuation,
                    auth,
                )
                .await
            }
            ToolCall::ShowChanges {
                project,
                session_id,
                include_diff,
                max_hunks,
                max_hunk_lines,
                session_event_limit,
            } => {
                self.show_changes(
                    project,
                    session_id,
                    include_diff,
                    max_hunks,
                    max_hunk_lines,
                    session_event_limit,
                )
                .await
            }
            _ => unreachable!("non-git tool routed to git dispatcher"),
        }
    }
}
