use crate::auth::scopes;

use super::OAuthAuthorizeError;

/// Non-agent scopes that OAuth2 clients may request. Agent transport scopes
/// (`agent:*`) are excluded because OAuth2 access tokens are rejected on agent
/// transport surfaces. `admin` is excluded because it is a bootstrap/superuser
/// scope not intended for OAuth2 delegation.
const OAUTH_SCOPES_SUPPORTED: &[&str] = &[
    scopes::SCOPE_RUNTIME_READ,
    scopes::SCOPE_PROJECT_READ,
    scopes::SCOPE_PROJECT_WRITE,
    scopes::SCOPE_JOB_RUN,
    scopes::SCOPE_ACCOUNT_MANAGE,
];

/// Return the canonical global OAuth scope registry.
///
/// The order is stable and is used for authorization-time normalization.
pub(crate) fn oauth_scopes_supported() -> &'static [&'static str] {
    OAUTH_SCOPES_SUPPORTED
}

/// Normalize authorize-time OAuth scopes against a registered client's allowed
/// scopes and the global OAuth scope registry.
///
/// If `requested` is absent or ASCII-whitespace-only, default to the
/// intersection of `client_allowed` and the global OAuth scope registry. When
/// `requested` is present, every requested scope must be both globally
/// supported and allowed by the client. Output is deduplicated and ordered by
/// the global registry.
#[allow(dead_code)]
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
                if !oauth_scopes_supported().contains(&scope) || !client_allowed.contains(scope) {
                    return Err(OAuthAuthorizeError::InvalidScope("invalid scope"));
                }
                requested_scopes.insert(scope);
            }

            oauth_scopes_supported()
                .iter()
                .copied()
                .filter(|scope| requested_scopes.contains(scope))
                .collect::<Vec<_>>()
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
