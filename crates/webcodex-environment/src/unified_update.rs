//! Stable release discovery, verified private downloads, update observation and
//! installer admission shared by Desktop and CLI. Native launch UI lives in host
//! adapters; the privileged handoff and Core transaction retain their authority.
mod cache;
mod cancellation;
#[path = "unified_update/data_dir.rs"]
pub mod desktop_data_dir;
pub mod discovery;
mod download;
pub(crate) mod install;
mod view;
mod windows_handoff;
pub use cancellation::CancellationSignal;
pub use desktop_data_dir::{default_desktop_data_dir, DESKTOP_DATA_DIR_ENV};
pub use discovery::{
    fetch_latest, is_newer_stable, valid_notice, ReleaseNotice, UpdateCompatibility,
};
pub use download::{DownloadPhase, DownloadStatus, InstallationKind, UpdateManager};
pub use install::context::{
    assess_headless_installation, installed_desktop_path, probe_installed_build,
};
pub use install::{
    assess_installation, detected_installer_target, InstallContext, InstalledBinaries,
    LaunchAdapter, LaunchOutcome, LaunchRequest,
};
pub use view::*;
pub use windows_handoff::{WindowsHandoff, WINDOWS_GUARDED_HANDOFF_VERSION};
/// Raw, additive CLI build-info attestation for its same-source Windows outer bootstrap.
pub const WINDOWS_GUARDED_BOOTSTRAP_BUILD_INFO_FIELD: &str = "windows_guarded_bootstrap_contract";
pub const CHECK_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
#[cfg(unix)]
mod installer;
mod network;
mod source;
mod strict_json;

pub use cache::PrivateUpdateCache;
#[cfg(unix)]
pub use installer::{apply_verified_installer, verify_installed_update_cli, InstallerLaunchNotice};
pub use network::{fetch_release, http_client, read_response, ReleaseArtifacts};
pub use source::{verify_source_manifest, verify_source_manifest_for_flavor, UpdateSource};
pub(crate) use strict_json::{deserialize_unique, parse as strict_json};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const OFFICIAL_REPOSITORY: &str = "yyjeqhc/webcodex";
pub const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
pub const MAX_SOURCE_BYTES: u64 = 1024 * 1024;
// Matches the retained release bundle's per-member ceiling.
pub const MAX_INSTALLER_BYTES: u64 = 768 * 1024 * 1024;
pub const RUNTIME_BINARIES: [&str; 3] = ["webcodex", "webcodex-server", "webcodex-runner"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateError {
    NetworkUnavailable,
    ManifestMissing,
    ManifestInvalid,
    UnsupportedPlatform,
    DownloadFailed,
    DownloadTooLarge,
    ChecksumMismatch,
    SourceManifestInvalid,
    ProvenanceFailed,
    CacheUnavailable,
    Cancelled,
    UpgradePreflightFailed,
    AuthorizationRequired,
    InstallerLaunchFailed,
    GuardedHandoffUnavailable,
    UpgradeRolledBack,
    RecoveryRequired,
}

pub type UpdateResult<T> = Result<T, UpdateError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RuntimePlatform {
    #[serde(rename = "linux-x64")]
    LinuxX64,
    #[serde(rename = "linux-arm64")]
    LinuxArm64,
    #[serde(rename = "darwin-x64")]
    DarwinX64,
    #[serde(rename = "darwin-arm64")]
    DarwinArm64,
    #[serde(rename = "win32-x64")]
    Win32X64,
    #[serde(rename = "win32-arm64")]
    Win32Arm64,
}

impl RuntimePlatform {
    pub const ALL: [Self; 6] = [
        Self::LinuxX64,
        Self::LinuxArm64,
        Self::DarwinX64,
        Self::DarwinArm64,
        Self::Win32X64,
        Self::Win32Arm64,
    ];

    pub fn current() -> Option<Self> {
        Self::from_native(std::env::consts::OS, std::env::consts::ARCH)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxX64 => "linux-x64",
            Self::LinuxArm64 => "linux-arm64",
            Self::DarwinX64 => "darwin-x64",
            Self::DarwinArm64 => "darwin-arm64",
            Self::Win32X64 => "win32-x64",
            Self::Win32Arm64 => "win32-arm64",
        }
    }

    pub fn from_native(os: &str, arch: &str) -> Option<Self> {
        match (os, arch) {
            ("linux", "x86_64") => Some(Self::LinuxX64),
            ("linux", "aarch64") => Some(Self::LinuxArm64),
            ("macos", "x86_64") => Some(Self::DarwinX64),
            ("macos", "aarch64") => Some(Self::DarwinArm64),
            ("windows", "x86_64") => Some(Self::Win32X64),
            ("windows", "aarch64") => Some(Self::Win32Arm64),
            _ => None,
        }
    }

    pub const fn architecture(self) -> &'static str {
        match self {
            Self::LinuxX64 | Self::DarwinX64 | Self::Win32X64 => "x86_64",
            _ => "aarch64",
        }
    }

    pub const fn target(self) -> &'static str {
        match self {
            Self::LinuxX64 => "x86_64-unknown-linux-gnu",
            Self::LinuxArm64 => "aarch64-unknown-linux-gnu",
            Self::DarwinX64 => "x86_64-apple-darwin",
            Self::DarwinArm64 => "aarch64-apple-darwin",
            Self::Win32X64 => "x86_64-pc-windows-msvc",
            Self::Win32Arm64 => "aarch64-pc-windows-msvc",
        }
    }

    pub fn source_filename(self, version: &str) -> String {
        format!("webcodex-source-v{version}-{}.json", self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageFormat {
    Deb,
    Rpm,
    Pkg,
    Exe,
}

impl PackageFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Pkg => "pkg",
            Self::Exe => "exe",
        }
    }
}

/// Installed package composition; never an Environment role. Legacy wire targets
/// omit this field and retain the exact Full contract.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageFlavor {
    #[default]
    Full,
    Runtime,
}
impl PackageFlavor {
    pub const fn is_full(&self) -> bool {
        matches!(self, Self::Full)
    }
    pub const fn components(self) -> &'static [&'static str] {
        match self {
            Self::Full => &[
                "webcodex",
                "webcodex-server",
                "webcodex-runner",
                "webcodex-desktop",
            ],
            Self::Runtime => &RUNTIME_BINARIES,
        }
    }
    pub fn source_filename(self, platform: RuntimePlatform, version: &str) -> String {
        match self {
            Self::Full => platform.source_filename(version),
            Self::Runtime => format!(
                "webcodex-runtime-source-v{version}-{}.json",
                platform.as_str()
            ),
        }
    }
    pub const fn source_schema(self) -> u16 {
        match self {
            Self::Full => 1,
            Self::Runtime => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallerTarget {
    #[serde(default, skip_serializing_if = "PackageFlavor::is_full")]
    pub flavor: PackageFlavor,
    pub platform: RuntimePlatform,
    pub format: PackageFormat,
}

impl InstallerTarget {
    pub const ALL: [Self; 8] = [
        Self::new(RuntimePlatform::LinuxX64, PackageFormat::Deb),
        Self::new(RuntimePlatform::LinuxX64, PackageFormat::Rpm),
        Self::new(RuntimePlatform::LinuxArm64, PackageFormat::Deb),
        Self::new(RuntimePlatform::LinuxArm64, PackageFormat::Rpm),
        Self::new(RuntimePlatform::DarwinX64, PackageFormat::Pkg),
        Self::new(RuntimePlatform::DarwinArm64, PackageFormat::Pkg),
        Self::new(RuntimePlatform::Win32X64, PackageFormat::Exe),
        Self::new(RuntimePlatform::Win32Arm64, PackageFormat::Exe),
    ];

    pub const fn new(platform: RuntimePlatform, format: PackageFormat) -> Self {
        Self {
            platform,
            format,
            flavor: PackageFlavor::Full,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .chain(Self::RUNTIME)
            .find(|target| target.as_str() == value)
    }

    pub const fn for_platform(platform: RuntimePlatform, format: PackageFormat) -> Option<Self> {
        let target = Self::new(platform, format);
        if target.valid() {
            Some(target)
        } else {
            None
        }
    }

    pub const fn default_for_non_linux(platform: RuntimePlatform) -> Option<Self> {
        match platform {
            RuntimePlatform::DarwinX64 | RuntimePlatform::DarwinArm64 => {
                Self::for_platform(platform, PackageFormat::Pkg)
            }
            RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64 => {
                Self::for_platform(platform, PackageFormat::Exe)
            }
            _ => None,
        }
    }

    pub const RUNTIME: [Self; 4] = [
        Self::runtime(RuntimePlatform::LinuxX64, PackageFormat::Deb),
        Self::runtime(RuntimePlatform::LinuxX64, PackageFormat::Rpm),
        Self::runtime(RuntimePlatform::LinuxArm64, PackageFormat::Deb),
        Self::runtime(RuntimePlatform::LinuxArm64, PackageFormat::Rpm),
    ];
    pub const fn runtime(platform: RuntimePlatform, format: PackageFormat) -> Self {
        Self {
            platform,
            format,
            flavor: PackageFlavor::Runtime,
        }
    }
    pub fn as_str(self) -> String {
        let suffix = if self.flavor.is_full() {
            ""
        } else {
            "-runtime"
        };
        format!(
            "{}{suffix}-{}",
            self.platform.as_str(),
            self.format.extension()
        )
    }
    pub fn source_filename(self, version: &str) -> String {
        self.flavor.source_filename(self.platform, version)
    }

    pub fn installer_filename(self, version: &str) -> String {
        format!(
            "webcodex-{}-v{version}-{}.{}",
            if self.flavor.is_full() {
                "unified"
            } else {
                "runtime"
            },
            self.platform.as_str(),
            self.format.extension()
        )
    }

    pub const fn valid(self) -> bool {
        if !self.flavor.is_full() {
            return matches!(
                (self.platform, self.format),
                (
                    RuntimePlatform::LinuxX64 | RuntimePlatform::LinuxArm64,
                    PackageFormat::Deb | PackageFormat::Rpm
                )
            );
        }
        matches!(
            (self.platform, self.format),
            (
                RuntimePlatform::LinuxX64 | RuntimePlatform::LinuxArm64,
                PackageFormat::Deb | PackageFormat::Rpm
            ) | (
                RuntimePlatform::DarwinX64 | RuntimePlatform::DarwinArm64,
                PackageFormat::Pkg
            ) | (
                RuntimePlatform::Win32X64 | RuntimePlatform::Win32Arm64,
                PackageFormat::Exe
            )
        )
    }
}

/// Canonical SemVer only; a cache entry cannot contribute a path separator/tag.
pub fn stable_version(value: &str) -> bool {
    value.len() <= 96
        && semver::Version::parse(value).is_ok_and(|v| v.pre.is_empty() && v.to_string() == value)
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn valid_sha256(value: &str) -> bool {
    lowercase_hex(value, 64)
}

pub(crate) fn lowercase_hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub fn release_asset_url(version: &str, filename: &str) -> UpdateResult<String> {
    if !stable_version(version)
        || filename.is_empty()
        || filename.len() > 192
        || filename
            .bytes()
            .any(|b| !b.is_ascii_alphanumeric() && !b"._+-".contains(&b))
        || filename == "."
        || filename == ".."
    {
        return Err(UpdateError::ManifestInvalid);
    }
    Ok(format!(
        "https://github.com/{OFFICIAL_REPOSITORY}/releases/download/v{version}/{filename}"
    ))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeArtifact {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallerEntry {
    #[serde(default, skip_serializing_if = "PackageFlavor::is_full")]
    pub flavor: PackageFlavor,
    pub platform: RuntimePlatform,
    pub format: PackageFormat,
    pub filename: String,
    pub url: String,
    pub sha256: String,
    pub source_manifest_url: String,
    pub source_manifest_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnifiedInstallerManifest {
    #[serde(skip)]
    pub catalog_version: u16,
    pub version: String,
    pub binaries: Vec<String>,
    pub artifacts: BTreeMap<RuntimePlatform, RuntimeArtifact>,
    pub installers: BTreeMap<String, InstallerEntry>,
}
impl UnifiedInstallerManifest {
    pub fn parse(bytes: &[u8], version: &str) -> UpdateResult<Self> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(UpdateError::ManifestInvalid);
        }
        let value = strict_json(bytes).map_err(|_| UpdateError::ManifestInvalid)?;
        Self::parse_value(value, version, 1)
    }
    pub fn parse_v2(bytes: &[u8], version: &str) -> UpdateResult<Self> {
        if bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err(UpdateError::ManifestInvalid);
        }
        let mut value = strict_json(bytes).map_err(|_| UpdateError::ManifestInvalid)?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
        {
            return Err(UpdateError::ManifestInvalid);
        }
        value
            .as_object_mut()
            .ok_or(UpdateError::ManifestInvalid)?
            .remove("schema_version");
        Self::parse_value(value, version, 2)
    }
    /// Deterministic compatibility projection; v2 remains the only catalog
    /// authority. The generated legacy view contains the unchanged Full set.
    pub(crate) fn full_compatibility(&self) -> Self {
        let mut view = self.clone();
        view.catalog_version = 1;
        view.installers.retain(|_, entry| entry.flavor.is_full());
        view
    }
    fn parse_value(
        value: serde_json::Value,
        version: &str,
        catalog_version: u16,
    ) -> UpdateResult<Self> {
        for target in InstallerTarget::ALL {
            if value
                .get("installers")
                .and_then(|entries| entries.get(target.as_str()))
                .and_then(|entry| entry.get("flavor"))
                .is_some()
            {
                return Err(UpdateError::ManifestInvalid);
            }
        }
        let mut manifest: Self =
            serde_json::from_value(value).map_err(|_| UpdateError::ManifestInvalid)?;
        manifest.catalog_version = catalog_version;
        let targets: Vec<_> = InstallerTarget::ALL
            .into_iter()
            .chain(
                InstallerTarget::RUNTIME
                    .into_iter()
                    .filter(|_| catalog_version == 2),
            )
            .collect();
        if !stable_version(version)
            || manifest.version != version
            || manifest.binaries != RUNTIME_BINARIES
            || manifest.artifacts.len() != RuntimePlatform::ALL.len()
            || manifest.installers.len() != targets.len()
        {
            return Err(UpdateError::ManifestInvalid);
        }
        for platform in RuntimePlatform::ALL {
            let runtime = manifest
                .artifacts
                .get(&platform)
                .ok_or(UpdateError::ManifestInvalid)?;
            if runtime.url
                != release_asset_url(
                    version,
                    &format!("webcodex-v{version}-{}.tar.gz", platform.as_str()),
                )?
                || !valid_sha256(&runtime.sha256)
            {
                return Err(UpdateError::ManifestInvalid);
            }
        }
        for &target in &targets {
            let key = target.as_str();
            let entry = manifest
                .installers
                .get(&key)
                .ok_or(UpdateError::ManifestInvalid)?;
            let filename = target.installer_filename(version);
            if !target.valid()
                || entry.flavor != target.flavor
                || entry.platform != target.platform
                || entry.format != target.format
                || entry.filename != filename
                || entry.url != release_asset_url(version, &filename)?
                || entry.source_manifest_url
                    != release_asset_url(version, &target.source_filename(version))?
                || !valid_sha256(&entry.sha256)
                || !valid_sha256(&entry.source_manifest_sha256)
            {
                return Err(UpdateError::ManifestInvalid);
            }
        }
        for (platform, flavor) in RuntimePlatform::ALL
            .into_iter()
            .map(|p| (p, PackageFlavor::Full))
            .chain(
                [RuntimePlatform::LinuxX64, RuntimePlatform::LinuxArm64]
                    .into_iter()
                    .filter(|_| catalog_version == 2)
                    .map(|p| (p, PackageFlavor::Runtime)),
            )
        {
            let mut source_sha256 = None;
            for &target in targets
                .iter()
                .filter(|target| target.platform == platform && target.flavor == flavor)
            {
                let digest = manifest.entry(target)?.source_manifest_sha256.as_str();
                if source_sha256.is_some_and(|expected| expected != digest) {
                    return Err(UpdateError::ManifestInvalid);
                }
                source_sha256 = Some(digest);
            }
            if source_sha256.is_none() {
                return Err(UpdateError::ManifestInvalid);
            }
        }
        Ok(manifest)
    }

    pub fn entry(&self, target: InstallerTarget) -> UpdateResult<&InstallerEntry> {
        self.installers
            .get(&target.as_str())
            .ok_or(UpdateError::ManifestInvalid)
    }
}

/// Initial and redirected URLs share the same no-credentials HTTPS boundary.
pub fn trusted_download_url(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
        && matches!(
            url.host_str(),
            Some(
                "api.github.com"
                    | "github.com"
                    | "release-assets.githubusercontent.com"
                    | "objects.githubusercontent.com"
            )
        )
}

#[cfg(test)]
pub(crate) mod tests;
