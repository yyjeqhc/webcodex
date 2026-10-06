//! Stable discovery plus a private verified unified-installer workflow.
//! Background checks/downloads never change Runtime health or install anything.
mod download;
mod install;
pub use download::{DownloadStatus, InstallationKind, UpdateManager};
pub(crate) use install::detected_installer_target;
pub use install::{assess_installation, InstallContext};
use serde::{Deserialize, Serialize};

pub const CHECK_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;

#[cfg(test)]
use webcodex_environment::unified_update::UpdateCompatibility;
pub use webcodex_environment::unified_update::{
    fetch_latest, is_newer_stable, valid_notice, ReleaseNotice,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCache {
    pub last_check_at_ms: Option<u64>,
    pub last_success_at_ms: Option<u64>,
    pub latest: Option<ReleaseNotice>,
    pub remind_after_ms: Option<u64>,
    #[serde(default = "automatic_download_default")]
    pub automatic_download: bool,
}

fn automatic_download_default() -> bool {
    true
}
impl Default for UpdateCache {
    fn default() -> Self {
        Self {
            last_check_at_ms: None,
            last_success_at_ms: None,
            latest: None,
            remind_after_ms: None,
            automatic_download: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateStatus {
    pub state: String,
    pub latest: Option<ReleaseNotice>,
    pub update_available: bool,
    pub show_banner: bool,
    pub cached: bool,
    pub last_check_at_ms: Option<u64>,
    pub manual_error: Option<String>,
    pub automatic_download: bool,
    pub download: DownloadStatus,
}

impl UpdateCache {
    pub fn should_check(&self, now: u64, manual: bool) -> bool {
        manual
            || self.last_check_at_ms.is_none_or(|last| {
                now.saturating_sub(last) >= CHECK_INTERVAL_MS
                    || last > now.saturating_add(CHECK_INTERVAL_MS)
            })
    }
    pub fn status(
        &self,
        installed: &str,
        now: u64,
        cached: bool,
        manual_error: Option<String>,
    ) -> UpdateStatus {
        // Old or hand-edited persisted cache never turns an arbitrary URL into
        // an opener action, nor creates compatibility evidence of its own.
        let latest = self.latest.clone().filter(valid_notice);
        let update_available = latest
            .as_ref()
            .is_some_and(|notice| is_newer_stable(&notice.version, installed));
        UpdateStatus {
            state: if manual_error.is_some() {
                "check_failed"
            } else if update_available {
                "update_available"
            } else if latest.is_some() {
                "up_to_date"
            } else {
                "not_checked"
            }
            .into(),
            latest,
            update_available,
            show_banner: update_available && self.remind_after_ms.is_none_or(|after| now >= after),
            cached,
            last_check_at_ms: self.last_check_at_ms,
            manual_error,
            automatic_download: self.automatic_download,
            download: DownloadStatus::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use webcodex_environment::unified_update::discovery::{project_release, GithubRelease};
    fn release(tag: &str) -> GithubRelease {
        serde_json::from_value(json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":[]}))
            .unwrap()
    }
    #[test]
    fn check_cache_is_bounded_24h_and_manual_can_retry() {
        let cache = UpdateCache {
            last_check_at_ms: Some(1000),
            ..Default::default()
        };
        assert!(!cache.should_check(2000, false));
        assert!(cache.should_check(1000 + CHECK_INTERVAL_MS, false));
        assert!(cache.should_check(2000, true));
    }
    #[test]
    fn no_update_or_snooze_produces_no_banner() {
        let mut cache = UpdateCache {
            latest: project_release(&release("v0.5.0"), None),
            ..Default::default()
        };
        assert!(!cache.status("0.5.0", 1000, true, None).show_banner);
        assert!(cache.status("0.4.1", 1000, true, None).show_banner);
        cache.remind_after_ms = Some(2000);
        assert!(!cache.status("0.4.1", 1000, true, None).show_banner);
        assert!(cache.status("0.4.1", 2000, true, None).show_banner);
    }
    #[test]
    fn startup_failure_is_silent_manual_failure_is_an_ordinary_status() {
        let cache = UpdateCache::default();
        assert!(cache
            .status("0.4.1", 1000, false, None)
            .manual_error
            .is_none());
        let status = cache.status(
            "0.4.1",
            1000,
            false,
            Some("update_check_unavailable".into()),
        );
        assert_eq!(status.state, "check_failed");
        assert!(!status.update_available);
    }
    #[test]
    fn cache_cannot_inject_external_links() {
        let mut notice = project_release(&release("v0.5.0"), None).unwrap();
        notice.release_url = "https://evil.test/".into();
        assert!(!valid_notice(&notice));
        let cache = UpdateCache {
            latest: Some(notice),
            ..Default::default()
        };
        assert!(!cache.status("0.4.1", 1000, true, None).update_available);
    }
    #[test]
    fn old_preferences_default_enabled_and_runtime_version_cannot_hide_desktop_release() {
        let cache: UpdateCache = serde_json::from_str(r#"{"last_check_at_ms":null,"last_success_at_ms":null,"latest":null,"remind_after_ms":null}"#).unwrap();
        assert!(cache.automatic_download);
        let cache = UpdateCache {
            latest: Some(ReleaseNotice {
                version: "0.5.0".into(),
                runtime_version: "0.4.9".into(),
                release_url: "https://github.com/yyjeqhc/webcodex/releases/tag/v0.5.0".into(),
                compatibility: UpdateCompatibility::RuntimeCompatible,
            }),
            ..UpdateCache::default()
        };
        assert!(cache.status("0.4.9", 100, true, None).update_available);
        assert!(!cache.status("0.5.0", 100, true, None).update_available);
    }
}
