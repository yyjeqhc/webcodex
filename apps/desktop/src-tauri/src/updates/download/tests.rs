use super::*;
use crate::updates::{UpdateCache, UpdateCompatibility};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::net::TcpListener;

fn cache() -> (tempfile::TempDir, PrivateUpdateCache) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap().join("updates");
    let cache = PrivateUpdateCache::open(root).unwrap();
    (temp, cache)
}

// Only the low-level bounded streaming primitive receives a loopback response.
// Production admission reconstructs a fixed HTTPS URL and rejects this host.
async fn response(headers: &str, body: &[u8]) -> reqwest::Response {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let bytes = [format!("HTTP/1.1 200 OK\r\n{headers}\r\n").as_bytes(), body].concat();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 8192];
        let _ = socket.read(&mut request).await;
        let _ = socket.write_all(&bytes).await;
        let _ = socket.shutdown().await;
    });
    reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
        .get(format!("http://{address}/fixture"))
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn streamed_download_becomes_verified_only_after_atomic_hash_commit() {
    let (_temp, cache) = cache();
    let body = b"canonical installer fixture";
    let response = response(
        &format!("Content-Length: {}\r\nConnection: close\r\n", body.len()),
        body,
    )
    .await;
    let progress = std::sync::Mutex::new(Vec::new());
    let length = stream_installer(
        response,
        &cache,
        "installer.pkg",
        &unified::sha256(body),
        1024,
        &CancellationSignal::new(),
        |done, total| {
            assert!(!cache.file("installer.pkg").unwrap().exists());
            progress.lock().unwrap().push((done, total));
        },
    )
    .await
    .unwrap();
    assert_eq!(length, body.len() as u64);
    assert_eq!(cache.read("installer.pkg", 1024).unwrap().unwrap(), body);
    assert!(!cache.file("installer.part").unwrap().exists());
    assert_eq!(
        progress.lock().unwrap().last().copied(),
        Some((length, Some(length)))
    );
    assert_eq!(
        verify_file(
            &cache,
            "installer.pkg",
            &unified::sha256(body),
            1024,
            &CancellationSignal::new()
        )
        .await
        .unwrap(),
        length
    );
}

#[tokio::test]
async fn header_and_stream_limits_apply_before_candidate_can_be_executed() {
    for (headers, body) in [
        (
            "Content-Length: 99999\r\nConnection: close\r\n",
            b"large".as_slice(),
        ),
        (
            "Connection: close\r\n",
            b"too many streamed bytes".as_slice(),
        ),
        (
            "Transfer-Encoding: chunked\r\nConnection: close\r\n",
            b"8\r\n12345678\r\n8\r\nabcdefgh\r\n0\r\n\r\n".as_slice(),
        ),
    ] {
        let (_temp, cache) = cache();
        let result = stream_installer(
            response(headers, body).await,
            &cache,
            "installer.pkg",
            &"a".repeat(64),
            12,
            &CancellationSignal::new(),
            |_, _| {},
        )
        .await;
        assert_eq!(result, Err(UpdateError::DownloadTooLarge));
        assert!(!cache.file("installer.pkg").unwrap().exists());
        assert!(!cache.file("installer.part").unwrap().exists());
    }
}

#[tokio::test]
async fn unknown_total_reports_bytes_without_guessing_a_percentage() {
    let (_temp, cache) = cache();
    let body = b"12345678";
    let calls = AtomicUsize::new(0);
    let length = stream_installer(
        response("Connection: close\r\n", body).await,
        &cache,
        "installer.deb",
        &unified::sha256(body),
        64,
        &CancellationSignal::new(),
        |_, total| {
            assert_eq!(total, None);
            calls.fetch_add(1, Ordering::Relaxed);
        },
    )
    .await
    .unwrap();
    assert_eq!(length, 8);
    assert!(calls.load(Ordering::Relaxed) > 0);
}

#[tokio::test]
async fn checksum_mismatch_truncation_and_cancellation_remove_partial_bytes() {
    for (headers, body, expected, cancelled) in [
        (
            "Content-Length: 3\r\nConnection: close\r\n",
            b"pkg".as_slice(),
            "0".repeat(64),
            false,
        ),
        (
            "Content-Length: 10\r\nConnection: close\r\n",
            b"pkg".as_slice(),
            unified::sha256(b"pkg"),
            false,
        ),
        (
            "Content-Length: 3\r\nConnection: close\r\n",
            b"pkg".as_slice(),
            unified::sha256(b"pkg"),
            true,
        ),
    ] {
        let (_temp, cache) = cache();
        let signal = CancellationSignal::new();
        if cancelled {
            signal.cancel();
        }
        let result = stream_installer(
            response(headers, body).await,
            &cache,
            "installer.exe",
            &expected,
            64,
            &signal,
            |_, _| {},
        )
        .await;
        assert!(result.is_err());
        assert!(!cache.file("installer.exe").unwrap().exists());
        assert!(!cache.file("installer.part").unwrap().exists());
    }
}

fn current_target() -> InstallerTarget {
    let platform = RuntimePlatform::current().unwrap();
    InstallerTarget::default_for_non_linux(platform)
        .or_else(|| InstallerTarget::for_platform(platform, unified::PackageFormat::Deb))
        .unwrap()
}

fn target_record() -> UpdateRecord {
    UpdateRecord {
        phase: DownloadPhase::ReadyToInstall,
        version: Some("99.0.0".into()),
        target: Some(current_target()),
        downloaded_bytes: 3,
        total_bytes: Some(3),
        sha256: Some(unified::sha256(b"pkg")),
        source_manifest_sha256: Some("b".repeat(64)),
        source_sha: Some("a".repeat(40)),
        verified_at_ms: Some(100),
        ..UpdateRecord::default()
    }
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

#[test]
fn retry_download_cadence_is_separate_from_discovery_and_cancel_is_sticky() {
    let mut record = target_record();
    record.next_retry_at_ms = Some(1000);
    assert!(!record.may_retry(500, false));
    assert!(record.may_retry(1000, false));
    assert!(record.may_retry(500, true));
    record.cancelled = true;
    assert!(!record.may_retry(2000, false));
    assert!(record.may_retry(2000, true));
    assert!(RETRY_INTERVAL_MS < CHECK_INTERVAL_MS);
}

#[test]
fn restart_does_not_trust_a_ready_bit_and_removes_abandoned_partial() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let manager = UpdateManager::new(root.clone());
    let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    manager.change(|record| *record = target_record());
    manager.persist(&cache).unwrap();
    let target = manager.target_cache(&cache).unwrap();
    target.write("installer.part", b"partial").unwrap();
    target.write("installer.pkg", b"pkg").unwrap();
    let restarted = UpdateManager::new(root);
    restarted.load(&cache).unwrap();
    assert_eq!(restarted.current().phase, DownloadPhase::Available);
    assert!(!restarted.snapshot().can_install);
    assert!(!target.file("installer.part").unwrap().exists());
    assert!(target.file("installer.pkg").unwrap().exists()); // rehashed/reused later, not downloaded again blindly
}

#[test]
fn invalid_persisted_paths_platforms_schema_and_sizes_fail_closed() {
    for mutation in [
        "version",
        "target",
        "schema_version",
        "downloaded_bytes",
        "installer_path",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
        let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
        let mut value = serde_json::to_value(target_record()).unwrap();
        value[mutation] = match mutation {
            "schema_version" => 99.into(),
            "downloaded_bytes" => (MAX_INSTALLER_BYTES + 1).into(),
            "version" => "../../outside".into(),
            "target" => "freebsd-x64".into(),
            _ => "/tmp/malicious.exe".into(),
        };
        let bytes = serde_json::to_vec(&value).unwrap();
        cache.write(STATE_FILE, &bytes).unwrap();
        assert!(manager.load(&cache).is_err(), "{mutation}");
        assert_eq!(cache.read(STATE_FILE, STATE_BYTES).unwrap().unwrap(), bytes);
        assert!(!manager.snapshot().can_install);
    }
}

#[tokio::test]
async fn installed_or_stale_target_is_cleaned_without_network_or_runtime_changes() {
    let temp = tempfile::tempdir().unwrap();
    let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
    let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    let old = cache.child("0.0.1").unwrap();
    old.write("fixture.pkg", b"old installer").unwrap();
    cache.write("unrelated-note", b"keep").unwrap();
    manager
        .run(
            None,
            true,
            false,
            InstallationKind::SourceBuild,
            None,
            &CancellationSignal::new(),
        )
        .await
        .unwrap();
    assert!(!cache.file("0.0.1").unwrap().exists());
    assert!(cache.file("unrelated-note").unwrap().exists());
    assert_eq!(manager.snapshot().phase, DownloadPhase::Idle);
}

#[tokio::test]
async fn source_build_automatic_check_never_downloads_or_installs() {
    let temp = tempfile::tempdir().unwrap();
    let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
    let notice = ReleaseNotice {
        version: "99.0.0".into(),
        runtime_version: "99.0.0".into(),
        release_url: "https://github.com/yyjeqhc/webcodex/releases/tag/v99.0.0".into(),
        compatibility: UpdateCompatibility::Unknown,
    };
    manager
        .run(
            Some(notice),
            true,
            false,
            InstallationKind::SourceBuild,
            None,
            &CancellationSignal::new(),
        )
        .await
        .unwrap();
    assert_eq!(manager.current().phase, DownloadPhase::Available);
    assert!(manager.current().last_attempt_at_ms.is_none());
    assert!(manager.current().pending.is_none());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn same_version_package_family_change_invalidates_old_target_cache() {
    let temp = tempfile::tempdir().unwrap();
    let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
    let cache = PrivateUpdateCache::open(manager.root.clone()).unwrap();
    let platform = RuntimePlatform::current().unwrap();
    let deb = InstallerTarget::for_platform(platform, unified::PackageFormat::Deb).unwrap();
    let rpm = InstallerTarget::for_platform(platform, unified::PackageFormat::Rpm).unwrap();
    manager.change(|record| {
        *record = target_record();
        record.target = Some(deb);
    });
    let old = cache.child("99.0.0").unwrap().child(&deb.as_str()).unwrap();
    old.write("installer.deb", b"stale").unwrap();
    manager
        .run_locked(
            &cache,
            Some(ReleaseNotice {
                version: "99.0.0".into(),
                runtime_version: "99.0.0".into(),
                release_url: "https://github.com/yyjeqhc/webcodex/releases/tag/v99.0.0".into(),
                compatibility: UpdateCompatibility::Unknown,
            }),
            false,
            false,
            InstallationKind::SourceBuild,
            Some(rpm),
            &CancellationSignal::new(),
        )
        .await
        .unwrap();
    let state = manager.current();
    assert_eq!(state.target, Some(rpm));
    assert_eq!(state.phase, DownloadPhase::Available);
    assert!(!cache
        .child("99.0.0")
        .unwrap()
        .file(&deb.as_str())
        .unwrap()
        .exists());
}

#[test]
fn pending_transaction_never_advertises_install_authority_even_with_a_ready_phase() {
    let temp = tempfile::tempdir().unwrap();
    let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
    manager.change(|record| {
        *record = target_record();
        record.pending = Some(PendingInstall {
            environment_id: "environment".into(),
            operation_id: None,
            started_at_ms: 100,
        });
    });
    manager.set_installation(InstallationKind::Managed);
    assert!(manager.snapshot().pending_install);
    assert!(!manager.snapshot().can_install);
}

#[test]
fn frontend_projection_contains_no_local_path_or_execution_authority() {
    let temp = tempfile::tempdir().unwrap();
    let manager = UpdateManager::new(temp.path().canonicalize().unwrap());
    manager.change(|record| *record = target_record());
    assert!(!manager.snapshot().can_install);
    manager.set_installation(InstallationKind::Managed);
    assert!(manager.snapshot().can_install);
    let value = serde_json::to_value(manager.snapshot()).unwrap();
    for key in [
        "installer_path",
        "root",
        "candidate_dir",
        "sha256",
        "environment_dir",
        "operation_id",
    ] {
        assert!(value.get(key).is_none(), "{key}");
    }
}
