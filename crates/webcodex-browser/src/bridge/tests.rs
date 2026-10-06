use super::*;
use std::io::Read;

fn root() -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().canonicalize().unwrap().join("extension");
    (temp, path)
}
fn config(server: &BridgeServer) -> Rendezvous {
    serde_json::from_slice(&std::fs::read(&server.record).unwrap()).unwrap()
}
fn connect(server: &BridgeServer) -> TcpStream {
    let config = config(server);
    let mut stream = TcpStream::connect(config.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    protocol::write_frame(
        &mut stream,
        &json!({"version":VERSION,"instance":config.instance,"token":config.token,
        "extension_id":EXTENSION_ID,"parent_pid":std::process::id()}),
        4096,
        false,
    )
    .unwrap();
    assert_eq!(
        protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap()["kind"],
        "ready"
    );
    stream
}
fn offer(server: &BridgeServer, stream: &mut TcpStream) -> String {
    protocol::write_frame(stream, &json!({"kind":"offer","tab":123,"window":9,"title":"Existing signed-in fixture", "url":"https://example.test/account"}), MAX_RESPONSE, false).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(offer) = server.discover().first() {
            return offer.attachment_id.clone();
        }
        assert!(
            Instant::now() < deadline,
            "authenticated offer was not observed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn respond(stream: &mut TcpStream, command: &Value, result: Value) {
    protocol::write_frame(
        stream,
        &json!({"kind":"message","channel":command["channel"],"lease":command["lease"],
        "message":{"id":command["request"]["id"],"result":result}}),
        MAX_RESPONSE,
        false,
    )
    .unwrap();
}
fn peer_worker(mut stream: TcpStream, operations: Arc<Mutex<Vec<String>>>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(command) = protocol::read_blocking(&mut stream, MAX_RESPONSE, false) {
            let method = command["request"]["method"].as_str().unwrap().to_string();
            lock(&operations).push(method.clone());
            assert_ne!(
                method, "Browser.close",
                "external transport must never close its Chrome owner"
            );
            let result = match method.as_str() {
                "targets" => {
                    json!([{"id":"tab_123","type":"page","title":"Signed-in fixture","url":"https://example.test/account"}])
                }
                "Page.getFrameTree" => {
                    json!({"frameTree":{"frame":{"id":"frame-one","loaderId":"document-one"}}})
                }
                _ => json!({}),
            };
            respond(&mut stream, &command, result);
        }
    })
}

#[test]
fn authentication_requires_exact_version_instance_origin_and_secret() {
    let config = Rendezvous {
        version: VERSION,
        address: "127.0.0.1:1".parse().unwrap(),
        instance: "instance".into(),
        token: "secret".into(),
    };
    let mut hello = Hello {
        version: VERSION,
        instance: "instance".into(),
        token: "secret".into(),
        extension_id: EXTENSION_ID.into(),
        parent_pid: 123,
    };
    assert!(config.accepts(&hello));
    hello.token = "other-secret".into();
    assert!(!config.accepts(&hello));
    hello.token = "secret".into();
    hello.instance = "old-instance".into();
    assert!(!config.accepts(&hello));
    hello.instance = "instance".into();
    hello.extension_id = "unapproved".into();
    assert!(!config.accepts(&hello));
    hello.extension_id = EXTENSION_ID.into();
    hello.version += 1;
    assert!(!config.accepts(&hello));
    hello.version = VERSION;
    hello.parent_pid = 0;
    assert!(!config.accepts(&hello));
    assert!(native::run(&["chrome-extension://wrong/".into()]).is_err());
}
#[test]
fn unauthenticated_socket_cannot_offer_tabs_and_native_owner_is_exclusive() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    assert!(BridgeServer::start_at(&root).is_err());
    let mut socket = TcpStream::connect(config(&server).address).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    protocol::write_frame(&mut socket, &json!({"kind":"offer","tab":123}), 4096, false).unwrap();
    let mut byte = [0];
    assert!(matches!(socket.read(&mut byte), Ok(0) | Err(_)));
    assert!(server.discover().is_empty());
    drop(server);
    assert!(BridgeServer::start_at(&root).is_ok());
}
#[test]
fn external_backend_reuses_cdp_and_shutdown_only_detaches() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let observed = serde_json::to_string(&server.discover()).unwrap();
    for private in [
        "token",
        "parent_pid",
        "window",
        "tab_123",
        &config(&server).token,
    ] {
        assert!(!observed.contains(private));
    }
    let operations = Arc::new(Mutex::new(Vec::new()));
    let worker = peer_worker(stream, operations.clone());
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    assert!(server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .is_err());
    let mut backend = crate::cdp::attach_external(lease);
    assert_eq!(
        backend.ownership(),
        crate::BrowserOwnership::AttachedExternal
    );
    assert_eq!(backend.live_process_id().unwrap(), std::process::id());
    let pages = backend.pages().unwrap();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].document_id, "document-one");
    backend.shutdown(Duration::from_secs(2)).unwrap();
    drop(backend);
    assert_eq!(
        *lock(&operations),
        ["attach", "targets", "Page.getFrameTree", "detach"]
    );
    assert!(server.discover().is_empty());
    // Only the native connection is closed; the externally observed process is
    // still executing this test. No ManagedChild is present in the backend.
    drop(server);
    worker.join().unwrap();
}
#[test]
fn bridge_loss_invalidates_old_attachment_and_does_not_replay_a_sent_effect() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let worker = std::thread::spawn(move || {
        let attach = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        respond(&mut stream, &attach, json!({}));
        let effect = protocol::read_blocking(&mut stream, MAX_RESPONSE, false).unwrap();
        assert_eq!(effect["request"]["method"], "close_page");
        // Disconnect after the exact first dispatch, without its acknowledgement.
        drop(stream);
    });
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    let result = lease.browser_call(
        "Target.closeTarget",
        json!({"targetId":"tab_123"}),
        true,
        Instant::now() + Duration::from_secs(2),
    );
    assert_eq!(
        result.unwrap_err().execution_state,
        crate::ExecutionState::OutcomeUnknown
    );
    worker.join().unwrap();
    assert!(lease.live_process_id().is_err());
    assert!(server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .is_err());
}
#[test]
fn dropping_runner_bridge_closes_channels_without_owning_external_process() {
    let (_temp, root) = root();
    let server = BridgeServer::start_at(&root).unwrap();
    let mut stream = connect(&server);
    let id = offer(&server, &mut stream);
    let operations = Arc::new(Mutex::new(Vec::new()));
    let worker = peer_worker(stream, operations.clone());
    let lease = server
        .attach(&id, Instant::now() + Duration::from_secs(2))
        .unwrap();
    drop(server);
    worker.join().unwrap();
    assert!(lease.live_process_id().is_err());
    assert_eq!(*lock(&operations), ["attach"]);
}
#[test]
fn queued_bytes_have_one_global_bound_and_are_released_on_receiver_drop() {
    let state = State {
        registry: Mutex::new(Registry::default()),
        stop: AtomicBool::new(false),
        next_channel: AtomicU64::new(1),
        queued: Arc::new(AtomicUsize::new(0)),
    };
    let (tx, rx) = mpsc::sync_channel(128);
    let route = Route {
        peer: "peer".into(),
        lease: "lease".into(),
        target: None,
        pending: None,
        tx,
    };
    let value = json!({"data":"x".repeat(MAX_RESPONSE / 2)});
    let mut count = 0;
    while state.queue(&route, &value) {
        count += 1;
        assert!(count < 20);
    }
    assert!(count > 0);
    assert!(state.queued.load(Ordering::Acquire) <= MAX_QUEUED_BYTES);
    drop(rx);
    assert_eq!(state.queued.load(Ordering::Acquire), 0);
}
