use super::*;
use crate::process::CommandOutputExt;
use std::process::Stdio;

#[test]
fn capture_preserves_stdin_eof_success_and_output_limit() {
    let output = Command::new("cmd.exe")
        .args(["/D", "/C", "echo output & echo error 1>&2"])
        .output_with_limits(Duration::from_secs(5), 1024)
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("output"));
    assert!(String::from_utf8_lossy(&output.stderr).contains("error"));
    assert!(Command::new("cmd.exe")
        .args(["/D", "/C", "echo 123456789"])
        .output_with_limits(Duration::from_secs(5), 4)
        .is_err());
}

#[test]
#[ignore = "owned native subprocess fixture, invoked by exited_leader_pipe_timeout"]
fn pipe_holder() {
    match std::env::var("WEBCODEX_TEST_PIPE_HOLDER").as_deref() {
        Ok("leader") => {
            let child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "process::windows::tests::pipe_holder",
                    "--ignored",
                    "--nocapture",
                ])
                .env("WEBCODEX_TEST_PIPE_HOLDER", "descendant")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            std::fs::write(
                std::env::var_os("WEBCODEX_TEST_PID_FILE").unwrap(),
                child.id().to_string(),
            )
            .unwrap();
            std::process::exit(0);
        }
        Ok("descendant") => std::thread::sleep(Duration::from_secs(30)),
        _ => panic!("fixture requires its exact role"),
    }
}

#[test]
fn exited_leader_pipe_timeout_closes_the_owned_job() {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("owned.pid");
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "process::windows::tests::pipe_holder",
            "--ignored",
            "--nocapture",
        ])
        .env("WEBCODEX_TEST_PIPE_HOLDER", "leader")
        .env("WEBCODEX_TEST_PID_FILE", &pid_file)
        .output_with_limits(Duration::from_secs(5), 4096);
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
    let pid = std::fs::read_to_string(pid_file)
        .unwrap()
        .parse::<u32>()
        .unwrap();
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if !handle.is_null() {
        let until = Instant::now() + Duration::from_secs(2);
        let mut active = true;
        while Instant::now() < until {
            let mut code = 0;
            if unsafe { GetExitCodeProcess(handle, &mut code) } != 0 && code != STILL_ACTIVE as u32
            {
                active = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        unsafe { CloseHandle(handle) };
        assert!(!active, "owned descendant survived the job guard");
    }
}
