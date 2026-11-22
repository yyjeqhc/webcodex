//! Private native control-plane adapter. Runtime credentials stay in Rust; only
//! the explicit OAuth handoff response can carry the newly issued client secret.
use super::*;
use serde::{Deserialize, Serialize, Serializer};
use webcodex_environment::{Secret, TunnelProvider};

mod gemini;
pub use gemini::GeminiConnectionRequest;

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
        expected_generation: u64,
        expected_configuration_id: String,
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

    fn stage(&self) -> &'static str {
        match self {
            Self::Status { .. } => "cloudflare_status",
            Self::Start { .. } => "cloudflare_start",
            Self::Stop { .. } => "cloudflare_stop",
            Self::ConfigureOauth { .. } => "cloudflare_oauth",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareConnectionStatus {
    pub configuration_id: String,
    pub profile_id: String,
    pub server_instance_id: String,
    pub process_generation: u64,
    pub lifecycle: String,
    pub public_origin: Option<String>,
    pub oauth_configured: bool,
    pub observed_authorization: bool,
    #[serde(default)]
    pub registration_available: bool,
    #[serde(default)]
    pub observed_tool_call: bool,
    pub configured_revision: u64,
    pub applied_revision: Option<u64>,
    pub local_target: String,
    pub reason_code: Option<String>,
}

impl CloudflareConnectionStatus {
    fn check_materialized_revision(&mut self, revision: u64) {
        // Windows Server reads protected materializations. Preserve the observed
        // generation for Stop while the user applies the newer catalog revision.
        if cfg!(windows) && self.configured_revision < revision {
            self.reason_code = Some("cloudflare_restart_server_required".into());
            self.registration_available = false;
        }
    }
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
    .with_component(crate::error::ErrorComponent::Tunnel)
}
fn unavailable_for(stage: &str, profile_id: &str, reason_code: Option<&str>) -> DesktopError {
    let mut error =
        unavailable()
            .with_stage(stage)
            .with_operation_context(None, None, Some(profile_id), None);
    if let Some(reason) = reason_code {
        error = error.with_reason_code(reason);
    }
    error
}
fn stale_revision(stage: &str, profile_id: &str) -> DesktopError {
    DesktopError::new(
        "cloudflare_revision_stale",
        "This Cloudflare connection changed before the control request completed",
        "Refresh the connection status to review the current configuration before retrying.",
    )
    .with_component(crate::error::ErrorComponent::Tunnel)
    .with_stage(stage)
    .with_reason_code("stale_fence")
    .with_operation_context(None, None, Some(profile_id), None)
}
fn requires_environment() -> DesktopError {
    DesktopError::new(
        "cloudflare_environment_required",
        "Cloudflare needs a persistent local Environment",
        "Use Runtime setup or repair to adopt the existing local Server first.",
    )
    .with_component(crate::error::ErrorComponent::Tunnel)
    .with_stage("cloudflare_preflight")
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
        if let CloudflareConnectionRequest::Start {
            expected_configuration_id,
            ..
        } = &request
        {
            if *expected_configuration_id != profile.configuration_id {
                return Err(stale_revision(request.stage(), &profile.profile_id));
            }
        }
        if profile.provider == TunnelProvider::Openai {
            return Err(unavailable());
        }
        // Never follow redirects or use a system proxy for private loopback auth.
        validate_control_origin(&profile.local_server_url)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .retry(reqwest::retry::never())
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
                        expected_generation,
                        ..
                    } => Some((
                        webcodex_environment::ServiceOperation::Start,
                        server_instance_id.as_str(),
                        Some(*expected_revision),
                        Some(*expected_generation),
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
                let observed: CloudflareConnectionStatus = serde_json::from_slice(&observed)
                    .map_err(|_| {
                        unavailable_for(
                            request.stage(),
                            &profile.profile_id,
                            Some("invalid_response"),
                        )
                    })?;
                if observed.configuration_id != profile.configuration_id
                    || observed.profile_id != profile.profile_id
                    || observed.server_instance_id != expected_instance
                    || expected_revision.is_some_and(|revision| {
                        revision != observed.configured_revision || revision != profile.revision
                    })
                    || expected_generation
                        .is_some_and(|generation| generation != observed.process_generation)
                {
                    return Err(stale_revision(request.stage(), &profile.profile_id));
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
        let mut status: CloudflareConnectionStatus =
            serde_json::from_slice(&bytes).map_err(|_| unavailable())?;
        if status.profile_id != request.profile_id()
            || !matches!(
                status.lifecycle.as_str(),
                "stopped" | "starting" | "running" | "disconnected" | "error"
            )
        {
            return Err(unavailable());
        }
        if status.configuration_id != profile.configuration_id {
            return Err(stale_revision(request.stage(), &profile.profile_id));
        }
        status.check_materialized_revision(profile.revision);
        Ok(CloudflareConnectionResponse::Status(status))
    }
}

async fn control_response(
    client: &reqwest::Client,
    endpoint: &str,
    token: &Secret,
    request: &CloudflareConnectionRequest,
) -> DesktopResult<Vec<u8>> {
    let stage = request.stage();
    let profile_id = request.profile_id();
    let uncertain_write = |error: DesktopError| {
        if matches!(request, CloudflareConnectionRequest::Status { .. }) {
            error
        } else {
            error.with_recovery(
                crate::error::RecoveryKind::ManualCheck,
                crate::error::SafeRecoveryAction::ObserveBeforeRetry,
                false,
            )
        }
    };
    let mut response = client
        .post(endpoint)
        .bearer_auth(token.expose())
        .json(request)
        .send()
        .await
        .map_err(|err| {
            let reason = if err.is_timeout() {
                "request_timeout"
            } else {
                "server_unreachable"
            };
            uncertain_write(
                DesktopError::new(
                    "cloudflare_server_unreachable",
                    "Could not reach the local Server to control this Cloudflare connection",
                    "Check that the local Server is running, then refresh this connection.",
                )
                .with_component(crate::error::ErrorComponent::Tunnel)
                .with_stage(stage)
                .with_reason_code(reason)
                .with_operation_context(None, None, Some(profile_id), None),
            )
        })?;
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > 32 * 1024)
    {
        return Err(uncertain_write(unavailable_for(
            stage,
            profile_id,
            Some("response_limit"),
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| {
        uncertain_write(unavailable_for(
            stage,
            profile_id,
            Some("response_uncertain"),
        ))
    })? {
        if bytes.len().saturating_add(chunk.len()) > 32 * 1024 {
            return Err(uncertain_write(unavailable_for(
                stage,
                profile_id,
                Some("response_limit"),
            )));
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
            )
            .with_component(crate::error::ErrorComponent::Tunnel)
            .with_stage(stage)
            .with_http_status(status.as_u16())
            .with_operation_context(None, None, Some(profile_id), None));
        }
        if matches!(
            status,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) {
            return Err(DesktopError::new(
                "cloudflare_authentication_failed",
                "The local Server rejected authorization for this Cloudflare connection",
                "Refresh Diagnostics and verify the local Environment configuration.",
            )
            .with_component(crate::error::ErrorComponent::Auth)
            .with_stage(stage)
            .with_http_status(status.as_u16())
            .with_operation_context(None, None, Some(profile_id), None));
        }
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Conflict {
            error: String,
        }
        if status == reqwest::StatusCode::CONFLICT {
            if let Ok(conflict) = serde_json::from_slice::<Conflict>(&bytes) {
                match conflict.error.as_str() {
                    "cloudflare_restart_server_required" => {
                        return Err(DesktopError::new(
                            "cloudflare_ingress_not_applied",
                            "Cloudflare configuration requires Server apply",
                            "Restart the selected Server to apply the saved configuration.",
                        )
                        .with_component(crate::error::ErrorComponent::Tunnel)
                        .with_stage(stage)
                        .with_http_status(status.as_u16())
                        .with_operation_context(
                            None,
                            None,
                            Some(profile_id),
                            None,
                        ));
                    }
                    "cloudflare_already_active"
                    | "cloudflare_identity_repair_required"
                    | "cloudflare_owner_repair_required"
                    | "cloudflare_revision_conflict"
                    | "cloudflare_stale_attempt" => {
                        return Err(DesktopError::new(
                            conflict.error,
                            "Cloudflare could not apply this operation",
                            "Review the current connection before retrying.",
                        )
                        .with_component(crate::error::ErrorComponent::Tunnel)
                        .with_stage(stage)
                        .with_http_status(status.as_u16())
                        .with_recovery(
                            crate::error::RecoveryKind::ConfirmationRequired,
                            crate::error::SafeRecoveryAction::ObserveBeforeRetry,
                            false,
                        )
                        .with_operation_context(
                            None,
                            None,
                            Some(profile_id),
                            None,
                        ));
                    }
                    _ => {}
                }
            }
            // A generic HTTP 409 does not establish which ownership/revision
            // fence failed. Preserve the status without inventing a stale
            // revision diagnosis or permitting an uncertain mutation replay.
            return Err(unavailable_for(stage, profile_id, None)
                .with_http_status(status.as_u16())
                .with_recovery(
                    crate::error::RecoveryKind::ConfirmationRequired,
                    crate::error::SafeRecoveryAction::ObserveBeforeRetry,
                    false,
                ));
        }
        return Err(unavailable_for(stage, profile_id, None).with_http_status(status.as_u16()));
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
