use super::*;
use crate::types::ExecutionState;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

fn accept_until(listener: &TcpListener, deadline: Instant) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).unwrap();
                let remaining = deadline.saturating_duration_since(Instant::now());
                assert!(!remaining.is_zero(), "fixture accept deadline expired");
                stream.set_read_timeout(Some(remaining)).unwrap();
                stream.set_write_timeout(Some(remaining)).unwrap();
                return stream;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline, "fixture accept deadline expired");
                thread::sleep(Duration::from_millis(1));
            }
            Err(error) => panic!("fixture accept failed: {error}"),
        }
    }
}

fn receive_timeout(effect: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Url::parse(&format!("ws://{}/page", listener.local_addr().unwrap())).unwrap();
    let fixture_deadline = Instant::now() + Duration::from_secs(3);
    let server = thread::spawn(move || {
        let mut websocket = tungstenite::accept(accept_until(&listener, fixture_deadline)).unwrap();
        let request = websocket.read().unwrap().into_text().unwrap();
        let request: Value = serde_json::from_str(&request).unwrap();
        assert_eq!(request["id"], 7);
        // Keep the connection open without replying until the client drops it.
        // Any redispatch on this session is a failure, as is a new connection.
        assert!(
            websocket.read().is_err(),
            "request must be dispatched only once"
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        request
    });
    let mut websocket = open_loopback_websocket(&endpoint, fixture_deadline).unwrap();
    let mut next_id = 7;
    let method = if effect {
        "Page.navigate"
    } else {
        "Page.getFrameTree"
    };
    let error = cdp_call_on_websocket_until(
        &mut websocket,
        &mut next_id,
        method,
        json!({}),
        effect,
        Instant::now() + Duration::from_millis(80),
    )
    .unwrap_err();
    drop(websocket);
    let request = server.join().unwrap();
    assert_eq!(request["method"], method);
    assert_eq!(next_id, 8);
    assert_eq!(error.kind, "cdp_receive_timeout");
    assert_eq!(
        error.execution_state,
        if effect {
            ExecutionState::OutcomeUnknown
        } else {
            ExecutionState::Completed
        }
    );
    assert_eq!(
        error.recovery_action,
        if effect { Some("snapshot") } else { None }
    );
}

#[test]
fn observation_socket_timeout_is_canonical() {
    receive_timeout(false);
}

#[test]
fn effect_socket_timeout_is_unknown_without_redispatch() {
    receive_timeout(true);
}

fn snapshot_session(failed_method: Option<&'static str>, iframe: bool, card: bool) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut stream = accept_until(&listener, deadline);
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("GET /json/list "));
        loop {
            line.clear();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
        }
        let body = json!([
            {"id":"other", "webSocketDebuggerUrl":format!("ws://{address}/wrong")},
            {"id":"exact-target", "webSocketDebuggerUrl":format!("ws://{address}/exact-target")}
        ])
        .to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        drop(stream);
        let stream = accept_until(&listener, deadline);
        let mut websocket = tungstenite::accept_hdr(
            stream,
            |request: &tungstenite::handshake::server::Request, response| {
                assert_eq!(request.uri().path(), "/exact-target");
                Ok(response)
            },
        )
        .unwrap();
        let mut replies = vec![
            (
                "Page.getFrameTree",
                json!({}),
                json!({"frameTree":{"frame":{"loaderId":"document-1"}}}),
            ),
            (
                "Accessibility.getFullAXTree",
                json!({"depth": 32}),
                json!({"nodes":[
                    {"nodeId":"date", "role":{"value":"Date"}, "backendDOMNodeId":10},
                    {"nodeId":"picker", "parentId":"date", "role":{"value":"button"}, "backendDOMNodeId":11},
                    {"nodeId":"continue", "role":{"value":"button"}, "name":{"value":"Continue"}, "backendDOMNodeId":102},
                    {"nodeId":"name", "role":{"value":"textbox"}, "name":{"value":"Name"}, "backendDOMNodeId":103},
                    {"nodeId":"choose", "role":{"value":"combobox"}, "name":{"value":"Choose"}, "backendDOMNodeId":104},
                    {"nodeId":"month", "role":{"value":"DateTime"}, "name":{"value":"Month"}, "backendDOMNodeId":130}
                ]}),
            ),
            (
                "DOM.getDocument",
                json!({"depth":DOM_CONTROL_INDEX_DEPTH,"pierce":true}),
                json!({"root":{"nodeType":9}}),
            ),
        ];
        if card {
            replies[1].2["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"nodeId":"card", "backendDOMNodeId":200, "role":{"value":"generic"}}));
            replies[2].2 = json!({"root":{"nodeType":9,"backendNodeId":1,"childNodeCount":1,"children":[
                {"nodeType":1,"localName":"div","backendNodeId":200,"childNodeCount":1,"children":[
                    {"nodeType":3,"nodeValue":"Graduate engineer","backendNodeId":201}
                ]}
            ]}});
            if failed_method == Some("card_dom_incomplete") {
                replies[2].2["root"]["childNodeCount"] = json!(2);
            } else {
                replies.extend([
                ("DOMSnapshot.captureSnapshot", json!({"computedStyles":["cursor","visibility","display","pointer-events","opacity"]}),
                    json!({"strings":["pointer","visible","block","auto","1"],"documents":[{
                        "nodes":{"backendNodeId":[1,200,201],"isClickable":{"index":[1]}},
                        "layout":{"nodeIndex":[1],"styles":[[0,1,2,3,4]],"bounds":[[0,0,200,100]]}
                    }]})),
                ("Page.getFrameTree", json!({}), json!({"frameTree":{"frame":{"loaderId":if failed_method == Some("card_document_changed") { "document-2" } else { "document-1" }}}})),
            ]);
            }
        }
        if iframe {
            let tree = json!({"frameTree":{"frame":{"id":"top","loaderId":"document-1","securityOrigin":"https://example.test"},
                "childFrames":[{"frame":{"id":"child","parentId":"top","loaderId":"child-loader","securityOrigin":"https://example.test"}},
                    {"frame":{"id":"foreign","parentId":"top","loaderId":"foreign-loader","securityOrigin":"https://other.test"}}]}});
            let dom = json!({"root":{"nodeType":9,"backendNodeId":1,"children":[
                {"nodeType":1,"localName":"iframe","frameId":"child","backendNodeId":2,
                    "contentDocument":{"nodeType":9,"backendNodeId":3,"children":[
                        {"nodeType":1,"localName":"input","backendNodeId":20,"attributes":["type","text"]}]}},
                {"nodeType":1,"localName":"iframe","frameId":"foreign","backendNodeId":4,
                    "contentDocument":{"nodeType":9,"backendNodeId":5}}
            ]}});
            replies[0].2 = tree.clone();
            replies[2].2 = dom.clone();
            let mut final_tree = tree.clone();
            if failed_method == Some("frame_changed") {
                final_tree["frameTree"]["childFrames"][0]["frame"]["loaderId"] =
                    json!("new-loader");
            }
            replies.extend([
                ("Accessibility.getFullAXTree", json!({"frameId":"child","depth":32}),
                    json!({"nodes":[{"nodeId":"field","backendDOMNodeId":20,"role":{"value":"textbox"},"name":{"value":"Inside iframe"}}]})),
                ("Page.getFrameTree", json!({}), final_tree),
                ("DOM.getDocument", json!({"depth":DOM_CONTROL_INDEX_DEPTH,"pierce":true}), dom),
            ]);
        }
        let mut count = 0;
        for (method, params, result) in replies {
            let request = websocket.read().unwrap().into_text().unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            count += 1;
            assert_eq!(request["id"], 40 + count);
            assert_eq!(request["method"], method);
            assert_eq!(request["params"], params);
            let response = if failed_method == Some(method) {
                json!({"id":request["id"],"error":{"code":-32000,"message":"fixture failure"}})
            } else {
                json!({"id":request["id"],"result":result})
            };
            websocket
                .send(Message::Text(response.to_string().into()))
                .unwrap();
            if failed_method == Some(method) && method != "DOMSnapshot.captureSnapshot" {
                break;
            }
        }
        assert!(websocket.read().is_err(), "snapshot session must end here");
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        count
    });
    // Supply the backend's process owner without requiring Chromium or a shell.
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .arg("--list")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = ManagedChild::spawn(&mut command).unwrap();
    assert!(child.wait_tree_exit(Duration::from_secs(3)).unwrap());
    let mut backend = CdpBackend {
        owner: CdpOwner::Owned {
            child,
            profile: OwnedProfile::Ephemeral(tempfile::tempdir().unwrap()),
            endpoint: Url::parse(&format!("ws://{address}/browser")).unwrap(),
        },
        next_id: 41,
        collectors: HashMap::new(),
    };
    let result = backend.snapshot("exact-target", 100);
    let count = server.join().unwrap();
    assert_eq!(backend.next_id, 41 + count);
    match failed_method {
        Some("frame_changed" | "card_document_changed") => {
            assert_eq!(result.unwrap_err().kind, "stale_element")
        }
        Some("DOM.getDocument") if iframe => {
            assert_eq!(result.unwrap_err().kind, "frame_document_unavailable")
        }
        Some("Page.getFrameTree" | "Accessibility.getFullAXTree") => {
            assert_eq!(result.unwrap_err().kind, "cdp_error");
        }
        _ => {
            assert_eq!(
                count,
                if iframe {
                    6
                } else if card && failed_method != Some("card_dom_incomplete") {
                    5
                } else {
                    3
                }
            );
            let snapshot = result.unwrap();
            assert_eq!(snapshot.document_id, "document-1");
            if card {
                let card = snapshot
                    .nodes
                    .iter()
                    .find(|node| node.backend_node_id == Some(200))
                    .unwrap();
                assert_eq!(
                    card.capability.admits_any(),
                    !matches!(
                        failed_method,
                        Some("DOMSnapshot.captureSnapshot" | "card_dom_incomplete")
                    )
                );
            }
            if iframe {
                let field = snapshot
                    .nodes
                    .iter()
                    .find(|n| n.backend_node_id == Some(20))
                    .unwrap();
                assert_eq!(
                    field.capability.action_names(),
                    vec!["click", "input_text", "set_value"]
                );
                assert!(field.frame_fence.is_some());
            }
            let picker = snapshot
                .nodes
                .iter()
                .find(|node| node.backend_node_id == Some(11))
                .unwrap();
            assert!(picker.capability.action_names().is_empty());
            if failed_method == Some("DOM.getDocument") {
                let actions = |backend_node_id: i64| {
                    snapshot
                        .nodes
                        .iter()
                        .find(|node| node.backend_node_id == Some(backend_node_id))
                        .unwrap()
                        .capability
                        .action_names()
                };
                assert_eq!(actions(102), ["click"]);
                assert_eq!(actions(103), ["click", "input_text"]);
                assert_eq!(actions(104), ["click"]);
                assert!(actions(130).is_empty());
            }
        }
    }
}

#[test]
fn snapshot_discovers_once_and_reuses_exact_page_session() {
    snapshot_session(None, false, false);
}

#[test]
fn snapshot_dom_failure_remains_best_effort_without_picker_authority() {
    snapshot_session(Some("DOM.getDocument"), false, false);
}

#[test]
fn snapshot_frame_and_ax_remain_required() {
    snapshot_session(Some("Page.getFrameTree"), false, false);
    snapshot_session(Some("Accessibility.getFullAXTree"), false, false);
}

#[test]
fn iframe_snapshot_reads_only_same_origin_frames_and_rechecks_documents() {
    snapshot_session(None, true, false);
}

#[test]
fn iframe_snapshot_rejects_mid_collection_navigation_and_missing_dom() {
    snapshot_session(Some("frame_changed"), true, false);
    snapshot_session(Some("DOM.getDocument"), true, false);
}

#[test]
fn snapshot_card_uses_one_bounded_capture_and_checks_document_afterwards() {
    snapshot_session(None, false, true);
    snapshot_session(Some("card_document_changed"), false, true);
    snapshot_session(Some("DOMSnapshot.captureSnapshot"), false, true);
    snapshot_session(Some("card_dom_incomplete"), false, true);
}

#[test]
fn ax_missing_descendants_remain_incomplete_for_queries() {
    assert!(ax_source_incomplete(&[
        json!({"nodeId":"root","childIds":["missing"]})
    ]));
    assert!(!ax_source_incomplete(&[
        json!({"nodeId":"root","childIds":["child"]}),
        json!({"nodeId":"child"})
    ]));
}
