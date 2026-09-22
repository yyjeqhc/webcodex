//! Explicit Runtime source selection and bounded candidate inspection. A probe
//! never changes the running processes, credentials or persisted selection.
pub(crate) mod lifecycle;

use crate::deadline::Deadline;
use crate::error::{DesktopError, DesktopResult};
use crate::operation::CancellationContext;
use crate::webcodex::cli::{
    run_json_until, CliCommandContext, ResolvedBinaries, ResolvedBinarySource,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use webcodex_core::desktop_runtime_contract::{
    build_alignment, BuildAlignment, MachineBuildInfo, ProtocolCompatibility,
    DESKTOP_RUNTIME_CONTRACT,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RuntimeSource {
    #[default]
    Bundled,
    Custom {
        directory: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BinaryProbe {
    pub name: String,
    pub present: bool,
    pub executable: bool,
    pub metadata: Option<MachineBuildInfo>,
    pub sha256: Option<String>,
    pub error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeCandidate {
    pub candidate_id: String,
    pub source: RuntimeSource,
    pub selection_revision: u64,
    pub checked_at_ms: u64,
    pub directory: Option<PathBuf>,
    pub binaries: Vec<BinaryProbe>,
    pub compatibility: ProtocolCompatibility,
    pub build_alignment: BuildAlignment,
    pub advisories: Vec<String>,
    pub error_code: Option<String>,
    pub fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSettings {
    pub source: RuntimeSource,
    pub selection_revision: u64,
    pub desktop_contract: webcodex_core::desktop_runtime_contract::DesktopRuntimeContract,
    pub selected: Option<RuntimeCandidate>,
    pub candidate: Option<RuntimeCandidate>,
    pub previous_source: Option<RuntimeSource>,
    pub last_switch: Option<RuntimeSwitchResult>,
    pub unavailable_code: Option<String>,
    pub active_jobs: Option<u64>,
    pub can_switch: bool,
    pub switch_unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSwitchResult {
    pub outcome: String,
    pub reason_code: Option<String>,
    pub rollback_reason_code: Option<String>,
    pub selection_revision: u64,
    pub restart_required: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeSwitchRequest {
    pub candidate_id: String,
    pub expected_selection_revision: u64,
    pub confirm_interrupt: bool,
}

pub fn error(code: &str) -> DesktopError {
    DesktopError::new(code, "The selected Runtime could not be verified or activated", "Inspect Runtime diagnostics; select a compatible Runtime or explicitly use bundled Runtime.")
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn source_directory(
    source: &RuntimeSource,
    bundled: Option<&Path>,
) -> DesktopResult<(PathBuf, ResolvedBinarySource)> {
    match source {
        RuntimeSource::Custom { directory } => {
            if !directory.is_absolute() || directory.as_os_str().len() > 4096 {
                return Err(error("runtime_directory_invalid"));
            }
            Ok((directory.clone(), ResolvedBinarySource::Custom))
        }
        RuntimeSource::Bundled => {
            if let Some(path) = bundled.filter(|p| p.is_dir()) {
                return Ok((path.into(), ResolvedBinarySource::Bundled));
            }
            if !cfg!(debug_assertions) {
                return Err(error("bundled_runtime_missing"));
            }
            // Development-only fallback. A persisted Custom always takes priority,
            // and a release install never searches environment variables or PATH.
            if let Some(value) = std::env::var_os("WEBCODEX_DESKTOP_BIN_DIR") {
                if value.is_empty() {
                    return Err(error("runtime_directory_invalid"));
                }
                return Ok((PathBuf::from(value), ResolvedBinarySource::Environment));
            }
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .ok_or_else(|| error("runtime_directory_invalid"))?;
            Ok((
                root.join("target/dogfood"),
                ResolvedBinarySource::SourceDogfoodTarget,
            ))
        }
    }
}

pub fn file_name(binary: &str) -> String {
    if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    }
}

pub fn file_digest(path: &Path) -> DesktopResult<String> {
    const MAX_BINARY_BYTES: u64 = 1024 * 1024 * 1024;
    let mut file = std::fs::File::open(path).map_err(|_| error("runtime_file_unreadable"))?;
    let metadata = file
        .metadata()
        .map_err(|_| error("runtime_file_unreadable"))?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > MAX_BINARY_BYTES {
        return Err(error("runtime_file_invalid"));
    }
    let mut hasher = Sha256::new();
    let mut remaining = MAX_BINARY_BYTES;
    let mut buffer = [0u8; 65536];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| error("runtime_file_unreadable"))?;
        if read == 0 {
            break;
        }
        remaining = remaining
            .checked_sub(read as u64)
            .ok_or_else(|| error("runtime_file_invalid"))?;
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        return std::fs::metadata(path)
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

fn host_compatible(info: &MachineBuildInfo) -> bool {
    let arch = webcodex_core::build_info::current()
        .architecture
        .unwrap_or(std::env::consts::ARCH);
    let os_matches = match std::env::consts::OS {
        "macos" => info.target.contains("apple-darwin"),
        "windows" => info.target.contains("windows"),
        "linux" => info.target.contains("linux"),
        _ => false,
    };
    info.architecture == arch && os_matches
}

async fn probe_binary(
    path: &Path,
    name: &str,
    cancellation: &CancellationContext,
    deadline: Deadline,
) -> DesktopResult<BinaryProbe> {
    let mut result = BinaryProbe {
        name: name.into(),
        present: path.is_file(),
        executable: executable(path),
        metadata: None,
        sha256: None,
        error_code: None,
    };
    if !result.present {
        result.error_code = Some("binary_missing".into());
        return Ok(result);
    }
    if !result.executable {
        result.error_code = Some("binary_not_executable".into());
        return Ok(result);
    }
    let owned = path.to_path_buf();
    let before = tokio::task::spawn_blocking(move || file_digest(&owned))
        .await
        .map_err(|_| error("binary_probe_failed"))?;
    let before = match before {
        Ok(digest) => digest,
        Err(e) => {
            result.error_code = Some(e.code);
            return Ok(result);
        }
    };
    cancellation.check()?;
    let info: DesktopResult<MachineBuildInfo> = run_json_until(
        path,
        &["--build-info-json".into()],
        None,
        false,
        CliCommandContext::new("runtime_probe", "build-info"),
        cancellation,
        deadline,
    )
    .await;
    match info {
        Err(e) if e.code == "desktop_operation_cancelled" => return Err(e),
        Err(_) => result.error_code = Some("build_info_unverifiable".into()),
        Ok(info) => {
            let validation = info.validate(name).map_err(str::to_string);
            match validation {
                Err(code) => result.error_code = Some(code),
                Ok(()) => {
                    if !host_compatible(&info) {
                        result.error_code = Some("binary_architecture_mismatch".into());
                    } else if !info
                        .desktop_runtime_contract
                        .overlaps(DESKTOP_RUNTIME_CONTRACT)
                    {
                        result.error_code = Some("runtime_contract_incompatible".into());
                    }
                    result.metadata = Some(info);
                }
            }
        }
    }
    let owned = path.to_path_buf();
    let after = tokio::task::spawn_blocking(move || file_digest(&owned))
        .await
        .map_err(|_| error("binary_probe_failed"))??;
    if before != after {
        result.error_code = Some("runtime_candidate_changed".into());
    }
    result.sha256 = Some(after);
    Ok(result)
}

pub async fn probe(
    source: RuntimeSource,
    bundled: Option<&Path>,
    revision: u64,
    cancellation: &CancellationContext,
    deadline: Deadline,
) -> DesktopResult<(RuntimeCandidate, Option<ResolvedBinaries>)> {
    cancellation.check()?;
    let mut view = RuntimeCandidate {
        candidate_id: uuid::Uuid::new_v4().to_string(),
        source: source.clone(),
        selection_revision: revision,
        checked_at_ms: now_ms(),
        directory: None,
        binaries: Vec::new(),
        compatibility: ProtocolCompatibility::Unknown,
        build_alignment: BuildAlignment::Unknown,
        advisories: Vec::new(),
        error_code: None,
        fingerprint: None,
    };
    let (directory, resolution) =
        match source_directory(&source, bundled).and_then(|(dir, provenance)| {
            dir.canonicalize()
                .map(|dir| (dir, provenance))
                .map_err(|_| error("runtime_directory_missing"))
        }) {
            Ok(pair) => pair,
            Err(e) => {
                view.error_code = Some(e.code);
                return Ok((view, None));
            }
        };
    view.directory = Some(directory.clone());
    for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
        view.binaries.push(
            probe_binary(
                &directory.join(file_name(name)),
                name,
                cancellation,
                deadline,
            )
            .await?,
        );
    }
    if let Some(code) = view.binaries.iter().find_map(|b| b.error_code.clone()) {
        view.compatibility = ProtocolCompatibility::Incompatible;
        view.error_code = Some(code);
        return Ok((view, None));
    }
    let builds: Vec<MachineBuildInfo> = view
        .binaries
        .iter()
        .filter_map(|b| b.metadata.clone())
        .collect();
    if builds.len() != 3 {
        return Err(error("build_info_unverifiable"));
    }
    let common = builds
        .iter()
        .try_fold(DESKTOP_RUNTIME_CONTRACT, |range, build| {
            range.intersection(build.desktop_runtime_contract)
        });
    if common.is_none() {
        view.compatibility = ProtocolCompatibility::Incompatible;
        view.error_code = Some("runtime_contract_incompatible".into());
        return Ok((view, None));
    }
    view.compatibility = ProtocolCompatibility::Compatible;
    view.build_alignment = BuildAlignment::Exact;
    for item in &builds {
        let alignment = build_alignment(
            Some(&builds[0].version),
            builds[0].git_commit.as_deref(),
            builds[0].git_dirty,
            Some(&item.version),
            item.git_commit.as_deref(),
            item.git_dirty,
        );
        view.build_alignment = merge_alignment(view.build_alignment, alignment);
        if item.git_dirty == Some(true) {
            view.advisories
                .push("dirty_build_operator_responsibility".into());
        }
        if item.git_commit != builds[0].git_commit {
            view.advisories.push("different_source_revisions".into());
        }
        if item.version != builds[0].version {
            view.advisories.push("different_package_versions".into());
        }
        if item.git_commit.is_none() || item.git_dirty.is_none() {
            view.advisories.push("build_identity_incomplete".into());
        }
    }
    if matches!(source, RuntimeSource::Custom { .. }) {
        view.advisories
            .push("custom_build_operator_responsibility".into());
    }
    view.advisories.sort();
    view.advisories.dedup();
    let mut hasher = Sha256::new();
    hasher.update(directory.to_string_lossy().as_bytes());
    for binary in &view.binaries {
        hasher.update(binary.sha256.as_deref().unwrap_or_default().as_bytes());
    }
    let fingerprint = format!("{:x}", hasher.finalize());
    view.fingerprint = Some(fingerprint.clone());
    let resolved = ResolvedBinaries {
        directory: directory.clone(),
        webcodex: directory.join(file_name("webcodex")),
        server: directory.join(file_name("webcodex-server")),
        runner: directory.join(file_name("webcodex-runner")),
        version: builds[0].version.clone(),
        git_commit: builds[0]
            .git_commit
            .clone()
            .unwrap_or_else(|| "unknown".into()),
        source: resolution,
        builds,
        fingerprint,
    };
    Ok((view, Some(resolved)))
}

/// Recheck immutable candidate evidence immediately before using it. File changes
/// never inherit the approval given to an earlier preview.
pub async fn verify_resolved_files(binaries: &ResolvedBinaries) -> DesktopResult<()> {
    let directory = binaries.directory.clone();
    let expected = binaries.fingerprint.clone();
    tokio::task::spawn_blocking(move || {
        let mut hash = Sha256::new();
        hash.update(directory.to_string_lossy().as_bytes());
        for name in ["webcodex", "webcodex-server", "webcodex-runner"] {
            hash.update(file_digest(&directory.join(file_name(name)))?.as_bytes());
        }
        if format!("{:x}", hash.finalize()) != expected {
            return Err(error("runtime_candidate_changed"));
        }
        Ok(())
    })
    .await
    .map_err(|_| error("binary_probe_failed"))?
}

fn merge_alignment(left: BuildAlignment, right: BuildAlignment) -> BuildAlignment {
    fn rank(value: BuildAlignment) -> u8 {
        match value {
            BuildAlignment::Exact => 0,
            BuildAlignment::Unknown => 1,
            BuildAlignment::DifferentCommit => 2,
            BuildAlignment::DifferentVersion => 3,
            BuildAlignment::Dirty => 4,
        }
    }
    if rank(right) > rank(left) {
        right
    } else {
        left
    }
}

pub fn candidate_from_resolved(
    binaries: &ResolvedBinaries,
    source: RuntimeSource,
    revision: u64,
) -> RuntimeCandidate {
    let mut advisories = Vec::new();
    let mut alignment = BuildAlignment::Exact;
    if let Some(first) = binaries.builds.first() {
        for build in &binaries.builds {
            let current = build_alignment(
                Some(&first.version),
                first.git_commit.as_deref(),
                first.git_dirty,
                Some(&build.version),
                build.git_commit.as_deref(),
                build.git_dirty,
            );
            alignment = merge_alignment(alignment, current);
            if build.git_dirty == Some(true) {
                advisories.push("dirty_build_operator_responsibility".into());
            }
            if build.git_commit != first.git_commit {
                advisories.push("different_source_revisions".into());
            }
            if build.version != first.version {
                advisories.push("different_package_versions".into());
            }
        }
    } else {
        alignment = BuildAlignment::Unknown;
    }
    if matches!(source, RuntimeSource::Custom { .. }) {
        advisories.push("custom_build_operator_responsibility".into());
    }
    advisories.sort();
    advisories.dedup();
    RuntimeCandidate {
        candidate_id: String::new(),
        source,
        selection_revision: revision,
        checked_at_ms: 0,
        directory: Some(binaries.directory.clone()),
        binaries: binaries
            .builds
            .iter()
            .map(|build| BinaryProbe {
                name: build.binary.clone(),
                present: true,
                executable: true,
                metadata: Some(build.clone()),
                sha256: None,
                error_code: None,
            })
            .collect(),
        compatibility: if binaries.builds.len() == 3 {
            ProtocolCompatibility::Compatible
        } else {
            ProtocolCompatibility::Unknown
        },
        build_alignment: alignment,
        advisories,
        error_code: None,
        fingerprint: Some(binaries.fingerprint.clone()),
    }
}

#[cfg(test)]
mod tests;
