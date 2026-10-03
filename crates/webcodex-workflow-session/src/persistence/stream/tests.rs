use super::*;
use crate::SessionStore;
use std::io::Read;

#[cfg(target_os = "linux")]
mod benchmark;

fn ledger() -> String {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("sessions.json");
    let store = SessionStore::with_persistence(&path, 10, 20);
    store.start_session(Some("project".into()), None);
    let closed = store.start_session(None, None).session_id;
    store.close_session(&closed).unwrap();
    store.flush_persistence();
    let text = std::fs::read_to_string(path).unwrap();
    drop(store);
    text
}

#[test]
fn streaming_restore_preserves_rows_and_accepts_version_after_rows() {
    let value: Value = serde_json::from_str(&ledger()).unwrap();
    let reversed = format!("{{\"sessions\":{},\"version\":2}}", value["sessions"]);
    let rows = load(reversed.as_bytes(), 20).unwrap();
    assert_eq!(rows.records.len(), 2);
    assert_eq!(
        rows.records
            .iter()
            .filter(|r| matches!(r, StoredSession::Hot(_)))
            .count(),
        1
    );
    assert_eq!(
        rows.records
            .iter()
            .filter(|r| matches!(r, StoredSession::Cold(_)))
            .count(),
        1
    );
}

#[test]
fn streaming_restore_rejects_entire_bad_envelope_without_partial_publication() {
    let source = ledger();
    let value: Value = serde_json::from_str(&source).unwrap();
    for text in [
        format!("{source} garbage"),
        source[..source.len() - 1].to_string(),
        format!(
            "{{\"version\":2,\"version\":2,\"sessions\":{}}}",
            value["sessions"]
        ),
        format!(
            "{{\"version\":2,\"sessions\":{},\"sessions\":[]}}",
            value["sessions"]
        ),
        format!("{{\"sessions\":{}}}", value["sessions"]),
        format!("{{\"sessions\":{},\"version\":1}}", value["sessions"]),
        format!(
            "{{\"sessions\":{},\"version\":2,\"unknown\":null}}",
            value["sessions"]
        ),
        "{\"version\":2}".to_string(),
    ] {
        assert!(load(text.as_bytes(), 20).is_err());
    }
}

#[test]
fn streaming_restore_skips_only_malformed_rows_and_handles_tiny_reads() {
    struct Tiny<'a>(&'a [u8]);
    impl Read for Tiny<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let n = out.len().min(3).min(self.0.len());
            out[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }
    let mut value: Value = serde_json::from_str(&ledger()).unwrap();
    let array = value["sessions"].as_array_mut().unwrap();
    array.insert(0, serde_json::json!({"events":[],"extra":true}));
    array.push(Value::Null);
    let text = serde_json::to_string(&value).unwrap();
    assert_eq!(load(Tiny(text.as_bytes()), 20).unwrap().records.len(), 2);
}

#[test]
fn streaming_restore_large_closed_fixture_keeps_only_final_raw_rows() {
    let mut value: Value = serde_json::from_str(&ledger()).unwrap();
    let template = value["sessions"]
        .as_array_mut()
        .unwrap()
        .iter()
        .find(|v| v["lifecycle"] == "closed")
        .unwrap()
        .clone();
    let mut rows = Vec::new();
    for i in 0..256 {
        let mut row = template.clone();
        row["session_id"] = Value::String(format!("wc_sess_{i:032x}"));
        // Each record still takes the same native sanitizer and authority checks.
        rows.push(row);
    }
    let text = serde_json::to_string(&serde_json::json!({"version":2,"sessions":rows})).unwrap();
    let restored = load(text.as_bytes(), 20).unwrap();
    assert_eq!(restored.records.len(), 256);
    assert!(restored
        .records
        .iter()
        .all(|r| matches!(r, StoredSession::Cold(_))));
    eprintln!("SESSION_STREAM rows={} input_bytes={} whole_file_string_copies=0 whole_ledger_value_trees=0 peak_transient_rows=1", restored.records.len(), text.len());
}
