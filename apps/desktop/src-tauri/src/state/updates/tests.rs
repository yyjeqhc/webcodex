use super::*;
use std::future::{poll_fn, Future};
use std::pin::Pin;
use std::task::Poll;
use webcodex_environment::unified_update::{
    DownloadPhase, InstallerTarget, ReleaseNotice, RuntimePlatform, UpdateCompatibility,
};

fn fixture() -> (tempfile::TempDir, AppState) {
    let root = tempfile::tempdir().unwrap();
    let data = root.path().canonicalize().unwrap();
    let app = AppState::new(data.clone(), data.join("resources")).unwrap();
    (root, app)
}

fn release(version: &str) -> ReleaseNotice {
    ReleaseNotice {
        version: version.into(),
        runtime_version: DESKTOP_VERSION.into(),
        release_url: format!("https://github.com/yyjeqhc/webcodex/releases/tag/v{version}"),
        compatibility: UpdateCompatibility::RuntimeCompatible,
    }
}

// Poll exactly once to place the operation in the core mutex's FIFO queue.
// No sleeps, network, or assumptions about background task scheduling.
async fn enqueue<T>(mut future: Pin<&mut impl Future<Output = T>>) {
    poll_fn(|cx| {
        assert!(future.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
}

async fn tracked_cancellation(app: &AppState) -> CancellationSignal {
    let cancellation = CancellationSignal::new();
    // No release means the manager only initializes its owned temporary cache.
    // Retain the actual manager cancellation signal without fetching an installer.
    app.updates
        .download_now(
            None,
            false,
            false,
            InstallationKind::SourceBuild,
            None,
            &cancellation,
        )
        .await
        .unwrap();
    cancellation
}

#[tokio::test]
async fn delayed_automatic_admission_observes_completed_disable() {
    let (root, app) = fixture();
    let cancellation = tracked_cancellation(&app).await;
    let mut slot = app.core.lock().await;
    let core = slot.as_mut().unwrap();
    core.config.update_cache.latest = Some(release("99.0.0"));
    let old_check_cache = core.config.update_cache.clone();
    assert!(old_check_cache.automatic_download);

    let mut disable = std::pin::pin!(app.set_automatic_update_download(false));
    enqueue(disable.as_mut()).await;
    let mut submitted = false;
    let (disabled, checked) = {
        let mut delayed =
            std::pin::pin!(app.request_update_download_with(None, |cache, manual, _| {
                assert!(app.core.try_lock().is_err());
                assert!(!cache.automatic_download);
                assert!(!manual);
                assert!(cancellation.is_cancelled());
                submitted = true;
            }));
        enqueue(delayed.as_mut()).await;
        drop(slot);
        tokio::join!(disable, delayed)
    };
    assert!(!disabled.unwrap().automatic_download);
    assert!(!checked.unwrap().automatic_download);
    assert!(submitted, "disabled checks must retain local maintenance");
    assert!(cancellation.is_cancelled());
    let persisted: StoredDesktopConfig =
        serde_json::from_slice(&std::fs::read(root.path().join("desktop-state.json")).unwrap())
            .unwrap();
    assert!(!persisted.update_cache.automatic_download);
}

#[tokio::test]
async fn cached_check_returns_current_preference_after_a_concurrent_disable() {
    let (root, app) = fixture();
    let mut slot = app.core.lock().await;
    slot.as_mut().unwrap().config.update_cache.last_check_at_ms = Some(runtime_selection::now_ms());

    let mut check = std::pin::pin!(app.check_for_updates(false));
    enqueue(check.as_mut()).await;
    let mut disable = std::pin::pin!(app.set_automatic_update_download(false));
    enqueue(disable.as_mut()).await;
    drop(slot);

    // The check acquires its initial enabled snapshot first, then yields to
    // the queued disable before it can admit automatic work.
    enqueue(check.as_mut()).await;
    assert!(!disable.await.unwrap().automatic_download);
    let result = check.await.unwrap();
    assert!(result.cached);
    assert!(!result.automatic_download);
    assert!(result.manual_error.is_none());
    tokio::task::yield_now().await;
    assert_eq!(app.get_update_download_state().phase, DownloadPhase::Idle);
    let persisted: StoredDesktopConfig =
        serde_json::from_slice(&std::fs::read(root.path().join("desktop-state.json")).unwrap())
            .unwrap();
    assert!(!persisted.update_cache.automatic_download);
}

#[tokio::test]
async fn disabled_admission_cannot_download_with_a_managed_installation() {
    let (_root, app) = fixture();
    let target = InstallerTarget::ALL
        .into_iter()
        .find(|target| Some(target.platform) == RuntimePlatform::current())
        .expect("the native Desktop platform has a supported installer target");
    let status = app
        .updates
        .download_now(
            Some(release("99.0.0")),
            false,
            false,
            InstallationKind::Managed,
            Some(target),
            &CancellationSignal::new(),
        )
        .await
        .unwrap();
    // A valid target and a newer release get past version/platform guards.
    // Reaching Available with no error proves the automatic=false gate kept
    // the manager out of the network download path.
    assert_eq!(status.version.as_deref(), Some("99.0.0"));
    assert_eq!(status.target, Some(target));
    assert_eq!(status.phase, DownloadPhase::Available);
    assert!(status.error_kind.is_none());
    assert_eq!(status.downloaded_bytes, 0);
    assert!(!status.legacy_release);
    assert!(!status.pending_install);
}

#[tokio::test]
async fn changed_installation_inputs_reject_admission_after_the_probe() {
    let (_root, app) = fixture();
    let slot = app.core.lock().await;
    let mut request = std::pin::pin!(app.request_update_download_with(None, |_, _, _| {
        panic!("an installation probe must not be reused for a changed environment");
    }));
    enqueue(request.as_mut()).await;
    let mut change = std::pin::pin!(async {
        let mut slot = app.core.lock().await;
        slot.as_mut().unwrap().config.persistent_environment = Some("changed-environment".into());
    });
    enqueue(change.as_mut()).await;
    drop(slot);

    // Capture the old installation, then stop at the final admission lock.
    enqueue(request.as_mut()).await;
    change.await;
    let error = request.await.unwrap_err();
    assert_eq!(error.code, "runtime_selection_changed");
}

#[tokio::test]
async fn later_explicit_enable_admits_current_release_while_holding_core_lock() {
    let (_root, app) = fixture();
    assert!(
        !app.set_automatic_update_download(false)
            .await
            .unwrap()
            .automatic_download
    );
    // The real enable path has no release to download in this fixture.
    assert!(
        app.set_automatic_update_download(true)
            .await
            .unwrap()
            .automatic_download
    );
    tokio::task::yield_now().await;
    app.core
        .lock()
        .await
        .as_mut()
        .unwrap()
        .config
        .update_cache
        .latest = Some(release("99.0.0"));

    let mut submitted = false;
    let cache = app
        .request_update_download_with(None, |cache, manual, _| {
            assert!(app.core.try_lock().is_err());
            assert!(cache.automatic_download);
            assert!(!manual);
            assert_eq!(cache.latest.as_ref().unwrap().version, "99.0.0");
            submitted = true;
        })
        .await
        .unwrap();
    assert!(submitted);
    assert!(cache.automatic_download);
}

#[tokio::test]
async fn manual_download_after_disable_requires_the_current_release() {
    let (_root, app) = fixture();
    app.core
        .lock()
        .await
        .as_mut()
        .unwrap()
        .config
        .update_cache
        .latest = Some(release("99.0.0"));
    app.set_automatic_update_download(false).await.unwrap();

    let mut submitted = false;
    app.request_update_download_with(Some("99.0.0"), |cache, manual, _| {
        assert!(app.core.try_lock().is_err());
        assert!(!cache.automatic_download);
        assert!(manual);
        submitted = true;
    })
    .await
    .unwrap();
    assert!(submitted);

    let error = app.download_update("98.0.0").await.unwrap_err();
    assert_eq!(error.code, "update_action_unavailable");
}

#[tokio::test]
async fn failed_disable_preserves_preference_and_existing_cancellation_signal() {
    let (root, app) = fixture();
    let cancellation = tracked_cancellation(&app).await;
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    app.core.lock().await.as_mut().unwrap().config_path = blocked.join("desktop-state.json");

    let error = app.set_automatic_update_download(false).await.unwrap_err();
    assert_eq!(error.code, "desktop_state_unavailable");
    assert!(!cancellation.is_cancelled());

    let mut submitted = false;
    let cache = app
        .request_update_download_with(None, |cache, manual, _| {
            assert!(cache.automatic_download);
            assert!(!manual);
            submitted = true;
        })
        .await
        .unwrap();
    assert!(cache.automatic_download);
    assert!(submitted);
}
