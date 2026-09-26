//! Installer preflight and a recoverable program/data switch. No project
//! directory is copied, rewritten or removed by an upgrade.
use crate::service::{Component, Ownership, ServiceAccount, ServiceManager, ServiceSpec};
use crate::storage::ensure_private_directory;
use crate::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component as PathComponent, Path, PathBuf};
use webcodex_core::desktop_runtime_contract::{MachineBuildInfo, DESKTOP_RUNTIME_CONTRACT};
mod desktop_tree;

const COMPONENTS: [&str; 4] = [
    "webcodex",
    "webcodex-server",
    "webcodex-runner",
    "webcodex-desktop",
];
const DATA_FORMAT: u16 = 1;
const SNAPSHOT_LIMIT: u64 = 16 * 1024 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeCandidate {
    pub version: String,
    pub source_sha: String,
    pub platform: String,
    pub source_workflow_run_id: u64,
    pub source_workflow_ref: String,
    pub manifest_sha256: String,
    #[serde(default)]
    pub provenance_verified: bool,
    pub root: PathBuf,
    pub artifacts: BTreeMap<String, CandidateArtifact>,
    #[serde(default)]
    pub desktop: Option<CandidateDesktop>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDesktop {
    pub path: PathBuf,
    pub sha256: String,
    pub executable: PathBuf,
    #[serde(default)]
    pub managed_files: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateArtifact {
    pub path: PathBuf,
    pub sha256: String,
    pub build_info: MachineBuildInfo,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePreflight {
    pub ready: bool,
    pub diagnostics: Vec<SetupDiagnostic>,
    pub candidate: UpgradeCandidate,
    pub active_tasks: u64,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Phase {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgramBackup {
    target: PathBuf,
    backup: PathBuf,
    sha256: String,
    name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpgradeJournal {
    schema_version: u16,
    operation_id: String,
    candidate: UpgradeCandidate,
    phase: Phase,
    record: EnvironmentRecord,
    services: Vec<ServiceSpec>,
    #[serde(default)]
    all_services: Vec<ServiceSpec>,
    #[serde(default)]
    inventory_complete: bool,
    programs: Vec<ProgramBackup>,
    #[serde(default)]
    desktop: Option<DesktopBackup>,
    data: Option<PathBuf>,
    data_snapshot: Option<PathBuf>,
    #[serde(default)]
    rollback_from: Option<Phase>,
    #[serde(default)]
    replacement_started: bool,
    #[serde(default)]
    snapshot_digest: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct DesktopBackup {
    pub(crate) target: PathBuf,
    pub(crate) backup: PathBuf,
    pub(crate) sha256: String,
    #[serde(default)]
    pub(crate) directory_modes_sha256: String,
}
fn managed_desktop_target(binaries: &RuntimeBinaries) -> Option<PathBuf> {
    // Only package-managed program payloads are part of the Core rollback.
    // Linux .desktop entries, NSIS metadata, and OS launcher registrations
    // belong to the package manager's installation transaction.
    let runtime = binaries.cli.parent()?;
    if binaries.cli != runtime.join(format!("webcodex{}", std::env::consts::EXE_SUFFIX))
        || binaries.server
            != runtime.join(format!("webcodex-server{}", std::env::consts::EXE_SUFFIX))
        || binaries.runner
            != runtime.join(format!("webcodex-runner{}", std::env::consts::EXE_SUFFIX))
    {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        (runtime == Path::new("/usr/lib/webcodex/webcodex-runtime"))
            .then(|| PathBuf::from("/usr/lib/webcodex/webcodex-desktop"))
    }
    #[cfg(target_os = "macos")]
    {
        (runtime == Path::new("/Library/Application Support/WebCodex/runtime"))
            .then(|| PathBuf::from("/Applications/WebCodex Desktop.app"))
    }
    #[cfg(windows)]
    {
        (runtime.file_name().and_then(|part| part.to_str()) == Some("webcodex-runtime"))
            .then(|| runtime.parent().unwrap().join("WebCodex.exe"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        None
    }
}
fn desktop_installed_hash(path: &Path) -> SetupResultValue<String> {
    #[cfg(target_os = "linux")]
    {
        desktop_tree::digest_named(path, Some("webcodex-desktop"))
    }
    #[cfg(target_os = "macos")]
    {
        desktop_tree::digest(path)
    }
    #[cfg(windows)]
    {
        digest(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Err(SetupDiagnostic::io())
    }
}
fn desktop_directory_modes_hash(path: &Path) -> SetupResultValue<String> {
    #[cfg(unix)]
    {
        desktop_tree::directory_modes_digest(path)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(String::new())
    }
}
fn snapshot_desktop(
    binaries: &RuntimeBinaries,
    backup_root: &Path,
) -> SetupResultValue<Option<DesktopBackup>> {
    let Some(target) = managed_desktop_target(binaries) else {
        return Ok(None);
    };
    for ancestor in target.ancestors() {
        if std::fs::symlink_metadata(ancestor)
            .map_err(|_| SetupDiagnostic::io())?
            .is_symlink()
        {
            return Err(error(
                "upgrade_desktop_path",
                "The managed Desktop installation contains a link",
            ));
        }
    }
    #[cfg(windows)]
    if crate::storage::windows_path_owner(&target)?
        != crate::storage::windows_path_owner(&binaries.cli)?
    {
        return Err(error(
            "upgrade_desktop_owner",
            "The managed Desktop executable belongs to another owner",
        ));
    }
    let backup = backup_root.join(if cfg!(target_os = "macos") {
        "desktop.app"
    } else {
        "desktop"
    });
    #[cfg(unix)]
    desktop_tree::copy(&target, &backup)?;
    #[cfg(windows)]
    {
        let mut remaining = SNAPSHOT_LIMIT;
        copy_bounded(&target, &backup, &mut remaining)?;
    }
    let sha256 = desktop_installed_hash(&backup)?;
    let directory_modes_sha256 = desktop_directory_modes_hash(&backup)?;
    if desktop_installed_hash(&target)? != sha256
        || desktop_directory_modes_hash(&target)? != directory_modes_sha256
    {
        return Err(error(
            "upgrade_desktop_changed",
            "The Desktop installation changed during its recovery snapshot",
        ));
    }
    Ok(Some(DesktopBackup {
        target,
        backup,
        sha256,
        directory_modes_sha256,
    }))
}
/// Non-secret, owner-bound handoff to an elevated package installer. The
/// installer must use these exact targets and pass this exact receipt path;
/// it must never infer an environment directory from its root account.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedInstallationReceipt {
    pub schema_version: u16,
    pub operation_id: String,
    pub environment_id: String,
    pub environment_dir: PathBuf,
    pub owner_identity: String,
    pub manifest_sha256: String,
    pub targets: BTreeMap<String, PathBuf>,
    #[serde(default)]
    pub desktop_target: Option<PathBuf>,
}
const PREPARED_RECEIPT: &str = "upgrade-prepared.json";

fn prepared_receipt(root: &Path, journal: &UpgradeJournal) -> PreparedInstallationReceipt {
    PreparedInstallationReceipt {
        schema_version: 1,
        operation_id: journal.operation_id.clone(),
        environment_id: journal.record.environment_id.clone(),
        environment_dir: root.to_path_buf(),
        owner_identity: journal.record.request.account.identity.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        targets: journal
            .programs
            .iter()
            .map(|item| (item.name.clone(), item.target.clone()))
            .collect(),
        desktop_target: journal.desktop.as_ref().map(|item| item.target.clone()),
    }
}

fn write_prepared_receipt(
    store: &EnvironmentStore,
    journal: &UpgradeJournal,
) -> SetupResultValue<()> {
    store.write_json(PREPARED_RECEIPT, &prepared_receipt(store.root(), journal))
}

fn verify_service_inventory(journal: &UpgradeJournal) -> SetupResultValue<()> {
    if !journal.inventory_complete
        || (journal.record.request.local_server()
            && !journal
                .all_services
                .iter()
                .any(|spec| spec.component == Component::Server))
        || (journal.record.request.local_runner()
            && !journal
                .all_services
                .iter()
                .any(|spec| spec.component == Component::Runner))
        || journal
            .services
            .iter()
            .any(|running| !journal.all_services.contains(running))
    {
        return Err(error(
            "upgrade_service_inventory",
            "The prepared service inventory is incomplete",
        ));
    }
    for spec in &journal.all_services {
        let expected_program = match spec.component {
            Component::Runner => &journal.record.request.binaries.runner,
            Component::Server if cfg!(windows) => &journal.record.request.binaries.server,
            Component::Server | Component::Tunnel => &journal.record.request.binaries.cli,
        };
        let identity_matches = if spec.component == Component::Tunnel {
            spec.config_identity
                .starts_with(&format!("{}-tunnel-", journal.record.environment_id))
        } else {
            spec.config_identity == journal.record.environment_id
        };
        if !identity_matches || spec.program != *expected_program {
            return Err(error(
                "upgrade_service_inventory",
                "A saved service differs from the installed environment",
            ));
        }
    }
    Ok(())
}

fn checked_owner(path: &Path, identity: &str) -> SetupResultValue<()> {
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| SetupDiagnostic::io())?;
        if metadata.is_symlink() {
            return Err(error(
                "upgrade_receipt_owner",
                "The prepared installation path contains a link",
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(error(
                    "upgrade_receipt_owner",
                    "The prepared installation path contains a reparse point",
                ));
            }
        }
    }
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if metadata.is_symlink() {
        return Err(error(
            "upgrade_receipt_owner",
            "The prepared installation path is a link",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid().to_string() != identity || metadata.mode() & 0o077 != 0 {
            return Err(error(
                "upgrade_receipt_owner",
                "The prepared installation is not private to the original owner",
            ));
        }
    }
    #[cfg(windows)]
    {
        if crate::storage::windows_path_owner(path)? != identity {
            return Err(error(
                "upgrade_receipt_owner",
                "The prepared installation belongs to another account",
            ));
        }
        crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| {
            error(
                "upgrade_receipt_owner",
                "The prepared installation ACL is not private",
            )
        })?;
    }
    Ok(())
}

/// Validate an explicit user-created prepare receipt before a privileged
/// installer changes binaries. The caller must keep the validated exact
/// target list; no default Store or alternate target is accepted.
pub async fn verify_prepared_installation(
    receipt_path: &Path,
    candidate_dir: &Path,
) -> SetupResultValue<PreparedInstallationReceipt> {
    if !receipt_path.is_absolute()
        || receipt_path.file_name().and_then(|name| name.to_str()) != Some(PREPARED_RECEIPT)
    {
        return Err(error(
            "upgrade_receipt",
            "Pass the exact prepared installation receipt path",
        ));
    }
    let root = receipt_path.parent().ok_or_else(SetupDiagnostic::io)?;
    if root.canonicalize().map_err(|_| SetupDiagnostic::io())? != root {
        return Err(error(
            "upgrade_receipt",
            "The prepared environment directory must be canonical",
        ));
    }
    let bytes = bounded_file(receipt_path)?;
    let receipt: PreparedInstallationReceipt = serde_json::from_slice(&bytes).map_err(|_| {
        error(
            "upgrade_receipt",
            "The prepared installation receipt is invalid",
        )
    })?;
    if receipt.schema_version != 1
        || receipt.environment_dir != root
        || receipt.operation_id.is_empty()
        || receipt.environment_id.is_empty()
    {
        return Err(error(
            "upgrade_receipt",
            "The prepared installation receipt does not match its directory",
        ));
    }
    checked_owner(root, &receipt.owner_identity)?;
    checked_owner(receipt_path, &receipt.owner_identity)?;
    let journal_path = root.join("upgrade.json");
    checked_owner(&journal_path, &receipt.owner_identity)?;
    let journal: UpgradeJournal = serde_json::from_slice(&bounded_file(&journal_path)?)
        .map_err(|_| error("upgrade_receipt", "The upgrade journal is invalid"))?;
    let record_path = root.join("environment.json");
    checked_owner(&record_path, &receipt.owner_identity)?;
    let current: EnvironmentRecord = serde_json::from_slice(&bounded_file(&record_path)?)
        .map_err(|_| error("upgrade_receipt", "The saved environment is invalid"))?;
    if receipt.owner_identity != current.request.account.identity
        || receipt.owner_identity != journal.record.request.account.identity
        || current.environment_id != receipt.environment_id
        || current.request.account != journal.record.request.account
        || current.request.binaries != journal.record.request.binaries
    {
        return Err(error(
            "upgrade_receipt",
            "The prepared installation differs from the saved environment",
        ));
    }
    if journal.phase != Phase::SnapshotReady
        || !journal.candidate.provenance_verified
        || receipt != prepared_receipt(root, &journal)
    {
        return Err(error(
            "upgrade_receipt",
            "The upgrade is not prepared for this owner and candidate",
        ));
    }
    let candidate = verify_upgrade_candidate(candidate_dir)?;
    if candidate.manifest_sha256 != receipt.manifest_sha256 {
        return Err(error(
            "upgrade_receipt",
            "The installer candidate differs from the prepared candidate",
        ));
    }
    let mut prepared_candidate = journal.candidate.clone();
    prepared_candidate.provenance_verified = false;
    if serde_json::to_value(&prepared_candidate).map_err(|_| SetupDiagnostic::io())?
        != serde_json::to_value(&candidate).map_err(|_| SetupDiagnostic::io())?
    {
        return Err(error(
            "upgrade_receipt",
            "Prepared component identities differ from the published candidate",
        ));
    }
    verify_published_provenance(&candidate).await?;
    // The user-side prepare already ran the bounded metadata probe. Elevated
    // installer verification must never execute a candidate as administrator.
    verify_candidate_executable_headers(&candidate)?;
    verify_service_inventory(&journal)?;
    if let Some(desktop) = &journal.desktop {
        if managed_desktop_target(&journal.record.request.binaries).as_ref()
            != Some(&desktop.target)
            || desktop_installed_hash(&desktop.backup)? != desktop.sha256
            || desktop_installed_hash(&desktop.target)? != desktop.sha256
            || desktop_directory_modes_hash(&desktop.backup)? != desktop.directory_modes_sha256
            || desktop_directory_modes_hash(&desktop.target)? != desktop.directory_modes_sha256
        {
            return Err(error(
                "upgrade_desktop_changed",
                "The prepared Desktop backup or installation changed",
            ));
        }
    }
    for spec in &journal.all_services {
        let status = ServiceManager::inspect(spec).map_err(crate::native::service_error)?;
        if status.ownership != Ownership::Owned || status.running != Some(false) {
            return Err(error(
                "upgrade_service_running",
                "An original service is not confirmed stopped",
            ));
        }
    }
    Ok(receipt)
}

#[cfg(unix)]
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FrozenInstallerUpgrade {
    pub root: PathBuf,
    pub operation_id: String,
    pub owner: LocalAccount,
    pub environment_id: String,
    pub services: Vec<ServiceSpec>,
    pub all_services: Vec<ServiceSpec>,
    pub programs: Vec<(String, PathBuf, PathBuf, String)>,
    pub(crate) desktop: Option<DesktopBackup>,
    pub manifest_sha256: String,
    pub record_fingerprint: String,
    pub candidate_fingerprint: String,
    pub installed_cli: PathBuf,
    pub candidate_cli_sha256: String,
    pub backup_cli: PathBuf,
}

#[cfg(unix)]
pub(crate) fn freeze_installer_upgrade(
    receipt: &PreparedInstallationReceipt,
) -> SetupResultValue<FrozenInstallerUpgrade> {
    let root = &receipt.environment_dir;
    checked_owner(root, &receipt.owner_identity)?;
    let journal_path = root.join("upgrade.json");
    let record_path = root.join("environment.json");
    let receipt_path = root.join(PREPARED_RECEIPT);
    for path in [&journal_path, &record_path, &receipt_path] {
        checked_owner(path, &receipt.owner_identity)?;
    }
    let saved_receipt: PreparedInstallationReceipt =
        serde_json::from_slice(&bounded_file(&receipt_path)?)
            .map_err(|_| error("upgrade_receipt", "The saved prepared receipt is invalid"))?;
    let journal: UpgradeJournal = serde_json::from_slice(&bounded_file(&journal_path)?)
        .map_err(|_| error("upgrade_receipt", "The prepared journal is invalid"))?;
    let current: EnvironmentRecord = serde_json::from_slice(&bounded_file(&record_path)?)
        .map_err(|_| error("upgrade_receipt", "The saved environment is invalid"))?;
    if journal.phase != Phase::SnapshotReady
        || !journal.candidate.provenance_verified
        || saved_receipt != *receipt
        || prepared_receipt(root, &journal) != *receipt
        || receipt.owner_identity != current.request.account.identity
        || receipt.owner_identity != journal.record.request.account.identity
        || current.environment_id != receipt.environment_id
        || current.request.binaries != journal.record.request.binaries
    {
        return Err(error(
            "upgrade_receipt",
            "The original owner, environment, or prepared operation changed",
        ));
    }
    verify_service_inventory(&journal)?;
    for spec in &journal.all_services {
        let ServiceAccount::SystemUser {
            expected_identity, ..
        } = &spec.account
        else {
            return Err(error(
                "upgrade_service_inventory",
                "A Unix service uses a different account type",
            ));
        };
        if expected_identity != &receipt.owner_identity {
            return Err(error(
                "upgrade_service_inventory",
                "A service account differs from the original owner",
            ));
        }
        let status = ServiceManager::inspect(spec).map_err(crate::native::service_error)?;
        if status.ownership != Ownership::Owned || status.running != Some(false) {
            return Err(error(
                "upgrade_service_running",
                "A prepared service is no longer confirmed stopped",
            ));
        }
    }
    let backup_root = root.join("upgrade-backups").join(&journal.operation_id);
    checked_owner(&backup_root, &receipt.owner_identity)?;
    let mut programs = Vec::new();
    for backup in &journal.programs {
        if receipt.targets.get(&backup.name) != Some(&backup.target)
            || backup.backup
                != backup_root.join(format!("{}{}", backup.name, std::env::consts::EXE_SUFFIX))
        {
            return Err(error(
                "upgrade_restore_authorization",
                "A prepared program target or backup changed",
            ));
        }
        checked_owner(&backup.backup, &receipt.owner_identity)?;
        if digest(&backup.backup)? != backup.sha256 {
            return Err(error(
                "upgrade_backup_changed",
                "A retained program backup changed",
            ));
        }
        programs.push((
            backup.name.clone(),
            backup.target.clone(),
            backup.backup.clone(),
            backup.sha256.clone(),
        ));
    }
    if journal.desktop.as_ref().map(|item| &item.target) != receipt.desktop_target.as_ref()
        || managed_desktop_target(&journal.record.request.binaries).as_ref()
            != receipt.desktop_target.as_ref()
    {
        return Err(error(
            "upgrade_restore_authorization",
            "The prepared Desktop target changed",
        ));
    }
    if let Some(item) = &journal.desktop {
        if item.backup
            != backup_root.join(if cfg!(target_os = "macos") {
                "desktop.app"
            } else {
                "desktop"
            })
            || desktop_installed_hash(&item.backup)? != item.sha256
            || desktop_directory_modes_hash(&item.backup)? != item.directory_modes_sha256
        {
            return Err(error(
                "upgrade_backup_changed",
                "The retained Desktop backup changed",
            ));
        }
    }
    let cli = journal
        .candidate
        .artifacts
        .get("webcodex")
        .ok_or_else(|| error("candidate_components", "The prepared CLI is missing"))?;
    let backup_cli = journal
        .programs
        .iter()
        .find(|backup| backup.name == "webcodex")
        .ok_or_else(|| {
            error(
                "upgrade_backup_changed",
                "The retained CLI backup is missing",
            )
        })?;
    Ok(FrozenInstallerUpgrade {
        root: root.clone(),
        operation_id: journal.operation_id,
        owner: current.request.account,
        environment_id: receipt.environment_id.clone(),
        services: journal.services,
        all_services: journal.all_services,
        programs,
        desktop: journal.desktop,
        manifest_sha256: receipt.manifest_sha256.clone(),
        installed_cli: current.request.binaries.cli,
        record_fingerprint: hex(
            &serde_json::to_vec(&journal.record).map_err(|_| SetupDiagnostic::io())?
        ),
        candidate_fingerprint: hex(
            &serde_json::to_vec(&journal.candidate).map_err(|_| SetupDiagnostic::io())?
        ),
        candidate_cli_sha256: cli.sha256.clone(),
        backup_cli: backup_cli.backup.clone(),
    })
}

#[cfg(unix)]
pub(crate) fn check_frozen_installer_transition(
    frozen: &FrozenInstallerUpgrade,
) -> SetupResultValue<&'static str> {
    checked_owner(&frozen.root.join("upgrade.json"), &frozen.owner.identity)?;
    let journal: UpgradeJournal =
        serde_json::from_slice(&bounded_file(&frozen.root.join("upgrade.json"))?)
            .map_err(|_| error("upgrade_receipt", "The upgrade journal is invalid"))?;
    let programs: Vec<_> = journal
        .programs
        .iter()
        .map(|backup| {
            (
                backup.name.clone(),
                backup.target.clone(),
                backup.backup.clone(),
                backup.sha256.clone(),
            )
        })
        .collect();
    if journal.operation_id != frozen.operation_id
        || journal.record.environment_id != frozen.environment_id
        || journal.record.request.account != frozen.owner
        || journal.candidate.manifest_sha256 != frozen.manifest_sha256
        || hex(&serde_json::to_vec(&journal.record).map_err(|_| SetupDiagnostic::io())?)
            != frozen.record_fingerprint
        || hex(&serde_json::to_vec(&journal.candidate).map_err(|_| SetupDiagnostic::io())?)
            != frozen.candidate_fingerprint
        || journal.services != frozen.services
        || journal.all_services != frozen.all_services
        || programs != frozen.programs
        || journal.desktop != frozen.desktop
    {
        return Err(error(
            "upgrade_restore_authorization",
            "The authorized upgrade identity changed during finalization",
        ));
    }
    Ok(match journal.phase {
        Phase::SnapshotReady => "snapshot_ready",
        Phase::Verifying => "verifying",
        Phase::Committed => "committed",
        Phase::Restoring => "restoring",
        Phase::RolledBack => "rolled_back",
        _ => "invalid",
    })
}

#[cfg(unix)]
pub(crate) fn installed_cli_matches(frozen: &FrozenInstallerUpgrade) -> bool {
    digest(&frozen.installed_cli).ok().as_deref() == Some(&frozen.candidate_cli_sha256)
}
fn error(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Keep the installed version and saved credentials; inspect environment doctor before resuming this upgrade")
}
/// Call while holding `EnvironmentStore::lock()` before any non-upgrade
/// operation can mutate services, projects, credentials, or setup state.
pub fn ensure_upgrade_idle_under_lock(store: &EnvironmentStore) -> SetupResultValue<()> {
    let journal: Option<UpgradeJournal> = store.read_json("upgrade.json")?;
    if journal.is_some_and(|journal| !matches!(journal.phase, Phase::Committed | Phase::RolledBack))
    {
        return Err(error(
            "upgrade_pending",
            "Finish or roll back the prepared upgrade before changing this environment",
        ));
    }
    Ok(())
}
fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn digest(path: &Path) -> SetupResultValue<String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| error("candidate_file", "An upgrade artifact is missing"))?;
    if !metadata.is_file() || metadata.is_symlink() || metadata.len() > 4 * 1024 * 1024 * 1024 {
        return Err(error(
            "candidate_file",
            "An upgrade artifact is not a bounded regular file",
        ));
    }
    let mut stream = std::fs::File::open(path).map_err(|_| SetupDiagnostic::io())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let size = stream
            .read(&mut buffer)
            .map_err(|_| SetupDiagnostic::io())?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn bounded_file(path: &Path) -> SetupResultValue<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if metadata.is_symlink() || !metadata.is_file() || metadata.len() > 1024 * 1024 {
        return Err(error(
            "candidate_manifest",
            "Candidate metadata must be a bounded regular file",
        ));
    }
    let mut data = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| SetupDiagnostic::io())?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut data)
        .map_err(|_| SetupDiagnostic::io())?;
    if data.len() > 1024 * 1024 {
        return Err(SetupDiagnostic::io());
    }
    Ok(data)
}
fn artifact_path(root: &Path, value: &str) -> SetupResultValue<PathBuf> {
    let relative = Path::new(value);
    if value.contains('\\')
        || relative
            .components()
            .any(|part| !matches!(part, PathComponent::Normal(_)))
    {
        return Err(error(
            "candidate_path",
            "An artifact path leaves the candidate directory",
        ));
    }
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part.as_os_str());
        if std::fs::symlink_metadata(&path)
            .map_err(|_| SetupDiagnostic::io())?
            .is_symlink()
        {
            return Err(error(
                "candidate_path",
                "Artifact paths must not contain links",
            ));
        }
    }
    if !path
        .canonicalize()
        .map_err(|_| SetupDiagnostic::io())?
        .starts_with(root)
    {
        return Err(error(
            "candidate_path",
            "Artifact path escapes the candidate directory",
        ));
    }
    Ok(path)
}
fn string<'a>(value: &'a Value, field: &str) -> SetupResultValue<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| error("candidate_manifest", "Required release metadata is absent"))
}
fn parse_desktop_payload(
    root: &Path,
    manifest: &Value,
    artifacts: &BTreeMap<String, CandidateArtifact>,
) -> SetupResultValue<CandidateDesktop> {
    let payload = manifest
        .get("desktop_payload")
        .ok_or_else(|| error("candidate_manifest", "Desktop payload evidence is missing"))?;
    if payload
        .as_object()
        .is_none_or(|object| object.len() != if cfg!(windows) { 4 } else { 3 })
    {
        return Err(error(
            "candidate_manifest",
            "Desktop payload evidence has an unexpected shape",
        ));
    }
    let path = artifact_path(root, string(payload, "path")?)?;
    let expected = string(payload, "sha256")?;
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(error(
            "candidate_checksum",
            "Desktop payload digest is invalid",
        ));
    }
    let executable_relative = string(payload, "executable")?;
    let executable_path = Path::new(executable_relative);
    if executable_relative.contains('\\')
        || executable_path
            .components()
            .any(|part| !matches!(part, PathComponent::Normal(_)))
    {
        return Err(error(
            "candidate_path",
            "Desktop executable path is invalid",
        ));
    }
    let executable = if cfg!(target_os = "macos") {
        if path.extension().and_then(|part| part.to_str()) != Some("app")
            || !executable_relative.starts_with("Contents/MacOS/")
        {
            return Err(error(
                "candidate_path",
                "Desktop app bundle layout is invalid",
            ));
        }
        path.join(executable_path)
    } else {
        if path.is_dir()
            || executable_path.file_name() != path.file_name()
            || executable_path.components().count() != 1
        {
            return Err(error(
                "candidate_path",
                "Desktop executable layout is invalid",
            ));
        }
        path.clone()
    };
    let executable = executable
        .canonicalize()
        .map_err(|_| SetupDiagnostic::io())?;
    if executable
        != artifacts
            .get("webcodex-desktop")
            .ok_or_else(|| error("candidate_components", "Desktop binary is missing"))?
            .path
    {
        return Err(error(
            "candidate_identity",
            "Desktop payload does not contain the published executable",
        ));
    }
    #[cfg(unix)]
    if desktop_installed_hash(&path)? != expected {
        return Err(error(
            "candidate_checksum",
            "Desktop payload tree differs from the published manifest",
        ));
    }
    let mut managed_files = BTreeMap::new();
    if cfg!(windows) {
        let entries = payload
            .get("managed_files")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                error(
                    "candidate_manifest",
                    "Windows managed Desktop files are missing",
                )
            })?;
        if entries.len() != 4 {
            return Err(error(
                "candidate_manifest",
                "Windows managed Desktop file list is incomplete",
            ));
        }
        for entry in entries {
            if entry.as_object().is_none_or(|object| object.len() != 2) {
                return Err(error(
                    "candidate_manifest",
                    "A Windows managed file has an unexpected shape",
                ));
            }
            let relative = string(entry, "path")?;
            let hash = string(entry, "sha256")?;
            if hash.len() != 64
                || !hash
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || managed_files
                    .insert(relative.to_string(), hash.to_string())
                    .is_some()
            {
                return Err(error(
                    "candidate_manifest",
                    "Windows managed Desktop file evidence is invalid",
                ));
            }
        }
        for (relative, component) in [
            ("WebCodex.exe", "webcodex-desktop"),
            ("webcodex-runtime/webcodex.exe", "webcodex"),
            ("webcodex-runtime/webcodex-server.exe", "webcodex-server"),
            ("webcodex-runtime/webcodex-runner.exe", "webcodex-runner"),
        ] {
            if managed_files.get(relative).map(String::as_str)
                != artifacts.get(component).map(|item| item.sha256.as_str())
            {
                return Err(error(
                    "candidate_identity",
                    "Windows managed Desktop files differ from the component artifacts",
                ));
            }
        }
    } else if payload.get("managed_files").is_some() {
        return Err(error(
            "candidate_manifest",
            "Unexpected managed Desktop file list",
        ));
    }
    Ok(CandidateDesktop {
        path,
        sha256: expected.into(),
        executable,
        managed_files,
    })
}
pub fn verify_upgrade_candidate(root: &Path) -> SetupResultValue<UpgradeCandidate> {
    let root = root
        .canonicalize()
        .map_err(|_| error("candidate_path", "Candidate directory is unavailable"))?;
    let bytes = bounded_file(&root.join("source-manifest.json"))?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|_| {
        error(
            "candidate_manifest",
            "Candidate release manifest is invalid",
        )
    })?;
    if manifest.get("schema_version").and_then(Value::as_u64) != Some(1) {
        return Err(error(
            "candidate_schema",
            "Candidate release schema is unsupported",
        ));
    }
    let version = string(&manifest, "version")?;
    let source = string(&manifest, "source_sha")?;
    if source.len() != 40
        || !source
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(error("candidate_source", "Candidate source SHA is invalid"));
    }
    let workflow = string(&manifest, "source_workflow_ref")?;
    if ![
        "yyjeqhc/webcodex/.github/workflows/release-build.yml@",
        "Xiiiing/webcodex/.github/workflows/release-build.yml@",
    ]
    .iter()
    .any(|prefix| workflow.starts_with(prefix))
    {
        return Err(error(
            "candidate_provenance",
            "Candidate build provenance does not identify an approved release workflow",
        ));
    }
    let run = manifest
        .get("source_workflow_run_id")
        .and_then(Value::as_u64)
        .filter(|run| *run > 0)
        .ok_or_else(|| {
            error(
                "candidate_provenance",
                "The native build provenance is missing",
            )
        })?;
    let platform = format!(
        "{}-{}",
        match std::env::consts::OS {
            "macos" => "darwin",
            "windows" => "win32",
            other => other,
        },
        match std::env::consts::ARCH {
            "x86_64" => "x64",
            "aarch64" => "arm64",
            other => other,
        }
    );
    if string(&manifest, "platform")? != platform {
        return Err(error(
            "candidate_architecture",
            "The candidate does not match this operating system and CPU architecture",
        ));
    }
    let sums = String::from_utf8(bounded_file(&root.join("SHA256SUMS"))?)
        .map_err(|_| error("candidate_checksum", "Candidate checksums are invalid"))?;
    let mut checksums = BTreeMap::new();
    for line in sums.lines() {
        let (hash, name) = line
            .split_once("  ")
            .ok_or_else(|| error("candidate_checksum", "Candidate checksums are malformed"))?;
        if checksums.insert(name, hash).is_some() {
            return Err(error(
                "candidate_checksum",
                "Candidate checksums contain duplicate paths",
            ));
        }
    }
    if checksums.get("source-manifest.json").copied() != Some(hex(&bytes).as_str()) {
        return Err(error(
            "candidate_checksum",
            "Candidate release manifest hash does not match",
        ));
    }
    let declared = manifest
        .get("artifacts")
        .and_then(Value::as_object)
        .ok_or_else(|| error("candidate_manifest", "Component manifest is missing"))?;
    if declared.len() != COMPONENTS.len() {
        return Err(error(
            "candidate_components",
            "A unified candidate must contain exactly four components",
        ));
    }
    let mut artifacts = BTreeMap::new();
    for name in COMPONENTS {
        let item = declared.get(name).ok_or_else(|| {
            error(
                "candidate_components",
                "The candidate omits a required component",
            )
        })?;
        let info_value = item
            .get("build_info")
            .ok_or_else(|| error("candidate_build_info", "Component build metadata is absent"))?;
        if hex(&serde_json::to_vec(info_value).map_err(|_| SetupDiagnostic::io())?)
            != string(item, "build_info_sha256")?
        {
            return Err(error(
                "candidate_build_info",
                "Component metadata hash does not match",
            ));
        }
        let info: MachineBuildInfo = serde_json::from_value(info_value.clone())
            .map_err(|_| error("candidate_build_info", "Component metadata is invalid"))?;
        info.validate(name).map_err(|_| {
            error(
                "candidate_build_info",
                "Component build identity is invalid",
            )
        })?;
        if info.version != version
            || info.git_commit.as_deref() != Some(source)
            || info.git_dirty != Some(false)
            || info.architecture != std::env::consts::ARCH
            || info.target != string(&manifest, "target")?
        {
            return Err(error(
                "candidate_identity",
                "Components do not share the declared source, version and native architecture",
            ));
        }
        if !info
            .desktop_runtime_contract
            .overlaps(DESKTOP_RUNTIME_CONTRACT)
            || info.environment_data_format != Some(DATA_FORMAT)
        {
            return Err(error("candidate_data_compatibility", "This version requires a protocol or persistent-data migration that is not supported"));
        }
        if matches!(name, "webcodex-server" | "webcodex-runner")
            && info.agent_protocol_generation
                != Some(webcodex_core::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2.get())
        {
            return Err(error(
                "candidate_protocol",
                "The candidate Runner protocol is incompatible",
            ));
        }
        if string(item, "probe")? != "native-build-job" {
            return Err(error(
                "candidate_provenance",
                "A component has no native build verification",
            ));
        }
        let relative = string(item, "path")?;
        let path = artifact_path(&root, relative)?;
        let expected = string(item, "sha256")?;
        if checksums.get(relative).copied() != Some(expected) || digest(&path)? != expected {
            return Err(error(
                "candidate_checksum",
                "A candidate component hash does not match",
            ));
        }
        artifacts.insert(
            name.to_string(),
            CandidateArtifact {
                path,
                sha256: expected.into(),
                build_info: info,
            },
        );
    }
    let desktop = parse_desktop_payload(&root, &manifest, &artifacts)?;
    Ok(UpgradeCandidate {
        version: version.into(),
        source_sha: source.into(),
        platform,
        source_workflow_run_id: run,
        source_workflow_ref: workflow.into(),
        manifest_sha256: hex(&bytes),
        provenance_verified: false,
        root,
        artifacts,
        desktop: Some(desktop),
    })
}

/// Proves that reinstalling this published package would not replace any
/// managed program byte. This is read-only and never infers an EnvironmentStore.
pub async fn verify_same_installed_package(
    candidate_dir: &Path,
    expected_runtime_dir: &Path,
) -> SetupResultValue<()> {
    let candidate = verify_upgrade_candidate(candidate_dir)?;
    verify_published_provenance(&candidate).await?;
    verify_candidate_executable_headers(&candidate)?;
    if !expected_runtime_dir.is_absolute()
        || expected_runtime_dir
            .canonicalize()
            .map_err(|_| SetupDiagnostic::io())?
            != expected_runtime_dir
    {
        return Err(error(
            "installer_targets",
            "The existing runtime directory is not canonical",
        ));
    }
    #[cfg(target_os = "linux")]
    if expected_runtime_dir != Path::new("/usr/lib/webcodex/webcodex-runtime") {
        return Err(error(
            "installer_targets",
            "The Linux package runtime target differs",
        ));
    }
    #[cfg(target_os = "macos")]
    if expected_runtime_dir != Path::new("/Library/Application Support/WebCodex/runtime") {
        return Err(error(
            "installer_targets",
            "The macOS package runtime target differs",
        ));
    }
    #[cfg(windows)]
    if expected_runtime_dir
        .file_name()
        .and_then(|name| name.to_str())
        != Some("webcodex-runtime")
    {
        return Err(error(
            "installer_targets",
            "The Windows package runtime target differs",
        ));
    }
    let desktop = candidate
        .desktop
        .as_ref()
        .ok_or_else(|| error("candidate_manifest", "Desktop payload evidence is missing"))?;
    for (name, target) in [
        (
            "webcodex",
            expected_runtime_dir.join(format!("webcodex{}", std::env::consts::EXE_SUFFIX)),
        ),
        (
            "webcodex-server",
            expected_runtime_dir.join(format!("webcodex-server{}", std::env::consts::EXE_SUFFIX)),
        ),
        (
            "webcodex-runner",
            expected_runtime_dir.join(format!("webcodex-runner{}", std::env::consts::EXE_SUFFIX)),
        ),
    ] {
        verify_installed_target_path(&target)?;
        if digest(&target)?
            != candidate
                .artifacts
                .get(name)
                .ok_or_else(|| SetupDiagnostic::io())?
                .sha256
        {
            return Err(error(
                "installer_not_same",
                "An installed runtime differs from the published package",
            ));
        }
    }
    #[cfg(target_os = "linux")]
    let desktop_target = PathBuf::from("/usr/lib/webcodex/webcodex-desktop");
    #[cfg(target_os = "macos")]
    let desktop_target = PathBuf::from("/Applications/WebCodex Desktop.app");
    #[cfg(windows)]
    let desktop_target = expected_runtime_dir
        .parent()
        .ok_or_else(SetupDiagnostic::io)?
        .join("WebCodex.exe");
    verify_installed_target_path(&desktop_target)?;
    #[cfg(unix)]
    if desktop_installed_hash(&desktop_target)? != desktop.sha256 {
        return Err(error(
            "installer_not_same",
            "The installed Desktop tree differs from the published package",
        ));
    }
    #[cfg(windows)]
    if digest(&desktop_target)?
        != *desktop
            .managed_files
            .get("WebCodex.exe")
            .ok_or_else(|| SetupDiagnostic::io())?
    {
        return Err(error(
            "installer_not_same",
            "The installed Desktop executable differs from the published package",
        ));
    }
    Ok(())
}

fn verify_installed_target_path(path: &Path) -> SetupResultValue<()> {
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor).map_err(|_| SetupDiagnostic::io())?;
        if metadata.is_symlink() {
            return Err(error(
                "installer_targets",
                "An installed package path contains a link",
            ));
        }
        #[cfg(windows)]
        if std::os::windows::fs::MetadataExt::file_attributes(&metadata) & 0x400 != 0 {
            return Err(error(
                "installer_targets",
                "An installed package path contains a reparse point",
            ));
        }
    }
    Ok(())
}

impl NativeEnvironment {
    pub async fn upgrade_preflight(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
    ) -> SetupResultValue<UpgradePreflight> {
        self.upgrade_preflight_with_options(store, candidate_dir, false)
            .await
    }
    /// Explicit development testing; never represented as verified release provenance.
    pub async fn upgrade_preflight_development(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
    ) -> SetupResultValue<UpgradePreflight> {
        self.upgrade_preflight_with_options(store, candidate_dir, true)
            .await
    }
    async fn upgrade_preflight_with_options(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
        development_build: bool,
    ) -> SetupResultValue<UpgradePreflight> {
        let mut candidate = verify_upgrade_candidate(candidate_dir)?;
        if !development_build {
            verify_published_provenance(&candidate).await?;
            candidate.provenance_verified = true;
        }
        verify_candidate_executables(&candidate).await?;
        let Some(record) = store.load_environment()? else {
            if store.load_journal()?.is_some() {
                return Err(error(
                    "setup_incomplete",
                    "Finish or recover environment setup before upgrading",
                ));
            }
            return Ok(UpgradePreflight {
                ready: true,
                candidate,
                diagnostics: vec![],
                active_tasks: 0,
            });
        };
        let mut diagnostics = Vec::new();
        for spec in configured_specs(store, &record)? {
            let status = ServiceManager::inspect(&spec).map_err(crate::native::service_error)?;
            if status.ownership != Ownership::Owned || status.running.is_none() {
                diagnostics.push(error(
                    "upgrade_owner_unknown",
                    "The current service owner or process state could not be verified",
                ));
            }
        }
        let token = if record.request.local_server() {
            crate::native::bootstrap_token(store)?
        } else {
            read_secret(&store.root().join("webcodex-user-token"))?
        };
        let status = self
            .post(
                &record.request.server_url,
                "/api/runtime/status",
                Some(token.expose()),
                json!({}),
            )
            .await?;
        let status = status.get("output").unwrap_or(&status);
        let active = if record.request.local_server() {
            status.pointer("/jobs/active_count").and_then(Value::as_u64)
        } else if let Some(client_id) = &record.runner_client_id {
            let token = read_secret(&store.root().join("webcodex-user-token"))?;
            let runner = self
                .post(
                    &record.request.server_url,
                    "/api/runtime-console/runner",
                    Some(token.expose()),
                    json!({"client_id":client_id,"project_limit":1}),
                )
                .await?;
            runner.get("active_jobs").and_then(Value::as_u64)
        } else {
            Some(0)
        }
        .ok_or_else(|| {
            error(
                "upgrade_tasks_unknown",
                "Active task state is unavailable; an idle environment cannot be assumed",
            )
        })?;
        if active != 0 {
            diagnostics.push(error(
                "upgrade_active_tasks",
                "Wait for active tasks to finish before upgrading",
            ));
        }
        Ok(UpgradePreflight {
            ready: diagnostics.is_empty(),
            diagnostics,
            candidate,
            active_tasks: active,
        })
    }

    pub async fn upgrade_prepare(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
    ) -> SetupResultValue<UpgradePreflight> {
        self.upgrade_prepare_with_options(store, candidate_dir, false)
            .await
    }
    pub async fn upgrade_prepare_development(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
    ) -> SetupResultValue<UpgradePreflight> {
        self.upgrade_prepare_with_options(store, candidate_dir, true)
            .await
    }
    async fn upgrade_prepare_with_options(
        &self,
        store: &EnvironmentStore,
        candidate_dir: &Path,
        development_build: bool,
    ) -> SetupResultValue<UpgradePreflight> {
        let lock = store.lock()?;
        let mut candidate = verify_upgrade_candidate(candidate_dir)?;
        if !development_build {
            verify_published_provenance(&candidate).await?;
            candidate.provenance_verified = true;
        }
        verify_candidate_executables(&candidate).await?;
        let previous: Option<UpgradeJournal> = store.read_json("upgrade.json")?;
        if let Some(previous) = previous
            .as_ref()
            .filter(|journal| matches!(journal.phase, Phase::Committed | Phase::RolledBack))
        {
            // A final decision may be durable while its gate release is still
            // awaiting a response. Finish that exact lease before admitting a
            // different upgrade operation.
            crate::upgrade_transport::end_maintenance(store, &previous.record).await?;
        }
        let mut journal = if let Some(previous) = previous
            .filter(|journal| !matches!(journal.phase, Phase::Committed | Phase::RolledBack))
        {
            if previous.candidate.manifest_sha256 != candidate.manifest_sha256 {
                return Err(error(
                    "upgrade_pending",
                    "A different candidate owns the unfinished upgrade",
                ));
            }
            if previous.phase == Phase::SnapshotReady {
                verify_stopped(store, &previous)?;
                write_prepared_receipt(store, &previous)?;
                return Ok(UpgradePreflight {
                    ready: true,
                    candidate,
                    diagnostics: vec![],
                    active_tasks: 0,
                });
            }
            if !matches!(
                previous.phase,
                Phase::Prepared | Phase::Stopping | Phase::Stopped
            ) {
                return Err(error(
                    "upgrade_pending",
                    "Finish or roll back the existing program switch before preparing another",
                ));
            }
            previous
        } else {
            let checked = self
                .upgrade_preflight_with_options(store, candidate_dir, development_build)
                .await?;
            if !checked.ready {
                return Err(checked.diagnostics[0].clone());
            }
            let Some(record) = store.load_environment()? else {
                return Ok(checked);
            };
            let operation_id = uuid::Uuid::new_v4().to_string();
            let backup = store.root().join("upgrade-backups").join(&operation_id);
            ensure_private_directory(&backup)?;
            let mut programs = Vec::new();
            let mut bytes_left = SNAPSHOT_LIMIT;
            for (name, target) in [
                ("webcodex", &record.request.binaries.cli),
                ("webcodex-server", &record.request.binaries.server),
                ("webcodex-runner", &record.request.binaries.runner),
            ] {
                let target = target.canonicalize().map_err(|_| SetupDiagnostic::io())?;
                let copy = backup.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
                copy_bounded(&target, &copy, &mut bytes_left)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&copy, std::fs::Permissions::from_mode(0o700))
                        .map_err(|_| SetupDiagnostic::io())?;
                }
                programs.push(ProgramBackup {
                    sha256: digest(&copy)?,
                    target,
                    backup: copy,
                    name: name.into(),
                });
            }
            let desktop = snapshot_desktop(&record.request.binaries, &backup)?;
            let mut running = Vec::new();
            let all_services = configured_specs(store, &record)?;
            for spec in &all_services {
                if ServiceManager::inspect(spec)
                    .map_err(crate::native::service_error)?
                    .running
                    == Some(true)
                {
                    running.push(spec.clone());
                }
            }
            let data = if record.request.local_server() {
                let env = read_secret(&store.root().join("server/webcodex.env"))?;
                let data = PathBuf::from(
                    crate::native::env_value(env.expose(), "WEBCODEX_DATA").ok_or_else(|| {
                        error("upgrade_data_path", "Server data location is unavailable")
                    })?,
                );
                if !data.is_absolute() {
                    return Err(error(
                        "upgrade_data_path",
                        "Server data location must be absolute before upgrading",
                    ));
                }
                Some(data)
            } else {
                None
            };
            let journal = UpgradeJournal {
                schema_version: 1,
                operation_id,
                candidate: candidate.clone(),
                phase: Phase::Prepared,
                record,
                services: running,
                all_services,
                inventory_complete: true,
                programs,
                desktop,
                data,
                data_snapshot: None,
                rollback_from: None,
                replacement_started: false,
                snapshot_digest: None,
            };
            save(store, &journal)?;
            journal
        };
        let prepared = self.complete_upgrade_preparation(store, &mut journal).await;
        drop(lock);
        if let Err(original) = prepared {
            if let Err(recovery) = self.upgrade_rollback(store).await {
                return Err(SetupDiagnostic::new(
                    "upgrade_recovery_required",
                    "Upgrade preparation failed and automatic restoration could not be verified",
                    &format!("{}; {}", original.code, recovery.code),
                ));
            }
            return Err(original);
        }
        Ok(UpgradePreflight {
            ready: true,
            candidate,
            diagnostics: vec![],
            active_tasks: 0,
        })
    }

    async fn complete_upgrade_preparation(
        &self,
        store: &EnvironmentStore,
        journal: &mut UpgradeJournal,
    ) -> SetupResultValue<()> {
        if journal.phase == Phase::Prepared {
            if journal.record.request.local_server() || journal.record.runner_client_id.is_some() {
                crate::upgrade_transport::begin_maintenance(
                    store,
                    &journal.record,
                    if journal.record.request.local_server() {
                        None
                    } else {
                        journal.record.runner_client_id.as_deref()
                    },
                )
                .await?;
            }
            journal.phase = Phase::Stopping;
            save(store, journal)?;
        }
        if journal.phase == Phase::Stopping {
            // A retry verifies the same fence before touching any still-running
            // owner. A stopped local Server already has no admission path.
            let local_stopped = journal.record.request.local_server()
                && ServiceManager::inspect(&service_spec(
                    store,
                    &journal.record,
                    Component::Server,
                )?)
                .map_err(crate::native::service_error)?
                .running
                    == Some(false);
            if !local_stopped
                && (journal.record.request.local_server()
                    || journal.record.runner_client_id.is_some())
            {
                crate::upgrade_transport::begin_maintenance(
                    store,
                    &journal.record,
                    if journal.record.request.local_server() {
                        None
                    } else {
                        journal.record.runner_client_id.as_deref()
                    },
                )
                .await?;
            }
            for component in [Component::Tunnel, Component::Runner, Component::Server] {
                for spec in journal
                    .services
                    .iter()
                    .filter(|spec| spec.component == component)
                {
                    crate::privilege::service_operation_spec(
                        store,
                        &journal.record,
                        spec.clone(),
                        ServiceOperation::Stop,
                        None,
                    )
                    .await?;
                }
            }
            verify_stopped(store, journal)?;
            journal.phase = Phase::Stopped;
            save(store, journal)?;
        }
        verify_stopped(store, journal)?;
        if let Some(desktop) = &journal.desktop {
            if desktop_installed_hash(&desktop.target)? != desktop.sha256
                || desktop_installed_hash(&desktop.backup)? != desktop.sha256
                || desktop_directory_modes_hash(&desktop.target)? != desktop.directory_modes_sha256
                || desktop_directory_modes_hash(&desktop.backup)? != desktop.directory_modes_sha256
            {
                return Err(error(
                    "upgrade_desktop_changed",
                    "The managed Desktop tree changed during preparation",
                ));
            }
        }
        if let Some(data) = &journal.data {
            let backup = store
                .root()
                .join("upgrade-backups")
                .join(&journal.operation_id);
            let snapshot = backup.join("server-data");
            if !snapshot.exists() {
                let partial = backup.join("server-data.partial");
                // Only our uncommitted snapshot is discarded. Source data and
                // completed recovery points are never removed on retry.
                if partial.exists() {
                    std::fs::remove_dir_all(&partial).map_err(|_| SetupDiagnostic::io())?;
                }
                let mut remaining = SNAPSHOT_LIMIT;
                copy_tree(data, &partial, &mut remaining, &mut 100_000usize)?;
                std::fs::rename(&partial, &snapshot).map_err(|_| SetupDiagnostic::io())?;
                #[cfg(unix)]
                std::fs::File::open(&backup)
                    .and_then(|file| file.sync_all())
                    .map_err(|_| SetupDiagnostic::io())?;
            }
            let digest = tree_digest(&snapshot)?;
            if tree_digest(data)? != digest {
                return Err(error(
                    "upgrade_data_changed",
                    "Server data changed while preparing its recovery snapshot",
                ));
            }
            journal.snapshot_digest = Some(digest);
            journal.data_snapshot = Some(snapshot);
        }
        journal.phase = Phase::SnapshotReady;
        save(store, journal)?;
        write_prepared_receipt(store, journal)
    }

    pub async fn upgrade_finish(&mut self, store: &EnvironmentStore) -> SetupResultValue<()> {
        let lock = store.lock()?;
        let Some(mut journal): Option<UpgradeJournal> = store.read_json("upgrade.json")? else {
            return Ok(());
        };
        if journal.phase == Phase::Committed {
            return crate::upgrade_transport::end_maintenance(store, &journal.record).await;
        }
        if !matches!(journal.phase, Phase::SnapshotReady | Phase::Verifying) {
            return Err(error(
                "upgrade_recovery_required",
                "The upgrade has not reached a recoverable installation boundary",
            ));
        }
        let completion = self.complete_upgrade_finish(store, &mut journal).await;
        drop(lock);
        if let Err(original) = completion {
            // A persisted commit cannot be undone. A failed gate release is
            // retried by calling finish again with the same journal.
            if journal.phase == Phase::Committed {
                return Err(original);
            }
            return match self.upgrade_rollback(store).await {
                Ok(()) => Err(original),
                Err(recovery) => Err(SetupDiagnostic::new(
                    "upgrade_recovery_required",
                    "Upgrade validation failed and automatic restoration could not be verified",
                    &format!("{}; {}", original.code, recovery.code),
                )),
            };
        }
        Ok(())
    }

    async fn complete_upgrade_finish(
        &mut self,
        store: &EnvironmentStore,
        journal: &mut UpgradeJournal,
    ) -> SetupResultValue<()> {
        journal.phase = Phase::Verifying;
        save(store, &journal)?;
        for backup in &journal.programs {
            let artifact = &journal.candidate.artifacts[&backup.name];
            if digest(&backup.target)? != artifact.sha256 {
                return Err(error("upgrade_installed_hash", "An installed component does not match the verified candidate; restore the previous installation"));
            }
        }
        if let Some(desktop) = &journal.desktop {
            let candidate = journal.candidate.desktop.as_ref().ok_or_else(|| {
                error(
                    "upgrade_desktop_missing",
                    "The candidate has no Desktop payload evidence",
                )
            })?;
            #[cfg(unix)]
            let expected = &candidate.sha256;
            #[cfg(windows)]
            let expected = candidate.managed_files.get("WebCodex.exe").ok_or_else(|| {
                error(
                    "candidate_manifest",
                    "The Windows Desktop executable hash is missing",
                )
            })?;
            if desktop_installed_hash(&desktop.target)? != *expected {
                return Err(error(
                    "upgrade_installed_hash",
                    "The installed Desktop tree differs from the published candidate",
                ));
            }
        }
        if !journal.replacement_started {
            if let (Some(data), Some(expected)) = (&journal.data, &journal.snapshot_digest) {
                if tree_digest(data)? != *expected {
                    return Err(error("upgrade_data_changed", "Server data changed after the installation snapshot; it was not overwritten"));
                }
            }
        }
        for component in [Component::Server, Component::Runner, Component::Tunnel] {
            if component == Component::Server
                && journal
                    .services
                    .iter()
                    .any(|spec| spec.component == component)
            {
                journal.replacement_started = true;
                save(store, &journal)?;
            }
            for spec in journal
                .services
                .iter()
                .filter(|spec| spec.component == component)
            {
                crate::privilege::service_operation_spec(
                    store,
                    &journal.record,
                    spec.clone(),
                    ServiceOperation::Start,
                    None,
                )
                .await?;
            }
        }
        self.verify_original_running(store, journal).await?;
        persist_final_decision(store, journal, Phase::Committed)?;
        crate::upgrade_transport::end_maintenance(store, &journal.record).await
    }

    pub async fn upgrade_rollback(&self, store: &EnvironmentStore) -> SetupResultValue<()> {
        let _lock = store.lock()?;
        let Some(mut journal): Option<UpgradeJournal> = store.read_json("upgrade.json")? else {
            return Ok(());
        };
        if journal.phase == Phase::RolledBack {
            return crate::upgrade_transport::end_maintenance(store, &journal.record).await;
        }
        if journal.phase == Phase::Committed {
            return Err(error(
                "upgrade_committed",
                "A completed upgrade cannot restore an older live-data snapshot",
            ));
        }
        let from = journal.rollback_from.unwrap_or(journal.phase);
        journal.rollback_from = Some(from);
        journal.phase = Phase::Restoring;
        save(store, &journal)?;
        let may_have_replaced = matches!(
            from,
            Phase::SnapshotReady | Phase::Verifying | Phase::RecoveryRequired
        ) || journal
            .programs
            .iter()
            .any(|backup| digest(&backup.target).ok().as_deref() != Some(&backup.sha256));
        if may_have_replaced {
            for component in [Component::Tunnel, Component::Runner, Component::Server] {
                for spec in journal
                    .services
                    .iter()
                    .filter(|spec| spec.component == component)
                {
                    crate::privilege::service_operation_spec(
                        store,
                        &journal.record,
                        spec.clone(),
                        ServiceOperation::Stop,
                        None,
                    )
                    .await?;
                }
            }
            verify_stopped(store, &journal)?;
            crate::privilege::restore_upgrade_programs(
                store,
                &journal.record,
                &journal.operation_id,
            )
            .await?;
            if journal.replacement_started {
                if let (Some(data), Some(snapshot), Some(expected)) = (
                    &journal.data,
                    &journal.data_snapshot,
                    &journal.snapshot_digest,
                ) {
                    if tree_digest(snapshot)? != *expected {
                        return Err(error(
                            "upgrade_snapshot_changed",
                            "The retained Server recovery point failed its integrity check",
                        ));
                    }
                    let displaced =
                        data.with_extension(format!("failed-upgrade-{}", journal.operation_id));
                    if data.exists() && tree_digest(data)? == *expected {
                        // Either the replacement never wrote data, or a previous
                        // recovery already completed the atomic restore.
                    } else {
                        let restored =
                            data.with_extension(format!("restore-{}", journal.operation_id));
                        if restored.exists() && tree_digest(&restored)? != *expected {
                            return Err(error(
                                "upgrade_restore_changed",
                                "An existing recovery directory is incomplete or changed",
                            ));
                        }
                        if !restored.exists() {
                            let partial = data.with_extension(format!(
                                "restore-{}.partial",
                                journal.operation_id
                            ));
                            if partial.exists() {
                                std::fs::remove_dir_all(&partial)
                                    .map_err(|_| SetupDiagnostic::io())?;
                            }
                            let mut remaining = SNAPSHOT_LIMIT;
                            copy_tree(snapshot, &partial, &mut remaining, &mut 100_000usize)?;
                            if tree_digest(&partial)? != *expected {
                                return Err(error(
                                    "upgrade_snapshot_changed",
                                    "The restored data did not match the recovery point",
                                ));
                            }
                            std::fs::rename(&partial, &restored)
                                .map_err(|_| SetupDiagnostic::io())?;
                        }
                        if data.exists() {
                            if displaced.exists() {
                                return Err(error("upgrade_restore_conflict", "Both current and displaced data exist; recovery requires inspection"));
                            }
                            std::fs::rename(data, &displaced).map_err(|_| SetupDiagnostic::io())?;
                        }
                        std::fs::rename(&restored, data).map_err(|_| SetupDiagnostic::io())?;
                        #[cfg(unix)]
                        if let Some(parent) = data.parent() {
                            std::fs::File::open(parent)
                                .and_then(|file| file.sync_all())
                                .map_err(|_| SetupDiagnostic::io())?;
                        }
                    }
                }
            }
        }
        for component in [Component::Server, Component::Runner, Component::Tunnel] {
            for spec in journal
                .services
                .iter()
                .filter(|spec| spec.component == component)
            {
                crate::privilege::service_operation_spec(
                    store,
                    &journal.record,
                    spec.clone(),
                    ServiceOperation::Start,
                    None,
                )
                .await?;
            }
        }
        self.verify_original_running(store, &journal).await?;
        persist_final_decision(store, &mut journal, Phase::RolledBack)?;
        crate::upgrade_transport::end_maintenance(store, &journal.record).await
    }

    async fn verify_original_running(
        &self,
        store: &EnvironmentStore,
        journal: &UpgradeJournal,
    ) -> SetupResultValue<()> {
        let deadline = tokio::time::Instant::now() + self.readiness_timeout;
        loop {
            match self.verify_original_running_once(store, journal).await {
                Ok(()) => break,
                Err(error) if tokio::time::Instant::now() < deadline => {
                    let _ = error;
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                }
                Err(error) => return Err(error),
            }
        }
        for spec in journal
            .services
            .iter()
            .filter(|spec| spec.component == Component::Tunnel)
        {
            crate::tunnel::wait_tunnel_readiness(spec).await?;
        }
        Ok(())
    }

    async fn verify_original_running_once(
        &self,
        store: &EnvironmentStore,
        journal: &UpgradeJournal,
    ) -> SetupResultValue<()> {
        for spec in &journal.services {
            let status = ServiceManager::inspect(spec).map_err(crate::native::service_error)?;
            if status.ownership != Ownership::Owned || status.running != Some(true) {
                return Err(error(
                    "upgrade_runtime_not_ready",
                    "An originally running service did not resume under its original owner",
                ));
            }
        }
        let check_server = journal
            .services
            .iter()
            .any(|spec| spec.component == Component::Server);
        let check_runner = journal
            .services
            .iter()
            .any(|spec| spec.component == Component::Runner);
        if check_server || check_runner {
            let token = if journal.record.request.local_server() {
                crate::native::bootstrap_token(store)?
            } else {
                read_secret(&store.root().join("webcodex-user-token"))?
            };
            self.post(
                &journal.record.request.server_url,
                "/api/runtime/status",
                Some(token.expose()),
                json!({}),
            )
            .await?;
            if check_runner {
                let client_id = journal.record.runner_client_id.as_deref().ok_or_else(|| {
                    error(
                        "upgrade_runtime_not_ready",
                        "The original Runner identity is missing",
                    )
                })?;
                let user_token = read_secret(&store.root().join("webcodex-user-token"))?;
                let runner = self
                    .post(
                        &journal.record.request.server_url,
                        "/api/runtime-console/runner",
                        Some(user_token.expose()),
                        json!({"client_id":client_id,"project_limit":200}),
                    )
                    .await?;
                if runner.get("connected").and_then(Value::as_bool) != Some(true) {
                    return Err(error(
                        "upgrade_runtime_not_ready",
                        "The originally running Runner is not connected",
                    ));
                }
            }
        }
        Ok(())
    }
}
fn save(store: &EnvironmentStore, journal: &UpgradeJournal) -> SetupResultValue<()> {
    store.write_json("upgrade.json", journal)
}
fn persist_final_decision(
    store: &EnvironmentStore,
    journal: &mut UpgradeJournal,
    decision: Phase,
) -> SetupResultValue<()> {
    if !matches!(
        (journal.phase, decision),
        (Phase::Verifying, Phase::Committed) | (Phase::Restoring, Phase::RolledBack)
    ) {
        return Err(error(
            "upgrade_phase",
            "An invalid upgrade final decision was requested",
        ));
    }
    journal.phase = decision;
    save(store, journal)
}
fn configured_specs(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
) -> SetupResultValue<Vec<ServiceSpec>> {
    let mut out = Vec::new();
    if record.request.local_server() {
        out.push(service_spec(store, record, Component::Server)?);
    }
    if record.request.local_runner() {
        out.push(service_spec(store, record, Component::Runner)?);
    }
    for profile in tunnel_profiles(store)?
        .into_iter()
        .filter(|profile| profile.installed)
    {
        out.push(tunnel_service_spec(store, record, &profile.profile_id)?);
    }
    Ok(out)
}
fn verify_stopped(_store: &EnvironmentStore, journal: &UpgradeJournal) -> SetupResultValue<()> {
    verify_service_inventory(journal)?;
    for spec in &journal.all_services {
        let status = ServiceManager::inspect(spec).map_err(crate::native::service_error)?;
        if status.ownership != Ownership::Owned || status.running != Some(false) {
            return Err(error("upgrade_owner_unknown", "A previous runtime may still be running; no data snapshot or second process is allowed"));
        }
    }
    Ok(())
}
fn copy_bounded(source: &Path, target: &Path, remaining: &mut u64) -> SetupResultValue<()> {
    let meta = std::fs::symlink_metadata(source).map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_file() || meta.is_symlink() || meta.len() > *remaining {
        return Err(error(
            "upgrade_snapshot",
            "Snapshot contains an unsupported file or exceeds its bound",
        ));
    }
    let mut source_options = std::fs::OpenOptions::new();
    source_options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        source_options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        source_options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let input = source_options
        .open(source)
        .map_err(|_| SetupDiagnostic::io())?;
    let opened = input.metadata().map_err(|_| SetupDiagnostic::io())?;
    #[cfg(windows)]
    if std::os::windows::fs::MetadataExt::file_attributes(&opened) & 0x400 != 0 {
        return Err(error(
            "upgrade_snapshot",
            "Snapshot source became a reparse point",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if opened.dev() != meta.dev() || opened.ino() != meta.ino() {
            return Err(error(
                "upgrade_snapshot",
                "Snapshot source changed while opening it",
            ));
        }
    }
    if !opened.is_file() || opened.len() > *remaining {
        return Err(error(
            "upgrade_snapshot",
            "Snapshot source grew beyond its bound",
        ));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut output = options.open(target).map_err(|_| SetupDiagnostic::io())?;
    let copied = std::io::copy(&mut input.take(*remaining + 1), &mut output)
        .map_err(|_| SetupDiagnostic::io())?;
    if copied > *remaining {
        return Err(error(
            "upgrade_snapshot",
            "Snapshot source grew beyond its bound",
        ));
    }
    *remaining -= copied;
    output.sync_all().map_err(|_| SetupDiagnostic::io())?;
    Ok(())
}
fn copy_tree(
    source: &Path,
    target: &Path,
    remaining: &mut u64,
    entries: &mut usize,
) -> SetupResultValue<()> {
    let meta = std::fs::symlink_metadata(source).map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_dir() || meta.is_symlink() {
        return Err(error(
            "upgrade_snapshot",
            "Server data must be a real directory",
        ));
    }
    ensure_private_directory(target)?;
    for entry in std::fs::read_dir(source).map_err(|_| SetupDiagnostic::io())? {
        if *entries == 0 {
            return Err(error(
                "upgrade_snapshot",
                "Server snapshot has too many entries",
            ));
        }
        *entries -= 1;
        let entry = entry.map_err(|_| SetupDiagnostic::io())?;
        let meta = entry.file_type().map_err(|_| SetupDiagnostic::io())?;
        let destination = target.join(entry.file_name());
        if meta.is_dir() {
            copy_tree(&entry.path(), &destination, remaining, entries)?;
        } else {
            copy_bounded(&entry.path(), &destination, remaining)?;
        }
    }
    #[cfg(unix)]
    std::fs::File::open(target)
        .and_then(|file| file.sync_all())
        .map_err(|_| SetupDiagnostic::io())?;
    Ok(())
}
pub(crate) fn restore_programs_from_journal(
    root: &Path,
    operation_id: &str,
    requester_identity: &str,
) -> SetupResultValue<()> {
    if !root.is_absolute() || uuid::Uuid::parse_str(operation_id).is_err() {
        return Err(error(
            "upgrade_restore_authorization",
            "The restore operation identity is invalid",
        ));
    }
    let path = root.join("upgrade.json");
    let journal: UpgradeJournal = serde_json::from_slice(&bounded_file(&path)?).map_err(|_| {
        error(
            "upgrade_restore_authorization",
            "The upgrade journal is invalid",
        )
    })?;
    if journal.phase != Phase::Restoring
        || journal.operation_id != operation_id
        || journal.record.request.account.identity != requester_identity
        || requester_identity.is_empty()
    {
        return Err(error(
            "upgrade_restore_authorization",
            "The upgrade journal does not authorize program restoration",
        ));
    }
    checked_owner(root, &journal.record.request.account.identity)?;
    checked_owner(&path, &journal.record.request.account.identity)?;
    // Restore the launcher last so a self-replacement failure cannot prevent
    // Server and Runner restoration on platforms that lock running images.
    let expected = [
        (&journal.record.request.binaries.server, "webcodex-server"),
        (&journal.record.request.binaries.runner, "webcodex-runner"),
        (&journal.record.request.binaries.cli, "webcodex"),
    ];
    if journal.programs.len() != expected.len() {
        return Err(error(
            "upgrade_restore_authorization",
            "The saved program list is incomplete",
        ));
    }
    verify_service_inventory(&journal)?;
    for spec in &journal.all_services {
        let status = ServiceManager::inspect(spec).map_err(crate::native::service_error)?;
        if status.ownership != Ownership::Owned || status.running != Some(false) {
            return Err(error(
                "upgrade_restore_authorization",
                "An original service is still running",
            ));
        }
    }
    let backup_dir = root.join("upgrade-backups").join(operation_id);
    checked_owner(&backup_dir, &journal.record.request.account.identity)?;
    let needs_write = journal
        .programs
        .iter()
        .any(|item| digest(&item.target).ok().as_deref() != Some(&item.sha256))
        || journal.desktop.as_ref().is_some_and(|item| {
            desktop_installed_hash(&item.target).ok().as_deref() != Some(&item.sha256)
                || desktop_directory_modes_hash(&item.target).ok().as_deref()
                    != Some(&item.directory_modes_sha256)
        });
    if !needs_write {
        return Ok(());
    }
    authorize_restore_destinations(root, &journal)?;
    for (target, name) in expected {
        let item = journal
            .programs
            .iter()
            .find(|item| item.name == name)
            .ok_or_else(|| {
                error(
                    "upgrade_restore_authorization",
                    "A saved program is missing",
                )
            })?;
        if item.target != *target
            || item.backup != backup_dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        {
            return Err(error(
                "upgrade_restore_authorization",
                "A restore path differs from the installed runtime",
            ));
        }
        for ancestor in item.target.ancestors() {
            let metadata =
                std::fs::symlink_metadata(ancestor).map_err(|_| SetupDiagnostic::io())?;
            if metadata.is_symlink() {
                return Err(error(
                    "upgrade_restore_authorization",
                    "A restore target contains a link",
                ));
            }
        }
        checked_owner(&item.backup, &journal.record.request.account.identity)?;
        if digest(&item.backup)? != item.sha256 {
            return Err(error(
                "upgrade_backup_changed",
                "The retained program backup was modified",
            ));
        }
        if digest(&item.target).ok().as_deref() != Some(&item.sha256) {
            restore_program(item)?;
        }
    }
    if let Some(desktop) = &journal.desktop {
        if managed_desktop_target(&journal.record.request.binaries).as_ref()
            != Some(&desktop.target)
            || desktop.backup
                != backup_dir.join(if cfg!(target_os = "macos") {
                    "desktop.app"
                } else {
                    "desktop"
                })
        {
            return Err(error(
                "upgrade_restore_authorization",
                "The Desktop recovery target changed",
            ));
        }
        if desktop_installed_hash(&desktop.backup)? != desktop.sha256
            || desktop_directory_modes_hash(&desktop.backup)? != desktop.directory_modes_sha256
        {
            return Err(error(
                "upgrade_backup_changed",
                "The retained Desktop backup changed",
            ));
        }
        restore_desktop(desktop, operation_id, requester_identity)?;
    }
    Ok(())
}

fn authorize_restore_destinations(root: &Path, journal: &UpgradeJournal) -> SetupResultValue<()> {
    #[cfg(unix)]
    {
        let system = crate::installer_authorization::system_directory()?;
        if system.join("authorization.json").exists() {
            let store = EnvironmentStore::open(system)?;
            let (_, frozen) = crate::installer_authorization::load_authorized_upgrade(&store)?;
            if frozen.root != root
                || frozen.operation_id != journal.operation_id
                || frozen.owner != journal.record.request.account
                || check_frozen_installer_transition(&frozen)? != "restoring"
            {
                return Err(error(
                    "upgrade_restore_authorization",
                    "The protected installer authorization differs from this restore",
                ));
            }
            return Ok(());
        }
    }
    // Outside an authorized system package, a privileged helper may only
    // restore into directories and files already owned by this user.
    let mut targets: Vec<&Path> = journal
        .programs
        .iter()
        .map(|item| item.target.as_path())
        .collect();
    if let Some(desktop) = &journal.desktop {
        targets.push(&desktop.target);
    }
    for target in targets {
        let parent = target.parent().ok_or_else(SetupDiagnostic::io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            for path in [parent, target] {
                let metadata = std::fs::symlink_metadata(path).map_err(|_| {
                    error(
                        "upgrade_restore_authorization",
                        "An unowned restore destination is absent",
                    )
                })?;
                if metadata.is_symlink()
                    || metadata.uid().to_string() != journal.record.request.account.identity
                {
                    return Err(error(
                        "upgrade_restore_authorization",
                        "A restore destination is not owned by the original user",
                    ));
                }
            }
        }
        #[cfg(windows)]
        for path in [parent, target] {
            if crate::storage::windows_path_owner(path)? != journal.record.request.account.identity
            {
                return Err(error(
                    "upgrade_restore_authorization",
                    "A restore destination is not owned by the original user",
                ));
            }
        }
    }
    Ok(())
}

fn restore_desktop(
    backup: &DesktopBackup,
    operation_id: &str,
    owner: &str,
) -> SetupResultValue<()> {
    let parent = backup.target.parent().ok_or_else(SetupDiagnostic::io)?;
    for ancestor in parent.ancestors() {
        if std::fs::symlink_metadata(ancestor)
            .map_err(|_| SetupDiagnostic::io())?
            .is_symlink()
        {
            return Err(error(
                "upgrade_restore_authorization",
                "A Desktop restore parent is a link",
            ));
        }
    }
    if let Ok(metadata) = std::fs::symlink_metadata(&backup.target) {
        if metadata.is_symlink() {
            return Err(error(
                "upgrade_restore_authorization",
                "The Desktop target became a link",
            ));
        }
        #[cfg(windows)]
        if crate::storage::windows_path_owner(&backup.target)? != owner {
            return Err(error(
                "upgrade_restore_authorization",
                "The Desktop target belongs to another account",
            ));
        }
    }
    let _ = owner;
    if desktop_installed_hash(&backup.target).ok().as_deref() == Some(&backup.sha256)
        && desktop_directory_modes_hash(&backup.target).ok().as_deref()
            == Some(&backup.directory_modes_sha256)
    {
        return Ok(());
    }
    #[cfg(windows)]
    {
        return restore_program(&ProgramBackup {
            target: backup.target.clone(),
            backup: backup.backup.clone(),
            sha256: backup.sha256.clone(),
            name: "webcodex-desktop".into(),
        });
    }
    #[cfg(unix)]
    {
        let temporary = parent.join(format!(
            ".webcodex-desktop-restore-{}",
            uuid::Uuid::new_v4().simple()
        ));
        desktop_tree::copy(&backup.backup, &temporary)?;
        if desktop_installed_hash(&temporary)? != backup.sha256
            || desktop_directory_modes_hash(&temporary)? != backup.directory_modes_sha256
        {
            return Err(error(
                "upgrade_backup_changed",
                "The Desktop recovery copy changed",
            ));
        }
        let displaced = backup
            .target
            .with_extension(format!("failed-upgrade-{operation_id}"));
        if backup.target.exists() {
            if displaced.exists() {
                return Err(error(
                    "upgrade_restore_conflict",
                    "A displaced Desktop installation already exists",
                ));
            }
            std::fs::rename(&backup.target, &displaced).map_err(|_| SetupDiagnostic::io())?;
        }
        std::fs::rename(&temporary, &backup.target).map_err(|_| SetupDiagnostic::io())?;
        std::fs::File::open(parent)
            .and_then(|file| file.sync_all())
            .map_err(|_| SetupDiagnostic::io())?;
        Ok(())
    }
}

fn restore_program(backup: &ProgramBackup) -> SetupResultValue<()> {
    let temporary = backup
        .target
        .with_extension(format!("restore-{}", uuid::Uuid::new_v4().simple()));
    let mut remaining = SNAPSHOT_LIMIT;
    copy_bounded(&backup.backup, &temporary, &mut remaining)?;
    if digest(&temporary)? != backup.sha256 {
        return Err(error(
            "upgrade_backup_changed",
            "The copied program backup failed its integrity check",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o755))
            .map_err(|_| SetupDiagnostic::io())?;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        };
        let wide = |path: &Path| {
            path.as_os_str()
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<u16>>()
        };
        if unsafe {
            MoveFileExW(
                wide(&temporary).as_ptr(),
                wide(&backup.target).as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(error(
                "upgrade_restore_authorization",
                "Windows could not atomically replace the stopped runtime program",
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(&temporary, &backup.target).map_err(|_| SetupDiagnostic::io())
    }
}

fn native_executable_architecture(bytes: &[u8]) -> SetupResultValue<&'static str> {
    let mismatch = || {
        error(
            "candidate_architecture",
            "A candidate executable has the wrong native format or CPU architecture",
        )
    };
    let architecture = if bytes.starts_with(b"\x7fELF") && bytes.len() >= 20 {
        if !cfg!(target_os = "linux") {
            return Err(mismatch());
        }
        if bytes[5] != 1 {
            return Err(mismatch());
        }
        match u16::from_le_bytes([bytes[18], bytes[19]]) {
            62 => "x86_64",
            183 => "aarch64",
            _ => return Err(mismatch()),
        }
    } else if bytes.starts_with(b"MZ") && bytes.len() >= 64 {
        if !cfg!(windows) {
            return Err(mismatch());
        }
        let offset =
            u32::from_le_bytes(bytes[0x3c..0x40].try_into().map_err(|_| mismatch())?) as usize;
        if offset.checked_add(6).is_none_or(|end| end > bytes.len())
            || &bytes[offset..offset + 4] != b"PE\0\0"
        {
            return Err(mismatch());
        }
        match u16::from_le_bytes([bytes[offset + 4], bytes[offset + 5]]) {
            0x8664 => "x86_64",
            0xaa64 => "aarch64",
            _ => return Err(mismatch()),
        }
    } else if bytes.len() >= 8 && &bytes[..4] == b"\xcf\xfa\xed\xfe" {
        if !cfg!(target_os = "macos") {
            return Err(mismatch());
        }
        match u32::from_le_bytes(bytes[4..8].try_into().map_err(|_| mismatch())?) {
            0x01000007 => "x86_64",
            0x0100000c => "aarch64",
            _ => return Err(mismatch()),
        }
    } else {
        return Err(mismatch());
    };
    Ok(architecture)
}

async fn verify_candidate_executables(candidate: &UpgradeCandidate) -> SetupResultValue<()> {
    verify_candidate_executable_headers(candidate)?;
    for name in COMPONENTS {
        let artifact = candidate
            .artifacts
            .get(name)
            .ok_or_else(|| error("candidate_components", "A runtime executable is missing"))?;
        // Provenance was established before executing any candidate byte.
        // Development mode is a separate explicit opt-in and remains unproven.
        let probe = async {
            use tokio::io::AsyncReadExt;
            let mut child = tokio::process::Command::new(&artifact.path)
                .arg("--build-info-json")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|_| {
                    error(
                        "candidate_probe",
                        "Candidate build metadata probe could not execute",
                    )
                })?;
            let stdout = child.stdout.take().ok_or_else(SetupDiagnostic::io)?;
            let mut bytes = Vec::new();
            stdout
                .take(16385)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| error("candidate_probe", "Candidate build metadata probe failed"))?;
            if bytes.len() > 16384 {
                let _ = child.kill().await;
                return Err(error(
                    "candidate_probe",
                    "Candidate build metadata probe exceeded its output bound",
                ));
            }
            let status = child
                .wait()
                .await
                .map_err(|_| error("candidate_probe", "Candidate build metadata probe failed"))?;
            Ok((status, bytes))
        };
        let (status, output) = tokio::time::timeout(std::time::Duration::from_secs(5), probe)
            .await
            .map_err(|_| {
                error(
                    "candidate_probe",
                    "Candidate build metadata probe timed out",
                )
            })??;
        if !status.success() {
            return Err(error(
                "candidate_probe",
                "Candidate build metadata probe failed",
            ));
        }
        let actual: MachineBuildInfo = serde_json::from_slice(&output).map_err(|_| {
            error(
                "candidate_probe",
                "Candidate build metadata probe was invalid",
            )
        })?;
        if actual != artifact.build_info {
            return Err(error(
                "candidate_identity",
                "Runtime-reported build identity differs from the published manifest",
            ));
        }
        if digest(&artifact.path)? != artifact.sha256 {
            return Err(error(
                "candidate_checksum",
                "A candidate executable changed during validation",
            ));
        }
    }
    Ok(())
}

fn verify_candidate_executable_headers(candidate: &UpgradeCandidate) -> SetupResultValue<()> {
    for name in COMPONENTS {
        let artifact = candidate
            .artifacts
            .get(name)
            .ok_or_else(|| error("candidate_components", "A runtime executable is missing"))?;
        let mut file = std::fs::File::open(&artifact.path).map_err(|_| SetupDiagnostic::io())?;
        let mut header = [0u8; 4096];
        let count = file.read(&mut header).map_err(|_| SetupDiagnostic::io())?;
        if native_executable_architecture(&header[..count])? != std::env::consts::ARCH {
            return Err(error(
                "candidate_architecture",
                "A runtime executable does not match this CPU architecture",
            ));
        }
    }
    Ok(())
}

fn tree_digest(root: &Path) -> SetupResultValue<String> {
    fn visit(
        root: &Path,
        hash: &mut Sha256,
        entries: &mut usize,
        remaining: &mut u64,
    ) -> SetupResultValue<()> {
        let metadata = std::fs::symlink_metadata(root).map_err(|_| SetupDiagnostic::io())?;
        if !metadata.is_dir() || metadata.is_symlink() {
            return Err(error(
                "upgrade_snapshot",
                "Recovery data contains an unsafe directory",
            ));
        }
        let mut items: Vec<_> = std::fs::read_dir(root)
            .map_err(|_| SetupDiagnostic::io())?
            .collect::<Result<_, _>>()
            .map_err(|_| SetupDiagnostic::io())?;
        items.sort_by_key(|entry| entry.file_name());
        for entry in items {
            if *entries == 0 {
                return Err(error(
                    "upgrade_snapshot",
                    "Recovery data exceeds its entry bound",
                ));
            }
            *entries -= 1;
            let name = entry
                .file_name()
                .to_str()
                .ok_or_else(SetupDiagnostic::io)?
                .to_owned();
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            let meta =
                std::fs::symlink_metadata(entry.path()).map_err(|_| SetupDiagnostic::io())?;
            if meta.is_symlink() {
                return Err(error("upgrade_snapshot", "Recovery data contains a link"));
            }
            if meta.is_dir() {
                hash.update([0]);
                visit(&entry.path(), hash, entries, remaining)?;
                hash.update([2]);
            } else if meta.is_file() && meta.len() <= *remaining {
                *remaining -= meta.len();
                hash.update([1]);
                hash.update(digest(&entry.path())?.as_bytes());
            } else {
                return Err(error(
                    "upgrade_snapshot",
                    "Recovery data contains an unsupported or oversized file",
                ));
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    visit(
        root,
        &mut hash,
        &mut 100_000usize,
        &mut SNAPSHOT_LIMIT.clone(),
    )?;
    Ok(format!("{:x}", hash.finalize()))
}

async fn verify_published_provenance(candidate: &UpgradeCandidate) -> SetupResultValue<()> {
    // The authoritative copy comes from the approved publisher over TLS, not
    // from the candidate's self-asserted manifest/checksum pair.
    let repo = if candidate
        .source_workflow_ref
        .starts_with("yyjeqhc/webcodex/")
    {
        "yyjeqhc/webcodex"
    } else {
        "Xiiiing/webcodex"
    };
    let mut url = url::Url::parse(&format!("https://github.com/{repo}/releases/download/"))
        .map_err(|_| SetupDiagnostic::io())?;
    url.path_segments_mut()
        .map_err(|_| SetupDiagnostic::io())?
        .pop_if_empty()
        .push(&format!("v{}", candidate.version))
        .push(&format!(
            "webcodex-source-v{}-{}.json",
            candidate.version, candidate.platform
        ));
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 4
                && attempt.url().scheme() == "https"
                && matches!(
                    attempt.url().host_str(),
                    Some(
                        "github.com"
                            | "release-assets.githubusercontent.com"
                            | "objects.githubusercontent.com"
                    )
                )
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| SetupDiagnostic::io())?;
    let mut response = client.get(url).send().await.map_err(|_| {
        error(
            "candidate_provenance_unverified",
            "The published source manifest could not be verified over HTTPS",
        )
    })?;
    if !response.status().is_success() {
        return Err(error(
            "candidate_provenance_unverified",
            "This candidate has no matching published source manifest",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        error(
            "candidate_provenance_unverified",
            "The published source manifest response was interrupted",
        )
    })? {
        if bytes.len().saturating_add(chunk.len()) > 1024 * 1024 {
            return Err(error(
                "candidate_provenance_unverified",
                "Published provenance exceeded its bound",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    if hex(&bytes) != candidate.manifest_sha256 {
        return Err(error(
            "candidate_provenance_mismatch",
            "Candidate source metadata differs from the publisher's release",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(root: &Path, phase: Phase) -> UpgradeJournal {
        let account = LocalAccount {
            name: "owner".into(),
            identity: "1000".into(),
            home: root.into(),
        };
        let binaries = RuntimeBinaries {
            cli: root.join("webcodex"),
            server: root.join("webcodex-server"),
            runner: root.join("webcodex-runner"),
        };
        UpgradeJournal {
            schema_version: 1,
            operation_id: uuid::Uuid::new_v4().to_string(),
            candidate: UpgradeCandidate {
                version: "1.0.0".into(),
                source_sha: "a".repeat(40),
                platform: "linux-x64".into(),
                source_workflow_run_id: 1,
                source_workflow_ref: "yyjeqhc/webcodex/.github/workflows/release-build.yml@main"
                    .into(),
                manifest_sha256: "b".repeat(64),
                provenance_verified: true,
                root: root.into(),
                artifacts: BTreeMap::new(),
                desktop: None,
            },
            phase,
            record: EnvironmentRecord {
                schema_version: 1,
                environment_id: "env-fixture".into(),
                request: SetupRequest {
                    mode: EnvironmentMode::Join,
                    server_url: "http://127.0.0.1:1".into(),
                    project: None,
                    account,
                    binaries,
                },
                username: None,
                runner_client_id: None,
                projects: vec![],
                configured: true,
            },
            services: vec![],
            all_services: vec![],
            inventory_complete: true,
            programs: vec![],
            desktop: None,
            data: None,
            data_snapshot: None,
            rollback_from: None,
            replacement_started: false,
            snapshot_digest: None,
        }
    }

    #[test]
    fn final_decision_is_durable_before_gate_can_be_released() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let store = EnvironmentStore::open(directory.path().to_path_buf()).unwrap();
        let mut journal = fixture(store.root(), Phase::Verifying);
        persist_final_decision(&store, &mut journal, Phase::Committed).unwrap();
        let recovered: UpgradeJournal = store.read_json("upgrade.json").unwrap().unwrap();
        assert_eq!(recovered.phase, Phase::Committed);
        assert_eq!(recovered.operation_id, journal.operation_id);
        assert!(persist_final_decision(&store, &mut journal, Phase::RolledBack).is_err());
        let mut rollback = fixture(store.root(), Phase::Restoring);
        persist_final_decision(&store, &mut rollback, Phase::RolledBack).unwrap();
        let recovered: UpgradeJournal = store.read_json("upgrade.json").unwrap().unwrap();
        assert_eq!(recovered.phase, Phase::RolledBack);
        assert!(persist_final_decision(&store, &mut rollback, Phase::Committed).is_err());
    }

    #[tokio::test]
    async fn recovered_final_decisions_never_restore_old_data() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let store = EnvironmentStore::open(directory.path().to_path_buf()).unwrap();
        let mut journal = fixture(store.root(), Phase::Verifying);
        persist_final_decision(&store, &mut journal, Phase::Committed).unwrap();
        let backend = NativeEnvironment::new().unwrap();
        assert_eq!(
            backend.upgrade_rollback(&store).await.unwrap_err().code,
            "upgrade_committed"
        );
        let mut rollback = fixture(store.root(), Phase::Restoring);
        persist_final_decision(&store, &mut rollback, Phase::RolledBack).unwrap();
        backend.upgrade_rollback(&store).await.unwrap();
    }

    #[test]
    fn privileged_restore_rejects_wrong_operation_and_request_owner_before_effects() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let store = EnvironmentStore::open(directory.path().to_path_buf()).unwrap();
        let journal = fixture(store.root(), Phase::Restoring);
        save(&store, &journal).unwrap();
        assert_eq!(
            restore_programs_from_journal(store.root(), &journal.operation_id, "other")
                .unwrap_err()
                .code,
            "upgrade_restore_authorization"
        );
        assert_eq!(
            restore_programs_from_journal(
                store.root(),
                &uuid::Uuid::new_v4().to_string(),
                &journal.record.request.account.identity
            )
            .unwrap_err()
            .code,
            "upgrade_restore_authorization"
        );
    }

    #[test]
    fn receipt_binds_original_owner_environment_and_program_targets() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let mut journal = fixture(root, Phase::SnapshotReady);
        journal.programs.push(ProgramBackup {
            target: root.join("webcodex"),
            backup: root.join("backup-webcodex"),
            sha256: "c".repeat(64),
            name: "webcodex".into(),
        });
        let receipt = prepared_receipt(root, &journal);
        assert_eq!(receipt.environment_id, journal.record.environment_id);
        assert_eq!(
            receipt.owner_identity,
            journal.record.request.account.identity
        );
        assert_eq!(
            receipt.targets.get("webcodex"),
            Some(&root.join("webcodex"))
        );
        journal.operation_id = uuid::Uuid::new_v4().to_string();
        assert_ne!(receipt, prepared_receipt(root, &journal));
    }

    #[test]
    fn unfinished_upgrade_blocks_other_store_mutations_after_recovery() {
        let directory = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))
                .unwrap();
        }
        let store = EnvironmentStore::open(directory.path().to_path_buf()).unwrap();
        let _lock = store.lock().unwrap();
        for phase in [
            Phase::Prepared,
            Phase::Stopping,
            Phase::Stopped,
            Phase::SnapshotReady,
            Phase::Verifying,
            Phase::Restoring,
            Phase::RecoveryRequired,
        ] {
            save(&store, &fixture(store.root(), phase)).unwrap();
            assert_eq!(
                ensure_upgrade_idle_under_lock(&store).unwrap_err().code,
                "upgrade_pending"
            );
        }
        for phase in [Phase::Committed, Phase::RolledBack] {
            save(&store, &fixture(store.root(), phase)).unwrap();
            ensure_upgrade_idle_under_lock(&store).unwrap();
        }
    }

    #[test]
    fn executable_header_rejects_wrong_architecture_and_format() {
        let mut elf = [0u8; 64];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[5] = 1;
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        if cfg!(target_os = "linux") {
            assert_eq!(native_executable_architecture(&elf).unwrap(), "x86_64");
        } else {
            assert!(native_executable_architecture(&elf).is_err());
        }
        elf[18..20].copy_from_slice(&183u16.to_le_bytes());
        if cfg!(target_os = "linux") {
            assert_eq!(native_executable_architecture(&elf).unwrap(), "aarch64");
        }
        elf[18..20].copy_from_slice(&3u16.to_le_bytes());
        assert!(native_executable_architecture(&elf).is_err());
        assert!(native_executable_architecture(b"MZ").is_err());
        assert!(native_executable_architecture(b"not an executable").is_err());
    }

    #[test]
    fn malformed_candidate_metadata_fails_before_artifact_execution() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("source-manifest.json"),
            br#"{"schema_version":1,"version":"1.0.0","source_sha":"invalid"}"#,
        )
        .unwrap();
        assert_eq!(
            verify_upgrade_candidate(directory.path()).unwrap_err().code,
            "candidate_source"
        );
    }

    #[cfg(unix)]
    #[test]
    fn desktop_restore_replaces_only_the_recorded_payload() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("Desktop.app");
        let backup = directory.path().join("desktop-backup.app");
        std::fs::create_dir(&target).unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(target.join("binary"), b"original").unwrap();
        std::fs::set_permissions(
            target.join("binary"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        desktop_tree::copy(&target, &backup).unwrap();
        let sha256 = desktop_installed_hash(&backup).unwrap();
        std::fs::write(target.join("binary"), b"replacement").unwrap();
        let unrelated = directory.path().join("user-data");
        std::fs::write(&unrelated, b"unchanged").unwrap();
        let directory_modes_sha256 = desktop_directory_modes_hash(&backup).unwrap();
        restore_desktop(
            &DesktopBackup {
                target: target.clone(),
                backup,
                sha256: sha256.clone(),
                directory_modes_sha256,
            },
            "fixture",
            "1000",
        )
        .unwrap();
        assert_eq!(desktop_installed_hash(&target).unwrap(), sha256);
        assert_eq!(std::fs::read(unrelated).unwrap(), b"unchanged");
        assert_eq!(
            std::fs::read(
                target
                    .with_extension("failed-upgrade-fixture")
                    .join("binary")
            )
            .unwrap(),
            b"replacement"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_desktop_file_hash_is_stable_across_backup_and_restore_names() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("webcodex-desktop");
        let backup = directory.path().join("desktop");
        std::fs::write(&target, b"old executable").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        desktop_tree::copy(&target, &backup).unwrap();
        let sha256 = desktop_installed_hash(&target).unwrap();
        assert_eq!(
            sha256,
            "073f8ba13f215f732747b6715da58a8d838a186f6a85d0d964a044bdd972f92e"
        );
        assert_eq!(desktop_installed_hash(&backup).unwrap(), sha256);
        std::fs::write(&target, b"new executable").unwrap();
        restore_desktop(
            &DesktopBackup {
                target: target.clone(),
                backup,
                sha256: sha256.clone(),
                directory_modes_sha256: desktop_directory_modes_hash(&target).unwrap(),
            },
            "file-fixture",
            "1000",
        )
        .unwrap();
        assert_eq!(desktop_installed_hash(&target).unwrap(), sha256);
        assert_eq!(std::fs::read(&target).unwrap(), b"old executable");
    }
}
