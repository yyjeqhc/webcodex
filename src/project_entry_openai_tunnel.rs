use super::{
    setup_service::{create_private_dir, write_new_private},
    ProductError,
};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{sync::oneshot, task::JoinHandle};
use webcodex_openai_tunnel::{
    policy::{DeadlinePolicy, Limits},
    ControlPlaneIdentity, Credential, Error, FixedMcpTarget, Health, TunnelClient,
};

const CONTROL_PLANE: &str = "https://api.openai.com";

pub(super) struct OpenAiTunnelPrerequisites {
    pub(super) tunnel_id: String,
    credential: String,
}

pub(super) struct OpenAiTunnel {
    task: Option<JoinHandle<Result<(), Error>>>,
    stop: Option<oneshot::Sender<()>>,
    health: Health,
    guard: PathBuf,
}
impl OpenAiTunnel {
    pub(super) fn health(&self) -> Health {
        self.health.clone()
    }
    pub(super) async fn wait_for_exit(&mut self) -> Result<(), ProductError> {
        let result = match self.task.as_mut() {
            Some(task) => task.await,
            None => return Err(tunnel_error(Error::Uncertain)),
        };
        self.task = None;
        if result.is_ok() && !self.health.has_uncertain_work() {
            let _ = std::fs::remove_file(&self.guard);
        }
        Err(tunnel_error(
            result
                .unwrap_or(Err(Error::Uncertain))
                .err()
                .unwrap_or(Error::Transport),
        ))
    }
    pub(super) async fn stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.as_mut() {
            // Retain ownership while awaiting: cancellation of stop must leave
            // Drop able to abort the task rather than silently detaching it.
            let result = task.await;
            self.task = None;
            if result.is_ok() && !self.health.has_uncertain_work() {
                let _ = std::fs::remove_file(&self.guard);
            } else {
                eprintln!("WebCodex Tunnel stopped with unconfirmed work; automatic restart is blocked. Resolve the previous effects and pending Tunnel work before clearing its run marker, or use a new Tunnel identity.");
            }
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
    let tunnel_id =
        std::env::var("CONTROL_PLANE_TUNNEL_ID").map_err(|_| missing_configuration())?;
    if !valid_tunnel_id(&tunnel_id) {
        return Err(missing_configuration());
    }
    let credential = std::env::var("CONTROL_PLANE_API_KEY").map_err(|_| missing_configuration())?;
    Credential::bearer(&credential).map_err(|_| missing_configuration())?;
    Ok(OpenAiTunnelPrerequisites {
        tunnel_id,
        credential,
    })
}

pub(super) async fn start_openai_tunnel(
    prerequisites: &OpenAiTunnelPrerequisites,
    mcp_url: &str,
    local_token: &str,
    deadline: Instant,
) -> Result<OpenAiTunnel, ProductError> {
    // The adapter only exposes the owned loopback MCP endpoint. The independent
    // library may be used with an explicitly configured HTTPS target.
    let mut url = url::Url::parse(mcp_url).map_err(|_| tunnel_error(Error::Configuration))?;
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| tunnel_error(Error::Configuration))?;
    }
    let mcp_url = url.as_str();
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || url.path() != "/mcp"
        || url.query().is_some()
    {
        return Err(tunnel_error(Error::Configuration));
    }
    let client = TunnelClient::new(
        ControlPlaneIdentity::new(
            CONTROL_PLANE,
            &prerequisites.tunnel_id,
            Credential::bearer(&prerequisites.credential).map_err(tunnel_error)?,
        )
        .map_err(tunnel_error)?,
        FixedMcpTarget::new(
            mcp_url,
            Credential::bearer(local_token).map_err(tunnel_error)?,
        )
        .map_err(tunnel_error)?,
        DeadlinePolicy::RequireFinite {
            max_duration: Duration::from_secs(120),
        },
        Limits::default(),
    )
    .map_err(tunnel_error)?;
    let probe = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|_| tunnel_error(Error::Configuration))?;
    let local_ready = tokio::time::timeout_at(
        deadline.into(),
        super::regular_tunnel_service::probe_local_mcp(&probe, mcp_url, local_token),
    )
    .await
    .unwrap_or(false);
    if !local_ready {
        return Err(ProductError::new(
            "local_mcp_unavailable",
            "Local authenticated MCP endpoint is not ready",
            Some("Start the local WebCodex Server and verify its credential."),
        ));
    }
    let root = guard_root()?;
    let guard = acquire_guard(&root, &prerequisites.tunnel_id)?;
    let health = client.health();
    let (stop, rx) = oneshot::channel();
    let task = tokio::spawn(client.run(async {
        let _ = rx.await;
    }));
    let mut tunnel = OpenAiTunnel {
        task: Some(task),
        stop: Some(stop),
        health,
        guard,
    };
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
    match tokio::time::timeout_at(deadline.into(), ready).await {
        Ok(Ok(())) => Ok(tunnel),
        result => {
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
    let base = super::setup_service::default_state_base_from(
        std::env::var_os("XDG_STATE_HOME").as_deref(),
        std::env::var_os("HOME").as_deref(),
        if cfg!(windows) {
            std::env::var_os("LOCALAPPDATA")
        } else {
            None
        }
        .as_deref(),
    )?;
    if !base.is_absolute() {
        return Err(tunnel_error(Error::Configuration));
    }
    Ok(base
        .parent()
        .ok_or_else(|| tunnel_error(Error::Configuration))?
        .join("tunnel-runs"))
}
fn acquire_guard(root: &Path, tunnel: &str) -> Result<PathBuf, ProductError> {
    if std::fs::symlink_metadata(root).is_ok_and(|m| !m.file_type().is_dir()) {
        return Err(tunnel_error(Error::Configuration));
    }
    create_private_dir(root)?;
    let key = format!("{:x}", Sha256::digest(format!("{CONTROL_PLANE}/{tunnel}")));
    let path = root.join(format!("{key}.active"));
    // Exclusive creation blocks another owner and every unconfirmed previous run.
    // This is one coarse latch, not a per-command durable dedupe database.
    write_new_private(&path, b"native-tunnel-run-v1\n")
        .map_err(|_| tunnel_error(Error::Uncertain))?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|f| f.sync_all())
        .map_err(|_| tunnel_error(Error::Uncertain))?;
    #[cfg(unix)]
    for directory in root.ancestors() {
        std::fs::File::open(directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| tunnel_error(Error::Uncertain))?;
    }
    Ok(path)
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
        Error::Uncertain => "Do not automatically restart. Resolve prior effects and pending work, then remove the matching marker in the private webcodex/tunnel-runs directory, or use a new Tunnel identity.",
        _ => "Check the fixed local MCP endpoint, Tunnel runtime credential and control-plane connectivity.",
    }))
}

#[cfg(test)]
#[path = "project_entry_openai_tunnel_tests.rs"]
mod tests;
