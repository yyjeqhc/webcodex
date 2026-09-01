use super::*;
use crate::shell_client::{AgentTransport, TRANSPORT_WEBSOCKET};
use crate::shell_protocol::{ShellClientCapabilities, ShellJobOpRequest};
use tokio::time::Instant;

const SESSION_TEST_TIMEOUT: Duration = Duration::from_millis(250);

struct PendingReader;

impl AgentReader for PendingReader {
    async fn recv(&mut self) -> RecvOutcome {
        std::future::pending::<RecvOutcome>().await
    }
}

fn deadline() -> Instant {
    Instant::now() + SESSION_TEST_TIMEOUT
}

fn streaming_registration(client_id: &str, agent_instance_id: &str) -> ShellClientRegisterRequest {
    let mut capabilities = ShellClientCapabilities::default();
    capabilities.async_jobs = true;
    capabilities.async_shell_jobs = true;
    crate::test_support::current_runner_registration(ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: Some(capabilities),
        policy: None,
    })
}

fn start_job_request(client_id: &str) -> ShellJobOpRequest {
    ShellJobOpRequest {
        op: "start".to_string(),
        client_id: Some(client_id.to_string()),
        cwd: None,
        command: Some("sleep 1".to_string()),
        timeout_secs: Some(1),
        job_id: None,
        since_stdout_line: None,
        since_stderr_line: None,
        tail_lines: None,
        limit: None,
        codex: None,
    }
}

async fn register_streaming(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    connection_id: &str,
) -> (Arc<Notify>, watch::Receiver<bool>) {
    let notify = Arc::new(Notify::new());
    let (_view, cancel) = registry
        .register_streaming_session_with_cancel(
            streaming_registration(client_id, instance),
            None,
            connection_id,
            AgentTransport::WebSocket,
            notify.clone(),
        )
        .await
        .unwrap();
    (notify, cancel)
}

fn alive_writer() -> (mpsc::Sender<AgentEnvelope>, JoinHandle<WriterExit>) {
    let (out_tx, mut out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
    let writer = tokio::spawn(async move {
        while out_rx.recv().await.is_some() {}
        WriterExit::ChannelClosed
    });
    (out_tx, writer)
}

#[tokio::test]
async fn writer_failure_terminates_session_and_reconciles_active_job() {
    let registry = Arc::new(ShellClientRegistry::default());
    let (notify, cancel) = register_streaming(&registry, "writer-fail", "inst-a", "conn-a").await;
    let job = registry
        .start_job(start_job_request("writer-fail"), "test".to_string())
        .await
        .unwrap();
    let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
    let writer_task = tokio::spawn(async { WriterExit::TransportFailed });

    tokio::time::timeout_at(
        deadline(),
        run_agent_session(
            SessionContext {
                registry: &registry,
                client_id: "writer-fail",
                agent_instance_id: "inst-a",
                connection_id: "conn-a",
                notify,
                cancel,
                transport_label: "websocket",
            },
            out_tx,
            PendingReader,
            writer_task,
        ),
    )
    .await
    .expect("writer failure must terminate a pending reader session");

    let view = registry.get_client_view("writer-fail").await.unwrap();
    assert!(!view.connected);
    assert_eq!(registry.get_job(&job.job_id).await.unwrap().status, "lost");
}

#[tokio::test]
async fn writer_task_panic_terminates_session() {
    let registry = Arc::new(ShellClientRegistry::default());
    let (notify, cancel) = register_streaming(&registry, "writer-panic", "inst-a", "conn-a").await;
    let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
    let writer_task = tokio::spawn(async { panic!("synthetic writer panic") });

    tokio::time::timeout_at(
        deadline(),
        run_agent_session(
            SessionContext {
                registry: &registry,
                client_id: "writer-panic",
                agent_instance_id: "inst-a",
                connection_id: "conn-a",
                notify,
                cancel,
                transport_label: "websocket",
            },
            out_tx,
            PendingReader,
            writer_task,
        ),
    )
    .await
    .expect("writer panic must terminate a pending reader session");

    assert!(
        !registry
            .get_client_view("writer-panic")
            .await
            .unwrap()
            .connected
    );
}

#[tokio::test]
async fn pump_exit_terminates_pending_reader_and_reconciles_exact_connection() {
    for (client_id, exit) in [
        ("pump-lease-lost", PumpExit::LeaseLost),
        ("pump-registry-failed", PumpExit::RegistryFailed),
    ] {
        let registry = Arc::new(ShellClientRegistry::default());
        let (notify, cancel) = register_streaming(&registry, client_id, "inst-a", "conn-a").await;
        let (out_tx, writer_task) = alive_writer();
        let pump_task = tokio::spawn(async move { exit });

        tokio::time::timeout_at(
            deadline(),
            run_agent_session_with_pump(
                SessionContext {
                    registry: &registry,
                    client_id,
                    agent_instance_id: "inst-a",
                    connection_id: "conn-a",
                    notify,
                    cancel,
                    transport_label: "websocket",
                },
                out_tx,
                PendingReader,
                writer_task,
                pump_task,
            ),
        )
        .await
        .expect("pump exit must terminate a pending-reader session");

        let view = registry.get_client_view(client_id).await.unwrap();
        assert!(
            !view.connected,
            "{client_id} exact lease must be reconciled"
        );
    }
}

#[tokio::test]
async fn pump_task_panic_terminates_session() {
    let registry = Arc::new(ShellClientRegistry::default());
    let (notify, cancel) = register_streaming(&registry, "pump-panic", "inst-a", "conn-a").await;
    let (out_tx, writer_task) = alive_writer();
    let pump_task = tokio::spawn(async { panic!("synthetic pump panic") });

    tokio::time::timeout_at(
        deadline(),
        run_agent_session_with_pump(
            SessionContext {
                registry: &registry,
                client_id: "pump-panic",
                agent_instance_id: "inst-a",
                connection_id: "conn-a",
                notify,
                cancel,
                transport_label: "websocket",
            },
            out_tx,
            PendingReader,
            writer_task,
            pump_task,
        ),
    )
    .await
    .expect("pump panic must terminate a pending-reader session");

    assert!(
        !registry
            .get_client_view("pump-panic")
            .await
            .unwrap()
            .connected
    );
}

#[tokio::test]
async fn same_instance_replacement_actively_terminates_old_session_without_losing_job() {
    let registry = Arc::new(ShellClientRegistry::default());
    let (notify_a, cancel_a) =
        register_streaming(&registry, "replace-active", "inst-a", "conn-a").await;
    let active_job = registry
        .start_job(start_job_request("replace-active"), "test".to_string())
        .await
        .unwrap();
    let (out_tx, writer_task) = alive_writer();
    let session_registry = Arc::clone(&registry);
    let session_task = tokio::spawn(async move {
        run_agent_session(
            SessionContext {
                registry: &session_registry,
                client_id: "replace-active",
                agent_instance_id: "inst-a",
                connection_id: "conn-a",
                notify: notify_a,
                cancel: cancel_a,
                transport_label: "websocket",
            },
            out_tx,
            PendingReader,
            writer_task,
        )
        .await;
    });

    let notify_b = Arc::new(Notify::new());
    let (_view_b, cancel_b) = registry
        .register_streaming_session_with_cancel(
            streaming_registration("replace-active", "inst-a"),
            None,
            "conn-b",
            AgentTransport::WebSocket,
            notify_b,
        )
        .await
        .unwrap();
    assert!(!*cancel_b.borrow(), "new connection must start uncancelled");

    tokio::time::timeout_at(deadline(), session_task)
        .await
        .expect("successful replacement must actively terminate connection A")
        .expect("old session task must not panic");

    let replacement = registry
        .get_client_view_for_connection("replace-active", "inst-a", "conn-b")
        .await
        .expect("connection B must remain authoritative");
    assert!(replacement.connected);
    assert_eq!(replacement.transport, TRANSPORT_WEBSOCKET);
    assert_ne!(
        registry.get_job(&active_job.job_id).await.unwrap().status,
        "lost",
        "same-instance replacement must not manufacture job loss"
    );

    // A's teardown already reconciled its exact stale lease. Repeating that
    // stale cleanup is still a no-op and cannot affect B.
    registry
        .reconcile_disconnect_for_connection("replace-active", "inst-a", "conn-a")
        .await;
    assert!(
        registry
            .get_client_view_for_connection("replace-active", "inst-a", "conn-b")
            .await
            .expect("stale A reconciliation must not remove B")
            .connected
    );
}

#[tokio::test]
async fn stale_writer_failure_cannot_reconcile_replacement_connection() {
    let registry = Arc::new(ShellClientRegistry::default());
    let (notify_a, cancel_a) =
        register_streaming(&registry, "writer-stale", "inst-a", "conn-a").await;
    register_streaming(&registry, "writer-stale", "inst-a", "conn-b").await;
    let (out_tx, _out_rx) = mpsc::channel(OUTGOING_CHANNEL_CAPACITY);
    let writer_task = tokio::spawn(async { WriterExit::TransportFailed });

    tokio::time::timeout_at(
        deadline(),
        run_agent_session(
            SessionContext {
                registry: &registry,
                client_id: "writer-stale",
                agent_instance_id: "inst-a",
                connection_id: "conn-a",
                notify: notify_a,
                cancel: cancel_a,
                transport_label: "quic",
            },
            out_tx,
            PendingReader,
            writer_task,
        ),
    )
    .await
    .expect("stale writer/cancel completion must not touch replacement");

    let replacement = registry
        .get_client_view_for_connection("writer-stale", "inst-a", "conn-b")
        .await
        .expect("replacement connection must remain authoritative");
    assert!(replacement.connected);
    assert_eq!(replacement.transport, TRANSPORT_WEBSOCKET);
}
