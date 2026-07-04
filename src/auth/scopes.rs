//! Scope definitions and validation for the WebCodex auth system.
//!
//! Scopes are string-based permissions attached to tokens. Bootstrap auth is
//! treated as holding every scope. PAT (personal API token) and future OAuth2
//! tokens carry an explicit set of granted scopes.

use std::collections::HashSet;

use super::principal::AuthContext;
use crate::config::legacy_codex_run_enabled;
use crate::tool_runtime::metadata::lookup_tool_metadata;
use crate::tool_runtime::tool_definition::runtime_tool_oauth_scope;

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
    SCOPE_ACCOUNT_MANAGE,
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
// OAuth delegated scope policy
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OAuthRouteScopePolicy {
    Public,
    FirstPartyOnly,
    AgentSurface,
    Require(&'static str),
    BodyAware(OAuthBodyAwarePolicy),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OAuthBodyAwarePolicy {
    RuntimeToolCall,
    McpToolCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum OAuthToolScopePolicy {
    Require(&'static str),
    FirstPartyOnly,
    Unknown,
}

pub(crate) fn oauth_route_scope_policy_for_path_method(
    method: &str,
    path: &str,
) -> OAuthRouteScopePolicy {
    let method = method.trim().to_ascii_uppercase();
    let path = normalize_route_path(path);

    match (method.as_str(), path.as_str()) {
        (_, "/.well-known/oauth-protected-resource")
        | (_, "/.well-known/oauth-authorization-server")
        | (_, "/oauth/token")
        | (_, "/oauth/revoke")
        | ("POST", "/oauth/authorize/login")
        | ("POST", "/oauth/authorize/consent")
        | ("POST", "/oauth/authorize/bridge") => OAuthRouteScopePolicy::Public,
        // `/oauth/authorize` is NOT mounted behind `AuthMiddleware` (the
        // handler does its own Bearer PAT / session cookie validation), so
        // this `FirstPartyOnly` entry is audit/documentation only — it
        // records the intended identity boundary but is never enforced by the
        // middleware for this route.
        (_, "/oauth/authorize") => OAuthRouteScopePolicy::FirstPartyOnly,

        // First-party OAuth client management API. Only Bootstrap / ApiToken
        // may call these; OAuth2 access tokens are blocked even with
        // `account:manage`.
        ("POST", "/api/oauth/clients/create")
        | ("POST", "/api/oauth/clients/list")
        | ("POST", "/api/oauth/clients/revoke") => OAuthRouteScopePolicy::FirstPartyOnly,

        ("GET", "/mcp") => OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ),
        ("POST", "/mcp") => OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::McpToolCall),
        ("POST", "/api/runtime/status") | ("POST", "/api/tools/list") => {
            OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ)
        }
        ("POST", "/api/tools/call") => {
            OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::RuntimeToolCall)
        }
        ("POST", "/api/codex/run") if legacy_codex_run_enabled() => {
            OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN)
        }
        ("POST", "/api/artifacts/import") => OAuthRouteScopePolicy::Require(SCOPE_PROJECT_WRITE),

        ("POST", "/api/jobs/status")
        | ("POST", "/api/jobs/log")
        | ("POST", "/api/jobs/list")
        | ("POST", "/api/jobs/tail")
        | ("POST", "/api/shell/jobs/status")
        | ("POST", "/api/shell/jobs/log")
        | ("POST", "/api/shell/jobs/list") => OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ),
        ("POST", "/api/jobs/stop") | ("POST", "/api/shell/jobs/stop") => {
            OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN)
        }

        ("POST", "/api/projects/list")
        | ("POST", "/api/projects/read_file")
        | ("POST", "/api/projects/git_status")
        | ("POST", "/api/projects/git_diff")
        | ("POST", "/api/projects/git_diff_summary")
        | ("POST", "/api/projects/list_files")
        | ("POST", "/api/projects/search_text")
        | ("POST", "/api/projects/validate_patch") => {
            OAuthRouteScopePolicy::Require(SCOPE_PROJECT_READ)
        }
        ("POST", "/api/projects/register")
        | ("POST", "/api/projects/create")
        | ("POST", "/api/projects/apply_patch")
        | ("POST", "/api/projects/apply_patch_checked")
        | ("POST", "/api/projects/delete_files")
        | ("POST", "/api/projects/git_restore_paths")
        | ("POST", "/api/projects/discard_untracked")
        | ("POST", "/api/projects/replace_in_file")
        | ("POST", "/api/projects/write_file")
        | ("POST", "/api/shell/file") => OAuthRouteScopePolicy::Require(SCOPE_PROJECT_WRITE),
        ("POST", "/api/projects/run_shell")
        | ("POST", "/api/projects/run_job")
        | ("POST", "/api/shell/run")
        | ("POST", "/api/shell/job") => OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN),

        ("POST", "/api/codex/context")
        | ("POST", "/api/codex/projects")
        | ("POST", "/api/codex/context_batch")
        | ("POST", "/api/codex/report") => OAuthRouteScopePolicy::Require(SCOPE_PROJECT_READ),
        ("POST", "/api/codex/apply_patch")
        | ("POST", "/api/codex/edit")
        | ("POST", "/api/codex/artifact")
        | ("POST", "/api/codex/git") => OAuthRouteScopePolicy::Require(SCOPE_PROJECT_WRITE),
        ("POST", "/api/codex/job") => OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN),

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
        | ("POST", "/api/audit/stats") => OAuthRouteScopePolicy::Require(SCOPE_ACCOUNT_MANAGE),

        ("POST", "/api/pairing/enroll")
        | ("POST", "/api/shell/agent/register")
        | ("POST", "/api/shell/agent/poll")
        | ("POST", "/api/shell/agent/result")
        | ("POST", "/api/shell/agent/job_update")
        | ("GET", "/api/agents/ws") => OAuthRouteScopePolicy::AgentSurface,
        _ => OAuthRouteScopePolicy::Unknown,
    }
}

pub(crate) fn oauth_scope_policy_for_runtime_tool(tool_name: &str) -> OAuthToolScopePolicy {
    runtime_tool_oauth_scope(tool_name)
        .or_else(|| lookup_tool_metadata(tool_name).and_then(|metadata| metadata.oauth_scope))
        .map(OAuthToolScopePolicy::Require)
        .unwrap_or(OAuthToolScopePolicy::Unknown)
}

#[allow(dead_code)]
pub(crate) fn required_oauth_scope_for_path_method(
    method: &str,
    path: &str,
) -> Option<&'static str> {
    match oauth_route_scope_policy_for_path_method(method, path) {
        OAuthRouteScopePolicy::Require(scope) => Some(scope),
        _ => None,
    }
}

pub(crate) fn enforce_oauth_route_scope(
    ctx: &AuthContext,
    method: &str,
    path: &str,
) -> Result<(), (Option<&'static str>, String)> {
    if !ctx.is_oauth_token() {
        return Ok(());
    }

    match oauth_route_scope_policy_for_path_method(method, path) {
        OAuthRouteScopePolicy::Public | OAuthRouteScopePolicy::BodyAware(_) => Ok(()),
        OAuthRouteScopePolicy::Require(scope) => {
            if ctx.has_scope(scope) {
                Ok(())
            } else {
                Err((Some(scope), format!("missing required scope: {}", scope)))
            }
        }
        OAuthRouteScopePolicy::FirstPartyOnly => Err((
            None,
            "OAuth2 access tokens cannot call first-party-only routes".to_string(),
        )),
        OAuthRouteScopePolicy::AgentSurface => Err((
            None,
            "OAuth2 access tokens cannot call agent transport routes".to_string(),
        )),
        OAuthRouteScopePolicy::Unknown => Err((
            None,
            "OAuth2 access tokens cannot call unknown authenticated routes".to_string(),
        )),
    }
}

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
    use crate::tool_runtime::metadata::lookup_tool_metadata;
    use crate::tool_runtime::KNOWN_TOOL_NAMES;

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
    fn oauth_route_policy_public_endpoints() {
        for (method, path) in [
            ("GET", "/.well-known/oauth-protected-resource"),
            ("GET", "/.well-known/oauth-authorization-server"),
            ("POST", "/oauth/token"),
            ("POST", "/oauth/revoke"),
            ("POST", "/oauth/authorize/login"),
            ("POST", "/oauth/authorize/consent"),
            ("POST", "/oauth/authorize/bridge"),
        ] {
            assert_eq!(
                oauth_route_scope_policy_for_path_method(method, path),
                OAuthRouteScopePolicy::Public,
                "{method} {path}"
            );
            assert_eq!(required_oauth_scope_for_path_method(method, path), None);
        }
    }

    #[test]
    fn oauth_route_policy_authorize_is_first_party_only() {
        assert_eq!(
            oauth_route_scope_policy_for_path_method("GET", "/oauth/authorize"),
            OAuthRouteScopePolicy::FirstPartyOnly
        );
        assert_eq!(
            required_oauth_scope_for_path_method("GET", "/oauth/authorize"),
            None
        );
    }

    #[test]
    fn oauth_route_policy_oauth_client_management_is_first_party_only() {
        for path in [
            "/api/oauth/clients/create",
            "/api/oauth/clients/list",
            "/api/oauth/clients/revoke",
        ] {
            assert_eq!(
                oauth_route_scope_policy_for_path_method("POST", path),
                OAuthRouteScopePolicy::FirstPartyOnly,
                "POST {path}"
            );
            assert_eq!(required_oauth_scope_for_path_method("POST", path), None);
        }
    }

    #[test]
    fn oauth_route_policy_agent_surfaces() {
        for (method, path) in [
            ("POST", "/api/pairing/enroll"),
            ("POST", "/api/shell/agent/register"),
            ("POST", "/api/shell/agent/poll"),
            ("POST", "/api/shell/agent/result"),
            ("POST", "/api/shell/agent/job_update"),
            ("GET", "/api/agents/ws"),
        ] {
            assert_eq!(
                oauth_route_scope_policy_for_path_method(method, path),
                OAuthRouteScopePolicy::AgentSurface,
                "{method} {path}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_simple_require_scopes() {
        for (method, path, scope) in [
            ("GET", "/mcp", SCOPE_RUNTIME_READ),
            ("POST", "/api/runtime/status", SCOPE_RUNTIME_READ),
            ("POST", "/api/tools/list", SCOPE_RUNTIME_READ),
            ("POST", "/api/projects/read_file", SCOPE_PROJECT_READ),
            ("POST", "/api/projects/write_file", SCOPE_PROJECT_WRITE),
            ("POST", "/api/projects/run_job", SCOPE_JOB_RUN),
            ("POST", "/api/users/me", SCOPE_ACCOUNT_MANAGE),
            ("POST", "/api/tokens/list", SCOPE_ACCOUNT_MANAGE),
            ("POST", "/api/audit/stats", SCOPE_ACCOUNT_MANAGE),
        ] {
            assert_eq!(
                oauth_route_scope_policy_for_path_method(method, path),
                OAuthRouteScopePolicy::Require(scope),
                "{method} {path}"
            );
            assert_eq!(
                required_oauth_scope_for_path_method(method, path),
                Some(scope),
                "{method} {path}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_body_aware_routes() {
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/api/tools/call"),
            OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::RuntimeToolCall)
        );
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/mcp"),
            OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::McpToolCall)
        );
    }

    #[test]
    fn oauth_route_policy_unknown_is_unknown() {
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/api/future/authenticated-route"),
            OAuthRouteScopePolicy::Unknown
        );
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/api/tools/list/extra"),
            OAuthRouteScopePolicy::Unknown
        );
    }

    #[test]
    fn oauth_route_policy_legacy_codex_run_is_flag_gated() {
        let _guard = crate::admin_cli::TEST_ENV_LOCK.lock().unwrap();
        std::env::remove_var("WEBCODEX_ENABLE_LEGACY_CODEX_RUN");
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/api/codex/run"),
            OAuthRouteScopePolicy::Unknown
        );

        std::env::set_var("WEBCODEX_ENABLE_LEGACY_CODEX_RUN", "1");
        assert_eq!(
            oauth_route_scope_policy_for_path_method("POST", "/api/codex/run"),
            OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN)
        );
        std::env::remove_var("WEBCODEX_ENABLE_LEGACY_CODEX_RUN");
    }

    #[test]
    fn oauth_route_policy_authenticated_route_audit() {
        for (method, path) in [
            ("POST", "/api/tools/list"),
            ("POST", "/api/tools/call"),
            ("POST", "/api/artifacts/import"),
            ("POST", "/api/jobs/status"),
            ("POST", "/api/jobs/log"),
            ("POST", "/api/jobs/stop"),
            ("POST", "/api/jobs/list"),
            ("POST", "/api/jobs/tail"),
            ("POST", "/api/projects/list"),
            ("POST", "/api/projects/register"),
            ("POST", "/api/projects/create"),
            ("POST", "/api/projects/read_file"),
            ("POST", "/api/projects/git_status"),
            ("POST", "/api/projects/git_diff"),
            ("POST", "/api/projects/git_diff_summary"),
            ("POST", "/api/projects/list_files"),
            ("POST", "/api/projects/search_text"),
            ("POST", "/api/projects/apply_patch"),
            ("POST", "/api/projects/validate_patch"),
            ("POST", "/api/projects/run_shell"),
            ("POST", "/api/projects/apply_patch_checked"),
            ("POST", "/api/projects/delete_files"),
            ("POST", "/api/projects/git_restore_paths"),
            ("POST", "/api/projects/discard_untracked"),
            ("POST", "/api/projects/replace_in_file"),
            ("POST", "/api/projects/write_file"),
            ("POST", "/api/projects/run_job"),
            ("POST", "/api/runtime/status"),
            ("POST", "/api/users/create"),
            ("POST", "/api/users/list"),
            ("POST", "/api/users/me"),
            ("POST", "/api/tokens/create"),
            ("POST", "/api/tokens/register_hash"),
            ("POST", "/api/tokens/list"),
            ("POST", "/api/tokens/revoke"),
            ("POST", "/api/agent-tokens/create"),
            ("POST", "/api/agent-tokens/register_hash"),
            ("POST", "/api/agent-tokens/list"),
            ("POST", "/api/agent-tokens/revoke"),
            ("POST", "/api/shell/run"),
            ("POST", "/api/shell/file"),
            ("POST", "/api/shell/job"),
            ("POST", "/api/shell/jobs/status"),
            ("POST", "/api/shell/jobs/log"),
            ("POST", "/api/shell/jobs/stop"),
            ("POST", "/api/shell/jobs/list"),
            ("POST", "/api/shell/agent/register"),
            ("POST", "/api/shell/agent/poll"),
            ("POST", "/api/shell/agent/result"),
            ("POST", "/api/shell/agent/job_update"),
            ("GET", "/api/agents/ws"),
            ("POST", "/api/pairing/enroll"),
            ("POST", "/api/pairing/create"),
            ("POST", "/api/codex/context"),
            ("POST", "/api/codex/projects"),
            ("POST", "/api/codex/context_batch"),
            ("POST", "/api/codex/apply_patch"),
            ("POST", "/api/codex/edit"),
            ("POST", "/api/codex/artifact"),
            ("POST", "/api/codex/git"),
            ("POST", "/api/codex/job"),
            ("POST", "/api/codex/report"),
            ("POST", "/api/audit/sessions"),
            ("POST", "/api/audit/session"),
            ("POST", "/api/audit/stats"),
            ("GET", "/mcp"),
            ("POST", "/mcp"),
            ("GET", "/oauth/authorize"),
            ("POST", "/oauth/authorize/login"),
            ("POST", "/oauth/authorize/consent"),
            ("POST", "/api/oauth/clients/create"),
            ("POST", "/api/oauth/clients/list"),
            ("POST", "/api/oauth/clients/revoke"),
        ] {
            assert_ne!(
                oauth_route_scope_policy_for_path_method(method, path),
                OAuthRouteScopePolicy::Unknown,
                "{method} {path}"
            );
        }
    }

    #[test]
    fn oauth_scope_policy_runtime_tool_scopes() {
        for (tool, policy) in [
            (
                "list_tools",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "start_session",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "session_summary",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "post_session_message",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "list_session_messages",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "resolve_session_message",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "session_discussion_summary",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "bind_current_session",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "current_session",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "unbind_current_session",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "runtime_status",
                OAuthToolScopePolicy::Require(SCOPE_RUNTIME_READ),
            ),
            (
                "read_file",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "show_changes",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "workspace_checkpoint_create",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "workspace_checkpoint_restore",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            ("git_log", OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ)),
            (
                "write_project_file",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            (
                "artifact_upload_begin",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            (
                "artifact_upload_chunk",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            (
                "artifact_upload_finish",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            (
                "artifact_upload_abort",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            (
                "replace_line_range",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE),
            ),
            ("run_shell", OAuthToolScopePolicy::Require(SCOPE_JOB_RUN)),
            ("stop_job", OAuthToolScopePolicy::Require(SCOPE_JOB_RUN)),
            ("cargo_test", OAuthToolScopePolicy::Require(SCOPE_JOB_RUN)),
        ] {
            assert_eq!(oauth_scope_policy_for_runtime_tool(tool), policy, "{tool}");
        }
    }

    #[test]
    fn oauth_route_policy_tool_scope_policy_matches_metadata_for_representative_tools() {
        for tool in [
            "list_tools",
            "start_session",
            "session_summary",
            "post_session_message",
            "list_session_messages",
            "resolve_session_message",
            "session_discussion_summary",
            "bind_current_session",
            "current_session",
            "unbind_current_session",
            "workspace_checkpoint_create",
            "workspace_checkpoint_restore",
            "show_changes",
            "read_file",
            "write_project_file",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
            "apply_patch_checked",
            "run_shell",
            "cargo_test",
        ] {
            let metadata = lookup_tool_metadata(tool).unwrap();
            assert_eq!(
                oauth_scope_policy_for_runtime_tool(tool),
                OAuthToolScopePolicy::Require(metadata.oauth_scope.unwrap()),
                "{tool}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_tool_scope_policy_covers_metadata_for_known_tools() {
        for tool in KNOWN_TOOL_NAMES {
            let metadata = lookup_tool_metadata(tool).unwrap();
            assert_eq!(
                oauth_scope_policy_for_runtime_tool(tool),
                OAuthToolScopePolicy::Require(metadata.oauth_scope.unwrap()),
                "{tool}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_preserves_legacy_non_runtime_metadata_scope() {
        assert!(!KNOWN_TOOL_NAMES.contains(&"delete_files"));
        assert_eq!(
            oauth_scope_policy_for_runtime_tool("delete_files"),
            OAuthToolScopePolicy::Require(SCOPE_PROJECT_WRITE)
        );
    }

    #[test]
    fn oauth_scope_policy_unknown_tool_is_unknown() {
        assert_eq!(
            oauth_scope_policy_for_runtime_tool("definitely_not_a_tool"),
            OAuthToolScopePolicy::Unknown
        );
    }
}
