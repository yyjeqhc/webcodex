use super::client_handoff_service::{
    copy_mcp_url, copy_text_to_clipboard, maybe_spawn_chatgpt_open_prompt, mcp_url,
    render_clipboard_status, ClipboardCopyOutcome,
};
use super::setup_service::{
    create_private_dir, generate_project_credential, read_private_value, read_project_credential,
    remove_private_state_file, replace_private_file, write_new_private, ProjectConfig,
    ProjectPaths,
};
use super::{
    configured_project, ensure_local_runtime_port_available, parse_options,
    remove_npm_wrapper_network_environment, setup, start_local_runtime, LocalRuntimeOptions,
    ProductError, ProjectCommandOptions, ProjectShareOAuthRuntimeOptions,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

const SHARE_START_TIMEOUT: Duration = Duration::from_secs(60);
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
    pub(crate) oauth_persist_on_restart: bool,
    pub(crate) public_url: Option<String>,
    pub(crate) copy_url: bool,
    pub(crate) stop_on_stdin_eof: bool,
    /// Explicit parent-root grant for this OAuth share's project-share
    /// subjects. Canonicalized and validated at parse time; persisted next to
    /// the share OAuth client state so restarts keep the same grant.
    pub(crate) oauth_project_roots: Vec<PathBuf>,
    /// Whether the invocation expressed a parent-root choice at all
    /// (`--project-roots` or `--no-project-roots`). Explicit invocations
    /// replace the persisted grant; plain restarts restore it.
    pub(crate) oauth_project_roots_explicit: bool,
}

pub(crate) fn parse_share_options(args: &[String]) -> Result<ShareCommandOptions, String> {
    let mut tunnel = TunnelProvider::CloudflareQuick;
    let mut auth = ShareAuth::Bearer;
    let mut oauth_redirect_uri = None;
    let mut oauth_persist_on_restart = false;
    let mut public_url = None;
    let mut copy_url = true;
    let mut stop_on_stdin_eof = false;
    let mut oauth_project_roots: Vec<PathBuf> = Vec::new();
    let mut oauth_project_roots_explicit = false;
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
            "--oauth-persist-on-restart" => oauth_persist_on_restart = true,
            "--no-copy-url" => copy_url = false,
            "--stop-on-stdin-eof" => stop_on_stdin_eof = true,
            "--project-roots" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--project-roots requires a directory".to_string())?;
                let root = validate_share_project_root(value)?;
                if oauth_project_roots.len() >= crate::config::MAX_PROJECT_SHARE_ROOTS {
                    return Err(format!(
                        "at most {} --project-roots directories are supported",
                        crate::config::MAX_PROJECT_SHARE_ROOTS
                    ));
                }
                if !oauth_project_roots.contains(&root) {
                    oauth_project_roots.push(root);
                }
                oauth_project_roots_explicit = true;
            }
            "--no-project-roots" => {
                oauth_project_roots.clear();
                oauth_project_roots_explicit = true;
            }
            flag => project_args.push(flag.to_string()),
        }
        index += 1;
    }
    if tunnel != TunnelProvider::None && public_url.is_some() {
        return Err("--public-url requires --tunnel none; managed tunnel providers own their transport endpoint".to_string());
    }
    if tunnel == TunnelProvider::OpenAiSecure && auth != ShareAuth::Bearer {
        return Err("--tunnel openai currently requires --auth bearer; WebCodex keeps that temporary Bearer credential local and the native Tunnel client injects it into the private MCP hop".to_string());
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
    if oauth_persist_on_restart && auth != ShareAuth::OAuth {
        return Err("--oauth-persist-on-restart requires --auth oauth".to_string());
    }
    let project = parse_options(&project_args, "share")?;
    if project.json && auth == ShareAuth::OAuth {
        return Err(
            "share --json supports bearer or query-token handoff; OAuth client secrets are intentionally excluded from the machine protocol"
                .to_string(),
        );
    }
    if !oauth_project_roots.is_empty() && auth != ShareAuth::OAuth {
        return Err("--project-roots requires --auth oauth".to_string());
    }
    if oauth_project_roots_explicit && auth != ShareAuth::OAuth {
        return Err("--no-project-roots requires --auth oauth".to_string());
    }
    if stop_on_stdin_eof && !project.json {
        return Err("--stop-on-stdin-eof requires --json machine integration mode".to_string());
    }
    Ok(ShareCommandOptions {
        project,
        tunnel,
        auth,
        oauth_redirect_uri,
        oauth_persist_on_restart,
        public_url,
        copy_url,
        stop_on_stdin_eof,
        oauth_project_roots,
        oauth_project_roots_explicit,
    })
}

/// Validate one `--project-roots` value at share-start time. The root must be
/// an existing absolute directory; it is canonicalized here so the persisted
/// grant and the Server-side fence compare the same real directory, and so a
/// symlinked spelling cannot smuggle a different authorization target.
fn validate_share_project_root(value: &str) -> Result<PathBuf, String> {
    let trimmed = value.trim();
    let path = Path::new(trimmed);
    if trimmed.is_empty() || !path.is_absolute() {
        return Err(format!(
            "--project-roots requires an absolute directory path, got '{trimmed}'"
        ));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("--project-roots directory '{trimmed}' is not usable: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("--project-roots '{trimmed}' is not a directory"));
    }
    Ok(canonical)
}

const SHARE_PROJECT_ROOTS_FILE: &str = "project-roots";

/// Read the persisted parent-root grant from the share OAuth state directory.
/// A JSON array of canonical absolute paths; unreadable or malformed state
/// fails closed, and an absent file means no grant (the exact-path fence stays).
fn read_persisted_share_project_roots(directory: &Path) -> Vec<PathBuf> {
    let Ok(content) = std::fs::read_to_string(directory.join(SHARE_PROJECT_ROOTS_FILE)) else {
        return Vec::new();
    };
    // A line-delimited path list is unsafe: on Unix a legitimate directory
    // name may contain a newline, so a path like /tmp/grant\n/etc would
    // restore as an unrelated grant to /etc. Persist one unambiguous JSON
    // array instead and reject obsolete/malformed encodings fail closed.
    let Ok(roots) = serde_json::from_str::<Vec<String>>(&content) else {
        return Vec::new();
    };
    if roots.len() > crate::config::MAX_PROJECT_SHARE_ROOTS {
        return Vec::new();
    }
    roots
        .iter()
        .filter_map(|path| validate_share_project_root(path).ok())
        .collect()
}

/// Whether the stored roots file may serve as the no-op fast path: a regular
/// file (never through a symlink) at owner-only permissions holding exactly
/// the requested bytes. Anything else — a symlink with equal content, a
/// wide-permission file, or a different value — falls through to the safe
/// same-directory replacement, which repairs the target type and permissions
/// instead of returning success while leaving a non-conforming target.
fn stored_roots_is_owner_only_file_with(path: &Path, bytes: &[u8]) -> bool {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return false;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o777 != 0o600 {
            return false;
        }
    }
    std::fs::read(path).ok().as_deref() == Some(bytes)
}

/// Persist the parent-root grant next to the share OAuth client state.
///
/// Explicit invocations must be idempotent and updatable: the same list may be
/// re-sent by an unchanging systemd unit on every restart, a changed list
/// replaces the previous grant, and an empty list clears it. An identical
/// stored grant is left untouched; a different one is replaced through an
/// atomic same-directory rename that never destroys the previous value on
/// failure. Failure is fatal for an explicit grant: a share that silently
/// drops its roots would report success while denying the intended
/// directories.
fn persist_share_project_roots(directory: &Path, roots: &[PathBuf]) -> Result<(), ProductError> {
    create_private_dir(directory)?;
    let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
    if roots.is_empty() {
        return remove_private_state_file(&path);
    }
    // JSON escapes embedded newlines and path delimiters. Never reinterpret
    // one canonical user-approved root as multiple grants after a restart.
    let encoded_roots = roots
        .iter()
        .map(|root| {
            root.to_str()
                .ok_or_else(|| share_oauth_state_error("project share root is not valid UTF-8"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let bytes = serde_json::to_vec(&encoded_roots)
        .map_err(|_| share_oauth_state_error("could not encode project share root grants"))?;
    if stored_roots_is_owner_only_file_with(&path, &bytes) {
        return Ok(());
    }
    replace_private_file(&path, &bytes)
}

fn mcp_query_token_url(public_url: &str, credential: &str) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    query.append_pair("token", credential);
    format!("{}?{}", mcp_url(public_url), query.finish())
}

fn clipboard_outcome_name(outcome: ClipboardCopyOutcome) -> &'static str {
    match outcome {
        ClipboardCopyOutcome::Copied => "copied",
        ClipboardCopyOutcome::Unavailable => "unavailable",
        ClipboardCopyOutcome::Disabled => "disabled",
    }
}

fn machine_share_ready_event(
    project_name: &str,
    tunnel: TunnelProvider,
    auth: ShareAuth,
    externally_managed: bool,
    local_url: &str,
    public_url: &str,
    clipboard: ClipboardCopyOutcome,
) -> Value {
    let exposure = match (tunnel, externally_managed) {
        (TunnelProvider::CloudflareQuick, _) => "cloudflare",
        (TunnelProvider::OpenAiSecure, _) => "openai_tunnel",
        (TunnelProvider::None, true) => "existing_https",
        (TunnelProvider::None, false) => "none",
    };
    let remote_ready = tunnel != TunnelProvider::None || externally_managed;
    let clipboard_contains = match (tunnel, auth) {
        (TunnelProvider::OpenAiSecure, _) => "tunnel_id",
        (_, ShareAuth::Bearer) => "bearer_credential",
        (_, ShareAuth::QueryToken) => "sensitive_mcp_url",
        (_, ShareAuth::OAuth) => "none",
    };
    let authentication = if tunnel == TunnelProvider::OpenAiSecure {
        "none"
    } else if auth == ShareAuth::QueryToken {
        "query_token"
    } else {
        "bearer"
    };
    let safe_mcp_url = (tunnel != TunnelProvider::OpenAiSecure && auth != ShareAuth::QueryToken)
        .then(|| mcp_url(public_url));
    json!({
        "event": "ready",
        "schema_version": 1,
        "experience": "quick_share",
        "project": project_name,
        "server": { "kind": "local", "url": local_url },
        "runner": { "kind": "local" },
        "exposure": {
            "kind": exposure,
            "state": if remote_ready { "remote_ready" } else { "local_ready" }
        },
        "connection": {
            "kind": if tunnel == TunnelProvider::OpenAiSecure { "openai_tunnel" } else { "mcp_url" },
            "mcp_url": safe_mcp_url,
            "authentication": authentication,
            "clipboard_state": clipboard_outcome_name(clipboard),
            "clipboard_contains": clipboard_contains,
        },
        "ready_for_chatgpt": remote_ready,
    })
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
        let credential_file = directory.join("project-credential");
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
    /// Protected per-redirect share OAuth state directory; also holds the
    /// persisted parent-root grant.
    state_directory: PathBuf,
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
    persist_on_restart: bool,
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
                || !crate::auth::verify_oauth_client_secret(&db, &client_id, &client_secret)
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
                dynamic_registration: None,
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
        // An ordinary restart retires the previous run's grants. Persistence is
        // an explicit opt-in: expiry, revocation and token replay rules still
        // decide whether a retained grant remains usable.
        if !persist_on_restart {
            revoke_previous_share_oauth_grants(&db, &client_id)?;
        }
        return Ok(ShareOAuthClient {
            client_id,
            client_secret,
            redirect_uri,
            state_directory: directory,
        });
    }

    let client_id = crate::auth::generate_oauth_client_id();
    let client_secret = crate::auth::generate_oauth_client_secret();
    db.insert_oauth_client(&crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: client_id.clone(),
        dynamic_registration: None,
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
        state_directory: directory,
    })
}

#[derive(Debug)]
struct CloudflareTunnel {
    child: Child,
    process_group_id: Option<u32>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

struct CloudflareTunnelStartup {
    child: Child,
    process_group_id: Option<u32>,
    stdout_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
    recent: Arc<Mutex<VecDeque<String>>>,
    url_rx: mpsc::Receiver<String>,
}

/// Owns the startup-stage process group until it is either explicitly
/// terminated or transferred into a live `CloudflareTunnel`. This closes the
/// async-cancellation/error gap before the tunnel object itself exists: Tokio's
/// `Child::kill_on_drop` only kills the direct child, not inherited descendants.
struct CloudflareStartupProcessGroupGuard {
    process_group_id: Option<u32>,
}

impl CloudflareStartupProcessGroupGuard {
    fn new(process_group_id: Option<u32>) -> Self {
        Self { process_group_id }
    }

    fn take(&mut self) -> Option<u32> {
        self.process_group_id.take()
    }
}

impl Drop for CloudflareStartupProcessGroupGuard {
    fn drop(&mut self) {
        #[cfg(unix)]
        signal_cloudflare_process_group(self.process_group_id.take());
    }
}

impl CloudflareTunnel {
    async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        let status = self
            .child
            .wait()
            .await
            .map_err(|_| tunnel_runtime_error())?;
        #[cfg(unix)]
        signal_cloudflare_process_group(self.process_group_id.take());
        Err(ProductError::new(
            "tunnel_unavailable",
            format!("Cloudflare Quick Tunnel stopped unexpectedly ({status})"),
            Some("Check network connectivity and cloudflared, then retry webcodex share."),
        ))
    }

    async fn stop(&mut self) {
        let process_group_id = self.process_group_id.take();
        terminate_cloudflare_process_tree(&mut self.child, process_group_id).await;
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

impl Drop for CloudflareTunnel {
    fn drop(&mut self) {
        #[cfg(unix)]
        signal_cloudflare_process_group(self.process_group_id);
        self.stdout_task.abort();
        self.stderr_task.abort();
    }
}

/// Put cloudflared in its own process group so the whole tree it spawns
/// (including backgrounded descendants and the npm wrapper's children) can be
/// reaped as a group. `process_group(0)` makes the child's pid its group id.
#[cfg(unix)]
fn configure_cloudflare_process_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
}

#[cfg(not(unix))]
fn configure_cloudflare_process_tree(_command: &mut Command) {}

/// Best-effort SIGKILL of the tunnel's whole process group (`kill(-pgid,
/// SIGKILL)`; a negative target signals every process in the group). Reaps
/// background descendants `cloudflared` or its npm wrapper may leave behind,
/// which a direct child kill alone would leak. `pgid` is always our own
/// child's pid (made a group leader via `process_group(0)`), so this only ever
/// targets the tunnel's own subtree. Uses the direct syscall: the `kill`
/// binary rejects negative pgid arguments on some coreutils builds. Failure
/// (e.g. the group already fully exited, ESRCH) is expected and ignored.
#[cfg(unix)]
fn signal_cloudflare_process_group(process_group_id: Option<u32>) {
    // Guard against ever signalling pid 0 / -1 ("current group" / "all
    // processes") if a caller somehow passed a zero id.
    let Some(process_group_id) = process_group_id.filter(|value| *value > 0) else {
        return;
    };
    // SAFETY: the process group id is captured from our own child immediately
    // after spawning it as a new group leader. A stale/invalid pgid yields
    // ESRCH, which we deliberately ignore.
    unsafe {
        libc::kill(-(process_group_id as i32), libc::SIGKILL);
    }
}

/// Tear down the tunnel process tree: on Unix signal the whole group (covering
/// any background descendants a direct kill would miss), then reap the child.
/// On non-Unix targets fall back to a direct child kill.
async fn terminate_cloudflare_process_tree(child: &mut Child, _process_group_id: Option<u32>) {
    #[cfg(unix)]
    signal_cloudflare_process_group(_process_group_id);
    #[cfg(not(unix))]
    {
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
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
            Some(super::cloudflared_service::resolve_cloudflared(|_| {}).await?)
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
    let persistent_credential = read_project_credential(&paths.project_credential)?;
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
            options.oauth_persist_on_restart,
        )?),
    };
    // Parent-root grant: an explicit flag replaces the persisted list; without
    // the flag the persisted grant is restored so restarts keep the same
    // authorization without retyping it.
    let oauth_project_roots = match oauth_client.as_ref() {
        Some(client) if options.oauth_project_roots_explicit => {
            persist_share_project_roots(&client.state_directory, &options.oauth_project_roots)?;
            options.oauth_project_roots.clone()
        }
        Some(client) => read_persisted_share_project_roots(&client.state_directory),
        None => Vec::new(),
    };
    let project_share_oauth = oauth_client
        .as_ref()
        .map(|client| {
            Ok(ProjectShareOAuthRuntimeOptions {
                project_grant_id: config.project_grant_id(&paths),
                session_id: if options.oauth_persist_on_restart {
                    crate::auth::project_share_session_id_for_client(&client.client_id)
                        .map_err(share_oauth_state_error)?
                } else {
                    crate::auth::generate_project_share_session_id()
                },
                project_roots: oauth_project_roots.clone(),
            })
        })
        .transpose()?;

    // All process/network readiness for this share uses one absolute budget.
    // Managed binary acquisition and local state setup above are preflight, not
    // readiness stages, and therefore do not reset this deadline.
    let startup_deadline = Instant::now() + SHARE_START_TIMEOUT;
    let local_url = config.server_url();
    let (public_url, mut cloudflare_tunnel) = match options.tunnel {
        TunnelProvider::CloudflareQuick => {
            let binary = cloudflared_binary
                .as_deref()
                .ok_or_else(tunnel_runtime_error)?;
            let (url, tunnel) =
                start_cloudflare_quick_with_binary(binary, &local_url, startup_deadline).await?;
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

    let mut runtime = match start_local_runtime(
        &options.project,
        LocalRuntimeOptions {
            public_url: Some(public_url.clone()),
            project_credential_file: Some(session.credential_file.clone()),
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
            readiness_deadline: Some(startup_deadline),
        },
    )
    .await
    {
        Ok(runtime) => runtime,
        Err(error) => {
            if let Some(tunnel) = cloudflare_tunnel.as_mut() {
                tunnel.stop().await;
            }
            return Err(error);
        }
    };

    if let Some(tunnel) = cloudflare_tunnel.as_mut() {
        if let Err(error) =
            wait_for_cloudflare_forwarding(tunnel, &runtime.public_url, startup_deadline).await
        {
            runtime.stop().await;
            tunnel.stop().await;
            return Err(error);
        }
    }

    let mut openai_tunnel = if let Some(prerequisites) = openai_prerequisites.as_ref() {
        match super::openai_tunnel_service::start_temporary_openai_tunnel(
            prerequisites,
            &mcp_url(&runtime.local_url),
            &session.credential,
            startup_deadline,
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
    let copy_remote_mcp_url =
        options.tunnel == TunnelProvider::CloudflareQuick || externally_managed;
    let open_chatgpt_handoff =
        copy_remote_mcp_url || options.tunnel == TunnelProvider::OpenAiSecure;
    let handoff_task = if options.project.json {
        let query_token_url = (options.auth == ShareAuth::QueryToken)
            .then(|| mcp_query_token_url(&runtime.public_url, &session.credential));
        let clipboard_value = if let Some(prerequisites) = openai_prerequisites.as_ref() {
            Some(prerequisites.tunnel_id.as_str())
        } else if let Some(url) = query_token_url.as_deref() {
            Some(url)
        } else {
            Some(session.credential.as_str())
        };
        let clipboard_outcome = match clipboard_value {
            Some(value) => copy_text_to_clipboard(value, options.copy_url).await,
            None => ClipboardCopyOutcome::Disabled,
        };
        let event = machine_share_ready_event(
            &runtime.project_name,
            options.tunnel,
            options.auth,
            externally_managed,
            &runtime.local_url,
            &runtime.public_url,
            clipboard_outcome,
        );
        let encoded = serde_json::to_string(&event).map_err(|_| {
            ProductError::new(
                "machine_output_failed",
                "WebCodex could not encode Quick Share readiness",
                Some("Retry Quick Share or use the interactive CLI output."),
            )
        })?;
        println!("{encoded}");
        None
    } else {
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
        open_chatgpt_handoff
            .then(maybe_spawn_chatgpt_open_prompt)
            .flatten()
    };

    let outcome = match (cloudflare_tunnel.as_mut(), openai_tunnel.as_mut()) {
        (Some(tunnel), None) => tokio::select! {
            _ = wait_for_share_stop_signal(options.stop_on_stdin_eof) => Ok(()),
            result = runtime.wait_for_exit() => result,
            result = tunnel.wait_for_exit() => result,
        },
        (None, Some(tunnel)) => tokio::select! {
            _ = wait_for_share_stop_signal(options.stop_on_stdin_eof) => Ok(()),
            result = runtime.wait_for_exit() => result,
            result = tunnel.wait_for_exit() => result,
        },
        (None, None) => tokio::select! {
            _ = wait_for_share_stop_signal(options.stop_on_stdin_eof) => Ok(()),
            result = runtime.wait_for_exit() => result,
        },
        (Some(_), Some(_)) => unreachable!("one share cannot own two tunnel providers"),
    };

    if let Some(task) = handoff_task {
        task.abort();
        let _ = task.await;
    }
    // Keep the owned local MCP server available until Tunnel response delivery
    // has settled. Closing HTTP first turns otherwise clean stops into uncertainty.
    let stopped = if let Some(tunnel) = openai_tunnel.as_mut() {
        tunnel.stop_with_outcome().await
    } else {
        Ok(())
    };
    if let Some(tunnel) = cloudflare_tunnel.as_mut() {
        tunnel.stop().await;
    }
    runtime.stop().await;
    outcome.and(stopped)
}

async fn wait_for_share_stop_signal(stop_on_stdin_eof: bool) {
    if !stop_on_stdin_eof {
        wait_for_platform_share_stop_signal().await;
        return;
    }
    tokio::select! {
        _ = wait_for_platform_share_stop_signal() => {},
        _ = wait_for_share_stdin_eof() => {},
    }
}

async fn wait_for_share_stdin_eof() {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let _ = std::thread::Builder::new()
        .name("webcodex-share-stdin".to_string())
        .spawn(move || {
            let mut stdin = std::io::stdin().lock();
            let mut buffer = [0_u8; 256];
            loop {
                match stdin.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
            }
            let _ = tx.send(());
        });
    let _ = rx.await;
}

#[cfg(not(windows))]
async fn wait_for_platform_share_stop_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(windows)]
async fn wait_for_platform_share_stop_signal() {
    let mut ctrl_break = match tokio::signal::windows::ctrl_break() {
        Ok(signal) => signal,
        Err(_) => {
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {},
        _ = ctrl_break.recv() => {},
    }
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
        "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\nWhat to do next\n1. In ChatGPT Developer Mode, create a custom MCP app.\n2. Connection: Tunnel\n3. Tunnel: {tunnel_id}\n4. Authentication: No authentication\n5. Scan Tools.\n6. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\nReady for ChatGPT through OpenAI Secure MCP Tunnel.\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: OpenAI Secure MCP Tunnel\nPublic access: no public WebCodex endpoint; outbound-only OpenAI Tunnel transport\nWebCodex authentication: the temporary Bearer credential stays local and is injected by the native Tunnel client into the private MCP hop. Do not paste it into ChatGPT.\nCredential lifetime: temporary; stopping this share removes the local credential and Tunnel task. The Platform Tunnel identity remains operator managed.\nPress Ctrl-C to stop sharing."
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
            "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\nWhat to do next\n{client_step}\n2. MCP URL (sensitive): {endpoint}\n3. Authentication: No authentication\n4. Scan Tools.\n5. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nCredential transport: URL query (`token=`), explicitly opted in for this temporary share only.\nSecurity: treat the entire MCP URL as a secret; query credentials may appear in client, proxy, clipboard, or access logs. Prefer `--auth bearer` when the client supports headers.\nCredential lifetime: temporary; stopping this share removes the accepted credential.\nPress Ctrl-C to stop sharing."
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
        "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\n{next_steps}\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nCredential lifetime: {lifetime_message}\nPress Ctrl-C to stop sharing."
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
        "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\nWhat to do next\n{client_step}\n2. MCP URL: {base}/mcp\n3. Authentication: OAuth 2.0 Authorization Code + PKCE S256\n4. Client ID: {}\n5. Client secret: {}\n6. Redirect URI: {}\n7. Scan Tools and complete the WebCodex authorization flow.\n   Project share credential (this share only): {credential}\n   Enter it only on the WebCodex authorization page; do not put it in ChatGPT.\n8. First prompt: \"Inspect this repository and summarize its structure. Do not make changes.\"\n\n{ready_message}\n\nDetails\nProject: {project_name}\nRuntime: local\nTunnel: {tunnel_name}\nPublic access: {public_access}\nAuthorization server: {base}\nOAuth grant lifetime: fenced to this share process; access/refresh grants cannot survive a restart.\nCredential lifetime: {lifetime_message}\nPress Ctrl-C to stop sharing.",
        oauth.client_id, oauth.client_secret, oauth.redirect_uri
    )
}

async fn start_cloudflare_quick_with_binary(
    binary: &Path,
    local_url: &str,
    deadline: Instant,
) -> Result<(String, CloudflareTunnel), ProductError> {
    let startup = spawn_cloudflare_quick_with_binary(binary, local_url)?;
    wait_for_cloudflare_quick_url(startup, deadline).await
}

fn spawn_cloudflare_quick_with_binary(
    binary: &Path,
    local_url: &str,
) -> Result<CloudflareTunnelStartup, ProductError> {
    let mut tunnel_command = Command::new(binary);
    remove_npm_wrapper_network_environment(&mut tunnel_command);
    tunnel_command
        .arg("tunnel")
        .arg("--url")
        .arg(local_url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    configure_cloudflare_process_tree(&mut tunnel_command);
    let mut child = tunnel_command.spawn().map_err(|error| {
        ProductError::new(
            "tunnel_unavailable",
            format!("cloudflared could not start ({:?})", error.kind()),
            Some("Check the cloudflared executable and retry webcodex share."),
        )
    })?;
    let process_group_id = child.id();
    let stdout = child.stdout.take().ok_or_else(tunnel_runtime_error)?;
    let stderr = child.stderr.take().ok_or_else(tunnel_runtime_error)?;
    let recent = Arc::new(Mutex::new(VecDeque::with_capacity(TUNNEL_LOG_LINES)));
    let (url_tx, url_rx) = mpsc::channel(2);
    let stdout_task = spawn_tunnel_reader(stdout, recent.clone(), url_tx.clone());
    let stderr_task = spawn_tunnel_reader(stderr, recent.clone(), url_tx);
    Ok(CloudflareTunnelStartup {
        child,
        process_group_id,
        stdout_task,
        stderr_task,
        recent,
        url_rx,
    })
}

async fn wait_for_cloudflare_quick_url(
    startup: CloudflareTunnelStartup,
    deadline: Instant,
) -> Result<(String, CloudflareTunnel), ProductError> {
    let CloudflareTunnelStartup {
        mut child,
        process_group_id,
        stdout_task,
        stderr_task,
        recent,
        mut url_rx,
    } = startup;
    let mut process_group_guard = CloudflareStartupProcessGroupGuard::new(process_group_id);
    loop {
        if let Some(status) = child.try_wait().map_err(|_| tunnel_runtime_error())? {
            // The child can exit after writing a valid Quick Tunnel URL but before
            // the async pipe readers are scheduled. Give those readers one bounded
            // chance to publish a URL already emitted by the child before classifying
            // startup as failed. A returned tunnel may therefore already be terminal;
            // the normal forwarding/exit supervision path will report that state.
            if let Ok(Some(url)) =
                tokio::time::timeout(TUNNEL_LOG_DRAIN_TIMEOUT, url_rx.recv()).await
            {
                return Ok((
                    url,
                    CloudflareTunnel {
                        child,
                        process_group_id: process_group_guard.take(),
                        stdout_task,
                        stderr_task,
                    },
                ));
            }
            terminate_cloudflare_process_tree(&mut child, process_group_guard.take()).await;
            // Kill any descendants first so inherited pipe handles are closed.
            // Bytes already written to the kernel pipes remain readable, letting
            // the reader tasks deterministically drain diagnostics to EOF instead
            // of racing a timeout against a still-open descendant descriptor.
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
            terminate_cloudflare_process_tree(&mut child, process_group_guard.take()).await;
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
                    process_group_id: process_group_guard.take(),
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

async fn wait_for_cloudflare_forwarding(
    tunnel: &mut CloudflareTunnel,
    public_url: &str,
    deadline: Instant,
) -> Result<(), ProductError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| tunnel_runtime_error())?;
    let probe_url = format!("{}/healthz", public_url.trim_end_matches('/'));
    loop {
        if let Some(status) = tunnel
            .child
            .try_wait()
            .map_err(|_| tunnel_runtime_error())?
        {
            return Err(ProductError::new(
                "tunnel_unavailable",
                format!("Cloudflare Quick Tunnel stopped before public forwarding became ready ({status})"),
                Some("Check network connectivity and cloudflared, then retry."),
            ));
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(ProductError::new(
                "tunnel_unavailable",
                "Cloudflare Quick Tunnel did not forward the local Server before the share startup timeout",
                Some("Check network connectivity and cloudflared, then retry."),
            ));
        }
        let remaining = deadline.saturating_duration_since(now);
        if let Ok(Ok(response)) = tokio::time::timeout(
            remaining.min(Duration::from_secs(2)),
            client.get(&probe_url).send(),
        )
        .await
        {
            if response.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(150)),
        )
        .await;
    }
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

pub(super) fn parse_quick_tunnel_url(line: &str) -> Option<String> {
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
        && parsed.username().is_empty()
        && parsed.password().is_none()
        && parsed.port().is_none()
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
        assert!(!default.stop_on_stdin_eof);
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
    fn share_json_is_a_safe_foreground_integration_mode() {
        let machine = parse_share_options(&[
            "--tunnel".to_string(),
            "cloudflare".to_string(),
            "--json".to_string(),
            "--stop-on-stdin-eof".to_string(),
        ])
        .unwrap();
        assert!(machine.project.json);
        assert_eq!(machine.auth, ShareAuth::Bearer);
        assert!(machine.stop_on_stdin_eof);
        assert!(parse_share_options(&["--stop-on-stdin-eof".to_string()]).is_err());

        let oauth = parse_share_options(&[
            "--json".to_string(),
            "--auth".to_string(),
            "oauth".to_string(),
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
        ])
        .unwrap_err();
        assert!(oauth.contains("OAuth client secrets are intentionally excluded"));
    }

    #[test]
    fn oauth_persist_on_restart_is_an_explicit_oauth_only_choice() {
        let oauth = parse_share_options(&[
            "--auth".to_string(),
            "oauth".to_string(),
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
        ])
        .unwrap();
        assert!(!oauth.oauth_persist_on_restart);
        let persistent = parse_share_options(&[
            "--auth".to_string(),
            "oauth".to_string(),
            "--oauth-redirect-uri".to_string(),
            "https://client.example/callback".to_string(),
            "--oauth-persist-on-restart".to_string(),
        ])
        .unwrap();
        assert!(persistent.oauth_persist_on_restart);
        assert!(parse_share_options(&["--oauth-persist-on-restart".to_string()]).is_err());
        assert!(parse_share_options(&[
            "--auth".to_string(),
            "bearer".to_string(),
            "--oauth-persist-on-restart".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn project_share_restart_preserves_client_and_only_retires_default_mode_grants() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join("project-state");
        let data = state.join("data");
        fs::create_dir_all(&data).unwrap();
        let paths = ProjectPaths {
            data,
            credentials: state.join("credentials"),
            project_registry: state.join("project-registry"),
            logs: state.join("logs"),
            config: state.join("project.toml"),
            bootstrap_key: state.join("bootstrap-key"),
            project_credential: state.join("project-credential"),
            agent_token: state.join("agent-token"),
            runner_config: state.join("runner-config"),
            state,
        };
        let config = ProjectConfig {
            version: 1,
            project_name: "test-project".to_string(),
            root: temp.path().join("project"),
            profile: "default".to_string(),
            port: 24444,
            executor_project_id: "test-project".to_string(),
            executor_client_id: "test-client".to_string(),
        };
        let redirect = "https://client.example/callback";
        let first = prepare_share_oauth_client(&config, &paths, redirect, true).unwrap();
        let stable = crate::auth::project_share_session_id_for_client(&first.client_id).unwrap();
        let db = crate::Database::open(&paths.data.join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        let access = crate::auth::generate_oauth_access_token();
        let refresh = crate::auth::generate_oauth_refresh_token();
        let revoked = crate::auth::generate_oauth_access_token();
        let subject = format!("{}|{stable}", config.project_grant_id(&paths));
        db.insert_oauth_access_token(&crate::models::OAuthAccessTokenRecord {
            admin_authority: false,
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&access),
            client_id: first.client_id.clone(),
            subject_kind: crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
            subject_id: subject.clone(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        })
        .unwrap();
        db.insert_oauth_access_token(&crate::models::OAuthAccessTokenRecord {
            admin_authority: false,
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&revoked),
            client_id: first.client_id.clone(),
            subject_kind: crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
            subject_id: subject.clone(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: Some(now),
            last_used_at: None,
        })
        .unwrap();
        db.insert_oauth_refresh_token(&crate::models::OAuthRefreshTokenRecord {
            admin_authority: false,
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&refresh),
            client_id: first.client_id.clone(),
            subject_kind: crate::auth::PROJECT_SHARE_OAUTH_SUBJECT_KIND.to_string(),
            subject_id: subject,
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        })
        .unwrap();
        drop(db);

        // Repeated persisting restarts keep one validated client, one stable
        // session identity and the still-valid grants.
        for _ in 0..2 {
            let restarted = prepare_share_oauth_client(&config, &paths, redirect, true).unwrap();
            assert_eq!(restarted.client_id, first.client_id);
            assert_eq!(restarted.client_secret, first.client_secret);
            assert_eq!(
                crate::auth::project_share_session_id_for_client(&restarted.client_id).unwrap(),
                stable
            );
        }
        let db = crate::Database::open(&paths.data.join("webcodex.db")).unwrap();
        assert_eq!(
            db.get_oauth_access_token_by_hash(&crate::auth::hash_token(&access))
                .unwrap()
                .unwrap()
                .revoked_at,
            None
        );
        assert_eq!(
            db.get_oauth_refresh_token_by_hash(&crate::auth::hash_token(&refresh))
                .unwrap()
                .unwrap()
                .revoked_at,
            None
        );
        // A persisting restart never makes a revoked grant usable again; the
        // store only resolves unrevoked tokens, so the revoked one stays absent.
        assert!(db
            .get_oauth_access_token_by_hash(&crate::auth::hash_token(&revoked))
            .unwrap()
            .is_none());
        drop(db);

        // Switching back to the default mode retires the retained grants.
        prepare_share_oauth_client(&config, &paths, redirect, false).unwrap();
        let db = crate::Database::open(&paths.data.join("webcodex.db")).unwrap();
        assert!(db
            .get_oauth_access_token_by_hash(&crate::auth::hash_token(&access))
            .unwrap()
            .is_none());
        assert!(db
            .get_oauth_refresh_token_by_hash(&crate::auth::hash_token(&refresh))
            .unwrap()
            .is_none());
    }

    #[test]
    fn machine_share_ready_event_never_contains_handoff_secret_material() {
        let cloudflare = machine_share_ready_event(
            "demo",
            TunnelProvider::CloudflareQuick,
            ShareAuth::Bearer,
            false,
            "http://127.0.0.1:43210",
            "https://demo.trycloudflare.com",
            ClipboardCopyOutcome::Copied,
        );
        assert_eq!(cloudflare["event"], "ready");
        assert_eq!(cloudflare["experience"], "quick_share");
        assert_eq!(cloudflare["exposure"]["state"], "remote_ready");
        assert_eq!(
            cloudflare["connection"]["mcp_url"],
            "https://demo.trycloudflare.com/mcp"
        );
        assert_eq!(
            cloudflare["connection"]["clipboard_contains"],
            "bearer_credential"
        );
        let encoded = serde_json::to_string(&cloudflare).unwrap();
        assert!(!encoded.contains("wc_pair_"));
        assert!(!encoded.contains("webcodex_temporary"));

        let openai = machine_share_ready_event(
            "demo",
            TunnelProvider::OpenAiSecure,
            ShareAuth::Bearer,
            false,
            "http://127.0.0.1:43210",
            "http://127.0.0.1:43210",
            ClipboardCopyOutcome::Copied,
        );
        assert!(openai["connection"]["mcp_url"].is_null());
        assert_eq!(openai["connection"]["authentication"], "none");
        assert_eq!(openai["connection"]["clipboard_contains"], "tunnel_id");
        assert_eq!(openai["ready_for_chatgpt"], true);
    }

    #[test]
    fn share_output_contains_only_the_temporary_project_credential() {
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
        assert!(output.starts_with(
            "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\nWhat to do next"
        ));
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
        assert!(output.contains("Temporary share"));
        assert!(output.contains("This session ends when this command exits."));
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
        assert!(output.contains("Temporary share"));
        assert!(output.contains("This session ends when this command exits."));
        assert!(!output.contains("Credential (this share only)"));
        assert!(!output.contains("MCP URL:"));
    }

    #[test]
    fn oauth_share_output_keeps_project_credential_separate_from_client_secret() {
        let oauth = ShareOAuthClient {
            client_id: "wc_client_test".to_string(),
            client_secret: "wc_csec_test".to_string(),
            redirect_uri: "https://client.example/callback".to_string(),
            state_directory: PathBuf::new(),
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
        assert!(output.starts_with(
            "WebCodex ready\n\nTemporary share\nThis session ends when this command exits.\n\nWhat to do next"
        ));
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
        let persistent = state.join("credentials/project-credential");
        let persistent_value = generate_project_credential();
        write_new_private(&persistent, format!("{persistent_value}\n").as_bytes()).unwrap();
        let persistent_before = fs::read_to_string(&persistent).unwrap();
        let session = ShareSession::create(&state).unwrap();
        assert_ne!(session.credential, persistent_value);
        assert_eq!(fs::read_to_string(&persistent).unwrap(), persistent_before);
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
        }
        let directory = session.directory.clone();
        #[cfg(windows)]
        {
            for (path, directory) in [
                (state.join("share"), true),
                (session.directory.clone(), true),
                (session.credential_file.clone(), false),
            ] {
                let sddl =
                    super::super::windows_private_state::dacl_sddl(&path, directory).unwrap();
                assert!(sddl.starts_with("D:P"), "DACL must be protected: {sddl}");
                for broad in [";;;WD)", ";;;BU)", ";;;BA)"] {
                    assert!(
                        !sddl.contains(broad),
                        "broad Windows ACE {broad} remained: {sddl}"
                    );
                }
            }
        }
        drop(session);
        assert!(!directory.exists());
        assert!(persistent.is_file());
    }

    #[cfg(unix)]
    fn fake_cloudflared(script: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = crate::test_support::executable_tempdir();
        let path = temp.path().join("cloudflared");
        // A concurrent fork can inherit a writable handle to an executable inode
        // even after an atomic rename, so execute an immutable checked-in
        // entrypoint and keep each test's body in a sibling data file.
        fs::write(temp.path().join("cloudflared.script"), script).unwrap();
        std::os::unix::fs::symlink(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cloudflared.sh"),
            &path,
        )
        .unwrap();
        (temp, path)
    }

    #[cfg(unix)]
    #[test]
    fn fake_cloudflared_executes_with_open_fixture_writers() {
        let (temp, binary) = fake_cloudflared("#!/bin/sh\nprintf fixture-ready");
        // Model writable handles inherited by an unrelated concurrent fork.
        let writers: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(Result::unwrap)
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| {
                fs::OpenOptions::new()
                    .write(true)
                    .open(entry.path())
                    .unwrap()
            })
            .collect();
        assert!(!writers.is_empty());
        let output = std::process::Command::new(binary)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"fixture-ready");
        drop(writers);
    }

    #[cfg(unix)]
    fn tunnel_test_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        &LOCK
    }

    #[cfg(unix)]
    enum FakeCloudflaredLifecycle {
        StartupTimeout,
        Ready,
        ParentExit,
    }

    /// Make a fake cloudflared parent that creates its own long-lived shell
    /// descendant, writes that child's PID only after the background spawn
    /// succeeds, then follows the requested tunnel lifecycle. The background
    /// child inherits the parent's process group, exactly like a descendant
    /// cloudflared or npm wrapper leaves behind in production. It traps the
    /// term-able signals, so only the group-level SIGKILL used by cleanup can
    /// reap it: a stray SIGTERM/SIGINT code path would leave it alive and
    /// `assert_pid_gone` would fail. The subshell itself is the recorded
    /// descendant (no extra `sleep` child), so a failed background spawn
    /// leaves the PID file empty and the test fails instead of passing vacuously.
    #[cfg(unix)]
    fn fake_cloudflared_with_descendant(
        lifecycle: FakeCloudflaredLifecycle,
    ) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let lifecycle_script = match lifecycle {
            FakeCloudflaredLifecycle::StartupTimeout => "while :; do :; done\n",
            FakeCloudflaredLifecycle::Ready => {
                "echo https://cleanup-test.trycloudflare.com >&2\nwhile :; do :; done\n"
            }
            FakeCloudflaredLifecycle::ParentExit => {
                "echo https://short-lived.trycloudflare.com >&2\nexit 9\n"
            }
        };
        let script = format!(
            "#!/bin/sh\n( trap '' TERM INT HUP; while :; do :; done ) &\ndescendant=$!\nprintf '%s\\n' \"$descendant\" > \"$0.descendant-pid\"\n{lifecycle_script}"
        );
        let (temp, binary) = fake_cloudflared(&script);
        let pid_file = PathBuf::from(format!("{}.descendant-pid", binary.display()));
        (temp, binary, pid_file)
    }

    #[cfg(unix)]
    async fn wait_for_pid_file(path: &Path, deadline: Instant) -> u32 {
        loop {
            if let Ok(value) = fs::read_to_string(path) {
                if let Ok(pid) = value.trim().parse::<u32>() {
                    assert!(pid > 0, "fake cloudflared published pid 0");
                    return pid;
                }
            }
            assert!(
                Instant::now() < deadline,
                "fake cloudflared did not publish descendant pid at {}",
                path.display()
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(unix)]
    async fn assert_pid_gone(pid: u32, deadline: Instant) {
        loop {
            // SAFETY: `kill(pid, 0)` does not signal the process. It only checks
            // whether the kernel still has a process table entry for this exact
            // PID. The test's fake parent published this PID after spawning the
            // descendant, so ESRCH is the required proof that cleanup reaped it.
            let result = unsafe { libc::kill(pid as i32, 0) };
            if result == -1 {
                let error = std::io::Error::last_os_error();
                assert_eq!(
                    error.raw_os_error(),
                    Some(libc::ESRCH),
                    "descendant pid {pid} became inaccessible for an unexpected reason: {error}"
                );
                return;
            }
            assert!(
                Instant::now() < deadline,
                "pid {pid} still exists after the process-tree cleanup deadline"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cloudflare_quick_tunnel_startup_failure_reports_bounded_diagnostics() {
        let _lock = tunnel_test_lock().lock().await;
        let (_temp, binary) = fake_cloudflared("#!/bin/sh\necho startup-failed >&2\nexit 7\n");
        let error = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Instant::now() + TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("startup-failed"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cloudflare_quick_tunnel_cancelled_startup_reaps_descendant() {
        let _lock = tunnel_test_lock().lock().await;
        let (_temp, binary, pid_file) =
            fake_cloudflared_with_descendant(FakeCloudflaredLifecycle::StartupTimeout);
        let startup =
            spawn_cloudflare_quick_with_binary(&binary, "http://127.0.0.1:23456").unwrap();
        let parent_pid = startup.child.id().unwrap();
        let descendant_pid =
            wait_for_pid_file(&pid_file, Instant::now() + Duration::from_secs(2)).await;
        assert_ne!(parent_pid, descendant_pid);

        let cancelled = tokio::time::timeout(
            Duration::from_millis(20),
            wait_for_cloudflare_quick_url(startup, Instant::now() + Duration::from_secs(5)),
        )
        .await;
        assert!(cancelled.is_err(), "startup future unexpectedly completed");

        let deadline = Instant::now() + Duration::from_secs(5);
        assert_pid_gone(parent_pid, deadline).await;
        assert_pid_gone(descendant_pid, deadline).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cloudflare_quick_tunnel_startup_timeout_reaps_descendant() {
        let _lock = tunnel_test_lock().lock().await;
        let (_temp, binary, pid_file) =
            fake_cloudflared_with_descendant(FakeCloudflaredLifecycle::StartupTimeout);
        let startup =
            spawn_cloudflare_quick_with_binary(&binary, "http://127.0.0.1:23456").unwrap();
        let parent_pid = startup.child.id().unwrap();
        let descendant_pid =
            wait_for_pid_file(&pid_file, Instant::now() + Duration::from_secs(2)).await;
        assert_ne!(parent_pid, descendant_pid);
        let error = wait_for_cloudflare_quick_url(startup, Instant::now())
            .await
            .unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(
            error.message.contains("startup timeout"),
            "unexpected tunnel startup error: {}",
            error.message
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_pid_gone(parent_pid, deadline).await;
        assert_pid_gone(descendant_pid, deadline).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cloudflare_quick_tunnel_stop_reaps_descendant() {
        let _lock = tunnel_test_lock().lock().await;
        let (_temp, binary, pid_file) =
            fake_cloudflared_with_descendant(FakeCloudflaredLifecycle::Ready);
        let (_url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Instant::now() + TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap();
        let parent_pid = tunnel.child.id().unwrap();
        let descendant_pid =
            wait_for_pid_file(&pid_file, Instant::now() + Duration::from_secs(2)).await;
        assert_ne!(parent_pid, descendant_pid);
        tunnel.stop().await;
        assert!(tunnel.child.try_wait().unwrap().is_some());
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_pid_gone(descendant_pid, deadline).await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cloudflare_quick_tunnel_parent_exit_reaps_descendant() {
        let _lock = tunnel_test_lock().lock().await;
        let (_temp, binary, pid_file) =
            fake_cloudflared_with_descendant(FakeCloudflaredLifecycle::ParentExit);
        let (url, mut tunnel) = start_cloudflare_quick_with_binary(
            &binary,
            "http://127.0.0.1:23456",
            Instant::now() + TUNNEL_TEST_START_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(url, "https://short-lived.trycloudflare.com");
        let descendant_pid =
            wait_for_pid_file(&pid_file, Instant::now() + Duration::from_secs(2)).await;
        let error = tunnel.wait_for_exit().await.unwrap_err();
        assert_eq!(error.code, "tunnel_unavailable");
        assert!(error.message.contains("stopped unexpectedly"));
        let deadline = Instant::now() + Duration::from_secs(5);
        assert_pid_gone(descendant_pid, deadline).await;
    }

    fn share_root_fixture() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn share_args(extra: &[&str]) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "--auth".into(),
            "oauth".into(),
            "--oauth-redirect-uri".into(),
            "https://client.example/callback".into(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    }

    #[test]
    fn parse_share_project_roots_canonicalize_and_dedupe() {
        let temp = share_root_fixture();
        let root = temp.path().join("Projects");
        std::fs::create_dir_all(&root).unwrap();
        let canonical = root.canonicalize().unwrap();

        let parsed = parse_share_options(&share_args(&[
            "--project-roots",
            &root.to_string_lossy(),
            "--project-roots",
            &canonical.to_string_lossy(),
        ]))
        .unwrap();
        assert_eq!(parsed.oauth_project_roots, vec![canonical.clone()]);
        assert!(parsed.oauth_project_roots_explicit);
    }

    #[test]
    fn parse_share_project_roots_validate_inputs() {
        let temp = share_root_fixture();
        let root = temp.path().join("Projects");
        std::fs::create_dir_all(&root).unwrap();

        // Relative path rejected.
        assert!(parse_share_options(&share_args(&["--project-roots", "relative/dir"])).is_err());
        // Missing directory rejected.
        let missing = temp.path().join("missing").to_string_lossy().into_owned();
        assert!(parse_share_options(&share_args(&["--project-roots", &missing])).is_err());
        // A file is not a directory root.
        let file = temp.path().join("file");
        std::fs::write(&file, b"x").unwrap();
        let file = file.to_string_lossy().into_owned();
        assert!(parse_share_options(&share_args(&["--project-roots", &file])).is_err());
        // Missing value rejected.
        assert!(parse_share_options(&share_args(&["--project-roots"])).is_err());
        // Non-OAuth auth rejects the option.
        let bearer_args: Vec<String> = vec![
            "--auth".into(),
            "bearer".into(),
            "--project-roots".into(),
            root.to_string_lossy().into_owned(),
        ];
        assert!(parse_share_options(&bearer_args).is_err());
        // --no-project-roots is explicit but empty.
        let cleared = parse_share_options(&share_args(&["--no-project-roots"])).unwrap();
        assert!(cleared.oauth_project_roots.is_empty());
        assert!(cleared.oauth_project_roots_explicit);
    }

    #[test]
    fn share_project_roots_explicit_updates_are_idempotent_and_replaceable() {
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        let root_b = temp.path().join("b");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        let roots_a = vec![root_a.canonicalize().unwrap()];
        let roots_b = vec![root_b.canonicalize().unwrap()];
        let directory = temp.path().join("share-oauth");

        // First explicit grant persists.
        persist_share_project_roots(&directory, &roots_a).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
        let first = std::fs::read(&path).unwrap();

        // Same explicit grant again: idempotent success, identical content.
        persist_share_project_roots(&directory, &roots_a).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            first,
            "same-value update churned the file"
        );

        // A different explicit list replaces the previous one.
        persist_share_project_roots(&directory, &roots_b).unwrap();
        assert_eq!(
            read_persisted_share_project_roots(&directory),
            roots_b,
            "replacement grant not stored"
        );

        // Replacing an existing file never leaves temp segments behind.
        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".replace-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "temp segments left behind: {leftovers:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn share_project_roots_with_embedded_newline_do_not_widen_on_restart() {
        let temp = share_root_fixture();
        let nested = temp.path().join("granted\n").join("etc");
        std::fs::create_dir_all(&nested).unwrap();
        let original = nested.canonicalize().unwrap();
        let directory = temp.path().join("share-oauth");

        // A previously accepted directory path containing '\n' must never
        // become a grant to the unrelated system /etc on the next launch.
        persist_share_project_roots(&directory, &[original.clone()]).unwrap();
        let persisted = std::fs::read_to_string(directory.join(SHARE_PROJECT_ROOTS_FILE)).unwrap();
        assert!(persisted.contains(r"\n"), "newline must be escaped in JSON");
        assert!(!persisted.contains("granted\n"));

        let restored = read_persisted_share_project_roots(&directory);
        assert_eq!(restored, vec![original]);
        assert!(!restored.iter().any(|root| root == Path::new("/etc")));
    }

    #[test]
    fn share_project_roots_rejects_ambiguous_legacy_line_format() {
        let temp = share_root_fixture();
        let directory = temp.path().join("share-oauth");
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(SHARE_PROJECT_ROOTS_FILE),
            b"/tmp/granted\n/etc\n",
        )
        .unwrap();
        assert!(read_persisted_share_project_roots(&directory).is_empty());
    }

    #[test]
    fn share_project_roots_clear_is_idempotent_and_goes_fully_away() {
        let temp = share_root_fixture();
        let root = temp.path().join("a");
        std::fs::create_dir_all(&root).unwrap();
        let directory = temp.path().join("share-oauth");
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);

        // Clearing an absent grant is already the cleared state.
        persist_share_project_roots(&directory, &[]).unwrap();
        assert!(!path.exists());

        persist_share_project_roots(&directory, &[root.canonicalize().unwrap()]).unwrap();
        assert!(path.exists());
        persist_share_project_roots(&directory, &[]).unwrap();
        assert!(!path.exists(), "cleared grant left the roots file behind");
        // Repeated clear stays successful.
        persist_share_project_roots(&directory, &[]).unwrap();
        // And a later no-flag launch has nothing to restore.
        assert!(read_persisted_share_project_roots(&directory).is_empty());
    }

    #[test]
    fn share_project_roots_failed_update_leaves_previous_value_intact() {
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        let root_b = temp.path().join("b");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        let directory = temp.path().join("share-oauth");
        let roots_a = vec![root_a.canonicalize().unwrap()];
        let roots_b = vec![root_b.canonicalize().unwrap()];
        persist_share_project_roots(&directory, &roots_a).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
        let previous = std::fs::read(&path).unwrap();

        // Failure injection without a framework: occupy the target path with a
        // directory holding a sentinel, so the final same-directory rename
        // cannot replace it. (Making the parent read-only is not injectable:
        // create_private_dir deliberately re-asserts owner-only permissions,
        // which the self-healing test below pins instead.)
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        std::fs::write(path.join("sentinel"), b"occupied").unwrap();

        let update = persist_share_project_roots(&directory, &roots_b);
        assert!(
            update.is_err(),
            "update over a directory unexpectedly succeeded"
        );
        assert!(
            std::fs::read(path.join("sentinel")).unwrap() == b"occupied",
            "failed update disturbed the occupied target"
        );

        // Clearing against the same occupied path must also fail explicitly
        // instead of pretending the grant is gone.
        let clear = persist_share_project_roots(&directory, &[]);
        assert!(
            clear.is_err(),
            "clear over a directory unexpectedly succeeded"
        );
        assert!(path.is_dir(), "failed clear removed the occupied target");

        // Restore the regular file and prove the previous grant survives the
        // whole failed sequence.
        std::fs::remove_dir_all(&path).unwrap();
        std::fs::write(&path, &previous).unwrap();
        assert_eq!(
            read_persisted_share_project_roots(&directory),
            roots_a,
            "the previous grant was not preserved across the failed operations"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".replace-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "failed update left temp segments: {leftovers:?}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn share_project_roots_state_directory_self_heals_owner_only_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = share_root_fixture();
        let root = temp.path().join("a");
        std::fs::create_dir_all(&root).unwrap();
        let directory = temp.path().join("share-oauth");
        persist_share_project_roots(&directory, &[root.canonicalize().unwrap()]).unwrap();

        // A loosened directory must not stay loosened: the private-state
        // helpers re-assert owner-only protection on every write.
        let mut permissions = std::fs::metadata(&directory).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&directory, permissions).unwrap();
        persist_share_project_roots(&directory, &[root.canonicalize().unwrap()]).unwrap();
        let dir_mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "loosened state directory was not re-protected: {dir_mode:o}"
        );
    }

    #[test]
    fn share_project_roots_replacement_replaces_a_planted_symlink_not_its_target() {
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        let root_b = temp.path().join("b");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        let directory = temp.path().join("share-oauth");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);

        // Plant a symlink at the exact target path pointing outside the state
        // directory; the replacement must overwrite the link, never write
        // through it.
        let decoy = temp.path().join("decoy-roots");
        std::fs::write(&decoy, b"planted\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&decoy, &path).unwrap();
        #[cfg(not(unix))]
        std::fs::write(&path, b"planted\n").unwrap();

        let roots_b = vec![root_b.canonicalize().unwrap()];
        persist_share_project_roots(&directory, &roots_b).unwrap();

        assert_eq!(
            read_persisted_share_project_roots(&directory),
            roots_b,
            "replacement did not take effect over the planted link"
        );
        #[cfg(unix)]
        {
            let metadata = std::fs::symlink_metadata(&path).unwrap();
            assert!(
                !metadata.file_type().is_symlink(),
                "target is still a symlink"
            );
            assert_eq!(
                std::fs::read(&decoy).unwrap(),
                b"planted\n",
                "decoy was written through"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn share_project_roots_same_value_symlink_is_replaced_not_written_through() {
        use std::os::unix::fs::PermissionsExt;
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        std::fs::create_dir_all(&root_a).unwrap();
        let directory = temp.path().join("share-oauth");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
        let roots = vec![root_a.canonicalize().unwrap()];
        // The pre-existing state must be byte-identical to the current JSON
        // representation, so this exercises permission/target repair rather
        // than accidentally migrating a legacy line-delimited fixture.
        let content = serde_json::to_vec(&roots).unwrap();

        // Plant a symlink at the target pointing OUTSIDE the state directory,
        // holding exactly the bytes that would be stored.
        let outside = temp.path().join("outside-roots");
        std::fs::write(&outside, &content).unwrap();
        std::os::unix::fs::symlink(&outside, &path).unwrap();

        // Re-sending the identical grant must repair the target, not accept
        // the link (and never write through it).
        persist_share_project_roots(&directory, &roots).unwrap();

        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "symlink survived a same-value update"
        );
        assert!(
            metadata.is_file(),
            "target is not a regular file after repair"
        );
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "repaired target is not owner-private: {mode:o}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), content.as_slice());
        // The external target was never written through: it still holds the
        // planted bytes and remains a plain file outside the state directory.
        assert_eq!(std::fs::read(&outside).unwrap(), content.as_slice());
        assert!(std::fs::symlink_metadata(&outside).unwrap().is_file());
    }

    #[cfg(unix)]
    #[test]
    fn share_project_roots_same_value_wide_permissions_are_repaired() {
        use std::os::unix::fs::PermissionsExt;
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        std::fs::create_dir_all(&root_a).unwrap();
        let directory = temp.path().join("share-oauth");
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
        let roots = vec![root_a.canonicalize().unwrap()];
        // The pre-existing state must be byte-identical to the current JSON
        // representation, so this exercises permission/target repair rather
        // than accidentally migrating a legacy line-delimited fixture.
        let content = serde_json::to_vec(&roots).unwrap();

        // A same-content file with group/other read bits must not qualify for
        // the no-op fast path: the update repairs it to owner-only.
        std::fs::write(&path, &content).unwrap();
        let mut wide = std::fs::metadata(&path).unwrap().permissions();
        wide.set_mode(0o644);
        std::fs::set_permissions(&path, wide).unwrap();

        persist_share_project_roots(&directory, &roots).unwrap();

        let metadata = std::fs::symlink_metadata(&path).unwrap();
        assert!(metadata.is_file());
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "wide permissions survived a same-value update: {mode:o}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), content.as_slice());
        // A second same-value update now takes the no-op path and stays
        // owner-only (idempotence preserved after the repair).
        persist_share_project_roots(&directory, &roots).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn share_project_roots_replacement_keeps_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = share_root_fixture();
        let root = temp.path().join("a");
        std::fs::create_dir_all(&root).unwrap();
        let directory = temp.path().join("share-oauth");
        persist_share_project_roots(&directory, &[root.canonicalize().unwrap()]).unwrap();
        let path = directory.join(SHARE_PROJECT_ROOTS_FILE);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "roots file is not owner-private: {mode:o}");
        let dir_mode = std::fs::metadata(&directory).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            dir_mode, 0o700,
            "state directory is not owner-private: {dir_mode:o}"
        );
    }

    #[test]
    fn share_project_roots_persist_and_restore_round_trip() {
        let temp = share_root_fixture();
        let root_a = temp.path().join("a");
        let root_b = temp.path().join("b");
        std::fs::create_dir_all(&root_a).unwrap();
        std::fs::create_dir_all(&root_b).unwrap();
        let roots = vec![
            root_a.canonicalize().unwrap(),
            root_b.canonicalize().unwrap(),
        ];

        let directory = temp.path().join("share-oauth");
        persist_share_project_roots(&directory, &roots).unwrap();
        assert_eq!(
            read_persisted_share_project_roots(&directory),
            roots,
            "persisted roots must restore verbatim"
        );

        // An absent state directory means no grant.
        assert!(read_persisted_share_project_roots(&temp.path().join("absent")).is_empty());

        // Invalid entries in an otherwise well-formed JSON array are
        // skipped, without changing the valid grant. Invalid encodings and
        // the retired newline format are tested separately as fail-closed.
        let mixed = serde_json::to_vec(&[
            root_a.canonicalize().unwrap().to_str().unwrap(),
            "not-a-directory",
            "../escape",
        ])
        .unwrap();
        std::fs::write(directory.join(SHARE_PROJECT_ROOTS_FILE), mixed).unwrap();
        assert_eq!(
            read_persisted_share_project_roots(&directory),
            vec![root_a.canonicalize().unwrap()],
            "only still-valid roots survive restore"
        );
    }
}
