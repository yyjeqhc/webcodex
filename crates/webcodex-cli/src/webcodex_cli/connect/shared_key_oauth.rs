use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use webcodex_admin::build_server_http_client;

use super::super::http::{post_json_authed, ApiCall};
use super::profile::{atomic_write, validate_existing_regular_file, ConnectOptions, ResolvedKey};
use super::ConnectResult;

const BRIDGE_PROFILE_VERSION: u32 = 1;
const BRIDGE_PROFILE_PREFIX: &str = "shared-key-oauth-";
const BRIDGE_SECRET_DISCLOSED_PREFIX: &str = ".shared-key-oauth-secret-disclosed-";
const BRIDGE_BASELINE_SCOPES: &[&str] = &[
    "runtime:read",
    "project:read",
    "project:write",
    "job:run",
    "computer:read",
    "computer:control",
];
const BRIDGE_OPTIONAL_COMPUTER_SCOPES: &[&str] = &[
    "computer:launch",
    "computer:display_read",
    "computer:pointer_control",
    "computer:clipboard_read",
    "computer:clipboard_write",
];
// Canonical ceiling for a fresh Computer-enabled client. Existing profiles may
// retain a narrower non-empty baseline subset plus all optional Computer scopes.
const BRIDGE_COMPUTER_ENABLED_SCOPES: &[&str] = &[
    "runtime:read",
    "project:read",
    "project:write",
    "job:run",
    "computer:read",
    "computer:control",
    "computer:launch",
    "computer:display_read",
    "computer:pointer_control",
    "computer:clipboard_read",
    "computer:clipboard_write",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SharedKeyOAuthProfile {
    version: u32,
    server_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    allowed_scopes: Vec<String>,
    #[serde(default)]
    computer_permissions_enabled: bool,
}

#[derive(Debug, Clone)]
struct OAuthMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
}

fn validate_redirect_uri(uri: &str) -> Result<String, String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err("OAuth redirect URI cannot be empty".to_string());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| "OAuth redirect URI is not a valid URL".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("OAuth redirect URI must not contain userinfo".to_string());
    }
    if parsed.fragment().is_some() {
        return Err("OAuth redirect URI must not contain a fragment".to_string());
    }
    let scheme = parsed.scheme().to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") {
        return Err("OAuth redirect URI must use http or https".to_string());
    }
    let host = parsed.host_str().unwrap_or("");
    if host.is_empty() {
        return Err("OAuth redirect URI must have a host".to_string());
    }
    if scheme == "http" && !matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") {
        return Err(
            "http OAuth redirect URI is only allowed for loopback; use https for other hosts"
                .to_string(),
        );
    }
    Ok(trimmed.to_string())
}

fn profile_path(profile_dir: &Path, redirect_uri: &str) -> PathBuf {
    let digest = Sha256::digest(redirect_uri.as_bytes());
    profile_dir.join(format!("{BRIDGE_PROFILE_PREFIX}{digest:x}.toml"))
}

fn disclosure_marker(profile_dir: &Path, client_id: &str) -> PathBuf {
    let digest = Sha256::digest(client_id.as_bytes());
    profile_dir.join(format!("{BRIDGE_SECRET_DISCLOSED_PREFIX}{digest:x}"))
}

fn scope_set_matches(scopes: &[String], expected: &[&str]) -> bool {
    scopes.len() == expected.len()
        && expected
            .iter()
            .all(|expected_scope| scopes.iter().any(|scope| scope == expected_scope))
}

fn string_scope_set_matches(scopes: &[String], expected: &[String]) -> bool {
    scopes.len() == expected.len()
        && expected
            .iter()
            .all(|expected_scope| scopes.iter().any(|scope| scope == expected_scope))
}

fn scope_list_is_unique(scopes: &[String]) -> bool {
    scopes
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        == scopes.len()
}

fn baseline_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    !scopes.is_empty()
        && scope_list_is_unique(scopes)
        && scopes
            .iter()
            .all(|scope| BRIDGE_BASELINE_SCOPES.contains(&scope.as_str()))
}

fn computer_enabled_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    if scopes.is_empty() || !scope_list_is_unique(scopes) {
        return false;
    }

    let mut has_baseline_scope = false;
    for scope in scopes {
        if BRIDGE_BASELINE_SCOPES.contains(&scope.as_str()) {
            has_baseline_scope = true;
        } else if !BRIDGE_OPTIONAL_COMPUTER_SCOPES.contains(&scope.as_str()) {
            return false;
        }
    }

    has_baseline_scope
        && BRIDGE_OPTIONAL_COMPUTER_SCOPES
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}

fn computer_enabled_scope_ceiling_from_existing(scopes: &[String]) -> Option<Vec<String>> {
    if computer_enabled_scope_ceiling_is_valid(scopes) {
        return Some(scopes.to_vec());
    }
    if !baseline_scope_ceiling_is_valid(scopes) {
        return None;
    }

    let mut expanded = scopes.to_vec();
    expanded.extend(
        BRIDGE_OPTIONAL_COMPUTER_SCOPES
            .iter()
            .map(|scope| (*scope).to_string()),
    );
    Some(expanded)
}

fn profile_scope_ceiling_is_valid(profile: &SharedKeyOAuthProfile) -> bool {
    if profile.computer_permissions_enabled {
        computer_enabled_scope_ceiling_is_valid(&profile.allowed_scopes)
    } else {
        baseline_scope_ceiling_is_valid(&profile.allowed_scopes)
    }
}

fn read_profile(path: &Path) -> Result<Option<SharedKeyOAuthProfile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    validate_existing_regular_file(path)?;
    let content = std::fs::read_to_string(path)
        .map_err(|error| format!("failed to read shared-key OAuth profile: {error}"))?;
    let profile: SharedKeyOAuthProfile = toml::from_str(&content)
        .map_err(|error| format!("failed to parse shared-key OAuth profile: {error}"))?;
    if profile.version != BRIDGE_PROFILE_VERSION
        || !profile.client_id.starts_with("wc_client_")
        || !profile.client_secret.starts_with("wc_csec_")
        || !profile_scope_ceiling_is_valid(&profile)
    {
        return Err(
            "existing shared-key OAuth profile is invalid; refusing to guess credential state"
                .to_string(),
        );
    }
    Ok(Some(profile))
}

async fn fetch_metadata(opts: &ConnectOptions, server_url: &str) -> Result<OAuthMetadata, String> {
    let client = build_server_http_client(&opts.server_http)?;
    let response = client
        .get(format!(
            "{}/.well-known/oauth-authorization-server",
            server_url.trim_end_matches('/')
        ))
        .send()
        .await
        .map_err(|error| format!("failed to discover Server OAuth metadata: {error}"))?;
    if response.status().as_u16() == 404 {
        return Err("the remote WebCodex Server does not have OAuth enabled".to_string());
    }
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|error| format!("failed to parse Server OAuth metadata: {error}"))?;
    if !status.is_success() {
        return Err(format!(
            "Server OAuth discovery failed with HTTP {}",
            status.as_u16()
        ));
    }
    let field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Server OAuth metadata is missing {name}"))
    };
    Ok(OAuthMetadata {
        issuer: field("issuer")?,
        authorization_endpoint: field("authorization_endpoint")?,
        token_endpoint: field("token_endpoint")?,
    })
}

fn bridge_authorization_endpoint(metadata: &OAuthMetadata) -> Result<String, String> {
    let mut url = url::Url::parse(&metadata.authorization_endpoint)
        .map_err(|_| "Server OAuth authorization endpoint is invalid".to_string())?;
    url.query_pairs_mut().append_pair("bridge", "shared_key");
    Ok(url.to_string())
}

async fn provision_client(
    opts: &ConnectOptions,
    server_url: &str,
    shared_key: &str,
    redirect_uri: &str,
    existing: Option<&SharedKeyOAuthProfile>,
) -> Result<(SharedKeyOAuthProfile, bool), String> {
    let value = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token: shared_key,
        path: "/api/oauth/shared-key-client/provision",
        body: json!({
            "redirect_uri": redirect_uri,
            "client_id": existing.map(|profile| profile.client_id.as_str()),
            "previous_allowed_scopes": existing.map(|profile| profile.allowed_scopes.as_slice()),
            "computer_permissions": opts.oauth_computer_permissions,
        }),
    })
    .await?;
    let client_id = value
        .pointer("/client/client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "shared-key OAuth provision response omitted client_id".to_string())?
        .to_string();
    let returned_redirect = value
        .pointer("/client/redirect_uri")
        .and_then(Value::as_str)
        .ok_or_else(|| "shared-key OAuth provision response omitted redirect_uri".to_string())?;
    if returned_redirect != redirect_uri {
        return Err("shared-key OAuth provision response changed the redirect URI".to_string());
    }
    let allowed_scopes = value
        .pointer("/client/allowed_scopes")
        .and_then(Value::as_array)
        .ok_or_else(|| "shared-key OAuth provision response omitted allowed_scopes".to_string())?
        .iter()
        .map(|scope| {
            scope.as_str().map(str::to_string).ok_or_else(|| {
                "shared-key OAuth provision response contains an invalid scope".to_string()
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if opts.oauth_computer_permissions {
        if !computer_enabled_scope_ceiling_is_valid(&allowed_scopes) {
            return Err(
                "Server returned an invalid Computer-enabled shared-key OAuth ceiling".to_string(),
            );
        }
        let expected_scopes = if let Some(existing) = existing {
            computer_enabled_scope_ceiling_from_existing(&existing.allowed_scopes).ok_or_else(|| {
                "existing shared-key OAuth profile cannot be safely upgraded to Computer permissions"
                    .to_string()
            })?
        } else {
            BRIDGE_COMPUTER_ENABLED_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect()
        };
        if !string_scope_set_matches(&allowed_scopes, &expected_scopes) {
            return Err(
                "Server changed baseline authority while enabling optional Computer permissions"
                    .to_string(),
            );
        }
    } else {
        if !baseline_scope_ceiling_is_valid(&allowed_scopes) {
            return Err(
                "Server returned a scope outside the ordinary shared-key OAuth baseline ceiling"
                    .to_string(),
            );
        }
        if let Some(existing) = existing {
            if allowed_scopes != existing.allowed_scopes {
                return Err("persisted shared-key OAuth client differs from the Server; refusing to widen or rewrite it implicitly".to_string());
            }
        } else if !scope_set_matches(&allowed_scopes, BRIDGE_BASELINE_SCOPES) {
            return Err(
                "Server returned a scope outside the ordinary shared-key OAuth baseline ceiling"
                    .to_string(),
            );
        }
    }
    let reused = value
        .get("reused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if reused {
        let existing = existing.ok_or_else(|| {
            "Server reused a shared-key OAuth client without matching local protected state"
                .to_string()
        })?;
        if existing.client_id != client_id {
            return Err(
                "persisted shared-key OAuth client differs from the Server; refusing to rewrite its identity implicitly"
                    .to_string(),
            );
        }
        let mut updated = existing.clone();
        updated.allowed_scopes = allowed_scopes;
        updated.computer_permissions_enabled = opts.oauth_computer_permissions;
        let changed = updated != *existing;
        return Ok((updated, changed));
    }
    let client_secret = value
        .get("client_secret")
        .and_then(Value::as_str)
        .filter(|secret| secret.starts_with("wc_csec_"))
        .ok_or_else(|| "new shared-key OAuth client response omitted client_secret".to_string())?
        .to_string();
    Ok((
        SharedKeyOAuthProfile {
            version: BRIDGE_PROFILE_VERSION,
            server_url: server_url.to_string(),
            client_id,
            client_secret,
            redirect_uri: redirect_uri.to_string(),
            allowed_scopes,
            computer_permissions_enabled: opts.oauth_computer_permissions,
        },
        true,
    ))
}

fn bridge_scope_output(profile: &SharedKeyOAuthProfile) -> String {
    if profile.computer_permissions_enabled {
        format!(
            "Client may request: {}\nProtocol scope: offline_access\nBrowser consent: Additional Computer permissions are granted only when selected on the WebCodex authorization page.\n",
            profile.allowed_scopes.join(" ")
        )
    } else {
        format!(
            "Scopes:        {} offline_access\n",
            profile.allowed_scopes.join(" ")
        )
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn finish_shared_key_oauth_connect(
    opts: &ConnectOptions,
    server_url: &str,
    profile: &str,
    runner_client_id: &str,
    runtime_project_id: &str,
    config_path: &Path,
    log_path: &Path,
    profile_dir: &Path,
    resolved_key: &ResolvedKey,
) -> Result<ConnectResult, String> {
    let redirect_uri = validate_redirect_uri(
        opts.oauth_redirect_uri
            .as_deref()
            .ok_or_else(|| "--auth oauth requires --oauth-redirect-uri <URL>".to_string())?,
    )?;
    let state_path = profile_path(profile_dir, &redirect_uri);
    let existing = read_profile(&state_path)?;
    if let Some(existing) = existing.as_ref() {
        if existing.server_url != server_url || existing.redirect_uri != redirect_uri {
            return Err(
                "shared-key OAuth profile belongs to a different Server or redirect URI"
                    .to_string(),
            );
        }
        if existing.computer_permissions_enabled && !opts.oauth_computer_permissions {
            return Err(
                "this shared-key OAuth profile already has optional Computer permissions enabled; reconnect with --oauth-computer-permissions to reuse it, or use a different profile/redirect URI for a baseline client"
                    .to_string(),
            );
        }
    }
    let metadata = fetch_metadata(opts, server_url).await?;
    let (oauth, created_or_rotated) = provision_client(
        opts,
        server_url,
        &resolved_key.value,
        &redirect_uri,
        existing.as_ref(),
    )
    .await?;
    if created_or_rotated {
        let content = toml::to_string(&oauth)
            .map_err(|error| format!("failed to render shared-key OAuth profile: {error}"))?;
        atomic_write(&state_path, content.as_bytes(), true)?;
    }

    let secret_marker = disclosure_marker(profile_dir, &oauth.client_id);
    let disclose_client_secret = !secret_marker.is_file();
    let mut disclosure_markers = Vec::new();
    if resolved_key.generated {
        disclosure_markers.push(profile_dir.join(super::profile::KEY_DISCLOSED_FILE));
    }
    if disclose_client_secret {
        disclosure_markers.push(secret_marker);
    }
    let key_line = if resolved_key.generated {
        format!(
            "Browser authorization key: {}\nEnter this key only on the WebCodex authorize page; do not put it in ChatGPT.\n",
            resolved_key.value
        )
    } else {
        "Browser authorization key: use the existing shared key for this hosted profile on the WebCodex authorize page.\n".to_string()
    };
    let secret_line = if disclose_client_secret {
        format!("Client secret: {}\n", oauth.client_secret)
    } else {
        String::new()
    };
    let authorization_endpoint = bridge_authorization_endpoint(&metadata)?;
    let scope_lines = bridge_scope_output(&oauth);
    let output = format!(
        "Connected to WebCodex\n\nServer:       {server_url}\nMCP URL:      {server_url}/mcp\nProfile:      {profile}\nClient:       {runner_client_id}\nProject:      {runtime_project_id}\nRunner:       running\nConfig:       {}\nLogs:         {}\n\nChatGPT OAuth: Authorization Code + PKCE S256\nIssuer:        {}\nAuthorization: {}\nToken endpoint: {}\nOAuth client ID: {}\n{secret_line}Redirect URI:  {}\n{scope_lines}\n{key_line}The Runner continues to use the direct shared key. ChatGPT receives only OAuth credentials/tokens; OAuth access tokens remain invalid on Agent transport.\n",
        config_path.display(),
        log_path.display(),
        metadata.issuer,
        authorization_endpoint,
        metadata.token_endpoint,
        oauth.client_id,
        oauth.redirect_uri,
    );
    Ok(ConnectResult {
        output,
        disclosure_markers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use webcodex_admin::ServerHttpOptions;

    fn options(server_url: String) -> ConnectOptions {
        ConnectOptions {
            server_url,
            server_http: ServerHttpOptions {
                proxy: None,
                no_system_proxy: true,
            },
            key: Some("ordinary-connect-shared-key".to_string()),
            key_file: None,
            auth: super::super::ConnectAuth::SharedKeyOAuth,
            oauth_redirect_uri: Some("https://chatgpt.example/callback".to_string()),
            oauth_computer_permissions: false,
            username: None,
            project: PathBuf::from("."),
            profile: None,
            client_id: None,
            project_id: None,
            config_base: None,
            state_base: None,
            runner_bin: None,
            wait_timeout_ms: 100,
        }
    }

    fn json_responses(bodies: Vec<Value>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            for body in bodies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = vec![0u8; 32 * 1024];
                let read = stream.read(&mut request).unwrap();
                assert!(read > 0);
                let request = String::from_utf8_lossy(&request[..read]).to_ascii_lowercase();
                assert!(request.contains("authorization: bearer ordinary-connect-shared-key"));
                let payload = body.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    payload.len(),
                    payload
                )
                .unwrap();
            }
        });
        (format!("http://{address}"), handle)
    }

    #[tokio::test]
    async fn ordinary_connect_oauth_provisions_then_reuses_same_shared_key_client() {
        let scopes = vec![
            "runtime:read",
            "project:read",
            "project:write",
            "job:run",
            "computer:read",
            "computer:control",
        ];
        let (server, handle) = json_responses(vec![
            json!({
                "success": true,
                "reused": false,
                "client": {
                    "client_id": "wc_client_bridge_created",
                    "redirect_uri": "https://chatgpt.example/callback",
                    "allowed_scopes": scopes,
                },
                "client_secret": "wc_csec_bridge_created"
            }),
            json!({
                "success": true,
                "reused": true,
                "client": {
                    "client_id": "wc_client_bridge_created",
                    "redirect_uri": "https://chatgpt.example/callback",
                    "allowed_scopes": [
                        "runtime:read",
                        "project:read",
                        "project:write",
                        "job:run",
                        "computer:read",
                        "computer:control"
                    ]
                }
            }),
        ]);
        let opts = options(server.clone());
        let (created, did_create) = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            None,
        )
        .await
        .unwrap();
        assert!(did_create);
        assert_eq!(created.client_id, "wc_client_bridge_created");
        assert_eq!(created.client_secret, "wc_csec_bridge_created");
        assert_eq!(created.allowed_scopes, scopes);

        let (reused, did_create) = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            Some(&created),
        )
        .await
        .unwrap();
        assert!(!did_create);
        assert_eq!(reused, created);
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn explicit_computer_opt_in_preserves_existing_narrow_baseline() {
        let narrow_computer_scopes = vec![
            "runtime:read".to_string(),
            "project:read".to_string(),
            "computer:launch".to_string(),
            "computer:display_read".to_string(),
            "computer:pointer_control".to_string(),
            "computer:clipboard_read".to_string(),
            "computer:clipboard_write".to_string(),
        ];
        let (server, handle) = json_responses(vec![json!({
            "success": true,
            "reused": true,
            "scope_ceiling_changed": true,
            "client": {
                "client_id": "wc_client_bridge_existing",
                "redirect_uri": "https://chatgpt.example/callback",
                "allowed_scopes": narrow_computer_scopes,
            }
        })]);
        let mut opts = options(server.clone());
        opts.oauth_computer_permissions = true;
        let existing = SharedKeyOAuthProfile {
            version: BRIDGE_PROFILE_VERSION,
            server_url: server.clone(),
            client_id: "wc_client_bridge_existing".to_string(),
            client_secret: "wc_csec_existing_secret".to_string(),
            redirect_uri: "https://chatgpt.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string(), "project:read".to_string()],
            computer_permissions_enabled: false,
        };
        let (upgraded, changed) = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            Some(&existing),
        )
        .await
        .unwrap();
        assert!(changed);
        assert!(upgraded.computer_permissions_enabled);
        assert_eq!(upgraded.client_secret, existing.client_secret);
        assert_eq!(upgraded.allowed_scopes, narrow_computer_scopes);
        for restored in [
            "project:write",
            "job:run",
            "computer:read",
            "computer:control",
        ] {
            assert!(!upgraded
                .allowed_scopes
                .iter()
                .any(|scope| scope == restored));
        }
        handle.join().unwrap();

        // Missing/revoked client replacement accepts the same narrow protected
        // baseline and does not recover full baseline authority.
        let (server, handle) = json_responses(vec![json!({
            "success": true,
            "reused": false,
            "client": {
                "client_id": "wc_client_bridge_rotated",
                "redirect_uri": "https://chatgpt.example/callback",
                "allowed_scopes": narrow_computer_scopes,
            },
            "client_secret": "wc_csec_rotated_secret"
        })]);
        let mut opts = options(server.clone());
        opts.oauth_computer_permissions = true;
        let (rotated, changed) = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            Some(&existing),
        )
        .await
        .unwrap();
        assert!(changed);
        assert!(rotated.computer_permissions_enabled);
        assert_eq!(rotated.allowed_scopes, narrow_computer_scopes);
        assert_eq!(rotated.client_secret, "wc_csec_rotated_secret");
        handle.join().unwrap();

        let mut future_scopes = narrow_computer_scopes
            .iter()
            .cloned()
            .map(serde_json::Value::String)
            .collect::<Vec<_>>();
        future_scopes.push(serde_json::Value::String("computer:future".to_string()));
        let (server, handle) = json_responses(vec![json!({
            "success": true,
            "reused": true,
            "client": {
                "client_id": "wc_client_bridge_existing",
                "redirect_uri": "https://chatgpt.example/callback",
                "allowed_scopes": future_scopes,
            }
        })]);
        let mut opts = options(server.clone());
        opts.oauth_computer_permissions = true;
        let error = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            Some(&existing),
        )
        .await
        .unwrap_err();
        assert!(error.contains("invalid Computer-enabled shared-key OAuth ceiling"));
        handle.join().unwrap();

        // Fresh opt-in continues to accept the canonical full baseline + optional ceiling.
        let full_scopes = BRIDGE_COMPUTER_ENABLED_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect::<Vec<_>>();
        let (server, handle) = json_responses(vec![json!({
            "success": true,
            "reused": false,
            "client": {
                "client_id": "wc_client_bridge_fresh_computer",
                "redirect_uri": "https://chatgpt.example/callback",
                "allowed_scopes": full_scopes,
            },
            "client_secret": "wc_csec_fresh_computer"
        })]);
        let mut opts = options(server.clone());
        opts.oauth_computer_permissions = true;
        let (fresh, changed) = provision_client(
            &opts,
            &server,
            "ordinary-connect-shared-key",
            "https://chatgpt.example/callback",
            None,
        )
        .await
        .unwrap();
        assert!(changed);
        assert!(fresh.computer_permissions_enabled);
        assert!(scope_set_matches(
            &fresh.allowed_scopes,
            BRIDGE_COMPUTER_ENABLED_SCOPES
        ));
        handle.join().unwrap();
    }

    #[test]
    fn profile_and_cli_output_accept_narrow_computer_ceiling_and_reject_invalid_scopes() {
        let baseline = SharedKeyOAuthProfile {
            version: BRIDGE_PROFILE_VERSION,
            server_url: "https://server.example".to_string(),
            client_id: "wc_client_baseline".to_string(),
            client_secret: "wc_csec_baseline".to_string(),
            redirect_uri: "https://chatgpt.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string(), "project:read".to_string()],
            computer_permissions_enabled: false,
        };
        assert!(profile_scope_ceiling_is_valid(&baseline));
        let baseline_output = bridge_scope_output(&baseline);
        assert!(baseline_output.contains("Scopes:"));
        assert!(!baseline_output.contains("Client may request:"));

        let mut enabled = baseline.clone();
        enabled.allowed_scopes.extend(
            BRIDGE_OPTIONAL_COMPUTER_SCOPES
                .iter()
                .map(|scope| (*scope).to_string()),
        );
        enabled.computer_permissions_enabled = true;
        assert!(profile_scope_ceiling_is_valid(&enabled));
        assert_eq!(
            computer_enabled_scope_ceiling_from_existing(&enabled.allowed_scopes),
            Some(enabled.allowed_scopes.clone())
        );
        let enabled_output = bridge_scope_output(&enabled);
        assert!(enabled_output.contains("Client may request:"));
        assert!(enabled_output.contains("Browser consent: Additional Computer permissions"));
        assert!(enabled_output.contains("computer:pointer_control"));
        for absent in [
            "project:write",
            "job:run",
            "computer:read",
            "computer:control",
        ] {
            assert!(!enabled.allowed_scopes.iter().any(|scope| scope == absent));
        }

        let mut invalid = enabled.clone();
        invalid.allowed_scopes.pop();
        assert!(!profile_scope_ceiling_is_valid(&invalid));

        let mut invalid = enabled.clone();
        invalid.allowed_scopes.push("computer:launch".to_string());
        assert!(!profile_scope_ceiling_is_valid(&invalid));

        for forbidden in [
            "account:manage",
            "admin",
            "job:detach",
            "agent:register",
            "agent:future",
            "computer:future",
        ] {
            let mut invalid = enabled.clone();
            invalid.allowed_scopes.push(forbidden.to_string());
            assert!(
                !profile_scope_ceiling_is_valid(&invalid),
                "forbidden scope accepted: {forbidden}"
            );
        }

        let full_enabled = SharedKeyOAuthProfile {
            allowed_scopes: BRIDGE_COMPUTER_ENABLED_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect(),
            ..enabled
        };
        assert!(profile_scope_ceiling_is_valid(&full_enabled));
    }

    #[test]
    fn bridge_authorization_endpoint_preserves_existing_query_and_adds_selector() {
        let metadata = OAuthMetadata {
            issuer: "https://server.example".to_string(),
            authorization_endpoint: "https://server.example/oauth/authorize?tenant=one".to_string(),
            token_endpoint: "https://server.example/oauth/token".to_string(),
        };
        let endpoint = bridge_authorization_endpoint(&metadata).unwrap();
        assert!(endpoint.contains("tenant=one"));
        assert!(endpoint.contains("bridge=shared_key"));
    }

    #[test]
    fn bridge_profile_path_is_callback_specific() {
        let root = Path::new("profile");
        assert_ne!(
            profile_path(root, "https://chatgpt.example/a"),
            profile_path(root, "https://chatgpt.example/b")
        );
    }
}
