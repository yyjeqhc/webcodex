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

fn release_catalog(bytes: &[u8], version: &str) -> UpdateResult<Option<u16>> {
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
    Ok(if seen.contains("manifest-v2.json") {
        Some(2)
    } else if seen.contains("manifest.json") {
        Some(1)
    } else {
        None
    })
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
    let name = if manifest.catalog_version == 2 {
        "manifest-v2.json"
    } else {
        "manifest.json"
    };
    if sums.len() > 32 || sums.get(name).copied() != Some(sha256(raw).as_str()) {
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
    for target in InstallerTarget::ALL.into_iter().chain(
        InstallerTarget::RUNTIME
            .into_iter()
            .filter(|_| manifest.catalog_version == 2),
    ) {
        let entry = manifest.entry(target)?;
        let source = target.source_filename(&manifest.version);
        if sums.get(source.as_str()).copied() != Some(entry.source_manifest_sha256.as_str())
            || sums.get(entry.filename.as_str()).copied() != Some(entry.sha256.as_str())
        {
            return Err(UpdateError::ChecksumMismatch);
        }
    }
    Ok(())
}

fn verify_compatibility_view(
    manifest: &UnifiedInstallerManifest,
    legacy: &[u8],
    sums: &[u8],
) -> UpdateResult<()> {
    let view = UnifiedInstallerManifest::parse(legacy, &manifest.version)?;
    if serde_json::to_value(&view).map_err(|_| UpdateError::ManifestInvalid)?
        != serde_json::to_value(manifest.full_compatibility())
            .map_err(|_| UpdateError::ManifestInvalid)?
    {
        return Err(UpdateError::ManifestInvalid);
    }
    verify_checksums(sums, &view, legacy)
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
    let Some(catalog_version) = release_catalog(&release, version)? else {
        return Ok(None);
    };
    if catalog_version == 1 && !target.flavor.is_full() {
        return Ok(None);
    }
    // Declared v2 is publisher intent: its absence, invalid bytes or checksum
    // failure never retries the legacy asset.
    let catalog_name = if catalog_version == 2 {
        "manifest-v2.json"
    } else {
        "manifest.json"
    };
    let manifest_bytes = metadata(
        &client,
        &release_asset_url(version, catalog_name)?,
        MAX_MANIFEST_BYTES,
    )
    .await?
    .ok_or(UpdateError::ManifestInvalid)?;
    let manifest = if catalog_version == 2 {
        UnifiedInstallerManifest::parse_v2(&manifest_bytes, version)?
    } else {
        UnifiedInstallerManifest::parse(&manifest_bytes, version)?
    };
    let sums = metadata(
        &client,
        &release_asset_url(version, "SHA256SUMS")?,
        16 * 1024,
    )
    .await?
    .ok_or(UpdateError::ManifestInvalid)?;
    verify_checksums(&sums, &manifest, &manifest_bytes)?;
    if catalog_version == 2 {
        let legacy = metadata(
            &client,
            &release_asset_url(version, "manifest.json")?,
            MAX_MANIFEST_BYTES,
        )
        .await?
        .ok_or(UpdateError::ManifestInvalid)?;
        verify_compatibility_view(&manifest, &legacy, &sums)?;
    }
    let entry = manifest.entry(target)?;
    let source_bytes = metadata(&client, &entry.source_manifest_url, MAX_SOURCE_BYTES)
        .await?
        .ok_or(UpdateError::SourceManifestInvalid)?;
    if sha256(&source_bytes) != entry.source_manifest_sha256 {
        return Err(UpdateError::ChecksumMismatch);
    }
    let source =
        verify_source_manifest_for_flavor(&source_bytes, version, platform, target.flavor)?;
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
            release_catalog(&serde_json::to_vec(value).unwrap(), "1.2.3")
        };
        assert_eq!(check(&release), Ok(Some(1)));
        for key in ["draft", "prerelease"] {
            release[key] = true.into();
            assert!(check(&release).is_err());
            release[key] = false.into();
        }
        release["tag_name"] = "1.2.3".into();
        assert!(check(&release).is_err());
        release["tag_name"] = "v1.2.3".into();
        release["assets"] = serde_json::json!([]);
        assert_eq!(check(&release), Ok(None));
        release["assets"] = serde_json::json!([{"name":"manifest.json"},{"name":"manifest.json"}]);
        assert!(check(&release).is_err());
    }
}

#[cfg(test)]
mod catalog_compatibility_tests {
    use super::*;
    #[test]
    fn v2_declaration_never_selects_legacy_and_duplicate_assets_fail() {
        let mut value = serde_json::json!({"tag_name":"v1.2.3","draft":false,"prerelease":false,"assets":[{"name":"manifest-v2.json"},{"name":"manifest.json"}]});
        let classify =
            |v: &serde_json::Value| release_catalog(&serde_json::to_vec(v).unwrap(), "1.2.3");
        assert_eq!(classify(&value), Ok(Some(2)));
        value["assets"] = serde_json::json!([{"name":"manifest-v2.json"}]);
        assert_eq!(classify(&value), Ok(Some(2)));
        value["assets"] =
            serde_json::json!([{"name":"manifest-v2.json"},{"name":"manifest-v2.json"}]);
        assert!(classify(&value).is_err());
    }
    #[test]
    fn full_compatibility_accepts_exact_32_sums_and_rejects_split_authority() {
        let raw = serde_json::to_vec(&super::super::tests::catalog_v2()).unwrap();
        let manifest = UnifiedInstallerManifest::parse_v2(&raw, "1.2.3").unwrap();
        let legacy = serde_json::to_vec(&manifest.full_compatibility()).unwrap();
        let mut sums = BTreeMap::<String, String>::new();
        sums.insert("manifest-v2.json".into(), sha256(&raw));
        sums.insert("manifest.json".into(), sha256(&legacy));
        for platform in RuntimePlatform::ALL {
            sums.insert(
                format!("webcodex-v1.2.3-{}.tar.gz", platform.as_str()),
                manifest.artifacts[&platform].sha256.clone(),
            );
        }
        for target in InstallerTarget::ALL
            .into_iter()
            .chain(InstallerTarget::RUNTIME)
        {
            let entry = manifest.entry(target).unwrap();
            sums.insert(entry.filename.clone(), entry.sha256.clone());
            sums.insert(
                target.source_filename("1.2.3"),
                entry.source_manifest_sha256.clone(),
            );
        }
        for name in [
            "webcodex-desktop-v1.2.3-linux-x64.AppImage",
            "webcodex-desktop-v1.2.3-darwin-arm64.dmg",
            "webcodex-desktop-v1.2.3-win32-x64.exe",
            "webcodex-desktop-runtime.json",
        ] {
            sums.insert(name.into(), "f".repeat(64));
        }
        assert_eq!(sums.len(), 32);
        let text: String = sums
            .iter()
            .map(|(name, hash)| format!("{hash}  {name}\n"))
            .collect();
        assert_eq!(verify_checksums(text.as_bytes(), &manifest, &raw), Ok(()));
        assert_eq!(
            verify_compatibility_view(&manifest, &legacy, text.as_bytes()),
            Ok(())
        );
        // Frozen old updater budget/Full shape still accepts the compatibility
        // view despite the new Runtime/v2 lines, without changing its 32 cap.
        assert_eq!(
            verify_checksums(
                text.as_bytes(),
                &UnifiedInstallerManifest::parse(&legacy, "1.2.3").unwrap(),
                &legacy
            ),
            Ok(())
        );
        let oversized = format!("{text}{}  extra.bin\n", "f".repeat(64));
        assert!(verify_checksums(oversized.as_bytes(), &manifest, &raw).is_err());
        let mut changed: serde_json::Value = serde_json::from_slice(&legacy).unwrap();
        changed["artifacts"]["linux-x64"]["sha256"] = "e".repeat(64).into();
        assert!(verify_compatibility_view(
            &manifest,
            &serde_json::to_vec(&changed).unwrap(),
            text.as_bytes()
        )
        .is_err());
    }
}
