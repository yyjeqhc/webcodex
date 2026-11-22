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

#[derive(Clone, Copy)]
pub(crate) enum OpenAiTunnelRecovery {
    Durable,
    // Each share owns a fresh local credential. Preserve manual reconciliation
    // after uncertainty rather than permanently binding this Tunnel to one share.
    TemporaryShare,
}

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
        recovery: OpenAiTunnelRecovery,
    ) -> Result<Self, ProductError> {
        let (client, guard) = Self::prepare_client(
            prerequisites,
            mcp_url,
            local_token,
            &guard_root()?,
            recovery,
        )?;
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

    fn prepare_client(
        prerequisites: &OpenAiTunnelPrerequisites,
        mcp_url: &str,
        local_token: &str,
        root: &Path,
        recovery: OpenAiTunnelRecovery,
    ) -> Result<(TunnelClient, webcodex_openai_tunnel::run_fence::RunFence), ProductError> {
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
        match recovery {
            OpenAiTunnelRecovery::TemporaryShare => {
                let guard = acquire_guard(root, &prerequisites.tunnel_id)?;
                Ok((client, guard))
            }
            OpenAiTunnelRecovery::Durable => client
                .with_restart_recovery(root, current_owner_started(), owner_absent)
                .map_err(tunnel_recovery_error),
        }
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
    let proxy = match std::env::var("WEBCODEX_TUNNEL_PROXY") {
        Ok(value) if !value.trim().is_empty() => {
            ControlPlaneProxy::Explicit(value.trim().to_owned())
        }
        _ => ControlPlaneProxy::System,
    };
    OpenAiTunnelPrerequisites::bound(
        std::env::var("CONTROL_PLANE_TUNNEL_ID").map_err(|_| missing_configuration())?,
        std::env::var("CONTROL_PLANE_API_KEY").map_err(|_| missing_configuration())?,
        proxy,
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

pub(super) async fn start_temporary_openai_tunnel(
    prerequisites: &OpenAiTunnelPrerequisites,
    mcp_url: &str,
    local_token: &str,
    deadline: Instant,
) -> Result<OpenAiTunnel, ProductError> {
    start_openai_tunnel_with_stop(
        prerequisites,
        mcp_url,
        local_token,
        OpenAiTunnelRecovery::TemporaryShare,
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
    recovery: OpenAiTunnelRecovery,
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
    let tunnel = OpenAiTunnel::launch(prerequisites, &mcp_url, local_token, recovery)?;
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

fn current_owner_started() -> Option<u64> {
    #[cfg(windows)]
    {
        process_instance(std::process::id()).ok().flatten()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn owner_absent(identity: &webcodex_openai_tunnel::run_fence::RunIdentity) -> bool {
    let pid = identity.owner_pid;
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        match process_instance(pid) {
            Ok(None) => true,
            // A reboot can reuse a PID. Its creation time proves that the old
            // process instance is gone without stopping the replacement process.
            Ok(Some(started)) => identity.owner_started.is_some_and(|old| old != started),
            Err(()) => false,
        }
    }
    #[cfg(unix)]
    {
        let Ok(pid) = i32::try_from(pid) else {
            return false;
        };
        unsafe {
            libc::kill(pid, 0) == -1
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
        }
    }
    #[cfg(not(any(windows, unix)))]
    {
        false
    }
}

#[cfg(windows)]
fn process_instance(pid: u32) -> Result<Option<u64>, ()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_INVALID_PARAMETER, FILETIME, STILL_ACTIVE},
        System::Threading::{
            GetExitCodeProcess, GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    };
    // Denied access or failed observations are unknown, never proof of exit.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return if GetLastError() == ERROR_INVALID_PARAMETER {
                Ok(None)
            } else {
                Err(())
            };
        }
        let mut code = STILL_ACTIVE as u32;
        if GetExitCodeProcess(handle, &mut code) == 0 {
            CloseHandle(handle);
            return Err(());
        }
        if code != STILL_ACTIVE as u32 {
            CloseHandle(handle);
            return Ok(None);
        }
        let mut created: FILETIME = std::mem::zeroed();
        let mut exited: FILETIME = std::mem::zeroed();
        let mut kernel: FILETIME = std::mem::zeroed();
        let mut user: FILETIME = std::mem::zeroed();
        let observed =
            GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0;
        CloseHandle(handle);
        let started = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
        if observed && started != 0 {
            Ok(Some(started))
        } else {
            Err(())
        }
    }
}
fn missing_configuration() -> ProductError {
    ProductError::new(
        "tunnel_unavailable",
        "OpenAI Tunnel requires a valid CONTROL_PLANE_TUNNEL_ID and CONTROL_PLANE_API_KEY",
        Some("Select a Tunnel and supply its Restricted runtime key with Tunnels Read + Use."),
    )
}
fn tunnel_recovery_error(code: &'static str) -> ProductError {
    match code {
        "tunnel_protected_history_binding_changed"
        | "tunnel_receipts_binding_changed"
        | "tunnel_archive_binding_changed" => ProductError::new(
            code,
            "This Tunnel's saved history does not match the local address or credential",
            Some("Use the original local environment, or create a new Tunnel for this environment. Preserve the existing history; restarting does not authorize a different local binding."),
        ),
        "tunnel_archive_capacity" => tunnel_error(Error::Capacity),
        _ => tunnel_error(Error::Uncertain),
    }
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
        Error::Uncertain => "Do not replay unconfirmed effects. Protected runs can reconnect after their old process exits with intact durable receipts; legacy fences, changed bindings, missing or corrupt receipt history still require explicit reconciliation. Use environment recover-tunnel with the exact Environment and Profile to diagnose the fence.",
        _ => "Check the fixed local MCP endpoint, Tunnel runtime credential and control-plane connectivity.",
    }))
}

#[cfg(test)]
#[path = "project_entry_openai_tunnel_tests.rs"]
mod tests;
