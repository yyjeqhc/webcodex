//! Core-owned, allowlisted upgrade projection. Never serialize the journal,
//! program targets, service definitions, installer receipts or maintenance data.
use super::*;
use crate::service::ServiceScope;

const MAX_STATUS_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradePhase {
    Prepared,
    Stopping,
    Stopped,
    SnapshotReady,
    Verifying,
    Committed,
    Restoring,
    RolledBack,
    RecoveryRequired,
}
impl From<Phase> for UpgradePhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Prepared => Self::Prepared,
            Phase::Stopping => Self::Stopping,
            Phase::Stopped => Self::Stopped,
            Phase::SnapshotReady => Self::SnapshotReady,
            Phase::Verifying => Self::Verifying,
            Phase::Committed => Self::Committed,
            Phase::Restoring => Self::Restoring,
            Phase::RolledBack => Self::RolledBack,
            Phase::RecoveryRequired => Self::RecoveryRequired,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeFileComponent {
    Cli,
    Server,
    Runner,
    Desktop,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeServiceKind {
    Server,
    Runner,
    Tunnel,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeServiceComponent {
    pub component: UpgradeServiceKind,
    pub scope: ServiceScope,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeStatus {
    #[serde(default, skip_serializing_if = "PackageFlavor::is_full")]
    pub package_flavor: PackageFlavor,
    pub schema_version: u16,
    pub environment_id: String,
    pub operation_id: String,
    pub version: String,
    pub source_sha: String,
    pub manifest_sha256: String,
    pub phase: UpgradePhase,
    pub files: Vec<UpgradeFileComponent>,
    pub services: Vec<UpgradeServiceComponent>,
    pub service_inventory_complete: bool,
}
/// The identity the caller selected. None admits only a fresh prepare; finish
/// and rollback require an exact persisted operation, including final retries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpgradeTarget {
    pub environment_id: String,
    pub manifest_sha256: String,
    pub operation_id: Option<String>,
}

fn bounded_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn invalid_identity() -> SetupDiagnostic {
    error(
        "upgrade_identity",
        "The saved upgrade identity is invalid or belongs to another environment",
    )
}
pub(super) fn validate_identity(
    journal: &UpgradeJournal,
    record: &EnvironmentRecord,
) -> SetupResultValue<()> {
    if journal.schema_version != journal.candidate.package_flavor.source_schema()
        || !bounded_identity(&record.environment_id)
        || journal.record.environment_id != record.environment_id
        || journal.record.request.account != record.request.account
        || record.request.account.identity != crate::current_account()?.identity
        || uuid::Uuid::parse_str(&journal.operation_id).is_err()
        || journal.operation_id.len() > 36
        || journal.candidate.version.len() > 64
        || !crate::unified_update::stable_version(&journal.candidate.version)
        || !crate::unified_update::lowercase_hex(&journal.candidate.source_sha, 40)
        || !crate::unified_update::valid_sha256(&journal.candidate.manifest_sha256)
    {
        return Err(invalid_identity());
    }
    Ok(())
}
pub(super) fn validate_target_under_lock(
    store: &EnvironmentStore,
    target: &UpgradeTarget,
    journal: Option<&UpgradeJournal>,
    candidate_manifest: Option<&str>,
) -> SetupResultValue<()> {
    let record = store.load_environment()?.ok_or_else(stale_target)?;
    if !bounded_identity(&target.environment_id)
        || !crate::unified_update::valid_sha256(&target.manifest_sha256)
        || record.environment_id != target.environment_id
        || record.request.account.identity != crate::current_account()?.identity
        || candidate_manifest.is_some_and(|manifest| manifest != target.manifest_sha256)
    {
        return Err(stale_target());
    }
    if let Some(journal) = journal {
        validate_identity(journal, &record)?;
    }
    match (&target.operation_id, journal) {
        (Some(operation), Some(journal))
            if operation == &journal.operation_id
                && target.manifest_sha256 == journal.candidate.manifest_sha256
                && !(candidate_manifest.is_some()
                    && matches!(journal.phase, Phase::Committed | Phase::RolledBack)) =>
        {
            Ok(())
        }
        (None, None) if candidate_manifest.is_some() => Ok(()),
        (None, Some(journal))
            if candidate_manifest.is_some()
                && matches!(journal.phase, Phase::Committed | Phase::RolledBack) =>
        {
            Ok(())
        }
        _ => Err(stale_target()),
    }
}
fn stale_target() -> SetupDiagnostic {
    error(
        "upgrade_target_changed",
        "The selected environment, candidate or upgrade operation changed; refresh update status",
    )
}
pub(super) fn tasks_unknown() -> SetupDiagnostic {
    error(
        "upgrade_tasks_unknown",
        "Active task state is unavailable; an idle environment cannot be assumed",
    )
}
pub(super) fn validate_task_record(
    record: &EnvironmentRecord,
    expected_environment_id: &str,
    owner_identity: &str,
) -> SetupResultValue<()> {
    if !bounded_identity(expected_environment_id)
        || record.environment_id != expected_environment_id
    {
        return Err(stale_target());
    }
    if record.request.account.identity != owner_identity {
        return Err(error(
            "upgrade_owner_unknown",
            "The saved environment does not belong to the current account",
        ));
    }
    if record.schema_version != 1
        || !record.configured
        || !crate::canonical_server_url(&record.request.server_url)
            .is_ok_and(|url| url == record.request.server_url)
    {
        return Err(tasks_unknown());
    }
    Ok(())
}
pub(super) fn load_task_record(
    store: &EnvironmentStore,
    expected_environment_id: &str,
) -> SetupResultValue<EnvironmentRecord> {
    let existing = EnvironmentStore::open_existing(store.root().to_path_buf())
        .map_err(|_| tasks_unknown())?
        .ok_or_else(stale_target)?;
    let record = existing
        .load_environment()
        .map_err(|_| tasks_unknown())?
        .ok_or_else(stale_target)?;
    let owner = crate::current_account().map_err(|_| tasks_unknown())?;
    validate_task_record(&record, expected_environment_id, &owner.identity)?;
    Ok(record)
}
pub(super) fn verify_task_record_unchanged(
    before: &EnvironmentRecord,
    after: &EnvironmentRecord,
) -> SetupResultValue<()> {
    if before.schema_version != after.schema_version
        || before.environment_id != after.environment_id
        || before.request != after.request
        || before.username != after.username
        || before.runner_client_id != after.runner_client_id
        || before.projects != after.projects
        || before.configured != after.configured
    {
        return Err(stale_target());
    }
    Ok(())
}
pub(super) fn task_count_from_responses(
    record: &EnvironmentRecord,
    runtime: &Value,
    runner: Option<&Value>,
) -> SetupResultValue<u64> {
    let runtime = runtime.get("output").unwrap_or(runtime);
    let count = if record.request.local_server() {
        runtime
            .pointer("/jobs/active_count")
            .and_then(Value::as_u64)
    } else if record.runner_client_id.is_some() {
        runner
            .and_then(|value| value.get("active_jobs"))
            .and_then(Value::as_u64)
    } else {
        Some(0)
    };
    count.ok_or_else(tasks_unknown)
}
pub(super) fn manual_recovery() -> SetupDiagnostic {
    error("upgrade_manual_recovery", "The installer may still own this upgrade boundary; inspect local status and use explicit installer recovery")
}
fn project(
    journal: &UpgradeJournal,
    record: &EnvironmentRecord,
) -> SetupResultValue<UpgradeStatus> {
    validate_identity(journal, record)?;
    let mut files = Vec::new();
    for program in &journal.programs {
        let component = match program.name.as_str() {
            "webcodex" => UpgradeFileComponent::Cli,
            "webcodex-server" => UpgradeFileComponent::Server,
            "webcodex-runner" => UpgradeFileComponent::Runner,
            _ => return Err(invalid_identity()),
        };
        if !files.contains(&component) {
            files.push(component);
        }
    }
    if journal.desktop.is_some() {
        files.push(UpgradeFileComponent::Desktop);
    }
    let mut services = Vec::new();
    let inventory = if journal.inventory_complete {
        &journal.all_services
    } else {
        &journal.services
    };
    for spec in inventory {
        let service = UpgradeServiceComponent {
            component: match spec.component {
                Component::Server => UpgradeServiceKind::Server,
                Component::Runner => UpgradeServiceKind::Runner,
                Component::Tunnel => UpgradeServiceKind::Tunnel,
            },
            scope: spec.scope,
        };
        if !services.contains(&service) {
            services.push(service);
        }
    }
    let status = UpgradeStatus {
        package_flavor: journal.candidate.package_flavor,
        schema_version: 1,
        environment_id: record.environment_id.clone(),
        operation_id: journal.operation_id.clone(),
        version: journal.candidate.version.clone(),
        source_sha: journal.candidate.source_sha.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        phase: journal.phase.into(),
        files,
        services,
        service_inventory_complete: journal.inventory_complete,
    };
    if serde_json::to_vec(&status)
        .map_err(|_| SetupDiagnostic::io())?
        .len()
        > MAX_STATUS_BYTES
    {
        return Err(error(
            "upgrade_status_bound",
            "Upgrade status exceeded its serialized bound",
        ));
    }
    Ok(status)
}
/// Query only existing saved state. An absent root is normal; a missing or busy
/// setup fence in an existing root cannot confirm status and fails closed.
pub fn upgrade_status_at(root: &Path) -> SetupResultValue<Option<UpgradeStatus>> {
    let Some(store) = EnvironmentStore::open_existing(root.to_path_buf())? else {
        return Ok(None);
    };
    let _lock = store.lock_existing()?;
    let Some(journal): Option<UpgradeJournal> = store.read_json("upgrade.json")? else {
        return Ok(None);
    };
    let record = store.load_environment()?.ok_or_else(invalid_identity)?;
    project(&journal, &record).map(Some)
}
pub fn upgrade_status(store: &EnvironmentStore) -> SetupResultValue<Option<UpgradeStatus>> {
    upgrade_status_at(store.root())
}

#[cfg(test)]
mod tests;
