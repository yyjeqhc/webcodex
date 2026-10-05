mod support;
use serde_json::json;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    Error,
};

#[tokio::test]
async fn shutdown_with_inflight_effect_returns_uncertain_and_closes_owned_work() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (stop, stopped) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(client.run_with_drain(
        async {
            let _ = stopped.await;
        },
        std::time::Duration::from_millis(100),
    ));
    let held = mcp.next().await;
    assert_eq!(finish(stop, task).await, Err(Error::Uncertain));
    assert!(!health.is_ready());
    held.respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    assert!(polls.responses.try_recv().is_err());
}
#[tokio::test]
async fn ingress_stop_drains_admitted_queue_and_confirms_delivery_without_replay() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(
        &cp,
        &mcp,
        Limits {
            concurrency: 1,
            ..Limits::default()
        },
        DeadlinePolicy::default(),
    );
    let health = client.health();
    let mut polls = Polls::start(&mut cp, vec![json!([command("a"), command("b")])]);
    let (stop, task) = start(client);
    let first = mcp.next().await;
    stop.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while health.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    // The local MCP server remains available throughout ingress-stop/drain.
    first.respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    polls.response().await.respond(200, "");
    let second = mcp.next().await;
    assert_eq!(second.json()["id"], "b");
    second.respond(200, r#"{"jsonrpc":"2.0","id":"b","result":{}}"#);
    polls.response().await.respond(200, "");
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Ok(())
    );
    assert!(!health.has_uncertain_work());
    assert_eq!(polls.count.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn control_plane_auth_rejection_stops_without_retry() {
    for status in [401, 403] {
        let mut cp = Server::new().await;
        let mcp = Server::new().await;
        let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
        let (_stop, task) = start(client);
        cp.next().await.respond(status, "");
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap(),
            Err(Error::Authentication)
        );
        assert!(cp.requests.try_recv().is_err());
    }
}
#[tokio::test]
async fn same_origin_and_cross_origin_redirects_never_forward_credentials() {
    for same in [true, false] {
        let mut cp = Server::new().await;
        let mut mcp = Server::new().await;
        let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
        let (_stop, task) = start(client);
        let target = if same { &cp.url } else { &mcp.url };
        let location = format!("Location: {target}/other\r\n");
        cp.next().await.raw(307, &location, "");
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap(),
            Err(Error::Redirect)
        );
        assert!(cp.requests.try_recv().is_err());
        assert!(mcp.requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn mcp_redirect_is_not_followed_even_on_the_same_origin() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let _polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (_stop, task) = start(client);
    let location = format!("Location: {}/changed-target\r\n", mcp.url);
    mcp.next().await.raw(307, &location, "");
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Redirect)
    );
    assert!(mcp.requests.try_recv().is_err());
}

#[tokio::test]
async fn response_redirect_never_sends_command_token_to_new_origin() {
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let mut other = Server::new().await;
    let client = client(&cp, &mcp, Limits::default(), DeadlinePolicy::default());
    let mut polls = Polls::start(&mut cp, vec![json!([command("a")])]);
    let (_stop, task) = start(client);
    mcp.next()
        .await
        .respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    polls
        .response()
        .await
        .raw(307, &format!("Location: {}/capture\r\n", other.url), "");
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Err(Error::Redirect)
    );
    assert!(other.requests.try_recv().is_err());
}
