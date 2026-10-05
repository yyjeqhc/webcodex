mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    Error,
};

#[tokio::test]
async fn disconnected_notification_returns_failure_ack_but_retains_uncertainty() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let mut polls = Polls::start(
        &mut cp,
        vec![json!([{
            "request_id":"notify", "shard_token":"s", "command_type":"jsonrpc",
            "jsonrpc":{"jsonrpc":"2.0","method":"notifications/initialized"}
        }])],
    );
    let (stop, task) = start(client);
    mcp.next().await.disconnect();
    let returned = polls.response().await;
    assert_eq!(returned.json()["resp_type"], "notify_ack");
    assert_eq!(returned.json()["resp_code"], 502);
    assert!(returned.json().get("resp_json").is_none());
    returned.respond(200, "{}");
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn attempted_sse_terminal_is_not_replaced_after_delivery_failure() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let limits = Limits {
        concurrency: 1,
        response_attempts: 1,
        ..Limits::default()
    };
    let client = client(&cp, &mcp, limits, DeadlinePolicy::default());
    let mut polls = Polls::start(&mut cp, vec![json!([command("one"), command("two")])]);
    let (stop, task) = start(client);
    mcp.next().await.raw(
        200,
        "Content-Type: text/event-stream\r\n",
        "data: {\"jsonrpc\":\"2.0\",\"id\":\"one\",\"result\":{}}\n\n",
    );
    let first = polls.response().await;
    assert_eq!(first.json()["resp_json"]["id"], "one");
    first.disconnect();
    // With one worker, seeing the second dispatch proves the first worker
    // completed; a replacement terminal must not have been queued before it.
    mcp.next()
        .await
        .respond(200, r#"{"jsonrpc":"2.0","id":"two","result":{}}"#);
    let second = polls.response().await;
    assert_eq!(second.json()["request_id"], "two");
    second.respond(200, "{}");
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn empty_sse_priming_event_keeps_the_original_exchange_alive() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("one")])]);
    let (stop, task) = start(client);
    mcp.next().await.raw(
        200,
        "Content-Type: text/event-stream\r\n",
        "id: priming\ndata:\n\ndata: {\"jsonrpc\":\"2.0\",\"id\":\"one\",\"result\":{}}\n\n",
    );
    let returned = polls.response().await;
    assert_eq!(returned.json()["resp_json"]["result"], json!({}));
    returned.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn incomplete_sse_returns_a_terminal_failure_without_replaying_mcp() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(
        &mut cp,
        vec![json!([command("one")]), json!([command("one")])],
    );
    let (stop, task) = start(client);
    mcp.next().await.raw(
        200,
        "Content-Type: text/event-stream\r\n",
        "data: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/progress\"}\n\n",
    );
    let progress = polls.response().await;
    assert_eq!(progress.json()["resp_type"], "jsonrpc_notify");
    progress.respond(200, "{}");
    let returned = polls.response().await;
    let value = returned.json();
    assert_eq!(value["resp_type"], "jsonrpc_response");
    assert_eq!(value["resp_code"], 502);
    assert_eq!(value["resp_json"]["id"], "one");
    assert_eq!(value["resp_json"]["error"]["code"], -32603);
    assert_eq!(
        value["resp_json"]["error"]["data"]["tunnel_failure"]["upstream_response_received"],
        true
    );
    returned.respond(200, "{}");
    assert!(health.has_uncertain_work());
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn oversized_mcp_body_returns_bounded_failure_without_echoing_body() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let limits = Limits {
        body_bytes: 1024,
        event_bytes: 512,
        ..Limits::default()
    };
    let client = client(&cp, &mcp, limits, DeadlinePolicy::default());
    let mut polls = Polls::start(&mut cp, vec![json!([command("one")])]);
    let (stop, task) = start(client);
    mcp.next()
        .await
        .respond(200, &"private-target-text".repeat(200));
    let returned = polls.response().await;
    assert!(returned.body.len() <= 1024);
    assert_eq!(returned.json()["resp_code"], 502);
    assert_eq!(returned.json()["resp_json"]["error"]["code"], -32603);
    assert!(!String::from_utf8_lossy(&returned.body).contains("private-target-text"));
    returned.respond(200, "{}");
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn rejected_notification_and_termination_preserve_target_status() {
    for (termination, status) in [(false, 400), (true, 405), (true, 404)] {
        let mut cp = Server::new().await;
        let mut mcp = Server::new().await;
        let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
        let health = client.health();
        let command = if termination {
            json!({"request_id":"one","shard_token":"s","command_type":"session_termination","headers":{"Mcp-Session-Id":["session"]}})
        } else {
            json!({"request_id":"one","shard_token":"s","command_type":"jsonrpc","jsonrpc":{"jsonrpc":"2.0","method":"notifications/initialized"}})
        };
        let mut polls = Polls::start(&mut cp, vec![json!([command])]);
        let (stop, task) = start(client);
        mcp.next().await.respond(status, "");
        let returned = polls.response().await;
        assert_eq!(returned.json()["resp_code"], status);
        assert_eq!(
            returned.json()["resp_type"],
            if termination {
                "session_termination_response"
            } else {
                "notify_ack"
            }
        );
        assert!(returned.json().get("resp_json").is_none());
        returned.respond(200, "{}");
        settled(&health).await;
        assert_eq!(finish(stop, task).await, Ok(()));
        assert!(mcp.requests.try_recv().is_err());
    }
}
