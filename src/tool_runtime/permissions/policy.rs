//! Permission mode policy: how each mode maps to a decision outcome.
//!
//! Hard safety rules (session guard, path policy, scopes, …) are **not**
//! implemented here and must never be weakened by mode choice.

use super::model::{
    PermissionDecision, PermissionMode, PermissionModeParseError, PermissionOutcome,
    DEFAULT_PERMISSION_POLICY, PERMISSION_MODE_ENV, RELEASE_RECOMMENDED_PERMISSION_POLICY,
};
use serde_json::{json, Value};

/// Effective configuration used by the evaluator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EffectivePermissionConfig {
    /// A known mode (including the default when the env var is unset).
    Active(PermissionMode),
    /// Env was set to an unrecognized value — refuse auto-approval.
    InvalidMode { value: String },
}

impl EffectivePermissionConfig {
    /// Resolve `WEBCODEX_PERMISSION_MODE` from the process environment.
    ///
    /// Unset or empty → [`PermissionMode::DevAutoApprove`].
    /// Unknown non-empty value → [`EffectivePermissionConfig::InvalidMode`].
    pub(crate) fn from_env() -> Self {
        match std::env::var(PERMISSION_MODE_ENV) {
            Err(std::env::VarError::NotPresent) => Self::Active(PermissionMode::DEFAULT),
            Err(std::env::VarError::NotUnicode(_)) => Self::InvalidMode {
                value: "<non-utf8>".to_string(),
            },
            Ok(raw) => Self::from_raw(Some(raw.as_str())),
        }
    }

    /// Resolve from an optional raw mode string (tests and explicit config).
    pub(crate) fn from_raw(raw: Option<&str>) -> Self {
        match resolve_permission_mode(raw) {
            Ok(mode) => Self::Active(mode),
            Err(err) => Self::InvalidMode { value: err.value },
        }
    }

    #[cfg(test)]
    pub(crate) fn with_mode(mode: PermissionMode) -> Self {
        Self::Active(mode)
    }

    pub(crate) fn policy_name(&self) -> &str {
        match self {
            Self::Active(mode) => mode.as_str(),
            Self::InvalidMode { .. } => "invalid",
        }
    }

    pub(crate) fn human_approval_required(&self) -> bool {
        match self {
            Self::Active(mode) => mode.human_approval_required(),
            // Fail closed: do not advertise frictionless auto-approve.
            Self::InvalidMode { .. } => true,
        }
    }

    pub(crate) fn auto_approve(&self) -> bool {
        match self {
            Self::Active(mode) => mode.auto_approve(),
            Self::InvalidMode { .. } => false,
        }
    }
}

/// Resolve a mode from optional raw config.
///
/// - `None` / empty / whitespace → default `dev_auto_approve`
/// - known mode name → `Ok`
/// - anything else → `Err` with the invalid value
pub(crate) fn resolve_permission_mode(
    raw: Option<&str>,
) -> Result<PermissionMode, PermissionModeParseError> {
    match raw {
        None => Ok(PermissionMode::DEFAULT),
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Ok(PermissionMode::DEFAULT)
            } else {
                PermissionMode::parse(trimmed)
            }
        }
    }
}

/// Apply mode policy for a permission-bearing tool call.
///
/// Caller must already have determined that the tool requires permission.
pub(crate) fn decide_for_required_tool(
    config: &EffectivePermissionConfig,
    tool_name: &str,
    project: Option<&str>,
    risk: &str,
) -> PermissionDecision {
    match config {
        EffectivePermissionConfig::Active(PermissionMode::DevAutoApprove) => {
            PermissionDecision::new(
                PermissionMode::DevAutoApprove.as_str(),
                PermissionOutcome::AutoApproved,
                // Wire-compatible reason echo of the policy name.
                DEFAULT_PERMISSION_POLICY,
                risk,
                tool_name,
                project,
            )
        }
        EffectivePermissionConfig::Active(PermissionMode::AuditOnly) => PermissionDecision::new(
            PermissionMode::AuditOnly.as_str(),
            PermissionOutcome::AuditOnlyAllowed,
            "audit_only",
            risk,
            tool_name,
            project,
        ),
        EffectivePermissionConfig::Active(PermissionMode::RequireApproval) => {
            // Real pending/approve is not implemented. Never emit auto_approved.
            PermissionDecision::new(
                PermissionMode::RequireApproval.as_str(),
                PermissionOutcome::Denied,
                "require_approval_not_implemented",
                risk,
                tool_name,
                project,
            )
        }
        EffectivePermissionConfig::InvalidMode { value } => PermissionDecision::new(
            "invalid",
            PermissionOutcome::Denied,
            format!("invalid_permission_mode:{value}"),
            risk,
            tool_name,
            project,
        ),
    }
}

/// Operator-facing permission profile for runtime_status / coding-task startup.
pub(crate) fn permission_profile_payload_for(config: &EffectivePermissionConfig) -> Value {
    json!({
        "policy": match config {
            // Profile schema historically lists dev_auto_approve / require_approval /
            // disabled / off. Keep known public names; surface audit_only and
            // invalid as their policy strings for honesty without OpenAPI edits.
            EffectivePermissionConfig::Active(mode) => mode.as_str(),
            EffectivePermissionConfig::InvalidMode { .. } => "invalid",
        },
        "human_approval_required": config.human_approval_required(),
        "auto_approve": config.auto_approve(),
        "release_recommended_policy": RELEASE_RECOMMENDED_PERMISSION_POLICY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_and_empty_resolve_to_dev_auto_approve() {
        assert_eq!(
            resolve_permission_mode(None).unwrap(),
            PermissionMode::DevAutoApprove
        );
        assert_eq!(
            resolve_permission_mode(Some("")).unwrap(),
            PermissionMode::DevAutoApprove
        );
        assert_eq!(
            resolve_permission_mode(Some("   ")).unwrap(),
            PermissionMode::DevAutoApprove
        );
    }

    #[test]
    fn known_modes_parse() {
        assert_eq!(
            resolve_permission_mode(Some("dev_auto_approve")).unwrap(),
            PermissionMode::DevAutoApprove
        );
        assert_eq!(
            resolve_permission_mode(Some("audit_only")).unwrap(),
            PermissionMode::AuditOnly
        );
        assert_eq!(
            resolve_permission_mode(Some("require_approval")).unwrap(),
            PermissionMode::RequireApproval
        );
    }

    #[test]
    fn illegal_mode_is_explicit_error() {
        let err = resolve_permission_mode(Some("nope")).unwrap_err();
        assert_eq!(err.value, "nope");
        let message = err.to_string();
        assert!(message.contains(PERMISSION_MODE_ENV), "{message}");
        assert!(message.contains("dev_auto_approve"), "{message}");
        assert!(message.contains("nope"), "{message}");
    }
}
