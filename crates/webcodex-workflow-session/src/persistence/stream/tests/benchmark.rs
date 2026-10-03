//! Manual isolated-process comparison. The eager reference reproduces the old
//! whole-file/whole-Value/materialized-vector allocation pattern on synthetic data.
use super::*;
use std::process::{Command, Stdio};
use std::time::Instant;

fn eager_reference(path: &std::path::Path) -> Vec<StoredSession> {
    let content = std::fs::read_to_string(path).unwrap();
    let value: Value = serde_json::from_str(&content).unwrap();
    let Value::Object(mut object) = value else {
        unreachable!()
    };
    let Value::Array(rows) = object.remove("sessions").unwrap() else {
        unreachable!()
    };
    let records: Vec<_> = rows
        .into_iter()
        .filter_map(|value| {
            assert!(v2_record_has_canonical_logical_invocation_shape(&value));
            serde_json::from_value::<PersistedSessionRecord>(value)
                .unwrap()
                .into_record(2000)
        })
        .collect();
    records
        .into_iter()
        .map(|record| {
            if record.lifecycle.allows_mutation() {
                StoredSession::Hot(record)
            } else {
                StoredSession::Cold(cold_session_from_record(&record, 2000).unwrap())
            }
        })
        .collect()
}

#[test]
#[ignore = "manual isolated-process memory benchmark, synthetic ledger only"]
fn streaming_restore_memory_benchmark() {
    if let Ok(mode) = std::env::var("WEBCODEX_TEST_RESTORE_MODE") {
        let path =
            std::path::PathBuf::from(std::env::var_os("WEBCODEX_TEST_RESTORE_PATH").unwrap());
        let started = Instant::now();
        let records = if mode == "eager_reference" {
            eager_reference(&path)
        } else {
            load(
                io::BufReader::new(std::fs::File::open(&path).unwrap()),
                2000,
            )
            .unwrap()
            .records
        };
        assert_eq!(records.len(), 512);
        assert!(records
            .iter()
            .all(|record| matches!(record, StoredSession::Cold(_))));
        let elapsed = started.elapsed();
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let peak = status
            .lines()
            .find(|line| line.starts_with("VmHWM:"))
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap();
        println!("SESSION_RESTORE_BENCH mode={mode} input_bytes={} rows={} elapsed_ms={:.3} peak_rss_kib={peak}",
            std::fs::metadata(path).unwrap().len(),records.len(),elapsed.as_secs_f64()*1000.0);
        return;
    }
    let value: Value = serde_json::from_str(&ledger()).unwrap();
    let template = value["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["lifecycle"] == "closed")
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("fixture.json");
    {
        use std::io::Write;
        let mut file = io::BufWriter::new(std::fs::File::create(&path).unwrap());
        file.write_all(b"{\"version\":2,\"sessions\":[").unwrap();
        for i in 0..512 {
            let id = format!("wc_sess_{i:032x}");
            let mut row = template.clone();
            row["session_id"] = Value::String(id.clone());
            let events: Vec<_> = (0..64)
                .map(|n| {
                    let mut event = template["events"][0].clone();
                    event["session_id"] = Value::String(id.clone());
                    event["event_id"] = Value::String(format!(
                        "{}{:032x}",
                        crate::model::EVENT_ID_PREFIX,
                        i * 64 + n
                    ));
                    event["error_message_summary"] =
                        Value::String("synthetic observation ".repeat(10));
                    event
                })
                .collect();
            row["events"] = Value::Array(events);
            row["events_observed"] = Value::from(64);
            if i > 0 {
                file.write_all(b",").unwrap();
            }
            serde_json::to_writer(&mut file, &row).unwrap();
        }
        file.write_all(b"]}").unwrap();
        file.flush().unwrap();
    }
    for mode in ["eager_reference", "stream"] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::stream::tests::benchmark::streaming_restore_memory_benchmark",
                "--ignored",
                "--nocapture",
            ])
            .env("WEBCODEX_TEST_RESTORE_MODE", mode)
            .env("WEBCODEX_TEST_RESTORE_PATH", &path)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
