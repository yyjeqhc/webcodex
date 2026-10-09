//! Private native control-plane adapter. Runtime credentials stay in Rust; only
//! the explicit OAuth handoff response can carry the newly issued client secret.
use super::*;
use serde::{Deserialize, Serialize, Serializer};
use webcodex_environment::{Secret, TunnelProvider};

#[derive(Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum CloudflareConnectionRequest {
    Status {
        profile_id: String,
    },
    Start {
        profile_id: String,
        server_instance_id: String,
        expected_revision: u64,
    },
    Stop {
        profile_id: String,
        server_instance_id: String,
        process_generation: u64,
    },
    ConfigureOauth {
        profile_id: String,
        server_instance_id: String,
        process_generation: u64,
        redirect_uri: String,
        scopes: Vec<String>,
        #[serde(default)]
        replace: bool,
    },
}
impl CloudflareConnectionRequest {
    fn profile_id(&self) -> &str {
        match self {
            Self::Status { profile_id }
            | Self::Start { profile_id, .. }
            | Self::Stop { profile_id, .. }
            | Self::ConfigureOauth { profile_id, .. } => profile_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareConnectionStatus {
    pub profile_id: String,
    pub server_instance_id: String,
    pub process_generation: u64,
    pub lifecycle: String,
    pub public_origin: Option<String>,
    pub oauth_configured: bool,
    pub observed_authorization: bool,
    pub configured_revision: u64,
    pub applied_revision: Option<u64>,
    pub local_target: String,
    pub reason_code: Option<String>,
}

#[derive(Serialize)]
pub struct CloudflareOAuthHandoff {
    pub client_id: String,
    pub already_configured: bool,
    #[serde(serialize_with = "serialize_secret")]
    client_secret: Option<Secret>,
}
fn serialize_secret<S: Serializer>(
    value: &Option<Secret>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(secret) => serializer.serialize_some(secret.expose()),
        None => serializer.serialize_none(),
    }
}
#[derive(Serialize)]
#[serde(untagged)]
pub enum CloudflareConnectionResponse {
    Status(CloudflareConnectionStatus),
    OAuth(CloudflareOAuthHandoff),
}

fn unavailable() -> DesktopError {
    DesktopError::new(
        "cloudflare_control_unavailable",
        "The selected Cloudflare connection could not be controlled",
        "Check the local Server and refresh this connection before retrying.",
    )
}
fn requires_environment() -> DesktopError {
    DesktopError::new(
        "cloudflare_environment_required",
        "Cloudflare needs a persistent local Environment",
        "Use Runtime setup or repair to adopt the existing local Server first.",
    )
}

impl AppState {
    pub async fn cloudflare_connection(
        &self,
        request: CloudflareConnectionRequest,
    ) -> DesktopResult<CloudflareConnectionResponse> {
        let state = self.get_state();
        let expected_environment = state
            .persistent_environment
            .as_deref()
            .ok_or_else(requires_environment)?;
        if !state.topology.as_ref().is_some_and(|topology| {
            topology.experience == Experience::Full
                && matches!(topology.server, ServerTopology::Local)
        }) {
            return Err(requires_environment());
        }
        let store = environment::store()?;
        let saved = store
            .load_environment()
            .map_err(environment::desktop_error)?
            .ok_or_else(requires_environment)?;
        if saved.environment_id != expected_environment || !saved.request.local_server() {
            return Err(requires_environment());
        }
        let profile = webcodex_environment::cloudflare_tunnel_profile(&store, request.profile_id())
            .map_err(environment::desktop_error)?;
        if profile.provider == TunnelProvider::Openai {
            return Err(unavailable());
        }
        // Never follow redirects or use a system proxy for private loopback auth.
        validate_control_origin(&profile.local_server_url)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|_| unavailable())?;
        let endpoint = format!(
            "{}/api/connections/cloudflare",
            profile.local_server_url.trim_end_matches('/')
        );
        let service_action =
            if profile.host_mode == webcodex_environment::TunnelHostMode::Standalone {
                match &request {
                    CloudflareConnectionRequest::Start {
                        server_instance_id,
                        expected_revision,
                        ..
                    } => Some((
                        webcodex_environment::ServiceOperation::Start,
                        server_instance_id.as_str(),
                        Some(*expected_revision),
                        None,
                    )),
                    CloudflareConnectionRequest::Stop {
                        server_instance_id,
                        process_generation,
                        ..
                    } => Some((
                        webcodex_environment::ServiceOperation::Stop,
                        server_instance_id.as_str(),
                        None,
                        Some(*process_generation),
                    )),
                    _ => None,
                }
            } else {
                None
            };
        let bytes =
            if let Some((action, expected_instance, expected_revision, expected_generation)) =
                service_action
            {
                let status_request = CloudflareConnectionRequest::Status {
                    profile_id: profile.profile_id.clone(),
                };
                let observed = control_response(
                    &client,
                    &endpoint,
                    &profile.bootstrap_token,
                    &status_request,
                )
                .await?;
                let observed: CloudflareConnectionStatus =
                    serde_json::from_slice(&observed).map_err(|_| unavailable())?;
                if observed.profile_id != profile.profile_id
                    || observed.server_instance_id != expected_instance
                    || expected_revision.is_some_and(|revision| {
                        revision != observed.configured_revision || revision != profile.revision
                    })
                    || expected_generation
                        .is_some_and(|generation| generation != observed.process_generation)
                {
                    return Err(unavailable());
                }
                // A standalone service is durable Environment startup intent. Its
                // new run performs its own fenced Server prepare handshake; Desktop
                // never restarts the shared Server to control that service.
                webcodex_environment::NativeEnvironment::new()
                    .map_err(environment::desktop_error)?
                    .control_cloudflare_tunnel(&store, expected_environment, &profile, action)
                    .await
                    .map_err(environment::desktop_error)?;
                control_response(
                    &client,
                    &endpoint,
                    &profile.bootstrap_token,
                    &status_request,
                )
                .await?
            } else {
                control_response(&client, &endpoint, &profile.bootstrap_token, &request).await?
            };
        if matches!(request, CloudflareConnectionRequest::ConfigureOauth { .. }) {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct WireHandoff {
                client_id: String,
                client_secret: Option<String>,
                #[serde(default)]
                already_configured: bool,
            }
            let handoff: WireHandoff = serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
            if handoff.client_id.is_empty()
                || handoff.client_id.len() > 256
                || handoff
                    .client_secret
                    .as_ref()
                    .is_some_and(|secret| secret.len() > 8192)
            {
                return Err(unavailable());
            }
            return Ok(CloudflareConnectionResponse::OAuth(
                CloudflareOAuthHandoff {
                    client_id: handoff.client_id,
                    already_configured: handoff.already_configured,
                    client_secret: handoff.client_secret.map(Secret::new),
                },
            ));
        }
        let status: CloudflareConnectionStatus =
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        if status.profile_id != request.profile_id()
            || !matches!(
                status.lifecycle.as_str(),
                "stopped" | "starting" | "running" | "disconnected" | "error"
            )
        {
            return Err(unavailable());
        }
        Ok(CloudflareConnectionResponse::Status(status))
    }
}

async fn control_response(
    client: &reqwest::Client,
    endpoint: &str,
    token: &Secret,
    request: &CloudflareConnectionRequest,
) -> DesktopResult<Vec<u8>> {
    let mut response = client
        .post(endpoint)
        .bearer_auth(token.expose())
        .json(request)
        .send()
        .await
        .map_err(|_| unavailable())?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > 32 * 1024)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len().saturating_add(chunk.len()) > 32 * 1024 {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ControlFailure {
            error: String,
            next_action: String,
        }
        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
            && serde_json::from_slice::<ControlFailure>(&bytes).is_ok_and(|failure| {
                failure.error == "cloudflare_ingress_not_applied"
                    && failure.next_action == "restart_server"
            })
        {
            return Err(DesktopError::new(
                "cloudflare_ingress_not_applied",
                "The running Server has not loaded Cloudflare connections yet",
                "Review the saved connection and explicitly restart the Server to apply it.",
            ));
        }
        return Err(unavailable());
    }
    Ok(bytes)
}

fn validate_control_origin(origin: &str) -> DesktopResult<()> {
    let url = url::Url::parse(origin).map_err(|_| unavailable())?;
    let loopback = match url.host() {
        Some(url::Host::Domain(host)) => host.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if !loopback
        || url.scheme() != "http"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(unavailable());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
