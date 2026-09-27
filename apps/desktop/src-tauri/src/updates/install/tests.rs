use super::super::download::{DownloadPhase, PendingInstall};
use super::*;

fn record() -> UpdateRecord {
    let mut record = UpdateRecord::default();
    record.version = Some("1.2.3".into());
    record.platform = InstallerPlatform::current();
    record.source_sha = Some("a".repeat(40));
    record.source_manifest_sha256 = Some("b".repeat(64));
    record.pending = Some(PendingInstall {
        environment_id: "environment".into(),
        operation_id: Some("operation".into()),
        started_at_ms: 100,
    });
    record.phase = DownloadPhase::InstallingOrHandedOff;
    record
}
fn observation(outcome: UpgradeOutcome) -> UpgradeObservation {
    UpgradeObservation {
        environment_id: "environment".into(),
        operation_id: "operation".into(),
        version: "1.2.3".into(),
        source_sha: "a".repeat(40),
        manifest_sha256: "b".repeat(64),
        outcome,
    }
}
fn build(version: &str) -> webcodex_core::desktop_runtime_contract::MachineBuildInfo {
    let mut build = crate::commands::get_desktop_build_info();
    build.version = version.into();
    build.git_commit = Some("a".repeat(40));
    build.git_dirty = Some(false);
    build
}

#[test]
fn handoff_never_means_installed_without_version_and_core_commit() {
    let record = record();
    let current = build("1.2.2");
    assert_eq!(
        reconcile(&record, None, &current, 101),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Pending)),
            &current,
            101
        ),
        Reconciliation::InProgress
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Pending)),
            &current,
            2_000_000
        ),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Committed)),
            &current,
            101
        ),
        Reconciliation::RecoveryRequired
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::Committed)),
            &build("1.2.3"),
            101
        ),
        Reconciliation::Installed
    );
    assert_eq!(
        reconcile(
            &record,
            Some(&observation(UpgradeOutcome::RolledBack)),
            &current,
            101
        ),
        Reconciliation::Restored
    );
}

#[test]
fn changed_owner_operation_source_or_dirty_build_cannot_clear_pending() {
    for changed in ["owner", "operation", "source", "manifest"] {
        let mut observed = observation(UpgradeOutcome::Pending);
        match changed {
            "owner" => observed.environment_id = "other".into(),
            "operation" => observed.operation_id = "other".into(),
            "source" => observed.source_sha = "c".repeat(40),
            _ => observed.manifest_sha256 = "c".repeat(64),
        }
        assert_eq!(
            reconcile(&record(), Some(&observed), &build("1.2.2"), 101),
            Reconciliation::RecoveryRequired
        );
    }
    let mut dirty = build("1.2.3");
    dirty.git_dirty = Some(true);
    assert_eq!(
        reconcile(
            &record(),
            Some(&observation(UpgradeOutcome::Committed)),
            &dirty,
            101
        ),
        Reconciliation::RecoveryRequired
    );
}

#[test]
fn newer_committed_generation_can_reconcile_a_stale_download() {
    let mut observed = observation(UpgradeOutcome::Committed);
    observed.version = "1.2.4".into();
    observed.operation_id = "newer-operation".into();
    assert_eq!(
        reconcile(&record(), Some(&observed), &build("1.2.4"), 101),
        Reconciliation::Installed
    );
}
