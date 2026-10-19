use super::jobs::PendingRequestEnqueueError;
use super::{DETACHED_IDEMPOTENCY_CONFLICT, DETACHED_IDEMPOTENCY_RECOVERY_PREFIX};
use std::fmt;

/// A rejection before this request is enqueued. This is not execution outcome
/// evidence: detached recovery can refer to an already existing logical Job.
/// Keep the reason typed until the caller projects its machine code and message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionAdmissionError(AdmissionRejection);

#[derive(Debug, Clone, PartialEq, Eq)]
enum AdmissionRejection {
    Pending(PendingRequestEnqueueError),
    InvalidRequest(String),
    CapabilityUnavailable(String),
    PermissionDenied(String),
    TargetChanged(String),
    Unavailable(String),
    IdempotencyConflict(String),
    IdempotencyRecoveryRequired { job_id: String },
}

impl ExecutionAdmissionError {
    pub(crate) fn invalid_request(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::InvalidRequest(message.into()))
    }

    pub(crate) fn unknown_runner(client_id: impl Into<String>) -> Self {
        PendingRequestEnqueueError::UnknownRunner {
            client_id: client_id.into(),
        }
        .into()
    }

    pub(crate) fn capability_unavailable(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::CapabilityUnavailable(message.into()))
    }

    pub(crate) fn permission_denied(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::PermissionDenied(message.into()))
    }

    pub(crate) fn target_changed(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::TargetChanged(message.into()))
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::Unavailable(message.into()))
    }

    pub(crate) fn idempotency_conflict(message: impl Into<String>) -> Self {
        Self(AdmissionRejection::IdempotencyConflict(message.into()))
    }

    pub(crate) fn idempotency_recovery(job_id: String) -> Self {
        Self(AdmissionRejection::IdempotencyRecoveryRequired { job_id })
    }

    /// Stable ToolRuntime projection; display text and caller identifiers never
    /// participate in classification. Lifecycle state remains owned by the caller.
    pub fn failure_kind(&self) -> &'static str {
        match &self.0 {
            AdmissionRejection::Pending(PendingRequestEnqueueError::UnknownRunner { .. })
            | AdmissionRejection::Pending(PendingRequestEnqueueError::RunnerOffline { .. }) => {
                "agent_offline"
            }
            AdmissionRejection::Pending(PendingRequestEnqueueError::InvalidOperation {
                ..
            })
            | AdmissionRejection::InvalidRequest(_) => "invalid_arguments",
            AdmissionRejection::CapabilityUnavailable(_) => "capability_unavailable",
            AdmissionRejection::PermissionDenied(_) => "permission_denied",
            AdmissionRejection::IdempotencyConflict(_) => "idempotency_conflict",
            AdmissionRejection::IdempotencyRecoveryRequired { .. } => {
                "idempotency_recovery_required"
            }
            AdmissionRejection::Pending(PendingRequestEnqueueError::QueueFull { .. })
            | AdmissionRejection::Pending(PendingRequestEnqueueError::Maintenance)
            | AdmissionRejection::TargetChanged(_)
            | AdmissionRejection::Unavailable(_) => "runtime_error",
        }
    }

    pub fn recovery_job_id(&self) -> Option<&str> {
        match &self.0 {
            AdmissionRejection::IdempotencyRecoveryRequired { job_id } => Some(job_id),
            _ => None,
        }
    }

    pub fn is_idempotency_conflict(&self) -> bool {
        matches!(&self.0, AdmissionRejection::IdempotencyConflict(_))
    }
}

impl From<PendingRequestEnqueueError> for ExecutionAdmissionError {
    fn from(error: PendingRequestEnqueueError) -> Self {
        Self(AdmissionRejection::Pending(error))
    }
}

impl fmt::Display for ExecutionAdmissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            AdmissionRejection::Pending(error) => fmt::Display::fmt(error, formatter),
            AdmissionRejection::InvalidRequest(message)
            | AdmissionRejection::CapabilityUnavailable(message)
            | AdmissionRejection::PermissionDenied(message)
            | AdmissionRejection::TargetChanged(message)
            | AdmissionRejection::Unavailable(message) => formatter.write_str(message),
            AdmissionRejection::IdempotencyConflict(message) => {
                write!(formatter, "{DETACHED_IDEMPOTENCY_CONFLICT}: {message}")
            }
            AdmissionRejection::IdempotencyRecoveryRequired { job_id } => {
                write!(formatter, "{DETACHED_IDEMPOTENCY_RECOVERY_PREFIX}{job_id}")
            }
        }
    }
}

impl std::error::Error for ExecutionAdmissionError {}
