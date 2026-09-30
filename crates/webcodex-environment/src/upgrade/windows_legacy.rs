//! A Windows package transaction has no Environment identity or service authority.
//! In particular v0.4.3 cannot execute Environment commands. The manifest-bound
//! candidate coordinates this transaction as the installing user, never as admin.
use super::*;

const JOURNAL: &str = "windows-package-upgrade.json";
const RECEIPT: &str = "windows-package-prepared.json";
const OFFICIAL_V043: &str = "b96a59a712ca5355ad8609cd861cd0c7acb9f99e";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallationKind {
    Fresh,
    Legacy,
    Unconfigured,
    Environment,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageJournal {
    schema_version: u16,
    operation_id: String,
    owner_identity: String,
    runtime_dir: PathBuf,
    kind: InstallationKind,
    candidate: UpgradeCandidate,
    phase: Phase,
    programs: Vec<ProgramBackup>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PackageReceipt {
    schema_version: u16,
    operation_id: String,
    owner_identity: String,
    runtime_dir: PathBuf,
    manifest_sha256: String,
    targets: BTreeMap<String, PathBuf>,
}

fn require_windows() -> SetupResultValue<()> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(error(
            "installer_platform",
            "This package transaction is only available on Windows",
        ))
    }
}

fn targets(runtime: &Path) -> SetupResultValue<BTreeMap<String, PathBuf>> {
    if !runtime.is_absolute()
        || runtime.file_name().and_then(|s| s.to_str()) != Some("webcodex-runtime")
    {
        return Err(error(
            "installer_targets",
            "The Windows package runtime target is invalid",
        ));
    }
    let mut targets = BTreeMap::new();
    for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
        targets.insert(name.into(), runtime.join(format!("{name}.exe")));
    }
    targets.insert(
        "webcodex-desktop".into(),
        runtime
            .parent()
            .ok_or_else(SetupDiagnostic::io)?
            .join("WebCodex.exe"),
    );
    Ok(targets)
}

fn owner_path(path: &Path, owner: &str) -> SetupResultValue<()> {
    verify_installed_target_path(path)?;
    #[cfg(windows)]
    if crate::storage::windows_path_owner(path)? != owner {
        return Err(error(
            "installer_owner",
            "The installed program belongs to another Windows account",
        ));
    }
    #[cfg(not(windows))]
    let _ = owner;
    Ok(())
}

fn no_environment(store: &EnvironmentStore) -> SetupResultValue<()> {
    if store.load_environment()?.is_some() || store.load_journal()?.is_some() {
        return Err(error(
            "installer_environment",
            "Use the original Environment owner's upgrade transaction",
        ));
    }
    super::ensure_upgrade_idle_environment(store)
}

// An exclusive writable handle rejects mapped/running executable images and
// inaccessible files. No process is killed and no service ownership is inferred.
fn stopped(path: &Path) -> SetupResultValue<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(path)
            .map_err(|_| error("legacy_program_busy", "Quit the old Desktop and stop its Server/Runner before upgrading; a program is running or cannot be replaced"))?;
    }
    #[cfg(not(windows))]
    let _ = path;
    Ok(())
}

async fn probe_installed(path: &Path, name: &str) -> SetupResultValue<MachineBuildInfo> {
    use tokio::io::AsyncReadExt;
    let probe = async {
        let mut child = tokio::process::Command::new(path)
            .arg("--build-info-json")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| SetupDiagnostic::io())?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .ok_or_else(SetupDiagnostic::io)?
            .take(16385)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| SetupDiagnostic::io())?;
        if bytes.len() > 16384 {
            let _ = child.kill().await;
            return Err(SetupDiagnostic::io());
        }
        if !child
            .wait()
            .await
            .map_err(|_| SetupDiagnostic::io())?
            .success()
        {
            return Err(SetupDiagnostic::io());
        }
        let info: MachineBuildInfo =
            serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
        info.validate(name).map_err(|_| SetupDiagnostic::io())?;
        Ok(info)
    };
    tokio::time::timeout(std::time::Duration::from_secs(5), probe)
        .await
        .map_err(|_| {
            error(
                "legacy_identity",
                "The installed build identity probe timed out",
            )
        })?
        .map_err(|_| {
            error(
                "legacy_identity",
                "The installed build identity could not be verified",
            )
        })
}

fn pre_environment(info: &MachineBuildInfo) -> bool {
    info.version == "0.4.3"
        && info.git_commit.as_deref() == Some(OFFICIAL_V043)
        && info.git_dirty == Some(false)
        && info.environment_data_format.is_none()
        && info.target == "x86_64-pc-windows-msvc"
        && info.architecture == "x86_64"
}

async fn classify_inner(
    store: &EnvironmentStore,
    runtime: &Path,
) -> SetupResultValue<InstallationKind> {
    let files = targets(runtime)?;
    let owner = current_account()?.identity;
    checked_owner(store.root(), &owner)?;
    if let Some(record) = store.load_environment()? {
        checked_owner(&store.root().join("environment.json"), &owner)?;
        if record.request.account.identity != owner
            || record.request.binaries.cli != files["webcodex"]
            || record.request.binaries.server != files["webcodex-server"]
            || record.request.binaries.runner != files["webcodex-runner"]
        {
            return Err(error(
                "installer_owner",
                "The installed Environment owner or runtime target differs",
            ));
        }
        for path in files.values() {
            owner_path(path, &owner)?;
        }
        return Ok(InstallationKind::Environment);
    }
    no_environment(store)?;
    if let Some(journal) = store.read_json::<PackageJournal>(JOURNAL)? {
        if !matches!(journal.phase, Phase::Committed | Phase::RolledBack) {
            // Recovery identity comes from the original sealed transaction,
            // including when a partial installer left a program missing.
            return Ok(load_bound(store, runtime)?.kind);
        }
    }
    let present = files
        .values()
        .filter(|p| p.try_exists().unwrap_or(true))
        .count();
    if present == 0 {
        // Check existing ancestors even for a new destination.
        let ancestor = runtime
            .ancestors()
            .find(|p| p.exists())
            .ok_or_else(SetupDiagnostic::io)?;
        verify_installed_target_path(ancestor)?;
        return Ok(InstallationKind::Fresh);
    }
    if present != files.len() {
        return Err(error(
            "legacy_installation_incomplete",
            "The old Windows package is incomplete; repair it before upgrading",
        ));
    }
    for path in files.values() {
        owner_path(path, &owner)?;
    }
    let mut kind = None;
    let mut identity = None;
    for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
        let before = digest(&files[name])?;
        let info = probe_installed(&files[name], name).await?;
        let build = (
            info.version.clone(),
            info.git_commit.clone(),
            info.git_dirty,
            info.target.clone(),
            info.architecture.clone(),
        );
        if identity.as_ref().is_some_and(|previous| previous != &build) {
            return Err(error(
                "legacy_identity",
                "The installed runtime components have different build identities",
            ));
        }
        identity = Some(build);
        let observed = if pre_environment(&info) {
            InstallationKind::Legacy
        } else if info.environment_data_format == Some(DATA_FORMAT)
            && info.target.ends_with("-pc-windows-msvc")
        {
            InstallationKind::Unconfigured
        } else {
            return Err(error(
                "legacy_identity",
                "This installed package has no supported legacy upgrade identity",
            ));
        };
        if digest(&files[name])? != before || kind.is_some_and(|k| k != observed) {
            return Err(error(
                "legacy_identity",
                "The old package contains changing or mixed installation identities",
            ));
        }
        kind = Some(observed);
    }
    Ok(kind.expect("three checked programs"))
}

pub async fn classify(
    store: &EnvironmentStore,
    runtime: &Path,
) -> SetupResultValue<InstallationKind> {
    require_windows()?;
    let _lock = store.lock()?;
    classify_inner(store, runtime).await
}

pub(super) fn ensure_idle(store: &EnvironmentStore) -> SetupResultValue<()> {
    if store
        .read_json::<PackageJournal>(JOURNAL)?
        .is_some_and(|j| !matches!(j.phase, Phase::Committed | Phase::RolledBack))
    {
        return Err(error(
            "upgrade_pending",
            "Finish or roll back the prepared Windows package upgrade first",
        ));
    }
    Ok(())
}

fn receipt(journal: &PackageJournal) -> PackageReceipt {
    PackageReceipt {
        schema_version: 1,
        operation_id: journal.operation_id.clone(),
        owner_identity: journal.owner_identity.clone(),
        runtime_dir: journal.runtime_dir.clone(),
        manifest_sha256: journal.candidate.manifest_sha256.clone(),
        targets: journal
            .programs
            .iter()
            .map(|p| (p.name.clone(), p.target.clone()))
            .collect(),
    }
}

fn load_bound(store: &EnvironmentStore, runtime: &Path) -> SetupResultValue<PackageJournal> {
    no_environment(store)?;
    let owner = current_account()?.identity;
    checked_owner(store.root(), &owner)?;
    checked_owner(&store.root().join(JOURNAL), &owner)?;
    let journal: PackageJournal = store.read_json(JOURNAL)?.ok_or_else(SetupDiagnostic::io)?;
    let expected = targets(runtime)?;
    if journal.schema_version != 1
        || !journal.candidate.provenance_verified
        || journal.owner_identity != owner
        || journal.runtime_dir != runtime
        || !matches!(
            journal.kind,
            InstallationKind::Legacy | InstallationKind::Unconfigured
        )
        || receipt(&journal).targets != expected
        || journal.programs.len() != expected.len()
        || uuid::Uuid::parse_str(&journal.operation_id).is_err()
    {
        return Err(error(
            "upgrade_receipt",
            "The package owner, installation target or operation changed",
        ));
    }
    let backup_root = store
        .root()
        .join("upgrade-backups")
        .join(&journal.operation_id);
    checked_owner(&backup_root, &owner)?;
    for p in &journal.programs {
        if p.backup != backup_root.join(&p.name) {
            return Err(error(
                "upgrade_receipt",
                "The program backup target changed",
            ));
        }
        checked_owner(&p.backup, &owner)?;
        if digest(&p.backup)? != p.sha256 {
            return Err(error(
                "upgrade_backup_changed",
                "A retained program backup failed its integrity check",
            ));
        }
    }
    Ok(journal)
}

pub async fn preflight(
    store: &EnvironmentStore,
    candidate: &Path,
    runtime: &Path,
) -> SetupResultValue<UpgradePreflight> {
    require_windows()?;
    let native = NativeEnvironment::new()?;
    let checked = native.upgrade_preflight(store, candidate).await?;
    preflight_verified(store, runtime, checked).await
}

async fn preflight_verified(
    store: &EnvironmentStore,
    runtime: &Path,
    checked: UpgradePreflight,
) -> SetupResultValue<UpgradePreflight> {
    if !checked.candidate.provenance_verified {
        return Err(error(
            "candidate_provenance",
            "Package preflight requires verified published candidate provenance",
        ));
    }
    let _lock = store.lock()?;
    if let Some(previous) = store.read_json::<PackageJournal>(JOURNAL)? {
        if !matches!(previous.phase, Phase::Committed | Phase::RolledBack) {
            let previous = load_bound(store, runtime)?;
            if previous.candidate == checked.candidate
                && matches!(previous.phase, Phase::Prepared | Phase::SnapshotReady)
            {
                verify_original_programs(&previous)?;
                return Ok(checked);
            }
            return Err(error(
                "upgrade_pending",
                "Roll back the original package transaction before starting another upgrade",
            ));
        }
    }
    if !matches!(
        classify_inner(store, runtime).await?,
        InstallationKind::Legacy | InstallationKind::Unconfigured
    ) {
        return Err(error(
            "installer_environment",
            "This is not an unconfigured Windows package upgrade",
        ));
    }
    for path in targets(runtime)?.values() {
        stopped(path)?;
    }
    Ok(checked)
}

fn prepare_verified(
    store: &EnvironmentStore,
    candidate: UpgradeCandidate,
    runtime: &Path,
    kind: InstallationKind,
) -> SetupResultValue<()> {
    if !candidate.provenance_verified {
        return Err(error(
            "candidate_provenance",
            "Package preparation requires verified published candidate provenance",
        ));
    }
    let owner = current_account()?.identity;
    let id = uuid::Uuid::new_v4().to_string();
    let backup_root = store.root().join("upgrade-backups").join(&id);
    ensure_private_directory(&backup_root)?;
    let mut journal = PackageJournal {
        schema_version: 1,
        operation_id: id,
        owner_identity: owner.clone(),
        runtime_dir: runtime.into(),
        kind,
        candidate,
        phase: Phase::Prepared,
        programs: Vec::new(),
    };
    let preparation = (|| -> SetupResultValue<()> {
        let mut remaining = SNAPSHOT_LIMIT;
        for (name, target) in targets(runtime)? {
            owner_path(&target, &owner)?;
            stopped(&target)?;
            let sha256 = digest(&target)?;
            let backup = backup_root.join(&name);
            copy_bounded(&target, &backup, &mut remaining)?;
            #[cfg(windows)]
            crate::storage::secure_windows_path(&backup)?;
            if digest(&backup)? != sha256 || digest(&target)? != sha256 {
                return Err(error(
                    "upgrade_program_changed",
                    "The old program changed while preparing its backup",
                ));
            }
            journal.programs.push(ProgramBackup {
                target,
                backup,
                sha256,
                name,
            });
        }
        // Replacement authority exists only after all backups have been sealed.
        store.write_json(JOURNAL, &journal)?;
        journal.phase = Phase::SnapshotReady;
        store.write_json(JOURNAL, &journal)?;
        store.write_json(RECEIPT, &receipt(&journal))
    })();
    if preparation.is_err() {
        // Only a new, unreferenced partial backup can be discarded. Once its
        // journal is durable, keep the recovery point even if receipt writing fails.
        if store
            .read_json::<PackageJournal>(JOURNAL)
            .is_ok_and(|saved| saved.is_none_or(|j| j.operation_id != journal.operation_id))
        {
            let _ = std::fs::remove_dir_all(&backup_root);
        }
    }
    preparation
}

pub async fn prepare(
    store: &EnvironmentStore,
    candidate_dir: &Path,
    runtime: &Path,
) -> SetupResultValue<()> {
    require_windows()?;
    let checked = NativeEnvironment::new()?
        .upgrade_preflight(store, candidate_dir)
        .await?;
    if !checked.ready {
        return Err(error(
            "upgrade_not_ready",
            "The candidate upgrade is not ready",
        ));
    }
    let _lock = store.lock()?;
    no_environment(store)?;
    if let Some(previous) = store.read_json::<PackageJournal>(JOURNAL)? {
        if !matches!(previous.phase, Phase::Committed | Phase::RolledBack) {
            let previous = load_bound(store, runtime)?;
            if previous.candidate == checked.candidate
                && matches!(previous.phase, Phase::Prepared | Phase::SnapshotReady)
            {
                verify_original_programs(&previous)?;
                let mut previous = previous;
                previous.phase = Phase::SnapshotReady;
                store.write_json(JOURNAL, &previous)?;
                return store.write_json(RECEIPT, &receipt(&previous));
            }
            return Err(error(
                "upgrade_pending",
                "Recover the previous Windows package upgrade first",
            ));
        }
    }
    let kind = classify_inner(store, runtime).await?;
    if !matches!(
        kind,
        InstallationKind::Legacy | InstallationKind::Unconfigured
    ) {
        return Err(error(
            "installer_environment",
            "Use the installed Environment owner's transaction",
        ));
    }
    prepare_verified(store, checked.candidate, runtime, kind)
}

pub async fn verify(
    store: &EnvironmentStore,
    candidate_dir: &Path,
    runtime: &Path,
) -> SetupResultValue<PackageReceipt> {
    require_windows()?;
    let candidate = verify_upgrade_candidate(candidate_dir)?;
    verify_published_provenance(&candidate).await?;
    let _lock = store.lock()?;
    verify_receipt_inner(store, &candidate, runtime)
}

fn verify_receipt_inner(
    store: &EnvironmentStore,
    candidate: &UpgradeCandidate,
    runtime: &Path,
) -> SetupResultValue<PackageReceipt> {
    let journal = load_bound(store, runtime)?;
    let actual: PackageReceipt = store.read_json(RECEIPT)?.ok_or_else(SetupDiagnostic::io)?;
    checked_owner(&store.root().join(RECEIPT), &journal.owner_identity)?;
    let mut verified = candidate.clone();
    verified.provenance_verified = true;
    if journal.candidate != verified
        || journal.phase != Phase::SnapshotReady
        || actual != receipt(&journal)
        || actual.manifest_sha256 != candidate.manifest_sha256
    {
        return Err(error(
            "upgrade_receipt",
            "The prepared package receipt does not match this verified candidate",
        ));
    }
    verify_original_programs(&journal)?;
    Ok(actual)
}

fn verify_original_programs(journal: &PackageJournal) -> SetupResultValue<()> {
    for p in &journal.programs {
        owner_path(&p.target, &journal.owner_identity)?;
        stopped(&p.target)?;
        if digest(&p.target)? != p.sha256 {
            return Err(error(
                "upgrade_program_changed",
                "The old installation changed after preparation",
            ));
        }
    }
    Ok(())
}

pub async fn finish(store: &EnvironmentStore, runtime: &Path) -> SetupResultValue<()> {
    require_windows()?;
    let _lock = store.lock()?;
    finish_inner(store, runtime).await
}

async fn finish_inner(store: &EnvironmentStore, runtime: &Path) -> SetupResultValue<()> {
    let mut journal = load_bound(store, runtime)?;
    if journal.phase == Phase::Committed {
        return Ok(());
    }
    if !matches!(journal.phase, Phase::SnapshotReady | Phase::Verifying) {
        return Err(error(
            "upgrade_recovery_required",
            "The package has not reached its installation boundary",
        ));
    }
    journal.phase = Phase::Verifying;
    store.write_json(JOURNAL, &journal)?;
    let mut installed = journal.candidate.clone();
    for p in &journal.programs {
        owner_path(&p.target, &journal.owner_identity)?;
        let artifact = installed
            .artifacts
            .get_mut(&p.name)
            .ok_or_else(SetupDiagnostic::io)?;
        let expected = if p.name == "webcodex-desktop" {
            journal
                .candidate
                .desktop
                .as_ref()
                .and_then(|d| d.managed_files.get("WebCodex.exe"))
                .ok_or_else(SetupDiagnostic::io)?
        } else {
            &artifact.sha256
        };
        if digest(&p.target)? != *expected {
            return Err(error(
                "upgrade_installed_hash",
                "An installed program differs from the prepared package; roll back this upgrade",
            ));
        }
        artifact.path = p.target.clone();
    }
    verify_candidate_executables(&installed).await?;
    journal.phase = Phase::Committed;
    store.write_json(JOURNAL, &journal)
}

fn rollback_inner(store: &EnvironmentStore, runtime: &Path) -> SetupResultValue<()> {
    if store.read_json::<PackageJournal>(JOURNAL)?.is_none() {
        return Ok(());
    }
    let mut journal = load_bound(store, runtime)?;
    if journal.phase == Phase::RolledBack {
        return Ok(());
    }
    if journal.phase == Phase::Committed {
        return Err(error(
            "upgrade_committed",
            "A committed Windows upgrade cannot be rolled back",
        ));
    }
    journal.phase = Phase::Restoring;
    store.write_json(JOURNAL, &journal)?;
    for p in &journal.programs {
        owner_path(
            p.target.parent().ok_or_else(SetupDiagnostic::io)?,
            &journal.owner_identity,
        )?;
        if p.target.try_exists().map_err(|_| SetupDiagnostic::io())? {
            owner_path(&p.target, &journal.owner_identity)?;
            stopped(&p.target)?;
        }
        restore_program(p)?;
        #[cfg(windows)]
        crate::storage::secure_windows_path(&p.target)?;
        if digest(&p.target)? != p.sha256 {
            return Err(error(
                "upgrade_restore_failed",
                "The old program could not be restored",
            ));
        }
    }
    journal.phase = Phase::RolledBack;
    store.write_json(JOURNAL, &journal)
}

pub fn rollback(store: &EnvironmentStore, runtime: &Path) -> SetupResultValue<()> {
    require_windows()?;
    let _lock = store.lock()?;
    rollback_inner(store, runtime)
}

#[cfg(test)]
mod tests;
