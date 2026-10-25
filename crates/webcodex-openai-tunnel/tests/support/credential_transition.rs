#![allow(dead_code)]
use crate::support::*;
use serde_json::{json, Value};
use std::time::Duration;
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, Credential, FixedMcpTarget, OperatorEnqueueCutoff, TunnelClient,
};

// Synthetic operator attestation and control-plane timestamps; these fixtures do
// not establish the clock, enqueue semantics or readiness of a real Host.
pub(crate) const CUTOFF: &str = "2099-01-01T00:00:00Z";
pub(crate) const AFTER: &str = "2099-01-01T00:00:00.000000001Z";
pub(crate) const BEFORE: &str = "2098-12-31T23:59:59Z";

pub(crate) fn bound(cp: &Server, mcp: &Server, credential: &str) -> TunnelClient {
    binding(
        &cp.url,
        "tunnel_fixture",
        &format!("{}/mcp", mcp.url),
        credential,
    )
}
pub(crate) fn binding(cp: &str, tunnel: &str, target: &str, credential: &str) -> TunnelClient {
    TunnelClient::new(
        ControlPlaneIdentity::new(cp, tunnel, Credential::bearer("control-secret").unwrap())
            .unwrap(),
        FixedMcpTarget::new(target, Credential::bearer(credential).unwrap()).unwrap(),
        DeadlinePolicy::default(),
        Limits {
            concurrency: 1,
            ..Limits::default()
        },
    )
    .unwrap()
}
pub(crate) fn cutoff() -> OperatorEnqueueCutoff {
    OperatorEnqueueCutoff::attest_future_utc(CUTOFF).unwrap()
}
pub(crate) fn stamped(mut command: Value, stamp: Option<Value>) -> Value {
    if let Some(stamp) = stamp {
        command["created_at"] = stamp;
    }
    command
}
pub(crate) fn termination(id: &str) -> Value {
    json!({"request_id":id, "shard_token":"fixture-shard",
        "command_type":"session_termination", "channel":"main",
        "headers":{"Mcp-Session-Id":["fixture-session"]}})
}

// Pump one complete ingress batch and acknowledge every expected local effect.
// Completion uses explicit admission/rejection counts, not a timing-only claim
// that a forbidden request did not arrive. Both fixture sockets remain reusable
// for the next protected run with the exact same bound URLs.
pub(crate) async fn batch(
    client: TunnelClient,
    cp: &mut Server,
    mcp: &mut Server,
    commands: Vec<Value>,
    effects: &[(&str, &str)],
    rejects: usize,
    authorization: &str,
) {
    let health = client.health();
    let (stop, task) = start(client);
    cp.next()
        .await
        .respond(200, &json!({"commands":commands}).to_string());
    tokio::time::timeout(Duration::from_secs(5), async {
        let mut dispatched = 0;
        let mut returned = 0;
        loop {
            if returned == effects.len()
                && health.rejected_commands() == rejects
                && !health.has_uncertain_work()
            {
                break;
            }
            tokio::select! {
                request = mcp.requests.recv() => {
                    let request = request.unwrap();
                    let (method, id) = effects.get(dispatched).expect("unexpected MCP dispatch");
                    assert_eq!(request.method, *method);
                    assert_eq!(request.headers["authorization"], authorization);
                    if *method == "POST" {
                        assert_eq!(request.json()["id"], *id);
                        request.respond(200, &json!({"jsonrpc":"2.0","id":id,"result":{}}).to_string());
                    } else {
                        request.respond(204, "");
                    }
                    dispatched += 1;
                },
                request = cp.requests.recv() => {
                    let request = request.unwrap();
                    assert_eq!(request.headers["authorization"], "Bearer control-secret");
                    if request.path.contains("/poll?") {
                        request.respond(204, "");
                    } else {
                        assert!(request.path.ends_with("/response"));
                        returned += 1;
                        assert!(returned <= effects.len());
                        request.respond(200, "{}");
                    }
                },
                _ = tokio::task::yield_now() => {},
            }
        }
        assert_eq!(dispatched, effects.len());
    }).await.unwrap();
    // Hold the sole next poll until shutdown so an old owner's late request
    // cannot consume the next protected run's command batch.
    let held_poll = cp.next().await;
    assert!(held_poll.path.contains("/poll?"));
    assert_eq!(finish(stop, task).await, Ok(()));
    let (valid, unverifiable) = health.enqueue_timestamp_counts();
    assert_eq!(valid + unverifiable, commands.len() as u64);
    held_poll.disconnect();
    assert!(mcp.requests.try_recv().is_err());
    assert!(cp.requests.try_recv().is_err());
}
