use crate::activity::ActivityLog;
use crate::process::{ProcessKind, ProcessSupervisor};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::process::{Child, Command};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn unique_marker(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "webcodex-desktop-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

async fn wait_for_file(path: &Path) {
    let deadline = tokio::time::Instant::now() + TEST_TIMEOUT;
    loop {
        if path.is_file() {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

fn process_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    let result = unsafe { libc::kill(pid, 0) };
    if result == 0 {
        return true;
    }
    !matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    )
}

async fn wait_for_process_exit(pid: u32) {
    let deadline = tokio::time::Instant::now() + TEST_TIMEOUT;
    loop {
        if !process_exists(pid) {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "PID {pid} did not exit before deadline"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

async fn stop_control(mut child: Child) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(TEST_TIMEOUT, child.wait()).await;
}

#[tokio::test]
async fn desktop_owned_group_kills_descendant_without_touching_unrelated_process() {
    let marker = unique_marker("process-group-descendant");
    let mut owned = Command::new("/bin/sh");
    owned
        .arg("-c")
        .arg("sleep 60 & descendant=$!; printf '%s\\n' \"$descendant\" > \"$1\"; wait \"$descendant\"")
        .arg("webcodex-owned-tree")
        .arg(&marker);

    let mut control = Command::new("/bin/sleep")
        .arg("60")
        .spawn()
        .expect("spawn unrelated control process");
    let control_pid = control.id().expect("control pid");

    let activity = ActivityLog::default();
    let mut supervisor = ProcessSupervisor::new(activity);
    supervisor
        .spawn_owned(ProcessKind::LocalServer, owned, false)
        .await
        .expect("spawn Desktop-owned process group");
    let root_pid = supervisor
        .snapshot(ProcessKind::LocalServer)
        .and_then(|snapshot| snapshot.pid)
        .expect("owned root pid");

    let root_pid_i32 = i32::try_from(root_pid).expect("root pid fits pid_t");
    assert_eq!(
        unsafe { libc::getpgid(root_pid_i32) },
        root_pid_i32,
        "spawn contract must establish pgid == root pid"
    );

    wait_for_file(&marker).await;
    let descendant_pid: u32 = std::fs::read_to_string(&marker)
        .expect("read descendant pid")
        .trim()
        .parse()
        .expect("parse descendant pid");
    assert!(process_exists(descendant_pid));
    assert!(process_exists(control_pid));

    supervisor.stop(ProcessKind::LocalServer).await;

    wait_for_process_exit(root_pid).await;
    wait_for_process_exit(descendant_pid).await;
    assert!(
        process_exists(control_pid),
        "unrelated process must survive owned-group stop"
    );
    assert!(control.try_wait().expect("observe control").is_none());

    stop_control(control).await;
    let _ = std::fs::remove_file(marker);
}

#[tokio::test]
async fn respawn_reclaims_descendants_from_a_terminal_previous_generation() {
    let marker = unique_marker("terminal-generation-descendant");
    let mut previous = Command::new("/bin/sh");
    previous
        .arg("-c")
        .arg("nohup sleep 60 >/dev/null 2>&1 & descendant=$!; printf '%s\\n' \"$descendant\" > \"$1\"; exit 0")
        .arg("webcodex-terminal-generation")
        .arg(&marker);

    let activity = ActivityLog::default();
    let mut supervisor = ProcessSupervisor::new(activity);
    supervisor
        .spawn_owned(ProcessKind::LocalServer, previous, false)
        .await
        .expect("spawn previous Desktop-owned generation");
    wait_for_file(&marker).await;
    let descendant_pid: u32 = std::fs::read_to_string(&marker)
        .expect("read previous descendant pid")
        .trim()
        .parse()
        .expect("parse previous descendant pid");
    assert!(process_exists(descendant_pid));

    let deadline = tokio::time::Instant::now() + TEST_TIMEOUT;
    loop {
        let terminal = supervisor
            .snapshot(ProcessKind::LocalServer)
            .is_some_and(|snapshot| {
                matches!(
                    snapshot.phase,
                    crate::process::ProcessPhase::Exited | crate::process::ProcessPhase::Failed
                )
            });
        if terminal {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "previous owned root did not become terminal before deadline"
        );
        tokio::time::sleep(POLL_INTERVAL).await;
    }

    let mut replacement = Command::new("/bin/sleep");
    replacement.arg("60");
    supervisor
        .spawn_owned(ProcessKind::LocalServer, replacement, false)
        .await
        .expect("spawn replacement Desktop-owned generation");

    wait_for_process_exit(descendant_pid).await;
    supervisor.stop(ProcessKind::LocalServer).await;
    let _ = std::fs::remove_file(marker);
}

#[tokio::test]
async fn regular_tunnel_stop_observes_stdin_eof_before_group_termination() {
    let marker = unique_marker("regular-tunnel-eof");
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg("cat >/dev/null; printf 'eof\\n' > \"$1\"")
        .arg("webcodex-tunnel-eof")
        .arg(&marker);

    let activity = ActivityLog::default();
    let mut supervisor = ProcessSupervisor::new(activity);
    supervisor
        .spawn_owned(ProcessKind::RegularTunnel, command, false)
        .await
        .expect("start EOF fixture");
    supervisor.stop(ProcessKind::RegularTunnel).await;

    assert!(
        marker.is_file(),
        "regular tunnel child must observe stdin EOF before the graceful stop completes"
    );
    let _ = std::fs::remove_file(marker);
}
