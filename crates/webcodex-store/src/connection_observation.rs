use rusqlite::Connection;
use std::ops::{Deref, DerefMut};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub(crate) const STORE_CONNECTION_ACQUISITIONS_TOTAL: &str = "store_connection_acquisitions_total";
pub(crate) const STORE_CONNECTION_LOCK_WAIT_SECONDS: &str = "store_connection_lock_wait_seconds";
pub(crate) const STORE_CONNECTION_HOLD_SECONDS: &str = "store_connection_hold_seconds";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreDomain {
    Accounts,
    Activity,
    AdminProjectLifecycle,
    AgentTask,
    AgentWait,
    AgentWake,
    Audit,
    Communication,
    Core,
    Goal,
    JobReceipts,
    JobTerminalWait,
    Memory,
    OAuth,
    ProjectReference,
    Schema,
    WindowActivity,
}

impl StoreDomain {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 17] = [
        Self::Accounts,
        Self::Activity,
        Self::AdminProjectLifecycle,
        Self::AgentTask,
        Self::AgentWait,
        Self::AgentWake,
        Self::Audit,
        Self::Communication,
        Self::Core,
        Self::Goal,
        Self::JobReceipts,
        Self::JobTerminalWait,
        Self::Memory,
        Self::OAuth,
        Self::ProjectReference,
        Self::Schema,
        Self::WindowActivity,
    ];

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Accounts => "accounts",
            Self::Activity => "activity",
            Self::AdminProjectLifecycle => "admin_project_lifecycle",
            Self::AgentTask => "agent_task",
            Self::AgentWait => "agent_wait",
            Self::AgentWake => "agent_wake",
            Self::Audit => "audit",
            Self::Communication => "communication",
            Self::Core => "core",
            Self::Goal => "goal",
            Self::JobReceipts => "job_receipts",
            Self::JobTerminalWait => "job_terminal_wait",
            Self::Memory => "memory",
            Self::OAuth => "oauth",
            Self::ProjectReference => "project_reference",
            Self::Schema => "schema",
            Self::WindowActivity => "window_activity",
        }
    }
}

pub(crate) trait StoreConnectionObserver: Send + Sync {
    fn record_acquisition(&self, domain: StoreDomain, wait: Duration);
    fn record_hold(&self, domain: StoreDomain, hold: Duration);
}

pub(crate) struct TracingStoreConnectionObserver;

impl StoreConnectionObserver for TracingStoreConnectionObserver {
    fn record_acquisition(&self, domain: StoreDomain, wait: Duration) {
        tracing::trace!(
            target: "webcodex_store::connection",
            metric = STORE_CONNECTION_ACQUISITIONS_TOTAL,
            domain = domain.as_str(),
            value = 1.0_f64,
            unit = "count",
            "store connection observation"
        );
        tracing::trace!(
            target: "webcodex_store::connection",
            metric = STORE_CONNECTION_LOCK_WAIT_SECONDS,
            domain = domain.as_str(),
            value = wait.as_secs_f64(),
            unit = "seconds",
            "store connection observation"
        );
    }

    fn record_hold(&self, domain: StoreDomain, hold: Duration) {
        tracing::trace!(
            target: "webcodex_store::connection",
            metric = STORE_CONNECTION_HOLD_SECONDS,
            domain = domain.as_str(),
            value = hold.as_secs_f64(),
            unit = "seconds",
            "store connection observation"
        );
    }
}

pub(crate) struct StoreConnectionGuard<'a> {
    guard: Option<MutexGuard<'a, Connection>>,
    observer: &'a dyn StoreConnectionObserver,
    domain: StoreDomain,
    wait: Duration,
    acquired_at: Instant,
}

pub(crate) fn lock_connection<'a>(
    connection: &'a Mutex<Connection>,
    observer: &'a dyn StoreConnectionObserver,
    domain: StoreDomain,
) -> StoreConnectionGuard<'a> {
    let wait_started_at = Instant::now();
    // Keep the historical poison behavior: a poisoned connection mutex still
    // panics here rather than being translated into a store/business error.
    let guard = connection.lock().unwrap();
    let acquired_at = Instant::now();
    let wait = acquired_at.duration_since(wait_started_at);
    StoreConnectionGuard {
        guard: Some(guard),
        observer,
        domain,
        wait,
        acquired_at,
    }
}

impl Deref for StoreConnectionGuard<'_> {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        self.guard
            .as_deref()
            .expect("store connection guard exists until drop")
    }
}

impl DerefMut for StoreConnectionGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard
            .as_deref_mut()
            .expect("store connection guard exists until drop")
    }
}

impl Drop for StoreConnectionGuard<'_> {
    fn drop(&mut self) {
        let hold = self.acquired_at.elapsed();
        // Release the SQLite connection mutex before telemetry. Observation
        // therefore cannot inflate the measured critical section or serialize
        // another store caller behind a slow subscriber.
        drop(self.guard.take());
        observe_fail_open(|| self.observer.record_acquisition(self.domain, self.wait));
        observe_fail_open(|| self.observer.record_hold(self.domain, hold));
    }
}

fn observe_fail_open(observe: impl FnOnce()) {
    let _ = catch_unwind(AssertUnwindSafe(observe));
}
