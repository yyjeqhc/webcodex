#[cfg(unix)]
use crate::unified_update::{InstallerTarget, PackageFormat, PrivateUpdateCache};
use crate::unified_update::{UpdateError, UpdateResult};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;
#[cfg(unix)]
use std::process::Stdio;

const MAX_EXPANDED_BYTES: u64 = 8 * 1024 * 1024 * 1024;
#[cfg(target_os = "linux")]
const MAX_RPM_INVENTORY_BYTES: u64 = 4 * 1024 * 1024;
#[cfg(target_os = "linux")]
fn rpm_candidate_root(flavor: crate::unified_update::PackageFlavor) -> &'static str {
    if flavor.is_full() {
        "/usr/share/webcodex/upgrade-candidate"
    } else {
        "/usr/share/webcodex-runtime/upgrade-candidate"
    }
}

#[cfg(unix)]
fn extraction_plan(
    target: InstallerTarget,
    package: &Path,
    destination: &Path,
) -> UpdateResult<(&'static str, Vec<std::ffi::OsString>, PathBuf)> {
    if !package.is_absolute() || !destination.is_absolute() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    if !target.valid() {
        return Err(UpdateError::UnsupportedPlatform);
    }
    match target.format {
        PackageFormat::Pkg => Ok((
            "/usr/sbin/pkgutil",
            vec![
                "--expand-full".into(),
                package.as_os_str().into(),
                destination.as_os_str().into(),
            ],
            destination.join("Scripts/upgrade-candidate"),
        )),
        PackageFormat::Deb => Ok((
            "/usr/bin/dpkg-deb",
            vec![
                "--control".into(),
                package.as_os_str().into(),
                destination.as_os_str().into(),
            ],
            destination.join("upgrade-candidate"),
        )),
        PackageFormat::Rpm | PackageFormat::Exe => Err(UpdateError::UnsupportedPlatform),
    }
}

#[cfg(target_os = "linux")]
fn rpm_candidate_patterns_from_inventory(
    bytes: &[u8],
    flavor: crate::unified_update::PackageFlavor,
) -> UpdateResult<Vec<std::ffi::OsString>> {
    if bytes.len() as u64 > MAX_RPM_INVENTORY_BYTES {
        return Err(UpdateError::DownloadTooLarge);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let root = Path::new(rpm_candidate_root(flavor));
    let mut seen = std::collections::BTreeSet::new();
    let mut patterns = Vec::new();
    let mut has_manifest = false;
    let mut has_sums = false;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let path = fields.next().ok_or(UpdateError::UpgradePreflightFailed)?;
        let mode = fields.next().ok_or(UpdateError::UpgradePreflightFailed)?;
        let link = fields.next().ok_or(UpdateError::UpgradePreflightFailed)?;
        if !path.starts_with('/') || path.bytes().any(|b| b.is_ascii_control()) {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        let path = Path::new(path);
        if path != root && !path.starts_with(root) {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| UpdateError::UpgradePreflightFailed)?;
        let mut depth = 0usize;
        for component in relative.components() {
            match component {
                std::path::Component::Normal(_) => depth += 1,
                _ => return Err(UpdateError::UpgradePreflightFailed),
            }
        }
        if depth > 40 || patterns.len() >= 50_000 || !seen.insert(path.to_path_buf()) {
            return Err(UpdateError::DownloadTooLarge);
        }
        // The canonical candidate contains only directories and regular files.
        // Reject links/devices before cpio sees them so extraction cannot redirect
        // a later path outside the private destination.
        if !(mode.starts_with('d') || mode.starts_with('-')) || !link.is_empty() {
            return Err(UpdateError::ProvenanceFailed);
        }
        has_manifest |= relative == Path::new("source-manifest.json");
        has_sums |= relative == Path::new("SHA256SUMS");
        patterns.push(std::ffi::OsString::from(format!(".{}", path.display())));
    }
    if patterns.is_empty() || !has_manifest || !has_sums {
        return Err(UpdateError::ProvenanceFailed);
    }
    Ok(patterns)
}

#[cfg(target_os = "linux")]
async fn rpm_inventory(
    package: &Path,
    flavor: crate::unified_update::PackageFlavor,
) -> UpdateResult<Vec<std::ffi::OsString>> {
    use tokio::io::AsyncReadExt;
    let mut command = tokio::process::Command::new("/usr/bin/rpm");
    command
        .args([
            "-qp",
            "--qf",
            "[%{FILENAMES}\\t%{FILEMODES:perms}\\t%{FILELINKTOS}\\n]",
        ])
        .arg(package)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let stdout = child
        .stdout
        .take()
        .ok_or(UpdateError::UpgradePreflightFailed)?;
    let mut bytes = Vec::new();
    let read = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        stdout
            .take(MAX_RPM_INVENTORY_BYTES.saturating_add(1))
            .read_to_end(&mut bytes),
    )
    .await;
    if !matches!(read, Ok(Ok(_))) || bytes.len() as u64 > MAX_RPM_INVENTORY_BYTES {
        let _ = child.start_kill();
        let _ = child.wait().await;
        return Err(UpdateError::DownloadTooLarge);
    }
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if !status.success() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    rpm_candidate_patterns_from_inventory(&bytes, flavor)
}

#[cfg(target_os = "linux")]
async fn write_rpm_cpio(cache: &PrivateUpdateCache, package: &Path) -> UpdateResult<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    cache.remove_file("payload.cpio")?;
    let file = cache.create_file("payload.cpio")?;
    let mut output = tokio::fs::File::from_std(file);
    let mut command = tokio::process::Command::new("/usr/bin/rpm2cpio");
    command
        .arg(package)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or(UpdateError::UpgradePreflightFailed)?;
    let result = tokio::time::timeout(std::time::Duration::from_secs(180), async {
        let mut count = 0u64;
        let mut buffer = [0u8; 128 * 1024];
        loop {
            let read = stdout
                .read(&mut buffer)
                .await
                .map_err(|_| UpdateError::UpgradePreflightFailed)?;
            if read == 0 {
                break;
            }
            count = count.saturating_add(read as u64);
            if count > MAX_EXPANDED_BYTES {
                return Err(UpdateError::DownloadTooLarge);
            }
            output
                .write_all(&buffer[..read])
                .await
                .map_err(|_| UpdateError::CacheUnavailable)?;
        }
        output
            .flush()
            .await
            .map_err(|_| UpdateError::CacheUnavailable)?;
        output
            .sync_all()
            .await
            .map_err(|_| UpdateError::CacheUnavailable)?;
        Ok::<(), UpdateError>(())
    })
    .await;
    if !matches!(result, Ok(Ok(()))) {
        let _ = child.start_kill();
        let _ = child.wait().await;
        let _ = cache.remove_file("payload.cpio");
        return match result {
            Ok(Err(error)) => Err(error),
            _ => Err(UpdateError::UpgradePreflightFailed),
        };
    }
    let status = tokio::time::timeout(std::time::Duration::from_secs(10), child.wait())
        .await
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if !status.success() {
        let _ = cache.remove_file("payload.cpio");
        return Err(UpdateError::UpgradePreflightFailed);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
async fn extract_rpm_candidate(
    cache: &PrivateUpdateCache,
    package: &Path,
    destination: &Path,
    flavor: crate::unified_update::PackageFlavor,
) -> UpdateResult<PathBuf> {
    let patterns = rpm_inventory(package, flavor).await?;
    write_rpm_cpio(cache, package).await?;
    let archive = cache.open_file("payload.cpio")?;
    let mut command = tokio::process::Command::new("/usr/bin/cpio");
    command
        .args([
            "--extract",
            "--make-directories",
            "--no-absolute-filenames",
            "--quiet",
        ])
        .args(patterns)
        .current_dir(destination)
        .stdin(Stdio::from(archive))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(180), child.wait()).await;
    let _ = cache.remove_file("payload.cpio");
    let status = status
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if !status.success() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    let candidate = destination.join(rpm_candidate_root(flavor).trim_start_matches('/'));
    bound_tree(destination, 0, &mut 0, &mut 0)?;
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
    target: InstallerTarget,
) -> UpdateResult<PathBuf> {
    cache.remove_child_tree("expanded")?;
    let destination = cache.file("expanded")?;
    #[cfg(target_os = "linux")]
    if target.format == PackageFormat::Rpm {
        PrivateUpdateCache::open(destination.clone())?;
        return extract_rpm_candidate(cache, installer, &destination, target.flavor).await;
    }
    let (program, arguments, candidate) = extraction_plan(target, installer, &destination)?;
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
    use crate::unified_update::RuntimePlatform;
    #[cfg(unix)]
    #[test]
    fn extraction_uses_only_package_metadata_not_installation_commands() {
        let path = Path::new("/private/a b/pkg.pkg");
        let root = Path::new("/private/cache/expanded");
        let (program, args, candidate) = extraction_plan(
            InstallerTarget::new(RuntimePlatform::DarwinArm64, PackageFormat::Pkg),
            path,
            root,
        )
        .unwrap();
        assert_eq!(program, "/usr/sbin/pkgutil");
        assert_eq!(args[0], "--expand-full");
        assert_eq!(candidate, root.join("Scripts/upgrade-candidate"));
        let (program, args, candidate) = extraction_plan(
            InstallerTarget::new(RuntimePlatform::LinuxX64, PackageFormat::Deb),
            path,
            root,
        )
        .unwrap();
        assert_eq!(program, "/usr/bin/dpkg-deb");
        assert_eq!(args[0], "--control");
        assert_eq!(candidate, root.join("upgrade-candidate"));
        assert!(extraction_plan(
            InstallerTarget::new(RuntimePlatform::LinuxX64, PackageFormat::Rpm),
            path,
            root
        )
        .is_err());
        assert!(extraction_plan(
            InstallerTarget::new(RuntimePlatform::Win32Arm64, PackageFormat::Exe),
            path,
            root
        )
        .is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rpm_inventory_is_bounded_relative_and_link_free() {
        let valid = b"/usr/share/webcodex/upgrade-candidate\tdrwxr-xr-x\t\n/usr/share/webcodex/upgrade-candidate/source-manifest.json\t-rw-r--r--\t\n/usr/share/webcodex/upgrade-candidate/SHA256SUMS\t-rw-r--r--\t\n/usr/lib/webcodex/webcodex-desktop\t-rwxr-xr-x\t\n";
        let patterns = rpm_candidate_patterns_from_inventory(
            valid,
            crate::unified_update::PackageFlavor::Full,
        )
        .unwrap();
        assert_eq!(patterns.len(), 3);
        let runtime = String::from_utf8(valid.to_vec())
            .unwrap()
            .replace("/usr/share/webcodex/", "/usr/share/webcodex-runtime/");
        let runtime_patterns = rpm_candidate_patterns_from_inventory(
            runtime.as_bytes(),
            crate::unified_update::PackageFlavor::Runtime,
        )
        .unwrap();
        assert_eq!(runtime_patterns.len(), 3);
        assert!(runtime_patterns.iter().all(|p| p
            .to_string_lossy()
            .starts_with("./usr/share/webcodex-runtime/upgrade-candidate")));
        assert!(rpm_candidate_patterns_from_inventory(
            valid,
            crate::unified_update::PackageFlavor::Runtime
        )
        .is_err());
        assert!(rpm_candidate_patterns_from_inventory(
            runtime.as_bytes(),
            crate::unified_update::PackageFlavor::Full
        )
        .is_err());
        for invalid in [
            b"/usr/share/webcodex/upgrade-candidate\tdrwxr-xr-x\t\n/usr/share/webcodex/upgrade-candidate/source-manifest.json\tlrwxrwxrwx\t/etc/passwd\n/usr/share/webcodex/upgrade-candidate/SHA256SUMS\t-rw-r--r--\t\n".as_slice(),
            b"/usr/share/webcodex/upgrade-candidate\tdrwxr-xr-x\t\n/usr/share/webcodex/upgrade-candidate/../escape\t-rw-r--r--\t\n/usr/share/webcodex/upgrade-candidate/source-manifest.json\t-rw-r--r--\t\n/usr/share/webcodex/upgrade-candidate/SHA256SUMS\t-rw-r--r--\t\n".as_slice(),
            b"/usr/share/webcodex/upgrade-candidate/source-manifest.json\t-rw-r--r--\t\n".as_slice(),
        ] {
            assert!(rpm_candidate_patterns_from_inventory(invalid, crate::unified_update::PackageFlavor::Full).is_err());
        }
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
