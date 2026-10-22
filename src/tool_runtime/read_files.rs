//! Bounded multi-file reads built from the canonical single-file read core.

use super::files::{
    build_read_file_success, slice_read_file_range, FileReadFailure, FileReadOutcome,
};
use super::project_resolution::ResolvedProject;
use super::workspace_reads::WorkspaceReadRuntime;
use super::{ReadFilesItem, SuggestedToolCall, ToolCall, ToolResult, ToolRuntime};
use crate::json_measurement::serialized_json_len;
use futures_util::{stream, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time::Instant;
use webcodex_core::runtime_contract::MODEL_INSPECTION_MAX_RESULT_BYTES as MAX_SERIALIZED_OUTPUT_BYTES;
use webcodex_workspace::file_read_normalize::MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
use webcodex_workspace::file_read_range::{FileReadRange, ReadFileReason};

pub(crate) const MAX_READ_FILES_ITEMS: usize = 8;
// A max-size read batch may issue all eight independent read-only requests in
// one Server fanout wave. The public item cap, shared deadline, result budget,
// and downstream Runner admission (for example polling capacity) remain hard bounds.
pub(crate) const MAX_READ_FILES_CONCURRENCY: usize = 8;
pub(crate) const DEFAULT_READ_FILES_DEADLINE: Duration = Duration::from_secs(30);
const READ_FILES_MERGE_GAP_LINES: usize = 20;
const READ_FILES_MAX_MERGED_LINES: usize = 400;
pub(crate) use webcodex_core::runtime_contract::{
    DEFAULT_READ_FILES_RESULT_BYTES, MIN_READ_FILES_RESULT_BYTES,
};

#[derive(Clone, Debug)]
struct PlannedReadMember {
    index: usize,
    start_line: Option<usize>,
    limit: Option<usize>,
}

#[derive(Clone, Debug)]
struct PlannedRead {
    item: ReadFilesItem,
    members: Vec<PlannedReadMember>,
}

struct ObservedRead {
    range: FileReadRange,
    revision: u64,
}

/// Keep request order, continuation ranges and revision facts independent of
/// the public JSON representation, including coalesced byte-budget fallback.
struct CompletedReadMember {
    index: usize,
    path: String,
    result: Result<ObservedRead, FileReadFailure>,
}

/// Source ranges whose text actually survives the model-output budget. Entries
/// stay aligned with output items, including failures and empty file reads.
/// Sparse presentation never supplies or widens these facts.
#[derive(Debug, Default)]
pub(crate) struct RetainedReadRanges(Vec<Option<RetainedReadRange>>);

#[derive(Debug)]
struct RetainedReadRange {
    path: String,
    start_line: usize,
    end_line: usize,
}

impl RetainedReadRanges {
    pub(crate) fn apply_retention(&mut self, retention: ReadOutputRetention) {
        let Some(retention) = retention.0 else {
            return;
        };
        self.0.truncate(retention.item_count);
        if let Some(keep_lines) = retention.last_item_lines {
            if let Some(Some(range)) = self.0.last_mut() {
                range.end_line = range.end_line.min(
                    range
                        .start_line
                        .saturating_add(keep_lines)
                        .saturating_sub(1),
                );
            }
        }
    }

    pub(crate) fn contains(&self, path: &str, line: u64) -> bool {
        self.0.iter().flatten().any(|range| {
            range.path == path && (range.start_line as u64..=range.end_line as u64).contains(&line)
        })
    }
}

pub(crate) struct ReadFilesOutput {
    pub result: ToolResult,
    pub items: Vec<ReadFilesItem>,
    pub retained: RetainedReadRanges,
}

impl ReadFilesOutput {
    fn failure(result: ToolResult) -> Self {
        Self {
            result,
            items: Vec::new(),
            retained: RetainedReadRanges::default(),
        }
    }
}

/// Budget algorithms retain an ordered item prefix, possibly shortening its
/// final item. Record that decision where it is made, before sparse projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ReadRetainedPrefix {
    item_count: usize,
    last_item_lines: Option<usize>,
}

impl ReadRetainedPrefix {
    fn complete(item_count: usize) -> Self {
        Self {
            item_count,
            last_item_lines: None,
        }
    }
}

/// Optional change from a projection pass. Ordinary read projection callers
/// may discard it; compound inspection applies it to its typed evidence.
#[derive(Debug, Default)]
pub(crate) struct ReadOutputRetention(Option<ReadRetainedPrefix>);

struct BudgetedReadBatch {
    output: Value,
    retention: ReadRetainedPrefix,
}

struct TruncatedReadItem {
    output: Value,
    retained_lines: usize,
}

impl CompletedReadMember {
    fn new(
        member: PlannedReadMember,
        path: &str,
        observed: Result<(&FileReadRange, u64), FileReadFailure>,
        with_line_numbers: bool,
    ) -> Self {
        let result = observed.and_then(|(range, revision)| {
            slice_read_file_range(
                range,
                member.start_line,
                member.limit,
                with_line_numbers,
                path,
            )
            .map(|range| ObservedRead { range, revision })
        });
        Self {
            index: member.index,
            path: path.to_owned(),
            result,
        }
    }

    fn into_output(self, with_line_numbers: bool) -> (Value, Option<RetainedReadRange>) {
        let mut retained = None;
        let result = match self.result {
            Ok(observed) => {
                let mut result =
                    build_read_file_success(&observed.range, with_line_numbers, Some(&self.path));
                if result.success {
                    result.output["read_revision"] = json!(observed.revision);
                    retained = observed.range.end_line.map(|end_line| RetainedReadRange {
                        path: self.path.clone(),
                        start_line: observed.range.start_line,
                        end_line,
                    });
                }
                result
            }
            Err(failure) => failure.into_tool_result(Some(&self.path)),
        };
        (
            json!({
                "index": self.index,
                "path": self.path,
                "success": result.success,
                "output": result.output,
                "error": result.error,
            }),
            retained,
        )
    }
}

fn plan_read_files(items: Vec<ReadFilesItem>) -> Vec<PlannedRead> {
    let mut normalized = items
        .into_iter()
        .enumerate()
        .map(|(index, item)| {
            let (start, _, end) =
                super::files::effective_read_file_range(item.start_line, item.limit);
            (
                item.path.clone(),
                item.expected_read_revision,
                start,
                end,
                PlannedReadMember {
                    index,
                    start_line: item.start_line,
                    limit: item.limit,
                },
            )
        })
        .collect::<Vec<_>>();
    normalized.sort_by(|left, right| {
        left.0
            .cmp(&right.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| left.2.cmp(&right.2))
            .then_with(|| left.3.cmp(&right.3))
    });

    let mut planned: Vec<PlannedRead> = Vec::with_capacity(normalized.len());
    for (path, expected_read_revision, start, end, member) in normalized {
        let merge_into_last = planned.last_mut().filter(|existing| {
            existing.item.path == path
                && existing.item.expected_read_revision == expected_read_revision
        });
        if let Some(existing) = merge_into_last {
            let (existing_start, _, existing_end) = super::files::effective_read_file_range(
                existing.item.start_line,
                existing.item.limit,
            );
            let merged_end = existing_end.max(end);
            let merged_lines = merged_end.saturating_sub(existing_start).saturating_add(1);
            let close_enough = start <= existing_end.saturating_add(READ_FILES_MERGE_GAP_LINES);
            // Contained/identical ranges add no physical lines or bytes, even
            // when the original caller range exceeds the union-planning cap.
            if end <= existing_end || (close_enough && merged_lines <= READ_FILES_MAX_MERGED_LINES)
            {
                existing.item.limit = Some(merged_lines);
                existing.members.push(member);
                continue;
            }
        }

        planned.push(PlannedRead {
            item: ReadFilesItem {
                path,
                start_line: Some(start),
                limit: Some(end.saturating_sub(start).saturating_add(1)),
                expected_read_revision,
            },
            members: vec![member],
        });
    }
    planned
}

/// Inspect the unique ranges of the canonical physical read plan in tests.
#[cfg(test)]
pub(crate) fn coalesce_read_files_items(items: Vec<ReadFilesItem>) -> Vec<ReadFilesItem> {
    plan_read_files(items)
        .into_iter()
        .map(|planned| planned.item)
        .collect()
}

/// Read request facts captured before the ToolCall is moved into execution.
/// Canonical read results remain independent of this projection; these facts
/// exist only so a final model-facing partial result can provide a directly
/// reusable next call without inventing a new cursor.
#[derive(Clone, Debug)]
pub(crate) enum ReadModelProjection {
    None,
    Batch {
        project: String,
        items: Vec<ReadFilesItem>,
        session_id: Option<String>,
        with_line_numbers: Option<bool>,
        max_result_bytes: Option<usize>,
    },
}

impl ReadModelProjection {
    pub(crate) fn capture(call: &ToolCall) -> Self {
        match call {
            ToolCall::ReadFiles {
                project,
                items,
                session_id,
                with_line_numbers,
                max_result_bytes,
            } => Self::Batch {
                project: project.clone(),
                items: items.clone(),
                session_id: session_id.clone(),
                with_line_numbers: *with_line_numbers,
                max_result_bytes: max_result_bytes
                    .map(|bytes| normalized_result_budget(Some(bytes))),
            },
            _ => Self::None,
        }
    }

    /// Replace shorthand with the exact Project identity selected by the same
    /// authoritative resolver pass used for this call. Recovery must not
    /// re-enter shorthand resolution and retarget after registry churn.
    pub(crate) fn bind_resolved_project(&mut self, resolved: Option<&ResolvedProject>) {
        let Some(resolved) = resolved else {
            return;
        };
        match self {
            Self::Batch { project, .. } => {
                *project = resolved.resolved_id.clone();
            }
            Self::None => {}
        }
    }
}

fn read_range_next_item(item: &Value) -> Option<ReadFilesItem> {
    if item.get("success").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let output = item.get("output")?.as_object()?;
    if output.get("has_more").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let read_revision = output.get("read_revision")?.as_u64()?;
    let next_start_line = output.get("next_start_line")?.as_u64()? as usize;
    let total_lines = output.get("total_lines")?.as_u64()? as usize;
    let remaining_lines = total_lines
        .saturating_sub(next_start_line)
        .saturating_add(1);
    if remaining_lines == 0 {
        return None;
    }
    let requested_limit = output
        .get("budget_next_limit")
        .and_then(Value::as_u64)
        .or_else(|| output.get("limit").and_then(Value::as_u64))?
        as usize;
    Some(ReadFilesItem {
        path: item.get("path")?.as_str()?.to_string(),
        start_line: Some(next_start_line),
        limit: Some(requested_limit.min(remaining_lines).max(1)),
        expected_read_revision: Some(read_revision),
    })
}

fn read_files_suggested_arguments(
    project: &str,
    items: &[ReadFilesItem],
    session_id: Option<&str>,
    with_line_numbers: Option<bool>,
    max_result_bytes: Option<usize>,
) -> Value {
    let suggested_items = items
        .iter()
        .map(|item| {
            let mut suggested = json!({"path": item.path});
            if let Some(start_line) = item.start_line {
                suggested["start_line"] = json!(start_line);
            }
            if let Some(limit) = item.limit {
                suggested["limit"] = json!(limit);
            }
            if let Some(expected_read_revision) = item.expected_read_revision {
                suggested["expected_read_revision"] = json!(expected_read_revision);
            }
            suggested
        })
        .collect::<Vec<_>>();
    let mut arguments = json!({
        "project": project,
        "items": suggested_items,
    });
    if let Some(session_id) = session_id {
        arguments["session_id"] = json!(session_id);
    }
    if let Some(with_line_numbers) = with_line_numbers {
        arguments["with_line_numbers"] = json!(with_line_numbers);
    }
    if let Some(max_result_bytes) = max_result_bytes {
        arguments["max_result_bytes"] = json!(max_result_bytes);
    }
    arguments
}

/// Project one parser-ready follow-up for the invocation. Continued ranges are
/// fenced to the read_revision observed for the returned partial item; original
/// unreturned items retain exactly the caller-supplied shape.
pub(crate) fn add_actionable_read_continuations(
    projection: &ReadModelProjection,
    result: &mut ToolResult,
) {
    if !result.success {
        return;
    }
    let ReadModelProjection::Batch {
        project,
        items: original_items,
        session_id,
        with_line_numbers,
        max_result_bytes,
    } = projection
    else {
        return;
    };
    let Some(output) = result.output.as_object_mut() else {
        return;
    };
    let Some(returned) = output.get("items").and_then(Value::as_array) else {
        return;
    };
    let truncated = output.get("output_truncated").and_then(Value::as_bool) == Some(true);
    let mut next_items = returned
        .iter()
        .filter_map(read_range_next_item)
        .collect::<Vec<_>>();
    let mut next_budget = *max_result_bytes;
    if truncated {
        let Some(next_index) = output.get("next_index").and_then(Value::as_u64) else {
            return;
        };
        let next_index = next_index as usize;
        if returned.is_empty() && next_index == 0 {
            // Repeating a zero-progress request only helps with a larger budget.
            // At the hard cap there is no proven next call.
            if max_result_bytes.unwrap_or(DEFAULT_READ_FILES_RESULT_BYTES)
                >= MAX_SERIALIZED_OUTPUT_BYTES
            {
                return;
            }
            next_budget = Some(MAX_SERIALIZED_OUTPUT_BYTES);
        }
        let current_returned = returned
            .iter()
            .any(|item| item["index"].as_u64() == Some(next_index as u64));
        let first_unreturned = next_index.saturating_add(usize::from(current_returned));
        if let Some(remaining) = original_items.get(first_unreturned..) {
            next_items.extend_from_slice(remaining);
        }
    }
    if !next_items.is_empty() {
        output.insert(
            "suggested_call".to_string(),
            SuggestedToolCall::mechanically_followable(
                "read_files",
                read_files_suggested_arguments(
                    project,
                    &next_items,
                    session_id.as_deref(),
                    *with_line_numbers,
                    next_budget,
                ),
            )
            .to_value(),
        );
    }
}

fn normalized_result_budget(max_result_bytes: Option<usize>) -> usize {
    max_result_bytes
        .unwrap_or(DEFAULT_READ_FILES_RESULT_BYTES)
        .clamp(MIN_READ_FILES_RESULT_BYTES, MAX_SERIALIZED_OUTPUT_BYTES)
}

fn batch_output(
    project: &str,
    requested_count: usize,
    items: Vec<Value>,
    output_truncated: bool,
    next_index: Option<usize>,
    truncation_reason: Option<&str>,
) -> Value {
    let succeeded_count = items
        .iter()
        .filter(|item| item["success"].as_bool() == Some(true))
        .count();
    let returned_count = items.len();
    let mut output = json!({
        "project": project,
        "requested_count": requested_count,
        "returned_count": returned_count,
        "succeeded_count": succeeded_count,
        "failed_count": returned_count - succeeded_count,
        "items": items,
        "output_truncated": output_truncated,
        "next_index": next_index,
    });
    if let Some(reason) = truncation_reason {
        output["truncation_reason"] = json!(reason);
    }
    output
}

#[cfg(test)]
fn serialized_batch_len(output: &Value) -> usize {
    serde_json::to_vec(&ToolResult::ok(output.clone()))
        .map(|bytes| bytes.len())
        .unwrap_or(usize::MAX)
}

fn serialized_value_len(value: &Value) -> usize {
    serialized_json_len(value).unwrap_or(usize::MAX)
}

// Borrow the original text while cloning only the surrounding item metadata.
// The same helper keeps the single-item budget probe and the whole-batch size
// projection consistent, without retaining file content in a new cache.
fn cloned_read_item_without_text(item: &Value) -> Option<(Value, &str)> {
    let fields = item.as_object()?;
    let read_fields = fields.get("output")?.as_object()?;
    let text = read_fields.get("text")?.as_str()?;

    let mut read_clone = serde_json::Map::new();
    for (key, value) in read_fields {
        read_clone.insert(
            key.clone(),
            if key == "text" {
                json!("")
            } else {
                value.clone()
            },
        );
    }
    let mut item_clone = serde_json::Map::new();
    for (key, value) in fields {
        if key != "output" {
            item_clone.insert(key.clone(), value.clone());
        }
    }
    item_clone.insert("output".to_string(), Value::Object(read_clone));
    Some((Value::Object(item_clone), text))
}

// The projection needs the read metadata, not a second allocation of each
// (potentially hundreds of KiB) text body. Temporarily substitute empty JSON
// strings in the cloned view and add the exact serialized lengths of the
// original strings back to the measured envelope.
fn cloned_batch_without_read_text<'a>(output: &'a Value) -> (Value, Vec<(usize, &'a str)>) {
    let Some(root) = output.as_object() else {
        return (output.clone(), Vec::new());
    };
    let mut texts = Vec::new();
    let mut clone = serde_json::Map::new();
    for (key, value) in root {
        if key == "items" {
            if let Some(items) = value.as_array() {
                let items = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        if let Some((metadata, text)) = cloned_read_item_without_text(item) {
                            texts.push((index, text));
                            metadata
                        } else {
                            item.clone()
                        }
                    })
                    .collect::<Vec<_>>();
                clone.insert(key.clone(), Value::Array(items));
                continue;
            }
        }
        clone.insert(key.clone(), value.clone());
    }
    (Value::Object(clone), texts)
}

// serde_json emits printable UTF-8 bytes unchanged and escapes only ASCII
// control bytes, quote and backslash. Count this exactly instead of invoking
// a second JSON serializer on potentially large unchanged read bodies.
fn serialized_read_text_len(text: &str) -> Option<usize> {
    let mut bytes = text.len().checked_add(2)?; // opening and closing JSON quotes
    for byte in text.bytes() {
        let extra = match byte {
            b'"' | b'\\' | b'\x08' | b'\t' | b'\n' | b'\x0c' | b'\r' => 1,
            0x00..=0x1f => 5, // JSON \\u00XX expands one input byte to six
            _ => 0,
        };
        bytes = bytes.checked_add(extra)?;
    }
    Some(bytes)
}

fn projected_batch_serialized_len(output: &Value, projection: &ReadModelProjection) -> usize {
    let (lightweight, texts) = cloned_batch_without_read_text(output);
    let mut projected = ToolResult::ok(lightweight);
    add_actionable_read_continuations(projection, &mut projected);
    super::result_projection::sparsify_complete_read_success("read_files", &mut projected);

    // If a later projection changes/removes the text field, its semantics
    // must be measured by the canonical path rather than silently undercounted.
    if texts
        .iter()
        .any(|(index, _)| projected.output["items"][*index]["output"]["text"].as_str() != Some(""))
    {
        let mut original = ToolResult::ok(output.clone());
        add_actionable_read_continuations(projection, &mut original);
        super::result_projection::sparsify_complete_read_success("read_files", &mut original);
        return serialized_json_len(&original).unwrap_or(usize::MAX);
    }
    let Some(mut bytes) = serialized_json_len(&projected).ok() else {
        return usize::MAX;
    };
    for (_, text) in texts {
        let Some(encoded_text) = serialized_read_text_len(text) else {
            return usize::MAX;
        };
        let Some(next) = bytes.checked_add(encoded_text.saturating_sub(2)) else {
            return usize::MAX;
        };
        bytes = next;
    }
    bytes
}

fn projected_read_item_len(item: &Value) -> usize {
    let mut projected = item.clone();
    if projected["success"].as_bool() == Some(true) {
        let outer_path = projected
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string);
        if let (Some(outer_path), Some(output)) = (
            outer_path,
            projected.get_mut("output").and_then(Value::as_object_mut),
        ) {
            super::result_projection::sparsify_complete_file_read_output(output, Some(&outer_path));
            if output
                .get("read_revision")
                .and_then(Value::as_u64)
                .is_some()
            {
                output.remove("sha256");
            }
        }
    }
    serialized_value_len(&projected)
}

fn projected_batch_len(base_len: usize, item_bytes: usize, item_count: usize) -> usize {
    base_len
        .saturating_add(item_bytes)
        .saturating_add(item_count.saturating_sub(1))
}

fn set_truncated_read_metadata(
    output: &mut serde_json::Map<String, Value>,
    keep_lines: usize,
    returned_lines: usize,
    start_line: usize,
    existing_budget_next_limit: usize,
) {
    output.insert("returned_lines".to_string(), json!(keep_lines));
    output.insert(
        "end_line".to_string(),
        json!(start_line.saturating_add(keep_lines).saturating_sub(1)),
    );
    output.insert("has_more".to_string(), json!(true));
    output.insert(
        "next_start_line".to_string(),
        json!(start_line.saturating_add(keep_lines)),
    );
    output.insert("budget_truncated".to_string(), json!(true));
    output.insert(
        "budget_next_limit".to_string(),
        json!(returned_lines
            .saturating_sub(keep_lines)
            .saturating_add(existing_budget_next_limit)),
    );
}

fn truncate_read_item(item: &Value, keep_lines: usize) -> Option<Value> {
    if item["success"].as_bool() != Some(true) || keep_lines == 0 {
        return None;
    }
    let output = item.get("output")?.as_object()?;
    let returned_lines = output.get("returned_lines")?.as_u64()? as usize;
    if keep_lines >= returned_lines || returned_lines <= 1 {
        return None;
    }
    let start_line = output.get("start_line")?.as_u64()? as usize;
    let text = output.get("text")?.as_str()?;
    let lines = text.split('\n').collect::<Vec<_>>();
    if lines.len() != returned_lines {
        return None;
    }

    let existing_budget_next_limit = output
        .get("budget_next_limit")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let mut projected = item.clone();
    let projected_output = projected.get_mut("output")?.as_object_mut()?;
    projected_output.insert("text".to_string(), json!(lines[..keep_lines].join("\n")));
    set_truncated_read_metadata(
        projected_output,
        keep_lines,
        returned_lines,
        start_line,
        existing_budget_next_limit,
    );
    Some(projected)
}

// Build one indexed view over the original line separators. Each prefix length
// is its *serialized JSON body* byte count (without the two enclosing quotes),
// including the two-byte escaping of retained newlines. This makes every
// binary-search probe O(metadata), rather than resplitting or copying the text.
fn read_prefix_lengths(text: &str, returned_lines: usize) -> Option<Vec<usize>> {
    let mut prefixes = Vec::with_capacity(returned_lines.min(4096));
    let mut json_bytes = 0usize;
    for (index, line) in text.split('\n').enumerate() {
        if index >= returned_lines {
            return None;
        }
        let line_bytes = serialized_read_text_len(line)?.checked_sub(2)?;
        json_bytes = json_bytes.checked_add(line_bytes)?;
        prefixes.push(json_bytes);
        // Kept newlines serialize as \n (two JSON bytes). The trailing value
        // after the last line is ignored.
        json_bytes = json_bytes.checked_add(2)?;
    }
    (prefixes.len() == returned_lines).then_some(prefixes)
}

fn truncate_read_item_to_fit(item: &Value, max_item_bytes: usize) -> Option<TruncatedReadItem> {
    if item["success"].as_bool() != Some(true) {
        return None;
    }
    let output = item.get("output")?.as_object()?;
    let returned_lines = output.get("returned_lines")?.as_u64()? as usize;
    if returned_lines <= 1 {
        return None;
    }
    let start_line = output.get("start_line")?.as_u64()? as usize;
    let existing_budget_next_limit = output
        .get("budget_next_limit")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let (mut probe, text) = cloned_read_item_without_text(item)?;
    let prefixes = read_prefix_lengths(text, returned_lines)?;

    let mut low = 1usize;
    let mut high = returned_lines - 1;
    let mut best_keep = None;
    while low <= high {
        let keep = low + (high - low) / 2;
        let metadata = probe.get_mut("output")?.as_object_mut()?;
        set_truncated_read_metadata(
            metadata,
            keep,
            returned_lines,
            start_line,
            existing_budget_next_limit,
        );

        // The probe retains its empty-string placeholder. The partial result
        // always has has_more=true and a next_start_line, so complete-file
        // sparsification cannot remove its text field.
        let metadata_len = projected_read_item_len(&probe);
        let candidate_len = metadata_len.saturating_add(prefixes[keep - 1]);
        if candidate_len <= max_item_bytes {
            best_keep = Some(keep);
            low = keep.saturating_add(1);
        } else {
            high = keep.saturating_sub(1);
        }
    }

    // Construct the one selected output through the same canonical function
    // used by final hard-cap trimming, preserving every range/continuation key.
    best_keep.and_then(|keep| {
        truncate_read_item(item, keep).map(|output| TruncatedReadItem {
            output,
            retained_lines: keep,
        })
    })
}

// The second-stage follow-up can change in size when a partial read range is
// shortened. Keep one owned batch in place while probing candidate lengths:
// cloning the already-accepted prefix for every binary-search step scales with
// the entire batch rather than the candidate item.
fn sync_truncated_batch_counts(output: &mut Value, next_index: Option<usize>) {
    let items = output["items"]
        .as_array()
        .expect("truncated batch must contain canonical items");
    let returned_count = items.len();
    let succeeded_count = items
        .iter()
        .filter(|item| item["success"].as_bool() == Some(true))
        .count();
    output["returned_count"] = json!(returned_count);
    output["succeeded_count"] = json!(succeeded_count);
    output["failed_count"] = json!(returned_count - succeeded_count);
    output["next_index"] = json!(next_index);
}

fn fit_truncated_batch_to_budget(
    mut output: Value,
    payload_budget: usize,
    projection: &ReadModelProjection,
    mut retention: ReadRetainedPrefix,
) -> BudgetedReadBatch {
    while projected_batch_serialized_len(&output, projection) > payload_budget {
        let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
            break;
        };
        let Some(removed) = items.pop() else {
            break;
        };
        retention.item_count -= 1;
        retention.last_item_lines = None;
        let next_index = removed["index"].as_u64().map(|index| index as usize);
        let mut low = 1usize;
        let mut high = removed["output"]["returned_lines"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(1) as usize;
        let mut best = None;

        while low <= high {
            let keep = low + (high - low) / 2;
            let Some(partial) = truncate_read_item(&removed, keep) else {
                break;
            };
            output["items"]
                .as_array_mut()
                .expect("validated canonical batch")
                .push(partial);
            sync_truncated_batch_counts(&mut output, next_index);
            let fits = projected_batch_serialized_len(&output, projection) <= payload_budget;
            // Retain only the candidate item. The accepted prefix stays in
            // the original batch throughout the binary search.
            let candidate = output["items"]
                .as_array_mut()
                .expect("validated canonical batch")
                .pop()
                .expect("candidate was just appended");
            if fits {
                best = Some((candidate, keep));
                low = keep + 1;
            } else {
                high = keep.saturating_sub(1);
            }
        }
        if let Some((partial, keep)) = best {
            output["items"]
                .as_array_mut()
                .expect("validated canonical batch")
                .push(partial);
            sync_truncated_batch_counts(&mut output, next_index);
            retention.item_count += 1;
            retention.last_item_lines = Some(keep);
            return BudgetedReadBatch { output, retention };
        }
        sync_truncated_batch_counts(&mut output, next_index);
    }
    BudgetedReadBatch { output, retention }
}

fn apply_output_budget(
    project: &str,
    requested_count: usize,
    completed: Vec<Value>,
    max_result_bytes: Option<usize>,
    projection: &ReadModelProjection,
) -> BudgetedReadBatch {
    let mut retention = ReadRetainedPrefix::complete(completed.len());
    let result_budget = normalized_result_budget(max_result_bytes);
    let payload_budget = result_budget.saturating_sub(MODEL_RESULT_ENVELOPE_RESERVE_BYTES);
    // Own the canonical batch while measuring the final model projection.
    // The common complete-fit path can return it without cloning every read
    // body; on truncation, move its items into the existing budgeting loop.
    let mut complete = batch_output(project, requested_count, completed, false, None, None);
    // Budget the shape the model would actually receive after the existing
    // sparse projection. The canonical representation remains available for
    // Session recording and for any partial-item cursor construction below.
    if projected_batch_serialized_len(&complete, projection) <= payload_budget {
        return BudgetedReadBatch {
            output: complete,
            retention,
        };
    }
    let completed = complete["items"]
        .as_array_mut()
        .map(std::mem::take)
        .expect("batch_output always contains the canonical read items");

    let truncation_reason = if result_budget == MAX_SERIALIZED_OUTPUT_BYTES {
        "hard_result_cap"
    } else {
        "batch_response_budget"
    };
    // Estimate the outer follow-up cost from the empty truncated batch. The
    // final exact measurement below also covers changed partial-range arguments.
    let base_len = projected_batch_serialized_len(
        &batch_output(
            project,
            requested_count,
            Vec::new(),
            true,
            Some(0),
            Some(truncation_reason),
        ),
        projection,
    );
    let mut returned = Vec::with_capacity(completed.len());
    retention.item_count = 0;
    let mut returned_item_bytes = 0usize;
    let mut next_index = None;

    for item in completed {
        let index = item["index"].as_u64().unwrap_or(returned.len() as u64) as usize;
        let item_len = projected_read_item_len(&item);
        let candidate_item_count = returned.len() + 1;
        if projected_batch_len(
            base_len,
            returned_item_bytes.saturating_add(item_len),
            candidate_item_count,
        ) <= payload_budget
        {
            returned_item_bytes = returned_item_bytes.saturating_add(item_len);
            returned.push(item);
            retention.item_count += 1;
            continue;
        }

        let separator_bytes = usize::from(!returned.is_empty());
        let max_item_bytes = payload_budget
            .saturating_sub(base_len)
            .saturating_sub(returned_item_bytes)
            .saturating_sub(separator_bytes);
        if let Some(partial) = truncate_read_item_to_fit(&item, max_item_bytes) {
            returned.push(partial.output);
            retention.item_count += 1;
            retention.last_item_lines = Some(partial.retained_lines);
        }
        // A partial current item resumes from its next_start_line, while an
        // omitted item resumes from its original range. In both cases the
        // existing next_index can deterministically point at this same item.
        next_index = Some(index);
        break;
    }

    let output = batch_output(
        project,
        requested_count,
        returned,
        true,
        next_index,
        Some(truncation_reason),
    );
    // Recheck the exact final projection using in-place binary-search probes,
    // retaining the accepted prefix and all canonical continuation semantics.
    fit_truncated_batch_to_budget(output, payload_budget, projection, retention)
}

pub(crate) fn apply_model_facing_output_budget(
    result: &mut ToolResult,
    max_result_bytes: Option<usize>,
    projection: &ReadModelProjection,
) -> ReadOutputRetention {
    if !result.success {
        return ReadOutputRetention::default();
    }
    let Some(output) = result.output.as_object() else {
        return ReadOutputRetention::default();
    };
    let Some(project) = output
        .get("project")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return ReadOutputRetention::default();
    };
    let Some(requested_count) = output
        .get("requested_count")
        .and_then(Value::as_u64)
        .map(|count| count as usize)
    else {
        return ReadOutputRetention::default();
    };
    // All metadata has been validated; the original read bodies are no longer
    // needed separately after budgeting. Keep the surrounding result metadata
    // while transferring item ownership instead of cloning potentially 512 KiB.
    let Some(completed) = result
        .output
        .get_mut("items")
        .and_then(Value::as_array_mut)
        .map(std::mem::take)
    else {
        return ReadOutputRetention::default();
    };

    let budgeted = apply_output_budget(
        &project,
        requested_count,
        completed,
        max_result_bytes,
        projection,
    );
    let Some(root) = result.output.as_object_mut() else {
        return ReadOutputRetention::default();
    };
    for key in [
        "project",
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "items",
        "output_truncated",
        "next_index",
        "truncation_reason",
    ] {
        root.remove(key);
    }
    // Merge the owned budget result back into the original envelope to retain
    // any independent Session/context overlays, with no second item copy.
    if let Value::Object(budgeted) = budgeted.output {
        for (key, value) in budgeted {
            root.insert(key, value);
        }
    }
    ReadOutputRetention(Some(budgeted.retention))
}

fn final_model_result_len(output: &Value, projection: &ReadModelProjection) -> usize {
    // The final hard cap needs the same exact model-visible projection as
    // primary budgeting. Reuse its text-borrowing JSON sizer rather than
    // repeatedly cloning multi-hundred-KiB file bodies on each backoff probe.
    // Session/Job overlays remain in the canonical envelope and are counted.
    projected_batch_serialized_len(output, projection)
}

fn mark_final_hard_cap_truncation(output: &mut Value, next_index: usize) {
    let Some(root) = output.as_object_mut() else {
        return;
    };
    let (returned_count, succeeded_count) = root
        .get("items")
        .and_then(Value::as_array)
        .map(|items| {
            (
                items.len(),
                items
                    .iter()
                    .filter(|item| item["success"].as_bool() == Some(true))
                    .count(),
            )
        })
        .unwrap_or_default();
    root.insert("returned_count".to_string(), json!(returned_count));
    root.insert("succeeded_count".to_string(), json!(succeeded_count));
    root.insert(
        "failed_count".to_string(),
        json!(returned_count.saturating_sub(succeeded_count)),
    );
    root.insert("output_truncated".to_string(), json!(true));
    root.insert("next_index".to_string(), json!(next_index));
    root.insert("truncation_reason".to_string(), json!("hard_result_cap"));
}

/// Enforce the explicit 512 KiB model-inspection ceiling against the actual final
/// serialized ToolResult, including Session/continuity overlays. The primary
/// batch budget remains independent; this pass only removes/shortens read body
/// content when the fully decorated response would otherwise violate the hard
/// cap.
///
/// This expects the canonical batch envelope (before complete-success sparse
/// projection), so project/count/continuation metadata can remain truthful when
/// final hard-cap pressure turns a previously complete response into a partial
/// one.
pub(crate) fn enforce_final_model_facing_hard_cap(
    result: &mut ToolResult,
    projection: &ReadModelProjection,
) -> ReadOutputRetention {
    if !result.success
        || final_model_result_len(&result.output, projection) <= MAX_SERIALIZED_OUTPUT_BYTES
    {
        return ReadOutputRetention::default();
    }
    let Some(root) = result.output.as_object() else {
        return ReadOutputRetention::default();
    };
    if root.get("project").and_then(Value::as_str).is_none()
        || root
            .get("requested_count")
            .and_then(Value::as_u64)
            .is_none()
        || root.get("items").and_then(Value::as_array).is_none()
    {
        return ReadOutputRetention::default();
    }
    let mut retention = ReadRetainedPrefix::complete(
        root["items"]
            .as_array()
            .expect("validated read batch")
            .len(),
    );

    loop {
        let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut) else {
            return ReadOutputRetention(Some(retention));
        };
        // Move the last source item out while probing candidate line counts.
        // Previously every probe cloned the whole response (including all
        // accepted file bodies and Session/Job metadata) before trimming it.
        let Some(last) = items.pop() else {
            return ReadOutputRetention(Some(retention));
        };
        retention.item_count -= 1;
        retention.last_item_lines = None;
        let index = last["index"].as_u64().unwrap_or(0) as usize;

        // Preserve as many whole source lines as possible from the last success
        // item. Measuring the complete decorated ToolResult makes this exact and
        // naturally accounts for recovery/handoff/attention bytes.
        if last["success"].as_bool() == Some(true) {
            let returned_lines = last
                .get("output")
                .and_then(|output| output.get("returned_lines"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as usize;
            if returned_lines > 1 {
                let mut low = 1usize;
                let mut high = returned_lines - 1;
                let mut best = None;
                while low <= high {
                    let keep = low + (high - low) / 2;
                    let Some(partial) = truncate_read_item(&last, keep) else {
                        break;
                    };
                    result.output["items"]
                        .as_array_mut()
                        .expect("validated read batch")
                        .push(partial);
                    mark_final_hard_cap_truncation(&mut result.output, index);
                    let fits = final_model_result_len(&result.output, projection)
                        <= MAX_SERIALIZED_OUTPUT_BYTES;
                    // Take only the candidate back out; the accepted prefix
                    // and unrelated result metadata remain in the same object.
                    let candidate = result.output["items"]
                        .as_array_mut()
                        .expect("validated read batch")
                        .pop()
                        .expect("just appended candidate");
                    if fits {
                        best = Some((candidate, keep));
                        low = keep.saturating_add(1);
                    } else {
                        high = keep.saturating_sub(1);
                    }
                }
                if let Some((best, keep)) = best {
                    result.output["items"]
                        .as_array_mut()
                        .expect("validated read batch")
                        .push(best);
                    mark_final_hard_cap_truncation(&mut result.output, index);
                    retention.item_count += 1;
                    retention.last_item_lines = Some(keep);
                    return ReadOutputRetention(Some(retention));
                }
            }
        }

        // The item could not fit even after trimming. It has already been
        // removed; keep its exact original index as the continuation boundary.
        mark_final_hard_cap_truncation(&mut result.output, index);
        if final_model_result_len(&result.output, projection) <= MAX_SERIALIZED_OUTPUT_BYTES {
            return ReadOutputRetention(Some(retention));
        }
    }
}

impl ToolRuntime {
    /// Compose the read service without exposing unrelated runtime capabilities
    /// to its shared physical work. Authorization remains in the caller path.
    pub(crate) async fn read_project_snapshot(
        &self,
        resolved: &ResolvedProject,
        runner_project_id: &str,
        runner_instance_id: &str,
        path: String,
        start_line: Option<usize>,
        limit: Option<usize>,
        expected_sha256: Option<&str>,
        deadline: Instant,
    ) -> FileReadOutcome {
        let reader = super::files::ProjectFileReader::new(self.runner_registry.clone());
        self.reads
            .read_project_snapshot(
                &reader,
                resolved,
                runner_project_id,
                runner_instance_id,
                path,
                start_line,
                limit,
                expected_sha256,
                deadline,
            )
            .await
    }

    pub(crate) async fn read_files(
        &self,
        project: String,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        let resolved = match self.resolve_project_input(&project).await {
            Ok(project) => project,
            Err(error) => return ToolResult::err(error),
        };
        self.read_files_resolved(&resolved, items, with_line_numbers)
            .await
    }

    async fn read_planned_member(
        &self,
        resolved: &ResolvedProject,
        runner_project_id: &str,
        runner_instance_id: &str,
        path: &str,
        member: PlannedReadMember,
        expected_sha256: Option<&str>,
        with_line_numbers: bool,
        deadline: Instant,
    ) -> CompletedReadMember {
        let result = self
            .read_project_snapshot(
                resolved,
                runner_project_id,
                runner_instance_id,
                path.to_string(),
                member.start_line,
                member.limit,
                expected_sha256,
                deadline,
            )
            .await;
        let observed = match &result {
            Ok(range) => {
                let target =
                    WorkspaceReadRuntime::revision_target(resolved, path, runner_instance_id);
                Ok((
                    range,
                    self.reads.observe_revision(target, range.sha256.clone()),
                ))
            }
            Err(failure) => Err(*failure),
        };
        CompletedReadMember::new(member, path, observed, with_line_numbers)
    }

    pub(crate) async fn read_files_resolved(
        &self,
        resolved: &ResolvedProject,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
    ) -> ToolResult {
        self.read_files_planned_resolved(resolved, items, with_line_numbers, false)
            .await
            .result
    }

    /// Return successful union ranges once, while retaining the original member
    /// fallback. The accompanying items describe the actual output order/ranges
    /// so compound inspection can use canonical budget and continuation logic.
    pub(crate) async fn read_files_coalesced_resolved(
        &self,
        resolved: &ResolvedProject,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
    ) -> ReadFilesOutput {
        self.read_files_planned_resolved(resolved, items, with_line_numbers, true)
            .await
    }

    async fn read_files_planned_resolved(
        &self,
        resolved: &ResolvedProject,
        items: Vec<ReadFilesItem>,
        with_line_numbers: Option<bool>,
        coalesced_output: bool,
    ) -> ReadFilesOutput {
        if !(1..=MAX_READ_FILES_ITEMS).contains(&items.len())
            || items.iter().any(|item| item.path.trim().is_empty())
        {
            return ReadFilesOutput::failure(ToolResult::err(
                "read_files requires 1 to 8 items with non-empty paths",
            ));
        }

        let runtime_project_id = resolved.resolved_id.clone();
        let Some(runner_project_id) =
            crate::tool_runtime::runner_local_project_id(&resolved.resolved_id).map(str::to_string)
        else {
            return ReadFilesOutput::failure(ToolResult::err(
                "read_files could not bind the resolved Project to a Runner-local project id",
            ));
        };
        let planned_reads = plan_read_files(items.clone());
        let with_line_numbers = with_line_numbers.unwrap_or(false);
        let deadline = Instant::now() + self.read_files_deadline;
        // This is one absolute batch latency/resource deadline shared by all
        // planned reads. It is independent from any Runner execution lifetime
        // and is never reset as concurrent groups make progress.
        // Capture the active Runner process before dispatch. A replacement that
        // races this batch therefore makes these handles unusable rather than
        // silently retargeting them to the replacement Runner.
        let runner_instance_id = match self
            .runner_registry
            .get_runner_view(&resolved.config.client_id)
            .await
        {
            Some(view) => view.runner_instance_id,
            None => {
                return ReadFilesOutput::failure(
                    ToolResult::err_with_output(
                        "read_files could not bind the read snapshot to an active Runner process; retry after the Runner is available",
                        json!({
                            "project": runtime_project_id,
                            "state_changed": false,
                            "error_kind": "runner_unavailable",
                            "retry_guidance": "retry read_files after the owning Runner is available"
                        }),
                    ),
                )
            }
        };

        // The concurrency slot covers validation, enqueue, and response wait.
        // No request can reach the Runner until its future is polled by
        // `buffer_unordered`, so at most MAX_READ_FILES_CONCURRENCY file reads
        // are actually in flight.
        let completed_groups: Vec<Vec<CompletedReadMember>> =
            stream::iter(planned_reads.into_iter().map(|planned| {
                let runner_project_id = runner_project_id.clone();
                let runner_instance_id = runner_instance_id.clone();
                async move {
                    let PlannedRead { item, members } = planned;
                    let path = item.path;
                    let target =
                        WorkspaceReadRuntime::revision_target(resolved, &path, &runner_instance_id);
                    let expected_sha256 = match item.expected_read_revision {
                        Some(revision) => match self.reads.resolve_revision(revision, &target) {
                            Ok(sha256) => Some(sha256),
                            Err(_) => {
                                return members
                                    .into_iter()
                                    .map(|member| {
                                        CompletedReadMember::new(
                                            member,
                                            &path,
                                            Err(FileReadFailure::StaleRevision),
                                            with_line_numbers,
                                        )
                                    })
                                    .collect();
                            }
                        },
                        None => None,
                    };
                    let result = self
                        .read_project_snapshot(
                            resolved,
                            &runner_project_id,
                            &runner_instance_id,
                            path.clone(),
                            item.start_line,
                            item.limit,
                            expected_sha256.as_deref(),
                            deadline,
                        )
                        .await;
                    // Coalescing is an optimization, never a semantic reason for
                    // otherwise-valid member reads to fail. A merged UTF-8 range
                    // can cross the canonical raw-byte ceiling even when each
                    // caller range fits independently; only that bounded failure
                    // falls back to the original member reads.
                    if members.len() > 1
                        && matches!(
                            &result,
                            Err(FileReadFailure::Read(ReadFileReason::RangeTooLarge))
                        )
                    {
                        let mut fallback = Vec::with_capacity(members.len());
                        for member in members {
                            fallback.push(
                                self.read_planned_member(
                                    resolved,
                                    &runner_project_id,
                                    &runner_instance_id,
                                    &path,
                                    member,
                                    expected_sha256.as_deref(),
                                    with_line_numbers,
                                    deadline,
                                )
                                .await,
                            );
                        }
                        return fallback;
                    }

                    // Only a successful physical union can replace its members.
                    // Ordinary read_files and all failures retain caller ranges.
                    // A plain union can fit while numbering/JSON projection
                    // does not. In that case slice the original members from
                    // the successful snapshot; no additional Runner read.
                    let union_fits = coalesced_output
                        && result.as_ref().is_ok_and(|range| {
                            slice_read_file_range(
                                range,
                                item.start_line,
                                item.limit,
                                with_line_numbers,
                                &path,
                            )
                            .is_ok()
                        });
                    let members = if union_fits {
                        vec![PlannedReadMember {
                            index: members
                                .iter()
                                .map(|member| member.index)
                                .min()
                                .expect("planned read has at least one member"),
                            start_line: item.start_line,
                            limit: item.limit,
                        }]
                    } else {
                        members
                    };
                    let observed = match &result {
                        Ok(range) => Ok((
                            range,
                            self.reads.observe_revision(target, range.sha256.clone()),
                        )),
                        Err(failure) => Err(*failure),
                    };
                    members
                        .into_iter()
                        .map(|member| {
                            CompletedReadMember::new(member, &path, observed, with_line_numbers)
                        })
                        .collect()
                }
            }))
            .buffer_unordered(MAX_READ_FILES_CONCURRENCY)
            .collect()
            .await;
        let mut completed: Vec<CompletedReadMember> =
            completed_groups.into_iter().flatten().collect();
        completed.sort_by_key(|item| item.index);

        let output_items = if coalesced_output {
            completed
                .iter_mut()
                .enumerate()
                .map(|(index, entry)| {
                    let mut item = items[entry.index].clone();
                    if let Ok(observed) = &entry.result {
                        item.start_line = Some(observed.range.start_line);
                        item.limit = Some(observed.range.limit);
                    }
                    // Budget continuation indexes refer to this actual plan,
                    // including original ranges after byte-ceiling fallback.
                    entry.index = index;
                    item
                })
                .collect::<Vec<_>>()
        } else {
            items
        };
        let (outputs, retained): (Vec<_>, Vec<_>) = completed
            .into_iter()
            .map(|member| member.into_output(with_line_numbers))
            .unzip();
        ReadFilesOutput {
            result: ToolResult::ok(batch_output(
                &runtime_project_id,
                output_items.len(),
                outputs,
                false,
                None,
                None,
            )),
            items: output_items,
            retained: RetainedReadRanges(retained),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed_member(index: usize, content: &str) -> CompletedReadMember {
        let range = webcodex_workspace::file_read_range::read_range_from(
            content.as_bytes(),
            webcodex_workspace::file_read_range::EffectiveRange::new(None, None),
        )
        .unwrap();
        CompletedReadMember::new(
            PlannedReadMember {
                index,
                start_line: None,
                limit: None,
            },
            &format!("src/{index}.rs"),
            Ok((&range, 71 + index as u64)),
            false,
        )
    }

    fn typed_batch(members: Vec<CompletedReadMember>) -> (ToolResult, RetainedReadRanges) {
        let count = members.len();
        let (items, ranges) = members
            .into_iter()
            .map(|member| member.into_output(false))
            .unzip();
        (
            ToolResult::ok(batch_output(
                "agent:oe:demo",
                count,
                items,
                false,
                None,
                None,
            )),
            RetainedReadRanges(ranges),
        )
    }

    #[test]
    fn retained_read_ranges_follow_primary_budget_and_complete_file_sparsity() {
        let large = vec!["x".repeat(900); 200].join("\n");
        let (mut result, mut retained) = typed_batch(vec![
            typed_member(0, "one\ntwo\nthree"),
            typed_member(1, &large),
            typed_member(2, "later"),
        ]);
        let projection = batch_projection(3, None);
        retained.apply_retention(apply_model_facing_output_budget(
            &mut result,
            None,
            &projection,
        ));
        let items = result.output["items"].as_array().unwrap();
        assert_eq!(items.len(), 2, "the later item must be omitted");
        let kept = items[1]["output"]["returned_lines"].as_u64().unwrap();
        assert!((1..200).contains(&kept));
        assert!(retained.contains("src/1.rs", kept));
        assert!(!retained.contains("src/1.rs", kept + 1));
        assert!(!retained.contains("src/2.rs", 1));
        add_actionable_read_continuations(&projection, &mut result);
        super::super::result_projection::sparsify_complete_read_success("read_files", &mut result);
        assert!(result.output["items"][0]["output"]
            .get("start_line")
            .is_none());
        assert!(retained.contains("src/0.rs", 3));
        assert!(
            !retained.contains("src/0.rs", 4),
            "omitted range fields never imply whole-file wildcard coverage"
        );
    }

    #[test]
    fn retained_read_ranges_follow_final_hard_cap_after_primary_budget() {
        let large = vec!["x".repeat(900); 200].join("\n");
        let (mut result, mut retained) = typed_batch(vec![
            typed_member(0, "one\ntwo"),
            typed_member(1, &large),
            typed_member(2, "later"),
        ]);
        let projection = batch_projection(3, Some(MAX_SERIALIZED_OUTPUT_BYTES));
        retained.apply_retention(apply_model_facing_output_budget(
            &mut result,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &projection,
        ));
        assert!(retained.contains("src/1.rs", 200));
        assert!(retained.contains("src/2.rs", 1));
        result.output["context"] = json!("m".repeat(400_000));
        retained.apply_retention(enforce_final_model_facing_hard_cap(
            &mut result,
            &projection,
        ));
        let items = result.output["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        let kept = items[1]["output"]["returned_lines"].as_u64().unwrap();
        assert!((1..200).contains(&kept));
        assert!(retained.contains("src/1.rs", kept));
        assert!(!retained.contains("src/1.rs", kept + 1));
        assert!(!retained.contains("src/2.rs", 1));
        assert!(final_model_result_len(&result.output, &projection) <= MAX_SERIALIZED_OUTPUT_BYTES);
    }

    #[test]
    fn failed_empty_and_omitted_reads_never_claim_materialized_text() {
        let failed = CompletedReadMember::new(
            PlannedReadMember {
                index: 0,
                start_line: Some(1),
                limit: Some(10),
            },
            "src/0.rs",
            Err(ReadFileReason::NotFound.into()),
            false,
        );
        let (mut result, mut retained) = typed_batch(vec![
            failed,
            typed_member(1, ""),
            typed_member(2, &"x".repeat(100_000)),
        ]);
        let projection = batch_projection(3, None);
        retained.apply_retention(apply_model_facing_output_budget(
            &mut result,
            None,
            &projection,
        ));
        assert_eq!(result.output["items"].as_array().unwrap().len(), 2);
        for path in ["src/0.rs", "src/1.rs", "src/2.rs"] {
            assert!(
                !retained.contains(path, 1),
                "no returned source line for {path}"
            );
        }
    }

    fn batch_projection(count: usize, max_result_bytes: Option<usize>) -> ReadModelProjection {
        ReadModelProjection::Batch {
            project: "agent:oe:demo".to_string(),
            session_id: None,
            items: (0..count)
                .map(|index| ReadFilesItem {
                    path: format!("src/{index}.rs"),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                })
                .collect(),
            with_line_numbers: None,
            max_result_bytes,
        }
    }

    fn ranged_item(index: usize, start_line: usize, lines: &[String]) -> Value {
        let returned_lines = lines.len();
        json!({
            "index": index,
            "path": format!("src/{index}.rs"),
            "success": true,
            "output": {
                "text": lines.join("\n"),
                "format": "plain",
                "path": format!("src/{index}.rs"),
                "sha256": "c".repeat(64),
                "read_revision": 10_000_u64 + index as u64,
                "start_line": start_line,
                "limit": returned_lines,
                "total_lines": start_line + returned_lines - 1,
                "returned_lines": returned_lines,
                "end_line": start_line + returned_lines - 1,
                "has_more": false,
                "next_start_line": null
            },
            "error": null
        })
    }

    fn default_complete_item(index: usize, lines: &[String]) -> Value {
        let returned_lines = lines.len();
        let default_limit =
            webcodex_workspace::file_read_range::EffectiveRange::new(None, None).limit;
        json!({
            "index": index,
            "path": format!("src/{index}.rs"),
            "success": true,
            "output": {
                "text": lines.join("\n"),
                "format": "plain",
                "path": format!("src/{index}.rs"),
                "sha256": "d".repeat(64),
                "read_revision": 20_000_u64 + index as u64,
                "start_line": 1,
                "limit": default_limit,
                "total_lines": returned_lines,
                "returned_lines": returned_lines,
                "end_line": if returned_lines == 0 { Value::Null } else { json!(returned_lines) },
                "has_more": false,
                "next_start_line": null
            },
            "error": null
        })
    }

    // Frozen reference of the previous second-stage trimming implementation.
    // Deliberately retains its prefix clone and fresh batch construction so
    // correctness can be tested independently from the optimized ownership
    // path, including exact continuation and serialized sizing semantics.
    fn reference_truncated_batch_fit(
        mut output: Value,
        payload_budget: usize,
        projection: &ReadModelProjection,
    ) -> Value {
        let project = output["project"].as_str().unwrap().to_string();
        let requested_count = output["requested_count"].as_u64().unwrap() as usize;
        let reason = output["truncation_reason"].as_str().unwrap().to_string();
        while projected_batch_serialized_len(&output, projection) > payload_budget {
            let Some(items) = output.get_mut("items").and_then(Value::as_array_mut) else {
                break;
            };
            let Some(removed) = items.pop() else {
                break;
            };
            let next_index = removed["index"].as_u64().map(|index| index as usize);
            let prefix = items.clone();
            let mut low = 1;
            let mut high = removed["output"]["returned_lines"]
                .as_u64()
                .unwrap_or(0)
                .saturating_sub(1) as usize;
            let mut best = None;
            while low <= high {
                let keep = low + (high - low) / 2;
                let Some(partial) = truncate_read_item(&removed, keep) else {
                    break;
                };
                let mut candidate_items = prefix.clone();
                candidate_items.push(partial);
                let candidate = batch_output(
                    &project,
                    requested_count,
                    candidate_items,
                    true,
                    next_index,
                    Some(&reason),
                );
                if projected_batch_serialized_len(&candidate, projection) <= payload_budget {
                    best = Some(candidate);
                    low = keep + 1;
                } else {
                    high = keep.saturating_sub(1);
                }
            }
            if let Some(candidate) = best {
                return candidate;
            }
            output = batch_output(
                &project,
                requested_count,
                prefix,
                true,
                next_index,
                Some(&reason),
            );
        }
        output
    }

    #[test]
    fn truncated_batch_backoff_matches_previous_exact_model_result_across_budgets() {
        let unicode = (0..180)
            .map(|index| format!("第{index:03}行-{}-\\\"\\t", "界".repeat(25)))
            .collect::<Vec<_>>();
        let ascii = (0..210)
            .map(|index| format!("line-{index:04}-{}", "x".repeat(100)))
            .collect::<Vec<_>>();
        let mut failed = ranged_item(1, 25, &["rejected".to_string()]);
        failed["success"] = json!(false);
        failed["error"] = json!("read_file failed: PRIVATE_MISSING");
        let batches = [
            (vec![ranged_item(0, 11, &unicode)], Some(0usize)),
            (
                vec![
                    default_complete_item(0, &["prefix".repeat(1000)]),
                    ranged_item(1, 11, &ascii),
                ],
                Some(1),
            ),
            (
                vec![
                    default_complete_item(0, &["prefix".repeat(3500)]),
                    default_complete_item(1, &["sentinel".repeat(1200)]),
                    ranged_item(2, 51, &unicode),
                ],
                Some(2),
            ),
            (
                vec![
                    default_complete_item(0, &["first".repeat(600)]),
                    failed,
                    ranged_item(2, 11, &ascii),
                ],
                Some(2),
            ),
            (Vec::new(), Some(0)),
        ];
        let mut exercised_partial = 0;
        let mut exercised_complete_drop = 0;
        for (items, next_index) in batches {
            for reason in ["batch_response_budget", "hard_result_cap"] {
                for budget in [
                    256,
                    1024,
                    2048,
                    4096,
                    8 * 1024,
                    16 * 1024,
                    32 * 1024,
                    64 * 1024,
                ] {
                    let count = items.len();
                    let input = batch_output(
                        "agent:oe:demo",
                        count,
                        items.clone(),
                        true,
                        next_index,
                        Some(reason),
                    );
                    let mut projection =
                        batch_projection(count, Some(budget + MODEL_RESULT_ENVELOPE_RESERVE_BYTES));
                    if let ReadModelProjection::Batch {
                        with_line_numbers, ..
                    } = &mut projection
                    {
                        *with_line_numbers = Some(true);
                    }
                    let expected =
                        reference_truncated_batch_fit(input.clone(), budget, &projection);
                    let actual = fit_truncated_batch_to_budget(
                        input,
                        budget,
                        &projection,
                        ReadRetainedPrefix::complete(count),
                    )
                    .output;
                    assert_eq!(
                        actual, expected,
                        "budget={budget} reason={reason} items={count}: optimized backoff must be exact"
                    );
                    if actual["items"].as_array().is_some_and(|returned| {
                        returned
                            .iter()
                            .any(|item| item["output"]["budget_truncated"] == true)
                    }) {
                        exercised_partial += 1;
                    }
                    if actual["items"]
                        .as_array()
                        .is_some_and(|returned| returned.len() < count)
                    {
                        exercised_complete_drop += 1;
                    }
                    let mut model = ToolResult::ok(actual);
                    add_actionable_read_continuations(&projection, &mut model);
                    super::super::result_projection::sparsify_complete_read_success(
                        "read_files",
                        &mut model,
                    );
                    if let Some(call) = model.output.get("suggested_call") {
                        assert_eq!(call["tool"], "read_files");
                        ToolCall::from_tool_name("read_files", call["arguments"].clone())
                            .expect("backoff continuation must be parser-ready");
                    }
                }
            }
        }
        assert!(
            exercised_partial > 0,
            "fixture must exercise partial backoff"
        );
        assert!(
            exercised_complete_drop > 0,
            "fixture must exercise whole-item backoff"
        );
    }

    #[test]
    fn owned_budget_merge_preserves_overlays_and_continuations() {
        let metadata = json!({
            "session_hint": {"open_counts": {"todo": 2}},
            "job_attention": {"items": [{"state": "terminal", "outcome": "passed"}]},
            "future_metadata": {"value": "opaque", "count": 7}
        });
        let partial_lines = (0..900)
            .map(|index| format!("第{index:04}行-{}", "界".repeat(40)))
            .collect::<Vec<_>>();
        let failed = json!({
            "index": 1,
            "path": "src/1.rs",
            "success": false,
            "output": {"reason_code": "not_file"},
            "error": "read_file failed: not_file"
        });

        for (items, budget) in [
            (
                vec![
                    default_complete_item(0, &["alpha\n\t界".to_string()]),
                    default_complete_item(1, &["beta".to_string()]),
                ],
                None,
            ),
            (vec![ranged_item(0, 11, &partial_lines)], None),
            (
                vec![
                    default_complete_item(0, &["x".repeat(140 * 1024)]),
                    failed,
                    default_complete_item(2, &["y".repeat(140 * 1024)]),
                ],
                Some(256 * 1024),
            ),
        ] {
            let count = items.len();
            let projection = batch_projection(count, budget);
            let mut expected = ToolResult::ok(
                apply_output_budget("agent:oe:demo", count, items.clone(), budget, &projection)
                    .output,
            );
            let mut actual = ToolResult::ok(batch_output(
                "agent:oe:demo",
                count,
                items,
                false,
                None,
                None,
            ));
            for (key, value) in metadata.as_object().unwrap() {
                actual.output[key] = value.clone();
                expected.output[key] = value.clone();
            }

            apply_model_facing_output_budget(&mut actual, budget, &projection);
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "moving owned items must preserve all canonical fields and overlays"
            );
            add_actionable_read_continuations(&projection, &mut actual);
            add_actionable_read_continuations(&projection, &mut expected);
            super::super::result_projection::sparsify_complete_read_success(
                "read_files",
                &mut actual,
            );
            super::super::result_projection::sparsify_complete_read_success(
                "read_files",
                &mut expected,
            );
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(&expected).unwrap(),
                "continuation and sparse model projection must be unchanged"
            );
        }
    }

    #[test]
    fn owned_budget_guard_preserves_invalid_or_failed_result_without_mutation() {
        for mut result in [
            ToolResult::ok(json!({"project": "demo", "requested_count": 1})),
            ToolResult::ok(json!({"requested_count": 1, "items": []})),
            ToolResult::ok(json!({"project": "demo", "items": []})),
            ToolResult::ok(json!({"project": "demo", "requested_count": 1, "items": "invalid"})),
            ToolResult::err_with_output(
                "read_files failed",
                json!({"project": "demo", "requested_count": 1, "items": []}),
            ),
        ] {
            let before = serde_json::to_value(&result).unwrap();
            apply_model_facing_output_budget(&mut result, None, &batch_projection(1, None));
            assert_eq!(
                serde_json::to_value(&result).unwrap(),
                before,
                "invalid or failed results cannot lose items or metadata"
            );
        }
    }

    // Independent copy of the pre-optimization implementation. Do not
    // delegate to the optimized range-field helper: this is the behavioral
    // oracle for both chosen text and the exact continuation metadata.
    fn legacy_partial_read_item(item: &Value, keep_lines: usize) -> Option<Value> {
        if item["success"].as_bool() != Some(true) || keep_lines == 0 {
            return None;
        }
        let output = item.get("output")?.as_object()?;
        let returned_lines = output.get("returned_lines")?.as_u64()? as usize;
        if keep_lines >= returned_lines || returned_lines <= 1 {
            return None;
        }
        let start_line = output.get("start_line")?.as_u64()? as usize;
        let text = output.get("text")?.as_str()?;
        let lines = text.split('\n').collect::<Vec<_>>();
        if lines.len() != returned_lines {
            return None;
        }
        let mut projected = item.clone();
        let projected_output = projected.get_mut("output")?.as_object_mut()?;
        projected_output.insert("text".to_string(), json!(lines[..keep_lines].join("\n")));
        projected_output.insert("returned_lines".to_string(), json!(keep_lines));
        projected_output.insert(
            "end_line".to_string(),
            json!(start_line.saturating_add(keep_lines).saturating_sub(1)),
        );
        projected_output.insert("has_more".to_string(), json!(true));
        projected_output.insert(
            "next_start_line".to_string(),
            json!(start_line.saturating_add(keep_lines)),
        );
        projected_output.insert("budget_truncated".to_string(), json!(true));
        let existing_budget_next_limit = output
            .get("budget_next_limit")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        projected_output.insert(
            "budget_next_limit".to_string(),
            json!(returned_lines
                .saturating_sub(keep_lines)
                .saturating_add(existing_budget_next_limit)),
        );
        Some(projected)
    }

    fn legacy_truncate_read_item_to_fit(item: &Value, max_item_bytes: usize) -> Option<Value> {
        let returned_lines = item.get("output")?.get("returned_lines")?.as_u64()? as usize;
        if returned_lines <= 1 {
            return None;
        }
        let mut low = 1usize;
        let mut high = returned_lines - 1;
        let mut best = None;
        while low <= high {
            let keep = low + (high - low) / 2;
            let Some(candidate) = legacy_partial_read_item(item, keep) else {
                break;
            };
            if projected_read_item_len(&candidate) <= max_item_bytes {
                best = Some(candidate);
                low = keep.saturating_add(1);
            } else {
                high = keep.saturating_sub(1);
            }
        }
        best
    }

    #[test]
    fn truncate_read_item_to_fit_matches_legacy_over_utf8_budget_and_metadata() {
        let unicode = (0..900)
            .map(|index| format!("第{index:04}行-{}-{}", "界".repeat(30), "🦀".repeat(8)))
            .collect::<Vec<_>>();
        let control = (0u8..=31).map(char::from).collect::<String>();
        let escape_lines = (0..240)
            .map(|index| format!("{index}: {control} \" \\\\ \t 🦀"))
            .collect::<Vec<_>>();
        let mut continued = ranged_item(0, 11, &unicode);
        continued["output"]["has_more"] = json!(true);
        continued["output"]["total_lines"] = json!(1010);
        continued["output"]["next_start_line"] = json!(911);
        continued["output"]["budget_truncated"] = json!(true);
        continued["output"]["budget_next_limit"] = json!(110);
        continued["output"]["unexpected_private_field"] = json!({
            "sentinel": "keep-this-field",
            "escaped": "quotation: \" and newlines: \n"
        });
        let numbered = ranged_item(0, 37, &escape_lines);
        let trailing_empty = ranged_item(0, 1, &["last".to_string(), String::new()]);
        let mut malformed = ranged_item(0, 11, &unicode[..3]);
        malformed["output"]["returned_lines"] = json!(4);
        let mut no_start = ranged_item(0, 11, &unicode[..3]);
        no_start["output"]
            .as_object_mut()
            .unwrap()
            .remove("start_line");
        let mut failed = ranged_item(0, 11, &unicode[..3]);
        failed["success"] = json!(false);
        let mut bad_text = ranged_item(0, 11, &unicode[..3]);
        bad_text["output"]["text"] = json!(["not-a-string"]);
        let mut no_outer_path = ranged_item(0, 11, &unicode[..3]);
        no_outer_path.as_object_mut().unwrap().remove("path");

        let fixtures = [
            continued,
            numbered,
            trailing_empty,
            ranged_item(0, 11, &unicode[..3]),
            malformed,
            no_start,
            failed,
            bad_text,
            no_outer_path,
            default_complete_item(0, &["first".to_string(), "second".to_string()]),
            ranged_item(0, 11, &["only one line".to_string()]),
        ];
        let budgets = [
            0usize,
            1,
            16,
            64,
            128,
            256,
            512,
            1024,
            2048,
            4096,
            8192,
            16 * 1024,
            32 * 1024,
            48 * 1024,
            64 * 1024,
            96 * 1024,
            128 * 1024,
        ];
        let mut observed_partial = 0;
        let mut observed_no_fit = 0;
        for (index, item) in fixtures.iter().enumerate() {
            let unchanged = item.clone();
            for &max in &budgets {
                let expected = legacy_truncate_read_item_to_fit(item, max);
                let actual = truncate_read_item_to_fit(item, max).map(|partial| partial.output);
                assert_eq!(
                    actual, expected,
                    "fixture={index} max_item_bytes={max}: candidate content/range/continuation changed"
                );
                if let Some(partial) = &actual {
                    observed_partial += 1;
                    assert!(
                        projected_read_item_len(partial) <= max,
                        "chosen item must respect exact projected byte ceiling"
                    );
                } else {
                    observed_no_fit += 1;
                }
            }
            assert_eq!(item, &unchanged, "measurement may not mutate caller input");
        }
        assert!(observed_partial > 0);
        assert!(observed_no_fit > 0);
    }

    #[test]
    fn read_prefix_json_lengths_match_actual_joined_utf8_and_controls() {
        let unicode_and_controls = (0..25)
            .map(|i| format!("行-{i} \" \\ \t\u{0001}\u{001f} 😀"))
            .collect::<Vec<_>>();
        let text = unicode_and_controls.join("\n");
        let prefixes = read_prefix_lengths(&text, unicode_and_controls.len()).unwrap();
        for keep in 1..=unicode_and_controls.len() {
            let expected =
                serialized_read_text_len(&unicode_and_controls[..keep].join("\n")).unwrap() - 2;
            assert_eq!(prefixes[keep - 1], expected, "keep={keep}");
        }
        for incorrect in [
            0usize,
            unicode_and_controls.len() - 1,
            unicode_and_controls.len() + 1,
        ] {
            assert!(read_prefix_lengths(&text, incorrect).is_none());
        }
        assert_eq!(read_prefix_lengths("first\n", 2).unwrap(), vec![5, 7]);
        assert_eq!(read_prefix_lengths("", 1).unwrap(), vec![0]);
    }

    #[test]
    fn complete_default_sparse_fit_is_not_preemptively_budget_truncated() {
        let payload_budget = DEFAULT_READ_FILES_RESULT_BYTES - MODEL_RESULT_ENVELOPE_RESERVE_BYTES;
        let mut selected = None;
        for text_bytes in 6_000..=9_000 {
            let completed = (0..8)
                .map(|index| default_complete_item(index, &["x".repeat(text_bytes)]))
                .collect::<Vec<_>>();
            let canonical = batch_output("agent:oe:demo", 8, completed.clone(), false, None, None);
            let canonical_bytes = serialized_batch_len(&canonical);
            let sparse_bytes =
                projected_batch_serialized_len(&canonical, &batch_projection(8, None));
            if canonical_bytes > payload_budget && sparse_bytes <= payload_budget {
                selected = Some((completed, canonical_bytes, sparse_bytes));
                break;
            }
        }
        let (completed, canonical_bytes, sparse_bytes) =
            selected.expect("test fixture must straddle canonical/sparse budget boundary");
        assert!(canonical_bytes > payload_budget);
        assert!(sparse_bytes <= payload_budget);

        let mut result = ToolResult::ok(batch_output(
            "agent:oe:demo",
            8,
            completed.clone(),
            false,
            None,
            None,
        ));
        apply_model_facing_output_budget(&mut result, None, &batch_projection(8, None));
        assert_eq!(result.output["output_truncated"], false);
        assert!(result.output["next_index"].is_null());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
        for (actual, expected) in result.output["items"]
            .as_array()
            .unwrap()
            .iter()
            .zip(completed.iter())
        {
            assert_eq!(actual["output"]["text"], expected["output"]["text"]);
        }

        super::super::result_projection::sparsify_complete_read_success("read_files", &mut result);
        assert!(result.output.get("output_truncated").is_none());
        assert!(result.output.get("next_index").is_none());
        assert_eq!(result.output["items"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn output_budget_keeps_whole_items_and_points_at_first_omitted_index() {
        let item = |index, text: String| {
            json!({
                "index": index,
                "path": format!("src/{index}.rs"),
                "success": true,
                "output": {
                    "text": text,
                    "format": "plain",
                    "path": format!("src/{index}.rs"),
                    "sha256": "a".repeat(64),
                    "start_line": 1,
                    "limit": 1,
                    "total_lines": 1,
                    "returned_lines": 1,
                    "end_line": 1,
                    "has_more": false,
                    "next_start_line": null
                },
                "error": null
            })
        };
        let legacy_budget = 256 * 1024;
        let projection = batch_projection(2, Some(legacy_budget));
        let completed = vec![
            item(0, "x".repeat(140 * 1024)),
            item(1, "y".repeat(140 * 1024)),
        ];
        let output = apply_output_budget(
            "agent:oe:demo",
            2,
            completed.clone(),
            Some(legacy_budget),
            &projection,
        )
        .output;
        assert_eq!(output["returned_count"], 1);
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["next_index"], 1);
        assert_eq!(output["items"].as_array().unwrap().len(), 1);
        let serialized = serde_json::to_vec(&ToolResult::ok(output.clone())).unwrap();
        assert!(serialized.len() <= legacy_budget);
        let mut model = ToolResult::ok(output);
        add_actionable_read_continuations(&projection, &mut model);
        super::super::result_projection::sparsify_complete_read_success("read_files", &mut model);
        let serialized = serde_json::to_vec(&model).unwrap();
        assert!(
            serialized.len() <= legacy_budget,
            "actionable continuation must remain inside the explicit 256 KiB budget: {} bytes",
            serialized.len()
        );

        let expanded_projection = batch_projection(2, Some(MAX_SERIALIZED_OUTPUT_BYTES));
        let expanded = apply_output_budget(
            "agent:oe:demo",
            2,
            completed,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &expanded_projection,
        )
        .output;
        assert_eq!(expanded["returned_count"], 2);
        assert_eq!(expanded["output_truncated"], false);
    }

    #[test]
    fn omitted_batch_items_have_reusable_sliced_read_files_call() {
        let legacy_budget = 256 * 1024;
        let first = vec!["x".repeat(140 * 1024)];
        let second = vec!["y".repeat(140 * 1024)];
        let third = vec!["z".to_string()];
        let mut projection = batch_projection(3, Some(legacy_budget));
        if let ReadModelProjection::Batch {
            items, session_id, ..
        } = &mut projection
        {
            items[1].expected_read_revision = Some(1234);
            *session_id = Some("wc_sess_batch_recovery".to_string());
        }
        let output = apply_output_budget(
            "agent:oe:demo",
            3,
            vec![
                ranged_item(0, 1, &first),
                ranged_item(1, 1, &second),
                ranged_item(2, 1, &third),
            ],
            Some(legacy_budget),
            &projection,
        )
        .output;
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["next_index"], 1);
        assert_eq!(output["items"].as_array().unwrap().len(), 1);

        let mut model = ToolResult::ok(output);
        add_actionable_read_continuations(&projection, &mut model);
        let suggested = &model.output["suggested_call"];
        assert_eq!(suggested["tool"], "read_files");
        assert_eq!(
            suggested["arguments"]["session_id"],
            "wc_sess_batch_recovery"
        );
        assert!(suggested["arguments"].get("next_index").is_none());
        assert_eq!(
            suggested["arguments"]["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["path"].as_str().unwrap())
                .collect::<Vec<_>>(),
            vec!["src/1.rs", "src/2.rs"]
        );
        assert_eq!(
            suggested["arguments"]["items"][0]["expected_read_revision"],
            1234
        );
        assert!(suggested["arguments"]["items"][1]
            .get("expected_read_revision")
            .is_none());
        let next = ToolCall::from_tool_name(
            suggested["tool"].as_str().unwrap(),
            suggested["arguments"].clone(),
        )
        .expect("batch continuation suggested_call must parse");
        assert!(matches!(
            next,
            ToolCall::ReadFiles {
                ref items,
                session_id: Some(ref next_session_id),
                max_result_bytes: Some(bytes),
                ..
            } if bytes == legacy_budget
                && next_session_id == "wc_sess_batch_recovery"
                && items.iter().map(|item| item.path.as_str()).collect::<Vec<_>>()
                    == vec!["src/1.rs", "src/2.rs"]
        ));
    }

    #[test]
    fn partial_item_and_remaining_items_have_one_ordered_call() {
        let lines = (0..900)
            .map(|index| format!("第{index:04}行-{}", "界".repeat(40)))
            .collect::<Vec<_>>();
        for (count, partial_index) in [(1, 0), (3, 0), (3, 1), (3, 2)] {
            let projection = batch_projection(count, None);
            let completed = (0..count)
                .map(|index| {
                    if index == partial_index {
                        default_complete_item(index, &lines)
                    } else {
                        default_complete_item(index, &["complete".to_string()])
                    }
                })
                .collect();
            let output =
                apply_output_budget("agent:oe:demo", count, completed, None, &projection).output;
            let partial = &output["items"][partial_index]["output"];
            assert_eq!(partial["budget_truncated"], true);
            let next_start = partial["next_start_line"].clone();
            let next_limit = partial["budget_next_limit"].clone();
            let revision = partial["read_revision"].clone();
            let mut model = ToolResult::ok(output);
            add_actionable_read_continuations(&projection, &mut model);
            super::super::result_projection::sparsify_complete_read_success(
                "read_files",
                &mut model,
            );
            let call = &model.output["suggested_call"];
            ToolCall::from_tool_name(call["tool"].as_str().unwrap(), call["arguments"].clone())
                .unwrap();
            let remaining = call["arguments"]["items"].as_array().unwrap();
            assert_eq!(remaining.len(), count - partial_index);
            assert_eq!(remaining[0]["start_line"], next_start);
            assert_eq!(remaining[0]["limit"], next_limit);
            assert_eq!(remaining[0]["expected_read_revision"], revision);
            for item in remaining.iter().skip(1) {
                assert!(item.get("expected_read_revision").is_none());
            }
            for (offset, item) in remaining.iter().enumerate() {
                assert_eq!(item["path"], format!("src/{}.rs", partial_index + offset));
            }
            assert_eq!(
                model.output["items"][partial_index]["output"]["read_revision"],
                revision
            );
            assert!(model.output.get("next_index").is_none());
            assert!(model.output.get("continuation").is_none());
            assert!(model.output["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item.get("continuation").is_none()));
            assert!(serde_json::to_vec(&model).unwrap().len() <= DEFAULT_READ_FILES_RESULT_BYTES);
        }
    }

    #[test]
    fn zero_progress_primary_budget_recommends_bounded_budget_refinement() {
        let huge_line = vec!["x".repeat(90 * 1024)];
        let projection = batch_projection(1, None);
        let output = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![ranged_item(0, 1, &huge_line)],
            None,
            &projection,
        )
        .output;
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["next_index"], 0);
        assert!(output["items"].as_array().unwrap().is_empty());

        let mut model = ToolResult::ok(output);
        add_actionable_read_continuations(&projection, &mut model);
        let suggested = &model.output["suggested_call"];
        assert_eq!(
            suggested["arguments"]["max_result_bytes"],
            MAX_SERIALIZED_OUTPUT_BYTES
        );
        let next = ToolCall::from_tool_name(
            suggested["tool"].as_str().unwrap(),
            suggested["arguments"].clone(),
        )
        .expect("budget refinement suggested_call must parse");
        assert!(matches!(
            next,
            ToolCall::ReadFiles {
                max_result_bytes: Some(bytes),
                ..
            } if bytes == MAX_SERIALIZED_OUTPUT_BYTES
        ));
    }

    #[test]
    fn zero_progress_hard_cap_does_not_advertise_fake_batch_replay() {
        let oversized_single_line = vec!["x".repeat(MAX_SERIALIZED_OUTPUT_BYTES + 1024)];
        let projection = batch_projection(1, Some(MAX_SERIALIZED_OUTPUT_BYTES));
        let output = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![ranged_item(0, 1, &oversized_single_line)],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &projection,
        )
        .output;
        assert_eq!(output["output_truncated"], true);
        assert_eq!(output["truncation_reason"], "hard_result_cap");
        assert_eq!(output["next_index"], 0);
        assert!(output["items"].as_array().unwrap().is_empty());

        let mut model = ToolResult::ok(output);
        add_actionable_read_continuations(&projection, &mut model);
        assert!(
            model.output.get("suggested_call").is_none(),
            "a hard-cap zero-progress replay is not actionable: {}",
            model.output
        );
    }

    #[test]
    fn output_budget_reserves_space_for_outer_session_metadata() {
        let item = |index, text: String| {
            json!({
                "index": index,
                "path": format!("src/{index}.rs"),
                "success": true,
                "output": {
                    "text": text,
                    "format": "plain",
                    "path": format!("src/{index}.rs"),
                    "sha256": "b".repeat(64),
                    "start_line": 1,
                    "limit": 1,
                    "total_lines": 1,
                    "returned_lines": 1,
                    "end_line": 1,
                    "has_more": false,
                    "next_start_line": null
                },
                "error": null
            })
        };
        let output = apply_output_budget(
            "agent:oe:demo",
            3,
            vec![
                item(0, "x".repeat(180 * 1024)),
                item(1, "y".repeat(180 * 1024)),
                item(2, "z".repeat(180 * 1024)),
            ],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &batch_projection(3, Some(MAX_SERIALIZED_OUTPUT_BYTES)),
        )
        .output;
        assert_eq!(output["returned_count"], 2);
        assert_eq!(output["next_index"], 2);

        let mut result = ToolResult::ok(output);
        result.output["session_hint"] = json!({
            "has_open_messages": true,
            "open_counts": {
                "guidance": u64::MAX,
                "question": u64::MAX,
                "todo": u64::MAX,
                "risk": u64::MAX
            },
            "highest_priority": "high",
            "suggested_next_tool": "read_session_discussion_summary"
        });
        assert!(serde_json::to_vec(&result).unwrap().len() <= MAX_SERIALIZED_OUTPUT_BYTES);
    }

    #[test]
    fn default_budget_partials_on_line_boundaries_and_explicit_large_returns_more() {
        let lines = (0..900)
            .map(|index| format!("第{index:04}行-{}", "界".repeat(40)))
            .collect::<Vec<_>>();
        let completed = vec![default_complete_item(0, &lines)];

        let default = apply_output_budget(
            "agent:oe:demo",
            1,
            completed.clone(),
            None,
            &batch_projection(1, None),
        )
        .output;
        let large = apply_output_budget(
            "agent:oe:demo",
            1,
            completed,
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &batch_projection(1, Some(MAX_SERIALIZED_OUTPUT_BYTES)),
        )
        .output;
        let partial = &default["items"][0]["output"];
        let kept = partial["returned_lines"].as_u64().unwrap() as usize;
        assert!(kept > 0 && kept < lines.len());
        assert_eq!(default["next_index"], 0);
        assert_eq!(default["truncation_reason"], "batch_response_budget");
        assert_eq!(partial["budget_truncated"], true);
        assert_eq!(partial["next_start_line"], kept + 1);
        assert_eq!(partial["budget_next_limit"], lines.len() - kept);
        assert_eq!(partial["text"], lines[..kept].join("\n"));
        assert!(std::str::from_utf8(partial["text"].as_str().unwrap().as_bytes()).is_ok());
        assert!(
            serde_json::to_vec(&ToolResult::ok(default)).unwrap().len()
                <= DEFAULT_READ_FILES_RESULT_BYTES
        );
        assert_eq!(large["output_truncated"], false);
        assert_eq!(large["items"][0]["output"]["returned_lines"], lines.len());
    }

    #[test]
    fn read_budget_cursor_reconstructs_original_range_without_gaps() {
        let lines = (0..700)
            .map(|index| format!("line-{index:04}-{}", "x".repeat(90)))
            .collect::<Vec<_>>();
        let first = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![ranged_item(0, 11, &lines)],
            None,
            &batch_projection(1, None),
        )
        .output;
        let first_output = &first["items"][0]["output"];
        let kept = first_output["returned_lines"].as_u64().unwrap() as usize;
        let next_start = first_output["next_start_line"].as_u64().unwrap() as usize;
        let next_limit = first_output["budget_next_limit"].as_u64().unwrap() as usize;
        assert_eq!(next_start, 11 + kept);
        assert_eq!(next_limit, lines.len() - kept);

        let continuation = apply_output_budget(
            "agent:oe:demo",
            1,
            vec![ranged_item(0, next_start, &lines[kept..])],
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &batch_projection(1, Some(MAX_SERIALIZED_OUTPUT_BYTES)),
        )
        .output;
        let joined = format!(
            "{}\n{}",
            first_output["text"].as_str().unwrap(),
            continuation["items"][0]["output"]["text"].as_str().unwrap()
        );
        assert_eq!(joined, lines.join("\n"));
    }

    #[test]
    fn final_read_trim_preserves_existing_budget_tail_without_gaps() {
        let first_chunk = (0..60)
            .map(|index| format!("line-{index:03}"))
            .collect::<Vec<_>>();
        let mut item = ranged_item(0, 11, &first_chunk);
        item["output"]["limit"] = json!(100);
        item["output"]["total_lines"] = json!(110);
        item["output"]["has_more"] = json!(true);
        item["output"]["next_start_line"] = json!(71);
        item["output"]["budget_truncated"] = json!(true);
        item["output"]["budget_next_limit"] = json!(40);

        let partial = truncate_read_item(&item, 25).expect("second-stage partial read");
        let output = &partial["output"];
        assert_eq!(output["returned_lines"], 25);
        assert_eq!(output["end_line"], 35);
        assert_eq!(output["next_start_line"], 36);
        assert_eq!(output["budget_next_limit"], 75);
        assert_eq!(output["budget_truncated"], true);
    }

    #[test]
    fn read_continuation_byte_measurements_cover_complete_partial_and_batch_recovery() {
        let canonical_bytes = |output: &Value| {
            serde_json::to_vec(&ToolResult::ok(output.clone()))
                .unwrap()
                .len()
        };
        let model_bytes = |tool: &str, output: &Value, projection: &ReadModelProjection| {
            let mut model = ToolResult::ok(output.clone());
            add_actionable_read_continuations(projection, &mut model);
            super::super::result_projection::sparsify_complete_read_success(tool, &mut model);
            serde_json::to_vec(&model).unwrap().len()
        };

        let complete_batch = batch_output(
            "agent:oe:demo",
            2,
            vec![
                default_complete_item(0, &["alpha".to_string()]),
                default_complete_item(1, &["beta".to_string()]),
            ],
            false,
            None,
            None,
        );
        let complete_batch_projection = batch_projection(2, None);
        let complete_batch_canonical = canonical_bytes(&complete_batch);
        let complete_batch_model =
            model_bytes("read_files", &complete_batch, &complete_batch_projection);

        let mut partial_item = ranged_item(0, 11, &["line-11".to_string()]);
        partial_item["output"]["limit"] = json!(1);
        partial_item["output"]["total_lines"] = json!(20);
        partial_item["output"]["has_more"] = json!(true);
        partial_item["output"]["next_start_line"] = json!(12);
        let partial_batch = batch_output("agent:oe:demo", 1, vec![partial_item], false, None, None);
        let partial_batch_projection = ReadModelProjection::Batch {
            project: "agent:oe:demo".to_string(),
            session_id: None,
            items: vec![ReadFilesItem {
                path: "src/0.rs".to_string(),
                start_line: Some(11),
                limit: Some(1),
                expected_read_revision: None,
            }],
            with_line_numbers: None,
            max_result_bytes: None,
        };
        let partial_batch_canonical = canonical_bytes(&partial_batch);
        let partial_batch_model =
            model_bytes("read_files", &partial_batch, &partial_batch_projection);

        let large_projection = batch_projection(2, Some(MAX_SERIALIZED_OUTPUT_BYTES));
        let full_budget_batch = batch_output(
            "agent:oe:demo",
            2,
            vec![
                ranged_item(0, 1, &["x".repeat(140 * 1024)]),
                ranged_item(1, 1, &["y".repeat(140 * 1024)]),
            ],
            false,
            None,
            None,
        );
        let budget_batch_canonical = canonical_bytes(&full_budget_batch);
        let budgeted = apply_output_budget(
            "agent:oe:demo",
            2,
            full_budget_batch["items"].as_array().unwrap().clone(),
            Some(MAX_SERIALIZED_OUTPUT_BYTES),
            &large_projection,
        )
        .output;
        let budget_batch_model = model_bytes("read_files", &budgeted, &large_projection);

        let partial_lines = (0..900)
            .map(|index| format!("第{index:04}行-{}", "界".repeat(40)))
            .collect::<Vec<_>>();
        let partial_plus_later_full = batch_output(
            "agent:oe:demo",
            3,
            vec![
                default_complete_item(0, &partial_lines),
                ranged_item(1, 1, &["later".to_string()]),
                ranged_item(2, 1, &["last".to_string()]),
            ],
            false,
            None,
            None,
        );
        let partial_plus_later_projection = batch_projection(3, None);
        let partial_plus_later_canonical = canonical_bytes(&partial_plus_later_full);
        let partial_plus_later_budgeted = apply_output_budget(
            "agent:oe:demo",
            3,
            partial_plus_later_full["items"].as_array().unwrap().clone(),
            None,
            &partial_plus_later_projection,
        )
        .output;
        let partial_plus_later_model = model_bytes(
            "read_files",
            &partial_plus_later_budgeted,
            &partial_plus_later_projection,
        );

        eprintln!(
            "read_continuation_bytes complete_read_files={complete_batch_canonical}->{complete_batch_model} partial_item={partial_batch_canonical}->{partial_batch_model} batch_budget={budget_batch_canonical}->{budget_batch_model} partial_plus_batch={partial_plus_later_canonical}->{partial_plus_later_model}"
        );

        assert!(complete_batch_model < complete_batch_canonical);
        assert!(budget_batch_model <= MAX_SERIALIZED_OUTPUT_BYTES);
        assert!(partial_plus_later_model <= DEFAULT_READ_FILES_RESULT_BYTES);
    }

    // Frozen previous final-cap size measurement. Independently checks the
    // borrowed-text projection used by production against the full JSON clone.
    fn legacy_final_model_result_len(output: &Value, projection: &ReadModelProjection) -> usize {
        let mut projected = ToolResult::ok(output.clone());
        add_actionable_read_continuations(projection, &mut projected);
        super::super::result_projection::sparsify_complete_read_success(
            "read_files",
            &mut projected,
        );
        serialized_json_len(&projected).unwrap_or(usize::MAX)
    }

    // Frozen original implementation before in-place final hard-cap candidates.
    fn legacy_enforce_final_model_facing_hard_cap(
        result: &mut ToolResult,
        projection: &ReadModelProjection,
    ) {
        if !result.success
            || legacy_final_model_result_len(&result.output, projection)
                <= MAX_SERIALIZED_OUTPUT_BYTES
        {
            return;
        }
        let Some(root) = result.output.as_object() else {
            return;
        };
        if root.get("project").and_then(Value::as_str).is_none()
            || root
                .get("requested_count")
                .and_then(Value::as_u64)
                .is_none()
            || root.get("items").and_then(Value::as_array).is_none()
        {
            return;
        }

        loop {
            let Some(items) = result.output.get("items").and_then(Value::as_array) else {
                return;
            };
            let Some(last) = items.last() else {
                return;
            };
            let index = last["index"].as_u64().unwrap_or(0) as usize;

            // Preserve as many whole source lines as possible from the last success
            // item. Measuring the complete decorated ToolResult makes this exact and
            // naturally accounts for recovery/handoff/attention bytes.
            if last["success"].as_bool() == Some(true) {
                let returned_lines = last
                    .get("output")
                    .and_then(|output| output.get("returned_lines"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0) as usize;
                if returned_lines > 1 {
                    let mut low = 1usize;
                    let mut high = returned_lines - 1;
                    let mut best = None;
                    while low <= high {
                        let keep = low + (high - low) / 2;
                        let Some(partial) = truncate_read_item(last, keep) else {
                            break;
                        };
                        let mut candidate = result.output.clone();
                        if let Some(candidate_items) =
                            candidate.get_mut("items").and_then(Value::as_array_mut)
                        {
                            let last_index = candidate_items.len() - 1;
                            candidate_items[last_index] = partial;
                        }
                        mark_final_hard_cap_truncation(&mut candidate, index);
                        if legacy_final_model_result_len(&candidate, projection)
                            <= MAX_SERIALIZED_OUTPUT_BYTES
                        {
                            best = Some(candidate);
                            low = keep.saturating_add(1);
                        } else {
                            high = keep.saturating_sub(1);
                        }
                    }
                    if let Some(best) = best {
                        result.output = best;
                        return;
                    }
                }
            }

            let removed_index = {
                let Some(items) = result.output.get_mut("items").and_then(Value::as_array_mut)
                else {
                    return;
                };
                items
                    .pop()
                    .and_then(|removed| removed["index"].as_u64())
                    .unwrap_or(index as u64) as usize
            };
            mark_final_hard_cap_truncation(&mut result.output, removed_index);
            if legacy_final_model_result_len(&result.output, projection)
                <= MAX_SERIALIZED_OUTPUT_BYTES
            {
                return;
            }
        }
    }

    #[test]
    fn final_hard_cap_size_matches_frozen_full_clone_on_all_projection_shapes() {
        let utf = (0..850)
            .map(|line| format!("line-{line:04} {} quote=\" slash=\\", "界🦀".repeat(25)))
            .collect::<Vec<_>>();
        let mut failure = ranged_item(1, 1, &["private failure".to_string()]);
        failure["success"] = json!(false);
        failure["error"] = json!("error");
        let examples = [
            batch_output(
                "agent:oe:demo",
                1,
                vec![default_complete_item(0, &["abc".to_string()])],
                false,
                None,
                None,
            ),
            batch_output(
                "agent:oe:demo",
                2,
                vec![ranged_item(0, 11, &utf), failure],
                true,
                Some(0),
                Some("hard_result_cap"),
            ),
            batch_output(
                "agent:oe:demo",
                1,
                vec![default_complete_item(0, &["a".repeat(488 * 1024)])],
                false,
                None,
                None,
            ),
            json!({"unexpected": "中\\n🌍", "items": "malformed"}),
            json!({"items": [], "output_truncated": true, "next_index": 0,
                   "unknown_metadata": {"error": "unknown"}}),
        ];
        for mut output in examples {
            output["session_hint"] = json!({"work": "unfinished", "notes": "界🦀"});
            output["job_attention"] = json!({"items": [{"count": 3, "status": "ready"}]});
            output["extra"] = json!({"unicode": "🥟", "slash": "\\\\", "tab": "\\t"});
            let snapshot = output.clone();
            for projection in [
                batch_projection(1, None),
                batch_projection(2, Some(MAX_SERIALIZED_OUTPUT_BYTES)),
                ReadModelProjection::None,
            ] {
                assert_eq!(
                    final_model_result_len(&output, &projection),
                    legacy_final_model_result_len(&output, &projection),
                    "final 512 KiB cap sizer must equal frozen canonical serialization"
                );
                assert_eq!(output, snapshot, "sizing must not mutate result");
            }
        }
    }

    #[test]
    fn final_hard_cap_in_place_matches_legacy_across_partial_drops_and_metadata() {
        let utf_lines = (0..900)
            .map(|line| format!("第{line:04}行-{}", "界".repeat(42)))
            .collect::<Vec<_>>();
        let ascii_lines = (0..1100)
            .map(|line| format!("line={line} {} \" \\", "x".repeat(150)))
            .collect::<Vec<_>>();
        let mut existing_partial = ranged_item(1, 11, &ascii_lines);
        existing_partial["output"]["budget_next_limit"] = json!(89);
        existing_partial["output"]["budget_truncated"] = json!(true);
        existing_partial["output"]["has_more"] = json!(true);
        existing_partial["output"]["next_start_line"] = json!(1111);
        existing_partial["output"]["total_lines"] = json!(1199);

        let mut failed = ranged_item(0, 1, &["bad read".into()]);
        failed["success"] = json!(false);
        failed["error"] = json!("private_error_marker");

        let cases = [
            (
                vec![default_complete_item(0, &["short".to_string()])],
                0usize,
            ),
            (vec![ranged_item(0, 11, &utf_lines)], 455 * 1024),
            (
                vec![
                    default_complete_item(0, &["p".repeat(340 * 1024)]),
                    existing_partial.clone(),
                ],
                92 * 1024,
            ),
            (
                vec![
                    default_complete_item(0, &["p".repeat(90 * 1024)]),
                    failed.clone(),
                    ranged_item(2, 11, &utf_lines),
                ],
                390 * 1024,
            ),
            (
                (0..8)
                    .map(|index| default_complete_item(index, &["X".repeat(85 * 1024)]))
                    .collect(),
                0,
            ),
            (
                vec![default_complete_item(0, &["Y".repeat(510 * 1024)])],
                25 * 1024,
            ),
            (vec![failed], 550 * 1024),
        ];

        let mut actual_partials = 0usize;
        let mut actual_drops = 0usize;
        for (case, (items, metadata_bytes)) in cases.into_iter().enumerate() {
            let count = items.len();
            let projection = batch_projection(count, Some(MAX_SERIALIZED_OUTPUT_BYTES));
            let mut base = ToolResult::ok(batch_output(
                "agent:oe:demo",
                count,
                items,
                false,
                None,
                None,
            ));
            base.output["session_hint"] = json!({
                "purpose": "keep",
                "todo": ["question", "context", "open"],
                "weird": "新しい 🦀 \n \t \" \\"
            });
            base.output["job_attention"] = json!({
                "items": [{"state": "terminal", "outcome": "passed"}]
            });
            base.output["unknown_extended_metadata"] = json!({
                "padding": "m".repeat(metadata_bytes)
            });
            let input = ToolResult::ok(base.output.clone());
            let mut previous = ToolResult::ok(base.output.clone());
            legacy_enforce_final_model_facing_hard_cap(&mut previous, &projection);
            enforce_final_model_facing_hard_cap(&mut base, &projection);
            assert_eq!(
                serde_json::to_value(&base).unwrap(),
                serde_json::to_value(&previous).unwrap(),
                "case={case}: canonical ToolResult differs from legacy"
            );
            let before_items = input.output["items"].as_array().unwrap();
            let after_items = base.output["items"].as_array().unwrap();
            actual_drops += usize::from(after_items.len() < before_items.len());
            if after_items.iter().any(|item| {
                item["output"]["budget_truncated"] == true
                    && before_items.iter().any(|prior| {
                        prior["index"] == item["index"]
                            && prior["output"]["returned_lines"] != item["output"]["returned_lines"]
                    })
            }) {
                actual_partials += 1;
            }
            assert!(base.output["session_hint"].is_object());
            assert_eq!(base.output["job_attention"], input.output["job_attention"]);

            let mut model = ToolResult::ok(base.output.clone());
            let mut old_model = previous;
            add_actionable_read_continuations(&projection, &mut model);
            add_actionable_read_continuations(&projection, &mut old_model);
            super::super::result_projection::sparsify_complete_read_success(
                "read_files",
                &mut model,
            );
            super::super::result_projection::sparsify_complete_read_success(
                "read_files",
                &mut old_model,
            );
            assert_eq!(
                serde_json::to_value(&model).unwrap(),
                serde_json::to_value(&old_model).unwrap(),
                "case={case}: projected parser-visible result/continuations differ"
            );
            if let Some(next) = model.output.get("suggested_call") {
                assert_eq!(next["tool"], "read_files");
                ToolCall::from_tool_name("read_files", next["arguments"].clone())
                    .expect("suggested continuation must parse");
            }
        }
        assert!(
            actual_partials > 0,
            "some fixture must fit a partial final read"
        );
        assert!(
            actual_drops > 0,
            "some fixture must drop an entire final item"
        );
    }

    #[test]
    fn final_hard_cap_stops_on_missing_metadata_and_failure_without_mutation() {
        for mut result in [
            ToolResult::ok(
                json!({"items": [], "requested_count": 1, "other": "x".repeat(530 * 1024)}),
            ),
            ToolResult::ok(
                json!({"project": "demo", "items": [], "other": "x".repeat(530 * 1024)}),
            ),
            ToolResult::ok(
                json!({"project": "demo", "requested_count": 1, "items": "not-array",
                                  "other": "x".repeat(530 * 1024)}),
            ),
            ToolResult::err_with_output(
                "failed",
                json!({
                    "project": "demo", "requested_count": 1,
                    "items": [], "other": "x".repeat(530 * 1024)
                }),
            ),
        ] {
            let before = serde_json::to_value(&result).unwrap();
            enforce_final_model_facing_hard_cap(&mut result, &batch_projection(1, None));
            assert_eq!(serde_json::to_value(&result).unwrap(), before);
        }
    }

    #[test]
    fn inspection_ceiling_is_independent_from_single_file_read_cap() {
        assert_eq!(MAX_SERIALIZED_OUTPUT_BYTES, 512 * 1024);
        assert_eq!(
            webcodex_workspace::file_read_range::MAX_SERIALIZED_OUTPUT_BYTES,
            256 * 1024
        );
        assert_eq!(
            webcodex_workspace::file_read_range::MAX_RANGE_CONTENT_BYTES,
            192 * 1024
        );
    }

    fn canonical_projected_batch_bytes(output: &Value, projection: &ReadModelProjection) -> usize {
        let mut result = ToolResult::ok(output.clone());
        add_actionable_read_continuations(projection, &mut result);
        super::super::result_projection::sparsify_complete_read_success("read_files", &mut result);
        serialized_json_len(&result).unwrap_or(usize::MAX)
    }

    #[test]
    fn serialized_read_text_len_matches_serde_json_for_all_control_bytes_and_utf8() {
        let control: String = (0u8..=0x1f).map(char::from).collect();
        let examples = [
            "",
            "\"",
            "\\",
            "\n\r\t\x08\x0c",
            "\u{0000}\u{001f}\u{007f}",
            "slash / and HTML < > &",
            "Àéहिंदी日本語🦀",
            "\u{2028}\u{2029}\u{0085}",
            "control plus quotes",
        ];
        for text in examples
            .iter()
            .copied()
            .chain(std::iter::once(control.as_str()))
        {
            assert_eq!(
                serialized_read_text_len(text),
                Some(serialized_json_len(text).unwrap()),
                "exact escaped JSON length must match serde_json for {text:?}"
            );
        }
        let mixed = format!(
            "{}{}{}{}",
            control.repeat(140),
            "quotes \\\" and escaped \\\\ \n\r\t".repeat(400),
            "界🦀".repeat(4096),
            "ASCII only".repeat(2048),
        );
        assert_eq!(
            serialized_read_text_len(&mixed),
            Some(serialized_json_len(&mixed).unwrap())
        );
        assert_eq!(
            serialized_read_text_len(&"x".repeat(128 * 1024)),
            Some(128 * 1024 + 2)
        );
    }

    #[test]
    fn projected_budget_measurement_skips_read_text_copies_exactly() {
        let strings = [
            String::new(),
            "plain ASCII".to_string(),
            "unicode 漢字 🦀 café".to_string(),
            "quote=\\\" escape=\\\\ newline=\\n tab=\\t".to_string(),
            "\\u{0000}\\u{0001}\\u{001f}\\u{007f}".to_string(),
            "A".repeat(95 * 1024),
            "界".repeat(20 * 1024),
        ];
        for value in &strings {
            for (numbered, output_truncated) in [(false, false), (true, false), (false, true)] {
                let mut item = if numbered {
                    ranged_item(0, 11, &[value.clone()])
                } else {
                    default_complete_item(0, &[value.clone()])
                };
                if output_truncated {
                    item["output"]["has_more"] = json!(true);
                    item["output"]["total_lines"] = json!(30);
                    item["output"]["next_start_line"] = json!(12);
                    item["output"]["budget_truncated"] = json!(true);
                    item["output"]["budget_next_limit"] = json!(19);
                }
                let mut output = batch_output(
                    "agent:oe:demo",
                    1,
                    vec![item],
                    output_truncated,
                    output_truncated.then_some(0),
                    output_truncated.then_some("batch_response_budget"),
                );
                // Data unrelated to read text must still count in the exact
                // projection; known/unknown fields cannot disappear.
                output["unknown_metadata"] = json!({
                    "unicode": "田中 🦀",
                    "escaped": "one\\ntwo\\tthree",
                });
                let projection = ReadModelProjection::Batch {
                    project: "agent:oe:demo".to_string(),
                    items: vec![ReadFilesItem {
                        path: "src/0.rs".to_string(),
                        start_line: Some(if numbered { 11 } else { 1 }),
                        limit: Some(100),
                        expected_read_revision: None,
                    }],
                    session_id: Some("wc_sess_budget_probe".to_string()),
                    with_line_numbers: Some(numbered),
                    max_result_bytes: Some(256 * 1024),
                };
                let snapshot = output.clone();
                let expected = canonical_projected_batch_bytes(&output, &projection);
                let actual = projected_batch_serialized_len(&output, &projection);
                assert_eq!(actual, expected,
                    "JSON projection sizing must match canonical bytes: numbered={numbered}, truncated={output_truncated}, input_bytes={}",
                    value.len());
                assert_eq!(
                    output, snapshot,
                    "measurement must not mutate original read result"
                );
            }
        }
    }

    #[test]
    fn projected_budget_measurement_preserves_multiple_and_malformed_items() {
        let mut failed = ranged_item(2, 3, &["failed".to_string()]);
        failed["success"] = json!(false);
        failed["error"] = json!("private failure");
        let outputs = [
            batch_output(
                "agent:oe:demo",
                3,
                vec![
                    default_complete_item(0, &["valid".to_string()]),
                    ranged_item(1, 21, &["test".repeat(30_000)]),
                    failed,
                ],
                false,
                None,
                None,
            ),
            batch_output(
                "agent:oe:demo",
                2,
                vec![
                    json!({"success": true, "index": 0, "output": {"text": ["not", "a", "string"]}}),
                    json!({"success": false, "index": 1, "error": "failed", "output": null}),
                ],
                true,
                Some(1),
                Some("hard_result_cap"),
            ),
            batch_output(
                "agent:oe:demo",
                0,
                vec![],
                true,
                Some(0),
                Some("hard_result_cap"),
            ),
            json!({"items": "not an array", "other": {"text": "PRIVATE"}}),
            json!({"items": [null, [], {}, {"output": {"text": "content"}}]}),
        ];
        for output in &outputs {
            for projection in [
                batch_projection(3, None),
                batch_projection(3, Some(MAX_SERIALIZED_OUTPUT_BYTES)),
                ReadModelProjection::None,
            ] {
                assert_eq!(
                    projected_batch_serialized_len(output, &projection),
                    canonical_projected_batch_bytes(output, &projection),
                    "malformed/partial/read_failure schema cannot change size accounting"
                );
            }
        }
    }

    #[test]
    fn result_budget_clamps_to_existing_hard_bounds() {
        assert_eq!(
            normalized_result_budget(Some(MIN_READ_FILES_RESULT_BYTES / 2)),
            MIN_READ_FILES_RESULT_BYTES
        );
        assert_eq!(
            normalized_result_budget(Some(MAX_SERIALIZED_OUTPUT_BYTES * 2)),
            MAX_SERIALIZED_OUTPUT_BYTES
        );
    }

    #[test]
    fn model_projection_canonicalizes_explicit_result_budget() {
        for (requested, effective) in [
            (MIN_READ_FILES_RESULT_BYTES / 2, MIN_READ_FILES_RESULT_BYTES),
            (MAX_SERIALIZED_OUTPUT_BYTES * 2, MAX_SERIALIZED_OUTPUT_BYTES),
        ] {
            let call = ToolCall::ReadFiles {
                project: "demo".to_string(),
                items: vec![ReadFilesItem {
                    path: "src/lib.rs".to_string(),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                }],
                session_id: None,
                with_line_numbers: None,
                max_result_bytes: Some(requested),
            };
            let ReadModelProjection::Batch {
                max_result_bytes, ..
            } = ReadModelProjection::capture(&call)
            else {
                panic!("read_files projection must capture batch call");
            };
            assert_eq!(max_result_bytes, Some(effective));
        }
    }

    #[test]
    fn read_plan_coalesces_duplicate_and_nearby_ranges() {
        let planned = plan_read_files(vec![
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(100),
                limit: Some(50),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(100),
                limit: Some(50),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(160),
                limit: Some(20),
                expected_read_revision: None,
            },
        ]);

        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].item.start_line, Some(100));
        assert_eq!(planned[0].item.limit, Some(80));
        assert_eq!(planned[0].members.len(), 3);
        assert_eq!(
            planned[0]
                .members
                .iter()
                .map(|member| member.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn read_plan_merges_ranges_transitively_after_sorting() {
        let planned = plan_read_files(vec![
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(1),
                limit: Some(20),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(55),
                limit: Some(16),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(21),
                limit: Some(34),
                expected_read_revision: None,
            },
        ]);

        assert_eq!(planned.len(), 1);
        assert_eq!(planned[0].item.start_line, Some(1));
        assert_eq!(planned[0].item.limit, Some(70));
        assert_eq!(planned[0].members.len(), 3);
    }

    #[test]
    fn read_plan_benchmark_scenarios_reduce_runner_reads() {
        let duplicate = (0..8)
            .map(|_| ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(100),
                limit: Some(50),
                expected_read_revision: None,
            })
            .collect::<Vec<_>>();
        let adjacent = (0..8)
            .map(|index| ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(1 + index * 40),
                limit: Some(40),
                expected_read_revision: None,
            })
            .collect::<Vec<_>>();
        let mixed = vec![
            ("src/a.rs", 1, 40),
            ("src/a.rs", 35, 40),
            ("src/a.rs", 120, 40),
            ("src/a.rs", 155, 40),
            ("src/b.rs", 10, 30),
            ("src/b.rs", 25, 30),
            ("src/c.rs", 1, 20),
            ("src/d.rs", 1, 20),
        ]
        .into_iter()
        .map(|(path, start, limit)| ReadFilesItem {
            path: path.to_string(),
            start_line: Some(start),
            limit: Some(limit),
            expected_read_revision: None,
        })
        .collect::<Vec<_>>();

        let duplicate_plans = plan_read_files(duplicate);
        let adjacent_plans = plan_read_files(adjacent);
        let mixed_plans = plan_read_files(mixed);

        eprintln!(
            "read_plan_benchmark duplicate=8->{} adjacent=8->{} mixed=8->{}",
            duplicate_plans.len(),
            adjacent_plans.len(),
            mixed_plans.len()
        );
        assert_eq!(duplicate_plans.len(), 1);
        assert_eq!(adjacent_plans.len(), 1);
        assert_eq!(mixed_plans.len(), 5);
    }

    #[test]
    fn read_plan_keeps_distant_or_differently_fenced_ranges_independent() {
        let planned = plan_read_files(vec![
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(1),
                limit: Some(20),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(200),
                limit: Some(20),
                expected_read_revision: None,
            },
            ReadFilesItem {
                path: "src/lib.rs".to_string(),
                start_line: Some(15),
                limit: Some(20),
                expected_read_revision: Some(7),
            },
        ]);

        assert_eq!(planned.len(), 3);
        assert!(planned.iter().all(|plan| plan.members.len() == 1));
    }
}
