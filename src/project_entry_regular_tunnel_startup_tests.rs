//! Parent-stdin lifecycle fixture. Only loopback probes and synthetic credentials.
use super::*;
use std::process::{Command, Stdio};
use webcodex_process::{ManagedChild, SpawnOptions};

const CHILD: &str =
    "project_entry::regular_tunnel_service::startup_tests::parent_eof_startup_child";
const ROOT: &str = "WEBCODEX_REGULAR_TUNNEL_STARTUP_TEST_ROOT";
const URL: &str = "WEBCODEX_REGULAR_TUNNEL_STARTUP_TEST_URL";

#[tokio::test]
#[ignore = "real-process lane: managed tunnel stdin EOF during startup"]
async fn regular_tunnel_real_process_parent_eof_during_local_probe() {
    use tokio::io::AsyncReadExt;
    let root = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let output = std::fs::File::create(root.path().join("child.log")).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", CHILD, "--ignored", "--nocapture"])
        .env(ROOT, root.path())
        .env(URL, format!("http://{}", listener.local_addr().unwrap()))
        .env("HOME", root.path())
        .env("XDG_STATE_HOME", root.path().join("state"))
        .env(
            "CONTROL_PLANE_TUNNEL_ID",
            "tunnel_0123456789abcdef0123456789abcdef",
        )
        .env("CONTROL_PLANE_API_KEY", "fixture-control")
        .stdin(Stdio::piped())
        .stdout(Stdio::from(output.try_clone().unwrap()))
        .stderr(Stdio::from(output));
    let mut child = ManagedChild::spawn_with_options(
        &mut command,
        SpawnOptions {
            windows_creation_flags: 0x08000000, // CREATE_NO_WINDOW, ignored on Unix.
            ..SpawnOptions::default()
        },
    )
    .unwrap();
    let _held_probe = tokio::time::timeout(Duration::from_secs(5), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        assert!(socket.read(&mut [0; 8192]).await.unwrap() > 0);
        socket // Keep the local readiness probe pending; never contact OpenAI.
    })
    .await
    .expect("child did not begin local readiness probe");
    drop(child.child_mut().stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            child.terminate_tree().unwrap();
            panic!("parent EOF did not settle startup");
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(
        status.success(),
        "{}",
        std::fs::read_to_string(root.path().join("child.log")).unwrap()
    );
    assert!(child.wait_tree_exit(Duration::from_secs(2)).unwrap());
    assert!(root.path().join("passed").is_file());
}

#[tokio::test]
#[ignore = "child fixture, invoked only by the managed startup lane"]
async fn parent_eof_startup_child() {
    let Some(root) = std::env::var_os(ROOT).map(PathBuf::from) else {
        return;
    };
    let options = RegularServerTunnelOptions {
        local_server_url: std::env::var(URL).unwrap(),
        bootstrap_token: "fixture-local".into(),
        runtime_parent: root.clone(),
        stop_on_stdin_eof: true,
    };
    run_regular_server_tunnel_with_stop(&options, std::future::pending())
        .await
        .expect("startup EOF should be a clean stop, not a readiness failure");
    std::fs::write(root.join("passed"), "passed").unwrap();
}
