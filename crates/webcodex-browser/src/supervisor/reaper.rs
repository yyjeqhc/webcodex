use super::{BrowserError, BrowserResult, SupervisorState, SHUTDOWN_TIMEOUT};
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
        let worker = thread::Builder::new()
            .name("webcodex-browser-reaper".into())
            .spawn(move || run(state, shutting_down, receiver, interval))
            .ok();
        Self {
            control,
            worker: Mutex::new(worker),
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
        reap(&state, &shutting_down);
        // No strong Supervisor/Browser-state ownership survives the idle wait.
        drop(state);
        if let Some(completed) = completed {
            let _ = completed.send(());
        }
    }
}

fn reap(state: &Mutex<SupervisorState>, shutting_down: &AtomicBool) {
    let mut state = match state.try_lock() {
        Ok(state) => state,
        Err(TryLockError::Poisoned(err)) => err.into_inner(),
        Err(TryLockError::WouldBlock) => return,
    };
    if shutting_down.load(Ordering::Acquire) {
        return;
    }
    let expired = state.take_expired(Instant::now());
    drop(state);
    for mut runtime in expired {
        // Once shutdown is requested, remaining detached runtimes skip graceful
        // CDP work but still terminate/reap owned trees (or detach external leases).
        let timeout = if shutting_down.load(Ordering::Acquire) {
            Duration::ZERO
        } else {
            SHUTDOWN_TIMEOUT
        };
        let _ = runtime.backend.shutdown(timeout);
    }
}
