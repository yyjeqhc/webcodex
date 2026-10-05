mod support;
use serde_json::json;
use std::time::Duration;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    Error,
};

#[tokio::test]
async fn expired_and_malformed_deadlines_do_not_dispatch() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut expired = command("expired");
    expired["response_timeout"] = json!("0s");
    let mut malformed = command("malformed");
    malformed["response_timeout"] = json!("later");
    let mut polls = Polls::start(&mut cp, vec![json!([expired, malformed, command("good")])]);
    let (stop, task) = start(client);
    let r = mcp.next().await;
    assert_eq!(r.json()["id"], "good");
    r.respond(200, r#"{"jsonrpc":"2.0","id":"good","result":{}}"#);
    polls.response().await.respond(200, "{}");
    settled(&health).await;
    assert_eq!(finish(stop, task).await, Ok(()));
    assert!(mcp.requests.try_recv().is_err());
}
#[tokio::test]
async fn deadline_during_dispatch_cancels_without_late_error_or_replay() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(
        &cp,
        &mcp,
        Limits::default(),
        DeadlinePolicy::RequireFinite {
            max_duration: Duration::from_millis(100),
        },
    );
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (stop, task) = start(client);
    let held = mcp.next().await;
    // Deliberately cross a real transport deadline; no retries or larger timeouts.
    tokio::time::sleep(Duration::from_millis(150)).await;
    held.respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(polls.responses.try_recv().is_err());
}
#[tokio::test]
async fn deadline_after_mcp_result_stops_response_retry() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(
        &cp,
        &mcp,
        Limits::default(),
        DeadlinePolicy::RequireFinite {
            max_duration: Duration::from_millis(150),
        },
    );
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (stop, task) = start(client);
    mcp.next()
        .await
        .respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    // Retry-After cannot extend the original command deadline.
    polls.response().await.raw(503, "Retry-After: 60\r\n", "");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(polls.responses.try_recv().is_err());
    assert!(mcp.requests.try_recv().is_err());
}
