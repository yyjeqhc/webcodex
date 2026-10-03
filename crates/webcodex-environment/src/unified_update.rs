//! The existing six-platform release/download manifest, shared by Desktop and
//! the privileged unified-installer handoff. No Desktop-only update authority.
mod cache;
#[cfg(unix)]
mod installer;
mod network;
mod source;
mod strict_json;

pub use cache::{PrivateUpdateCache, UpdateCacheLock};
#[cfg(unix)]
pub use installer::{apply_verified_installer, verify_installed_update_cli, InstallerLaunchNotice};
pub use network::{fetch_release, http_client, read_response, ReleaseArtifacts};
pub use source::{verify_source_manifest, UpdateSource};
pub(crate) use strict_json::parse as strict_json;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallerTarget {
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
        Self { platform, format }
    }

    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
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

    pub fn as_str(self) -> String {
        format!("{}-{}", self.platform.as_str(), self.format.extension())
    }

    pub fn installer_filename(self, version: &str) -> String {
        format!(
            "webcodex-unified-v{version}-{}.{}",
            self.platform.as_str(),
            self.format.extension()
        )
    }

    pub const fn valid(self) -> bool {
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
        let manifest: Self =
            serde_json::from_value(value).map_err(|_| UpdateError::ManifestInvalid)?;
        if !stable_version(version)
            || manifest.version != version
            || manifest.binaries != RUNTIME_BINARIES
            || manifest.artifacts.len() != RuntimePlatform::ALL.len()
            || manifest.installers.len() != InstallerTarget::ALL.len()
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
        for target in InstallerTarget::ALL {
            let key = target.as_str();
            let entry = manifest
                .installers
                .get(&key)
                .ok_or(UpdateError::ManifestInvalid)?;
            let filename = target.installer_filename(version);
            if !target.valid()
                || entry.platform != target.platform
                || entry.format != target.format
                || entry.filename != filename
                || entry.url != release_asset_url(version, &filename)?
                || entry.source_manifest_url
                    != release_asset_url(version, &target.platform.source_filename(version))?
                || !valid_sha256(&entry.sha256)
                || !valid_sha256(&entry.source_manifest_sha256)
            {
                return Err(UpdateError::ManifestInvalid);
            }
        }
        for platform in RuntimePlatform::ALL {
            let mut source_sha256 = None;
            for target in InstallerTarget::ALL
                .into_iter()
                .filter(|target| target.platform == platform)
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
mod tests;
