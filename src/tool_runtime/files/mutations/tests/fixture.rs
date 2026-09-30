use super::super::preflight::recoverable_write_rejection;
use super::*;

/// Pure, allocation-only computation of an `apply_text_edits` plan against
/// `original` UTF-8 content. Performs every semantic validation (unique
/// match, no overlap, whole-file sha guard) and returns the new content plus
/// a structured summary. Never touches the filesystem — the runtime/agent
/// layer decides whether to write. Used directly by unit tests; the agent
/// handler mirrors these exact semantics for the production write path.
#[cfg(test)]
pub(crate) fn apply_text_edits_to_string(
    original: &str,
    path: &str,
    edits: &[ApplyTextEditInput],
    expected_file_sha256: Option<&str>,
    dry_run: bool,
) -> Result<(String, Value), String> {
    if edits.is_empty() {
        return Err("edits must contain at least one edit".to_string());
    }
    if edits.len() > MAX_APPLY_TEXT_EDITS {
        return Err(format!(
            "too many edits; maximum is {}",
            MAX_APPLY_TEXT_EDITS
        ));
    }
    let old_sha256 = sha256_hex_bytes(original.as_bytes());
    if edits.iter().any(|edit| edit.expected_match_count.is_some())
        && expected_file_sha256.is_none()
    {
        return Err(recoverable_write_rejection(
            "bulk exact replacement requires an expected_file_sha256 guard",
        ));
    }
    if let Some(expected) = expected_file_sha256 {
        if old_sha256 != expected {
            return Err(recoverable_write_rejection("expected_file_sha256 mismatch"));
        }
    }

    let raw_original = original;
    let line_ending =
        detect_apply_text_line_ending(original).map_err(recoverable_write_rejection)?;
    let canonical_original = canonicalize_apply_text_line_endings(original, line_ending)
        .map_err(recoverable_write_rejection)?;
    let original = canonical_original.as_ref();

    // Resolve each edit to a (start, end, replacement, index) op against the
    // original content. start/end are byte offsets; inserts are zero-width.
    let mut ops: Vec<(usize, usize, String, usize)> = Vec::with_capacity(edits.len());
    let ignored_noop_count = edits
        .iter()
        .filter(|edit| {
            matches!(
                edit.kind,
                ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter
            ) && edit.new_text.as_deref() == Some("")
        })
        .count();
    for (index, edit) in edits.iter().enumerate() {
        let kind = edit.kind;
        if let Some(expected) = edit.expected_match_count {
            if kind != ApplyTextEditKind::ReplaceExact
                || edit.occurrence.is_some()
                || expected == 0
                || expected > crate::apply_edits_shared::MAX_APPLY_TEXT_EXPECTED_MATCH_COUNT
            {
                return Err(edit_field_error(
                    index,
                    kind,
                    "invalid expected_match_count combination or bound",
                ));
            }
        }
        if edit.occurrence == Some(0) {
            return Err(edit_field_error(
                index,
                kind,
                "occurrence must be at least 1",
            ));
        }
        if let Some(line_scope) = edit.line_scope {
            line_scope
                .validate()
                .map_err(|reason| edit_field_error(index, kind, reason))?;
        }
        if kind == ApplyTextEditKind::ReplaceRange {
            if edit.old_text.is_some()
                || edit.anchor_text.is_some()
                || edit.occurrence.is_some()
                || edit.expected_match_count.is_some()
            {
                return Err(edit_field_error(
                    index,
                    kind,
                    "old_text, anchor_text, occurrence, and expected_match_count are not allowed",
                ));
            }
            let range = edit
                .line_scope
                .ok_or_else(|| edit_field_error(index, kind, "start_line/end_line are required"))?;
            let replacement = edit
                .new_text
                .as_deref()
                .ok_or_else(|| edit_field_error(index, kind, "new_text is required"))?;
            if replacement.contains('\0') {
                return Err(edit_field_error(
                    index,
                    kind,
                    "edit text cannot contain NUL bytes",
                ));
            }
            if replacement.len() > MAX_APPLY_TEXT_EDIT_FIELD_BYTES {
                return Err(edit_field_error(index, kind, "edit field is too large"));
            }
            let replacement = canonicalize_apply_text_line_endings(replacement, line_ending)
                .map_err(|reason| edit_field_error(index, kind, reason))?
                .into_owned();
            let (start, end) =
                crate::apply_edits_shared::resolve_apply_text_line_range(original, range)
                    .map_err(|reason| edit_field_error(index, kind, reason))?;
            ops.push((start, end, replacement, index));
            continue;
        }
        let (needle, replacement): (&str, String) = match kind {
            ApplyTextEditKind::ReplaceRange => unreachable!("handled above"),
            ApplyTextEditKind::ReplaceExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                let new = edit.new_text.clone().unwrap_or_default();
                (old, new)
            }
            ApplyTextEditKind::DeleteExact => {
                let old = edit
                    .old_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| edit_field_error(index, kind, "old_text must be non-empty"))?;
                (old, String::new())
            }
            ApplyTextEditKind::InsertBefore | ApplyTextEditKind::InsertAfter => {
                let anchor = edit
                    .anchor_text
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        edit_field_error(index, kind, "anchor_text must be non-empty")
                    })?;
                let new = edit
                    .new_text
                    .as_deref()
                    .ok_or_else(|| edit_field_error(index, kind, "new_text is required"))?;
                if new.is_empty() {
                    continue;
                }
                (anchor, new.to_string())
            }
        };
        if needle.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "match text cannot contain NUL bytes",
            ));
        }
        if replacement.contains('\0') {
            return Err(edit_field_error(
                index,
                kind,
                "replacement text cannot contain NUL bytes",
            ));
        }
        let needle = canonicalize_apply_text_line_endings(needle, line_ending)
            .map_err(|error| edit_match_error(index, kind, error))?;
        let replacement = canonicalize_apply_text_line_endings(&replacement, line_ending)
            .map_err(|error| edit_field_error(index, kind, error))?
            .into_owned();
        let needle = needle.as_ref();
        if let Some(expected) = edit.expected_match_count {
            let matches = crate::apply_edits_shared::resolve_apply_text_bulk_matches(
                original,
                needle,
                edit.line_scope.as_ref(),
            );
            if matches.match_count != expected {
                return Err(edit_match_error(
                    index,
                    kind,
                    "expected_match_count mismatch",
                ));
            }
            for (start, end) in matches.ranges {
                ops.push((start, end, replacement.clone(), index));
            }
            continue;
        }
        let (start, end) = crate::apply_edits_shared::resolve_apply_text_match(
            original,
            needle,
            edit.occurrence,
            edit.line_scope.as_ref(),
        )
        .map_err(|conflict| {
            use crate::apply_edits_shared::ApplyTextMatchConflictKind;
            let message = match conflict.kind {
                ApplyTextMatchConflictKind::MatchNotFound if conflict.line_scope.is_some() => {
                    "match text was not found within line_scope".to_string()
                }
                ApplyTextMatchConflictKind::MatchNotFound => "match text was not found".to_string(),
                ApplyTextMatchConflictKind::MultipleMatches => format!(
                    "match text matched {} times{}; refusing ambiguous edit",
                    conflict
                        .line_scope_match_count
                        .unwrap_or(conflict.match_count),
                    if conflict.line_scope.is_some() {
                        " within line_scope"
                    } else {
                        ""
                    }
                ),
                ApplyTextMatchConflictKind::OccurrenceOutOfRange => format!(
                    "requested occurrence {} is out of range for {} exact matches",
                    conflict.requested_occurrence.unwrap_or(0),
                    conflict.match_count
                ),
                ApplyTextMatchConflictKind::OccurrenceOutsideLineScope => format!(
                    "requested occurrence {} is outside line_scope",
                    conflict.requested_occurrence.unwrap_or(0)
                ),
            };
            edit_match_error(index, kind, &message)
        })?;
        let (range_start, range_end) = match kind {
            ApplyTextEditKind::InsertBefore => (start, start),
            ApplyTextEditKind::InsertAfter => (end, end),
            _ => (start, end),
        };
        ops.push((range_start, range_end, replacement, index));
    }

    // Stable sort by (start, end, original index) so the slice build is
    // deterministic and ties (e.g. multiple inserts at one point) keep caller
    // order.
    ops.sort_by_key(|&(s, e, _, i)| (s, e, i));

    // Reject overlapping edits: a later op must not start before an earlier
    // op ends. Zero-width ops (inserts) never trigger this because their
    // start == end.
    for w in ops.windows(2) {
        let (_, e1, _, _) = w[0];
        let (s2, _, _, _) = w[1];
        if s2 < e1 {
            return Err(recoverable_write_rejection(
                "edits overlap; refusing ambiguous atomic edit batch",
            ));
        }
    }

    // Build the new content by slicing the original at op boundaries.
    let mut new_content = String::with_capacity(original.len() + 64);
    let mut cursor = 0usize;
    let mut edit_summaries: Vec<Value> = Vec::with_capacity(ops.len());
    for &(start, end, ref replacement, index) in &ops {
        new_content.push_str(&original[cursor..start]);
        new_content.push_str(replacement);
        cursor = end;
        let edit = &edits[index];
        let old_start_line = 1 + original[..start].matches('\n').count();
        let mut old_end_line = 1 + original[..end].matches('\n').count();
        if end > start && end <= original.len() && original.as_bytes()[end - 1] == b'\n' {
            old_end_line = old_end_line.saturating_sub(1).max(old_start_line);
        }
        if end == start {
            old_end_line = old_start_line;
        }
        let new_line_count = if replacement.is_empty() {
            0
        } else {
            replacement.lines().count()
        };
        edit_summaries.push(json!({
            "index": index,
            "kind": edit.kind.as_str(),
            "old_start_line": old_start_line,
            "old_end_line": old_end_line,
            "new_line_count": new_line_count,
        }));
    }
    new_content.push_str(&original[cursor..]);
    let new_content = restore_apply_text_line_endings(new_content, line_ending);

    let new_sha256 = sha256_hex_bytes(new_content.as_bytes());
    let changed = new_content != raw_original;
    let output = json!({
        "path": path,
        "dry_run": dry_run,
        "applied_count": edits.len(),
        "ignored_noop_count": ignored_noop_count,
        "old_sha256": old_sha256,
        "new_sha256": new_sha256,
        "changed": changed,
        "would_change": changed,
        "edits": edit_summaries,
        "changed_paths": [path],
    });
    Ok((new_content, output))
}

#[cfg(test)]
fn edit_field_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with corrected edit fields.",
        index,
        kind.as_str(),
        msg
    )
}

#[cfg(test)]
fn edit_match_error(index: usize, kind: ApplyTextEditKind, msg: &str) -> String {
    format!(
        "Rejected before write: edit {} ({}): {}.\nNo files were modified.\nRetry guidance: read the file again to refresh context, then retry with a more exact match text.",
        index,
        kind.as_str(),
        msg
    )
}
