use super::super::download::InstallationKind;
use crate::runtime_selection::RuntimeSource;
use crate::webcodex::cli::{ResolvedBinaries, ResolvedBinarySource};
use std::path::{Path, PathBuf};
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;
use webcodex_environment::unified_update::{
    self as unified, InstallerTarget, PackageFormat, RuntimePlatform, UpdateError, UpdateResult,
};
use webcodex_environment::{EnvironmentStore, RuntimeBinaries};

#[derive(Clone)]
pub struct InstallContext {
    pub(super) environment_id: String,
    pub(super) binaries: RuntimeBinaries,
    pub(super) desktop: PathBuf,
    pub(super) build: MachineBuildInfo,
    pub(crate) target: InstallerTarget,
}

#[cfg(target_os = "linux")]
fn os_release_family(text: &str) -> Option<PackageFormat> {
    let mut tokens = std::collections::BTreeSet::new();
    for line in text.lines().take(128) {
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        if !matches!(key, "ID" | "ID_LIKE") {
            continue;
        }
        let value = raw.trim().trim_matches('"').trim_matches('\'');
        for token in value.split_ascii_whitespace().take(16) {
            tokens.insert(token.to_ascii_lowercase());
        }
    }
    let deb = tokens
        .iter()
        .any(|value| matches!(value.as_str(), "debian" | "ubuntu"));
    let rpm = tokens.iter().any(|value| {
        matches!(
            value.as_str(),
            "fedora" | "rhel" | "centos" | "rocky" | "almalinux" | "openeuler" | "openruyi"
        )
    });
    match (deb, rpm) {
        (true, false) => Some(PackageFormat::Deb),
        (false, true) => Some(PackageFormat::Rpm),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn installed_linux_package_format() -> Option<PackageFormat> {
    let deb = std::process::Command::new("/usr/bin/dpkg-query")
        .args(["-W", "-f=${db:Status-Status}", "webcodex"])
        .output()
        .ok()
        .is_some_and(|output| output.status.success() && output.stdout == b"installed");
    let rpm = std::process::Command::new("/usr/bin/rpm")
        .args(["-q", "--quiet", "webcodex"])
        .status()
        .ok()
        .is_some_and(|status| status.success());
    match (deb, rpm) {
        (true, false) => Some(PackageFormat::Deb),
        (false, true) => Some(PackageFormat::Rpm),
        (true, true) => None,
        (false, false) => std::fs::read_to_string("/etc/os-release")
            .ok()
            .as_deref()
            .and_then(os_release_family),
    }
}

fn current_installer_target(platform: RuntimePlatform) -> Option<InstallerTarget> {
    #[cfg(target_os = "linux")]
    {
        return InstallerTarget::for_platform(platform, installed_linux_package_format()?);
    }
    #[cfg(not(target_os = "linux"))]
    {
        InstallerTarget::default_for_non_linux(platform)
    }
}

pub(crate) fn detected_installer_target() -> Option<InstallerTarget> {
    RuntimePlatform::current().and_then(current_installer_target)
}

fn aligned_release_build(build: &MachineBuildInfo, binaries: &ResolvedBinaries) -> bool {
    build.git_dirty == Some(false)
        && unified::stable_version(&build.version)
        && build.git_commit.as_deref().is_some_and(|v| {
            v.len() == 40
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        && matches!(binaries.source, ResolvedBinarySource::Bundled)
        && binaries.builds.len() == 3
        && unified::RUNTIME_BINARIES.iter().all(|name| {
            binaries.builds.iter().any(|info| {
                info.binary == *name
                    && info.validate(name).is_ok()
                    && info.version == build.version
                    && info.git_commit == build.git_commit
                    && info.git_dirty == Some(false)
            })
        })
}

fn package_layout(target: InstallerTarget, executable: &Path, runtime: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        executable.parent()
            == Some(Path::new(
                "/Applications/WebCodex Desktop.app/Contents/MacOS",
            ))
            && runtime == Path::new("/Library/Application Support/WebCodex/runtime")
            && Path::new("/var/db/receipts/dev.webcodex.unified-installer.plist").is_file()
    }
    #[cfg(target_os = "linux")]
    {
        let provenance = match target.format {
            PackageFormat::Deb => Path::new("/usr/share/doc/webcodex/unified-source-manifest.json"),
            PackageFormat::Rpm => Path::new("/usr/share/webcodex/unified-source-manifest.json"),
            _ => return false,
        };
        executable == Path::new("/usr/lib/webcodex/webcodex-desktop")
            && runtime == Path::new("/usr/lib/webcodex/webcodex-runtime")
            && provenance.is_file()
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let installed = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\WebCodex Desktop",
            )
            .and_then(|key| key.get_value::<String, _>("InstallLocation"))
            .ok()
            .and_then(|value| PathBuf::from(value.trim_matches('"')).canonicalize().ok());
        installed.is_some_and(|root| {
            executable == root.join("WebCodex.exe") && runtime == root.join("webcodex-runtime")
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (executable, runtime);
        false
    }
}

/// Cheap installation classification, separate from Runtime readiness. A later
/// explicit Install rechecks the *published bytes* of this installed generation.
pub fn assess_installation(
    build: MachineBuildInfo,
    binaries: Option<ResolvedBinaries>,
    source: RuntimeSource,
    environment_id: Option<String>,
) -> (InstallationKind, Option<InstallContext>) {
    let Some(target) = detected_installer_target() else {
        return (InstallationKind::UnsupportedPlatform, None);
    };
    if build.git_dirty != Some(false) || !matches!(source, RuntimeSource::Bundled) {
        return (InstallationKind::SourceBuild, None);
    }
    let Some(binaries) = binaries else {
        return (InstallationKind::UnmanagedInstallation, None);
    };
    if !aligned_release_build(&build, &binaries) {
        return (InstallationKind::SourceBuild, None);
    }
    let Some(desktop) = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok())
    else {
        return (InstallationKind::UnmanagedInstallation, None);
    };
    let Some(runtime) = binaries.directory.canonicalize().ok() else {
        return (InstallationKind::UnmanagedInstallation, None);
    };
    if !package_layout(target, &desktop, &runtime) {
        return (InstallationKind::UnmanagedInstallation, None);
    }
    let Some(environment_id) = environment_id else {
        return (InstallationKind::EnvironmentNotConfigured, None);
    };
    let context = (|| {
        let root = webcodex_environment::default_environment_dir().ok()?;
        if !root.join("environment.json").is_file() {
            return None;
        }
        let store = EnvironmentStore::open(root).ok()?;
        let record = store.load_environment().ok()??;
        if !record.configured
            || record.environment_id != environment_id
            || record.request.account.identity
                != webcodex_environment::current_account().ok()?.identity
        {
            return None;
        }
        for (stored, resolved, name) in [
            (&record.request.binaries.cli, &binaries.webcodex, "webcodex"),
            (
                &record.request.binaries.server,
                &binaries.server,
                "webcodex-server",
            ),
            (
                &record.request.binaries.runner,
                &binaries.runner,
                "webcodex-runner",
            ),
        ] {
            let exact = runtime.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            if stored.canonicalize().ok().as_ref() != Some(&exact)
                || resolved.canonicalize().ok().as_ref() != Some(&exact)
            {
                return None;
            }
        }
        Some(InstallContext {
            environment_id,
            binaries: record.request.binaries,
            desktop,
            build,
            target,
        })
    })();
    match context {
        Some(context) => (InstallationKind::Managed, Some(context)),
        None => (InstallationKind::EnvironmentNotConfigured, None),
    }
}

pub(super) fn environment(context: &InstallContext) -> UpdateResult<EnvironmentStore> {
    let root = webcodex_environment::default_environment_dir()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    if !root.join("environment.json").is_file() {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    let store = EnvironmentStore::open(root).map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let current = store
        .load_environment()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
        .ok_or(UpdateError::UpgradePreflightFailed)?;
    if !current.configured
        || current.environment_id != context.environment_id
        || current.request.binaries != context.binaries
        || current.request.account.identity
            != webcodex_environment::current_account()
                .map_err(|_| UpdateError::UpgradePreflightFailed)?
                .identity
    {
        return Err(UpdateError::UpgradePreflightFailed);
    }
    Ok(store)
}

pub(super) async fn verify_installed_generation(
    context: &InstallContext,
    target: InstallerTarget,
) -> UpdateResult<()> {
    #[cfg(unix)]
    unified::verify_installed_update_cli(&context.binaries.cli)?;
    let published = unified::fetch_release(&context.build.version, target)
        .await?
        .ok_or(UpdateError::ProvenanceFailed)?;
    if context.build.git_commit.as_deref() != Some(published.source.source_sha.as_str())
        || context.build.git_dirty != Some(false)
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    for (name, path) in [
        ("webcodex-desktop", &context.desktop),
        ("webcodex", &context.binaries.cli),
        ("webcodex-server", &context.binaries.server),
        ("webcodex-runner", &context.binaries.runner),
    ] {
        let expected = published
            .source
            .component_sha256(name)
            .ok_or(UpdateError::ProvenanceFailed)?;
        if hash_program(path).await?.as_str() != expected {
            return Err(UpdateError::ProvenanceFailed);
        }
    }
    Ok(())
}

async fn hash_program(path: &Path) -> UpdateResult<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options
        .open(path)
        .map_err(|_| UpdateError::ProvenanceFailed)?;
    let metadata = file.metadata().map_err(|_| UpdateError::ProvenanceFailed)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > unified::MAX_INSTALLER_BYTES {
        return Err(UpdateError::ProvenanceFailed);
    }
    let mut file = tokio::fs::File::from_std(file);
    let mut hash = Sha256::new();
    let mut count = 0u64;
    let mut bytes = vec![0; 128 * 1024];
    loop {
        let size = file
            .read(&mut bytes)
            .await
            .map_err(|_| UpdateError::ProvenanceFailed)?;
        if size == 0 {
            break;
        }
        count = count.saturating_add(size as u64);
        if count > unified::MAX_INSTALLER_BYTES {
            return Err(UpdateError::ProvenanceFailed);
        }
        hash.update(&bytes[..size]);
    }
    if count != metadata.len() {
        return Err(UpdateError::ProvenanceFailed);
    }
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_or_dirty_desktop_never_reaches_installation_layout_or_environment() {
        let mut build = crate::commands::get_desktop_build_info();
        build.git_dirty = Some(true);
        let (kind, context) = assess_installation(
            build,
            None,
            RuntimeSource::Bundled,
            Some("environment".into()),
        );
        assert_eq!(kind, InstallationKind::SourceBuild);
        assert!(context.is_none());
        let mut build = crate::commands::get_desktop_build_info();
        build.git_dirty = None;
        assert_eq!(
            assess_installation(build, None, RuntimeSource::Bundled, None).0,
            InstallationKind::SourceBuild
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn os_release_classification_is_bounded_and_ambiguous_fails_closed() {
        assert_eq!(
            os_release_family("ID=ubuntu\nID_LIKE=debian\n"),
            Some(PackageFormat::Deb)
        );
        assert_eq!(os_release_family("ID=fedora\n"), Some(PackageFormat::Rpm));
        assert_eq!(
            os_release_family("ID=centos\nID_LIKE=\"rhel fedora\"\n"),
            Some(PackageFormat::Rpm)
        );
        assert_eq!(
            os_release_family("ID=openeuler\nID_LIKE=\"rhel fedora\"\n"),
            Some(PackageFormat::Rpm)
        );
        assert_eq!(
            os_release_family("ID=ubuntu\nID_LIKE=\"debian rhel\"\n"),
            None
        );
        assert_eq!(os_release_family("ID=unknown\n"), None);
    }
}
