//! Real stdio ACP provider roundtrip against a loopback model API.

use super::*;

#[test]
fn model_gateway_adapter_completes_real_runner_acp_roundtrip() {
    let Ok(python) = Command::new("python3")
        .args([
            "-c",
            "import sys; assert sys.version_info >= (3, 10); print(sys.executable)",
        ])
        .stdin(Stdio::null())
        .output()
    else {
        eprintln!("skipping model gateway ACP roundtrip: python3 is unavailable");
        return;
    };
    if !python.status.success() {
        eprintln!("skipping model gateway ACP roundtrip: Python 3.10+ is required");
        return;
    }
    let executable = String::from_utf8(python.stdout).unwrap().trim().to_string();
    crate::tests::IsolatedEnv::new()
        .set("WEBCODEX_TEST_MODEL_GATEWAY_KEY", "local-mock-key")
        .run("real-model-gateway", || {
            let temp = crate::tests::executable_tempdir();
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let server = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                let stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "adapter never connected");
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("mock HTTP accept failed: {error}"),
                    }
                };
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut reader = BufReader::new(stream);
                let mut request_line = String::new();
                reader.read_line(&mut request_line).unwrap();
                assert_eq!(request_line, "POST /v1/responses HTTP/1.1\r\n");
                let mut length = None;
                let mut authorized = false;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    let (name, value) = line.split_once(':').unwrap();
                    if name.eq_ignore_ascii_case("content-length") {
                        length = Some(value.trim().parse::<usize>().unwrap());
                    }
                    if name.eq_ignore_ascii_case("authorization") {
                        authorized = value.trim() == "Bearer local-mock-key";
                    }
                }
                assert!(authorized, "explicit Runner environment mapping was not applied");
                let length = length.unwrap();
                assert!(length <= 32 * 1024);
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                let payload: Value = serde_json::from_slice(&body).unwrap();
                let events = concat!(
                    "data: {\"type\":\"response.output_text.delta\",\"delta\":\"评审结果 🌎\"}\n\n",
                    "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[]}}\n\n"
                );
                write!(
                    reader.get_mut(),
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    events.len(), events
                )
                .unwrap();
                reader.get_mut().flush().unwrap();
                payload
            });
            let adapter = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../integrations/model_gateway/adapter.py");
            let mut cfg = fake_config(
                executable,
                vec![
                    adapter.to_string_lossy().into_owned(),
                    "--base-url".to_string(),
                    format!("http://{address}/v1"),
                    "--model".to_string(),
                    "local-review-model".to_string(),
                ],
            );
            cfg.agents[0].env_from_env = BTreeMap::from([(
                "MODEL_API_KEY".to_string(),
                "WEBCODEX_TEST_MODEL_GATEWAY_KEY".to_string(),
            )]);
            cfg.agents[0].allowed_config_options.clear();
            let projects = project_fixture(&temp);
            let root = temp.path().join("repo");
            let manager =
                CodingAgentManager::with_store(&cfg, temp.path().join("store")).unwrap();
            let run = "wc_agent_run_modelgateway0001";
            let mut request = start_request(&manager, &root, run, BTreeMap::new());
            let prompt = "Review the complete context:\nDecision: preserve account boundaries.\n上下文：只评审，不执行工具。";
            let CodingAgentRequest::Start(start) = &mut request else {
                unreachable!();
            };
            start.instruction = prompt.to_string();
            let response = manager.handle(request, &projects);
            assert!(response.error.is_none(), "{:?}", response.error);
            let observation = wait_for_terminal_observation(&manager, run);
            assert_eq!(observation.run.state, CodingAgentRunState::Completed);
            assert!(observation.events.iter().any(|event| {
                event.kind == CodingAgentEventKind::AgentMessage
                    && event.text.as_deref() == Some("评审结果 🌎")
            }));
            let payload = server.join().unwrap();
            assert_eq!(payload["model"], "local-review-model");
            assert_eq!(payload["stream"], true);
            assert_eq!(payload["store"], false);
            assert_eq!(
                payload["input"],
                json!([{"role":"user","content":[{"type":"input_text","text":prompt}]}])
            );
            assert!(payload.get("tools").is_none());
            assert!(payload.get("tool_choice").is_none());
        });
}
