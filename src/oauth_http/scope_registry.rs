use crate::auth::scopes;

use super::OAuthAuthorizeError;

/// Non-agent scopes that OAuth2 clients may request. Runner transport scopes
/// (`agent:*`) are excluded because OAuth2 access tokens are rejected on agent
/// transport surfaces. `admin` is excluded because it is a bootstrap/superuser
/// scope not intended for OAuth2 delegation.
const OAUTH_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_RUNNER_MANAGE,
    scopes::SCOPE_SESSION_COLLABORATE,
    scopes::SCOPE_COMMUNICATION_READ,
    scopes::SCOPE_COMMUNICATION_MANAGE,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_MEMORY_READ,
    scopes::SCOPE_MEMORY_MANAGE,
    scopes::SCOPE_JOB_RUN,
    scopes::SCOPE_JOB_DETACH,
    scopes::SCOPE_COMPUTER_READ,
    scopes::SCOPE_COMPUTER_CONTROL,
    scopes::SCOPE_COMPUTER_LAUNCH,
    scopes::SCOPE_COMPUTER_DISPLAY_READ,
    scopes::SCOPE_COMPUTER_POINTER_CONTROL,
    scopes::SCOPE_COMPUTER_CLIPBOARD_READ,
    scopes::SCOPE_COMPUTER_CLIPBOARD_WRITE,
    scopes::SCOPE_MCP_LOCAL,
    scopes::SCOPE_PLUGIN_INSPECT,
    scopes::SCOPE_PLUGIN_INVOKE,
    scopes::SCOPE_PLUGIN_MANAGE,
    scopes::SCOPE_SSH_LOCAL,
    scopes::SCOPE_CODING_AGENT_RUN,
    scopes::SCOPE_ACCOUNT_MANAGE,
];

/// Protocol-level scope used by OAuth clients to request refresh-token access.
/// It does not grant any WebCodex API permission by itself and therefore is not
/// stored in a client's `allowed_scopes` permission allow-list.
pub(crate) const OAUTH_OFFLINE_ACCESS_SCOPE: &str = "offline_access";

/// Return the canonical global OAuth scope registry.
///
/// The order is stable and is used for authorization-time normalization.
pub(crate) fn oauth_scopes_supported() -> &'static [&'static str] {
    OAUTH_SCOPES_SUPPORTED
}

/// Return scopes advertised through OAuth authorization-server discovery. This
/// includes WebCodex permission scopes plus protocol capabilities such as
/// `offline_access`.
pub(crate) fn oauth_discovery_scopes_supported() -> Vec<&'static str> {
    let mut scopes = oauth_scopes_supported().to_vec();
    scopes.push(OAUTH_OFFLINE_ACCESS_SCOPE);
    scopes
}

/// Normalize authorize-time OAuth scopes against a registered client's allowed
/// scopes and the global OAuth scope registry.
///
/// If `requested` is absent or ASCII-whitespace-only, default to the
/// intersection of `client_allowed` and the WebCodex permission-scope registry.
/// When `requested` is present, permission scopes must also be allowed by the
/// registered client. Protocol scopes such as `offline_access` are accepted
/// independently because they confer no WebCodex API permission and refresh
/// tokens were already issued by the pre-existing flow. Output is deduplicated
/// and ordered by permission scope first, then protocol scope.
pub(crate) fn normalize_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let client_allowed: std::collections::HashSet<&str> =
        client_allowed.split_ascii_whitespace().collect();

    let normalized = match requested {
        Some(raw) if raw.split_ascii_whitespace().next().is_some() => {
            let mut requested_scopes = std::collections::HashSet::new();
            for scope in raw.split_ascii_whitespace() {
                if oauth_scopes_supported().contains(&scope) {
                    if !client_allowed.contains(scope) {
                        return Err(OAuthAuthorizeError::InvalidScope("invalid scope"));
                    }
                } else if scope != OAUTH_OFFLINE_ACCESS_SCOPE {
                    return Err(OAuthAuthorizeError::InvalidScope("invalid scope"));
                }
                requested_scopes.insert(scope);
            }

            let mut normalized = oauth_scopes_supported()
                .iter()
                .copied()
                .filter(|scope| requested_scopes.contains(scope))
                .collect::<Vec<_>>();
            if requested_scopes.contains(OAUTH_OFFLINE_ACCESS_SCOPE) {
                normalized.push(OAUTH_OFFLINE_ACCESS_SCOPE);
            }
            normalized
        }
        _ => oauth_scopes_supported()
            .iter()
            .copied()
            .filter(|scope| client_allowed.contains(scope))
            .collect::<Vec<_>>(),
    };

    if normalized.is_empty() {
        return Err(OAuthAuthorizeError::InvalidScope("empty scope"));
    }

    Ok(normalized.join(" "))
}
