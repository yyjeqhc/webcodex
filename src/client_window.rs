//! Stable, host- or transport-owned chat-window identity.
//!
//! The raw host/transport value is never persisted or returned by a tool. Runtime
//! state uses only the domain-separated SHA-256 key below together with the
//! authenticated subject. Project-coupled observations additionally bind the exact
//! Project; principal-scoped Peer communication deliberately does not.

use salvo::http::header::{HeaderValue, SET_COOKIE};
use salvo::prelude::{Request, Response};
use sha2::{Digest, Sha256};

pub(crate) const MCP_SESSION_HEADER: &str = "mcp-session-id";
pub(crate) const CLIENT_WINDOW_HEADER: &str = "x-webcodex-client-window";
const OPENAI_CONVERSATION_HEADER: &str = "openai-conversation-id";
const WINDOW_COOKIE: &str = "webcodex_window";
const WINDOW_COOKIE_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 90;
const MAX_OPAQUE_ID_BYTES: usize = 256;
pub(crate) const PEER_ID_PREFIX: &str = "wc_peer_";
const PEER_WINDOW_KEY_PREFIX_HEX_LEN: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClientWindow {
    key: String,
    source: &'static str,
}

impl ClientWindow {
    pub(crate) fn from_opaque(source: &'static str, value: &str) -> Option<Self> {
        let value = valid_opaque_id(value)?;
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex.client-window.v1\0");
        hasher.update(source.as_bytes());
        hasher.update(b"\0");
        hasher.update(value.as_bytes());
        Some(Self {
            key: format!("{:x}", hasher.finalize()),
            source,
        })
    }

    fn from_scoped(source: &'static str, scope: &[&str], value: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"webcodex.client-window.scoped.v1\0");
        let mut part = |bytes: &[u8]| {
            hasher.update((bytes.len() as u64).to_be_bytes());
            hasher.update(bytes);
        };
        part(source.as_bytes());
        part(&(scope.len() as u64).to_be_bytes());
        for component in scope {
            part(component.as_bytes());
        }
        part(value.as_bytes());
        Self {
            key: format!("{:x}", hasher.finalize()),
            source,
        }
    }

    pub(crate) fn key(&self) -> &str {
        &self.key
    }

    pub(crate) fn peer_id(&self) -> String {
        peer_id_from_window_key(&self.key)
            .expect("ClientWindow keys are canonical lowercase SHA-256 hex")
    }

    pub(crate) fn source(&self) -> &'static str {
        self.source
    }

    #[cfg(test)]
    pub(crate) fn for_test(value: &str) -> Self {
        Self::from_opaque("test", value).expect("test window identity")
    }
}

#[derive(Debug, Clone)]
pub(crate) struct McpWindow {
    pub(crate) identity: Option<ClientWindow>,
    /// Newly minted protocol session id returned on initialize.
    pub(crate) issued_session_id: Option<String>,
}

/// Resolve supported MCP Window sources before audit, liveness or dispatch.
/// Legacy protocol session issuance is independent from the chosen Window.
/// A format-valid legacy header is not checked against an issuance registry.
pub(crate) fn mcp_window(
    req: &Request,
    params: &serde_json::Value,
    auth: Option<&crate::auth::AuthContext>,
    legacy_protocol: bool,
    initialize: bool,
) -> Result<McpWindow, &'static str> {
    // Validate every supplied supported source, including lower-priority ones:
    // malformed metadata must never silently select a different identity.
    let openai = metadata_window_id(params, "openai/session", true)?;
    let codex = metadata_window_id(params, "threadId", false)?;
    let header = explicit_window_header(req)?;

    let legacy = legacy_protocol
        .then(|| request_header(req, MCP_SESSION_HEADER))
        .flatten();
    let issued_session_id = (legacy_protocol && initialize && legacy.is_none())
        .then(|| format!("wc_mcp_{}", uuid::Uuid::new_v4().simple()));
    let identity = if let Some(raw) = openai {
        // Keep the existing ChatGPT v1 key exactly, including its historical
        // trimming rule, so existing conversation bindings continue to work.
        ClientWindow::from_opaque("openai-session", raw)
    } else if let Some(raw) = codex {
        scoped_mcp_window("codex-thread", raw, auth)
    } else if let Some(raw) = header {
        scoped_mcp_window("client-window-header", raw, auth)
    } else {
        legacy
            .or(issued_session_id.as_deref())
            .and_then(|raw| ClientWindow::from_opaque("mcp", raw))
    };
    Ok(McpWindow {
        identity,
        issued_session_id,
    })
}

const INVALID_MCP_WINDOW: &str = "invalid_client_window: malformed or ambiguous window identity";

fn metadata_window_id<'a>(
    params: &'a serde_json::Value,
    name: &str,
    legacy_trim: bool,
) -> Result<Option<&'a str>, &'static str> {
    let Some(value) = params.get("_meta").and_then(|meta| meta.get(name)) else {
        return Ok(None);
    };
    let raw = value.as_str().ok_or(INVALID_MCP_WINDOW)?;
    let valid = if legacy_trim {
        valid_opaque_id(raw)
    } else {
        valid_exact_opaque_id(raw)
    };
    valid.map(Some).ok_or(INVALID_MCP_WINDOW)
}

fn explicit_window_header(req: &Request) -> Result<Option<&str>, &'static str> {
    let mut values = req.headers().get_all(CLIENT_WINDOW_HEADER).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(INVALID_MCP_WINDOW);
    }
    let raw = value.to_str().map_err(|_| INVALID_MCP_WINDOW)?;
    if valid_exact_opaque_id(raw).is_none()
        || !raw
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(INVALID_MCP_WINDOW);
    }
    Ok(Some(raw))
}

fn scoped_mcp_window(
    source: &'static str,
    raw: &str,
    auth: Option<&crate::auth::AuthContext>,
) -> Option<ClientWindow> {
    use crate::auth::AuthKind;

    let auth = auth.filter(|auth| !auth.is_bootstrap())?;
    if let Some((user, client)) = auth.managed_oauth_window_identity() {
        return Some(ClientWindow::from_scoped(
            source,
            &["managed-oauth", user, client],
            raw,
        ));
    }
    let (scope_kind, scope_id) = if auth.is_project_scoped_model_subject() {
        ("project-grant", auth.project_grant_id.as_deref())
    } else if auth.is_shared_key() || auth.is_oauth_shared_key_subject() {
        // Direct shared-key and its OAuth bridge identify the same credential,
        // not individual holders. Never use an access-token id for this scope.
        ("shared-key", auth.shared_key_hash.as_deref())
    } else if auth.kind == AuthKind::ApiToken {
        ("api-token", auth.api_key_id.as_deref())
    } else {
        // Anonymous, bootstrap, incomplete managed OAuth and unsupported
        // credential kinds can still execute normally without Window affinity.
        return None;
    };
    let scope_id = scope_id.filter(|id| !id.trim().is_empty())?;
    Some(ClientWindow::from_scoped(
        source,
        &[scope_kind, scope_id],
        raw,
    ))
}

// Existing protocol-independent App/trace fixtures need only the ChatGPT
// adapter. Keep this delegate test-only; HTTP has exactly one resolver above.
#[cfg(test)]
pub(crate) fn stateless_mcp_window(params: &serde_json::Value) -> McpWindow {
    mcp_window(&Request::new(), params, None, false, false)
        .expect("fixture contains valid Window metadata")
}
pub(crate) fn peer_id_from_window_key(window_key: &str) -> Option<String> {
    if window_key.len() != 64
        || !window_key
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    Some(format!(
        "{PEER_ID_PREFIX}{}",
        &window_key[..PEER_WINDOW_KEY_PREFIX_HEX_LEN]
    ))
}

pub(crate) fn peer_window_key_prefix(peer_id: &str) -> Option<&str> {
    let suffix = peer_id.strip_prefix(PEER_ID_PREFIX)?;
    if suffix.len() != PEER_WINDOW_KEY_PREFIX_HEX_LEN
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    Some(suffix)
}

pub(crate) fn set_mcp_session_header(res: &mut Response, session_id: &str) {
    if let Ok(value) = HeaderValue::from_str(session_id) {
        res.headers_mut().insert(MCP_SESSION_HEADER, value);
    }
}

/// Resolve an HTTP/API window without adding a public operation or request
/// field. Hosted Actions provide a conversation-scoped header; first-party
/// HTTP clients otherwise receive an opaque HttpOnly cookie and must use one
/// cookie jar per logical window.
pub(crate) fn api_window(req: &Request, res: &mut Response) -> ClientWindow {
    if let Some(raw) = request_header(req, OPENAI_CONVERSATION_HEADER) {
        if let Some(window) = ClientWindow::from_opaque("openai-conversation", raw) {
            return window;
        }
    }
    if let Some(raw) = request_cookie(req, WINDOW_COOKIE) {
        if let Some(window) = ClientWindow::from_opaque("http-cookie", raw) {
            return window;
        }
    }

    let raw = format!("wc_win_{}", uuid::Uuid::new_v4().simple());
    let window =
        ClientWindow::from_opaque("http-cookie", &raw).expect("generated window id is valid");
    let mut cookie = format!(
        "{WINDOW_COOKIE}={raw}; Max-Age={WINDOW_COOKIE_MAX_AGE_SECS}; Path=/; HttpOnly; SameSite=Lax"
    );
    if request_is_https(req) {
        cookie.push_str("; Secure");
    }
    if let Ok(value) = HeaderValue::from_str(&cookie) {
        res.headers_mut().append(SET_COOKIE, value);
    }
    window
}

fn request_header<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    req.headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(valid_opaque_id)
}

fn request_cookie<'a>(req: &'a Request, name: &str) -> Option<&'a str> {
    let header = req.headers().get("cookie")?.to_str().ok()?;
    header.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == name).then_some(value).and_then(valid_opaque_id)
    })
}

fn valid_opaque_id(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= MAX_OPAQUE_ID_BYTES
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte)))
    .then_some(value)
}

fn valid_exact_opaque_id(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= MAX_OPAQUE_ID_BYTES
        && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte)))
    .then_some(value)
}

fn request_is_https(req: &Request) -> bool {
    request_header(req, "x-forwarded-proto")
        .is_some_and(|value| value.eq_ignore_ascii_case("https"))
        || req.uri().scheme_str() == Some("https")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_window_values_are_domain_separated_and_not_retained() {
        let mcp = ClientWindow::from_opaque("mcp", "same-value").unwrap();
        let http = ClientWindow::from_opaque("http-cookie", "same-value").unwrap();
        assert_ne!(mcp.key(), http.key());
        assert!(!mcp.key().contains("same-value"));
        assert_eq!(mcp.key().len(), 64);
    }

    #[test]
    fn malformed_or_oversized_ids_are_rejected() {
        assert!(ClientWindow::from_opaque("mcp", "").is_none());
        assert!(ClientWindow::from_opaque("mcp", "contains space").is_none());
        assert!(ClientWindow::from_opaque("mcp", &"x".repeat(257)).is_none());
    }

    #[test]
    fn openai_window_preserves_v1_hash_and_trimming() {
        let params = serde_json::json!({
            "_meta": {"openai/session": "  chat-session-opaque-value  "}
        });
        let first = stateless_mcp_window(&params);
        let identity = first.identity.unwrap();
        let mut hash = Sha256::new();
        hash.update(b"webcodex.client-window.v1\0openai-session\0chat-session-opaque-value");
        assert_eq!(identity.key(), format!("{:x}", hash.finalize()));
        assert_eq!(identity.source(), "openai-session");
        assert!(first.issued_session_id.is_none());
    }

    #[test]
    fn supported_metadata_is_exact_and_invalid_presence_rejects() {
        let request = Request::new();
        for value in [
            serde_json::json!(""),
            serde_json::json!("a".repeat(257)),
            serde_json::json!(" padded "),
            serde_json::json!("contains space"),
            serde_json::json!("non-ascii-界"),
            serde_json::json!("control\n"),
            serde_json::json!(42),
            serde_json::Value::Null,
        ] {
            let params = serde_json::json!({"_meta": {"threadId": value}});
            assert!(mcp_window(&request, &params, None, false, false).is_err());
        }
        for value in [serde_json::json!(42), serde_json::json!("contains space")] {
            let params = serde_json::json!({"_meta": {"openai/session": value}});
            assert!(mcp_window(&request, &params, None, false, false).is_err());
        }
        for params in [
            serde_json::json!({}),
            serde_json::json!({"_meta": {}}),
            serde_json::json!({"_meta": {"windowId": "ignored", "sessionId": "ignored"}}),
        ] {
            let result = mcp_window(&request, &params, None, false, false).unwrap();
            assert!(result.identity.is_none());
        }
    }

    #[test]
    fn scoped_hash_separates_source_and_component_boundaries() {
        let first = ClientWindow::from_scoped("codex-thread", &["managed-oauth", "ab", "c"], "raw");
        let second =
            ClientWindow::from_scoped("codex-thread", &["managed-oauth", "a", "bc"], "raw");
        let header =
            ClientWindow::from_scoped("client-window-header", &["managed-oauth", "ab", "c"], "raw");
        let legacy = ClientWindow::from_opaque("codex-thread", "raw").unwrap();
        assert_ne!(first.key(), second.key());
        assert_ne!(first.key(), header.key());
        assert_ne!(first.key(), legacy.key());
    }

    #[test]
    fn header_validation_rejects_ambiguous_or_noncanonical_values() {
        for raw in ["", " padded ", "a,b", "a/b", "a=b", "tab\t", "control\n"] {
            let mut request = Request::new();
            if let Ok(value) = HeaderValue::from_bytes(raw.as_bytes()) {
                request.headers_mut().append(CLIENT_WINDOW_HEADER, value);
                assert!(explicit_window_header(&request).is_err());
            }
        }
        let mut request = Request::new();
        request
            .headers_mut()
            .append(CLIENT_WINDOW_HEADER, HeaderValue::from_static("same"));
        request
            .headers_mut()
            .append(CLIENT_WINDOW_HEADER, HeaderValue::from_static("same"));
        assert!(explicit_window_header(&request).is_err());
        let mut request = Request::new();
        request.headers_mut().append(
            CLIENT_WINDOW_HEADER,
            HeaderValue::from_bytes(&[0xff]).unwrap(),
        );
        assert!(explicit_window_header(&request).is_err());
    }

    #[test]
    fn missing_verified_scope_never_infers_one_from_other_auth_fields() {
        use crate::auth::{AuthContext, AuthKind};
        for kind in [
            AuthKind::OpenAnonymous,
            AuthKind::Bootstrap,
            AuthKind::OAuth2Token,
            AuthKind::AgentToken,
            AuthKind::AccountCredential,
            AuthKind::ApiToken,
            AuthKind::SharedKey,
            AuthKind::ProjectCredential,
        ] {
            let mut auth = AuthContext::new(kind);
            auth.user_id = Some("same-user".into());
            auth.username = Some("same-name".into());
            auth.allowed_client_id = Some("same-client".into());
            assert!(scoped_mcp_window("codex-thread", "thread", Some(&auth)).is_none());
        }
        let mut oauth = AuthContext::new(AuthKind::OAuth2Token);
        oauth.token_kind = Some("oauth2".into());
        oauth.user_id = Some("user".into());
        oauth.api_key_id = Some("access-token-must-not-substitute-for-client".into());
        assert!(scoped_mcp_window("codex-thread", "thread", Some(&oauth)).is_none());
    }
}
