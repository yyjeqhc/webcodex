//! A bounded local observation for CLI/Desktop reconciliation. The upgrade
//! journal remains Core-owned; no paths, service definitions or lease tokens
//! cross this projection, and observing never finishes or rolls back a switch.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeOutcome {
    Pending,
    Committed,
    RolledBack,
    RecoveryRequired,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpgradeObservation {
    #[serde(default, skip_serializing_if = "PackageFlavor::is_full")]
    pub package_flavor: PackageFlavor,
    pub environment_id: String,
    pub operation_id: String,
    pub version: String,
    pub source_sha: String,
    pub manifest_sha256: String,
    pub outcome: UpgradeOutcome,
}

fn outcome(phase: Phase) -> UpgradeOutcome {
    match phase {
        Phase::Committed => UpgradeOutcome::Committed,
        Phase::RolledBack => UpgradeOutcome::RolledBack,
        Phase::Restoring | Phase::RecoveryRequired => UpgradeOutcome::RecoveryRequired,
        _ => UpgradeOutcome::Pending,
    }
}

pub fn upgrade_observation(
    store: &EnvironmentStore,
) -> SetupResultValue<Option<UpgradeObservation>> {
    let _lock = store.lock()?;
    upgrade_observation_under_lock(store)
}

/// Internal readers that hold the existing setup fence can recheck identity
/// without creating a lock or attempting to acquire it recursively.
pub(crate) fn upgrade_observation_under_lock(
    store: &EnvironmentStore,
) -> SetupResultValue<Option<UpgradeObservation>> {
    let Some(journal): Option<UpgradeJournal> = store.read_json("upgrade.json")? else {
        return Ok(None);
    };
    let record = store.load_environment()?.ok_or_else(|| {
        error(
            "upgrade_observation",
            "The upgrade's original environment is unavailable",
        )
    })?;
    if journal.schema_version != journal.candidate.package_flavor.source_schema()
        || journal.record.environment_id != record.environment_id
        || journal.record.request.account != record.request.account
        || record.request.account.identity != crate::current_account()?.identity
        || uuid::Uuid::parse_str(&journal.operation_id).is_err()
        || !crate::unified_update::stable_version(&journal.candidate.version)
        || !crate::unified_update::lowercase_hex(&journal.candidate.source_sha, 40)
        || !crate::unified_update::valid_sha256(&journal.candidate.manifest_sha256)
    {
        return Err(error(
            "upgrade_observation",
            "The saved upgrade identity is invalid or belongs to another environment",
        ));
    }
    Ok(Some(UpgradeObservation {
        package_flavor: journal.candidate.package_flavor,
        environment_id: record.environment_id,
        operation_id: journal.operation_id,
        version: journal.candidate.version,
        source_sha: journal.candidate.source_sha,
        manifest_sha256: journal.candidate.manifest_sha256,
        outcome: outcome(journal.phase),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn starting_or_preparing_is_not_installation_success() {
        for phase in [
            Phase::Prepared,
            Phase::Stopping,
            Phase::Stopped,
            Phase::SnapshotReady,
            Phase::Verifying,
        ] {
            assert_eq!(outcome(phase), UpgradeOutcome::Pending);
        }
        assert_eq!(outcome(Phase::Committed), UpgradeOutcome::Committed);
        assert_eq!(outcome(Phase::RolledBack), UpgradeOutcome::RolledBack);
        assert_eq!(outcome(Phase::Restoring), UpgradeOutcome::RecoveryRequired);
        assert_eq!(
            outcome(Phase::RecoveryRequired),
            UpgradeOutcome::RecoveryRequired
        );
    }
}
