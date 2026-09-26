//! Canonical ToolCall parser, wire, accessor, and request-audit helper tests.

use super::tool_call_test_support::*;
use crate::*;
use serde_json::{json, Value};
use webcodex_core::workflow_session_contract as sessions;

#[test]
fn from_tool_name_parses_unit_tools_without_arguments() {
    for name in [
        "list_tools",
        "list_projects",
        "list_runners",
        "runtime_status",
    ] {
        let call = ToolCall::from_tool_name(name, Value::Null).unwrap_or_else(|e| panic!("{}", e));
        assert!(
            matches!(
                call,
                ToolCall::ListTools { .. }
                    | ToolCall::ListProjects { .. }
                    | ToolCall::ListRunners { .. }
                    | ToolCall::RuntimeStatus { .. }
            ),
            "unit tool {} should parse",
            name
        );
    }
}

#[test]
fn from_tool_name_parses_unit_tools_with_empty_object() {
    let call = ToolCall::from_tool_name("list_tools", json!({})).unwrap();
    assert!(matches!(call, ToolCall::ListTools { .. }));
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn code_mode_exec_is_not_a_tool_call_without_feature() {
    let error = ToolCall::from_tool_name(
        "code_mode_exec",
        json!({
            "project": "agent:special:demo",
            "session_id": format!("wc_sess_{}", "1".repeat(32)),
            "source": "text('x')",
        }),
    )
    .expect_err("feature-off parser must reject code_mode_exec");
    assert!(error.contains("unknown tool 'code_mode_exec'"), "{error}");
    assert!(!is_known_tool_name("code_mode_exec"));
    assert!(!known_tool_names().any(|name| name == "code_mode_exec"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec"));
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn code_mode_exec_effectful_is_not_a_tool_call_without_feature() {
    let error = ToolCall::from_tool_name(
        "code_mode_exec_effectful",
        json!({
            "project": "agent:special:demo",
            "session_id": format!("wc_sess_{}", "2".repeat(32)),
            "source": "text('x')",
        }),
    )
    .expect_err("feature-off parser must reject code_mode_exec_effectful");
    assert!(
        error.contains("unknown tool 'code_mode_exec_effectful'"),
        "{error}"
    );
    assert!(!is_known_tool_name("code_mode_exec_effectful"));
    assert!(!known_tool_names().any(|name| name == "code_mode_exec_effectful"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec_effectful"));
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn code_mode_exec_mutating_is_not_a_tool_call_without_feature() {
    let error = ToolCall::from_tool_name(
        "code_mode_exec_mutating",
        json!({
            "project": "agent:special:demo",
            "session_id": format!("wc_sess_{}", "3".repeat(32)),
            "source": "text('x')",
        }),
    )
    .expect_err("feature-off parser must reject code_mode_exec_mutating");
    assert!(
        error.contains("unknown tool 'code_mode_exec_mutating'"),
        "{error}"
    );
    assert!(!is_known_tool_name("code_mode_exec_mutating"));
    assert!(!known_tool_names().any(|name| name == "code_mode_exec_mutating"));
    assert!(!registered_tool_specs()
        .iter()
        .any(|spec| spec.name == "code_mode_exec_mutating"));
}

#[test]
fn apply_text_edits_shorthand_normalizes_once_to_canonical_call() {
    let revision = 3817291045227_u64;
    let call = ToolCall::from_tool_name(
        "apply_text_edits",
        json!({
            "project": "agent:special:demo",
            "changes": [{
                "path": "src/lib.rs",
                "old_text": "old",
                "new_text": "new",
                "expected_read_revision": revision
            }]
        }),
    )
    .unwrap();
    let ToolCall::ApplyTextEdits { changes, .. } = call else {
        panic!("expected apply_text_edits");
    };
    assert_eq!(changes.len(), 1);
    let change = &changes[0];
    assert_eq!(change.kind, ApplyFileChangeKind::Edit);
    assert_eq!(change.path, "src/lib.rs");
    assert!(change.to_path.is_none());
    assert!(change.content.is_none());
    assert_eq!(change.expected_read_revision, Some(revision));
    assert_eq!(change.edits.len(), 1);
    let edit = &change.edits[0];
    assert_eq!(edit.kind, ApplyTextEditKind::ReplaceExact);
    assert_eq!(edit.old_text.as_deref(), Some("old"));
    assert_eq!(edit.new_text.as_deref(), Some("new"));
    assert!(edit.anchor_text.is_none());
    assert!(edit.occurrence.is_none());
    assert!(edit.line_scope.is_none());

    let canonical = ToolCall::from_tool_name(
        "apply_text_edits",
        json!({
            "project": "agent:special:demo",
            "changes": [{
                "kind": "edit",
                "path": "src/lib.rs",
                "edits": [{"kind": "replace_exact", "old_text": "old", "new_text": "new"}]
            }]
        }),
    )
    .unwrap();
    let ToolCall::ApplyTextEdits { changes, .. } = canonical else {
        panic!("expected canonical apply_text_edits");
    };
    assert_eq!(changes[0].kind, ApplyFileChangeKind::Edit);
    assert_eq!(changes[0].edits[0].kind, ApplyTextEditKind::ReplaceExact);
    assert_eq!(changes[0].edits[0].old_text.as_deref(), Some("old"));
    assert_eq!(changes[0].edits[0].new_text.as_deref(), Some("new"));

    for invalid in [
        json!({"project":"agent:special:demo","changes":[{"path":"src/lib.rs","old_text":"old","new_text":"new","unknown":true}]}),
        json!({"project":"agent:special:demo","changes":[{"path":"src/lib.rs","old_text":"old","new_text":"new","occurrence":2}]}),
        json!({"project":"agent:special:demo","changes":[{"path":"src/lib.rs","old_text":"old","new_text":"new","expected_read_revision":revision,"line_scope":{"start_line":10,"end_line":20}}]}),
        json!({"project":"agent:special:demo","changes":[{"kind":"edit","path":"src/lib.rs","old_text":"old","new_text":"new"}]}),
        json!({"project":"agent:special:demo","changes":[{"path":"new.rs","content":"fn main() {}"}]}),
    ] {
        assert!(ToolCall::from_tool_name("apply_text_edits", invalid).is_err());
    }
}

#[test]
fn start_agent_task_endpoint_continuation_parses_ref_or_explicit_tuple() {
    let by_ref = ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({"attempt_ref": "~ta1"}),
    )
    .unwrap();
    assert!(matches!(
        by_ref,
        ToolCall::StartAgentTaskEndpointContinuation {
            attempt_ref: Some(ref selector),
            task_id: None,
            attempt_id: None,
            assignee_agent_id: None,
            attempt_fence: None,
            attempt_controller_generation: None,
        } if selector == "~ta1"
    ));
    let by_tuple = ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({
            "task_id": "wc_agent_task_ERERERERERERERER",
            "attempt_id": "wc_agent_task_attempt_IiIiIiIiIiIiIiIi",
            "assignee_agent_id": "wc_dagent_MzMzMzMzMzMzMzMz",
            "attempt_fence": "wc_agent_task_fence_RERERERERERERERERERERA",
            "attempt_controller_generation": 1
        }),
    )
    .unwrap();
    assert!(matches!(
        by_tuple,
        ToolCall::StartAgentTaskEndpointContinuation {
            attempt_ref: None,
            task_id: Some(_),
            attempt_id: Some(_),
            assignee_agent_id: Some(_),
            attempt_fence: Some(_),
            attempt_controller_generation: Some(1),
        }
    ));
    assert!(ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({
            "attempt_ref": "~ta1",
            "task_id": "wc_agent_task_ERERERERERERERER"
        }),
    )
    .is_ok());
    assert!(ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({"task_id": "wc_agent_task_ERERERERERERERER"}),
    )
    .is_ok());
    assert!(ToolCall::from_tool_name(
        "start_agent_task_endpoint_continuation",
        json!({
            "attempt_ref": "~ta1",
            "session_id": "wc_sess_0123456789abcdef0123456789abcdef"
        }),
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "heartbeat_agent_task_attempt",
        json!({
            "attempt_ref": "~ta1",
            "task_id": "wc_agent_task_ERERERERERERERER",
            "attempt_id": "wc_agent_task_attempt_IiIiIiIiIiIiIiIi",
            "assignee_agent_id": "wc_dagent_MzMzMzMzMzMzMzMz",
            "attempt_fence": "wc_agent_task_fence_RERERERERERERERERERERA",
            "attempt_controller_generation": 1
        }),
    )
    .is_err());
}

#[test]
fn heartbeat_agent_task_attempt_parses_optional_active_turn_proof() {
    let base = json!({
        "task_id": "wc_agent_task_ERERERERERERERER".to_string(),
        "attempt_id": "wc_agent_task_attempt_IiIiIiIiIiIiIiIi".to_string(),
        "assignee_agent_id": "wc_dagent_MzMzMzMzMzMzMzMz".to_string(),
        "attempt_fence": "wc_agent_task_fence_RERERERERERERERERERERA".to_string(),
        "attempt_controller_generation": 7,
    });
    let ordinary = ToolCall::from_tool_name("heartbeat_agent_task_attempt", base.clone()).unwrap();
    assert!(matches!(
        ordinary,
        ToolCall::HeartbeatAgentTaskAttempt {
            active_turn_wake_id: None,
            active_turn_consume_token: None,
            ..
        }
    ));

    let mut with_proof = base;
    with_proof["active_turn_wake_id"] = json!("wc_wake_VVVVVVVVVVVVVVVV".to_string());
    with_proof["active_turn_consume_token"] =
        json!("wc_wake_consume_ZmZmZmZmZmZmZmZmZmZmZg".to_string());
    let renewed = ToolCall::from_tool_name("heartbeat_agent_task_attempt", with_proof).unwrap();
    assert!(matches!(
        renewed,
        ToolCall::HeartbeatAgentTaskAttempt {
            active_turn_wake_id: Some(ref wake_id),
            active_turn_consume_token: Some(ref consume_token),
            ..
        } if wake_id.starts_with("wc_wake_") && consume_token.starts_with("wc_wake_consume_")
    ));
}

#[test]
fn agent_wait_calls_parse_closed_selectors() {
    const PRIVATE_TASK: &str = "wc_agent_task_ze-rze-rze-rze-r";
    const PRIVATE_KEY: &str = "PRIVATE_WAIT_KEY_MUST_NOT_PERSIST";
    let call = ToolCall::from_tool_name(
        "wait_for_agent_events",
        json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 4,
            "events": [{"kind":"agent_task_terminal","task_id":PRIVATE_TASK}],
            "idempotency_key": PRIVATE_KEY,
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::WaitForAgentEvents {
            expected_controller_generation: 4,
            mode: AgentWaitModeCall::Any,
            goal_id: None,
            ref events,
            ..
        } if events.len() == 1 && events[0].kind == "agent_task_terminal" && events[0].task_id == PRIVATE_TASK
    ));
    let explicit_any = ToolCall::from_tool_name(
        "wait_for_agent_events",
        json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 4,
            "mode": "any",
            "events": [{"kind":"agent_task_terminal","task_id":PRIVATE_TASK}],
            "idempotency_key": PRIVATE_KEY,
        }),
    )
    .unwrap();
    assert!(matches!(
        explicit_any,
        ToolCall::WaitForAgentEvents {
            mode: AgentWaitModeCall::Any,
            ..
        }
    ));
    let all = ToolCall::from_tool_name(
        "wait_for_agent_events",
        json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 4,
            "mode": "all",
            "events": [{"kind":"agent_task_terminal","task_id":PRIVATE_TASK}],
            "idempotency_key": PRIVATE_KEY,
        }),
    )
    .unwrap();
    assert!(matches!(
        all,
        ToolCall::WaitForAgentEvents {
            mode: AgentWaitModeCall::All,
            ..
        }
    ));
    let scoped = ToolCall::from_tool_name(
        "wait_for_agent_events",
        json!({
            "agent_id": "wc_dagent_iavN7wEjRWeJq83v",
            "endpoint_id": "wc_endpoint_iavN7wEjRWeJq83v",
            "expected_controller_generation": 4,
            "mode": "all",
            "goal_id": "wc_goal_GoGoGoGoGoGoGoGo",
            "events": [{"kind":"agent_task_terminal","task_id":PRIVATE_TASK}],
            "idempotency_key": PRIVATE_KEY,
        }),
    )
    .unwrap();
    assert!(matches!(
        scoped,
        ToolCall::WaitForAgentEvents {
            mode: AgentWaitModeCall::All,
            goal_id: Some(ref goal_id),
            ..
        } if goal_id == "wc_goal_GoGoGoGoGoGoGoGo"
    ));
    let specs = crate::registered_tool_specs();
    let wait_spec = specs
        .iter()
        .find(|spec| spec.name == "wait_for_agent_events")
        .unwrap();
    let mode_schema = &wait_spec.input_schema["properties"]["mode"];
    assert_eq!(mode_schema["enum"], json!(["any", "all"]));
    assert_eq!(mode_schema["default"], "any");
    assert_eq!(
        wait_spec.input_schema["properties"]["goal_id"]["type"],
        "string"
    );
    assert_eq!(
        wait_spec.input_schema["properties"]["goal_id"]["pattern"],
        "^wc_goal_[A-Za-z0-9_-]{16}$"
    );

    let read = ToolCall::from_tool_name(
        "read_agent_wait",
        json!({"wait_id": "wc_agent_wait_ZmZmZmZmZmZmZmZm".to_string()}),
    )
    .unwrap();
    assert!(matches!(read, ToolCall::ReadAgentWait { .. }));
    let state = ToolCall::from_tool_name(
        "agent_wait_state",
        json!({"wait_id": "wc_agent_wait_ZmZmZmZmZmZmZmZm".to_string()}),
    )
    .unwrap();
    assert!(matches!(state, ToolCall::AgentWaitState { .. }));
}

#[test]
fn runner_config_tools_parse_closed_contracts_and_keep_governance_split() {
    use crate::{
        RunnerCapabilityRequirement, ToolApprovalPolicy, ToolEffect, ToolIdempotency, ToolRisk,
    };

    let check =
        ToolCall::from_tool_name("runner_config_check", json!({"client_id": "special"})).unwrap();
    assert!(matches!(
        check,
        ToolCall::RunnerConfigCheck { ref client_id } if client_id == "special"
    ));

    let reload = ToolCall::from_tool_name(
        "runner_config_reload",
        json!({"client_id": "special", "expected_generation": 7}),
    )
    .unwrap();
    assert!(matches!(
        reload,
        ToolCall::RunnerConfigReload {
            ref client_id,
            expected_generation: 7,
        } if client_id == "special"
    ));

    for (name, args) in [
        (
            "runner_config_check",
            json!({"client_id": "special", "path": "/tmp/forbidden"}),
        ),
        (
            "runner_config_reload",
            json!({"client_id": "special", "expected_generation": 7, "path": "/tmp/forbidden"}),
        ),
    ] {
        assert!(ToolCall::from_tool_name(name, args).is_err(), "{name}");
    }
    assert!(ToolCall::from_tool_name("runner_config", json!({"action": "check"})).is_err());
    assert!(lookup_tool_definition("describe_config").is_none());

    let check_definition = lookup_tool_definition("runner_config_check").unwrap();
    assert_eq!(check_definition.metadata.effect, ToolEffect::Observe);
    assert_eq!(check_definition.metadata.risk, ToolRisk::Read);
    assert_eq!(check_definition.metadata.approval, ToolApprovalPolicy::None);
    assert_eq!(
        check_definition.metadata.idempotency,
        ToolIdempotency::PureRead
    );
    assert_eq!(
        check_definition.runner_capability,
        Some(RunnerCapabilityRequirement::RunnerConfigControl)
    );
    assert_eq!(
        check_definition.metadata.authority,
        webcodex_core::authority::ToolAuthorityPolicy::Require(
            webcodex_core::authority::SCOPE_RUNTIME_READ
        )
    );

    let reload_definition = lookup_tool_definition("runner_config_reload").unwrap();
    assert_eq!(reload_definition.metadata.effect, ToolEffect::Mutate);
    assert_eq!(reload_definition.metadata.risk, ToolRisk::RunControl);
    assert_eq!(
        reload_definition.metadata.approval,
        ToolApprovalPolicy::Standard
    );
    assert_eq!(
        reload_definition.metadata.idempotency,
        ToolIdempotency::FencedReplay
    );
    assert_eq!(
        reload_definition.runner_capability,
        Some(RunnerCapabilityRequirement::RunnerConfigControl)
    );
    assert_eq!(
        reload_definition.metadata.authority,
        webcodex_core::authority::ToolAuthorityPolicy::Require(
            webcodex_core::authority::SCOPE_RUNNER_MANAGE
        )
    );
}

#[test]
fn ssh_resource_parses_as_canonical_gateway_with_closed_action_vocabulary() {
    let list = ToolCall::from_tool_name(
        "ssh_resource",
        json!({"action": "list", "runner": "special"}),
    )
    .unwrap();
    assert!(matches!(
        list,
        ToolCall::SshResource(SshResourceToolCall {
            ref action,
            runner: Some(ref runner),
            ..
        }) if action == "list" && runner == "special"
    ));

    let register = ToolCall::from_tool_name(
        "ssh_resource",
        json!({
            "action": "register",
            "binding": "wc_sbind_ASNFZ4mrze8BI0VniavN7w",
            "name": "spe",
            "target": "root@spe",
            "default_cwd": "/root/git"
        }),
    )
    .unwrap();
    assert!(matches!(
        register,
        ToolCall::SshResource(SshResourceToolCall {
            ref action,
            name: Some(ref name),
            target: Some(ref target),
            ..
        }) if action == "register" && name == "spe" && target == "root@spe"
    ));

    for invalid in [
        json!({"action": "probe", "runner": "special"}),
        json!({"action": "list", "runner": "special", "unknown": true}),
    ] {
        assert!(ToolCall::from_tool_name("ssh_resource", invalid).is_err());
    }
}

#[test]
fn list_agents_live_ingress_is_rejected_without_second_definition() {
    let error = ToolCall::from_tool_name(
        "list_agents",
        json!({"client_id": "special", "include_projects": false}),
    )
    .expect_err("retired list_agents must not remain a live ingress alias");
    assert!(error.contains("unknown tool 'list_agents'"), "{error}");

    let call = ToolCall::from_tool_name(
        "list_runners",
        json!({"client_id": "special", "include_projects": false}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::ListRunners {
            client_id: Some(ref client_id),
            include_projects: Some(false),
            ..
        } if client_id == "special"
    ));
    assert!(lookup_tool_definition("list_agents").is_none());
    assert!(lookup_tool_definition("list_runners").is_some());
}

#[test]
fn from_tool_name_parses_bounded_list_tools_options() {
    let call = ToolCall::from_tool_name(
        "list_tools",
        json!({
            "category": "artifact",
            "features": "artifact_upload",
            "summary_only": true,
            "limit": 4
        }),
    )
    .unwrap();
    match call {
        ToolCall::ListTools {
            category,
            features,
            summary_only,
            limit,
        } => {
            assert_eq!(category.as_deref(), Some("artifact"));
            assert_eq!(features.as_deref(), Some("artifact_upload"));
            assert!(summary_only);
            assert_eq!(limit, Some(4));
        }
        other => panic!("expected ListTools, got {:?}", other),
    }
}

#[test]
fn call_hierarchy_parser_preserves_default_and_oversized_positive_limit_for_runtime_normalization()
{
    let omitted = ToolCall::from_tool_name(
        "call_hierarchy",
        json!({
            "project": "agent:test:demo",
            "path": "src/main.rs",
            "line": 1,
            "column": 1
        }),
    )
    .unwrap();
    assert!(matches!(
        omitted,
        ToolCall::CallHierarchy {
            direction: webcodex_core::lsp_bridge::CallHierarchyDirection::Both,
            depth: 1,
            limit: 50,
            ..
        }
    ));

    let oversized = ToolCall::from_tool_name(
        "call_hierarchy",
        json!({
            "project": "agent:test:demo",
            "path": "src/main.rs",
            "line": 1,
            "column": 1,
            "limit": 500
        }),
    )
    .unwrap();
    assert!(matches!(
        oversized,
        ToolCall::CallHierarchy { limit: 500, .. }
    ));
}

#[test]
fn cargo_test_lib_false_canonicalizes_to_omission_and_true_is_preserved() {
    for arguments in [
        json!({"project": "demo"}),
        json!({"project": "demo", "lib": false}),
    ] {
        let call = ToolCall::from_tool_name("cargo_test", arguments).unwrap();
        assert!(matches!(call, ToolCall::CargoTest { lib: None, .. }));
    }

    let call =
        ToolCall::from_tool_name("cargo_test", json!({"project": "demo", "lib": true})).unwrap();
    assert!(matches!(
        call,
        ToolCall::CargoTest {
            lib: Some(true),
            ..
        }
    ));
}

#[test]
fn cargo_check_package_selectors_canonicalize_to_one_internal_shape() {
    let single = ToolCall::from_tool_name(
        "cargo_check",
        json!({"project": "demo", "package": " package-b "}),
    )
    .unwrap();
    assert!(matches!(
        single,
        ToolCall::CargoCheck {
            package: None,
            packages: Some(ref packages),
            ..
        } if packages == &["package-b"]
    ));

    let multiple = ToolCall::from_tool_name(
        "cargo_check",
        json!({
            "project": "demo",
            "packages": ["package-c", " package-a ", "package-c", "package-b"]
        }),
    )
    .unwrap();
    assert!(matches!(
        multiple,
        ToolCall::CargoCheck {
            package: None,
            packages: Some(ref packages),
            ..
        } if packages == &["package-a", "package-b", "package-c"]
    ));

    for invalid in [
        json!({"project": "demo", "package": "package-a", "packages": ["package-b"]}),
        json!({"project": "demo", "packages": []}),
        json!({"project": "demo", "packages": ["   "]}),
    ] {
        let error = ToolCall::from_tool_name("cargo_check", invalid)
            .expect_err("invalid package selector must fail closed");
        assert!(error.contains("package"), "{error}");
    }

    let error = ToolCall::from_tool_name("cargo_check", json!({"project": "demo", "packages": []}))
        .expect_err("empty package selection must fail closed");
    assert_eq!(
        error,
        "invalid arguments for tool 'cargo_check': packages must contain between 1 and 32 items"
    );
}

#[test]
fn tool_manifest_default_flows_follow_discovery_shape() {
    for arguments in [
        json!({"tool_name": "cargo_test"}),
        json!({"tool_name": "cargo_test", "include_recommended_flows": false}),
    ] {
        let call = ToolCall::from_tool_name("tool_manifest", arguments).unwrap();
        assert!(matches!(
            call,
            ToolCall::ToolManifest {
                include_recommended_flows: false,
                ..
            }
        ));
    }

    let exact_opt_in = ToolCall::from_tool_name(
        "tool_manifest",
        json!({"tool_name": "cargo_test", "include_recommended_flows": true}),
    )
    .unwrap();
    assert!(matches!(
        exact_opt_in,
        ToolCall::ToolManifest {
            include_recommended_flows: true,
            ..
        }
    ));

    for arguments in [
        json!({}),
        json!({"category": "validation"}),
        json!({"intent": "coding"}),
    ] {
        let call = ToolCall::from_tool_name("tool_manifest", arguments).unwrap();
        assert!(matches!(
            call,
            ToolCall::ToolManifest {
                include_recommended_flows: true,
                ..
            }
        ));
    }
}

#[test]
fn artifact_upload_followup_tools_missing_path_error_is_actionable() {
    for name in [
        "artifact_upload_chunk",
        "artifact_upload_finish",
        "artifact_upload_abort",
    ] {
        let err =
            ToolCall::from_tool_name(name, json!({"upload_id": "wc_upload_test_1"})).unwrap_err();
        assert!(
            err.contains("path is required")
                && err.contains("artifact_upload_begin")
                && err.contains("bind upload_id"),
            "{name}: {err}"
        );
    }
}

#[test]
fn from_tool_name_parses_read_project_artifact_metadata_allow_missing() {
    let call = ToolCall::from_tool_name(
        "read_project_artifact_metadata",
        json!({
            "project": "agent:demo:smoke",
            "path": "artifacts/smoke/missing.artifact",
            "allow_missing": true
        }),
    )
    .unwrap();

    match call {
        ToolCall::ReadProjectArtifactMetadata {
            project,
            path,
            allow_missing,
            ..
        } => {
            assert_eq!(project, "agent:demo:smoke");
            assert_eq!(path, "artifacts/smoke/missing.artifact");
            assert_eq!(allow_missing, Some(true));
        }
        other => panic!("expected ReadProjectArtifactMetadata, got {:?}", other),
    }
}

#[test]
fn from_tool_name_parses_run_shell_with_required_fields() {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({"project": "demo", "command": "echo hi"}),
    )
    .unwrap();
    match call {
        ToolCall::RunShell {
            project,
            command,
            timeout_secs,
            cwd,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(command, "echo hi");
            assert_eq!(timeout_secs, None);
            assert_eq!(cwd, None);
        }
        other => panic!("expected RunShell, got {:?}", other),
    }
}

#[test]
fn from_tool_name_parses_run_shell_with_optional_fields() {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({"project": "demo", "command": "ls", "timeout_secs": 180, "sync_wait_secs": 7, "cwd": "sub"}),
    )
    .unwrap();
    match call {
        ToolCall::RunShell {
            project,
            command,
            timeout_secs,
            sync_wait_secs,
            cwd,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(command, "ls");
            assert_eq!(timeout_secs, Some(180));
            assert_eq!(sync_wait_secs, Some(7));
            assert_eq!(cwd, Some("sub".to_string()));
        }
        other => panic!("expected RunShell, got {:?}", other),
    }
}

#[test]
fn structured_validation_sync_wait_parser_enforces_lifecycle_bounds() {
    for (name, arguments) in [
        (
            "cargo_check",
            json!({"project": "demo", "timeout_secs": 600, "sync_wait_secs": 1}),
        ),
        (
            "cargo_test",
            json!({"project": "demo", "timeout_secs": 600, "sync_wait_secs": 60}),
        ),
        (
            "go_test",
            json!({"project": "demo", "timeout_secs": 60, "sync_wait_secs": 60}),
        ),
        (
            "cargo_fmt",
            json!({"project": "demo", "check": true, "timeout_secs": 60, "sync_wait_secs": 60}),
        ),
        (
            "cargo_fmt",
            json!({"project": "demo", "check": false, "timeout_secs": 60, "sync_wait_secs": 1}),
        ),
        (
            "cargo_fmt",
            json!({"project": "demo", "timeout_secs": 60, "sync_wait_secs": 60}),
        ),
        (
            "cargo_test",
            json!({"project": "demo", "timeout_secs": 600, "sync_wait_secs": 61}),
        ),
        (
            "go_test",
            json!({"project": "demo", "timeout_secs": 30, "sync_wait_secs": 31}),
        ),
    ] {
        ToolCall::from_tool_name(name, arguments)
            .unwrap_or_else(|error| panic!("{name} valid sync wait should parse: {error}"));
    }

    let check = ToolCall::from_tool_name(
        "cargo_fmt",
        json!({"project": "demo", "check": true, "timeout_secs": 60, "sync_wait_secs": 60}),
    )
    .unwrap();
    assert!(matches!(
        check,
        ToolCall::CargoFmt {
            check: Some(true),
            sync_wait_secs: Some(60),
            ..
        }
    ));
    for arguments in [
        json!({"project": "demo", "check": false, "timeout_secs": 60, "sync_wait_secs": 1}),
        json!({"project": "demo", "timeout_secs": 60, "sync_wait_secs": 60}),
    ] {
        let ensure = ToolCall::from_tool_name("cargo_fmt", arguments).unwrap();
        assert!(matches!(
            ensure,
            ToolCall::CargoFmt {
                sync_wait_secs: None,
                ..
            }
        ));
    }

    for (name, arguments) in [
        (
            "cargo_check",
            json!({"project": "demo", "timeout_secs": 600, "sync_wait_secs": 0}),
        ),
        (
            "cargo_fmt",
            json!({"project": "demo", "check": false, "timeout_secs": 60, "sync_wait_secs": 0}),
        ),
        (
            "cargo_fmt",
            json!({"project": "demo", "timeout_secs": 60, "sync_wait_secs": 0}),
        ),
    ] {
        let error =
            ToolCall::from_tool_name(name, arguments).expect_err("zero sync wait must fail closed");
        assert!(error.contains("sync_wait_secs"), "{name}: {error}");
    }
}

#[test]
fn from_tool_name_parses_structured_run_process_boundaries() {
    let call = ToolCall::from_tool_name(
        "run_process",
        json!({
            "project": "demo",
            "executable": "git",
            "args": ["status", "--porcelain", "two words", "$(literal)"],
            "cwd": ".",
            "stdin": "input\n",
            "timeout_secs": 60,
            "sync_wait_secs": 45,
            "purpose": "diagnostic",
            "session_id": "wc_sess_process"
        }),
    )
    .unwrap();
    match call {
        ToolCall::RunProcess {
            project,
            executable,
            args,
            cwd,
            stdin,
            timeout_secs,
            sync_wait_secs,
            purpose,
            session_id,
        } => {
            assert_eq!(project, "demo");
            assert_eq!(executable, "git");
            assert_eq!(
                args,
                ["status", "--porcelain", "two words", "$(literal)"].map(str::to_string)
            );
            assert_eq!(cwd.as_deref(), Some("."));
            assert_eq!(stdin.as_deref(), Some("input\n"));
            assert_eq!(timeout_secs, Some(60));
            assert_eq!(sync_wait_secs, Some(45));
            assert_eq!(purpose, Some(ExecutionPurpose::Diagnostic));
            assert_eq!(session_id.as_deref(), Some("wc_sess_process"));
        }
        other => panic!("expected RunProcess, got {other:?}"),
    }

    let empty = ToolCall::from_tool_name(
        "run_process",
        json!({
            "project": "demo",
            "executable": "git",
            "stdin": null
        }),
    )
    .unwrap();
    match empty {
        ToolCall::RunProcess { args, stdin, .. } => {
            assert!(args.is_empty());
            assert!(stdin.is_none());
        }
        other => panic!("expected RunProcess, got {other:?}"),
    }
}

#[test]
fn process_argv_alias_is_exact_and_canonical() {
    for name in ["run_process", "run_detached_process"] {
        let mut base = json!({"project":"demo", "executable":"git"});
        if name == "run_detached_process" {
            base["idempotency_key"] = json!("exact-key");
        }
        let mut alias = base.clone();
        alias["argv"] = json!(["status"]);
        let (call, code) =
            ToolCall::from_tool_name_with_normalization(name, alias.clone()).unwrap();
        assert_eq!(code, Some("argv_to_args"));
        assert_eq!(
            serde_json::to_value(&call).unwrap()["params"]["args"],
            json!(["status"])
        );
        assert!(serde_json::to_string(&call).unwrap().find("argv").is_none());

        let mut canonical = base.clone();
        canonical["args"] = json!(["status"]);
        assert_eq!(
            ToolCall::from_tool_name_with_normalization(name, canonical.clone())
                .unwrap()
                .1,
            None
        );
        alias["args"] = json!(["status"]);
        assert_eq!(
            ToolCall::from_tool_name_with_normalization(name, alias.clone())
                .unwrap()
                .1,
            Some("argv_to_args")
        );
        alias["args"] = json!(["different"]);
        assert_eq!(
            ToolCall::from_tool_name(name, alias).unwrap_err(),
            "ambiguous compatibility alias: args and argv differ"
        );
        for (field, value) in [("argv", json!("status")), ("arguments", json!(["status"]))] {
            let mut invalid = base.clone();
            invalid[field] = value;
            assert!(
                ToolCall::from_tool_name(name, invalid).is_err(),
                "{name}: {field}"
            );
        }
        for field in ["timeout", "workdir", "arg", "command_args", "params"] {
            let mut invalid = base.clone();
            invalid[field] = json!("value");
            assert!(
                ToolCall::from_tool_name(name, invalid).is_err(),
                "{name}: {field}"
            );
        }
    }
}

#[test]
fn python_is_semantic_script_language_without_interpreter_alias() {
    let (call, code) = ToolCall::from_tool_name_with_normalization(
        "run_script",
        json!({
            "project":"demo", "language":"python", "script":"print('雪')"
        }),
    )
    .unwrap();
    assert_eq!(code, None);
    assert!(matches!(
        call,
        ToolCall::RunScript {
            language: webcodex_core::runner_protocol::ShellScriptLanguage::Python,
            ..
        }
    ));
    assert!(ToolCall::from_tool_name(
        "run_script",
        json!({
            "project":"demo", "language":"python3", "script":"print('x')"
        })
    )
    .is_err());
    for field in ["python_path", "interpreter", "runtime_flags"] {
        let mut request = json!({"project":"demo", "language":"python", "script":"print('x')"});
        request[field] = json!("--unsafe");
        assert!(
            ToolCall::from_tool_name("run_script", request).is_err(),
            "{field}"
        );
    }
    assert!(ToolCall::from_tool_name(
        "run_script",
        json!({
            "project":"demo", "language":"python", "script":"print('x')", "command":"print('x')"
        })
    )
    .is_err());
}

#[test]
fn bash_login_requires_explicit_bash_selection() {
    let call = ToolCall::from_tool_name(
        "run_shell",
        json!({
            "project":"demo", "shell":"bash", "login":true, "command":"printf ok"
        }),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::RunShell { login: true, .. }));
    for shell in [None, Some("sh")] {
        let mut request = json!({"project":"demo", "login":true, "command":"printf ok"});
        if let Some(shell) = shell {
            request["shell"] = json!(shell);
        }
        assert_eq!(
            ToolCall::from_tool_name("run_shell", request).unwrap_err(),
            "run_shell login=true requires shell=bash"
        );
    }
    let default = ToolCall::from_tool_name(
        "run_shell",
        json!({
            "project":"demo", "shell":"bash", "command":"printf ok"
        }),
    )
    .unwrap();
    assert!(matches!(default, ToolCall::RunShell { login: false, .. }));
}

#[test]
fn from_tool_name_rejects_retired_job_status_and_job_log() {
    for (name, args) in [
        ("job_status", json!({"job_id": "abc"})),
        ("job_log", json!({"job_id": "abc", "offset": 10})),
    ] {
        let error = ToolCall::from_tool_name(name, args).unwrap_err();
        assert!(error.contains("unknown tool"), "{name}: {error}");
    }
}

#[test]
fn from_tool_name_parses_stop_job_with_default_confirmation_false() {
    let call =
        ToolCall::from_tool_name("stop_job", json!({"project": "demo", "job_id": "abc"})).unwrap();
    match call {
        ToolCall::StopJob {
            project,
            job_id,
            confirm,
            session_id,
        } => {
            assert_eq!(project, "demo");
            assert_eq!(job_id, "abc");
            assert!(!confirm);
            assert!(session_id.is_none());
        }
        other => panic!("expected StopJob, got {:?}", other),
    }

    let call = ToolCall::from_tool_name(
        "stop_job",
        json!({"project": "demo", "job_id": "abc", "session_id": "wc_sess_x", "confirm": true}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::StopJob {
            ref project,
            ref job_id,
            ref session_id,
            confirm: true,
        } if project == "demo" && job_id == "abc" && session_id.as_deref() == Some("wc_sess_x")
    ));
}

#[test]
fn from_tool_name_rejects_retired_inspection_tools_and_parses_retained_git_tools() {
    let error =
        ToolCall::from_tool_name("read_file", json!({"project": "demo", "path": "README.md"}))
            .unwrap_err();
    assert!(error.contains("unknown tool"), "{error}");

    let call = ToolCall::from_tool_name("git_status", json!({"project": "demo"})).unwrap();
    assert!(matches!(call, ToolCall::GitStatus { .. }));

    for name in ["git_diff", "git_diff_summary"] {
        let error = ToolCall::from_tool_name(name, json!({"project": "demo"})).unwrap_err();
        assert!(error.contains("unknown tool"), "{name}: {error}");
    }

    let call = ToolCall::from_tool_name(
        "apply_unified_diff",
        json!({"project": "demo", "diff": "diff --git a/a b/a\n--- a/a\n+++ b/a\n@@ -1 +1 @@\n-a\n+b\n"}),
    )
    .unwrap();
    assert!(matches!(call, ToolCall::ApplyUnifiedDiff { .. }));

    let call =
        ToolCall::from_tool_name("run_job", json!({"project": "demo", "command": "make"})).unwrap();
    assert!(matches!(call, ToolCall::RunJob { .. }));
}

#[test]
fn continuation_endpoint_rotation_has_canonical_and_legacy_tool_names() {
    let args = json!({
        "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq".to_string(),
        "host": "ChatGPT",
        "client_attachment_id": "window-a",
        "idempotency_key": "rotate-endpoint-1"
    });

    let canonical =
        ToolCall::from_tool_name("rotate_agent_continuation_endpoint", args.clone()).unwrap();
    assert_eq!(canonical.tool_name(), "rotate_agent_continuation_endpoint");
    assert!(matches!(
        canonical,
        ToolCall::RotateAgentContinuationEndpoint { .. }
    ));

    let legacy = ToolCall::from_tool_name("attach_agent_endpoint", args).unwrap();
    assert_eq!(legacy.tool_name(), "attach_agent_endpoint");
    assert!(matches!(legacy, ToolCall::AttachAgentEndpoint { .. }));
}

#[test]
fn from_tool_name_rejects_unknown_tool_name() {
    let err = ToolCall::from_tool_name("not_a_tool", Value::Null).unwrap_err();
    assert!(err.contains("not_a_tool"));
}

#[test]
fn from_tool_name_rejects_missing_required_field() {
    let err = ToolCall::from_tool_name("run_shell", json!({"command": "echo"})).unwrap_err();
    assert!(
        err.contains("project"),
        "error should mention missing field: {}",
        err
    );

    let err = ToolCall::from_tool_name("job_tail", json!({})).unwrap_err();
    assert!(err.contains("job_id"));
}

#[test]
fn from_tool_name_rejects_wrong_field_type() {
    let err = ToolCall::from_tool_name("run_shell", json!({"project": 123, "command": "echo"}))
        .unwrap_err();
    assert!(!err.is_empty());
}

#[test]
fn from_tool_name_error_includes_tool_name() {
    let err = ToolCall::from_tool_name("run_shell", json!({})).unwrap_err();
    assert!(err.contains("run_shell"));
}

#[test]
fn tool_call_project_accessor_covers_project_tool_specs() {
    for spec in registered_tool_specs() {
        let args = sample_tool_args(&spec.name);
        let expected_project = match spec.name.as_str() {
            "start_session" => {
                // start_session project is task association metadata, not an
                // execution target exposed by the project accessor.
                None
            }
            "unregister_project" => {
                // unregister_project carries an exact lifecycle target, but it
                // must bypass generic project pre-resolution so a terminal
                // already_unregistered outcome remains representable.
                None
            }
            _ => args
                .get("project")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        let call = ToolCall::from_tool_name(&spec.name, args)
            .unwrap_or_else(|e| panic!("{} should deserialize: {}", spec.name, e));
        assert_eq!(
            call.project(),
            expected_project.as_deref(),
            "{} ToolCall::project() mismatch",
            spec.name
        );
    }

    // start_session's optional project is task association metadata, not an
    // execution target used for authorization or kernel project reporting.
    let start_session =
        ToolCall::from_tool_name("start_session", json!({"project": "agent:oe:private-drop"}))
            .unwrap();
    assert_eq!(start_session.project(), None);

    // session_handoff_summary's optional project IS exposed by project()
    // when provided, so the kernel can report it and authorize the workspace
    // git inspection path.
    let handoff = ToolCall::from_tool_name(
        "session_handoff_summary",
        json!({"session_id": "wc_sess_x", "project": "agent:oe:private-drop"}),
    )
    .unwrap();
    assert_eq!(handoff.project(), Some("agent:oe:private-drop"));

    // Adapter-only handoff state keeps its exact business target but never
    // exposes that Session through the generic recorder projection.
    let handoff_state = ToolCall::from_tool_name(
        "session_handoff_state",
        json!({"session_id": "wc_sess_x", "project": "agent:oe:private-drop"}),
    )
    .unwrap();
    assert_eq!(handoff_state.project(), Some("agent:oe:private-drop"));
    assert_eq!(handoff_state.session_id(), None);
    assert!(is_model_hidden_tool_name("session_handoff_state"));
    assert!(runtime_tool_requires_explicit_business_session(
        "session_handoff_state"
    ));
    let activity = runtime_tool_activity_semantics("session_handoff_state");
    assert_eq!(activity.presentation.as_str(), "support");
    assert!(!activity.interaction.is_meaningful());
}

#[test]
fn tool_call_session_id_accessor_covers_session_tool_specs() {
    for spec in registered_tool_specs() {
        if spec.input_schema["properties"].get("session_id").is_none() {
            continue;
        }
        if spec.input_schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == "session_id")
        {
            continue;
        }
        let call = ToolCall::from_tool_name(&spec.name, sample_tool_args_with_session(&spec.name))
            .unwrap_or_else(|e| panic!("{} should deserialize: {}", spec.name, e));
        let expected = match spec.name.as_str() {
            "list_jobs" => {
                // list_jobs.session_id is an exact metadata filter over the
                // already-authorized Job set. It deliberately does not opt into
                // generic Workflow Session lookup/recording, which would turn a
                // foreign or missing filter value into an existence oracle.
                None
            }
            "present_work_result" => {
                // Work Result is Window-first. Its optional session_id is
                // compatibility/context evidence only and must not re-enter the
                // generic business-Session lookup or recorder projection.
                None
            }
            _ => Some("wc_sess_accessor"),
        };
        assert_eq!(
            call.session_id(),
            expected,
            "{} ToolCall::session_id() mismatch",
            spec.name
        );
    }
}

#[test]
fn from_tool_name_unknown_tool_lists_available_tools_and_hint() {
    let err = ToolCall::from_tool_name("definitely_not_a_tool", Value::Null).unwrap_err();
    assert!(err.contains("definitely_not_a_tool"));
    assert!(
        err.contains("tool_manifest") && err.contains("tool_name"),
        "unknown-tool error should hint at canonical discovery: {}",
        err
    );
    // Should list at least a couple of known tool names.
    assert!(err.contains("show_changes"));
    assert!(err.contains("apply_unified_diff"));
    // Must not leak secret/config artifacts.
    let lower = err.to_lowercase();
    for forbidden in [
        "token",
        "authorization",
        "runner.toml",
        "agent.toml",
        "webcodex.env",
        "secret",
    ] {
        assert!(
            !lower.contains(forbidden),
            "unknown-tool error must not leak '{}': {}",
            forbidden,
            err
        );
    }
}

#[test]
fn known_tool_names_matches_spec_count() {
    let specs = registered_tool_specs();
    for spec in &specs {
        assert!(
            is_known_tool_name(&spec.name),
            "{} spec must be known to ToolCall",
            spec.name
        );
        assert!(
            !is_model_hidden_tool_name(&spec.name),
            "{} must not be model-hidden when exposed in registered tool specs",
            spec.name
        );
    }
    assert_eq!(
        specs.len(),
        known_tool_names().count() - model_hidden_tool_names().count(),
        "registered tool specs should cover every model-visible known runtime tool; \
         hidden tools are parser-known but carry no ToolSpec"
    );
    // Every known name must be recognized (i.e. must NOT yield the
    // "unknown tool" error). Unit tools parse with null args; non-unit
    // tools fail with a missing-field error, which is still a recognition
    // success (the variant matched).
    for name in known_tool_names() {
        assert!(
            is_known_tool_name(name),
            "known name '{}' not recognized by is_known_tool_name",
            name
        );
        let result = ToolCall::from_tool_name(name, Value::Null);
        match result {
            Ok(_) => {}
            Err(e) => {
                assert!(
                    !e.contains("unknown tool"),
                    "known tool '{}' was treated as unknown: {}",
                    name,
                    e
                );
            }
        }
    }
    // An unknown name must still produce the unknown-tool error.
    let err = ToolCall::from_tool_name("not_a_real_tool", Value::Null).unwrap_err();
    assert!(err.contains("unknown tool"));
    assert!(
        !err.contains("run_codex"),
        "unknown-tool guidance must not advertise hidden tools: {}",
        err
    );
}

#[test]
fn from_tool_name_parses_runtime_status() {
    let call = ToolCall::from_tool_name("runtime_status", Value::Null).unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: false,
            summary_only: false,
            client_id: None,
        }
    ));
    // Also accepts an empty object.
    let call = ToolCall::from_tool_name("runtime_status", json!({})).unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: false,
            summary_only: false,
            client_id: None,
        }
    ));
    let call = ToolCall::from_tool_name(
        "runtime_status",
        json!({"compact": true, "summary_only": true}),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::RuntimeStatus {
            compact: true,
            summary_only: true,
            client_id: None,
        }
    ));
}

#[test]
fn work_on_project_parses_path_source_and_rejects_ambiguous_sources() {
    let work = ToolCall::from_tool_name(
        "work_on_project",
        json!({
            "client_id": "runner-1",
            "path": "/root/git/example",
            "instruction": "implement it"
        }),
    )
    .unwrap();
    assert!(work.project().is_none());
    for path in [
        r"C:\repo",
        "c:/repo",
        r"\\?\C:\repo",
        r"\\server\share\repo",
    ] {
        ToolCall::from_tool_name(
            "work_on_project",
            json!({"client_id": "runner-1", "path": path, "instruction": "implement it"}),
        )
        .unwrap_or_else(|error| panic!("work_on_project rejected {path:?}: {error}"));
    }

    for (tool, arguments) in [
        (
            "work_on_project",
            json!({"project": "agent:x:y", "path": "/tmp/y", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "path": "relative/repo", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "x", "path": r"\repo", "instruction": "x"}),
        ),
        (
            "work_on_project",
            json!({"client_id": "", "path": "/tmp/y", "instruction": "x"}),
        ),
    ] {
        let error = ToolCall::from_tool_name(tool, arguments).unwrap_err();
        assert!(
            error.contains("conflicting fields")
                || error.contains("missing ")
                || error.contains("must be an absolute")
                || error.contains("must not be empty"),
            "{error}"
        );
    }
}

#[test]
fn session_execution_context_parses_as_strongly_typed_replacement() {
    let ssh = ToolCall::from_tool_name(
        "update_session_context",
        json!({
            "project": "agent:oe:demo",
            "session_id": "wc_sess_context01",
            "execution_context": {
                "default_cwd": "/opt/webcodex-edge",
                "resource": "tmp"
            }
        }),
    )
    .unwrap();
    assert!(matches!(
        ssh,
        ToolCall::UpdateSessionContext {
            execution_context: sessions::SessionExecutionContext {
                default_cwd: Some(ref cwd),
                resource: Some(ref resource),
                ..
            },
            ..
        } if cwd == "/opt/webcodex-edge" && resource == "tmp"
    ));

    let clear = ToolCall::from_tool_name(
        "update_session_context",
        json!({
            "project": "agent:oe:demo",
            "session_id": "wc_sess_context01",
            "execution_context": {}
        }),
    )
    .unwrap();
    assert!(matches!(
        clear,
        ToolCall::UpdateSessionContext {
            ref project,
            ref session_id,
            execution_context: sessions::SessionExecutionContext {
                default_cwd: None,
                default_shell: None,
                ..
            },
        } if project == "agent:oe:demo" && session_id == "wc_sess_context01"
    ));

    for invalid in [
        json!({
            "project": "agent:oe:demo",
            "session_id": "wc_sess_context01",
            "execution_context": {"env": {"TOKEN": "secret"}}
        }),
        json!({
            "project": "agent:oe:demo",
            "session_id": "wc_sess_context01",
            "execution_context": {"default_shell": "zsh"}
        }),
    ] {
        let error = ToolCall::from_tool_name("update_session_context", invalid).unwrap_err();
        assert!(error.contains("invalid arguments"));
    }
}

#[test]
fn from_tool_name_parses_finish_coding_task_workspace_projection_flag() {
    let call = ToolCall::from_tool_name(
        "finish_coding_task",
        json!({
            "project": "agent:client:demo",
            "session_id": "wc_sess_demo",
            "summary_only": true,
            "include_workspace": true,
            "include_validation_summary": true,
            "include_hygiene": true,
            "include_handoff": true,
            "include_diff": false
        }),
    )
    .unwrap();

    match call {
        ToolCall::FinishCodingTask {
            project,
            session_id,
            summary_only,
            include_diff,
            include_workspace,
            include_hygiene,
            include_handoff,
            include_validation_summary,
        } => {
            assert_eq!(project, "agent:client:demo");
            assert_eq!(session_id, "wc_sess_demo");
            assert!(summary_only);
            assert_eq!(include_diff, Some(false));
            assert_eq!(include_workspace, Some(true));
            assert_eq!(include_hygiene, Some(true));
            assert_eq!(include_handoff, Some(true));
            assert_eq!(include_validation_summary, Some(true));
        }
        other => panic!("expected finish_coding_task, got {other:?}"),
    }
}

#[test]
fn project_overview_tool_call_parses() {
    let call = ToolCall::from_tool_name(
        "project_overview",
        json!({
            "project": "agent:client:demo",
            "path": "crates/example",
            "max_depth": 3,
            "limit": 120
        }),
    )
    .unwrap();

    match call {
        ToolCall::ProjectOverview {
            project,
            path,
            max_depth,
            limit,
            ..
        } => {
            assert_eq!(project, "agent:client:demo");
            assert_eq!(path.as_deref(), Some("crates/example"));
            assert_eq!(max_depth, Some(3));
            assert_eq!(limit, Some(120));
        }
        other => panic!("expected ProjectOverview, got {other:?}"),
    }
}

#[test]
fn from_tool_name_parses_phase_a_tools() {
    let call = ToolCall::from_tool_name(
        "list_project_files",
        json!({"project": "demo", "limit": 50, "offset": 75}),
    )
    .unwrap();
    match call {
        ToolCall::ListProjectFiles {
            project,
            path,
            limit,
            offset,
            ..
        } => {
            assert_eq!(project, "demo");
            assert_eq!(path, None);
            assert_eq!(limit, Some(50));
            assert_eq!(offset, Some(75));
        }
        other => panic!("expected ListProjectFiles, got {:?}", other),
    }

    let error = ToolCall::from_tool_name(
        "search_project_text",
        json!({"project": "demo", "pattern": "fn main"}),
    )
    .unwrap_err();
    assert!(error.contains("unknown tool"), "{error}");

    let call = ToolCall::from_tool_name(
        "search_project_texts",
        json!({
            "project": "demo",
            "queries": [{
                "pattern": "fn main",
                "limit": 5,
                "context_before": 3,
                "context_after": 8,
                "include_globs": ["**/*.rs"],
                "exclude_globs": ["vendor/**"],
                "result_mode": "count",
                "timeout_secs": 45
            }]
        }),
    )
    .unwrap();
    assert!(matches!(
        call,
        ToolCall::SearchProjectTexts { ref project, ref queries, .. }
            if project == "demo"
                && queries.len() == 1
                && queries[0].result_mode == Some(SearchResultMode::Count)
                && queries[0].timeout_secs == Some(45)
    ));

    for name in ["job_status", "job_log", "git_diff", "git_diff_summary"] {
        let error = ToolCall::from_tool_name(name, json!({})).unwrap_err();
        assert!(error.contains("unknown tool"), "{name}: {error}");
    }

    // list_jobs has only optional fields; null arguments must still parse.
    let call = ToolCall::from_tool_name("list_jobs", Value::Null).unwrap();
    assert!(matches!(
        call,
        ToolCall::ListJobs {
            limit: None,
            status: None,
            project: None,
            session_id: None,
        }
    ));
    let call =
        ToolCall::from_tool_name("list_jobs", json!({"limit": 3, "status": "running"})).unwrap();
    match call {
        ToolCall::ListJobs { limit, status, .. } => {
            assert_eq!(limit, Some(3));
            assert_eq!(status.as_deref(), Some("running"));
        }
        other => panic!("expected ListJobs, got {:?}", other),
    }

    let call =
        ToolCall::from_tool_name("job_tail", json!({"job_id": "abc", "tail_lines": 10})).unwrap();
    match call {
        ToolCall::JobTail {
            job_id,
            tail_lines,
            after_observation_token,
            wait_secs,
        } => {
            assert_eq!(job_id, "abc");
            assert_eq!(tail_lines, Some(10));
            assert_eq!(after_observation_token, None);
            assert_eq!(wait_secs, None);
        }
        other => panic!("expected JobTail, got {:?}", other),
    }
}

#[test]
fn from_tool_name_list_jobs_with_null_arguments_parses() {
    // Regression: a non-unit tool with all-optional fields must deserialize
    // when a caller passes `null` arguments (normalized to an empty object).
    let call = ToolCall::from_tool_name("list_jobs", Value::Null)
        .unwrap_or_else(|e| panic!("list_jobs with null args should parse: {}", e));
    assert!(matches!(call, ToolCall::ListJobs { .. }));
}

#[test]
fn from_tool_name_parses_unified_diff_and_cleanup_tools() {
    let unified = ToolCall::from_tool_name(
        "apply_unified_diff",
        json!({"project":"agent:c:p","diff":"diff","deny_sensitive_paths":true}),
    )
    .unwrap();
    assert!(matches!(
        unified,
        ToolCall::ApplyUnifiedDiff { project, diff, deny_sensitive_paths, .. }
            if project == "agent:c:p" && diff == "diff" && deny_sensitive_paths == Some(true)
    ));

    let delete = ToolCall::from_tool_name(
        "delete_project_files",
        json!({"project":"agent:c:p","paths":["tmp.txt"]}),
    )
    .unwrap();
    assert!(
        matches!(delete, ToolCall::DeleteProjectFiles { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
    );

    let restore = ToolCall::from_tool_name(
        "git_restore_paths",
        json!({"project":"agent:c:p","paths":["README.md"]}),
    )
    .unwrap();
    assert!(
        matches!(restore, ToolCall::GitRestorePaths { project, paths, .. } if project == "agent:c:p" && paths == vec!["README.md"])
    );

    let discard = ToolCall::from_tool_name(
        "discard_untracked",
        json!({"project":"agent:c:p","paths":["tmp.txt"]}),
    )
    .unwrap();
    assert!(
        matches!(discard, ToolCall::DiscardUntracked { project, paths, .. } if project == "agent:c:p" && paths == vec!["tmp.txt"])
    );
}

#[test]
fn from_tool_name_parses_apply_patch_and_rejects_retired_patch_helpers() {
    let patch = ToolCall::from_tool_name(
        "apply_patch",
        json!({
            "project": "agent:c:p",
            "patch": "*** Begin Patch\n*** Add File: new.txt\n+hello\n*** End Patch",
            "matching_mode": "exact_unique"
        }),
    )
    .expect("current apply_patch DSL tool must parse");
    assert!(matches!(
        patch,
        ToolCall::ApplyPatch { project, patch, dry_run, matching_mode, .. }
            if project == "agent:c:p"
                && patch.contains("*** Add File: new.txt")
                && dry_run.is_none()
                && matching_mode == Some(webcodex_core::apply_patch_shared::ApplyPatchMatchingMode::ExactUnique)
    ));

    for legacy_strict in [true, false] {
        let error = ToolCall::from_tool_name(
            "apply_patch",
            json!({
                "project": "agent:c:p",
                "patch": "*** Begin Patch\n*** Add File: legacy.txt\n+hello\n*** End Patch",
                "strict_matching": legacy_strict
            }),
        )
        .expect_err("legacy strict_matching must fail closed at current Server ingress");
        assert!(error.contains("strict_matching"), "{error}");
        assert!(error.contains("matching_mode"), "{error}");
    }

    for removed in ["apply_patch_checked", "validate_patch"] {
        let error = ToolCall::from_tool_name(removed, json!({"project":"agent:c:p"}))
            .expect_err("retired patch helper names must not parse");
        assert!(error.contains(removed), "{error}");
    }
}

#[test]
fn from_tool_name_parses_write_project_file() {
    let write = ToolCall::from_tool_name(
        "write_project_file",
        json!({
            "project": "agent:c:p",
            "path": "new.txt",
            "content": "hello"
        }),
    )
    .unwrap();
    assert!(matches!(
        write,
        ToolCall::WriteProjectFile { project, path, content, overwrite, expected_read_revision, .. }
            if project == "agent:c:p"
            && path == "new.txt"
            && content == "hello"
            && overwrite.is_none()
            && expected_read_revision.is_none()
    ));
}

#[test]
fn from_tool_name_rejects_retired_write_prefix_guard() {
    let error = ToolCall::from_tool_name(
        "write_project_file",
        json!({
            "project": "agent:c:p",
            "path": "existing.txt",
            "content": "replacement",
            "overwrite": true,
            "expected_read_revision": 3817291045227_u64,
            "expected_content_prefix": "legacy"
        }),
    )
    .expect_err("retired prefix guard must fail before dispatch");
    assert!(error.contains("expected_content_prefix"), "{error}");
    assert!(error.contains("no longer supported"), "{error}");
    assert!(error.contains("expected_read_revision"), "{error}");
}

#[test]
fn from_tool_name_rejects_removed_legacy_edit_tools() {
    // The 7 legacy edit tools replaced by `apply_text_edits` are no longer
    // known ToolDefinitions, so `from_tool_name` must reject them with the
    // same unknown-tool error as any other unknown name.
    for name in [
        "replace_in_file",
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
    ] {
        let err = ToolCall::from_tool_name(name, Value::Null).unwrap_err();
        assert!(err.contains("unknown tool"), "{name}: {err}");
    }
}

#[test]
fn from_tool_name_parses_project_management_tools() {
    let register = ToolCall::from_tool_name(
        "register_project",
        json!({
            "client_id":"oe",
            "id":"my-project",
            "name":"My Project",
            "path":"/root/git/my-project"
        }),
    )
    .unwrap();
    assert!(matches!(
        register,
        ToolCall::RegisterProject { ref client_id, ref id, ref name, ref path, .. }
            if client_id == "oe" && id == "my-project" && name == "My Project"
            && path == "/root/git/my-project"
    ));

    let revision = format!("sha256:{}", "a".repeat(64));
    let unregister = ToolCall::from_tool_name(
        "unregister_project",
        json!({
            "project":"agent:oe:my-project",
            "expected_revision": revision
        }),
    )
    .unwrap();
    assert!(matches!(
        unregister,
        ToolCall::UnregisterProject { ref project, ref expected_revision }
            if project == "agent:oe:my-project" && expected_revision == &revision
    ));
    assert_eq!(unregister.project(), None, "unregister must bypass generic project pre-resolution so already_unregistered remains representable");

    let create = ToolCall::from_tool_name(
        "create_project",
        json!({
            "client_id":"oe",
            "id":"hello",
            "name":"Hello",
            "path":"/root/git/hello",
            "template":"basic",
            "git_init":true
        }),
    )
    .unwrap();
    assert!(matches!(
        create,
        ToolCall::CreateProject { ref client_id, ref id, ref name, ref path, ref template, git_init, .. }
            if client_id == "oe" && id == "hello" && name == "Hello"
            && path == "/root/git/hello" && template.as_deref() == Some("basic")
            && git_init
    ));
}

#[test]
fn create_project_rejects_retired_managed_temporary_field() {
    let error = ToolCall::from_tool_name(
        "create_project",
        json!({
            "client_id":"oe",
            "id":"hello",
            "name":"Hello",
            "path":"/root/git/hello",
            "managed_temporary_project":true
        }),
    )
    .expect_err("retired managed-temporary field must not be model/API reachable");
    assert!(error.contains("managed_temporary_project"), "{error}");
}

#[test]
fn create_project_rejects_retired_allow_existing_empty_with_migration_hint() {
    let error = ToolCall::from_tool_name(
        "create_project",
        json!({
            "client_id":"oe",
            "id":"hello",
            "name":"Hello",
            "path":"/root/git/hello",
            "allow_existing_empty":true
        }),
    )
    .expect_err("retired empty-directory adoption field must fail closed");
    assert!(error.contains("allow_existing_empty"), "{error}");
    assert!(error.contains("adopt_existing_empty"), "{error}");
}

#[test]
fn observe_jobs_wake_policy_defaults_and_validates() {
    for (policy, expected) in [
        (None, ObserveJobsWakeOn::Change),
        (Some("change"), ObserveJobsWakeOn::Change),
        (Some("terminal"), ObserveJobsWakeOn::Terminal),
        (Some("all_terminal"), ObserveJobsWakeOn::AllTerminal),
    ] {
        let mut args = json!({
            "items": [{"job_id": "job", "after_observation_token": "private-observation-cursor"}],
            "wait_secs": 1
        });
        if let Some(policy) = policy {
            args["wake_on"] = json!(policy);
        }
        let call = ToolCall::from_tool_name("observe_jobs", args.clone()).unwrap();
        assert!(matches!(&call, ToolCall::ObserveJobs { wake_on, .. } if *wake_on == expected));
    }
    for policy in [
        json!("unknown-private-value"),
        json!(null),
        json!(1),
        json!({"bad": true}),
    ] {
        let args = json!({"items": [{"job_id": "job"}], "wake_on": policy});
        assert!(ToolCall::from_tool_name("observe_jobs", args.clone()).is_err());
    }
}

#[test]
fn observe_jobs_compact_ref_selector_is_additive_and_unambiguous() {
    let raw = ToolCall::from_tool_name(
        "observe_jobs",
        json!({
            "items": [{
                "job_id": "wc_job_example",
                "after_observation_token": "wj3_example"
            }],
            "summary_only": true
        }),
    )
    .expect("classic observe_jobs path must remain supported");
    assert!(matches!(
        raw,
        ToolCall::ObserveJobs {
            summary_only: true,
            ..
        }
    ));

    let compact = ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"observation_ref": "~j12"}]}),
    )
    .expect("compact observation_ref path must parse");
    let ToolCall::ObserveJobs { items, .. } = compact else {
        panic!("expected observe_jobs");
    };
    assert_eq!(items.len(), 1);
    assert!(items[0].job_id.is_empty());
    assert_eq!(items[0].observation_ref.as_deref(), Some("~j12"));
    assert!(items[0].after_observation_token.is_none());

    for invalid in [
        json!({"items": [{}]}),
        json!({"items": [{"job_id": "job", "observation_ref": "~j1"}]}),
        json!({"items": [{"observation_ref": "~j1", "after_observation_token": "wj3_x"}]}),
        json!({"items": [{"observation_ref": "j1"}]}),
        json!({"items": [{"observation_ref": "~j2"}, {"observation_ref": "~j2"}]}),
    ] {
        assert!(
            ToolCall::from_tool_name("observe_jobs", invalid).is_err(),
            "ambiguous/invalid compact selector must fail closed"
        );
    }

    let schema = crate::input_schema_for_tool("observe_jobs");
    let branches = schema["properties"]["items"]["items"]["oneOf"]
        .as_array()
        .expect("observe_jobs item schema must expose selector oneOf");
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[0]["required"], json!(["job_id"]));
    assert_eq!(branches[1]["required"], json!(["observation_ref"]));
    assert_eq!(branches[0]["additionalProperties"], false);
    assert_eq!(branches[1]["additionalProperties"], false);
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_exec_parses_outer_authority() {
    const PRIVATE_SOURCE: &str = "const secret = 'NEVER_PERSIST_CODE_MODE_SOURCE'; text(secret);";
    let session_id = format!("wc_sess_{}", "1".repeat(32));
    let call = ToolCall::from_tool_name(
        "code_mode_exec",
        json!({
            "project": "agent:special:demo",
            "session_id": session_id,
            "source": PRIVATE_SOURCE,
            "timeout_ms": 7_500,
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "code_mode_exec");
    assert_eq!(call.project(), Some("agent:special:demo"));
    assert_eq!(call.session_id(), Some(session_id.as_str()));
}

#[cfg(feature = "experimental-code-mode")]
#[test]
fn code_mode_exec_effectful_parses_outer_authority() {
    const PRIVATE_SOURCE: &str =
        "const secret = 'NEVER_PERSIST_EFFECTFUL_CODE_MODE_SOURCE'; text(secret);";
    let session_id = format!("wc_sess_{}", "2".repeat(32));
    let call = ToolCall::from_tool_name(
        "code_mode_exec_effectful",
        json!({
            "project": "agent:special:demo",
            "session_id": session_id,
            "source": PRIVATE_SOURCE,
            "timeout_ms": 4_000,
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "code_mode_exec_effectful");
    assert_eq!(call.project(), Some("agent:special:demo"));
    assert_eq!(call.session_id(), Some(session_id.as_str()));
}

#[test]
fn observe_session_messages_tool_call_is_bounded() {
    let raw_token = "wsm2_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    let call = ToolCall::from_tool_name(
        "observe_session_messages",
        json!({
            "session_id": "wc_sess_demo",
            "after_observation_token": raw_token,
            "wait_secs": 7,
            "limit": 25
        }),
    )
    .unwrap();
    assert_eq!(call.tool_name(), "observe_session_messages");
    let oversized = ToolCall::from_tool_name(
        "observe_session_messages",
        json!({
            "session_id": "wc_sess_demo",
            "after_observation_token": "x".repeat(webcodex_core::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN + 1)
        }),
    );
    assert!(oversized.is_err());
}

#[test]
fn retired_start_coding_task_is_a_canonical_unknown_tool() {
    let error = ToolCall::from_tool_name("start_coding_task", json!({"project": "demo"}))
        .expect_err("retired start_coding_task must be an ordinary unknown tool");
    assert!(
        error.contains("unknown tool 'start_coding_task'"),
        "{error}"
    );
}

#[test]
fn present_agent_continuation_parses_ref_or_explicit_tuple_without_session() {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "present_agent_continuation")
        .unwrap();
    let properties = spec.input_schema["properties"].as_object().unwrap();
    assert!(properties.contains_key("agent_continuation_ref"));
    assert!(properties.contains_key("agent_id"));
    if let Some(fields) = spec.input_schema["required"].as_array() {
        for field in fields {
            assert!(
                field != "agent_id"
                    && field != "endpoint_id"
                    && field != "expected_controller_generation"
                    && field != "agent_continuation_ref",
                "present_agent_continuation must accept either selector form"
            );
        }
    }
    let schema_text = spec.input_schema.to_string();
    assert!(schema_text.contains(crate::AGENT_CONTINUATION_REF_PATTERN));

    let by_ref = ToolCall::from_tool_name(
        "present_agent_continuation",
        json!({"agent_continuation_ref": "~ac1"}),
    )
    .unwrap();
    assert!(matches!(
        by_ref,
        ToolCall::PresentAgentContinuation {
            agent_continuation_ref: Some(ref selector),
            agent_id: None,
            endpoint_id: None,
            expected_controller_generation: None,
        } if selector == "~ac1"
    ));
    let by_tuple = ToolCall::from_tool_name(
        "present_agent_continuation",
        json!({
            "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq",
            "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7",
            "expected_controller_generation": 1
        }),
    )
    .unwrap();
    assert!(matches!(
        by_tuple,
        ToolCall::PresentAgentContinuation {
            agent_continuation_ref: None,
            agent_id: Some(_),
            endpoint_id: Some(_),
            expected_controller_generation: Some(1),
        }
    ));
    assert!(ToolCall::from_tool_name(
        "present_agent_continuation",
        json!({
            "agent_continuation_ref": "~ac1",
            "session_id": "wc_sess_0123456789abcdef0123456789abcdef"
        }),
    )
    .is_err());
}

#[test]
fn agent_continuation_bind_requires_view_fence() {
    let binding_id = format!("wc_host_binding_{}", "a0".repeat(16));
    let mut args = json!({
        "agent_id": "wc_dagent_qqqqqqqqqqqqqqqq".to_string(),
        "endpoint_id": "wc_endpoint_u7u7u7u7u7u7u7u7".to_string(),
        "expected_controller_generation": 1,
        "binding_id": binding_id,
    });
    let call = ToolCall::from_tool_name("agent_continuation_bind", args.clone()).unwrap();
    assert!(
        matches!(&call, ToolCall::AgentContinuationBind { binding_id: parsed, .. } if parsed == &binding_id)
    );
    args.as_object_mut().unwrap().remove("binding_id");
    assert!(ToolCall::from_tool_name("agent_continuation_bind", args).is_err());
}

#[test]
fn job_terminal_continuation_calls_require_explicit_wait_and_private_view_fence() {
    let wait_id = "wc_job_wait_q6urq6urq6urq6ur".to_string();
    let binding_id = format!(
        "wc_host_binding_{}",
        webcodex_core::compact::encode([0xb1; 16])
    );
    let present = ToolCall::from_tool_name(
        "present_job_terminal_continuation",
        json!({"wait_id": wait_id}),
    )
    .unwrap();
    assert!(matches!(
        present,
        ToolCall::PresentJobTerminalContinuation { .. }
    ));

    let bind = ToolCall::from_tool_name(
        "job_terminal_continuation_bind",
        json!({"wait_id": wait_id, "binding_id": binding_id}),
    )
    .unwrap();
    assert!(matches!(bind, ToolCall::JobTerminalContinuationBind { .. }));
    for forbidden in ["client_window", "peer_id", "principal_digest", "session_id"] {
        let mut args = json!({"wait_id": wait_id, "binding_id": binding_id});
        args.as_object_mut()
            .unwrap()
            .insert(forbidden.to_string(), json!("caller-authored-routing"));
        let error = ToolCall::from_tool_name("job_terminal_continuation_bind", args)
            .expect_err("routing/authority sideband must not be accepted in business input");
        assert!(error.contains("unknown field"), "{error}");
    }
}

#[test]
fn guidance_profile_defaults_and_schema_follow_compiled_availability() {
    let base = json!({"project": "agent:profile:demo", "instruction": "inspect the project"});
    let default = ToolCall::from_tool_name("work_on_project", base.clone()).unwrap();
    assert!(matches!(
        default,
        ToolCall::WorkOnProject {
            guidance_profile: None,
            ..
        }
    ));
    let schema = crate::request_schema::input_schema_for_tool("work_on_project");
    let property = &schema["properties"]["guidance_profile"];
    assert!(property.get("default").is_none());
    assert!(!schema["required"]
        .as_array()
        .unwrap()
        .contains(&json!("guidance_profile")));
    let mut profiles = vec!["direct", "host_code_mode"];
    if cfg!(feature = "experimental-code-mode") {
        profiles.push("code_mode");
    }
    assert_eq!(property["enum"], json!(profiles), "{property}");
    let output_schema = output_schema_for_tool("work_on_project");
    let output_properties = output_schema["properties"]["output"]["properties"]
        .as_object()
        .unwrap();
    assert!(!output_properties.contains_key("workflow"));
    assert!(output_properties.contains_key("goal_context"));
    for profile in profiles {
        let mut args = base.clone();
        args["guidance_profile"] = json!(profile);
        let call = ToolCall::from_tool_name("work_on_project", args).unwrap();
        assert_eq!(call.project(), default.project());
        assert_eq!(call.session_id(), default.session_id());
        assert_eq!(call.tool_name(), default.tool_name());
        assert_eq!(
            serde_json::to_value(call).unwrap()["params"]["guidance_profile"],
            profile
        );
    }
    for invalid in [json!("unknown"), json!(""), json!(null), json!(1)] {
        let mut args = base.clone();
        args["guidance_profile"] = invalid;
        assert!(ToolCall::from_tool_name("work_on_project", args).is_err());
    }
}

#[test]
fn current_window_activity_has_no_model_supplied_window_selector() {
    let input = crate::request_schema::input_schema_for_tool("current_window_activity");
    let properties = input["properties"].as_object().unwrap();
    assert!(properties.contains_key("limit"));
    assert!(properties.contains_key("include_nonmeaningful"));
    assert!(!properties.contains_key("client_window_key"));
    assert!(!properties.contains_key("window"));
    assert!(ToolCall::from_tool_name(
        "current_window_activity",
        json!({
            "client_window_key": "foreign"
        })
    )
    .is_err());
    assert_eq!(
        ToolCall::from_tool_name("current_window_activity", json!({"limit":20}))
            .unwrap()
            .tool_name(),
        "current_window_activity"
    );
}

#[test]
fn current_window_activity_description_keeps_timing_factual_and_overlap_explicit() {
    let spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "current_window_activity")
        .expect("current_window_activity ToolSpec");
    let description = spec.description.as_str();
    for phrase in [
        "WebCodex-observed request timing only",
        "gap threshold counts are cumulative",
        "short gaps never prove Host cells",
        "window_transition_kind=overlap",
        "never inferred from gap duration",
    ] {
        assert!(description.contains(phrase), "{phrase}: {description}");
    }
}

#[cfg(not(feature = "experimental-code-mode"))]
#[test]
fn guidance_profile_code_mode_fails_closed_when_feature_is_unavailable() {
    let error = ToolCall::from_tool_name("work_on_project", json!({
        "project": "agent:profile:demo", "instruction": "inspect", "guidance_profile": "code_mode"
    })).unwrap_err();
    assert!(
        error.contains("code_mode") && error.contains("direct"),
        "{error}"
    );
}

#[test]
fn external_observation_contract_uses_explicit_identities_and_no_raw_payload() {
    let mut value = json!({"project":"agent:r:p", "session_id":format!("wc_sess_{}","1".repeat(32)),
        "adapter_id":"a".repeat(64),"event_id":"b".repeat(64),"observed_tool":"Bash"});
    let call = ToolCall::from_tool_name("record_external_observation", value.clone()).unwrap();
    assert_eq!(call.project(), Some("agent:r:p"));
    assert_eq!(call.session_id(), None);
    assert!(matches!(
        call,
        ToolCall::RecordExternalObservation {
            exit_code: None,
            ..
        }
    ));
    value["command"] = json!("must not be accepted");
    assert!(ToolCall::from_tool_name("record_external_observation", value).is_err());
    assert!(!crate::is_model_visible_tool_name(
        "record_external_observation"
    ));
    assert!(crate::is_model_visible_tool_name(
        "list_external_observations"
    ));
    assert!(registered_tool_specs()
        .into_iter()
        .all(|spec| spec.name != "record_external_observation"));

    for name in ["record_external_observation", "list_external_observations"] {
        let activity = crate::runtime_tool_activity_semantics(name);
        assert_eq!(activity.presentation.as_str(), "support");
        assert!(!activity.interaction.is_meaningful());
    }

    let list_spec = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "list_external_observations")
        .unwrap();
    for field in ["session_id", "project"] {
        assert!(list_spec.input_schema["required"]
            .as_array()
            .unwrap()
            .contains(&json!(field)));
    }

    let record_schema = crate::registry::output_schema_for_tool("record_external_observation");
    let record_output = &record_schema["properties"]["output"]["properties"];
    let observation = &record_output["observation"];
    assert_eq!(observation["additionalProperties"], false);
    assert_eq!(
        observation["properties"]["status"]["enum"],
        json!(["unknown", "reported_success", "reported_failure"])
    );

    let list_output = &list_spec.output_schema["properties"]["output"]["properties"];
    let observations = &list_output["observations"];
    assert_eq!(observations["maxItems"], 256);
    assert_eq!(observations["items"]["additionalProperties"], false);
    let coverage = &list_output["coverage"];
    assert_eq!(coverage["additionalProperties"], false);
    assert_eq!(coverage["properties"]["complete"]["const"], false);
    assert_eq!(
        coverage["properties"]["reason"]["enum"],
        json!(["source_sequence_unavailable"])
    );
}
