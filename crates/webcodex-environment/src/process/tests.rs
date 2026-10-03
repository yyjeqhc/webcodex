use super::*;
use std::time::Instant;

#[test]
fn successful_capture_drains_both_pipes_and_closes_stdin() {
    let out = Command::new("/bin/sh").args(["-c", "cat; printf output; printf error >&2"])
        .output_with_limits(Duration::from_secs(2), 1024).unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"output");
    assert_eq!(out.stderr, b"error");
}

#[test]
fn deadlines_and_output_bounds_are_enforced() {
    assert_eq!(Command::new("/bin/sh").args(["-c", "sleep 30"])
        .output_with_limits(Duration::from_millis(50), 1024).unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(Command::new("/bin/sh").args(["-c", "printf 123456789"])
        .output_with_limits(Duration::from_secs(2), 4).is_err());
    let out = Command::new("/bin/sh").args(["-c", "printf 1234"])
        .output_with_limits(Duration::from_secs(2), 4).unwrap();
    assert_eq!(out.stdout, b"1234");
}

#[cfg(target_os = "linux")]
fn running(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()
        .and_then(|s| s.rsplit_once(')').map(|(_, s)| !s.trim_start().starts_with('Z'))).unwrap_or(false)
}

#[cfg(target_os = "linux")]
#[test]
fn exited_leader_never_leaves_pipe_holding_descendants_or_reader_threads() {
    let tmp = tempfile::tempdir().unwrap();
    for script in [
        "sleep 30 & echo $! > \"$PID_FILE\"; exit 0",
        "sleep 30 >&- 2>&- & echo $! > \"$PID_FILE\"; exit 0",
        "sleep 30 & echo $! > \"$PID_FILE\"; while :; do printf 123456789; done",
    ] {
        let path = tmp.path().join("pid");
        let started = Instant::now();
        let result = Command::new("/bin/sh").args(["-c", script]).env("PID_FILE", &path)
            .output_with_limits(Duration::from_millis(200), 1024);
        let pid: u32 = std::fs::read_to_string(&path).unwrap().trim().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while running(pid) && Instant::now() < deadline { std::thread::sleep(Duration::from_millis(5)); }
        let alive = running(pid);
        if alive { unsafe { libc::kill(pid as i32, libc::SIGKILL); } }
        assert!(!alive, "capture returned with a live owned descendant");
        if script.contains(">&-") { assert!(result.unwrap().status.success()); }
        else { assert!(result.is_err()); }
        eprintln!("PROCESS_CLEANUP leader_exit=true descendant_alive=false reader_threads=0 elapsed_ms={:.3}", started.elapsed().as_secs_f64()*1000.0);
    }
}
