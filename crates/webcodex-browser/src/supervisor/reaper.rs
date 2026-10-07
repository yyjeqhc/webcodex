use super::{
    BrowserError, BrowserResult, BrowserShutdownReport, SupervisorState, SHUTDOWN_TIMEOUT,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex, TryLockError, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const REAP_INTERVAL: Duration = Duration::from_secs(60);

enum Command {
    Stop,
    #[cfg(test)]
    Check(mpsc::Sender<()>),
}

pub(super) struct Reaper {
    control: mpsc::SyncSender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
    progress: Arc<Mutex<CleanupProgress>>,
}

#[derive(Default)]
struct CleanupProgress {
    report: BrowserShutdownReport,
    pending: usize,
    reported_pending: usize,
}

impl Reaper {
    pub(super) fn start(
        state: &Arc<Mutex<SupervisorState>>,
        shutting_down: &Arc<AtomicBool>,
    ) -> Self {
        Self::start_with_interval(state, shutting_down, REAP_INTERVAL)
    }

    fn start_with_interval(
        state: &Arc<Mutex<SupervisorState>>,
        shutting_down: &Arc<AtomicBool>,
        interval: Duration,
    ) -> Self {
        let (control, receiver) = mpsc::sync_channel(1);
        let state = Arc::downgrade(state);
        let shutting_down = Arc::clone(shutting_down);
        let progress = Arc::new(Mutex::new(CleanupProgress::default()));
        let worker_progress = Arc::clone(&progress);
        let worker = thread::Builder::new()
            .name("webcodex-browser-reaper".into())
            .spawn(move || run(state, shutting_down, receiver, interval, worker_progress))
            .ok();
        Self {
            control,
            worker: Mutex::new(worker),
            progress,
        }
    }

    pub(super) fn ensure_available(&self) -> BrowserResult<()> {
        let worker = self.worker.lock().unwrap_or_else(|err| err.into_inner());
        if worker.as_ref().is_some_and(|worker| !worker.is_finished()) {
            Ok(())
        } else {
            Err(BrowserError::not_started(
                "browser_reaper_unavailable",
                "Browser lifecycle worker is unavailable",
            ))
        }
    }

    pub(super) fn stop(&self) {
        // Coalesce repeated shutdown requests; admission never waits for CDP I/O.
        let _ = self.control.try_send(Command::Stop);
    }

    pub(super) fn join(&self) {
        let mut worker = self.worker.lock().unwrap_or_else(|err| err.into_inner());
        if let Some(worker) = worker.take() {
            let _ = worker.join();
        }
    }

    pub(super) fn join_until(&self, deadline: Instant) {
        loop {
            let mut worker = self.worker.lock().unwrap_or_else(|err| err.into_inner());
            if worker.as_ref().is_none_or(|worker| worker.is_finished()) {
                if let Some(worker) = worker.take() {
                    let _ = worker.join();
                }
                return;
            }
            // Retain ownership when the deadline expires. Only final-owner Drop
            // may join unfinished cleanup without a caller-provided deadline.
            drop(worker);
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return;
            }
            thread::sleep(Duration::from_millis(5).min(remaining));
        }
    }

    pub(super) fn take_report(&self) -> BrowserShutdownReport {
        let mut progress = self.progress.lock().unwrap_or_else(|err| err.into_inner());
        let mut report = std::mem::take(&mut progress.report);
        // Pending cleanup has exhausted this caller's wait budget. Report each
        // Browser once, but retain any failure that arrives after this snapshot.
        report.timed_out = report
            .timed_out
            .saturating_add(progress.pending - progress.reported_pending);
        progress.reported_pending = progress.pending;
        report
    }

    #[cfg(test)]
    pub(super) fn check(&self) -> mpsc::Receiver<()> {
        let (completed, receiver) = mpsc::channel();
        self.control.try_send(Command::Check(completed)).unwrap();
        receiver
    }

    #[cfg(test)]
    pub(super) fn immediate(
        state: &Arc<Mutex<SupervisorState>>,
        shutting_down: &Arc<AtomicBool>,
    ) -> Self {
        Self::start_with_interval(state, shutting_down, Duration::ZERO)
    }
}

impl Drop for Reaper {
    fn drop(&mut self) {
        self.stop();
        self.join();
    }
}

fn run(
    state: Weak<Mutex<SupervisorState>>,
    shutting_down: Arc<AtomicBool>,
    receiver: mpsc::Receiver<Command>,
    interval: Duration,
    progress: Arc<Mutex<CleanupProgress>>,
) {
    loop {
        let completed = match receiver.recv_timeout(interval) {
            Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => None::<mpsc::Sender<()>>,
            #[cfg(test)]
            Ok(Command::Check(completed)) => Some(completed),
        };
        if shutting_down.load(Ordering::Acquire) {
            break;
        }
        let Some(state) = state.upgrade() else {
            break;
        };
        reap(&state, &shutting_down, &progress);
        // No strong Supervisor/Browser-state ownership survives the idle wait.
        drop(state);
        if let Some(completed) = completed {
            let _ = completed.send(());
        }
    }
}

fn reap(
    state: &Mutex<SupervisorState>,
    shutting_down: &AtomicBool,
    progress: &Mutex<CleanupProgress>,
) {
    let mut state = match state.try_lock() {
        Ok(state) => state,
        Err(TryLockError::Poisoned(err)) => err.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if shutting_down.load(Ordering::Acquire) {
        return;
    }
    let expired = state.take_expired(Instant::now());
    {
        // Register removed runtimes before releasing Browser state so shutdown
        // cannot observe a gap between active and worker-owned cleanup.
        let mut progress = progress.lock().unwrap_or_else(|err| err.into_inner());
        progress.pending += expired.len();
        progress.report.browsers = progress.report.browsers.saturating_add(expired.len());
    }
    drop(state);
    for mut runtime in expired {
        // Once shutdown is requested, remaining removed runtimes skip graceful
        // CDP work but still terminate/reap owned trees (or detach external leases).
        let timeout = if shutting_down.load(Ordering::Acquire) {
            Duration::ZERO
        } else {
            SHUTDOWN_TIMEOUT
        };
        let failed = runtime.backend.shutdown(timeout).is_err();
        drop(runtime);
        let mut progress = progress.lock().unwrap_or_else(|err| err.into_inner());
        progress.pending -= 1;
        if progress.reported_pending > 0 {
            progress.reported_pending -= 1;
        } else if timeout.is_zero() {
            progress.report.timed_out = progress.report.timed_out.saturating_add(1);
        }
        if failed {
            progress.report.failures = progress.report.failures.saturating_add(1);
        }
    }
}
