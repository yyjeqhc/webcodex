use crate::activity::{ActivityLevel, ActivityLog};
use crate::error::{DesktopError, DesktopResult};
use crate::models::{
    aggregate_readiness, DesktopStateSnapshot, Enrollment, Experience, Exposure, ExposureReadiness,
    ProjectReadiness, ProjectSelection, QuickShareState, RunnerReadiness, RunnerTopology,
    RuntimeTopology, ServerReadiness, ServerTopology, StoredDesktopConfig, StoredRuntime,
};
use crate::process::{ProcessKind, ProcessPhase, ProcessSupervisor};
use crate::webcodex::{ProjectRuntimeIdentity, QuickShareReadyEvent, WebCodexAdapter};
use serde_json::Value;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::Mutex;

const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(20);
const RUNNER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const PROJECT_READY_TIMEOUT: Duration = Duration::from_secs(20);
const QUICK_SHARE_READY_TIMEOUT: Duration = Duration::from_secs(90);
const POLL_INTERVAL: Duration = Duration::from_millis(300);

pub struct AppState {
    core: Mutex<DesktopCore>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        Self {
            core: Mutex::new(DesktopCore::new(data_dir)),
        }
    }

    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, DesktopCore> {
        self.core.lock().await
    }

    pub async fn shutdown(&self) {
        let mut core = self.core.lock().await;
        core.supervisor.stop_all().await;
    }
}

pub struct DesktopCore {
    data_dir: PathBuf,
    config_path: PathBuf,
    config: StoredDesktopConfig,
    snapshot: DesktopStateSnapshot,
    adapter: WebCodexAdapter,
    supervisor: ProcessSupervisor,
    activity: ActivityLog,
}

impl DesktopCore {
    fn new(data_dir: PathBuf) -> Self {
        let activity = ActivityLog::default();
        let config_path = data_dir.join("desktop-state.json");
        let config = load_config(&config_path).unwrap_or_default();
        let mut snapshot = DesktopStateSnapshot::default();
        snapshot.topology = config.topology.clone();
        snapshot.project = project_snapshot(&config);
        snapshot.openai_tunnel_configured = openai_tunnel_is_configured();
        snapshot.regular_tunnel_available = false;
        Self {
            data_dir,
            config_path,
            config,
            snapshot,
            adapter: WebCodexAdapter::new(),
            supervisor: ProcessSupervisor::new(activity.clone()),
            activity,
        }
    }

    pub async fn inspect_project(&self, path: &str) -> DesktopResult<ProjectSelection> {
        self.adapter.inspect_project(path).await
    }

    pub async fn get_state(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        self.supervisor.refresh();
        self.snapshot.openai_tunnel_configured = openai_tunnel_is_configured();
        self.snapshot.activity_sequence = self.activity.latest_sequence();
        Ok(self.snapshot.clone())
    }

    pub async fn refresh_runtime_status(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        self.supervisor.refresh();
        if self.snapshot.quick_share.is_some() {
            let active = self
                .supervisor
                .snapshot(ProcessKind::QuickShare)
                .is_some_and(|process| {
                    matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
                });
            if !active {
                self.snapshot.readiness = aggregate_readiness(
                    ServerReadiness::Stopped,
                    RunnerReadiness::Stopped,
                    ExposureReadiness::Error,
                    ProjectReadiness::Configured,
                );
                self.snapshot.readiness.summary = "Quick Share stopped".to_string();
                self.snapshot.readiness.next_action = Some("Start Quick Share again.".to_string());
            }
            return self.get_state().await;
        }

        let Some(identity) = identity_from_config(&self.config) else {
            self.snapshot.topology = self.config.topology.clone();
            self.snapshot.project = project_snapshot(&self.config);
            return self.get_state().await;
        };
        self.adapter.ensure_binaries().await.ok();
        if let Ok(binaries) = self.adapter.binaries() {
            self.snapshot.binaries = Some(binaries.info());
        }
        let server = match self
            .adapter
            .server_status(
                Some(&identity.server_url),
                self.config
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.server_env_file.as_deref()),
                Some(&identity.user_token_file),
            )
            .await
        {
            Ok(status) if status.http_reachable => ServerReadiness::Ready,
            Ok(_) => ServerReadiness::Error,
            Err(_) => ServerReadiness::Unknown,
        };
        let runner = match self.adapter.runner_ready(&identity).await {
            Ok(true) => RunnerReadiness::Ready,
            Ok(false) => RunnerReadiness::Connecting,
            Err(_) => RunnerReadiness::Unknown,
        };
        let project = match self.adapter.project_ready(&identity).await {
            Ok(true) => ProjectReadiness::Ready,
            Ok(false) => ProjectReadiness::ReloadRequired,
            Err(_) => ProjectReadiness::Unknown,
        };
        let exposure = exposure_readiness(self.config.topology.as_ref());
        self.snapshot.readiness = aggregate_readiness(server, runner, exposure, project);
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = project_snapshot(&self.config);
        self.get_state().await
    }

    pub async fn configure_local_setup(
        &mut self,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let project = self.adapter.inspect_project(project_path).await?;
        let binaries = self.adapter.ensure_binaries().await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        self.activity.push(
            "desktop",
            ActivityLevel::Info,
            "Preparing WebCodex on this computer",
        );
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        });
        self.snapshot.project = Some(project.clone());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Stopped,
            ExposureReadiness::Disabled,
            ProjectReadiness::Configured,
        );

        let local_dir = self.data_dir.join("runtime").join("local");
        let env_file = local_dir.join("webcodex.env");
        let data_dir = local_dir.join("data");
        tokio::fs::create_dir_all(&local_dir).await.map_err(|_| {
            DesktopError::new(
                "desktop_state_unavailable",
                "Desktop could not create its local runtime directory",
                "Check local app-data permissions and retry.",
            )
        })?;

        let server_url = if env_file.is_file() {
            self.adapter
                .server_status(None, Some(&env_file), None)
                .await?
                .probe_url
        } else {
            let listen = reserve_loopback_address()?;
            self.adapter
                .init_local_server(&listen, &data_dir, &env_file)
                .await?
                .probe_url
        };
        self.config.topology = self.snapshot.topology.clone();
        self.config.project = Some(project.clone());
        self.config.runtime = Some(StoredRuntime {
            server_url: server_url.clone(),
            server_env_file: Some(env_file.clone()),
            runner_config: None,
            user_token_file: None,
            project_id: None,
            runtime_project_id: None,
        });
        self.save_config().await?;

        let running = self
            .adapter
            .server_status(Some(&server_url), Some(&env_file), None)
            .await
            .is_ok_and(|status| status.http_reachable);
        if !running {
            let command = self.adapter.local_server_command(&env_file)?;
            self.supervisor
                .spawn_owned(ProcessKind::LocalServer, command, false)
                .await?;
        }
        self.wait_for_server(&server_url, Some(&env_file), None)
            .await?;
        self.snapshot.readiness.server = ServerReadiness::Ready;

        let identity = match identity_from_config(&self.config)
            .filter(|identity| same_project(&identity.project_path, &project.path))
        {
            Some(identity) => identity,
            None => {
                let pairing_code = self
                    .adapter
                    .create_local_pairing(&server_url, &env_file)
                    .await?;
                let identity = self
                    .adapter
                    .login_with_pairing(
                        &server_url,
                        &pairing_code,
                        &self.data_dir.join("connections"),
                        &project,
                    )
                    .await?;
                drop(pairing_code);
                self.store_identity(&project, &identity, Some(env_file.clone()))
                    .await?;
                identity
            }
        };

        if !self.adapter.runner_ready(&identity).await.unwrap_or(false) {
            self.snapshot.readiness.runner = RunnerReadiness::Connecting;
            let command = self.adapter.local_runner_command(&identity.runner_config)?;
            self.supervisor
                .spawn_owned(ProcessKind::LocalRunner, command, false)
                .await?;
        }
        self.wait_for_runner(&identity).await?;
        self.snapshot.readiness.runner = RunnerReadiness::Ready;
        self.wait_for_project(&identity).await?;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            ExposureReadiness::LocalReady,
            ProjectReadiness::Ready,
        );
        self.activity.push(
            "desktop",
            ActivityLevel::Info,
            "Local WebCodex runtime is ready",
        );
        self.get_state().await
    }

    pub async fn configure_remote_setup(
        &mut self,
        server_url: &str,
        pairing_code: &str,
        project_path: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let server_url = crate::webcodex::validate_server_url(server_url)?;
        let project = self.adapter.inspect_project(project_path).await?;
        let binaries = self.adapter.ensure_binaries().await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        let exposure = if server_url.starts_with("https://") {
            Exposure::ExistingHttps {
                url: server_url.clone(),
            }
        } else {
            Exposure::None
        };
        let topology = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: server_url.clone(),
            },
            runner: RunnerTopology::Local,
            exposure,
            enrollment: Enrollment::ManagedPairing,
        };
        self.snapshot.topology = Some(topology.clone());
        self.snapshot.project = Some(project.clone());
        self.config.topology = Some(topology.clone());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            if server_url.starts_with("https://") {
                ExposureReadiness::Starting
            } else {
                ExposureReadiness::Degraded
            },
            ProjectReadiness::Configured,
        );
        self.activity.push(
            "desktop",
            ActivityLevel::Info,
            "Connecting this computer to the existing WebCodex Server",
        );

        let identity = match identity_from_config(&self.config).filter(|identity| {
            same_server(&identity.server_url, &server_url)
                && same_project(&identity.project_path, &project.path)
        }) {
            Some(identity) => identity,
            None => {
                if !pairing_code.starts_with("wc_pair_") {
                    return Err(DesktopError::new(
                        "pairing_code_invalid",
                        "The one-time login code is not a WebCodex pairing code",
                        "Enter the wc_pair_… code issued by the existing Server.",
                    ));
                }
                let identity = self
                    .adapter
                    .login_with_pairing(
                        &server_url,
                        pairing_code,
                        &self.data_dir.join("connections"),
                        &project,
                    )
                    .await?;
                self.config.topology = Some(topology.clone());
                self.store_identity(&project, &identity, None).await?;
                identity
            }
        };

        let server_status = self
            .adapter
            .server_status(Some(&server_url), None, Some(&identity.user_token_file))
            .await?;
        if !server_status.http_reachable {
            return Err(DesktopError::new(
                "server_unreachable",
                "The existing WebCodex Server is not reachable",
                "Check the Server URL and network path, then retry.",
            ));
        }
        if !self.adapter.runner_ready(&identity).await.unwrap_or(false) {
            let command = self.adapter.local_runner_command(&identity.runner_config)?;
            self.supervisor
                .spawn_owned(ProcessKind::LocalRunner, command, false)
                .await?;
        }
        self.wait_for_runner(&identity).await?;
        self.wait_for_project(&identity).await?;
        self.config.topology = Some(topology);
        self.save_config().await?;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            if server_url.starts_with("https://") {
                ExposureReadiness::RemoteReady
            } else {
                ExposureReadiness::Degraded
            },
            ProjectReadiness::Ready,
        );
        self.activity.push(
            "desktop",
            ActivityLevel::Info,
            "This computer is connected to the existing WebCodex Server",
        );
        self.get_state().await
    }

    pub async fn start_quick_share(
        &mut self,
        project_path: &str,
        provider: &str,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let project = self.adapter.inspect_project(project_path).await?;
        let binaries = self.adapter.ensure_binaries().await?.clone();
        self.snapshot.binaries = Some(binaries.info());
        if self
            .supervisor
            .snapshot(ProcessKind::QuickShare)
            .is_some_and(|process| {
                matches!(
                    process.phase,
                    ProcessPhase::Starting | ProcessPhase::Running
                )
            })
        {
            return Err(DesktopError::new(
                "quick_share_already_running",
                "Quick Share is already running",
                "Stop the current share before starting another one.",
            ));
        }
        let command = self
            .adapter
            .quick_share_command(Path::new(&project.path), provider)?;
        let mut events = self
            .supervisor
            .spawn_owned(ProcessKind::QuickShare, command, true)
            .await?
            .expect("machine stdout requested");
        self.snapshot.topology = Some(RuntimeTopology {
            experience: Experience::QuickShare,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: match provider {
                "cloudflare" => Exposure::Cloudflare,
                "openai" => Exposure::OpenAiTunnel,
                _ => Exposure::None,
            },
            enrollment: Enrollment::ExistingProfile {
                profile: "temporary_share".to_string(),
            },
        });
        self.snapshot.project = Some(project.clone());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Starting,
            RunnerReadiness::Connecting,
            ExposureReadiness::Starting,
            ProjectReadiness::Configured,
        );
        self.activity.push(
            "quick_share",
            ActivityLevel::Info,
            "Starting the temporary Quick Share runtime",
        );

        let event_value = tokio::time::timeout(QUICK_SHARE_READY_TIMEOUT, async {
            while let Some(value) = events.recv().await {
                if value.get("event").and_then(Value::as_str) == Some("ready") {
                    return Some(value);
                }
            }
            None
        })
        .await
        .ok()
        .flatten();
        let Some(event_value) = event_value else {
            let logs = self.supervisor.logs(ProcessKind::QuickShare);
            self.supervisor.stop(ProcessKind::QuickShare).await;
            return Err(DesktopError::new(
                "quick_share_not_ready",
                "Quick Share did not reach verified readiness",
                "Check Activity and Tunnel prerequisites, then retry.",
            )
            .with_details(serde_json::json!({ "diagnostic_lines": logs })));
        };
        let event: QuickShareReadyEvent = serde_json::from_value(event_value).map_err(|_| {
            DesktopError::new(
                "webcodex_contract_invalid",
                "Quick Share returned an invalid readiness event",
                "Verify that Desktop and WebCodex binaries come from the same source baseline.",
            )
        })?;
        if event.event != "ready"
            || event.schema_version != 1
            || event.experience != "quick_share"
            || event.project.trim().is_empty()
            || event.exposure.kind.trim().is_empty()
        {
            return Err(DesktopError::new(
                "webcodex_contract_invalid",
                "Quick Share readiness identity is incomplete",
                "Update Desktop and WebCodex together.",
            ));
        }
        let clipboard_required = event.connection.clipboard_contains != "none";
        let handoff_available = !clipboard_required || event.connection.clipboard_state == "copied";
        let ready_for_chatgpt = event.ready_for_chatgpt && handoff_available;
        self.snapshot.quick_share = Some(QuickShareState {
            provider: provider.to_string(),
            project: project.path.clone(),
            mcp_url: event.connection.mcp_url,
            clipboard_state: event.connection.clipboard_state,
            clipboard_contains: event.connection.clipboard_contains,
            ready_for_chatgpt,
        });
        let exposure_readiness = match event.exposure.state.as_str() {
            "remote_ready" if handoff_available => ExposureReadiness::RemoteReady,
            "remote_ready" => ExposureReadiness::Degraded,
            "local_ready" => ExposureReadiness::LocalReady,
            _ => ExposureReadiness::Unknown,
        };
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            exposure_readiness,
            ProjectReadiness::Ready,
        );
        if !handoff_available {
            self.snapshot.readiness.next_action =
                Some("Clipboard handoff is unavailable; restart Quick Share after clipboard access is restored.".to_string());
        }
        self.activity.push(
            "quick_share",
            ActivityLevel::Info,
            "Quick Share reached verified readiness",
        );
        self.get_state().await
    }

    pub async fn stop_quick_share(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        self.supervisor.stop(ProcessKind::QuickShare).await;
        self.snapshot.quick_share = None;
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = self.config.project.clone();
        if self.config.runtime.is_some() {
            self.refresh_runtime_status().await
        } else {
            self.snapshot.readiness = DesktopStateSnapshot::default().readiness;
            self.get_state().await
        }
    }

    pub async fn stop_local_runtime(&mut self) -> DesktopResult<DesktopStateSnapshot> {
        self.supervisor.stop(ProcessKind::LocalRunner).await;
        self.supervisor.stop(ProcessKind::LocalServer).await;
        let exposure = exposure_readiness(self.config.topology.as_ref());
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            exposure,
            self.config
                .project
                .as_ref()
                .map(|_| ProjectReadiness::Configured)
                .unwrap_or(ProjectReadiness::None),
        );
        self.get_state().await
    }

    pub fn activity(&self) -> Vec<crate::activity::ActivityEntry> {
        self.activity.snapshot()
    }

    async fn wait_for_server(
        &mut self,
        server_url: &str,
        env_file: Option<&Path>,
        token_file: Option<&Path>,
    ) -> DesktopResult<()> {
        let deadline = tokio::time::Instant::now() + SERVER_READY_TIMEOUT;
        loop {
            if let Some(process) = self.supervisor.snapshot(ProcessKind::LocalServer) {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    return Err(DesktopError::new(
                        "server_start_failed",
                        "The Desktop-owned WebCodex Server exited during startup",
                        "Open Activity for safe diagnostics and retry.",
                    ));
                }
            }
            if self
                .adapter
                .server_status(Some(server_url), env_file, token_file)
                .await
                .is_ok_and(|status| status.http_reachable)
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DesktopError::new(
                    "server_unreachable",
                    "WebCodex Service did not become ready",
                    "Check the local Service diagnostics and retry.",
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn wait_for_runner(&mut self, identity: &ProjectRuntimeIdentity) -> DesktopResult<()> {
        let deadline = tokio::time::Instant::now() + RUNNER_READY_TIMEOUT;
        loop {
            if let Some(process) = self.supervisor.snapshot(ProcessKind::LocalRunner) {
                if matches!(process.phase, ProcessPhase::Exited | ProcessPhase::Failed) {
                    return Err(DesktopError::new(
                        "runner_offline",
                        "The Desktop-owned Runner exited while connecting",
                        "Open Activity for safe diagnostics and retry.",
                    ));
                }
            }
            if self.adapter.runner_ready(identity).await.unwrap_or(false) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DesktopError::new(
                    "runner_offline",
                    "Runner did not become connected",
                    "Check Server reachability and Runner diagnostics, then retry.",
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn wait_for_project(&mut self, identity: &ProjectRuntimeIdentity) -> DesktopResult<()> {
        let deadline = tokio::time::Instant::now() + PROJECT_READY_TIMEOUT;
        loop {
            if self.adapter.project_ready(identity).await.unwrap_or(false) {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(DesktopError::new(
                    "project_not_loaded",
                    "The selected project is registered but not loaded by the Runner",
                    "Restart the Runner or check the project registry, then retry.",
                ));
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    async fn store_identity(
        &mut self,
        project: &ProjectSelection,
        identity: &ProjectRuntimeIdentity,
        server_env_file: Option<PathBuf>,
    ) -> DesktopResult<()> {
        let mut project = project.clone();
        project.runtime_project_id = Some(identity.runtime_project_id.clone());
        self.config.project = Some(project.clone());
        self.config.runtime = Some(StoredRuntime {
            server_url: identity.server_url.clone(),
            server_env_file,
            runner_config: Some(identity.runner_config.clone()),
            user_token_file: Some(identity.user_token_file.clone()),
            project_id: Some(identity.project_id.clone()),
            runtime_project_id: Some(identity.runtime_project_id.clone()),
        });
        self.snapshot.project = Some(project);
        self.save_config().await
    }

    async fn save_config(&self) -> DesktopResult<()> {
        tokio::fs::create_dir_all(&self.data_dir)
            .await
            .map_err(|_| {
                DesktopError::new(
                    "desktop_state_unavailable",
                    "Desktop cannot create its app-data directory",
                    "Check local filesystem permissions and retry.",
                )
            })?;
        let encoded = serde_json::to_vec_pretty(&self.config).map_err(|_| {
            DesktopError::new(
                "desktop_state_invalid",
                "Desktop could not encode its non-secret runtime state",
                "Retry the setup operation.",
            )
        })?;
        tokio::fs::write(&self.config_path, encoded)
            .await
            .map_err(|_| {
                DesktopError::new(
                    "desktop_state_unavailable",
                    "Desktop could not persist its non-secret runtime state",
                    "Check local app-data permissions and retry.",
                )
            })
    }
}

fn project_snapshot(config: &StoredDesktopConfig) -> Option<ProjectSelection> {
    let mut project = config.project.clone()?;
    if identity_from_config(config).is_none() {
        project.runtime_project_id = None;
    }
    Some(project)
}

fn identity_from_config(config: &StoredDesktopConfig) -> Option<ProjectRuntimeIdentity> {
    let runtime = config.runtime.as_ref()?;
    let project = config.project.as_ref()?;
    let runner_config = runtime.runner_config.clone()?;
    let user_token_file = runtime.user_token_file.clone()?;
    let project_id = runtime.project_id.clone()?.trim().to_string();
    let runtime_project_id = runtime.runtime_project_id.clone()?.trim().to_string();
    if project_id.is_empty()
        || runtime_project_id.is_empty()
        || !runner_config.is_file()
        || !user_token_file.is_file()
    {
        return None;
    }
    Some(ProjectRuntimeIdentity {
        project_id,
        runtime_project_id,
        project_path: project.path.clone(),
        runner_config,
        user_token_file,
        server_url: runtime.server_url.clone(),
    })
}

fn load_config(path: &Path) -> Option<StoredDesktopConfig> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > 256 * 1024 {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn reserve_loopback_address() -> DesktopResult<String> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not reserve a loopback port for WebCodex",
            "Check local networking and retry.",
        )
    })?;
    let address = listener.local_addr().map_err(|_| {
        DesktopError::new(
            "local_port_unavailable",
            "Desktop could not inspect the reserved loopback port",
            "Retry setup.",
        )
    })?;
    Ok(address.to_string())
}

fn exposure_readiness(topology: Option<&RuntimeTopology>) -> ExposureReadiness {
    match topology.map(|topology| &topology.exposure) {
        Some(Exposure::None) => ExposureReadiness::LocalReady,
        Some(Exposure::ExistingHttps { .. }) => ExposureReadiness::RemoteReady,
        Some(Exposure::Cloudflare | Exposure::OpenAiTunnel) => ExposureReadiness::Unknown,
        None => ExposureReadiness::Unknown,
    }
}

fn openai_tunnel_is_configured() -> bool {
    ["CONTROL_PLANE_TUNNEL_ID", "CONTROL_PLANE_API_KEY"]
        .iter()
        .all(|name| std::env::var_os(name).is_some_and(|value| !value.is_empty()))
}

fn same_server(left: &str, right: &str) -> bool {
    left.trim_end_matches('/')
        .eq_ignore_ascii_case(right.trim_end_matches('/'))
}

fn same_project(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_runtime_contains_paths_not_credentials() {
        let runtime = StoredRuntime {
            server_url: "https://example.com".to_string(),
            server_env_file: Some(PathBuf::from("webcodex.env")),
            runner_config: Some(PathBuf::from("runner.toml")),
            user_token_file: Some(PathBuf::from("user-token")),
            project_id: Some("project".to_string()),
            runtime_project_id: Some("agent:desktop:project".to_string()),
        };
        let json = serde_json::to_string(&runtime).unwrap();
        assert!(!json.contains("wc_pat_"));
        assert!(!json.contains("wc_agent_"));
        assert!(!json.contains("CONTROL_PLANE_API_KEY"));
    }

    #[test]
    fn invalid_stored_identity_is_not_advertised_for_reuse() {
        let config = StoredDesktopConfig {
            topology: Some(RuntimeTopology {
                experience: Experience::Full,
                server: ServerTopology::Remote {
                    url: "https://example.test".to_string(),
                },
                runner: RunnerTopology::Local,
                exposure: Exposure::ExistingHttps {
                    url: "https://example.test".to_string(),
                },
                enrollment: Enrollment::ManagedPairing,
            }),
            project: Some(ProjectSelection {
                path: r"C:\repo".to_string(),
                allowed_root: r"C:\".to_string(),
                is_git_repository: true,
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
            runtime: Some(StoredRuntime {
                server_url: "https://example.test".to_string(),
                server_env_file: None,
                runner_config: Some(PathBuf::from("missing-runner.toml")),
                user_token_file: Some(PathBuf::from("missing-user-token")),
                project_id: Some("repo".to_string()),
                runtime_project_id: Some("agent:desktop:repo".to_string()),
            }),
        };
        assert_eq!(
            project_snapshot(&config).and_then(|project| project.runtime_project_id),
            None
        );
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_local_full_dogfood_reaches_ready_and_stops_owned_runtime() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-local-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone());
        let setup = core.configure_local_setup(&project).await;
        let snapshot = match setup {
            Ok(snapshot) => snapshot,
            Err(error) => {
                core.supervisor.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("local full setup failed: {error:?}");
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);

        let stopped = core.stop_local_runtime().await.expect("stop local runtime");
        assert_eq!(stopped.readiness.server, ServerReadiness::Stopped);
        assert_eq!(stopped.readiness.runner, RunnerReadiness::Stopped);
        assert!(core.supervisor.snapshot(ProcessKind::LocalServer).is_none());
        assert!(core.supervisor.snapshot(ProcessKind::LocalRunner).is_none());
        drop(core);
        std::fs::remove_dir_all(&data_dir).expect("remove local dogfood app data");
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_quick_share_dogfood_reaches_ready_and_stops_foreground_owner() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let data_dir = std::env::temp_dir().join(format!(
            "webcodex-desktop-share-dogfood-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&data_dir);
        let mut core = DesktopCore::new(data_dir.clone());
        let started = core.start_quick_share(&project, "none").await;
        let snapshot = match started {
            Ok(snapshot) => snapshot,
            Err(error) => {
                core.supervisor.stop_all().await;
                let _ = std::fs::remove_dir_all(&data_dir);
                panic!("Quick Share setup failed: {error:?}");
            }
        };
        assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
        assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
        assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
        assert_eq!(snapshot.readiness.exposure, ExposureReadiness::LocalReady);
        assert!(snapshot.readiness.runtime_ready);
        assert!(!snapshot.readiness.ready_for_chatgpt);
        assert!(core.supervisor.snapshot(ProcessKind::QuickShare).is_some());

        core.stop_quick_share().await.expect("stop Quick Share");
        assert!(core.supervisor.snapshot(ProcessKind::QuickShare).is_none());
        drop(core);
        let _ = std::fs::remove_dir_all(&data_dir);
    }

    #[tokio::test]
    #[ignore = "requires current-source dogfood binaries and a temporary project"]
    async fn windows_remote_full_dogfood_reuses_enrollment_without_local_server() {
        if !cfg!(windows) {
            return;
        }
        let project = std::env::var("WEBCODEX_DESKTOP_DOGFOOD_PROJECT")
            .expect("WEBCODEX_DESKTOP_DOGFOOD_PROJECT must point to the temporary fixture");
        let suffix = std::process::id();
        let host_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-host-dogfood-{suffix}"));
        let client_data =
            std::env::temp_dir().join(format!("webcodex-desktop-remote-client-dogfood-{suffix}"));
        let _ = std::fs::remove_dir_all(&host_data);
        let _ = std::fs::remove_dir_all(&client_data);

        let mut host = DesktopCore::new(host_data.clone());
        let host_runtime = host_data.join("runtime");
        let env_file = host_runtime.join("webcodex.env");
        let data_dir = host_runtime.join("data");
        tokio::fs::create_dir_all(&host_runtime)
            .await
            .expect("create remote dogfood host state");
        let listen = reserve_loopback_address().expect("reserve remote dogfood host port");
        let server_url = host
            .adapter
            .init_local_server(&listen, &data_dir, &env_file)
            .await
            .expect("initialize remote dogfood Server")
            .probe_url;
        let command = host
            .adapter
            .local_server_command(&env_file)
            .expect("build remote dogfood Server command");
        host.supervisor
            .spawn_owned(ProcessKind::LocalServer, command, false)
            .await
            .expect("start remote dogfood Server");

        let mut client = DesktopCore::new(client_data.clone());
        let result: DesktopResult<(DesktopStateSnapshot, DesktopStateSnapshot, bool, bool)> =
            async {
                host.wait_for_server(&server_url, Some(&env_file), None)
                    .await?;
                let pairing_code = host
                    .adapter
                    .create_local_pairing(&server_url, &env_file)
                    .await?;
                let first = client
                    .configure_remote_setup(&server_url, &pairing_code, &project)
                    .await?;
                let first_started_server = client
                    .supervisor
                    .snapshot(ProcessKind::LocalServer)
                    .is_some();
                client.stop_local_runtime().await?;

                let second = client
                    .configure_remote_setup(&server_url, "", &project)
                    .await?;
                let second_started_server = client
                    .supervisor
                    .snapshot(ProcessKind::LocalServer)
                    .is_some();
                client.stop_local_runtime().await?;
                Ok((first, second, first_started_server, second_started_server))
            }
            .await;

        client.supervisor.stop_all().await;
        host.supervisor.stop_all().await;
        let _ = std::fs::remove_dir_all(&client_data);
        let _ = std::fs::remove_dir_all(&host_data);

        let (first, second, first_started_server, second_started_server) =
            result.expect("remote full dogfood should complete");
        for snapshot in [&first, &second] {
            assert_eq!(snapshot.readiness.server, ServerReadiness::Ready);
            assert_eq!(snapshot.readiness.runner, RunnerReadiness::Ready);
            assert_eq!(snapshot.readiness.project, ProjectReadiness::Ready);
            assert!(snapshot.readiness.runtime_ready);
            assert!(matches!(
                snapshot.topology.as_ref().map(|topology| &topology.server),
                Some(ServerTopology::Remote { .. })
            ));
        }
        assert!(!first_started_server);
        assert!(!second_started_server);
    }

    #[test]
    fn local_and_remote_exposure_readiness_are_independent_of_runner() {
        let local = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Local,
            runner: RunnerTopology::Local,
            exposure: Exposure::None,
            enrollment: Enrollment::ManagedPairing,
        };
        let remote = RuntimeTopology {
            experience: Experience::Full,
            server: ServerTopology::Remote {
                url: "https://example.com".to_string(),
            },
            runner: RunnerTopology::Local,
            exposure: Exposure::ExistingHttps {
                url: "https://example.com".to_string(),
            },
            enrollment: Enrollment::ManagedPairing,
        };
        assert_eq!(
            exposure_readiness(Some(&local)),
            ExposureReadiness::LocalReady
        );
        assert_eq!(
            exposure_readiness(Some(&remote)),
            ExposureReadiness::RemoteReady
        );
    }
}
