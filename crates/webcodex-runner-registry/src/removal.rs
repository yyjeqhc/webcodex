//! Server-side removal of one stale Runner registration.
//!
//! Removal is a Server control-plane action: the Runner machine is never
//! contacted, and its local credentials, configuration and files are untouched.
//! A registration is removable only while it is provably idle — no live
//! transport, past the online window, no active Job, no queued request, and no
//! maintenance lease covering it. Terminal Job history is deliberately
//! retained: its authorization and recovery snapshot lives on the Job, so
//! removing a registration neither erases nor re-owns execution evidence.

use crate::access_control::runner_visible_to_access;
use crate::jobs::refresh_job_status_locked;
use crate::registry::{now_ts, RunnerRegistry, RUNNER_ONLINE_WINDOW_SECS};
use crate::state::RunnerRegistryInner;
use crate::RunnerAccess;
use std::fmt;

/// Why one Runner registration was not removed. Every variant leaves the
/// registry byte-for-byte untouched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerRemovalRefusal {
    /// No Runner visible to this caller has that `client_id`; the registration
    /// is already absent. Callers that treat absence as the target state
    /// convert this into success rather than an error.
    UnknownRunner,
    /// A live long-lived transport still owns the registration.
    Connected,
    /// The online window has not elapsed, so the Runner may still be polling.
    StillOnline { last_seen: i64 },
    /// Active Jobs still reference the registration.
    ActiveJobs { active: usize },
    /// Queued requests still reference the registration.
    PendingRequests { pending: usize },
    /// An upgrade/maintenance lease covers this Runner.
    Maintenance,
}

impl fmt::Display for RunnerRemovalRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownRunner => write!(formatter, "unknown shell client"),
            Self::Connected => write!(formatter, "runner transport is connected"),
            Self::StillOnline { last_seen } => {
                write!(formatter, "runner was seen within the online window ({last_seen})")
            }
            Self::ActiveJobs { active } => {
                write!(formatter, "runner still owns {active} active job(s)")
            }
            Self::PendingRequests { pending } => {
                write!(formatter, "runner still owns {pending} queued request(s)")
            }
            Self::Maintenance => write!(formatter, "maintenance lease covers this runner"),
        }
    }
}

impl std::error::Error for RunnerRemovalRefusal {}

/// Facts observed while removing one Runner registration. They are audit and
/// log evidence only; no authority is derived from them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerRemovalOutcome {
    pub client_id: String,
    pub runner_instance_id: String,
    /// Projects removed together with the registration, in any lifecycle state.
    pub projects_removed: usize,
}

/// Count active Jobs whose canonical record belongs to one Runner. Records are
/// refreshed through the shared recovery transition before the count is read,
/// so an expired recovery window cannot keep a registration undeletable
/// forever.
pub(super) fn active_jobs_for_runner_locked(
    inner: &mut RunnerRegistryInner,
    client_id: &str,
) -> usize {
    let mut active = 0;
    for job_id in inner.jobs_by_id.active_ids() {
        let belongs = inner
            .jobs_by_id
            .get(&job_id)
            .is_some_and(|job| job.client_id == client_id);
        if !belongs {
            continue;
        }
        refresh_job_status_locked(inner, &job_id);
        if inner
            .jobs_by_id
            .get(&job_id)
            .is_some_and(|job| job.lifecycle.is_active())
        {
            active += 1;
        }
    }
    active
}

/// Count every queued request (shell, persistent shell, MCP/plugin/coding-agent
/// gateway) that still references one Runner. Waiters only exist alongside a
/// `pending_by_id` record, so this covers all request kinds.
pub(super) fn pending_requests_for_runner_locked(
    inner: &RunnerRegistryInner,
    client_id: &str,
) -> usize {
    inner
        .pending_by_id
        .values()
        .filter(|pending| pending.request.client_id == client_id)
        .count()
}

/// Remove every Server-side projection of one Runner registration. Callers must
/// already hold the registry lock and have established that the record is
/// removable (fresh admission) or expired (shared-key TTL prune). Job records
/// and their receipts are intentionally retained.
pub(super) fn remove_runner_registration_locked(
    inner: &mut RunnerRegistryInner,
    client_id: &str,
) -> RunnerRemovalOutcome {
    let (runner_instance_id, projects_removed) = inner
        .runners
        .get(client_id)
        .map(|runner| (runner.runner_instance_id.clone(), runner.projects.len()))
        .unwrap_or_default();
    inner.runners.remove(client_id);
    inner.queues_by_runner.remove(client_id);
    inner.notifiers.remove(client_id);
    inner.retired_instances.remove(client_id);
    let project_prefix = format!("agent:{client_id}:");
    inner
        .unregistering_projects
        .retain(|project_id, _| !project_id.starts_with(&project_prefix));
    RunnerRemovalOutcome {
        client_id: client_id.to_string(),
        runner_instance_id,
        projects_removed,
    }
}

impl RunnerRegistry {
    /// Remove one stale Runner registration for an authorized caller.
    ///
    /// Fails closed: a live transport, a recent `last_seen`, an active Job, a
    /// queued request, or a maintenance lease refuses the removal without
    /// mutating anything. Callers must already be authorized for admin
    /// visibility; `None` preserves the trusted in-process behavior used by
    /// other registry operations.
    pub async fn remove_stale_runner_for_auth(
        &self,
        auth: Option<&RunnerAccess>,
        client_id: &str,
    ) -> Result<RunnerRemovalOutcome, RunnerRemovalRefusal> {
        self.prune_expired_shared_key_runners().await;
        let mut inner = self.inner.lock().await;
        let now = now_ts();
        let Some(runner) = inner.runners.get(client_id) else {
            return Err(RunnerRemovalRefusal::UnknownRunner);
        };
        // Concealment: an inaccessible Runner is indistinguishable from an
        // unknown one, matching the execution-admission boundary.
        if !runner_visible_to_access(auth, runner) {
            return Err(RunnerRemovalRefusal::UnknownRunner);
        }
        if inner.notifiers.contains_key(client_id) {
            return Err(RunnerRemovalRefusal::Connected);
        }
        if now.saturating_sub(runner.last_seen) <= RUNNER_ONLINE_WINDOW_SECS {
            return Err(RunnerRemovalRefusal::StillOnline {
                last_seen: runner.last_seen,
            });
        }
        if inner
            .maintenance
            .as_ref()
            .is_some_and(|lease| lease.scope.covers(client_id))
        {
            return Err(RunnerRemovalRefusal::Maintenance);
        }
        let active = active_jobs_for_runner_locked(&mut inner, client_id);
        if active > 0 {
            return Err(RunnerRemovalRefusal::ActiveJobs { active });
        }
        let pending = pending_requests_for_runner_locked(&inner, client_id);
        if pending > 0 {
            return Err(RunnerRemovalRefusal::PendingRequests { pending });
        }
        Ok(remove_runner_registration_locked(&mut inner, client_id))
    }
}
