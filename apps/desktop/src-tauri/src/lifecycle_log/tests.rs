use super::*;
use crate::activity::{ActivityEventKind, ActivityLevel, ActivityLog};
use crate::process::{ProcessKey, ProcessPhase};
use serde_json::Value;
use std::time::Duration;

fn temp() -> tempfile::TempDir {
    tempfile::Builder::new()
        .tempdir_in(std::env::temp_dir().canonicalize().unwrap())
        .unwrap()
}

fn barrier(log: &LifecycleLog) {
    let (tx, rx) = mpsc::sync_channel(1);
    log.sender.send(Message::Barrier(tx)).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
}

fn record(event: LifecycleEvent) -> Record {
    Record {
        version: 1,
        session_id: uuid::Uuid::new_v4().to_string(),
        desktop_pid: 42,
        timestamp_ms: 123,
        dropped_before: 0,
        event,
    }
}

#[test]
fn restart_retains_previous_session_without_inventing_exit_or_recovery() {
    let temp = temp();
    let first = LifecycleLog::start(temp.path()).unwrap();
    first.record(LifecycleEvent::ShutdownStarted);
    barrier(&first);
    let first_id = first.session_id;
    drop(first);
    let second = LifecycleLog::start(temp.path()).unwrap();
    barrier(&second);
    let text = fs::read_to_string(temp.path().join(DIRECTORY).join(CURRENT)).unwrap();
    let records: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["session_id"], first_id.to_string());
    assert_eq!(records[1]["event"], "shutdown_started");
    assert_eq!(records[2]["session_id"], second.session_id.to_string());
    assert_eq!(records[2]["event"], "desktop_started");
    assert!(!text.contains("recovery"));
    assert!(!text.contains("shutdown_wait_finished"));
}

#[test]
fn activity_messages_and_arbitrary_output_cannot_enter_metadata_records() {
    let temp = temp();
    let activity = ActivityLog::with_lifecycle(temp.path());
    activity.push(ActivityEventKind::ProcessExited, "private-source-canary", ActivityLevel::Error,
        "private-output-canary password=private-password-canary Authorization: Bearer private-token-canary");
    activity.record_lifecycle(LifecycleEvent::ProcessObserved {
        snapshot: ProcessSnapshot {
            kind: ProcessKey::RegularTunnel(crate::connection_id::TunnelProfileId::new()),
            generation: 7,
            phase: ProcessPhase::Failed,
            pid: Some(321),
            exit_code: Some(23),
            owned_by_desktop: true,
        },
    });
    barrier(activity.lifecycle.as_ref().unwrap());
    let text = fs::read_to_string(temp.path().join(DIRECTORY).join(CURRENT)).unwrap();
    assert!(!text.contains("private-"));
    let rows: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["snapshot"]["exit_code"], 23);
    assert_eq!(rows[1]["snapshot"]["generation"], 7);
    assert!(rows[1].get("message").is_none());
    assert!(rows[1].get("stdout").is_none());
}

#[test]
fn rotation_has_two_bounded_files_and_keeps_latest_records() {
    let temp = temp();
    let mut writer = Writer::open(temp.path()).unwrap();
    for generation in 0..3000 {
        writer
            .append(&record(LifecycleEvent::ProcessObserved {
                snapshot: ProcessSnapshot {
                    kind: ProcessKey::LocalRunner,
                    generation,
                    phase: ProcessPhase::Running,
                    pid: Some(10),
                    exit_code: None,
                    owned_by_desktop: true,
                },
            }))
            .unwrap();
    }
    let files: Vec<_> = fs::read_dir(temp.path().join(DIRECTORY))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(files.len(), 2);
    for file in files {
        assert!(file.metadata().unwrap().len() <= FILE_BYTES);
    }
    let text = fs::read_to_string(temp.path().join(DIRECTORY).join(CURRENT)).unwrap();
    let last: Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["snapshot"]["generation"], 2999);
}

#[test]
fn partial_last_line_does_not_swallow_the_next_session_record() {
    let temp = temp();
    let directory = temp.path().join(DIRECTORY);
    let writer = Writer::open(temp.path()).unwrap();
    drop(writer);
    fs::write(directory.join(CURRENT), b"{partial").unwrap();
    let mut writer = Writer::open(temp.path()).unwrap();
    writer
        .append(&record(LifecycleEvent::DesktopStarted))
        .unwrap();
    let text = fs::read_to_string(directory.join(CURRENT)).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("{partial"));
    let next: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(next["event"], "desktop_started");
}

#[test]
fn full_queue_never_waits_for_a_blocked_writer_and_reports_dropped_records() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let log = LifecycleLog {
        sender,
        session_id: uuid::Uuid::new_v4(),
        dropped: Arc::new(AtomicU64::new(0)),
    };
    log.record(LifecycleEvent::DesktopStarted);
    log.record(LifecycleEvent::ShutdownStarted);
    log.record(LifecycleEvent::ShutdownWaitFinished);
    assert_eq!(log.dropped.load(Ordering::Relaxed), 2);
    receiver.try_recv().unwrap();
    log.record(LifecycleEvent::DesktopExiting);
    let Message::Record(record) = receiver.try_recv().unwrap() else {
        panic!("expected record")
    };
    assert_eq!(record.dropped_before, 2);
}

#[test]
fn unsafe_log_targets_are_rejected_without_modifying_them() {
    let temp = temp();
    let writer = Writer::open(temp.path()).unwrap();
    drop(writer);
    let target = temp.path().join("original");
    fs::write(&target, b"original-canary").unwrap();
    let current = temp.path().join(DIRECTORY).join(CURRENT);
    fs::remove_file(&current).unwrap();
    fs::hard_link(&target, &current).unwrap();
    assert!(Writer::open(temp.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"original-canary");
    fs::remove_file(&current).unwrap();
    fs::create_dir(&current).unwrap();
    assert!(Writer::open(temp.path()).is_err());
}

#[cfg(unix)]
#[test]
fn journal_rejects_symlinks_and_uses_private_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let temp = temp();
    let writer = Writer::open(temp.path()).unwrap();
    drop(writer);
    let directory = temp.path().join(DIRECTORY);
    assert_eq!(
        fs::metadata(&directory).unwrap().permissions().mode() & 0o777,
        0o700
    );
    let current = directory.join(CURRENT);
    assert_eq!(
        fs::metadata(&current).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::remove_file(&current).unwrap();
    symlink(temp.path().join("target"), &current).unwrap();
    assert!(Writer::open(temp.path()).is_err());
}
