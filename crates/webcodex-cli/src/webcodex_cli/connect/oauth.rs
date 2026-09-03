use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use webcodex_admin::build_server_http_client;

use super::super::connections::{connections_for_server, ensure_real_directory_tree, Connection};
use super::super::http::{post_json_authed, ApiCall};
use super::super::profiles::{
    client_output_dir_for_profile, client_state_dir_for_profile, default_client_base_dir,
    default_client_state_base_dir, validate_client_profile,
};
use super::super::{discover_internal_binary, validate_user_api_token};
use super::probe::wait_for_connection;
use super::process::{
    ensure_runner_unlocked, load_runner_state, local_runner_log_path, local_runner_profile_marker,
    local_runner_state_summary, process_matches, stop_runner_unlocked, RunnerStart,
};
use super::profile::{
    atomic_write, derived_oauth_profile, ensure_private_directory, generated_client_id,
    read_existing_runner_config, render_project_file, render_runner_document, resolve_project,
    validate_existing_regular_file, ConnectOptions, ExistingRunnerConfig, ProfileLock,
};
use super::{ConnectResult, DEFAULT_CONNECT_WAIT_MS};

const OAUTH_PROFILE_FILE: &str = "oauth-connect.toml";
const OAUTH_PROFILE_VERSION: u32 = 1;
const OAUTH_SECRET_DISCLOSED_FILE_PREFIX: &str = ".oauth-client-secret-disclosed-";

// Hosted connect is intentionally narrower than the global OAuth registry.
// Keep this set closed: new Server scopes must never enter a persisted hosted
// client until this list is explicitly reviewed and changed.
const HOSTED_CONNECT_OAUTH_SCOPES: &[&str] = &[
    "runtime:read",
    "session:collaborate",
    "project:read",
    "project:write",
    "job:run",
    "job:detach",
    "computer:read",
    "computer:control",
    "computer:launch",
    "computer:display_read",
    "computer:pointer_control",
    "computer:clipboard_read",
    "computer:clipboard_write",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct OAuthConnectProfile {
    version: u32,
    server_url: String,
    username: String,
    oauth_client_id: String,
    oauth_client_secret: String,
    oauth_redirect_uri: String,
    allowed_scopes: Vec<String>,
    agent_token_id: String,
}

#[derive(Debug, Clone)]
struct OAuthServerMetadata {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    scopes_supported: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManagedIdentity {
    connection: Connection,
    user_token: String,
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

fn read_managed_identity(
    config_base: &Path,
    server_url: &str,
    requested_user: Option<&str>,
) -> Result<ManagedIdentity, String> {
    let mut connections = connections_for_server(config_base, server_url);
    if let Some(requested_user) = requested_user {
        connections.retain(|connection| connection.username.eq_ignore_ascii_case(requested_user));
    }
    let connection = match connections.len() {
        1 => connections.remove(0),
        0 if requested_user.is_some() => {
            return Err(format!(
                "no logged-in WebCodex user '{}' exists for this Server; run `webcodex login` first",
                requested_user.unwrap_or_default()
            ))
        }
        0 => {
            return Err(
                "OAuth connect requires a managed login for this Server; run `webcodex login <server> --code ...` first"
                    .to_string(),
            )
        }
        _ => {
            let users = connections
                .iter()
                .take(8)
                .map(|connection| connection.username.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "more than one logged-in user exists for this Server ({users}); rerun with --user USER"
            ));
        }
    };
    validate_existing_regular_file(&connection.paths.user_token)?;
    let user_token = std::fs::read_to_string(&connection.paths.user_token)
        .map_err(|error| format!("failed to read managed user token: {error}"))?
        .trim()
        .to_string();
    validate_user_api_token(&user_token)?;
    if !user_token.starts_with("wc_pat_") {
        return Err(
            "the selected login does not contain a managed user PAT; log in again before OAuth connect"
                .to_string(),
        );
    }
    Ok(ManagedIdentity {
        connection,
        user_token,
    })
}

async fn fetch_oauth_metadata(
    opts: &ConnectOptions,
    server_url: &str,
) -> Result<OAuthServerMetadata, String> {
    let client = build_server_http_client(&opts.server_http)?;
    let url = format!(
        "{}/.well-known/oauth-authorization-server",
        server_url.trim_end_matches('/')
    );
    let response = client
        .get(url)
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
    let string_field = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("Server OAuth metadata is missing {name}"))
    };
    let advertised_scopes = value
        .get("scopes_supported")
        .and_then(Value::as_array)
        .ok_or_else(|| "Server OAuth metadata is missing scopes_supported".to_string())?
        .iter()
        .filter_map(Value::as_str)
        .collect::<std::collections::HashSet<_>>();
    let scopes_supported = HOSTED_CONNECT_OAUTH_SCOPES
        .iter()
        .copied()
        .filter(|scope| advertised_scopes.contains(scope))
        .map(str::to_string)
        .collect::<Vec<_>>();
    if scopes_supported.is_empty() {
        return Err(
            "Server OAuth metadata exposes no delegable WebCodex permission scopes".to_string(),
        );
    }
    Ok(OAuthServerMetadata {
        issuer: string_field("issuer")?,
        authorization_endpoint: string_field("authorization_endpoint")?,
        token_endpoint: string_field("token_endpoint")?,
        scopes_supported,
    })
}

fn read_oauth_profile(path: &Path) -> Result<Option<OAuthConnectProfile>, String> {
    if !path.exists() {
        return Ok(None);
    }
    validate_existing_regular_file(path)?;
    let content = std::fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read OAuth connect profile {}: {error}",
            path.display()
        )
    })?;
    let profile: OAuthConnectProfile = toml::from_str(&content).map_err(|error| {
        format!(
            "failed to parse OAuth connect profile {}: {error}",
            path.display()
        )
    })?;
    if profile.version != OAUTH_PROFILE_VERSION
        || !profile.oauth_client_id.starts_with("wc_client_")
        || !profile.oauth_client_secret.starts_with("wc_csec_")
        || profile.agent_token_id.trim().is_empty()
        || profile.allowed_scopes.is_empty()
    {
        return Err(
            "existing OAuth connect profile is invalid; refusing to guess credential ownership"
                .to_string(),
        );
    }
    Ok(Some(profile))
}

fn render_oauth_profile(profile: &OAuthConnectProfile) -> Result<String, String> {
    toml::to_string(profile)
        .map_err(|error| format!("failed to render OAuth connect profile: {error}"))
}

fn validate_existing_oauth_runner(
    config: Option<&ExistingRunnerConfig>,
    server_url: &str,
) -> Result<(), String> {
    let Some(config) = config else {
        return Err("OAuth hosted profile has metadata but no Runner config".to_string());
    };
    let stored = super::super::connections::canonical_server_url(&config.server_url)
        .map_err(|_| "existing OAuth hosted profile has an invalid Server URL".to_string())?;
    if stored.url != server_url {
        return Err("selected OAuth hosted profile belongs to a different Server".to_string());
    }
    if !config.token.trim().starts_with("wc_agent_") {
        return Err(
            "OAuth hosted profile Runner credential is not a Runner transport token".to_string(),
        );
    }
    Ok(())
}

async fn list_oauth_clients(
    server_url: &str,
    opts: &ConnectOptions,
    token: &str,
) -> Result<Vec<Value>, String> {
    let value = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token,
        path: "/api/oauth/clients/list",
        body: json!({}),
    })
    .await?;
    value
        .get("clients")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| "Server returned an invalid OAuth client list".to_string())
}

async fn create_oauth_client(
    server_url: &str,
    opts: &ConnectOptions,
    token: &str,
    name: &str,
    redirect_uri: &str,
    scopes: &[String],
) -> Result<(String, String), String> {
    let value = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token,
        path: "/api/oauth/clients/create",
        body: json!({
            "name": name,
            "redirect_uris": [redirect_uri],
            "allowed_scopes": scopes,
        }),
    })
    .await?;
    let client_id = value
        .pointer("/client/client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth client create response omitted client_id".to_string())?
        .to_string();
    let client_secret = value
        .get("client_secret")
        .and_then(Value::as_str)
        .ok_or_else(|| "OAuth client create response omitted client_secret".to_string())?
        .to_string();
    Ok((client_id, client_secret))
}

async fn create_runner_token(
    server_url: &str,
    opts: &ConnectOptions,
    token: &str,
    username: &str,
    client_id: &str,
) -> Result<(String, String), String> {
    let value = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token,
        path: "/api/agent-tokens/create",
        body: json!({
            "username": username,
            "client_id": client_id,
            "name": format!("webcodex connect oauth {client_id}"),
        }),
    })
    .await?;
    let runner_token = value
        .get("token")
        .and_then(Value::as_str)
        .ok_or_else(|| "Runner transport token create response omitted token".to_string())?
        .to_string();
    let token_id = value
        .get("token_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "Runner transport token create response omitted token_id".to_string())?
        .to_string();
    Ok((runner_token, token_id))
}

async fn revoke_oauth_client(
    server_url: &str,
    opts: &ConnectOptions,
    token: &str,
    client_id: &str,
) {
    let _ = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token,
        path: "/api/oauth/clients/revoke",
        body: json!({"client_id": client_id}),
    })
    .await;
}

async fn revoke_runner_token(
    server_url: &str,
    opts: &ConnectOptions,
    token: &str,
    username: &str,
    token_id: &str,
) {
    let _ = post_json_authed(ApiCall {
        server_url,
        server_http: &opts.server_http,
        token,
        path: "/api/agent-tokens/revoke",
        body: json!({"username": username, "token_id": token_id}),
    })
    .await;
}

fn exact_string_array(value: Option<&Value>) -> Option<Vec<String>> {
    value?
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_string))
        .collect()
}

async fn ensure_oauth_client(
    server_url: &str,
    opts: &ConnectOptions,
    user_token: &str,
    profile_name: &str,
    profile: &mut OAuthConnectProfile,
) -> Result<bool, String> {
    let clients = list_oauth_clients(server_url, opts, user_token).await?;
    if let Some(remote) = clients.iter().find(|client| {
        client.get("client_id").and_then(Value::as_str) == Some(&profile.oauth_client_id)
    }) {
        let revoked_at = remote
            .get("revoked_at")
            .ok_or_else(|| "Server returned malformed OAuth revoked_at state".to_string())?;
        if revoked_at.is_null() {
            let redirects = exact_string_array(remote.get("redirect_uris"))
                .ok_or_else(|| "Server returned malformed OAuth redirect_uris".to_string())?;
            let scopes = exact_string_array(remote.get("allowed_scopes"))
                .ok_or_else(|| "Server returned malformed OAuth allowed_scopes".to_string())?;
            if redirects != vec![profile.oauth_redirect_uri.clone()]
                || scopes != profile.allowed_scopes
            {
                return Err(
                    "persisted OAuth client differs from the remote Server record; refusing to widen or rewrite it implicitly"
                        .to_string(),
                );
            }
            return Ok(false);
        }
        // A revoked client cannot authenticate again. Fall through to create a
        // replacement; the caller atomically publishes the updated protected
        // local profile only after creation succeeds.
    }

    let (client_id, client_secret) = create_oauth_client(
        server_url,
        opts,
        user_token,
        &format!("WebCodex connect {profile_name}"),
        &profile.oauth_redirect_uri,
        &profile.allowed_scopes,
    )
    .await?;
    profile.oauth_client_id = client_id;
    profile.oauth_client_secret = client_secret;
    Ok(true)
}

fn render_oauth_output(
    server_url: &str,
    profile: &str,
    client_id: &str,
    runtime_project_id: &str,
    config_path: &Path,
    log_path: &Path,
    oauth_profile_path: &Path,
    oauth: &OAuthConnectProfile,
    metadata: &OAuthServerMetadata,
    disclose_client_secret: bool,
) -> String {
    let secret_line = if disclose_client_secret {
        format!("Client secret: {}\n", oauth.oauth_client_secret)
    } else {
        format!(
            "Credential source: protected OAuth client profile at {} (client secret not reprinted).\n",
            oauth_profile_path.display()
        )
    };
    format!(
        "WebCodex connected\n\nWhat to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. MCP URL: {server_url}/mcp\n3. Authentication: OAuth 2.0 Authorization Code + PKCE S256\n4. Client ID: {}\n5. {secret_line}6. Redirect URI: {}\n7. Scan Tools and complete browser authorization.\n8. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\nDetails\nServer:          {server_url}\nRunner:          running\nProfile:         {profile}\nClient:          {client_id}\nRuntime project: {runtime_project_id}\nConfig:          {}\nLogs:            {}\nIssuer:          {}\nAuthorization:   {}\nToken endpoint:  {}\nScopes:          {} offline_access\n\nThe OAuth client is managed-user-owned on the remote Server. The Runner uses a separate Runner token and OAuth credentials are never sent to Runner transport.\n",
        oauth.oauth_client_id,
        oauth.oauth_redirect_uri,
        config_path.display(),
        log_path.display(),
        metadata.issuer,
        metadata.authorization_endpoint,
        metadata.token_endpoint,
        oauth.allowed_scopes.join(" "),
    )
}

fn oauth_secret_disclosure_marker(profile_dir: &Path, client_id: &str) -> PathBuf {
    let digest = Sha256::digest(client_id.as_bytes());
    profile_dir.join(format!("{OAUTH_SECRET_DISCLOSED_FILE_PREFIX}{digest:x}"))
}

fn build_oauth_connect_result(
    server_url: &str,
    profile: &str,
    client_id: &str,
    runtime_project_id: &str,
    config_path: &Path,
    log_path: &Path,
    profile_dir: &Path,
    oauth: &OAuthConnectProfile,
    metadata: &OAuthServerMetadata,
) -> ConnectResult {
    let disclosure_marker = oauth_secret_disclosure_marker(profile_dir, &oauth.oauth_client_id);
    let disclose_client_secret = !disclosure_marker.is_file();
    ConnectResult {
        output: render_oauth_output(
            server_url,
            profile,
            client_id,
            runtime_project_id,
            config_path,
            log_path,
            &profile_dir.join(OAUTH_PROFILE_FILE),
            oauth,
            metadata,
            disclose_client_secret,
        ),
        disclosure_markers: disclose_client_secret
            .then_some(disclosure_marker)
            .into_iter()
            .collect(),
    }
}

pub(super) async fn run_oauth_connect(opts: ConnectOptions) -> Result<ConnectResult, String> {
    let canonical_server = super::super::connections::canonical_server_url(&opts.server_url)?;
    let canonical_project = opts.project.canonicalize().map_err(|error| {
        format!(
            "project path {} does not exist or cannot be resolved: {error}",
            opts.project.display()
        )
    })?;
    if !canonical_project.is_dir() {
        return Err(format!(
            "project path {} is not a directory",
            canonical_project.display()
        ));
    }
    let redirect_uri =
        validate_redirect_uri(opts.oauth_redirect_uri.as_deref().ok_or_else(|| {
            "--auth managed-oauth requires --oauth-redirect-uri <URL>".to_string()
        })?)?;
    let raw_config_base = opts
        .config_base
        .clone()
        .map(Ok)
        .unwrap_or_else(default_client_base_dir)?;
    let identity = read_managed_identity(
        &raw_config_base,
        &canonical_server.url,
        opts.username.as_deref(),
    )?;
    let metadata = fetch_oauth_metadata(&opts, &canonical_server.url).await?;

    let explicit_profile = opts
        .profile
        .as_deref()
        .map(validate_client_profile)
        .transpose()?;
    let profile = explicit_profile.unwrap_or_else(|| {
        derived_oauth_profile(
            &canonical_server.url,
            &identity.connection.username,
            &redirect_uri,
        )
    });
    let profile = validate_client_profile(&profile)?;
    let config_base = ensure_real_directory_tree(&raw_config_base)?;
    let state_base = ensure_real_directory_tree(
        &opts
            .state_base
            .clone()
            .map(Ok)
            .unwrap_or_else(default_client_state_base_dir)?,
    )?;
    let profile_dir =
        ensure_private_directory(&client_output_dir_for_profile(&config_base, &profile))?;
    let state_dir = ensure_private_directory(&client_state_dir_for_profile(&state_base, &profile))?;
    let _lock = ProfileLock::acquire(&state_dir)?;
    let config_path = webcodex_runner_config::paths::resolve_runner_config_path(&profile_dir)?;
    let oauth_path = profile_dir.join(OAUTH_PROFILE_FILE);
    let project_registry_dir =
        webcodex_runner_config::paths::select_project_registry_dir(&profile_dir)?;
    let project_registry_dir = ensure_private_directory(&project_registry_dir)?;
    let existing_config = read_existing_runner_config(&config_path)?;
    let existing_oauth = read_oauth_profile(&oauth_path)?;
    let previous_oauth_bytes = if oauth_path.exists() {
        Some(std::fs::read(&oauth_path).map_err(|error| {
            format!("failed to preserve existing OAuth connect profile: {error}")
        })?)
    } else {
        None
    };
    if existing_config.is_some() != existing_oauth.is_some() {
        return Err(
            "OAuth hosted profile is incomplete; refusing to guess which remote credentials it owns"
                .to_string(),
        );
    }
    let existing_summary = local_runner_state_summary(&state_dir)?;
    let client_id = match (&opts.client_id, existing_config.as_ref()) {
        (Some(requested), Some(existing)) => {
            let requested = super::super::login::validate_client_id(requested)?;
            if requested != existing.client_id && existing_summary.running {
                return Err(
                    "--client-id differs from the active OAuth profile; stop that Runner before changing its identity"
                        .to_string(),
                );
            }
            if requested != existing.client_id {
                return Err(
                    "--client-id differs from the persisted OAuth profile; use another --profile or remove the old profile explicitly"
                        .to_string(),
                );
            }
            requested
        }
        (Some(requested), None) => super::super::login::validate_client_id(requested)?,
        (None, Some(existing)) => super::super::login::validate_client_id(&existing.client_id)?,
        (None, None) => generated_client_id(&canonical_server.url),
    };

    let (project_path, project, already_registered) = resolve_project(
        &project_registry_dir,
        &canonical_project,
        opts.project_id.as_deref(),
    )?;
    let runtime_project_id = format!("agent:{client_id}:{}", project.id);
    let runner_bin = opts
        .runner_bin
        .clone()
        .or_else(|| discover_internal_binary("webcodex-runner"))
        .ok_or_else(|| {
            "webcodex-runner was not found beside webcodex or in an absolute PATH entry".to_string()
        })?;

    let (runner_token, oauth_profile, created_runner_token, created_oauth) = if let Some(
        mut oauth_profile,
    ) = existing_oauth
    {
        validate_existing_oauth_runner(existing_config.as_ref(), &canonical_server.url)?;
        if oauth_profile.server_url != canonical_server.url
            || !oauth_profile
                .username
                .eq_ignore_ascii_case(&identity.connection.username)
            || oauth_profile.oauth_redirect_uri != redirect_uri
        {
            return Err(
                "selected OAuth hosted profile belongs to a different login or redirect URI"
                    .to_string(),
            );
        }
        if oauth_profile
            .allowed_scopes
            .iter()
            .any(|scope| !metadata.scopes_supported.contains(scope))
        {
            return Err(
                "the persisted OAuth client contains a scope outside the hosted-connect allow-list or no longer supported by the remote Server"
                    .to_string(),
            );
        }
        let created_oauth = ensure_oauth_client(
            &canonical_server.url,
            &opts,
            &identity.user_token,
            &profile,
            &mut oauth_profile,
        )
        .await?;
        let runner_token = existing_config
            .as_ref()
            .expect("validated existing OAuth config")
            .token
            .trim()
            .to_string();
        (runner_token, oauth_profile, false, created_oauth)
    } else {
        let (runner_token, agent_token_id) = create_runner_token(
            &canonical_server.url,
            &opts,
            &identity.user_token,
            &identity.connection.username,
            &client_id,
        )
        .await?;
        let scopes = metadata.scopes_supported.clone();
        let (oauth_client_id, oauth_client_secret) = match create_oauth_client(
            &canonical_server.url,
            &opts,
            &identity.user_token,
            &format!("WebCodex connect {profile}"),
            &redirect_uri,
            &scopes,
        )
        .await
        {
            Ok(client) => client,
            Err(error) => {
                revoke_runner_token(
                    &canonical_server.url,
                    &opts,
                    &identity.user_token,
                    &identity.connection.username,
                    &agent_token_id,
                )
                .await;
                return Err(error);
            }
        };
        (
            runner_token,
            OAuthConnectProfile {
                version: OAUTH_PROFILE_VERSION,
                server_url: canonical_server.url.clone(),
                username: identity.connection.username.clone(),
                oauth_client_id,
                oauth_client_secret,
                oauth_redirect_uri: redirect_uri.clone(),
                allowed_scopes: scopes,
                agent_token_id,
            },
            true,
            true,
        )
    };

    let runner_content = match render_runner_document(
        &config_path,
        &canonical_server.url,
        &runner_token,
        &client_id,
        &project_registry_dir,
        &canonical_project,
    ) {
        Ok(content) => content,
        Err(error) => {
            if created_oauth {
                revoke_oauth_client(
                    &canonical_server.url,
                    &opts,
                    &identity.user_token,
                    &oauth_profile.oauth_client_id,
                )
                .await;
            }
            if created_runner_token {
                revoke_runner_token(
                    &canonical_server.url,
                    &opts,
                    &identity.user_token,
                    &identity.connection.username,
                    &oauth_profile.agent_token_id,
                )
                .await;
            }
            return Err(error);
        }
    };
    let oauth_content = match render_oauth_profile(&oauth_profile) {
        Ok(content) => content,
        Err(error) => {
            if created_oauth {
                revoke_oauth_client(
                    &canonical_server.url,
                    &opts,
                    &identity.user_token,
                    &oauth_profile.oauth_client_id,
                )
                .await;
            }
            if created_runner_token {
                revoke_runner_token(
                    &canonical_server.url,
                    &opts,
                    &identity.user_token,
                    &identity.connection.username,
                    &oauth_profile.agent_token_id,
                )
                .await;
            }
            return Err(error);
        }
    };
    if let Err(error) = atomic_write(&oauth_path, oauth_content.as_bytes(), true) {
        if created_oauth {
            revoke_oauth_client(
                &canonical_server.url,
                &opts,
                &identity.user_token,
                &oauth_profile.oauth_client_id,
            )
            .await;
        }
        if created_runner_token {
            revoke_runner_token(
                &canonical_server.url,
                &opts,
                &identity.user_token,
                &identity.connection.username,
                &oauth_profile.agent_token_id,
            )
            .await;
        }
        return Err(error);
    }
    if let Err(error) = atomic_write(&config_path, runner_content.as_bytes(), true) {
        let oauth_restore = match previous_oauth_bytes.as_deref() {
            Some(previous) => atomic_write(&oauth_path, previous, true).map(|_| ()),
            None => match std::fs::remove_file(&oauth_path) {
                Ok(()) => Ok(()),
                Err(remove_error) if remove_error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(remove_error) => Err(format!(
                    "failed to remove newly written OAuth connect profile: {remove_error}"
                )),
            },
        };
        if created_oauth {
            revoke_oauth_client(
                &canonical_server.url,
                &opts,
                &identity.user_token,
                &oauth_profile.oauth_client_id,
            )
            .await;
        }
        if created_runner_token {
            revoke_runner_token(
                &canonical_server.url,
                &opts,
                &identity.user_token,
                &identity.connection.username,
                &oauth_profile.agent_token_id,
            )
            .await;
        }
        return match oauth_restore {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!(
                "{error}; additionally failed to restore OAuth connect profile: {restore_error}"
            )),
        };
    }
    atomic_write(
        &local_runner_profile_marker(&state_dir),
        format!("profile = {profile:?}\n").as_bytes(),
        false,
    )?;
    let project_changed = if already_registered {
        false
    } else {
        atomic_write(
            &project_path,
            render_project_file(&project)?.as_bytes(),
            false,
        )?
    };
    if project_changed
        && load_runner_state(&state_dir)?
            .as_ref()
            .is_some_and(process_matches)
    {
        stop_runner_unlocked(&state_dir)?;
    }
    let log_path = local_runner_log_path(&state_dir);
    let start = ensure_runner_unlocked(&runner_bin, &config_path, &state_dir)
        .map_err(|error| format!("{error}. Runner logs: {}", log_path.display()))?;
    if let Err(error) = wait_for_connection(
        &canonical_server.url,
        &opts.server_http,
        &identity.user_token,
        &client_id,
        &runtime_project_id,
        &state_dir,
        if opts.wait_timeout_ms == 0 {
            DEFAULT_CONNECT_WAIT_MS
        } else {
            opts.wait_timeout_ms
        },
    )
    .await
    {
        if start == RunnerStart::Started {
            let _ = stop_runner_unlocked(&state_dir);
        }
        return Err(format!("{error}. Runner logs: {}", log_path.display()));
    }

    Ok(build_oauth_connect_result(
        &canonical_server.url,
        &profile,
        &client_id,
        &runtime_project_id,
        &config_path,
        &log_path,
        &profile_dir,
        &oauth_profile,
        &metadata,
    ))
}

pub(super) fn observer_token_for_disconnect(
    profile_dir: &Path,
    config_base: &Path,
    config: &ExistingRunnerConfig,
) -> Result<Option<String>, String> {
    let Some(profile) = read_oauth_profile(&profile_dir.join(OAUTH_PROFILE_FILE))? else {
        return Ok(None);
    };
    let configured_server = super::super::connections::canonical_server_url(&config.server_url)?;
    if profile.server_url != configured_server.url {
        return Err(
            "OAuth hosted profile metadata does not match Runner config Server identity"
                .to_string(),
        );
    }
    let mut connections = connections_for_server(config_base, &profile.server_url)
        .into_iter()
        .filter(|connection| connection.username.eq_ignore_ascii_case(&profile.username))
        .collect::<Vec<_>>();
    if connections.len() != 1 {
        return Err(format!(
            "OAuth hosted profile requires the managed login for user {}; run `webcodex login` again before disconnecting the live project",
            profile.username
        ));
    }
    let connection = connections.remove(0);
    validate_existing_regular_file(&connection.paths.user_token)?;
    let token = std::fs::read_to_string(&connection.paths.user_token)
        .map_err(|error| {
            format!("failed to read managed user token for OAuth disconnect: {error}")
        })?
        .trim()
        .to_string();
    validate_user_api_token(&token)?;
    if !token.starts_with("wc_pat_") {
        return Err("OAuth hosted profile managed login no longer contains a user PAT".to_string());
    }
    Ok(Some(token))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use webcodex_admin::ServerHttpOptions;

    fn options(server_url: String) -> ConnectOptions {
        ConnectOptions {
            server_url,
            server_http: ServerHttpOptions {
                proxy: None,
                no_system_proxy: true,
            },
            key: None,
            key_file: None,
            auth: super::super::ConnectAuth::ManagedOAuth,
            oauth_redirect_uri: Some("https://client.example/callback".to_string()),
            oauth_computer_permissions: false,
            oauth_local_mcp: false,
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

    fn one_json_response(body: Value) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![0u8; 32 * 1024];
            let read = stream.read(&mut request).unwrap();
            assert!(read > 0);
            let payload = body.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            )
            .unwrap();
        });
        (format!("http://{address}"), handle)
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

    fn test_oauth_profile(client_id: &str, client_secret: &str) -> OAuthConnectProfile {
        OAuthConnectProfile {
            version: OAUTH_PROFILE_VERSION,
            server_url: "https://example.test".to_string(),
            username: "alice".to_string(),
            oauth_client_id: client_id.to_string(),
            oauth_client_secret: client_secret.to_string(),
            oauth_redirect_uri: "https://client.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            agent_token_id: "agent-token-id".to_string(),
        }
    }

    fn test_oauth_metadata() -> OAuthServerMetadata {
        OAuthServerMetadata {
            issuer: "https://example.test".to_string(),
            authorization_endpoint: "https://example.test/oauth/authorize".to_string(),
            token_endpoint: "https://example.test/oauth/token".to_string(),
            scopes_supported: vec!["runtime:read".to_string()],
        }
    }

    fn test_oauth_result(profile_dir: &Path, oauth: &OAuthConnectProfile) -> ConnectResult {
        build_oauth_connect_result(
            "https://example.test",
            "profile",
            "runner",
            "agent:runner:project",
            &profile_dir.join("runner.toml"),
            &profile_dir.join("runner.log"),
            profile_dir,
            oauth,
            &test_oauth_metadata(),
        )
    }

    struct FailingOutput {
        bytes: Vec<u8>,
        fail_write: bool,
        fail_flush: bool,
    }

    impl FailingOutput {
        fn write_failure() -> Self {
            Self {
                bytes: Vec::new(),
                fail_write: true,
                fail_flush: false,
            }
        }

        fn flush_failure() -> Self {
            Self {
                bytes: Vec::new(),
                fail_write: false,
                fail_flush: true,
            }
        }
    }

    impl Write for FailingOutput {
        fn write(&mut self, content: &[u8]) -> std::io::Result<usize> {
            if self.fail_write {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected OAuth output write failure",
                ));
            }
            self.bytes.extend_from_slice(content);
            Ok(content.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            if self.fail_flush {
                Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "injected OAuth output flush failure",
                ))
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn persisted_oauth_secret_remains_pending_after_later_connect_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let profile_dir = tmp.path();
        let oauth = test_oauth_profile("wc_client_created", "wc_csec_created");

        // The credential profile is committed before Runner startup/wait. A
        // later connect failure produces no stdout and therefore no disclosure
        // marker; the next attempt must still reveal the persisted secret.
        std::fs::write(
            profile_dir.join(OAUTH_PROFILE_FILE),
            render_oauth_profile(&oauth).unwrap(),
        )
        .unwrap();
        let retry = test_oauth_result(profile_dir, &oauth);
        assert!(retry.output.contains("Client secret: wc_csec_created"));
        let marker = oauth_secret_disclosure_marker(profile_dir, &oauth.oauth_client_id);
        assert_eq!(retry.disclosure_markers, vec![marker.clone()]);
        assert!(!marker.exists());
    }

    #[test]
    fn oauth_stdout_failure_leaves_secret_pending_for_retry() {
        for mut stdout in [
            FailingOutput::write_failure(),
            FailingOutput::flush_failure(),
        ] {
            let tmp = tempfile::tempdir().unwrap();
            let profile_dir = tmp.path();
            let oauth = test_oauth_profile("wc_client_pending", "wc_csec_pending");
            let marker = oauth_secret_disclosure_marker(profile_dir, &oauth.oauth_client_id);
            let result = test_oauth_result(profile_dir, &oauth);
            assert!(result.output.contains("Client secret: wc_csec_pending"));

            let error = super::super::write_connect_result(result, &mut stdout, &mut Vec::new())
                .unwrap_err();
            assert!(error.contains("connect output"));
            assert!(!marker.exists());

            let retry = test_oauth_result(profile_dir, &oauth);
            assert!(retry.output.contains("Client secret: wc_csec_pending"));
            assert_eq!(retry.disclosure_markers.len(), 1);
        }
    }

    #[test]
    fn successful_oauth_secret_disclosure_hides_secret_on_reconnect() {
        let tmp = tempfile::tempdir().unwrap();
        let profile_dir = tmp.path();
        let oauth = test_oauth_profile("wc_client_disclosed", "wc_csec_disclosed");
        let marker = oauth_secret_disclosure_marker(profile_dir, &oauth.oauth_client_id);
        let result = test_oauth_result(profile_dir, &oauth);
        assert!(result.output.contains("Client secret: wc_csec_disclosed"));

        super::super::write_connect_result(result, &mut Vec::new(), &mut Vec::new()).unwrap();
        assert_eq!(
            std::fs::read_to_string(&marker).unwrap(),
            "disclosed = true\n"
        );
        assert!(!std::fs::read_to_string(&marker)
            .unwrap()
            .contains("wc_csec_disclosed"));

        let reconnect = test_oauth_result(profile_dir, &oauth);
        assert!(!reconnect.output.contains("wc_csec_disclosed"));
        assert!(!reconnect.output.contains("Client secret:"));
        assert!(reconnect.disclosure_markers.is_empty());
    }

    #[test]
    fn oauth_client_rotation_invalidates_previous_disclosure_state() {
        let tmp = tempfile::tempdir().unwrap();
        let profile_dir = tmp.path();
        let old = test_oauth_profile("wc_client_old", "wc_csec_old");
        let old_marker = oauth_secret_disclosure_marker(profile_dir, &old.oauth_client_id);
        super::super::write_connect_result(
            test_oauth_result(profile_dir, &old),
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .unwrap();
        assert!(old_marker.is_file());

        let rotated = test_oauth_profile("wc_client_rotated", "wc_csec_rotated");
        let new_marker = oauth_secret_disclosure_marker(profile_dir, &rotated.oauth_client_id);
        assert_ne!(old_marker, new_marker);
        assert!(!new_marker.exists());
        let result = test_oauth_result(profile_dir, &rotated);
        assert!(result.output.contains("Client secret: wc_csec_rotated"));
        assert_eq!(result.disclosure_markers, vec![new_marker]);
    }

    #[tokio::test]
    async fn oauth_discovery_uses_closed_hosted_connect_scope_set() {
        let (server, handle) = one_json_response(json!({
            "issuer": "https://webcodex.example",
            "authorization_endpoint": "https://webcodex.example/oauth/authorize",
            "token_endpoint": "https://webcodex.example/oauth/token",
            "scopes_supported": [
                "runtime:read",
                "job:detach",
                "computer:launch",
                "computer:pointer_control",
                "account:manage",
                "computer:future_sensitive",
                "agent:poll",
                "admin",
                "offline_access"
            ]
        }));
        let metadata = fetch_oauth_metadata(&options(server.clone()), &server)
            .await
            .unwrap();
        assert_eq!(
            metadata.scopes_supported,
            vec![
                "runtime:read",
                "job:detach",
                "computer:launch",
                "computer:pointer_control"
            ]
        );
        assert!(!metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "account:manage"));
        assert!(!metadata
            .scopes_supported
            .iter()
            .any(|scope| scope == "computer:future_sensitive"));
        handle.join().unwrap();
    }

    #[test]
    fn oauth_redirect_uri_rejects_userinfo_and_fragments() {
        for uri in [
            "https://alice@example.test/callback",
            "https://example.test/callback#fragment",
        ] {
            assert!(validate_redirect_uri(uri).is_err(), "{uri}");
        }
    }

    #[test]
    fn managed_identity_requires_user_selection_when_multiple_logins_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path();
        let server =
            super::super::super::connections::canonical_server_url("https://example.test").unwrap();
        for user in ["alice", "bob"] {
            let dir = base.join(&server.slug).join(user);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("server.toml"),
                super::super::super::connections::descriptor_toml(
                    &server.url,
                    user,
                    "device",
                    "2026-08-20T00:00:00Z",
                ),
            )
            .unwrap();
            std::fs::write(dir.join("webcodex-user-token"), format!("wc_pat_{user}\n")).unwrap();
        }
        let error = read_managed_identity(base, &server.url, None).unwrap_err();
        assert!(error.contains("more than one logged-in user"), "{error}");
        let alice = read_managed_identity(base, &server.url, Some("alice")).unwrap();
        assert_eq!(alice.connection.username, "alice");
        assert_eq!(alice.user_token, "wc_pat_alice");
    }

    #[test]
    fn oauth_disconnect_uses_managed_pat_instead_of_runner_transport_token() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("config");
        let profile_dir = base.join("clients/oauth-profile");
        std::fs::create_dir_all(&profile_dir).unwrap();
        let server =
            super::super::super::connections::canonical_server_url("https://example.test").unwrap();
        let login_dir = base.join(&server.slug).join("alice");
        std::fs::create_dir_all(&login_dir).unwrap();
        std::fs::write(
            login_dir.join("server.toml"),
            super::super::super::connections::descriptor_toml(
                &server.url,
                "alice",
                "device",
                "2026-08-20T00:00:00Z",
            ),
        )
        .unwrap();
        std::fs::write(login_dir.join("webcodex-user-token"), "wc_pat_managed\n").unwrap();
        let oauth = OAuthConnectProfile {
            version: OAUTH_PROFILE_VERSION,
            server_url: server.url.clone(),
            username: "alice".to_string(),
            oauth_client_id: "wc_client_existing".to_string(),
            oauth_client_secret: "wc_csec_existing".to_string(),
            oauth_redirect_uri: "https://client.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            agent_token_id: "agent-token-id".to_string(),
        };
        std::fs::write(
            profile_dir.join(OAUTH_PROFILE_FILE),
            render_oauth_profile(&oauth).unwrap(),
        )
        .unwrap();
        let config = ExistingRunnerConfig {
            server_url: server.url,
            token: "wc_agent_runner-only".to_string(),
            client_id: "runner".to_string(),
        };
        assert_eq!(
            observer_token_for_disconnect(&profile_dir, &base, &config)
                .unwrap()
                .as_deref(),
            Some("wc_pat_managed")
        );
    }

    #[tokio::test]
    async fn revoked_persisted_oauth_client_rotates_to_new_credentials() {
        let (server, handle) = json_responses(vec![
            json!({
                "success": true,
                "clients": [{
                    "client_id": "wc_client_revoked",
                    "name": "Revoked",
                    "redirect_uris": ["https://client.example/callback"],
                    "allowed_scopes": ["runtime:read"],
                    "created_at": 1,
                    "revoked_at": 2
                }]
            }),
            json!({
                "success": true,
                "client": {"client_id": "wc_client_rotated"},
                "client_secret": "wc_csec_rotated"
            }),
        ]);
        let opts = options(server.clone());
        let mut profile = OAuthConnectProfile {
            version: OAUTH_PROFILE_VERSION,
            server_url: server.clone(),
            username: "alice".to_string(),
            oauth_client_id: "wc_client_revoked".to_string(),
            oauth_client_secret: "wc_csec_old".to_string(),
            oauth_redirect_uri: "https://client.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            agent_token_id: "agent-token-id".to_string(),
        };
        let created = ensure_oauth_client(&server, &opts, "wc_pat_alice", "profile", &mut profile)
            .await
            .unwrap();
        assert!(created);
        assert_eq!(profile.oauth_client_id, "wc_client_rotated");
        assert_eq!(profile.oauth_client_secret, "wc_csec_rotated");
        assert_eq!(profile.allowed_scopes, vec!["runtime:read"]);
        handle.join().unwrap();
    }

    #[tokio::test]
    async fn persisted_oauth_client_is_reused_without_implicit_scope_widening() {
        let (server, handle) = one_json_response(json!({
            "success": true,
            "clients": [{
                "client_id": "wc_client_existing",
                "name": "Existing",
                "redirect_uris": ["https://client.example/callback"],
                "allowed_scopes": ["runtime:read"],
                "created_at": 1,
                "revoked_at": null
            }]
        }));
        let opts = options(server.clone());
        let mut profile = OAuthConnectProfile {
            version: OAUTH_PROFILE_VERSION,
            server_url: server.clone(),
            username: "alice".to_string(),
            oauth_client_id: "wc_client_existing".to_string(),
            oauth_client_secret: "wc_csec_existing".to_string(),
            oauth_redirect_uri: "https://client.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            agent_token_id: "agent-token-id".to_string(),
        };
        let created = ensure_oauth_client(&server, &opts, "wc_pat_alice", "profile", &mut profile)
            .await
            .unwrap();
        assert!(!created);
        assert_eq!(profile.allowed_scopes, vec!["runtime:read"]);
        handle.join().unwrap();
    }

    #[test]
    fn oauth_output_discloses_client_secret_only_for_new_or_rotated_client() {
        let oauth = OAuthConnectProfile {
            version: OAUTH_PROFILE_VERSION,
            server_url: "https://example.test".to_string(),
            username: "alice".to_string(),
            oauth_client_id: "wc_client_existing".to_string(),
            oauth_client_secret: "wc_csec_existing".to_string(),
            oauth_redirect_uri: "https://client.example/callback".to_string(),
            allowed_scopes: vec!["runtime:read".to_string()],
            agent_token_id: "agent-token-id".to_string(),
        };
        let metadata = OAuthServerMetadata {
            issuer: "https://example.test".to_string(),
            authorization_endpoint: "https://example.test/oauth/authorize".to_string(),
            token_endpoint: "https://example.test/oauth/token".to_string(),
            scopes_supported: vec!["runtime:read".to_string()],
        };
        let reused = render_oauth_output(
            "https://example.test",
            "profile",
            "runner",
            "agent:runner:project",
            Path::new("runner.toml"),
            Path::new("runner.log"),
            Path::new("/protected/profile/oauth-client.toml"),
            &oauth,
            &metadata,
            false,
        );
        assert!(!reused.contains("wc_csec_existing"));
        assert!(!reused.contains("Client secret:"));
        assert!(reused.starts_with("WebCodex connected\n\nWhat to do next"));
        assert!(reused.find("MCP URL:").unwrap() < reused.find("Details").unwrap());
        assert!(reused.contains("Credential source:"));
        assert!(reused.contains("/protected/profile/oauth-client.toml"));
        assert!(reused.contains("not reprinted"));

        let created = render_oauth_output(
            "https://example.test",
            "profile",
            "runner",
            "agent:runner:project",
            Path::new("runner.toml"),
            Path::new("runner.log"),
            Path::new("/protected/profile/oauth-client.toml"),
            &oauth,
            &metadata,
            true,
        );
        assert!(created.contains("Client secret: wc_csec_existing"));
        assert_eq!(created.matches("wc_csec_existing").count(), 1);
        assert!(created.find("Client secret:").unwrap() < created.find("Details").unwrap());
    }
}
