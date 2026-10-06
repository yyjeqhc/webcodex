//! Version-one length-delimited JSON. TCP uses little endian; Chrome's native
//! messaging stdio uses native endian. Limits apply before allocation/decoding.
use serde_json::Value;
use std::io::{self, Read, Write};

pub(crate) const VERSION: u32 = 1;
pub(crate) const MAX_COMMAND: usize = 256 * 1024;
pub(crate) const MAX_RESPONSE: usize = 4 * 1024 * 1024;
pub(crate) const EXTENSION_ID: &str = "pjhnlafbcbgcgjpkfiomhjnhpnaohgcg";

pub(crate) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "invalid bounded Browser bridge frame",
    )
}

pub(crate) fn write_frame(
    writer: &mut impl Write,
    value: &Value,
    max: usize,
    native: bool,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(|_| invalid())?;
    if bytes.is_empty() || bytes.len() > max {
        return Err(invalid());
    }
    let length = bytes.len() as u32;
    writer.write_all(&if native {
        length.to_ne_bytes()
    } else {
        length.to_le_bytes()
    })?;
    writer.write_all(&bytes)?;
    writer.flush()
}

pub(crate) fn read_blocking(reader: &mut impl Read, max: usize, native: bool) -> io::Result<Value> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let size = if native {
        u32::from_ne_bytes(header)
    } else {
        u32::from_le_bytes(header)
    } as usize;
    if size == 0 || size > max {
        return Err(invalid());
    }
    let mut body = vec![0; size];
    reader.read_exact(&mut body)?;
    let value: Value = serde_json::from_slice(&body).map_err(|_| invalid())?;
    if !value.is_object() {
        return Err(invalid());
    }
    Ok(value)
}

/// Preserve partial frames over socket timeouts; never reinterpret a body suffix
/// as the next length header after a short read.
pub(crate) struct FrameReader {
    bytes: Vec<u8>,
    max: usize,
}
impl FrameReader {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max,
        }
    }
    pub(crate) fn poll(&mut self, reader: &mut impl Read) -> io::Result<Option<Value>> {
        // Socket timeouts bound a single read, not a succession of short reads.
        // Yield after four reads so the owner can check its absolute handshake
        // deadline and shutdown flag even when a peer trickles bytes forever.
        let mut reads = 0;
        loop {
            if self.bytes.len() >= 4 {
                let size = u32::from_le_bytes(self.bytes[..4].try_into().unwrap()) as usize;
                if size == 0 || size > self.max {
                    return Err(invalid());
                }
                if self.bytes.len() >= size + 4 {
                    let value: Value =
                        serde_json::from_slice(&self.bytes[4..size + 4]).map_err(|_| invalid())?;
                    self.bytes.drain(..size + 4);
                    if !value.is_object() {
                        return Err(invalid());
                    }
                    return Ok(Some(value));
                }
            }
            if reads == 4 {
                return Ok(None);
            }
            reads += 1;
            let mut chunk = [0; 4096];
            let count = chunk.len().min(self.max + 4 - self.bytes.len());
            match reader.read(&mut chunk[..count]) {
                Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
                Ok(count) => self.bytes.extend_from_slice(&chunk[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(error) => return Err(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_bounds_are_checked_before_reading_or_allocating_the_body() {
        let mut oversized = ((MAX_RESPONSE + 1) as u32)
            .to_le_bytes()
            .as_slice()
            .to_vec();
        assert!(read_blocking(&mut oversized.as_slice(), MAX_RESPONSE, false).is_err());
        assert!(FrameReader::new(MAX_RESPONSE)
            .poll(&mut oversized.as_slice())
            .is_err());
        oversized.clear();
        write_frame(&mut oversized, &serde_json::json!({"ok":true}), 100, false).unwrap();
        assert_eq!(
            read_blocking(&mut oversized.as_slice(), 100, false).unwrap()["ok"],
            true
        );
    }
    #[test]
    fn continuous_short_reads_yield_to_owner_deadlines_and_shutdown() {
        struct Trickle {
            bytes: std::io::Cursor<Vec<u8>>,
            reads: usize,
        }
        impl Read for Trickle {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.reads += 1;
                let n = bytes.len().min(1);
                self.bytes.read(&mut bytes[..n])
            }
        }
        let mut encoded = Vec::new();
        write_frame(
            &mut encoded,
            &serde_json::json!({"data":"x".repeat(100)}),
            200,
            false,
        )
        .unwrap();
        let mut input = Trickle {
            bytes: std::io::Cursor::new(encoded),
            reads: 0,
        };
        let mut reader = FrameReader::new(200);
        assert!(reader.poll(&mut input).unwrap().is_none());
        assert!(
            input.reads <= 4,
            "partial reads must yield to the owner, not renew its deadline"
        );
        for _ in 0..200 {
            if let Some(value) = reader.poll(&mut input).unwrap() {
                assert_eq!(value["data"], "x".repeat(100));
                return;
            }
        }
        panic!("yielding reader lost the partial frame");
    }

    #[test]
    fn partial_frames_survive_intermediate_would_block() {
        struct Fragment {
            bytes: std::io::Cursor<Vec<u8>>,
            block: bool,
        }
        impl Read for Fragment {
            fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
                self.block = !self.block;
                if self.block {
                    return Err(io::ErrorKind::WouldBlock.into());
                }
                let count = bytes.len().min(1);
                self.bytes.read(&mut bytes[..count])
            }
        }
        let mut encoded = Vec::new();
        write_frame(&mut encoded, &serde_json::json!({"id":5}), 100, false).unwrap();
        let mut input = Fragment {
            bytes: std::io::Cursor::new(encoded),
            block: false,
        };
        let mut reader = FrameReader::new(100);
        for _ in 0..50 {
            if let Some(value) = reader.poll(&mut input).unwrap() {
                assert_eq!(value["id"], 5);
                return;
            }
        }
        panic!("partial frame was not reconstructed");
    }
}
