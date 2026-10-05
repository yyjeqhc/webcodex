mod support;
use std::time::Duration;
use support::*;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, ControlPlaneProxy, Credential, FixedMcpTarget, TunnelClient,
};

fn bound(cp: &str, mcp: &str, id: &str, key: &str, proxy: ControlPlaneProxy) -> TunnelClient {
    TunnelClient::new_with_proxy(
        ControlPlaneIdentity::new(cp, id, Credential::bearer(key).unwrap()).unwrap(),
        FixedMcpTarget::new(
            &format!("{mcp}/mcp"),
            Credential::bearer("local-fixture").unwrap(),
        )
        .unwrap(),
        DeadlinePolicy::default(),
        Limits::default(),
        proxy,
    )
    .unwrap()
}

#[tokio::test]
async fn two_explicit_profiles_keep_credentials_and_health_independent() {
    let mut a = Server::new().await;
    let mut b = Server::new().await;
    let mcp = Server::new().await;
    let first = bound(
        &a.url,
        &mcp.url,
        "tunnel_first",
        "first-fixture",
        ControlPlaneProxy::Direct,
    );
    let second = bound(
        &b.url,
        &mcp.url,
        "tunnel_second",
        "second-fixture",
        ControlPlaneProxy::Direct,
    );
    let ah = first.health();
    let bh = second.health();
    let (as_, at) = start(first);
    let (bs, bt) = start(second);
    let ar = a.next().await;
    let br = b.next().await;
    assert!(ar.path.contains("tunnel_first"));
    assert_eq!(ar.headers["authorization"], "Bearer first-fixture");
    assert!(br.path.contains("tunnel_second"));
    assert_eq!(br.headers["authorization"], "Bearer second-fixture");
    ar.respond(204, "");
    tokio::time::timeout(Duration::from_secs(5), async {
        while !ah.is_ready() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!bh.is_ready());
    assert_eq!(finish(as_, at).await, Ok(()));
    assert!(!bt.is_finished());
    br.respond(204, "");
    assert_eq!(finish(bs, bt).await, Ok(()));
}

#[test]
fn inherited_process_credentials_cannot_override_explicit_profiles() {
    // Child-local environment: this fixture never mutates the test runner's globals.
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "two_explicit_profiles_keep_credentials_and_health_independent",
        ])
        .env("CONTROL_PLANE_TUNNEL_ID", "tunnel_inherited_wrong_identity")
        .env("CONTROL_PLANE_API_KEY", "inherited-wrong-fixture-key")
        .env("HTTP_PROXY", "http://127.0.0.1:1")
        .env("HTTPS_PROXY", "http://127.0.0.1:1")
        .env("ALL_PROXY", "http://127.0.0.1:1")
        .env("NO_PROXY", "")
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "explicit bindings failed under hostile inherited configuration"
    );
}

#[tokio::test]
async fn explicit_proxy_only_receives_control_plane_not_local_mcp_credentials() {
    let mut proxy = Server::new().await;
    let mut mcp = Server::new().await;
    let client = bound(
        "http://127.0.0.1:9",
        &mcp.url,
        "tunnel_proxy",
        "control-fixture",
        ControlPlaneProxy::Explicit(proxy.url.clone()),
    );
    let health = client.health();
    let (stop, task) = start(client);
    let poll = proxy.next().await;
    assert!(poll.path.starts_with("http://127.0.0.1:9/"));
    assert_eq!(poll.headers["authorization"], "Bearer control-fixture");
    poll.respond(
        200,
        &serde_json::json!({"commands":[command("a")]}).to_string(),
    );
    let local = mcp.next().await;
    assert_eq!(local.path, "/mcp");
    assert_eq!(local.headers["authorization"], "Bearer local-fixture");
    assert!(!local.headers.contains_key("proxy-authorization"));
    stop.send(()).unwrap();
    local.respond(200, r#"{"jsonrpc":"2.0","id":"a","result":{}}"#);
    loop {
        let request = proxy.next().await;
        assert_eq!(request.headers["authorization"], "Bearer control-fixture");
        let terminal = request.path.ends_with("/response");
        request.respond(if terminal { 200 } else { 204 }, "");
        if terminal {
            break;
        }
    }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .unwrap()
            .unwrap(),
        Ok(())
    );
    assert!(!health.has_uncertain_work());
}

#[test]
fn proxy_debug_and_validation_never_disclose_credentials() {
    let proxy =
        ControlPlaneProxy::Explicit("http://fixture-user:fixture-password@127.0.0.1:7890".into());
    let debug = format!("{proxy:?}");
    assert!(!debug.contains("fixture-"));
    for url in [
        "file:///private/key",
        "http://127.0.0.1:7890/path",
        "http://127.0.0.1:7890/?key=secret",
    ] {
        let result = TunnelClient::new_with_proxy(
            ControlPlaneIdentity::new(
                "https://api.openai.com",
                "fixture",
                Credential::bearer("fixture").unwrap(),
            )
            .unwrap(),
            FixedMcpTarget::unauthenticated("http://127.0.0.1:9/mcp").unwrap(),
            DeadlinePolicy::default(),
            Limits::default(),
            ControlPlaneProxy::Explicit(url.into()),
        );
        assert!(matches!(
            result,
            Err(webcodex_openai_tunnel::Error::Configuration)
        ));
    }
}
