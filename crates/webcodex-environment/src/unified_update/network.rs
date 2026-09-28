use super::*;
use serde::Deserialize;
use std::time::Duration;

pub fn http_client() -> UpdateResult<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("WebCodex-Desktop-Unified-Updater")
        .connect_timeout(Duration::from_secs(5))
        .read_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30 * 60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() < 4 && trusted_download_url(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("untrusted update redirect")
            }
        }))
        .build()
        .map_err(|_| UpdateError::NetworkUnavailable)
}

pub async fn read_response(mut response: reqwest::Response, limit: u64) -> UpdateResult<Vec<u8>> {
    if response.content_length().is_some_and(|n| n > limit) {
        return Err(UpdateError::DownloadTooLarge);
    }
    if !response.status().is_success() {
        return Err(UpdateError::NetworkUnavailable);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpdateError::NetworkUnavailable)?
    {
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > limit {
            return Err(UpdateError::DownloadTooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn metadata(
    client: &reqwest::Client,
    url: &str,
    limit: u64,
) -> UpdateResult<Option<Vec<u8>>> {
    if !url::Url::parse(url).is_ok_and(|url| trusted_download_url(&url)) {
        return Err(UpdateError::ManifestInvalid);
    }
    tokio::time::timeout(Duration::from_secs(30), async {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|_| UpdateError::NetworkUnavailable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        read_response(response, limit).await.map(Some)
    })
    .await
    .map_err(|_| UpdateError::NetworkUnavailable)?
}

#[derive(Deserialize)]
struct PublishedRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<PublishedAsset>,
}
#[derive(Deserialize)]
struct PublishedAsset {
    name: String,
}

fn verify_release(bytes: &[u8], version: &str) -> UpdateResult<bool> {
    let value = strict_json(bytes).map_err(|_| UpdateError::ManifestInvalid)?;
    let release: PublishedRelease =
        serde_json::from_value(value).map_err(|_| UpdateError::ManifestInvalid)?;
    if release.draft
        || release.prerelease
        || !stable_version(version)
        || release.tag_name != format!("v{version}")
        || release.assets.len() > 128
    {
        return Err(UpdateError::ManifestInvalid);
    }
    let mut seen = std::collections::BTreeSet::new();
    for asset in &release.assets {
        if asset.name.len() > 256 || !seen.insert(asset.name.as_str()) {
            return Err(UpdateError::ManifestInvalid);
        }
    }
    Ok(seen.contains("manifest.json"))
}

pub struct ReleaseArtifacts {
    pub manifest: UnifiedInstallerManifest,
    pub manifest_bytes: Vec<u8>,
    pub source: UpdateSource,
    pub source_bytes: Vec<u8>,
}

fn verify_checksums(
    bytes: &[u8],
    manifest: &UnifiedInstallerManifest,
    raw: &[u8],
) -> UpdateResult<()> {
    let text = std::str::from_utf8(bytes).map_err(|_| UpdateError::ManifestInvalid)?;
    let mut sums = BTreeMap::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let (hash, name) = line.split_once("  ").ok_or(UpdateError::ManifestInvalid)?;
        if !valid_sha256(hash)
            || name.len() > 192
            || name.is_empty()
            || name
                .bytes()
                .any(|c| !c.is_ascii_alphanumeric() && !b"._+-".contains(&c))
            || sums.insert(name, hash).is_some()
        {
            return Err(UpdateError::ManifestInvalid);
        }
    }
    if sums.len() > 32 || sums.get("manifest.json").copied() != Some(sha256(raw).as_str()) {
        return Err(UpdateError::ChecksumMismatch);
    }
    for platform in RuntimePlatform::ALL {
        let runtime = manifest
            .artifacts
            .get(&platform)
            .ok_or(UpdateError::ManifestInvalid)?;
        let runtime_name = format!(
            "webcodex-v{}-{}.tar.gz",
            manifest.version,
            platform.as_str()
        );
        if sums.get(runtime_name.as_str()).copied() != Some(runtime.sha256.as_str()) {
            return Err(UpdateError::ChecksumMismatch);
        }
        let source_name = platform.source_filename(&manifest.version);
        let source_digest = InstallerTarget::ALL
            .iter()
            .find(|target| target.platform == platform)
            .and_then(|target| manifest.entry(*target).ok())
            .map(|entry| entry.source_manifest_sha256.as_str())
            .ok_or(UpdateError::ManifestInvalid)?;
        if sums.get(source_name.as_str()).copied() != Some(source_digest) {
            return Err(UpdateError::ChecksumMismatch);
        }
    }
    for target in InstallerTarget::ALL {
        let entry = manifest.entry(target)?;
        if sums.get(entry.filename.as_str()).copied() != Some(entry.sha256.as_str()) {
            return Err(UpdateError::ChecksumMismatch);
        }
    }
    Ok(())
}

/// Always re-establish fixed publisher/tag authority before trusting cached
/// metadata. Missing legacy installer metadata is availability, not a failure.
pub async fn fetch_release(
    version: &str,
    target: InstallerTarget,
) -> UpdateResult<Option<ReleaseArtifacts>> {
    let platform = target.platform;
    if !stable_version(version) {
        return Err(UpdateError::ManifestInvalid);
    }
    let client = http_client()?;
    let release_url =
        format!("https://api.github.com/repos/{OFFICIAL_REPOSITORY}/releases/tags/v{version}");
    let release = metadata(&client, &release_url, MAX_MANIFEST_BYTES)
        .await?
        .ok_or(UpdateError::NetworkUnavailable)?;
    if !verify_release(&release, version)? {
        return Ok(None);
    }
    let manifest_bytes = metadata(
        &client,
        &release_asset_url(version, "manifest.json")?,
        MAX_MANIFEST_BYTES,
    )
    .await?
    .ok_or(UpdateError::ManifestInvalid)?;
    let manifest = UnifiedInstallerManifest::parse(&manifest_bytes, version)?;
    let sums = metadata(
        &client,
        &release_asset_url(version, "SHA256SUMS")?,
        16 * 1024,
    )
    .await?
    .ok_or(UpdateError::ManifestInvalid)?;
    verify_checksums(&sums, &manifest, &manifest_bytes)?;
    let entry = manifest.entry(target)?;
    let source_bytes = metadata(&client, &entry.source_manifest_url, MAX_SOURCE_BYTES)
        .await?
        .ok_or(UpdateError::SourceManifestInvalid)?;
    if sha256(&source_bytes) != entry.source_manifest_sha256 {
        return Err(UpdateError::ChecksumMismatch);
    }
    let source = verify_source_manifest(&source_bytes, version, platform)?;
    Ok(Some(ReleaseArtifacts {
        manifest,
        manifest_bytes,
        source,
        source_bytes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn checksum_fixture() -> (UnifiedInstallerManifest, Vec<u8>, String) {
        let raw = serde_json::to_vec(&super::super::tests::manifest()).unwrap();
        let manifest = UnifiedInstallerManifest::parse(&raw, "1.2.3").unwrap();
        let mut sums = format!("{}  manifest.json\n", sha256(&raw));
        for platform in RuntimePlatform::ALL {
            let runtime = &manifest.artifacts[&platform];
            let source = InstallerTarget::ALL
                .iter()
                .find(|target| target.platform == platform)
                .and_then(|target| manifest.entry(*target).ok())
                .unwrap();
            sums.push_str(&format!(
                "{}  webcodex-v1.2.3-{}.tar.gz\n{}  {}\n",
                runtime.sha256,
                platform.as_str(),
                source.source_manifest_sha256,
                platform.source_filename("1.2.3")
            ));
        }
        for target in InstallerTarget::ALL {
            let entry = manifest.entry(target).unwrap();
            sums.push_str(&format!("{}  {}\n", entry.sha256, entry.filename));
        }
        (manifest, raw, sums)
    }

    #[test]
    fn checksums_bind_six_runtime_six_source_and_eight_installer_sets() {
        let (manifest, raw, sums) = checksum_fixture();
        assert_eq!(verify_checksums(sums.as_bytes(), &manifest, &raw), Ok(()));
        let mut changed = raw.clone();
        changed.push(b' ');
        assert_eq!(
            verify_checksums(sums.as_bytes(), &manifest, &changed),
            Err(UpdateError::ChecksumMismatch)
        );
        for line in sums.lines() {
            let partial = sums
                .lines()
                .filter(|item| *item != line)
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(
                verify_checksums(partial.as_bytes(), &manifest, &raw),
                Err(UpdateError::ChecksumMismatch),
                "missing {line}"
            );
        }
    }

    #[test]
    fn duplicate_malformed_and_changed_checksum_entries_are_rejected() {
        let (manifest, raw, sums) = checksum_fixture();
        let duplicate = format!("{sums}{}\n", sums.lines().next().unwrap());
        assert_eq!(
            verify_checksums(duplicate.as_bytes(), &manifest, &raw),
            Err(UpdateError::ManifestInvalid)
        );
        for changed in [
            sums.replacen(&"b".repeat(64), &"d".repeat(64), 1),
            sums.replacen(&"c".repeat(64), &"d".repeat(64), 1),
        ] {
            assert_eq!(
                verify_checksums(changed.as_bytes(), &manifest, &raw),
                Err(UpdateError::ChecksumMismatch)
            );
        }
        let uppercase = sums.replacen(&"a".repeat(64), &"A".repeat(64), 1);
        assert_eq!(
            verify_checksums(uppercase.as_bytes(), &manifest, &raw),
            Err(UpdateError::ManifestInvalid)
        );
    }

    #[test]
    fn published_tag_must_be_exact_stable_and_not_duplicated() {
        let mut release = serde_json::json!({"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{"name":"manifest.json"}]});
        let check = |value: &serde_json::Value| {
            verify_release(&serde_json::to_vec(value).unwrap(), "1.2.3")
        };
        assert_eq!(check(&release), Ok(true));
        for key in ["draft", "prerelease"] {
            release[key] = true.into();
            assert!(check(&release).is_err());
            release[key] = false.into();
        }
        release["tag_name"] = "1.2.3".into();
        assert!(check(&release).is_err());
        release["tag_name"] = "v1.2.3".into();
        release["assets"] = serde_json::json!([]);
        assert_eq!(check(&release), Ok(false));
        release["assets"] = serde_json::json!([{"name":"manifest.json"},{"name":"manifest.json"}]);
        assert!(check(&release).is_err());
    }
}
