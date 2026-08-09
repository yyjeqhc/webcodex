//! Presentation-only normalization for captured process output.
//!
//! Execution state, identity, retry, and lifecycle decisions do not belong in
//! this module. Callers provide already-captured bytes and choose whether they
//! came from a local process or a remote SSH byte stream.

const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF16LE_BOM: &[u8] = &[0xFF, 0xFE];
const UTF16BE_BOM: &[u8] = &[0xFE, 0xFF];
const LONG_TRUNCATION_MARKER: &str = "[output truncated]\n";
const SHORT_TRUNCATION_MARKER: &str = "[...]\n";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputTextSource {
    LocalProcess,
    RemoteSsh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecodePolicy {
    Utf8Lossy,
    WindowsLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FullStreamUtf8Validity {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeadingBom {
    None,
    Utf8,
    Utf16Le,
    Utf16Be,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CapturedOutputEncoding {
    pub(crate) full_stream_utf8: FullStreamUtf8Validity,
    pub(crate) leading_bom: LeadingBom,
}

impl OutputTextSource {
    fn policy(self) -> DecodePolicy {
        match self {
            Self::LocalProcess if cfg!(windows) => DecodePolicy::WindowsLocal,
            Self::LocalProcess | Self::RemoteSsh => DecodePolicy::Utf8Lossy,
        }
    }
}

/// Decode a complete retained byte buffer, normalize its presentation, and
/// enforce the final UTF-8 byte budget. `raw_truncated` records truncation that
/// occurred during byte capture even when transcoding makes the retained text
/// shorter than `max_output_bytes`.
pub(crate) fn normalize_output_text(
    bytes: &[u8],
    raw_truncated: bool,
    max_output_bytes: usize,
    source: OutputTextSource,
) -> String {
    normalize_output_text_with_policy(bytes, raw_truncated, max_output_bytes, source.policy())
}

pub(crate) fn normalize_captured_output_text(
    bytes: &[u8],
    raw_truncated: bool,
    max_output_bytes: usize,
    source: OutputTextSource,
    encoding: CapturedOutputEncoding,
) -> String {
    normalize_output_text_with_policy_and_encoding(
        bytes,
        raw_truncated,
        max_output_bytes,
        source.policy(),
        Some(encoding),
    )
}

fn normalize_output_text_with_policy(
    bytes: &[u8],
    raw_truncated: bool,
    max_output_bytes: usize,
    policy: DecodePolicy,
) -> String {
    normalize_output_text_with_policy_and_encoding(
        bytes,
        raw_truncated,
        max_output_bytes,
        policy,
        None,
    )
}

fn normalize_output_text_with_policy_and_encoding(
    bytes: &[u8],
    raw_truncated: bool,
    max_output_bytes: usize,
    policy: DecodePolicy,
    encoding: Option<CapturedOutputEncoding>,
) -> String {
    let decoded = match policy {
        DecodePolicy::Utf8Lossy => String::from_utf8_lossy(bytes).into_owned(),
        DecodePolicy::WindowsLocal => decode_windows_local(bytes, encoding),
    };
    let decoded = if policy == DecodePolicy::WindowsLocal {
        normalize_windows_line_endings(&decoded)
    } else {
        decoded
    };
    bound_presented_text(&decoded, max_output_bytes, raw_truncated)
}

/// Append Runner-generated text without allowing it to bypass the same final
/// UTF-8 output budget as captured child output.
pub(crate) fn append_bounded_text(target: &mut String, suffix: &str, max_output_bytes: usize) {
    if !target.is_empty() && !target.ends_with('\n') {
        target.push('\n');
    }
    target.push_str(suffix);
    *target = bound_presented_text(target, max_output_bytes, false);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowsWholeBufferEncoding {
    Utf8Bom,
    Utf8,
    Utf16Le,
    Utf16Be,
    Oem,
}

fn windows_whole_buffer_encoding(
    bytes: &[u8],
    captured: Option<CapturedOutputEncoding>,
) -> WindowsWholeBufferEncoding {
    if let Some(captured) = captured {
        return match (captured.leading_bom, captured.full_stream_utf8) {
            (LeadingBom::Utf8, _) => WindowsWholeBufferEncoding::Utf8Bom,
            (_, FullStreamUtf8Validity::Valid) => WindowsWholeBufferEncoding::Utf8,
            (LeadingBom::Utf16Le, FullStreamUtf8Validity::Invalid) => {
                WindowsWholeBufferEncoding::Utf16Le
            }
            (LeadingBom::Utf16Be, FullStreamUtf8Validity::Invalid) => {
                WindowsWholeBufferEncoding::Utf16Be
            }
            (LeadingBom::None, FullStreamUtf8Validity::Invalid) => WindowsWholeBufferEncoding::Oem,
        };
    }
    if bytes.starts_with(UTF8_BOM) {
        WindowsWholeBufferEncoding::Utf8Bom
    } else if std::str::from_utf8(bytes).is_ok() {
        WindowsWholeBufferEncoding::Utf8
    } else if bytes.starts_with(UTF16LE_BOM) {
        WindowsWholeBufferEncoding::Utf16Le
    } else if bytes.starts_with(UTF16BE_BOM) {
        WindowsWholeBufferEncoding::Utf16Be
    } else {
        WindowsWholeBufferEncoding::Oem
    }
}

fn decode_windows_local(bytes: &[u8], captured: Option<CapturedOutputEncoding>) -> String {
    match windows_whole_buffer_encoding(bytes, captured) {
        WindowsWholeBufferEncoding::Utf8Bom => {
            String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)).into_owned()
        }
        WindowsWholeBufferEncoding::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        WindowsWholeBufferEncoding::Utf16Le => {
            decode_utf16(bytes.strip_prefix(UTF16LE_BOM).unwrap_or(bytes), true)
        }
        WindowsWholeBufferEncoding::Utf16Be => {
            decode_utf16(bytes.strip_prefix(UTF16BE_BOM).unwrap_or(bytes), false)
        }
        WindowsWholeBufferEncoding::Oem => decode_legacy_bytes(bytes, current_oem_code_page()),
    }
}

#[cfg(test)]
pub(crate) fn normalize_captured_output_text_as_windows_for_test(
    bytes: &[u8],
    raw_truncated: bool,
    max_output_bytes: usize,
    encoding: CapturedOutputEncoding,
) -> String {
    normalize_output_text_with_policy_and_encoding(
        bytes,
        raw_truncated,
        max_output_bytes,
        DecodePolicy::WindowsLocal,
        Some(encoding),
    )
}

#[cfg(test)]
pub(crate) fn captured_windows_output_uses_oem_for_test(
    bytes: &[u8],
    encoding: CapturedOutputEncoding,
) -> bool {
    windows_whole_buffer_encoding(bytes, Some(encoding)) == WindowsWholeBufferEncoding::Oem
}

fn decode_utf16(bytes: &[u8], little_endian: bool) -> String {
    let mut units = Vec::with_capacity(bytes.len() / 2);
    let mut chunks = bytes.chunks_exact(2);
    for pair in &mut chunks {
        units.push(if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        });
    }
    let mut output = String::from_utf16_lossy(&units);
    if !chunks.remainder().is_empty() {
        output.push('\u{fffd}');
    }
    output
}

fn normalize_windows_line_endings(text: &str) -> String {
    if !text.as_bytes().windows(2).any(|pair| pair == b"\r\n") {
        return text.to_string();
    }
    text.replace("\r\n", "\n")
}

fn truncation_marker(max_bytes: usize) -> &'static str {
    if max_bytes >= LONG_TRUNCATION_MARKER.len() {
        LONG_TRUNCATION_MARKER
    } else if max_bytes >= SHORT_TRUNCATION_MARKER.len() {
        SHORT_TRUNCATION_MARKER
    } else {
        ""
    }
}

fn bound_presented_text(text: &str, max_bytes: usize, force_truncated: bool) -> String {
    if !force_truncated && text.len() <= max_bytes {
        return text.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }
    let marker = truncation_marker(max_bytes);
    let tail_budget = max_bytes.saturating_sub(marker.len());
    let mut start = text.len().saturating_sub(tail_budget);
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    if marker.is_empty() {
        if tail_budget == 0 {
            return String::new();
        }
        return text[start..].to_string();
    }
    let mut output = String::with_capacity(max_bytes);
    output.push_str(marker);
    output.push_str(&text[start..]);
    output
}

#[derive(Debug)]
enum StreamDecodeState {
    Start(Vec<u8>),
    WindowsUndecided(Vec<u8>),
    Utf8(Vec<u8>),
    Utf16 {
        little_endian: bool,
        pending_byte: Option<u8>,
        pending_high_surrogate: Option<u16>,
    },
    Legacy {
        code_page: u32,
        pending_lead: Option<u8>,
    },
}

/// Incremental decoder for one stdout or stderr stream.
///
/// The only retained undecoded state is a BOM prefix (at most two bytes), an
/// incomplete UTF-8 scalar (at most three bytes), one UTF-16 byte plus one high
/// surrogate, or one DBCS lead byte. A trailing CR is presentation state, not
/// an output buffer.
#[derive(Debug)]
pub(crate) struct OutputTextDecoder {
    policy: DecodePolicy,
    state: StreamDecodeState,
    pending_cr: bool,
}

impl OutputTextDecoder {
    pub(crate) fn new(source: OutputTextSource) -> Self {
        Self::with_policy(source.policy())
    }

    fn with_policy(policy: DecodePolicy) -> Self {
        let state = match policy {
            DecodePolicy::Utf8Lossy => StreamDecodeState::Utf8(Vec::with_capacity(3)),
            DecodePolicy::WindowsLocal => StreamDecodeState::Start(Vec::with_capacity(3)),
        };
        Self {
            policy,
            state,
            pending_cr: false,
        }
    }

    /// Decode the next bytes. `end_of_stream` flushes malformed or incomplete
    /// decoder state deterministically.
    pub(crate) fn push(&mut self, bytes: &[u8], end_of_stream: bool) -> String {
        let decoded = self.decode(bytes, end_of_stream);
        if self.policy == DecodePolicy::WindowsLocal {
            self.present_windows(decoded, end_of_stream)
        } else {
            decoded
        }
    }

    fn decode(&mut self, bytes: &[u8], end_of_stream: bool) -> String {
        let state = std::mem::replace(
            &mut self.state,
            StreamDecodeState::Utf8(Vec::with_capacity(3)),
        );
        let (next, output) = match state {
            StreamDecodeState::Start(pending) => decode_stream_start(pending, bytes, end_of_stream),
            StreamDecodeState::WindowsUndecided(pending) => {
                decode_windows_undecided(pending, bytes, end_of_stream)
            }
            StreamDecodeState::Utf8(pending) => {
                let (pending, output) = decode_utf8_stream(pending, bytes, end_of_stream);
                (StreamDecodeState::Utf8(pending), output)
            }
            StreamDecodeState::Utf16 {
                little_endian,
                pending_byte,
                pending_high_surrogate,
            } => decode_utf16_stream(
                little_endian,
                pending_byte,
                pending_high_surrogate,
                bytes,
                end_of_stream,
            ),
            StreamDecodeState::Legacy {
                code_page,
                pending_lead,
            } => decode_legacy_stream(code_page, pending_lead, bytes, end_of_stream),
        };
        self.state = next;
        output
    }

    fn present_windows(&mut self, decoded: String, end_of_stream: bool) -> String {
        let mut output = String::with_capacity(decoded.len() + usize::from(self.pending_cr));
        for character in decoded.chars() {
            if self.pending_cr {
                self.pending_cr = false;
                if character == '\n' {
                    output.push('\n');
                    continue;
                }
                output.push('\r');
            }
            if character == '\r' {
                self.pending_cr = true;
            } else {
                output.push(character);
            }
        }
        if end_of_stream && self.pending_cr {
            self.pending_cr = false;
            output.push('\r');
        }
        output
    }

    #[cfg(test)]
    fn windows_local_for_test() -> Self {
        Self::with_policy(DecodePolicy::WindowsLocal)
    }

    #[cfg(test)]
    fn pending_state_bytes(&self) -> usize {
        let decoder = match &self.state {
            StreamDecodeState::Start(bytes)
            | StreamDecodeState::WindowsUndecided(bytes)
            | StreamDecodeState::Utf8(bytes) => bytes.len(),
            StreamDecodeState::Utf16 {
                pending_byte,
                pending_high_surrogate,
                ..
            } => {
                usize::from(pending_byte.is_some())
                    + 2 * usize::from(pending_high_surrogate.is_some())
            }
            StreamDecodeState::Legacy { pending_lead, .. } => usize::from(pending_lead.is_some()),
        };
        decoder + usize::from(self.pending_cr)
    }
}

fn decode_stream_start(
    mut pending: Vec<u8>,
    bytes: &[u8],
    end_of_stream: bool,
) -> (StreamDecodeState, String) {
    pending.extend_from_slice(bytes);
    if pending.starts_with(UTF8_BOM) {
        let remainder = pending.split_off(UTF8_BOM.len());
        let (pending, output) =
            decode_utf8_stream(Vec::with_capacity(3), &remainder, end_of_stream);
        return (StreamDecodeState::Utf8(pending), output);
    }
    if pending.starts_with(UTF16LE_BOM) {
        let remainder = pending.split_off(UTF16LE_BOM.len());
        return decode_utf16_stream(true, None, None, &remainder, end_of_stream);
    }
    if pending.starts_with(UTF16BE_BOM) {
        let remainder = pending.split_off(UTF16BE_BOM.len());
        return decode_utf16_stream(false, None, None, &remainder, end_of_stream);
    }
    let could_be_bom = UTF8_BOM.starts_with(&pending)
        || UTF16LE_BOM.starts_with(&pending)
        || UTF16BE_BOM.starts_with(&pending);
    if could_be_bom && !end_of_stream {
        return (StreamDecodeState::Start(pending), String::new());
    }
    decode_windows_undecided(Vec::new(), &pending, end_of_stream)
}

fn utf8_sequence_len(first: u8) -> Option<usize> {
    match first {
        0xC2..=0xDF => Some(2),
        0xE0..=0xEF => Some(3),
        0xF0..=0xF4 => Some(4),
        _ => None,
    }
}

fn decode_windows_undecided(
    mut pending: Vec<u8>,
    bytes: &[u8],
    end_of_stream: bool,
) -> (StreamDecodeState, String) {
    pending.extend_from_slice(bytes);
    let mut output = String::new();
    let mut index = 0;
    while index < pending.len() {
        let first = pending[index];
        if first.is_ascii() {
            output.push(char::from(first));
            index += 1;
            continue;
        }
        let Some(sequence_len) = utf8_sequence_len(first) else {
            let code_page = current_oem_code_page();
            let (state, legacy) =
                decode_legacy_stream(code_page, None, &pending[index..], end_of_stream);
            output.push_str(&legacy);
            return (state, output);
        };
        if pending.len() - index < sequence_len {
            if !end_of_stream {
                return (
                    StreamDecodeState::WindowsUndecided(pending[index..].to_vec()),
                    output,
                );
            }
            let code_page = current_oem_code_page();
            let (state, legacy) = decode_legacy_stream(code_page, None, &pending[index..], true);
            output.push_str(&legacy);
            return (state, output);
        }
        if std::str::from_utf8(&pending[index..index + sequence_len]).is_ok() {
            let (utf8_pending, utf8) =
                decode_utf8_stream(Vec::with_capacity(3), &pending[index..], end_of_stream);
            output.push_str(&utf8);
            return (StreamDecodeState::Utf8(utf8_pending), output);
        }
        let code_page = current_oem_code_page();
        let (state, legacy) =
            decode_legacy_stream(code_page, None, &pending[index..], end_of_stream);
        output.push_str(&legacy);
        return (state, output);
    }
    (
        StreamDecodeState::WindowsUndecided(Vec::with_capacity(3)),
        output,
    )
}

fn decode_utf8_stream(
    mut pending: Vec<u8>,
    bytes: &[u8],
    end_of_stream: bool,
) -> (Vec<u8>, String) {
    pending.extend_from_slice(bytes);
    let mut output = String::new();
    loop {
        match std::str::from_utf8(&pending) {
            Ok(text) => {
                output.push_str(text);
                pending.clear();
                break;
            }
            Err(error) => {
                let valid_up_to = error.valid_up_to();
                if valid_up_to > 0 {
                    let valid = std::str::from_utf8(&pending[..valid_up_to])
                        .expect("valid_up_to identifies valid UTF-8");
                    output.push_str(valid);
                    pending.drain(..valid_up_to);
                }
                if let Some(error_len) = error.error_len() {
                    pending.drain(..error_len);
                    output.push('\u{fffd}');
                    continue;
                }
                if end_of_stream {
                    output.push_str(&String::from_utf8_lossy(&pending));
                    pending.clear();
                }
                break;
            }
        }
    }
    (pending, output)
}

fn decode_utf16_stream(
    little_endian: bool,
    mut pending_byte: Option<u8>,
    mut pending_high_surrogate: Option<u16>,
    bytes: &[u8],
    end_of_stream: bool,
) -> (StreamDecodeState, String) {
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() || pending_byte.is_some() {
        let first = match pending_byte.take() {
            Some(byte) => byte,
            None => {
                let byte = bytes[index];
                index += 1;
                byte
            }
        };
        if index >= bytes.len() {
            pending_byte = Some(first);
            break;
        }
        let second = bytes[index];
        index += 1;
        let unit = if little_endian {
            u16::from_le_bytes([first, second])
        } else {
            u16::from_be_bytes([first, second])
        };
        if let Some(high) = pending_high_surrogate.take() {
            if (0xDC00..=0xDFFF).contains(&unit) {
                let scalar = 0x1_0000 + (((high as u32 - 0xD800) << 10) | (unit as u32 - 0xDC00));
                output.push(char::from_u32(scalar).expect("surrogate pair is a valid scalar"));
                continue;
            }
            output.push('\u{fffd}');
        }
        match unit {
            0xD800..=0xDBFF => pending_high_surrogate = Some(unit),
            0xDC00..=0xDFFF => output.push('\u{fffd}'),
            _ => output.push(char::from_u32(unit as u32).expect("non-surrogate UTF-16 unit")),
        }
    }
    if end_of_stream {
        if pending_high_surrogate.take().is_some() {
            output.push('\u{fffd}');
        }
        if pending_byte.take().is_some() {
            output.push('\u{fffd}');
        }
    }
    (
        StreamDecodeState::Utf16 {
            little_endian,
            pending_byte,
            pending_high_surrogate,
        },
        output,
    )
}

fn decode_legacy_bytes(bytes: &[u8], code_page: u32) -> String {
    let (_, output) = decode_legacy_stream(code_page, None, bytes, true);
    output
}

fn decode_legacy_stream(
    code_page: u32,
    mut pending_lead: Option<u8>,
    bytes: &[u8],
    end_of_stream: bool,
) -> (StreamDecodeState, String) {
    let mut output = String::new();
    let mut index = 0;
    if let Some(lead) = pending_lead.take() {
        if let Some(trail) = bytes.first().copied() {
            output.push_str(&decode_oem_unit(code_page, &[lead, trail]));
            index = 1;
        } else if end_of_stream {
            output.push('\u{fffd}');
        } else {
            pending_lead = Some(lead);
        }
    }
    while index < bytes.len() {
        let byte = bytes[index];
        if is_dbcs_lead_byte(code_page, byte) {
            if let Some(trail) = bytes.get(index + 1).copied() {
                output.push_str(&decode_oem_unit(code_page, &[byte, trail]));
                index += 2;
            } else {
                if end_of_stream {
                    output.push('\u{fffd}');
                } else {
                    pending_lead = Some(byte);
                }
                index += 1;
            }
        } else {
            output.push_str(&decode_oem_unit(code_page, &[byte]));
            index += 1;
        }
    }
    (
        StreamDecodeState::Legacy {
            code_page,
            pending_lead,
        },
        output,
    )
}

#[cfg(windows)]
fn current_oem_code_page() -> u32 {
    // SAFETY: GetOEMCP has no arguments and returns process-global system
    // configuration. The value is captured once when a stream selects its
    // legacy fallback and remains stable for that stream.
    unsafe { windows_sys::Win32::Globalization::GetOEMCP() }
}

#[cfg(not(windows))]
fn current_oem_code_page() -> u32 {
    0
}

#[cfg(windows)]
fn is_dbcs_lead_byte(code_page: u32, byte: u8) -> bool {
    // SAFETY: both arguments are plain values; no pointers are involved.
    unsafe { windows_sys::Win32::Globalization::IsDBCSLeadByteEx(code_page, byte) != 0 }
}

#[cfg(not(windows))]
fn is_dbcs_lead_byte(_code_page: u32, _byte: u8) -> bool {
    false
}

#[cfg(windows)]
fn decode_oem_unit(code_page: u32, bytes: &[u8]) -> String {
    use windows_sys::Win32::Globalization::{MultiByteToWideChar, MB_ERR_INVALID_CHARS};

    fn convert(code_page: u32, flags: u32, bytes: &[u8]) -> Option<String> {
        let input_len = i32::try_from(bytes.len()).ok()?;
        // SAFETY: `bytes` is valid for `input_len`; the first call requests
        // the exact UTF-16 length and writes no output.
        let needed = unsafe {
            MultiByteToWideChar(
                code_page,
                flags,
                bytes.as_ptr(),
                input_len,
                std::ptr::null_mut(),
                0,
            )
        };
        if needed <= 0 {
            return None;
        }
        let mut wide = vec![0_u16; needed as usize];
        // SAFETY: `wide` has the capacity returned by the sizing call and all
        // pointers remain valid for the duration of the conversion.
        let written = unsafe {
            MultiByteToWideChar(
                code_page,
                flags,
                bytes.as_ptr(),
                input_len,
                wide.as_mut_ptr(),
                needed,
            )
        };
        (written == needed).then(|| String::from_utf16_lossy(&wide))
    }

    convert(code_page, MB_ERR_INVALID_CHARS, bytes)
        .or_else(|| convert(code_page, 0, bytes))
        .unwrap_or_else(|| "\u{fffd}".to_string())
}

#[cfg(not(windows))]
fn decode_oem_unit(_code_page: u32, bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows_whole(bytes: &[u8], max: usize) -> String {
        normalize_output_text_with_policy(bytes, false, max, DecodePolicy::WindowsLocal)
    }

    fn utf16_bytes(text: &str, little_endian: bool) -> Vec<u8> {
        let mut bytes = if little_endian {
            UTF16LE_BOM.to_vec()
        } else {
            UTF16BE_BOM.to_vec()
        };
        for unit in text.encode_utf16() {
            let encoded = if little_endian {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            };
            bytes.extend_from_slice(&encoded);
        }
        bytes
    }

    #[test]
    fn phase_f_ascii_and_empty_output_remain_identical() {
        assert_eq!(windows_whole(b"plain ASCII\n", 1024), "plain ASCII\n");
        assert_eq!(windows_whole(b"", 1024), "");
    }

    #[test]
    fn phase_f_valid_utf8_unicode_wins_exactly() {
        for text in ["中文", "🙂 e\u{301}"] {
            assert_eq!(windows_whole(text.as_bytes(), 1024), text);
            assert!(!windows_whole(text.as_bytes(), 1024).contains('\u{fffd}'));
        }
    }

    #[test]
    fn phase_f_stream_reconstructs_split_utf8_scalar() {
        let bytes = "🙂".as_bytes();
        let mut decoder = OutputTextDecoder::windows_local_for_test();
        assert_eq!(decoder.push(&bytes[..2], false), "");
        assert_eq!(decoder.push(&bytes[2..], true), "🙂");
    }

    #[test]
    fn phase_f_utf8_bom_is_stripped_across_chunks() {
        assert_eq!(windows_whole(b"\xEF\xBB\xBFhello", 1024), "hello");
        assert_eq!(windows_whole(UTF8_BOM, 1024), "");
        assert_eq!(windows_whole(UTF16LE_BOM, 1024), "");
        assert_eq!(windows_whole(UTF16BE_BOM, 1024), "");

        let mut decoder = OutputTextDecoder::windows_local_for_test();
        assert_eq!(decoder.push(&UTF8_BOM[..1], false), "");
        assert_eq!(decoder.push(&UTF8_BOM[1..2], false), "");
        assert_eq!(decoder.push(&UTF8_BOM[2..], true), "");
    }

    #[test]
    fn phase_f_utf16le_ascii_chinese_and_surrogate_pair_decode() {
        let text = "ASCII 中文 🙂";
        assert_eq!(windows_whole(&utf16_bytes(text, true), 1024), text);
    }

    #[test]
    fn phase_f_utf16be_ascii_chinese_and_surrogate_pair_decode() {
        let text = "ASCII 中文 🙂";
        assert_eq!(windows_whole(&utf16_bytes(text, false), 1024), text);
    }

    #[test]
    fn phase_f_stream_utf16_bom_units_and_surrogates_split_across_chunks() {
        let bytes = utf16_bytes("中文🙂", true);
        let mut decoder = OutputTextDecoder::windows_local_for_test();
        let mut output = String::new();
        for byte in bytes {
            output.push_str(&decoder.push(&[byte], false));
        }
        output.push_str(&decoder.push(&[], true));
        assert_eq!(output, "中文🙂");
    }

    #[test]
    fn phase_f_malformed_utf16_is_lossy_bounded_and_never_panics() {
        assert_eq!(windows_whole(&[0xFF, 0xFE, 0x41], 1024), "\u{fffd}");
        assert_eq!(windows_whole(&[0xFF, 0xFE, 0x00, 0xD8], 1024), "\u{fffd}");
        assert_eq!(windows_whole(&[0xFE, 0xFF, 0xDC, 0x00], 1024), "\u{fffd}");
    }

    #[test]
    fn phase_f_windows_line_endings_normalize_without_changing_lone_cr() {
        assert_eq!(windows_whole(b"a\r\nb\rc\n", 1024), "a\nb\rc\n");

        let mut decoder = OutputTextDecoder::windows_local_for_test();
        assert_eq!(decoder.push(b"a\r", false), "a");
        assert_eq!(decoder.push(b"\nb\r", false), "\nb");
        assert_eq!(decoder.push(b"", true), "\r");
    }

    #[test]
    fn phase_f_final_utf8_cap_survives_utf16_expansion() {
        let bytes = utf16_bytes(&"中".repeat(1000), true);
        let output = windows_whole(&bytes, 127);
        assert!(output.len() <= 127);
        assert!(output.starts_with(LONG_TRUNCATION_MARKER));
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }

    #[test]
    fn phase_f_truncation_never_slices_utf8_scalar() {
        let output = windows_whole("🙂🙂🙂🙂".as_bytes(), 15);
        assert!(output.len() <= 15);
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }

    #[test]
    fn phase_f_raw_tail_truncation_is_marked_even_after_utf16_shrinks() {
        let bytes = utf16_bytes("short", true);
        let output =
            normalize_output_text_with_policy(&bytes, true, 64, DecodePolicy::WindowsLocal);
        assert!(output.starts_with(LONG_TRUNCATION_MARKER));
        assert!(output.len() <= 64);
    }

    #[test]
    fn phase_f_large_malformed_input_stays_bounded() {
        let output = windows_whole(&vec![0xFF; 1_000_000], 4096);
        assert!(output.len() <= 4096);
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }

    #[test]
    fn phase_f_streaming_decoder_pending_state_is_tiny() {
        let mut decoder = OutputTextDecoder::windows_local_for_test();
        for byte in [0xEF, 0xBB] {
            assert!(decoder.push(&[byte], false).is_empty());
            assert!(decoder.pending_state_bytes() <= 3);
        }
        assert_eq!(decoder.push(&[0xBF], false), "");
        for byte in "🙂".as_bytes().iter().copied().take(3) {
            let _ = decoder.push(&[byte], false);
            assert!(decoder.pending_state_bytes() <= 3);
        }
    }

    #[test]
    fn phase_f_remote_and_unix_policy_keep_existing_utf8_lossy_contract() {
        let bytes = b"a\r\n\xff";
        let output = normalize_output_text(bytes, false, 1024, OutputTextSource::RemoteSsh);
        assert_eq!(output, "a\r\n\u{fffd}");
    }

    #[test]
    fn phase_f_append_generated_text_remains_bounded() {
        let mut output = "🙂".repeat(100);
        append_bounded_text(&mut output, "command timed out", 64);
        assert!(output.len() <= 64);
        assert!(std::str::from_utf8(output.as_bytes()).is_ok());
    }
}
