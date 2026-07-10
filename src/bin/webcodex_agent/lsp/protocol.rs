use serde_json::Value;
use std::fmt;
use std::io::{self, BufRead, Write};

pub(super) const MAX_LSP_MESSAGE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
pub(super) enum FramingError {
    Io(io::Error),
    MalformedHeader(String),
    MissingContentLength,
    InvalidContentLength,
    MessageTooLarge { length: usize, maximum: usize },
}

impl fmt::Display for FramingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "LSP stream I/O failed: {error}"),
            Self::MalformedHeader(header) => write!(f, "malformed LSP header: {header}"),
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
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line)?;
        if read == 0 {
            return Err(FramingError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "LSP stdout closed",
            )));
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
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
}
