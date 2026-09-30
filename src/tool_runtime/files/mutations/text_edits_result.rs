//! Validation and bounded model projection for transactional edit receipts.

use super::lifecycle::{
    structured_edit_outcome_unknown_result, transactional_edit_agent_stdout_result,
};
use super::preflight::read_files_recovery;
use super::*;

fn apply_text_edits_sha256(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(crate::apply_edits_shared::is_lowercase_hex_sha256)
}

fn sanitize_apply_text_edit_advisories(output: &mut Value, changes: &[ApplyFileChangeInput]) {
    let Some(files) = output.get_mut("files").and_then(Value::as_array_mut) else {
        return;
    };
    for (file, change) in files.iter_mut().zip(changes) {
        let Some(summaries) = file.get_mut("edits").and_then(Value::as_array_mut) else {
            continue;
        };
        for summary in summaries {
            let Some(summary) = summary.as_object_mut() else {
                continue;
            };
            let requested_edit = summary
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .and_then(|index| change.edits.get(index));
            let preserve_warning = requested_edit.is_some_and(|edit| {
                change.kind == ApplyFileChangeKind::Edit
                    && matches!(
                        edit.kind,
                        ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter
                    )
                    && summary.get("kind").and_then(Value::as_str) == Some(edit.kind.as_str())
                    && summary.get("warning").and_then(Value::as_str)
                        == Some(crate::apply_edits_shared::APPLY_TEXT_EDIT_DUPLICATE_ANCHOR_WARNING)
            });
            if !preserve_warning {
                summary.remove("warning");
            }
        }
    }
}

fn validate_apply_text_edits_success_metadata(
    output: &Value,
    changes: &[ApplyFileChangeInput],
    expected_dry_run: bool,
) -> bool {
    let Some(files) = output.get("files").and_then(Value::as_array) else {
        return false;
    };
    if files.len() != changes.len() {
        return false;
    }

    let mut any_changed = false;
    let mut any_would_change = false;
    let mut total_ranges = 0usize;
    let mut observed_edits = 0usize;
    let mut resolved_matches = 0usize;
    let mut warnings = 0usize;
    let bulk_requested = changes
        .iter()
        .flat_map(|change| &change.edits)
        .any(|edit| edit.expected_match_count.is_some());
    for (expected_index, (file, change)) in files.iter().zip(changes).enumerate() {
        if file.get("index").and_then(Value::as_u64) != Some(expected_index as u64)
            || file.get("kind").and_then(Value::as_str) != Some(change.kind.as_str())
            || file.get("path").and_then(Value::as_str) != Some(change.path.as_str())
        {
            return false;
        }
        let to_path_matches = match change.to_path.as_deref() {
            Some(expected) => file.get("to_path").and_then(Value::as_str) == Some(expected),
            None => matches!(file.get("to_path"), Some(Value::Null)),
        };
        if !to_path_matches {
            return false;
        }
        let Some(edits) = file.get("edits").and_then(Value::as_array) else {
            return false;
        };
        observed_edits += edits.len();
        let mut seen_bulk = std::collections::HashSet::new();
        for summary in edits {
            let Some(fields) = summary.as_object() else {
                return false;
            };
            if fields.keys().any(|key| {
                !matches!(
                    key.as_str(),
                    "index"
                        | "kind"
                        | "old_start_line"
                        | "old_end_line"
                        | "new_line_count"
                        | "would_change"
                        | "warning"
                        | "match_count"
                        | "expected_match_count"
                        | "match_ranges"
                        | "match_ranges_truncated"
                )
            }) {
                return false;
            }
            let Some(index) = summary
                .get("index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
            else {
                return false;
            };
            let Some(requested) = change.edits.get(index) else {
                return false;
            };
            if requested.expected_match_count.is_some()
                && summary.get("kind").and_then(Value::as_str) != Some(requested.kind.as_str())
            {
                return false;
            }
            if let Some(expected) = requested.expected_match_count {
                if !seen_bulk.insert(index) {
                    return false;
                }
                if summary.get("expected_match_count").and_then(Value::as_u64)
                    != Some(expected as u64)
                    || summary.get("match_count").and_then(Value::as_u64) != Some(expected as u64)
                    || summary
                        .get("would_change")
                        .and_then(Value::as_bool)
                        .is_none()
                {
                    return false;
                }
                let Some(ranges) = summary.get("match_ranges").and_then(Value::as_array) else {
                    return false;
                };
                let expected_ranges = expected
                    .min(crate::apply_edits_shared::MAX_APPLY_TEXT_MATCH_RANGES_PER_EDIT)
                    .min(
                        crate::apply_edits_shared::MAX_APPLY_TEXT_MATCH_RANGES_TOTAL
                            .saturating_sub(total_ranges),
                    );
                if ranges.len() != expected_ranges {
                    return false;
                }
                total_ranges += ranges.len();
                if total_ranges > crate::apply_edits_shared::MAX_APPLY_TEXT_MATCH_RANGES_TOTAL {
                    return false;
                }
                if summary
                    .get("match_ranges_truncated")
                    .and_then(Value::as_bool)
                    != Some(ranges.len() < expected)
                {
                    return false;
                }
                if ranges.iter().any(|range| {
                    range.get("start_line").and_then(Value::as_u64).is_none()
                        || range.get("end_line").and_then(Value::as_u64).is_none()
                        || range.get("occurrence").and_then(Value::as_u64).is_none()
                        || range.as_object().is_none_or(|object| object.len() != 3)
                }) {
                    return false;
                }
                if ranges.iter().any(|range| {
                    let start = range["start_line"].as_u64().unwrap_or(0);
                    let end = range["end_line"].as_u64().unwrap_or(0);
                    start == 0 || end < start || range["occurrence"].as_u64() == Some(0)
                }) || ranges.windows(2).any(|pair| {
                    pair[0]["occurrence"].as_u64() >= pair[1]["occurrence"].as_u64()
                        || pair[0]["start_line"].as_u64() > pair[1]["start_line"].as_u64()
                }) {
                    return false;
                }
                resolved_matches += expected;
            } else {
                resolved_matches += 1;
            }
            warnings += usize::from(summary.get("warning").is_some());
        }
        if change
            .edits
            .iter()
            .enumerate()
            .any(|(index, edit)| edit.expected_match_count.is_some() && !seen_bulk.contains(&index))
        {
            return false;
        }

        let old_sha256 = file.get("old_sha256");
        let new_sha256 = file.get("new_sha256");
        let sha_shape_valid = match change.kind {
            ApplyFileChangeKind::Create => {
                matches!(old_sha256, Some(Value::Null)) && apply_text_edits_sha256(new_sha256)
            }
            ApplyFileChangeKind::Edit | ApplyFileChangeKind::Rename => {
                apply_text_edits_sha256(old_sha256) && apply_text_edits_sha256(new_sha256)
            }
            ApplyFileChangeKind::Delete => {
                apply_text_edits_sha256(old_sha256) && matches!(new_sha256, Some(Value::Null))
            }
        };
        if !sha_shape_valid {
            return false;
        }

        let Some(changed) = file.get("changed").and_then(Value::as_bool) else {
            return false;
        };
        let Some(would_change) = file.get("would_change").and_then(Value::as_bool) else {
            return false;
        };
        if change.kind != ApplyFileChangeKind::Edit && !would_change {
            return false;
        }
        if changed != (!expected_dry_run && would_change) {
            return false;
        }
        any_changed |= changed;
        any_would_change |= would_change;
    }

    let summary = output.get("change_summary");
    let requested_logical_edits: usize = changes.iter().map(|change| change.edits.len()).sum();
    let ignored_noop_count = output
        .get("ignored_noop_count")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let summary_valid = summary.is_some_and(|summary| {
        summary.get("requested_changes").and_then(Value::as_u64) == Some(changes.len() as u64)
            && summary.get("changed_files").and_then(Value::as_u64)
                == Some(
                    files
                        .iter()
                        .filter(|file| file.get("changed").and_then(Value::as_bool) == Some(true))
                        .count() as u64,
                )
            && summary.get("logical_edits").and_then(Value::as_u64)
                == Some(requested_logical_edits as u64)
            && observed_edits + ignored_noop_count == requested_logical_edits
            && summary.get("resolved_matches").and_then(Value::as_u64)
                == Some(resolved_matches as u64)
            && summary.get("warnings").and_then(Value::as_u64) == Some(warnings as u64)
    });
    output.get("changed").and_then(Value::as_bool) == Some(any_changed)
        && output.get("would_change").and_then(Value::as_bool) == Some(any_would_change)
        && (summary_valid || (!bulk_requested && summary.is_none()))
}

fn sanitize_apply_text_edits_model_recovery(
    mut result: ToolResult,
    project: &str,
    changes: &[ApplyFileChangeInput],
) -> ToolResult {
    if result.success {
        return result;
    }
    let change_index = result
        .output
        .get("change_index")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok());
    let change = change_index.and_then(|index| changes.get(index));
    let error_kind = result
        .output
        .get("error_kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let raw_conflict = result
        .output
        .as_object_mut()
        .and_then(|output| output.remove("conflict_recovery"));
    if let Some(output) = result.output.as_object_mut() {
        for key in [
            "retry_guidance",
            "recovery_action",
            "expected_read_revision",
            "reread_required",
            "suggested_call",
            "error",
        ] {
            output.remove(key);
        }
    }

    if error_kind == "sha256_conflict" {
        result.output["error_kind"] = json!("stale_file_revision");
        result.output["direct_retry_safe"] = json!(false);
        result.output["reread_required"] = json!(true);
        if let Some(change) = change {
            result.output["path"] = json!(change.path);
            result.output["recovery"] = read_files_recovery(project, &change.path);
        }
        result.error = Some(
            "Rejected transactional file batch: the source changed before mutation. No files were modified."
                .to_string(),
        );
        return result;
    }

    if error_kind != "edit_conflict" {
        return result;
    }
    let Some(raw_conflict) = raw_conflict.as_ref().and_then(Value::as_object) else {
        return result;
    };
    let Some(conflict_kind) = raw_conflict.get("conflict_kind").and_then(Value::as_str) else {
        return result;
    };
    let conflict_kind = match conflict_kind {
        "multiple_matches"
        | "match_count_mismatch"
        | "match_not_found"
        | "occurrence_out_of_range"
        | "occurrence_outside_line_scope"
        | "overlapping_edits" => conflict_kind,
        _ => return result,
    };
    result.output["error_kind"] = json!(conflict_kind);
    if let Some(match_count) = raw_conflict.get("match_count").and_then(Value::as_u64) {
        result.output["match_count"] = json!(match_count);
    }
    if conflict_kind == "match_count_mismatch" {
        let requested = change.and_then(|change| {
            result
                .output
                .get("edit_index")
                .and_then(Value::as_u64)
                .and_then(|index| change.edits.get(index as usize))
        });
        if let Some(expected) = requested.and_then(|edit| edit.expected_match_count) {
            result.output["expected_match_count"] = json!(expected);
            if let Some(scope) = requested.and_then(|edit| edit.line_scope) {
                result.output["line_scope"] = json!(scope);
            }
            result.output["direct_retry_safe"] = json!(false);
            result.output["reread_required"] = json!(true);
            if let Some(actual) = raw_conflict
                .get("actual_match_count")
                .and_then(Value::as_u64)
            {
                result.output["actual_match_count"] = json!(actual);
            }
        }
    }
    if let Some(truncated) = raw_conflict
        .get("candidates_truncated")
        .and_then(Value::as_bool)
    {
        result.output["candidates_truncated"] = json!(if conflict_kind == "match_count_mismatch" {
            raw_conflict
                .get("actual_match_count")
                .and_then(Value::as_u64)
                .is_some_and(|count| {
                    count > crate::apply_edits_shared::MAX_APPLY_TEXT_CONFLICT_CANDIDATES as u64
                })
        } else {
            truncated
        });
    }
    if let Some(indices) = raw_conflict
        .get("conflicting_edit_indices")
        .and_then(Value::as_array)
    {
        result.output["conflicting_edit_indices"] = json!(indices);
    }

    if conflict_kind == "overlapping_edits" {
        if let Some(ranges) = raw_conflict
            .get("conflicting_edit_ranges")
            .and_then(Value::as_array)
        {
            let ranges = ranges
                .iter()
                .take(2)
                .filter_map(|range| {
                    let edit_index = range.get("edit_index")?.as_u64()?;
                    let start_line = range.get("start_line")?.as_u64()?;
                    let end_line = range.get("end_line")?.as_u64()?;
                    if start_line == 0 || end_line < start_line {
                        return None;
                    }
                    Some(json!({
                        "edit_index": edit_index,
                        "start_line": start_line,
                        "end_line": end_line,
                    }))
                })
                .collect::<Vec<_>>();
            if !ranges.is_empty() {
                result.output["conflicting_edit_ranges"] = json!(ranges);
            }
        }
    }

    let guarded = change.is_some_and(|change| change.expected_read_revision.is_some());
    if let Some(candidates) = raw_conflict
        .get("candidate_ranges")
        .and_then(Value::as_array)
    {
        let candidates = candidates
            .iter()
            .take(crate::apply_edits_shared::MAX_APPLY_TEXT_CONFLICT_CANDIDATES)
            .filter_map(|candidate| {
                let start_line = candidate.get("start_line")?.as_u64()?;
                let end_line = candidate.get("end_line")?.as_u64()?;
                if guarded {
                    Some(json!({
                        "occurrence": candidate.get("occurrence")?.as_u64()?,
                        "start_line": start_line,
                        "end_line": end_line,
                    }))
                } else {
                    Some(json!({"start_line": start_line, "end_line": end_line}))
                }
            })
            .collect::<Vec<_>>();
        if !candidates.is_empty() || conflict_kind == "match_count_mismatch" {
            result.output["candidate_ranges"] = json!(candidates);
        }
    }

    if (!guarded && conflict_kind != "overlapping_edits") || conflict_kind == "match_count_mismatch"
    {
        if let Some(change) = change {
            result.output["recovery"] = read_files_recovery(project, &change.path);
        }
    }
    result.error = Some(
        match conflict_kind {
            "multiple_matches" => "Rejected transactional file batch: the exact target matched multiple locations. No files were modified.",
            "match_count_mismatch" => "Rejected transactional file batch: exact match count differed from expected_match_count. No files were modified.",
            "match_not_found" => "Rejected transactional file batch: the exact target was not found. No files were modified.",
            "occurrence_out_of_range" => "Rejected transactional file batch: the requested occurrence is outside the exact-match set. No files were modified.",
            "occurrence_outside_line_scope" => "Rejected transactional file batch: the requested occurrence is outside line_scope. No files were modified.",
            "overlapping_edits" => "Rejected transactional file batch: planned exact edit ranges overlap. No files were modified.",
            _ => unreachable!(),
        }
        .to_string(),
    );
    result
}

pub(super) fn apply_text_edits_agent_stdout_result(
    stdout: &str,
    expected_change_count: usize,
    expected_dry_run: bool,
    project: &str,
    changes: &[ApplyFileChangeInput],
) -> ToolResult {
    let mut result = sanitize_apply_text_edits_model_recovery(
        transactional_edit_agent_stdout_result(
            "edit_project_files",
            stdout,
            expected_change_count,
            expected_dry_run,
        ),
        project,
        changes,
    );
    if !result.success {
        return result;
    }
    sanitize_apply_text_edit_advisories(&mut result.output, changes);
    if validate_apply_text_edits_success_metadata(&result.output, changes, expected_dry_run) {
        return result;
    }
    structured_edit_outcome_unknown_result(
        "edit_project_files",
        "the Runner success payload contained invalid or contradictory file-result metadata",
        json!({}),
    )
}
