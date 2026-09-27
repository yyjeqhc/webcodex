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
use std::path::Path;
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
    platform: InstallerPlatform,
    path: &Path,
) -> UpdateResult<(&'static str, Vec<std::ffi::OsString>)> {
    if !path.is_absolute() {
        return Err(UpdateError::InstallerLaunchFailed);
    }
    match platform {
        InstallerPlatform::DarwinX64 | InstallerPlatform::DarwinArm64 => Ok((
            "/usr/sbin/installer",
            vec![
                "-pkg".into(),
                path.as_os_str().into(),
                "-target".into(),
                "/".into(),
            ],
        )),
        InstallerPlatform::LinuxX64 | InstallerPlatform::LinuxArm64 => Ok((
            "/usr/bin/dpkg",
            vec!["--install".into(), path.as_os_str().into()],
        )),
        _ => Err(UpdateError::UnsupportedPlatform),
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

fn clear_finished_download(
    cache: &PrivateUpdateCache,
    platform: InstallerPlatform,
) -> UpdateResult<()> {
    let Some(bytes) = cache.read("intent.json", MAX_SOURCE_BYTES)? else {
        return Ok(());
    };
    let intent: Intent =
        serde_json::from_slice(&bytes).map_err(|_| UpdateError::RecoveryRequired)?;
    if intent.schema_version != 1
        || !stable_version(&intent.version)
        || intent.filename != platform.installer_filename(&intent.version)
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
    platform: InstallerPlatform,
) -> UpdateResult<()> {
    // Called only after the sole durable intent is absent or proven terminal.
    // A crash during root-side copying must not accumulate packages by version.
    cache.remove_file("installer.part")?;
    let suffix = format!("-{}.{}", platform.as_str(), platform.extension());
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
        if stable_version(version) && name == platform.installer_filename(version) {
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
    mut started: impl FnMut(InstallerLaunchNotice),
) -> UpdateResult<()> {
    root_required()?;
    let platform = InstallerPlatform::current().ok_or(UpdateError::UnsupportedPlatform)?;
    if !matches!(
        platform,
        InstallerPlatform::DarwinX64
            | InstallerPlatform::DarwinArm64
            | InstallerPlatform::LinuxX64
            | InstallerPlatform::LinuxArm64
    ) {
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
    clear_finished_download(&cache, platform)?;
    remove_orphaned_packages(&cache, platform)?;
    let candidate = crate::verify_upgrade_candidate(candidate_dir)
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let Some(release) = fetch_release(&candidate.version, platform).await? else {
        return Err(UpdateError::ManifestMissing);
    };
    let entry = release.manifest.entry(platform)?;
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
    // The root cache has one candidate and cannot accumulate .pkg/.deb files.
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
    let (program, arguments) = package_program(platform, &package)?;
    if !Path::new(program).is_file() {
        return Err(UpdateError::InstallerLaunchFailed);
    }
    let mut intent = Intent {
        schema_version: 1,
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
    }
    if status.success() && phase == "committed" {
        Ok(())
    } else {
        Err(UpdateError::RecoveryRequired)
    }
}

#[cfg(test)]
mod tests;
