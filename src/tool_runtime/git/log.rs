use serde_json::{json, Map, Value};

use super::super::git_committed::normalize_exact_commit_id;
use super::super::tool_result::{SuggestedToolCall, ToolResult};
use super::super::ToolRuntime;
use super::shared::is_git_object_hex;

const DEFAULT_GIT_LOG_LIMIT: usize = 20;
const MAX_GIT_LOG_LIMIT: usize = 100;
const MAX_GIT_LOG_SKIP: usize = 10_000;
const GIT_LOG_RECORD_SEP: char = '\u{1e}';
const GIT_LOG_UNIT_SEP: char = '\u{1f}';
const GIT_LOG_SNAPSHOT_MARKER: &str = "__WEBCODEX_GIT_LOG_HEAD__=";
const GIT_LOG_SNAPSHOT_UNAVAILABLE_EXIT: i32 = 42;

pub(crate) fn normalize_git_log_limit(limit: Option<usize>) -> usize {
    limit
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_GIT_LOG_LIMIT)
        .min(MAX_GIT_LOG_LIMIT)
}

pub(crate) fn normalize_git_log_skip(skip: Option<usize>) -> usize {
    skip.unwrap_or(0).min(MAX_GIT_LOG_SKIP)
}

pub(crate) fn git_log_next_skip(
    skip: usize,
    returned_count: usize,
    truncated: bool,
) -> Option<usize> {
    if !truncated || returned_count == 0 {
        return None;
    }
    skip.checked_add(returned_count)
        .filter(|next| *next > skip && *next <= MAX_GIT_LOG_SKIP)
}

fn git_log_invocation(head: &str, limit: usize, skip: usize) -> String {
    let limit_plus_one = limit.saturating_add(1);
    format!(
        "git log --decorate=short --date=iso-strict --pretty=format:'%H%x1f%h%x1f%D%x1f%an%x1f%ae%x1f%aI%x1f%s%x1e' -n {limit_plus_one} --skip {skip} {head}",
    )
}

/// First-page command: resolve movable HEAD exactly once, then traverse only
/// that exact commit. The snapshot marker is emitted last so retained-tail
/// truncation cannot silently substitute a different snapshot identity.
pub(crate) fn git_log_command(limit: usize, skip: usize) -> String {
    let log = git_log_invocation("\"$wclog_head\"", limit, skip);
    format!(
        concat!(
            "wclog_head=$(git rev-parse --verify 'HEAD^{{commit}}' 2>/dev/null) || {{ ",
            "wclog_ref=$(git symbolic-ref -q HEAD 2>/dev/null || true); ",
            "if [ -n \"$wclog_ref\" ] && ! git show-ref --verify --quiet \"$wclog_ref\"; then ",
            "printf '{marker}unborn\\036'; exit 0; fi; ",
            "printf 'git log HEAD snapshot unavailable\\n' >&2; exit {snapshot_exit}; ",
            "}}; ",
            "{log}; wclog_status=$?; ",
            "if [ \"$wclog_status\" -ne 0 ]; then exit \"$wclog_status\"; fi; ",
            "printf '{marker}%s\\036' \"$wclog_head\""
        ),
        marker = GIT_LOG_SNAPSHOT_MARKER,
        snapshot_exit = GIT_LOG_SNAPSHOT_UNAVAILABLE_EXIT,
        log = log,
    )
}

pub(crate) fn git_log_command_at_head(head_commit: &str, limit: usize, skip: usize) -> String {
    debug_assert!(normalize_exact_commit_id(head_commit).is_ok());
    let log = git_log_invocation(head_commit, limit, skip);
    format!(
        concat!(
            "wclog_type=$(git cat-file -t {head} 2>/dev/null || true); ",
            "if [ \"$wclog_type\" != commit ]; then ",
            "printf 'git log snapshot unavailable\\n' >&2; exit {snapshot_exit}; fi; ",
            "{log}; wclog_status=$?; ",
            "if [ \"$wclog_status\" -ne 0 ]; then exit \"$wclog_status\"; fi; ",
            "printf '{marker}{head}\\036'"
        ),
        head = head_commit,
        snapshot_exit = GIT_LOG_SNAPSHOT_UNAVAILABLE_EXIT,
        log = log,
        marker = GIT_LOG_SNAPSHOT_MARKER,
    )
}

fn parse_git_log_refs(decorations: &str) -> Vec<String> {
    decorations
        .split(',')
        .flat_map(|part| {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else if let Some((head, branch)) = trimmed.split_once(" -> ") {
                vec![head.trim().to_string(), branch.trim().to_string()]
            } else if let Some(tag) = trimmed.strip_prefix("tag: ") {
                vec![tag.trim().to_string()]
            } else {
                vec![trimmed.to_string()]
            }
        })
        .collect()
}

pub(crate) fn parse_git_log_commits(
    stdout: &str,
    limit: usize,
) -> Result<(Vec<Value>, bool), &'static str> {
    if !stdout.trim_end_matches(['\n', '\r']).is_empty()
        && !stdout
            .trim_end_matches(['\n', '\r'])
            .ends_with(GIT_LOG_RECORD_SEP)
    {
        return Err("git log source ended inside a record; retry with a smaller limit");
    }
    let mut commits = Vec::new();
    let mut truncated = false;
    for record in stdout.split(GIT_LOG_RECORD_SEP) {
        let record = record.trim_matches(['\n', '\r']);
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.splitn(7, GIT_LOG_UNIT_SEP).collect();
        if fields.len() != 7 || !is_git_object_hex(fields[0]) {
            return Err("git log source is incomplete or malformed; retry with a smaller limit");
        }
        if commits.len() >= limit {
            truncated = true;
            break;
        }
        commits.push(json!({
            "hash": fields[0],
            "short_hash": fields[1],
            "subject": fields[6],
            "author_name": fields[3],
            "author_email": fields[4],
            "author_date": fields[5],
            "refs": parse_git_log_refs(fields[2]),
        }));
    }
    Ok((commits, truncated))
}

fn parse_git_log_page(
    stdout: &str,
    limit: usize,
) -> Result<(Option<String>, Vec<Value>, bool), &'static str> {
    let source = stdout.trim_end_matches(['\n', '\r']);
    let source = source
        .strip_suffix(GIT_LOG_RECORD_SEP)
        .ok_or("git log snapshot marker is incomplete or missing")?;
    let (commit_source, marker) = source
        .rsplit_once(GIT_LOG_RECORD_SEP)
        .map_or(("", source), |(commits, marker)| (commits, marker));
    let snapshot = marker
        .strip_prefix(GIT_LOG_SNAPSHOT_MARKER)
        .ok_or("git log snapshot marker is incomplete or missing")?;
    if snapshot == "unborn" {
        if !commit_source.trim_matches(['\n', '\r']).is_empty() {
            return Err("unborn git log snapshot unexpectedly contained commits");
        }
        return Ok((None, Vec::new(), false));
    }
    let head_commit = normalize_exact_commit_id(snapshot)
        .map_err(|_| "git log snapshot marker contained an invalid commit id")?;
    let commit_source = if commit_source.is_empty() {
        String::new()
    } else {
        format!("{commit_source}{GIT_LOG_RECORD_SEP}")
    };
    let (commits, truncated) = parse_git_log_commits(&commit_source, limit)?;
    Ok((Some(head_commit), commits, truncated))
}

fn git_log_suggested_call(
    project: &str,
    head_commit: &str,
    limit: usize,
    next_skip: usize,
    session_id: Option<&str>,
) -> Value {
    let mut arguments = Map::new();
    arguments.insert("project".to_string(), json!(project));
    arguments.insert("head_commit".to_string(), json!(head_commit));
    arguments.insert("limit".to_string(), json!(limit));
    arguments.insert("skip".to_string(), json!(next_skip));
    if let Some(session_id) = session_id {
        arguments.insert("session_id".to_string(), json!(session_id));
    }
    SuggestedToolCall::new("git_log", Value::Object(arguments)).to_value()
}

impl ToolRuntime {
    pub(crate) async fn git_log(
        &self,
        project: String,
        head_commit: Option<String>,
        limit: Option<usize>,
        skip: Option<usize>,
        session_id: Option<String>,
    ) -> ToolResult {
        let head_commit = match head_commit {
            Some(value) => match normalize_exact_commit_id(&value) {
                Ok(value) => Some(value),
                Err(reason) => {
                    return ToolResult::err_with_output(
                        format!("git_log failed: {reason}"),
                        json!({
                            "project": project,
                            "error_kind": reason,
                            "state_changed": false,
                        }),
                    )
                }
            },
            None => None,
        };
        let resolved_project = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved.resolved_id,
            Err(error) => return ToolResult::err(error),
        };
        let limit = normalize_git_log_limit(limit);
        let skip = normalize_git_log_skip(skip);
        let command = head_commit.as_deref().map_or_else(
            || git_log_command(limit, skip),
            |head| git_log_command_at_head(head, limit, skip),
        );
        let output = match self
            .run_project_internal_posix_script_capture(&resolved_project, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        if output.exit_code == Some(GIT_LOG_SNAPSHOT_UNAVAILABLE_EXIT) {
            return ToolResult::err_with_output(
                "git log snapshot is unavailable",
                json!({
                    "project": project,
                    "head_commit": head_commit,
                    "error_kind": "snapshot_unavailable",
                    "state_changed": false,
                }),
            );
        }
        if output.exit_code != Some(0) {
            return ToolResult {
                success: false,
                output: json!({
                    "project": project,
                    "head_commit": head_commit,
                    "limit": limit,
                    "skip": skip,
                    "exit_code": output.exit_code,
                    "stderr": output.stderr,
                }),
                error: Some("git log failed".to_string()),
            };
        }
        let (head_commit, commits, truncated) = match parse_git_log_page(&output.stdout, limit) {
            Ok(page) => page,
            Err(error) => {
                return ToolResult::err_with_output(
                    error,
                    json!({
                        "project": project,
                        "error_kind": "source_incomplete",
                        "state_changed": false,
                    }),
                );
            }
        };
        let next_skip = git_log_next_skip(skip, commits.len(), truncated);
        let mut payload = json!({
            "project": project,
            "head_commit": head_commit,
            "limit": limit,
            "skip": skip,
            "count": commits.len(),
            "truncated": truncated,
            "next_skip": next_skip,
            "commits": commits,
        });
        if let (Some(next_skip), Some(head_commit)) = (next_skip, head_commit.as_deref()) {
            payload["suggested_call"] = git_log_suggested_call(
                &resolved_project,
                head_commit,
                limit,
                next_skip,
                session_id.as_deref(),
            );
        }
        ToolResult::ok(payload)
    }
}
