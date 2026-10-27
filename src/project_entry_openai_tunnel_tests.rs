use super::*;

fn prepared_client(
    root: &Path,
    credential: &str,
    recovery: OpenAiTunnelRecovery,
) -> Result<(TunnelClient, webcodex_openai_tunnel::run_fence::RunFence), ProductError> {
    let prerequisites = OpenAiTunnelPrerequisites::bound(
        "tunnel_0123456789abcdef0123456789abcdef".into(),
        "fixture-control".into(),
        ControlPlaneProxy::Direct,
    )?;
    OpenAiTunnel::prepare_client(
        &prerequisites,
        "http://127.0.0.1:8080/mcp",
        credential,
        root,
        recovery,
    )
}

#[test]
fn temporary_share_restarts_cleanly_with_a_fresh_local_credential() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    for credential in ["first-share", "second-share"] {
        let (client, guard) =
            prepared_client(&root, credential, OpenAiTunnelRecovery::TemporaryShare).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&std::fs::read(guard.path()).unwrap())
                .unwrap()["version"],
            2
        );
        drop(client);
        guard.finish().unwrap();
    }
}

#[test]
fn temporary_share_preserves_uncertain_and_protected_history() {
    for recovery in [
        OpenAiTunnelRecovery::TemporaryShare,
        OpenAiTunnelRecovery::Durable,
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let (client, guard) = prepared_client(&root, "old-credential", recovery).unwrap();
        let path = guard.path().to_owned();
        let saved = std::fs::read(&path).unwrap();
        drop(client);
        drop(guard);
        assert!(prepared_client(
            &root,
            "new-credential",
            OpenAiTunnelRecovery::TemporaryShare
        )
        .is_err());
        assert_eq!(std::fs::read(path).unwrap(), saved);
    }
}

#[test]
fn clean_durable_history_cannot_be_rebound_or_downgraded_for_a_share() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let (client, guard) =
        prepared_client(&root, "old-credential", OpenAiTunnelRecovery::Durable).unwrap();
    let path = guard.path().to_owned();
    drop(client);
    guard.finish().unwrap();
    drop(guard);
    let history = std::fs::read(path.with_extension("lock")).unwrap();
    for recovery in [
        OpenAiTunnelRecovery::TemporaryShare,
        OpenAiTunnelRecovery::Durable,
    ] {
        assert!(prepared_client(&root, "new-credential", recovery).is_err());
    }
    assert!(!path.exists());
    assert_eq!(std::fs::read(path.with_extension("lock")).unwrap(), history);
}

#[tokio::test]
async fn cancelled_stop_retains_the_owned_task_for_drop_cleanup() {
    let temp = tempfile::tempdir().unwrap();
    let guard = acquire_guard(&temp.path().canonicalize().unwrap(), "cancelled-stop").unwrap();
    let (stop, _rx) = oneshot::channel();
    // A task that cannot complete before this owner observes it or aborts it.
    let task = tokio::spawn(std::future::pending::<Result<(), Error>>());
    let abort = task.abort_handle();
    let marker_path = guard.path().to_path_buf();
    let mut tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(stop),
        health: Health::default(),
        guard,
        terminal: None,
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
        marker_path.exists(),
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
    let root = temp.path().canonicalize().unwrap().join("runs");
    let marker = acquire_guard(&root, "tunnel_fixture").unwrap();
    assert_eq!(
        acquire_guard(&root, "tunnel_fixture").unwrap_err().code,
        "tunnel_restart_uncertain"
    );
    assert!(acquire_guard(&root, "tunnel_another").is_ok());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(marker.path()).unwrap())
            .unwrap()["version"],
        2
    );
    // Only an observed clean stop or explicit operator resolution clears this latch.
    marker.finish().unwrap();
    drop(marker);
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

#[test]
fn automatic_recovery_never_treats_the_live_host_or_zero_pid_as_absent() {
    let mut identity = webcodex_openai_tunnel::run_fence::RunIdentity {
        version: 3,
        run_id: uuid::Uuid::new_v4().to_string(),
        owner_pid: 0,
        receipt_binding: Some("0".repeat(64)),
        owner_started: current_owner_started(),
    };
    assert!(!owner_absent(&identity));
    identity.owner_pid = std::process::id();
    assert!(!owner_absent(&identity));
    #[cfg(windows)]
    {
        let started = identity
            .owner_started
            .expect("Windows process creation time");
        identity.owner_started = Some(started + 1);
        assert!(
            owner_absent(&identity),
            "a reused PID is a different process instance"
        );
        identity.owner_pid = u32::MAX;
        assert!(owner_absent(&identity));
    }
}

// Dedicated adapter lifecycle fixture. The crate's domain tests own wire behavior.
async fn pending_tunnel(root: &Path) -> (OpenAiTunnel, tokio::net::TcpStream) {
    use tokio::io::AsyncReadExt;
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
    let tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(tx),
        health,
        guard,
        terminal: None,
    };
    let socket = tokio::time::timeout(Duration::from_secs(5), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0; 8192];
        let n = socket.read(&mut buf).await.unwrap();
        assert!(n > 0);
        socket
    })
    .await
    .expect("fixture did not begin the first control-plane poll");
    assert!(!tunnel.health.is_ready());
    (tunnel, socket)
}

async fn idle_tunnel(root: &Path) -> OpenAiTunnel {
    use tokio::io::AsyncWriteExt;
    let (mut tunnel, mut socket) = pending_tunnel(root).await;
    socket
        .write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    if tokio::time::timeout(Duration::from_secs(5), async {
        while !tunnel.health.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .is_err()
    {
        tunnel.stop().await;
        panic!("fixture did not reach poll readiness");
    }
    tunnel
}

#[tokio::test]
async fn parent_shutdown_during_initial_poll_settles_startup_owner() {
    let temp = tempfile::tempdir().unwrap();
    // Hold the first real control-plane request without a response. No command
    // has been admitted, and readiness cannot win this cancellation race.
    let (tunnel, _socket) = pending_tunnel(&temp.path().canonicalize().unwrap()).await;
    let marker = tunnel.guard.path().to_path_buf();
    let abort = tunnel.task.as_ref().unwrap().abort_handle();
    let (polled_tx, polled_rx) = oneshot::channel();
    let (stop_tx, stop_rx) = oneshot::channel();
    let startup = tokio::spawn(finish_openai_tunnel_startup(
        tunnel,
        Instant::now() + Duration::from_secs(60),
        async {
            polled_tx.send(()).unwrap();
            let _ = stop_rx.await;
        },
    ));
    polled_rx.await.unwrap();
    stop_tx.send(()).unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(2), startup)
        .await
        .expect("parent shutdown must not await initial poll readiness")
        .unwrap()
        .unwrap();
    assert!(outcome.is_none());
    assert!(abort.is_finished(), "shutdown must observe task completion");
    assert!(
        !marker.exists(),
        "clean startup shutdown must retire its fence"
    );
    assert!(acquire_guard(&temp.path().canonicalize().unwrap(), "fixture").is_ok());
}

#[tokio::test]
async fn startup_shutdown_retains_fence_when_task_cannot_settle_cleanly() {
    let temp = tempfile::tempdir().unwrap();
    let guard = acquire_guard(&temp.path().canonicalize().unwrap(), "uncertain-startup").unwrap();
    let (stop, rx) = oneshot::channel();
    let task = tokio::spawn(async {
        let _ = rx.await;
        Err(Error::Uncertain)
    });
    let marker_path = guard.path().to_path_buf();
    let tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(stop),
        health: Health::default(),
        guard,
        terminal: None,
    };
    let result = finish_openai_tunnel_startup(
        tunnel,
        Instant::now() + Duration::from_secs(60),
        std::future::ready(()),
    )
    .await;
    assert_eq!(result.err().unwrap().code, "tunnel_restart_uncertain");
    assert!(marker_path.exists(), "uncertain startup must stay fenced");
}

#[tokio::test]
async fn startup_readiness_transfers_owner_until_observed_shutdown() {
    let temp = tempfile::tempdir().unwrap();
    let tunnel = idle_tunnel(&temp.path().canonicalize().unwrap()).await;
    let marker = tunnel.guard.path().to_path_buf();
    let mut tunnel = finish_openai_tunnel_startup(
        tunnel,
        Instant::now() + Duration::from_secs(5),
        std::future::pending(),
    )
    .await
    .unwrap()
    .expect("a ready connection should transfer its owner");
    assert!(marker.exists());
    tunnel.stop_with_outcome().await.unwrap();
    assert!(!marker.exists());
}

#[tokio::test]
async fn startup_timeout_settles_idle_owner_before_reporting_failure() {
    let temp = tempfile::tempdir().unwrap();
    let (tunnel, _socket) = pending_tunnel(&temp.path().canonicalize().unwrap()).await;
    let marker = tunnel.guard.path().to_path_buf();
    let result = finish_openai_tunnel_startup(tunnel, Instant::now(), std::future::pending()).await;
    assert_eq!(
        result.err().unwrap().code,
        "tunnel_control_plane_unreachable"
    );
    assert!(!marker.exists());
}

#[tokio::test]
async fn observed_idle_shutdown_clears_restart_marker() {
    let temp = tempfile::tempdir().unwrap();
    let mut tunnel = idle_tunnel(&temp.path().canonicalize().unwrap()).await;
    let marker = tunnel.guard.path().to_path_buf();
    tunnel.stop().await;
    assert!(!marker.exists());
    assert!(!tunnel.health.is_ready());
}
#[tokio::test]
async fn observed_task_error_is_not_a_clean_join_and_keeps_restart_fence() {
    let temp = tempfile::tempdir().unwrap();
    let guard = acquire_guard(&temp.path().canonicalize().unwrap(), "failed-fixture").unwrap();
    let task = tokio::spawn(async { Err(Error::Protocol) });
    let marker_path = guard.path().to_path_buf();
    let mut tunnel = OpenAiTunnel {
        task: Some(task),
        stop: None,
        health: Health::default(),
        guard,
        terminal: None,
    };
    assert!(tunnel.wait_for_exit().await.is_err());
    assert!(tunnel.stop_with_outcome().await.is_err());
    assert!(
        marker_path.exists(),
        "Ok(Err(...)) is not a clean task result"
    );
}

#[tokio::test]
async fn clean_owner_stop_allows_reacquiring_the_same_identity() {
    let temp = tempfile::tempdir().unwrap();
    let mut tunnel = idle_tunnel(&temp.path().canonicalize().unwrap()).await;
    assert!(tunnel.stop_with_outcome().await.is_ok());
    assert!(tunnel.stop_with_outcome().await.is_ok());
    assert!(acquire_guard(&temp.path().canonicalize().unwrap(), "fixture").is_ok());
}

#[tokio::test]
async fn dropping_owner_retains_restart_marker() {
    let temp = tempfile::tempdir().unwrap();
    let tunnel = idle_tunnel(&temp.path().canonicalize().unwrap()).await;
    let marker = tunnel.guard.path().to_path_buf();
    drop(tunnel);
    assert!(marker.exists());
    assert_eq!(
        acquire_guard(&temp.path().canonicalize().unwrap(), "fixture")
            .unwrap_err()
            .code,
        "tunnel_restart_uncertain"
    );
}
