use super::{UpdateError, UpdateResult};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use webcodex_core::desktop_runtime_contract::{
    ReleaseManifest, DESKTOP_RUNTIME_CONTRACT, RELEASE_MANIFEST_SCHEMA_VERSION,
};
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
pub struct GithubRelease {
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

pub fn project_release(release: &GithubRelease, manifest: Option<&[u8]>) -> Option<ReleaseNotice> {
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

async fn bounded_json(client: &reqwest::Client, url: &str) -> UpdateResult<Vec<u8>> {
    let mut response = client
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|_| UpdateError::NetworkUnavailable)?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > RESPONSE_BYTES as u64)
    {
        return Err(UpdateError::NetworkUnavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpdateError::NetworkUnavailable)?
    {
        if bytes.len().saturating_add(chunk.len()) > RESPONSE_BYTES {
            return Err(UpdateError::NetworkUnavailable);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub async fn fetch_latest() -> UpdateResult<Option<ReleaseNotice>> {
    // One wall-clock budget covers both API and optional manifest, including
    // redirects. Missing manifest is unknown compatibility, not a failure.
    tokio::time::timeout(Duration::from_secs(10), async {
        let client = reqwest::Client::builder()
            .user_agent("WebCodex-Desktop-Update-Check")
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(8))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() < 4 && super::trusted_download_url(attempt.url()) {
                    attempt.follow()
                } else {
                    attempt.error("untrusted update redirect")
                }
            }))
            .build()
            .map_err(|_| UpdateError::NetworkUnavailable)?;
        let bytes = bounded_json(
            &client,
            "https://api.github.com/repos/yyjeqhc/webcodex/releases/latest",
        )
        .await?;
        let release: GithubRelease =
            serde_json::from_slice(&bytes).map_err(|_| UpdateError::ManifestInvalid)?;
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
    .map_err(|_| UpdateError::NetworkUnavailable)?
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
}
