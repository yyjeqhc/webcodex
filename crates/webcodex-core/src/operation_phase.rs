//! Canonical cross-domain operation phase projection.
//!
//! This is deliberately a projection layer, not a replacement storage state
//! machine. Runner Jobs and durable service deployment receipts retain their
//! domain-specific lifecycle truth while external tools can reason about one
//! bounded vocabulary across both domains.

use crate::runner_job_lifecycle::RunnerJobLifecycle;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationPhase {
    Accepted,
    Queued,
    Running,
    WaitingExternal,
    Recovering,
    Succeeded,
    Failed,
    RolledBack,
    OutcomeUnknown,
}

impl OperationPhase {
    pub const fn as_wire(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingExternal => "waiting_external",
            Self::Recovering => "recovering",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::RolledBack | Self::OutcomeUnknown
        )
    }

    /// Project one Runner Job lifecycle plus the Server recovery overlay.
    /// Recovery wins while the underlying Runner-owned lifecycle is still active.
    pub const fn from_runner_job(lifecycle: RunnerJobLifecycle, recovering: bool) -> Self {
        if recovering {
            return Self::Recovering;
        }
        match lifecycle {
            RunnerJobLifecycle::Queued | RunnerJobLifecycle::RunnerQueued => Self::Queued,
            RunnerJobLifecycle::StartedLegacy
            | RunnerJobLifecycle::Running
            | RunnerJobLifecycle::StopRequested => Self::Running,
            RunnerJobLifecycle::Completed => Self::Succeeded,
            // A lost Runner Job is terminal only in the sense that the Server has
            // exhausted reconciliation. It does not prove the side effect's true
            // execution outcome, so never project it as an ordinary failure that
            // might invite a blind retry.
            RunnerJobLifecycle::Lost => Self::OutcomeUnknown,
            RunnerJobLifecycle::Failed
            | RunnerJobLifecycle::Stopped
            | RunnerJobLifecycle::Timeout
            | RunnerJobLifecycle::TimedOut
            | RunnerJobLifecycle::Cancelled => Self::Failed,
        }
    }

    /// Project the durable deployment receipt state without coupling core to the
    /// store crate. Unknown values fail closed instead of inventing progress.
    pub fn from_deployment_receipt_state(state: &str) -> Option<Self> {
        match state {
            "planned" => Some(Self::Accepted),
            "draining" | "ready" => Some(Self::WaitingExternal),
            "switching" | "verifying" => Some(Self::Running),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "rolled_back" => Some(Self::RolledBack),
            "outcome_unknown" => Some(Self::OutcomeUnknown),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_job_projection_preserves_unknown_outcome_and_recovery() {
        assert_eq!(
            OperationPhase::from_runner_job(RunnerJobLifecycle::Running, true),
            OperationPhase::Recovering
        );
        assert_eq!(
            OperationPhase::from_runner_job(RunnerJobLifecycle::Completed, false),
            OperationPhase::Succeeded
        );
        assert_eq!(
            OperationPhase::from_runner_job(RunnerJobLifecycle::Lost, false),
            OperationPhase::OutcomeUnknown
        );
        assert_eq!(
            OperationPhase::from_runner_job(RunnerJobLifecycle::TimedOut, false),
            OperationPhase::Failed
        );
    }

    #[test]
    fn deployment_projection_uses_same_terminal_vocabulary() {
        assert_eq!(
            OperationPhase::from_deployment_receipt_state("planned"),
            Some(OperationPhase::Accepted)
        );
        assert_eq!(
            OperationPhase::from_deployment_receipt_state("ready"),
            Some(OperationPhase::WaitingExternal)
        );
        assert_eq!(
            OperationPhase::from_deployment_receipt_state("switching"),
            Some(OperationPhase::Running)
        );
        assert_eq!(
            OperationPhase::from_deployment_receipt_state("rolled_back"),
            Some(OperationPhase::RolledBack)
        );
        assert_eq!(OperationPhase::from_deployment_receipt_state("bogus"), None);
    }
}
