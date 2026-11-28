//! Real Windows host lifecycle regression. Only fixture processes are terminated.
use super::{open_parent_process, profiles, protocol, run, ParentProcess, Rendezvous};
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle, RawHandle},
    process::{Child, Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
    System::Threading::{
        OpenProcess, TerminateProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    },
};

const CASE: &str =
    "bridge::native::tests::native_host_exits_when_parent_exits_with_input_still_open";
const ROLE: &str = "WEBCODEX_NATIVE_HOST_LIFETIME_FIXTURE";

struct ChildCleanup(Child);
impl ChildCleanup {
    fn stop(&mut self) -> std::io::Result<()> {
        if self.0.try_wait()?.is_some() {
            return Ok(());
        }
        let _ = self.0.kill();
        let state = unsafe { WaitForSingleObject(self.0.as_raw_handle(), 5_000) };
        if state != WAIT_OBJECT_0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!("fixture cleanup did not complete: wait result {state}"),
            ));
        }
        self.0.try_wait()?;
        Ok(())
    }
}
impl Drop for ChildCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            cleanup_failure(&error.to_string());
        }
    }
}

fn cleanup_failure(message: &str) {
    if thread::panicking() {
        eprintln!("fixture cleanup failed during unwinding: {message}");
    } else {
        panic!("fixture cleanup failed: {message}");
    }
}

// This handle is opened for a PID reported by our still-live fixture parent,
// before that parent exits. Cleanup never looks up a PID again.
struct HostCleanup(OwnedHandle);
impl Drop for HostCleanup {
    fn drop(&mut self) {
        let handle = self.0.as_raw_handle();
        unsafe {
            if WaitForSingleObject(handle, 0) != WAIT_OBJECT_0 {
                TerminateProcess(handle, 99);
                let state = WaitForSingleObject(handle, 5_000);
                if state != WAIT_OBJECT_0 {
                    cleanup_failure(&format!("native host wait result {state}"));
                }
            }
        }
    }
}

fn fixture_command(role: &str) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CASE, "--ignored", "--nocapture"])
        .env(ROLE, role)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                // Windows accepted sockets inherit the listener's mode.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "fixture host did not connect");
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
}

#[test]
fn invalid_parent_process_fails_closed() {
    assert!(open_parent_process(0).is_err());
}

#[test]
#[ignore = "spawns an owned Windows process; run explicitly"]
fn fixed_parent_handle_detects_exit() {
    let mut child = ChildCleanup(
        fixture_command("holder")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let parent = ParentProcess {
        pid: child.0.id(),
        process: child.0.as_handle().try_clone_to_owned().unwrap(),
    };
    assert!(parent.ensure_alive().is_ok());
    child.stop().unwrap();
    assert!(
        parent.ensure_alive().is_err(),
        "an exited parent's retained handle must not authorize more work"
    );
}

#[test]
#[ignore = "spawns an owned Windows process; run explicitly"]
fn newer_process_cannot_substitute_for_parent() {
    let child = ChildCleanup(
        fixture_command("holder")
            .stdin(Stdio::null())
            .spawn()
            .unwrap(),
    );
    assert!(
        open_parent_process(child.0.id()).is_err(),
        "a process created after this host cannot be its original parent"
    );
}

#[test]
#[ignore = "spawns native Windows host and parent fixtures; run explicitly"]
fn native_host_exits_when_parent_exits_with_input_still_open() {
    match std::env::var(ROLE).as_deref() {
        Ok("holder") => {
            thread::sleep(Duration::from_secs(20));
            return;
        }
        Ok("host") => {
            let result = run(&[format!("chrome-extension://{}/", protocol::EXTENSION_ID)]);
            std::process::exit(i32::from(result.is_ok()));
        }
        Ok("parent") => {
            let child = fixture_command("host")
                .stdin(Stdio::inherit())
                .spawn()
                .unwrap();
            let _cleanup = ChildCleanup(child);
            println!("fixture-host-pid:{}", _cleanup.0.id());
            std::io::stdout().flush().unwrap();
            // The test terminates this exact parent after the host handshake.
            // This fallback bounds the helper even if the outer test fails.
            thread::sleep(Duration::from_secs(20));
            return;
        }
        _ => {}
    }

    let temporary = tempfile::tempdir().unwrap();
    let local_appdata = temporary.path().canonicalize().unwrap();
    let record = local_appdata.join("webcodex/browser/extension/bridge.json");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let config = Rendezvous {
        version: protocol::VERSION,
        address: listener.local_addr().unwrap(),
        instance: uuid::Uuid::new_v4().simple().to_string(),
        token: format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        ),
    };
    let mut file = profiles::private_file(&record, true).unwrap();
    serde_json::to_writer(&mut file, &config).unwrap();
    file.flush().unwrap();
    drop(file);

    let mut parent = ChildCleanup(
        fixture_command("parent")
            .env("LOCALAPPDATA", &local_appdata)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    // The host inherits the read end. Keeping our write end alive rules out
    // stdin EOF as the reason for its exit after the fixture parent terminates.
    let _keep_stdin_open = parent.0.stdin.take().unwrap();
    let stdout = parent.0.stdout.take().unwrap();
    let (sender, receiver) = mpsc::channel();
    let output = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some((_, pid)) = line.split_once("fixture-host-pid:") {
                let _ = sender.send(pid.trim().parse::<u32>().unwrap());
                break;
            }
        }
    });
    let host_pid = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("fixture parent did not report its host");
    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
            0,
            host_pid,
        )
    };
    assert!(
        !handle.is_null(),
        "could not bind exact fixture host handle"
    );
    let handle = unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) };
    // The parent retains its Child handle until it exits, preventing PID reuse.
    // Do not grant cleanup/termination ownership if the parent already left.
    assert!(
        parent.0.try_wait().unwrap().is_none(),
        "fixture parent exited before host identity was bound"
    );
    let host = HostCleanup(handle);

    let mut stream = accept(&listener);
    let hello = protocol::read_blocking(&mut stream, 4096, false).unwrap();
    assert_eq!(hello["parent_pid"].as_u64(), Some(parent.0.id() as u64));
    assert_eq!(hello["instance"], config.instance);
    assert_eq!(hello["token"], config.token);
    protocol::write_frame(
        &mut stream,
        &serde_json::json!({"kind":"ready", "version":protocol::VERSION}),
        4096,
        false,
    )
    .unwrap();

    output.join().unwrap();
    assert_eq!(
        unsafe { WaitForSingleObject(host.0.as_raw_handle(), 0) },
        WAIT_TIMEOUT,
        "fixture host exited before its parent"
    );
    parent.stop().unwrap();
    // Both pipes and the authenticated socket are still open. The host must
    // observe the original parent's lifetime, not keep re-reading its fixed PID.
    let state = unsafe { WaitForSingleObject(host.0.as_raw_handle(), 5_000) };
    assert_eq!(
        state, WAIT_OBJECT_0,
        "native host outlived its parent while stdin and socket stayed open"
    );
}
