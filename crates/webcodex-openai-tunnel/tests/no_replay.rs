mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    Error,
};

#[tokio::test]
async fn mcp_effect_is_dispatched_once_when_terminal_response_delivery_is_ambiguous() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(
        &mut cp,
        vec![json!([command("one")]), json!([command("one")])],
    );
    let (stop, task) = start(client);
    let effect = mcp.next().await;
    assert_eq!(effect.headers["authorization"], "Bearer local-secret");
    assert!(!effect.headers.contains_key("x-tunnel-shard-token"));
    effect.respond(200, r#"{"jsonrpc":"2.0","id":"one","result":{}}"#);
    let first = polls.response().await;
    let original = first.body.clone();
    assert_eq!(first.headers["x-tunnel-shard-token"], "fixture-shard");
    first.disconnect();
    let second = polls.response().await;
    assert_eq!(second.body, original);
    second.respond(503, "");
    let third = polls.response().await;
    assert_eq!(third.body, original);
    third.respond(200, "{}");
    settled(&health).await;
    assert!(mcp.requests.try_recv().is_err());
    assert_eq!(finish(stop, task).await, Ok(()));
}

#[tokio::test]
async fn mcp_disconnect_after_body_arrival_is_not_replayed() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let mut polls = Polls::start(
        &mut cp,
        vec![json!([command("one")]), json!([command("one")])],
    );
    let (stop, task) = start(client);
    mcp.next().await.disconnect();
    polls.response().await.respond(200, "{}");
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn unknown_type_and_protected_header_override_never_dispatch() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut unknown = command("unknown");
    unknown["command_type"] = json!("future");
    let mut evil = command("evil");
    evil["headers"] = json!({"Authorization":["Bearer attacker"]});
    let mut polls = Polls::start(&mut cp, vec![json!([unknown, evil, command("good")])]);
    let (stop, task) = start(client);
    let request = mcp.next().await;
    assert_eq!(request.json()["id"], "good");
    request.respond(200, r#"{"jsonrpc":"2.0","id":"good","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled(&health).await;
    assert_eq!(health.rejected_commands(), 2);
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(mcp.requests.try_recv().is_err());
}
