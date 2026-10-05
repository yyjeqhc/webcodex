mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::policy::{DeadlinePolicy, Limits};

#[tokio::test]
async fn real_control_plane_metadata_is_filtered_without_rejecting_the_request() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut cmd = command("one");
    // Field names observed during Linux dogfood; all values are synthetic.
    cmd["headers"] = json!({
        "x-openai-subject":["subject-context"], "x-openai-session":["window-context"],
        "x-request-id":["ingress-request"], "accept":["text/event-stream"],
        "mcp-protocol-version":["2026-07-28"], "mcp-method":["tools/call"], "mcp-name":["effect"],
        "x-origin-ingress-name":["ingress"], "x-forwarded-client-cert":["untrusted-proxy-identity"],
        "x-openai-pod-uid":["pod"], "x-datadog-trace-id":["trace"], "x-datadog-parent-id":["parent"],
        "x-datadog-sampling-priority":["1"], "x-datadog-tags":["tags"],
        "traceparent":["trace-parent"], "tracestate":["trace-state"],
        "host":["not-the-target.invalid"], "content-type":["text/plain"], "content-length":["0"]
    });
    let mut polls = Polls::start(&mut cp, vec![json!([cmd])]);
    let (stop, task) = start(client);
    let request = mcp.next().await;
    assert_eq!(request.headers["authorization"], "Bearer local-secret");
    assert_eq!(
        request.headers["accept"],
        "application/json, text/event-stream"
    );
    assert_eq!(request.headers["content-type"], "application/json");
    assert_ne!(request.headers["host"], "not-the-target.invalid");
    assert_eq!(
        request.headers["content-length"].parse::<usize>().unwrap(),
        request.body.len()
    );
    for (header, expected) in [
        ("mcp-protocol-version", "2026-07-28"),
        ("mcp-method", "tools/call"),
        ("mcp-name", "effect"),
        ("x-openai-session", "window-context"),
        ("x-openai-subject", "subject-context"),
    ] {
        assert_eq!(request.headers[header], expected);
    }
    for ignored in [
        "x-request-id",
        "x-origin-ingress-name",
        "x-forwarded-client-cert",
        "x-openai-pod-uid",
        "x-datadog-trace-id",
        "x-datadog-parent-id",
        "x-datadog-sampling-priority",
        "x-datadog-tags",
        "traceparent",
        "tracestate",
    ] {
        assert!(!request.headers.contains_key(ignored));
    }
    assert_eq!(request.json(), command("one")["jsonrpc"]);
    request.respond(200, r#"{"jsonrpc":"2.0","id":"one","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled(&health).await;
    assert_eq!(health.rejected_commands(), 0);
    assert_eq!(finish(stop, task).await, Ok(()));
}

#[tokio::test]
async fn credential_overrides_ambiguous_metadata_and_oversized_ignored_headers_still_reject() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let invalid = vec![
        json!({"Authorization":["Bearer other"]}),
        json!({"Proxy-Authorization":["Basic other"]}),
        json!({"Cookie":["session=other"]}),
        json!({"Mcp-Method":["tools/call"], "mcp-method":["initialize"]}),
        json!({"Mcp-Name":["first", "second"]}),
        json!({"x-ignored":["x".repeat(8192)]}),
        json!({"x-ignored":["value\r\ninjected: yes"]}),
    ];
    let count = invalid.len();
    let mut batch = Vec::new();
    for (i, headers) in invalid.into_iter().enumerate() {
        let mut cmd = command(&format!("invalid-{i}"));
        cmd["headers"] = headers;
        batch.push(cmd);
    }
    batch.push(command("valid"));
    let mut polls = Polls::start(&mut cp, vec![json!(batch)]);
    let (stop, task) = start(client);
    let request = mcp.next().await;
    assert_eq!(request.json()["id"], "valid");
    request.respond(200, r#"{"jsonrpc":"2.0","id":"valid","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled(&health).await;
    assert_eq!(health.rejected_commands(), count);
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn connection_nominated_metadata_is_not_forwarded() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut cmd = command("one");
    cmd["headers"] = json!({"Connection":["keep-alive, X-OpenAI-Session"],
        "X-OpenAI-Session":["connection-local"], "Mcp-Method":["tools/call"]});
    let mut polls = Polls::start(&mut cp, vec![json!([cmd])]);
    let (stop, task) = start(client);
    let request = mcp.next().await;
    assert!(!request.headers.contains_key("x-openai-session"));
    assert_eq!(request.headers["mcp-method"], "tools/call");
    assert_ne!(
        request.headers.get("connection").map(String::as_str),
        Some("keep-alive, X-OpenAI-Session")
    );
    request.respond(200, r#"{"jsonrpc":"2.0","id":"one","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
}
