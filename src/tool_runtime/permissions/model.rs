//! Permission decision types and wire-stable constants.
//!
//! Wire shape of [`PermissionDecision`] is preserved for ledger / handoff
//! compatibility. Typed enums ([`PermissionMode`], [`PermissionOutcome`]) are
//! the internal model; string fields on the decision remain the serialized form.

use serde::{Deserialize, Serialize};

/// Default permission policy / mode name when `WEBCODEX_PERMISSION_MODE` is unset.
pub(crate) const DEFAULT_PERMISSION_POLICY: &str = "dev_auto_approve";

/// Recommended future release policy (documentation / profile only).
pub(crate) const RELEASE_RECOMMENDED_PERMISSION_POLICY: &str = "require_approval";

/// Bounded recent permission rows in session handoff summaries.
pub(crate) const DEFAULT_PERMISSION_RECENT_LIMIT: usize = 20;

/// Environment variable for the active permission mode.
pub(crate) const PERMISSION_MODE_ENV: &str = "WEBCODEX_PERMISSION_MODE";

/// Configurable permission mode (soft policy; never overrides hard safety).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PermissionMode {
    /// Default: auto-approve permission-bearing tools after hard safety.
    DevAutoApprove,
    /// Shadow recommendations; execution still allowed after hard safety.
    AuditOnly,
    /// Future human gate. Not implemented — must not pretend to approve.
    RequireApproval,
}

impl PermissionMode {
    pub(crate) const DEFAULT: Self = Self::DevAutoApprove;

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::DevAutoApprove => "dev_auto_approve",
            Self::AuditOnly => "audit_only",
            Self::RequireApproval => "require_approval",
        }
    }

    /// Parse a mode name (case-sensitive, trimmed by caller).
    pub(crate) fn parse(raw: &str) -> Result<Self, PermissionModeParseError> {
        match raw {
            "dev_auto_approve" => Ok(Self::DevAutoApprove),
            "audit_only" => Ok(Self::AuditOnly),
            "require_approval" => Ok(Self::RequireApproval),
            other => Err(PermissionModeParseError {
                value: other.to_string(),
            }),
        }
    }

    pub(crate) fn human_approval_required(self) -> bool {
        matches!(self, Self::RequireApproval)
    }

    pub(crate) fn auto_approve(self) -> bool {
        matches!(self, Self::DevAutoApprove | Self::AuditOnly)
    }
}

impl Default for PermissionMode {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Failed to parse a configured permission mode string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PermissionModeParseError {
    pub(crate) value: String,
}

impl std::fmt::Display for PermissionModeParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid {PERMISSION_MODE_ENV} value {:?}; expected one of: \
             dev_auto_approve, audit_only, require_approval",
            self.value
        )
    }
}

impl std::error::Error for PermissionModeParseError {}

/// Execution-eligibility outcome from the permission decision layer.
///
/// Distinct from HTTP/MCP protocol success. Wire form is stored on
/// [`PermissionDecision::status`].
///
/// Variants beyond the current scaffold (`Approved`, `Pending`, `HardDenied`)
/// exist so summaries and future modes share one parse table without thrash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // Approved / Pending / HardDenied reserved for later phases.
pub(crate) enum PermissionOutcome {
    AutoApproved,
    AuditOnlyAllowed,
    Approved,
    Denied,
    /// Ledger / summary historical label for pending requests.
    Pending,
    HardDenied,
}

impl PermissionOutcome {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::AutoApproved => "auto_approved",
            Self::AuditOnlyAllowed => "audit_only_allowed",
            Self::Approved => "approved",
            Self::Denied => "denied",
            // Summary counters historically match `requested` for pending.
            Self::Pending => "requested",
            Self::HardDenied => "hard_denied",
        }
    }

    /// Whether this outcome authorizes tool mutation / execution.
    ///
    /// Centralized so call sites never ad-hoc match outcomes. `audit_only`
    /// allows execution; denied / pending / hard-denied do not.
    pub(crate) fn allows_execution(self) -> bool {
        matches!(
            self,
            Self::AutoApproved | Self::AuditOnlyAllowed | Self::Approved
        )
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        match raw {
            "auto_approved" => Some(Self::AutoApproved),
            "audit_only_allowed" => Some(Self::AuditOnlyAllowed),
            "approved" => Some(Self::Approved),
            "denied" | "expired" => Some(Self::Denied),
            "requested" | "pending" => Some(Self::Pending),
            "hard_denied" => Some(Self::HardDenied),
            _ => None,
        }
    }
}

/// Permission decision attached to high-risk tool results and session ledger events.
///
/// Field names and semantics are wire-stable for existing clients and handoffs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct PermissionDecision {
    pub(crate) required: bool,
    pub(crate) policy: String,
    pub(crate) request_id: String,
    /// Wire name for outcome (`auto_approved`, `denied`, …).
    pub(crate) status: String,
    pub(crate) reason: String,
    pub(crate) risk: String,
    pub(crate) tool_name: String,
    pub(crate) project: Option<String>,
}

impl PermissionDecision {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn outcome(&self) -> Option<PermissionOutcome> {
        PermissionOutcome::parse(&self.status)
    }

    /// Whether this decision authorizes tool mutation / execution.
    ///
    /// Unknown or unparsable `status` values fail closed (do not execute).
    pub(crate) fn allows_execution(&self) -> bool {
        self.outcome()
            .map(PermissionOutcome::allows_execution)
            .unwrap_or(false)
    }

    pub(crate) fn new(
        policy: impl Into<String>,
        outcome: PermissionOutcome,
        reason: impl Into<String>,
        risk: impl Into<String>,
        tool_name: impl Into<String>,
        project: Option<&str>,
    ) -> Self {
        Self {
            required: true,
            policy: policy.into(),
            request_id: format!("wc_perm_{}", uuid::Uuid::new_v4().simple()),
            status: outcome.as_str().to_string(),
            reason: reason.into(),
            risk: risk.into(),
            tool_name: tool_name.into(),
            project: project.map(str::to_string),
        }
    }
}
