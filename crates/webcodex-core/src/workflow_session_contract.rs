//! Stable protocol-neutral contracts shared with the Workflow Session domain.

use serde::{Deserialize, Serialize};

pub const TOOL_CALL_CONTEXT_REQUEST_INTERNAL_FIELD: &str = "__webcodex_stateless_context_request";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionShell {
    Sh,
    Bash,
}

impl ExecutionShell {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    #[default]
    Normal,
    ReadOnly,
}

impl SessionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::ReadOnly => "read_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOutcome {
    AutoApproved,
    Approved,
    Denied,
    Pending,
    HardDenied,
}

impl PermissionOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoApproved => "auto_approved",
            Self::Approved => "approved",
            Self::Denied => "denied",
            Self::Pending => "requested",
            Self::HardDenied => "hard_denied",
        }
    }

    pub fn allows_execution(self) -> bool {
        matches!(self, Self::AutoApproved | Self::Approved)
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto_approved" => Some(Self::AutoApproved),
            "approved" => Some(Self::Approved),
            "denied" | "expired" => Some(Self::Denied),
            "requested" | "pending" => Some(Self::Pending),
            "hard_denied" => Some(Self::HardDenied),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionDecision {
    pub required: bool,
    pub policy: String,
    pub request_id: String,
    pub status: String,
    pub reason: String,
    pub risk: String,
    pub tool_name: String,
    pub project: Option<String>,
}

impl PermissionDecision {
    pub fn outcome(&self) -> Option<PermissionOutcome> {
        PermissionOutcome::parse(&self.status)
    }

    pub fn allows_execution(&self) -> bool {
        self.outcome()
            .map(PermissionOutcome::allows_execution)
            .unwrap_or(false)
    }
}

pub fn is_safe_job_id(job_id: &str) -> bool {
    if job_id.is_empty() || job_id.len() > 80 || job_id.contains("..") {
        return false;
    }
    job_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}
