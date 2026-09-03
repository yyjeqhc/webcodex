//! Shared model-output normalization for `read_file`.
//!
//! Both the local ToolRuntime and the Runner-backed path produce their final
//! model-facing success object through [`success_output`], so the field set,
//! cursor math, and line-numbered text representation are owned in one place.
//! The agent path additionally validates the runner's v1 envelope (see the
//! ToolRuntime caller) and then hands the resulting [`FileReadRange`] here.

use crate::file_read_range::{FileReadRange, MAX_SERIALIZED_OUTPUT_BYTES};
use serde_json::{json, Value};

/// Space reserved inside the final 256 KiB model-result budget for the outer
/// `ToolResult` envelope and bounded model-facing Session metadata added after
/// the file payload is built (for example continuity/recovery and inbox hints).
/// `read_file` is read-only, so it never receives a permission decision block.
pub const MODEL_RESULT_ENVELOPE_RESERVE_BYTES: usize = 4 * 1024;

/// Maximum serialized size of the inner `output` object before later wrapping.
pub const MAX_SERIALIZED_OUTPUT_PAYLOAD_BYTES: usize =
    MAX_SERIALIZED_OUTPUT_BYTES - MODEL_RESULT_ENVELOPE_RESERVE_BYTES;

/// Build the unified `read_file` success output object from a shared range
/// result. `with_line_numbers` selects the `text` representation and `format`
/// (`plain` or `numbered`); it never changes the range cursor metadata.
///
/// The returned object is the complete model-facing success payload — it
/// carries only the canonical fields and no agent envelope extras. Callers
/// still set `path` afterwards.
pub fn success_output(range: &FileReadRange, with_line_numbers: bool) -> Value {
    let (text, format) = if with_line_numbers {
        (
            numbered_text(&range.content, range.start_line, range.returned_lines),
            "numbered",
        )
    } else {
        (range.content.clone(), "plain")
    };
    json!({
        "text": text,
        "format": format,
        "sha256": range.sha256,
        "start_line": range.start_line,
        "limit": range.limit,
        "total_lines": range.total_lines,
        "returned_lines": range.returned_lines,
        "end_line": range.end_line,
        "has_more": range.has_more,
        "next_start_line": range.next_start_line,
    })
}

/// Render selected content as `"{start} | {line}"` per line joined by `\n`,
/// matching the established line-numbered format. `returned_lines`
/// disambiguates an empty selection (`0` → empty text) from a single selected
/// blank line (`1` with empty `content` → `"{start} | "`).
fn numbered_text(content: &str, start_line: usize, returned_lines: usize) -> String {
    if returned_lines == 0 {
        return String::new();
    }
    // For returned_lines > 0 the content has exactly `returned_lines`
    // `\n`-joined segments (one segment even when the single line is blank).
    content
        .split('\n')
        .take(returned_lines)
        .enumerate()
        .map(|(idx, line)| format!("{} | {}", start_line + idx, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Hard limit on the serialized model output. Exposed for callers that want to
/// re-check the final serialized size (including after JSON escaping and line
/// numbering) against the same constant the normalizer is anchored to.
pub fn max_serialized_output_bytes() -> usize {
    MAX_SERIALIZED_OUTPUT_BYTES
}

/// Serialize the inner `output` object and report whether it leaves the fixed
/// reserve required by the outer result envelope and bounded Session telemetry.
/// This is enforced after JSON escaping and optional line numbering, not just
/// against raw content length.
pub fn serialized_fits(output: &Value) -> bool {
    serde_json::to_vec(output)
        .map(|bytes| bytes.len() <= MAX_SERIALIZED_OUTPUT_PAYLOAD_BYTES)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_read_range::{EffectiveRange, ReadFileReason};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn read(path: &std::path::Path, start: Option<usize>, limit: Option<usize>) -> FileReadRange {
        crate::file_read_range::read_range(path, EffectiveRange::new(start, limit)).unwrap()
    }

    #[test]
    fn plain_success_fields() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "one").unwrap();
        writeln!(f, "two").unwrap();
        write!(f, "three").unwrap();
        let r = read(f.path(), Some(2), Some(1));
        let out = success_output(&r, false);
        assert_eq!(out["text"], "two");
        assert_eq!(out["format"], "plain");
        assert_eq!(out["start_line"], 2);
        assert_eq!(out["limit"], 1);
        assert_eq!(out["total_lines"], 3);
        assert_eq!(out["returned_lines"], 1);
        assert_eq!(out["end_line"], 2);
        assert!(out["has_more"].as_bool().unwrap());
        assert_eq!(out["next_start_line"], 3);
    }

    #[test]
    fn numbered_format_preserves_cursor() {
        let mut f = NamedTempFile::new().unwrap();
        writeln!(f, "one").unwrap();
        writeln!(f, "two").unwrap();
        write!(f, "three").unwrap();
        let r = read(f.path(), Some(2), Some(1));
        let out = success_output(&r, true);
        assert_eq!(out["text"], "2 | two");
        assert_eq!(out["format"], "numbered");
        // cursors unchanged by numbering
        assert_eq!(out["returned_lines"], 1);
        assert_eq!(out["end_line"], 2);
        assert_eq!(out["next_start_line"], 3);
    }

    #[test]
    fn empty_content_returned_lines_zero() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "").unwrap();
        let r = read(f.path(), Some(1), Some(5));
        let out = success_output(&r, false);
        assert_eq!(out["text"], "");
        assert_eq!(out["returned_lines"], 0);
        assert!(out["end_line"].is_null());
        assert!(!out["has_more"].as_bool().unwrap());
        assert!(out["next_start_line"].is_null());
    }

    #[test]
    fn no_unknown_fields_in_success() {
        let mut f = NamedTempFile::new().unwrap();
        write!(f, "x").unwrap();
        let r = read(f.path(), None, None);
        let out = success_output(&r, false);
        let keys: Vec<&str> = out
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected = [
            "text",
            "format",
            "sha256",
            "start_line",
            "limit",
            "total_lines",
            "returned_lines",
            "end_line",
            "has_more",
            "next_start_line",
        ];
        let mut exp: Vec<&str> = expected.to_vec();
        exp.sort();
        let mut got = keys.clone();
        got.sort();
        assert_eq!(got, exp);
    }

    #[test]
    fn reason_codes_are_stable_strings() {
        assert_eq!(ReadFileReason::InvalidPath.as_str(), "invalid_path");
        assert_eq!(ReadFileReason::RangeTooLarge.as_str(), "range_too_large");
        assert_eq!(ReadFileReason::InvalidUtf8.as_str(), "invalid_utf8");
    }
}
