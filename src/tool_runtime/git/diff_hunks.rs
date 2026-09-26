use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub(crate) use webcodex_core::runtime_contract::GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES;
use webcodex_core::runtime_contract::{
    DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES, MAX_GIT_DIFF_HUNKS_PAGE_BYTES,
    MIN_GIT_DIFF_HUNKS_PAGE_BYTES, MODEL_INSPECTION_MAX_RESULT_BYTES,
};
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;

use super::super::git_committed::{
    committed_git_discovery_prefix, committed_git_isolated_view_setup, normalize_exact_commit_id,
    CommittedGitScope,
};
use super::super::helpers::{
    decode_git_quoted_path, shell_escape_simple, validate_project_relative_path,
};
use super::super::shell::{command_execution_state_name, ProjectCommandOutput};
use super::super::tool_result::ToolResult;
use super::super::{SuggestedToolCall, ToolRuntime};
use super::shared::{
    is_git_object_hex, parse_fixed_decimal, parse_optional_bool, parse_optional_usize,
    parse_status_result_field, strip_wire_lf,
};

pub(super) const DEFAULT_MAX_HUNKS: usize = 30;
const MAX_MAX_HUNKS: usize = 100;
pub(super) const DEFAULT_MAX_HUNK_LINES: usize = 160;
pub(crate) const MAX_MAX_HUNK_LINES: usize = 400;
const GIT_DIFF_HUNKS_V2_CONTINUATION_PREFIX: &str = "wcdh2.";
const GIT_DIFF_HUNKS_V2_WORKTREE_PAGE: u8 = 1;
const GIT_DIFF_HUNKS_V2_COMMITTED_PAGE: u8 = 2;
const GIT_DIFF_HUNKS_V2_WORKTREE_FRAGMENT: u8 = 3;
const GIT_DIFF_HUNKS_V2_COMMITTED_FRAGMENT: u8 = 4;
const GIT_DIFF_HUNKS_STDERR_BYTES: usize = 8 * 1024;
const GIT_DIFF_HUNKS_BLOCK_TRAILER_BYTES: usize = 30;
const GIT_DIFF_HUNKS_BLOCK_MAGIC: &[u8; 6] = b"WCDH1:";

fn normalize_git_diff_hunks_page_bytes(max_page_bytes: Option<usize>) -> usize {
    max_page_bytes
        .unwrap_or(DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES)
        .clamp(MIN_GIT_DIFF_HUNKS_PAGE_BYTES, MAX_GIT_DIFF_HUNKS_PAGE_BYTES)
}

pub(super) fn git_diff_file_hunk_count(files: &[Value]) -> Option<usize> {
    files.iter().try_fold(0usize, |count, file| {
        file.get("hunks")
            .and_then(Value::as_array)
            .map(|hunks| count + hunks.len())
    })
}

pub(super) fn git_diff_files_have_omitted_lines(files: &[Value]) -> bool {
    files.iter().any(|file| {
        file.get("hunks")
            .and_then(Value::as_array)
            .is_some_and(|hunks| {
                hunks
                    .iter()
                    .any(|hunk| hunk.get("truncated").and_then(Value::as_bool) == Some(true))
            })
    })
}

pub(super) fn sparsify_complete_diff_files(files: &mut [Value]) {
    for file in files {
        let Some(file) = file.as_object_mut() else {
            continue;
        };
        let duplicate_old_path = match (
            file.get("path").and_then(Value::as_str),
            file.get("old_path").and_then(Value::as_str),
        ) {
            (Some(path), Some(old_path)) => path == old_path,
            _ => false,
        };
        if duplicate_old_path {
            file.remove("old_path");
        }
        let Some(hunks) = file.get_mut("hunks").and_then(Value::as_array_mut) else {
            continue;
        };
        for hunk in hunks {
            let Some(hunk) = hunk.as_object_mut() else {
                continue;
            };
            if hunk.get("truncated").and_then(Value::as_bool) == Some(false) {
                hunk.remove("truncated");
            }
            let line_count_is_derived = match (
                hunk.get("line_count").and_then(Value::as_u64),
                hunk.get("diff").and_then(Value::as_str),
            ) {
                (Some(line_count), Some(diff)) => line_count == diff.lines().count() as u64,
                _ => false,
            };
            if line_count_is_derived {
                hunk.remove("line_count");
            }
        }
    }
}

pub(super) fn sparsify_complete_git_diff_hunks_output(output: &mut serde_json::Map<String, Value>) {
    let Some(files) = output.get("files").and_then(Value::as_array) else {
        return;
    };
    let Some(computed_hunks) = git_diff_file_hunk_count(files) else {
        return;
    };
    let ordinary_complete = output.get("hunk_count").and_then(Value::as_u64)
        == Some(computed_hunks as u64)
        && output.get("truncated").and_then(Value::as_bool) == Some(false)
        && output
            .get("truncation_reasons")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        && output.get("has_more").and_then(Value::as_bool) == Some(false)
        && output.get("recovery").is_none()
        && output.get("exit_code").and_then(Value::as_i64) == Some(0)
        && output.get("stderr").and_then(Value::as_str) == Some("")
        && !git_diff_files_have_omitted_lines(files);
    if !ordinary_complete {
        return;
    }

    if let Some(files) = output.get_mut("files").and_then(Value::as_array_mut) {
        sparsify_complete_diff_files(files);
    }
    for key in [
        "hunk_count",
        "truncated",
        "truncation_reasons",
        "has_more",
        "exit_code",
        "stderr",
    ] {
        output.remove(key);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GitDiffHunksContinuation {
    Page {
        fence: String,
        next_record: usize,
    },
    HunkFragment {
        fence: String,
        record_index: usize,
        next_line: usize,
    },
}

impl GitDiffHunksContinuation {
    fn fence(&self) -> &str {
        match self {
            Self::Page { fence, .. } | Self::HunkFragment { fence, .. } => fence,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitDiffHunksContinuationError {
    Invalid,
    ScopeMismatch,
}

fn git_diff_hunks_scope_digest(
    resolved_project: &str,
    paths: &[String],
    cached: bool,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: usize,
) -> String {
    let mut normalized_paths = paths.to_vec();
    normalized_paths.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.git-diff-hunks.scope.v2\0");
    hasher.update(resolved_project.as_bytes());
    hasher.update([0]);
    if cached {
        hasher.update(b"cached");
    } else {
        hasher.update(b"worktree");
    }
    hasher.update((max_hunks as u64).to_be_bytes());
    hasher.update((max_hunk_lines as u64).to_be_bytes());
    hasher.update((max_page_bytes as u64).to_be_bytes());
    for path in normalized_paths {
        hasher.update([0]);
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn git_diff_hunks_committed_scope_digest(
    resolved_project: &str,
    paths: &[String],
    scope: &CommittedGitScope,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: usize,
) -> String {
    let mut normalized_paths = paths.to_vec();
    normalized_paths.sort();
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex.git-diff-hunks.scope.committed.v2\0");
    for value in [
        resolved_project,
        scope.requested_base.as_str(),
        scope.requested_head.as_str(),
        scope.merge_base.as_str(),
    ] {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hasher.update((max_hunks as u64).to_be_bytes());
    hasher.update((max_hunk_lines as u64).to_be_bytes());
    hasher.update((max_page_bytes as u64).to_be_bytes());
    for path in normalized_paths {
        hasher.update((path.len() as u64).to_be_bytes());
        hasher.update(path.as_bytes());
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn git_diff_hunks_committed_token_mac(
    key: &[u8; 32],
    scope: &str,
    fence: &str,
    next: u64,
) -> String {
    const BLOCK_BYTES: usize = 64;
    let mut ipad = [0x36u8; BLOCK_BYTES];
    let mut opad = [0x5cu8; BLOCK_BYTES];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(b"webcodex.git-diff-hunks.continuation.committed.v2\0");
    inner.update((scope.len() as u64).to_be_bytes());
    inner.update(scope.as_bytes());
    inner.update((fence.len() as u64).to_be_bytes());
    inner.update(fence.as_bytes());
    inner.update(next.to_be_bytes());
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    format!("{:x}", outer.finalize())
}

fn git_diff_hunks_committed_hunk_fragment_token_mac(
    key: &[u8; 32],
    scope: &str,
    fence: &str,
    record_index: u64,
    next_line: u64,
) -> String {
    const BLOCK_BYTES: usize = 64;
    let mut ipad = [0x36u8; BLOCK_BYTES];
    let mut opad = [0x5cu8; BLOCK_BYTES];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(b"webcodex.git-diff-hunks.continuation.hunk-fragment.committed.v1\0");
    inner.update((scope.len() as u64).to_be_bytes());
    inner.update(scope.as_bytes());
    inner.update((fence.len() as u64).to_be_bytes());
    inner.update(fence.as_bytes());
    inner.update(record_index.to_be_bytes());
    inner.update(next_line.to_be_bytes());
    let inner = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner);
    format!("{:x}", outer.finalize())
}

fn decode_lower_hex_bytes(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let nibble = |byte: u8| -> Option<u8> {
                match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    _ => None,
                }
            };
            Some((nibble(pair[0])? << 4) | nibble(pair[1])?)
        })
        .collect()
}

fn lower_hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        value.push(HEX[(byte >> 4) as usize] as char);
        value.push(HEX[(byte & 0x0f) as usize] as char);
    }
    value
}

fn fixed_time_bytes_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .fold(0u8, |diff, (left, right)| diff | (left ^ right))
            == 0
}

fn encode_git_diff_hunks_v2_token(
    tag: u8,
    scope: &str,
    fence: &str,
    next: u64,
    line: Option<u64>,
    mac: Option<&str>,
) -> Result<String, String> {
    let scope_bytes = decode_lower_hex_bytes(scope)
        .filter(|bytes| bytes.len() == 32)
        .ok_or_else(|| "invalid git diff continuation scope".to_string())?;
    let fence_bytes = decode_lower_hex_bytes(fence)
        .filter(|bytes| matches!(bytes.len(), 20 | 32) && is_git_object_hex(fence))
        .ok_or_else(|| "invalid git diff continuation fence".to_string())?;
    if next == 0 || line.is_some_and(|line| line == 0) {
        return Err("invalid git diff continuation cursor".to_string());
    }
    let committed = matches!(
        tag,
        GIT_DIFF_HUNKS_V2_COMMITTED_PAGE | GIT_DIFF_HUNKS_V2_COMMITTED_FRAGMENT
    );
    let fragment = matches!(
        tag,
        GIT_DIFF_HUNKS_V2_WORKTREE_FRAGMENT | GIT_DIFF_HUNKS_V2_COMMITTED_FRAGMENT
    );
    if !matches!(tag, 1..=4) || fragment != line.is_some() || committed != mac.is_some() {
        return Err("invalid git diff continuation shape".to_string());
    }
    let mac_bytes = match mac {
        Some(mac) => Some(
            decode_lower_hex_bytes(mac)
                .filter(|bytes| bytes.len() == 32)
                .ok_or_else(|| "invalid git diff continuation mac".to_string())?,
        ),
        None => None,
    };
    let mut payload = Vec::with_capacity(
        1 + 32
            + 1
            + fence_bytes.len()
            + 8
            + usize::from(fragment) * 8
            + usize::from(committed) * 32,
    );
    payload.push(tag);
    payload.extend_from_slice(&scope_bytes);
    payload.push(fence_bytes.len() as u8);
    payload.extend_from_slice(&fence_bytes);
    payload.extend_from_slice(&next.to_be_bytes());
    if let Some(line) = line {
        payload.extend_from_slice(&line.to_be_bytes());
    }
    if let Some(mac) = mac_bytes {
        payload.extend_from_slice(&mac);
    }
    let value = format!(
        "{GIT_DIFF_HUNKS_V2_CONTINUATION_PREFIX}{}",
        general_purpose::URL_SAFE_NO_PAD.encode(payload)
    );
    if value.len() > GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES {
        return Err("git diff continuation exceeded its size bound".to_string());
    }
    Ok(value)
}

fn encode_git_diff_hunks_continuation(
    scope: &str,
    fence: &str,
    next: usize,
) -> Result<String, String> {
    let next = u64::try_from(next).map_err(|_| "git diff continuation cursor overflow")?;
    encode_git_diff_hunks_v2_token(
        GIT_DIFF_HUNKS_V2_WORKTREE_PAGE,
        scope,
        fence,
        next,
        None,
        None,
    )
}

fn encode_git_diff_hunks_committed_continuation(
    key: &[u8; 32],
    scope: &str,
    fence: &str,
    next: usize,
) -> Result<String, String> {
    let next = u64::try_from(next).map_err(|_| "git diff continuation cursor overflow")?;
    let mac = git_diff_hunks_committed_token_mac(key, scope, fence, next);
    encode_git_diff_hunks_v2_token(
        GIT_DIFF_HUNKS_V2_COMMITTED_PAGE,
        scope,
        fence,
        next,
        None,
        Some(&mac),
    )
}

fn encode_git_diff_hunks_hunk_fragment_continuation(
    scope: &str,
    fence: &str,
    record_index: usize,
    next_line: usize,
) -> Result<String, String> {
    let record_index =
        u64::try_from(record_index).map_err(|_| "git diff continuation cursor overflow")?;
    let next_line = u64::try_from(next_line).map_err(|_| "git diff continuation line overflow")?;
    encode_git_diff_hunks_v2_token(
        GIT_DIFF_HUNKS_V2_WORKTREE_FRAGMENT,
        scope,
        fence,
        record_index,
        Some(next_line),
        None,
    )
}

fn encode_git_diff_hunks_committed_hunk_fragment_continuation(
    key: &[u8; 32],
    scope: &str,
    fence: &str,
    record_index: usize,
    next_line: usize,
) -> Result<String, String> {
    let record_index =
        u64::try_from(record_index).map_err(|_| "git diff continuation cursor overflow")?;
    let next_line = u64::try_from(next_line).map_err(|_| "git diff continuation line overflow")?;
    let mac = git_diff_hunks_committed_hunk_fragment_token_mac(
        key,
        scope,
        fence,
        record_index,
        next_line,
    );
    encode_git_diff_hunks_v2_token(
        GIT_DIFF_HUNKS_V2_COMMITTED_FRAGMENT,
        scope,
        fence,
        record_index,
        Some(next_line),
        Some(&mac),
    )
}

fn decode_git_diff_hunks_v2_continuation(
    raw: &str,
    expected_scope: &str,
    committed_mac_key: Option<&[u8; 32]>,
) -> Result<GitDiffHunksContinuation, GitDiffHunksContinuationError> {
    let encoded = raw
        .strip_prefix(GIT_DIFF_HUNKS_V2_CONTINUATION_PREFIX)
        .ok_or(GitDiffHunksContinuationError::Invalid)?;
    let payload = general_purpose::URL_SAFE_NO_PAD
        .decode(encoded.as_bytes())
        .map_err(|_| GitDiffHunksContinuationError::Invalid)?;
    if payload.len() < 1 + 32 + 1 + 20 + 8 {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    let tag = payload[0];
    let (committed, fragment) = match tag {
        GIT_DIFF_HUNKS_V2_WORKTREE_PAGE => (false, false),
        GIT_DIFF_HUNKS_V2_COMMITTED_PAGE => (true, false),
        GIT_DIFF_HUNKS_V2_WORKTREE_FRAGMENT => (false, true),
        GIT_DIFF_HUNKS_V2_COMMITTED_FRAGMENT => (true, true),
        _ => return Err(GitDiffHunksContinuationError::Invalid),
    };
    if committed != committed_mac_key.is_some() {
        return Err(GitDiffHunksContinuationError::ScopeMismatch);
    }
    let scope = lower_hex_bytes(&payload[1..33]);
    let fence_len = payload[33] as usize;
    if !matches!(fence_len, 20 | 32) {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    let expected_len =
        1 + 32 + 1 + fence_len + 8 + usize::from(fragment) * 8 + usize::from(committed) * 32;
    if payload.len() != expected_len {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    let fence_start = 34;
    let fence_end = fence_start + fence_len;
    let fence = lower_hex_bytes(&payload[fence_start..fence_end]);
    if !is_git_object_hex(&fence) {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    let mut cursor = fence_end;
    let next = u64::from_be_bytes(
        payload[cursor..cursor + 8]
            .try_into()
            .map_err(|_| GitDiffHunksContinuationError::Invalid)?,
    );
    cursor += 8;
    if next == 0 {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    let record_or_next =
        usize::try_from(next).map_err(|_| GitDiffHunksContinuationError::Invalid)?;
    let line = if fragment {
        let line = u64::from_be_bytes(
            payload[cursor..cursor + 8]
                .try_into()
                .map_err(|_| GitDiffHunksContinuationError::Invalid)?,
        );
        cursor += 8;
        if line == 0 || usize::try_from(line).is_err() {
            return Err(GitDiffHunksContinuationError::Invalid);
        }
        Some(line)
    } else {
        None
    };
    if let Some(key) = committed_mac_key {
        let supplied_mac = &payload[cursor..cursor + 32];
        let expected_mac = if let Some(line) = line {
            git_diff_hunks_committed_hunk_fragment_token_mac(key, &scope, &fence, next, line)
        } else {
            git_diff_hunks_committed_token_mac(key, &scope, &fence, next)
        };
        let expected_mac =
            decode_lower_hex_bytes(&expected_mac).ok_or(GitDiffHunksContinuationError::Invalid)?;
        if !fixed_time_bytes_eq(supplied_mac, &expected_mac) {
            return Err(GitDiffHunksContinuationError::Invalid);
        }
    }
    if scope != expected_scope {
        return Err(GitDiffHunksContinuationError::ScopeMismatch);
    }
    if let Some(line) = line {
        Ok(GitDiffHunksContinuation::HunkFragment {
            fence,
            record_index: record_or_next,
            next_line: usize::try_from(line).map_err(|_| GitDiffHunksContinuationError::Invalid)?,
        })
    } else {
        Ok(GitDiffHunksContinuation::Page {
            fence,
            next_record: record_or_next,
        })
    }
}

fn decode_git_diff_hunks_continuation(
    raw: &str,
    expected_scope: &str,
    committed_mac_key: Option<&[u8; 32]>,
) -> Result<GitDiffHunksContinuation, GitDiffHunksContinuationError> {
    if raw.is_empty() || raw.len() > GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES {
        return Err(GitDiffHunksContinuationError::Invalid);
    }
    decode_git_diff_hunks_v2_continuation(raw, expected_scope, committed_mac_key)
}

fn bounded_git_diff_hunks_stderr(stderr: &str) -> String {
    if stderr.len() <= GIT_DIFF_HUNKS_STDERR_BYTES {
        return stderr.to_string();
    }
    let mut end = GIT_DIFF_HUNKS_STDERR_BYTES;
    while !stderr.is_char_boundary(end) {
        end -= 1;
    }
    stderr[..end].to_string()
}

fn git_diff_hunks_failure(
    project: &str,
    paths: &[String],
    cached: bool,
    reason_code: &'static str,
    exit_code: Option<i32>,
    stderr: &str,
) -> ToolResult {
    ToolResult::err_with_output(
        format!("git_diff_hunks failed: {reason_code}"),
        json!({
            "project": project,
            "paths": paths,
            "cached": cached,
            "files": [],
            "hunk_count": 0,
            "truncated": false,
            "truncation_reasons": [],
            "has_more": false,
            "exit_code": exit_code,
            "stderr": bounded_git_diff_hunks_stderr(stderr),
            "error_kind": "git_diff_hunks_failed",
            "reason_code": reason_code,
            "state_changed": false,
        }),
    )
}

fn git_diff_hunks_source_failure(
    project: &str,
    paths: &[String],
    cached: bool,
    reason_code: &'static str,
    source_stage: &'static str,
    output: Option<&ProjectCommandOutput>,
) -> ToolResult {
    let stderr = output
        .map(|output| bounded_git_diff_hunks_stderr(&output.stderr))
        .unwrap_or_default();
    ToolResult::err_with_output(
        format!("git_diff_hunks failed: {reason_code}"),
        json!({
            "project": project,
            "paths": paths,
            "cached": cached,
            "files": [],
            "hunk_count": 0,
            "truncated": false,
            "truncation_reasons": [],
            "has_more": false,
            "exit_code": output.and_then(|output| output.exit_code),
            "stderr": stderr,
            "error_kind": "git_diff_hunks_failed",
            "reason_code": reason_code,
            "source_stage": source_stage,
            "execution_state": output.map(|output| command_execution_state_name(output.execution_state)),
            "stdout_bytes": output.map(|output| output.stdout.len()),
            "stderr_bytes": output.map(|output| output.stderr.len()),
            "stdout_truncated": output.map(|output| output.stdout_truncated).unwrap_or(false),
            "stderr_truncated": output.map(|output| output.stderr_truncated).unwrap_or(false),
            "state_changed": false,
        }),
    )
}

fn committed_git_diff_hunks_scope_value(scope: &CommittedGitScope) -> Value {
    json!({
        "mode": "committed",
        "requested_base": scope.requested_base,
        "requested_head": scope.requested_head,
        "merge_base": scope.merge_base,
        "base_is_ancestor": scope.base_is_ancestor,
        "diff_range": format!("{}..{}", scope.merge_base, scope.requested_head),
    })
}

fn git_diff_hunks_call_arguments(
    project: &str,
    paths: &[String],
    cached: bool,
    committed_scope: Option<&CommittedGitScope>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: usize,
    continuation: Option<&str>,
) -> Value {
    let mut arguments = serde_json::Map::new();
    arguments.insert("project".to_string(), json!(project));
    arguments.insert("paths".to_string(), json!(paths));
    arguments.insert("max_hunks".to_string(), json!(max_hunks));
    arguments.insert("max_hunk_lines".to_string(), json!(max_hunk_lines));
    arguments.insert("max_page_bytes".to_string(), json!(max_page_bytes));
    if let Some(scope) = committed_scope {
        arguments.insert("base_commit".to_string(), json!(scope.requested_base));
        arguments.insert("head_commit".to_string(), json!(scope.requested_head));
    } else {
        arguments.insert("cached".to_string(), json!(cached));
    }
    if let Some(continuation) = continuation {
        arguments.insert("continuation".to_string(), json!(continuation));
    }
    Value::Object(arguments)
}

fn git_diff_hunks_exact_truncated_paths(files: &[Value]) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    let mut saw_truncated_hunk = false;
    for file in files {
        let has_omitted_lines = file
            .get("hunks")
            .and_then(Value::as_array)
            .is_some_and(|hunks| {
                hunks
                    .iter()
                    .any(|hunk| hunk.get("truncated").and_then(Value::as_bool) == Some(true))
            });
        if !has_omitted_lines {
            continue;
        }
        saw_truncated_hunk = true;
        let Some(path) = file
            .get("path")
            .and_then(Value::as_str)
            .filter(|path| !path.is_empty())
            .or_else(|| {
                file.get("old_path")
                    .and_then(Value::as_str)
                    .filter(|path| !path.is_empty())
            })
        else {
            return None;
        };
        if !paths.iter().any(|existing| existing == path) {
            paths.push(path.to_string());
        }
    }
    saw_truncated_hunk.then_some(paths)
}

fn git_diff_hunks_recovery_value(
    project: &str,
    paths: &[String],
    cached: bool,
    committed_scope: Option<&CommittedGitScope>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: usize,
    files: &[Value],
    page_hunk_limit: bool,
    hunk_line_limit: bool,
    page_byte_budget: bool,
    truncated_hunks: &[usize],
    line_ceiling_hunks: &[usize],
    line_recoverable_hunks: &[usize],
    next_continuation: Option<&str>,
    hunk_fragment_continuation: Option<&str>,
) -> Option<Value> {
    let page_truncated = page_hunk_limit || page_byte_budget || next_continuation.is_some();
    let omitted_lines_present = hunk_line_limit || git_diff_files_have_omitted_lines(files);
    if !page_truncated && !omitted_lines_present {
        return None;
    }

    let continuation_call = next_continuation.map(|continuation| {
        SuggestedToolCall::new(
            "git_diff_hunks",
            git_diff_hunks_call_arguments(
                project,
                paths,
                cached,
                committed_scope,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                Some(continuation),
            ),
        )
        .to_value()
    });

    let (omitted_line_paths, path_provenance) = if omitted_lines_present {
        match git_diff_hunks_exact_truncated_paths(files) {
            Some(paths) => (paths, "exact"),
            None => (paths.to_vec(), "scope"),
        }
    } else {
        (Vec::new(), "none")
    };
    // The producer drains each returned hunk to its boundary even when its
    // model-facing body is line-bounded. These bounded index lists therefore
    // prove whether *all* omitted returned hunks fit both the 400-line ceiling
    // and a fresh path-scoped page using the same producer byte budget. Never
    // infer future byte fit from the already-emitted prefix alone.
    let omitted_lines_fit_line_ceiling = !truncated_hunks.is_empty()
        && truncated_hunks
            .iter()
            .all(|index| line_ceiling_hunks.contains(index));
    let omitted_lines_recovery_proven = !truncated_hunks.is_empty()
        && truncated_hunks
            .iter()
            .all(|index| line_recoverable_hunks.contains(index));
    let single_returned_hunk = git_diff_file_hunk_count(files) == Some(1);
    let exact_single_hunk_recovery = path_provenance == "exact"
        && omitted_line_paths.len() == 1
        && truncated_hunks.len() == 1
        && single_returned_hunk
        && !page_truncated;
    let refinement_recoverable = omitted_lines_present
        && hunk_line_limit
        && max_hunk_lines < MAX_MAX_HUNK_LINES
        && omitted_lines_recovery_proven
        && exact_single_hunk_recovery;
    let fragment_recoverable =
        omitted_lines_present && !refinement_recoverable && hunk_fragment_continuation.is_some();
    let omitted_lines_reason = if !omitted_lines_present {
        Value::Null
    } else if refinement_recoverable {
        json!("larger_max_hunk_lines_available")
    } else if fragment_recoverable {
        json!("hunk_fragment_continuation_available")
    } else if !hunk_line_limit {
        json!("page_byte_budget_prevents_proven_recovery")
    } else if max_hunk_lines >= MAX_MAX_HUNK_LINES && !omitted_lines_fit_line_ceiling {
        json!("max_hunk_lines_ceiling_reached")
    } else if !omitted_lines_fit_line_ceiling {
        json!("max_hunk_lines_ceiling_insufficient")
    } else if !omitted_lines_recovery_proven {
        json!("page_byte_budget_prevents_proven_recovery")
    } else {
        json!("bounded_recovery_unavailable")
    };
    let refinement_call = refinement_recoverable.then(|| {
        SuggestedToolCall::new(
            "git_diff_hunks",
            git_diff_hunks_call_arguments(
                project,
                &omitted_line_paths,
                cached,
                committed_scope,
                max_hunks,
                MAX_MAX_HUNK_LINES,
                max_page_bytes,
                None,
            ),
        )
        .to_value()
    });
    let fragment_call = fragment_recoverable.then(|| {
        SuggestedToolCall::new(
            "git_diff_hunks",
            git_diff_hunks_call_arguments(
                project,
                paths,
                cached,
                committed_scope,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
                hunk_fragment_continuation,
            ),
        )
        .to_value()
    });
    let omitted_lines_call = refinement_call.as_ref().or(fragment_call.as_ref());
    let mut recovery = serde_json::Map::new();
    if omitted_lines_present {
        let mut current_hunk = json!({"reason_code": omitted_lines_reason});
        if let Some(call) = omitted_lines_call {
            current_hunk["next_call"] = call.clone();
        }
        recovery.insert("current_hunk".to_string(), current_hunk);
    }
    if let Some(call) = continuation_call {
        recovery.insert("later_hunks".to_string(), json!({"next_call": call}));
    }
    Some(Value::Object(recovery))
}

fn git_diff_hunks_committed_failure(
    project: &str,
    paths: &[String],
    requested_base: Option<&str>,
    requested_head: Option<&str>,
    reason_code: &'static str,
    exit_code: Option<i32>,
    stderr: &str,
) -> ToolResult {
    let mut result = git_diff_hunks_failure(project, paths, false, reason_code, exit_code, stderr);
    let requested_base = requested_base.and_then(|value| normalize_exact_commit_id(value).ok());
    let requested_head = requested_head.and_then(|value| normalize_exact_commit_id(value).ok());
    if let Some(output) = result.output.as_object_mut() {
        output.insert(
            "scope".to_string(),
            json!({
                "mode": "committed",
                "requested_base": requested_base,
                "requested_head": requested_head,
                "merge_base": null,
                "base_is_ancestor": null,
                "diff_range": null,
            }),
        );
    }
    result
}

fn normalize_git_diff_hunks_committed_range(
    base_commit: Option<&str>,
    head_commit: Option<&str>,
    cached: bool,
) -> Result<Option<(String, String)>, &'static str> {
    match (base_commit, head_commit) {
        (None, None) => Ok(None),
        (Some(_), None) | (None, Some(_)) => Err("committed_range_requires_base_and_head"),
        (Some(base), Some(head)) => {
            if cached {
                return Err("committed_range_conflicts_with_cached");
            }
            Ok(Some((
                normalize_exact_commit_id(&base)?,
                normalize_exact_commit_id(&head)?,
            )))
        }
    }
}

fn clean_optional_paths(paths: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut clean = Vec::new();
    for raw in paths.unwrap_or_default() {
        validate_project_relative_path(&raw)?;
        let path = raw.trim().trim_start_matches("./").trim_end_matches('/');
        if path.is_empty() || path == "." {
            return Err(
                "diff path must name a file or directory, not the project root".to_string(),
            );
        }
        if !clean.iter().any(|p: &String| p == path) {
            clean.push(path.to_string());
        }
    }
    Ok(clean)
}

fn git_diff_hunks_paths_include_secret(paths: &[String]) -> bool {
    paths
        .iter()
        .any(|path| crate::sensitive_paths::is_secret_path(path))
}

fn git_diff_hunks_files_include_secret(files: &[Value]) -> bool {
    files.iter().any(|file| {
        ["path", "old_path"].into_iter().any(|field| {
            file.get(field)
                .and_then(Value::as_str)
                .is_some_and(crate::sensitive_paths::is_secret_path)
        })
    })
}

pub(crate) fn git_diff_hunks_command(paths: &[String], cached: bool) -> Result<String, String> {
    let mut parts = vec!["git".to_string(), "diff".to_string()];
    if cached {
        parts.push("--cached".to_string());
    }
    parts.push("--no-ext-diff".to_string());
    parts.push("--no-textconv".to_string());
    parts.push("--unified=80".to_string());
    if !paths.is_empty() {
        parts.push("--".to_string());
        parts.extend(paths.iter().map(|path| shell_escape_simple(path)));
    }
    Ok(parts.join(" "))
}

fn git_diff_hunks_fingerprint_command(paths: &[String], cached: bool) -> String {
    let mut parts = vec!["git".to_string(), "diff".to_string()];
    if cached {
        parts.push("--cached".to_string());
    }
    parts.extend([
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--binary".to_string(),
        "--full-index".to_string(),
        "--unified=80".to_string(),
    ]);
    if !paths.is_empty() {
        parts.push("--".to_string());
        parts.extend(paths.iter().map(|path| shell_escape_simple(path)));
    }
    parts.join(" ")
}

fn git_diff_hunks_committed_command(
    scope: &CommittedGitScope,
    paths: &[String],
    fingerprint: bool,
) -> String {
    let head_q = shell_escape_simple(&scope.requested_head);
    let mut parts = vec![
        "git".to_string(),
        "--no-pager".to_string(),
        "-c".to_string(),
        "core.quotePath=false".to_string(),
        "diff".to_string(),
        "--no-ext-diff".to_string(),
        "--no-textconv".to_string(),
        "--find-renames".to_string(),
    ];
    if fingerprint {
        parts.push("--binary".to_string());
        parts.push("--full-index".to_string());
    }
    parts.push("--unified=80".to_string());
    parts.push(shell_escape_simple(&scope.merge_base));
    parts.push(head_q);
    if !paths.is_empty() {
        parts.push("--".to_string());
        parts.extend(paths.iter().map(|path| shell_escape_simple(path)));
    }
    parts.join(" ")
}

fn git_diff_hunks_page_command(
    paths: &[String],
    cached: bool,
    start_position: usize,
    fragment_line_position: Option<usize>,
    max_hunks: usize,
    max_hunk_lines: usize,
    max_page_bytes: usize,
    expected_fence: Option<&str>,
    committed_scope: Option<&CommittedGitScope>,
) -> Result<String, String> {
    let (diff_command, fingerprint_command, prelude) = match committed_scope {
        Some(scope) => (
            git_diff_hunks_committed_command(scope, paths, false),
            git_diff_hunks_committed_command(scope, paths, true),
            format!(
                "{}{}",
                committed_git_discovery_prefix(),
                committed_git_isolated_view_setup(&scope.requested_head, "exit 91")
            ),
        ),
        None => (
            git_diff_hunks_command(paths, cached)?,
            git_diff_hunks_fingerprint_command(paths, cached),
            String::new(),
        ),
    };
    let expected_fence = shell_escape_simple(expected_fence.unwrap_or(""));
    let fragment_mode = usize::from(fragment_line_position.is_some());
    let fragment_line_position = fragment_line_position.unwrap_or(0);
    let script = r#"__PRELUDE__
LC_ALL=C; export LC_ALL
page_budget=__PAGE_BUDGET__
max_hunks=__MAX_HUNKS__
max_hunk_lines=__MAX_HUNK_LINES__
max_recovery_hunk_lines=__MAX_RECOVERY_HUNK_LINES__
start_position=__START_POSITION__
fragment_mode=__FRAGMENT_MODE__
fragment_line_position=__FRAGMENT_LINE_POSITION__
expected_fence=__EXPECTED_FENCE__
pre_fence=$(__FINGERPRINT_COMMAND__ | git hash-object --stdin)
pre_hash_exit=$?
__FINGERPRINT_COMMAND__ >/dev/null
pre_diff_exit=$?
stale=0
if [ -n "$expected_fence" ] && [ "$pre_fence" != "$expected_fence" ]; then stale=1; fi
if [ "$pre_hash_exit" -eq 0 ] && [ "$pre_diff_exit" -eq 0 ] && [ "$stale" -eq 0 ]; then
  { __DIFF_COMMAND__; diff_exit=$?; printf '__WCDH_DIFF_EXIT__=%s\n' "$diff_exit"; } |
  awk -v page_budget="$page_budget" -v max_hunks="$max_hunks" -v max_hunk_lines="$max_hunk_lines" -v max_recovery_hunk_lines="$max_recovery_hunk_lines" -v start_position="$start_position" -v fragment_mode="$fragment_mode" -v fragment_line_position="$fragment_line_position" '
function reset_record() {
  record_kind=""; record_buf=""; record_bytes=0; record_byte_trunc=0; record_unreturnable=0;
  hunk_line_count=0; hunk_full_bytes=0; hunk_header_bytes=0; hunk_line_trunc=0;
  fragment_emitted_lines=0; first_omitted_line=-1; first_omitted_line_bytes=0;
}
function available_record_bytes(    available) {
  available=page_budget-page_bytes;
  if (record_kind=="hunk" && !current_file_context_emitted) available-=file_ctx_bytes;
  return (available>0 ? available : 0);
}
function append_record(line,    line_bytes, available) {
  if (stopped || record_byte_trunc) return 0;
  line_bytes=length(line)+1;
  available=available_record_bytes();
  if (record_bytes==0 && line_bytes>available) {
    record_unreturnable=1; record_byte_trunc=1; return 0;
  }
  if (record_bytes+line_bytes<=available) {
    record_buf=record_buf line "\n"; record_bytes+=line_bytes; return 1;
  }
  record_byte_trunc=1;
  return 0;
}
function note_first_omitted_line(line_position, line_bytes) {
  if (first_omitted_line<0) {
    first_omitted_line=line_position; first_omitted_line_bytes=line_bytes;
  }
}
function append_hunk_line(line,    line_bytes, line_position, appended) {
  line_bytes=length(line)+1;
  line_position=hunk_line_count;
  hunk_full_bytes+=line_bytes;
  if (line_position==0) hunk_header_bytes=line_bytes;
  if (fragment_mode && record_index==start_position) {
    if (line_position==0) {
      append_record(line);
    } else if (line_position<fragment_line_position) {
      # Replay-drain prior complete lines without returning them.
    } else if (fragment_emitted_lines<fragment_capacity) {
      appended=append_record(line);
      if (appended) fragment_emitted_lines++;
      else note_first_omitted_line(line_position, line_bytes);
    } else {
      hunk_line_trunc=1; note_first_omitted_line(line_position, line_bytes);
    }
  } else if (hunk_line_count<max_hunk_lines) {
    appended=append_record(line);
    if (!appended) note_first_omitted_line(line_position, line_bytes);
  } else {
    hunk_line_trunc=1; note_first_omitted_line(line_position, line_bytes);
  }
  hunk_line_count++;
}
function note_truncated_hunk(idx) {
  if (truncated_hunks=="") truncated_hunks=idx; else truncated_hunks=truncated_hunks "," idx;
}
function note_line_ceiling_hunk(idx) {
  if (line_ceiling_hunks=="") line_ceiling_hunks=idx; else line_ceiling_hunks=line_ceiling_hunks "," idx;
}
function note_line_recoverable_hunk(idx) {
  if (line_recoverable_hunks=="") line_recoverable_hunks=idx; else line_recoverable_hunks=line_recoverable_hunks "," idx;
}
function flush_record(    need_context, combined_bytes, context_truncated, hunk_index, line_ceiling_fit, line_recovery_safe, candidate_line, candidate_safe, byte_fragment_progress, byte_fragment_safe) {
  if (record_kind=="") return;
  if (record_kind=="file") {
    file_ctx=record_buf; file_ctx_bytes=record_bytes; file_ctx_truncated=record_byte_trunc;
    file_ctx_record=record_index;
  }
  if (record_index<start_position) { reset_record(); return; }
  if (stopped) { has_more=1; reset_record(); return; }
  if (fragment_mode && record_index==start_position && record_kind!="hunk") {
    fragment_invalid=1; stopped=1; reset_record(); return;
  }
  if (record_unreturnable) {
    page_byte_budget=1; has_more=1; stopped=1; reset_record(); return;
  }
  if (record_kind=="hunk" && returned_hunks>=max_hunks) {
    page_hunk_limit=1; has_more=1; stopped=1; reset_record(); return;
  }
  need_context=(record_kind=="hunk" && !current_file_context_emitted);
  context_truncated=0;
  candidate_line=first_omitted_line;
  byte_fragment_progress=(fragment_mode && record_index==start_position ? fragment_emitted_lines>0 : candidate_line>1 && record_bytes>hunk_header_bytes);
  byte_fragment_safe=(record_kind=="hunk" && record_byte_trunc && record_index==start_position && byte_fragment_progress && candidate_line>0 && first_omitted_line_bytes>0 && !file_ctx_truncated && fragment_capacity>0 && file_ctx_bytes+hunk_header_bytes+first_omitted_line_bytes<=page_budget);
  if (record_byte_trunc && (record_kind!="hunk" || record_index!=start_position || !byte_fragment_safe)) {
    page_byte_budget=1; has_more=1; stopped=1; reset_record(); return;
  }
  combined_bytes=record_bytes + (need_context ? file_ctx_bytes : 0);
  if (page_bytes+combined_bytes>page_budget) {
    page_byte_budget=1; has_more=1; stopped=1; reset_record(); return;
  }
  if (need_context) {
    if (file_ctx_bytes>0) { printf "%s", file_ctx; page_bytes+=file_ctx_bytes; }
    current_file_context_emitted=1;
    if (file_ctx_record<start_position && returned_file_records==0 && returned_hunks==0)
      first_file_context_only=1;
    if (file_ctx_truncated) { page_byte_budget=1; context_truncated=1; }
  }
  if (record_bytes>0) { printf "%s", record_buf; page_bytes+=record_bytes; }
  next_position=record_index+1;
  if (record_kind=="file") {
    returned_file_records++;
    current_file_context_emitted=1;
  } else {
    hunk_index=returned_hunks;
    returned_hunks++;
    if (fragment_mode && record_index==start_position) {
      fragment_found=1;
      fragment_progress_lines=fragment_emitted_lines;
      if (fragment_line_position<=0 || fragment_line_position>=hunk_line_count || fragment_emitted_lines<=0)
        fragment_invalid=1;
    }
    if (hunk_line_trunc || record_byte_trunc) note_truncated_hunk(hunk_index);
    line_ceiling_fit=(hunk_line_count<=max_recovery_hunk_lines);
    if ((hunk_line_trunc || record_byte_trunc) && line_ceiling_fit)
      note_line_ceiling_hunk(hunk_index);
    line_recovery_safe=(hunk_line_trunc && !record_byte_trunc && line_ceiling_fit && !file_ctx_truncated && file_ctx_bytes+hunk_full_bytes<=page_budget);
    if (line_recovery_safe) note_line_recoverable_hunk(hunk_index);
    candidate_safe=((hunk_line_trunc || record_byte_trunc) && !context_truncated && !file_ctx_truncated && fragment_capacity>0 && candidate_line>0 && first_omitted_line_bytes>0 && file_ctx_bytes+hunk_header_bytes+first_omitted_line_bytes<=page_budget);
    if (candidate_safe) {
      fragment_candidate_safe=1;
      fragment_candidate_record=record_index;
      fragment_candidate_line=candidate_line;
    }
    if (hunk_line_trunc) hunk_line_limit=1;
    if (record_byte_trunc) page_byte_budget=1;
    if (hunk_line_trunc || record_byte_trunc || (fragment_mode && record_index==start_position)) stopped=1;
  }
  if (record_byte_trunc || context_truncated) stopped=1;
  reset_record();
}
function start_file(line) {
  flush_record();
  record_kind="file"; record_index=record_count; record_count++;
  current_file_context_emitted=0; file_ctx=""; file_ctx_bytes=0; file_ctx_truncated=0;
  append_record(line);
}
function start_hunk(line) {
  flush_record();
  record_kind="hunk"; record_index=record_count; record_count++;
  hunk_line_count=0; append_hunk_line(line);
}
function process_line(line) {
  if (substr(line,1,11)=="diff --git ") { start_file(line); return; }
  if (substr(line,1,3)=="@@ ") { start_hunk(line); return; }
  if (record_kind=="hunk") append_hunk_line(line); else if (record_kind=="file") append_record(line);
}
BEGIN {
  record_count=0; next_position=start_position; page_bytes=0; returned_hunks=0; returned_file_records=0;
  has_more=0; stopped=0; page_hunk_limit=0; hunk_line_limit=0; page_byte_budget=0;
  first_file_context_only=0; truncated_hunks=""; line_ceiling_hunks=""; line_recoverable_hunks=""; current_file_context_emitted=0; file_ctx_record=-1;
  fragment_capacity=(max_hunk_lines>1 ? max_hunk_lines-1 : 0);
  fragment_found=0; fragment_invalid=0; fragment_progress_lines=0;
  fragment_candidate_safe=0; fragment_candidate_record=0; fragment_candidate_line=0;
  reset_record(); have_pending=0; diff_exit=-1;
}
{
  if (have_pending) process_line(pending);
  pending=$0; have_pending=1;
}
END {
  if (have_pending && index(pending,"__WCDH_DIFF_EXIT__=")==1) {
    diff_exit=substr(pending,20)+0;
  } else if (have_pending) {
    process_line(pending);
  }
  flush_record();
  if (fragment_mode && !fragment_found) fragment_invalid=1;
  if (fragment_mode && has_more) page_hunk_limit=1;
  meta=sprintf("diff_exit=%d\nnext_position=%d\ntotal_records=%d\nhas_more=%d\nreturned_hunks=%d\nreturned_file_records=%d\nfirst_file_context_only=%d\npage_hunk_limit=%d\nhunk_line_limit=%d\npage_byte_budget=%d\npage_bytes=%d\ntruncated_hunks=%s\nline_ceiling_hunks=%s\nline_recoverable_hunks=%s\nfragment_mode=%d\nfragment_found=%d\nfragment_invalid=%d\nfragment_progress_lines=%d\nfragment_candidate_safe=%d\nfragment_candidate_record=%d\nfragment_candidate_line=%d", diff_exit, next_position, record_count, has_more, returned_hunks, returned_file_records, first_file_context_only, page_hunk_limit, hunk_line_limit, page_byte_budget, page_bytes, truncated_hunks, line_ceiling_hunks, line_recoverable_hunks, fragment_mode, fragment_found, fragment_invalid, fragment_progress_lines, fragment_candidate_safe, fragment_candidate_record, fragment_candidate_line);
  printf "%s\n", meta;
  printf "WCDH1:P:%010d:%010d\n", page_bytes, length(meta)+1;
}
'
  page_filter_exit=$?
else
  page_meta=$(printf 'diff_exit=-1\nnext_position=%s\ntotal_records=0\nhas_more=0\nreturned_hunks=0\nreturned_file_records=0\nfirst_file_context_only=0\npage_hunk_limit=0\nhunk_line_limit=0\npage_byte_budget=0\npage_bytes=0\ntruncated_hunks=\nline_ceiling_hunks=\nline_recoverable_hunks=\nfragment_mode=%s\nfragment_found=0\nfragment_invalid=0\nfragment_progress_lines=0\nfragment_candidate_safe=0\nfragment_candidate_record=0\nfragment_candidate_line=0' "$start_position" "$fragment_mode")
  page_meta_bytes=${#page_meta}
  printf '%s\n' "$page_meta"
  printf 'WCDH1:P:%010d:%010d\n' 0 "$((page_meta_bytes+1))"
  page_filter_exit=0
fi
post_fence=$(__FINGERPRINT_COMMAND__ | git hash-object --stdin)
post_hash_exit=$?
__FINGERPRINT_COMMAND__ >/dev/null
post_diff_exit=$?
obs_meta=$(printf 'pre_fence=%s\npost_fence=%s\npre_hash_exit=%s\npost_hash_exit=%s\npre_diff_exit=%s\npost_diff_exit=%s\nstale=%s\npage_filter_exit=%s' "$pre_fence" "$post_fence" "$pre_hash_exit" "$post_hash_exit" "$pre_diff_exit" "$post_diff_exit" "$stale" "$page_filter_exit")
obs_meta_bytes=${#obs_meta}
printf '%s\n' "$obs_meta"
printf 'WCDH1:O:%010d:%010d\n' 0 "$((obs_meta_bytes+1))"
if [ "$pre_hash_exit" -eq 0 ] && [ "$post_hash_exit" -eq 0 ] && [ "$pre_diff_exit" -eq 0 ] && [ "$post_diff_exit" -eq 0 ] && [ "$stale" -eq 0 ] && [ "$page_filter_exit" -eq 0 ] && [ "$pre_fence" = "$post_fence" ]; then
  exit 0
fi
exit 1
"#;
    Ok(script
        .replace("__PRELUDE__", &prelude)
        .replace("__PAGE_BUDGET__", &max_page_bytes.to_string())
        .replace("__MAX_HUNKS__", &max_hunks.to_string())
        .replace("__MAX_HUNK_LINES__", &max_hunk_lines.to_string())
        .replace(
            "__MAX_RECOVERY_HUNK_LINES__",
            &MAX_MAX_HUNK_LINES.to_string(),
        )
        .replace("__START_POSITION__", &start_position.to_string())
        .replace("__FRAGMENT_MODE__", &fragment_mode.to_string())
        .replace(
            "__FRAGMENT_LINE_POSITION__",
            &fragment_line_position.to_string(),
        )
        .replace("__EXPECTED_FENCE__", &expected_fence)
        .replace("__FINGERPRINT_COMMAND__", &fingerprint_command)
        .replace("__DIFF_COMMAND__", &diff_command))
}

#[derive(Debug)]
struct GitDiffHunksPageWire {
    diff: String,
    diff_exit: i32,
    next_position: usize,
    total_records: usize,
    has_more: bool,
    returned_hunks: usize,
    returned_file_records: usize,
    first_file_context_only: bool,
    page_hunk_limit: bool,
    hunk_line_limit: bool,
    page_byte_budget: bool,
    page_bytes: usize,
    truncated_hunks: Vec<usize>,
    line_ceiling_hunks: Vec<usize>,
    line_recoverable_hunks: Vec<usize>,
    fragment_mode: bool,
    fragment_found: bool,
    fragment_invalid: bool,
    fragment_progress_lines: usize,
    fragment_candidate_safe: bool,
    fragment_candidate_record: usize,
    fragment_candidate_line: usize,
    pre_fence: String,
    post_fence: String,
    pre_hash_exit: i32,
    post_hash_exit: i32,
    pre_diff_exit: i32,
    post_diff_exit: i32,
    stale: bool,
    page_filter_exit: i32,
}

fn parse_git_diff_hunks_wire_block(
    stdout: &str,
    end: usize,
    kind: u8,
) -> Option<(&str, &str, usize)> {
    let bytes = stdout.as_bytes();
    let trailer_start = end.checked_sub(GIT_DIFF_HUNKS_BLOCK_TRAILER_BYTES)?;
    let trailer = bytes.get(trailer_start..end)?;
    if trailer.get(..6)? != GIT_DIFF_HUNKS_BLOCK_MAGIC
        || trailer.get(6).copied()? != kind
        || trailer.get(7).copied()? != b':'
        || trailer.get(18).copied()? != b':'
        || trailer.get(29).copied()? != b'\n'
    {
        return None;
    }
    let data_wire_bytes = parse_fixed_decimal(trailer.get(8..18)?)?;
    let meta_wire_bytes = parse_fixed_decimal(trailer.get(19..29)?)?;
    let meta_start = trailer_start.checked_sub(meta_wire_bytes)?;
    let data_start = meta_start.checked_sub(data_wire_bytes)?;
    let data = std::str::from_utf8(bytes.get(data_start..meta_start)?).ok()?;
    let meta = std::str::from_utf8(bytes.get(meta_start..trailer_start)?).ok()?;
    Some((data, meta, data_start))
}

fn parse_required_i32(meta: &str, key: &str) -> Option<i32> {
    parse_status_result_field(meta, key)?.parse().ok()
}

fn parse_hunk_indices(meta: &str, key: &str, returned_hunks: usize) -> Option<Vec<usize>> {
    let raw = parse_status_result_field(meta, key)?;
    if raw.is_empty() {
        return Some(Vec::new());
    }
    let mut indices = Vec::new();
    for value in raw.split(',') {
        let index = value.parse::<usize>().ok()?;
        if index >= returned_hunks || indices.contains(&index) {
            return None;
        }
        indices.push(index);
    }
    Some(indices)
}

fn parse_framed_git_diff_hunks_stdout(
    stdout: &str,
    max_page_bytes: usize,
) -> Option<GitDiffHunksPageWire> {
    let mut cursor = stdout.len();
    let (obs_data, obs_meta, start) = parse_git_diff_hunks_wire_block(stdout, cursor, b'O')?;
    if !obs_data.is_empty() {
        return None;
    }
    cursor = start;
    let (diff, page_meta, start) = parse_git_diff_hunks_wire_block(stdout, cursor, b'P')?;
    if start != 0 || diff.len() > max_page_bytes {
        return None;
    }
    let obs_meta = strip_wire_lf(obs_meta)?;
    let page_meta = strip_wire_lf(page_meta)?;
    let page_bytes = parse_optional_usize(&page_meta, "page_bytes")?;
    if page_bytes != diff.len() {
        return None;
    }
    let returned_hunks = parse_optional_usize(&page_meta, "returned_hunks")?;
    let returned_file_records = parse_optional_usize(&page_meta, "returned_file_records")?;
    let truncated_hunks = parse_hunk_indices(&page_meta, "truncated_hunks", returned_hunks)?;
    let line_ceiling_hunks = parse_hunk_indices(&page_meta, "line_ceiling_hunks", returned_hunks)?;
    let line_recoverable_hunks =
        parse_hunk_indices(&page_meta, "line_recoverable_hunks", returned_hunks)?;
    Some(GitDiffHunksPageWire {
        diff: strip_wire_lf(diff)?,
        diff_exit: parse_required_i32(&page_meta, "diff_exit")?,
        next_position: parse_optional_usize(&page_meta, "next_position")?,
        total_records: parse_optional_usize(&page_meta, "total_records")?,
        has_more: parse_optional_bool(&page_meta, "has_more")?,
        returned_hunks,
        returned_file_records,
        first_file_context_only: parse_optional_bool(&page_meta, "first_file_context_only")?,
        page_hunk_limit: parse_optional_bool(&page_meta, "page_hunk_limit")?,
        hunk_line_limit: parse_optional_bool(&page_meta, "hunk_line_limit")?,
        page_byte_budget: parse_optional_bool(&page_meta, "page_byte_budget")?,
        page_bytes,
        truncated_hunks,
        line_ceiling_hunks,
        line_recoverable_hunks,
        fragment_mode: parse_optional_bool(&page_meta, "fragment_mode")?,
        fragment_found: parse_optional_bool(&page_meta, "fragment_found")?,
        fragment_invalid: parse_optional_bool(&page_meta, "fragment_invalid")?,
        fragment_progress_lines: parse_optional_usize(&page_meta, "fragment_progress_lines")?,
        fragment_candidate_safe: parse_optional_bool(&page_meta, "fragment_candidate_safe")?,
        fragment_candidate_record: parse_optional_usize(&page_meta, "fragment_candidate_record")?,
        fragment_candidate_line: parse_optional_usize(&page_meta, "fragment_candidate_line")?,
        pre_fence: parse_status_result_field(&obs_meta, "pre_fence")?.to_string(),
        post_fence: parse_status_result_field(&obs_meta, "post_fence")?.to_string(),
        pre_hash_exit: parse_required_i32(&obs_meta, "pre_hash_exit")?,
        post_hash_exit: parse_required_i32(&obs_meta, "post_hash_exit")?,
        pre_diff_exit: parse_required_i32(&obs_meta, "pre_diff_exit")?,
        post_diff_exit: parse_required_i32(&obs_meta, "post_diff_exit")?,
        stale: parse_optional_bool(&obs_meta, "stale")?,
        page_filter_exit: parse_required_i32(&obs_meta, "page_filter_exit")?,
    })
}

fn mark_git_diff_hunks_page_metadata(
    files: &mut [Value],
    truncated_hunks: &[usize],
    first_file_context_only: bool,
) -> usize {
    if first_file_context_only {
        if let Some(file) = files.first_mut().and_then(Value::as_object_mut) {
            file.insert("continued".to_string(), json!(true));
        }
    }
    let mut index = 0usize;
    for file in files {
        let Some(hunks) = file.get_mut("hunks").and_then(Value::as_array_mut) else {
            continue;
        };
        for hunk in hunks {
            if truncated_hunks.contains(&index) {
                if let Some(hunk) = hunk.as_object_mut() {
                    hunk.insert("truncated".to_string(), json!(true));
                }
            }
            index += 1;
        }
    }
    index
}

fn project_git_diff_hunk_fragment(files: &mut [Value]) -> bool {
    let mut projected = false;
    for file in files {
        let Some(hunks) = file.get_mut("hunks").and_then(Value::as_array_mut) else {
            continue;
        };
        for hunk in hunks {
            if projected {
                return false;
            }
            let Some(hunk) = hunk.as_object_mut() else {
                return false;
            };
            let Some(header) = hunk.get("header").and_then(Value::as_str) else {
                return false;
            };
            let Some(diff) = hunk.get("diff").and_then(Value::as_str) else {
                return false;
            };
            let prefix = format!("{header}\n");
            let Some(body) = diff.strip_prefix(&prefix) else {
                return false;
            };
            if body.is_empty() {
                return false;
            }
            let body = body.to_string();
            hunk.insert("diff".to_string(), json!(body));
            hunk.insert("line_count".to_string(), json!(body.lines().count()));
            hunk.insert("continued".to_string(), json!(true));
            projected = true;
        }
    }
    projected
}

fn strip_diff_prefix(path: &str) -> String {
    let decoded = decode_git_quoted_path(path).unwrap_or_else(|| path.to_string());
    decoded
        .strip_prefix("a/")
        .or_else(|| decoded.strip_prefix("b/"))
        .unwrap_or(&decoded)
        .to_string()
}

fn parse_binary_diff_paths(line: &str) -> Option<(Option<String>, Option<String>)> {
    let body = line
        .strip_prefix("Binary files ")?
        .strip_suffix(" differ")?;
    for (index, _) in body.match_indices(" and ") {
        let old_raw = &body[..index];
        let new_raw = &body[index + " and ".len()..];
        let old_path = (old_raw != "/dev/null").then(|| strip_diff_prefix(old_raw));
        let path = (new_raw != "/dev/null").then(|| strip_diff_prefix(new_raw));
        let plausible = match (&old_path, &path) {
            (None, Some(_)) | (Some(_), None) => true,
            (Some(old_path), Some(path)) => old_path == path,
            (None, None) => false,
        };
        if plausible {
            return Some((old_path, path));
        }
    }
    None
}

fn parse_hunk_header(header: &str) -> (i64, i64, i64, i64) {
    fn parse_range(raw: &str) -> (i64, i64) {
        let raw = raw.trim_start_matches(['-', '+']);
        let mut parts = raw.splitn(2, ',');
        let start = parts.next().unwrap_or("0").parse::<i64>().unwrap_or(0);
        let lines = parts.next().unwrap_or("1").parse::<i64>().unwrap_or(1);
        (start, lines)
    }
    let mut parts = header.split_whitespace();
    let _at = parts.next();
    let old = parts.next().unwrap_or("-0,0");
    let new = parts.next().unwrap_or("+0,0");
    let (old_start, old_lines) = parse_range(old);
    let (new_start, new_lines) = parse_range(new);
    (old_start, old_lines, new_start, new_lines)
}

fn finish_hunk(
    file: &mut serde_json::Map<String, serde_json::Value>,
    current_hunk: &mut Option<serde_json::Map<String, serde_json::Value>>,
    hunk_lines: &mut Vec<String>,
) {
    let Some(mut hunk) = current_hunk.take() else {
        return;
    };
    hunk.insert("diff".to_string(), json!(hunk_lines.join("\n")));
    hunk.insert("line_count".to_string(), json!(hunk_lines.len()));
    file.entry("hunks".to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .expect("hunks array")
        .push(json!(hunk));
    hunk_lines.clear();
}

fn finish_file(
    files: &mut Vec<serde_json::Value>,
    current_file: &mut Option<serde_json::Map<String, serde_json::Value>>,
    current_hunk: &mut Option<serde_json::Map<String, serde_json::Value>>,
    hunk_lines: &mut Vec<String>,
) {
    let Some(mut file) = current_file.take() else {
        return;
    };
    finish_hunk(&mut file, current_hunk, hunk_lines);
    if file.get("hunks").is_none() {
        file.insert("hunks".to_string(), json!([]));
    }
    files.push(json!(file));
}

pub(crate) fn parse_git_diff_hunks(
    diff: &str,
    max_hunks: usize,
    max_hunk_lines: usize,
) -> (Vec<serde_json::Value>, usize, bool) {
    let mut files = Vec::new();
    let mut current_file: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut current_hunk: Option<serde_json::Map<String, serde_json::Value>> = None;
    let mut hunk_lines = Vec::new();
    let mut hunk_count = 0usize;
    let mut truncated = false;
    let mut skip_current_hunk = false;

    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish_file(
                &mut files,
                &mut current_file,
                &mut current_hunk,
                &mut hunk_lines,
            );
            let mut parts = rest.split_whitespace();
            let old_path = parts.next().map(strip_diff_prefix).unwrap_or_default();
            let path = parts.next().map(strip_diff_prefix).unwrap_or_default();
            let mut file = serde_json::Map::new();
            file.insert("path".to_string(), json!(path));
            file.insert("old_path".to_string(), json!(old_path));
            file.insert("status".to_string(), json!("modified"));
            file.insert("hunks".to_string(), json!([]));
            current_file = Some(file);
            skip_current_hunk = false;
            continue;
        }

        let Some(file) = current_file.as_mut() else {
            continue;
        };

        if line.starts_with("new file mode ") {
            file.insert("status".to_string(), json!("added"));
        } else if line.starts_with("deleted file mode ") {
            file.insert("status".to_string(), json!("deleted"));
        } else if let Some(path) = line.strip_prefix("rename from ") {
            file.insert("old_path".to_string(), json!(strip_diff_prefix(path)));
            file.insert("status".to_string(), json!("renamed"));
        } else if let Some(path) = line.strip_prefix("rename to ") {
            file.insert("path".to_string(), json!(strip_diff_prefix(path)));
            file.insert("status".to_string(), json!("renamed"));
        } else if line.starts_with("Binary files ") {
            file.insert("binary".to_string(), json!(true));
            if file.get("status").and_then(Value::as_str) != Some("renamed") {
                if let Some((old_path, path)) = parse_binary_diff_paths(line) {
                    file.insert("old_path".to_string(), json!(old_path));
                    file.insert("path".to_string(), json!(path));
                    if file.get("old_path").is_some_and(Value::is_null) {
                        file.insert("status".to_string(), json!("added"));
                    } else if file.get("path").is_some_and(Value::is_null) {
                        file.insert("status".to_string(), json!("deleted"));
                    }
                }
            }
        } else if let Some(path) = line.strip_prefix("--- ") {
            if path == "/dev/null" {
                file.insert("old_path".to_string(), json!(null));
                file.insert("status".to_string(), json!("added"));
            } else {
                file.insert("old_path".to_string(), json!(strip_diff_prefix(path)));
            }
        } else if let Some(path) = line.strip_prefix("+++ ") {
            if path == "/dev/null" {
                file.insert("path".to_string(), json!(null));
                file.insert("status".to_string(), json!("deleted"));
            } else {
                file.insert("path".to_string(), json!(strip_diff_prefix(path)));
            }
        }

        if line.starts_with("@@ ") {
            finish_hunk(file, &mut current_hunk, &mut hunk_lines);
            if hunk_count >= max_hunks {
                truncated = true;
                skip_current_hunk = true;
                continue;
            }
            let (old_start, old_lines, new_start, new_lines) = parse_hunk_header(line);
            let mut hunk = serde_json::Map::new();
            hunk.insert("old_start".to_string(), json!(old_start));
            hunk.insert("old_lines".to_string(), json!(old_lines));
            hunk.insert("new_start".to_string(), json!(new_start));
            hunk.insert("new_lines".to_string(), json!(new_lines));
            hunk.insert("header".to_string(), json!(line));
            hunk.insert("truncated".to_string(), json!(false));
            current_hunk = Some(hunk);
            hunk_lines.push(line.to_string());
            hunk_count += 1;
            skip_current_hunk = false;
            continue;
        }

        if current_hunk.is_some() && !skip_current_hunk {
            if hunk_lines.len() < max_hunk_lines {
                hunk_lines.push(line.to_string());
            } else {
                truncated = true;
                if let Some(hunk) = current_hunk.as_mut() {
                    hunk.insert("truncated".to_string(), json!(true));
                }
            }
        }
    }
    finish_file(
        &mut files,
        &mut current_file,
        &mut current_hunk,
        &mut hunk_lines,
    );
    (files, hunk_count, truncated)
}

impl ToolRuntime {
    #[cfg(test)]
    pub(crate) async fn git_diff_hunks(
        &self,
        project: String,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        cached: Option<bool>,
    ) -> ToolResult {
        self.git_diff_hunks_continued(project, paths, max_hunks, max_hunk_lines, cached, None)
            .await
    }

    #[cfg(test)]
    pub(crate) async fn git_diff_hunks_continued(
        &self,
        project: String,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        cached: Option<bool>,
        continuation: Option<String>,
    ) -> ToolResult {
        self.git_diff_hunks_continued_with_range(
            project,
            paths,
            max_hunks,
            max_hunk_lines,
            cached,
            None,
            None,
            continuation,
        )
        .await
    }

    #[cfg(test)]
    pub(crate) async fn git_diff_hunks_continued_with_range(
        &self,
        project: String,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        cached: Option<bool>,
        base_commit: Option<String>,
        head_commit: Option<String>,
        continuation: Option<String>,
    ) -> ToolResult {
        self.git_diff_hunks_continued_with_range_and_page_bytes(
            project,
            paths,
            max_hunks,
            max_hunk_lines,
            None,
            cached,
            base_commit,
            head_commit,
            continuation,
        )
        .await
    }

    pub(crate) async fn git_diff_hunks_continued_with_range_and_page_bytes(
        &self,
        project: String,
        paths: Option<Vec<String>>,
        max_hunks: Option<usize>,
        max_hunk_lines: Option<usize>,
        max_page_bytes: Option<usize>,
        cached: Option<bool>,
        base_commit: Option<String>,
        head_commit: Option<String>,
        continuation: Option<String>,
    ) -> ToolResult {
        let paths = match clean_optional_paths(paths) {
            Ok(paths) => paths,
            Err(e) => return ToolResult::err(e),
        };
        let cached = cached.unwrap_or(false);
        if git_diff_hunks_paths_include_secret(&paths) {
            return git_diff_hunks_failure(&project, &paths, cached, "sensitive_path", None, "");
        }
        let committed_range = match normalize_git_diff_hunks_committed_range(
            base_commit.as_deref(),
            head_commit.as_deref(),
            cached,
        ) {
            Ok(range) => range,
            Err(reason) => {
                return git_diff_hunks_committed_failure(
                    &project,
                    &paths,
                    base_commit.as_deref(),
                    head_commit.as_deref(),
                    reason,
                    None,
                    "",
                )
            }
        };
        let max_hunks = max_hunks
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_HUNKS)
            .min(MAX_MAX_HUNKS);
        let max_hunk_lines = max_hunk_lines
            .filter(|n| *n > 0)
            .unwrap_or(DEFAULT_MAX_HUNK_LINES)
            .min(MAX_MAX_HUNK_LINES);
        let max_page_bytes = normalize_git_diff_hunks_page_bytes(max_page_bytes);
        if continuation
            .as_ref()
            .is_some_and(|value| value.len() > GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES)
        {
            return git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                "invalid_continuation",
                None,
                "",
            );
        }
        let resolved = match self.resolve_project_input(&project).await {
            Ok(resolved) => resolved,
            Err(error) => return error.into_tool_result(),
        };
        let committed_scope = match committed_range.as_ref() {
            Some((base, head)) => match self
                .resolve_committed_git_scope(&resolved.resolved_id, base, head)
                .await
            {
                Ok(scope) => Some(scope),
                Err(reason) => {
                    return git_diff_hunks_committed_failure(
                        &project,
                        &paths,
                        Some(base),
                        Some(head),
                        reason,
                        None,
                        "",
                    )
                }
            },
            None => None,
        };
        let scope = match committed_scope.as_ref() {
            Some(committed_scope) => git_diff_hunks_committed_scope_digest(
                &resolved.resolved_id,
                &paths,
                committed_scope,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
            ),
            None => git_diff_hunks_scope_digest(
                &resolved.resolved_id,
                &paths,
                cached,
                max_hunks,
                max_hunk_lines,
                max_page_bytes,
            ),
        };
        let mut command_paths = paths.clone();
        command_paths.sort();
        let decoded = match continuation.as_deref() {
            Some(raw) => {
                match decode_git_diff_hunks_continuation(
                    raw,
                    &scope,
                    committed_scope
                        .as_ref()
                        .map(|_| self.git_diff_hunks_continuation_mac_key.as_ref()),
                ) {
                    Ok(token) => Some(token),
                    Err(GitDiffHunksContinuationError::Invalid) => {
                        return git_diff_hunks_failure(
                            &project,
                            &paths,
                            cached,
                            "invalid_continuation",
                            None,
                            "",
                        )
                    }
                    Err(GitDiffHunksContinuationError::ScopeMismatch) => {
                        return git_diff_hunks_failure(
                            &project,
                            &paths,
                            cached,
                            "continuation_mismatch",
                            None,
                            "",
                        )
                    }
                }
            }
            None => None,
        };
        let (start_position, fragment_line_position) = match decoded.as_ref() {
            Some(GitDiffHunksContinuation::Page { next_record, .. }) => (*next_record, None),
            Some(GitDiffHunksContinuation::HunkFragment {
                record_index,
                next_line,
                ..
            }) => (*record_index, Some(*next_line)),
            None => (0, None),
        };
        let expected_fence = decoded.as_ref().map(GitDiffHunksContinuation::fence);
        let command = match git_diff_hunks_page_command(
            &command_paths,
            cached,
            start_position,
            fragment_line_position,
            max_hunks,
            max_hunk_lines,
            max_page_bytes,
            expected_fence,
            committed_scope.as_ref(),
        ) {
            Ok(command) => command,
            Err(_) => {
                return git_diff_hunks_source_failure(
                    &project,
                    &paths,
                    cached,
                    "source_dispatch_unavailable",
                    "command_build",
                    None,
                )
            }
        };
        let output = match self
            .run_project_internal_posix_script_capture(&resolved.resolved_id, command, 30, None)
            .await
        {
            Ok(output) => output,
            Err(_) => {
                return git_diff_hunks_source_failure(
                    &project,
                    &paths,
                    cached,
                    "source_dispatch_unavailable",
                    "runner_dispatch",
                    None,
                )
            }
        };
        if output.stdout_truncated {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_output_truncated",
                "runner_output",
                Some(&output),
            );
        }
        if output.execution_state != crate::runner_protocol::ShellCommandExecutionState::Completed
            || output.error.is_some()
        {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_execution_failed",
                "source_execution",
                Some(&output),
            );
        }
        let stderr = bounded_git_diff_hunks_stderr(&output.stderr);
        let Some(wire) = parse_framed_git_diff_hunks_stdout(&output.stdout, max_page_bytes) else {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                if output.exit_code == Some(0) {
                    "source_frame_invalid"
                } else {
                    "source_execution_failed"
                },
                if output.exit_code == Some(0) {
                    "frame_parse"
                } else {
                    "source_execution"
                },
                Some(&output),
            );
        };
        if wire.pre_hash_exit != 0
            || wire.post_hash_exit != 0
            || !is_git_object_hex(&wire.pre_fence)
            || !is_git_object_hex(&wire.post_fence)
        {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_fence_unavailable",
                "source_fence",
                Some(&output),
            );
        }
        if wire.stale
            || decoded
                .as_ref()
                .is_some_and(|token| token.fence() != wire.pre_fence)
        {
            return git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                "stale_continuation",
                output.exit_code,
                &stderr,
            );
        }
        if wire.pre_diff_exit != 0 || wire.post_diff_exit != 0 || wire.diff_exit != 0 {
            return git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                "git_diff_failed",
                output.exit_code,
                &stderr,
            );
        }
        if wire.pre_fence != wire.post_fence {
            return git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                if decoded.is_some() {
                    "stale_continuation"
                } else {
                    "source_changed_during_observation"
                },
                output.exit_code,
                &stderr,
            );
        }
        if wire.page_filter_exit != 0 || wire.page_bytes > max_page_bytes {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_page_filter_failed",
                "page_filter",
                Some(&output),
            );
        }
        // A valid WCDH frame carries more precise source-stage evidence than the
        // wrapper process exit. The generated script intentionally exits nonzero
        // for fence, source-change, and page-filter failures after emitting that
        // frame, so classify those facts first and use the wrapper exit only as a
        // fallback when the framed observation itself reports no specific fault.
        if output.exit_code != Some(0) {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_execution_failed",
                "source_execution",
                Some(&output),
            );
        }
        let fragment_requested = fragment_line_position.is_some();
        let fragment_shape_invalid = wire.fragment_mode != fragment_requested
            || (fragment_requested
                && (!wire.fragment_found
                    || wire.fragment_invalid
                    || wire.fragment_progress_lines == 0
                    || wire.returned_hunks != 1
                    || wire.next_position != start_position.saturating_add(1)))
            || (!fragment_requested
                && (wire.fragment_found
                    || wire.fragment_invalid
                    || wire.fragment_progress_lines != 0))
            || (wire.fragment_candidate_safe
                && (!(wire.hunk_line_limit || wire.page_byte_budget)
                    || wire.fragment_candidate_record == 0
                    || wire.fragment_candidate_line == 0
                    || wire.returned_hunks == 0
                    || wire.fragment_candidate_record.saturating_add(1) != wire.next_position))
            || (!wire.fragment_candidate_safe
                && (wire.fragment_candidate_record != 0 || wire.fragment_candidate_line != 0))
            || (fragment_requested
                && wire.fragment_candidate_safe
                && (wire.fragment_candidate_record != start_position
                    || wire.fragment_candidate_line
                        <= fragment_line_position.expect("fragment request line")));
        if start_position > wire.total_records
            || wire.next_position < start_position
            || wire.next_position > wire.total_records
            || (wire.has_more && wire.next_position <= start_position)
            || wire.returned_hunks > max_hunks
            || fragment_shape_invalid
        {
            if wire.has_more && wire.next_position <= start_position {
                return git_diff_hunks_failure(
                    &project,
                    &paths,
                    cached,
                    "page_record_too_large",
                    output.exit_code,
                    &stderr,
                );
            }
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_projection_inconsistent",
                "projection",
                Some(&output),
            );
        }
        let (mut files, parsed_hunks, parser_truncated) =
            parse_git_diff_hunks(&wire.diff, MAX_MAX_HUNKS, MAX_MAX_HUNK_LINES);
        if git_diff_hunks_files_include_secret(&files) {
            return git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                "sensitive_path",
                output.exit_code,
                "",
            );
        }
        let marked_hunks = mark_git_diff_hunks_page_metadata(
            &mut files,
            &wire.truncated_hunks,
            wire.first_file_context_only,
        );
        let fragment_projection_valid =
            !fragment_requested || project_git_diff_hunk_fragment(&mut files);
        let expected_files = wire.returned_file_records
            + usize::from(wire.first_file_context_only && wire.returned_hunks > 0);
        if parser_truncated
            || parsed_hunks != wire.returned_hunks
            || marked_hunks != wire.returned_hunks
            || !fragment_projection_valid
            || files.len() != expected_files
            || wire
                .line_ceiling_hunks
                .iter()
                .any(|index| !wire.truncated_hunks.contains(index))
            || wire
                .line_recoverable_hunks
                .iter()
                .any(|index| !wire.line_ceiling_hunks.contains(index))
            || (!wire.hunk_line_limit && !wire.line_recoverable_hunks.is_empty())
        {
            return git_diff_hunks_source_failure(
                &project,
                &paths,
                cached,
                "source_projection_inconsistent",
                "projection",
                Some(&output),
            );
        }
        let mut truncation_reasons = Vec::new();
        if wire.page_hunk_limit {
            truncation_reasons.push("page_hunk_limit");
        }
        if wire.hunk_line_limit {
            truncation_reasons.push("hunk_line_limit");
        }
        if wire.page_byte_budget {
            truncation_reasons.push("page_byte_budget");
        }
        let next_continuation = if wire.has_more {
            let encoded = if committed_scope.is_some() {
                encode_git_diff_hunks_committed_continuation(
                    self.git_diff_hunks_continuation_mac_key.as_ref(),
                    &scope,
                    &wire.pre_fence,
                    wire.next_position,
                )
            } else {
                encode_git_diff_hunks_continuation(&scope, &wire.pre_fence, wire.next_position)
            };
            match encoded {
                Ok(value) => Some(value),
                Err(_) => {
                    return git_diff_hunks_source_failure(
                        &project,
                        &paths,
                        cached,
                        "continuation_encoding_failed",
                        "continuation_encoding",
                        Some(&output),
                    )
                }
            }
        } else {
            None
        };
        let hunk_fragment_continuation = if wire.fragment_candidate_safe {
            let encoded = if committed_scope.is_some() {
                encode_git_diff_hunks_committed_hunk_fragment_continuation(
                    self.git_diff_hunks_continuation_mac_key.as_ref(),
                    &scope,
                    &wire.pre_fence,
                    wire.fragment_candidate_record,
                    wire.fragment_candidate_line,
                )
            } else {
                encode_git_diff_hunks_hunk_fragment_continuation(
                    &scope,
                    &wire.pre_fence,
                    wire.fragment_candidate_record,
                    wire.fragment_candidate_line,
                )
            };
            match encoded {
                Ok(value) => Some(value),
                Err(_) => {
                    return git_diff_hunks_source_failure(
                        &project,
                        &paths,
                        cached,
                        "continuation_encoding_failed",
                        "continuation_encoding",
                        Some(&output),
                    )
                }
            }
        } else {
            None
        };
        let recovery = git_diff_hunks_recovery_value(
            &project,
            &paths,
            cached,
            committed_scope.as_ref(),
            max_hunks,
            max_hunk_lines,
            max_page_bytes,
            &files,
            wire.page_hunk_limit,
            wire.hunk_line_limit,
            wire.page_byte_budget,
            &wire.truncated_hunks,
            &wire.line_ceiling_hunks,
            &wire.line_recoverable_hunks,
            next_continuation.as_deref(),
            hunk_fragment_continuation.as_deref(),
        );
        let mut payload = json!({
            "project": project,
            "paths": paths,
            "cached": cached,
            "max_page_bytes": max_page_bytes,
            "files": files,
            "hunk_count": parsed_hunks,
            "truncated": !truncation_reasons.is_empty(),
            "truncation_reasons": truncation_reasons,
            "has_more": wire.has_more,
            "exit_code": wire.diff_exit,
            "stderr": stderr,
        });
        if let Some(recovery) = recovery {
            payload["recovery"] = recovery;
        }
        if let Some(committed_scope) = committed_scope.as_ref() {
            if let Some(payload) = payload.as_object_mut() {
                payload.insert(
                    "scope".to_string(),
                    committed_git_diff_hunks_scope_value(committed_scope),
                );
            }
        }
        let result = ToolResult::ok(payload);
        if serde_json::to_vec(&result)
            .map(|bytes| {
                bytes.len()
                    <= MODEL_INSPECTION_MAX_RESULT_BYTES
                        .saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES)
            })
            .unwrap_or(false)
        {
            result
        } else {
            git_diff_hunks_failure(
                &project,
                &paths,
                cached,
                "output_budget_exceeded",
                Some(wire.diff_exit),
                "",
            )
        }
    }
}

#[cfg(test)]
mod continuation_token_tests {
    use super::*;

    fn decode_payload(token: &str) -> Vec<u8> {
        general_purpose::URL_SAFE_NO_PAD
            .decode(
                token
                    .strip_prefix(GIT_DIFF_HUNKS_V2_CONTINUATION_PREFIX)
                    .unwrap(),
            )
            .unwrap()
    }

    fn encode_payload(payload: &[u8]) -> String {
        format!(
            "{GIT_DIFF_HUNKS_V2_CONTINUATION_PREFIX}{}",
            general_purpose::URL_SAFE_NO_PAD.encode(payload)
        )
    }

    #[test]
    fn git_diff_hunks_v2_variants_round_trip_with_worst_case_bounds() {
        let scope = "f".repeat(64);
        let fence = "e".repeat(64);
        let key = [0xa5; 32];
        let worktree_page = encode_git_diff_hunks_continuation(&scope, &fence, usize::MAX).unwrap();
        let committed_page =
            encode_git_diff_hunks_committed_continuation(&key, &scope, &fence, usize::MAX).unwrap();
        let worktree_fragment = encode_git_diff_hunks_hunk_fragment_continuation(
            &scope,
            &fence,
            usize::MAX,
            usize::MAX,
        )
        .unwrap();
        let committed_fragment = encode_git_diff_hunks_committed_hunk_fragment_continuation(
            &key,
            &scope,
            &fence,
            usize::MAX,
            usize::MAX,
        )
        .unwrap();

        for token in [
            &worktree_page,
            &committed_page,
            &worktree_fragment,
            &committed_fragment,
        ] {
            assert!(token.starts_with("wcdh2."));
            assert!(token.len() < GIT_DIFF_HUNKS_CONTINUATION_MAX_BYTES);
        }
        assert_eq!(worktree_page.len(), 105);
        assert_eq!(committed_page.len(), 148);
        assert_eq!(worktree_fragment.len(), 116);
        assert_eq!(committed_fragment.len(), 158);

        assert_eq!(
            decode_git_diff_hunks_continuation(&worktree_page, &scope, None),
            Ok(GitDiffHunksContinuation::Page {
                fence: fence.clone(),
                next_record: usize::MAX,
            })
        );
        assert_eq!(
            decode_git_diff_hunks_continuation(&committed_page, &scope, Some(&key)),
            Ok(GitDiffHunksContinuation::Page {
                fence: fence.clone(),
                next_record: usize::MAX,
            })
        );
        assert_eq!(
            decode_git_diff_hunks_continuation(&worktree_fragment, &scope, None),
            Ok(GitDiffHunksContinuation::HunkFragment {
                fence: fence.clone(),
                record_index: usize::MAX,
                next_line: usize::MAX,
            })
        );
        assert_eq!(
            decode_git_diff_hunks_continuation(&committed_fragment, &scope, Some(&key)),
            Ok(GitDiffHunksContinuation::HunkFragment {
                fence,
                record_index: usize::MAX,
                next_line: usize::MAX,
            })
        );
    }

    #[test]
    fn git_diff_hunks_v2_rejects_malformed_scope_and_restart_mismatches() {
        let scope = "1".repeat(64);
        let fence = "2".repeat(40);
        let key = [3; 32];
        let other_key = [4; 32];
        let worktree = encode_git_diff_hunks_continuation(&scope, &fence, 7).unwrap();
        let committed =
            encode_git_diff_hunks_committed_continuation(&key, &scope, &fence, 7).unwrap();

        assert_eq!(
            decode_git_diff_hunks_continuation(&committed, &scope, Some(&other_key)),
            Err(GitDiffHunksContinuationError::Invalid)
        );
        assert!(decode_git_diff_hunks_continuation(&worktree, &scope, None).is_ok());
        assert_eq!(
            decode_git_diff_hunks_continuation(&worktree, &scope, Some(&key)),
            Err(GitDiffHunksContinuationError::ScopeMismatch)
        );
        assert_eq!(
            decode_git_diff_hunks_continuation(&worktree, &"3".repeat(64), None),
            Err(GitDiffHunksContinuationError::ScopeMismatch)
        );
        assert_eq!(
            decode_git_diff_hunks_continuation("wcdh1.e30", &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );
        assert_eq!(
            decode_git_diff_hunks_continuation("wcdh2.%%%", &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );

        let mut payload = decode_payload(&worktree);
        payload[0] = 0xff;
        assert_eq!(
            decode_git_diff_hunks_continuation(&encode_payload(&payload), &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );
        let mut payload = decode_payload(&worktree);
        payload[33] = 21;
        assert_eq!(
            decode_git_diff_hunks_continuation(&encode_payload(&payload), &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );
        let mut payload = decode_payload(&worktree);
        payload.pop();
        assert_eq!(
            decode_git_diff_hunks_continuation(&encode_payload(&payload), &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );
        let mut payload = decode_payload(&worktree);
        let cursor = 34 + payload[33] as usize;
        payload[cursor..cursor + 8].fill(0);
        assert_eq!(
            decode_git_diff_hunks_continuation(&encode_payload(&payload), &scope, None),
            Err(GitDiffHunksContinuationError::Invalid)
        );
    }

    #[test]
    fn git_diff_hunks_v2_committed_tampering_fails_closed() {
        let scope = "4".repeat(64);
        let fence = "5".repeat(64);
        let key = [6; 32];
        let token =
            encode_git_diff_hunks_committed_hunk_fragment_continuation(&key, &scope, &fence, 9, 11)
                .unwrap();
        let original = decode_payload(&token);
        let fence_len = original[33] as usize;
        let fence_start = 34;
        let cursor_start = fence_start + fence_len;
        let line_start = cursor_start + 8;
        let mac_start = line_start + 8;
        for offset in [1, fence_start, cursor_start + 7, line_start + 7, mac_start] {
            let mut payload = original.clone();
            payload[offset] ^= 1;
            assert_eq!(
                decode_git_diff_hunks_continuation(&encode_payload(&payload), &scope, Some(&key)),
                Err(GitDiffHunksContinuationError::Invalid),
                "tamper offset {offset} must fail closed"
            );
        }
    }
}
