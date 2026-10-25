mod support;
#[path = "support/credential_transition.rs"]
mod transition_support;
use serde_json::{json, Value};
use std::path::Path;
use support::*;
use transition_support::*;
use webcodex_openai_tunnel::{Credential, TunnelClient};

fn rejected_commands(prefix: &str) -> Vec<Value> {
    let mut commands = Vec::new();
    for (name, timestamp) in [
        ("before", Some(json!(BEFORE))),
        ("equal", Some(json!(CUTOFF))),
        ("missing", None),
        ("invalid", Some(json!("not-a-timestamp"))),
        ("null", Some(Value::Null)),
        ("numeric", Some(json!(4070908801u64))),
    ] {
        commands.push(stamped(
            command(&format!("{prefix}-{name}-rpc")),
            timestamp.clone(),
        ));
        commands.push(stamped(
            termination(&format!("{prefix}-{name}-end")),
            timestamp,
        ));
    }
    commands
}

#[tokio::test]
async fn authorized_transition_preserves_receipts_and_cutoff_across_clean_restarts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let (old, guard) = bound(&cp, &mcp, "old-local")
        .with_restart_recovery(&root, None, |_| true)
        .unwrap();
    batch(
        old,
        &mut cp,
        &mut mcp,
        vec![command("old-effect")],
        &[("POST", "old-effect")],
        0,
        "Bearer old-local",
    )
    .await;
    guard.finish().unwrap();

    let old = bound(&cp, &mcp, "old-local");
    let transition = old
        .authorize_local_credential_transition(
            &root,
            Credential::bearer("new-local").unwrap(),
            cutoff(),
            0,
        )
        .unwrap();
    assert_eq!(transition.revision, 1);
    assert!(!transition.replayed);
    let replay = old
        .authorize_local_credential_transition(
            &root,
            Credential::bearer("new-local").unwrap(),
            cutoff(),
            0,
        )
        .unwrap();
    assert_eq!(replay.revision, transition.revision);
    assert_eq!(replay.cutoff_seconds, transition.cutoff_seconds);
    assert!(replay.replayed);
    assert!(old.with_restart_recovery(&root, None, |_| true).is_err());

    for round in 0..2 {
        let (new, guard) = bound(&cp, &mcp, "new-local")
            .with_restart_recovery(&root, None, |_| true)
            .unwrap();
        let mut commands = rejected_commands(&format!("round-{round}"));
        let rejects = commands.len() + 1;
        // A newly dated copy of an old request must still fail the receipt fence.
        commands.push(stamped(command("old-effect"), Some(json!(AFTER))));
        let fresh = format!("fresh-{round}");
        commands.push(stamped(command(&fresh), Some(json!(AFTER))));
        commands.push(stamped(
            termination(&format!("end-{round}")),
            Some(json!(AFTER)),
        ));
        batch(
            new,
            &mut cp,
            &mut mcp,
            commands,
            &[("POST", &fresh), ("DELETE", "")],
            rejects,
            "Bearer new-local",
        )
        .await;
        guard.finish().unwrap();
    }
}

#[tokio::test]
async fn absent_transition_keeps_missing_and_invalid_enqueue_metadata_compatible() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mut cp = Server::new().await;
    let mut mcp = Server::new().await;
    let (client, guard) = bound(&cp, &mcp, "old-local")
        .with_restart_recovery(&root, None, |_| true)
        .unwrap();
    let commands = vec![
        command("missing"),
        stamped(command("invalid"), Some(json!("invalid"))),
        termination("end-missing"),
        stamped(termination("end-invalid"), Some(json!(false))),
    ];
    batch(
        client,
        &mut cp,
        &mut mcp,
        commands,
        &[
            ("POST", "missing"),
            ("POST", "invalid"),
            ("DELETE", ""),
            ("DELETE", ""),
        ],
        0,
        "Bearer old-local",
    )
    .await;
    guard.finish().unwrap();
}

#[tokio::test]
async fn transition_requires_the_existing_stopped_exact_binding() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let cp = Server::new().await;
    let mcp = Server::new().await;
    let other_cp = Server::new().await;
    let target = format!("{}/mcp", mcp.url);
    let authorize = |client: TunnelClient, root: &Path| {
        client.authorize_local_credential_transition(
            root,
            Credential::bearer("new-local").unwrap(),
            cutoff(),
            0,
        )
    };
    assert!(authorize(bound(&cp, &mcp, "old-local"), &root).is_err());
    let (active, guard) = bound(&cp, &mcp, "old-local")
        .with_restart_recovery(&root, None, |_| true)
        .unwrap();
    assert!(authorize(active, &root).is_err());
    // A new client cannot authorize while an existing guard owns the history.
    assert!(authorize(bound(&cp, &mcp, "old-local"), &root).is_err());
    guard.finish().unwrap();
    for wrong in [
        binding(&other_cp.url, "tunnel_fixture", &target, "old-local"),
        binding(&cp.url, "tunnel_other", &target, "old-local"),
        binding(
            &cp.url,
            "tunnel_fixture",
            &format!("{}/other", mcp.url),
            "old-local",
        ),
        bound(&cp, &mcp, "wrong-old-local"),
    ] {
        assert!(authorize(wrong, &root).is_err());
    }
    assert!(authorize(bound(&cp, &mcp, "old-local"), &root).is_ok());
}
