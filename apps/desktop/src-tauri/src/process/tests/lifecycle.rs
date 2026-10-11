//! A real child exit must remain inspectable after Desktop's memory is gone.
use crate::activity::ActivityLog;
use crate::lifecycle_log::{CURRENT, DIRECTORY};
use crate::process::{ProcessKey, ProcessPhase, ProcessSupervisor};
use std::process::Command;
use std::time::Duration;

#[tokio::test]
async fn owned_child_exit_code_survives_in_lifecycle_metadata() {
    let temp = tempfile::Builder::new()
        .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
        .unwrap();
    let activity = ActivityLog::with_lifecycle(temp.path());
    let log = activity.lifecycle.as_ref().unwrap().clone();
    let mut supervisor = ProcessSupervisor::new(activity);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args([
        "--ignored",
        "--exact",
        "process::tests::lifecycle::lifecycle_exit_code_child",
        "--nocapture",
    ]);
    command.env("WEBCODEX_LIFECYCLE_TEST_CHILD", "1");
    supervisor
        .spawn_owned(ProcessKey::LocalRunner, command, false)
        .await
        .unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let terminal = loop {
        let snapshot = supervisor.snapshot(ProcessKey::LocalRunner).unwrap();
        if snapshot.phase == ProcessPhase::Failed {
            break snapshot;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(terminal.exit_code, Some(23));
    supervisor.stop_all().await;
    log.test_barrier();
    drop(supervisor);
    let text = std::fs::read_to_string(temp.path().join(DIRECTORY).join(CURRENT)).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    for event in ["process_observed", "process_stopped"] {
        assert!(rows.iter().any(|row| row["event"] == event
            && row["snapshot"]["phase"] == "failed"
            && row["snapshot"]["exit_code"] == 23
            && row["snapshot"]["pid"] == terminal.pid.unwrap()
            && row["snapshot"]["generation"] == terminal.generation));
    }
}

#[test]
#[ignore = "disposable exit-code fixture invoked by the owned-process test"]
fn lifecycle_exit_code_child() {
    if std::env::var_os("WEBCODEX_LIFECYCLE_TEST_CHILD").is_some() {
        std::process::exit(23);
    }
}
