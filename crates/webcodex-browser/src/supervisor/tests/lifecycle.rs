use super::*;
use std::sync::mpsc;

const TEST_DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn unavailable_worker_rejects_runtime_creation_before_backend_dispatch() {
    let factory = Arc::new(FakeFactory::default());
    let supervisor = BrowserSupervisor::with_factory(factory.clone());
    supervisor.reaper.stop();
    supervisor.reaper.join();
    assert_eq!(
        supervisor.launch().unwrap_err().kind,
        "browser_reaper_unavailable"
    );
    assert_eq!(
        supervisor
            .attach_external("attachment_fixture")
            .unwrap_err()
            .kind,
        "browser_reaper_unavailable"
    );
    assert_eq!(factory.launches.load(Ordering::SeqCst), 0);
    assert!(supervisor.state().browsers.is_empty());
}

pub(super) struct ShutdownProbe {
    pub(super) ownership: crate::BrowserOwnership,
    entered: mpsc::Sender<Duration>,
    release: Option<mpsc::Receiver<()>>,
}

impl ShutdownProbe {
    pub(super) fn shutdown(&mut self, timeout: Duration) {
        let _ = self.entered.send(timeout);
        if let Some(release) = self.release.take() {
            release.recv_timeout(TEST_DEADLINE).unwrap();
        }
    }
}

fn probe(
    supervisor: &BrowserSupervisor,
    browser: &str,
    release: Option<mpsc::Receiver<()>>,
) -> mpsc::Receiver<Duration> {
    let (entered, receiver) = mpsc::channel();
    let mut backend = FakeBackend::new();
    backend.shutdown_probe = Some(ShutdownProbe {
        ownership: crate::BrowserOwnership::OwnedEphemeral,
        entered,
        release,
    });
    supervisor
        .state()
        .browsers
        .get_mut(browser)
        .unwrap()
        .backend = Box::new(backend);
    receiver
}

fn check(supervisor: &BrowserSupervisor) {
    supervisor
        .reaper
        .check()
        .recv_timeout(TEST_DEADLINE)
        .unwrap();
}

#[test]
fn periodic_timer_reaps_without_browser_requests_or_manual_ticks() {
    let mut supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let shutdown = probe(&supervisor, &browser, None);
    supervisor
        .state()
        .browsers
        .get_mut(&browser)
        .unwrap()
        .last_activity_at = Instant::now() - BROWSER_IDLE_TIMEOUT;
    supervisor.reaper.stop();
    supervisor.reaper.join();
    // A zero interval deterministically takes recv_timeout's Timeout branch.
    // Wait for backend readiness, not elapsed time or another Browser request.
    supervisor.reaper = Arc::new(reaper::Reaper::immediate(
        &supervisor.inner,
        &supervisor.shutting_down,
    ));
    assert_eq!(
        shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        SHUTDOWN_TIMEOUT
    );
    supervisor.begin_shutdown();
    supervisor.reaper.join();
    assert!(supervisor.state().browsers.is_empty());
}

#[test]
fn proactive_reaping_expires_idle_and_lifetime_without_browser_requests() {
    let supervisor = fixture();
    let idle = supervisor.launch().unwrap().browser_id;
    let lifetime = supervisor.launch().unwrap().browser_id;
    let live = supervisor.launch().unwrap().browser_id;
    let idle_shutdown = probe(&supervisor, &idle, None);
    let lifetime_shutdown = probe(&supervisor, &lifetime, None);
    let live_shutdown = probe(&supervisor, &live, None);
    {
        let mut state = supervisor.state();
        state.browsers.get_mut(&idle).unwrap().last_activity_at =
            Instant::now() - BROWSER_IDLE_TIMEOUT;
        state.browsers.get_mut(&lifetime).unwrap().created_at =
            Instant::now() - MAX_BROWSER_LIFETIME;
    }
    // Drive the actual worker's timer branch by notification, without a Browser
    // API call, real Chromium process, scheduler sleep, or shortened expiry policy.
    check(&supervisor);
    assert_eq!(
        idle_shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        SHUTDOWN_TIMEOUT
    );
    assert_eq!(
        lifetime_shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        SHUTDOWN_TIMEOUT
    );
    assert!(live_shutdown.try_recv().is_err());
    assert_eq!(supervisor.state().browsers.len(), 1);
    assert!(supervisor.state().browsers.contains_key(&live));
    assert_eq!(
        supervisor.pages(&idle, 1).unwrap_err().kind,
        "stale_browser"
    );
    assert_eq!(
        supervisor.pages(&lifetime, 1).unwrap_err().kind,
        "stale_browser"
    );
}

#[test]
fn proactive_reaping_skips_busy_state_and_rechecks_current_activity() {
    let supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let shutdown = probe(&supervisor, &browser, None);
    {
        let mut state = supervisor.state();
        state.browsers.get_mut(&browser).unwrap().last_activity_at =
            Instant::now() - BROWSER_IDLE_TIMEOUT;
        // A completion while this guard is held proves the tick did not queue
        // behind the Browser operation. Activity changes before the next tick.
        check(&supervisor);
        assert!(state.browsers.contains_key(&browser));
        state.browsers.get_mut(&browser).unwrap().last_activity_at = Instant::now();
    }
    check(&supervisor);
    assert!(supervisor.state().browsers.contains_key(&browser));
    assert!(shutdown.try_recv().is_err());
}

#[test]
fn proactive_cleanup_releases_state_lock_and_finishes_once_before_shutdown() {
    let supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let (release, wait) = mpsc::channel();
    let shutdown = probe(&supervisor, &browser, Some(wait));
    supervisor
        .state()
        .browsers
        .get_mut(&browser)
        .unwrap()
        .last_activity_at = Instant::now() - BROWSER_IDLE_TIMEOUT;
    let completed = supervisor.reaper.check();
    assert_eq!(
        shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        SHUTDOWN_TIMEOUT
    );
    // The fake backend is still blocked in shutdown: another operation can run.
    let live = supervisor.launch().unwrap().browser_id;
    let live_shutdown = probe(&supervisor, &live, None);
    supervisor.begin_shutdown();
    assert_eq!(
        supervisor.launch().unwrap_err().kind,
        "runner_shutting_down"
    );
    release.send(()).unwrap();
    completed.recv_timeout(TEST_DEADLINE).unwrap();
    let report = supervisor.shutdown_until(Instant::now());
    assert_eq!(report.browsers, 1);
    assert_eq!(report.timed_out, 1);
    assert_eq!(report.failures, 0);
    assert_eq!(
        live_shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        Duration::ZERO
    );
    assert!(shutdown.try_recv().is_err());
    assert!(supervisor.state().browsers.is_empty());
}

#[test]
fn proactive_worker_is_shared_by_clones_and_released_with_the_last_owner() {
    let supervisor = fixture();
    let clone = supervisor.clone();
    assert!(Arc::ptr_eq(&supervisor.reaper, &clone.reaper));
    let state = Arc::downgrade(&supervisor.inner);
    let reaper = Arc::downgrade(&supervisor.reaper);
    drop(supervisor);
    let browser = clone.launch().unwrap().browser_id;
    clone.state().browsers.get_mut(&browser).unwrap().created_at =
        Instant::now() - MAX_BROWSER_LIFETIME;
    check(&clone);
    assert!(clone.state().browsers.is_empty());
    drop(clone);
    assert!(reaper.upgrade().is_none());
    assert!(state.upgrade().is_none());
}

#[test]
fn proactive_reaping_delegates_each_ownership_mode_to_backend_cleanup() {
    for ownership in [
        crate::BrowserOwnership::OwnedEphemeral,
        crate::BrowserOwnership::OwnedManagedPersistent,
        crate::BrowserOwnership::AttachedExternal,
    ] {
        let supervisor = fixture();
        let (entered, shutdown) = mpsc::channel();
        let mut backend = FakeBackend::new();
        backend.shutdown_probe = Some(ShutdownProbe {
            ownership,
            entered,
            release: None,
        });
        let runtime = BrowserRuntime::new(Box::new(backend));
        let browser = opaque_id("browser");
        {
            let mut state = supervisor.state();
            state.browsers.insert(browser.clone(), runtime);
            let runtime = state.browsers.get_mut(&browser).unwrap();
            assert_eq!(runtime.backend.ownership(), ownership);
            runtime.created_at = Instant::now() - MAX_BROWSER_LIFETIME;
        }
        check(&supervisor);
        assert_eq!(
            shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
            SHUTDOWN_TIMEOUT
        );
        assert!(supervisor.state().browsers.is_empty());
    }
}

#[test]
fn shutdown_admission_does_not_wait_for_busy_browser_state() {
    let supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let state = supervisor.state();
    supervisor.begin_shutdown();
    supervisor.reaper.join();
    assert!(state.browsers.contains_key(&browser));
    drop(state);
    assert_eq!(supervisor.shutdown_until(Instant::now()).browsers, 1);
}
