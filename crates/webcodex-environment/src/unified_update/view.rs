//! One bounded, path-free observation consumed by Desktop and the CLI.
use super::*;
use serde::{Deserialize, Serialize};
use webcodex_core::desktop_runtime_contract::MachineBuildInfo;

pub const MAX_UPDATE_VIEW_BYTES: usize = 64 * 1024;
pub const UPDATE_COMPONENTS: [&str; 4] = [
    "webcodex-desktop",
    "webcodex",
    "webcodex-server",
    "webcodex-runner",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateIdentity {
    pub version: String,
    pub target: InstallerTarget,
    pub source_sha: String,
    pub manifest_sha256: String,
    pub installer_sha256: String,
}
impl CandidateIdentity {
    pub(super) fn matches_record(&self, record: &super::download::UpdateRecord) -> bool {
        record.version.as_deref() == Some(self.version.as_str())
            && record.target == Some(self.target)
            && record.source_sha.as_deref() == Some(self.source_sha.as_str())
            && record.source_manifest_sha256.as_deref() == Some(self.manifest_sha256.as_str())
            && record.sha256.as_deref() == Some(self.installer_sha256.as_str())
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct ComponentBuild {
    pub binary: String,
    pub build: Option<MachineBuildInfo>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateBlocker {
    EnvironmentNotConfigured,
    UnsupportedInstallation,
    UnsupportedPlatform,
    GuardedHandoffUnavailable,
    CandidateNotVerified,
    DownloadRequired,
    PendingInstall,
    RecoveryRequired,
    UpgradeInProgress,
    ActiveTasks,
    TaskObservationUnavailable,
}
#[derive(Debug, Clone, Serialize)]
pub struct UpdateView {
    pub schema_version: u16,
    pub download: DownloadStatus,
    pub installed: Vec<ComponentBuild>,
    pub candidate: Option<CandidateIdentity>,
    pub candidate_components: Vec<ComponentBuild>,
    pub upgrade: Option<crate::UpgradeStatus>,
    pub blockers: Vec<UpdateBlocker>,
    pub restart_required: bool,
}

fn components(builds: &[MachineBuildInfo]) -> Vec<ComponentBuild> {
    UPDATE_COMPONENTS
        .into_iter()
        .map(|binary| ComponentBuild {
            binary: binary.into(),
            build: builds
                .iter()
                .find_map(|build| safe_component_build(build, binary)),
        })
        .collect()
}

/// Build probes are diagnostic input and may describe custom programs. Only
/// known platform identities and plain release versions cross this surface.
/// This does not narrow the underlying MachineBuildInfo wire contract.
fn safe_component_build(build: &MachineBuildInfo, binary: &str) -> Option<MachineBuildInfo> {
    build.validate(binary).ok()?;
    let version = semver::Version::parse(&build.version).ok()?;
    if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != build.version
    {
        return None;
    }
    let platform = RuntimePlatform::ALL.into_iter().find(|platform| {
        build.target == platform.target() && build.architecture == platform.architecture()
    })?;
    Some(MachineBuildInfo {
        schema_version: build.schema_version,
        binary: binary.into(),
        version: version.to_string(),
        git_commit: build.git_commit.clone(),
        git_dirty: build.git_dirty,
        built_at: build.built_at.clone(),
        target: platform.target().into(),
        architecture: platform.architecture().into(),
        desktop_runtime_contract: build.desktop_runtime_contract,
        agent_protocol_generation: build.agent_protocol_generation,
        environment_data_format: build.environment_data_format,
    })
}
impl UpdateManager {
    pub fn candidate_identity(&self) -> UpdateResult<Option<CandidateIdentity>> {
        let Some(cache) = PrivateUpdateCache::open_existing(self.root.clone())? else {
            return Ok(None);
        };
        let _lock = cache.lock_existing()?;
        let record = self.read_record_locked(&cache)?;
        Self::candidate_locked(&cache, &record).map(|c| c.map(|(identity, _)| identity))
    }
    fn read_record_locked(
        &self,
        cache: &PrivateUpdateCache,
    ) -> UpdateResult<super::download::UpdateRecord> {
        let record = match cache.read("update-state.json", 24 * 1024)? {
            Some(bytes) => serde_json::from_slice::<super::download::UpdateRecord>(&bytes)
                .map_err(|_| UpdateError::RecoveryRequired)?,
            None => super::download::UpdateRecord::default(),
        };
        if !record.valid()
            || record
                .target
                .is_some_and(|t| Some(t.platform) != RuntimePlatform::current())
        {
            return Err(UpdateError::RecoveryRequired);
        }
        Ok(record)
    }
    fn candidate_locked(
        cache: &PrivateUpdateCache,
        record: &super::download::UpdateRecord,
    ) -> UpdateResult<Option<(CandidateIdentity, UpdateSource)>> {
        let Some(version) = record.version.as_deref() else {
            return Ok(None);
        };
        let Some(target) = record.target else {
            return Ok(None);
        };
        let Some(cache) = cache.existing_child(version)? else {
            return Ok(None);
        };
        let Some(cache) = cache.existing_child(&target.as_str())? else {
            return Ok(None);
        };
        let Some(bytes) = cache.read("source-manifest.json", MAX_SOURCE_BYTES)? else {
            return Ok(None);
        };
        let source = verify_source_manifest(&bytes, version, target.platform)?;
        let identity = CandidateIdentity {
            version: version.into(),
            target,
            source_sha: source.source_sha.clone(),
            manifest_sha256: source.manifest_sha256.clone(),
            installer_sha256: record.sha256.clone().ok_or(UpdateError::ProvenanceFailed)?,
        };
        if !identity.matches_record(record) {
            return Err(UpdateError::ProvenanceFailed);
        }
        Ok(Some((identity, source)))
    }
    /// One private cache fence covers record and candidate identity. Observation
    /// never overwrites a concurrently active in-memory download.
    pub fn status_view(&self, installed: &[MachineBuildInfo]) -> UpdateResult<UpdateView> {
        let attempt = self.attempt.try_lock();
        if attempt.is_err() {
            let mut download = self.snapshot();
            download.can_install = false;
            return bounded_view(UpdateView {
                schema_version: 1,
                download,
                installed: components(installed),
                candidate: None,
                candidate_components: components(&[]),
                upgrade: None,
                blockers: vec![UpdateBlocker::UpgradeInProgress],
                restart_required: false,
            });
        }
        let cache = PrivateUpdateCache::open_existing(self.root.clone())?;
        let _lock = cache
            .as_ref()
            .map(PrivateUpdateCache::lock_existing)
            .transpose()?;
        let record = match &cache {
            Some(cache) => self.read_record_locked(cache)?,
            None => super::download::UpdateRecord::default(),
        };
        let installation = self.snapshot().installation;
        let mut download = DownloadStatus {
            phase: record.phase,
            version: record.version.clone(),
            platform: record.target.map(|t| t.platform),
            target: record.target,
            downloaded_bytes: record.downloaded_bytes,
            total_bytes: record.total_bytes,
            error_kind: record.error_kind,
            installation,
            can_install: false,
            pending_install: record.pending.is_some(),
            legacy_release: record.legacy_release,
            cancelled: record.cancelled,
        };
        let candidate = cache
            .as_ref()
            .map(|cache| Self::candidate_locked(cache, &record))
            .transpose()?
            .flatten();
        let candidate_components = components(
            &candidate
                .as_ref()
                .map(|(_, source)| {
                    UPDATE_COMPONENTS
                        .into_iter()
                        .filter_map(|name| source.component_build(name).cloned())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        );
        let guarded_handoff_available = candidate.as_ref().is_none_or(|(identity, source)| {
            identity.target.format != PackageFormat::Exe
                || source.supports_guarded_windows_handoff()
        });
        let candidate = candidate.map(|(identity, _)| identity);
        let upgrade = crate::upgrade_status_at(&self.environment_root)
            .map_err(|_| UpdateError::RecoveryRequired)?;
        let mut blockers = Vec::new();
        if !guarded_handoff_available {
            blockers.push(UpdateBlocker::GuardedHandoffUnavailable);
        }
        match download.installation {
            InstallationKind::Managed => {}
            InstallationKind::EnvironmentNotConfigured => {
                blockers.push(UpdateBlocker::EnvironmentNotConfigured)
            }
            InstallationKind::UnsupportedPlatform => {
                blockers.push(UpdateBlocker::UnsupportedPlatform)
            }
            _ => blockers.push(UpdateBlocker::UnsupportedInstallation),
        }
        if candidate.is_none() {
            blockers.push(UpdateBlocker::CandidateNotVerified);
        }
        if record.phase != DownloadPhase::ReadyToInstall {
            blockers.push(UpdateBlocker::DownloadRequired);
        }
        if record.pending.is_some() {
            blockers.push(UpdateBlocker::PendingInstall);
        }
        if record.pending.as_ref().is_some_and(|p| {
            upgrade.as_ref().is_none_or(|u| {
                u.environment_id != p.environment_id
                    || p.operation_id
                        .as_deref()
                        .is_none_or(|op| op != u.operation_id)
            })
        }) || download.error_kind == Some(UpdateError::RecoveryRequired)
        {
            blockers.push(UpdateBlocker::RecoveryRequired);
        }
        if upgrade.as_ref().is_some_and(|u| {
            !matches!(
                u.phase,
                crate::UpgradePhase::Committed | crate::UpgradePhase::RolledBack
            )
        }) {
            blockers.push(UpdateBlocker::UpgradeInProgress);
        }
        if blockers.is_empty() {
            if let (Some(root), Some(identity)) = (&cache, &candidate) {
                let target_cache = root
                    .existing_child(&identity.version)?
                    .ok_or(UpdateError::CacheUnavailable)?
                    .existing_child(&identity.target.as_str())?
                    .ok_or(UpdateError::CacheUnavailable)?;
                let filename = identity.target.installer_filename(&identity.version);
                let mut file = target_cache.open_file(&filename)?;
                use sha2::Digest;
                use std::io::Read;
                let mut hash = sha2::Sha256::new();
                let mut count = 0u64;
                let mut bytes = [0u8; 128 * 1024];
                loop {
                    let size = file
                        .read(&mut bytes)
                        .map_err(|_| UpdateError::CacheUnavailable)?;
                    if size == 0 {
                        break;
                    }
                    count = count.saturating_add(size as u64);
                    if count > MAX_INSTALLER_BYTES {
                        return Err(UpdateError::DownloadTooLarge);
                    }
                    hash.update(&bytes[..size]);
                }
                if count > 0
                    && count == record.downloaded_bytes
                    && format!("{:x}", hash.finalize()) == identity.installer_sha256
                {
                    download.can_install = true;
                } else {
                    blockers.push(UpdateBlocker::CandidateNotVerified);
                }
            }
        }
        let view = UpdateView {
            schema_version: 1,
            download,
            installed: components(installed),
            candidate,
            candidate_components,
            upgrade,
            blockers,
            restart_required: false,
        };
        bounded_view(view)
    }
}

fn bounded_view(view: UpdateView) -> UpdateResult<UpdateView> {
    if serde_json::to_vec(&view)
        .map_err(|_| UpdateError::CacheUnavailable)?
        .len()
        > MAX_UPDATE_VIEW_BYTES
    {
        return Err(UpdateError::CacheUnavailable);
    }
    Ok(view)
}
#[cfg(test)]
mod tests;
