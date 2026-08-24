use super::*;
use serde::Deserialize;
use serde_json::Value;
use tokio_tungstenite::tungstenite::http::header::AUTHORIZATION;

/// Frozen top-level registration representation from latest stable v0.3.8.
/// `deny_unknown_fields` intentionally turns any new mandatory top-level wire
/// field into an explicit rolling-compatibility review instead of silently
/// widening the guaranteed current -> latest-stable contract. Nested additive
/// capability/policy content remains opaque because v0.3.8 serde accepts those
/// additions and current capability semantics are independently fail-closed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LatestStableV038RegisterRequest {
    client_id: String,
    agent_instance_id: String,
    #[serde(default, rename = "display_name")]
    _display_name: Option<Value>,
    #[serde(default, rename = "owner")]
    _owner: Option<Value>,
    #[serde(default, rename = "hostname")]
    _hostname: Option<Value>,
    #[serde(default)]
    capabilities: Option<Value>,
    #[serde(default, rename = "host_context")]
    _host_context: Option<Value>,
    #[serde(default)]
    projects: Option<Value>,
    #[serde(default)]
    agent_protocol_version: Option<String>,
    #[serde(default, rename = "policy")]
    _policy: Option<Value>,
    #[serde(default, rename = "process_started_at")]
    _process_started_at: Option<Value>,
    #[serde(default, rename = "build")]
    _build: Option<Value>,
    #[serde(default, rename = "job_concurrency_limit")]
    _job_concurrency_limit: Option<Value>,
    #[serde(default, rename = "job_inventory")]
    _job_inventory: Option<Value>,
}

#[test]
fn current_runner_registration_is_readable_by_latest_stable_v038_top_level_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    for protocol in [
        AGENT_PROTOCOL_VERSION_POLLING_V1,
        AGENT_PROTOCOL_VERSION_WEBSOCKET_V1,
        AGENT_PROTOCOL_VERSION_QUIC_V1,
    ] {
        let current = build_register_request(&cfg, Vec::new(), protocol, "inst-current", 0);
        let json = serde_json::to_vec(&current).unwrap();
        let stable: LatestStableV038RegisterRequest = serde_json::from_slice(&json).unwrap();
        assert_eq!(stable.client_id, cfg.client_id);
        assert_eq!(stable.agent_instance_id, "inst-current");
        assert_eq!(stable.agent_protocol_version.as_deref(), Some(protocol));
        assert!(stable.capabilities.is_some());
        assert!(stable.projects.is_some());
    }
}

#[test]
fn current_runner_websocket_request_preserves_latest_stable_v038_handshake_contract() {
    let request = build_ws_request(
        "ws://127.0.0.1:8080/api/agents/ws",
        "latest-stable-compat-token",
    )
    .unwrap();
    assert_eq!(
        request.headers().get(AUTHORIZATION).unwrap(),
        "Bearer latest-stable-compat-token"
    );
    assert_eq!(request.uri().path(), "/api/agents/ws");
    assert!(request.uri().query().is_none());
}
