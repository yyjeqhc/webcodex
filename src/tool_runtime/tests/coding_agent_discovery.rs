use super::support::*;
use crate::auth::AuthContext;
use crate::runner_protocol::{RunnerCapabilities, RunnerRegisterRequest};
use crate::tool_runtime::runtime_info::ListRunnersOptions;
use crate::tool_runtime::ToolRuntime;
use serde_json::{json, Value};
use webcodex_core::coding_agent::{safe_provider_inventory, CodingAgentProvider};

fn provider(id: &str) -> CodingAgentProvider {
    CodingAgentProvider {
        provider_id: id.into(),
        provider_instance_id: format!("private-instance-{id}"),
        name: format!("{id} Agent"),
    }
}

async fn register(
    runtime: &ToolRuntime,
    client_id: &str,
    providers: Option<Vec<CodingAgentProvider>>,
    auth: Option<&AuthContext>,
) {
    let has_providers = providers
        .as_ref()
        .is_some_and(|providers| !providers.is_empty());
    runtime
        .runner_registry
        .register_with_auth(
            RunnerRegisterRequest {
                client_id: client_id.into(),
                runner_instance_id: format!("inst-{client_id}"),
                runner_protocol_generation: crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2,
                display_name: None,
                owner: auth.and_then(|auth| auth.username.clone()),
                hostname: None,
                host_context: None,
                process_started_at: None,
                build: None,
                job_concurrency_limit: None,
                job_inventory: None,
                coding_agent_inventory: has_providers.then(Default::default),
                coding_agent_providers: providers,
                capabilities: crate::test_support::current_runner_capabilities(
                    RunnerCapabilities {
                        coding_agent_runs: has_providers,
                        ..Default::default()
                    },
                ),
                policy: None,
            },
            auth.map(crate::test_support::runner_access).as_ref(),
        )
        .await
        .unwrap();
}

fn assert_safe_inventory(value: &Value) {
    assert_eq!(
        value,
        &json!([
            {"provider_id":"pi", "name":"pi Agent"},
            {"provider_id":"codex", "name":"codex Agent"}
        ])
    );
    for entry in value.as_array().unwrap() {
        assert_eq!(entry.as_object().unwrap().len(), 2);
    }
    for forbidden in [
        "provider_instance_id",
        "private-instance",
        "executable",
        "args",
        "env",
        "session_id",
    ] {
        assert!(!value.to_string().contains(forbidden));
    }
}

#[tokio::test]
async fn coding_agent_discovery_is_available_in_diagnostic_status_and_runner_listing() {
    let runtime = test_runtime();
    register(
        &runtime,
        "mini",
        Some(vec![provider("pi"), provider("codex")]),
        None,
    )
    .await;
    for compact in [false, true] {
        let all = runtime
            .runtime_status_with_options(None, compact, false, None)
            .await;
        assert!(all.success);
        if compact {
            assert!(all.output["runners"].get("clients").is_none());
        } else {
            assert_safe_inventory(&all.output["runners"]["clients"][0]["coding_agent_providers"]);
        }
        let focused = runtime
            .runtime_status_with_options(None, compact, false, Some("mini".into()))
            .await;
        assert!(focused.success);
        if compact {
            assert!(focused.output["focus"]
                .get("coding_agent_providers")
                .is_none());
        } else {
            assert_safe_inventory(&focused.output["focus"]["coding_agent_providers"]);
        }
        assert!(!focused.output.to_string().contains("private-instance"));
    }
    for summary_only in [false, true] {
        let result = runtime
            .list_runners_with_options(
                None,
                ListRunnersOptions {
                    summary_only,
                    ..Default::default()
                },
            )
            .await;
        assert!(result.success);
        assert_safe_inventory(&result.output["runners"][0]["coding_agent_providers"]);
    }
}

#[tokio::test]
async fn coding_agent_discovery_keeps_old_runners_compatible_and_other_owners_private() {
    let runtime = test_runtime();
    let mut alice = auth_context(Some("alice"), false);
    alice.scopes = vec![
        crate::auth::SCOPE_RUNTIME_READ.into(),
        crate::auth::SCOPE_PROJECT_READ.into(),
    ];
    let mut bob = auth_context(Some("bob"), false);
    bob.scopes = alice.scopes.clone();
    register(&runtime, "alice", None, Some(&alice)).await;
    register(
        &runtime,
        "bob",
        Some(vec![provider("private-bob")]),
        Some(&bob),
    )
    .await;
    for compact in [false, true] {
        let result = runtime
            .runtime_status_with_options(Some(&alice), compact, false, None)
            .await;
        assert_eq!(result.output["runners"]["count"], 1);
        if compact {
            assert!(result.output["runners"].get("clients").is_none());
        } else {
            assert_eq!(
                result.output["runners"]["clients"][0]["coding_agent_providers"],
                json!([])
            );
        }
        assert!(!result.output.to_string().contains("private-bob"));
        assert!(
            !runtime
                .runtime_status_with_options(Some(&alice), compact, false, Some("bob".into()))
                .await
                .success
        );
    }
    let list = runtime.list_runners(Some(&alice)).await;
    assert_eq!(list.output["runners"].as_array().unwrap().len(), 1);
    assert!(!list.output.to_string().contains("private-bob"));
    register(&runtime, "empty", Some(vec![]), None).await;
    let focused = runtime
        .runtime_status_with_options(None, true, false, Some("empty".into()))
        .await;
    assert!(focused.output["focus"]
        .get("coding_agent_providers")
        .is_none());
}

#[tokio::test]
async fn coding_agent_discovery_bootstrap_selects_only_the_online_project_owner() {
    let runtime = test_runtime();
    register(&runtime, "mini", Some(vec![provider("pi")]), None).await;
    register(&runtime, "other", Some(vec![provider("codex")]), None).await;
    let status = runtime.runtime_status(None).await.output;
    let providers =
        crate::tool_runtime::coding_task::project_coding_agent_providers("mini", &status);
    assert_eq!(
        serde_json::to_value(providers).unwrap(),
        json!([{"provider_id":"pi","name":"pi Agent"}])
    );
    assert!(
        crate::tool_runtime::coding_task::project_coding_agent_providers("missing", &status)
            .is_empty()
    );
    let offline = json!({"runners":{"clients":[{"client_id":"mini","connected":false,"coding_agent_providers":[{"provider_id":"pi","name":"Pi"}]}]}});
    assert!(
        crate::tool_runtime::coding_task::project_coding_agent_providers("mini", &offline)
            .is_empty()
    );
}

#[tokio::test]
async fn coding_agent_discovery_start_never_falls_back_and_returns_exact_recovery() {
    let runtime = test_runtime();
    register(
        &runtime,
        "mini",
        Some(vec![provider("pi"), provider("codex")]),
        None,
    )
    .await;
    crate::test_support::apply_project_inventory_snapshot(
        &runtime.runner_registry,
        "mini",
        "inst-mini",
        vec![registered_project("demo", "/tmp/acp-discovery-demo")],
    )
    .await;
    let auth = auth_context(Some("operator"), true);
    let start = |id: &str| {
        runtime.prepare_coding_agent_start(
            "agent:mini:demo".into(),
            id.into(),
            format!("discover-{id}"),
            "Read-only review; do not change files".into(),
            None,
            Some(10),
            Some(&auth),
        )
    };
    assert!(start("pi").await.is_ok());
    let Err(error) = start("missing").await else {
        panic!("missing provider was admitted")
    };
    assert_eq!(
        error.output["error_kind"],
        "coding_agent_provider_unavailable"
    );
    assert_eq!(error.output["execution_state"], "not_started");
    assert_safe_inventory(&error.output["available_providers"]);
    assert_eq!(
        error.output["suggested_call"],
        json!({"tool":"runtime_status","arguments":{"client_id":"mini","compact":true}})
    );
}

#[test]
fn coding_agent_discovery_summary_is_bounded_and_wire_compatible() {
    assert!(safe_provider_inventory(None).is_empty());
    assert!(safe_provider_inventory(Some(&[])).is_empty());
    let many: Vec<_> = (0..100)
        .map(|index| provider(&format!("p{index}")))
        .collect();
    let safe = safe_provider_inventory(Some(&many));
    assert_eq!(
        safe.len(),
        webcodex_core::coding_agent::CODING_AGENT_MAX_PROVIDERS
    );
    assert!(serde_json::to_vec(&safe).unwrap().len() < 2048);
    let mut invalid = provider("pi");
    invalid.name = "x".repeat(129);
    assert!(safe_provider_inventory(Some(&[invalid])).is_empty());
}
