//! Bounded pipe tails, complete-stream encoding evidence, and BOM alignment.

use super::*;

#[derive(Debug)]
pub(super) struct BoundedPipeTail {
    pub(super) bytes: Vec<u8>,
    pub(super) raw_truncated: bool,
    pub(super) encoding: CapturedOutputEncoding,
}

impl BoundedPipeTail {
    pub(super) fn normalize_with_truncation(&self, max_output_bytes: usize) -> (String, bool) {
        normalize_captured_output_text_with_truncation(
            &self.bytes,
            self.raw_truncated,
            max_output_bytes,
            OutputTextSource::LocalProcess,
            self.encoding,
        )
    }

    #[cfg(test)]
    pub(super) fn normalize_as_windows_for_test(&self, max_output_bytes: usize) -> String {
        crate::webcodex_runner::output_text::normalize_captured_output_text_as_windows_for_test(
            &self.bytes,
            self.raw_truncated,
            max_output_bytes,
            self.encoding,
        )
    }
}

#[derive(Debug)]
pub(super) struct IncrementalUtf8Validator {
    pub(super) valid_so_far: bool,
    pub(super) pending: Vec<u8>,
}

pub(super) fn incomplete_utf8_sequence_len(first: u8) -> Option<usize> {
    match first {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

impl IncrementalUtf8Validator {
    pub(super) fn new() -> Self {
        Self {
            valid_so_far: true,
            pending: Vec::with_capacity(3),
        }
    }

    pub(super) fn push(&mut self, mut bytes: &[u8]) {
        if !self.valid_so_far {
            return;
        }
        if !self.pending.is_empty() {
            let sequence_len = incomplete_utf8_sequence_len(self.pending[0])
                .expect("pending UTF-8 starts with a valid multibyte lead byte");
            let needed = sequence_len - self.pending.len();
            let take = needed.min(bytes.len());
            if take < needed {
                self.pending.extend_from_slice(&bytes[..take]);
                if std::str::from_utf8(&self.pending)
                    .is_err_and(|error| error.error_len().is_some())
                {
                    self.pending.clear();
                    self.valid_so_far = false;
                    return;
                }
                debug_assert!(self.pending.len() <= 3);
                return;
            }
            let mut sequence = [0_u8; 4];
            sequence[..self.pending.len()].copy_from_slice(&self.pending);
            sequence[self.pending.len()..sequence_len].copy_from_slice(&bytes[..take]);
            self.pending.clear();
            if std::str::from_utf8(&sequence[..sequence_len]).is_err() {
                self.valid_so_far = false;
                return;
            }
            bytes = &bytes[take..];
        }
        match std::str::from_utf8(bytes) {
            Ok(_) => {}
            Err(error) if error.error_len().is_some() => {
                self.valid_so_far = false;
            }
            Err(error) => {
                self.pending
                    .extend_from_slice(&bytes[error.valid_up_to()..]);
                debug_assert!(self.pending.len() <= 3);
            }
        }
    }

    pub(super) fn finish(&self) -> FullStreamUtf8Validity {
        if self.valid_so_far && self.pending.is_empty() {
            FullStreamUtf8Validity::Valid
        } else {
            FullStreamUtf8Validity::Invalid
        }
    }
}

pub(super) fn read_bounded_pipe_tail(
    mut pipe: impl Read,
    max_bytes: usize,
    stream_name: &'static str,
) -> Result<BoundedPipeTail, String> {
    // Four extra raw bytes retain truncation evidence plus enough alignment
    // room for the largest UTF-8 scalar. The decoded result is independently
    // bounded after transcoding.
    let retained_limit = max_bytes
        .saturating_add(RAW_TAIL_CAPTURE_ALLOWANCE)
        .max(RAW_TAIL_CAPTURE_ALLOWANCE);
    let mut output = Vec::with_capacity(retained_limit.min(64 * 1024));
    let mut prefix = Vec::with_capacity(3);
    let mut utf8_validator = IncrementalUtf8Validator::new();
    let mut total_bytes = 0usize;
    let mut raw_truncated = false;
    let mut chunk = [0_u8; 8192];
    loop {
        let read = pipe
            .read(&mut chunk)
            .map_err(|error| format!("failed to read {stream_name}: {error}"))?;
        if read == 0 {
            let encoding = CapturedOutputEncoding {
                full_stream_utf8: utf8_validator.finish(),
                leading_bom: leading_bom(&prefix),
            };
            if raw_truncated {
                align_and_restore_bom(encoding, total_bytes, &mut output);
            }
            return Ok(BoundedPipeTail {
                bytes: output,
                raw_truncated,
                encoding,
            });
        }
        if prefix.len() < 3 {
            let prefix_bytes = (3 - prefix.len()).min(read);
            prefix.extend_from_slice(&chunk[..prefix_bytes]);
        }
        utf8_validator.push(&chunk[..read]);
        total_bytes = total_bytes.saturating_add(read);
        output.extend_from_slice(&chunk[..read]);
        if output.len() > retained_limit {
            let discard = output.len() - retained_limit;
            output.drain(..discard);
            raw_truncated = true;
        }
    }
}

pub(super) fn leading_bom(prefix: &[u8]) -> LeadingBom {
    if prefix.starts_with(&[0xEF, 0xBB, 0xBF]) {
        LeadingBom::Utf8
    } else if prefix.starts_with(&[0xFF, 0xFE]) {
        LeadingBom::Utf16Le
    } else if prefix.starts_with(&[0xFE, 0xFF]) {
        LeadingBom::Utf16Be
    } else {
        LeadingBom::None
    }
}

pub(super) fn align_and_restore_bom(
    encoding: CapturedOutputEncoding,
    total_bytes: usize,
    tail: &mut Vec<u8>,
) {
    match encoding.leading_bom {
        LeadingBom::Utf8 => restore_utf8_bom(tail),
        LeadingBom::Utf16Le => restore_utf16_bom(tail, total_bytes, true),
        LeadingBom::Utf16Be => restore_utf16_bom(tail, total_bytes, false),
        LeadingBom::None if encoding.full_stream_utf8 == FullStreamUtf8Validity::Valid => {
            align_valid_utf8_tail(tail);
        }
        LeadingBom::None => {}
    }
}

pub(super) fn align_valid_utf8_tail(tail: &mut Vec<u8>) {
    let discard = tail
        .iter()
        .take(3)
        .take_while(|byte| **byte & 0b1100_0000 == 0b1000_0000)
        .count();
    tail.drain(..discard);
}

pub(super) fn restore_utf8_bom(tail: &mut Vec<u8>) {
    let replace = 3.min(tail.len());
    tail.drain(..replace);
    align_valid_utf8_tail(tail);
    let mut with_bom = Vec::with_capacity(3 + tail.len());
    with_bom.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    with_bom.extend_from_slice(tail);
    *tail = with_bom;
}

pub(super) fn restore_utf16_bom(tail: &mut Vec<u8>, total_bytes: usize, little_endian: bool) {
    let mut tail_start = total_bytes.saturating_sub(tail.len());
    let alignment_discard = if tail_start < 2 {
        2 - tail_start
    } else {
        (tail_start - 2) % 2
    }
    .min(tail.len());
    if alignment_discard > 0 {
        tail.drain(..alignment_discard);
        tail_start = tail_start.saturating_add(alignment_discard);
    }
    debug_assert!(tail_start >= 2 || tail.is_empty());
    let replace = 2.min(tail.len());
    tail.drain(..replace);
    if tail.len() >= 2 {
        let first = if little_endian {
            u16::from_le_bytes([tail[0], tail[1]])
        } else {
            u16::from_be_bytes([tail[0], tail[1]])
        };
        if (0xDC00..=0xDFFF).contains(&first) {
            tail.drain(..2);
        }
    }
    let bom = if little_endian {
        [0xFF, 0xFE]
    } else {
        [0xFE, 0xFF]
    };
    let mut with_bom = Vec::with_capacity(2 + tail.len());
    with_bom.extend_from_slice(&bom);
    with_bom.extend_from_slice(tail);
    *tail = with_bom;
}
