use super::*;
use crate::shell_protocol::{
    ShellCommandExecutionState, AGENT_PROTOCOL_VERSION_QUIC_V1,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_PACKAGES,
    SHELL_CLIENT_CAPABILITY_STRUCTURED_GO_TEST_TOOL,
};

fn auth_context(username: Option<&str>, is_bootstrap: bool) -> crate::auth::AuthContext {
    let (role, scopes) = if is_bootstrap {
        ("admin".to_string(), vec!["admin".to_string()])
    } else {
        ("user".to_string(), Vec::new())
    };
    crate::auth::AuthContext {
        kind: if is_bootstrap {
            crate::auth::AuthKind::Bootstrap
        } else {
            crate::auth::AuthKind::ApiToken
        },
        user_id: username.map(|username| format!("user-{}", username)),
        username: username.map(str::to_string),
        api_key_id: username.map(|username| format!("key-{}", username)),
        role: Some(role),
        scopes,
        is_bootstrap,
        token_kind: if is_bootstrap {
            None
        } else {
            Some("user".to_string())
        },
        allowed_client_id: None,
        shared_key_hash: None,
        project_grant_id: None,
    }
}

/// Phase 3 test helper: build an agent-token AuthContext bound to
/// `username` and `allowed_client_id`, carrying the given agent scopes.
fn agent_auth_context(
    username: &str,
    allowed_client_id: &str,
    scopes: Vec<&str>,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::AgentToken,
        user_id: Some(format!("user-{}", username)),
        username: Some(username.to_string()),
        api_key_id: Some("key-agent".to_string()),
        role: Some("user".to_string()),
        scopes: scopes.into_iter().map(str::to_string).collect(),
        is_bootstrap: false,
        token_kind: Some("agent".to_string()),
        allowed_client_id: Some(allowed_client_id.to_string()),
        shared_key_hash: None,
        project_grant_id: None,
    }
}

fn open_auth_context() -> crate::auth::AuthContext {
    crate::auth::shared_key::open_anonymous_context()
}

fn oauth_bridge_auth_context(hash: &str, scopes: Vec<&str>) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: None,
        username: None,
        api_key_id: Some("oauth-access-token".to_string()),
        role: Some("shared-key".to_string()),
        scopes: scopes.into_iter().map(str::to_string).collect(),
        is_bootstrap: false,
        token_kind: Some("oauth2_shared_key".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: Some(hash.to_string()),
        project_grant_id: None,
    }
}

fn managed_oauth_auth_context(
    username: &str,
    shared_key_hash: Option<&str>,
) -> crate::auth::AuthContext {
    crate::auth::AuthContext {
        kind: crate::auth::AuthKind::OAuth2Token,
        user_id: Some(format!("user-{}", username)),
        username: Some(username.to_string()),
        api_key_id: Some("oauth-access-token".to_string()),
        role: Some("user".to_string()),
        scopes: Vec::new(),
        is_bootstrap: false,
        token_kind: Some("oauth2".to_string()),
        allowed_client_id: Some("oauth-client".to_string()),
        shared_key_hash: shared_key_hash.map(str::to_string),
        project_grant_id: None,
    }
}

fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("rust".to_string()),
        description: Some("test project".to_string()),
        hooks: vec!["doctor".to_string(), "precommit".to_string()],
        disabled: false,
        revision: None,
        git_branch: Some("codex".to_string()),
        git_head: Some("9a7d3ce".to_string()),
        git_dirty: Some(false),
        updated_at: 123456,
        shell_profile: None,
    }
}

fn runner_registration(
    client_id: &str,
    agent_instance_id: &str,
    projects: Vec<ShellAgentProjectSummary>,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: Some(async_job_capabilities()),
        projects: Some(projects),
        agent_protocol_version: None,
        policy: None,
    }
}

fn async_job_capabilities() -> ShellClientCapabilities {
    ShellClientCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        jobs: true,
        ..Default::default()
    }
}

#[path = "mod_tests/registration_projection.rs"]
mod registration_projection;

fn file_request(op: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: op.to_string(),
        client_id: "oe".to_string(),
        path: "src/auth/scopes.rs".to_string(),
        cwd: Some("/root/git/webcodex".to_string()),
        content: None,
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 0,
    }
}

#[path = "mod_tests/file_validation.rs"]
mod file_validation;

#[path = "mod_tests/shared_key_isolation.rs"]
mod shared_key_isolation;

#[path = "mod_tests/shared_key_limits.rs"]
mod shared_key_limits;

#[path = "mod_tests/shared_key_ttl.rs"]
mod shared_key_ttl;

#[test]
fn requested_by_from_auth_uses_bootstrap_username_or_anonymous() {
    let bootstrap = auth_context(None, true);
    assert_eq!(requested_by_from_auth(Some(&bootstrap)), "bootstrap");

    let alice = auth_context(Some("alice"), false);
    assert_eq!(requested_by_from_auth(Some(&alice)), "alice");

    assert_eq!(requested_by_from_auth(None), "anonymous");
}

#[test]
fn assert_shell_client_owner_enforces_owner_boundary() {
    let bootstrap = auth_context(None, true);
    assert!(assert_shell_client_owner(Some(&bootstrap), "client-1", None).is_ok());

    let alice = auth_context(Some("alice"), false);
    assert!(assert_shell_client_owner(Some(&alice), "client-1", Some("alice")).is_ok());

    let mismatch = assert_shell_client_owner(Some(&alice), "client-1", Some("bob")).unwrap_err();
    assert!(mismatch.contains("owned by bob"));
    assert!(mismatch.contains("belongs to alice"));

    let missing = assert_shell_client_owner(Some(&alice), "client-1", None).unwrap_err();
    assert_eq!(missing, "agent client client-1 has no owner");

    let anonymous = assert_shell_client_owner(None, "client-1", Some("anonymous")).unwrap_err();
    assert!(anonymous.contains("belongs to anonymous"));
}

#[path = "mod_tests/project_projection.rs"]
mod project_projection;

#[path = "mod_tests/protocol.rs"]
mod protocol;

#[path = "mod_tests/polling.rs"]
mod polling;

#[path = "mod_tests/run_enqueue.rs"]
mod run_enqueue;

#[path = "mod_tests/internal_posix.rs"]
mod internal_posix;

#[path = "mod_tests/artifact_export.rs"]
mod artifact_export;

#[path = "mod_tests/instance_lease.rs"]
mod instance_lease;

#[path = "mod_tests/connection_lease.rs"]
mod connection_lease;

#[path = "mod_tests/structured_file_delete.rs"]
mod structured_file_delete;

#[path = "mod_tests/computer_observe.rs"]
mod computer_observe;

#[path = "mod_tests/computer_snapshot_artifact.rs"]
mod computer_snapshot_artifact;

#[path = "mod_tests/computer_accessibility.rs"]
mod computer_accessibility;

#[path = "mod_tests/computer_control.rs"]
mod computer_control;

#[path = "mod_tests/computer_text_input.rs"]
mod computer_text_input;

async fn register_computer_test_client(
    registry: &ShellClientRegistry,
    client_id: &str,
    owner: &str,
    observe_capable: bool,
    accessibility_capable: bool,
    control_capable: bool,
    text_input_capable: bool,
) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "computer-inst".to_string(),
            display_name: None,
            owner: Some(owner.to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: observe_capable,
                computer_accessibility_observe: accessibility_capable,
                computer_control: control_capable,
                computer_window_activate: false,
                computer_text_input: text_input_capable,
                ..Default::default()
            }),
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
}

#[path = "mod_tests/lsp.rs"]
mod lsp;

async fn register_quic_v1_client(registry: &ShellClientRegistry, client_id: &str) {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: Some(vec![project_summary("webcodex", "/tmp/webcodex")]),
            agent_protocol_version: Some(AGENT_PROTOCOL_VERSION_QUIC_V1.to_string()),
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_transport(client_id, TRANSPORT_QUIC)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Structured delete enqueue: the authoritative `structured_file_delete`
// capability fence. The capability check and pending-request admission must
// happen under the same registry lock, so a client that re-registered without
// the capability never receives an unknown `file_delete_project_files` op and
// a failed admission leaves no request or waiter behind.
// ---------------------------------------------------------------------------

#[path = "mod_tests/quic_queueing.rs"]
mod quic_queueing;

async fn register_instance_with_capabilities(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    capabilities: ShellClientCapabilities,
) -> Result<ShellClientView, String> {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(capabilities),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
}

async fn assert_structured_delete_client_idle(registry: &ShellClientRegistry, client_id: &str) {
    let inner = registry.inner.lock().await;
    assert!(inner
        .queues_by_client
        .get(client_id)
        .is_none_or(|queue| queue.is_empty()));
    assert!(inner
        .pending_by_id
        .values()
        .all(|pending| pending.request.client_id != client_id));
}

#[path = "mod_tests/raw_shell.rs"]
mod raw_shell;

#[path = "mod_tests/job_lifecycle.rs"]
mod job_lifecycle;

#[path = "mod_tests/client_liveness.rs"]
mod client_liveness;

#[test]
fn enforce_register_owner_cases() {
    let bootstrap = auth_context(None, true);
    let user_alice = auth_context(Some("alice"), false);
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    let agent_alice = agent_auth_context(
        "alice",
        "alice-laptop",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    let agent_alice_register_only =
        agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);

    // (case, auth, client_id, owner, Ok or Err(required error fragments)).
    let cases = vec![
        // No AuthMiddleware (unit tests): defer to the middleware, which in
        // production rejects anonymous requests before the handler runs.
        (
            "no auth skips with owner",
            None,
            "client-1",
            Some("anyone"),
            Ok(()),
        ),
        (
            "no auth skips without owner",
            None,
            "client-1",
            None,
            Ok(()),
        ),
        // Bootstrap may register any owner.
        (
            "bootstrap allows missing owner",
            Some(&bootstrap),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "bootstrap allows any owner",
            Some(&bootstrap),
            "client-1",
            Some("bob"),
            Ok(()),
        ),
        (
            "shared key ignores missing owner",
            Some(&shared),
            "client-1",
            None,
            Ok(()),
        ),
        (
            "shared key ignores untrusted owner",
            Some(&shared),
            "client-1",
            Some("forged-owner"),
            Ok(()),
        ),
        // Phase 3: user tokens (Phase 2 personal API tokens) are no longer
        // allowed on agent transport endpoints. Only bootstrap or agent
        // tokens may register.
        (
            "user token is rejected",
            Some(&user_alice),
            "client-1",
            Some("alice"),
            Err(vec!["user tokens are not allowed"]),
        ),
        // Matching client_id + matching owner -> Ok.
        (
            "agent token matching client_id and owner",
            Some(&agent_alice),
            "alice-laptop",
            Some("alice"),
            Ok(()),
        ),
        // Matching client_id + missing owner -> Ok (owner filled in by the
        // caller via effective_register_owner).
        (
            "agent token matching client_id, missing owner",
            Some(&agent_alice),
            "alice-laptop",
            None,
            Ok(()),
        ),
        (
            "agent token wrong client_id rejected",
            Some(&agent_alice_register_only),
            "other-laptop",
            None,
            Err(vec!["not bound to client_id"]),
        ),
        (
            "agent token owner mismatch rejected",
            Some(&agent_alice_register_only),
            "alice-laptop",
            Some("bob"),
            Err(vec!["agent token owner is 'alice'", "bob"]),
        ),
    ];

    for (case, auth, client_id, owner, expected) in cases {
        let result = enforce_register_owner(auth, client_id, owner);
        match expected {
            Ok(()) => assert!(result.is_ok(), "case '{case}': got: {result:?}"),
            Err(fragments) => {
                let err = result.expect_err(&format!("case '{case}': expected an error"));
                for fragment in fragments {
                    assert!(
                        err.contains(fragment),
                        "case '{case}': missing '{fragment}' in error: {err}"
                    );
                }
            }
        }
    }
}

#[test]
fn effective_register_owner_agent_token_fills_username() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:register"]);
    // Missing owner -> filled with the token's username.
    assert_eq!(
        effective_register_owner(Some(&alice), None),
        Some("alice".to_string())
    );
    // Matching owner preserved.
    assert_eq!(
        effective_register_owner(Some(&alice), Some("alice")),
        Some("alice".to_string())
    );
    // Bootstrap keeps the request owner.
    let bootstrap = auth_context(None, true);
    assert_eq!(
        effective_register_owner(Some(&bootstrap), Some("bob")),
        Some("bob".to_string())
    );
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert_eq!(
        effective_register_owner(Some(&shared), Some("forged-owner")),
        None,
        "shared-key owner must not become an authorization input"
    );
}

#[test]
fn enforce_agent_transport_rejects_user_token() {
    let alice = auth_context(Some("alice"), false);
    let err = enforce_agent_transport(Some(&alice), "client-1").unwrap_err();
    assert!(err.contains("user tokens are not allowed"), "got: {}", err);
}

#[test]
fn enforce_agent_transport_agent_token_matching_client_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(enforce_agent_transport(Some(&alice), "alice-laptop").is_ok());
    let err = enforce_agent_transport(Some(&alice), "other").unwrap_err();
    assert!(err.contains("not bound"), "got: {}", err);
}

#[test]
fn enforce_agent_transport_bootstrap_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(enforce_agent_transport(Some(&bootstrap), "any-client").is_ok());
}

#[test]
fn enforce_agent_transport_direct_shared_key_succeeds() {
    let shared = crate::auth::shared_key::shared_key_context("shared-a");
    assert!(enforce_agent_transport(Some(&shared), "any-client").is_ok());
    for scope in crate::auth::AGENT_SCOPES {
        assert!(require_agent_transport_scope(Some(&shared), scope).is_ok());
    }
}

#[test]
fn enforce_agent_transport_open_anonymous_is_rejected() {
    let open = open_auth_context();
    assert!(enforce_agent_transport(Some(&open), "client-a").is_err());
    assert!(require_agent_transport_scope(Some(&open), "agent:register").is_err());
}

#[test]
fn require_agent_transport_scope_agent_token_with_scope_succeeds() {
    let alice = agent_auth_context("alice", "alice-laptop", vec!["agent:poll"]);
    assert!(require_agent_transport_scope(Some(&alice), "agent:poll").is_ok());
    assert!(require_agent_transport_scope(Some(&alice), "agent:register").is_err());
}

#[test]
fn require_agent_transport_scope_bootstrap_always_succeeds() {
    let bootstrap = auth_context(None, true);
    assert!(require_agent_transport_scope(Some(&bootstrap), "agent:register").is_ok());
}

#[test]
fn require_agent_transport_scope_user_token_rejected() {
    let alice = auth_context(Some("alice"), false);
    let err = require_agent_transport_scope(Some(&alice), "agent:register").unwrap_err();
    assert!(err.contains("missing required scope"), "got: {}", err);
}

#[test]
fn oauth_bridge_token_remains_blocked_from_agent_transport() {
    let bridge = oauth_bridge_auth_context(
        "hash-a",
        vec![
            "agent:register",
            "agent:poll",
            "agent:result",
            "agent:job_update",
        ],
    );
    assert!(!bridge.is_lightweight());
    assert!(enforce_agent_transport(Some(&bridge), "client-a")
        .unwrap_err()
        .contains("user tokens are not allowed"));
    assert!(
        require_agent_transport_scope(Some(&bridge), "agent:register")
            .unwrap_err()
            .contains("missing required scope")
    );
}

#[tokio::test]
async fn registry_rejects_enqueue_when_queue_full() {
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "full".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    // Fill the queue to the limit without any consumer draining it.
    for _ in 0..MAX_QUEUED_REQUESTS_PER_CLIENT {
        registry
            .enqueue_run(
                ShellRunRequest {
                    client_id: "full".to_string(),
                    cwd: None,
                    command: "echo hi".to_string(),
                    stdin: None,
                    timeout_secs: 5,
                    wait_timeout_secs: 0,
                },
                "tester".to_string(),
            )
            .await
            .unwrap();
    }
    // The next enqueue must be rejected with a structured error instead
    // of growing the queue unboundedly.
    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "full".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(err.contains("too many pending requests"));
    assert!(err.contains("full"));
    // The queue is exactly at the cap; memory is bounded.
    let view = registry.get_client_view("full").await.unwrap();
    assert_eq!(view.pending_requests, MAX_QUEUED_REQUESTS_PER_CLIENT);
}

#[tokio::test]
async fn registry_rejects_enqueue_when_client_offline() {
    // Registered-but-stale agents must fail fast at enqueue rather than
    // accepting work that can only time out (or fill the 256-deep queue).
    let registry = ShellClientRegistry::default();
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: "stale".to_string(),
            agent_instance_id: "inst".to_string(),
            display_name: None,
            owner: None,
            hostname: None,
            host_context: None,
            capabilities: None,
            projects: None,
            agent_protocol_version: None,
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_last_seen_for_test("stale", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;

    let err = registry
        .enqueue_run(
            ShellRunRequest {
                client_id: "stale".to_string(),
                cwd: None,
                command: "echo hi".to_string(),
                stdin: None,
                timeout_secs: 5,
                wait_timeout_secs: 0,
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        err.contains("offline"),
        "enqueue against a stale agent must fail fast as offline: {err}"
    );
    let view = registry.get_client_view("stale").await.unwrap();
    assert_eq!(view.pending_requests, 0);
    assert!(!view.connected);
}

#[path = "mod_tests/disconnect_reconciliation.rs"]
mod disconnect_reconciliation;

#[tokio::test]
async fn abandoned_sync_cleanup_removes_only_closed_waiters() {
    let registry = ShellClientRegistry::default();
    register_quic_v1_client(&registry, "oe").await;
    let (abandoned_id, abandoned_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    let (live_id, live_rx) = registry
        .enqueue_file_op(file_request("read"), "tester".to_string())
        .await
        .unwrap();
    drop(abandoned_rx);

    assert_eq!(registry.cancel_abandoned_sync_requests().await, 1);
    assert_eq!(
        registry
            .get_client_view("oe")
            .await
            .unwrap()
            .pending_requests,
        1
    );
    assert!(
        !registry.cancel_request(&abandoned_id).await,
        "closed-waiter request should already be removed"
    );
    assert_eq!(
        registry.cancel_request_dispatch_state(&live_id).await,
        Some(false),
        "cleanup must preserve an undispatched synchronous request with a live receiver"
    );
    drop(live_rx);
}

// ------------------------------------------------------------------------
// Agent instance identity / lease model (Phase 1)
// ------------------------------------------------------------------------

/// Helper: register a client with an explicit `agent_instance_id`.
async fn register_with_instance(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
) -> ShellClientView {
    registry
        .register(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: Some(async_job_capabilities()),
            projects: None,
            agent_protocol_version: Some("polling-v1".to_string()),
            policy: None,
        })
        .await
        .unwrap()
}

#[path = "mod_tests/project_unregister.rs"]
mod project_unregister;

#[path = "mod_tests/job_log_wait.rs"]
mod job_log_wait;
