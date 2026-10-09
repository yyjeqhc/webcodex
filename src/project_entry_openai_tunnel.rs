use super::ProductError;
use std::{
    future::Future,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{sync::oneshot, task::JoinHandle};
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, ControlPlaneProxy, Credential, Error, FixedMcpTarget, Health,
    TunnelClient,
};

const CONTROL_PLANE: &str = "https://api.openai.com";

pub(crate) struct OpenAiTunnelPrerequisites {
    pub(super) tunnel_id: String,
    credential: String,
    proxy: ControlPlaneProxy,
}
impl OpenAiTunnelPrerequisites {
    /// One explicit identity/key pair. Embedded callers never consult CONTROL_PLANE_*
    /// or inherit proxy policy from another profile's process environment.
    pub(crate) fn bound(
        tunnel_id: String,
        credential: String,
        proxy: ControlPlaneProxy,
    ) -> Result<Self, ProductError> {
        if !valid_tunnel_id(&tunnel_id) {
            return Err(missing_configuration());
        }
        Credential::bearer(&credential).map_err(|_| missing_configuration())?;
        Ok(Self {
            tunnel_id,
            credential,
            proxy,
        })
    }
}

pub(crate) struct OpenAiTunnel {
    task: Option<JoinHandle<Result<(), Error>>>,
    stop: Option<oneshot::Sender<()>>,
    health: Health,
    guard: webcodex_openai_tunnel::run_fence::RunFence,
    terminal: Option<Result<(), Error>>,
}
impl OpenAiTunnel {
    pub(crate) fn health(&self) -> Health {
        self.health.clone()
    }

    /// The caller must first prove its fixed local MCP endpoint ready. This owner
    /// retains the task until it observes completion, or aborts it on abrupt Drop.
    pub(crate) fn launch(
        prerequisites: &OpenAiTunnelPrerequisites,
        mcp_url: &str,
        local_token: &str,
    ) -> Result<Self, ProductError> {
        let mcp_url = local_mcp_url(mcp_url)?;
        let client = TunnelClient::new_with_proxy(
            ControlPlaneIdentity::new(
                CONTROL_PLANE,
                &prerequisites.tunnel_id,
                Credential::bearer(&prerequisites.credential).map_err(tunnel_error)?,
            )
            .map_err(tunnel_error)?,
            FixedMcpTarget::new(
                &mcp_url,
                Credential::bearer(local_token).map_err(tunnel_error)?,
            )
            .map_err(tunnel_error)?,
            DeadlinePolicy::RequireFinite {
                max_duration: Duration::from_secs(120),
            },
            Limits::default(),
            prerequisites.proxy.clone(),
        )
        .map_err(tunnel_error)?;
        let guard = acquire_guard(&guard_root()?, &prerequisites.tunnel_id)?;
        let health = client.health();
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(client.run(async {
            let _ = rx.await;
        }));
        Ok(Self {
            task: Some(task),
            stop: Some(stop),
            health,
            guard,
            terminal: None,
        })
    }

    fn completed(
        &mut self,
        joined: Result<Result<(), Error>, tokio::task::JoinError>,
    ) -> Result<(), Error> {
        self.task = None;
        let mut result = joined.unwrap_or(Err(Error::Uncertain));
        if self.health.has_uncertain_work() {
            result = Err(Error::Uncertain);
        }
        if result.is_ok() && self.guard.finish().is_err() {
            result = Err(Error::Uncertain);
        }
        self.terminal = Some(result);
        result
    }

    pub(crate) async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        let result = match self.task.as_mut() {
            Some(task) => task.await,
            None => {
                return Err(tunnel_error(
                    self.terminal
                        .unwrap_or(Err(Error::Uncertain))
                        .err()
                        .unwrap_or(Error::Transport),
                ))
            }
        };
        // An unsolicited exit is observable failure, even if the task returned Ok.
        Err(tunnel_error(
            self.completed(result).err().unwrap_or(Error::Transport),
        ))
    }

    pub(crate) async fn stop_with_outcome(&mut self) -> Result<(), ProductError> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.as_mut() {
            // Retain ownership while awaiting: cancelling this method must not
            // detach the task. Library drain is bounded at ten seconds.
            let joined = match tokio::time::timeout(Duration::from_secs(12), task).await {
                Ok(joined) => joined,
                Err(_) => {
                    if let Some(task) = self.task.as_ref() {
                        task.abort();
                    }
                    self.terminal = Some(Err(Error::Uncertain));
                    return Err(tunnel_error(Error::Uncertain));
                }
            };
            self.completed(joined).map_err(tunnel_error)
        } else {
            self.terminal
                .unwrap_or(Err(Error::Uncertain))
                .map_err(tunnel_error)
        }
    }

    pub(super) async fn stop(&mut self) {
        if self.stop_with_outcome().await.is_err() {
            eprintln!("WebCodex Tunnel shutdown was not confirmed clean; its restart fence must be inspected before reuse.");
        }
    }
}
impl Drop for OpenAiTunnel {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            task.abort();
        }
        // A dropped owner cannot prove shutdown completed. Keep the restart fence.
    }
}

pub(super) async fn prepare_openai_tunnel() -> Result<OpenAiTunnelPrerequisites, ProductError> {
    OpenAiTunnelPrerequisites::bound(
        std::env::var("CONTROL_PLANE_TUNNEL_ID").map_err(|_| missing_configuration())?,
        std::env::var("CONTROL_PLANE_API_KEY").map_err(|_| missing_configuration())?,
        ControlPlaneProxy::System,
    )
}

fn local_mcp_url(value: &str) -> Result<String, ProductError> {
    let mut url = url::Url::parse(value).map_err(|_| tunnel_error(Error::Configuration))?;
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| tunnel_error(Error::Configuration))?;
    }
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || url.path() != "/mcp"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(tunnel_error(Error::Configuration));
    }
    Ok(url.to_string())
}

pub(super) async fn start_openai_tunnel(
    prerequisites: &OpenAiTunnelPrerequisites,
    mcp_url: &str,
    local_token: &str,
    deadline: Instant,
) -> Result<OpenAiTunnel, ProductError> {
    start_openai_tunnel_with_stop(
        prerequisites,
        mcp_url,
        local_token,
        deadline,
        std::future::pending(),
    )
    .await?
    .ok_or_else(|| tunnel_error(Error::Uncertain))
}

/// Cancellation must settle an already-launched owner rather than drop it.
/// None means shutdown was observed before readiness; uncertainty stays fenced.
pub(super) async fn start_openai_tunnel_with_stop(
    prerequisites: &OpenAiTunnelPrerequisites,
    mcp_url: &str,
    local_token: &str,
    deadline: Instant,
    stop: impl Future<Output = ()>,
) -> Result<Option<OpenAiTunnel>, ProductError> {
    tokio::pin!(stop);
    let mcp_url = local_mcp_url(mcp_url)?;
    let probe = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| tunnel_error(Error::Configuration))?;
    let local_ready = tokio::select! {
        biased;
        _ = &mut stop => return Ok(None),
        result = tokio::time::timeout_at(
            deadline.into(),
            super::regular_tunnel_service::probe_local_mcp(&probe, &mcp_url, local_token),
        ) => result.unwrap_or(false),
    };
    if !local_ready {
        return Err(ProductError::new(
            "local_mcp_unavailable",
            "Local authenticated MCP endpoint is not ready",
            Some("Start the local WebCodex Server and verify its credential."),
        ));
    }
    let tunnel = OpenAiTunnel::launch(prerequisites, &mcp_url, local_token)?;
    finish_openai_tunnel_startup(tunnel, deadline, &mut stop).await
}

async fn finish_openai_tunnel_startup(
    mut tunnel: OpenAiTunnel,
    deadline: Instant,
    stop: impl Future<Output = ()>,
) -> Result<Option<OpenAiTunnel>, ProductError> {
    let readiness = {
        let ready = async {
            loop {
                if tunnel.health.is_ready() {
                    return Ok(());
                }
                tokio::select! {
                    result = tunnel.wait_for_exit() => return result,
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {},
                }
            }
        };
        tokio::select! {
            biased;
            _ = stop => None,
            result = tokio::time::timeout_at(deadline.into(), ready) => Some(result),
        }
    };
    match readiness {
        None => {
            tunnel.stop_with_outcome().await?;
            Ok(None)
        }
        Some(Ok(Ok(()))) => Ok(Some(tunnel)),
        Some(result) => {
            tunnel.stop().await;
            Err(result
                .unwrap_or_else(|_| Err(tunnel_error(Error::Transport)))
                .unwrap_err())
        }
    }
}
fn valid_tunnel_id(value: &str) -> bool {
    value.strip_prefix("tunnel_").is_some_and(|suffix| {
        suffix.len() == 32
            && suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn guard_root() -> Result<PathBuf, ProductError> {
    webcodex_openai_tunnel::run_fence::default_root()
        .map_err(|_| tunnel_error(Error::Configuration))
}
fn acquire_guard(
    root: &Path,
    tunnel: &str,
) -> Result<webcodex_openai_tunnel::run_fence::RunFence, ProductError> {
    webcodex_openai_tunnel::run_fence::RunFence::acquire(root, CONTROL_PLANE, tunnel)
        .map_err(|_| tunnel_error(Error::Uncertain))
}
fn missing_configuration() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "OpenAI Tunnel requires a valid CONTROL_PLANE_TUNNEL_ID and CONTROL_PLANE_API_KEY",
        Some("Select a Tunnel and supply its Restricted runtime key with Tunnels Read + Use."),
    )
}
fn tunnel_error(error: Error) -> ProductError {
    let code = match error {
        Error::Authentication => "tunnel_auth_rejected",
        Error::Uncertain => "tunnel_restart_uncertain",
        Error::Capacity => "tunnel_capacity_exhausted",
        Error::Transport => "tunnel_control_plane_unreachable",
        _ => "tunnel_protocol_failed",
    };
    ProductError::new(code, error.to_string(), Some(match error {
        Error::Uncertain => "Do not automatically restart. Use environment recover-tunnel with the exact Environment and Profile to diagnose the fence. Resolve prior effects and pending work before explicit recovery; legacy ownerless fences remain blocked.",
        _ => "Check the fixed local MCP endpoint, Tunnel runtime credential and control-plane connectivity.",
    }))
}

#[cfg(test)]
#[path = "project_entry_openai_tunnel_tests.rs"]
mod tests;
