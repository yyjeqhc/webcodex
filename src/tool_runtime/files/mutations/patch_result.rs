//! Validation and bounded recovery projection for untrusted patch receipts.

use super::lifecycle::{
    structured_edit_outcome_unknown_result, transactional_edit_agent_stdout_result,
};
use super::*;

fn apply_patch_sha256(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        value.len() == 64
            && value
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    })
}

fn apply_patch_nullable_sha256(value: Option<&Value>, required: bool) -> bool {
    match (value, required) {
        (Some(value), true) => apply_patch_sha256(value),
        (Some(Value::Null), false) => true,
        _ => false,
    }
}

const APPLY_PATCH_SUCCESS_TOP_LEVEL_FIELDS: [&str; 9] = [
    "dry_run",
    "applied_count",
    "changed",
    "state_changed",
    "execution_state",
    "would_change",
    "files",
    "changed_paths",
    "requested_matching_mode",
];

const APPLY_PATCH_SUCCESS_FILE_FIELDS: [&str; 9] = [
    "index",
    "kind",
    "path",
    "to_path",
    "old_sha256",
    "new_sha256",
    "changed",
    "would_change",
    "edits",
];

const APPLY_PATCH_SUCCESS_EDIT_FIELDS: [&str; 11] = [
    "chunk_index",
    "change_context_present",
    "old_line_count",
    "new_line_count",
    "end_of_file",
    "match_mode",
    "match_source",
    "matched_start_line",
    "candidate_count",
    "unique_match",
    "strict_match",
];

const APPLY_PATCH_FAILURE_TOP_LEVEL_FIELDS: [&str; 19] = [
    "changed",
    "state_changed",
    "execution_state",
    "error_kind",
    "failure_kind",
    "tool_failure",
    "recovery_action",
    "recovery_kind",
    "recovery_tool",
    "rollback_complete",
    "change_index",
    "kind",
    "path",
    "patch_line",
    "expected_format",
    "retry_guidance",
    "error",
    "match_diagnostic",
    "capability",
];

const APPLY_PATCH_FAILURE_MATCH_DIAGNOSTIC_FIELDS: [&str; 10] = [
    "chunk_index",
    "match_source",
    "search_start_line",
    "expected_line_count",
    "available_line_count",
    "closest_start_line",
    "closest_exact_line_matches",
    "closest_trim_end_line_matches",
    "closest_trim_line_matches",
    "first_exact_mismatch_offset",
];

const APPLY_PATCH_RECOVERY_MARGIN_BEFORE: usize = 8;
const APPLY_PATCH_RECOVERY_MARGIN_AFTER: usize = 8;

#[derive(Debug)]
struct ValidatedApplyPatchMatchRejection {
    change_index: usize,
    chunk_index: usize,
    path: String,
    requested_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
    match_mode: &'static str,
    match_source: &'static str,
    matched_start_line: Option<usize>,
    candidate_count: usize,
    candidate_start_lines: Vec<usize>,
    candidate_positions_truncated: bool,
    expected_line_count: usize,
    source_line_count: usize,
    classification: &'static str,
}

fn expected_apply_patch_failure_pattern_len(
    hunk: &crate::apply_patch_shared::CodexPatchHunk,
    chunk_index: usize,
    match_source: &str,
) -> Option<usize> {
    let crate::apply_patch_shared::CodexPatchHunk::UpdateFile { chunks, .. } = hunk else {
        return None;
    };
    let chunk = chunks.get(chunk_index)?;
    match match_source {
        "change_context" => chunk.change_context.as_ref().map(|_| 1),
        "old_lines" if !chunk.old_lines.is_empty() => {
            let count = chunk.old_lines.len()
                - usize::from(chunk.old_lines.last().is_some_and(String::is_empty));
            (count > 0).then_some(count)
        }
        _ => None,
    }
}

fn validated_apply_patch_match_rejection(
    patch: &crate::apply_patch_shared::CodexPatch,
    failure_output: &Value,
    expected_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) -> Option<ValidatedApplyPatchMatchRejection> {
    if expected_matching_mode == crate::apply_patch_shared::ApplyPatchMatchingMode::FirstMatch
        || failure_output.get("changed").and_then(Value::as_bool) != Some(false)
        || failure_output.get("state_changed").and_then(Value::as_bool) != Some(false)
        || failure_output
            .get("execution_state")
            .and_then(Value::as_str)
            != Some("not_started")
        || failure_output.get("error_kind").and_then(Value::as_str)
            != Some("matching_mode_rejected")
        || failure_output
            .get("requested_matching_mode")
            .and_then(Value::as_str)
            != Some(expected_matching_mode.as_str())
        || failure_output
            .get("matching_mode_satisfied")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return None;
    }

    let change_index = failure_output
        .get("change_index")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let hunk = patch.hunks.get(change_index)?;
    if failure_output.get("path").and_then(Value::as_str) != Some(hunk.path()) {
        return None;
    }
    let crate::apply_patch_shared::CodexPatchHunk::UpdateFile { chunks, .. } = hunk else {
        return None;
    };
    let chunk_index = failure_output
        .get("chunk_index")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let chunk = chunks.get(chunk_index)?;
    let match_source = match failure_output.get("match_source").and_then(Value::as_str)? {
        "old_lines" if !chunk.old_lines.is_empty() => "old_lines",
        "change_context"
            if chunk.change_context.is_some()
                && (expected_matching_mode
                    == crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique
                    || chunk.old_lines.is_empty()) =>
        {
            "change_context"
        }
        _ => {
            // Unanchored append performs no text matching and is strict-safe;
            // other sources contradict the parsed chunk shape or the selected
            // matching mode's positioning semantics.
            return None;
        }
    };
    let expected_line_count =
        expected_apply_patch_failure_pattern_len(hunk, chunk_index, match_source)?;
    let match_mode = match failure_output.get("match_mode").and_then(Value::as_str)? {
        "exact" => "exact",
        "trim_end" => "trim_end",
        "trim" => "trim",
        "normalized" => "normalized",
        _ => return None,
    };
    let search_start_line = failure_output
        .get("search_start_line")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let source_line_count = failure_output
        .get("source_line_count")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    if search_start_line == 0 || source_line_count < expected_line_count {
        return None;
    }
    let last_start_line = source_line_count
        .checked_sub(expected_line_count)?
        .checked_add(1)?;
    if search_start_line > last_start_line {
        return None;
    }
    if expected_matching_mode == crate::apply_patch_shared::ApplyPatchMatchingMode::Unique
        && match_source == "old_lines"
        && chunk.is_end_of_file
        && search_start_line != last_start_line
    {
        // Under Unique, *** End of File is a structural eligibility fence for
        // old_lines. A Runner cannot expand that eligible range back toward the
        // beginning of the file by forging search_start_line metadata.
        return None;
    }
    let max_candidate_count = last_start_line
        .checked_sub(search_start_line)?
        .checked_add(1)?;
    let candidate_count = failure_output
        .get("candidate_count")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    if candidate_count == 0 || candidate_count > max_candidate_count {
        return None;
    }

    let candidate_positions_truncated = failure_output
        .get("candidate_positions_truncated")?
        .as_bool()?;
    let candidate_start_lines = failure_output
        .get("candidate_start_lines")?
        .as_array()?
        .iter()
        .map(|value| value.as_u64().and_then(|value| usize::try_from(value).ok()))
        .collect::<Option<Vec<_>>>()?;
    if candidate_start_lines.is_empty()
        || candidate_start_lines.len()
            > crate::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS
        || candidate_start_lines
            .windows(2)
            .any(|pair| pair[0] >= pair[1])
        || candidate_start_lines
            .iter()
            .any(|line| *line < search_start_line || *line > last_start_line)
    {
        return None;
    }
    let position_cap = crate::apply_patch_shared::MAX_CODEX_PATCH_CANDIDATE_POSITIONS;
    if candidate_count <= position_cap {
        if candidate_positions_truncated || candidate_start_lines.len() != candidate_count {
            return None;
        }
    } else if !candidate_positions_truncated || candidate_start_lines.len() != position_cap {
        return None;
    }

    let classification = if candidate_count == 1 {
        if expected_matching_mode != crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique
            || match_mode == "exact"
        {
            return None;
        }
        "unique_fuzzy_candidate"
    } else {
        "ambiguous_candidate"
    };
    let matched_start_line = match classification {
        "unique_fuzzy_candidate" => {
            let line = failure_output
                .get("matched_start_line")?
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())?;
            if candidate_start_lines.as_slice() != [line] {
                return None;
            }
            Some(line)
        }
        "ambiguous_candidate" => {
            if failure_output.get("matched_start_line") != Some(&Value::Null) {
                return None;
            }
            None
        }
        _ => return None,
    };
    Some(ValidatedApplyPatchMatchRejection {
        change_index,
        chunk_index,
        path: hunk.path().to_string(),
        requested_matching_mode: expected_matching_mode,
        match_mode,
        match_source,
        matched_start_line,
        candidate_count,
        candidate_start_lines,
        candidate_positions_truncated,
        expected_line_count,
        classification,
        source_line_count,
    })
}

fn apply_patch_match_rejection_recovery(
    rejection: &ValidatedApplyPatchMatchRejection,
) -> Option<Value> {
    let requested_limit = rejection
        .expected_line_count
        .saturating_add(APPLY_PATCH_RECOVERY_MARGIN_BEFORE)
        .saturating_add(APPLY_PATCH_RECOVERY_MARGIN_AFTER)
        .min(crate::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES)
        .max(1);
    let mut items = Vec::with_capacity(rejection.candidate_start_lines.len());
    for candidate_start_line in &rejection.candidate_start_lines {
        let start_line = candidate_start_line
            .saturating_sub(APPLY_PATCH_RECOVERY_MARGIN_BEFORE)
            .max(1);
        let available_from_start = rejection
            .source_line_count
            .checked_sub(start_line)?
            .checked_add(1)?;
        let limit = requested_limit.min(available_from_start);
        if limit == 0 {
            return None;
        }
        items.push(json!({
            "path": rejection.path.as_str(),
            "start_line": start_line,
            "limit": limit,
        }));
    }
    if items.is_empty() {
        return None;
    }
    Some(json!({
        "action": "read_files",
        "reason": if rejection.classification == "ambiguous_candidate" {
            "matching_mode_rejected_ambiguous"
        } else {
            "matching_mode_rejected_unique_fuzzy"
        },
        "items": items,
        "change_index": rejection.change_index,
        "chunk_index": rejection.chunk_index,
    }))
}

fn apply_patch_match_rejection_diagnostic(rejection: &ValidatedApplyPatchMatchRejection) -> Value {
    json!({
        "classification": rejection.classification,
        "requested_matching_mode": rejection.requested_matching_mode.as_str(),
        "chunk_index": rejection.chunk_index,
        "match_mode": rejection.match_mode,
        "match_source": rejection.match_source,
        "matched_start_line": rejection.matched_start_line,
        "candidate_count": rejection.candidate_count,
        "candidate_start_lines": rejection.candidate_start_lines,
        "candidate_positions_truncated": rejection.candidate_positions_truncated,
        "expected_line_count": rejection.expected_line_count,
        "matching_mode_satisfied": false,
    })
}

fn valid_apply_patch_failure_match_diagnostic(
    value: &Value,
    patch: &crate::apply_patch_shared::CodexPatch,
    failure_output: &Value,
) -> bool {
    let Some(diagnostic) = value.as_object() else {
        return false;
    };
    if diagnostic.len() != APPLY_PATCH_FAILURE_MATCH_DIAGNOSTIC_FIELDS.len()
        || !diagnostic
            .keys()
            .all(|key| APPLY_PATCH_FAILURE_MATCH_DIAGNOSTIC_FIELDS.contains(&key.as_str()))
        || failure_output.get("error_kind").and_then(Value::as_str) != Some("context_mismatch")
    {
        return false;
    }

    let Some(change_index) = failure_output
        .get("change_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    else {
        return false;
    };
    let Some(hunk) = patch.hunks.get(change_index) else {
        return false;
    };
    if failure_output.get("path").and_then(Value::as_str) != Some(hunk.path()) {
        return false;
    }
    let Some(chunk_index) = diagnostic
        .get("chunk_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
    else {
        return false;
    };
    let Some(match_source) = diagnostic.get("match_source").and_then(Value::as_str) else {
        return false;
    };
    let Some(expected_pattern_len) =
        expected_apply_patch_failure_pattern_len(hunk, chunk_index, match_source)
    else {
        return false;
    };
    let Some(expected_line_count) = diagnostic
        .get("expected_line_count")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .filter(|count| *count == expected_pattern_len)
    else {
        return false;
    };
    let Some(search_start_line) = diagnostic
        .get("search_start_line")
        .and_then(Value::as_u64)
        .filter(|line| *line >= 1)
    else {
        return false;
    };
    let Some(available_line_count) = diagnostic
        .get("available_line_count")
        .and_then(Value::as_u64)
    else {
        return false;
    };
    let Some(exact) = diagnostic
        .get("closest_exact_line_matches")
        .and_then(Value::as_u64)
    else {
        return false;
    };
    let Some(trim_end) = diagnostic
        .get("closest_trim_end_line_matches")
        .and_then(Value::as_u64)
    else {
        return false;
    };
    let Some(trim) = diagnostic
        .get("closest_trim_line_matches")
        .and_then(Value::as_u64)
    else {
        return false;
    };
    let expected_line_count = expected_line_count as u64;
    if !(exact <= trim_end && trim_end <= trim && trim < expected_line_count) {
        return false;
    }

    let closest_start_valid = match diagnostic.get("closest_start_line") {
        Some(Value::Null) => available_line_count == 0 && exact == 0 && trim_end == 0 && trim == 0,
        Some(value) => value.as_u64().is_some_and(|line| {
            available_line_count > 0
                && line >= search_start_line
                && line < search_start_line.saturating_add(available_line_count)
        }),
        None => false,
    };
    let mismatch_valid = diagnostic
        .get("first_exact_mismatch_offset")
        .and_then(Value::as_u64)
        .is_some_and(|offset| (1..=expected_line_count).contains(&offset));
    closest_start_valid && mismatch_valid
}

fn apply_patch_context_mismatch_recovery(
    patch: &crate::apply_patch_shared::CodexPatch,
    failure_output: &Value,
) -> Option<Value> {
    if failure_output.get("changed").and_then(Value::as_bool) != Some(false)
        || failure_output.get("state_changed").and_then(Value::as_bool) != Some(false)
        || failure_output
            .get("execution_state")
            .and_then(Value::as_str)
            != Some("not_started")
        || failure_output.get("error_kind").and_then(Value::as_str) != Some("context_mismatch")
    {
        return None;
    }

    let diagnostic = failure_output.get("match_diagnostic")?;
    if !valid_apply_patch_failure_match_diagnostic(diagnostic, patch, failure_output) {
        return None;
    }
    let change_index = failure_output
        .get("change_index")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let hunk = patch.hunks.get(change_index)?;
    let path = hunk.path();
    let diagnostic = diagnostic.as_object()?;
    let chunk_index = diagnostic
        .get("chunk_index")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let search_start_line = diagnostic
        .get("search_start_line")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let expected_line_count = diagnostic
        .get("expected_line_count")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let available_line_count = diagnostic
        .get("available_line_count")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let closest_start_line = diagnostic
        .get("closest_start_line")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;
    let first_exact_mismatch_offset = diagnostic
        .get("first_exact_mismatch_offset")?
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())?;

    let total_line_count = search_start_line
        .checked_sub(1)?
        .checked_add(available_line_count)?;
    if total_line_count == 0 || closest_start_line > total_line_count {
        return None;
    }

    let mismatch_line = closest_start_line
        .checked_add(first_exact_mismatch_offset.checked_sub(1)?)?
        .min(total_line_count);
    let initial_start_line = closest_start_line
        .saturating_sub(APPLY_PATCH_RECOVERY_MARGIN_BEFORE)
        .max(1);
    let requested_limit = expected_line_count
        .saturating_add(APPLY_PATCH_RECOVERY_MARGIN_BEFORE)
        .saturating_add(APPLY_PATCH_RECOVERY_MARGIN_AFTER)
        .min(crate::apply_patch_shared::MAX_CODEX_PATCH_RECOVERY_READ_LINES);
    // Short hunks retain context around the candidate start. Once the bounded
    // read cap applies, shift only as far as needed to include the first known
    // mismatch plus useful trailing context; otherwise a large stale hunk can
    // return a window containing only lines that still match.
    let desired_end_line = mismatch_line
        .saturating_add(APPLY_PATCH_RECOVERY_MARGIN_AFTER)
        .min(total_line_count);
    let mismatch_centered_start = desired_end_line
        .saturating_sub(requested_limit.saturating_sub(1))
        .max(1);
    let start_line = initial_start_line.max(mismatch_centered_start);
    let available_from_start = total_line_count.checked_sub(start_line)?.checked_add(1)?;
    let limit = requested_limit.min(available_from_start);
    if limit == 0 {
        return None;
    }

    Some(json!({
        "action": "read_files",
        "reason": "context_mismatch",
        "items": [{
            "path": path,
            "start_line": start_line,
            "limit": limit,
        }],
        "change_index": change_index,
        "chunk_index": chunk_index,
    }))
}

fn sanitize_apply_patch_match_rejection(
    result: &mut ToolResult,
    patch: &crate::apply_patch_shared::CodexPatch,
    expected_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) -> bool {
    if result.output.get("error_kind").and_then(Value::as_str) != Some("matching_mode_rejected") {
        return false;
    }
    let validated =
        validated_apply_patch_match_rejection(patch, &result.output, expected_matching_mode);
    let (message, recovery_action, retry_guidance) = match validated.as_ref() {
        Some(rejection)
            if rejection.requested_matching_mode
                == crate::apply_patch_shared::ApplyPatchMatchingMode::Unique =>
        {
            (
                "Rejected Codex patch before write: Server-validated positioning is ambiguous under matching_mode=unique. No files were modified.".to_string(),
                "read_equal_candidates_and_refine_context",
                "read every recovery.items window as an equal candidate, then add a stable parent/function/test/module anchor or small surrounding context and retry with matching_mode=unique; do not choose a candidate from its order",
            )
        }
        Some(rejection) if rejection.classification == "unique_fuzzy_candidate" => (
            "Rejected Codex patch before write: matching_mode=exact_unique found one non-exact candidate. No files were modified.".to_string(),
            "reread_and_regenerate_exact_unique_patch",
            "read recovery.items, regenerate this chunk from exact current source, and retry with matching_mode=exact_unique; do not downgrade the requested fence",
        ),
        Some(_) => (
            "Rejected Codex patch before write: Server-validated positioning is ambiguous under matching_mode=exact_unique. No files were modified.".to_string(),
            "read_equal_candidates_and_add_exact_context",
            "read every recovery.items window as an equal candidate, expand exact context until the target is unique, and retry with matching_mode=exact_unique; do not choose a candidate from its order",
        ),
        None => (
            "Rejected Codex patch before write: Runner matching metadata was invalid or contradictory and was suppressed. No files were modified.".to_string(),
            "reread_and_regenerate_patch",
            "do not trust the rejected target metadata; reread current source through normal read tooling, regenerate context, and retry with the same matching_mode",
        ),
    };

    if let Some(fields) = result.output.as_object_mut() {
        fields.retain(|key, _| APPLY_PATCH_FAILURE_TOP_LEVEL_FIELDS.contains(&key.as_str()));
        for key in [
            "change_index",
            "kind",
            "path",
            "patch_line",
            "expected_format",
            "match_diagnostic",
            "capability",
            "recovery_action",
            "recovery_kind",
            "recovery_tool",
            "retry_guidance",
            "error",
        ] {
            fields.remove(key);
        }
        fields.insert(
            "requested_matching_mode".to_string(),
            json!(expected_matching_mode.as_str()),
        );
        fields.insert("recovery_action".to_string(), json!(recovery_action));
        fields.insert("retry_guidance".to_string(), json!(retry_guidance));
        fields.insert("error".to_string(), json!(message.as_str()));
        if let Some(rejection) = validated.as_ref() {
            fields.insert("change_index".to_string(), json!(rejection.change_index));
            fields.insert("path".to_string(), json!(rejection.path.as_str()));
            fields.insert(
                "match_rejection_diagnostic".to_string(),
                apply_patch_match_rejection_diagnostic(rejection),
            );
            if let Some(recovery) = apply_patch_match_rejection_recovery(rejection) {
                fields.insert("recovery".to_string(), recovery);
            }
        }
    }
    result.error = Some(message);
    true
}

fn sanitize_apply_patch_failure_metadata(
    result: &mut ToolResult,
    patch: &crate::apply_patch_shared::CodexPatch,
    expected_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) {
    if result.output.get("error_kind").and_then(Value::as_str) == Some("matching_mode_rejected") {
        let _ = sanitize_apply_patch_match_rejection(result, patch, expected_matching_mode);
        return;
    }
    if result.output.get("error_kind").and_then(Value::as_str) == Some("strict_match_rejected") {
        let message = "Rejected apply_patch result: a current matching_mode request received legacy strict-match metadata; target metadata was suppressed.".to_string();
        if let Some(fields) = result.output.as_object_mut() {
            fields.retain(|key, _| {
                matches!(
                    key.as_str(),
                    "changed" | "state_changed" | "execution_state" | "error_kind" | "tool_failure"
                )
            });
            fields.insert(
                "requested_matching_mode".to_string(),
                json!(expected_matching_mode.as_str()),
            );
            fields.insert(
                "recovery_action".to_string(),
                json!("reread_and_regenerate_patch"),
            );
            fields.insert(
                "retry_guidance".to_string(),
                json!("do not trust legacy target metadata; reread current source and retry with the same matching_mode"),
            );
            fields.insert("error".to_string(), json!(message.as_str()));
        }
        result.error = Some(message);
        return;
    }

    let output = &mut result.output;
    let recovery = apply_patch_context_mismatch_recovery(patch, output);
    let diagnostic_valid = output
        .get("match_diagnostic")
        .is_none_or(|value| valid_apply_patch_failure_match_diagnostic(value, patch, output));
    let Some(fields) = output.as_object_mut() else {
        return;
    };
    fields.retain(|key, _| APPLY_PATCH_FAILURE_TOP_LEVEL_FIELDS.contains(&key.as_str()));
    fields.insert(
        "requested_matching_mode".to_string(),
        json!(expected_matching_mode.as_str()),
    );
    if !diagnostic_valid {
        fields.remove("match_diagnostic");
    }
    if let Some(recovery) = recovery {
        fields.insert("recovery".to_string(), recovery);
    }
}

fn sanitize_apply_patch_success_metadata(output: &mut Value) {
    let Some(top_level) = output.as_object_mut() else {
        return;
    };
    top_level.retain(|key, _| APPLY_PATCH_SUCCESS_TOP_LEVEL_FIELDS.contains(&key.as_str()));
    let Some(files) = top_level.get_mut("files").and_then(Value::as_array_mut) else {
        return;
    };
    for file in files {
        let Some(file) = file.as_object_mut() else {
            continue;
        };
        file.retain(|key, _| APPLY_PATCH_SUCCESS_FILE_FIELDS.contains(&key.as_str()));
        let Some(edits) = file.get_mut("edits").and_then(Value::as_array_mut) else {
            continue;
        };
        for edit in edits {
            if let Some(edit) = edit.as_object_mut() {
                edit.retain(|key, _| APPLY_PATCH_SUCCESS_EDIT_FIELDS.contains(&key.as_str()));
            }
        }
    }
}

fn validate_apply_patch_edit_summary(
    value: &Value,
    chunk_index: usize,
    chunk: &crate::apply_patch_shared::CodexPatchChunk,
    matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) -> bool {
    let Some(edit) = value.as_object() else {
        return false;
    };
    if edit.get("chunk_index").and_then(Value::as_u64) != Some(chunk_index as u64)
        || edit.get("change_context_present").and_then(Value::as_bool)
            != Some(chunk.change_context.is_some())
        || edit.get("old_line_count").and_then(Value::as_u64) != Some(chunk.old_lines.len() as u64)
        || edit.get("new_line_count").and_then(Value::as_u64) != Some(chunk.new_lines.len() as u64)
        || edit.get("end_of_file").and_then(Value::as_bool) != Some(chunk.is_end_of_file)
        || !edit
            .get("matched_start_line")
            .and_then(Value::as_u64)
            .is_some_and(|line| line >= 1)
    {
        return false;
    }

    let expected_source = if !chunk.old_lines.is_empty() {
        "old_lines"
    } else if chunk.change_context.is_some() {
        "change_context"
    } else {
        "append"
    };
    if edit.get("match_source").and_then(Value::as_str) != Some(expected_source) {
        return false;
    }

    let match_mode = edit.get("match_mode");
    let candidate_count = edit.get("candidate_count");
    let positioning_shape_valid = if expected_source == "append" {
        match_mode == Some(&Value::Null) && candidate_count == Some(&Value::Null)
    } else {
        match_mode
            .and_then(Value::as_str)
            .is_some_and(|mode| matches!(mode, "exact" | "trim_end" | "trim" | "normalized"))
            && candidate_count
                .and_then(Value::as_u64)
                .is_some_and(|count| count >= 1)
    };
    if !positioning_shape_valid {
        return false;
    }

    let Some(unique_match) = edit.get("unique_match").and_then(Value::as_bool) else {
        return false;
    };
    let Some(strict_match) = edit.get("strict_match").and_then(Value::as_bool) else {
        return false;
    };
    if expected_source == "append" {
        return unique_match && strict_match;
    }
    let candidate_is_unique = candidate_count.and_then(Value::as_u64) == Some(1);
    if unique_match && !candidate_is_unique {
        return false;
    }
    if unique_match != candidate_is_unique {
        return false;
    }
    if strict_match
        && (match_mode.and_then(Value::as_str) != Some("exact")
            || !candidate_is_unique
            || !unique_match)
    {
        return false;
    }
    match matching_mode {
        crate::apply_patch_shared::ApplyPatchMatchingMode::FirstMatch => true,
        crate::apply_patch_shared::ApplyPatchMatchingMode::Unique => unique_match,
        crate::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique => strict_match,
    }
}

fn validate_apply_patch_success_metadata(
    output: &Value,
    patch: &crate::apply_patch_shared::CodexPatch,
    expected_dry_run: bool,
    expected_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) -> bool {
    if !output.is_object()
        || output
            .get("requested_matching_mode")
            .and_then(Value::as_str)
            != Some(expected_matching_mode.as_str())
    {
        return false;
    }
    let Some(files) = output.get("files").and_then(Value::as_array) else {
        return false;
    };
    if files.len() != patch.hunks.len() {
        return false;
    }

    let mut expected_changed_paths = Vec::new();
    let mut any_would_change = false;
    for (index, (file, hunk)) in files.iter().zip(&patch.hunks).enumerate() {
        let Some(file) = file.as_object() else {
            return false;
        };
        let (expected_kind, expected_path, expected_to_path, expected_chunks, old_sha, new_sha) =
            match hunk {
                crate::apply_patch_shared::CodexPatchHunk::AddFile { path, .. } => {
                    ("create", path.as_str(), None, None, false, true)
                }
                crate::apply_patch_shared::CodexPatchHunk::DeleteFile { path } => {
                    ("delete", path.as_str(), None, None, true, false)
                }
                crate::apply_patch_shared::CodexPatchHunk::UpdateFile {
                    path,
                    move_path,
                    chunks,
                } => {
                    let destination = move_path
                        .as_deref()
                        .filter(|destination| *destination != path.as_str());
                    (
                        if destination.is_some() {
                            "rename"
                        } else {
                            "edit"
                        },
                        path.as_str(),
                        destination,
                        Some(chunks.as_slice()),
                        true,
                        true,
                    )
                }
            };
        if file.get("index").and_then(Value::as_u64) != Some(index as u64)
            || file.get("kind").and_then(Value::as_str) != Some(expected_kind)
            || file.get("path").and_then(Value::as_str) != Some(expected_path)
            || match expected_to_path {
                Some(path) => file.get("to_path").and_then(Value::as_str) != Some(path),
                None => file.get("to_path") != Some(&Value::Null),
            }
            || !apply_patch_nullable_sha256(file.get("old_sha256"), old_sha)
            || !apply_patch_nullable_sha256(file.get("new_sha256"), new_sha)
        {
            return false;
        }

        let Some(would_change) = file.get("would_change").and_then(Value::as_bool) else {
            return false;
        };
        if expected_kind != "edit" && !would_change {
            return false;
        }
        if file.get("changed").and_then(Value::as_bool) != Some(!expected_dry_run && would_change) {
            return false;
        }
        any_would_change |= would_change;
        if would_change {
            expected_changed_paths.push(expected_path.to_string());
            if let Some(destination) = expected_to_path {
                expected_changed_paths.push(destination.to_string());
            }
        }

        let Some(edits) = file.get("edits").and_then(Value::as_array) else {
            return false;
        };
        match expected_chunks {
            None if !edits.is_empty() => return false,
            Some(chunks) => {
                if edits.len() != chunks.len()
                    || !edits
                        .iter()
                        .zip(chunks)
                        .enumerate()
                        .all(|(chunk_index, (edit, chunk))| {
                            validate_apply_patch_edit_summary(
                                edit,
                                chunk_index,
                                chunk,
                                expected_matching_mode,
                            )
                        })
                {
                    return false;
                }
            }
            None => {}
        }
    }

    if output.get("would_change").and_then(Value::as_bool) != Some(any_would_change) {
        return false;
    }
    let Some(changed_paths) = output.get("changed_paths").and_then(Value::as_array) else {
        return false;
    };
    changed_paths.len() == expected_changed_paths.len()
        && changed_paths
            .iter()
            .zip(expected_changed_paths)
            .all(|(actual, expected)| actual.as_str() == Some(expected.as_str()))
}

pub(super) fn apply_patch_agent_stdout_result(
    stdout: &str,
    patch: &crate::apply_patch_shared::CodexPatch,
    expected_dry_run: bool,
    expected_matching_mode: crate::apply_patch_shared::ApplyPatchMatchingMode,
) -> ToolResult {
    let mut result = transactional_edit_agent_stdout_result(
        "apply_patch",
        stdout,
        patch.hunks.len(),
        expected_dry_run,
    );
    if !result.success {
        sanitize_apply_patch_failure_metadata(&mut result, patch, expected_matching_mode);
        return result;
    }
    sanitize_apply_patch_success_metadata(&mut result.output);
    if validate_apply_patch_success_metadata(
        &result.output,
        patch,
        expected_dry_run,
        expected_matching_mode,
    ) {
        return result;
    }
    structured_edit_outcome_unknown_result(
        "apply_patch",
        "the Runner success payload contained invalid or contradictory patch-plan metadata",
        json!({}),
    )
}
