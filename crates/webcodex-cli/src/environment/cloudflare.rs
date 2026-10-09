//! Explicit Cloudflare desired-state and owner-fenced control handoffs.
use super::*;
use crate::ServerTunnelProvider;
use serde_json::{json, Value};
use std::time::Duration;

const CONTROL_ACTION_REJECTED: &str = "The owning Server rejected the Cloudflare action; refresh its profile and runtime status before retrying";
const INGRESS_RESTART_REQUIRED: &str = "The owning Server has not applied its Cloudflare ingress configuration; restart that Server, then refresh its profile status and retry";

pub(super) fn validate_input(input: &Input) -> Result<(), String> {
    if (input.tunnel_provider.is_some()
        || input.public_origin.is_some()
        || input.tunnel_id.is_some()
        || input.name.is_some()
        || input.autostart.is_some()
        || input.ingress_port.is_some())
        && input.command != "configure-tunnel"
    {
        return Err("Tunnel provider configuration options apply only to configure-tunnel".into());
    }
    if input.ingress_port == Some(0) || input.expected_revision == Some(0) {
        return Err("Port and revision values must be nonzero".into());
    }
    if input.redirect_uri.is_some() || input.scopes.is_some() || input.replace {
        if input.command != "cloudflare-oauth" {
            return Err("OAuth options apply only to cloudflare-oauth".into());
        }
    }
    if input.command.starts_with("cloudflare-")
        && (input.operand.is_none() || input.profile.is_some() || input.token_file.is_some())
    {
        return Err("Cloudflare control requires one exact profile operand and uses its saved private binding".into());
    }
    if matches!(input.command.as_str(), "start" | "restart")
        && input.expected_revision.is_some()
        && input.operand.as_deref() != Some("tunnel")
    {
        return Err("--expected-revision identifies a Tunnel profile".into());
    }
    if input.command == "cloudflare-oauth" && input.redirect_uri.is_none() {
        return Err("cloudflare-oauth requires --redirect-uri".into());
    }
    if input.expected_revision.is_some()
        && !matches!(
            input.command.as_str(),
            "configure-tunnel" | "cloudflare-start" | "start" | "restart" | "remove-tunnel"
        )
    {
        return Err(
            "--expected-revision applies only to profile configuration, start or removal".into(),
        );
    }
    if input.scopes.as_ref().is_some_and(|scopes| {
        scopes.len() > 32
            || scopes.iter().any(|scope| {
                scope.is_empty()
                    || scope.len() > 128
                    || !scope.bytes().all(|byte| byte.is_ascii_graphic())
            })
    }) {
        return Err("OAuth scopes exceed their bounded string-array contract".into());
    }
    if input.command == "configure-tunnel"
        && input.token_file.is_some()
        && input.credentials_file.is_some()
    {
        return Err("Choose --token-file or --credentials-file for Cloudflare credentials".into());
    }
    if input.tunnel_provider == Some(ServerTunnelProvider::CloudflareQuick)
        && (input.token_file.is_some()
            || input.credentials_file.is_some()
            || input.tunnel_id.is_some()
            || input.public_origin.is_some())
    {
        return Err("Quick Tunnel profiles do not accept credentials, a Tunnel identity or a saved public origin".into());
    }
    Ok(())
}

pub(super) fn selected_provider(
    store: &EnvironmentStore,
    input: &Input,
) -> Result<ServerTunnelProvider, String> {
    let saved = tunnel_profiles(store).map_err(|error| error.to_string())?;
    let provider = input.tunnel_provider.unwrap_or_else(|| {
        match saved
            .iter()
            .find(|profile| profile.profile_id == input.operand.as_deref().unwrap_or("default"))
            .map(|profile| &profile.provider)
        {
            Some(TunnelProvider::CloudflareNamed { .. }) => ServerTunnelProvider::CloudflareNamed,
            Some(TunnelProvider::CloudflareQuick) => ServerTunnelProvider::CloudflareQuick,
            _ => ServerTunnelProvider::Openai,
        }
    });
    if provider == ServerTunnelProvider::Openai
        && (input.token_file.is_some()
            || input.public_origin.is_some()
            || input.tunnel_id.is_some()
            || input.ingress_port.is_some())
    {
        return Err("Cloudflare options require the selected Cloudflare provider; OpenAI credentials use --credentials-file".into());
    }
    Ok(provider)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedCredentials {
    #[serde(default)]
    tunnel_id: Option<String>,
    #[serde(default)]
    public_origin: Option<String>,
    #[serde(default)]
    token: Option<String>,
}

fn read_named_credentials(path: &Path) -> Result<NamedCredentials, String> {
    let content = read_secret(path).map_err(|error| error.to_string())?;
    if content.expose().len() > 32 * 1024 {
        return Err("Cloudflare credential file exceeds its bounded size".into());
    }
    serde_json::from_str(content.expose()).map_err(|_| "Cloudflare credential file must contain only tunnel_id, public_origin and token strings".into())
}

fn one_value(
    flag: Option<&str>,
    file: Option<&str>,
    saved: Option<&str>,
) -> Result<String, String> {
    if flag.is_some() && file.is_some() {
        return Err("Specify each Named Tunnel identity/origin only once, in options or the protected credential file".into());
    }
    flag.or(file).or(saved).map(str::to_owned).ok_or_else(|| "Named Tunnel requires --public-origin and --tunnel-id (or their protected credential-file fields)".into())
}

pub(super) async fn configure(
    store: &EnvironmentStore,
    backend: &NativeEnvironment,
    input: &Input,
) -> Result<String, String> {
    let id = input.operand.as_deref().unwrap_or("default");
    let profiles = tunnel_profiles(store).map_err(|error| error.to_string())?;
    let saved = profiles.iter().find(|profile| profile.profile_id == id);
    let credentials = input
        .credentials_file
        .as_ref()
        .map(|path| read_named_credentials(&absolute(path)?))
        .transpose()?;
    let kind = selected_provider(store, input)?;
    let (saved_origin, saved_identity) = match saved.map(|profile| &profile.provider) {
        Some(TunnelProvider::CloudflareNamed {
            public_origin,
            tunnel_id,
        }) => (Some(public_origin.as_str()), Some(tunnel_id.as_str())),
        _ => (None, None),
    };
    let provider = match kind {
        ServerTunnelProvider::CloudflareNamed => TunnelProvider::CloudflareNamed {
            public_origin: one_value(
                input.public_origin.as_deref(),
                credentials
                    .as_ref()
                    .and_then(|file| file.public_origin.as_deref()),
                saved_origin,
            )?,
            tunnel_id: one_value(
                input.tunnel_id.as_deref(),
                credentials
                    .as_ref()
                    .and_then(|file| file.tunnel_id.as_deref()),
                saved_identity,
            )?,
        },
        ServerTunnelProvider::CloudflareQuick
            if credentials.is_none()
                && input.token_file.is_none()
                && input.public_origin.is_none()
                && input.tunnel_id.is_none() =>
        {
            TunnelProvider::CloudflareQuick
        }
        _ => return Err("Quick Tunnel profiles accept no saved origin or credentials".into()),
    };
    let token = if let Some(path) = &input.token_file {
        Some(read_secret(&absolute(path)?).map_err(|error| error.to_string())?)
    } else {
        credentials.and_then(|file| file.token).map(Secret::new)
    };
    let request = CloudflareTunnelProfileRequest {
        profile_id: id,
        name: input.name.as_deref(),
        provider,
        host_mode: input
            .tunnel_host
            .or_else(|| saved.map(|profile| profile.host_mode))
            .unwrap_or(TunnelHostMode::Embedded),
        autostart: input
            .autostart
            .or_else(|| saved.map(|profile| profile.autostart))
            .unwrap_or(true),
        expected_revision: input.expected_revision,
        token: token.as_ref(),
        ingress_port: input.ingress_port,
    };
    let result = backend
        .configure_cloudflare_tunnel_profile(store, &request)
        .await
        .map_err(|error| error.to_string())?;
    if input.json {
        return serde_json::to_string_pretty(&result)
            .map_err(|_| "Could not encode Cloudflare configuration".into());
    }
    let target = result
        .profile
        .ingress_port
        .map(|port| format!("http://127.0.0.1:{port}"))
        .unwrap_or_else(|| "unavailable".into());
    Ok(format!(
        "{id}: Cloudflare profile saved (revision {}); local ingress {target}; next action {:?}",
        result.profile.revision, result.next_action
    ))
}

async fn control(
    profile: &CloudflareTunnelRuntimeProfile,
    request: Value,
) -> Result<Value, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "Could not prepare local Cloudflare control request")?;
    let mut response = client
        .post(format!(
            "{}/api/connections/cloudflare",
            profile.local_server_url.trim_end_matches('/')
        ))
        .bearer_auth(profile.bootstrap_token.expose())
        .json(&request)
        .send()
        .await
        .map_err(|_| "The owning Server's Cloudflare control endpoint is unavailable")?;
    let status = response.status();
    if !status.is_success() && status != reqwest::StatusCode::SERVICE_UNAVAILABLE {
        return Err(CONTROL_ACTION_REJECTED.into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        if status.is_success() {
            "Could not read local Cloudflare control response"
        } else {
            CONTROL_ACTION_REJECTED
        }
    })? {
        if bytes.len().saturating_add(chunk.len()) > 64 * 1024 {
            return Err(if status.is_success() {
                "Local Cloudflare control response exceeds its bounded size".into()
            } else {
                CONTROL_ACTION_REJECTED.into()
            });
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RejectedControlResponse {
            error: String,
            next_action: String,
        }
        // Error bodies are untrusted. Recognize only this restart contract;
        // never project response text or credentials into a CLI error.
        let restart_required =
            serde_json::from_slice::<RejectedControlResponse>(&bytes).is_ok_and(|rejected| {
                rejected.error == "cloudflare_ingress_not_applied"
                    && rejected.next_action == "restart_server"
            });
        return Err(if restart_required {
            INGRESS_RESTART_REQUIRED.into()
        } else {
            CONTROL_ACTION_REJECTED.into()
        });
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "The owning Server returned an invalid Cloudflare response")?;
    project_response(
        profile,
        request
            .get("action")
            .and_then(Value::as_str)
            .ok_or("Invalid Cloudflare action")?,
        value,
    )
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SafeStatus {
    profile_id: String,
    server_instance_id: String,
    process_generation: i64,
    lifecycle: String,
    public_origin: Option<String>,
    oauth_configured: bool,
    observed_authorization: bool,
    configured_revision: u64,
    applied_revision: Option<u64>,
    local_target: String,
    reason_code: Option<String>,
}
#[derive(serde::Deserialize, serde::Serialize)]
struct IssuedOauth {
    client_id: String,
    client_secret: Option<String>,
    already_configured: bool,
}
fn project_response(
    profile: &CloudflareTunnelRuntimeProfile,
    action: &str,
    value: Value,
) -> Result<Value, String> {
    if action == "configure_oauth" {
        let issued: IssuedOauth = serde_json::from_value(value)
            .map_err(|_| "The owning Server returned an invalid OAuth result")?;
        if issued.client_id.is_empty()
            || issued.client_id.len() > 256
            || issued
                .client_secret
                .as_ref()
                .is_some_and(|secret| secret.len() > 8192)
        {
            return Err("The owning Server returned an invalid OAuth result".into());
        }
        return serde_json::to_value(issued)
            .map_err(|_| "Could not project the OAuth result".into());
    }
    let status: SafeStatus = serde_json::from_value(value)
        .map_err(|_| "The owning Server returned an invalid Cloudflare status")?;
    if status.profile_id != profile.profile_id
        || status.server_instance_id.is_empty()
        || status.server_instance_id.len() > 128
        || status.process_generation < 0
    {
        return Err("The owning Server returned a mismatched Cloudflare profile status".into());
    }
    serde_json::to_value(status).map_err(|_| "Could not project Cloudflare status".into())
}

fn server_instance(status: &Value) -> Result<&str, String> {
    status
        .get("server_instance_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty() && id.len() <= 128)
        .ok_or_else(|| "Refresh Cloudflare status from a compatible owning Server".into())
}

fn action_request(
    profile: &CloudflareTunnelRuntimeProfile,
    action: &str,
    expected_revision: Option<u64>,
    status: &Value,
) -> Result<Value, String> {
    let server_instance_id = server_instance(status)?;
    match action {
        "start" => Ok(
            json!({"action":"start","profile_id":profile.profile_id,"expected_revision":expected_revision.unwrap_or(profile.revision),"server_instance_id":server_instance_id}),
        ),
        "stop" => {
            let generation = status
                .get("process_generation")
                .and_then(Value::as_u64)
                .ok_or("Cloudflare status does not identify a current process generation")?;
            Ok(
                json!({"action":"stop","profile_id":profile.profile_id,"server_instance_id":server_instance_id,"process_generation":generation}),
            )
        }
        _ => Err("Unsupported Cloudflare profile action".into()),
    }
}

fn oauth_request(input: &Input, id: &str, status: &Value) -> Result<Value, String> {
    let process_generation = status
        .get("process_generation")
        .and_then(Value::as_i64)
        .filter(|generation| *generation > 0)
        .ok_or("Cloudflare status does not identify a current process generation")?;
    Ok(json!({
        "action":"configure_oauth",
        "profile_id":id,
        "server_instance_id":server_instance(status)?,
        "process_generation":process_generation,
        "redirect_uri":input.redirect_uri,
        "scopes":input.scopes.clone().unwrap_or_default(),
        "replace":input.replace,
    }))
}

pub(super) async fn run_profile_action(
    store: &EnvironmentStore,
    backend: &NativeEnvironment,
    id: &str,
    action: &str,
    expected_revision: Option<u64>,
    json_output: bool,
) -> Result<String, String> {
    let environment_id = store
        .load_environment()
        .map_err(|error| error.to_string())?
        .ok_or("No saved environment is available")?
        .environment_id;
    let profile = cloudflare_tunnel_profile(store, id).map_err(|error| error.to_string())?;
    if expected_revision.is_some_and(|revision| revision != profile.revision) {
        return Err("The Cloudflare profile changed; reload it before starting".into());
    }
    let value = if profile.host_mode == TunnelHostMode::Standalone
        && matches!(action, "start" | "stop")
    {
        let operation = if action == "start" {
            ServiceOperation::Start
        } else {
            ServiceOperation::Stop
        };
        let status = backend
            .control_cloudflare_tunnel(store, &environment_id, &profile, operation)
            .await
            .map_err(|error| error.to_string())?;
        serde_json::to_value(status).map_err(|_| "Could not encode standalone Cloudflare status")?
    } else {
        let status = control(&profile, json!({"action":"status","profile_id":id})).await?;
        if action == "status" {
            status
        } else {
            control(
                &profile,
                action_request(&profile, action, expected_revision, &status)?,
            )
            .await?
        }
    };
    render(value, json_output)
}

pub(super) async fn run_control(
    store: &EnvironmentStore,
    backend: &NativeEnvironment,
    input: &Input,
) -> Result<String, String> {
    let id = input
        .operand
        .as_deref()
        .ok_or("Specify an exact Cloudflare profile")?;
    let action = match input.command.as_str() {
        "cloudflare-start" => "start",
        "cloudflare-stop" => "stop",
        "cloudflare-status" => "status",
        "cloudflare-oauth" => "configure_oauth",
        _ => return Err("Unsupported Cloudflare command".into()),
    };
    if action != "configure_oauth" {
        return run_profile_action(
            store,
            backend,
            id,
            action,
            input.expected_revision,
            input.json,
        )
        .await;
    }
    let profile = cloudflare_tunnel_profile(store, id).map_err(|error| error.to_string())?;
    let status = control(&profile, json!({"action":"status","profile_id":id})).await?;
    let response = control(&profile, oauth_request(input, id, &status)?).await?;
    // This explicit credential issuance command returns the first-time secret
    // once to its caller. Errors never echo response bodies or credential input.
    render(response, input.json)
}

fn render(value: Value, json_output: bool) -> Result<String, String> {
    if json_output {
        return serde_json::to_string_pretty(&value)
            .map_err(|_| "Could not encode Cloudflare result".into());
    }
    if let Some(id) = value.get("client_id").and_then(Value::as_str) {
        let mut output = format!("OAuth client ID: {id}");
        if let Some(secret) = value.get("client_secret").and_then(Value::as_str) {
            output.push_str(&format!("\nOAuth client secret (shown once): {secret}"));
        } else {
            output.push_str("\nOAuth is already configured; its existing secret is retained.");
        }
        return Ok(output);
    }
    serde_json::to_string_pretty(&value).map_err(|_| "Could not encode Cloudflare result".into())
}

#[cfg(test)]
mod tests;
