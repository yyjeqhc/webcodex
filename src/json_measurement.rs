//! Exact JSON size measurement without materializing serialized bytes.

use serde::Serialize;
use std::io::{self, Write};

struct CountingWriter(usize);

impl Write for CountingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 = self.0.checked_add(buf.len()).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::OutOfMemory,
                "serialized JSON length overflow",
            )
        })?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn serialized_json_len<T: Serialize + ?Sized>(
    value: &T,
) -> Result<usize, serde_json::Error> {
    let mut counter = CountingWriter(0);
    serde_json::to_writer(&mut counter, value)?;
    Ok(counter.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn assert_exact_len<T: Serialize + ?Sized>(value: &T) {
        assert_eq!(
            serialized_json_len(value).unwrap(),
            serde_json::to_vec(value).unwrap().len()
        );
    }

    #[test]
    fn exact_json_length_matches_buffered_serialization() {
        assert_exact_len("plain ASCII");
        assert_exact_len("quote=\" slash=\\ newline=\n tab=\t control=\u{0008}");
        assert_exact_len("Unicode: 你好 🦀 café 日本語 🌍");
        assert_exact_len(&json!({
            "nested": [null, true, 42, {"escaped": "line\n\"quoted\""}],
            "unicode": ["日本語", "🌍"]
        }));

        let tool_result = crate::tool_runtime::ToolResult::ok(json!({
            "items": [{"path": "src/lib.rs", "text": "fn main() {}"}],
            "output_truncated": false
        }));
        assert_exact_len(&tool_result);

        let tools_list = json!({
            "tools": [{
                "name": "read_files",
                "description": "Read files with exact JSON sizing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {"items": {"type": "array"}}
                },
                "annotations": {"readOnlyHint": true}
            }],
            "resultType": "complete",
            "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "webpi"}}
        });
        assert_exact_len(&tools_list);
    }
}
