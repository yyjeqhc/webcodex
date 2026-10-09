//! Isolated native messages and real children; never request an OS shutdown.
use super::*;
use crate::activity::ActivityLog;
use crate::connection_id::TunnelProfileId;
use crate::deadline::Deadline;
use crate::process::{ProcessKey, ProcessSupervisor};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use webcodex_process::ManagedChild;

const CHILD_TEST: &str = "platform::windows::session_shutdown::tests::session_shutdown_child";
const EOF_TEST: &str = "platform::windows::session_shutdown::tests::session_shutdown_eof_child";
const ROOT: &str = "WEBCODEX_SESSION_SHUTDOWN_TEST_ROOT";

#[test]
#[ignore = "Desktop Windows real-process lane: isolated native session notifications"]
fn desktop_real_process_windows_session_shutdown_drains_all_profiles() {
    let root = tempfile::tempdir().unwrap();
    let output = std::fs::File::create(root.path().join("child.log")).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD_TEST, "--ignored", "--nocapture"])
        .env(ROOT, root.path())
        .stdin(Stdio::null())
        .stdout(Stdio::from(output.try_clone().unwrap()))
        .stderr(Stdio::from(output));
    let mut child = ManagedChild::spawn_with_options(
        &mut command,
        crate::platform::managed_spawn_options(false),
    )
    .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if std::time::Instant::now() >= deadline {
            child.terminate_tree().unwrap();
            panic!("session shutdown fixture timed out");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(root.path().join("child.log")).unwrap()
    );
    // Job Object descendant accounting may settle just after the fixture's
    // direct child exits. Wait a bounded interval instead of racing the
    // final kernel notification.
    let tree_deadline = std::time::Instant::now() + Duration::from_secs(2);
    while !child.try_tree_exit().unwrap() {
        if std::time::Instant::now() >= tree_deadline {
            let _ = child.terminate_tree();
            panic!("owned fixture descendants survived");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        std::fs::read_to_string(root.path().join("passed")).unwrap(),
        "passed"
    );
}

#[test]
#[ignore = "child fixture, run only by the isolated native lane"]
fn session_shutdown_child() {
    let Some(root) = std::env::var_os(ROOT).map(std::path::PathBuf::from) else {
        return;
    };
    let runtime = Arc::new(tokio::runtime::Runtime::new().unwrap());
    let supervisor = Arc::new(tokio::sync::Mutex::new(ProcessSupervisor::new(
        ActivityLog::default(),
    )));
    runtime.block_on(async {
        let mut owner = supervisor.lock().await;
        for index in 0..4 {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", EOF_TEST, "--ignored", "--nocapture"])
                .env(ROOT, &root)
                .env("WEBCODEX_SESSION_SHUTDOWN_TEST_INDEX", index.to_string());
            let mut events = owner
                .spawn_owned(
                    ProcessKey::RegularTunnel(TunnelProfileId::new()),
                    command,
                    true,
                )
                .await
                .unwrap()
                .unwrap();
            tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .unwrap()
                .unwrap();
        }
        let mut server = Command::new(std::env::current_exe().unwrap());
        server
            .args(["--exact", EOF_TEST, "--ignored", "--nocapture"])
            .env(ROOT, &root)
            .env("WEBCODEX_SESSION_SHUTDOWN_TEST_INDEX", "server");
        let mut events = owner
            .spawn_owned(ProcessKey::LocalServer, server, true)
            .await
            .unwrap()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
    });
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = Arc::clone(&calls);
    let callback_runtime = Arc::clone(&runtime);
    let callback_owner = Arc::clone(&supervisor);
    let observer = SessionShutdownObserver::start(move || {
        callback_calls.fetch_add(1, Ordering::SeqCst);
        callback_runtime.block_on(async {
            callback_owner
                .lock()
                .await
                .stop_all_until(Deadline::after(Duration::from_secs(4)))
                .await;
        });
    })
    .unwrap();
    let window = *observer.window.lock().unwrap() as HWND;
    let mut level = 0;
    let mut flags = 0;
    assert_ne!(
        unsafe {
            windows_sys::Win32::System::Threading::GetProcessShutdownParameters(
                &mut level, &mut flags,
            )
        },
        0
    );
    assert_eq!(level, OWNER_SHUTDOWN_LEVEL);
    // Query and cancellation are not permission to tear down live connections.
    assert_eq!(unsafe { SendMessageW(window, WM_QUERYENDSESSION, 0, 0) }, 1);
    unsafe { SendMessageW(window, WM_ENDSESSION, 0, 0) };
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        runtime.block_on(async { supervisor.lock().await.keys().len() }),
        5
    );
    let started = std::time::Instant::now();
    unsafe { SendMessageW(window, WM_ENDSESSION, 1, 0) };
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    for index in 0..4 {
        assert!(
            root.join(format!("settled-{index}")).is_file(),
            "profile {index} was killed before all peers got EOF"
        );
    }
    assert!(
        root.join("settled-server").is_file(),
        "shared Server must stay alive until all profiles settle"
    );
    assert!(runtime.block_on(async { supervisor.lock().await.keys().is_empty() }));
    assert!(started.elapsed() < Duration::from_secs(5));
    unsafe { SendMessageW(window, WM_ENDSESSION, 1, ENDSESSION_LOGOFF as isize) };
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    observer.close();
    std::fs::write(root.join("passed"), "passed").unwrap();
}

#[test]
#[ignore = "child fixture, reads only its own stdin lease"]
fn session_shutdown_eof_child() {
    let Some(root) = std::env::var_os(ROOT).map(std::path::PathBuf::from) else {
        return;
    };
    let index = std::env::var("WEBCODEX_SESSION_SHUTDOWN_TEST_INDEX").unwrap();
    println!("{{\"event\":\"ready\"}}");
    let mut input = Vec::new();
    std::io::stdin().read_to_end(&mut input).unwrap();
    std::fs::write(root.join(format!("eof-{index}")), "eof").unwrap();
    if index == "server" {
        assert!(
            (0..4).all(|peer| root.join(format!("settled-{peer}")).is_file()),
            "shared Server stopped before exposure cleanup"
        );
        std::fs::write(root.join("settled-server"), "settled").unwrap();
        return;
    }
    // Every profile waits for all four leases, exposing the old serial stop
    // regardless of HashMap order. No arbitrary delay/retry hides the bug.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !(0..4).all(|peer| root.join(format!("eof-{peer}")).is_file()) {
        assert!(
            std::time::Instant::now() < deadline,
            "peer EOF never arrived"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    std::fs::write(root.join(format!("settled-{index}")), "settled").unwrap();
}
