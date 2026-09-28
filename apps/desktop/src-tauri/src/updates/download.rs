use super::{is_newer_stable, valid_notice, ReleaseNotice, CHECK_INTERVAL_MS};
use crate::operation::CancellationSignal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use webcodex_environment::unified_update::{
    self as unified, InstallerTarget, PrivateUpdateCache, RuntimePlatform, UpdateError,
    UpdateResult, MAX_INSTALLER_BYTES,
};

pub(super) const RETRY_INTERVAL_MS: u64 = 60 * 60 * 1000;
const STATE_BYTES: u64 = 24 * 1024;
const STATE_FILE: &str = "update-state.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Verifying,
    ReadyToInstall,
    Preparing,
    InstallingOrHandedOff,
    Failed,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallationKind {
    Managed,
    #[default]
    SourceBuild,
    UnmanagedInstallation,
    EnvironmentNotConfigured,
    UnsupportedPlatform,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct DownloadStatus {
    pub phase: DownloadPhase,
    pub version: Option<String>,
    pub platform: Option<RuntimePlatform>,
    pub target: Option<InstallerTarget>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error_kind: Option<UpdateError>,
    pub installation: InstallationKind,
    pub can_install: bool,
    pub pending_install: bool,
    pub legacy_release: bool,
    pub cancelled: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PendingInstall {
    pub environment_id: String,
    pub operation_id: Option<String>,
    pub started_at_ms: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct UpdateRecord {
    schema_version: u16,
    pub phase: DownloadPhase,
    pub version: Option<String>,
    pub target: Option<InstallerTarget>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub sha256: Option<String>,
    pub source_manifest_sha256: Option<String>,
    pub source_sha: Option<String>,
    pub verified_at_ms: Option<u64>,
    pub last_attempt_at_ms: Option<u64>,
    pub next_retry_at_ms: Option<u64>,
    pub cancelled: bool,
    pub legacy_release: bool,
    pub error_kind: Option<UpdateError>,
    pub pending: Option<PendingInstall>,
}
impl Default for UpdateRecord {
    fn default() -> Self {
        Self {
            schema_version: 2,
            phase: DownloadPhase::Idle,
            version: None,
            target: None,
            downloaded_bytes: 0,
            total_bytes: None,
            sha256: None,
            source_manifest_sha256: None,
            source_sha: None,
            verified_at_ms: None,
            last_attempt_at_ms: None,
            next_retry_at_ms: None,
            cancelled: false,
            legacy_release: false,
            error_kind: None,
            pending: None,
        }
    }
}
impl UpdateRecord {
    fn valid(&self) -> bool {
        self.schema_version == 2
            && self.version.as_deref().is_none_or(unified::stable_version)
            && self.version.is_some() == self.target.is_some()
            && self.target.is_none_or(|target| target.valid())
            && self.downloaded_bytes <= MAX_INSTALLER_BYTES
            && self
                .total_bytes
                .is_none_or(|n| n > 0 && n <= MAX_INSTALLER_BYTES && self.downloaded_bytes <= n)
            && self.sha256.as_deref().is_none_or(unified::valid_sha256)
            && self
                .source_manifest_sha256
                .as_deref()
                .is_none_or(unified::valid_sha256)
            && self.source_sha.as_deref().is_none_or(|v| {
                v.len() == 40
                    && v.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            && self.pending.as_ref().is_none_or(|p| {
                self.version.is_some()
                    && !p.environment_id.is_empty()
                    && p.environment_id.len() <= 128
                    && p.operation_id
                        .as_ref()
                        .is_none_or(|v| !v.is_empty() && v.len() <= 64)
            })
            && (self.phase != DownloadPhase::InstallingOrHandedOff || self.pending.is_some())
            && (self.phase != DownloadPhase::ReadyToInstall
                || (self.version.is_some()
                    && self.sha256.is_some()
                    && self.source_manifest_sha256.is_some()
                    && self.source_sha.is_some()
                    && self.verified_at_ms.is_some()
                    && self.downloaded_bytes > 0))
    }
    fn may_retry(&self, now: u64, manual: bool) -> bool {
        manual
            || (!self.cancelled
                && self
                    .next_retry_at_ms
                    .is_none_or(|next| next <= now || next > now.saturating_add(CHECK_INTERVAL_MS)))
    }
}

pub struct UpdateManager {
    pub(super) root: PathBuf,
    pub(super) record: Mutex<UpdateRecord>,
    pub(super) attempt: Arc<tokio::sync::Mutex<()>>,
    installation: Mutex<InstallationKind>,
    loaded: std::sync::atomic::AtomicBool,
    cancellation: Mutex<(CancellationSignal, bool)>,
}

impl UpdateManager {
    /// No filesystem/network failure here can prevent Desktop/Runtime startup.
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            root: data_dir.join("stable-updates-v1"),
            record: Mutex::new(UpdateRecord::default()),
            attempt: Arc::new(tokio::sync::Mutex::new(())),
            installation: Mutex::new(InstallationKind::default()),
            loaded: std::sync::atomic::AtomicBool::new(false),
            cancellation: Mutex::new((CancellationSignal::new(), false)),
        })
    }
    pub(super) fn current(&self) -> UpdateRecord {
        self.record
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
    pub(super) fn change(&self, apply: impl FnOnce(&mut UpdateRecord)) {
        apply(&mut self.record.lock().unwrap_or_else(|e| e.into_inner()));
    }
    pub(super) fn persist(&self, cache: &PrivateUpdateCache) -> UpdateResult<()> {
        let bytes =
            serde_json::to_vec(&self.current()).map_err(|_| UpdateError::CacheUnavailable)?;
        if bytes.len() as u64 > STATE_BYTES {
            return Err(UpdateError::CacheUnavailable);
        }
        cache.write(STATE_FILE, &bytes)
    }
    pub fn set_installation(&self, kind: InstallationKind) {
        *self.installation.lock().unwrap_or_else(|e| e.into_inner()) = kind;
    }
    pub fn snapshot(&self) -> DownloadStatus {
        let record = self.current();
        let installation = *self.installation.lock().unwrap_or_else(|e| e.into_inner());
        DownloadStatus {
            phase: record.phase,
            version: record.version,
            platform: record.target.map(|target| target.platform),
            target: record.target,
            downloaded_bytes: record.downloaded_bytes,
            total_bytes: record.total_bytes,
            error_kind: record.error_kind,
            installation,
            can_install: record.phase == DownloadPhase::ReadyToInstall
                && record.pending.is_none()
                && installation == InstallationKind::Managed,
            pending_install: record.pending.is_some(),
            legacy_release: record.legacy_release,
            cancelled: record.cancelled,
        }
    }
    pub fn note_action_error(&self, error: UpdateError) {
        self.change(|record| {
            record.error_kind = Some(if record.pending.is_some() {
                UpdateError::RecoveryRequired
            } else {
                error
            })
        });
    }
    pub fn cancel_download(&self, user_requested: bool) {
        if matches!(
            self.current().phase,
            DownloadPhase::Preparing | DownloadPhase::InstallingOrHandedOff
        ) {
            return;
        }
        let mut cancellation = self.cancellation.lock().unwrap_or_else(|e| e.into_inner());
        cancellation.1 = user_requested;
        cancellation.0.cancel();
    }
    pub fn request(
        self: &Arc<Self>,
        notice: Option<ReleaseNotice>,
        automatic: bool,
        manual: bool,
        installation: InstallationKind,
        target: Option<InstallerTarget>,
    ) {
        self.set_installation(installation);
        let Ok(guard) = Arc::clone(&self.attempt).try_lock_owned() else {
            if notice.as_ref().is_some_and(|n| {
                self.current()
                    .version
                    .as_deref()
                    .is_some_and(|v| v != n.version)
            }) {
                self.cancel_download(false);
            }
            return;
        };
        let cancellation = CancellationSignal::new();
        *self.cancellation.lock().unwrap_or_else(|e| e.into_inner()) =
            (cancellation.clone(), false);
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let _guard = guard;
            let result = manager
                .run(
                    notice,
                    automatic,
                    manual,
                    installation,
                    target,
                    &cancellation,
                )
                .await;
            if let Err(error) = result {
                // Cache admission/loading failed. Do not overwrite an unknown
                // pending transaction or another process's persisted state.
                manager.change(|state| {
                    state.phase = DownloadPhase::Failed;
                    state.error_kind = Some(error);
                });
            }
        });
    }
    pub(super) fn record_failure(&self, cache: &PrivateUpdateCache, error: UpdateError) {
        let now = crate::runtime_selection::now_ms();
        let user_cancelled = self
            .cancellation
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .1;
        self.change(|state| {
            if state.pending.is_some() {
                state.phase = DownloadPhase::Failed;
                state.error_kind = Some(UpdateError::RecoveryRequired);
            } else if error == UpdateError::Cancelled {
                state.phase = DownloadPhase::Available;
                state.error_kind = None;
                state.cancelled = user_cancelled;
                state.downloaded_bytes = 0;
                state.total_bytes = None;
            } else {
                state.phase = DownloadPhase::Failed;
                state.error_kind = Some(error);
                state.next_retry_at_ms = Some(now.saturating_add(RETRY_INTERVAL_MS));
            }
        });
        if self.current().pending.is_none() {
            if let Ok(target) = self.target_cache(cache) {
                let _ = target.remove_file("installer.part");
            }
        }
        if self.persist(cache).is_err() {
            self.change(|state| {
                state.phase = DownloadPhase::Failed;
                state.error_kind = Some(UpdateError::CacheUnavailable);
            });
        }
    }
    pub(super) fn target_cache(
        &self,
        root: &PrivateUpdateCache,
    ) -> UpdateResult<PrivateUpdateCache> {
        let record = self.current();
        let version = record
            .version
            .as_deref()
            .filter(|v| unified::stable_version(v))
            .ok_or(UpdateError::ManifestInvalid)?;
        let target = record.target.ok_or(UpdateError::UnsupportedPlatform)?;
        root.child(version)?.child(&target.as_str())
    }
    pub(super) fn load(&self, cache: &PrivateUpdateCache) -> UpdateResult<()> {
        use std::sync::atomic::Ordering;
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let mut record = match cache.read(STATE_FILE, STATE_BYTES)? {
            Some(bytes) => serde_json::from_slice::<UpdateRecord>(&bytes)
                .map_err(|_| UpdateError::RecoveryRequired)?,
            None => UpdateRecord::default(),
        };
        if !record.valid()
            || record
                .target
                .is_some_and(|target| Some(target.platform) != RuntimePlatform::current())
        {
            return Err(UpdateError::RecoveryRequired);
        }
        if record.pending.is_none()
            && matches!(
                record.phase,
                DownloadPhase::Checking
                    | DownloadPhase::Downloading
                    | DownloadPhase::Verifying
                    | DownloadPhase::ReadyToInstall
                    | DownloadPhase::Preparing
            )
        {
            // A persisted ready bit is never execution authority. Re-establish
            // publisher identity and hash the complete private file on restart.
            record.phase = DownloadPhase::Available;
            record.next_retry_at_ms = None;
            record.downloaded_bytes = 0;
            record.total_bytes = None;
        }
        self.change(|value| *value = record);
        if self.current().version.is_some() {
            self.target_cache(cache)?.remove_file("installer.part")?;
        }
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }
    async fn run(
        &self,
        notice: Option<ReleaseNotice>,
        automatic: bool,
        manual: bool,
        installation: InstallationKind,
        target: Option<InstallerTarget>,
        cancellation: &CancellationSignal,
    ) -> UpdateResult<()> {
        let cache = PrivateUpdateCache::open(self.root.clone())?;
        let _process_lock = cache.lock()?;
        self.load(&cache)?;
        let result = self
            .run_locked(
                &cache,
                notice,
                automatic,
                manual,
                installation,
                target,
                cancellation,
            )
            .await;
        if let Err(error) = result {
            self.record_failure(&cache, error);
        }
        Ok(())
    }
    async fn run_locked(
        &self,
        cache: &PrivateUpdateCache,
        notice: Option<ReleaseNotice>,
        automatic: bool,
        manual: bool,
        installation: InstallationKind,
        target: Option<InstallerTarget>,
        cancellation: &CancellationSignal,
    ) -> UpdateResult<()> {
        if self.current().pending.is_some() {
            self.reconcile_pending(cache).await?;
            if self.current().pending.is_some() {
                return Ok(());
            }
        }
        let notice = notice
            .filter(|n| valid_notice(n) && is_newer_stable(&n.version, env!("CARGO_PKG_VERSION")));
        let Some(notice) = notice else {
            cache.retain_version(None)?;
            self.change(|s| *s = UpdateRecord::default());
            return self.persist(&cache);
        };
        let Some(target) = target
            .filter(|target| target.valid() && Some(target.platform) == RuntimePlatform::current())
        else {
            self.change(|s| {
                s.phase = DownloadPhase::Available;
                s.error_kind = Some(UpdateError::UnsupportedPlatform);
            });
            return Ok(());
        };
        let current = self.current();
        let version_changed = current.version.as_deref() != Some(&notice.version);
        let target_changed = current.target != Some(target);
        if version_changed || target_changed {
            cache.retain_version(Some(&notice.version))?;
            if !version_changed {
                if let Some(previous) = current.target {
                    cache
                        .child(&notice.version)?
                        .remove_child_tree(&previous.as_str())?;
                }
            }
            self.change(|s| {
                *s = UpdateRecord {
                    version: Some(notice.version.clone()),
                    target: Some(target),
                    phase: DownloadPhase::Available,
                    ..Default::default()
                }
            });
            self.persist(&cache)?;
        }
        if self.current().phase == DownloadPhase::ReadyToInstall {
            return Ok(());
        }
        if !self
            .current()
            .may_retry(crate::runtime_selection::now_ms(), manual)
        {
            return Ok(());
        }
        // Source/legacy builds may be downloaded explicitly, never replaced.
        let allowed = manual
            || (automatic
                && matches!(
                    installation,
                    InstallationKind::Managed | InstallationKind::EnvironmentNotConfigured
                ));
        if !allowed && self.current().verified_at_ms.is_none() {
            return Ok(());
        }
        if notice.release_url
            != format!(
                "https://github.com/yyjeqhc/webcodex/releases/tag/v{}",
                notice.version
            )
        {
            self.change(|s| {
                s.phase = DownloadPhase::Available;
                s.legacy_release = true;
            });
            return self.persist(&cache);
        }
        let now = crate::runtime_selection::now_ms();
        self.change(|s| {
            s.phase = DownloadPhase::Checking;
            s.error_kind = None;
            s.cancelled = false;
            s.last_attempt_at_ms = Some(now);
        });
        self.persist(&cache)?;
        tokio::select! {
            _ = cancellation.cancelled() => Err(UpdateError::Cancelled),
            result = self.obtain(&cache, &notice.version, target, allowed, cancellation) => result,
        }
    }
    async fn obtain(
        &self,
        cache: &PrivateUpdateCache,
        version: &str,
        target: InstallerTarget,
        allowed: bool,
        cancellation: &CancellationSignal,
    ) -> UpdateResult<()> {
        let Some(release) = unified::fetch_release(version, target).await? else {
            self.change(|s| {
                s.phase = DownloadPhase::Available;
                s.legacy_release = true;
                s.error_kind = None;
                s.next_retry_at_ms =
                    Some(crate::runtime_selection::now_ms().saturating_add(CHECK_INTERVAL_MS));
            });
            return self.persist(cache);
        };
        let entry = release.manifest.entry(target)?;
        let target = self.target_cache(cache)?;
        self.change(|s| {
            s.phase = DownloadPhase::Verifying;
            s.legacy_release = false;
            s.sha256 = Some(entry.sha256.clone());
            s.source_manifest_sha256 = Some(entry.source_manifest_sha256.clone());
            s.source_sha = Some(release.source.source_sha.clone());
        });
        let existing = target
            .file(&entry.filename)?
            .try_exists()
            .map_err(|_| UpdateError::CacheUnavailable)?;
        let size = if existing {
            match verify_file(
                &target,
                &entry.filename,
                &entry.sha256,
                MAX_INSTALLER_BYTES,
                cancellation,
            )
            .await
            {
                Ok(bytes) => bytes,
                Err(UpdateError::ChecksumMismatch | UpdateError::DownloadTooLarge) => {
                    target.remove_file(&entry.filename)?;
                    if !allowed {
                        return Err(UpdateError::ChecksumMismatch);
                    }
                    self.download(&target, entry, cancellation).await?
                }
                Err(error) => return Err(error),
            }
        } else {
            if !allowed {
                self.change(|s| {
                    s.phase = DownloadPhase::Available;
                    s.verified_at_ms = None;
                });
                return self.persist(cache);
            }
            self.download(&target, entry, cancellation).await?
        };
        target.write("manifest.json", &release.manifest_bytes)?;
        target.write("source-manifest.json", &release.source_bytes)?;
        self.change(|s| {
            s.phase = DownloadPhase::ReadyToInstall;
            s.downloaded_bytes = size;
            s.total_bytes = Some(size);
            s.verified_at_ms = Some(crate::runtime_selection::now_ms());
            s.error_kind = None;
            s.next_retry_at_ms = None;
        });
        self.persist(cache)
    }
    async fn download(
        &self,
        target: &PrivateUpdateCache,
        entry: &unified::InstallerEntry,
        cancellation: &CancellationSignal,
    ) -> UpdateResult<u64> {
        target.remove_file("installer.part")?;
        self.change(|s| {
            s.phase = DownloadPhase::Downloading;
            s.downloaded_bytes = 0;
            s.total_bytes = None;
        });
        let response = unified::http_client()?
            .get(&entry.url)
            .send()
            .await
            .map_err(|_| UpdateError::NetworkUnavailable)?;
        stream_installer(
            response,
            target,
            &entry.filename,
            &entry.sha256,
            MAX_INSTALLER_BYTES,
            cancellation,
            |done, total| {
                self.change(|s| {
                    s.downloaded_bytes = done;
                    s.total_bytes = total;
                });
            },
        )
        .await
    }
}

pub(super) async fn verify_file(
    cache: &PrivateUpdateCache,
    filename: &str,
    expected: &str,
    maximum: u64,
    cancellation: &CancellationSignal,
) -> UpdateResult<u64> {
    let file = cache.open_file(filename)?;
    let length = file
        .metadata()
        .map_err(|_| UpdateError::CacheUnavailable)?
        .len();
    if length == 0 || length > maximum {
        return Err(UpdateError::DownloadTooLarge);
    }
    let mut file = tokio::fs::File::from_std(file);
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 128 * 1024];
    let mut count = 0u64;
    loop {
        let n = tokio::select! { _ = cancellation.cancelled() => return Err(UpdateError::Cancelled), n = file.read(&mut buffer) => n.map_err(|_| UpdateError::CacheUnavailable)? };
        if n == 0 {
            break;
        }
        count = count.saturating_add(n as u64);
        if count > maximum {
            return Err(UpdateError::DownloadTooLarge);
        }
        hasher.update(&buffer[..n]);
    }
    if count != length || format!("{:x}", hasher.finalize()) != expected {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(count)
}

pub(super) async fn stream_installer(
    mut response: reqwest::Response,
    cache: &PrivateUpdateCache,
    filename: &str,
    expected: &str,
    maximum: u64,
    cancellation: &CancellationSignal,
    progress: impl Fn(u64, Option<u64>),
) -> UpdateResult<u64> {
    if !response.status().is_success() {
        return Err(UpdateError::DownloadFailed);
    }
    let total = response.content_length();
    if total.is_some_and(|v| v == 0 || v > maximum) {
        return Err(UpdateError::DownloadTooLarge);
    }
    let mut file = tokio::fs::File::from_std(cache.create_file("installer.part")?);
    let result = async {
        let mut hash = Sha256::new();
        let mut count = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| UpdateError::DownloadFailed)?
        {
            count = count.saturating_add(chunk.len() as u64);
            if count > maximum {
                return Err(UpdateError::DownloadTooLarge);
            }
            file.write_all(&chunk)
                .await
                .map_err(|_| UpdateError::CacheUnavailable)?;
            hash.update(&chunk);
            progress(count, total);
        }
        if count == 0 || total.is_some_and(|v| v != count) {
            return Err(UpdateError::DownloadFailed);
        }
        if format!("{:x}", hash.finalize()) != expected {
            return Err(UpdateError::ChecksumMismatch);
        }
        file.sync_all()
            .await
            .map_err(|_| UpdateError::CacheUnavailable)?;
        Ok(count)
    };
    let result = tokio::select! { biased; _ = cancellation.cancelled() => Err(UpdateError::Cancelled), result = result => result };
    drop(file);
    match result {
        Ok(count) => {
            cache.commit("installer.part", filename)?;
            Ok(count)
        }
        Err(error) => {
            let _ = cache.remove_file("installer.part");
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests;
