use super::*;
use salvo::conn::{Listener, TcpListener};
use salvo::prelude::{affix_state, handler, Depot, Request, Response, Router};
use salvo::websocket::WebSocketUpgrade;
use std::convert::Infallible;
use std::future::pending;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{oneshot, Notify};

async fn start_test_server(
    router: Router,
    graceful_timeout: Duration,
) -> (
    SocketAddr,
    oneshot::Sender<ShutdownReason>,
    Arc<ShutdownCoordinator>,
    tokio::task::JoinHandle<io::Result<()>>,
) {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .expect("test listener should be a TCP socket");
    let server = Server::new(acceptor);
    let (signal_tx, signal_rx) = oneshot::channel();
    let coordinator = Arc::new(ShutdownCoordinator::default());
    let task_coordinator = coordinator.clone();
    let task = tokio::spawn(async move {
        serve_with_signal(
            server,
            router,
            task_coordinator,
            async move { signal_rx.await.expect("test shutdown sender dropped") },
            graceful_timeout,
        )
        .await
    });
    (addr, signal_tx, coordinator, task)
}

async fn wait_for_state(coordinator: &ShutdownCoordinator, expected: ShutdownState) {
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if coordinator.state() == expected {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("shutdown state transition timed out");
}

async fn wait_until_not_running(coordinator: &ShutdownCoordinator) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while coordinator.state() == ShutdownState::Running {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("shutdown did not leave running state");
}

#[handler]
async fn hello() -> &'static str {
    "hello"
}

#[tokio::test]
async fn no_signal_keeps_server_running() {
    let acceptor = TcpListener::new("127.0.0.1:0").bind().await;
    let addr = acceptor.holdings()[0]
        .local_addr
        .clone()
        .into_std()
        .unwrap();
    let server = Server::new(acceptor);
    let force_handle = server.handle();
    let coordinator = Arc::new(ShutdownCoordinator::default());
    let task_coordinator = coordinator.clone();
    let task = tokio::spawn(async move {
        serve_with_signal(
            server,
            Router::new().get(hello),
            task_coordinator,
            pending::<ShutdownReason>(),
            Duration::from_millis(200),
        )
        .await
    });

    let body = reqwest::get(format!("http://{addr}/"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(body, "hello");
    assert_eq!(coordinator.state(), ShutdownState::Running);
    assert!(!task.is_finished());

    force_handle.stop_forcible();
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect("forced test cleanup timed out")
        .unwrap()
        .unwrap();
    assert_eq!(coordinator.state(), ShutdownState::Stopped);
}

#[test]
fn shutdown_state_transition_is_authoritative_once() {
    let coordinator = ShutdownCoordinator::default();
    assert_eq!(coordinator.state(), ShutdownState::Running);
    assert!(coordinator.begin_draining());
    assert_eq!(coordinator.state(), ShutdownState::Draining);
    assert!(!coordinator.begin_draining());
    assert_eq!(coordinator.state(), ShutdownState::Draining);
    coordinator.mark_stopped();
    assert_eq!(coordinator.state(), ShutdownState::Stopped);
    assert!(!coordinator.begin_draining());
}

#[test]
fn production_grace_exceeds_request_hard_timeout_with_systemd_margin() {
    assert_eq!(crate::REQUEST_HARD_TIMEOUT_SECS, 300);
    assert_eq!(crate::SERVER_GRACEFUL_SHUTDOWN_TIMEOUT_SECS, 315);
    assert!(crate::SERVER_GRACEFUL_SHUTDOWN_TIMEOUT_SECS > crate::REQUEST_HARD_TIMEOUT_SECS);
    assert_eq!(crate::SERVER_SYSTEMD_TIMEOUT_STOP_SECS, 330);
    assert!(crate::SERVER_SYSTEMD_TIMEOUT_STOP_SECS > crate::SERVER_GRACEFUL_SHUTDOWN_TIMEOUT_SECS);
}

#[derive(Clone)]
struct SlowGate {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[handler]
async fn slow_handler(depot: &mut Depot) -> &'static str {
    let gate = depot.obtain::<SlowGate>().unwrap().clone();
    gate.started.notify_one();
    gate.release.notified().await;
    "slow-complete"
}

#[tokio::test]
async fn finite_request_started_before_drain_reaches_client_before_server_exit() {
    let gate = SlowGate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let router = Router::new()
        .hoop(affix_state::inject(gate.clone()))
        .get(slow_handler);
    let (addr, shutdown, coordinator, server_task) =
        start_test_server(router, Duration::from_secs(2)).await;
    let request = tokio::spawn(async move {
        reqwest::get(format!("http://{addr}/"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    });
    tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
        .await
        .expect("slow handler never started");

    shutdown.send(ShutdownReason::Sigterm).unwrap();
    wait_for_state(&coordinator, ShutdownState::Draining).await;
    assert!(
        !request.is_finished(),
        "in-flight request was cancelled at drain start"
    );
    gate.release.notify_waiters();

    let body = tokio::time::timeout(Duration::from_secs(1), request)
        .await
        .expect("client did not receive drained response")
        .unwrap();
    assert_eq!(body, "slow-complete");
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("server did not exit after finite request completed")
        .unwrap()
        .unwrap();
    assert_eq!(coordinator.state(), ShutdownState::Stopped);
}

#[derive(Clone)]
struct CountGate(Arc<AtomicUsize>);

#[handler]
async fn counted_handler(depot: &mut Depot) -> &'static str {
    depot
        .obtain::<CountGate>()
        .unwrap()
        .0
        .fetch_add(1, Ordering::SeqCst);
    "ok"
}

async fn read_http_response(stream: &mut tokio::net::TcpStream) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut buf = [0u8; 1024];
    let header_end = loop {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "response headers truncated",
            ));
        }
        bytes.extend_from_slice(&buf[..n]);
        if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let headers = std::str::from_utf8(&bytes[..header_end])
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing content-length"))?;
    while bytes.len() - header_end < content_length {
        let n = stream.read(&mut buf).await?;
        if n == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "response body truncated",
            ));
        }
        bytes.extend_from_slice(&buf[..n]);
    }
    Ok(bytes)
}

#[tokio::test]
async fn drain_disables_existing_http1_keepalive_before_new_handler_dispatch() {
    let counter = Arc::new(AtomicUsize::new(0));
    let router = Router::new()
        .hoop(affix_state::inject(CountGate(counter.clone())))
        .get(counted_handler);
    let (addr, shutdown, coordinator, server_task) =
        start_test_server(router, Duration::from_secs(1)).await;
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n")
        .await
        .unwrap();
    let first = read_http_response(&mut stream).await.unwrap();
    assert!(first.ends_with(b"ok"));
    assert_eq!(counter.load(Ordering::SeqCst), 1);

    shutdown.send(ShutdownReason::Sigterm).unwrap();
    // An idle keepalive may close so quickly that Draining is only transient;
    // either Draining or Stopped proves the shutdown command became authoritative
    // before the second request is attempted.
    wait_until_not_running(&coordinator).await;
    let _ = stream
        .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;
    let mut byte = [0u8; 1];
    let _ = tokio::time::timeout(Duration::from_millis(200), stream.read(&mut byte)).await;

    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("idle keepalive blocked graceful shutdown")
        .unwrap()
        .unwrap();
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "post-drain request reached handler"
    );
}

#[derive(Clone)]
struct WsGate {
    started: Arc<Notify>,
    release: Arc<Notify>,
}

#[handler]
async fn ws_handler(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let gate = depot.obtain::<WsGate>().unwrap().clone();
    WebSocketUpgrade::new()
        .upgrade(req, res, move |_socket| async move {
            gate.started.notify_one();
            gate.release.notified().await;
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn upgraded_websocket_does_not_hold_http_server_graceful_completion() {
    let gate = WsGate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let router = Router::with_path("ws")
        .hoop(affix_state::inject(gate.clone()))
        .get(ws_handler);
    let (addr, shutdown, _coordinator, server_task) =
        start_test_server(router, Duration::from_secs(1)).await;
    let (websocket, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
        .await
        .expect("websocket callback did not start");

    shutdown.send(ShutdownReason::Sigterm).unwrap();
    tokio::time::timeout(Duration::from_millis(500), server_task)
        .await
        .expect("upgraded websocket held HTTP graceful shutdown")
        .unwrap()
        .unwrap();

    drop(websocket);
    gate.release.notify_waiters();
}

#[tokio::test]
async fn graceful_deadline_forces_hung_finite_request_boundedly() {
    let gate = SlowGate {
        started: Arc::new(Notify::new()),
        release: Arc::new(Notify::new()),
    };
    let router = Router::new()
        .hoop(affix_state::inject(gate.clone()))
        .get(slow_handler);
    let (addr, shutdown, _coordinator, server_task) =
        start_test_server(router, Duration::from_millis(80)).await;
    let request = tokio::spawn(async move { reqwest::get(format!("http://{addr}/")).await });
    tokio::time::timeout(Duration::from_secs(1), gate.started.notified())
        .await
        .expect("hung test handler did not start");

    let started = Instant::now();
    shutdown.send(ShutdownReason::Sigterm).unwrap();
    tokio::time::timeout(Duration::from_secs(1), server_task)
        .await
        .expect("grace timeout failed to bound Server exit")
        .unwrap()
        .unwrap();
    assert!(started.elapsed() >= Duration::from_millis(60));
    assert!(started.elapsed() < Duration::from_secs(1));
    request.abort();
    gate.release.notify_waiters();
}

#[allow(dead_code)]
fn _assert_infallible(_: Infallible) {}
