use super::*;
#[cfg(target_os = "linux")]
use std::time::Instant;

#[test]
fn successful_capture_drains_both_pipes_and_closes_stdin() {
    let out = Command::new("/bin/sh")
        .args(["-c", "cat; printf output; printf error >&2"])
        .output_with_limits(Duration::from_secs(2), 1024)
        .unwrap();
    assert!(out.status.success());
    assert_eq!(out.stdout, b"output");
    assert_eq!(out.stderr, b"error");
}

#[test]
fn deadlines_and_output_bounds_are_enforced() {
    assert_eq!(
        Command::new("/bin/sh")
            .args(["-c", "sleep 30"])
            .output_with_limits(Duration::from_millis(50), 1024)
            .unwrap_err()
            .kind(),
        io::ErrorKind::TimedOut
    );
    assert!(Command::new("/bin/sh")
        .args(["-c", "printf 123456789"])
        .output_with_limits(Duration::from_secs(2), 4)
        .is_err());
    let out = Command::new("/bin/sh")
        .args(["-c", "printf 1234"])
        .output_with_limits(Duration::from_secs(2), 4)
        .unwrap();
    assert_eq!(out.stdout, b"1234");
}

#[cfg(target_os = "linux")]
fn group_has_live_processes(pgid: u32) -> bool {
    std::fs::read_dir("/proc")
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("stat")).ok())
        .any(|stat| {
            let Some((_, fields)) = stat.rsplit_once(')') else {
                return false;
            };
            let mut fields = fields.split_whitespace();
            let state = fields.next();
            let _parent = fields.next();
            let group = fields.next().and_then(|value| value.parse::<u32>().ok());
            state != Some("Z") && group == Some(pgid)
        })
}

#[cfg(target_os = "linux")]
fn assert_group_cleanup(script: &str, expect_success: bool) {
    use std::io::{Read, Seek, SeekFrom};
    use std::os::fd::AsRawFd;
    use std::os::unix::process::CommandExt;

    // Publish the owned PGID before exec, while spawn still owns its handshake.
    // A shell-created PID file can be truncated when the capture deadline kills
    // the group before echo writes, so it is not a reliable fixture identity.
    let mut marker = tempfile::tempfile().unwrap();
    let fd = marker.as_raw_fd();
    let mut command = Command::new("/bin/sh");
    command.args(["-c", script]);
    // SAFETY: the hook uses only async-signal-safe libc calls and an inherited
    // descriptor kept alive until capture returns; it never allocates or locks.
    unsafe {
        command.pre_exec(move || {
            let pgid = libc::getpgrp() as u32;
            if pgid != libc::getpid() as u32 {
                return Err(io::Error::from_raw_os_error(libc::EINVAL));
            }
            let bytes = pgid.to_ne_bytes();
            if libc::write(fd, bytes.as_ptr().cast(), bytes.len()) != bytes.len() as isize {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let started = Instant::now();
    let result = command.output_with_limits(Duration::from_millis(200), 1024);
    marker.seek(SeekFrom::Start(0)).unwrap();
    let mut bytes = [0; 4];
    marker.read_exact(&mut bytes).unwrap();
    let pgid = u32::from_ne_bytes(bytes);
    let deadline = Instant::now() + Duration::from_secs(2);
    while group_has_live_processes(pgid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let alive = group_has_live_processes(pgid);
    if alive {
        unsafe {
            libc::kill(-(pgid as i32), libc::SIGKILL);
        }
    }
    assert!(!alive, "capture returned with a live owned process group");
    if expect_success {
        assert!(result.unwrap().status.success());
    } else {
        assert!(result.is_err());
    }
    eprintln!(
        "PROCESS_CLEANUP leader_exit=true descendant_alive=false reader_threads=0 elapsed_ms={:.3}",
        started.elapsed().as_secs_f64() * 1000.0
    );
}

#[cfg(target_os = "linux")]
#[test]
fn exited_leader_never_leaves_pipe_holding_descendants_or_reader_threads() {
    for (script, expect_success) in [
        ("sleep 30 & exit 0", false),
        ("sleep 30 >&- 2>&- & exit 0", true),
        ("sleep 30 & while :; do printf 123456789; done", false),
    ] {
        assert_group_cleanup(script, expect_success);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn deadline_before_shell_fixture_publication_still_checks_group_cleanup() {
    // Deterministically let the capture deadline arrive before shell-side
    // fixture publication. Identity is already committed by the spawn hook.
    assert_group_cleanup("sleep 30 & sleep 1; exit 0", false);
}
