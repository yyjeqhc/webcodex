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
        assert!(OperationCompletion::new(kind, Some(&cancelled))
            .apply_runtime_error(&mut snapshot, None));
        assert!(snapshot.runtime_error.is_none());
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
        assert!(completion.apply_runtime_error(&mut snapshot, Some(&switch)));
        assert_eq!(snapshot.runtime_error.is_some(), outcome != "switched");
        snapshot.runtime_error = Some(failure());
        OperationCompletion::new(DesktopOperationKind::RuntimeResume, None)
            .apply_runtime_error(&mut snapshot, Some(&switch));
        assert!(
            snapshot.runtime_error.is_none(),
            "unrelated success must not inherit switch policy"
        );
    }
}
