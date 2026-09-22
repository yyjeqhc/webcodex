//! Narrow, explicit first-party operator approval for an existing local Desktop
//! connection. No credential or hash is projected into the WebView or logs.
use crate::error::{DesktopError, DesktopResult};
use crate::models::StoredRuntime;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncReadExt;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRequest {
    pub expected: crate::webcodex::settings::SettingsTarget,
    pub confirmed: bool,
}

pub fn can_authorize(runtime: &StoredRuntime) -> bool {
    runtime.server_env_file.is_some() && local_url(runtime).is_ok()
}

fn local_url(runtime: &StoredRuntime) -> DesktopResult<url::Url> {
    let url = url::Url::parse(&runtime.server_url).map_err(|_| unavailable())?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]" | "::1"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(unavailable());
    }
    Ok(url)
}

/// Read only the generated operator assignment, not shell syntax. Reject
/// duplicate/interpolated/malformed values rather than evaluating an env file.
pub(crate) fn operator_token(content: &str) -> DesktopResult<String> {
    let mut token = None;
    for line in content.lines() {
        let line = line.trim_start();
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != "WEBCODEX_TOKEN" {
            continue;
        }
        if token.is_some() {
            return Err(unavailable());
        }
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        if value.is_empty()
            || value.len() > 16384
            || !value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.~+/=".contains(&b))
        {
            return Err(unavailable());
        }
        token = Some(value.to_owned());
    }
    token.ok_or_else(unavailable)
}

#[derive(Serialize)]
struct NativeGrant<'a> {
    client_id: &'a str,
    user_token_hash: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantResponse {
    success: bool,
    changed: bool,
    applied_scopes: Vec<String>,
}

pub async fn grant(runtime: &StoredRuntime, operator_token: String) -> DesktopResult<()> {
    let mut url = local_url(runtime)?;
    url.set_path("/api/pairing/runner-capabilities");
    let runner = runtime
        .runner_client_id
        .as_deref()
        .filter(|id| !id.is_empty())
        .ok_or_else(unavailable)?;
    let path = runtime.user_token_file.as_ref().ok_or_else(unavailable)?;
    let mut token = String::new();
    tokio::fs::File::open(path)
        .await
        .map_err(|_| unavailable())?
        .take(16385)
        .read_to_string(&mut token)
        .await
        .map_err(|_| unavailable())?;
    if token.len() > 16384 || token.trim().is_empty() {
        return Err(unavailable());
    }
    let user_token_hash = format!("{:x}", Sha256::digest(token.trim().as_bytes()));
    drop(token);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|_| unavailable())?;
    let result = client
        .post(url)
        .bearer_auth(&operator_token)
        .json(&NativeGrant {
            client_id: runner,
            user_token_hash,
        })
        .send()
        .await;
    drop(operator_token);
    let mut response = result.map_err(|_| unavailable())?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > 4096)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len().saturating_add(chunk.len()) > 4096 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    let response: GrantResponse = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
    if !response.success
        || response.applied_scopes.len() != 2
        || !response
            .applied_scopes
            .iter()
            .any(|scope| scope == "ssh:local")
        || !response
            .applied_scopes
            .iter()
            .any(|scope| scope == "coding_agent:run")
    {
        return Err(unavailable());
    }
    let _ = response.changed; // An already-applied operator grant is idempotent.
    Ok(())
}

pub fn unavailable() -> DesktopError {
    DesktopError::new("runner_capability_grant_unavailable", "Runner capability authorization was not confirmed",
        "Use the Desktop-owned local Server with operator authority, then refresh. Remote connections require a Server-operator-approved pairing code.")
}

#[cfg(test)]
mod tests;
