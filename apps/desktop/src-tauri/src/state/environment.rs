use super::*;
use sha2::{Digest, Sha256};
use webcodex_environment::{
    canonical_server_url, current_account, default_environment_dir, EnvironmentMode,
    EnvironmentRecord, EnvironmentSetup, EnvironmentStore, NativeEnvironment, RuntimeBinaries,
    RuntimeObservation, Secret, SetupDiagnostic, SetupProgress, SetupRequest, SetupResult,
    SetupResultValue, SetupSecrets,
};
use webcodex_environment::{
    migrate_legacy_environment, migration_journal, LegacyImport, LegacyOwner, LegacyOwnerSnapshot,
    LegacyProcess, LegacyTunnelProfile, ProjectRecord,
};
use webcodex_environment::{service::Component, ServiceOperation};

pub(super) fn desktop_error(error: SetupDiagnostic) -> DesktopError {
    DesktopError::new(error.code, error.message, error.recovery)
}

pub(super) fn store() -> DesktopResult<EnvironmentStore> {
    EnvironmentStore::open(default_environment_dir().map_err(desktop_error)?).map_err(desktop_error)
}

impl AppState {
    pub async fn repair_environment_user_credential(
        &self,
        request: crate::models::EnvironmentUserCredentialRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let token = Secret::new(request.user_token);
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::EnvironmentService, false)
            .await?;
        let result = async {
            cancellation.check()?;
            if core.config.persistent_environment.as_deref()
                != Some(request.environment_id.as_str())
            {
                return Err(DesktopError::new(
                    "environment_changed",
                    "The environment changed",
                    "Refresh before restoring its user credential.",
                ));
            }
            NativeEnvironment::new()
                .map_err(desktop_error)?
                .repair_user_credential(&store()?, &request.environment_id, &token)
                .await
                .map_err(desktop_error)?;
            core.refresh_environment_status(&cancellation).await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }

    pub async fn environment_service_action(
        &self,
        request: crate::models::EnvironmentServiceRequest,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let (operation, cancellation, mut core, baseline) = self
            .begin_operation(DesktopOperationKind::EnvironmentService, false)
            .await?;
        let result = async {
            cancellation.check()?;
            if core.config.persistent_environment.as_deref()
                != Some(request.environment_id.as_str())
            {
                return Err(DesktopError::new(
                    "environment_changed",
                    "The environment changed",
                    "Refresh before controlling local services.",
                ));
            }
            use crate::models::{
                EnvironmentServiceAction as Action, EnvironmentServiceComponent as Target,
            };
            let component = match request.component {
                Target::Server => Component::Server,
                Target::Runner => Component::Runner,
            };
            let action = match request.action {
                Action::Start => ServiceOperation::Start,
                Action::Stop => ServiceOperation::Stop,
                Action::Restart => ServiceOperation::Restart,
                Action::RepairCredential if cfg!(windows) && component == Component::Runner => {
                    ServiceOperation::UpdateCredential
                }
                Action::RepairCredential => {
                    return Err(DesktopError::new(
                        "credential_repair_unavailable",
                        "This component does not use a Windows user service credential",
                        "Select a configured Windows Runner service.",
                    ))
                }
            };
            NativeEnvironment::new()
                .map_err(desktop_error)?
                .control_service_for_environment(
                    &store()?,
                    Some(&request.environment_id),
                    component,
                    action,
                )
                .await
                .map_err(desktop_error)?;
            core.refresh_environment_status(&cancellation).await
        }
        .await;
        self.finish_operation(operation, cancellation, core, baseline, result)
            .await
    }
}

fn persisted_environment(config: &StoredDesktopConfig) -> bool {
    config.persistent_environment.is_some()
}

fn migration_target_conflict() -> DesktopError {
    DesktopError::new(
        "migration_target_conflict",
        "Migration must preserve the existing Server and project identity",
        "Use the current Server address and project; change ownership only through an explicit later operation.",
    )
}

fn migration_diagnostic(error: DesktopError) -> SetupDiagnostic {
    SetupDiagnostic::new(&error.code, &error.message, &error.next_action)
}

fn legacy_process_key(key: ProcessKey) -> String {
    match key {
        ProcessKey::LocalServer => "server".into(),
        ProcessKey::LocalRunner => "runner".into(),
        ProcessKey::RegularTunnel(id) => format!("tunnel:{id}"),
        ProcessKey::QuickShare => "quick_share".into(),
    }
}

fn config_fingerprint(config: &StoredDesktopConfig) -> SetupResultValue<String> {
    let runtime = config.runtime.as_ref().ok_or_else(|| {
        SetupDiagnostic::new(
            "legacy_runtime",
            "The original runtime identity is missing",
            "Restore the original Desktop configuration",
        )
    })?;
    let mut digest = Sha256::new();
    digest.update(runtime.server_url.as_bytes());
    for path in [&runtime.server_env_file, &runtime.runner_config] {
        let Some(path) = path else { continue };
        digest.update(path.to_string_lossy().as_bytes());
        let metadata = std::fs::metadata(path).map_err(|_| {
            SetupDiagnostic::new(
                "legacy_file",
                "The original configuration file is unavailable",
                "Restore the original configuration before migration",
            )
        })?;
        digest.update(metadata.len().to_le_bytes());
        if let Ok(modified) = metadata.modified() {
            if let Ok(elapsed) = modified.duration_since(std::time::UNIX_EPOCH) {
                digest.update(elapsed.as_nanos().to_le_bytes());
            }
        }
        if runtime.server_env_file.as_ref() == Some(path) {
            let content = std::fs::read_to_string(path).map_err(|_| {
                SetupDiagnostic::new(
                    "legacy_file",
                    "The original Server configuration cannot be read",
                    "Restore the original configuration before migration",
                )
            })?;
            for line in content.lines() {
                let key = line
                    .split_once('=')
                    .map(|(key, _)| key.trim().to_ascii_uppercase())
                    .unwrap_or_default();
                if !(key.contains("TOKEN")
                    || key.contains("KEY")
                    || key.contains("SECRET")
                    || key.contains("PASSWORD"))
                {
                    digest.update(line.as_bytes());
                }
            }
        } else {
            let content = std::fs::read_to_string(path).map_err(|_| {
                SetupDiagnostic::new(
                    "legacy_file",
                    "The original Runner configuration cannot be read",
                    "Restore the original configuration before migration",
                )
            })?;
            let mut document = content.parse::<toml_edit::DocumentMut>().map_err(|_| {
                SetupDiagnostic::new(
                    "legacy_runner_configuration",
                    "The original Runner configuration is invalid",
                    "Repair the original configuration before migration",
                )
            })?;
            document.remove("token");
            digest.update(document.to_string().as_bytes());
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Read-only projection for environments configured by CLI before Desktop opens.
/// Existing Desktop-owned state is deliberately left for explicit migration.
pub(super) fn adopt_saved_environment(config: &mut StoredDesktopConfig) {
    if config.topology.is_some() && config.persistent_environment.is_none() {
        let committed = store()
            .ok()
            .and_then(|store| migration_journal(&store).ok().flatten())
            .is_some_and(|journal| {
                journal.phase == webcodex_environment::MigrationPhase::Committed
            });
        if !committed {
            return;
        }
    }
    let Ok(store) = store() else { return };
    let Ok(Some(record)) = store.load_environment() else {
        return;
    };
    if config
        .persistent_environment
        .as_deref()
        .is_some_and(|id| id != record.environment_id)
    {
        return;
    }
    let local_server = record.request.local_server();
    let local_runner = record.request.local_runner();
    let project = record.request.project.as_ref().map(|path| {
        let runtime_project_id = record.runner_client_id.as_ref().and_then(|client| {
            record
                .projects
                .iter()
                .find(|entry| entry.path == *path)
                .map(|entry| format!("agent:{client}:{}", entry.id))
        });
        ProjectSelection {
            path: path.to_string_lossy().into_owned(),
            allowed_root: path.to_string_lossy().into_owned(),
            is_git_repository: path.join(".git").exists(),
            runtime_project_id,
        }
    });
    config.persistent_environment = Some(record.environment_id);
    config.topology = Some(RuntimeTopology {
        experience: Experience::Full,
        server: if local_server {
            ServerTopology::Local
        } else {
            ServerTopology::Remote {
                url: record.request.server_url.clone(),
            }
        },
        runner: if local_runner {
            RunnerTopology::Local
        } else {
            RunnerTopology::None
        },
        exposure: if local_server {
            Exposure::None
        } else if record.request.server_url.starts_with("https://") {
            Exposure::ExistingHttps {
                url: record.request.server_url.clone(),
            }
        } else {
            Exposure::None
        },
        enrollment: if local_runner && !local_server {
            Enrollment::ManagedPairing
        } else {
            Enrollment::UserCredential
        },
    });
    config.project = project.clone();
    config.runtime = Some(StoredRuntime {
        server_url: record.request.server_url,
        server_env_file: local_server.then(|| store.root().join("server/webcodex.env")),
        runner_config: local_runner.then(|| store.root().join("runner.toml")),
        user_token_file: Some(store.root().join("webcodex-user-token")),
        runner_client_id: record.runner_client_id,
        project_id: project
            .as_ref()
            .and_then(|project| project.runtime_project_id.as_deref())
            .and_then(|id| id.rsplit(':').next())
            .map(str::to_owned),
        runtime_project_id: project
            .as_ref()
            .and_then(|project| project.runtime_project_id.clone()),
    });
    config.runtime_autostart = Some(true);
}

pub(super) fn migration_in_progress() -> bool {
    store()
        .ok()
        .and_then(|store| migration_journal(&store).ok().flatten())
        .is_some_and(|journal| {
            !matches!(
                journal.phase,
                webcodex_environment::MigrationPhase::Committed
                    | webcodex_environment::MigrationPhase::RecoveryRequired
            )
        })
}

impl DesktopCore {
    fn setup_progress_sink(&self) -> impl FnMut(SetupProgress) + Send + 'static {
        let published = Arc::clone(&self.published);
        move |progress| {
            published
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .setup_progress = Some(progress);
        }
    }

    pub async fn configure_environment(
        &mut self,
        input: crate::models::EnvironmentInput,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let legacy = self
            .config
            .topology
            .as_ref()
            .is_some_and(|topology| topology.experience == Experience::Full)
            && !persisted_environment(&self.config);
        let project = input
            .project_path
            .as_deref()
            .filter(|path| !path.trim().is_empty());
        let project = match project {
            Some(path) => Some(std::fs::canonicalize(path).map_err(|_| {
                DesktopError::new(
                    "project_path",
                    "The selected project directory is unavailable",
                    "Choose an existing project directory and retry.",
                )
            })?),
            None => None,
        };
        let (mode, server_url) = match input.mode.as_str() {
            "create" if persisted_environment(&self.config) => {
                let saved = store()?
                    .load_environment()
                    .map_err(desktop_error)?
                    .ok_or_else(migration_target_conflict)?;
                if !saved.request.local_server() {
                    return Err(migration_target_conflict());
                }
                // Preserve the Core's exact listener, including an explicitly
                // migrated old Server; the UI never chooses a replacement bind.
                (saved.request.mode, saved.request.server_url)
            }
            "create" => {
                let old_url = if legacy || persisted_environment(&self.config) {
                    if !matches!(
                        self.config
                            .topology
                            .as_ref()
                            .map(|topology| &topology.server),
                        Some(ServerTopology::Local)
                    ) {
                        return Err(migration_target_conflict());
                    }
                    self.config
                        .runtime
                        .as_ref()
                        .map(|runtime| runtime.server_url.clone())
                        .ok_or_else(migration_target_conflict)?
                } else {
                    "http://127.0.0.1:8080".to_owned()
                };
                let parsed = url::Url::parse(&old_url).map_err(|_| migration_target_conflict())?;
                let listen = format!(
                    "{}:{}",
                    parsed.host_str().ok_or_else(migration_target_conflict)?,
                    parsed.port().ok_or_else(migration_target_conflict)?
                );
                (
                    EnvironmentMode::Create { listen },
                    canonical_server_url(&old_url).map_err(desktop_error)?,
                )
            }
            "join" => (
                EnvironmentMode::Join,
                canonical_server_url(input.server_url.as_deref().unwrap_or(""))
                    .map_err(desktop_error)?,
            ),
            _ => {
                return Err(DesktopError::new(
                    "setup_mode",
                    "Choose Create or Join",
                    "Select an environment action.",
                ))
            }
        };
        if legacy {
            let old = self
                .config
                .runtime
                .as_ref()
                .ok_or_else(migration_target_conflict)?;
            if old.server_url != server_url
                || self
                    .config
                    .project
                    .as_ref()
                    .map(|value| std::fs::canonicalize(&value.path).ok())
                    != Some(project.clone())
            {
                return Err(migration_target_conflict());
            }
        }
        let binaries = self.adapter.ensure_binaries(cancellation).await?.clone();
        crate::runtime_selection::verify_resolved_files(&binaries).await?;
        self.snapshot.binaries = Some(binaries.info());
        let request = SetupRequest {
            mode,
            server_url,
            project,
            account: current_account().map_err(desktop_error)?,
            binaries: RuntimeBinaries {
                cli: binaries.webcodex,
                server: binaries.server,
                runner: binaries.runner,
            },
        };
        let secrets = SetupSecrets {
            pairing_code: if legacy {
                None
            } else {
                input
                    .pairing_code
                    .filter(|value| !value.is_empty())
                    .map(Secret::new)
            },
            user_token: input
                .user_token
                .filter(|value| !value.is_empty())
                .map(Secret::new),
            replacement_pairing_code: input.replace_pairing_code,
            ..Default::default()
        };
        let store = store()?;
        if legacy {
            return self
                .migrate_environment(&store, request, &secrets, cancellation)
                .await;
        }
        if persisted_environment(&self.config) {
            let saved = store
                .load_environment()
                .map_err(desktop_error)?
                .ok_or_else(migration_target_conflict)?;
            if self.config.persistent_environment.as_deref() != Some(saved.environment_id.as_str())
                || saved.request.server_url != request.server_url
                || saved.request.mode != request.mode
                || saved.request.account.identity != request.account.identity
            {
                return Err(migration_target_conflict());
            }
            let mut setup = EnvironmentSetup::new(NativeEnvironment::new().map_err(desktop_error)?);
            let result = match request.project {
                Some(project) if saved.runner_client_id.is_none() => {
                    setup
                        .enable_runner(&store, project, &secrets, self.setup_progress_sink())
                        .await
                }
                Some(project) if saved.request.project.as_ref() != Some(&project) => {
                    setup.backend.add_project(&store, &project).await
                }
                Some(_) => {
                    setup
                        .resume(&store, &secrets, self.setup_progress_sink())
                        .await
                }
                None if saved.request.project == request.project => {
                    setup
                        .resume(&store, &secrets, self.setup_progress_sink())
                        .await
                }
                None => {
                    return Err(DesktopError::new(
                        "project_removal_explicit",
                        "Skipping the folder cannot remove a saved Runner project",
                        "Use the confirmed project removal control instead.",
                    ))
                }
            }
            .map_err(desktop_error)?;
            cancellation.check()?;
            return self.project_environment_result(&store, result).await;
        }
        let mut setup = EnvironmentSetup::new(NativeEnvironment::new().map_err(desktop_error)?);
        // Core owns the persisted step journal and reconciliation. Do not log
        // transient one-time credentials or duplicate its step implementation.
        let result = setup
            .configure(&store, request, &secrets, self.setup_progress_sink())
            .await
            .map_err(desktop_error)?;
        cancellation.check()?;
        self.project_environment_result(&store, result).await
    }

    pub async fn resume_environment(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let store = store()?;
        let mut setup = EnvironmentSetup::new(NativeEnvironment::new().map_err(desktop_error)?);
        let result = setup
            .resume(&store, &SetupSecrets::default(), self.setup_progress_sink())
            .await
            .map_err(desktop_error)?;
        cancellation.check()?;
        self.project_environment_result(&store, result).await
    }

    pub async fn refresh_environment_status(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let store = store()?;
        let mut native = NativeEnvironment::new().map_err(desktop_error)?;
        let result = native.status(&store).await.map_err(desktop_error)?;
        cancellation.check()?;
        self.project_environment_result(&store, result).await
    }

    pub async fn add_environment_project(
        &mut self,
        project_path: &str,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let store = store()?;
        let saved = store
            .load_environment()
            .map_err(desktop_error)?
            .ok_or_else(migration_target_conflict)?;
        let result = if saved.runner_client_id.is_none() {
            let mut setup = EnvironmentSetup::new(NativeEnvironment::new().map_err(desktop_error)?);
            setup
                .enable_runner(
                    &store,
                    PathBuf::from(project_path),
                    &SetupSecrets::default(),
                    self.setup_progress_sink(),
                )
                .await
                .map_err(desktop_error)?
        } else {
            let mut native = NativeEnvironment::new().map_err(desktop_error)?;
            native
                .add_project(&store, Path::new(project_path))
                .await
                .map_err(desktop_error)?
        };
        cancellation.check()?;
        self.config.project = Some(self.adapter.inspect_project(project_path).await?);
        self.project_environment_result(&store, result).await
    }

    async fn migrate_environment(
        &mut self,
        store: &EnvironmentStore,
        request: SetupRequest,
        secrets: &SetupSecrets,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        if let Some(journal) = migration_journal(store).map_err(desktop_error)? {
            if journal.request != request {
                return Err(migration_target_conflict());
            }
            let progress = self.setup_progress_sink();
            let result =
                migrate_legacy_environment(store, request, journal.import, self, secrets, progress)
                    .await
                    .map_err(desktop_error)?;
            cancellation.check()?;
            return self.project_environment_result(store, result).await;
        }
        if self.snapshot.quick_share.is_some() {
            return Err(DesktopError::new(
                "quick_share_active",
                "Quick Share is using the Desktop process owner",
                "Stop Quick Share before migrating the saved runtime.",
            ));
        }
        let runtime = self
            .config
            .runtime
            .clone()
            .ok_or_else(migration_target_conflict)?;
        let runner_owner = self.process_snapshot(ProcessKey::LocalRunner).await;
        if !runner_owner.is_some_and(|process| {
            process.owned_by_desktop
                && matches!(
                    process.phase,
                    ProcessPhase::Starting | ProcessPhase::Running
                )
        }) {
            return Err(DesktopError::new(
                "legacy_owner_unknown",
                "The running legacy Runner is not owned by this Desktop",
                "Inspect its actual process owner before migrating this environment.",
            ));
        }
        if request.local_server() {
            let server_owner = self.process_snapshot(ProcessKey::LocalServer).await;
            if !server_owner.is_some_and(|process| {
                process.owned_by_desktop
                    && matches!(
                        process.phase,
                        ProcessPhase::Starting | ProcessPhase::Running
                    )
            }) {
                return Err(DesktopError::new(
                    "legacy_owner_unknown",
                    "The running legacy Server is not owned by this Desktop",
                    "Inspect its actual process owner before migrating this environment.",
                ));
            }
        }
        let config_path = runtime
            .runner_config
            .clone()
            .ok_or_else(migration_target_conflict)?;
        let document = std::fs::read_to_string(&config_path)
            .map_err(|_| migration_target_conflict())?
            .parse::<toml_edit::DocumentMut>()
            .map_err(|_| migration_target_conflict())?;
        let username = document
            .get("owner")
            .and_then(|value| value.as_str())
            .filter(|value| !value.is_empty())
            .ok_or_else(migration_target_conflict)?
            .to_owned();
        let client_id =
            stored_runner_client_id(&self.config).ok_or_else(migration_target_conflict)?;
        let inventory = crate::workspace::query(
            &runtime,
            crate::workspace::WorkspaceRequest::RunnerDetails {},
        )
        .await?;
        if inventory.get("connected").and_then(Value::as_bool) != Some(true)
            || inventory.get("projects_truncated").and_then(Value::as_bool) != Some(false)
        {
            return Err(DesktopError::new("migration_inventory_incomplete",
                "The original Runner inventory is not complete",
                "Restore the original Runner connection before migrating its project registrations."));
        }
        let rows = inventory
            .get("projects")
            .and_then(Value::as_array)
            .ok_or_else(migration_target_conflict)?;
        if inventory
            .get("visible_project_count")
            .and_then(Value::as_u64)
            != Some(rows.len() as u64)
        {
            return Err(DesktopError::new(
                "migration_inventory_incomplete",
                "The original Runner inventory is truncated",
                "Retry after the Server reports a complete inventory.",
            ));
        }
        let prefix = format!("agent:{client_id}:");
        let mut projects = Vec::with_capacity(rows.len());
        for row in rows {
            let id = row
                .get("id")
                .and_then(Value::as_str)
                .and_then(|value| value.strip_prefix(&prefix))
                .filter(|value| !value.is_empty())
                .ok_or_else(migration_target_conflict)?;
            let path = row
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(migration_target_conflict)?;
            projects.push(ProjectRecord {
                id: id.to_owned(),
                path: std::fs::canonicalize(path).map_err(|_| migration_target_conflict())?,
            });
        }
        if !projects
            .iter()
            .any(|project| Some(&project.path) == request.project.as_ref())
        {
            return Err(migration_target_conflict());
        }
        let import = LegacyImport {
            server_env_file: if request.local_server() {
                runtime.server_env_file.clone()
            } else {
                None
            },
            runner_config_file: Some(config_path),
            user_token_file: runtime
                .user_token_file
                .clone()
                .ok_or_else(migration_target_conflict)?,
            username,
            runner_client_id: Some(client_id),
            projects,
            tunnel_profiles: if request.local_server() {
                let mut profiles = Vec::new();
                for profile in self.tunnel_config.profiles() {
                    profiles.push(LegacyTunnelProfile {
                        profile_id: profile.id.to_string(),
                        start: process_is_active(
                            self.process_snapshot(ProcessKey::RegularTunnel(profile.id))
                                .await,
                        ),
                    });
                }
                profiles
            } else {
                Vec::new()
            },
        };
        let progress = self.setup_progress_sink();
        let result = migrate_legacy_environment(store, request, import, self, secrets, progress)
            .await
            .map_err(desktop_error)?;
        cancellation.check()?;
        self.project_environment_result(store, result).await
    }

    pub async fn stop_environment(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let store = store()?;
        let record = store
            .load_environment()
            .map_err(desktop_error)?
            .ok_or_else(|| {
                DesktopError::new(
                    "not_configured",
                    "No persistent environment is saved",
                    "Inspect environment status before controlling services.",
                )
            })?;
        let native = NativeEnvironment::new().map_err(desktop_error)?;
        if record.request.local_runner() {
            native
                .control_service(&store, Component::Runner, ServiceOperation::Stop)
                .await
                .map_err(desktop_error)?;
        }
        if record.request.local_server() {
            native
                .control_service(&store, Component::Server, ServiceOperation::Stop)
                .await
                .map_err(desktop_error)?;
        }
        cancellation.check()?;
        self.config.runtime_autostart = Some(false);
        self.save_config().await?;
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Stopped,
            RunnerReadiness::Stopped,
            ExposureReadiness::Disabled,
            if record.request.local_runner() {
                ProjectReadiness::Configured
            } else {
                ProjectReadiness::None
            },
        );
        self.get_state().await
    }

    pub async fn restart_environment_runner(
        &mut self,
        cancellation: &CancellationContext,
    ) -> DesktopResult<DesktopStateSnapshot> {
        cancellation.check()?;
        let store = store()?;
        let native = NativeEnvironment::new().map_err(desktop_error)?;
        native
            .control_service(&store, Component::Runner, ServiceOperation::Restart)
            .await
            .map_err(desktop_error)?;
        self.refresh_environment_status(cancellation).await
    }

    pub(super) async fn project_environment_result(
        &mut self,
        store: &EnvironmentStore,
        result: SetupResult,
    ) -> DesktopResult<DesktopStateSnapshot> {
        let record = result.environment;
        let observation = result.observation;
        self.apply_environment_record(store, &record, &observation)
            .await?;
        self.get_state().await
    }

    async fn apply_environment_record(
        &mut self,
        store: &EnvironmentStore,
        record: &EnvironmentRecord,
        observation: &RuntimeObservation,
    ) -> DesktopResult<()> {
        let local_server = record.request.local_server();
        let local_runner = record.request.local_runner();
        let selected = self
            .config
            .project
            .as_ref()
            .map(|project| PathBuf::from(&project.path))
            .filter(|path| record.projects.iter().any(|entry| entry.path == *path))
            .or_else(|| {
                record
                    .request
                    .project
                    .as_ref()
                    .filter(|path| record.projects.iter().any(|entry| entry.path == **path))
                    .cloned()
            });
        let project = if let Some(path) = selected.as_ref() {
            let mut project = self
                .adapter
                .inspect_project(&path.to_string_lossy())
                .await?;
            if let (Some(client), Some(entry)) = (
                record.runner_client_id.as_deref(),
                record.projects.iter().find(|entry| entry.path == *path),
            ) {
                project.runtime_project_id = Some(format!("agent:{client}:{}", entry.id));
            }
            Some(project)
        } else {
            None
        };
        self.config.persistent_environment = Some(record.environment_id.clone());
        self.config.topology = Some(RuntimeTopology {
            experience: Experience::Full,
            server: if local_server {
                ServerTopology::Local
            } else {
                ServerTopology::Remote {
                    url: record.request.server_url.clone(),
                }
            },
            runner: if local_runner {
                RunnerTopology::Local
            } else {
                RunnerTopology::None
            },
            exposure: if local_server {
                Exposure::None
            } else if record.request.server_url.starts_with("https://") {
                Exposure::ExistingHttps {
                    url: record.request.server_url.clone(),
                }
            } else {
                Exposure::None
            },
            enrollment: if local_runner && !local_server {
                Enrollment::ManagedPairing
            } else {
                Enrollment::UserCredential
            },
        });
        self.config.project = project.clone();
        let project_id = project
            .as_ref()
            .and_then(|project| project.runtime_project_id.as_deref())
            .and_then(|id| id.rsplit(':').next())
            .map(str::to_owned);
        self.config.runtime = Some(StoredRuntime {
            server_url: record.request.server_url.clone(),
            server_env_file: local_server.then(|| store.root().join("server/webcodex.env")),
            runner_config: local_runner.then(|| store.root().join("runner.toml")),
            user_token_file: Some(store.root().join("webcodex-user-token")),
            runner_client_id: record.runner_client_id.clone(),
            project_id,
            runtime_project_id: project
                .as_ref()
                .and_then(|project| project.runtime_project_id.clone()),
        });
        self.config.runtime_autostart = Some(true);
        self.save_config().await?;
        self.snapshot.topology = self.config.topology.clone();
        self.snapshot.project = project;
        let server = if observation.server_reachable && observation.authenticated {
            ServerReadiness::Ready
        } else if observation.server_reachable {
            ServerReadiness::Error
        } else {
            ServerReadiness::Unknown
        };
        let runner = match observation.runner_online {
            Some(true) => RunnerReadiness::Ready,
            Some(false) => RunnerReadiness::Connecting,
            None => RunnerReadiness::Stopped,
        };
        let project_readiness = if self.config.project.is_none() {
            ProjectReadiness::None
        } else if record.projects.iter().any(|entry| {
            self.config
                .project
                .as_ref()
                .is_some_and(|project| Path::new(&project.path) == entry.path)
                && observation
                    .projects_visible
                    .iter()
                    .any(|visible| visible == &entry.id)
        }) {
            ProjectReadiness::Ready
        } else {
            ProjectReadiness::Configured
        };
        self.snapshot.readiness = aggregate_readiness(
            server.clone(),
            runner,
            exposure_readiness(self.config.topology.as_ref()),
            project_readiness,
        );
        if !local_runner {
            self.snapshot.readiness.runtime_ready = server == ServerReadiness::Ready;
            self.snapshot.readiness.ready_for_chatgpt = false;
            self.snapshot.readiness.summary = if local_server {
                "Server is available".to_owned()
            } else {
                "Connected to Server".to_owned()
            };
            self.snapshot.readiness.next_action = None;
            self.snapshot.readiness.next_action_kind = None;
        }
        Ok(())
    }
}

impl LegacyOwner for DesktopCore {
    async fn tunnel_credentials(
        &mut self,
        profile_id: &str,
    ) -> SetupResultValue<webcodex_environment::TunnelCredentials> {
        let id = crate::connection_id::TunnelProfileId::try_from(profile_id.to_owned()).map_err(
            |_| {
                SetupDiagnostic::new(
                    "legacy_tunnel_id",
                    "The original Tunnel identity is invalid",
                    "Inspect the saved connection profile before recovery",
                )
            },
        )?;
        self.tunnel_config
            .credentials_for(id)
            .map_err(migration_diagnostic)
    }

    async fn inspect(&mut self) -> SetupResultValue<LegacyOwnerSnapshot> {
        let runtime = self
            .config
            .runtime
            .as_ref()
            .ok_or_else(|| migration_diagnostic(migration_target_conflict()))?;
        let owner_id = format!(
            "desktop:{}:{}",
            self.config_path.display(),
            runtime.server_url
        );
        let configuration_fingerprint = config_fingerprint(&self.config)?;
        let mut supervisor = self.supervisor.lock().await;
        let mut processes = Vec::new();
        for key in supervisor.keys() {
            let Some(snapshot) = supervisor.snapshot(key) else {
                continue;
            };
            if !matches!(
                snapshot.phase,
                ProcessPhase::Starting | ProcessPhase::Running
            ) {
                continue;
            }
            if !snapshot.owned_by_desktop || key == ProcessKey::QuickShare {
                return Err(SetupDiagnostic::new(
                    "legacy_owner_unknown",
                    "A legacy process has an unknown or incompatible owner",
                    "Stop Quick Share and inspect the actual process owner before migration",
                ));
            }
            let pid = snapshot.pid.ok_or_else(|| {
                SetupDiagnostic::new(
                    "legacy_pid",
                    "The original process has no verifiable PID",
                    "Wait for process startup before migrating",
                )
            })?;
            processes.push(LegacyProcess {
                kind: legacy_process_key(key),
                pid,
                generation: snapshot.generation,
            });
        }
        processes.sort_by(|left, right| left.kind.cmp(&right.kind));
        Ok(LegacyOwnerSnapshot {
            owner_id,
            configuration_fingerprint,
            processes,
        })
    }

    async fn stop(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        let before = LegacyOwner::inspect(self).await?;
        if before.owner_id != expected.owner_id
            || before.configuration_fingerprint != expected.configuration_fingerprint
            || before
                .processes
                .iter()
                .any(|process| !expected.processes.contains(process))
        {
            return Err(SetupDiagnostic::new(
                "legacy_owner_changed",
                "The Desktop-owned process generation changed before shutdown",
                "Inspect and restart migration from the original owner",
            ));
        }
        let mut keys = {
            let mut supervisor = self.supervisor.lock().await;
            supervisor.keys()
        };
        keys.sort_by_key(|key| match key {
            ProcessKey::RegularTunnel(_) => 0,
            ProcessKey::LocalRunner => 1,
            ProcessKey::LocalServer => 2,
            ProcessKey::QuickShare => 3,
        });
        for key in keys {
            let kind = legacy_process_key(key);
            let Some(captured) = expected
                .processes
                .iter()
                .find(|process| process.kind == kind)
            else {
                continue;
            };
            let mut supervisor = self.supervisor.lock().await;
            let Some(current) = supervisor.snapshot(key) else {
                continue;
            };
            if current.generation != captured.generation
                || current.pid != Some(captured.pid)
                || !current.owned_by_desktop
            {
                return Err(SetupDiagnostic::new(
                    "legacy_process_changed",
                    "The process generation changed during owner handoff",
                    "Inspect both owners before another stop attempt",
                ));
            }
            if let ProcessKey::RegularTunnel(id) = key {
                self.connections.stopping(id);
            }
            let result = supervisor.stop_checked(key).await;
            if let ProcessKey::RegularTunnel(id) = key {
                self.connections.stopped(id, result.is_ok());
            }
            result.map_err(migration_diagnostic)?;
        }
        Ok(())
    }

    async fn restore(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        if expected.processes.is_empty() {
            return Ok(());
        }
        let cancellation =
            CancellationContext::new(CancellationSignal::new(), CancellationSignal::new());
        let runtime = self
            .config
            .runtime
            .clone()
            .ok_or_else(|| migration_diagnostic(migration_target_conflict()))?;
        self.adapter
            .ensure_binaries(&cancellation)
            .await
            .map_err(migration_diagnostic)?;
        if expected
            .processes
            .iter()
            .any(|process| process.kind == "server")
            && self
                .process_snapshot(ProcessKey::LocalServer)
                .await
                .is_none()
        {
            let env = runtime
                .server_env_file
                .as_deref()
                .ok_or_else(|| migration_diagnostic(migration_target_conflict()))?;
            let command = self
                .adapter
                .local_server_command(env)
                .map_err(migration_diagnostic)?;
            self.spawn_owned(ProcessKey::LocalServer, command, false, &cancellation)
                .await
                .map_err(migration_diagnostic)?;
            self.wait_for_server(
                &runtime.server_url,
                Some(env),
                runtime.user_token_file.as_deref(),
                &cancellation,
                Deadline::after(SERVER_READY_TIMEOUT),
                true,
            )
            .await
            .map_err(migration_diagnostic)?;
        }
        if expected
            .processes
            .iter()
            .any(|process| process.kind == "runner")
            && self
                .process_snapshot(ProcessKey::LocalRunner)
                .await
                .is_none()
        {
            let identity = runner_identity_from_config(&self.config)
                .ok_or_else(|| migration_diagnostic(migration_target_conflict()))?;
            self.spawn_configured_runner(&identity, &cancellation)
                .await
                .map_err(migration_diagnostic)?;
            self.wait_for_runner(
                &identity,
                &cancellation,
                Deadline::after(RUNNER_READY_TIMEOUT),
                true,
            )
            .await
            .map_err(migration_diagnostic)?;
        }
        self.snapshot.readiness = aggregate_readiness(
            ServerReadiness::Ready,
            RunnerReadiness::Ready,
            exposure_readiness(self.config.topology.as_ref()),
            if self.config.project.is_some() {
                ProjectReadiness::Ready
            } else {
                ProjectReadiness::None
            },
        );
        self.publish_snapshot();
        for process in &expected.processes {
            let Some(id) = process.kind.strip_prefix("tunnel:") else {
                continue;
            };
            let id =
                crate::connection_id::TunnelProfileId::try_from(id.to_owned()).map_err(|_| {
                    SetupDiagnostic::new(
                        "legacy_tunnel_id",
                        "The original Tunnel identity is invalid",
                        "Inspect the saved connection profile before recovery",
                    )
                })?;
            if self
                .process_snapshot(ProcessKey::RegularTunnel(id))
                .await
                .is_none()
            {
                self.start_connection_process(id, &cancellation)
                    .await
                    .map_err(migration_diagnostic)?;
            }
        }
        Ok(())
    }
}
