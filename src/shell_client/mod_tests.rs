use super::*;
use crate::shell_protocol::{
    AgentProtocolGenerationNumber, ShellCommandExecutionState, AGENT_PROTOCOL_GENERATION_V2,
    AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES,
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

fn project_summary(id: &str, path: &str) -> ShellAgentProjectSummary {
    ShellAgentProjectSummary {
        id: id.to_string(),
        name: Some(id.to_string()),
        path: path.to_string(),
        allow_patch: true,
        kind: Some("rust".to_string()),
        registration_source: None,
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
    _projects: Vec<ShellAgentProjectSummary>,
) -> ShellClientRegisterRequest {
    ShellClientRegisterRequest {
        process_started_at: None,
        build: None,
        job_concurrency_limit: None,
        job_inventory: None,
        coding_agent_providers: None,
        coding_agent_inventory: None,
        client_id: client_id.to_string(),
        agent_instance_id: agent_instance_id.to_string(),
        agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
        display_name: None,
        owner: None,
        hostname: None,
        host_context: None,
        capabilities: async_job_capabilities(),
        policy: None,
    }
}

fn v2_baseline_capabilities() -> ShellClientCapabilities {
    let mut value = serde_json::Map::new();
    // Shell remains RegistrationRequired in generation 2, so pin it false in
    // the baseline-only fixture.
    value.insert("shell".to_string(), serde_json::Value::Bool(false));
    for capability in AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES {
        value.insert((*capability).to_string(), serde_json::Value::Bool(true));
    }
    serde_json::from_value(serde_json::Value::Object(value)).unwrap()
}

fn current_runner_registration(
    registration: ShellClientRegisterRequest,
) -> ShellClientRegisterRequest {
    crate::test_support::current_runner_registration(registration)
}

fn async_job_capabilities() -> ShellClientCapabilities {
    let mut capabilities = v2_baseline_capabilities();
    capabilities.shell = true;
    capabilities
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

#[path = "mod_tests/project_projection.rs"]
mod project_projection;

#[path = "mod_tests/project_inventory.rs"]
mod project_inventory;

#[path = "mod_tests/auth_owner.rs"]
mod auth_owner;

#[path = "mod_tests/protocol.rs"]
mod protocol;

#[path = "mod_tests/capabilities.rs"]
mod capabilities;

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

#[path = "mod_tests/apply_text_edit_occurrence.rs"]
mod apply_text_edit_occurrence;

#[path = "mod_tests/apply_patch.rs"]
mod apply_patch;

#[path = "mod_tests/apply_text_edit_line_scope.rs"]
mod apply_text_edit_line_scope;

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
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "computer-inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some(owner.to_string()),
            hostname: None,
            host_context: None,
            capabilities: ShellClientCapabilities {
                shell: true,
                file_read: true,
                computer_observe: observe_capable,
                computer_accessibility_observe: accessibility_capable,
                computer_control: control_capable,
                computer_window_activate: false,
                computer_text_input: text_input_capable,
                ..Default::default()
            },
            policy: None,
        }))
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
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: "inst".to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: async_job_capabilities(),
            policy: None,
        })
        .await
        .unwrap();
    registry
        .set_transport(client_id, AgentTransport::Quic)
        .await
        .unwrap();
}

// ---------------------------------------------------------------------------
// Structured delete enqueue: keep the generation-2 capability invariant check
// atomic with pending-request admission. A failed invariant queues nothing and
// leaves no request or waiter behind; it never selects a compatibility path.
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
        .register(current_runner_registration(ShellClientRegisterRequest {
            process_started_at: None,
            build: None,
            job_concurrency_limit: None,
            job_inventory: None,
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: capabilities,
            policy: None,
        }))
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

#[path = "mod_tests/agent_transport_auth.rs"]
mod agent_transport_auth;

#[path = "mod_tests/queue_admission.rs"]
mod queue_admission;

#[path = "mod_tests/disconnect_reconciliation.rs"]
mod disconnect_reconciliation;

#[path = "mod_tests/abandoned_sync.rs"]
mod abandoned_sync;

#[path = "mod_tests/mcp_gateway.rs"]
mod mcp_gateway;

#[path = "mod_tests/skill_store.rs"]
mod skill_store;

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
            coding_agent_providers: None,
            coding_agent_inventory: None,
            client_id: client_id.to_string(),
            agent_instance_id: instance.to_string(),
            agent_protocol_generation: crate::shell_protocol::AGENT_PROTOCOL_GENERATION_V2,
            display_name: None,
            owner: Some("alice".to_string()),
            hostname: None,
            host_context: None,
            capabilities: async_job_capabilities(),
            policy: None,
        })
        .await
        .unwrap()
}

#[path = "mod_tests/project_unregister.rs"]
mod project_unregister;

#[path = "mod_tests/job_log_wait.rs"]
mod job_log_wait;
