use std::path::{Path, PathBuf};
use std::process::Stdio;
use webcodex_environment::unified_update::{
    InstallerPlatform, PrivateUpdateCache, UpdateError, UpdateResult,
};

const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[cfg(unix)]
fn extraction_plan(
    platform: InstallerPlatform,
    package: &Path,
    destination: &Path,
) -> UpdateResult<(&'static str, Vec<std::ffi::OsString>, PathBuf)> {
    if !package.is_absolute() || !destination.is_absolute() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    match platform {
        InstallerPlatform::DarwinX64 | InstallerPlatform::DarwinArm64 => Ok((
            "/usr/sbin/pkgutil",
            vec![
                "--expand-full".into(),
                package.as_os_str().into(),
                destination.as_os_str().into(),
            ],
            destination.join("Scripts/upgrade-candidate"),
        )),
        InstallerPlatform::LinuxX64 | InstallerPlatform::LinuxArm64 => Ok((
            "/usr/bin/dpkg-deb",
            vec![
                "--control".into(),
                package.as_os_str().into(),
                destination.as_os_str().into(),
            ],
            destination.join("upgrade-candidate"),
        )),
        _ => Err(UpdateError::UnsupportedPlatform),
    }
}

fn bound_tree(path: &Path, depth: usize, files: &mut usize, bytes: &mut u64) -> UpdateResult<()> {
    if depth > 40 || *files >= 50_000 {
        return Err(UpdateError::DownloadTooLarge);
    }
    *files += 1;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if metadata.is_symlink() {
        return Ok(());
    } // Never follow extracted links.
    if metadata.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|_| UpdateError::UpgradePreflightFailed)? {
            bound_tree(
                &entry
                    .map_err(|_| UpdateError::UpgradePreflightFailed)?
                    .path(),
                depth + 1,
                files,
                bytes,
            )?;
        }
    } else if metadata.is_file() {
        *bytes = bytes.saturating_add(metadata.len());
        if *bytes > MAX_EXPANDED_BYTES {
            return Err(UpdateError::DownloadTooLarge);
        }
    } else {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    Ok(())
}

#[cfg(unix)]
pub(super) async fn extract_candidate(
    cache: &PrivateUpdateCache,
    installer: &Path,
    platform: InstallerPlatform,
) -> UpdateResult<PathBuf> {
    cache.remove_child_tree("expanded")?;
    let destination = cache.file("expanded")?;
    let (program, arguments, candidate) = extraction_plan(platform, installer, &destination)?;
    // Native archive inspection does not execute package pre/postinstall hooks.
    let mut command = tokio::process::Command::new(program);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Keep the trusted extractor's descendants in one group for a bounded stop.
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let process_group = child.id();
    let status = match tokio::time::timeout(std::time::Duration::from_secs(180), child.wait()).await
    {
        Ok(Ok(status)) => status,
        _ => {
            if let Some(pid) = process_group {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
            return Err(UpdateError::UpgradePreflightFailed);
        }
    };
    if !status.success() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    bound_tree(&destination, 0, &mut 0, &mut 0)?;
    let exact = candidate
        .canonicalize()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let root = destination
        .canonicalize()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if exact != candidate || !exact.starts_with(&root) {
        return Err(UpdateError::ProvenanceFailed);
    }
    Ok(exact)
}

#[cfg(all(test, target_os = "macos"))]
mod native_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn extraction_uses_only_package_metadata_not_installation_commands() {
        let path = Path::new("/private/a b/pkg.pkg");
        let root = Path::new("/private/cache/expanded");
        let (program, args, candidate) =
            extraction_plan(InstallerPlatform::DarwinArm64, path, root).unwrap();
        assert_eq!(program, "/usr/sbin/pkgutil");
        assert_eq!(args[0], "--expand-full");
        assert_eq!(candidate, root.join("Scripts/upgrade-candidate"));
        let (program, args, candidate) =
            extraction_plan(InstallerPlatform::LinuxX64, path, root).unwrap();
        assert_eq!(program, "/usr/bin/dpkg-deb");
        assert_eq!(args[0], "--control");
        assert_eq!(candidate, root.join("upgrade-candidate"));
        assert!(extraction_plan(InstallerPlatform::Win32Arm64, path, root).is_err());
    }
    #[test]
    fn expanded_tree_has_entry_depth_and_byte_limits() {
        let temp = tempfile::tempdir().unwrap();
        assert!(bound_tree(temp.path(), 41, &mut 0, &mut 0).is_err());
        assert!(bound_tree(temp.path(), 0, &mut 50_000, &mut 0).is_err());
        let file = temp.path().join("large");
        std::fs::write(&file, b"a").unwrap();
        let mut bytes = MAX_EXPANDED_BYTES;
        assert!(bound_tree(&file, 0, &mut 0, &mut bytes).is_err());
    }
}
