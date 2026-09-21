use std::time::Duration;

use serde_json::{json, Value};

use super::super::git_committed::normalize_exact_commit_id;
use super::super::helpers::{shell_escape_simple, validate_limited_cleanup_paths};
use super::super::shell::{dispatch_uncertainty_lifecycle, runner_command_lifecycle};
use super::super::tool_result::{RecoveryKind, ToolResult};
use super::super::ToolRuntime;
use crate::runner_protocol::ShellCommandExecutionState;

const GIT_COMMIT_PATHS_MAX_PATHS: usize = 32;
const GIT_COMMIT_PATH_MAX_CHARS: usize = 512;
const GIT_COMMIT_MESSAGE_MAX_CHARS: usize = 1000;
pub(crate) const GIT_COMMIT_RESULT_PREFIX: &str = "@@WEBPI_GIT_COMMIT@@";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GitCommitMarker {
    pub(crate) status: String,
    pub(crate) previous_head: Option<String>,
    pub(crate) actual_head: Option<String>,
    pub(crate) new_head: Option<String>,
}

fn validate_git_commit_paths_input(
    expected_head: &str,
    paths: &[String],
    message: &str,
) -> Result<(String, Vec<String>, String), String> {
    let expected_head = normalize_exact_commit_id(expected_head)
        .map_err(|_| "expected_head must be one exact 40-hex commit id".to_string())?;
    if paths.len() > GIT_COMMIT_PATHS_MAX_PATHS {
        return Err(format!(
            "paths may contain at most {GIT_COMMIT_PATHS_MAX_PATHS} entries"
        ));
    }
    let paths = validate_limited_cleanup_paths(paths, true)?;
    for path in &paths {
        if path.chars().count() > GIT_COMMIT_PATH_MAX_CHARS {
            return Err(format!(
                "commit path exceeds {GIT_COMMIT_PATH_MAX_CHARS} characters"
            ));
        }
        if path.chars().any(char::is_control) {
            return Err("commit paths must not contain control characters".to_string());
        }
    }
    if message.trim().is_empty() {
        return Err("message must not be empty or whitespace-only".to_string());
    }
    if message.chars().count() > GIT_COMMIT_MESSAGE_MAX_CHARS {
        return Err(format!(
            "message may contain at most {GIT_COMMIT_MESSAGE_MAX_CHARS} characters"
        ));
    }
    if message.contains('\0') {
        return Err("message must not contain NUL".to_string());
    }
    Ok((expected_head, paths, message.to_string()))
}

fn git_commit_paths_script(expected_head: &str, paths: &[String], message: &str) -> String {
    let expected = shell_escape_simple(expected_head);
    let message = shell_escape_simple(message);
    let mut path_checks = String::new();
    let mut index_adds = String::new();
    let mut requested_lines = String::new();
    let mut reset_args = String::new();
    for path in paths {
        let quoted = shell_escape_simple(path);
        path_checks.push_str(&format!(
            "if [ -d {quoted} ]; then printf '{GIT_COMMIT_RESULT_PREFIX} status=directory_path\\n'; exit 23; fi\n\
             if [ ! -e {quoted} ] && [ ! -L {quoted} ] && ! git ls-files --error-unmatch -- {quoted} >/dev/null 2>&1; then printf '{GIT_COMMIT_RESULT_PREFIX} status=missing_path\\n'; exit 24; fi\n\
             if [ -z \"$(git status --porcelain=v1 --untracked-files=all -- {quoted})\" ]; then printf '{GIT_COMMIT_RESULT_PREFIX} status=unchanged_path\\n'; exit 25; fi\n"
        ));
        index_adds.push_str(&format!(
            "if ! GIT_INDEX_FILE=\"$index_file\" git add -A -- {quoted}; then printf '{GIT_COMMIT_RESULT_PREFIX} status=stage_failed\\n'; exit 26; fi\n"
        ));
        requested_lines.push_str(&format!("printf '%s\\n' {quoted} >> \"$req_file\"\n"));
        reset_args.push(' ');
        reset_args.push_str(&quoted);
    }

    format!(
        "set -eu\n\
         export LC_ALL=C GIT_PAGER=cat GIT_TERMINAL_PROMPT=0\n\
         unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR GIT_NAMESPACE\n\
         expected={expected}\n\
         index_file=$(mktemp \"${{TMPDIR:-/tmp}}/webcodex-index.XXXXXX\")\n\
         req_file=$(mktemp \"${{TMPDIR:-/tmp}}/webcodex-req.XXXXXX\")\n\
         req_sorted=$(mktemp \"${{TMPDIR:-/tmp}}/webcodex-req-sorted.XXXXXX\")\n\
         staged_file=$(mktemp \"${{TMPDIR:-/tmp}}/webcodex-staged.XXXXXX\")\n\
         msg_file=$(mktemp \"${{TMPDIR:-/tmp}}/webcodex-msg.XXXXXX\")\n\
         rm -f \"$index_file\"\n\
         cleanup() {{ rm -f \"$index_file\" \"$req_file\" \"$req_sorted\" \"$staged_file\" \"$msg_file\"; }}\n\
         trap cleanup EXIT HUP INT TERM\n\
         actual=$(git rev-parse --verify HEAD 2>/dev/null || true)\n\
         if [ \"$actual\" != \"$expected\" ]; then printf '{GIT_COMMIT_RESULT_PREFIX} status=head_mismatch actual=%s\\n' \"$actual\"; exit 20; fi\n\
         if [ -n \"$(git diff --name-only --diff-filter=U)\" ]; then printf '{GIT_COMMIT_RESULT_PREFIX} status=conflicts\\n'; exit 21; fi\n\
         if ! git diff --cached --quiet --exit-code; then printf '{GIT_COMMIT_RESULT_PREFIX} status=existing_staged\\n'; exit 22; fi\n\
         {path_checks}\
         {requested_lines}\
         if ! GIT_INDEX_FILE=\"$index_file\" git read-tree \"$expected\"; then printf '{GIT_COMMIT_RESULT_PREFIX} status=index_init_failed\\n'; exit 26; fi\n\
         {index_adds}\
         LC_ALL=C sort -u \"$req_file\" > \"$req_sorted\"\n\
         GIT_INDEX_FILE=\"$index_file\" git -c core.quotepath=false diff --cached --name-only --no-renames \"$expected\" | LC_ALL=C sort -u > \"$staged_file\"\n\
         if ! cmp -s \"$req_sorted\" \"$staged_file\"; then printf '{GIT_COMMIT_RESULT_PREFIX} status=staged_set_mismatch\\n'; exit 27; fi\n\
         if GIT_INDEX_FILE=\"$index_file\" git diff --cached --quiet --exit-code \"$expected\"; then printf '{GIT_COMMIT_RESULT_PREFIX} status=no_changes\\n'; exit 25; fi\n\
         if [ -n \"$(git diff --name-only --diff-filter=U)\" ] || ! git diff --cached --quiet --exit-code; then printf '{GIT_COMMIT_RESULT_PREFIX} status=staged_state_changed\\n'; exit 28; fi\n\
         actual=$(git rev-parse --verify HEAD 2>/dev/null || true)\n\
         if [ \"$actual\" != \"$expected\" ]; then printf '{GIT_COMMIT_RESULT_PREFIX} status=head_changed actual=%s\\n' \"$actual\"; exit 29; fi\n\
         tree=$(GIT_INDEX_FILE=\"$index_file\" git write-tree) || {{ printf '{GIT_COMMIT_RESULT_PREFIX} status=tree_failed\\n'; exit 30; }}\n\
         printf '%s' {message} > \"$msg_file\"\n\
         new=$(git commit-tree \"$tree\" -p \"$expected\" < \"$msg_file\") || {{ printf '{GIT_COMMIT_RESULT_PREFIX} status=commit_create_failed\\n'; exit 31; }}\n\
         if ! git update-ref -m 'webcodex git_commit_paths' HEAD \"$new\" \"$expected\"; then actual=$(git rev-parse --verify HEAD 2>/dev/null || true); printf '{GIT_COMMIT_RESULT_PREFIX} status=head_update_failed actual=%s\\n' \"$actual\"; exit 32; fi\n\
         if ! git reset -q \"$new\" --{reset_args}; then printf '{GIT_COMMIT_RESULT_PREFIX} status=index_cleanup_failed previous=%s new=%s\\n' \"$expected\" \"$new\"; exit 33; fi\n\
         printf '{GIT_COMMIT_RESULT_PREFIX} status=success previous=%s new=%s\\n' \"$expected\" \"$new\"\n"
    )
}

pub(crate) fn parse_git_commit_marker(stdout: &str) -> Option<GitCommitMarker> {
    let line = stdout
        .lines()
        .rev()
        .find(|line| line.starts_with(GIT_COMMIT_RESULT_PREFIX))?;
    let mut status = None;
    let mut previous_head = None;
    let mut actual_head = None;
    let mut new_head = None;
    for field in line[GIT_COMMIT_RESULT_PREFIX.len()..].split_whitespace() {
        let (key, value) = field.split_once('=')?;
        match key {
            "status" => status = Some(value.to_string()),
            "previous" => previous_head = normalize_exact_commit_id(value).ok(),
            "actual" => actual_head = normalize_exact_commit_id(value).ok(),
            "new" => new_head = normalize_exact_commit_id(value).ok(),
            _ => {}
        }
    }
    Some(GitCommitMarker {
        status: status?,
        previous_head,
        actual_head,
        new_head,
    })
}

fn git_commit_paths_failure_output(
    expected_head: &str,
    failure_kind: &str,
    actual_head: Option<String>,
) -> Value {
    json!({
        "committed": false,
        "expected_head": expected_head,
        "previous_head": null,
        "actual_head": actual_head,
        "new_head": null,
        "committed_paths": [],
        "state_changed": false,
        "outcome_unknown": false,
        "failure_kind": failure_kind,
        "hook_policy": "bypassed_exact_tree",
    })
}

fn git_commit_paths_outcome_unknown(expected_head: &str, reason: &str) -> ToolResult {
    ToolResult::err_with_output(
        format!(
            "git_commit_paths outcome is unknown: {reason}. Do not retry the commit blindly; observe HEAD/status first."
        ),
        json!({
            "committed": null,
            "expected_head": expected_head,
            "previous_head": null,
            "actual_head": null,
            "new_head": null,
            "committed_paths": [],
            "state_changed": null,
            "outcome_unknown": true,
            "failure_kind": "outcome_unknown",
            "hook_policy": "bypassed_exact_tree",
        }),
    )
    .with_recovery(RecoveryKind::Reobserve)
}

impl ToolRuntime {
    pub(crate) async fn git_restore_paths(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let mut args = vec!["restore".to_string(), "--".to_string()];
        args.extend(paths.iter().cloned());
        let result = self
            .run_internal_process_sync(project, "git".to_string(), args, 30)
            .await;
        if result.success {
            ToolResult::ok(json!({
                "restored_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    pub(crate) async fn discard_untracked(
        &self,
        project: String,
        paths: Vec<String>,
    ) -> ToolResult {
        let paths = match validate_limited_cleanup_paths(&paths, true) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let mut args = vec!["clean".to_string(), "-f".to_string(), "--".to_string()];
        args.extend(paths.iter().cloned());
        let result = self
            .run_internal_process_sync(project, "git".to_string(), args, 30)
            .await;
        if result.success {
            ToolResult::ok(json!({
                "discarded_untracked_paths": paths,
                "command_result": result.output,
            }))
        } else {
            result
        }
    }

    pub(crate) async fn git_commit_paths(
        &self,
        project: String,
        expected_head: String,
        paths: Vec<String>,
        message: String,
    ) -> ToolResult {
        let (expected_head, paths, message) =
            match validate_git_commit_paths_input(&expected_head, &paths, &message) {
                Ok(values) => values,
                Err(error) => return ToolResult::err(error),
            };
        let proj = match self.resolve_project(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        let script = git_commit_paths_script(&expected_head, &paths, &message);
        let client_id = proj.client_id.clone();
        let (request_id, rx) = match self
            .runner_registry
            .enqueue_internal_posix_script(
                client_id,
                Some(proj.path.clone()),
                script,
                60,
                60,
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(request) => request,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    git_commit_paths_failure_output(&expected_head, "runner_rejected", None),
                )
            }
        };

        match tokio::time::timeout(Duration::from_secs(64), rx).await {
            Ok(Ok(response)) => {
                let lifecycle = runner_command_lifecycle(&response, 60);
                if lifecycle == ShellCommandExecutionState::NotStarted {
                    return ToolResult::err_with_output(
                        response
                            .error
                            .unwrap_or_else(|| "git commit request was not started".to_string()),
                        git_commit_paths_failure_output(&expected_head, "not_started", None),
                    );
                }
                if matches!(
                    lifecycle,
                    ShellCommandExecutionState::OutcomeUnknown
                        | ShellCommandExecutionState::TimedOut
                ) {
                    return git_commit_paths_outcome_unknown(
                        &expected_head,
                        response.error.as_deref().unwrap_or(
                            "Runner did not provide a trustworthy terminal commit result",
                        ),
                    );
                }

                let stdout = response.stdout.unwrap_or_default();
                let Some(marker) = parse_git_commit_marker(&stdout) else {
                    return git_commit_paths_outcome_unknown(
                        &expected_head,
                        "completed Runner response omitted the structured commit marker",
                    );
                };
                match marker.status.as_str() {
                    "success"
                        if response.exit_code == Some(0)
                            && marker.previous_head.as_deref() == Some(expected_head.as_str())
                            && marker.new_head.is_some() =>
                    {
                        let new_head = marker.new_head.expect("checked above");
                        ToolResult::ok(json!({
                            "committed": true,
                            "expected_head": expected_head,
                            "previous_head": expected_head,
                            "actual_head": new_head,
                            "new_head": new_head,
                            "committed_paths": paths,
                            "state_changed": true,
                            "outcome_unknown": false,
                            "failure_kind": null,
                            "hook_policy": "bypassed_exact_tree",
                        }))
                    }
                    "index_cleanup_failed"
                        if marker.previous_head.as_deref() == Some(expected_head.as_str())
                            && marker.new_head.is_some() =>
                    {
                        let new_head = marker.new_head.expect("checked above");
                        ToolResult::err_with_output(
                            "commit was created and HEAD advanced, but the real index could not be aligned to the new commit; do not retry the commit",
                            json!({
                                "committed": true,
                                "expected_head": expected_head,
                                "previous_head": expected_head,
                                "actual_head": new_head,
                                "new_head": new_head,
                                "committed_paths": paths,
                                "state_changed": true,
                                "outcome_unknown": false,
                                "failure_kind": "index_cleanup_failed",
                                "hook_policy": "bypassed_exact_tree",
                            }),
                        )
                        .with_recovery(RecoveryKind::Reobserve)
                    }
                    "success" | "index_cleanup_failed" => git_commit_paths_outcome_unknown(
                        &expected_head,
                        "Runner returned a mutation-like commit marker with inconsistent SHA or exit-code evidence",
                    ),
                    status => ToolResult::err_with_output(
                        format!("git_commit_paths rejected or failed: {status}"),
                        git_commit_paths_failure_output(&expected_head, status, marker.actual_head),
                    ),
                }
            }
            Ok(Err(_)) => {
                let dispatch = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if dispatch_uncertainty_lifecycle(dispatch)
                    == ShellCommandExecutionState::NotStarted
                {
                    ToolResult::err_with_output(
                        "git commit request waiter was dropped before Runner dispatch",
                        git_commit_paths_failure_output(&expected_head, "not_started", None),
                    )
                } else {
                    git_commit_paths_outcome_unknown(
                        &expected_head,
                        "git commit request waiter was dropped after dispatch may have occurred",
                    )
                }
            }
            Err(_) => {
                let dispatch = self
                    .runner_registry
                    .cancel_request_dispatch_state(&request_id)
                    .await;
                if dispatch_uncertainty_lifecycle(dispatch)
                    == ShellCommandExecutionState::NotStarted
                {
                    ToolResult::err_with_output(
                        "timed out before git commit request reached the Runner",
                        git_commit_paths_failure_output(&expected_head, "not_started", None),
                    )
                } else {
                    git_commit_paths_outcome_unknown(
                        &expected_head,
                        "timed out after Runner dispatch may have occurred",
                    )
                }
            }
        }
    }
}
