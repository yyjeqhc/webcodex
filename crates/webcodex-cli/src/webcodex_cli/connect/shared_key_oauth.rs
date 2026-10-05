use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use webcodex_admin::build_server_http_client;
use webcodex_core::authority::profiles::OPTIONAL_BROWSER;

use super::super::http::{post_json_authed, ApiCall};
use super::profile::{atomic_write, validate_existing_regular_file, ConnectOptions, ResolvedKey};
use super::ConnectResult;

const BRIDGE_PROFILE_VERSION: u32 = 1;
const BRIDGE_PROFILE_PREFIX: &str = "shared-key-oauth-";
const BRIDGE_SECRET_DISCLOSED_PREFIX: &str = ".shared-key-oauth-secret-disclosed-";
const LOCAL_MCP_SCOPE: &str = "mcp:local";
const LOCAL_PLUGIN_INSPECT_SCOPE: &str = "plugin:inspect";
const LOCAL_PLUGIN_INVOKE_SCOPE: &str = "plugin:invoke";
const LOCAL_SSH_SCOPE: &str = "ssh:local";
const CODING_AGENT_SCOPE: &str = "coding_agent:run";
#[cfg(test)]
use webcodex_core::authority::profiles::OPTIONAL_COMPUTER as BRIDGE_OPTIONAL_COMPUTER_SCOPES;
use webcodex_core::authority::profiles::{
    SHARED_KEY_COMPUTER as BRIDGE_COMPUTER_ENABLED_SCOPES,
    SHARED_KEY_MODEL as BRIDGE_BASELINE_SCOPES,
};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SharedKeyOAuthProfile {
    version: u32,
    server_url: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    allowed_scopes: Vec<String>,
    #[serde(default)]
    browser_permissions_enabled: bool,
    #[serde(default)]
    computer_permissions_enabled: bool,
    #[serde(default)]
    local_mcp_enabled: bool,
    #[serde(default)]
    local_plugins_enabled: bool,
    #[serde(default)]
    local_ssh_enabled: bool,
    #[serde(default)]
    coding_agent_enabled: bool,
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

#[cfg(test)]
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

fn without_optional_class_scopes(scopes: &[String]) -> Vec<String> {
    scopes
        .iter()
        .filter(|scope| {
            !OPTIONAL_BROWSER.contains(&scope.as_str())
                && !matches!(
                    scope.as_str(),
                    LOCAL_MCP_SCOPE
                        | LOCAL_PLUGIN_INSPECT_SCOPE
                        | LOCAL_PLUGIN_INVOKE_SCOPE
                        | LOCAL_SSH_SCOPE
                        | CODING_AGENT_SCOPE
                )
        })
        .cloned()
        .collect()
}

fn baseline_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    webcodex_core::authority::profiles::is_baseline(scopes)
}
fn computer_enabled_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    webcodex_core::authority::profiles::is_computer(scopes)
}
fn computer_enabled_scope_ceiling_from_existing(scopes: &[String]) -> Option<Vec<String>> {
    webcodex_core::authority::profiles::with_computer(scopes)
}

fn profile_scope_ceiling_is_valid(profile: &SharedKeyOAuthProfile) -> bool {
    if !scope_list_is_unique(&profile.allowed_scopes) {
        return false;
    }
    let local_mcp_present = profile
        .allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_MCP_SCOPE);
    if local_mcp_present != profile.local_mcp_enabled {
        return false;
    }
    let local_plugin_inspect_present = profile
        .allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_PLUGIN_INSPECT_SCOPE);
    let local_plugin_invoke_present = profile
        .allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_PLUGIN_INVOKE_SCOPE);
    if local_plugin_inspect_present != profile.local_plugins_enabled
        || local_plugin_invoke_present != profile.local_plugins_enabled
    {
        return false;
    }
    let local_ssh_present = profile
        .allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_SSH_SCOPE);
    if local_ssh_present != profile.local_ssh_enabled {
        return false;
    }
    let coding_agent_present = profile
        .allowed_scopes
        .iter()
        .any(|scope| scope == CODING_AGENT_SCOPE);
    if coding_agent_present != profile.coding_agent_enabled {
        return false;
    }
    if OPTIONAL_BROWSER.iter().any(|scope| {
        profile.allowed_scopes.iter().any(|s| s == scope) != profile.browser_permissions_enabled
    }) {
        return false;
    }
    let authority_scopes = without_optional_class_scopes(&profile.allowed_scopes);
    if profile.computer_permissions_enabled {
        computer_enabled_scope_ceiling_is_valid(&authority_scopes)
    } else {
        baseline_scope_ceiling_is_valid(&authority_scopes)
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
            "browser_permissions": opts.oauth_browser_permissions,
            "computer_permissions": opts.oauth_computer_permissions,
            "local_mcp": opts.oauth_local_mcp,
            "local_plugins": opts.oauth_local_plugins,
            "local_ssh": opts.oauth_local_ssh,
            "coding_agent": opts.oauth_coding_agent,
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
    if !scope_list_is_unique(&allowed_scopes) {
        return Err("Server returned duplicate OAuth scopes".to_string());
    }
    let local_mcp_present = allowed_scopes.iter().any(|scope| scope == LOCAL_MCP_SCOPE);
    if local_mcp_present != opts.oauth_local_mcp {
        return Err(
            "Server changed local MCP OAuth authority without matching the explicit connect opt-in"
                .to_string(),
        );
    }
    let local_plugin_inspect_present = allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_PLUGIN_INSPECT_SCOPE);
    let local_plugin_invoke_present = allowed_scopes
        .iter()
        .any(|scope| scope == LOCAL_PLUGIN_INVOKE_SCOPE);
    if local_plugin_inspect_present != opts.oauth_local_plugins
        || local_plugin_invoke_present != opts.oauth_local_plugins
    {
        return Err(
            "Server changed local Plugin OAuth authority without matching the explicit connect opt-in"
                .to_string(),
        );
    }
    let local_ssh_present = allowed_scopes.iter().any(|scope| scope == LOCAL_SSH_SCOPE);
    if local_ssh_present != opts.oauth_local_ssh {
        return Err(
            "Server changed local SSH OAuth authority without matching the explicit connect opt-in"
                .to_string(),
        );
    }
    let coding_agent_present = allowed_scopes
        .iter()
        .any(|scope| scope == CODING_AGENT_SCOPE);
    if coding_agent_present != opts.oauth_coding_agent {
        return Err(
            "Server changed coding-agent OAuth authority without matching the explicit connect opt-in"
                .to_string(),
        );
    }
    if OPTIONAL_BROWSER
        .iter()
        .any(|scope| allowed_scopes.iter().any(|s| s == scope) != opts.oauth_browser_permissions)
    {
        return Err(
            "Server changed Browser OAuth authority without matching the explicit connect opt-in"
                .to_string(),
        );
    }
    let authority_scopes = without_optional_class_scopes(&allowed_scopes);
    if opts.oauth_computer_permissions {
        if !computer_enabled_scope_ceiling_is_valid(&authority_scopes) {
            return Err(
                "Server returned an invalid Computer-enabled shared-key OAuth ceiling".to_string(),
            );
        }
        let expected_scopes = if let Some(existing) = existing {
            computer_enabled_scope_ceiling_from_existing(&without_optional_class_scopes(&existing.allowed_scopes))
                .ok_or_else(|| {
                    "existing shared-key OAuth profile cannot be safely upgraded to Computer permissions"
                        .to_string()
                })?
        } else {
            BRIDGE_COMPUTER_ENABLED_SCOPES
                .iter()
                .map(|scope| (*scope).to_string())
                .collect()
        };
        if !string_scope_set_matches(&authority_scopes, &expected_scopes) {
            return Err(
                "Server changed baseline authority while enabling optional Computer permissions"
                    .to_string(),
            );
        }
    } else {
        if !baseline_scope_ceiling_is_valid(&authority_scopes) {
            return Err(
                "Server returned a scope outside the ordinary shared-key OAuth baseline ceiling"
                    .to_string(),
            );
        }
        let expected_scopes = existing
            .map(|profile| without_optional_class_scopes(&profile.allowed_scopes))
            .unwrap_or_else(|| {
                BRIDGE_BASELINE_SCOPES
                    .iter()
                    .map(|scope| (*scope).to_string())
                    .collect()
            });
        if !string_scope_set_matches(&authority_scopes, &expected_scopes) {
            return Err(
                "Server changed baseline authority while provisioning local MCP OAuth access"
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
        updated.browser_permissions_enabled = opts.oauth_browser_permissions;
        updated.computer_permissions_enabled = opts.oauth_computer_permissions;
        updated.local_mcp_enabled = opts.oauth_local_mcp;
        updated.local_plugins_enabled = opts.oauth_local_plugins;
        updated.local_ssh_enabled = opts.oauth_local_ssh;
        updated.coding_agent_enabled = opts.oauth_coding_agent;
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
            browser_permissions_enabled: opts.oauth_browser_permissions,
            computer_permissions_enabled: opts.oauth_computer_permissions,
            local_mcp_enabled: opts.oauth_local_mcp,
            local_plugins_enabled: opts.oauth_local_plugins,
            local_ssh_enabled: opts.oauth_local_ssh,
            coding_agent_enabled: opts.oauth_coding_agent,
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
fn bridge_browser_authorization_key_line(resolved_key: &ResolvedKey, config_path: &Path) -> String {
    if resolved_key.generated {
        format!(
            "Browser authorization key: {}\nEnter this key only on the WebCodex authorize page; do not put it in ChatGPT.\n",
            resolved_key.value
        )
    } else if resolved_key.recovered_profile.is_some() {
        format!(
            "Browser authorization key source: {} (top-level token; not reprinted).\nEnter that key only on the WebCodex authorize page; do not put it in ChatGPT.\n",
            config_path.display()
        )
    } else {
        "Browser authorization key: use the shared key supplied to this command on the WebCodex authorize page; do not put it in ChatGPT.\n".to_string()
    }
}

fn bridge_client_secret_line(
    oauth: &SharedKeyOAuthProfile,
    disclose_client_secret: bool,
    state_path: &Path,
) -> String {
    if disclose_client_secret {
        format!("Client secret: {}\n", oauth.client_secret)
    } else {
        format!(
            "Credential source: protected OAuth client profile at {} (client secret not reprinted).\n",
            state_path.display()
        )
    }
}

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
        if existing.browser_permissions_enabled && !opts.oauth_browser_permissions {
            return Err("this shared-key OAuth profile is Browser-enabled; reconnect with --oauth-browser-permissions".to_string());
        }
        if existing.computer_permissions_enabled && !opts.oauth_computer_permissions {
            return Err(
                "this shared-key OAuth profile already has optional Computer permissions enabled; reconnect with --oauth-computer-permissions to reuse it, or use a different profile/redirect URI for a baseline client"
                    .to_string(),
            );
        }
        if existing.local_mcp_enabled && !opts.oauth_local_mcp {
            return Err(
                "this shared-key OAuth profile already has local MCP authority enabled; reconnect with --oauth-local-mcp to reuse it, or use a different profile/redirect URI"
                    .to_string(),
            );
        }
        if existing.local_plugins_enabled && !opts.oauth_local_plugins {
            return Err(
                "this shared-key OAuth profile already has local Plugin authority enabled; reconnect with --oauth-local-plugins to reuse it, or use a different profile/redirect URI"
                    .to_string(),
            );
        }
        if existing.local_ssh_enabled && !opts.oauth_local_ssh {
            return Err(
                "this shared-key OAuth profile already has local SSH authority enabled; reconnect with --oauth-local-ssh to reuse it, or use a different profile/redirect URI"
                    .to_string(),
            );
        }
        if existing.coding_agent_enabled && !opts.oauth_coding_agent {
            return Err(
                "this shared-key OAuth profile already has coding-agent authority enabled; reconnect with --oauth-coding-agent to reuse it, or use a different profile/redirect URI"
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
    let key_line = bridge_browser_authorization_key_line(resolved_key, config_path);
    let secret_line = bridge_client_secret_line(&oauth, disclose_client_secret, &state_path);
    let authorization_endpoint = bridge_authorization_endpoint(&metadata)?;
    let scope_lines = bridge_scope_output(&oauth);
    let output = format!(
        "WebCodex connected\n\nWhat to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. MCP URL: {server_url}/mcp\n3. Authentication: OAuth 2.0 Authorization Code + PKCE S256\n4. OAuth client ID: {}\n5. {secret_line}6. Redirect URI: {}\n7. Scan Tools and complete the WebCodex browser authorization flow.\n{key_line}8. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\nDetails\nServer:          {server_url}\nRunner:          running\nProfile:         {profile}\nClient:          {runner_client_id}\nRuntime project: {runtime_project_id}\nConfig:          {}\nLogs:            {}\nIssuer:          {}\nAuthorization:   {}\nToken endpoint:  {}\n{scope_lines}\nThe Runner continues to use the direct shared key. ChatGPT receives only OAuth credentials/tokens; OAuth access tokens remain invalid on Runner transport.\n",
        oauth.client_id,
        oauth.redirect_uri,
        config_path.display(),
        log_path.display(),
        metadata.issuer,
        authorization_endpoint,
        metadata.token_endpoint,
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
            oauth_browser_permissions: false,
            oauth_computer_permissions: false,
            oauth_local_mcp: false,
            oauth_local_plugins: false,
            oauth_local_ssh: false,
            oauth_coding_agent: false,
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
        let scopes = BRIDGE_BASELINE_SCOPES;
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
                    "allowed_scopes": scopes
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
            browser_permissions_enabled: false,
            computer_permissions_enabled: false,
            local_mcp_enabled: false,
            local_plugins_enabled: false,
            local_ssh_enabled: false,
            coding_agent_enabled: false,
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
            browser_permissions_enabled: false,
            computer_permissions_enabled: false,
            local_mcp_enabled: false,
            local_plugins_enabled: false,
            local_ssh_enabled: false,
            coding_agent_enabled: false,
        };
        assert!(profile_scope_ceiling_is_valid(&baseline));

        let mut local_mcp = baseline.clone();
        local_mcp.local_mcp_enabled = true;
        local_mcp.allowed_scopes.push(LOCAL_MCP_SCOPE.to_string());
        assert!(profile_scope_ceiling_is_valid(&local_mcp));
        let mut duplicated = local_mcp.clone();
        duplicated.allowed_scopes.push(LOCAL_MCP_SCOPE.into());
        assert!(!profile_scope_ceiling_is_valid(&duplicated));
        let mut mismatched_local_mcp = local_mcp.clone();
        mismatched_local_mcp.local_mcp_enabled = false;
        assert!(!profile_scope_ceiling_is_valid(&mismatched_local_mcp));

        let mut local_plugins = baseline.clone();
        local_plugins.local_plugins_enabled = true;
        local_plugins
            .allowed_scopes
            .push(LOCAL_PLUGIN_INSPECT_SCOPE.to_string());
        local_plugins
            .allowed_scopes
            .push(LOCAL_PLUGIN_INVOKE_SCOPE.to_string());
        assert!(profile_scope_ceiling_is_valid(&local_plugins));
        let mut mismatched_local_plugins = local_plugins.clone();
        mismatched_local_plugins.local_plugins_enabled = false;
        assert!(!profile_scope_ceiling_is_valid(&mismatched_local_plugins));
        let mut inspect_only_plugins = baseline.clone();
        inspect_only_plugins.local_plugins_enabled = true;
        inspect_only_plugins
            .allowed_scopes
            .push(LOCAL_PLUGIN_INSPECT_SCOPE.to_string());
        assert!(!profile_scope_ceiling_is_valid(&inspect_only_plugins));
        let mut invoke_only_plugins = baseline.clone();
        invoke_only_plugins.local_plugins_enabled = true;
        invoke_only_plugins
            .allowed_scopes
            .push(LOCAL_PLUGIN_INVOKE_SCOPE.to_string());
        assert!(!profile_scope_ceiling_is_valid(&invoke_only_plugins));
        let mut legacy_plugin_scope = baseline.clone();
        legacy_plugin_scope.local_plugins_enabled = true;
        legacy_plugin_scope
            .allowed_scopes
            .push("plugin:local".to_string());
        assert!(!profile_scope_ceiling_is_valid(&legacy_plugin_scope));
        let mut manage_plugin_scope = local_plugins.clone();
        manage_plugin_scope
            .allowed_scopes
            .push("plugin:manage".to_string());
        assert!(!profile_scope_ceiling_is_valid(&manage_plugin_scope));

        let mut local_ssh = baseline.clone();
        local_ssh.local_ssh_enabled = true;
        local_ssh.allowed_scopes.push(LOCAL_SSH_SCOPE.to_string());
        assert!(profile_scope_ceiling_is_valid(&local_ssh));
        let mut mismatched_local_ssh = local_ssh.clone();
        mismatched_local_ssh.local_ssh_enabled = false;
        assert!(!profile_scope_ceiling_is_valid(&mismatched_local_ssh));

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
    fn shared_key_oauth_credential_lines_preserve_first_disclosure_and_recovery_sources() {
        let oauth = SharedKeyOAuthProfile {
            version: BRIDGE_PROFILE_VERSION,
            server_url: "https://server.example".to_string(),
            client_id: "wc_client_test".to_string(),
            client_secret: "wc_csec_test_once".to_string(),
            redirect_uri: "https://chatgpt.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            browser_permissions_enabled: false,
            computer_permissions_enabled: false,
            local_mcp_enabled: false,
            local_plugins_enabled: false,
            local_ssh_enabled: false,
            coding_agent_enabled: false,
        };
        let state_path = Path::new("/protected/profile/shared-key-oauth.toml");
        let first = bridge_client_secret_line(&oauth, true, state_path);
        assert_eq!(first.matches("wc_csec_test_once").count(), 1);
        let reused = bridge_client_secret_line(&oauth, false, state_path);
        assert!(!reused.contains("wc_csec_test_once"));
        assert!(reused.contains("/protected/profile/shared-key-oauth.toml"));
        assert!(reused.contains("not reprinted"));

        let generated = ResolvedKey {
            value: "wck_browser_once".to_string(),
            generated: true,
            recovered_profile: None,
            warn_short: false,
        };
        let generated_line =
            bridge_browser_authorization_key_line(&generated, Path::new("runner.toml"));
        assert_eq!(generated_line.matches("wck_browser_once").count(), 1);

        let recovered = ResolvedKey {
            value: "wck_browser_hidden".to_string(),
            generated: false,
            recovered_profile: Some("profile".to_string()),
            warn_short: false,
        };
        let recovered_line = bridge_browser_authorization_key_line(
            &recovered,
            Path::new("/protected/profile/agent.toml"),
        );
        assert!(!recovered_line.contains("wck_browser_hidden"));
        assert!(recovered_line.contains("/protected/profile/agent.toml"));
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
    #[tokio::test]
    async fn browser_opt_in_validates_server_authority_and_preserves_narrow_profiles() {
        for computer in [false, true] {
            for browser in [false, true] {
                for returned_browser_count in [0, 1, 3] {
                    let mut prior = vec!["runtime:read".to_string()];
                    if computer {
                        prior.extend(
                            BRIDGE_OPTIONAL_COMPUTER_SCOPES
                                .iter()
                                .map(|s| s.to_string()),
                        );
                    }
                    let mut returned = prior.clone();
                    returned.extend(
                        OPTIONAL_BROWSER[..returned_browser_count]
                            .iter()
                            .map(|s| s.to_string()),
                    );
                    let (server, handle) = json_responses(vec![json!({
                        "reused": true, "client": {
                            "client_id": "wc_client_existing",
                            "redirect_uri": "https://chatgpt.example/callback",
                            "allowed_scopes": returned
                        }
                    })]);
                    let mut opts = options(server.clone());
                    opts.oauth_browser_permissions = browser;
                    opts.oauth_computer_permissions = computer;
                    let existing = SharedKeyOAuthProfile {
                        version: BRIDGE_PROFILE_VERSION,
                        server_url: server.clone(),
                        client_id: "wc_client_existing".into(),
                        client_secret: "wc_csec_existing".into(),
                        redirect_uri: "https://chatgpt.example/callback".into(),
                        allowed_scopes: prior,
                        browser_permissions_enabled: false,
                        computer_permissions_enabled: computer,
                        local_mcp_enabled: false,
                        local_plugins_enabled: false,
                        local_ssh_enabled: false,
                        coding_agent_enabled: false,
                    };
                    let result = provision_client(
                        &opts,
                        &server,
                        "ordinary-connect-shared-key",
                        &existing.redirect_uri,
                        Some(&existing),
                    )
                    .await;
                    assert_eq!(
                        result.is_ok(),
                        returned_browser_count == if browser { 3 } else { 0 }
                    );
                    if let Ok((profile, _)) = result {
                        assert_eq!(profile.allowed_scopes, returned);
                        assert!(profile_scope_ceiling_is_valid(&profile));
                        assert_eq!(profile.browser_permissions_enabled, browser);
                        assert_eq!(
                            bridge_scope_output(&profile).contains("browser:launch"),
                            browser
                        );
                        let legacy = toml::to_string(&profile)
                            .unwrap()
                            .lines()
                            .filter(|line| !line.starts_with("browser_permissions_enabled"))
                            .collect::<Vec<_>>()
                            .join("\n");
                        let decoded: SharedKeyOAuthProfile = toml::from_str(&legacy).unwrap();
                        assert!(!decoded.browser_permissions_enabled);
                        assert_eq!(profile_scope_ceiling_is_valid(&decoded), !browser);
                    }
                    handle.join().unwrap();
                }
            }
        }
    }
}
