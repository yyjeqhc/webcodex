//! Best-effort stable-release discovery. No authentication, installation,
//! runtime-health changes, or dependency on the availability of GitHub.
use serde::{Deserialize, Serialize};
use std::time::Duration;
use webcodex_core::desktop_runtime_contract::{
    ReleaseManifest, DESKTOP_RUNTIME_CONTRACT, RELEASE_MANIFEST_SCHEMA_VERSION,
};

pub const CHECK_INTERVAL_MS: u64 = 24 * 60 * 60 * 1000;
const RESPONSE_BYTES: usize = 128 * 1024;
const MANIFEST_NAME: &str = "webcodex-release-manifest.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UpdateCompatibility {
    RuntimeCompatible,
    DesktopRequired,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseNotice {
    pub version: String,
    pub runtime_version: String,
    pub release_url: String,
    pub compatibility: UpdateCompatibility,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpdateCache {
    pub last_check_at_ms: Option<u64>,
    pub last_success_at_ms: Option<u64>,
    pub latest: Option<ReleaseNotice>,
    pub remind_after_ms: Option<u64>,
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
            .is_some_and(|notice| is_newer_stable(&notice.runtime_version, installed));
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
        }
    }
}

pub fn is_newer_stable(candidate: &str, installed: &str) -> bool {
    match (
        semver::Version::parse(candidate),
        semver::Version::parse(installed),
    ) {
        (Ok(candidate), Ok(installed)) => candidate.pre.is_empty() && candidate > installed,
        _ => false,
    }
}

pub fn valid_notice(notice: &ReleaseNotice) -> bool {
    let Ok(version) = semver::Version::parse(&notice.version) else {
        return false;
    };
    let Ok(runtime) = semver::Version::parse(&notice.runtime_version) else {
        return false;
    };
    if !version.pre.is_empty()
        || !runtime.pre.is_empty()
        || notice.version.len() > 96
        || notice.runtime_version.len() > 96
    {
        return false;
    }
    let expected = format!(
        "https://github.com/yyjeqhc/webcodex/releases/tag/v{}",
        notice.version
    );
    let alternate = format!(
        "https://github.com/yyjeqhc/webcodex/releases/tag/{}",
        notice.version
    );
    notice.release_url == expected || notice.release_url == alternate
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}
#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
}

pub(crate) fn project_release(
    release: &GithubRelease,
    manifest: Option<&[u8]>,
) -> Option<ReleaseNotice> {
    if release.draft || release.prerelease || release.tag_name.len() > 97 {
        return None;
    }
    let version = semver::Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )
    .ok()?;
    if !version.pre.is_empty() {
        return None;
    }
    let version = version.to_string();
    let mut notice = ReleaseNotice {
        version: version.clone(),
        runtime_version: version.clone(),
        release_url: format!(
            "https://github.com/yyjeqhc/webcodex/releases/tag/{}",
            release.tag_name
        ),
        compatibility: UpdateCompatibility::Unknown,
    };
    if let Some(manifest) =
        manifest.and_then(|bytes| serde_json::from_slice::<ReleaseManifest>(bytes).ok())
    {
        if manifest.schema_version == RELEASE_MANIFEST_SCHEMA_VERSION
            && manifest.release_version == version
            && manifest.desktop_runtime_contract.is_valid()
            && semver::Version::parse(&manifest.runtime_version)
                .is_ok_and(|version| version.pre.is_empty())
        {
            notice.runtime_version = manifest.runtime_version;
            notice.compatibility =
                if DESKTOP_RUNTIME_CONTRACT.overlaps(manifest.desktop_runtime_contract) {
                    UpdateCompatibility::RuntimeCompatible
                } else {
                    UpdateCompatibility::DesktopRequired
                };
        }
    }
    valid_notice(&notice).then_some(notice)
}

async fn bounded_json(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, ()> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| ())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > RESPONSE_BYTES as u64)
    {
        return Err(());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_BYTES {
            return Err(());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn fetch_latest() -> Result<Option<ReleaseNotice>, ()> {
    // One wall-clock budget covers both API and optional manifest, including
    // redirects. Missing manifest is unknown compatibility, not a failure.
    tokio::time::timeout(Duration::from_secs(10), async {
        let client = reqwest::Client::builder()
            .user_agent("WebCodex-Desktop-Update-Check")
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() > 4 {
                    return attempt.stop();
                }
                let url = attempt.url();
                if url.scheme() == "https"
                    && matches!(
                        url.host_str(),
                        Some(
                            "api.github.com"
                                | "github.com"
                                | "release-assets.githubusercontent.com"
                                | "objects.githubusercontent.com"
                        )
                    )
                    && url.username().is_empty()
                    && url.password().is_none()
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()
            .map_err(|_| ())?;
        let bytes = bounded_json(
            &client,
            "https://api.github.com/repos/yyjeqhc/webcodex/releases/latest",
        )
        .await?;
        let release: GithubRelease = serde_json::from_slice(&bytes).map_err(|_| ())?;
        let Some(base) = project_release(&release, None) else {
            return Ok(None);
        };
        let manifest = if release
            .assets
            .iter()
            .take(128)
            .any(|asset| asset.name == MANIFEST_NAME)
        {
            // Never trust browser_download_url from cache or an unrecognized
            // asset. Construct the fixed official repository/tag asset route.
            let url = format!(
                "https://github.com/yyjeqhc/webcodex/releases/download/{}/{MANIFEST_NAME}",
                release.tag_name
            );
            bounded_json(&client, &url).await.ok()
        } else {
            None
        };
        Ok(project_release(&release, manifest.as_deref()).or(Some(base)))
    })
    .await
    .map_err(|_| ())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn release(tag: &str) -> GithubRelease {
        serde_json::from_value(json!({"tag_name":tag,"draft":false,"prerelease":false,"assets":[]}))
            .unwrap()
    }
    fn manifest(min: u16, max: u16) -> Vec<u8> {
        serde_json::to_vec(&json!({"schema_version":1,"release_version":"0.5.0","runtime_version":"0.5.0","desktop_runtime_contract":{"min_generation":min,"max_generation":max},"future_field":true})).unwrap()
    }
    #[test]
    fn version_order_is_semver_not_lexical() {
        assert!(is_newer_stable("0.10.0", "0.9.9"));
        assert!(!is_newer_stable("0.9.0", "0.10.0"));
        assert!(!is_newer_stable("0.5.0", "0.5.0"));
        assert!(!is_newer_stable("0.6.0-beta.1", "0.5.0"));
    }
    #[test]
    fn drafts_prereleases_and_unverifiable_tags_are_ignored() {
        let mut r = release("v0.5.0");
        r.draft = true;
        assert!(project_release(&r, None).is_none());
        r.draft = false;
        r.prerelease = true;
        assert!(project_release(&r, None).is_none());
        assert!(project_release(&release("v0.5.0-beta.1"), None).is_none());
        assert!(project_release(&release("../../malicious"), None).is_none());
    }
    #[test]
    fn manifest_overlap_means_runtime_only_update() {
        assert_eq!(
            project_release(&release("v0.5.0"), Some(&manifest(1, 2)))
                .unwrap()
                .compatibility,
            UpdateCompatibility::RuntimeCompatible
        );
    }
    #[test]
    fn disjoint_valid_manifest_requires_desktop_update() {
        assert_eq!(
            project_release(&release("v0.5.0"), Some(&manifest(2, 2)))
                .unwrap()
                .compatibility,
            UpdateCompatibility::DesktopRequired
        );
    }
    #[test]
    fn missing_malformed_unknown_or_wrong_release_manifest_is_generic() {
        for bytes in [
            None,
            Some(b"not json".to_vec()),
            Some(manifest(0, 1)),
            Some(manifest(3, 1)),
            Some(
                manifest(1, 1)
                    .into_iter()
                    .map(|b| if b == b'5' { b'6' } else { b })
                    .collect(),
            ),
        ] {
            assert_eq!(
                project_release(&release("v0.5.0"), bytes.as_deref())
                    .unwrap()
                    .compatibility,
                UpdateCompatibility::Unknown
            );
        }
        let mut m: serde_json::Value = serde_json::from_slice(&manifest(1, 1)).unwrap();
        m["schema_version"] = json!(99);
        assert_eq!(
            project_release(&release("v0.5.0"), Some(&serde_json::to_vec(&m).unwrap()))
                .unwrap()
                .compatibility,
            UpdateCompatibility::Unknown
        );
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
}
