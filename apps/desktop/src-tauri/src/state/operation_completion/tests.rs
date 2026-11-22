use super::*;

fn failure() -> DesktopError {
    DesktopError::new("runtime_failed", "test failure", "inspect")
}

fn starting() -> DesktopStateSnapshot {
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Starting,
        RunnerReadiness::Connecting,
        ExposureReadiness::Starting,
        ProjectReadiness::Configured,
    );
    snapshot
}

#[test]
fn every_operation_has_an_explicit_cleanup_owner() {
    use DesktopOperationKind::*;
    let error = failure();
    for kind in [EnvironmentMigration, DesktopUpdate] {
        let completion = OperationCompletion::new(kind, Some(&error));
        assert!(!completion.requires_process_cleanup(), "{kind:?}");
        let mut snapshot = starting();
        assert!(completion.apply_failure(&mut snapshot, &Default::default(), None));
        assert_eq!(snapshot.readiness.server, ServerReadiness::Error);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Error);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::Error);
    }
    for kind in [
        EnvironmentService,
        LocalSetup,
        LocalProjectActivate,
        ProjectUnregister,
        RemoteSetup,
        QuickShareStart,
        QuickShareStop,
        RegularTunnelStart,
        RegularTunnelStop,
        LocalRuntimeStop,
        RuntimeRefresh,
        RuntimeResume,
        TunnelProxyUpdate,
        TunnelConfigUpdate,
        RunnerSettingsUpdate,
        RunnerRestart,
        RuntimeProbe,
        RuntimeSwitch,
        TraceUpdate,
        ConfigurationRestore,
    ] {
        assert!(
            OperationCompletion::new(kind, Some(&error)).requires_process_cleanup(),
            "{kind:?}"
        );
        let success = OperationCompletion::new(kind, None);
        assert!(!success.requires_process_cleanup(), "{kind:?}");
        assert!(!success.apply_failure(&mut starting(), &Default::default(), None));
    }
}

#[test]
fn invitation_failure_cannot_terminalize_or_cleanup_runtime_startup() {
    let error = failure();
    let completion =
        OperationCompletion::new(DesktopOperationKind::EnvironmentInvite, Some(&error));
    let mut snapshot = starting();
    snapshot.runtime_error = Some(failure());
    let before = snapshot.clone();
    assert!(!completion.requires_process_cleanup());
    assert!(!completion.apply_failure(&mut snapshot, &Default::default(), None));
    assert!(!completion.apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot, before);
}

#[test]
fn cancellation_does_not_reclaim_a_durable_coordinators_restored_generation() {
    let error = crate::operation::cancelled_error();
    for kind in [
        DesktopOperationKind::EnvironmentMigration,
        DesktopOperationKind::DesktopUpdate,
    ] {
        let completion = OperationCompletion::new(kind, Some(&error));
        assert!(!completion.requires_process_cleanup());
        let mut snapshot = starting();
        completion.apply_failure(&mut snapshot, &Default::default(), None);
        assert_eq!(snapshot.readiness.server, ServerReadiness::Stopped);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Stopped);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::Disabled);
    }
}

#[test]
fn cancelled_observation_restores_baseline_without_runtime_error_overwrite() {
    let error = crate::operation::cancelled_error();
    let mut baseline = DesktopStateSnapshot::default();
    baseline.runtime_error = Some(failure());
    let mut snapshot = starting();
    let completion = OperationCompletion::new(DesktopOperationKind::RuntimeRefresh, Some(&error));
    completion.apply_failure(&mut snapshot, &baseline, Some(ProcessCleanup::default()));
    assert_eq!(snapshot.readiness, baseline.readiness);
    assert!(!completion.apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error, baseline.runtime_error);
}

#[test]
fn runtime_error_updates_are_kind_scoped_and_cancellation_is_not_a_failure() {
    use DesktopOperationKind::*;
    let error = failure();
    let cancelled = crate::operation::cancelled_error();
    for kind in [
        LocalSetup,
        RemoteSetup,
        RuntimeResume,
        RunnerRestart,
        RuntimeSwitch,
        EnvironmentMigration,
        EnvironmentService,
    ] {
        let mut snapshot = DesktopStateSnapshot::default();
        assert!(
            OperationCompletion::new(kind, Some(&error)).apply_runtime_error(&mut snapshot, None)
        );
        assert_eq!(snapshot.runtime_error.as_ref(), Some(&error));
        assert!(!OperationCompletion::new(kind, Some(&cancelled))
            .apply_runtime_error(&mut snapshot, None));
        assert_eq!(snapshot.runtime_error.as_ref(), Some(&error));
    }
    for kind in [
        DesktopUpdate,
        RuntimeRefresh,
        TunnelConfigUpdate,
        RuntimeProbe,
    ] {
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.runtime_error = Some(error.clone());
        assert!(!OperationCompletion::new(kind, None).apply_runtime_error(&mut snapshot, None));
        assert_eq!(snapshot.runtime_error.as_ref(), Some(&error));
    }
}

#[test]
fn cancelling_an_operation_preserves_independent_component_failures() {
    let runner = DesktopError::new("runner_offline", "Runner offline", "Wait");
    let service = DesktopError::new(
        "service_ownershipunknown",
        "Ownership is unverified",
        "Inspect",
    );
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot
        .component_errors
        .insert(ErrorComponent::Runner, runner.clone());
    snapshot
        .component_errors
        .insert(ErrorComponent::Service, service.clone());
    snapshot.runtime_error = Some(service.clone());
    let cancelled = crate::operation::cancelled_error();
    let completion =
        OperationCompletion::new(DesktopOperationKind::EnvironmentService, Some(&cancelled));
    assert!(!completion.apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&service));
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Runner),
        Some(&runner)
    );
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Service),
        Some(&service)
    );
}

#[test]
fn successful_service_action_does_not_erase_unrelated_component_failures() {
    let server = DesktopError::new(
        "server_unreachable",
        "Server is still unreachable",
        "Inspect server",
    );
    let runner = DesktopError::new("runner_offline", "Runner is offline", "Wait");
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot
        .component_errors
        .insert(ErrorComponent::Server, server.clone());
    snapshot
        .component_errors
        .insert(ErrorComponent::Runner, runner.clone());
    snapshot.runtime_error = Some(server.clone());
    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Error,
        RunnerReadiness::Connecting,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );

    let service = OperationCompletion::new(DesktopOperationKind::EnvironmentService, None);
    assert!(!service.apply_runtime_error(&mut snapshot, None));
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Server),
        Some(&server)
    );
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Runner),
        Some(&runner)
    );
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&server));

    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Error,
        RunnerReadiness::Ready,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    let restarted = OperationCompletion::new(DesktopOperationKind::RunnerRestart, None);
    assert!(restarted.apply_runtime_error(&mut snapshot, None));
    assert!(!snapshot
        .component_errors
        .contains_key(&ErrorComponent::Runner));
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Server),
        Some(&server)
    );
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&server));
}

#[test]
fn confirmed_server_recovery_clears_port_conflict_without_retrying_failed_start() {
    let conflict = DesktopError::new(
        "server_start_failed",
        "Server could not bind HTTP listener",
        "Inspect port owner",
    )
    .with_reason_code("local_port_unavailable")
    .with_recovery(
        crate::error::RecoveryKind::ManualCheck,
        crate::error::SafeRecoveryAction::OpenDiagnostics,
        false,
    );
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot
        .component_errors
        .insert(ErrorComponent::Server, conflict.clone());
    snapshot.runtime_error = Some(conflict);
    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Connecting,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    assert!(reconcile_runtime_errors_after_observation(&mut snapshot));
    assert!(snapshot.runtime_error.is_none());
    assert!(snapshot.component_errors.is_empty());
}

#[test]
fn successful_runner_observation_reconciles_only_stale_runner_offline_error() {
    use DesktopOperationKind::*;
    let offline = DesktopError::new("runner_offline", "Runner offline", "Retry setup");
    let other = failure();
    let ready = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Ready,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    let connecting = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Connecting,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    let observation = OperationCompletion::new(RuntimeRefresh, None);
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot.runtime_error = Some(offline.clone());
    snapshot.readiness = connecting;
    assert!(!observation.apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&offline));

    snapshot.readiness = ready;
    assert!(observation.apply_runtime_error(&mut snapshot, None));
    assert!(snapshot.runtime_error.is_none());
    assert!(!observation.apply_runtime_error(&mut snapshot, None));

    snapshot.runtime_error = Some(other.clone());
    assert!(!observation.apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&other));

    snapshot.runtime_error = Some(offline.clone());
    assert!(!OperationCompletion::new(RuntimeRefresh, Some(&other))
        .apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&offline));
    assert!(!OperationCompletion::new(RuntimeProbe, None).apply_runtime_error(&mut snapshot, None));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&offline));
}

#[test]
fn successful_switch_preserves_durable_rollback_or_recovery_diagnostics_only() {
    for outcome in ["rolled_back", "recovery_required", "switched"] {
        let switch = RuntimeSwitchResult {
            outcome: outcome.into(),
            reason_code: None,
            rollback_reason_code: None,
            selection_revision: 2,
            restart_required: false,
        };
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.runtime_error = Some(failure());
        let completion = OperationCompletion::new(DesktopOperationKind::RuntimeSwitch, None);
        assert_eq!(
            completion.apply_runtime_error(&mut snapshot, Some(&switch)),
            outcome == "switched",
        );
        assert_eq!(snapshot.runtime_error.is_some(), outcome != "switched");
        snapshot.runtime_error = Some(failure());
        assert!(
            !OperationCompletion::new(DesktopOperationKind::RuntimeResume, None)
                .apply_runtime_error(&mut snapshot, Some(&switch)),
        );
        assert!(
            snapshot.runtime_error.is_some(),
            "a successful resume without confirming readiness must not erase a previous error"
        );
    }
}

#[test]
fn component_recovery_clears_only_recovered_component_and_preserves_other_or_uncertain_errors() {
    use crate::error::ErrorComponent;

    let runner_offline = DesktopError::new("runner_offline", "Runner offline", "Wait");
    let server_unreachable =
        DesktopError::new("server_unreachable", "Server offline", "Start Server");
    let uncertain = DesktopError::new("operation_uncertain", "Outcome unknown", "Inspect state");

    // 1. Runner recovery clears runner_offline while preserving an active Server error.
    let mut snapshot = DesktopStateSnapshot::default();
    snapshot
        .component_errors
        .insert(ErrorComponent::Server, server_unreachable.clone());
    snapshot
        .component_errors
        .insert(ErrorComponent::Runner, runner_offline.clone());
    snapshot.runtime_error = Some(runner_offline.clone());
    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Error,
        RunnerReadiness::Ready,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    let observation = OperationCompletion::new(DesktopOperationKind::RuntimeRefresh, None);
    assert!(observation.apply_runtime_error(&mut snapshot, None));
    assert!(!snapshot
        .component_errors
        .contains_key(&ErrorComponent::Runner));
    assert_eq!(
        snapshot.component_errors.get(&ErrorComponent::Server),
        Some(&server_unreachable)
    );
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&server_unreachable));

    // 2. Server recovery clears server_unreachable without clearing a still-connecting Runner error.
    snapshot
        .component_errors
        .insert(ErrorComponent::Runner, runner_offline.clone());
    snapshot.readiness = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Connecting,
        ExposureReadiness::Disabled,
        ProjectReadiness::Configured,
    );
    assert!(observation.apply_runtime_error(&mut snapshot, None));
    assert!(!snapshot
        .component_errors
        .contains_key(&ErrorComponent::Server));
    assert_eq!(snapshot.runtime_error.as_ref(), Some(&runner_offline));

    // 3. Uncertain write outcome must NOT be auto-cleared merely because components observe Ready.
    let mut uncertain_snapshot = DesktopStateSnapshot::default();
    uncertain_snapshot
        .component_errors
        .insert(uncertain.component(), uncertain.clone());
    uncertain_snapshot.runtime_error = Some(uncertain.clone());
    uncertain_snapshot.readiness = aggregate_readiness(
        ServerReadiness::Ready,
        RunnerReadiness::Ready,
        ExposureReadiness::Disabled,
        ProjectReadiness::Ready,
    );
    assert!(!observation.apply_runtime_error(&mut uncertain_snapshot, None));
    assert_eq!(uncertain_snapshot.runtime_error.as_ref(), Some(&uncertain));
}
