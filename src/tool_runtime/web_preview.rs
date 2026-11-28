use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use url::Url;

const PREVIEW_AGENT_SOURCE: &str = include_str!("../../integrations/web-preview/agent.mjs");
const PREVIEW_SESSION_TTL_SECS: u64 = 30 * 60;
/// A lease is dropped a minute early so an almost-expired preview is never offered.
const PREVIEW_LEASE_GRACE: Duration = Duration::from_secs(60);

/// Running previews, keyed by the exact Project the session was opened for.
///
/// In-memory only, like the Runner registry: a restart forgets previews. The Work
/// Result card reads this lease to offer its "preview" link; the Workbench keeps
/// using the URL the tool returned directly.
struct PreviewLease {
    url: String,
    expires: Instant,
}

static ACTIVE_PREVIEWS: OnceLock<Mutex<HashMap<String, PreviewLease>>> = OnceLock::new();

fn active_previews() -> &'static Mutex<HashMap<String, PreviewLease>> {
    ACTIVE_PREVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn record_preview_lease(project: &str, url: &str, ttl: Duration) {
    let Ok(mut map) = active_previews().lock() else {
        return;
    };
    let now = Instant::now();
    map.retain(|_, lease| lease.expires > now);
    map.insert(
        project.to_string(),
        PreviewLease {
            url: url.to_string(),
            expires: now + ttl.saturating_sub(PREVIEW_LEASE_GRACE),
        },
    );
}

/// Remember a ready preview for the project so the App can link to it.
pub(super) fn record_active_preview(project: &str, url: &str) {
    record_preview_lease(project, url, Duration::from_secs(PREVIEW_SESSION_TTL_SECS));
}

/// The project's live preview, only while its lease holds and it matches the
/// configured temporary preview origin.
pub(super) fn active_preview_url(project: &str) -> Option<String> {
    let raw = {
        let Ok(mut map) = active_previews().lock() else {
            return None;
        };
        let now = Instant::now();
        map.retain(|_, lease| lease.expires > now);
        map.get(project)?.url.clone()
    };
    valid_preview_url(&raw)
}

#[derive(Debug, Deserialize)]
struct GatewaySession {
    id: String,
    agent_token: String,
    agent_path: String,
    preview_url: String,
    expires_at: String,
}

#[derive(Debug, Deserialize)]
struct GatewaySessionStatus {
    connected: bool,
}

fn valid_preview_base_domain(domain: &str) -> bool {
    domain.len() <= 253
        && domain.split('.').count() >= 2
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn valid_preview_url_for_domain(raw: &str, domain: &str) -> Option<String> {
    if !valid_preview_base_domain(domain) {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    let suffix = format!(".{domain}");
    let subdomain = url.host_str()?.strip_suffix(&suffix)?;
    let id = subdomain.strip_prefix("wcprev-")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
        || id.len() != 32
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return None;
    }
    Some(url.to_string())
}

pub(super) fn valid_preview_url(raw: &str) -> Option<String> {
    let domain = std::env::var("WEBCODEX_PREVIEW_BASE_DOMAIN").ok()?;
    valid_preview_url_for_domain(raw, &domain)
}

fn valid_idempotency_key(value: &str) -> bool {
    (16..=100).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn namespaced_idempotency_key(project: &str, key: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(project.as_bytes());
    digest.update([0]);
    digest.update(key.as_bytes());
    format!("{:x}", digest.finalize())
}

fn gateway_base_url(raw: &str) -> Result<Url, &'static str> {
    let url = Url::parse(raw).map_err(|_| "invalid_preview_gateway_url")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err("invalid_preview_gateway_url");
    }
    Ok(url)
}

fn validate_gateway_session(
    session: &GatewaySession,
    domain: &str,
) -> Result<String, &'static str> {
    let valid_id = session.id.len() == 32
        && session
            .id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    let valid_token = (40..=128).contains(&session.agent_token.len())
        && session
            .agent_token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
    if !valid_id || !valid_token || session.agent_path != format!("/agent/{}", session.id) {
        return Err("invalid_preview_gateway_response");
    }
    let preview_url = valid_preview_url_for_domain(&session.preview_url, domain)
        .ok_or("invalid_preview_gateway_response")?;
    let expected_url = format!("https://wcprev-{}.{}/", session.id, domain);
    if preview_url != expected_url {
        return Err("invalid_preview_gateway_response");
    }
    Ok(preview_url)
}

fn gateway_session_url(base: &Url, id: &str) -> Result<Url, &'static str> {
    if id.len() != 32 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid_preview_session_id");
    }
    base.join(&format!("/admin/sessions/{id}"))
        .map_err(|_| "invalid_preview_gateway_url")
}

async fn create_gateway_session(
    client: &reqwest::Client,
    base: &Url,
    admin_token: &str,
    port: u16,
    idempotency_key: &str,
) -> Result<GatewaySession, String> {
    let url = base
        .join("/admin/sessions")
        .map_err(|_| "invalid_preview_gateway_url".to_string())?;
    let response = client
        .post(url)
        .bearer_auth(admin_token)
        .json(&json!({ "port": port, "idempotency_key": idempotency_key }))
        .send()
        .await
        .map_err(|_| "preview_gateway_unavailable".to_string())?;
    let status = response.status();
    let body = response
        .json::<Value>()
        .await
        .map_err(|_| "invalid_preview_gateway_response".to_string())?;
    if !status.is_success() {
        let code = body["error"].as_str().unwrap_or("gateway_error");
        return Err(format!(
            "preview_gateway_rejected:{}:{code}",
            status.as_u16()
        ));
    }
    serde_json::from_value(body).map_err(|_| "invalid_preview_gateway_response".to_string())
}

async fn gateway_session_connected(
    client: &reqwest::Client,
    base: &Url,
    admin_token: &str,
    id: &str,
) -> Result<bool, String> {
    let url = gateway_session_url(base, id).map_err(str::to_string)?;
    let response = client
        .get(url)
        .bearer_auth(admin_token)
        .send()
        .await
        .map_err(|_| "preview_gateway_status_unavailable".to_string())?;
    if !response.status().is_success() {
        return Err("preview_gateway_status_unavailable".to_string());
    }
    let status = response
        .json::<GatewaySessionStatus>()
        .await
        .map_err(|_| "invalid_preview_gateway_status".to_string())?;
    Ok(status.connected)
}

async fn close_gateway_session(client: &reqwest::Client, base: &Url, admin_token: &str, id: &str) {
    let Ok(url) = gateway_session_url(base, id) else {
        return;
    };
    let _ = client.delete(url).bearer_auth(admin_token).send().await;
}

fn preview_failure(
    message: impl Into<String>,
    project: &str,
    port: u16,
    failure_kind: &str,
    retry_with_new_key: bool,
    job_id: Option<&str>,
) -> ToolResult {
    ToolResult::err_with_output(
        message.into(),
        json!({
            "preview_url": null,
            "job_id": job_id,
            "expires_at": null,
            "project": project,
            "port": port,
            "status": "failed",
            "failure_kind": failure_kind,
            "retry_with_new_key": retry_with_new_key,
        }),
    )
}

impl ToolRuntime {
    pub(crate) async fn start_web_preview(
        &self,
        project: String,
        port: u16,
        idempotency_key: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if port == 0 {
            return preview_failure(
                "Choose a valid local HTTP port.",
                &project,
                port,
                "invalid_port",
                false,
                None,
            );
        }
        if !valid_idempotency_key(&idempotency_key) {
            return preview_failure(
                "Invalid preview idempotency key.",
                &project,
                port,
                "invalid_idempotency_key",
                false,
                None,
            );
        }
        if super::kernel::check_runtime_tool_scope(auth, "list_projects").is_err()
            || super::kernel::check_runtime_tool_scope(auth, "run_detached_process").is_err()
        {
            return preview_failure(
                "Preview permission is unavailable for this account.",
                &project,
                port,
                "permission_denied",
                false,
                None,
            );
        }
        let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
            Ok(resolved) => resolved,
            Err(_) => {
                return preview_failure(
                    "The selected Runner Project is unavailable.",
                    &project,
                    port,
                    "project_unavailable",
                    false,
                    None,
                );
            }
        };
        let project_id = resolved.resolved_id;
        let gateway_raw = match std::env::var("WEBCODEX_PREVIEW_GATEWAY_URL") {
            Ok(value) => value,
            Err(_) => {
                return preview_failure(
                    "WebCodex preview gateway is not configured on the Server.",
                    &project_id,
                    port,
                    "preview_not_configured",
                    false,
                    None,
                );
            }
        };
        let admin_token = match std::env::var("WEBCODEX_PREVIEW_ADMIN_TOKEN") {
            Ok(value) if value.len() >= 24 => value,
            _ => {
                return preview_failure(
                    "WebCodex preview gateway credentials are not configured on the Server.",
                    &project_id,
                    port,
                    "preview_not_configured",
                    false,
                    None,
                );
            }
        };
        let domain = match std::env::var("WEBCODEX_PREVIEW_BASE_DOMAIN") {
            Ok(value) if valid_preview_base_domain(&value) => value,
            _ => {
                return preview_failure(
                    "WebCodex preview base domain is not configured on the Server.",
                    &project_id,
                    port,
                    "preview_not_configured",
                    false,
                    None,
                );
            }
        };
        let base = match gateway_base_url(&gateway_raw) {
            Ok(base) => base,
            Err(kind) => {
                return preview_failure(
                    "WebCodex preview gateway URL must be an HTTPS origin.",
                    &project_id,
                    port,
                    kind,
                    false,
                    None,
                );
            }
        };
        let client = match reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        {
            Ok(client) => client,
            Err(_) => {
                return preview_failure(
                    "Unable to initialize the preview gateway client.",
                    &project_id,
                    port,
                    "preview_client_unavailable",
                    false,
                    None,
                );
            }
        };

        let gateway_key = namespaced_idempotency_key(&project_id, &idempotency_key);
        let session =
            match create_gateway_session(&client, &base, &admin_token, port, &gateway_key).await {
                Ok(session) => session,
                Err(kind) => {
                    return preview_failure(
                        format!("Unable to create preview session: {kind}"),
                        &project_id,
                        port,
                        "preview_gateway_rejected",
                        kind.contains("idempotency_conflict"),
                        None,
                    );
                }
            };
        let preview_url = match validate_gateway_session(&session, &domain) {
            Ok(url) => {
                record_active_preview(&project_id, &url);
                url
            }
            Err(kind) => {
                close_gateway_session(&client, &base, &admin_token, &session.id).await;
                return preview_failure(
                    "Preview gateway returned an invalid session.",
                    &project_id,
                    port,
                    kind,
                    true,
                    None,
                );
            }
        };
        let mut agent_url = base.clone();
        if agent_url.set_scheme("wss").is_err() {
            close_gateway_session(&client, &base, &admin_token, &session.id).await;
            return preview_failure(
                "Unable to construct the Runner preview connection.",
                &project_id,
                port,
                "invalid_preview_gateway_url",
                false,
                None,
            );
        }
        agent_url.set_path(&session.agent_path);
        let agent_program = format!(
            "{PREVIEW_AGENT_SOURCE}\nconst previewConfig = await readPreviewConfig();\nawait connectPreviewAgent(previewConfig);\n"
        );
        let agent_stdin = serde_json::to_string(&json!({
            "gatewayUrl": agent_url.as_str(),
            "agentToken": session.agent_token,
            "port": port,
        }))
        .unwrap_or_default();
        let job = self
            .run_detached_process_with_contract(
                project_id.clone(),
                format!("web-preview-{gateway_key}"),
                "node".to_string(),
                vec![
                    "--input-type=module".to_string(),
                    "-e".to_string(),
                    agent_program,
                ],
                Some(agent_stdin),
                Some(PREVIEW_SESSION_TTL_SECS),
                Some(".".to_string()),
                None,
                None,
                None,
                auth,
            )
            .await;
        let job_id = job.output["job_id"].as_str().map(str::to_string);
        let Some(job_id) = job_id else {
            close_gateway_session(&client, &base, &admin_token, &session.id).await;
            let failure_kind = job.output["failure_kind"]
                .as_str()
                .unwrap_or("runner_process_rejected");
            let message = job
                .error
                .unwrap_or_else(|| "Runner could not start the preview agent.".to_string());
            return preview_failure(message, &project_id, port, failure_kind, false, None);
        };

        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        let mut connected = false;
        while tokio::time::Instant::now() < deadline {
            if gateway_session_connected(&client, &base, &admin_token, &session.id)
                .await
                .unwrap_or(false)
            {
                connected = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        if !connected {
            close_gateway_session(&client, &base, &admin_token, &session.id).await;
            return preview_failure(
                "Runner preview agent did not connect. Ensure Node.js 22+ is installed and the selected port is serving HTTP on 127.0.0.1.",
                &project_id,
                port,
                "preview_agent_not_connected",
                true,
                Some(&job_id),
            );
        }

        ToolResult::ok(json!({
            "preview_url": preview_url,
            "job_id": job_id,
            "expires_at": session.expires_at,
            "project": project_id,
            "port": port,
            "status": "ready",
            "failure_kind": null,
            "retry_with_new_key": false,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_urls_are_limited_to_the_configured_temporary_origin() {
        let id = "0123456789abcdef0123456789abcdef";
        let valid = format!("https://wcprev-{id}.zetx.tech/");
        assert_eq!(
            valid_preview_url_for_domain(&valid, "zetx.tech").as_deref(),
            Some(valid.as_str())
        );
        assert!(valid_preview_url_for_domain("https://example.com/", "zetx.tech").is_none());
        assert!(valid_preview_url_for_domain(
            &format!("http://wcprev-{id}.zetx.tech/"),
            "zetx.tech"
        )
        .is_none());
        assert!(valid_preview_url_for_domain(
            &format!("https://wcprev-{id}.other.tech/"),
            "zetx.tech"
        )
        .is_none());
        assert!(valid_preview_url_for_domain(
            &format!("https://wcprev-{id}.zetx.tech:8443/"),
            "zetx.tech"
        )
        .is_none());
    }

    #[test]
    fn preview_leases_are_per_project_and_expire() {
        let id = "0123456789abcdef0123456789abcdef";
        let url = format!("https://wcprev-{id}.zetx.tech/");
        record_preview_lease("agent:runner-a:project", &url, Duration::from_secs(600));
        let live = active_previews()
            .lock()
            .ok()
            .and_then(|map| map.get("agent:runner-a:project").map(|lease| lease.url.clone()));
        assert_eq!(live.as_deref(), Some(url.as_str()));
        assert!(!active_previews()
            .lock()
            .map(|map| map.contains_key("agent:runner-b:project"))
            .unwrap_or(true));
        // A lease recorded with no remaining life is pruned on the next read.
        record_preview_lease("agent:runner-a:project", &url, PREVIEW_LEASE_GRACE);
        assert!(!active_previews()
            .lock()
            .map(|map| map.contains_key("agent:runner-a:project"))
            .unwrap_or(true));
    }

    #[test]
    fn preview_idempotency_is_namespaced_by_project_and_stable() {
        let key = "b9a7c1a4-6b0c-4e18-9c24";
        let first = namespaced_idempotency_key("agent:runner-a:project", key);
        assert_eq!(
            first,
            namespaced_idempotency_key("agent:runner-a:project", key)
        );
        assert_ne!(
            first,
            namespaced_idempotency_key("agent:runner-b:project", key)
        );
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn embedded_agent_fits_runner_native_argument_limit() {
        let program = format!(
            "{PREVIEW_AGENT_SOURCE}\nconst previewConfig = await readPreviewConfig();\nawait connectPreviewAgent(previewConfig);\n"
        );
        assert!(program.len() <= crate::runner_protocol::PROCESS_ARG_MAX_BYTES);
        assert!(!program.contains('\0'));
    }

    #[test]
    fn gateway_origin_requires_https_and_no_extra_path() {
        assert!(gateway_base_url("https://codex.example.com").is_ok());
        assert!(gateway_base_url("http://codex.example.com").is_err());
        assert!(gateway_base_url("https://codex.example.com/path").is_err());
        assert!(gateway_base_url("https://user@codex.example.com").is_err());
    }
}
