mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::policy::{DeadlinePolicy, Limits};

#[tokio::test]
async fn ambiguous_notification_is_not_replayed_and_final_result_is_delivered() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("one")])]);
    let (stop, task) = start(client);
    mcp.next().await.raw(200,"Content-Type: text/event-stream\r\n",
        "\u{feff}:comment\r\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\r\n\r\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\ndata: {\"jsonrpc\":\"2.0\",\ndata: \"id\":\"one\",\"result\":{}}\r\r");
    let progress = polls.response().await;
    assert_eq!(progress.json()["resp_type"], "jsonrpc_notify");
    progress.disconnect();
    let terminal = polls.response().await;
    assert_eq!(terminal.json()["resp_type"], "jsonrpc_response");
    terminal.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(polls.responses.try_recv().is_err());
}
#[tokio::test]
async fn valid_jsonrpc_error_and_response_headers_are_preserved() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("one")])]);
    let (stop, task) = start(client);
    let error = json!({"jsonrpc":"2.0","id":"one","error":{"code":-32004,"message":"version","data":{"supported":["x"]}}});
    mcp.next().await.raw(400,"Content-Type: application/json\r\nMcp-Session-Id: s\r\nWWW-Authenticate: first\r\nWWW-Authenticate: second\r\nConnection: Mcp-Session-Id\r\nX-Private: secret\r\n",&error.to_string());
    let response = polls.response().await;
    let value = response.json();
    assert_eq!(value["resp_json"], error);
    assert_eq!(value["resp_code"], 400);
    assert!(value["resp_headers"].get("mcp-session-id").is_none());
    assert!(value["resp_headers"].get("x-private").is_none());
    assert_eq!(
        value["resp_headers"]["www-authenticate"],
        json!(["first", "second"])
    );
    response.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
}
#[tokio::test]
async fn session_termination_uses_delete_and_acknowledges_without_jsonrpc() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(
        &mut cp,
        vec![
            json!([{"request_id":"end","shard_token":"s","command_type":"session_termination","headers":{"Mcp-Session-Id":["session"]}}]),
        ],
    );
    let (stop, task) = start(client);
    let request = mcp.next().await;
    assert_eq!(request.method, "DELETE");
    assert_eq!(request.headers["mcp-session-id"], "session");
    request.respond(204, "");
    let response = polls.response().await;
    assert_eq!(response.json()["resp_type"], "session_termination_response");
    assert!(response.json().get("resp_json").is_none());
    response.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
}

#[tokio::test]
async fn notification_429_retries_same_payload_but_not_mcp_execution() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (stop, task) = start(client);
    mcp.next().await.raw(200,"Content-Type: text/event-stream\r\n",
        "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":\"a\",\"result\":{}}\n\n");
    let first = polls.response().await;
    let payload = first.body.clone();
    first.respond(429, "");
    let retried = polls.response().await;
    assert_eq!(retried.body, payload);
    retried.respond(200, "{}");
    let terminal = polls.response().await;
    assert_eq!(terminal.json()["resp_type"], "jsonrpc_response");
    terminal.respond(404, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn forwarding_preserves_raw_numeric_payloads_and_error_data() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let (stop, task) = start(client);
    let payload = r#"{"jsonrpc":"2.0","id":"raw","method":"tools/call","params":{"large":900719925474099312345,"precise":0.123456789012345678901}}"#;
    cp.next().await.respond(200, &format!(r#"{{"commands":[{{"request_id":"raw","shard_token":"s","command_type":"jsonrpc","jsonrpc":{payload}}}]}}"#));
    let mut polls = Polls::start(&mut cp, vec![]);
    let effect = mcp.next().await;
    assert_eq!(std::str::from_utf8(&effect.body).unwrap(), payload);
    let error = r#"{"jsonrpc":"2.0","id":"raw","error":{"code":-32004,"message":"version","data":{"large":900719925474099312345,"precise":0.123456789012345678901}}}"#;
    effect.respond(400, error);
    let returned = polls.response().await;
    assert!(std::str::from_utf8(&returned.body).unwrap().contains(error));
    returned.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
}
