use super::*;

#[tokio::test]
async fn cancelled_stop_retains_the_owned_task_for_drop_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let guard = acquire_guard(temp.path(), "cancelled-stop").unwrap();
    let (stop, _rx) = oneshot::channel();
    // A task that cannot complete before this owner observes it or aborts it.
    let task = tokio::spawn(std::future::pending::<Result<(), Error>>());
    let abort = task.abort_handle();
    let mut tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(stop),
        health: Health::default(),
        guard: guard.clone(),
    };
    tokio::select! {
        biased;
        _ = tunnel.stop() => panic!("pending task completed"),
        _ = std::future::ready(()) => {},
    }
    assert!(
        tunnel.task.is_some(),
        "cancelling stop must not detach the owned task"
    );
    drop(tunnel);
    tokio::time::timeout(Duration::from_secs(5), async {
        while !abort.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(
        guard.exists(),
        "unobserved shutdown must retain the restart fence"
    );
}

#[test]
fn tunnel_id_is_a_fixed_validated_binding() {
    assert!(valid_tunnel_id("tunnel_0123456789abcdef0123456789abcdef"));
    for invalid in [
        "",
        "tunnel_x",
        "tunnel_0123456789ABCDEF0123456789ABCDEF",
        "../other",
    ] {
        assert!(!valid_tunnel_id(invalid));
    }
}
#[test]
fn restart_guard_blocks_concurrent_owner_and_unclean_restart() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("runs");
    let marker = acquire_guard(&root, "tunnel_fixture").unwrap();
    assert_eq!(
        acquire_guard(&root, "tunnel_fixture").unwrap_err().code,
        "tunnel_restart_uncertain"
    );
    assert!(acquire_guard(&root, "tunnel_another").is_ok());
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "native-tunnel-run-v1\n"
    );
    // Only an observed clean stop or explicit operator resolution clears this latch.
    std::fs::remove_file(&marker).unwrap();
    assert!(acquire_guard(&root, "tunnel_fixture").is_ok());
}
#[test]
fn diagnostics_do_not_contain_authority_material() {
    for error in [Error::Authentication, Error::Uncertain, Error::Transport] {
        let product = tunnel_error(error);
        assert!(!product.message.contains("Bearer"));
        assert!(!product.message.contains("https://"));
    }
}

// Dedicated adapter lifecycle fixture. The crate's domain tests own wire behavior.
async fn idle_tunnel(root: &Path) -> OpenAiTunnel {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = TunnelClient::new(
        ControlPlaneIdentity::new(
            &format!("http://{}", listener.local_addr().unwrap()),
            "fixture",
            Credential::bearer("fixture-control").unwrap(),
        )
        .unwrap(),
        FixedMcpTarget::new(
            "http://127.0.0.1:9/mcp",
            Credential::bearer("fixture-local").unwrap(),
        )
        .unwrap(),
        DeadlinePolicy::default(),
        Limits::default(),
    )
    .unwrap();
    let guard = acquire_guard(root, "fixture").unwrap();
    let health = client.health();
    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(client.run(async {
        let _ = rx.await;
    }));
    let mut tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(tx),
        health,
        guard,
    };
    let observed = tokio::time::timeout(Duration::from_secs(5), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 8192];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        socket
            .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        while !tunnel.health.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await;
    if observed.is_err() {
        tunnel.stop().await;
        panic!("fixture did not reach poll readiness");
    }
    tunnel
}

#[tokio::test]
async fn observed_idle_shutdown_clears_restart_marker() {
    let temp = tempfile::tempdir().unwrap();
    let mut tunnel = idle_tunnel(temp.path()).await;
    let marker = tunnel.guard.clone();
    tunnel.stop().await;
    assert!(!marker.exists());
    assert!(!tunnel.health.is_ready());
}
#[tokio::test]
async fn dropping_owner_retains_restart_marker() {
    let temp = tempfile::tempdir().unwrap();
    let tunnel = idle_tunnel(temp.path()).await;
    let marker = tunnel.guard.clone();
    drop(tunnel);
    assert!(marker.exists());
    assert_eq!(
        acquire_guard(temp.path(), "fixture").unwrap_err().code,
        "tunnel_restart_uncertain"
    );
}
