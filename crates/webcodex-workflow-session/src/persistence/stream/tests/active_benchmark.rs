//! Isolated Linux memory comparison on generated data, not production sessions.
//! The reference uses the PREVIOUS streaming + per-row sanitation path, not the
//! older whole-document parser (which would overstate this change's benefit).
use super::*;
use serde::{Deserialize, Deserializer};
use std::process::{Command, Stdio};
use std::time::Instant;

struct HotRow(StoredSession);
impl<'de> Deserialize<'de> for HotRow {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(decoder)?;
        assert!(v2_record_has_canonical_logical_invocation_shape(&value));
        let row: PersistedSessionRecord = serde_json::from_value(value).unwrap();
        Ok(Self(StoredSession::Hot(row.into_record(2000).unwrap())))
    }
}
#[derive(Deserialize)]
struct HotLedger {
    version: u32,
    sessions: Vec<HotRow>,
}

#[test]
#[ignore = "manual isolated-process active residency benchmark; synthetic data only"]
fn active_restore_residency_memory_benchmark() {
    if let Ok(mode) = std::env::var("WEBCODEX_TEST_ACTIVE_RESIDENCY_MODE") {
        let path = std::path::PathBuf::from(
            std::env::var_os("WEBCODEX_TEST_ACTIVE_RESIDENCY_PATH").unwrap(),
        );
        let start = Instant::now();
        let mut records = if mode == "previous_stream_hot" {
            let ledger: HotLedger =
                serde_json::from_reader(io::BufReader::new(std::fs::File::open(&path).unwrap()))
                    .unwrap();
            assert_eq!(ledger.version, 2);
            ledger
                .sessions
                .into_iter()
                .map(|row| row.0)
                .collect::<Vec<_>>()
        } else {
            let rows = load(
                io::BufReader::new(std::fs::File::open(&path).unwrap()),
                2000,
            )
            .unwrap();
            rows.records
        };
        let restore_ms = start.elapsed().as_secs_f64() * 1000.0;
        if mode == "cold_plus_100_materialized" {
            for entry in records.iter_mut().take(100) {
                let StoredSession::Cold(cold) = entry else {
                    panic!("Cold restore");
                };
                *entry = StoredSession::Hot(materialize_cold_session(cold, 2000).unwrap());
            }
        }
        assert_eq!(records.len(), 512);
        assert!(records.iter().all(|row| row.lifecycle().allows_mutation()));
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let kib = |key: &str| {
            status
                .lines()
                .find(|line| line.starts_with(key))
                .unwrap()
                .split_whitespace()
                .nth(1)
                .unwrap()
                .parse::<u64>()
                .unwrap()
        };
        let rss = kib("VmRSS:");
        let peak = kib("VmHWM:");
        let sample = Instant::now();
        for entry in records.iter().rev().take(50) {
            match entry {
                StoredSession::Hot(record) => {
                    std::hint::black_box(record.events.len());
                }
                StoredSession::Cold(cold) => {
                    std::hint::black_box(
                        materialize_cold_session(cold, 2000).unwrap().events.len(),
                    );
                }
            }
        }
        println!(
            "ACTIVE_RESIDENCY_BENCH {}",
            serde_json::json!({
                "mode":mode,"input_bytes":std::fs::metadata(path).unwrap().len(),"sessions":records.len(),
                "hot":records.iter().filter(|row| row.hot().is_some()).count(),
                "rss_kib":rss,"peak_rss_kib":peak,"restore_ms":restore_ms,
                "materialize_50_ms":sample.elapsed().as_secs_f64()*1000.0,
                "scope":"synthetic 512 Active rows x 128 events; residency cost, not end-to-end latency"
            })
        );
        return;
    }
    let baseline: Value = serde_json::from_str(&ledger()).unwrap();
    let template = baseline["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["lifecycle"] == "closed")
        .unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("active-fixture.json");
    {
        use std::io::Write;
        let mut file = io::BufWriter::new(std::fs::File::create(&path).unwrap());
        file.write_all(b"{\"version\":2,\"sessions\":[").unwrap();
        for i in 0..512 {
            let id = format!("wc_sess_{i:032x}");
            let mut row = template.clone();
            row["session_id"] = id.clone().into();
            row["lifecycle"] = "active".into();
            row["events"] = (0..128)
                .map(|n| {
                    let mut event = template["events"][0].clone();
                    event["session_id"] = id.clone().into();
                    event["event_id"] = format!("evt_{:032x}", i * 128 + n).into();
                    event["error_message_summary"] = "synthetic observation ".repeat(10).into();
                    event
                })
                .collect::<Vec<_>>()
                .into();
            row["events_observed"] = 128.into();
            if i > 0 {
                file.write_all(b",").unwrap();
            }
            serde_json::to_writer(&mut file, &row).unwrap();
        }
        file.write_all(b"]}").unwrap();
        file.flush().unwrap();
    }
    for mode in [
        "previous_stream_hot",
        "cold_restore",
        "cold_plus_100_materialized",
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "persistence::stream::tests::active_benchmark::active_restore_residency_memory_benchmark", "--ignored", "--nocapture"])
            .env("WEBCODEX_TEST_ACTIVE_RESIDENCY_MODE", mode)
            .env("WEBCODEX_TEST_ACTIVE_RESIDENCY_PATH", &path)
            .stdin(Stdio::null()).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}
