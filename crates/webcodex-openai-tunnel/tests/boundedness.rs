mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    Error,
};

#[tokio::test]
async fn excess_over_poll_hint_is_drained_with_bounded_concurrency() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let limits = Limits {
        concurrency: 1,
        ingress_commands: 4,
        ..Limits::default()
    };
    let client = client(&cp, &mcp, limits, DeadlinePolicy::default());
    let health = client.health();
    let (stop, task) = start(client);
    let poll = cp.next().await;
    assert!(poll.path.contains("limit=1"));
    poll.respond(
        200,
        &json!({"commands":[command("a"),command("b"),command("c")]}).to_string(),
    );
    let mut polls = Polls::start(&mut cp, vec![]);
    for id in ["a", "b", "c"] {
        let effect = mcp.next().await;
        assert_eq!(effect.json()["id"], id);
        assert!(mcp.requests.try_recv().is_err());
        effect.respond(
            200,
            &json!({"jsonrpc":"2.0","id":id,"result":{}}).to_string(),
        );
        polls.response().await.respond(200, "{}");
    }
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
}
#[tokio::test]
async fn poll_batch_beyond_ingress_bound_stops_before_dispatch() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(
        &cp,
        &mcp,
        Limits {
            ingress_commands: 1,
            ..Limits::default()
        },
        DeadlinePolicy::default(),
    );
    let _polls = Polls::start(&mut cp, vec![json!([command("a"), command("b")])]);
    let (_stop, task) = start(client);
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Capacity)
    );
    assert!(mcp.requests.try_recv().is_err());
}
#[tokio::test]
async fn receipt_exhaustion_does_not_evict_old_commands() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(
        &cp,
        &mcp,
        Limits {
            ingress_commands: 1,
            lifetime_commands: 1,
            ..Limits::default()
        },
        DeadlinePolicy::default(),
    );
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")]), json!([command("a")])]);
    let (_stop, task) = start(client);
    mcp.next()
        .await
        .respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    polls.response().await.respond(200, "{}");
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Capacity)
    );
    assert_eq!(polls.count.load(std::sync::atomic::Ordering::SeqCst), 1);
}
