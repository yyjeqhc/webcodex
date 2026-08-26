use super::*;
use std::collections::BTreeSet;

fn wire_capabilities_with_only(feature: Option<RunnerFeature>) -> ShellClientCapabilities {
    let mut value = serde_json::Map::new();
    // `shell` is the one historical true-by-default field. Pin it false so an
    // omitted field really means false in this all-false/individual-true fixture.
    value.insert("shell".to_string(), serde_json::Value::Bool(false));
    if let Some(feature) = feature {
        value.insert(
            feature.as_wire_name().to_string(),
            serde_json::Value::Bool(true),
        );
    }
    serde_json::from_value(serde_json::Value::Object(value)).unwrap()
}

#[test]
fn canonical_runner_feature_inventory_matches_wire_names_exactly() {
    let wire_names = SHELL_CLIENT_CAPABILITY_NAMES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let canonical_names = RunnerFeature::all()
        .iter()
        .map(|feature| feature.as_wire_name())
        .collect::<BTreeSet<_>>();

    assert_eq!(wire_names.len(), SHELL_CLIENT_CAPABILITY_NAMES.len());
    assert_eq!(canonical_names.len(), RunnerFeature::all().len());
    assert_eq!(canonical_names, wire_names);
}

#[test]
fn canonical_runner_feature_wire_names_round_trip() {
    for feature in RunnerFeature::all() {
        assert_eq!(
            RunnerFeature::from_wire_name(feature.as_wire_name()),
            Some(*feature),
            "{}",
            feature.as_wire_name()
        );
    }
    assert_eq!(RunnerFeature::from_wire_name("future_runner_feature"), None);
}

#[test]
fn canonical_runner_feature_set_tracks_each_individual_wire_bool() {
    let all_false = RunnerFeatureSet::from_wire(&wire_capabilities_with_only(None));
    for feature in RunnerFeature::all() {
        assert!(!all_false.supports(*feature), "{}", feature.as_wire_name());
    }

    for advertised in RunnerFeature::all() {
        let semantics =
            RunnerFeatureSet::from_wire(&wire_capabilities_with_only(Some(*advertised)));
        for observed in RunnerFeature::all() {
            assert_eq!(
                semantics.supports(*observed),
                observed == advertised,
                "advertised={} observed={}",
                advertised.as_wire_name(),
                observed.as_wire_name()
            );
        }
    }
}

#[test]
fn capability_classification_keeps_environment_dependent_features_registration_required() {
    for feature in [
        RunnerFeature::Shell,
        RunnerFeature::Git,
        RunnerFeature::SshShell,
        RunnerFeature::PersistentShell,
        RunnerFeature::SshPersistentShell,
        RunnerFeature::DetachedProcessJobs,
        RunnerFeature::SandboxInspectCommands,
        RunnerFeature::ComputerObserve,
        RunnerFeature::ComputerControl,
        RunnerFeature::ComputerTextInput,
        RunnerFeature::JobStateReconciliation,
        RunnerFeature::CodingAgentRuns,
    ] {
        assert_eq!(
            feature.inference(),
            RunnerFeatureInference::RegistrationRequired,
            "{}",
            feature.as_wire_name()
        );
    }

    for feature in [
        RunnerFeature::FileRead,
        RunnerFeature::StructuredProcessArgv,
        RunnerFeature::LspReadOnlyNavigation,
        RunnerFeature::ProjectLifecycle,
    ] {
        assert_eq!(
            feature.inference(),
            RunnerFeatureInference::GenerationEligible,
            "{}",
            feature.as_wire_name()
        );
    }
}

#[test]
fn missing_additive_wire_fields_remain_false_in_canonical_semantics() {
    let legacy: ShellClientCapabilities = serde_json::from_str(r#"{}"#).unwrap();
    let semantics = RunnerFeatureSet::from_wire(&legacy);

    assert!(semantics.supports(RunnerFeature::Shell));
    for feature in RunnerFeature::all() {
        if *feature != RunnerFeature::Shell {
            assert!(!semantics.supports(*feature), "{}", feature.as_wire_name());
        }
    }
    assert!(!semantics.supports_wire_name("future_runner_feature"));
}

#[tokio::test]
async fn current_protocol_generation_never_infers_registration_required_host_features() {
    let registry = ShellClientRegistry::default();
    let mut registration = runner_registration("no-inference", "inst-a", Vec::new());
    registration.agent_protocol_version = Some(AGENT_PROTOCOL_VERSION_POLLING_V1.to_string());
    registration.capabilities = Some(wire_capabilities_with_only(None));
    registry.register(registration).await.unwrap();

    for feature in [
        RunnerFeature::SshShell,
        RunnerFeature::SandboxInspectCommands,
        RunnerFeature::ComputerObserve,
        RunnerFeature::ComputerControl,
        RunnerFeature::ComputerTextInput,
        RunnerFeature::CodingAgentRuns,
    ] {
        assert!(
            !registry
                .client_supports("no-inference", feature.as_wire_name())
                .await
                .unwrap(),
            "{} must require explicit Runner advertisement",
            feature.as_wire_name()
        );
    }
    assert!(!registry
        .client_supports("no-inference", "future_runner_feature")
        .await
        .unwrap());
}

#[tokio::test]
async fn shell_client_view_preserves_legacy_capability_wire_projection() {
    let registry = ShellClientRegistry::default();
    let mut advertised = wire_capabilities_with_only(None);
    advertised.file_read = true;
    advertised.structured_process_argv = true;
    advertised.computer_control = true;

    let mut registration = runner_registration("projection", "inst-a", Vec::new());
    registration.capabilities = Some(advertised.clone());
    let view = registry.register(registration).await.unwrap();

    assert_eq!(view.capabilities, advertised);
    let serialized = serde_json::to_value(&view.capabilities).unwrap();
    assert_eq!(serialized["shell"], false);
    assert_eq!(serialized["file_read"], true);
    assert_eq!(serialized["structured_process_argv"], true);
    assert_eq!(serialized["computer_control"], true);
    assert!(serialized.get("features").is_none());
    assert!(serialized.get("feature_classification").is_none());
}

#[tokio::test]
async fn semantic_snapshot_keeps_identity_state_and_features_atomic_across_replacement() {
    let registry = ShellClientRegistry::default();

    let mut first_capabilities = wire_capabilities_with_only(None);
    first_capabilities.file_read = true;
    first_capabilities.structured_process_argv = true;
    first_capabilities.lsp_read_only_navigation = true;
    first_capabilities.computer_observe = true;
    let mut first = runner_registration("semantic-snapshot", "inst-a", Vec::new());
    first.capabilities = Some(first_capabilities);
    registry.register(first).await.unwrap();

    let first_snapshot = registry
        .get_client_semantic_view("semantic-snapshot")
        .await
        .unwrap();
    assert_eq!(first_snapshot.view.agent_instance_id, "inst-a");
    assert!(first_snapshot.view.connected);
    assert!(first_snapshot.supports(RunnerFeature::FileRead));
    assert!(first_snapshot.supports(RunnerFeature::StructuredProcessArgv));
    assert!(first_snapshot.supports(RunnerFeature::LspReadOnlyNavigation));
    assert!(first_snapshot.supports(RunnerFeature::ComputerObserve));
    assert!(!first_snapshot.supports(RunnerFeature::FileWrite));
    assert!(!first_snapshot.supports(RunnerFeature::ComputerTextInput));

    registry
        .set_last_seen_for_test(
            "semantic-snapshot",
            now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1,
        )
        .await;
    let mut replacement_capabilities = wire_capabilities_with_only(None);
    replacement_capabilities.file_write = true;
    replacement_capabilities.structured_script_payload = true;
    replacement_capabilities.lsp_call_hierarchy = true;
    replacement_capabilities.computer_text_input = true;
    let mut replacement = runner_registration("semantic-snapshot", "inst-b", Vec::new());
    replacement.capabilities = Some(replacement_capabilities);
    registry.register(replacement).await.unwrap();

    let replacement_snapshot = registry
        .get_client_semantic_view("semantic-snapshot")
        .await
        .unwrap();
    assert_eq!(replacement_snapshot.view.agent_instance_id, "inst-b");
    assert!(replacement_snapshot.view.connected);
    assert!(!replacement_snapshot.supports(RunnerFeature::FileRead));
    assert!(replacement_snapshot.supports(RunnerFeature::FileWrite));
    assert!(replacement_snapshot.supports(RunnerFeature::StructuredScriptPayload));
    assert!(replacement_snapshot.supports(RunnerFeature::LspCallHierarchy));
    assert!(replacement_snapshot.supports(RunnerFeature::ComputerTextInput));

    // The prior immutable observation remains internally coherent rather than
    // being paired with feature truth from the replacement process.
    assert_eq!(first_snapshot.view.agent_instance_id, "inst-a");
    assert!(first_snapshot.supports(RunnerFeature::FileRead));
    assert!(!first_snapshot.supports(RunnerFeature::FileWrite));
    assert!(first_snapshot.supports(RunnerFeature::ComputerObserve));
    assert!(!first_snapshot.supports(RunnerFeature::ComputerTextInput));
}

#[tokio::test]
async fn project_operation_enqueue_rechecks_canonical_features_after_reregistration() {
    let registry = ShellClientRegistry::default();
    let mut initial_capabilities = wire_capabilities_with_only(None);
    initial_capabilities.project_path_registration = true;
    initial_capabilities.project_lifecycle = true;
    let mut initial = runner_registration("project-feature-fence", "inst-a", Vec::new());
    initial.capabilities = Some(initial_capabilities);
    registry.register(initial).await.unwrap();

    // Hold the old semantic observation to model the ToolRuntime preflight, then
    // allow the same process to re-register without these non-sticky features.
    let stale_preflight = registry
        .get_client_semantic_view("project-feature-fence")
        .await
        .unwrap();
    assert!(stale_preflight.supports(RunnerFeature::ProjectPathRegistration));
    assert!(stale_preflight.supports(RunnerFeature::ProjectLifecycle));

    let mut downgraded = runner_registration("project-feature-fence", "inst-a", Vec::new());
    downgraded.capabilities = Some(wire_capabilities_with_only(None));
    registry.register(downgraded).await.unwrap();

    let path_error = registry
        .enqueue_project_op(
            "project-feature-fence".to_string(),
            "resolve_or_register_project",
            "{}".to_string(),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        path_error.contains("project_path_registration"),
        "{path_error}"
    );

    let lifecycle_error = registry
        .enqueue_project_op(
            "project-feature-fence".to_string(),
            "project_lifecycle_disable",
            "{}".to_string(),
            "test".to_string(),
        )
        .await
        .unwrap_err();
    assert!(
        lifecycle_error.contains("project_lifecycle"),
        "{lifecycle_error}"
    );
}

async fn register_structured_delete_state(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let mut capabilities = wire_capabilities_with_only(None);
    capabilities.structured_file_delete = enabled;
    let mut registration = runner_registration(client_id, instance_id, Vec::new());
    registration.capabilities = Some(capabilities);
    registry.register(registration).await.map(|_| ())
}

#[tokio::test]
async fn coding_agent_registration_consistency_uses_canonical_feature_semantics() {
    let registry = ShellClientRegistry::default();

    let mut metadata_without_feature = runner_registration("coding-metadata", "inst-a", Vec::new());
    metadata_without_feature.capabilities = Some(wire_capabilities_with_only(None));
    metadata_without_feature.coding_agent_providers =
        Some(vec![webcodex_core::coding_agent::CodingAgentProvider {
            provider_id: "codex".to_string(),
            provider_instance_id: "provider-a".to_string(),
            name: "Codex".to_string(),
        }]);
    metadata_without_feature.coding_agent_inventory =
        Some(webcodex_core::coding_agent::CodingAgentRunInventory::default());
    let error = registry
        .register(metadata_without_feature)
        .await
        .unwrap_err();
    assert!(
        error.contains("requires coding_agent_runs capability"),
        "{error}"
    );

    let mut feature_without_metadata = runner_registration("coding-feature", "inst-a", Vec::new());
    feature_without_metadata.capabilities = Some(wire_capabilities_with_only(Some(
        RunnerFeature::CodingAgentRuns,
    )));
    let error = registry
        .register(feature_without_metadata)
        .await
        .unwrap_err();
    assert!(
        error.contains("requires non-empty provider inventory and Run inventory"),
        "{error}"
    );
}

async fn register_sticky_feature_state(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance_id: &str,
    feature: RunnerFeature,
    enabled: bool,
) -> Result<(), String> {
    let capabilities = if enabled {
        wire_capabilities_with_only(Some(feature))
    } else {
        wire_capabilities_with_only(None)
    };
    let mut registration = runner_registration(client_id, instance_id, Vec::new());
    registration.capabilities = Some(capabilities);

    if feature == RunnerFeature::JobStateReconciliation && enabled {
        registration.job_inventory = Some(crate::shell_protocol::ShellJobInventory {
            active_complete: true,
            jobs: Vec::new(),
        });
    }
    if feature == RunnerFeature::CodingAgentRuns && enabled {
        registration.coding_agent_providers =
            Some(vec![webcodex_core::coding_agent::CodingAgentProvider {
                provider_id: "codex".to_string(),
                provider_instance_id: "provider-sticky".to_string(),
                name: "Codex".to_string(),
            }]);
        registration.coding_agent_inventory =
            Some(webcodex_core::coding_agent::CodingAgentRunInventory::default());
    }

    registry.register(registration).await.map(|_| ())
}

#[tokio::test]
async fn all_seven_sticky_features_reject_same_instance_downgrade() {
    for feature in [
        RunnerFeature::JobStateReconciliation,
        RunnerFeature::CodingAgentRuns,
        RunnerFeature::StructuredFileDelete,
        RunnerFeature::ApplyTextEditOccurrence,
        RunnerFeature::InternalPosixScript,
        RunnerFeature::ArtifactExportChunkRead,
        RunnerFeature::ArtifactExportStreamingMetadata,
    ] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("sticky-{}", feature.as_wire_name());
        register_sticky_feature_state(&registry, &client_id, "inst-a", feature, true)
            .await
            .unwrap();
        let error = register_sticky_feature_state(&registry, &client_id, "inst-a", feature, false)
            .await
            .unwrap_err();
        assert!(
            error.contains(&format!("cannot downgrade {}", feature.as_wire_name())),
            "feature={} error={error}",
            feature.as_wire_name()
        );
        assert!(
            registry
                .client_supports(&client_id, feature.as_wire_name())
                .await
                .unwrap(),
            "rejected downgrade must preserve {}",
            feature.as_wire_name()
        );
    }
}

#[tokio::test]
async fn canonical_sticky_feature_fence_preserves_allowed_reconnect_transitions() {
    let false_false = ShellClientRegistry::default();
    register_structured_delete_state(&false_false, "ff", "inst-a", false)
        .await
        .unwrap();
    register_structured_delete_state(&false_false, "ff", "inst-a", false)
        .await
        .unwrap();

    let false_true = ShellClientRegistry::default();
    register_structured_delete_state(&false_true, "ft", "inst-a", false)
        .await
        .unwrap();
    register_structured_delete_state(&false_true, "ft", "inst-a", true)
        .await
        .unwrap();

    let true_true = ShellClientRegistry::default();
    register_structured_delete_state(&true_true, "tt", "inst-a", true)
        .await
        .unwrap();
    register_structured_delete_state(&true_true, "tt", "inst-a", true)
        .await
        .unwrap();

    let true_false = ShellClientRegistry::default();
    register_structured_delete_state(&true_false, "tf", "inst-a", true)
        .await
        .unwrap();
    let error = register_structured_delete_state(&true_false, "tf", "inst-a", false)
        .await
        .unwrap_err();
    assert!(error.contains("cannot downgrade structured_file_delete"));

    let replacement = ShellClientRegistry::default();
    register_structured_delete_state(&replacement, "replacement", "inst-a", true)
        .await
        .unwrap();
    replacement
        .set_last_seen_for_test("replacement", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    register_structured_delete_state(&replacement, "replacement", "inst-b", false)
        .await
        .unwrap();
    let view = replacement.get_client_view("replacement").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-b");
    assert!(!view.capabilities.structured_file_delete);
}
