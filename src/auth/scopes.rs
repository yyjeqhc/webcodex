//! Scope definitions and validation for the WebCodex auth system.
//!
//! Scopes are string-based permissions carried by authenticated principals.
//! Bootstrap auth is treated as holding every scope; managed tokens, delegated
//! OAuth access tokens, and lightweight contexts carry explicit granted scopes.

use std::collections::HashSet;

use super::context::AuthContext;
use crate::tool_runtime::metadata::lookup_tool_metadata;

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
pub const SCOPE_COMPUTER_READ: &str = "computer:read";
pub const SCOPE_COMPUTER_CONTROL: &str = "computer:control";
pub const SCOPE_COMPUTER_LAUNCH: &str = "computer:launch";
pub const SCOPE_COMPUTER_DISPLAY_READ: &str = "computer:display_read";
pub const SCOPE_COMPUTER_POINTER_CONTROL: &str = "computer:pointer_control";
pub const SCOPE_COMPUTER_CLIPBOARD_READ: &str = "computer:clipboard_read";
pub const SCOPE_COMPUTER_CLIPBOARD_WRITE: &str = "computer:clipboard_write";
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
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
    SCOPE_RUNTIME_READ,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_JOB_RUN,
    SCOPE_COMPUTER_READ,
    SCOPE_COMPUTER_CONTROL,
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
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

// ---------------------------------------------------------------------------
// Authenticated route/tool scope policy
// ---------------------------------------------------------------------------
// The OAuth-prefixed type/function names below are retained for compatibility
// with the existing policy registry. Enforcement is principal-neutral; OAuth
// remains special only for delegated-scope issuance and wire error framing.

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
pub(crate) enum OAuthToolScopePolicy {
    Require(&'static str),
    RequireAll(&'static [&'static str]),
    /// Reserved fail-closed policy for a runtime tool that may be exposed only
    /// to first-party credentials; no current tool definition selects it.
    #[allow(dead_code)]
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
        | ("POST", "/api/oauth/clients/update_scopes")
        | ("POST", "/api/oauth/clients/revoke") => OAuthRouteScopePolicy::FirstPartyOnly,

        ("GET", "/mcp") => OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ),
        ("POST", "/mcp") => OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::McpToolCall),
        ("POST", "/api/runtime/status") | ("POST", "/api/tools/list") => {
            OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ)
        }
        ("POST", "/api/connector/task/start") => OAuthRouteScopePolicy::Require(SCOPE_RUNTIME_READ),
        ("POST", "/api/connector/files/read")
        | ("POST", "/api/connector/files/search")
        | ("POST", "/api/connector/code/navigate")
        | ("POST", "/api/connector/code/impact")
        | ("POST", "/api/connector/task/review") => {
            OAuthRouteScopePolicy::Require(SCOPE_PROJECT_READ)
        }
        ("POST", "/api/connector/edits/apply") | ("POST", "/api/connector/task/finish") => {
            OAuthRouteScopePolicy::Require(SCOPE_PROJECT_WRITE)
        }
        ("POST", "/api/connector/checks/run")
        | ("POST", "/api/connector/commands/run")
        | ("POST", "/api/connector/task/cancel") => OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN),
        ("POST", "/api/tools/call") => {
            OAuthRouteScopePolicy::BodyAware(OAuthBodyAwarePolicy::RuntimeToolCall)
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

        ("POST", "/api/runtime-console/projects")
        | ("POST", "/api/runtime-console/workflow-sessions")
        | ("POST", "/api/runtime-console/workflow-session")
        | ("POST", "/api/projects/list")
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
        | ("POST", "/api/projects/unregister")
        | ("POST", "/api/projects/apply_patch")
        | ("POST", "/api/projects/apply_patch_checked")
        | ("POST", "/api/projects/delete_files")
        | ("POST", "/api/projects/git_restore_paths")
        | ("POST", "/api/projects/discard_untracked")
        | ("POST", "/api/shell/file") => OAuthRouteScopePolicy::Require(SCOPE_PROJECT_WRITE),
        ("POST", "/api/projects/run_shell")
        | ("POST", "/api/projects/run_job")
        | ("POST", "/api/shell/run")
        | ("POST", "/api/shell/job") => OAuthRouteScopePolicy::Require(SCOPE_JOB_RUN),

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
        | ("POST", "/api/shell/agent/persistent_shell_result")
        | ("POST", "/api/shell/agent/job_update")
        | ("GET", "/api/agents/ws") => OAuthRouteScopePolicy::AgentSurface,
        _ => OAuthRouteScopePolicy::Unknown,
    }
}

pub(crate) fn oauth_scope_policy_for_runtime_tool(tool_name: &str) -> OAuthToolScopePolicy {
    if tool_name == "computer_read_clipboard" {
        return OAuthToolScopePolicy::RequireAll(&[
            SCOPE_COMPUTER_READ,
            SCOPE_COMPUTER_CLIPBOARD_READ,
        ]);
    }
    if tool_name == "computer_write_clipboard" {
        return OAuthToolScopePolicy::RequireAll(&[
            SCOPE_COMPUTER_CONTROL,
            SCOPE_COMPUTER_CLIPBOARD_WRITE,
        ]);
    }
    if matches!(
        tool_name,
        "computer_pointer_move" | "computer_pointer_click"
    ) {
        return OAuthToolScopePolicy::RequireAll(&[
            SCOPE_COMPUTER_READ,
            SCOPE_COMPUTER_DISPLAY_READ,
            SCOPE_COMPUTER_CONTROL,
            SCOPE_COMPUTER_POINTER_CONTROL,
        ]);
    }
    if matches!(
        tool_name,
        "computer_list_displays" | "computer_snapshot_display"
    ) {
        return OAuthToolScopePolicy::RequireAll(&[
            SCOPE_COMPUTER_READ,
            SCOPE_COMPUTER_DISPLAY_READ,
        ]);
    }
    if tool_name == "computer_save_snapshot" {
        return OAuthToolScopePolicy::RequireAll(&[SCOPE_PROJECT_WRITE, SCOPE_COMPUTER_READ]);
    }
    lookup_tool_metadata(tool_name)
        .and_then(|metadata| metadata.oauth_scope)
        .map(OAuthToolScopePolicy::Require)
        .unwrap_or(OAuthToolScopePolicy::Unknown)
}

#[cfg(test)]
pub(crate) fn required_oauth_scope_for_path_method(
    method: &str,
    path: &str,
) -> Option<&'static str> {
    match oauth_route_scope_policy_for_path_method(method, path) {
        OAuthRouteScopePolicy::Require(scope) => Some(scope),
        _ => None,
    }
}

fn pat_account_manage_compatibility_route(method: &str, path: &str) -> bool {
    matches!(
        (method, path),
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
    )
}

pub(crate) fn enforce_route_scope(
    ctx: &AuthContext,
    method: &str,
    path: &str,
) -> Result<(), (Option<&'static str>, String)> {
    match oauth_route_scope_policy_for_path_method(method, path) {
        OAuthRouteScopePolicy::Public | OAuthRouteScopePolicy::BodyAware(_) => Ok(()),
        OAuthRouteScopePolicy::Require(scope) => {
            // A narrow set of account-management routes predates delegated
            // scopes and already enforces admin/self or admin-only identity in
            // its handler. Preserve PAT compatibility only for those exact
            // routes; other account:manage surfaces (notably global audit
            // reads) must carry the scope explicitly.
            let first_party_pat_account_route = scope == SCOPE_ACCOUNT_MANAGE
                && matches!(ctx.kind, super::context::AuthKind::ApiToken)
                && pat_account_manage_compatibility_route(method, path);
            if first_party_pat_account_route || ctx.has_scope(scope) {
                Ok(())
            } else {
                Err((Some(scope), format!("missing required scope: {}", scope)))
            }
        }
        OAuthRouteScopePolicy::FirstPartyOnly => {
            if matches!(
                ctx.kind,
                super::context::AuthKind::Bootstrap | super::context::AuthKind::ApiToken
            ) {
                Ok(())
            } else {
                Err((
                    None,
                    "route requires a first-party bootstrap or personal API token".to_string(),
                ))
            }
        }
        // Agent transport identity and exact agent:* scopes are enforced by the
        // dedicated surface gate and transport handlers. Do not reinterpret
        // those credentials as ordinary runtime principals here.
        OAuthRouteScopePolicy::AgentSurface => Ok(()),
        OAuthRouteScopePolicy::Unknown => {
            if ctx.is_bootstrap() {
                Ok(())
            } else {
                Err((
                    None,
                    "authenticated route has no declared scope policy".to_string(),
                ))
            }
        }
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
    use crate::tool_runtime::tool_definition::{is_known_tool_name, known_tool_names};

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
            "/api/oauth/clients/update_scopes",
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
            ("POST", "/api/shell/agent/persistent_shell_result"),
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
            ("POST", "/api/connector/task/start", SCOPE_RUNTIME_READ),
            ("POST", "/api/connector/files/read", SCOPE_PROJECT_READ),
            ("POST", "/api/connector/code/navigate", SCOPE_PROJECT_READ),
            ("POST", "/api/connector/code/impact", SCOPE_PROJECT_READ),
            ("POST", "/api/connector/edits/apply", SCOPE_PROJECT_WRITE),
            ("POST", "/api/connector/checks/run", SCOPE_JOB_RUN),
            ("POST", "/api/connector/task/cancel", SCOPE_JOB_RUN),
            ("POST", "/api/connector/task/finish", SCOPE_PROJECT_WRITE),
            ("POST", "/api/projects/read_file", SCOPE_PROJECT_READ),
            ("POST", "/api/runtime-console/projects", SCOPE_PROJECT_READ),
            (
                "POST",
                "/api/runtime-console/workflow-sessions",
                SCOPE_PROJECT_READ,
            ),
            (
                "POST",
                "/api/runtime-console/workflow-session",
                SCOPE_PROJECT_READ,
            ),
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
    fn route_scope_enforcement_is_principal_neutral() {
        let mut pat = AuthContext::new(super::super::context::AuthKind::ApiToken);
        pat.scopes = vec![SCOPE_RUNTIME_READ.to_string()];
        let mut oauth = AuthContext::new(super::super::context::AuthKind::OAuth2Token);
        oauth.scopes = vec![SCOPE_RUNTIME_READ.to_string()];
        let shared = crate::auth::shared_key_context("scope-matrix");
        let bootstrap = crate::auth::bootstrap_context();

        for (label, auth) in [("pat", &pat), ("oauth", &oauth), ("shared", &shared)] {
            assert!(
                enforce_route_scope(auth, "POST", "/api/runtime/status").is_ok(),
                "{label} should honor runtime:read"
            );
        }
        for (label, auth) in [("pat", &pat), ("oauth", &oauth)] {
            assert_eq!(
                enforce_route_scope(auth, "POST", "/api/projects/read_file"),
                Err((
                    Some(SCOPE_PROJECT_READ),
                    "missing required scope: project:read".to_string()
                )),
                "{label} must not bypass missing project:read"
            );
        }
        for (label, auth) in [("pat", &pat), ("oauth", &oauth)] {
            assert_eq!(
                enforce_route_scope(auth, "POST", "/api/runtime-console/projects"),
                Err((
                    Some(SCOPE_PROJECT_READ),
                    "missing required scope: project:read".to_string()
                )),
                "{label} must not use Runtime Console without project:read"
            );
        }
        assert!(
            enforce_route_scope(&shared, "POST", "/api/projects/read_file").is_ok(),
            "direct shared key should use its declared project:read scope"
        );
        assert!(
            enforce_route_scope(&shared, "POST", "/api/runtime-console/projects").is_ok(),
            "direct shared key should retain its existing project:read Runtime Console access"
        );
        assert!(
            enforce_route_scope(&pat, "POST", "/api/oauth/clients/list").is_ok(),
            "PAT remains an allowed first-party identity"
        );
        assert!(
            enforce_route_scope(&oauth, "POST", "/api/oauth/clients/list").is_err(),
            "OAuth delegation cannot become first-party client management"
        );
        assert!(
            enforce_route_scope(&shared, "POST", "/api/future/authenticated-route").is_err(),
            "ordinary principals must fail closed on undeclared routes"
        );
        assert!(
            enforce_route_scope(&bootstrap, "POST", "/api/future/authenticated-route").is_ok(),
            "bootstrap keeps explicit superuser compatibility"
        );
    }

    #[test]
    fn pat_account_manage_compatibility_is_route_bounded() {
        let mut pat = AuthContext::new(super::super::context::AuthKind::ApiToken);
        pat.scopes = vec![SCOPE_RUNTIME_READ.to_string()];

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
        ] {
            assert!(
                enforce_route_scope(&pat, "POST", path).is_ok(),
                "legacy PAT account compatibility should remain on {path}"
            );
        }

        for path in [
            "/api/audit/sessions",
            "/api/audit/session",
            "/api/audit/stats",
        ] {
            assert_eq!(
                enforce_route_scope(&pat, "POST", path),
                Err((
                    Some(SCOPE_ACCOUNT_MANAGE),
                    "missing required scope: account:manage".to_string()
                )),
                "PAT audit access must require account:manage on {path}"
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
    fn oauth_route_policy_legacy_codex_routes_are_removed() {
        for path in [
            "/api/codex/run",
            "/api/codex/context",
            "/api/codex/projects",
            "/api/codex/context_batch",
            "/api/codex/apply_patch",
            "/api/codex/edit",
            "/api/codex/artifact",
            "/api/codex/git",
            "/api/codex/job",
            "/api/codex/report",
        ] {
            assert_eq!(
                oauth_route_scope_policy_for_path_method("POST", path),
                OAuthRouteScopePolicy::Unknown,
                "{path}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_authenticated_route_audit() {
        for (method, path) in [
            ("POST", "/api/connector/task/start"),
            ("POST", "/api/connector/files/read"),
            ("POST", "/api/connector/files/search"),
            ("POST", "/api/connector/code/navigate"),
            ("POST", "/api/connector/code/impact"),
            ("POST", "/api/connector/edits/apply"),
            ("POST", "/api/connector/checks/run"),
            ("POST", "/api/connector/commands/run"),
            ("POST", "/api/connector/task/review"),
            ("POST", "/api/connector/task/cancel"),
            ("POST", "/api/connector/task/finish"),
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
            ("POST", "/api/shell/agent/persistent_shell_result"),
            ("POST", "/api/shell/agent/job_update"),
            ("GET", "/api/agents/ws"),
            ("POST", "/api/pairing/enroll"),
            ("POST", "/api/pairing/create"),
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
            ("POST", "/api/oauth/clients/update_scopes"),
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
                "update_session_context",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
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
                "read_files",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "show_changes",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            (
                "document_diagnostics",
                OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ),
            ),
            ("hover", OAuthToolScopePolicy::Require(SCOPE_PROJECT_READ)),
            (
                "workspace_symbols",
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
                "computer_list_windows",
                OAuthToolScopePolicy::Require(SCOPE_COMPUTER_READ),
            ),
            (
                "computer_snapshot",
                OAuthToolScopePolicy::Require(SCOPE_COMPUTER_READ),
            ),
            (
                "computer_save_snapshot",
                OAuthToolScopePolicy::RequireAll(&[SCOPE_PROJECT_WRITE, SCOPE_COMPUTER_READ]),
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
            "update_session_context",
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
            "document_diagnostics",
            "hover",
            "workspace_symbols",
            "read_file",
            "read_files",
            "write_project_file",
            "artifact_upload_begin",
            "artifact_upload_chunk",
            "artifact_upload_finish",
            "artifact_upload_abort",
            "apply_patch_checked",
            "computer_list_windows",
            "computer_find_elements",
            "computer_element_state",
            "computer_activate_window",
            "computer_snapshot",
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
        for tool in known_tool_names() {
            let metadata = lookup_tool_metadata(tool).unwrap();
            let expected = if tool == "computer_save_snapshot" {
                OAuthToolScopePolicy::RequireAll(&[SCOPE_PROJECT_WRITE, SCOPE_COMPUTER_READ])
            } else if tool == "computer_read_clipboard" {
                OAuthToolScopePolicy::RequireAll(&[
                    SCOPE_COMPUTER_READ,
                    SCOPE_COMPUTER_CLIPBOARD_READ,
                ])
            } else if tool == "computer_write_clipboard" {
                OAuthToolScopePolicy::RequireAll(&[
                    SCOPE_COMPUTER_CONTROL,
                    SCOPE_COMPUTER_CLIPBOARD_WRITE,
                ])
            } else if matches!(tool, "computer_pointer_move" | "computer_pointer_click") {
                OAuthToolScopePolicy::RequireAll(&[
                    SCOPE_COMPUTER_READ,
                    SCOPE_COMPUTER_DISPLAY_READ,
                    SCOPE_COMPUTER_CONTROL,
                    SCOPE_COMPUTER_POINTER_CONTROL,
                ])
            } else if matches!(tool, "computer_list_displays" | "computer_snapshot_display") {
                OAuthToolScopePolicy::RequireAll(&[
                    SCOPE_COMPUTER_READ,
                    SCOPE_COMPUTER_DISPLAY_READ,
                ])
            } else {
                OAuthToolScopePolicy::Require(metadata.oauth_scope.unwrap())
            };
            assert_eq!(
                oauth_scope_policy_for_runtime_tool(tool),
                expected,
                "{tool}"
            );
        }
    }

    #[test]
    fn oauth_route_policy_preserves_legacy_non_runtime_metadata_scope() {
        assert!(!is_known_tool_name("delete_files"));
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
