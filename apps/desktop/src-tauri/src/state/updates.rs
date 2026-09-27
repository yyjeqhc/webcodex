use super::*;
use crate::runtime_selection;
use crate::updates::{
    self, DownloadStatus, InstallContext, InstallationKind, UpdateCache, UpdateStatus,
};
use webcodex_environment::unified_update::UpdateError;

const DESKTOP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn action_error() -> DesktopError {
    DesktopError::new(
        "update_action_unavailable",
        "The update action could not be completed",
        "Review the update status and any recovery instructions before retrying.",
    )
}

impl AppState {
    async fn update_installation_context(
        &self,
    ) -> DesktopResult<(InstallationKind, Option<InstallContext>)> {
        let (binaries, source, environment_id) = {
            let slot = self.core.lock().await;
            let core = slot
                .as_ref()
                .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
            (
                core.adapter.binaries().ok().cloned(),
                core.config.runtime_binary_source.clone(),
                core.config.persistent_environment.clone(),
            )
        };
        Ok(updates::assess_installation(
            crate::commands::get_desktop_build_info(),
            binaries,
            source,
            environment_id,
        ))
    }

    fn update_status(
        &self,
        cache: &UpdateCache,
        now: u64,
        cached: bool,
        error: Option<String>,
    ) -> UpdateStatus {
        let mut status = cache.status(DESKTOP_VERSION, now, cached, error);
        status.download = self.updates.snapshot();
        status
    }

    async fn request_automatic_download(&self, cache: &UpdateCache) {
        if let Ok((kind, _)) = self.update_installation_context().await {
            self.updates
                .request(cache.latest.clone(), cache.automatic_download, false, kind);
        }
    }

    pub async fn check_for_updates(&self, manual: bool) -> DesktopResult<UpdateStatus> {
        let _guard = self.update_check.lock().await;
        let now = runtime_selection::now_ms();
        let cache = {
            let slot = self.core.lock().await;
            slot.as_ref()
                .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?
                .config
                .update_cache
                .clone()
        };
        if !cache.should_check(now, manual) {
            self.request_automatic_download(&cache).await;
            return Ok(self.update_status(&cache, now, true, None));
        }
        // No network under the Runtime core lock. Download retry cadence is
        // independent of this 24-hour metadata discovery cadence.
        let fetched = updates::fetch_latest().await;
        let mut slot = self.core.lock().await;
        let Some(core) = slot.as_mut() else {
            return Ok(self.update_status(
                &cache,
                now,
                false,
                manual.then(|| "update_check_not_saved_runtime_busy".into()),
            ));
        };
        if core.configuration_issue.is_some() {
            return Ok(self.update_status(
                &cache,
                now,
                false,
                manual.then(|| "configuration_migration_failed".into()),
            ));
        }
        let before = core.config.update_cache.clone();
        core.config.update_cache.last_check_at_ms = Some(now);
        if let Ok(latest) = &fetched {
            if core
                .config
                .update_cache
                .latest
                .as_ref()
                .map(|release| &release.version)
                != latest.as_ref().map(|release| &release.version)
            {
                core.config.update_cache.remind_after_ms = None;
            }
            core.config.update_cache.last_success_at_ms = Some(now);
            core.config.update_cache.latest = latest.clone();
        }
        let mut error = (manual && fetched.is_err()).then(|| "update_check_unavailable".into());
        if core.persist_runtime_preferences().await.is_err() {
            core.config.update_cache = before;
            if manual {
                error = Some("update_settings_not_saved".into());
            }
        }
        let cache = core.config.update_cache.clone();
        drop(slot);
        self.request_automatic_download(&cache).await;
        Ok(self.update_status(&cache, now, false, error))
    }

    pub fn get_update_download_state(&self) -> DownloadStatus {
        self.updates.snapshot()
    }

    pub async fn download_update(&self, version: &str) -> DesktopResult<UpdateStatus> {
        let cache = {
            let slot = self.core.lock().await;
            slot.as_ref()
                .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?
                .config
                .update_cache
                .clone()
        };
        if cache
            .latest
            .as_ref()
            .filter(|release| updates::valid_notice(release) && release.version == version)
            .is_none()
        {
            return Err(action_error());
        }
        let (kind, _) = self.update_installation_context().await?;
        self.updates
            .request(cache.latest.clone(), cache.automatic_download, true, kind);
        tokio::task::yield_now().await;
        Ok(self.update_status(&cache, runtime_selection::now_ms(), true, None))
    }

    pub fn cancel_update_download(&self) -> DownloadStatus {
        self.updates.cancel_download(true);
        self.updates.snapshot()
    }

    pub async fn set_automatic_update_download(
        &self,
        enabled: bool,
    ) -> DesktopResult<UpdateStatus> {
        let mut slot = self.core.lock().await;
        let core = slot
            .as_mut()
            .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
        if core.configuration_issue.is_some() {
            return Err(runtime_selection::error("configuration_migration_failed"));
        }
        let before = core.config.update_cache.clone();
        core.config.update_cache.automatic_download = enabled;
        if let Err(error) = core.persist_runtime_preferences().await {
            core.config.update_cache = before;
            return Err(error);
        }
        let cache = core.config.update_cache.clone();
        drop(slot);
        if enabled {
            self.request_automatic_download(&cache).await;
        } else {
            self.updates.cancel_download(false);
        }
        Ok(self.update_status(&cache, runtime_selection::now_ms(), true, None))
    }

    pub async fn install_verified_update(
        &self,
        version: &str,
        confirmed: bool,
    ) -> DesktopResult<bool> {
        if !confirmed {
            return Err(action_error());
        }
        let (operation, cancellation, core, baseline) = self
            .begin_operation(DesktopOperationKind::DesktopUpdate, false)
            .await?;
        let (kind, context) = updates::assess_installation(
            crate::commands::get_desktop_build_info(),
            core.adapter.binaries().ok().cloned(),
            core.config.runtime_binary_source.clone(),
            core.config.persistent_environment.clone(),
        );
        self.updates.set_installation(kind);
        let result = match context {
            Some(context) => self.updates.install(&context, version, confirmed).await,
            None => Err(UpdateError::UpgradePreflightFailed),
        };
        if let Err(error) = result {
            self.updates.note_action_error(error);
        }
        let operation_result = result
            .map(|_| core.snapshot.clone())
            .map_err(|_| action_error());
        let _ = self
            .finish_operation(operation, cancellation, core, baseline, operation_result)
            .await;
        result.map_err(|_| action_error())
    }

    pub async fn remind_update_later(&self) -> DesktopResult<UpdateStatus> {
        let mut slot = self.core.lock().await;
        let core = slot
            .as_mut()
            .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
        if core.configuration_issue.is_some() {
            return Err(runtime_selection::error("configuration_migration_failed"));
        }
        let before = core.config.update_cache.clone();
        let now = runtime_selection::now_ms();
        core.config.update_cache.remind_after_ms =
            Some(now.saturating_add(updates::CHECK_INTERVAL_MS));
        if let Err(error) = core.persist_runtime_preferences().await {
            core.config.update_cache = before;
            return Err(error);
        }
        Ok(self.update_status(&core.config.update_cache, now, true, None))
    }

    pub async fn open_latest_release(&self) -> DesktopResult<()> {
        let slot = self.core.lock().await;
        let core = slot
            .as_ref()
            .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
        let notice = core
            .config
            .update_cache
            .latest
            .as_ref()
            .filter(|notice| updates::valid_notice(notice))
            .ok_or_else(|| runtime_selection::error("release_unavailable"))?;
        let url = notice.release_url.clone();
        drop(slot);
        crate::platform::opener::url(&url)
    }
}
