use crate::service::ServiceScope;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathStatus {
    Present,
    Missing,
    Unreadable,
    UnsafePath,
    Invalid,
    Unconfirmed,
    NotApplicable,
    NotConfigured,
    Remote,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    File,
    Directory,
    SystemLog,
    InMemory,
    RemoteReference,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafetyCategory {
    Metadata,
    MixedConfiguration,
    Secret,
    Data,
    Log,
    Cache,
    Binary,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogSourceKind {
    SystemdJournal,
    WindowsEvents,
    TaskScheduler,
    LifecycleFile,
    InMemory,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogSource {
    pub kind: LogSourceKind,
    pub unit_name: Option<String>,
    pub service_scope: Option<ServiceScope>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathEntry {
    pub id: String,
    pub component: String,
    pub purpose: String,
    pub source: String,
    pub configured_path: Option<PathBuf>,
    pub canonical_path: Option<PathBuf>,
    pub status: PathStatus,
    pub kind: PathKind,
    pub category: SafetyCategory,
    pub directory_to_open: Option<PathBuf>,
    pub log_source: Option<LogSource>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InventoryIssue {
    pub code: String,
    pub entry_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafeBuild {
    pub binary: String,
    pub version: Option<String>,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub target: Option<String>,
    pub architecture: Option<String>,
    pub desktop_runtime_contract: webcodex_core::desktop_runtime_contract::DesktopRuntimeContract,
    pub environment_data_format: Option<u16>,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildSource {
    EntryPoint,
    PreviouslyVerified,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildObservation {
    pub source: BuildSource,
    pub build: SafeBuild,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IdentityObservation {
    pub kind: String,
    pub value: String,
    pub source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathInventory {
    /// Allowlisted values captured by the same bounded Runner configuration read.
    /// Only settings export emits them; they do not affect the path revision.
    #[serde(skip)]
    pub settings: super::SettingsObservation,
    pub schema_version: u16,
    pub observed_at_ms: u64,
    pub environment_id: Option<String>,
    pub local_server: Option<bool>,
    pub local_runner: Option<bool>,
    pub service_scope: Option<ServiceScope>,
    pub roots: Vec<PathEntry>,
    pub revision: String,
    pub entries: Vec<PathEntry>,
    pub issues: Vec<InventoryIssue>,
    pub builds: Vec<BuildObservation>,
    pub identities: Vec<IdentityObservation>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestProjection {
    pub category: String,
    pub description: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestExclusion {
    pub category: SafetyCategory,
    pub projection: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RestoreMaterial {
    pub id: String,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupManifest {
    pub schema_version: u16,
    pub kind: String,
    pub does_not_contain_files: bool,
    pub cannot_restore: bool,
    pub privacy_notice: String,
    pub inventory: PathInventory,
    pub exclusions: Vec<ManifestExclusion>,
    pub projections: Vec<ManifestProjection>,
    pub required_restore_materials: Vec<RestoreMaterial>,
}

pub fn safe_build(info: &webcodex_core::desktop_runtime_contract::MachineBuildInfo) -> SafeBuild {
    let version = semver::Version::parse(&info.version)
        .ok()
        .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch));
    let git_commit = info
        .git_commit
        .as_ref()
        .filter(|v| (7..=64).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_hexdigit()))
        .cloned();
    let target = (info.target.len() <= 128
        && info
            .target
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-+.".contains(&b))
        && ["apple-darwin", "windows", "linux"]
            .iter()
            .any(|os| info.target.contains(os)))
    .then(|| info.target.clone());
    let architecture = matches!(
        info.architecture.as_str(),
        "aarch64" | "x86_64" | "x86" | "arm" | "arm64" | "riscv64"
    )
    .then(|| info.architecture.clone());
    SafeBuild {
        binary: match info.binary.as_str() {
            "webcodex" | "webcodex-server" | "webcodex-runner" | "webcodex-desktop" => {
                info.binary.clone()
            }
            _ => "unknown".into(),
        },
        version,
        git_commit,
        git_dirty: info.git_dirty,
        target,
        architecture,
        desktop_runtime_contract: info.desktop_runtime_contract,
        environment_data_format: info.environment_data_format,
    }
}
