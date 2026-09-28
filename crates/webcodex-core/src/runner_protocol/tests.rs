use super::*;

// Golden wire bytes captured against the explicit pre-catalog struct. These
// freeze required false keys, omission and field order, not a capability inventory.
const LEGACY_DEFAULT_JSON: &str = concat!(
    r#"{"shell":true,"file_read":false,"file_write":false,"git":false,"jobs":false,"async_jobs":false,"async_shell_jobs":false,"ssh_shell":false,"persistent_shell":false,"ssh_persistent_shell":false,"structured_validation_argv":false,"project_validation_v1":false,"structured_process_argv":false,"structured_script_payload":false,"structured_execution_jobs":false,"lsp_read_only_navigation":false,"lsp_call_hierarchy":false,"project_lifecycle":false,"project_path_registration":false}"#,
);

#[test]
fn runner_capabilities_legacy_default_wire_golden() {
    let decoded: RunnerCapabilities = serde_json::from_str("{}").unwrap();
    assert_eq!(decoded, RunnerCapabilities::default());
    assert_eq!(
        serde_json::to_string(&decoded).unwrap(),
        LEGACY_DEFAULT_JSON
    );
    let future: RunnerCapabilities =
        serde_json::from_str(r#"{"future_capability_v1":true}"#).unwrap();
    assert_eq!(future, decoded);
}

#[test]
fn runner_capabilities_mixed_wire_golden() {
    let capabilities = RunnerCapabilities {
        shell: false,
        file_read: true,
        project_validation_v1: true,
        structured_cargo_test_lib: true,
        browser_observe: true,
        computer_control: false,
        ..Default::default()
    };
    let expected = r#"{"shell":false,"file_read":true,"file_write":false,"git":false,"jobs":false,"async_jobs":false,"async_shell_jobs":false,"ssh_shell":false,"persistent_shell":false,"ssh_persistent_shell":false,"structured_validation_argv":false,"structured_cargo_test_lib":true,"project_validation_v1":true,"structured_process_argv":false,"structured_script_payload":false,"structured_execution_jobs":false,"lsp_read_only_navigation":false,"lsp_call_hierarchy":false,"project_lifecycle":false,"project_path_registration":false,"browser_observe":true}"#;
    assert_eq!(serde_json::to_string(&capabilities).unwrap(), expected);
    assert_eq!(
        serde_json::from_str::<RunnerCapabilities>(expected).unwrap(),
        capabilities
    );
}

#[test]
fn runner_capabilities_catalog_wire_round_trip_and_projection() {
    use std::collections::BTreeSet;
    let ids = RunnerCapabilityId::all();
    let names: BTreeSet<_> = RUNNER_CAPABILITY_NAMES.iter().copied().collect();
    assert_eq!(names.len(), ids.len(), "wire names must be unique");
    assert_eq!(
        ids.iter().copied().collect::<BTreeSet<_>>().len(),
        ids.len()
    );
    assert_eq!(
        ids.iter().map(|id| id.as_wire_name()).collect::<Vec<_>>(),
        RUNNER_CAPABILITY_NAMES
    );
    assert_eq!(
        RunnerCapabilityId::from_wire_name("future_capability_v1"),
        None
    );

    let defaults: RunnerCapabilities = serde_json::from_str("{}").unwrap();
    let mut all = defaults.clone();
    for id in ids {
        assert_eq!(
            RunnerCapabilityId::from_wire_name(id.as_wire_name()),
            Some(*id)
        );
        assert_eq!(defaults.supports(*id), *id == RunnerCapabilityId::Shell);
        all.set(*id, true);
    }
    let serialized = serde_json::to_value(&all).unwrap();
    assert_eq!(
        serialized
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        names
    );
    // Catalog order is wire field order, including fields omitted when false.
    let expected = format!(
        "{{{}}}",
        RUNNER_CAPABILITY_NAMES
            .iter()
            .map(|name| format!("\"{name}\":true"))
            .collect::<Vec<_>>()
            .join(",")
    );
    assert_eq!(serde_json::to_string(&all).unwrap(), expected);
    let round_trip: RunnerCapabilities = serde_json::from_value(serialized).unwrap();
    assert_eq!(round_trip, all);
    for id in ids {
        assert!(round_trip.supports(*id));
        let mut toggled = all.clone();
        toggled.set(*id, false);
        let encoded = serde_json::to_value(&toggled).unwrap();
        assert!(!encoded[id.as_wire_name()].as_bool().unwrap_or(false));
        let decoded: RunnerCapabilities = serde_json::from_value(encoded).unwrap();
        for other in ids {
            assert_eq!(decoded.supports(*other), other != id, "{id:?} -> {other:?}");
        }
        toggled.set(*id, true);
        assert_eq!(toggled, all);
    }
}

#[test]
fn runner_capabilities_reject_non_boolean_known_fields() {
    for id in RunnerCapabilityId::all() {
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!("true"),
            serde_json::json!(1),
        ] {
            let value = serde_json::json!({ id.as_wire_name(): invalid });
            assert!(
                serde_json::from_value::<RunnerCapabilities>(value).is_err(),
                "{id:?}"
            );
        }
    }
}
