use super::client_handoff_service::{
    copy_mcp_url, maybe_spawn_chatgpt_open_prompt, mcp_url, render_clipboard_status,
    ClipboardCopyOutcome,
};
use super::setup_service::{
    create_private_dir, generate_project_credential, read_private_value, read_project_credential,
    write_new_private, ProjectConfig, ProjectPaths,
};
use super::{
    configured_project, ensure_local_runtime_port_available, parse_options,
    remove_npm_wrapper_network_environment, setup, start_local_runtime, LocalRuntimeOptions,
    ProductError, ProjectCommandOptions, ProjectShareOAuthRuntimeOptions,
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const TUNNEL_START_TIMEOUT: Duration = Duration::from_secs(20);
const TUNNEL_LOG_LINES: usize = 8;
const TUNNEL_LOG_LINE_BYTES: usize = 512;
const TUNNEL_LOG_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TunnelProvider {
    CloudflareQuick,
    OpenAiSecure,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShareAuth {
    Bearer,
    QueryToken,
    OAuth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShareCommandOptions {
    pub(crate) project: ProjectCommandOptions,
    pub(crate) tunnel: TunnelProvider,
    pub(crate) auth: ShareAuth,
    pub(crate) oauth_redirect_uri: Option<String>,
    pub(crate) public_url: Option<String>,
    pub(crate) copy_url: bool,
}

pub(crate) fn parse_share_options(args: &[String]) -> Result<ShareCommandOptions, String> {
    let mut tunnel = TunnelProvider::CloudflareQuick;
    let mut auth = ShareAuth::Bearer;
    let mut oauth_redirect_uri = None;
    let mut public_url = None;
    let mut copy_url = true;
    let mut project_args = Vec::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--tunnel" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--tunnel requires a value".to_string())?;
                tunnel = match value.as_str() {
                    "cloudflare" => TunnelProvider::CloudflareQuick,
                    "openai" => TunnelProvider::OpenAiSecure,
                    "none" => TunnelProvider::None,
                    _ => {
                        return Err(format!(
                        "unknown tunnel provider '{value}'; expected cloudflare, openai, or none"
                    ))
                    }
                };
            }
            "--auth" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--auth requires a value".to_string())?;
                auth = match value.as_str() {
                    "bearer" => ShareAuth::Bearer,
                    "query-token" => ShareAuth::QueryToken,
                    "oauth" => ShareAuth::OAuth,
                    _ => {
                        return Err(format!(
                            "unknown share auth '{value}'; expected bearer, query-token, or oauth"
                        ))
                    }
                };
            }
            "--oauth-redirect-uri" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--oauth-redirect-uri requires a value".to_string())?;
                if oauth_redirect_uri
                    .replace(value.trim().to_string())
                    .is_some()
                {
                    return Err("--oauth-redirect-uri may be specified only once".to_string());
                }
            }
            "--public-url" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--public-url requires a value".to_string())?;
                if public_url
                    .replace(validate_share_public_url(value)?)
                    .is_some()
                {
                    return Err("--public-url may be specified only once".to_string());
                }
            }
            "--no-copy-url" => copy_url = false,
            flag => project_args.push(flag.to_string()),
        }
        index += 1;
    }
    if tunnel != TunnelProvider::None && public_url.is_some() {
        return Err("--public-url requires --tunnel none; managed tunnel providers own their transport endpoint".to_string());
    }
    if tunnel == TunnelProvider::OpenAiSecure && auth != ShareAuth::Bearer {
        return Err("--tunnel openai currently requires --auth bearer; WebCodex keeps that temporary Bearer credential local and tunnel-client injects it into the private MCP hop".to_string());
    }
    match auth {
        ShareAuth::OAuth if oauth_redirect_uri.as_deref().is_none_or(str::is_empty) => {
            return Err("--auth oauth requires --oauth-redirect-uri <URL>".to_string());
        }
        ShareAuth::Bearer | ShareAuth::QueryToken if oauth_redirect_uri.is_some() => {
            return Err("--oauth-redirect-uri requires --auth oauth".to_string());
        }
        _ => {}
    }
    if let Some(redirect_uri) = oauth_redirect_uri.as_deref() {
        crate::oauth_http::validate_redirect_uri(redirect_uri)?;
    }
    let project = parse_options(&project_args, "share")?;
    Ok(ShareCommandOptions {
        project,
        tunnel,
        auth,
        oauth_redirect_uri,
        public_url,
        copy_url,
    })
}

fn mcp_query_token_url(public_url: &str, credential: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("token", credential);
    format!("{}?{}", mcp_url(public_url), query.finish())
}

fn validate_share_public_url(value: &str) -> Result<String, String> {
    let value = value.trim();
    let parsed =
        url::Url::parse(value).map_err(|_| "--public-url must be an absolute URL".to_string())?;
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err("--public-url must not contain credentials, query, or fragment".to_string());
    }
    if !matches!(parsed.path(), "" | "/") {
        return Err("--public-url must be an origin without a path".to_string());
    }
    let host = parsed.host_str().unwrap_or("");
    match parsed.scheme() {
        "https" if !host.is_empty() => {}
        "http" if matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]") => {}
        "http" => return Err("--public-url must use https unless it is loopback".to_string()),
        _ => return Err("--public-url must use http or https and include a host".to_string()),
    }
    Ok(value.trim_end_matches('/').to_string())
}

struct ShareSession {
    directory: PathBuf,
    credential_file: PathBuf,
    credential: String,
}

impl ShareSession {
    fn create(state: &Path) -> Result<Self, ProductError> {
        let share_root = state.join("share");
        create_private_dir(&share_root)?;
        let directory = share_root.join(uuid::Uuid::new_v4().simple().to_string());
        let credential_file = directory.join("connector-key");
        let result = (|| {
            create_private_dir(&directory)?;
            let credential = generate_project_credential();
            write_new_private(&credential_file, format!("{credential}\n").as_bytes())?;
            let credential = read_project_credential(&credential_file)?;
            Ok(Self {
                directory: directory.clone(),
                credential_file,
                credential,
            })
        })();
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&directory);
        }
        result
    }

    fn write_openai_authorization_file(&self) -> Result<PathBuf, ProductError> {
        let path = self.directory.join("openai-mcp-authorization");
        write_new_private(&path, format!("Bearer {}", self.credential).as_bytes())?;
        Ok(path)
    }
}

impl Drop for ShareSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

#[derive(Debug, Clone)]
struct ShareOAuthClient {
    client_id: String,
    client_secret: String,
    redirect_uri: String,
}

fn valid_generated_oauth_value(value: &str, prefix: &str) -> bool {
    value.strip_prefix(prefix).is_some_and(|suffix| {
        suffix.len() == 64
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

fn project_share_oauth_scopes() -> String {
    crate::auth::PROJECT_SHARE_OAUTH_SCOPES.join(" ")
}

fn share_oauth_state_error(message: impl Into<String>) -> ProductError {
    ProductError::new(
        "project_registration_invalid",
        message,
        Some("Resolve the protected share OAuth state, then retry webcodex share --auth oauth."),
    )
}

fn revoke_previous_share_oauth_grants(
    db: &crate::Database,
    client_id: &str,
) -> Result<(), ProductError> {
    let now = chrono::Utc::now().timestamp();
    db.revoke_oauth_access_tokens_for_client(client_id, now)
        .and_then(|_| db.revoke_oauth_refresh_tokens_for_client(client_id, now))
        .and_then(|_| db.revoke_oauth_authorization_codes_for_client(client_id, now))
        .map_err(|_| {
            share_oauth_state_error("WebCodex could not retire previous share OAuth grants")
        })?;
    Ok(())
}

fn prepare_share_oauth_client(
    config: &ProjectConfig,
    paths: &ProjectPaths,
    redirect_uri: &str,
) -> Result<ShareOAuthClient, ProductError> {
    crate::oauth_http::validate_redirect_uri(redirect_uri).map_err(|message| {
        share_oauth_state_error(format!("invalid OAuth redirect URI: {message}"))
    })?;
    let redirect_uri = redirect_uri.trim().to_string();
    let grant_id = config.project_grant_id(paths);
    let digest = format!("{:x}", Sha256::digest(redirect_uri.as_bytes()));
    let directory = paths.state.join("share-oauth").join(&digest[..32]);
    let client_id_file = directory.join("client-id");
    let client_secret_file = directory.join("client-secret");
    let redirect_uri_file = directory.join("redirect-uri");
    let present = [
        client_id_file.is_file(),
        client_secret_file.is_file(),
        redirect_uri_file.is_file(),
    ];
    if present.iter().any(|present| *present) && !present.iter().all(|present| *present) {
        return Err(share_oauth_state_error(
            "the persisted share OAuth client state is incomplete",
        ));
    }

    let db = crate::Database::open(&paths.data.join("webcodex.db"))
        .map_err(|_| share_oauth_state_error("WebCodex could not open project OAuth state"))?;
    let scopes = project_share_oauth_scopes();

    if present.iter().all(|present| *present) {
        let client_id = read_private_value(&client_id_file)?;
        let client_secret = read_private_value(&client_secret_file)?;
        let persisted_redirect = read_private_value(&redirect_uri_file)?;
        if persisted_redirect != redirect_uri
            || !valid_generated_oauth_value(&client_id, "wc_client_")
            || !valid_generated_oauth_value(&client_secret, "wc_csec_")
        {
            return Err(share_oauth_state_error(
                "the persisted share OAuth client state is invalid",
            ));
        }
        let existing = db
            .list_oauth_clients()
            .map_err(|_| share_oauth_state_error("WebCodex could not read project OAuth clients"))?
            .into_iter()
            .find(|client| client.client_id == client_id);
        if let Some(client) = existing {
            if client.is_revoked()
                || client.owner_project_grant_id.as_deref() != Some(grant_id.as_str())
                || !client.is_project_grant_owned()
                || client.redirect_uris_vec() != vec![redirect_uri.clone()]
                || client.allowed_scopes != scopes
                || !db
                    .verify_oauth_client_secret(&client_id, &client_secret)
                    .unwrap_or(false)
            {
                return Err(share_oauth_state_error(
                    "the persisted share OAuth client no longer matches this project",
                ));
            }
        } else {
            db.insert_oauth_client(&crate::models::OAuthClientRecord {
                id: uuid::Uuid::new_v4().to_string(),
                client_id: client_id.clone(),
                client_secret_hash: crate::auth::hash_token(&client_secret),
                name: format!("{} project share", config.project_name),
                owner_user_id: None,
                owner_project_grant_id: Some(grant_id.clone()),
                owner_shared_key_hash: None,
                redirect_uris: redirect_uri.clone(),
                allowed_scopes: scopes.clone(),
                created_at: chrono::Utc::now().timestamp(),
                revoked_at: None,
            })
            .map_err(|_| {
                share_oauth_state_error("WebCodex could not restore the share OAuth client")
            })?;
        }
        revoke_previous_share_oauth_grants(&db, &client_id)?;
        return Ok(ShareOAuthClient {
            client_id,
            client_secret,
            redirect_uri,
        });
    }

    let client_id = crate::auth::generate_oauth_client_id();
    let client_secret = crate::auth::generate_oauth_client_secret();
    db.insert_oauth_client(&crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: client_id.clone(),
        client_secret_hash: crate::auth::hash_token(&client_secret),
        name: format!("{} project share", config.project_name),
        owner_user_id: None,
        owner_project_grant_id: Some(grant_id),
        owner_shared_key_hash: None,
        redirect_uris: redirect_uri.clone(),
        allowed_scopes: scopes,
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    })
    .map_err(|_| share_oauth_state_error("WebCodex could not create the share OAuth client"))?;

    let persist = (|| {
        create_private_dir(&directory)?;
        write_new_private(&client_id_file, format!("{client_id}\n").as_bytes())?;
        write_new_private(&client_secret_file, format!("{client_secret}\n").as_bytes())?;
        write_new_private(&redirect_uri_file, format!("{redirect_uri}\n").as_bytes())?;
        Ok::<(), ProductError>(())
    })();
    if let Err(error) = persist {
        let now = chrono::Utc::now().timestamp();
        let _ = db.revoke_oauth_client_by_client_id(&client_id, now);
        let _ = std::fs::remove_dir_all(&directory);
        return Err(error);
    }

    Ok(ShareOAuthClient {
        client_id,
        client_secret,
        redirect_uri,
    })
}

#[derive(Debug)]
struct CloudflareTunnel {
    child: Child,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

impl CloudflareTunnel {
    async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|_| tunnel_runtime_error())?;
        Err(ProductError::new(
            "tunnel_unavailable",
            format!("Cloudflare Quick Tunnel stopped unexpectedly ({status})"),
            Some("Check network connectivity and cloudflared, then retry webcodex share."),
        ))
    }

    async fn stop(&mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

impl Drop for CloudflareTunnel {
    fn drop(&mut self) {
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

fn tunnel_runtime_error() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "Cloudflare Quick Tunnel process could not be supervised",
        Some("Check cloudflared installation and retry webcodex share."),
    )
}

pub(crate) async fn share(options: &ShareCommandOptions) -> Result<(), ProductError> {
    // Resolve managed transport dependencies before project setup/state creation.
    let cloudflared_binary = match options.tunnel {
        TunnelProvider::CloudflareQuick => {
            Some(super::cloudflared_service::resolve_cloudflared().await?)
        }
        TunnelProvider::OpenAiSecure | TunnelProvider::None => None,
    };
    let openai_prerequisites = match options.tunnel {
        TunnelProvider::OpenAiSecure => {
            Some(super::openai_tunnel_service::prepare_openai_tunnel().await?)
        }
        TunnelProvider::CloudflareQuick | TunnelProvider::None => None,
    };

    setup(&options.project)?;
    let (config, paths) = configured_project(&options.project)?;
    ensure_local_runtime_port_available(
        config.port,
        "Stop the conflicting process, then retry webcodex share.",
    )?;
    let persistent_credential = read_project_credential(&paths.connector_key)?;
    let session = ShareSession::create(&paths.state)?;
    if session.credential == persistent_credential {
        return Err(ProductError::new(
            "project_credential_invalid",
            "temporary share credential unexpectedly matched the persistent project credential",
            Some("Retry webcodex share."),
        ));
    }

    let oauth_client = match options.auth {
        ShareAuth::Bearer | ShareAuth::QueryToken => None,
        ShareAuth::OAuth => Some(prepare_share_oauth_client(
            &config,
            &paths,
            options
                .oauth_redirect_uri
                .as_deref()
                .ok_or_else(|| share_oauth_state_error("OAuth redirect URI is missing"))?,
        )?),
    };
    let project_share_oauth = oauth_client
        .as_ref()
        .map(|_| ProjectShareOAuthRuntimeOptions {
            project_grant_id: config.project_grant_id(&paths),
            session_id: crate::auth::generate_project_share_session_id(),
        });

    let local_url = config.server_url();
    let (public_url, mut cloudflare_tunnel) = match options.tunnel {
        TunnelProvider::CloudflareQuick => {
            let binary = cloudflared_binary
                .as_deref()
                .ok_or_else(tunnel_runtime_error)?;
            let (url, tunnel) =
                start_cloudflare_quick_with_binary(binary, &local_url, TUNNEL_START_TIMEOUT)
                    .await?;
            (url, Some(tunnel))
        }
        TunnelProvider::OpenAiSecure => (local_url.clone(), None),
        TunnelProvider::None => (
            options
                .public_url
                .clone()
                .unwrap_or_else(|| local_url.clone()),
            None,
        ),
    };

    let mut runtime = start_local_runtime(
        &options.project,
        LocalRuntimeOptions {
            public_url: Some(public_url.clone()),
            connector_credential_file: Some(session.credential_file.clone()),
            mcp_query_token_auth: options.auth == ShareAuth::QueryToken,
            project_share_oauth,
            child_environment_remove: if options.tunnel == TunnelProvider::OpenAiSecure {
                vec![
                    "CONTROL_PLANE_API_KEY",
                    "CONTROL_PLANE_TUNNEL_ID",
                    "OPENAI_ADMIN_KEY",
                    "OPENAI_API_KEY",
                ]
            } else {
                Vec::new()
            },
            port_conflict_action: "Stop the conflicting process, then retry webcodex share.",
        },
    )
    .await?;

    let mut openai_tunnel = if let Some(prerequisites) = openai_prerequisites.as_ref() {
        let authorization_file = session.write_openai_authorization_file()?;
        match super::openai_tunnel_service::start_openai_tunnel(
            prerequisites,
            &mcp_url(&runtime.local_url),
            &authorization_file,
            &session.directory,
        )
        .await
        {
            Ok(tunnel) => Some(tunnel),
            Err(error) => {
                runtime.stop().await;
                return Err(error);
            }
        }
    } else {
        None
    };

    let externally_managed = options.tunnel == TunnelProvider::None && options.public_url.is_some();
    let ready = if let Some(prerequisites) = openai_prerequisites.as_ref() {
        render_openai_share_ready(&runtime.project_name, &prerequisites.tunnel_id)
    } else {
        match oauth_client.as_ref() {
            Some(oauth) => render_share_oauth_ready(
                &runtime.project_name,
                options.tunnel,
                externally_managed,
                &runtime.public_url,
                &session.credential,
                oauth,
            ),
            None => render_share_ready(
                &runtime.project_name,
                options.tunnel,
                options.auth,
                externally_managed,
                &runtime.public_url,
                &session.credential,
            ),
        }
    };
    println!("{ready}");

    let copy_remote_mcp_url =
        options.tunnel == TunnelProvider::CloudflareQuick || externally_managed;
    let open_chatgpt_handoff =
        copy_remote_mcp_url || options.tunnel == TunnelProvider::OpenAiSecure;
    let clipboard_outcome = if copy_remote_mcp_url {
        let clipboard_url = if options.auth == ShareAuth::QueryToken {
            mcp_query_token_url(&runtime.public_url, &session.credential)
        } else {
            mcp_url(&runtime.public_url)
        };
        copy_mcp_url(&clipboard_url, options.copy_url).await
    } else {
        ClipboardCopyOutcome::Disabled
    };
    let clipboard_status = if options.auth == ShareAuth::QueryToken {
        match clipboard_outcome {
            ClipboardCopyOutcome::Copied => Some(
                "Sensitive MCP URL copied to clipboard. It contains the temporary share credential.",
            ),
            ClipboardCopyOutcome::Unavailable => {
                Some("Clipboard copy unavailable; copy the sensitive MCP URL above manually.")
            }
            ClipboardCopyOutcome::Disabled => None,
        }
    } else {
        render_clipboard_status(clipboard_outcome)
    };
    if let Some(status) = clipboard_status {
        println!("\n{status}");
    }
    let handoff_task = open_chatgpt_handoff
        .then(maybe_spawn_chatgpt_open_prompt)
        .flatten();

    let outcome = match (cloudflare_tunnel.as_mut(), openai_tunnel.as_mut()) {
        (Some(tunnel), None) => tokio::select! {
            _ = tokio::signal::ctrl_c() => Ok(()),
            result = runtime.wait_for_exit() => result,
            result = tunnel.wait_for_exit() => result,
        },
        (None, Some(tunnel)) => tokio::select! {
            _ = tokio::signal::ctrl_c() => Ok(()),
            result = runtime.wait_for_exit() => result,
            result = tunnel.wait_for_exit() => result,
        },
        (None, None) => tokio::select! {
            _ = tokio::signal::ctrl_c() => Ok(()),
            result = runtime.wait_for_exit() => result,
        },
        (Some(_), Some(_)) => unreachable!("one share cannot own two tunnel providers"),
    };

    if let Some(task) = handoff_task {
        task.abort();
        let _ = task.await;
    }
    runtime.stop().await;
    if let Some(tunnel) = cloudflare_tunnel.as_mut() {
        tunnel.stop().await;
    }
    if let Some(tunnel) = openai_tunnel.as_mut() {
        tunnel.stop().await;
    }
    outcome
}

fn share_access_labels(
    tunnel: TunnelProvider,
    externally_managed: bool,
) -> (&'static str, &'static str, &'static str) {
    match (tunnel, externally_managed) {
        (TunnelProvider::CloudflareQuick, _) => (
            "Cloudflare Quick Tunnel",
            "temporary",
            "Ready for ChatGPT or another remote MCP client.",
        ),
        (TunnelProvider::OpenAiSecure, _) => (
            "OpenAI Secure MCP Tunnel",
            "private through the selected OpenAI workspace Tunnel",
            "Ready for ChatGPT through OpenAI Secure MCP Tunnel.",
        ),
        (TunnelProvider::None, true) => (
            "none (externally managed)",
            "operator managed",
            "Ready behind the configured external HTTPS proxy or tunnel.",
        ),
        (TunnelProvider::None, false) => (
            "none (local only)",
            "local only",
            "Ready for a local MCP client. No public tunnel is running.",
        ),
    }
}

fn render_openai_share_ready(project_name: &str, tunnel_id: &str) -> String {
    format!(
        "WebCodex ready\n\nWhat to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. Connection: Tunnel\n3. Tunnel: {tunnel_id}\n4. Authentication: No authentication\n5. Scan Tools.\n6. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\nReady for ChatGPT through OpenAI Secure MCP Tunnel.\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: OpenAI Secure MCP Tunnel\nPublic access: no public WebCodex endpoint; outbound-only OpenAI Tunnel transport\nWebCodex authentication: the temporary Bearer credential stays local and is injected by tunnel-client into the private MCP hop. Do not paste it into ChatGPT.\nCredential lifetime: temporary; stopping this share removes the local credential and tunnel-client process. The Platform Tunnel identity remains operator managed.\nPress Ctrl-C to stop sharing."
    )
}

fn render_share_ready(
    project_name: &str,
    tunnel: TunnelProvider,
    auth: ShareAuth,
    externally_managed: bool,
    public_url: &str,
    credential: &str,
) -> String {
    let (tunnel_name, public_access, ready_message) =
        share_access_labels(tunnel, externally_managed);
    let base = public_url.trim_end_matches('/');
    if auth == ShareAuth::QueryToken {
        let endpoint = mcp_query_token_url(public_url, credential);
        let client_step = if tunnel == TunnelProvider::None && !externally_managed {
            "1. Add this MCP endpoint to a local MCP client."
        } else {
            "1. In ChatGPT Developer Mode, create a custom MCP app."
        };
        return format!(
            "WebCodex ready\n\nWhat to do next\n{client_step}\n2. MCP URL (sensitive): {endpoint}\n3. Authentication: No authentication\n4. Scan Tools.\n5. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nCredential transport: URL query (`token=`), explicitly opted in for this temporary share only.\nSecurity: treat the entire MCP URL as a secret; query credentials may appear in client, proxy, clipboard, or access logs. Prefer `--auth bearer` when the client supports headers.\nCredential lifetime: temporary; stopping this share removes the accepted credential.\nPress Ctrl-C to stop sharing."
        );
    }
    let next_steps = match tunnel {
        TunnelProvider::CloudflareQuick if !externally_managed => format!(
            "What to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. MCP URL: {base}/mcp\n3. Authentication: Bearer token\n4. Credential (this share only): {credential}\n5. Scan Tools.\n6. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\""
        ),
        TunnelProvider::None if externally_managed => format!(
            "What to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. MCP URL: {base}/mcp\n3. Authentication: Bearer token\n4. Credential (this share only): {credential}\n5. Scan Tools.\n6. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\""
        ),
        TunnelProvider::None => format!(
            "What to do next\n1. Add this MCP endpoint to a local MCP client: {base}/mcp\n2. Authentication: Bearer token\n3. Credential (this share only): {credential}\n4. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\""
        ),
        TunnelProvider::OpenAiSecure | TunnelProvider::CloudflareQuick => {
            "What to do next\nOpenAI Secure MCP Tunnel uses dedicated credential-free ChatGPT handoff output.".to_string()
        }
    };
    let lifetime_message = match tunnel {
        TunnelProvider::CloudflareQuick => "This credential and tunneled URL are temporary.",
        TunnelProvider::OpenAiSecure => {
            "The temporary credential stays local and is never printed by this output path."
        }
        TunnelProvider::None => "This credential is temporary.",
    };
    format!(
        "WebCodex ready\n\n{next_steps}\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nCredential lifetime: {lifetime_message}\nPress Ctrl-C to stop sharing."
    )
}

fn render_share_oauth_ready(
    project_name: &str,
    tunnel: TunnelProvider,
    externally_managed: bool,
    public_url: &str,
    credential: &str,
    oauth: &ShareOAuthClient,
) -> String {
    let (tunnel_name, public_access, ready_message) =
        share_access_labels(tunnel, externally_managed);
    let lifetime_message = if tunnel == TunnelProvider::CloudflareQuick {
        "The OAuth issuer URL and project share credential are temporary. The client ID/secret are persisted for this project and redirect URI."
    } else {
        "The project share credential and OAuth grants are temporary. The client ID/secret are persisted for this project and redirect URI."
    };
    let base = public_url.trim_end_matches('/');
    let client_step = if tunnel == TunnelProvider::CloudflareQuick || externally_managed {
        "1. In ChatGPT Developer Mode, create a custom MCP app."
    } else {
        "1. In a local MCP client, create an MCP connection."
    };
    format!(
        "WebCodex ready\n\nWhat to do next\n{client_step}\n2. MCP URL: {base}/mcp\n3. Authentication: OAuth 2.0 Authorization Code + PKCE S256\n4. Client ID: {}\n5. Client secret: {}\n6. Redirect URI: {}\n7. Scan Tools and complete the WebCodex authorization flow.\n   Project share credential (this share only): {credential}\n   Enter it only on the WebCodex authorization page; do not put it in ChatGPT.\n8. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nAuthorization server: {base}\nOAuth grant lifetime: fenced to this share process; access/refresh grants cannot survive a restart.\nCredential lifetime: {lifetime_message}\nPress Ctrl-C to stop sharing.",
        oauth.client_id, oauth.client_secret, oauth.redirect_uri
    )
}

async fn start_cloudflare_quick_with_binary(
    binary: &Path,
    local_url: &str,
    timeout: Duration,
) -> Result<(String, CloudflareTunnel), ProductError> {
    let mut tunnel_command = Command::new(binary);
    remove_npm_wrapper_network_environment(&mut tunnel_command);
    tunnel_command
        .arg("tunnel")
        .arg("--url")
        .arg(local_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = tunnel_command.spawn().map_err(|_| {
        ProductError::new(
            "tunnel_unavailable",
            "cloudflared could not start",
            Some("Check the cloudflared executable and retry webcodex share."),
        )
    })?;
    let stdout = child.stdout.take().ok_or_else(tunnel_runtime_error)?;
    let stderr = child.stderr.take().ok_or_else(tunnel_runtime_error)?;
    let recent = Arc::new(Mutex::new(VecDeque::with_capacity(TUNNEL_LOG_LINES)));
    let (url_tx, mut url_rx) = mpsc::channel(2);
    let stdout_task = spawn_tunnel_reader(stdout, recent.clone(), url_tx.clone());
    let stderr_task = spawn_tunnel_reader(stderr, recent.clone(), url_tx);
    let deadline = Instant::now() + timeout;

    loop {
        if let Some(status) = child.try_wait().map_err(|_| tunnel_runtime_error())? {
            drain_tunnel_readers(stdout_task, stderr_task).await;
            let detail = bounded_tunnel_log_summary(&recent);
            let message = if detail.is_empty() {
                format!("cloudflared exited before creating a Quick Tunnel ({status})")
            } else {
                format!("cloudflared exited before creating a Quick Tunnel ({status}): {detail}")
            };
            return Err(ProductError::new(
                "tunnel_unavailable",
                message,
                Some("Check cloudflared output and network connectivity, then retry."),
            ));
        }
        let now = Instant::now();
        if now >= deadline {
            let _ = child.start_kill();
            let _ = child.wait().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(ProductError::new(
                "tunnel_unavailable",
                "Cloudflare Quick Tunnel did not provide a public URL before the startup timeout",
                Some("Check network connectivity and cloudflared, then retry."),
            ));
        }
        let wait = (deadline - now).min(Duration::from_millis(100));
        if let Ok(Some(url)) = tokio::time::timeout(wait, url_rx.recv()).await {
            return Ok((
                url,
                CloudflareTunnel {
                    child,
                    stdout_task,
                    stderr_task,
                },
            ));
        }
    }
}

fn spawn_tunnel_reader<R>(
    reader: R,
    recent: Arc<Mutex<VecDeque<String>>>,
    url_tx: mpsc::Sender<String>,
) -> JoinHandle<()>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut lines = BufReader::new(reader).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            record_tunnel_line(&recent, &line);
            if let Some(url) = parse_quick_tunnel_url(&line) {
                let _ = url_tx.try_send(url);
            }
        }
    })
}

// A child can exit before the async readers are scheduled to consume bytes that
// are already buffered in its pipes. Give them a bounded chance to reach EOF so
// startup diagnostics are retained, but do not wait forever if a descendant
// inherited one of the pipe handles.
async fn drain_tunnel_readers(mut stdout_task: JoinHandle<()>, mut stderr_task: JoinHandle<()>) {
    let drained = tokio::time::timeout(TUNNEL_LOG_DRAIN_TIMEOUT, async {
        let _ = tokio::join!(&mut stdout_task, &mut stderr_task);
    })
    .await
    .is_ok();
    if !drained {
        stdout_task.abort();
        stderr_task.abort();
    }
}

fn sanitize_tunnel_log_line(line: &str) -> String {
    line.split_whitespace()
        .map(|token| {
            let Some(scheme_end) = token.find("://") else {
                return token.to_string();
            };
            let bytes = token.as_bytes();
            let mut start = scheme_end;
            while start > 0
                && (bytes[start - 1].is_ascii_alphanumeric()
                    || matches!(bytes[start - 1], b'+' | b'-' | b'.'))
            {
                start -= 1;
            }
            let mut end = token.len();
            while end > scheme_end + 3
                && matches!(
                    token.as_bytes()[end - 1],
                    b')' | b']' | b'}' | b',' | b';' | b'.'
                )
            {
                end -= 1;
            }
            let candidate = &token[start..end];
            let redacted = url::Url::parse(candidate)
                .ok()
                .and_then(|parsed| {
                    let host = parsed.host_str()?;
                    let host = if host.contains(':') {
                        format!("[{host}]")
                    } else {
                        host.to_string()
                    };
                    let port = parsed
                        .port()
                        .map(|port| format!(":{port}"))
                        .unwrap_or_default();
                    Some(format!("{}://{host}{port}/...", parsed.scheme()))
                })
                .unwrap_or_else(|| "[redacted URL]".to_string());
            format!("{}{}{}", &token[..start], redacted, &token[end..])
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn record_tunnel_line(recent: &Arc<Mutex<VecDeque<String>>>, line: &str) {
    // Tunnel diagnostics are model/user-facing on startup failure. Sanitize URL
    // credentials, paths, and query strings before retaining a bounded log tail;
    // Quick Tunnel URL discovery still parses the original unsanitized line.
    let mut line = sanitize_tunnel_log_line(line);
    if line.len() > TUNNEL_LOG_LINE_BYTES {
        let mut end = TUNNEL_LOG_LINE_BYTES;
        while !line.is_char_boundary(end) {
            end -= 1;
        }
        line.truncate(end);
    }
    if let Ok(mut lines) = recent.lock() {
        if lines.len() == TUNNEL_LOG_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }
}

fn bounded_tunnel_log_summary(recent: &Arc<Mutex<VecDeque<String>>>) -> String {
    recent
        .lock()
        .map(|lines| lines.iter().cloned().collect::<Vec<_>>().join(" | "))
        .unwrap_or_default()
}

fn parse_quick_tunnel_url(line: &str) -> Option<String> {
    let start = line.find("https://")?;
    let candidate = line[start..]
        .split_whitespace()
        .next()?
        .trim_end_matches(|character: char| {
            matches!(character, ',' | ';' | ')' | ']' | '}' | '"' | '\'')
        });
    let parsed = url::Url::parse(candidate).ok()?;
    let host = parsed.host_str()?;
    (parsed.scheme() == "https"
        && host.ends_with(".trycloudflare.com")
        && parsed.path() == "/"
        && parsed.query().is_none()
        && parsed.fragment().is_none())
    .then(|| format!("https://{host}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    const TUNNEL_TEST_START_TIMEOUT: Duration = Duration::from_secs(10);

    #[test]
    fn parses_cloudflare_quick_tunnel_url_from_bounded_log_line() {
        assert_eq!(
            parse_quick_tunnel_url(
                "INF +--------------------------------------------------------------------------------------------+ https://bright-example.trycloudflare.com"
            ),
            Some("https://bright-example.trycloudflare.com".to_string())
        );
        assert_eq!(parse_quick_tunnel_url("https://example.com"), None);
        assert_eq!(
            parse_quick_tunnel_url("https://bad.trycloudflare.com/mcp"),
            None
        );
    }

    #[test]
    fn share_cli_defaults_to_cloudflare_and_accepts_openai_and_none() {
        let default = parse_share_options(&[]).unwrap();
        assert_eq!(default.tunnel, TunnelProvider::CloudflareQuick);
        assert_eq!(default.auth, ShareAuth::Bearer);
        assert!(default.oauth_redirect_uri.is_none());
        assert!(default.public_url.is_none());
        assert!(default.copy_url);
        let explicit =
            parse_share_options(&["--tunnel".to_string(), "cloudflare".to_string()]).unwrap();
        assert_eq!(explicit.tunnel, TunnelProvider::CloudflareQuick);
        let openai = parse_share_options(&["--tunnel".to_string(), "openai".to_string()]).unwrap();
        assert_eq!(openai.tunnel, TunnelProvider::OpenAiSecure);
        assert_eq!(openai.auth, ShareAuth::Bearer);
        let query_token =
            parse_share_options(&["--auth".to_string(), "query-token".to_string()]).unwrap();
        assert_eq!(query_token.auth, ShareAuth::QueryToken);
        assert!(query_token.oauth_redirect_uri.is_none());
        assert!(parse_share_options(&[
            "--tunnel".to_string(),
            "openai".to_string(),
            "--auth".to_string(),
            "oauth".to_string(),
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
        ])
        .is_err());
        assert!(parse_share_options(&[
            "--tunnel".to_string(),
            "openai".to_string(),
            "--auth".to_string(),
            "query-token".to_string(),
        ])
        .is_err());
        assert!(parse_share_options(&[
            "--tunnel".to_string(),
            "openai".to_string(),
            "--public-url".to_string(),
            "https://share.example".to_string(),
        ])
        .is_err());
        let local = parse_share_options(&["--tunnel".to_string(), "none".to_string()]).unwrap();
        assert_eq!(local.tunnel, TunnelProvider::None);
        assert!(parse_share_options(&["--tunnel".to_string(), "unknown".to_string()]).is_err());
        let oauth = parse_share_options(&[
            "--auth".to_string(),
            "oauth".to_string(),
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
        ])
        .unwrap();
        assert_eq!(oauth.auth, ShareAuth::OAuth);
        assert_eq!(
            oauth.oauth_redirect_uri.as_deref(),
            Some("https://client.example/callback")
        );
        assert!(parse_share_options(&["--auth".to_string(), "oauth".to_string()]).is_err());
        assert!(parse_share_options(&[
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
        ])
        .is_err());
        let no_copy = parse_share_options(&["--no-copy-url".to_string()]).unwrap();
        assert!(!no_copy.copy_url);
        let stable = parse_share_options(&[
            "--tunnel".to_string(),
            "none".to_string(),
            "--public-url".to_string(),
            "https://share.example".to_string(),
        ])
        .unwrap();
        assert_eq!(stable.public_url.as_deref(), Some("https://share.example"));
        assert!(parse_share_options(&[
            "--public-url".to_string(),
            "https://share.example".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn share_output_contains_only_the_temporary_connector_credential() {
        let persistent = "webcodex_persistent-never-print";
        let temporary = "webcodex_temporary-print-once";
        let output = render_share_ready(
            "demo",
            TunnelProvider::CloudflareQuick,
            ShareAuth::Bearer,
            false,
            "https://demo.trycloudflare.com",
            temporary,
        );
        assert!(output.contains(temporary));
        assert!(!output.contains(persistent));
        assert!(output.starts_with("WebCodex ready\n\nWhat to do next"));
        assert!(output.contains("https://demo.trycloudflare.com/mcp"));
        assert!(output.contains("Authentication: Bearer token"));
        assert!(output.contains("Scan Tools"));
        assert!(output.contains("First prompt:"));
        assert!(output.find("What to do next").unwrap() < output.find("Details").unwrap());
    }

    #[test]
    fn query_token_share_outputs_one_encoded_sensitive_url() {
        let credential = "temporary value/?&=";
        let output = render_share_ready(
            "demo",
            TunnelProvider::CloudflareQuick,
            ShareAuth::QueryToken,
            false,
            "https://demo.trycloudflare.com",
            credential,
        );
        assert!(
            output.contains("https://demo.trycloudflare.com/mcp?token=temporary+value%2F%3F%26%3D")
        );
        assert!(output.contains("Authentication: No authentication"));
        assert!(output.contains("MCP URL (sensitive)"));
        assert!(output.contains("Prefer `--auth bearer`"));
        assert!(!output.contains("Credential (this share only)"));
        assert!(!output.contains("temporary value/?&="));
    }

    #[test]
    fn local_only_share_output_does_not_claim_remote_chatgpt_readiness() {
        let output = render_share_ready(
            "demo",
            TunnelProvider::None,
            ShareAuth::Bearer,
            false,
            "http://127.0.0.1:23456",
            "webcodex_temporary-print-once",
        );
        assert!(output.contains("Ready for a local MCP client"));
        assert!(output.contains("No public tunnel is running"));
        assert!(output.contains("Add this MCP endpoint to a local MCP client"));
        assert!(!output.contains("Ready for ChatGPT"));
        assert!(!output.contains("In ChatGPT Developer Mode"));
        assert!(!output.contains("tunneled URL"));
    }

    #[test]
    fn openai_share_output_keeps_webcodex_credential_local() {
        let output = render_openai_share_ready("demo", "tunnel_0123456789abcdef0123456789abcdef");
        assert!(output.contains("Connection: Tunnel"));
        assert!(output.contains("Authentication: No authentication"));
        assert!(output.contains("tunnel_0123456789abcdef0123456789abcdef"));
        assert!(output.contains("temporary Bearer credential stays local"));
        assert!(output.contains("Do not paste it into ChatGPT"));
        assert!(!output.contains("Credential (this share only)"));
        assert!(!output.contains("MCP URL:"));
    }

    #[test]
    fn oauth_share_output_keeps_project_credential_separate_from_client_secret() {
        let oauth = ShareOAuthClient {
            client_id: "wc_client_test".to_string(),
            client_secret: "wc_csec_test".to_string(),
            redirect_uri: "https://client.example/callback".to_string(),
        };
        let output = render_share_oauth_ready(
            "demo",
            TunnelProvider::None,
            true,
            "https://share.example",
            "webcodex_temporary-print-once",
            &oauth,
        );
        assert!(output.contains("OAuth 2.0 Authorization Code + PKCE S256"));
        assert!(output.contains("https://share.example/mcp"));
        assert!(output.contains("wc_client_test"));
        assert!(output.contains("wc_csec_test"));
        assert!(output.contains("webcodex_temporary-print-once"));
        assert!(output.starts_with("WebCodex ready\n\nWhat to do next"));
        assert!(output.contains("Project share credential (this share only)"));
        assert!(output.find("MCP URL:").unwrap() < output.find("Details").unwrap());
        assert!(output.contains("fenced to this share process"));
        assert!(output.contains("externally managed"));
    }

    #[test]
    fn tunnel_startup_diagnostics_redact_url_credentials_and_private_components() {
        let recent = Arc::new(Mutex::new(VecDeque::new()));
        record_tunnel_line(
            &recent,
            "ERR proxy=https://proxy-user:proxy-secret@proxy.example:8443/private/path?token=hidden",
        );
        let summary = bounded_tunnel_log_summary(&recent);
        assert!(summary.contains("https://proxy.example:8443/..."));
        for secret in ["proxy-user", "proxy-secret", "private/path", "token=hidden"] {
            assert!(
                !summary.contains(secret),
                "tunnel diagnostic leaked {secret}"
            );
        }
    }

    #[test]
    fn tunnel_log_truncation_preserves_utf8_boundaries() {
        let recent = Arc::new(Mutex::new(VecDeque::new()));
        let line = format!("{}界tail", "a".repeat(TUNNEL_LOG_LINE_BYTES - 1));
        record_tunnel_line(&recent, &line);
        let recorded = recent.lock().unwrap();
        let line = recorded.back().unwrap();
        assert!(line.len() <= TUNNEL_LOG_LINE_BYTES);
        assert_eq!(line, &"a".repeat(TUNNEL_LOG_LINE_BYTES - 1));
    }

    #[test]
    fn local_port_preflight_rejects_an_occupied_port() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let error = ensure_local_runtime_port_available(port, "stop conflict").unwrap_err();
        assert_eq!(error.code, "server_unreachable");
        assert!(error.message.contains("already in use"));
    }

    #[test]
    fn temporary_share_credential_is_private_distinct_and_cleaned_up() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("state");
        create_private_dir(&state).unwrap();
        let persistent = state.join("credentials/connector-key");
        let persistent_value = generate_project_credential();
        write_new_private(&persistent, format!("{persistent_value}\n").as_bytes()).unwrap();
        let persistent_before = fs::read_to_string(&persistent).unwrap();
        let session = ShareSession::create(&state).unwrap();
        assert_ne!(session.credential, persistent_value);
        assert_eq!(fs::read_to_string(&persistent).unwrap(), persistent_before);
        let authorization_file = session.write_openai_authorization_file().unwrap();
        assert_eq!(
            fs::read_to_string(&authorization_file).unwrap(),
            format!("Bearer {}", session.credential)
        );
        assert_eq!(
            crate::auth::read_protected_secret(&session.credential_file).unwrap(),
            session.credential
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(state.join("share"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&session.directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&session.credential_file)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&authorization_file)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        let directory = session.directory.clone();
        drop(session);
        assert!(!directory.exists());
        assert!(!authorization_file.exists());
        assert!(persistent.is_file());
    }

    #[cfg(unix)]
    fn fake_cloudflared(script: &str) -> (tempfile::TempDir, PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("cloudflared");
        fs::write(&path, script).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        (temp, path)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_startup_failure_is_reported() {
        let (_temp, binary) = fake_cloudflared("#!/bin/sh\necho startup-failed >&2\nexit 7\n");
        let error = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("startup-failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_startup_timeout_kills_fake_process() {
        let (_temp, binary) = fake_cloudflared("#!/bin/sh\nsleep 5\n");
        let error = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("startup timeout"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_stop_reaps_fake_process() {
        let (_temp, binary) = fake_cloudflared(
            "#!/bin/sh\necho https://cleanup-test.trycloudflare.com >&2\nsleep 5\n",
        );
        let (_url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap();
        tunnel.stop().await;
        assert!(tunnel.child.try_wait().unwrap().is_some());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn tunnel_early_exit_after_url_is_supervised() {
        let (_temp, binary) = fake_cloudflared(
            "#!/bin/sh\necho https://short-lived.trycloudflare.com >&2\nsleep 0.1\nexit 9\n",
        );
        let (url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(url, "https://short-lived.trycloudflare.com");
        let error = tunnel.wait_for_exit().await.unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("stopped unexpectedly"));
    }
}
