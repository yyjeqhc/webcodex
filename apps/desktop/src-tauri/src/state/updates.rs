use super::*;
use crate::runtime_selection;
use crate::updates::{self, UpdateStatus};

impl AppState {
    pub async fn check_for_updates(&self, manual: bool) -> DesktopResult<UpdateStatus> {
        let _guard = self.update_check.lock().await;
        let now = runtime_selection::now_ms();
        let (cache, installed) = {
            let slot = self.core.lock().await;
            let core = slot
                .as_ref()
                .ok_or_else(|| runtime_selection::error("desktop_operation_busy"))?;
            (
                core.config.update_cache.clone(),
                core.adapter
                    .binaries()
                    .map(|b| b.version.clone())
                    .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into()),
            )
        };
        if !cache.should_check(now, manual) {
            return Ok(cache.status(&installed, now, true, None));
        }
        // Network is never performed under the core/operation lock and does not
        // affect Runtime health, tray attention, or global error presentation.
        let fetched = updates::fetch_latest().await;
        let mut slot = self.core.lock().await;
        let Some(core) = slot.as_mut() else {
            return Ok(cache.status(
                &installed,
                now,
                false,
                manual.then(|| "update_check_not_saved_runtime_busy".into()),
            ));
        };
        if core.configuration_issue.is_some() {
            return Ok(cache.status(
                &installed,
                now,
                false,
                manual.then(|| "configuration_migration_failed".into()),
            ));
        };
        let before = core.config.update_cache.clone();
        core.config.update_cache.last_check_at_ms = Some(now);
        if let Ok(latest) = &fetched {
            core.config.update_cache.last_success_at_ms = Some(now);
            core.config.update_cache.latest = latest.clone();
        }
        let status = core.config.update_cache.status(
            &installed,
            now,
            false,
            if manual && fetched.is_err() {
                Some("update_check_unavailable".into())
            } else {
                None
            },
        );
        if core.persist_runtime_preferences().await.is_err() {
            core.config.update_cache = before;
        }
        Ok(status)
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
        let installed = core
            .adapter
            .binaries()
            .map(|b| b.version.as_str())
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        Ok(core.config.update_cache.status(installed, now, true, None))
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
