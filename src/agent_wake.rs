use crate::db::{
    AgentWakeClaim, AgentWakeEnvelope, AgentWakeRecord, CommunicationPrincipal,
    CommunicationStoreError, Database,
};

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
    pub(crate) const fn new(kind: &'static str) -> Self {
        Self { kind }
    }
}

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
pub(crate) trait ContinuationAdapter {
    fn adapter_kind(&self) -> &'static str;

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

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_next_agent_wake<A: ContinuationAdapter>(
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
