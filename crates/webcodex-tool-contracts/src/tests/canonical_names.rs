//! Tool identity policy, not an alias registry. The old/new table is test-only
//! migration evidence; production dispatch never consults it.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn canonical_names_do_not_rename_risk_or_native_protocol_vocabulary() {
    let control = lookup_tool_definition("control_computer").unwrap();
    assert_eq!(control.name, "control_computer");
    assert_eq!(
        crate::ToolRisk::ComputerControl.session_risk_class(),
        "computer_control"
    );
    assert_eq!(
        webcodex_core::runner_protocol::RUNNER_CAPABILITY_COMPUTER_CONTROL,
        "computer_control"
    );
    assert_eq!(webcodex_core::job_input::CAPABILITY, "job_process_input_v1");
    let input = ToolCall::from_tool_name(
        "write_job_input",
        serde_json::json!({
            "project":"p", "job_id":"j", "input_id":"i", "data":"text"
        }),
    )
    .unwrap();
    assert_eq!(input.tool_name(), "write_job_input");
    let schema = crate::output_schema_for_tool("work_on_project");
    fn visit(value: &serde_json::Value) {
        if let Some(object) = value.as_object() {
            if let (Some(properties), Some(required)) = (
                object.get("properties").and_then(|v| v.as_object()),
                object.get("required").and_then(|v| v.as_array()),
            ) {
                if object.get("additionalProperties") == Some(&serde_json::Value::Bool(false)) {
                    for key in required.iter().filter_map(|v| v.as_str()) {
                        assert!(
                            properties.contains_key(key),
                            "closed schema requires undeclared property: {key}"
                        );
                    }
                }
            }
            for child in object.values() {
                visit(child);
            }
        } else if let Some(items) = value.as_array() {
            for child in items {
                visit(child);
            }
        }
    }
    visit(&schema);
}

#[test]
fn canonical_operation_names_are_unique_verb_first_and_route_independent() {
    let verbs = [
        "list_",
        "read_",
        "get_",
        "observe_",
        "wait_for_",
        "present_",
        "create_",
        "update_",
        "assign_",
        "complete_",
        "stop_",
        "start_",
        "cancel_",
        "delete_",
        "remove_",
        "register_",
        "unregister_",
        "search_",
        "run_",
        "execute_",
        "write_",
        "edit_",
        "apply_",
        "review_",
        "check_",
        "reload_",
        "commit_",
        "restore_",
        "discard_",
        "save_",
        "import_",
        "transfer_",
        "inspect_",
        "begin_",
        "upload_",
        "finish_",
        "abort_",
        "open_",
        "close_",
        "control_",
        "bind_",
        "unbind_",
        "recover_",
        "acquire_",
        "prepare_",
        "rotate_",
        "bootstrap_",
        "detach_",
        "post_",
        "consume_",
        "reconcile_",
        "heartbeat_",
        "checkpoint_",
        "associate_",
        "sync_",
        "find_",
        "set_",
        "purge_",
        "record_",
        "resolve_",
        "send_",
        "load_",
        "install_",
        "activate_",
        "manage_",
        "work_on_",
    ];
    // Provider-specific validation and the two documented portable orchestration
    // entries are not noun-first CRUD. Gateways name a distinct callable namespace.
    let domain = [
        "cargo_fmt",
        "cargo_check",
        "cargo_test",
        "go_test",
        "project_build",
        "project_validate",
        "plugin_tool",
    ];
    let mut names = BTreeSet::new();
    for definition in tool_definitions() {
        let name = definition.name;
        assert!(names.insert(name), "duplicate canonical name: {name}");
        assert!(
            name.len() <= 64
                && name
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_'),
            "{name}"
        );
        assert!(
            domain.contains(&name) || verbs.iter().any(|prefix| name.starts_with(prefix)),
            "operation-first name required: {name}"
        );
        assert!(
            ![
                "direct_",
                "gateway_",
                "hidden_",
                "adaptive_",
                "host_",
                "gpt_"
            ]
            .iter()
            .any(|prefix| name.starts_with(prefix)),
            "routing is not identity: {name}"
        );
        if name.ends_with("_tool") {
            assert!(
                name == "plugin_tool",
                "not a separate tool namespace: {name}"
            );
        }
    }
}

#[test]
fn retired_v04_tool_names_are_not_admitted_or_reused_for_other_operations() {
    let renamed: BTreeMap<String, String> =
        serde_json::from_str(include_str!("fixtures/v05_tool_renames.json")).unwrap();
    let destination: BTreeSet<_> = renamed.values().collect();
    assert_eq!(renamed.len(), destination.len());
    for (old, new) in &renamed {
        assert!(
            !renamed.contains_key(new),
            "do not reuse a retired operation name for a different contract: {new}"
        );
        assert!(
            lookup_tool_definition(old).is_none(),
            "old definition: {old}"
        );
        assert!(
            !known_tool_names().any(|name| name == old),
            "old catalog identity: {old}"
        );
        assert!(
            ToolCall::from_tool_name(old, serde_json::json!({})).is_err(),
            "old parser identity: {old}"
        );
        if let Some(definition) = lookup_tool_definition(new) {
            assert_eq!(definition.name, new);
            assert_eq!(crate::runtime_tool_metadata(new).name, new);
        } else {
            assert!(
                new.contains("workspace_checkpoint") || new.contains("code_mode"),
                "unexpected missing new definition: {new}"
            );
        }
    }
    let input = lookup_tool_definition("write_job_input").unwrap();
    assert!(
        input.adaptive_runtime_direct.is_some(),
        "input's Direct policy must survive the rename"
    );
    assert!(!input.effect_annotations().read_only_hint);
    assert!(input.effect_annotations().idempotent_hint);
    assert_eq!(input.metadata().idempotency, crate::ToolIdempotency::Keyed);
}
