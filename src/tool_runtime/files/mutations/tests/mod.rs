use super::patch::apply_patch_capability_rejection;
use super::patch_result::apply_patch_agent_stdout_result;
use super::text_edits::compact_apply_text_edits_path_policy_rejection;
use super::text_edits_result::apply_text_edits_agent_stdout_result;
use super::write::write_project_file_agent_stdout_result;
use super::*;

fn apply_patch_success_payload(
    matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
    match_mode: &str,
    candidate_count: u64,
    unique_match: bool,
    strict_match: bool,
) -> Value {
    json!({
        "dry_run": true,
        "requested_matching_mode": matching_mode.as_str(),
        "applied_count": 1,
        "changed": false,
        "state_changed": false,
        "execution_state": "completed",
        "would_change": true,
        "files": [{
            "index": 0,
            "kind": "edit",
            "path": "file.txt",
            "to_path": null,
            "old_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "new_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            "changed": false,
            "would_change": true,
            "edits": [{
                "chunk_index": 0,
                "change_context_present": false,
                "old_line_count": 1,
                "new_line_count": 1,
                "end_of_file": false,
                "match_mode": match_mode,
                "match_source": "old_lines",
                "matched_start_line": 1,
                "candidate_count": candidate_count,
                "unique_match": unique_match,
                "strict_match": strict_match,
            }]
        }],
        "changed_paths": ["file.txt"]
    })
}

fn one_update_patch() -> crate::apply_patch_shared::CodexPatch {
    crate::apply_patch_shared::parse_codex_patch(
        "*** Begin Patch\n*** Update File: file.txt\n-old\n+new\n*** End Patch",
    )
    .unwrap()
}

fn update_patch_with_old_line_count(count: usize) -> crate::apply_patch_shared::CodexPatch {
    assert!(count > 0);
    let mut patch = String::from("*** Begin Patch\n*** Update File: file.txt\n");
    for index in 0..count {
        patch.push_str(&format!("-old-{index}\n"));
    }
    patch.push_str("+new\n*** End Patch");
    crate::apply_patch_shared::parse_codex_patch(&patch).unwrap()
}

fn context_mismatch_payload(
    expected_line_count: usize,
    search_start_line: usize,
    available_line_count: usize,
    closest_start_line: Option<usize>,
) -> Value {
    json!({
        "changed": false,
        "state_changed": false,
        "execution_state": "not_started",
        "error_kind": "context_mismatch",
        "change_index": 0,
        "path": "file.txt",
        "error": "Rejected Codex patch before write: context mismatch. No files were modified.",
        "match_diagnostic": {
            "chunk_index": 0,
            "match_source": "old_lines",
            "search_start_line": search_start_line,
            "expected_line_count": expected_line_count,
            "available_line_count": available_line_count,
            "closest_start_line": closest_start_line,
            "closest_exact_line_matches": 0,
            "closest_trim_end_line_matches": 0,
            "closest_trim_line_matches": 0,
            "first_exact_mismatch_offset": 1
        }
    })
}

fn matching_rejection_payload(
    matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
    match_mode: &str,
    candidate_start_lines: &[usize],
    candidate_count: usize,
    source_line_count: usize,
) -> Value {
    let ambiguous = candidate_count > 1;
    json!({
        "changed": false,
        "state_changed": false,
        "execution_state": "not_started",
        "error_kind": "matching_mode_rejected",
        "change_index": 0,
        "path": "file.txt",
        "chunk_index": 0,
        "requested_matching_mode": matching_mode.as_str(),
        "match_mode": match_mode,
        "match_source": "old_lines",
        "matched_start_line": if ambiguous {
            Value::Null
        } else {
            json!(candidate_start_lines.first().copied())
        },
        "candidate_count": candidate_count,
        "candidate_start_lines": candidate_start_lines,
        "candidate_positions_truncated": candidate_count > crate::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS,
        "matching_mode_satisfied": false,
        "search_start_line": 1,
        "source_line_count": source_line_count,
        "recovery_action": "RUNNER_MUST_NOT_CHOOSE_RECOVERY",
        "retry_guidance": "RUNNER_MUST_NOT_CHOOSE_GUIDANCE",
        "error": "RUNNER_MUST_NOT_CHOOSE_ERROR",
    })
}
mod fixture;
pub(crate) use fixture::apply_text_edits_to_string;
mod patch_recovery;
mod patch_result;
mod text_edits;
mod write;
