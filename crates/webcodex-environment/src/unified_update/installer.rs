//! Narrow privileged adapter for the existing #697 owner receipt. The caller
//! must obtain normal OS authorization for the installed trusted CLI. The
//! adapter never chooses an Environment, service owner, target or package URL.
use super::*;
use crate::installer_authorization::{
    cancel_matching_authorization, system_directory, system_runtime_directory,
};
use crate::upgrade::{
    check_frozen_installer_transition, freeze_installer_upgrade, FrozenInstallerUpgrade,
};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallerLaunchNotice {
    pub schema_version: u16,
    pub started: bool,
    pub safe_to_restore: bool,
    pub operation_id: Option<String>,
    pub version: Option<String>,
    pub error_kind: Option<UpdateError>,
}
impl InstallerLaunchNotice {
    pub fn not_started(error: UpdateError) -> Self {
        Self {
            schema_version: 1,
            started: false,
            safe_to_restore: error != UpdateError::RecoveryRequired,
            operation_id: None,
            version: None,
            error_kind: Some(error),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Intent {
    schema_version: u16,
    target: InstallerTarget,
    filename: String,
    version: String,
    finished: bool,
    frozen: FrozenInstallerUpgrade,
}

fn root_required() -> UpdateResult<()> {
    if unsafe { libc::geteuid() } == 0 {
        Ok(())
    } else {
        Err(UpdateError::AuthorizationRequired)
    }
}

fn package_program(
    target: InstallerTarget,
    path: &Path,
) -> UpdateResult<(&'static str, Vec<std::ffi::OsString>)> {
    if !path.is_absolute() || !target.valid() {
        return Err(UpdateError::InstallerLaunchFailed);
    }
    match target.format {
        PackageFormat::Pkg => Ok((
            "/usr/sbin/installer",
            vec![
                "-pkg".into(),
                path.as_os_str().into(),
                "-target".into(),
                "/".into(),
            ],
        )),
        PackageFormat::Deb => Ok((
            "/usr/bin/dpkg",
            vec!["--install".into(), path.as_os_str().into()],
        )),
        PackageFormat::Rpm => Ok((
            "/usr/bin/rpm",
            vec!["--upgrade".into(), path.as_os_str().into()],
        )),
        PackageFormat::Exe => Err(UpdateError::UnsupportedPlatform),
    }
}

/// Read-only admission used both before OS elevation and by the privileged
/// helper. A correct hash alone cannot protect a user-replaceable executable
/// during an authorization dialog: every ancestor must also be root-owned and
/// non-writable by the original user. This never adjusts permissions itself.
pub fn verify_installed_update_cli(cli: &Path) -> UpdateResult<()> {
    let expected = system_runtime_directory()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
        .join("webcodex");
    if cli != expected {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    for path in cli.ancestors() {
        let metadata =
            std::fs::symlink_metadata(path).map_err(|_| UpdateError::UpgradePreflightFailed)?;
        if metadata.is_symlink()
            || metadata.uid() != 0
            || metadata.mode() & 0o022 != 0
            || (path == cli && (!metadata.is_file() || metadata.mode() & 0o111 == 0))
        {
            return Err(UpdateError::UpgradePreflightFailed);
        }
    }
    Ok(())
}

fn trusted_installed_cli(receipt: &crate::PreparedInstallationReceipt) -> UpdateResult<()> {
    let runtime = system_runtime_directory().map_err(|_| UpdateError::UpgradePreflightFailed)?;
    crate::verify_installer_targets(receipt, &runtime)
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let cli = runtime.join("webcodex");
    if std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        != Some(cli.clone())
    {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    verify_installed_update_cli(&cli)
}

/// Freeze bytes into a root-owned private directory before a privileged package
/// manager can open them. The original user can change neither this inode nor
/// its parent after the final hash, unlike a user-writable download pathname.
fn copy_installer(
    input: &Path,
    owner_uid: u32,
    cache: &PrivateUpdateCache,
    filename: &str,
    expected: &str,
    maximum: u64,
) -> UpdateResult<()> {
    let meta = std::fs::symlink_metadata(input).map_err(|_| UpdateError::DownloadFailed)?;
    if meta.is_symlink()
        || !meta.is_file()
        || meta.uid() != owner_uid
        || meta.nlink() != 1
        || meta.mode() & 0o077 != 0
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    let mut original = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(input)
        .map_err(|_| UpdateError::DownloadFailed)?;
    let actual = original
        .metadata()
        .map_err(|_| UpdateError::DownloadFailed)?;
    if actual.dev() != meta.dev() || actual.ino() != meta.ino() || actual.len() != meta.len() {
        return Err(UpdateError::ProvenanceFailed);
    }
    if actual.len() == 0 || actual.len() > maximum {
        return Err(UpdateError::DownloadTooLarge);
    }
    cache.remove_file("installer.part")?;
    let mut copy = cache.create_file("installer.part")?;
    let result = (|| {
        let mut hash = Sha256::new();
        let mut count = 0u64;
        let mut buffer = [0; 128 * 1024];
        loop {
            let n = original
                .read(&mut buffer)
                .map_err(|_| UpdateError::DownloadFailed)?;
            if n == 0 {
                break;
            }
            count = count.saturating_add(n as u64);
            if count > maximum {
                return Err(UpdateError::DownloadTooLarge);
            }
            copy.write_all(&buffer[..n])
                .map_err(|_| UpdateError::CacheUnavailable)?;
            hash.update(&buffer[..n]);
        }
        if count != actual.len() || format!("{:x}", hash.finalize()) != expected {
            return Err(UpdateError::ChecksumMismatch);
        }
        copy.sync_all().map_err(|_| UpdateError::CacheUnavailable)?;
        Ok(())
    })();
    drop(copy);
    match result {
        Ok(()) => cache.commit("installer.part", filename),
        Err(error) => {
            let _ = cache.remove_file("installer.part");
            Err(error)
        }
    }
}

const MAX_RECOVERY_BYTES: u64 = 8 * 1024 * 1024 * 1024;

fn copy_recovery_tree(
    source: &Path,
    destination: &Path,
    depth: usize,
    entries: &mut usize,
    bytes: &mut u64,
) -> UpdateResult<()> {
    use std::os::unix::fs::PermissionsExt;
    if depth > 40 || *entries >= 50_000 {
        return Err(UpdateError::DownloadTooLarge);
    }
    *entries += 1;
    let metadata =
        std::fs::symlink_metadata(source).map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if metadata.is_symlink() {
        return Err(UpdateError::ProvenanceFailed);
    }
    if metadata.is_dir() {
        std::fs::create_dir(destination).map_err(|_| UpdateError::CacheUnavailable)?;
        std::fs::set_permissions(destination, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| UpdateError::CacheUnavailable)?;
        for entry in std::fs::read_dir(source).map_err(|_| UpdateError::UpgradePreflightFailed)? {
            let entry = entry.map_err(|_| UpdateError::UpgradePreflightFailed)?;
            copy_recovery_tree(
                &entry.path(),
                &destination.join(entry.file_name()),
                depth + 1,
                entries,
                bytes,
            )?;
        }
        std::fs::File::open(destination)
            .and_then(|file| file.sync_all())
            .map_err(|_| UpdateError::CacheUnavailable)?;
        return Ok(());
    }
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(UpdateError::ProvenanceFailed);
    }
    *bytes = bytes.saturating_add(metadata.len());
    if *bytes > MAX_RECOVERY_BYTES {
        return Err(UpdateError::DownloadTooLarge);
    }
    let mut input = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(source)
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let actual = input
        .metadata()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if actual.dev() != metadata.dev()
        || actual.ino() != metadata.ino()
        || actual.len() != metadata.len()
        || !actual.is_file()
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    let mut output = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_CLOEXEC)
        .open(destination)
        .map_err(|_| UpdateError::CacheUnavailable)?;
    std::io::copy(&mut input, &mut output).map_err(|_| UpdateError::CacheUnavailable)?;
    output
        .sync_all()
        .map_err(|_| UpdateError::CacheUnavailable)?;
    Ok(())
}

fn rpm_recovery_cache() -> UpdateResult<PrivateUpdateCache> {
    PrivateUpdateCache::open(
        system_directory()
            .map_err(|_| UpdateError::CacheUnavailable)?
            .join("recovery"),
    )
}

fn remove_rpm_recovery_candidate() -> UpdateResult<()> {
    rpm_recovery_cache()?.remove_child_tree("candidate")
}

fn freeze_rpm_recovery_candidate(
    candidate_dir: &Path,
    expected: &crate::UpgradeCandidate,
) -> UpdateResult<PathBuf> {
    let source = candidate_dir
        .canonicalize()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let recovery = rpm_recovery_cache()?;
    recovery.remove_child_tree("candidate")?;
    let destination = recovery.file("candidate")?;
    let result = copy_recovery_tree(&source, &destination, 0, &mut 0, &mut 0);
    if let Err(error) = result {
        let _ = recovery.remove_child_tree("candidate");
        return Err(error);
    }
    let copied =
        crate::verify_upgrade_candidate(&destination).map_err(|_| UpdateError::ProvenanceFailed)?;
    if copied.version != expected.version
        || copied.source_sha != expected.source_sha
        || copied.manifest_sha256 != expected.manifest_sha256
        || copied.platform != expected.platform
    {
        let _ = recovery.remove_child_tree("candidate");
        return Err(UpdateError::ProvenanceFailed);
    }
    Ok(destination)
}

fn clear_finished_download(
    cache: &PrivateUpdateCache,
    target: InstallerTarget,
) -> UpdateResult<()> {
    let Some(bytes) = cache.read("intent.json", MAX_SOURCE_BYTES)? else {
        return Ok(());
    };
    let intent: Intent =
        serde_json::from_slice(&bytes).map_err(|_| UpdateError::RecoveryRequired)?;
    if intent.schema_version != 1
        || !stable_version(&intent.version)
        || intent.target != target
        || intent.filename != target.installer_filename(&intent.version)
    {
        return Err(UpdateError::RecoveryRequired);
    }
    if !intent.finished
        && !matches!(
            check_frozen_installer_transition(&intent.frozen),
            Ok("committed" | "rolled_back")
        )
    {
        return Err(UpdateError::RecoveryRequired);
    }
    cache.remove_file(&intent.filename)?;
    cache.remove_file("intent.json")
}

fn remove_orphaned_packages(
    cache: &PrivateUpdateCache,
    target: InstallerTarget,
) -> UpdateResult<()> {
    // Called only after the sole durable intent is absent or proven terminal.
    // A crash during root-side copying must not accumulate packages by version.
    cache.remove_file("installer.part")?;
    let suffix = format!(
        "-{}.{}",
        target.platform.as_str(),
        target.format.extension()
    );
    for (index, item) in std::fs::read_dir(cache.root())
        .map_err(|_| UpdateError::CacheUnavailable)?
        .enumerate()
    {
        if index >= 128 {
            return Err(UpdateError::CacheUnavailable);
        }
        let item = item.map_err(|_| UpdateError::CacheUnavailable)?;
        let Some(name) = item.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(version) = name
            .strip_prefix("webcodex-unified-v")
            .and_then(|v| v.strip_suffix(&suffix))
        else {
            continue;
        };
        if stable_version(version) && name == target.installer_filename(version) {
            cache.remove_file(&name)?;
        }
    }
    Ok(())
}

fn save_intent(cache: &PrivateUpdateCache, intent: &Intent) -> UpdateResult<()> {
    cache.write(
        "intent.json",
        &serde_json::to_vec(intent).map_err(|_| UpdateError::CacheUnavailable)?,
    )
}

fn retract_before_launch(
    cache: &PrivateUpdateCache,
    intent: &mut Intent,
    receipt: &crate::PreparedInstallationReceipt,
) -> UpdateResult<()> {
    cancel_matching_authorization(receipt).map_err(|_| UpdateError::RecoveryRequired)?;
    intent.finished = true;
    save_intent(cache, intent).map_err(|_| UpdateError::RecoveryRequired)
}

/// Success here is a Core-verified terminal transaction, not `spawn()` success.
/// `started` is emitted earlier so Desktop can close after its explicit Install
/// confirmation. A lost acknowledgement is recovery-required, never redispatch.
pub async fn apply_verified_installer(
    receipt_path: &Path,
    candidate_dir: &Path,
    installer_path: &Path,
    target: InstallerTarget,
    mut started: impl FnMut(InstallerLaunchNotice),
) -> UpdateResult<()> {
    root_required()?;
    let platform = RuntimePlatform::current().ok_or(UpdateError::UnsupportedPlatform)?;
    if target.platform != platform || !target.valid() || matches!(target.format, PackageFormat::Exe)
    {
        return Err(UpdateError::UnsupportedPlatform);
    }
    let cache = PrivateUpdateCache::open(
        system_directory()
            .map_err(|_| UpdateError::CacheUnavailable)?
            .join("updater"),
    )?;
    // A busy or interrupted privileged attempt is not permission to roll back
    // another live package manager from the Desktop process.
    let _lock = cache.lock().map_err(|_| UpdateError::RecoveryRequired)?;
    clear_finished_download(&cache, target)?;
    remove_orphaned_packages(&cache, target)?;
    let candidate = crate::verify_upgrade_candidate(candidate_dir)
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let Some(release) = fetch_release(&candidate.version, target).await? else {
        return Err(UpdateError::ManifestMissing);
    };
    let entry = release.manifest.entry(target)?;
    if candidate.manifest_sha256 != entry.source_manifest_sha256
        || candidate.source_sha != release.source.source_sha
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    let receipt = crate::verify_prepared_installation(receipt_path, candidate_dir)
        .await
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    trusted_installed_cli(&receipt)?;
    let owner_uid = receipt
        .owner_identity
        .parse::<u32>()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if owner_uid == 0 {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    let frozen =
        freeze_installer_upgrade(&receipt).map_err(|_| UpdateError::UpgradePreflightFailed)?;
    // The root cache has one candidate and cannot accumulate package files.
    cache.remove_file(&entry.filename)?;
    copy_installer(
        installer_path,
        owner_uid,
        &cache,
        &entry.filename,
        &entry.sha256,
        MAX_INSTALLER_BYTES,
    )?;
    let package = cache.file(&entry.filename)?;
    let (program, arguments) = package_program(target, &package)?;
    if !Path::new(program).is_file() {
        return Err(UpdateError::InstallerLaunchFailed);
    }
    let recovery_candidate = if target.format == PackageFormat::Rpm {
        Some(freeze_rpm_recovery_candidate(candidate_dir, &candidate)?)
    } else {
        None
    };
    let mut intent = Intent {
        schema_version: 1,
        target,
        filename: entry.filename.clone(),
        version: candidate.version.clone(),
        finished: false,
        frozen,
    };
    // Durable launch intent precedes authorization and spawning. Process crash
    // at any following boundary requires transaction reconciliation.
    save_intent(&cache, &intent)?;
    if crate::authorize_prepared_installation(receipt_path, candidate_dir)
        .await
        .is_err()
    {
        retract_before_launch(&cache, &mut intent, &receipt)?;
        if recovery_candidate.is_some() {
            let _ = remove_rpm_recovery_candidate();
        }
        return Err(UpdateError::AuthorizationRequired);
    }
    let mut child = match tokio::process::Command::new(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            retract_before_launch(&cache, &mut intent, &receipt)?;
            if recovery_candidate.is_some() {
                let _ = remove_rpm_recovery_candidate();
            }
            return Err(UpdateError::InstallerLaunchFailed);
        }
    };
    started(InstallerLaunchNotice {
        schema_version: 1,
        started: true,
        safe_to_restore: false,
        operation_id: Some(receipt.operation_id.clone()),
        version: Some(candidate.version.clone()),
        error_kind: None,
    });
    let status = tokio::time::timeout(Duration::from_secs(30 * 60), child.wait())
        .await
        .map_err(|_| UpdateError::RecoveryRequired)?
        .map_err(|_| UpdateError::RecoveryRequired)?;
    if !status.success()
        && !matches!(
            check_frozen_installer_transition(&intent.frozen),
            Ok("committed" | "rolled_back")
        )
    {
        // Reuse the original-owner broker: it verifies installed bytes and
        // either finishes that exact transaction or restores its own snapshot.
        let _ = crate::finish_authorized_installation().await;
    }
    let phase = check_frozen_installer_transition(&intent.frozen)
        .map_err(|_| UpdateError::RecoveryRequired)?;
    if matches!(phase, "committed" | "rolled_back") {
        intent.finished = true;
        save_intent(&cache, &intent)?;
        cache.remove_file(&entry.filename)?;
        if recovery_candidate.is_some() {
            let _ = remove_rpm_recovery_candidate();
        }
    }
    if status.success() && phase == "committed" {
        Ok(())
    } else {
        Err(UpdateError::RecoveryRequired)
    }
}

#[cfg(test)]
mod tests;
