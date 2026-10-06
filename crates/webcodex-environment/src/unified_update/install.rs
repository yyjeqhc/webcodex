pub(crate) mod context;
#[cfg(any(unix, test))]
mod prepare;
pub use context::detected_installer_target;
pub use context::{assess_installation, InstallContext, InstalledBinaries};

use super::download::{
    verify_file, DownloadPhase, InstallationKind, PendingInstall, UpdateManager, UpdateRecord,
};
use super::CancellationSignal;
use crate::unified_update::{self as unified, PrivateUpdateCache, UpdateError, UpdateResult};
#[cfg(unix)]
use crate::NativeEnvironment;
use crate::{
    upgrade_observation, EnvironmentStore, UpgradeObservation, UpgradeOutcome, UpgradeTarget,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchOutcome {
    Started,
    StartedWithOperation(String),
    NotStarted(UpdateError),
    Unknown,
}

/// Only engine-validated local paths reach this host adapter. The adapter owns
/// OS authorization and process handoff, never candidate or journal authority.
pub struct LaunchRequest<'a> {
    pub cli: &'a std::path::Path,
    pub receipt: &'a std::path::Path,
    pub candidate: &'a std::path::Path,
    pub installer: &'a std::path::Path,
    pub version: &'a str,
    pub operation_id: &'a str,
    pub target: unified::InstallerTarget,
    pub installer_sha256: &'a str,
    pub environment_root: &'a std::path::Path,
    pub windows_handoff: Option<&'a super::WindowsHandoff>,
}
pub trait LaunchAdapter: Send + Sync {
    fn supported(&self, target: unified::InstallerTarget) -> bool;
    fn launch<'a>(
        &'a self,
        request: LaunchRequest<'a>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = LaunchOutcome> + Send + 'a>>;
}

impl UpdateManager {
    /// The only installer admission. No check, preference, timer or download
    /// completion calls this method, and the frontend supplies no executable,
    /// path, URL, environment id, installer flags or administrator credential.
    pub async fn install_checked(
        &self,
        context: &InstallContext,
        identity: &super::CandidateIdentity,
        target_identity: &UpgradeTarget,
        confirmed: bool,
        adapter: &impl LaunchAdapter,
    ) -> UpdateResult<LaunchOutcome> {
        let version = identity.version.as_str();
        if !confirmed {
            return Err(UpdateError::AuthorizationRequired);
        }
        let _attempt = self
            .attempt
            .try_lock()
            .map_err(|_| UpdateError::UpgradePreflightFailed)?;
        let cache = PrivateUpdateCache::open(self.root.clone())?;
        let _process_lock = cache.lock().map_err(|_| UpdateError::RecoveryRequired)?;
        self.load(&cache)?;
        let state = self.current();
        if state.pending.is_some() {
            return Err(UpdateError::RecoveryRequired);
        }
        if !identity.matches_record(&state)
            || target_identity.environment_id != context.environment_id
            || target_identity.manifest_sha256 != identity.manifest_sha256
            || state.version.as_deref() != Some(version)
            || !(state.phase == DownloadPhase::ReadyToInstall
                || (state.phase == DownloadPhase::Available && state.verified_at_ms.is_some()))
            || self.snapshot().installation != InstallationKind::Managed
            || !super::is_newer_stable(version, &context.build.version)
        {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        let target = state.target.ok_or(UpdateError::UnsupportedPlatform)?;
        if target != context.target || !adapter.supported(target) {
            return Err(UpdateError::UnsupportedPlatform);
        }
        self.change(|state| {
            state.phase = DownloadPhase::Preparing;
            state.error_kind = None;
        });
        self.persist(&cache)?;
        let result = self
            .install_locked(&cache, context, version, target_identity, adapter)
            .await;
        if let Err(error) = result {
            self.record_failure(&cache, error);
        }
        result
    }

    async fn install_locked(
        &self,
        cache: &PrivateUpdateCache,
        context: &InstallContext,
        version: &str,
        target_identity: &UpgradeTarget,
        adapter: &impl LaunchAdapter,
    ) -> UpdateResult<LaunchOutcome> {
        #[cfg(not(unix))]
        let _ = target_identity;
        let store = context::environment(context)?;
        if upgrade_observation(&store)
            .map_err(|_| UpdateError::RecoveryRequired)?
            .is_some_and(|state| {
                !matches!(
                    state.outcome,
                    UpgradeOutcome::Committed | UpgradeOutcome::RolledBack
                )
            })
        {
            return Err(UpdateError::RecoveryRequired);
        }
        let target = self
            .current()
            .target
            .ok_or(UpdateError::UnsupportedPlatform)?;
        if target != context.target {
            return Err(UpdateError::UnsupportedPlatform);
        }
        if let Err(error) = context::verify_installed_generation(context, target).await {
            if error == UpdateError::ProvenanceFailed {
                self.set_installation(InstallationKind::UnmanagedInstallation);
            }
            return Err(error);
        }
        let release = unified::fetch_release(version, target)
            .await?
            .ok_or(UpdateError::ManifestMissing)?;
        let entry = release.manifest.entry(target)?;
        let target_cache = self.target_cache(cache)?;
        if let Err(error) = verify_file(
            &target_cache,
            &entry.filename,
            &entry.sha256,
            unified::MAX_INSTALLER_BYTES,
            &CancellationSignal::new(),
        )
        .await
        {
            if matches!(
                error,
                UpdateError::ChecksumMismatch | UpdateError::DownloadTooLarge
            ) {
                target_cache.remove_file(&entry.filename)?;
            }
            return Err(error);
        }
        let state = self.current();
        if state.sha256.as_deref() != Some(&entry.sha256)
            || state.source_manifest_sha256.as_deref() != Some(&entry.source_manifest_sha256)
            || state.source_sha.as_deref() != Some(&release.source.source_sha)
        {
            return Err(UpdateError::ProvenanceFailed);
        }
        let package = target_cache.file(&entry.filename)?;
        #[cfg(unix)]
        {
            let candidate = prepare::extract_candidate(&target_cache, &package, target).await?;
            let checked = crate::verify_upgrade_candidate(&candidate)
                .map_err(|_| UpdateError::ProvenanceFailed)?;
            if checked.version != version
                || checked.manifest_sha256 != entry.source_manifest_sha256
                || checked.source_sha != release.source.source_sha
            {
                return Err(UpdateError::ProvenanceFailed);
            }
            let backend =
                NativeEnvironment::new().map_err(|_| UpdateError::UpgradePreflightFailed)?;
            let preflight = backend
                .upgrade_preflight(&store, &candidate)
                .await
                .map_err(|_| UpdateError::UpgradePreflightFailed)?;
            if !preflight.ready {
                return Err(UpdateError::UpgradePreflightFailed);
            }
            self.remember_pending(cache, context)?;
            let (prepared, prepared_receipt) = backend
                .upgrade_prepare_guarded_with_receipt(&store, &candidate, target_identity)
                .await
                .map_err(|_| UpdateError::RecoveryRequired)?;
            if !prepared.ready {
                return Err(UpdateError::RecoveryRequired);
            }
            // Bind the operation returned by this preparing invocation before
            // observing the journal again. A later same-candidate operation
            // must never become this launch's operation.
            self.change(|record| {
                if let Some(pending) = &mut record.pending {
                    pending.operation_id = Some(prepared_receipt.operation_id.clone());
                }
            });
            if self.persist(cache).is_err() {
                return Err(UpdateError::RecoveryRequired);
            }
            let observed = upgrade_observation(&store)
                .map_err(|_| UpdateError::RecoveryRequired)?
                .ok_or(UpdateError::RecoveryRequired)?;
            if !matches_target(&self.current(), &observed)
                || observed.outcome != UpgradeOutcome::Pending
            {
                return Err(UpdateError::RecoveryRequired);
            }
            let receipt = store.root().join("upgrade-prepared.json");
            let outcome = adapter
                .launch(LaunchRequest {
                    cli: &context.binaries.cli,
                    receipt: &receipt,
                    candidate: &candidate,
                    installer: &package,
                    version,
                    operation_id: &observed.operation_id,
                    target,
                    installer_sha256: &entry.sha256,
                    environment_root: store.root(),
                    windows_handoff: None,
                })
                .await;
            match outcome {
                LaunchOutcome::Started => self.handed_off(cache),
                LaunchOutcome::NotStarted(error) => {
                    self.restore_unlaunched(cache, &store).await?;
                    Err(error)
                }
                LaunchOutcome::Unknown | LaunchOutcome::StartedWithOperation(_) => {
                    Err(UpdateError::RecoveryRequired)
                }
            }
        }
        #[cfg(windows)]
        {
            // The verified *outer* NSIS bootstrap owns canonical extraction,
            // preflight/prepare, inner package invocation, finish and rollback.
            // Desktop must not duplicate a weaker preflight from the bare .exe.
            if !release.source.supports_guarded_windows_handoff() {
                return Err(UpdateError::GuardedHandoffUnavailable);
            }
            let handoff = super::WindowsHandoff::create(&target_cache, target_identity)?;
            self.remember_pending(cache, context)?;
            let receipt = store.root().join("upgrade-prepared.json");
            match adapter
                .launch(LaunchRequest {
                    cli: &context.binaries.cli,
                    receipt: &receipt,
                    candidate: store.root(),
                    installer: &package,
                    version,
                    operation_id: "",
                    target,
                    installer_sha256: &entry.sha256,
                    environment_root: store.root(),
                    windows_handoff: Some(&handoff),
                })
                .await
            {
                LaunchOutcome::StartedWithOperation(operation_id) => {
                    self.change(|state| {
                        if let Some(pending) = &mut state.pending {
                            pending.operation_id = Some(operation_id.clone());
                        }
                    });
                    self.persist(cache)?;
                    handoff.accept_prepared(&operation_id)?;
                    self.handed_off(cache)
                }
                LaunchOutcome::Started => Err(UpdateError::RecoveryRequired),
                LaunchOutcome::NotStarted(error) => {
                    self.change(|state| state.pending = None);
                    self.persist(cache)?;
                    Err(error)
                }
                LaunchOutcome::Unknown => Err(UpdateError::RecoveryRequired),
            }
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = package;
            Err(UpdateError::UnsupportedPlatform)
        }
    }

    fn remember_pending(
        &self,
        cache: &PrivateUpdateCache,
        context: &InstallContext,
    ) -> UpdateResult<()> {
        self.change(|state| {
            state.pending = Some(PendingInstall {
                environment_id: context.environment_id.clone(),
                operation_id: None,
                started_at_ms: super::now_ms(),
            })
        });
        // This write must succeed *before* Core prepare or the Windows outer
        // installer can change anything. Never start with an in-memory receipt.
        if let Err(error) = self.persist(cache) {
            self.change(|state| state.pending = None);
            return Err(error);
        }
        Ok(())
    }

    fn handed_off(&self, cache: &PrivateUpdateCache) -> UpdateResult<LaunchOutcome> {
        self.change(|state| {
            state.phase = DownloadPhase::InstallingOrHandedOff;
            state.error_kind = None;
        });
        // The pre-dispatch pending record is already durable. A failed final
        // status write must not keep Windows' running Desktop image open.
        let _ = self.persist(cache);
        Ok(LaunchOutcome::Started)
    }

    #[cfg(unix)]
    async fn restore_unlaunched(
        &self,
        cache: &PrivateUpdateCache,
        store: &EnvironmentStore,
    ) -> UpdateResult<()> {
        let record = self.current();
        let observation = upgrade_observation(store).map_err(|_| UpdateError::RecoveryRequired)?;
        match observation {
            Some(observation) if matches_target(&record, &observation) => {
                if observation.outcome == UpgradeOutcome::Committed {
                    return Err(UpdateError::RecoveryRequired);
                }
                if observation.outcome != UpgradeOutcome::RolledBack {
                    NativeEnvironment::new()
                        .map_err(|_| UpdateError::RecoveryRequired)?
                        .upgrade_rollback_guarded(
                            store,
                            &UpgradeTarget {
                                environment_id: observation.environment_id.clone(),
                                manifest_sha256: observation.manifest_sha256.clone(),
                                operation_id: Some(observation.operation_id.clone()),
                            },
                        )
                        .await
                        .map_err(|_| UpdateError::RecoveryRequired)?;
                }
            }
            None if record
                .pending
                .as_ref()
                .is_some_and(|p| p.operation_id.is_none()) => {}
            Some(observation)
                if record
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.operation_id.is_none())
                    && matches!(
                        observation.outcome,
                        UpgradeOutcome::Committed | UpgradeOutcome::RolledBack
                    ) => {}
            _ => return Err(UpdateError::RecoveryRequired),
        }
        self.change(|state| state.pending = None);
        self.persist(cache)
    }

    pub(super) async fn reconcile_pending(&self, cache: &PrivateUpdateCache) -> UpdateResult<()> {
        self.reconcile_pending_with_target(cache, None).await
    }

    /// Reconcile only the selected terminal operation after an explicit owner
    /// action. Status remains read-only; this never starts or recovers services.
    pub async fn reconcile_pending_guarded(&self, target: &UpgradeTarget) -> UpdateResult<()> {
        let operation_id = target
            .operation_id
            .as_deref()
            .ok_or(UpdateError::RecoveryRequired)?;
        if uuid::Uuid::parse_str(operation_id).is_err()
            || !unified::valid_sha256(&target.manifest_sha256)
        {
            return Err(UpdateError::RecoveryRequired);
        }
        let _attempt = self
            .attempt
            .try_lock()
            .map_err(|_| UpdateError::RecoveryRequired)?;
        let Some(cache) = PrivateUpdateCache::open_existing_for_update(self.root.clone())? else {
            return Ok(());
        };
        let _lock = cache.lock_existing_exclusive()?;
        let record = match cache.read("update-state.json", 24 * 1024)? {
            Some(bytes) => serde_json::from_slice::<UpdateRecord>(&bytes)
                .map_err(|_| UpdateError::RecoveryRequired)?,
            None => return Ok(()),
        };
        if !record.valid()
            || record
                .target
                .is_some_and(|value| Some(value.platform) != unified::RuntimePlatform::current())
        {
            return Err(UpdateError::RecoveryRequired);
        }
        if record.pending.is_none() {
            return Ok(());
        }
        if !pending_matches_guard(&record, target) {
            return Err(UpdateError::RecoveryRequired);
        }
        self.change(|state| *state = record);
        self.reconcile_pending_with_target(&cache, Some(target))
            .await?;
        if self.current().pending.is_some() {
            return Err(UpdateError::RecoveryRequired);
        }
        Ok(())
    }

    async fn reconcile_pending_with_target(
        &self,
        cache: &PrivateUpdateCache,
        target: Option<&UpgradeTarget>,
    ) -> UpdateResult<()> {
        let record = self.current();
        let root = self.environment_root.clone();
        if !root.join("environment.json").is_file() {
            return Err(UpdateError::RecoveryRequired);
        }
        let store = if target.is_some() {
            EnvironmentStore::open_existing(root)
                .map_err(|_| UpdateError::RecoveryRequired)?
                .ok_or(UpdateError::RecoveryRequired)?
        } else {
            EnvironmentStore::open(root).map_err(|_| UpdateError::RecoveryRequired)?
        };
        let observation = if let Some(target) = target {
            let _lock = store
                .lock_existing()
                .map_err(|_| UpdateError::RecoveryRequired)?;
            let observation = crate::upgrade::upgrade_observation_under_lock(&store)
                .map_err(|_| UpdateError::RecoveryRequired)?;
            if !observation
                .as_ref()
                .is_some_and(|observed| terminal_matches_guard(&record, observed, target))
            {
                return Err(UpdateError::RecoveryRequired);
            }
            observation
        } else {
            upgrade_observation(&store).map_err(|_| UpdateError::RecoveryRequired)?
        };
        let current = match observation
            .as_ref()
            .filter(|o| o.outcome == UpgradeOutcome::Committed)
        {
            Some(observed) => {
                let cached_source = self
                    .target_cache(cache)?
                    .read("source-manifest.json", unified::MAX_SOURCE_BYTES)?
                    .and_then(|bytes| {
                        unified::verify_source_manifest(
                            &bytes,
                            &observed.version,
                            record.target?.platform,
                        )
                        .ok()
                    })
                    .filter(|source| {
                        source.manifest_sha256 == observed.manifest_sha256
                            && source.source_sha == observed.source_sha
                    });
                context::verify_observed_generation(
                    &store,
                    observed,
                    record.target.ok_or(UpdateError::RecoveryRequired)?,
                    cached_source.as_ref(),
                )
                .await
                .ok()
            }
            None => None,
        };
        // The cache fence prevents another updater from replacing pending;
        // retain the Core fence through the final recheck and cleanup as well.
        let _environment_lock = target
            .map(|_| store.lock_existing())
            .transpose()
            .map_err(|_| UpdateError::RecoveryRequired)?;
        if let Some(target) = target {
            let latest = crate::upgrade::upgrade_observation_under_lock(&store)
                .map_err(|_| UpdateError::RecoveryRequired)?;
            if !latest.as_ref().is_some_and(|latest| {
                terminal_matches_guard(&record, latest, target)
                    && observation.as_ref().is_some_and(|before| {
                        latest.outcome == before.outcome
                            && latest.version == before.version
                            && latest.source_sha == before.source_sha
                    })
            }) {
                return Err(UpdateError::RecoveryRequired);
            }
        }
        match reconcile(
            &record,
            observation.as_ref(),
            current.as_ref(),
            super::now_ms(),
        ) {
            Reconciliation::Installed => {
                cache.retain_version(None)?;
                self.change(|state| *state = UpdateRecord::default());
            }
            Reconciliation::Restored => self.change(|state| {
                state.pending = None;
                state.phase = DownloadPhase::Failed;
                state.error_kind = Some(UpdateError::UpgradeRolledBack);
                state.cancelled = true; // A failed installation is never automatically retried.
            }),
            Reconciliation::InProgress => self.change(|state| {
                state.phase = DownloadPhase::InstallingOrHandedOff;
                state.error_kind = None;
            }),
            Reconciliation::RecoveryRequired => self.change(|state| {
                state.phase = DownloadPhase::Failed;
                state.error_kind = Some(UpdateError::RecoveryRequired);
            }),
        }
        self.persist(cache)
    }
}

fn pending_matches_guard(record: &UpdateRecord, target: &UpgradeTarget) -> bool {
    target.operation_id.is_some()
        && record.source_manifest_sha256.as_deref() == Some(&target.manifest_sha256)
        && record.pending.as_ref().is_some_and(|pending| {
            pending.environment_id == target.environment_id
                && pending.operation_id == target.operation_id
        })
}

fn terminal_matches_guard(
    record: &UpdateRecord,
    observed: &UpgradeObservation,
    target: &UpgradeTarget,
) -> bool {
    pending_matches_guard(record, target)
        && target.operation_id.as_deref() == Some(&observed.operation_id)
        && target.environment_id == observed.environment_id
        && target.manifest_sha256 == observed.manifest_sha256
        && matches!(
            observed.outcome,
            UpgradeOutcome::Committed | UpgradeOutcome::RolledBack
        )
        && matches_target(record, observed)
}

fn matches_target(record: &UpdateRecord, observed: &UpgradeObservation) -> bool {
    record.pending.as_ref().is_some_and(|pending| {
        pending.environment_id == observed.environment_id
            && pending
                .operation_id
                .as_ref()
                .is_none_or(|operation| *operation == observed.operation_id)
    }) && record.version.as_deref() == Some(observed.version.as_str())
        && record.source_sha.as_deref() == Some(observed.source_sha.as_str())
        && record.source_manifest_sha256.as_deref() == Some(observed.manifest_sha256.as_str())
}

#[derive(Debug, PartialEq, Eq)]
enum Reconciliation {
    Installed,
    Restored,
    InProgress,
    RecoveryRequired,
}
fn reconcile(
    record: &UpdateRecord,
    observed: Option<&UpgradeObservation>,
    current: Option<&webcodex_core::desktop_runtime_contract::MachineBuildInfo>,
    now: u64,
) -> Reconciliation {
    let Some(pending) = &record.pending else {
        return Reconciliation::RecoveryRequired;
    };
    let Some(observed) = observed else {
        return Reconciliation::RecoveryRequired;
    };
    if pending.operation_id.is_none()
        || observed.environment_id != pending.environment_id
        || !matches_target(record, observed)
    {
        return Reconciliation::RecoveryRequired;
    }
    let at_least_target = record.version.as_deref().is_some_and(|target| {
        current.is_some_and(|current| {
            current.version == target || super::is_newer_stable(&current.version, target)
        })
    });
    if at_least_target
        && observed.outcome == UpgradeOutcome::Committed
        && current.is_some_and(|current| {
            current.version == observed.version
                && current.git_dirty == Some(false)
                && current.git_commit.as_deref() == Some(observed.source_sha.as_str())
        })
    {
        return Reconciliation::Installed;
    }
    if observed.outcome == UpgradeOutcome::RolledBack {
        return Reconciliation::Restored;
    }
    if observed.outcome == UpgradeOutcome::Pending
        && pending.started_at_ms <= now.saturating_add(60_000)
        && now.saturating_sub(pending.started_at_ms) < 30 * 60 * 1000
    {
        return Reconciliation::InProgress;
    }
    Reconciliation::RecoveryRequired
}

#[cfg(test)]
mod tests;
