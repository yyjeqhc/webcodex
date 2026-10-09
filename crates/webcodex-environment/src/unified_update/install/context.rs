use super::super::download::InstallationKind;
#[cfg(target_os = "linux")]
use crate::unified_update::PackageFormat;
use crate::unified_update::{
    self as unified, InstallerTarget, RuntimePlatform, UpdateError, UpdateResult,
};
use crate::{EnvironmentStore, RuntimeBinaries};
use std::path::{Path, PathBuf};
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;

#[derive(Clone)]
pub struct InstallContext {
    pub environment_root: PathBuf,
    pub environment_id: String,
    pub binaries: RuntimeBinaries,
    pub desktop: Option<PathBuf>,
    pub build: MachineBuildInfo,
    pub target: InstallerTarget,
}

#[derive(Clone)]
pub struct InstalledBinaries {
    pub directory: PathBuf,
    pub binaries: RuntimeBinaries,
    pub builds: Vec<MachineBuildInfo>,
    pub bundled: bool,
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
    use crate::process::CommandOutputExt;
    const QUERY_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);
    const QUERY_OUTPUT_BYTES: usize = 4096;
    let deb = std::process::Command::new("/usr/bin/dpkg-query")
        .args(["-W", "-f=${db:Status-Status}", "webcodex"])
        .output_with_limits(QUERY_DEADLINE, QUERY_OUTPUT_BYTES)
        .ok()
        .is_some_and(|output| output.status.success() && output.stdout == b"installed");
    let rpm = std::process::Command::new("/usr/bin/rpm")
        .args(["-q", "--quiet", "webcodex"])
        .output_with_limits(QUERY_DEADLINE, QUERY_OUTPUT_BYTES)
        .ok()
        .is_some_and(|output| output.status.success());
    match (deb, rpm) {
        (true, false) => Some(PackageFormat::Deb),
        (false, true) => Some(PackageFormat::Rpm),
        (true, true) => None,
        (false, false) => os_release_path_family(Path::new("/etc/os-release")),
    }
}

#[cfg(target_os = "linux")]
fn os_release_path_family(path: &Path) -> Option<PackageFormat> {
    use std::io::Read;
    const LIMIT: u64 = 16 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > LIMIT {
        return None;
    }
    std::str::from_utf8(&bytes).ok().and_then(os_release_family)
}

fn current_installer_target(platform: RuntimePlatform) -> Option<InstallerTarget> {
    #[cfg(target_os = "linux")]
    {
        return super::package::installed_package().or_else(|| {
            InstallerTarget::for_platform(platform, installed_linux_package_format()?)
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        InstallerTarget::default_for_non_linux(platform)
    }
}

pub fn detected_installer_target() -> Option<InstallerTarget> {
    RuntimePlatform::current().and_then(current_installer_target)
}

fn aligned_release_build(build: &MachineBuildInfo, binaries: &InstalledBinaries) -> bool {
    build.validate("webcodex-desktop").is_ok()
        && build.git_dirty == Some(false)
        && unified::stable_version(&build.version)
        && build.git_commit.as_deref().is_some_and(|v| {
            v.len() == 40
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        && binaries.bundled
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
    #[cfg(not(target_os = "linux"))]
    let _ = target;
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
        super::package::installed_package() == Some(target)
            && target.flavor.is_full()
            && executable == Path::new("/usr/lib/webcodex/webcodex-desktop")
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

fn assessed_environment(root: &Path, environment_id: &str) -> Option<crate::EnvironmentRecord> {
    let store = EnvironmentStore::open_existing(root.to_path_buf()).ok()??;
    let record = store.load_environment().ok()??;
    if !record.configured
        || record.environment_id != environment_id
        || record.request.account.identity != crate::current_account().ok()?.identity
    {
        return None;
    }
    Some(record)
}

/// Cheap installation classification, separate from Runtime readiness. A later
/// explicit Install rechecks the *published bytes* of this installed generation.
pub fn assess_installation(
    build: MachineBuildInfo,
    binaries: Option<InstalledBinaries>,
    bundled_source: bool,
    environment_id: Option<String>,
    environment_root: PathBuf,
    desktop: PathBuf,
) -> (InstallationKind, Option<InstallContext>) {
    let Some(target) = detected_installer_target() else {
        return (InstallationKind::UnsupportedPlatform, None);
    };
    if !target.flavor.is_full() {
        return (InstallationKind::UnmanagedInstallation, None);
    }
    if build.git_dirty != Some(false) || !bundled_source {
        return (InstallationKind::SourceBuild, None);
    }
    let Some(binaries) = binaries else {
        return (InstallationKind::UnmanagedInstallation, None);
    };
    if !aligned_release_build(&build, &binaries) {
        return (InstallationKind::SourceBuild, None);
    }
    let Some(desktop) = desktop.canonicalize().ok() else {
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
        let root = environment_root.clone();
        let record = assessed_environment(&root, &environment_id)?;
        for (stored, resolved, name) in [
            (
                &record.request.binaries.cli,
                &binaries.binaries.cli,
                "webcodex",
            ),
            (
                &record.request.binaries.server,
                &binaries.binaries.server,
                "webcodex-server",
            ),
            (
                &record.request.binaries.runner,
                &binaries.binaries.runner,
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
            environment_root: root,
            environment_id,
            binaries: record.request.binaries,
            desktop: Some(desktop),
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
    let root = context.environment_root.clone();
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
            != crate::current_account()
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
    let runtime = context
        .binaries
        .cli
        .parent()
        .ok_or(UpdateError::ProvenanceFailed)?;
    let valid_layout = if target.flavor.is_full() {
        context
            .desktop
            .as_ref()
            .is_some_and(|desktop| package_layout(target, desktop, runtime))
    } else {
        context.desktop.is_none()
            && super::package::installed_package() == Some(target)
            && super::package::owned_layout(target, &context.binaries)
    };
    if !valid_layout {
        return Err(UpdateError::ProvenanceFailed);
    }
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
        ("webcodex", &context.binaries.cli),
        ("webcodex-server", &context.binaries.server),
        ("webcodex-runner", &context.binaries.runner),
    ]
    .into_iter()
    .chain(context.desktop.as_ref().map(|p| ("webcodex-desktop", p)))
    {
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
        let mut build: MachineBuildInfo = serde_json::from_value(serde_json::json!({"schema_version":1,"binary":"webcodex-desktop","version":"1.2.3","git_commit":null,"git_dirty":true,"built_at":null,"target":"x86_64-unknown-linux-gnu","architecture":"x86_64","desktop_runtime_contract":{"min_generation":1,"max_generation":1}})).unwrap();
        let root = PathBuf::from("/not-an-environment");
        let desktop = PathBuf::from("/not-the-desktop");
        let (kind, context) = assess_installation(
            build.clone(),
            None,
            true,
            Some("environment".into()),
            root.clone(),
            desktop.clone(),
        );
        assert_eq!(kind, InstallationKind::SourceBuild);
        assert!(context.is_none());
        build.git_dirty = None;
        assert_eq!(
            assess_installation(build, None, true, None, root, desktop).0,
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

    #[test]
    fn installation_environment_assessment_never_creates_an_absent_root() {
        let directory = crate::test_tempdir().unwrap();
        let root = directory.path().join("absent-environment");
        assert!(assessed_environment(&root, "selected-environment").is_none());
        assert!(!root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn installation_environment_assessment_preserves_existing_permissions_and_mtime() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let directory = crate::test_tempdir().unwrap();
        let root = directory.path().join("environment");
        let store = EnvironmentStore::open(root.clone()).unwrap();
        let record = crate::EnvironmentRecord {
            schema_version: 1,
            environment_id: "selected-environment".into(),
            request: crate::SetupRequest {
                service_scope: crate::service::ServiceScope::System,
                mode: crate::EnvironmentMode::Join,
                server_url: "http://127.0.0.1:1".into(),
                project: None,
                runner: None,
                runner_display_name: None,
                account: crate::current_account().unwrap(),
                binaries: RuntimeBinaries {
                    cli: root.join("webcodex"),
                    server: root.join("webcodex-server"),
                    runner: root.join("webcodex-runner"),
                },
            },
            username: None,
            runner_client_id: None,
            projects: vec![],
            configured: true,
        };
        store.save_environment(&record).unwrap();
        let state = root.join("environment.json");
        let metadata = std::fs::metadata(&root).unwrap();
        let state_mtime = std::fs::metadata(&state).unwrap().modified().unwrap();
        assert!(assessed_environment(&root, &record.environment_id).is_some());
        assert!(assessed_environment(&root, "stale-selection").is_none());
        let after = std::fs::metadata(&root).unwrap();
        assert_eq!(after.mode(), metadata.mode());
        assert_eq!(after.modified().unwrap(), metadata.modified().unwrap());
        assert_eq!(
            std::fs::metadata(&state).unwrap().modified().unwrap(),
            state_mtime
        );
        assert!(!root.join("setup.lock").exists());
        // An unsafe root is rejected as it stands, without repairing its ACL.
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
        let metadata = std::fs::metadata(&root).unwrap();
        assert!(assessed_environment(&root, &record.environment_id).is_none());
        let after = std::fs::metadata(&root).unwrap();
        assert_eq!(after.mode(), metadata.mode());
        assert_eq!(after.modified().unwrap(), metadata.modified().unwrap());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn os_release_file_bytes_are_bounded_before_classification() {
        let directory = crate::test_tempdir().unwrap();
        let file = directory.path().join("os-release");
        std::fs::write(&file, b"ID=ubuntu\n").unwrap();
        assert_eq!(os_release_path_family(&file), Some(PackageFormat::Deb));
        let mut oversized = b"ID=ubuntu\n".to_vec();
        oversized.resize(16 * 1024 + 1, b' ');
        std::fs::write(&file, oversized).unwrap();
        assert_eq!(os_release_path_family(&file), None);
    }
}

pub fn installed_desktop_path(target: InstallerTarget) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let _ = target;
        Some(PathBuf::from("/usr/lib/webcodex/webcodex-desktop"))
    }
    #[cfg(target_os = "macos")]
    {
        let _ = target;
        Some(PathBuf::from(
            "/Applications/WebCodex Desktop.app/Contents/MacOS/WebCodex",
        ))
    }
    #[cfg(windows)]
    {
        use winreg::{enums::HKEY_CURRENT_USER, RegKey};
        let _ = target;
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\WebCodex Desktop",
            )
            .and_then(|key| key.get_value::<String, _>("InstallLocation"))
            .ok()
            .and_then(|root| PathBuf::from(root.trim_matches('"')).canonicalize().ok())
            .map(|root| root.join("WebCodex.exe"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
    {
        let _ = target;
        None
    }
}

/// Runs only the metadata exit path (Desktop exits before Tauri initialization).
/// Caller must select an installed program from the authoritative layout.
pub async fn probe_installed_build(path: &Path, binary: &str) -> UpdateResult<MachineBuildInfo> {
    use tokio::io::AsyncReadExt;
    if !path.is_absolute()
        || !std::fs::metadata(path)
            .is_ok_and(|m| m.is_file() && m.len() > 0 && m.len() <= unified::MAX_INSTALLER_BYTES)
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    let mut command = tokio::process::Command::new(path);
    command
        .arg("--build-info-json")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command.spawn().map_err(|_| UpdateError::ProvenanceFailed)?;
    #[cfg(unix)]
    let process_group = child.id();
    let stdout = child.stdout.take().ok_or(UpdateError::ProvenanceFailed)?;
    let probe = async {
        let mut bytes = Vec::new();
        stdout
            .take(16385)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| UpdateError::ProvenanceFailed)?;
        if bytes.len() > 16384 {
            return Err(UpdateError::ProvenanceFailed);
        }
        if !child
            .wait()
            .await
            .map_err(|_| UpdateError::ProvenanceFailed)?
            .success()
        {
            return Err(UpdateError::ProvenanceFailed);
        }
        let info: MachineBuildInfo =
            serde_json::from_slice(&bytes).map_err(|_| UpdateError::ProvenanceFailed)?;
        info.validate(binary)
            .map_err(|_| UpdateError::ProvenanceFailed)?;
        Ok(info)
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), probe).await;
    match result {
        Ok(Ok(info)) => Ok(info),
        _ => {
            #[cfg(unix)]
            if let Some(pid) = process_group {
                unsafe {
                    libc::kill(-(pid as i32), libc::SIGKILL);
                }
            }
            let _ = child.start_kill();
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), child.wait()).await;
            Err(UpdateError::ProvenanceFailed)
        }
    }
}

pub async fn assess_headless_installation(
    root: PathBuf,
) -> UpdateResult<(
    InstallationKind,
    Option<InstallContext>,
    Vec<MachineBuildInfo>,
)> {
    let Some(target) = detected_installer_target() else {
        return Ok((InstallationKind::UnsupportedPlatform, None, vec![]));
    };
    // This is a read-only open; missing environment remains missing.
    let store = EnvironmentStore::open_existing(root.clone())
        .map_err(|_| UpdateError::UpgradePreflightFailed)?;
    let Some(store) = store else {
        return Ok((InstallationKind::EnvironmentNotConfigured, None, vec![]));
    };
    let Some(record) = store
        .load_environment()
        .map_err(|_| UpdateError::UpgradePreflightFailed)?
    else {
        return Ok((InstallationKind::EnvironmentNotConfigured, None, vec![]));
    };
    if !record.configured
        || record.request.account.identity
            != crate::current_account()
                .map_err(|_| UpdateError::UpgradePreflightFailed)?
                .identity
    {
        return Ok((InstallationKind::EnvironmentNotConfigured, None, vec![]));
    }
    if !target.flavor.is_full() {
        return assess_runtime_installation(root, record, target).await;
    }
    let desktop = installed_desktop_path(target).ok_or(UpdateError::UnsupportedPlatform)?;
    let runtime = record
        .request
        .binaries
        .cli
        .parent()
        .ok_or(UpdateError::UpgradePreflightFailed)?
        .to_path_buf();
    if !package_layout(target, &desktop, &runtime) {
        return Ok((InstallationKind::UnmanagedInstallation, None, vec![]));
    }
    let mut builds = Vec::new();
    for (name, path) in [
        ("webcodex", &record.request.binaries.cli),
        ("webcodex-server", &record.request.binaries.server),
        ("webcodex-runner", &record.request.binaries.runner),
        ("webcodex-desktop", &desktop),
    ] {
        let expected = if name == "webcodex-desktop" {
            desktop.clone()
        } else {
            runtime.join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
        };
        if path.canonicalize().ok().as_ref() != Some(&expected) {
            return Ok((InstallationKind::UnmanagedInstallation, None, builds));
        }
        match probe_installed_build(path, name).await {
            Ok(build) => builds.push(build),
            Err(_) => return Ok((InstallationKind::UnmanagedInstallation, None, builds)),
        }
    }
    let binaries = InstalledBinaries {
        directory: runtime,
        binaries: record.request.binaries,
        builds: builds[..3].to_vec(),
        bundled: true,
    };
    let (kind, context) = assess_installation(
        builds[3].clone(),
        Some(binaries),
        true,
        Some(record.environment_id),
        root,
        desktop,
    );
    Ok((kind, context, builds))
}

pub(super) async fn verify_observed_generation(
    store: &EnvironmentStore,
    observed: &crate::UpgradeObservation,
    target: InstallerTarget,
    cached_source: Option<&unified::UpdateSource>,
) -> UpdateResult<MachineBuildInfo> {
    let (kind, context, _) = assess_headless_installation(store.root().to_path_buf()).await?;
    let context = context
        .filter(|context| {
            kind == InstallationKind::Managed
                && context.target == target
                && context.target.flavor == observed.package_flavor
                && context.environment_id == observed.environment_id
                && context.build.version == observed.version
                && context.build.git_commit.as_deref() == Some(observed.source_sha.as_str())
        })
        .ok_or(UpdateError::ProvenanceFailed)?;
    let published;
    let source = match cached_source {
        Some(source) => source,
        None => {
            published = unified::fetch_release(&observed.version, target)
                .await?
                .ok_or(UpdateError::ProvenanceFailed)?;
            &published.source
        }
    };
    if source.flavor != observed.package_flavor
        || source.flavor != target.flavor
        || source.manifest_sha256 != observed.manifest_sha256
        || source.source_sha != observed.source_sha
    {
        return Err(UpdateError::ProvenanceFailed);
    }
    #[cfg(unix)]
    unified::verify_installed_update_cli(&context.binaries.cli)?;
    for (name, path) in [
        ("webcodex", &context.binaries.cli),
        ("webcodex-server", &context.binaries.server),
        ("webcodex-runner", &context.binaries.runner),
    ]
    .into_iter()
    .chain(context.desktop.as_ref().map(|p| ("webcodex-desktop", p)))
    {
        if source.component_sha256(name) != Some(hash_program(path).await?.as_str()) {
            return Err(UpdateError::ProvenanceFailed);
        }
    }
    Ok(context.build)
}

#[cfg(target_os = "linux")]
async fn assess_runtime_installation(
    root: PathBuf,
    record: crate::EnvironmentRecord,
    target: InstallerTarget,
) -> UpdateResult<(
    InstallationKind,
    Option<InstallContext>,
    Vec<MachineBuildInfo>,
)> {
    use std::io::Read;
    if super::package::installed_package() != Some(target)
        || !super::package::owned_layout(target, &record.request.binaries)
    {
        return Ok((InstallationKind::UnmanagedInstallation, None, vec![]));
    }
    let mut builds = Vec::new();
    for (name, path) in [
        ("webcodex", &record.request.binaries.cli),
        ("webcodex-server", &record.request.binaries.server),
        ("webcodex-runner", &record.request.binaries.runner),
    ] {
        match probe_installed_build(path, name).await {
            Ok(info) => builds.push(info),
            Err(_) => return Ok((InstallationKind::UnmanagedInstallation, None, builds)),
        }
    }
    let path = super::package::provenance_path(target).ok_or(UpdateError::ProvenanceFailed)?;
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| UpdateError::ProvenanceFailed)?
        .take(unified::MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| UpdateError::ProvenanceFailed)?;
    let source = unified::verify_source_manifest_for_flavor(
        &bytes,
        &builds[0].version,
        target.platform,
        target.flavor,
    )?;
    if builds
        .iter()
        .any(|build| source.component_build(&build.binary) != Some(build))
    {
        return Ok((InstallationKind::UnmanagedInstallation, None, builds));
    }
    let context = InstallContext {
        environment_root: root,
        environment_id: record.environment_id,
        binaries: record.request.binaries,
        desktop: None,
        build: builds[0].clone(),
        target,
    };
    Ok((InstallationKind::Managed, Some(context), builds))
}
#[cfg(not(target_os = "linux"))]
async fn assess_runtime_installation(
    _: PathBuf,
    _: crate::EnvironmentRecord,
    _: InstallerTarget,
) -> UpdateResult<(
    InstallationKind,
    Option<InstallContext>,
    Vec<MachineBuildInfo>,
)> {
    Err(UpdateError::UnsupportedPlatform)
}
