//! Short-lived display observations. Service control, ownership checks and
//! Diagnostics never consume this cache; they keep their own fresh evidence.

use super::*;
use std::sync::Mutex as StdMutex;
use std::time::Instant;

const DISPLAY_OBSERVATION_TTL: Duration = Duration::from_secs(3);
const DISPLAY_HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(750);

#[derive(Clone, Debug, PartialEq, Eq)]
struct ObservationKey {
    state_generation: u64,
    operation_generation: u64,
    environment_id: Option<String>,
}

impl ObservationKey {
    fn new(snapshot: &DesktopStateSnapshot, operation_generation: u64) -> Self {
        Self {
            state_generation: snapshot.state_generation,
            operation_generation,
            environment_id: snapshot.persistent_environment.clone(),
        }
    }
}

/// HTTP liveness only: deliberately excludes ServiceStatus, ownership and PIDs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RuntimeHeartbeat {
    server_reachable: bool,
    authenticated: bool,
    runner_online: Option<bool>,
    projects_visible: Vec<String>,
    diagnostics: Vec<webcodex_environment::SetupDiagnostic>,
}

impl From<&webcodex_environment::RuntimeObservation> for RuntimeHeartbeat {
    fn from(observation: &webcodex_environment::RuntimeObservation) -> Self {
        Self {
            server_reachable: observation.server_reachable,
            authenticated: observation.authenticated,
            runner_online: observation.runner_online,
            projects_visible: observation.projects_visible.clone(),
            diagnostics: observation.diagnostics.clone(),
        }
    }
}

pub(super) struct RefreshOutcome {
    snapshot: DesktopStateSnapshot,
    heartbeat: Option<RuntimeHeartbeat>,
    applied: bool,
}

impl RefreshOutcome {
    pub(super) fn applied(
        snapshot: DesktopStateSnapshot,
        heartbeat: Option<RuntimeHeartbeat>,
    ) -> Self {
        Self {
            snapshot,
            heartbeat,
            applied: true,
        }
    }

    pub(super) fn superseded(snapshot: DesktopStateSnapshot) -> Self {
        Self {
            snapshot,
            heartbeat: None,
            applied: false,
        }
    }
}

#[derive(Clone, Copy)]
struct ObservationTicket {
    epoch: u64,
    completion: u64,
}

struct CompletedObservation {
    key: ObservationKey,
    result: DesktopResult<()>,
    share_until: Instant,
}

struct CachedObservation {
    key: ObservationKey,
    observed_at: Instant,
    heartbeat: RuntimeHeartbeat,
}

#[derive(Default)]
struct ObservationState {
    epoch: u64,
    completion: u64,
    completed: Option<CompletedObservation>,
    cached: Option<CachedObservation>,
}

#[derive(Default)]
pub(super) struct RuntimeObservationCache {
    state: StdMutex<ObservationState>,
}

impl RuntimeObservationCache {
    pub(super) fn invalidate(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.epoch = state.epoch.wrapping_add(1);
        state.completed = None;
        state.cached = None;
    }

    fn ticket(&self) -> ObservationTicket {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ObservationTicket {
            epoch: state.epoch,
            completion: state.completion,
        }
    }

    fn completed_since(
        &self,
        ticket: ObservationTicket,
        key: &ObservationKey,
        now: Instant,
    ) -> Option<DesktopResult<()>> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let completed = state.completed.as_ref()?;
        (state.completion != ticket.completion
            && completed.key == *key
            && now < completed.share_until)
            .then(|| completed.result.clone())
    }

    fn cached_heartbeat(&self, key: &ObservationKey, now: Instant) -> Option<RuntimeHeartbeat> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let cached = state.cached.as_ref()?;
        (cached.key == *key && now.duration_since(cached.observed_at) < DISPLAY_OBSERVATION_TTL)
            .then(|| cached.heartbeat.clone())
    }

    fn complete_cached(
        &self,
        ticket: ObservationTicket,
        key: &ObservationKey,
        now: Instant,
    ) -> bool {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.epoch != ticket.epoch
            || !state.cached.as_ref().is_some_and(|cached| {
                cached.key == *key
                    && now.duration_since(cached.observed_at) < DISPLAY_OBSERVATION_TTL
            })
        {
            return false;
        }
        let share_until = state
            .cached
            .as_ref()
            .expect("checked display observation")
            .observed_at
            + DISPLAY_OBSERVATION_TTL;
        // Joining callers may share this heartbeat, but it never extends the
        // original native observation's lifetime.
        state.completion = state.completion.wrapping_add(1);
        state.completed = Some(CompletedObservation {
            key: key.clone(),
            result: Ok(()),
            share_until,
        });
        true
    }

    fn complete_fresh(
        &self,
        ticket: ObservationTicket,
        key: ObservationKey,
        result: DesktopResult<()>,
        heartbeat: Option<RuntimeHeartbeat>,
        observed_at: Instant,
        completed_at: Instant,
    ) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.epoch != ticket.epoch {
            return;
        }
        state.completion = state.completion.wrapping_add(1);
        state.cached = if result.is_ok() && key.environment_id.is_some() {
            heartbeat.map(|heartbeat| CachedObservation {
                key: key.clone(),
                observed_at,
                heartbeat,
            })
        } else {
            None
        };
        // A failed request is shared only with callers already waiting for it.
        // A later request retries immediately instead of caching the failure.
        state.completed = Some(CompletedObservation {
            key,
            result,
            share_until: completed_at + DISPLAY_OBSERVATION_TTL,
        });
    }
}

impl AppState {
    /// An explicit refresh invalidates previous display evidence and always
    /// performs a new observation, including when a background read is in flight.
    pub async fn refresh_runtime_status(&self) -> DesktopResult<DesktopStateSnapshot> {
        self.runtime_observation.invalidate();
        self.runtime_status_observation(false).await
    }

    /// Periodic display only. This method never admits a Desktop operation or
    /// lends its cached result to service mutations or ownership verification.
    pub async fn observe_runtime_status(&self) -> DesktopResult<DesktopStateSnapshot> {
        self.runtime_status_observation(true).await
    }

    fn check_runtime_observation_admission(&self) -> DesktopResult<()> {
        if self.shutdown_signal.is_cancelled() {
            return Err(cancelled_error());
        }
        if let Some(current) = self.operations.observe()? {
            return Err(crate::operation::busy_error(&current));
        }
        if self.get_state().configuration_issue.is_some() {
            return Err(DesktopError::new(
                "configuration_migration_failed",
                "Configuration could not be migrated",
                "Open Diagnostics or explicitly restore the previous known-good configuration.",
            ));
        }
        Ok(())
    }

    async fn runtime_status_observation(
        &self,
        allow_cached: bool,
    ) -> DesktopResult<DesktopStateSnapshot> {
        self.check_runtime_observation_admission()?;
        let waiting = self.runtime_observation.ticket();
        let _refresh_guard = self.refresh_lock.lock().await;
        self.check_runtime_observation_admission()?;
        let key = ObservationKey::new(&self.get_state(), self.operations.generation());
        if allow_cached {
            if let Some(result) =
                self.runtime_observation
                    .completed_since(waiting, &key, Instant::now())
            {
                return result.map(|()| self.get_state());
            }
            if let Some(heartbeat) = self
                .runtime_observation
                .cached_heartbeat(&key, Instant::now())
            {
                let ticket = self.runtime_observation.ticket();
                let unchanged = self.runtime_heartbeat_matches(&key, &heartbeat).await;
                self.check_runtime_observation_admission()?;
                let current_key =
                    ObservationKey::new(&self.get_state(), self.operations.generation());
                if unchanged
                    && current_key == key
                    && self
                        .runtime_observation
                        .complete_cached(ticket, &key, Instant::now())
                {
                    // Read the canonical published state, not a retained copy.
                    // A cache hit must not reconcile errors as fresh evidence.
                    return Ok(self.get_state());
                }
                self.runtime_observation.invalidate();
            }
        }

        let before = self.get_state();
        let operation_generation = self.operations.generation();
        let before_key = ObservationKey::new(&before, operation_generation);
        let ticket = self.runtime_observation.ticket();
        let observation_started = Instant::now();
        let observed = self
            .refresh_runtime_status_uncached(before.state_generation)
            .await;
        let current_key = ObservationKey::new(&self.get_state(), self.operations.generation());
        match &observed {
            Ok(outcome)
                if outcome.applied
                    && current_key
                        == ObservationKey::new(&outcome.snapshot, operation_generation)
                    && current_key.environment_id == before_key.environment_id =>
            {
                self.runtime_observation.complete_fresh(
                    ticket,
                    current_key,
                    Ok(()),
                    outcome.heartbeat.clone(),
                    observation_started,
                    Instant::now(),
                );
            }
            Err(error) if current_key == before_key => {
                self.runtime_observation.complete_fresh(
                    ticket,
                    current_key,
                    Err(error.clone()),
                    None,
                    observation_started,
                    Instant::now(),
                );
            }
            _ => self.runtime_observation.invalidate(),
        }
        observed.map(|outcome| outcome.snapshot)
    }

    async fn runtime_heartbeat_matches(
        &self,
        key: &ObservationKey,
        expected: &RuntimeHeartbeat,
    ) -> bool {
        let probe = async {
            let store = environment::store().ok()?;
            let record = store.load_environment().ok()??;
            if Some(&record.environment_id) != key.environment_id.as_ref() {
                return None;
            }
            let mut native = webcodex_environment::NativeEnvironment::new().ok()?;
            // The existing EnvironmentBackend heartbeat uses authenticated HTTP
            // only. A changed/failed heartbeat triggers the full native query;
            // it never authorizes a service action or projects cached ownership.
            let observed =
                webcodex_environment::EnvironmentBackend::observe(&mut native, &store, &record)
                    .await
                    .ok()?;
            Some(RuntimeHeartbeat::from(&observed) == *expected)
        };
        tokio::select! {
            _ = self.shutdown_signal.cancelled() => false,
            result = tokio::time::timeout(DISPLAY_HEARTBEAT_TIMEOUT, probe) => {
                matches!(result, Ok(Some(true)))
            }
        }
    }
}

#[cfg(test)]
mod tests;
