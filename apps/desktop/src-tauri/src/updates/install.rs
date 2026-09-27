mod context;
mod native;
mod prepare;
pub use context::{assess_installation, InstallContext};

use super::download::{
    verify_file, DownloadPhase, InstallationKind, PendingInstall, UpdateManager, UpdateRecord,
};
use crate::operation::CancellationSignal;
use native::LaunchOutcome;
use webcodex_environment::unified_update::{
    self as unified, InstallerPlatform, PrivateUpdateCache, UpdateError, UpdateResult,
};
use webcodex_environment::{
    upgrade_observation, EnvironmentStore, NativeEnvironment, UpgradeObservation, UpgradeOutcome,
};

impl UpdateManager {
    /// The only installer admission. No check, preference, timer or download
    /// completion calls this method, and the frontend supplies no executable,
    /// path, URL, environment id, installer flags or administrator credential.
    pub async fn install(
        &self,
        context: &InstallContext,
        version: &str,
        confirmed: bool,
    ) -> UpdateResult<bool> {
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
        if state.version.as_deref() != Some(version)
            || state.phase != DownloadPhase::ReadyToInstall
            || !self.snapshot().can_install
            || !super::is_newer_stable(version, env!("CARGO_PKG_VERSION"))
        {
            return Err(UpdateError::UpgradePreflightFailed);
        }
        if !native::supported() {
            return Err(UpdateError::UnsupportedPlatform);
        }
        self.change(|state| {
            state.phase = DownloadPhase::Preparing;
            state.error_kind = None;
        });
        self.persist(&cache)?;
        let result = self.install_locked(&cache, context, version).await;
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
    ) -> UpdateResult<bool> {
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
        let platform = InstallerPlatform::current().ok_or(UpdateError::UnsupportedPlatform)?;
        if let Err(error) = context::verify_installed_generation(context, platform).await {
            if error == UpdateError::ProvenanceFailed {
                self.set_installation(InstallationKind::UnmanagedInstallation);
            }
            return Err(error);
        }
        let release = unified::fetch_release(version, platform)
            .await?
            .ok_or(UpdateError::ManifestMissing)?;
        let entry = release.manifest.entry(platform)?;
        let target = self.target_cache(cache)?;
        if let Err(error) = verify_file(
            &target,
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
                target.remove_file(&entry.filename)?;
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
        let package = target.file(&entry.filename)?;
        #[cfg(unix)]
        {
            let candidate = prepare::extract_candidate(&target, &package, platform).await?;
            let checked = webcodex_environment::verify_upgrade_candidate(&candidate)
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
            let prepared = backend.upgrade_prepare(&store, &candidate).await;
            if !prepared.is_ok_and(|ready| ready.ready) {
                self.restore_unlaunched(cache, &store).await?;
                return Err(UpdateError::UpgradePreflightFailed);
            }
            let observed = upgrade_observation(&store)
                .map_err(|_| UpdateError::RecoveryRequired)?
                .ok_or(UpdateError::RecoveryRequired)?;
            if !matches_target(&self.current(), &observed)
                || observed.outcome != UpgradeOutcome::Pending
            {
                return Err(UpdateError::RecoveryRequired);
            }
            self.change(|record| {
                if let Some(pending) = &mut record.pending {
                    pending.operation_id = Some(observed.operation_id.clone());
                }
            });
            if self.persist(cache).is_err() {
                self.restore_unlaunched(cache, &store).await?;
                return Err(UpdateError::CacheUnavailable);
            }
            let outcome = native::launch_unix(
                &context.binaries.cli,
                &store.root().join("upgrade-prepared.json"),
                &candidate,
                &package,
                version,
                &observed.operation_id,
            )
            .await;
            match outcome {
                LaunchOutcome::Started => self.handed_off(cache),
                LaunchOutcome::NotStarted(error) => {
                    self.restore_unlaunched(cache, &store).await?;
                    Err(error)
                }
                LaunchOutcome::Unknown => Err(UpdateError::RecoveryRequired),
            }
        }
        #[cfg(windows)]
        {
            // The verified *outer* NSIS bootstrap owns canonical extraction,
            // preflight/prepare, inner package invocation, finish and rollback.
            // Desktop must not duplicate a weaker preflight from the bare .exe.
            self.remember_pending(cache, context)?;
            match native::launch_windows(&package, &entry.sha256, store.root()) {
                LaunchOutcome::Started => self.handed_off(cache),
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
                started_at_ms: crate::runtime_selection::now_ms(),
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

    fn handed_off(&self, cache: &PrivateUpdateCache) -> UpdateResult<bool> {
        self.change(|state| {
            state.phase = DownloadPhase::InstallingOrHandedOff;
            state.error_kind = None;
        });
        // The pre-dispatch pending record is already durable. A failed final
        // status write must not keep Windows' running Desktop image open.
        let _ = self.persist(cache);
        Ok(true)
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
                        .upgrade_rollback(store)
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
        let record = self.current();
        let root = webcodex_environment::default_environment_dir()
            .map_err(|_| UpdateError::RecoveryRequired)?;
        if !root.join("environment.json").is_file() {
            return Err(UpdateError::RecoveryRequired);
        }
        let store = EnvironmentStore::open(root).map_err(|_| UpdateError::RecoveryRequired)?;
        let observation = upgrade_observation(&store).map_err(|_| UpdateError::RecoveryRequired)?;
        let current = crate::commands::get_desktop_build_info();
        match reconcile(
            &record,
            observation.as_ref(),
            &current,
            crate::runtime_selection::now_ms(),
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
    current: &webcodex_core::desktop_runtime_contract::MachineBuildInfo,
    now: u64,
) -> Reconciliation {
    let Some(pending) = &record.pending else {
        return Reconciliation::RecoveryRequired;
    };
    let Some(observed) = observed else {
        return Reconciliation::RecoveryRequired;
    };
    if observed.environment_id != pending.environment_id {
        return Reconciliation::RecoveryRequired;
    }
    let at_least_target = record.version.as_deref().is_some_and(|target| {
        current.version == target || super::is_newer_stable(&current.version, target)
    });
    if at_least_target
        && observed.outcome == UpgradeOutcome::Committed
        && current.version == observed.version
        && current.git_dirty == Some(false)
        && current.git_commit.as_deref() == Some(observed.source_sha.as_str())
    {
        return Reconciliation::Installed;
    }
    if !matches_target(record, observed) {
        return Reconciliation::RecoveryRequired;
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
