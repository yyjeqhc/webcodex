//! Public ↔ LSP position conversion.
//!
//! Public tools use 1-based line and 1-based Unicode scalar columns.
//! LSP uses 0-based line and encoding-specific character offsets.

use super::supervisor::PositionEncoding;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Request-local cache of file text used for position conversion.
#[derive(Default)]
pub(crate) struct LineCache {
    files: HashMap<PathBuf, Option<String>>,
}

impl LineCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn text(&mut self, path: &Path) -> Option<&str> {
        if !self.files.contains_key(path) {
            let text = fs::read_to_string(path).ok();
            self.files.insert(path.to_path_buf(), text);
        }
        self.files.get(path).and_then(|value| value.as_deref())
    }

    pub(crate) fn seed(&mut self, path: &Path, text: String) {
        self.files.insert(path.to_path_buf(), Some(text));
    }
}

/// Convert public 1-based Unicode scalar coordinates to an LSP Position.
///
/// Column may be `scalar_count + 1` for the end-of-line caret position.
pub(crate) fn public_to_lsp(
    text: &str,
    line: usize,
    column: usize,
    encoding: PositionEncoding,
) -> Result<(u32, u32), String> {
    if line == 0 || column == 0 {
        return Err("line and column must be >= 1".to_string());
    }
    let zero_line = line - 1;
    let line_text = line_at(text, zero_line).ok_or_else(|| {
        format!(
            "line {line} is out of range (document has {} lines)",
            line_count(text)
        )
    })?;
    let scalar_len = line_text.chars().count();
    // End-of-line caret is scalar_count + 1.
    if column > scalar_len + 1 {
        return Err(format!(
            "column {column} is out of range for line {line} (length {scalar_len})"
        ));
    }
    let scalar_index = column - 1;
    let character = match encoding {
        PositionEncoding::Utf32 => scalar_index as u32,
        PositionEncoding::Utf16 => {
            let prefix: String = line_text.chars().take(scalar_index).collect();
            utf16_len(&prefix) as u32
        }
        PositionEncoding::Utf8 => {
            let prefix: String = line_text.chars().take(scalar_index).collect();
            prefix.len() as u32
        }
    };
    Ok((zero_line as u32, character))
}

/// Convert an LSP Position to public 1-based Unicode scalar coordinates.
///
/// Returns `None` when the LSP position is illegal for the given text/encoding.
pub(crate) fn lsp_to_public(
    text: &str,
    line: u32,
    character: u32,
    encoding: PositionEncoding,
) -> Option<(usize, usize)> {
    let zero_line = line as usize;
    let line_text = line_at(text, zero_line)?;
    let scalar_column = match encoding {
        PositionEncoding::Utf32 => {
            let char_index = character as usize;
            if char_index > line_text.chars().count() {
                return None;
            }
            char_index + 1
        }
        PositionEncoding::Utf16 => {
            let scalar_index = utf16_offset_to_scalar_index(line_text, character as usize)?;
            scalar_index + 1
        }
        PositionEncoding::Utf8 => {
            let scalar_index = utf8_offset_to_scalar_index(line_text, character as usize)?;
            scalar_index + 1
        }
    };
    Some((zero_line + 1, scalar_column))
}

pub(crate) fn line_count(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    // `split('\n')` yields a trailing empty segment after a final newline,
    // which correctly represents an empty final line. Empty text has 0 lines.
    text.split('\n').count()
}

pub(crate) fn line_at(text: &str, zero_based_line: usize) -> Option<&str> {
    if text.is_empty() {
        return if zero_based_line == 0 { Some("") } else { None };
    }
    text.split('\n').nth(zero_based_line).map(|line| {
        // Strip trailing CR from CRLF lines.
        line.strip_suffix('\r').unwrap_or(line)
    })
}

fn utf16_len(text: &str) -> usize {
    text.chars().map(|c| c.len_utf16()).sum()
}

fn utf16_offset_to_scalar_index(line: &str, utf16_offset: usize) -> Option<usize> {
    let mut units = 0usize;
    for (index, ch) in line.chars().enumerate() {
        if units == utf16_offset {
            return Some(index);
        }
        let width = ch.len_utf16();
        if units + width > utf16_offset {
            // Offset landed inside a surrogate pair — illegal.
            return None;
        }
        units += width;
    }
    if units == utf16_offset {
        Some(line.chars().count())
    } else {
        None
    }
}

fn utf8_offset_to_scalar_index(line: &str, utf8_offset: usize) -> Option<usize> {
    if utf8_offset > line.len() {
        return None;
    }
    if utf8_offset == line.len() {
        return Some(line.chars().count());
    }
    if !line.is_char_boundary(utf8_offset) {
        return None;
    }
    Some(line[..utf8_offset].chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_to_lsp_ascii_and_end_of_line() {
        let text = "hello\n";
        assert_eq!(
            public_to_lsp(text, 1, 1, PositionEncoding::Utf16).unwrap(),
            (0, 0)
        );
        assert_eq!(
            public_to_lsp(text, 1, 6, PositionEncoding::Utf16).unwrap(),
            (0, 5)
        );
        // End-of-line caret is scalar_count + 1 => column 6 for "hello".
        assert_eq!(
            public_to_lsp(text, 1, 6, PositionEncoding::Utf16).unwrap(),
            (0, 5)
        );
        assert!(public_to_lsp(text, 1, 7, PositionEncoding::Utf16).is_err());
        assert!(public_to_lsp(text, 0, 1, PositionEncoding::Utf16).is_err());
    }

    #[test]
    fn chinese_and_emoji_encodings() {
        // "你" is one scalar, 3 UTF-8 bytes, 1 UTF-16 unit.
        // "😀" is one scalar, 4 UTF-8 bytes, 2 UTF-16 units.
        let text = "你a😀b\n";
        // Columns: 1=你, 2=a, 3=😀, 4=b, 5=eol
        assert_eq!(
            public_to_lsp(text, 1, 1, PositionEncoding::Utf32).unwrap(),
            (0, 0)
        );
        assert_eq!(
            public_to_lsp(text, 1, 3, PositionEncoding::Utf32).unwrap(),
            (0, 2)
        );
        assert_eq!(
            public_to_lsp(text, 1, 3, PositionEncoding::Utf16).unwrap(),
            (0, 2) // 你(1) + a(1) = 2 before emoji
        );
        assert_eq!(
            public_to_lsp(text, 1, 4, PositionEncoding::Utf16).unwrap(),
            (0, 4) // after emoji
        );
        assert_eq!(
            public_to_lsp(text, 1, 3, PositionEncoding::Utf8).unwrap(),
            (0, 4) // 你(3) + a(1)
        );
        assert_eq!(
            public_to_lsp(text, 1, 4, PositionEncoding::Utf8).unwrap(),
            (0, 8) // + 😀(4)
        );

        assert_eq!(
            lsp_to_public(text, 0, 2, PositionEncoding::Utf16).unwrap(),
            (1, 3)
        );
        assert_eq!(
            lsp_to_public(text, 0, 4, PositionEncoding::Utf16).unwrap(),
            (1, 4)
        );
        assert_eq!(
            lsp_to_public(text, 0, 4, PositionEncoding::Utf8).unwrap(),
            (1, 3)
        );
        assert_eq!(
            lsp_to_public(text, 0, 8, PositionEncoding::Utf8).unwrap(),
            (1, 4)
        );
        assert_eq!(
            lsp_to_public(text, 0, 2, PositionEncoding::Utf32).unwrap(),
            (1, 3)
        );
    }

    #[test]
    fn crlf_and_empty_line() {
        let text = "a\r\n\r\nb\n";
        assert_eq!(line_at(text, 0).unwrap(), "a");
        assert_eq!(line_at(text, 1).unwrap(), "");
        assert_eq!(line_at(text, 2).unwrap(), "b");
        assert_eq!(
            public_to_lsp(text, 2, 1, PositionEncoding::Utf16).unwrap(),
            (1, 0)
        );
    }

    #[test]
    fn illegal_lsp_positions_return_none() {
        let text = "😀\n";
        // Character 1 is inside the surrogate pair for UTF-16.
        assert!(lsp_to_public(text, 0, 1, PositionEncoding::Utf16).is_none());
        // Mid-codepoint UTF-8 offset.
        assert!(lsp_to_public(text, 0, 1, PositionEncoding::Utf8).is_none());
        assert!(lsp_to_public(text, 5, 0, PositionEncoding::Utf16).is_none());
    }
}
