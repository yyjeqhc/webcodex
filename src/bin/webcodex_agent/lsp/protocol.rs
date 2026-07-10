use serde_json::Value;
use std::fmt;
use std::io::{self, BufRead, Write};

pub(super) const MAX_LSP_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
pub(super) const MAX_LSP_HEADER_LINE_BYTES: usize = 8 * 1024;
pub(super) const MAX_LSP_HEADER_BYTES: usize = 32 * 1024;
pub(super) const MAX_LSP_HEADER_COUNT: usize = 64;

#[derive(Debug)]
pub(super) enum FramingError {
    Io(io::Error),
    MalformedHeader(String),
    NonUtf8Header,
    HeaderLineTooLarge { length: usize, maximum: usize },
    HeaderTooLarge { length: usize, maximum: usize },
    TooManyHeaders { count: usize, maximum: usize },
    MissingContentLength,
    InvalidContentLength,
    MessageTooLarge { length: usize, maximum: usize },
}

impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "LSP stream I/O failed: {error}"),
            Self::MalformedHeader(header) => write!(f, "malformed LSP header: {header}"),
            Self::NonUtf8Header => f.write_str("LSP header is not valid UTF-8"),
            Self::HeaderLineTooLarge { length, maximum } => {
                write!(
                    f,
                    "LSP header line length {length} exceeds maximum {maximum}"
                )
            }
            Self::HeaderTooLarge { length, maximum } => {
                write!(
                    f,
                    "LSP header block length {length} exceeds maximum {maximum}"
                )
            }
            Self::TooManyHeaders { count, maximum } => {
                write!(f, "LSP header count {count} exceeds maximum {maximum}")
            }
            Self::MissingContentLength => f.write_str("LSP message is missing Content-Length"),
            Self::InvalidContentLength => f.write_str("LSP Content-Length is invalid"),
            Self::MessageTooLarge { length, maximum } => {
                write!(f, "LSP message length {length} exceeds maximum {maximum}")
            }
        }
    }
}

impl From<io::Error> for FramingError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(super) fn write_message(writer: &mut impl Write, message: &Value) -> Result<(), FramingError> {
    let body = serde_json::to_vec(message)
        .map_err(|error| FramingError::Io(io::Error::new(io::ErrorKind::InvalidData, error)))?;
    if body.len() > MAX_LSP_MESSAGE_BYTES {
        return Err(FramingError::MessageTooLarge {
            length: body.len(),
            maximum: MAX_LSP_MESSAGE_BYTES,
        });
    }
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

pub(super) fn read_message(
    reader: &mut impl BufRead,
    maximum: usize,
) -> Result<Value, FramingError> {
    let mut content_length = None;
    let mut total_header_bytes = 0_usize;
    let mut header_count = 0_usize;
    loop {
        let line = read_header_line(reader)?;
        if line.is_empty() {
            return Err(FramingError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LSP stdout closed",
            )));
        }

        total_header_bytes = total_header_bytes.saturating_add(line.len());
        if total_header_bytes > MAX_LSP_HEADER_BYTES {
            return Err(FramingError::HeaderTooLarge {
                length: total_header_bytes,
                maximum: MAX_LSP_HEADER_BYTES,
            });
        }

        if line == b"\r\n" || line == b"\n" {
            break;
        }

        header_count = header_count.saturating_add(1);
        if header_count > MAX_LSP_HEADER_COUNT {
            return Err(FramingError::TooManyHeaders {
                count: header_count,
                maximum: MAX_LSP_HEADER_COUNT,
            });
        }

        let text = std::str::from_utf8(&line).map_err(|_| FramingError::NonUtf8Header)?;
        let trimmed = text.trim_end_matches(['\r', '\n']);
        let (name, value) = trimmed
            .split_once(':')
            .ok_or_else(|| FramingError::MalformedHeader(trimmed.to_string()))?;
        if name.trim().eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(FramingError::MalformedHeader(
                    "duplicate Content-Length".to_string(),
                ));
            }
            let length = value
                .trim()
                .parse::<usize>()
                .map_err(|_| FramingError::InvalidContentLength)?;
            content_length = Some(length);
        }
    }

    let length = content_length.ok_or(FramingError::MissingContentLength)?;
    if length > maximum {
        return Err(FramingError::MessageTooLarge { length, maximum });
    }
    let mut body = vec![0_u8; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body).map_err(|error| {
        FramingError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("malformed LSP JSON: {error}"),
        ))
    })
}

/// Read one header line (including the trailing newline) with a hard byte cap.
///
/// Returns an empty buffer only on clean EOF before any bytes were read.
/// Exceeding [`MAX_LSP_HEADER_LINE_BYTES`] fails immediately without first
/// buffering an unbounded line into a `String`.
fn read_header_line(reader: &mut impl BufRead) -> Result<Vec<u8>, FramingError> {
    let mut line = Vec::new();
    loop {
        let available = {
            let buffer = reader.fill_buf().map_err(FramingError::Io)?;
            if buffer.is_empty() {
                return Ok(line);
            }
            if let Some(newline) = buffer.iter().position(|&byte| byte == b'\n') {
                let take = newline + 1;
                let next_len = line.len().saturating_add(take);
                if next_len > MAX_LSP_HEADER_LINE_BYTES {
                    return Err(FramingError::HeaderLineTooLarge {
                        length: next_len,
                        maximum: MAX_LSP_HEADER_LINE_BYTES,
                    });
                }
                line.extend_from_slice(&buffer[..take]);
                take
            } else {
                let next_len = line.len().saturating_add(buffer.len());
                if next_len > MAX_LSP_HEADER_LINE_BYTES {
                    return Err(FramingError::HeaderLineTooLarge {
                        length: next_len,
                        maximum: MAX_LSP_HEADER_LINE_BYTES,
                    });
                }
                line.extend_from_slice(buffer);
                buffer.len()
            }
        };
        reader.consume(available);
        if line.last() == Some(&b'\n') {
            return Ok(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::{BufReader, Cursor, Read};

    struct PartialReader<R> {
        inner: R,
        maximum_read: usize,
    }

    impl<R: Read> Read for PartialReader<R> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let limit = buffer.len().min(self.maximum_read);
            self.inner.read(&mut buffer[..limit])
        }
    }

    fn encoded(value: &Value) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_message(&mut bytes, value).unwrap();
        bytes
    }

    #[test]
    fn lsp_framing_encodes_content_length_and_utf8_byte_length() {
        let value = json!({"jsonrpc": "2.0", "method": "测试"});
        let bytes = encoded(&value);
        let body = serde_json::to_vec(&value).unwrap();
        let expected = format!("Content-Length: {}\r\n\r\n", body.len());
        assert!(bytes.starts_with(expected.as_bytes()));
        assert_eq!(&bytes[expected.len()..], body);
        assert!(body.len() > value.to_string().chars().count());
    }

    #[test]
    fn lsp_framing_reads_partial_headers_and_bodies() {
        let value = json!({"jsonrpc": "2.0", "id": 7, "result": {"name": "α"}});
        let reader = PartialReader {
            inner: Cursor::new(encoded(&value)),
            maximum_read: 2,
        };
        let mut reader = BufReader::with_capacity(3, reader);
        assert_eq!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES).unwrap(),
            value
        );
    }

    #[test]
    fn lsp_framing_reads_consecutive_messages_case_insensitively() {
        let first = json!({"jsonrpc": "2.0", "method": "one"});
        let second = json!({"jsonrpc": "2.0", "method": "two"});
        let mut bytes = encoded(&first);
        bytes.extend(encoded(&second));
        let header_end = bytes
            .windows(4)
            .position(|part| part == b"\r\n\r\n")
            .unwrap();
        bytes[..header_end].make_ascii_lowercase();
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert_eq!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES).unwrap(),
            first
        );
        assert_eq!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES).unwrap(),
            second
        );
    }

    #[test]
    fn lsp_framing_rejects_malformed_or_missing_headers() {
        let malformed = b"Content-Length nope\r\n\r\n{}";
        let mut reader = BufReader::new(Cursor::new(malformed));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::MalformedHeader(_))
        ));

        let missing = b"Content-Type: application/json\r\n\r\n{}";
        let mut reader = BufReader::new(Cursor::new(missing));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::MissingContentLength)
        ));
    }

    #[test]
    fn lsp_framing_rejects_invalid_and_oversized_content_length() {
        let invalid = b"Content-Length: nope\r\n\r\n";
        let mut reader = BufReader::new(Cursor::new(invalid));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::InvalidContentLength)
        ));

        let oversized = b"Content-Length: 12\r\n\r\n";
        let mut reader = BufReader::new(Cursor::new(oversized));
        assert!(matches!(
            read_message(&mut reader, 8),
            Err(FramingError::MessageTooLarge {
                length: 12,
                maximum: 8
            })
        ));
    }

    #[test]
    fn lsp_framing_rejects_oversized_header_line_without_buffering_unbounded() {
        let mut bytes = Vec::new();
        bytes.extend(b"X: ");
        bytes.extend(std::iter::repeat_n(b'a', MAX_LSP_HEADER_LINE_BYTES));
        bytes.extend(b"\r\n\r\n");
        let mut reader = BufReader::with_capacity(64, Cursor::new(bytes));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::HeaderLineTooLarge { .. })
        ));
    }

    #[test]
    fn lsp_framing_rejects_header_block_over_total_byte_limit() {
        // A few long headers (under the per-line cap) that together exceed the
        // total header block limit without hitting the count limit first.
        let mut bytes = Vec::new();
        let payload = "y".repeat(6 * 1024);
        for index in 0..6 {
            let line = format!("X-{index}: {payload}\r\n");
            assert!(line.len() <= MAX_LSP_HEADER_LINE_BYTES);
            bytes.extend_from_slice(line.as_bytes());
        }
        assert!(bytes.len() > MAX_LSP_HEADER_BYTES);
        bytes.extend_from_slice(b"Content-Length: 2\r\n\r\n{}");
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::HeaderTooLarge { .. })
        ));
    }

    #[test]
    fn lsp_framing_rejects_too_many_header_lines() {
        let mut bytes = Vec::new();
        for index in 0..=MAX_LSP_HEADER_COUNT {
            bytes.extend_from_slice(format!("X-{index}: 1\r\n").as_bytes());
        }
        bytes.extend_from_slice(b"Content-Length: 2\r\n\r\n{}");
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::TooManyHeaders { .. })
        ));
    }

    #[test]
    fn lsp_framing_rejects_non_utf8_header() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Content-Length: 2\r\n");
        bytes.extend_from_slice(b"X: \xff\xfe\r\n\r\n{}");
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert!(matches!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES),
            Err(FramingError::NonUtf8Header)
        ));
    }

    #[test]
    fn lsp_framing_accepts_header_line_at_max_length() {
        // Content-Length is short; pad with a max-length secondary header line.
        let mut pad = String::from("X: ");
        // line includes trailing \r\n, so payload + "X: " + "\r\n" == MAX
        let payload_len = MAX_LSP_HEADER_LINE_BYTES
            .saturating_sub(3 /* "X: " */)
            .saturating_sub(2 /* "\r\n" */);
        pad.push_str(&"y".repeat(payload_len));
        assert_eq!(pad.len() + 2, MAX_LSP_HEADER_LINE_BYTES);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"Content-Length: 2\r\n");
        bytes.extend_from_slice(pad.as_bytes());
        bytes.extend_from_slice(b"\r\n\r\n{}");
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert_eq!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES).unwrap(),
            json!({})
        );
    }

    #[test]
    fn lsp_framing_accepts_lf_only_headers() {
        let body = br#"{"jsonrpc":"2.0","id":1,"result":null}"#;
        let mut bytes = format!("Content-Length: {}\n\n", body.len()).into_bytes();
        bytes.extend_from_slice(body);
        let mut reader = BufReader::new(Cursor::new(bytes));
        assert_eq!(
            read_message(&mut reader, MAX_LSP_MESSAGE_BYTES).unwrap(),
            json!({"jsonrpc":"2.0","id":1,"result":null})
        );
    }
}
