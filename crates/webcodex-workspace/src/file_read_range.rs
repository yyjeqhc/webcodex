//! Shared, bounded, streaming UTF-8 range reader for `read_file`.
//!
//! Both the local ToolRuntime and the agent Runner call into this module so a
//! single implementation owns line-counting, full-file SHA-256, empty-line
//! semantics, range cursors, and content-budget enforcement. Neither caller
//! retains the full file body for a range request: this performs one sequential
//! scan, streams the SHA-256 digest and line count, and keeps only the selected
//! range text.
//!
//! Text semantics (shared by local and agent reads):
//! * the last line need not end with a newline;
//! * an empty file has `total_lines == 0`;
//! * a file containing exactly one `\n` has `total_lines == 1`;
//! * each `\n` starts a new line, so a trailing `\n` does not add an extra line;
//! * SHA-256 is the digest of the complete raw file bytes;
//! * returned text joins selected lines with `\n` and never invents a trailing
//!   newline;
//! * non-UTF-8 files fail closed — no partial text is returned.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Maximum raw byte size of the selected range text. A range whose text would
/// exceed this budget fails with `ReadRangeError::RangeTooLarge` rather than
/// being silently truncated. Callers must independently validate their complete
/// serialized envelope because JSON escaping can expand this raw content.
pub const MAX_RANGE_CONTENT_BYTES: usize = 192 * 1024;

/// Hard ceiling for the final serialized model output. The shared reader does
/// not serialize, but it exposes this so the Runner can preflight its complete
/// v1 envelope and ToolRuntime can re-check the final numbered/escaped output.
pub const MAX_SERIALIZED_OUTPUT_BYTES: usize =
    webcodex_core::runtime_contract::FILE_READ_MAX_SERIALIZED_OUTPUT_BYTES;

const RAW_READ_BUFFER_BYTES: usize = 64 * 1024;

/// Effective request range after applying the shared `read_file` defaults and
/// clamps. `start_line` defaults to 1 (min 1); `limit` defaults to 2000 and is
/// clamped to `1..=2000`. Both local and agent reads use these same rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveRange {
    pub start_line: usize,
    pub limit: usize,
}

impl EffectiveRange {
    /// Normalize raw optional request parameters into the shared effective
    /// range. `start_line` of `0` or `None` becomes `1`; `limit` of `None`
    /// becomes `2000`, while `Some(0)` clamps to `1`.
    pub fn new(start_line: Option<usize>, limit: Option<usize>) -> Self {
        let start_line = start_line.unwrap_or(1).max(1);
        let limit = limit
            .unwrap_or(webcodex_core::runtime_contract::FILE_READ_DEFAULT_LIMIT)
            .clamp(1, webcodex_core::runtime_contract::FILE_READ_MAX_LIMIT);
        Self { start_line, limit }
    }

    /// 1-based inclusive last line of the requested window when the file is
    /// unbounded. Callers pass this as `end_line` to the streaming reader.
    pub fn end_line(&self) -> usize {
        self.start_line.saturating_add(self.limit).saturating_sub(1)
    }
}

/// Stable, schema-backed reason codes for `read_file` failures. The shared
/// reader maps all OS/IO errors into these codes; callers must never surface
/// absolute paths, raw OS error text, or runner stdout/stderr to the model.
/// The agent-only codes (`AgentUnavailable`, `Timeout`, `MalformedAgentResponse`)
/// are produced by the ToolRuntime agent path, not the shared reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadFileReason {
    InvalidPath,
    SensitivePath,
    NotFound,
    NotFile,
    PermissionDenied,
    InvalidUtf8,
    RangeTooLarge,
    AgentUnavailable,
    Timeout,
    MalformedAgentResponse,
    IoError,
}

impl ReadFileReason {
    /// As a stable lowercase string for structured error payloads.
    pub fn as_str(&self) -> &'static str {
        match self {
            ReadFileReason::InvalidPath => "invalid_path",
            ReadFileReason::SensitivePath => "sensitive_path",
            ReadFileReason::NotFound => "not_found",
            ReadFileReason::NotFile => "not_file",
            ReadFileReason::PermissionDenied => "permission_denied",
            ReadFileReason::InvalidUtf8 => "invalid_utf8",
            ReadFileReason::RangeTooLarge => "range_too_large",
            ReadFileReason::AgentUnavailable => "agent_unavailable",
            ReadFileReason::Timeout => "timeout",
            ReadFileReason::MalformedAgentResponse => "malformed_agent_response",
            ReadFileReason::IoError => "io_error",
        }
    }

    /// Parse a formal stable reason code emitted by a current Runner.
    pub fn from_code(value: &str) -> Option<Self> {
        Some(match value {
            "invalid_path" => ReadFileReason::InvalidPath,
            "sensitive_path" => ReadFileReason::SensitivePath,
            "not_found" => ReadFileReason::NotFound,
            "not_file" => ReadFileReason::NotFile,
            "permission_denied" => ReadFileReason::PermissionDenied,
            "invalid_utf8" => ReadFileReason::InvalidUtf8,
            "range_too_large" => ReadFileReason::RangeTooLarge,
            "agent_unavailable" => ReadFileReason::AgentUnavailable,
            "timeout" => ReadFileReason::Timeout,
            "malformed_agent_response" => ReadFileReason::MalformedAgentResponse,
            "io_error" => ReadFileReason::IoError,
            _ => return None,
        })
    }
}

impl std::fmt::Display for ReadFileReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Failure surfaced by the shared reader. Carries only a stable reason code;
/// the path/OS text never leaves this module through the error type.
#[derive(Debug)]
pub struct ReadFileError {
    pub reason: ReadFileReason,
}

impl ReadFileError {
    pub fn new(reason: ReadFileReason) -> Self {
        Self { reason }
    }
}

impl std::fmt::Display for ReadFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "read_file_failed: {}", self.reason)
    }
}

impl std::error::Error for ReadFileError {}

/// Map an IO error kind to a stable reason code for file reads. `NotFound` and
/// `PermissionDenied` are exact; everything else collapses to `io_error` so no
/// raw OS message is ever required downstream.
fn io_error_to_reason(error: &std::io::Error) -> ReadFileReason {
    match error.kind() {
        std::io::ErrorKind::NotFound => ReadFileReason::NotFound,
        std::io::ErrorKind::PermissionDenied => ReadFileReason::PermissionDenied,
        std::io::ErrorKind::InvalidData => ReadFileReason::InvalidUtf8,
        _ => ReadFileReason::IoError,
    }
}

/// The successful result of a bounded range read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadRange {
    /// Selected range text, lines joined with `\n`, no invented trailing
    /// newline. Empty both for an empty file and for a single selected blank
    /// line — disambiguate with `returned_lines`.
    pub content: String,
    /// Full-file SHA-256 as 64 lowercase hex digits.
    pub sha256: String,
    /// Complete file line count.
    pub total_lines: usize,
    /// Effective 1-based start line applied.
    pub start_line: usize,
    /// Effective line limit applied.
    pub limit: usize,
    /// Number of original file lines actually returned in `content`.
    pub returned_lines: usize,
    /// `start_line + returned_lines - 1` when `returned_lines > 0`, else null.
    pub end_line: Option<usize>,
    /// True only when file lines remain after the returned range.
    pub has_more: bool,
    /// `end_line + 1` when `has_more`, else null.
    pub next_start_line: Option<usize>,
}

impl FileReadRange {
    fn new(content: String, sha256: String, total_lines: usize, range: EffectiveRange) -> Self {
        let returned_lines = count_returned_lines(&content, total_lines, range.start_line);
        let end_line = if returned_lines > 0 {
            Some(range.start_line + returned_lines - 1)
        } else {
            None
        };
        let has_more = end_line.is_some_and(|end| end < total_lines);
        let next_start_line = if has_more {
            end_line.map(|end| end + 1)
        } else {
            None
        };
        Self {
            content,
            sha256,
            total_lines,
            start_line: range.start_line,
            limit: range.limit,
            returned_lines,
            end_line,
            has_more,
            next_start_line,
        }
    }
}

/// Count how many original file lines the selected `content` represents.
///
/// A single blank selected line yields `content == ""` but `returned_lines ==
/// 1`, so this must be derived from the range and total line count rather than
/// from `content.lines().count()` (which would lose empty selections). The
/// selected window is the intersection of `[start_line, start_line+limit)` with
/// `[1, total_lines]`.
fn count_returned_lines(content: &str, total_lines: usize, start_line: usize) -> usize {
    if start_line > total_lines || total_lines == 0 {
        return 0;
    }
    // The selected window length is min(limit, total - start + 1), but the
    // streaming reader may return fewer than the window when the content budget
    // truncates mid-range. When truncation happens the reader fails instead of
    // returning a partial range, so `content` always carries the full window.
    // Derive the count from the joined content to stay exact for the empty-line
    // case: a non-empty window that is a single blank line has one segment.
    if content.is_empty() {
        // Disambiguate the single blank line from the empty file / overflow case
        // using the window: a window that starts inside the file and has length
        // 1 whose only line is blank still returns one line.
        if start_line <= total_lines {
            1
        } else {
            0
        }
    } else {
        content.split('\n').count()
    }
}

#[derive(Debug, Default)]
struct Utf8Validator {
    carry: [u8; 4],
    carry_len: usize,
}

impl Utf8Validator {
    fn validate_chunk(&mut self, bytes: &[u8]) -> Result<(), ReadFileError> {
        let mut offset = 0;
        if self.carry_len > 0 {
            let expected = utf8_sequence_width(self.carry[0])
                .ok_or_else(|| ReadFileError::new(ReadFileReason::InvalidUtf8))?;
            let take = (expected - self.carry_len).min(bytes.len());
            self.carry[self.carry_len..self.carry_len + take].copy_from_slice(&bytes[..take]);
            self.carry_len += take;
            offset += take;
            if self.carry_len < expected {
                return Ok(());
            }
            std::str::from_utf8(&self.carry[..expected])
                .map_err(|_| ReadFileError::new(ReadFileReason::InvalidUtf8))?;
            self.carry_len = 0;
        }

        let remaining = &bytes[offset..];
        match std::str::from_utf8(remaining) {
            Ok(_) => Ok(()),
            Err(error) if error.error_len().is_some() => {
                Err(ReadFileError::new(ReadFileReason::InvalidUtf8))
            }
            Err(error) => {
                let tail = &remaining[error.valid_up_to()..];
                if tail.is_empty() || tail.len() > 3 {
                    return Err(ReadFileError::new(ReadFileReason::InvalidUtf8));
                }
                self.carry[..tail.len()].copy_from_slice(tail);
                self.carry_len = tail.len();
                Ok(())
            }
        }
    }

    fn finish(&self) -> Result<(), ReadFileError> {
        if self.carry_len == 0 {
            Ok(())
        } else {
            Err(ReadFileError::new(ReadFileReason::InvalidUtf8))
        }
    }
}

fn utf8_sequence_width(first: u8) -> Option<usize> {
    match first {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

/// Fixed-memory scanner that hashes and validates every raw byte while keeping
/// only the selected `[start_line, end_line]` window. A range overflow clears
/// the partial selection and continues scanning so later invalid UTF-8 cannot
/// be hidden by an early content-budget failure.
struct RangeScan {
    content: Vec<u8>,
    sha256: String,
    total_lines: usize,
    wrote_any_line: bool,
    current_line_has_bytes: bool,
    selected_line_started: bool,
    pending_selected_cr: bool,
    range_too_large: bool,
}

impl RangeScan {
    fn new(content_budget: usize) -> Self {
        Self {
            content: Vec::with_capacity(content_budget),
            sha256: String::new(),
            total_lines: 0,
            wrote_any_line: false,
            current_line_has_bytes: false,
            selected_line_started: false,
            pending_selected_cr: false,
            range_too_large: false,
        }
    }

    fn current_line_selected(&self, start_line: usize, end_line: usize) -> bool {
        let line = self.total_lines.saturating_add(1);
        line >= start_line && line <= end_line
    }

    fn push_selected_byte(&mut self, byte: u8, content_budget: usize) {
        if self.range_too_large {
            return;
        }
        if self.content.len() >= content_budget {
            self.content.clear();
            self.range_too_large = true;
            return;
        }
        self.content.push(byte);
    }

    fn start_selected_line(&mut self, content_budget: usize) {
        if self.selected_line_started {
            return;
        }
        if self.wrote_any_line {
            self.push_selected_byte(b'\n', content_budget);
        }
        self.wrote_any_line = true;
        self.selected_line_started = true;
    }

    fn flush_pending_cr(&mut self, content_budget: usize) {
        if self.pending_selected_cr {
            self.push_selected_byte(b'\r', content_budget);
            self.pending_selected_cr = false;
        }
    }

    fn scan_chunk(
        &mut self,
        bytes: &[u8],
        start_line: usize,
        end_line: usize,
        content_budget: usize,
    ) {
        for &byte in bytes {
            let selected = self.current_line_selected(start_line, end_line);
            if byte == b'\n' {
                if selected {
                    self.start_selected_line(content_budget);
                    self.pending_selected_cr = false;
                }
                self.total_lines = self.total_lines.saturating_add(1);
                self.current_line_has_bytes = false;
                self.selected_line_started = false;
                self.pending_selected_cr = false;
                continue;
            }

            self.current_line_has_bytes = true;
            if !selected {
                continue;
            }
            self.start_selected_line(content_budget);
            if byte == b'\r' {
                self.flush_pending_cr(content_budget);
                self.pending_selected_cr = true;
            } else {
                self.flush_pending_cr(content_budget);
                self.push_selected_byte(byte, content_budget);
            }
        }
    }

    fn run(
        mut self,
        mut reader: impl Read,
        start_line: usize,
        end_line: usize,
        content_budget: usize,
    ) -> Result<Self, ReadFileError> {
        let mut hasher = Sha256::new();
        let mut utf8 = Utf8Validator::default();
        let mut raw = [0u8; RAW_READ_BUFFER_BYTES];
        loop {
            let bytes_read = reader
                .read(&mut raw)
                .map_err(|e| ReadFileError::new(io_error_to_reason(&e)))?;
            if bytes_read == 0 {
                break;
            }
            let chunk = &raw[..bytes_read];
            hasher.update(chunk);
            utf8.validate_chunk(chunk)?;
            self.scan_chunk(chunk, start_line, end_line, content_budget);
        }
        utf8.finish()?;
        if self.current_line_has_bytes {
            self.pending_selected_cr = false;
            self.total_lines = self.total_lines.saturating_add(1);
        }
        self.sha256 = format!("{:x}", hasher.finalize());
        if self.range_too_large {
            return Err(ReadFileError::new(ReadFileReason::RangeTooLarge));
        }
        Ok(self)
    }
}

/// Read a bounded UTF-8 text range from `path`. Performs one sequential scan;
/// the full file body is never retained. Returns the shared result on success
/// or a stable reason-coded error otherwise. Enforces the shared content
/// budget [`MAX_RANGE_CONTENT_BYTES`].
///
/// `path` is only used for opening; no absolute path is stored on the result or
/// in the error. Callers validate project-relative path semantics and secret
/// policies before reaching this function.
pub fn read_range(path: &Path, range: EffectiveRange) -> Result<FileReadRange, ReadFileError> {
    read_range_with_budget(path, range, MAX_RANGE_CONTENT_BYTES)
}

/// Like [`read_range`] but with a caller-supplied content budget, clamped to at
/// most [`MAX_RANGE_CONTENT_BYTES`]. The agent runner uses this so a tighter
/// transport cap (`max_bytes`) can still bound a range without exceeding the
/// shared model output limit.
pub fn read_range_with_budget(
    path: &Path,
    range: EffectiveRange,
    content_budget: usize,
) -> Result<FileReadRange, ReadFileError> {
    let end_line = range.end_line();
    let file = File::open(path).map_err(|e| ReadFileError::new(io_error_to_reason(&e)))?;
    let budget = clamp_budget(content_budget);
    let scan = RangeScan::new(budget).run(file, range.start_line, end_line, budget)?;
    let content = String::from_utf8(scan.content)
        .map_err(|_| ReadFileError::new(ReadFileReason::InvalidUtf8))?;

    Ok(FileReadRange::new(
        content,
        scan.sha256,
        scan.total_lines,
        range,
    ))
}

/// Read a bounded UTF-8 text range from an already-open reader. Used by callers
/// that have already opened and validated the file (for example, the agent
/// runner after a project-boundary canonicalize). Same streaming semantics and
/// budget enforcement as [`read_range`].
pub fn read_range_from(
    reader: impl Read,
    range: EffectiveRange,
) -> Result<FileReadRange, ReadFileError> {
    read_range_from_with_budget(reader, range, MAX_RANGE_CONTENT_BYTES)
}

/// Like [`read_range_from`] but with a caller-supplied content budget, clamped
/// to at most [`MAX_RANGE_CONTENT_BYTES`].
pub fn read_range_from_with_budget(
    reader: impl Read,
    range: EffectiveRange,
    content_budget: usize,
) -> Result<FileReadRange, ReadFileError> {
    let end_line = range.end_line();
    let budget = clamp_budget(content_budget);
    let scan = RangeScan::new(budget).run(reader, range.start_line, end_line, budget)?;
    let content = String::from_utf8(scan.content)
        .map_err(|_| ReadFileError::new(ReadFileReason::InvalidUtf8))?;
    Ok(FileReadRange::new(
        content,
        scan.sha256,
        scan.total_lines,
        range,
    ))
}

fn clamp_budget(content_budget: usize) -> usize {
    content_budget.min(MAX_RANGE_CONTENT_BYTES)
}

/// Validate a SHA-256 hex string: exactly 64 lowercase hex digits.
pub fn is_valid_sha256_hex(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use tempfile::NamedTempFile;

    struct RepeatedByteReader {
        byte: u8,
        remaining: usize,
    }

    impl RepeatedByteReader {
        fn new(byte: u8, remaining: usize) -> Self {
            Self { byte, remaining }
        }
    }

    impl Read for RepeatedByteReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let count = self.remaining.min(buffer.len());
            buffer[..count].fill(self.byte);
            self.remaining -= count;
            Ok(count)
        }
    }

    struct ChunkedReader {
        chunks: VecDeque<Vec<u8>>,
    }

    impl ChunkedReader {
        fn new(chunks: impl IntoIterator<Item = Vec<u8>>) -> Self {
            Self {
                chunks: chunks.into_iter().collect(),
            }
        }
    }

    impl Read for ChunkedReader {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            let Some(chunk) = self.chunks.pop_front() else {
                return Ok(0);
            };
            assert!(chunk.len() <= buffer.len());
            buffer[..chunk.len()].copy_from_slice(&chunk);
            Ok(chunk.len())
        }
    }

    fn repeated_sha(byte: u8, len: usize) -> String {
        let mut hasher = Sha256::new();
        let chunk = [byte; RAW_READ_BUFFER_BYTES];
        let mut remaining = len;
        while remaining > 0 {
            let count = remaining.min(chunk.len());
            hasher.update(&chunk[..count]);
            remaining -= count;
        }
        format!("{:x}", hasher.finalize())
    }

    fn write_tmp(content: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        f.flush().unwrap();
        f
    }

    fn full_sha(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        format!("{:x}", hasher.finalize())
    }

    #[test]
    fn effective_range_defaults_and_clamps() {
        assert_eq!(
            EffectiveRange::new(None, None),
            EffectiveRange {
                start_line: 1,
                limit: 2000
            }
        );
        assert_eq!(
            EffectiveRange::new(Some(0), Some(0)),
            EffectiveRange {
                start_line: 1,
                limit: 1
            }
        );
        assert_eq!(
            EffectiveRange::new(Some(7), Some(5000)),
            EffectiveRange {
                start_line: 7,
                limit: 2000
            }
        );
        assert_eq!(EffectiveRange::new(Some(7), Some(5000)).end_line(), 2006);
    }

    #[test]
    fn ordinary_middle_range() {
        let body = (1..=100)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(21), Some(20))).unwrap();
        assert_eq!(r.returned_lines, 20);
        assert_eq!(r.end_line, Some(40));
        assert!(r.has_more);
        assert_eq!(r.next_start_line, Some(41));
        assert_eq!(
            r.content,
            (21..=40)
                .map(|i| format!("line-{i}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(r.sha256, full_sha(body.as_bytes()));
        assert_eq!(r.total_lines, 100);
        assert_eq!(r.start_line, 21);
        assert_eq!(r.limit, 20);
    }

    #[test]
    fn exactly_eof_has_no_more() {
        let body = (1..=40)
            .map(|i| format!("line-{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(21), Some(20))).unwrap();
        assert_eq!(r.returned_lines, 20);
        assert_eq!(r.end_line, Some(40));
        assert!(!r.has_more);
        assert_eq!(r.next_start_line, None);
    }

    #[test]
    fn start_line_past_eof() {
        let body = "a\nb\nc";
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(10), Some(5))).unwrap();
        assert_eq!(r.content, "");
        assert_eq!(r.returned_lines, 0);
        assert_eq!(r.end_line, None);
        assert!(!r.has_more);
        assert_eq!(r.next_start_line, None);
        assert_eq!(r.total_lines, 3);
    }

    #[test]
    fn empty_file() {
        let f = write_tmp(b"");
        let r = read_range(f.path(), EffectiveRange::new(None, None)).unwrap();
        assert_eq!(r.content, "");
        assert_eq!(r.returned_lines, 0);
        assert_eq!(r.total_lines, 0);
        assert_eq!(r.end_line, None);
        assert!(!r.has_more);
        assert_eq!(r.next_start_line, None);
        assert_eq!(r.sha256, full_sha(b""));
    }

    #[test]
    fn single_newline_file_is_one_blank_line() {
        let f = write_tmp(b"\n");
        let r = read_range(f.path(), EffectiveRange::new(Some(1), Some(5))).unwrap();
        assert_eq!(r.total_lines, 1);
        assert_eq!(r.content, "");
        assert_eq!(r.returned_lines, 1);
        assert_eq!(r.end_line, Some(1));
        assert!(!r.has_more);
        assert_eq!(r.next_start_line, None);
        assert_eq!(r.sha256, full_sha(b"\n"));
    }

    #[test]
    fn leading_blank_line_preserved() {
        let body = "\nsecond\nthird";
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(1), Some(2))).unwrap();
        assert_eq!(r.content, "\nsecond");
        assert_eq!(r.returned_lines, 2);
        assert_eq!(r.end_line, Some(2));
        assert!(r.has_more);
        assert_eq!(r.next_start_line, Some(3));
        assert_eq!(r.total_lines, 3);
    }

    #[test]
    fn middle_blank_line_preserved() {
        let body = "a\n\nb";
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(2), Some(1))).unwrap();
        assert_eq!(r.content, "");
        assert_eq!(r.returned_lines, 1);
        assert_eq!(r.end_line, Some(2));
        assert!(r.has_more);
        assert_eq!(r.next_start_line, Some(3));
    }

    #[test]
    fn no_trailing_newline_last_line() {
        let body = "one\ntwo";
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(2), Some(5))).unwrap();
        assert_eq!(r.content, "two");
        assert_eq!(r.returned_lines, 1);
        assert_eq!(r.end_line, Some(2));
        assert!(!r.has_more);
        assert_eq!(r.total_lines, 2);
        assert_eq!(r.sha256, full_sha(body.as_bytes()));
    }

    #[test]
    fn large_file_small_range_is_small_output() {
        let line = "x".repeat(64);
        let body = (0..300_000)
            .map(|_| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let f = write_tmp(body.as_bytes());
        let r = read_range(f.path(), EffectiveRange::new(Some(150_000), Some(3))).unwrap();
        assert_eq!(r.returned_lines, 3);
        assert_eq!(
            r.content,
            [line.as_str(), line.as_str(), line.as_str()].join("\n")
        );
        assert_eq!(r.total_lines, 300_000);
        assert!(r.has_more);
        assert_eq!(r.next_start_line, Some(150_003));
        assert_eq!(r.sha256, full_sha(body.as_bytes()));
        assert!(r.content.len() < 256);
    }

    #[test]
    fn range_exceeding_content_budget_fails() {
        let line = "x".repeat(1024);
        let body = (0..512)
            .map(|_| line.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let f = write_tmp(body.as_bytes());
        let err = read_range(f.path(), EffectiveRange::new(Some(1), Some(2000))).unwrap_err();
        assert_eq!(err.reason, ReadFileReason::RangeTooLarge);
    }

    #[test]
    fn generated_overlong_unselected_line_uses_bounded_memory() {
        let len = 65 * 1024 * 1024;
        let result = read_range_from(
            RepeatedByteReader::new(b'x', len),
            EffectiveRange::new(Some(2), Some(1)),
        )
        .unwrap();
        assert_eq!(result.content, "");
        assert_eq!(result.total_lines, 1);
        assert_eq!(result.returned_lines, 0);
        assert_eq!(result.sha256, repeated_sha(b'x', len));
    }

    #[test]
    fn generated_overlong_selected_line_fails_without_partial_return() {
        let err = read_range_from(
            RepeatedByteReader::new(b'x', 65 * 1024 * 1024),
            EffectiveRange::new(Some(1), Some(1)),
        )
        .unwrap_err();
        assert_eq!(err.reason, ReadFileReason::RangeTooLarge);
    }

    #[test]
    fn utf8_code_point_may_cross_raw_chunks() {
        let result = read_range_from(
            ChunkedReader::new([
                b"first\n\xe2".to_vec(),
                vec![0x82],
                vec![0xAC, b'\n', b'l', b'a', b's', b't'],
            ]),
            EffectiveRange::new(Some(2), Some(1)),
        )
        .unwrap();
        assert_eq!(result.content, "€");
        assert_eq!(result.total_lines, 3);
    }

    #[test]
    fn invalid_utf8_crossing_chunks_fails_closed() {
        let err = read_range_from(
            ChunkedReader::new([b"ok\n\xe2".to_vec(), vec![0x28, 0xA1]]),
            EffectiveRange::new(Some(1), Some(1)),
        )
        .unwrap_err();
        assert_eq!(err.reason, ReadFileReason::InvalidUtf8);
    }

    #[test]
    fn incomplete_utf8_at_eof_fails_closed() {
        let err = read_range_from(
            ChunkedReader::new([b"ok\n".to_vec(), vec![0xF0, 0x9F]]),
            EffectiveRange::new(Some(1), Some(1)),
        )
        .unwrap_err();
        assert_eq!(err.reason, ReadFileReason::InvalidUtf8);
    }

    #[test]
    fn invalid_utf8_after_requested_range_still_fails() {
        let err = read_range_from(
            ChunkedReader::new([b"selected\nignored\n".to_vec(), vec![0xFF]]),
            EffectiveRange::new(Some(1), Some(1)),
        )
        .unwrap_err();
        assert_eq!(err.reason, ReadFileReason::InvalidUtf8);
    }

    #[test]
    fn crlf_and_terminal_cr_are_normalized_without_line_buffers() {
        let result = read_range_from(
            ChunkedReader::new([b"one\r".to_vec(), b"\ntwo\r".to_vec()]),
            EffectiveRange::new(Some(1), Some(2)),
        )
        .unwrap();
        assert_eq!(result.content, "one\ntwo");
        assert_eq!(result.total_lines, 2);
    }

    #[test]
    fn non_utf8_file_fails_closed() {
        let f = write_tmp(&[0xFF, 0xFE, b'\n', 0x80]);
        let err = read_range(f.path(), EffectiveRange::new(Some(1), Some(5))).unwrap_err();
        assert_eq!(err.reason, ReadFileReason::InvalidUtf8);
    }

    #[test]
    fn not_found_maps_to_reason() {
        let err = read_range(
            Path::new("/nonexistent/webcodex-missing-file"),
            EffectiveRange::new(None, None),
        )
        .unwrap_err();
        assert_eq!(err.reason, ReadFileReason::NotFound);
    }

    #[test]
    fn sha256_validator_rejects_uppercase_and_short() {
        assert!(is_valid_sha256_hex(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        ));
        assert!(!is_valid_sha256_hex("ABC"));
        assert!(!is_valid_sha256_hex(
            "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
        ));
    }
}
