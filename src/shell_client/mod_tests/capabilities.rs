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

fn with_wire_feature(
    capabilities: &ShellClientCapabilities,
    feature: RunnerFeature,
    enabled: bool,
) -> ShellClientCapabilities {
    let mut value = serde_json::to_value(capabilities).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert(feature.as_wire_name().to_string(), enabled.into());
    serde_json::from_value(value).unwrap()
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
    let all_false = RunnerFeatureSet::from_wire_for_test(&wire_capabilities_with_only(None));
    for feature in RunnerFeature::all() {
        assert!(!all_false.supports(*feature), "{}", feature.as_wire_name());
    }

    for advertised in RunnerFeature::all() {
        let semantics =
            RunnerFeatureSet::from_wire_for_test(&wire_capabilities_with_only(Some(*advertised)));
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
        RunnerFeature::SkillStoreRead,
        RunnerFeature::SkillStoreManage,
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
fn v2_baseline_exactly_matches_generation_eligible_classification() {
    let baseline = AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let generation_eligible = RunnerFeature::all()
        .iter()
        .copied()
        .filter(|feature| feature.inference() == RunnerFeatureInference::GenerationEligible)
        .map(RunnerFeature::as_wire_name)
        .collect::<BTreeSet<_>>();

    assert_eq!(baseline.len(), 22);
    assert_eq!(
        baseline.len(),
        AGENT_PROTOCOL_GENERATION_V2_BASELINE_CAPABILITY_NAMES.len()
    );
    assert_eq!(baseline, generation_eligible);
    for feature in RunnerFeature::all() {
        if feature.inference() == RunnerFeatureInference::RegistrationRequired {
            assert!(
                !baseline.contains(feature.as_wire_name()),
                "{}",
                feature.as_wire_name()
            );
        }
    }
}

#[test]
fn v2_generation_baseline_requires_explicit_bool_projection_without_silent_or() {
    let baseline = v2_baseline_capabilities();
    let accepted = RunnerFeatureSet::try_from_registration(&baseline).unwrap();

    let missing = wire_capabilities_with_only(None);
    assert_eq!(
        RunnerFeatureSet::try_from_registration(&missing).unwrap_err(),
        "runner generation baseline capability mismatch: file_read"
    );

    for feature in RunnerFeature::all()
        .iter()
        .copied()
        .filter(|feature| feature.inference() == RunnerFeatureInference::GenerationEligible)
    {
        assert!(accepted.supports(feature), "{}", feature.as_wire_name());
        let contradictory = with_wire_feature(&baseline, feature, false);
        let error = RunnerFeatureSet::try_from_registration(&contradictory).unwrap_err();
        assert_eq!(
            error,
            format!(
                "runner generation baseline capability mismatch: {}",
                feature.as_wire_name()
            )
        );
    }
}

#[test]
fn v2_registration_required_features_are_never_inferred_from_generation() {
    let baseline = v2_baseline_capabilities();
    let baseline_only = RunnerFeatureSet::try_from_registration(&baseline).unwrap();

    for feature in RunnerFeature::all()
        .iter()
        .copied()
        .filter(|feature| feature.inference() == RunnerFeatureInference::RegistrationRequired)
    {
        assert!(
            !baseline_only.supports(feature),
            "{}",
            feature.as_wire_name()
        );
        let explicitly_advertised = with_wire_feature(&baseline, feature, true);
        let accepted = RunnerFeatureSet::try_from_registration(&explicitly_advertised).unwrap();
        assert!(accepted.supports(feature), "{}", feature.as_wire_name());
    }

    for feature in [
        RunnerFeature::SshShell,
        RunnerFeature::SandboxInspectCommands,
        RunnerFeature::ComputerObserve,
        RunnerFeature::ComputerControl,
        RunnerFeature::ComputerTextInput,
        RunnerFeature::JobStateReconciliation,
        RunnerFeature::CodingAgentRuns,
    ] {
        assert_eq!(
            feature.inference(),
            RunnerFeatureInference::RegistrationRequired
        );
        assert!(!baseline_only.supports(feature));
    }
}

#[test]
fn missing_additive_wire_fields_remain_false_in_canonical_semantics() {
    let wire: ShellClientCapabilities = serde_json::from_str(r#"{}"#).unwrap();
    let semantics = RunnerFeatureSet::from_wire_for_test(&wire);

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
    let mut capabilities = v2_baseline_capabilities();
    capabilities.agent_protocol_generation = Some(AGENT_PROTOCOL_GENERATION_V2);
    registration.capabilities = Some(capabilities);
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
async fn shell_client_view_preserves_capability_wire_projection() {
    let registry = ShellClientRegistry::default();
    let mut advertised = v2_baseline_capabilities();
    advertised.computer_control = true;

    let mut registration = runner_registration("projection", "inst-a", Vec::new());
    registration.capabilities = Some(advertised.clone());
    let view = registry.register(registration).await.unwrap();

    advertised.agent_protocol_generation = None;
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
async fn v2_generation_advertisement_never_enters_public_capability_projection() {
    let registry = ShellClientRegistry::default();
    let mut capabilities = v2_baseline_capabilities();
    capabilities.agent_protocol_generation = Some(AGENT_PROTOCOL_GENERATION_V2);
    capabilities.computer_control = true;

    let mut registration = runner_registration("v2-projection", "inst-v2", Vec::new());
    registration.capabilities = Some(capabilities);
    let view = registry.register(registration).await.unwrap();

    assert!(view.capabilities.agent_protocol_generation.is_none());
    assert!(view.capabilities.computer_control);
    let serialized = serde_json::to_value(&view.capabilities).unwrap();
    assert!(serialized.get("agent_protocol_generation").is_none());
    assert_eq!(serialized["computer_control"], true);
}

#[tokio::test]
async fn semantic_snapshot_keeps_identity_state_and_features_atomic_across_replacement() {
    let registry = ShellClientRegistry::default();

    let mut first_capabilities = v2_baseline_capabilities();
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
    assert!(first_snapshot.supports(RunnerFeature::FileWrite));
    assert!(!first_snapshot.supports(RunnerFeature::ComputerTextInput));

    registry
        .set_last_seen_for_test(
            "semantic-snapshot",
            now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1,
        )
        .await;
    let mut replacement_capabilities = v2_baseline_capabilities();
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
    assert!(replacement_snapshot.supports(RunnerFeature::FileRead));
    assert!(replacement_snapshot.supports(RunnerFeature::FileWrite));
    assert!(replacement_snapshot.supports(RunnerFeature::StructuredScriptPayload));
    assert!(replacement_snapshot.supports(RunnerFeature::LspCallHierarchy));
    assert!(replacement_snapshot.supports(RunnerFeature::ComputerTextInput));

    // The prior immutable observation remains internally coherent rather than
    // being paired with feature truth from the replacement process.
    assert_eq!(first_snapshot.view.agent_instance_id, "inst-a");
    assert!(first_snapshot.supports(RunnerFeature::FileRead));
    assert!(first_snapshot.supports(RunnerFeature::FileWrite));
    assert!(first_snapshot.supports(RunnerFeature::ComputerObserve));
    assert!(!first_snapshot.supports(RunnerFeature::ComputerTextInput));
}

#[tokio::test]
async fn generation_baseline_project_features_cannot_downgrade_on_reregistration() {
    let registry = ShellClientRegistry::default();
    registry
        .register(runner_registration(
            "project-feature-fence",
            "inst-a",
            Vec::new(),
        ))
        .await
        .unwrap();

    let original = registry
        .get_client_semantic_view("project-feature-fence")
        .await
        .unwrap();
    assert!(original.supports(RunnerFeature::ProjectPathRegistration));
    assert!(original.supports(RunnerFeature::ProjectLifecycle));

    let mut downgraded = runner_registration("project-feature-fence", "inst-a", Vec::new());
    let mut capabilities = v2_baseline_capabilities();
    capabilities.project_path_registration = false;
    downgraded.capabilities = Some(capabilities);
    let error = registry.register(downgraded).await.unwrap_err();
    assert_eq!(
        error,
        "runner generation baseline capability mismatch: project_path_registration"
    );

    let preserved = registry
        .get_client_semantic_view("project-feature-fence")
        .await
        .unwrap();
    assert!(preserved.supports(RunnerFeature::ProjectPathRegistration));
    assert!(preserved.supports(RunnerFeature::ProjectLifecycle));
}

#[tokio::test]
async fn coding_agent_registration_consistency_uses_canonical_feature_semantics() {
    let registry = ShellClientRegistry::default();

    let mut metadata_without_feature = runner_registration("coding-metadata", "inst-a", Vec::new());
    metadata_without_feature.capabilities = Some(v2_baseline_capabilities());
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
    feature_without_metadata.capabilities = Some(with_wire_feature(
        &v2_baseline_capabilities(),
        RunnerFeature::CodingAgentRuns,
        true,
    ));
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
    let capabilities = with_wire_feature(&v2_baseline_capabilities(), feature, enabled);
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
async fn generation_baseline_features_reject_reregistration_downgrade() {
    for feature in [
        RunnerFeature::StructuredFileDelete,
        RunnerFeature::ApplyTextEditOccurrence,
        RunnerFeature::InternalPosixScript,
        RunnerFeature::ArtifactExportChunkRead,
        RunnerFeature::ArtifactExportStreamingMetadata,
    ] {
        let registry = ShellClientRegistry::default();
        let client_id = format!("baseline-{}", feature.as_wire_name());
        registry
            .register(runner_registration(&client_id, "inst-a", Vec::new()))
            .await
            .unwrap();
        let mut downgraded = runner_registration(&client_id, "inst-a", Vec::new());
        downgraded.capabilities = Some(with_wire_feature(
            &v2_baseline_capabilities(),
            feature,
            false,
        ));
        let error = registry.register(downgraded).await.unwrap_err();
        assert_eq!(
            error,
            format!(
                "runner generation baseline capability mismatch: {}",
                feature.as_wire_name()
            )
        );
        assert!(registry
            .client_supports(&client_id, feature.as_wire_name())
            .await
            .unwrap());
    }
}

#[tokio::test]
async fn registration_required_sticky_features_reject_same_instance_downgrade() {
    for feature in [
        RunnerFeature::JobStateReconciliation,
        RunnerFeature::CodingAgentRuns,
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
        assert!(registry
            .client_supports(&client_id, feature.as_wire_name())
            .await
            .unwrap());
    }
}

#[tokio::test]
async fn canonical_sticky_feature_fence_preserves_allowed_reconnect_transitions() {
    let feature = RunnerFeature::JobStateReconciliation;

    let false_false = ShellClientRegistry::default();
    register_sticky_feature_state(&false_false, "ff", "inst-a", feature, false)
        .await
        .unwrap();
    register_sticky_feature_state(&false_false, "ff", "inst-a", feature, false)
        .await
        .unwrap();

    let false_true = ShellClientRegistry::default();
    register_sticky_feature_state(&false_true, "ft", "inst-a", feature, false)
        .await
        .unwrap();
    register_sticky_feature_state(&false_true, "ft", "inst-a", feature, true)
        .await
        .unwrap();

    let true_true = ShellClientRegistry::default();
    register_sticky_feature_state(&true_true, "tt", "inst-a", feature, true)
        .await
        .unwrap();
    register_sticky_feature_state(&true_true, "tt", "inst-a", feature, true)
        .await
        .unwrap();

    let true_false = ShellClientRegistry::default();
    register_sticky_feature_state(&true_false, "tf", "inst-a", feature, true)
        .await
        .unwrap();
    let error = register_sticky_feature_state(&true_false, "tf", "inst-a", feature, false)
        .await
        .unwrap_err();
    assert!(error.contains("cannot downgrade job_state_reconciliation"));

    let replacement = ShellClientRegistry::default();
    register_sticky_feature_state(&replacement, "replacement", "inst-a", feature, true)
        .await
        .unwrap();
    replacement
        .set_last_seen_for_test("replacement", now_ts() - CLIENT_ONLINE_WINDOW_SECS - 1)
        .await;
    register_sticky_feature_state(&replacement, "replacement", "inst-b", feature, false)
        .await
        .unwrap();
    let view = replacement.get_client_view("replacement").await.unwrap();
    assert_eq!(view.agent_instance_id, "inst-b");
    assert!(!view.capabilities.job_state_reconciliation);
}
