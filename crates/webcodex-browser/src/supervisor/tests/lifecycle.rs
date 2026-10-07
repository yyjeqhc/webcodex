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
    result: BrowserResult<()>,
}

impl ShutdownProbe {
    pub(super) fn shutdown(&mut self, timeout: Duration) -> BrowserResult<()> {
        let _ = self.entered.send(timeout);
        if let Some(release) = self.release.take() {
            release.recv_timeout(TEST_DEADLINE).unwrap();
        }
        self.result.clone()
    }
}

fn probe(
    supervisor: &BrowserSupervisor,
    browser: &str,
    release: Option<mpsc::Receiver<()>>,
) -> mpsc::Receiver<Duration> {
    probe_with_result(supervisor, browser, release, Ok(()))
}

fn probe_with_result(
    supervisor: &BrowserSupervisor,
    browser: &str,
    release: Option<mpsc::Receiver<()>>,
    result: BrowserResult<()>,
) -> mpsc::Receiver<Duration> {
    let (entered, receiver) = mpsc::channel();
    let mut backend = FakeBackend::new();
    backend.shutdown_probe = Some(ShutdownProbe {
        ownership: crate::BrowserOwnership::OwnedEphemeral,
        entered,
        release,
        result,
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
    // Drive the actual worker's expiry check by notification, without a Browser
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
    let report = supervisor.shutdown_until(Instant::now() + TEST_DEADLINE);
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (3, 0, 0)
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
fn proactive_cleanup_releases_state_lock_and_shutdown_honors_expired_deadline() {
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
    let (reported, report) = mpsc::channel();
    let clone = supervisor.clone();
    let waiter = std::thread::spawn(move || {
        reported.send(clone.shutdown_until(Instant::now())).unwrap();
    });
    // Do not release the in-flight backend until shutdown has had to return.
    let report = report.recv_timeout(Duration::from_millis(100));
    let worker_owned = report.is_ok() && supervisor.reaper.ensure_available().is_ok();
    release.send(()).unwrap();
    completed.recv_timeout(TEST_DEADLINE).unwrap();
    waiter.join().unwrap();
    let report = report.expect("expired shutdown deadline must not join blocked cleanup");
    assert!(
        worker_owned,
        "unfinished cleanup must retain its owned worker"
    );
    assert_eq!(report.browsers, 2);
    assert_eq!(report.timed_out, 2);
    assert_eq!(report.failures, 0);
    assert_eq!(
        live_shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        Duration::ZERO
    );
    assert!(shutdown.try_recv().is_err());
    assert!(supervisor.state().browsers.is_empty());
    let report = supervisor.shutdown_until(Instant::now() + TEST_DEADLINE);
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (0, 0, 0)
    );
}

#[test]
fn shutdown_waits_only_until_future_deadline_for_in_flight_reaping() {
    let supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let (release, wait) = mpsc::channel();
    let shutdown = probe(&supervisor, &browser, Some(wait));
    supervisor
        .state()
        .browsers
        .get_mut(&browser)
        .unwrap()
        .created_at = Instant::now() - MAX_BROWSER_LIFETIME;
    let completed = supervisor.reaper.check();
    shutdown.recv_timeout(TEST_DEADLINE).unwrap();
    let (reported, report) = mpsc::channel();
    let clone = supervisor.clone();
    let waiter = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(25);
        let outcome = clone.shutdown_until(deadline);
        reported
            .send((outcome, Instant::now() >= deadline))
            .unwrap();
    });
    let report = report.recv_timeout(Duration::from_millis(100));
    release.send(()).unwrap();
    completed.recv_timeout(TEST_DEADLINE).unwrap();
    waiter.join().unwrap();
    let (report, deadline_reached) = report.expect("shutdown must stop waiting at its deadline");
    assert!(
        deadline_reached,
        "shutdown should allow cleanup its remaining budget"
    );
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (1, 1, 0)
    );
}

#[test]
fn completed_background_cleanup_failure_is_included_in_shutdown_report() {
    let supervisor = fixture();
    let browser = supervisor.launch().unwrap().browser_id;
    let shutdown = probe_with_result(
        &supervisor,
        &browser,
        None,
        Err(BrowserError::uncertain(
            "browser_shutdown_failed",
            "fixture failure",
            "browsers",
        )),
    );
    supervisor
        .state()
        .browsers
        .get_mut(&browser)
        .unwrap()
        .created_at = Instant::now() - MAX_BROWSER_LIFETIME;
    check(&supervisor);
    shutdown.recv_timeout(TEST_DEADLINE).unwrap();
    assert!(supervisor.state().browsers.is_empty());
    let report = supervisor.shutdown_until(Instant::now() + TEST_DEADLINE);
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (1, 0, 1)
    );
    let report = supervisor.shutdown_until(Instant::now());
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (0, 0, 0)
    );
}

#[test]
fn shutdown_reports_pending_batch_once_and_retains_late_cleanup_failure() {
    let supervisor = fixture();
    let (entered, shutdown) = mpsc::channel();
    let mut releases = Vec::new();
    for index in 0..2 {
        let (release, wait) = mpsc::channel();
        releases.push(release);
        let mut backend = FakeBackend::new();
        backend.shutdown_probe = Some(ShutdownProbe {
            ownership: crate::BrowserOwnership::OwnedEphemeral,
            entered: entered.clone(),
            release: Some(wait),
            result: if index == 0 {
                Err(BrowserError::uncertain(
                    "browser_shutdown_failed",
                    "late fixture failure",
                    "browsers",
                ))
            } else {
                Ok(())
            },
        });
        let mut runtime = BrowserRuntime::new(Box::new(backend));
        runtime.created_at = Instant::now() - MAX_BROWSER_LIFETIME;
        supervisor
            .state()
            .browsers
            .insert(opaque_id("browser"), runtime);
    }
    let completed = supervisor.reaper.check();
    assert_eq!(
        shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        SHUTDOWN_TIMEOUT
    );
    // One backend is still blocked; the other runtime belongs to the same batch
    // but has not entered cleanup. Both must be visible at this deadline.
    let report = supervisor.shutdown_until(Instant::now());
    for release in releases {
        release.send(()).unwrap();
    }
    completed.recv_timeout(TEST_DEADLINE).unwrap();
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (2, 2, 0)
    );
    assert_eq!(
        shutdown.recv_timeout(TEST_DEADLINE).unwrap(),
        Duration::ZERO
    );
    let report = supervisor.shutdown_until(Instant::now() + TEST_DEADLINE);
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (0, 0, 1)
    );
    let report = supervisor.shutdown_until(Instant::now());
    assert_eq!(
        (report.browsers, report.timed_out, report.failures),
        (0, 0, 0)
    );
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
            result: Ok(()),
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
