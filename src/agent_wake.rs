use crate::db::{
    AgentEndpointRecord, AgentWakeClaim, AgentWakeEnvelope, AgentWakeRecord,
    CommunicationPrincipal, CommunicationStoreError, Database,
};
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContinuationPreflight {
    pub(crate) wake_id: String,
    pub(crate) agent_id: String,
    pub(crate) endpoint_id: String,
    pub(crate) controller_generation: i64,
}

impl From<&AgentWakeClaim> for ContinuationPreflight {
    fn from(claim: &AgentWakeClaim) -> Self {
        Self {
            wake_id: claim.wake.wake_id.clone(),
            agent_id: claim.wake.target_agent_id.clone(),
            endpoint_id: claim.attempt.endpoint_id.clone(),
            controller_generation: claim.attempt.controller_generation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContinuationPreflightError {
    pub(crate) kind: &'static str,
}

impl ContinuationPreflightError {
    #[cfg(test)]
    pub(crate) const fn new(kind: &'static str) -> Self {
        Self { kind }
    }
}

// Production variants are returned by Host adapters. The current tree ships
// only deterministic test adapters; keeping this narrow contract is intentional.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContinuationDispatchOutcome {
    Delivered,
    OutcomeUnknown,
}

/// Narrow Host boundary for delivering one already-durable Agent continuation.
///
/// `preflight` runs before the durable dispatch fence and therefore must not
/// resume a model turn. `dispatch` runs only after the Wake Attempt is durably
/// prepared; any non-acknowledged result must be reported as `OutcomeUnknown`
/// rather than silently retried.
pub(crate) trait ContinuationAdapter: Send + Sync {
    fn adapter_kind(&self) -> &'static str;

    /// True only for a demonstrated Host primitive that can request a fresh
    /// production model turn. Deterministic/fake/manual test adapters leave
    /// this false.
    fn production_auto_resume_available(&self) -> bool {
        false
    }

    fn preflight(
        &self,
        continuation: &ContinuationPreflight,
    ) -> Result<(), ContinuationPreflightError>;

    fn dispatch(&self, envelope: &AgentWakeEnvelope) -> ContinuationDispatchOutcome;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AgentWakeDispatchReport {
    NoPendingWake,
    ReleasedBeforeDispatch {
        wake: AgentWakeRecord,
        adapter_error_kind: &'static str,
    },
    Delivered {
        wake: AgentWakeRecord,
    },
    DeliveryUnknown {
        wake: AgentWakeRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentHostBindingStatus {
    pub(crate) adapter_registered: bool,
    pub(crate) adapter_kind: Option<String>,
    pub(crate) production_auto_resume_available: bool,
}

#[derive(Clone)]
struct EndpointContinuationBinding {
    principal: CommunicationPrincipal,
    agent_id: String,
    endpoint_id: String,
    controller_generation: i64,
    adapter: Arc<dyn ContinuationAdapter>,
}

struct AgentContinuationControllerState {
    db: Arc<Database>,
    bindings: Mutex<HashMap<String, EndpointContinuationBinding>>,
    /// Exact Endpoints newly attached through this controller's ToolRuntime.
    /// This set is process-local so a successor cannot re-register a callback
    /// against a pre-restart Endpoint without an explicit replacement attach.
    attached_endpoints: Mutex<HashMap<String, (String, i64)>>,
    /// false = queued/running with no later event; true = at least one event
    /// arrived while the current dispatch opportunity was queued/running.
    scheduled_agents: Mutex<HashMap<String, bool>>,
}

/// Process-local Host adapter registry plus bounded event-driven Wake dispatcher.
///
/// Durable truth remains in SQLite. This registry stores only callable adapter
/// handles and exact Endpoint/generation bindings; a new process starts empty.
/// A bounded deduplicating queue reacts to Message commit, adapter registration,
/// and exact Wake consume without permanent polling.
#[derive(Clone)]
pub(crate) struct AgentContinuationController {
    state: Arc<AgentContinuationControllerState>,
    dispatch_tx: mpsc::SyncSender<String>,
}

impl AgentContinuationController {
    const DISPATCH_QUEUE_CAPACITY: usize = crate::db::MAX_DURABLE_AGENTS as usize;

    pub(crate) fn new(db: Arc<Database>) -> Self {
        let state = Arc::new(AgentContinuationControllerState {
            db,
            bindings: Mutex::new(HashMap::new()),
            attached_endpoints: Mutex::new(HashMap::new()),
            scheduled_agents: Mutex::new(HashMap::new()),
        });
        let (dispatch_tx, dispatch_rx) =
            mpsc::sync_channel::<String>(Self::DISPATCH_QUEUE_CAPACITY);
        let worker_state = state.clone();
        std::thread::Builder::new()
            .name("webcodex-agent-continuations".to_string())
            .spawn(move || {
                while let Ok(agent_id) = dispatch_rx.recv() {
                    loop {
                        worker_state.dispatch_one(&agent_id);
                        let mut scheduled = worker_state
                            .scheduled_agents
                            .lock()
                            .expect("Agent continuation schedule mutex poisoned");
                        let run_again = scheduled.get_mut(&agent_id).is_some_and(|dirty| {
                            let run_again = *dirty;
                            *dirty = false;
                            run_again
                        });
                        if !run_again {
                            scheduled.remove(&agent_id);
                            break;
                        }
                    }
                }
            })
            .expect("Agent continuation controller thread must start");
        Self { state, dispatch_tx }
    }

    /// Register a real process-local adapter for one exact current Endpoint.
    /// The durable capability bit is projected only after the callable handle
    /// exists; a failed projection removes the handle again.
    pub(crate) fn register_endpoint_adapter(
        &self,
        principal: CommunicationPrincipal,
        agent_id: String,
        endpoint_id: String,
        controller_generation: i64,
        adapter: Arc<dyn ContinuationAdapter>,
    ) -> Result<AgentEndpointRecord, CommunicationStoreError> {
        // Authorize the exact durable binding before consulting process-local
        // state so unauthorized probes cannot learn whether a Host callback is
        // registered or was attached in this process.
        self.state.db.verify_current_agent_endpoint(
            &principal,
            &agent_id,
            &endpoint_id,
            controller_generation,
        )?;
        let attached_here = self
            .state
            .attached_endpoints
            .lock()
            .expect("Agent continuation attachment mutex poisoned")
            .get(&agent_id)
            .is_some_and(|(attached_endpoint_id, attached_generation)| {
                attached_endpoint_id == &endpoint_id
                    && *attached_generation == controller_generation
            });
        if !attached_here {
            return Err(CommunicationStoreError::new(
                "endpoint_not_attached_in_process",
                "Registering a Host adapter requires a fresh Endpoint attach in this Server process",
            ));
        }
        let binding = EndpointContinuationBinding {
            principal: principal.clone(),
            agent_id: agent_id.clone(),
            endpoint_id: endpoint_id.clone(),
            controller_generation,
            adapter,
        };
        let previous = self
            .state
            .bindings
            .lock()
            .expect("Agent continuation registry mutex poisoned")
            .insert(agent_id.clone(), binding);
        let endpoint = match self.state.db.set_agent_endpoint_wake_capability(
            &principal,
            &agent_id,
            &endpoint_id,
            controller_generation,
            true,
        ) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                let mut bindings = self
                    .state
                    .bindings
                    .lock()
                    .expect("Agent continuation registry mutex poisoned");
                if bindings.get(&agent_id).is_some_and(|binding| {
                    binding.endpoint_id == endpoint_id
                        && binding.controller_generation == controller_generation
                }) {
                    if let Some(previous) = previous {
                        bindings.insert(agent_id, previous);
                    } else {
                        bindings.remove(&agent_id);
                    }
                }
                return Err(error);
            }
        };
        self.schedule_agent(&agent_id);
        Ok(endpoint)
    }

    pub(crate) fn unregister_endpoint_adapter(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        controller_generation: i64,
    ) -> Result<AgentEndpointRecord, CommunicationStoreError> {
        let endpoint = self.state.db.set_agent_endpoint_wake_capability(
            principal,
            agent_id,
            endpoint_id,
            controller_generation,
            false,
        )?;
        self.remove_exact_binding(agent_id, endpoint_id, controller_generation);
        Ok(endpoint)
    }

    /// Fence process-local state after a durable replacement attach. Exact
    /// idempotent attach replay preserves the already-registered binding.
    pub(crate) fn reconcile_attached_endpoint(
        &self,
        agent_id: &str,
        endpoint_id: &str,
        controller_generation: i64,
    ) {
        self.state
            .attached_endpoints
            .lock()
            .expect("Agent continuation attachment mutex poisoned")
            .insert(
                agent_id.to_string(),
                (endpoint_id.to_string(), controller_generation),
            );
        let mut bindings = self
            .state
            .bindings
            .lock()
            .expect("Agent continuation registry mutex poisoned");
        if bindings.get(agent_id).is_some_and(|binding| {
            binding.endpoint_id != endpoint_id
                || binding.controller_generation != controller_generation
        }) {
            bindings.remove(agent_id);
        }
    }

    pub(crate) fn endpoint_detached(
        &self,
        agent_id: &str,
        endpoint_id: &str,
        controller_generation: i64,
    ) {
        self.remove_exact_binding(agent_id, endpoint_id, controller_generation);
        let mut attached_endpoints = self
            .state
            .attached_endpoints
            .lock()
            .expect("Agent continuation attachment mutex poisoned");
        if attached_endpoints.get(agent_id).is_some_and(
            |(attached_endpoint_id, attached_generation)| {
                attached_endpoint_id == endpoint_id && *attached_generation == controller_generation
            },
        ) {
            attached_endpoints.remove(agent_id);
        }
    }

    pub(crate) fn binding_status(
        &self,
        agent_id: &str,
        endpoint_id: &str,
        controller_generation: i64,
    ) -> AgentHostBindingStatus {
        let bindings = self
            .state
            .bindings
            .lock()
            .expect("Agent continuation registry mutex poisoned");
        let binding = bindings.get(agent_id).filter(|binding| {
            binding.endpoint_id == endpoint_id
                && binding.controller_generation == controller_generation
        });
        AgentHostBindingStatus {
            adapter_registered: binding.is_some(),
            adapter_kind: binding.map(|binding| binding.adapter.adapter_kind().to_string()),
            production_auto_resume_available: binding
                .is_some_and(|binding| binding.adapter.production_auto_resume_available()),
        }
    }

    pub(crate) fn schedule_agent(&self, agent_id: &str) {
        let mut scheduled = self
            .state
            .scheduled_agents
            .lock()
            .expect("Agent continuation schedule mutex poisoned");
        if let Some(dirty) = scheduled.get_mut(agent_id) {
            *dirty = true;
            return;
        }
        scheduled.insert(agent_id.to_string(), false);
        match self.dispatch_tx.try_send(agent_id.to_string()) {
            Ok(()) => {}
            Err(error) => {
                scheduled.remove(agent_id);
                tracing::warn!(
                    agent_id,
                    error = %error,
                    "Agent continuation dispatch queue is unavailable; durable Wake remains authoritative"
                );
            }
        }
    }

    fn remove_exact_binding(&self, agent_id: &str, endpoint_id: &str, controller_generation: i64) {
        let mut bindings = self
            .state
            .bindings
            .lock()
            .expect("Agent continuation registry mutex poisoned");
        if bindings.get(agent_id).is_some_and(|binding| {
            binding.endpoint_id == endpoint_id
                && binding.controller_generation == controller_generation
        }) {
            bindings.remove(agent_id);
        }
    }
}

impl AgentContinuationControllerState {
    fn dispatch_one(&self, agent_id: &str) {
        let binding = self
            .bindings
            .lock()
            .expect("Agent continuation registry mutex poisoned")
            .get(agent_id)
            .cloned();
        let Some(binding) = binding else {
            return;
        };
        let result = dispatch_next_agent_wake(
            &self.db,
            &binding.principal,
            &binding.agent_id,
            &binding.endpoint_id,
            binding.controller_generation,
            binding.adapter.as_ref(),
        );
        match result {
            Ok(AgentWakeDispatchReport::NoPendingWake)
            | Ok(AgentWakeDispatchReport::ReleasedBeforeDispatch { .. })
            | Ok(AgentWakeDispatchReport::Delivered { .. })
            | Ok(AgentWakeDispatchReport::DeliveryUnknown { .. }) => {}
            Err(error)
                if matches!(
                    error.code(),
                    "endpoint_not_found"
                        | "endpoint_expired"
                        | "endpoint_detached"
                        | "endpoint_not_active"
                        | "endpoint_not_wake_capable"
                        | "endpoint_generation_stale"
                        | "endpoint_agent_mismatch"
                ) =>
            {
                let mut bindings = self
                    .bindings
                    .lock()
                    .expect("Agent continuation registry mutex poisoned");
                if bindings.get(agent_id).is_some_and(|current| {
                    current.endpoint_id == binding.endpoint_id
                        && current.controller_generation == binding.controller_generation
                }) {
                    bindings.remove(agent_id);
                }
            }
            Err(error) => {
                tracing::warn!(
                    agent_id,
                    endpoint_id = %binding.endpoint_id,
                    controller_generation = binding.controller_generation,
                    error_kind = error.code(),
                    "Agent continuation dispatch event failed; durable Wake state is preserved"
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_next_agent_wake<A: ContinuationAdapter + ?Sized>(
    db: &Database,
    principal: &CommunicationPrincipal,
    agent_id: &str,
    endpoint_id: &str,
    expected_controller_generation: i64,
    adapter: &A,
) -> Result<AgentWakeDispatchReport, CommunicationStoreError> {
    let Some(claim) = db.claim_next_agent_wake(
        principal,
        agent_id,
        endpoint_id,
        expected_controller_generation,
        adapter.adapter_kind(),
    )?
    else {
        return Ok(AgentWakeDispatchReport::NoPendingWake);
    };

    if let Err(error) = adapter.preflight(&ContinuationPreflight::from(&claim)) {
        let wake = db.release_agent_wake_claim(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            &claim.wake.wake_id,
            &claim.attempt.attempt_id,
            &claim.claim_fence,
        )?;
        return Ok(AgentWakeDispatchReport::ReleasedBeforeDispatch {
            wake,
            adapter_error_kind: error.kind,
        });
    }

    let prepared = db.prepare_agent_wake_dispatch(
        principal,
        agent_id,
        endpoint_id,
        expected_controller_generation,
        &claim.wake.wake_id,
        &claim.attempt.attempt_id,
        &claim.claim_fence,
        &claim.consume_token,
    )?;

    db.verify_agent_wake_dispatch_binding(
        principal,
        agent_id,
        endpoint_id,
        expected_controller_generation,
        &claim.wake.wake_id,
        &claim.attempt.attempt_id,
        &claim.claim_fence,
    )?;

    match adapter.dispatch(&prepared.envelope) {
        ContinuationDispatchOutcome::Delivered => {
            let wake = db.complete_agent_wake_delivery(
                principal,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                &claim.wake.wake_id,
                &claim.attempt.attempt_id,
                &claim.claim_fence,
            )?;
            Ok(AgentWakeDispatchReport::Delivered { wake })
        }
        ContinuationDispatchOutcome::OutcomeUnknown => {
            let wake = db.mark_agent_wake_delivery_unknown(
                principal,
                agent_id,
                endpoint_id,
                expected_controller_generation,
                &claim.wake.wake_id,
                &claim.attempt.attempt_id,
                &claim.claim_fence,
            )?;
            Ok(AgentWakeDispatchReport::DeliveryUnknown { wake })
        }
    }
}
