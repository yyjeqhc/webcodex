//! Scope definitions and validation for the WebCodex auth system.
//!
//! Scopes are string-based permissions attached to tokens. Bootstrap auth is
//! treated as holding every scope. PAT (personal API token) and future OAuth2
//! tokens carry an explicit set of granted scopes.

use std::collections::HashSet;

// ---------------------------------------------------------------------------
// Scope constants
// ---------------------------------------------------------------------------

/// The set of scopes a Phase 2 personal API token may carry. Bootstrap auth is
/// treated as having the `admin` scope (full access). Stored space-separated in
/// the database; parsed into a list on read.
pub const SCOPE_RUNTIME_READ: &str = "runtime:read";
pub const SCOPE_PROJECT_READ: &str = "project:read";
pub const SCOPE_PROJECT_WRITE: &str = "project:write";
pub const SCOPE_JOB_RUN: &str = "job:run";
pub const SCOPE_AGENT_REGISTER: &str = "agent:register";
pub const SCOPE_ADMIN: &str = "admin";

/// Phase 3 agent transport scopes. Agent tokens may only carry `agent:*`
/// scopes and may only be used on agent transport endpoints. They are rejected
/// by all normal runtime/project/admin/user-token-management endpoints.
pub const SCOPE_AGENT_POLL: &str = "agent:poll";
pub const SCOPE_AGENT_RESULT: &str = "agent:result";
pub const SCOPE_AGENT_JOB_UPDATE: &str = "agent:job_update";
pub const SCOPE_ACCOUNT_MANAGE: &str = "account:manage";

/// The complete set of agent transport scopes, in canonical order.
pub const AGENT_SCOPES: &[&str] = &[
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
];

/// All scopes recognized by this phase. Unknown scopes are rejected at token
/// creation time so the stored scope string stays clean.
pub const KNOWN_SCOPES: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
    SCOPE_AGENT_REGISTER,
    SCOPE_AGENT_POLL,
    SCOPE_AGENT_RESULT,
    SCOPE_AGENT_JOB_UPDATE,
    SCOPE_ADMIN,
];

/// True when `scope` is one of the agent transport scopes.
pub(crate) fn is_agent_scope(scope: &str) -> bool {
    AGENT_SCOPES.contains(&scope)
}

// ---------------------------------------------------------------------------
// Scope validation
// ---------------------------------------------------------------------------

/// Validate and normalize a list of agent transport scopes. Returns an error
/// if any scope is not an `agent:*` scope. Rejects duplicates and unknown
/// scopes.
pub(crate) fn validate_agent_scopes(scopes: &[String]) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(scopes.len());
    for raw in scopes {
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if !is_agent_scope(s) {
            return Err(format!(
                "agent tokens may only carry agent:* scopes; got '{}'",
                s
            ));
        }
        if !seen.insert(s.to_string()) {
            continue;
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// Validate and normalize a list of scopes. Returns the cleaned scope list.
/// Rejects duplicates and unknown scopes.
pub(crate) fn validate_scopes(scopes: &[String]) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut out = Vec::with_capacity(scopes.len());
    for raw in scopes {
        let s = raw.trim();
        if s.is_empty() {
            continue;
        }
        if !KNOWN_SCOPES.contains(&s) {
            return Err(format!("unknown scope '{}'", s));
        }
        if !seen.insert(s.to_string()) {
            continue;
        }
        out.push(s.to_string());
    }
    Ok(out)
}

/// Serialize a scope list into the space-separated storage form.
pub(crate) fn scopes_to_string(scopes: &[String]) -> String {
    scopes.join(" ")
}

// ---------------------------------------------------------------------------
// Scope authorization helpers (used by handlers and middleware)
// ---------------------------------------------------------------------------

/// Check whether a set of granted scopes includes the required scope, treating
/// `admin` as a wildcard that satisfies any requirement. Bootstrap callers
/// should pass an `admin`-containing scope set.
pub(crate) fn scopes_include(granted: &[String], required: &str) -> bool {
    granted.iter().any(|s| s == required || s == SCOPE_ADMIN)
}

/// Require that `granted` scopes include `required`. Returns `Ok(())` on
/// success, `Err(message)` when the scope is missing. `admin` satisfies any
/// requirement.
pub(crate) fn require_scope(granted: &[String], required: &str) -> Result<(), String> {
    if scopes_include(granted, required) {
        Ok(())
    } else {
        Err(format!("missing required scope: {}", required))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_include_with_admin_wildcard() {
        let admin_scopes = vec![SCOPE_ADMIN.to_string()];
        assert!(scopes_include(&admin_scopes, SCOPE_RUNTIME_READ));
        assert!(scopes_include(&admin_scopes, SCOPE_PROJECT_WRITE));
        assert!(scopes_include(&admin_scopes, "anything"));
    }

    #[test]
    fn scopes_include_exact_match() {
        let scopes = vec![
            SCOPE_RUNTIME_READ.to_string(),
            SCOPE_PROJECT_READ.to_string(),
        ];
        assert!(scopes_include(&scopes, SCOPE_RUNTIME_READ));
        assert!(scopes_include(&scopes, SCOPE_PROJECT_READ));
        assert!(!scopes_include(&scopes, SCOPE_PROJECT_WRITE));
    }

    #[test]
    fn require_scope_ok_and_err() {
        let scopes = vec![SCOPE_JOB_RUN.to_string()];
        assert!(require_scope(&scopes, SCOPE_JOB_RUN).is_ok());
        assert!(require_scope(&scopes, SCOPE_ADMIN).is_err());
    }

    #[test]
    fn validate_scopes_rejects_unknown() {
        assert!(validate_scopes(&["runtime:read".to_string()]).is_ok());
        assert!(validate_scopes(&["bogus:scope".to_string()]).is_err());
    }

    #[test]
    fn validate_scopes_rejects_duplicates() {
        let result =
            validate_scopes(&["runtime:read".to_string(), "runtime:read".to_string()]).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn validate_agent_scopes_rejects_non_agent() {
        assert!(validate_agent_scopes(&["agent:register".to_string()]).is_ok());
        assert!(validate_agent_scopes(
            &["agent:register".to_string(), "runtime:read".to_string(),]
        )
        .is_err());
        assert!(validate_agent_scopes(&["admin".to_string()]).is_err());
    }

    #[test]
    fn scopes_to_string_round_trips() {
        let scopes = vec!["runtime:read".to_string(), "project:read".to_string()];
        let s = scopes_to_string(&scopes);
        assert_eq!(s, "runtime:read project:read");
    }
}
