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
pub(crate) const KNOWN_SCOPES: &[&str] = &[
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
#[allow(dead_code)] // Utility kept for handler migration to Principal
pub(crate) fn scopes_include(granted: &[String], required: &str) -> bool {
    granted.iter().any(|s| s == required || s == SCOPE_ADMIN)
}

/// Require that `granted` scopes include `required`. Returns `Ok(())` on
/// success, `Err(message)` when the scope is missing. `admin` satisfies any
/// requirement.
#[allow(dead_code)] // Utility kept for handler migration to Principal
pub(crate) fn require_scope(granted: &[String], required: &str) -> Result<(), String> {
    if scopes_include(granted, required) {
        Ok(())
    } else {
        Err(format!("missing required scope: {}", required))
    }
}

// ---------------------------------------------------------------------------
// OAuth route scope policy (Phase 2f-0: definition only)
// ---------------------------------------------------------------------------

/// Return the delegated OAuth scope that a regular HTTP route should require
/// when Phase 2f-1 wires route-level enforcement into `AuthMiddleware` or a
/// route guard.
///
/// This helper is intentionally **not** called by authentication middleware in
/// Phase 2f-0, so it does not reject requests or change runtime behavior.
/// Future enforcement must apply this policy only when the authenticated
/// principal is `AuthKind::OAuth2Token`. First-party WebCodex credentials
/// (`Bootstrap`, `ApiToken`) must not be restricted by delegated OAuth scopes;
/// agent transport credentials (`AgentToken`, `AccountCredential`) remain
/// governed by their existing surface gates.
///
/// `None` means either a public OAuth endpoint / first-party authorization
/// endpoint, an agent-only surface that is not OAuth-delegable, or an unknown
/// route. Phase 2f-1 must audit all authenticated routes before using `None` as
/// an enforcement bypass.
#[allow(dead_code)] // Phase 2f-0 policy definition; wired in Phase 2f-1.
pub(crate) fn required_oauth_scope_for_path_method(
    method: &str,
    path: &str,
) -> Option<&'static str> {
    let method = method.trim().to_ascii_uppercase();
    let path = normalize_route_path(path);

    match (method.as_str(), path.as_str()) {
        (_, "/.well-known/oauth-protected-resource")
        | (_, "/.well-known/oauth-authorization-server")
        | (_, "/oauth/token")
        | (_, "/oauth/revoke") => None,
        (_, "/oauth/authorize") => None,

        ("GET", "/mcp") => Some(SCOPE_RUNTIME_READ),
        ("POST", "/mcp") => Some(SCOPE_JOB_RUN),
        ("POST", "/api/runtime/status") | ("POST", "/api/tools/list") => Some(SCOPE_RUNTIME_READ),
        ("POST", "/api/tools/call") | ("POST", "/api/codex/run") => Some(SCOPE_JOB_RUN),
        ("POST", "/api/artifacts/import") => Some(SCOPE_PROJECT_WRITE),

        ("POST", "/api/jobs/status")
        | ("POST", "/api/jobs/log")
        | ("POST", "/api/jobs/list")
        | ("POST", "/api/jobs/tail")
        | ("POST", "/api/shell/jobs/status")
        | ("POST", "/api/shell/jobs/log")
        | ("POST", "/api/shell/jobs/list") => Some(SCOPE_RUNTIME_READ),
        ("POST", "/api/jobs/stop") | ("POST", "/api/shell/jobs/stop") => Some(SCOPE_JOB_RUN),

        ("POST", "/api/projects/list")
        | ("POST", "/api/projects/read_file")
        | ("POST", "/api/projects/git_status")
        | ("POST", "/api/projects/git_diff")
        | ("POST", "/api/projects/git_diff_summary")
        | ("POST", "/api/projects/list_files")
        | ("POST", "/api/projects/search_text")
        | ("POST", "/api/projects/validate_patch") => Some(SCOPE_PROJECT_READ),
        ("POST", "/api/projects/register")
        | ("POST", "/api/projects/create")
        | ("POST", "/api/projects/apply_patch")
        | ("POST", "/api/projects/apply_patch_checked")
        | ("POST", "/api/projects/delete_files")
        | ("POST", "/api/projects/git_restore_paths")
        | ("POST", "/api/projects/discard_untracked")
        | ("POST", "/api/projects/replace_in_file")
        | ("POST", "/api/projects/write_file")
        | ("POST", "/api/shell/file") => Some(SCOPE_PROJECT_WRITE),
        ("POST", "/api/projects/run_shell")
        | ("POST", "/api/projects/run_job")
        | ("POST", "/api/shell/run")
        | ("POST", "/api/shell/job") => Some(SCOPE_JOB_RUN),

        ("POST", "/api/codex/context")
        | ("POST", "/api/codex/projects")
        | ("POST", "/api/codex/context_batch")
        | ("POST", "/api/codex/report") => Some(SCOPE_PROJECT_READ),
        ("POST", "/api/codex/apply_patch")
        | ("POST", "/api/codex/edit")
        | ("POST", "/api/codex/artifact")
        | ("POST", "/api/codex/git") => Some(SCOPE_PROJECT_WRITE),
        ("POST", "/api/codex/job") => Some(SCOPE_JOB_RUN),

        ("POST", "/api/users/create")
        | ("POST", "/api/users/list")
        | ("POST", "/api/users/me")
        | ("POST", "/api/tokens/create")
        | ("POST", "/api/tokens/register_hash")
        | ("POST", "/api/tokens/list")
        | ("POST", "/api/tokens/revoke")
        | ("POST", "/api/agent-tokens/create")
        | ("POST", "/api/agent-tokens/register_hash")
        | ("POST", "/api/agent-tokens/list")
        | ("POST", "/api/agent-tokens/revoke")
        | ("POST", "/api/pairing/create")
        | ("POST", "/api/audit/sessions")
        | ("POST", "/api/audit/session")
        | ("POST", "/api/audit/stats") => Some(SCOPE_ACCOUNT_MANAGE),

        ("POST", "/api/pairing/enroll")
        | ("POST", "/api/shell/agent/register")
        | ("POST", "/api/shell/agent/poll")
        | ("POST", "/api/shell/agent/result")
        | ("POST", "/api/shell/agent/job_update")
        | ("GET", "/api/agents/ws") => None,
        _ => None,
    }
}

#[allow(dead_code)] // Used by the Phase 2f-0 helper above.
fn normalize_route_path(path: &str) -> String {
    let path = path.trim();
    let path = path.split('?').next().unwrap_or(path);
    let path = if path.is_empty() { "/" } else { path };
    let with_slash = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if with_slash.len() > 1 {
        with_slash.trim_end_matches('/').to_string()
    } else {
        with_slash
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
    #[test]
    fn oauth_scope_policy_public_oauth_metadata_has_no_required_scope() {
        assert_eq!(
            required_oauth_scope_for_path_method("GET", "/.well-known/oauth-protected-resource"),
            None
        );
        assert_eq!(
            required_oauth_scope_for_path_method("GET", "/.well-known/oauth-authorization-server"),
            None
        );
    }

    #[test]
    fn oauth_scope_policy_token_endpoint_has_no_required_scope() {
        assert_eq!(
            required_oauth_scope_for_path_method("POST", "/oauth/token"),
            None
        );
    }

    #[test]
    fn oauth_scope_policy_revoke_endpoint_has_no_required_scope() {
        assert_eq!(
            required_oauth_scope_for_path_method("POST", "/oauth/revoke"),
            None
        );
    }

    #[test]
    fn oauth_scope_policy_authorize_endpoint_has_no_required_scope() {
        assert_eq!(
            required_oauth_scope_for_path_method("GET", "/oauth/authorize"),
            None
        );
    }

    #[test]
    fn oauth_scope_policy_runtime_read_routes_require_runtime_read() {
        for (method, path) in [
            ("POST", "/api/runtime/status"),
            ("POST", "/api/tools/list"),
            ("POST", "/api/jobs/status"),
            ("POST", "/api/jobs/log"),
            ("POST", "/api/jobs/list"),
            ("POST", "/api/jobs/tail"),
            ("GET", "/mcp"),
        ] {
            assert_eq!(
                required_oauth_scope_for_path_method(method, path),
                Some(SCOPE_RUNTIME_READ),
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_project_read_routes_require_project_read() {
        for path in [
            "/api/projects/list",
            "/api/projects/read_file",
            "/api/projects/git_status",
            "/api/projects/git_diff",
            "/api/projects/git_diff_summary",
            "/api/projects/list_files",
            "/api/projects/search_text",
            "/api/projects/validate_patch",
            "/api/codex/context",
            "/api/codex/projects",
        ] {
            assert_eq!(
                required_oauth_scope_for_path_method("POST", path),
                Some(SCOPE_PROJECT_READ),
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_project_write_routes_require_project_write() {
        for path in [
            "/api/artifacts/import",
            "/api/projects/register",
            "/api/projects/create",
            "/api/projects/apply_patch",
            "/api/projects/apply_patch_checked",
            "/api/projects/delete_files",
            "/api/projects/git_restore_paths",
            "/api/projects/discard_untracked",
            "/api/projects/replace_in_file",
            "/api/projects/write_file",
            "/api/shell/file",
            "/api/codex/apply_patch",
            "/api/codex/edit",
            "/api/codex/artifact",
            "/api/codex/git",
        ] {
            assert_eq!(
                required_oauth_scope_for_path_method("POST", path),
                Some(SCOPE_PROJECT_WRITE),
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_job_routes_require_job_run() {
        for (method, path) in [
            ("POST", "/api/tools/call"),
            ("POST", "/api/codex/run"),
            ("POST", "/api/projects/run_shell"),
            ("POST", "/api/projects/run_job"),
            ("POST", "/api/jobs/stop"),
            ("POST", "/api/shell/run"),
            ("POST", "/api/shell/job"),
            ("POST", "/api/shell/jobs/stop"),
            ("POST", "/api/codex/job"),
            ("POST", "/mcp"),
        ] {
            assert_eq!(
                required_oauth_scope_for_path_method(method, path),
                Some(SCOPE_JOB_RUN),
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_account_routes_require_account_manage() {
        for path in [
            "/api/users/create",
            "/api/users/list",
            "/api/users/me",
            "/api/tokens/create",
            "/api/tokens/register_hash",
            "/api/tokens/list",
            "/api/tokens/revoke",
            "/api/agent-tokens/create",
            "/api/agent-tokens/register_hash",
            "/api/agent-tokens/list",
            "/api/agent-tokens/revoke",
            "/api/pairing/create",
            "/api/audit/sessions",
        ] {
            assert_eq!(
                required_oauth_scope_for_path_method("POST", path),
                Some(SCOPE_ACCOUNT_MANAGE),
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_unknown_authenticated_route_is_conservative() {
        assert_eq!(
            required_oauth_scope_for_path_method("POST", "/api/future/authenticated-route"),
            None
        );
        assert_eq!(
            required_oauth_scope_for_path_method("POST", "/api/tools/list/extra"),
            None
        );
    }
}
