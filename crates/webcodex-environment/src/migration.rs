//! Durable handoff from a known legacy process owner to native system services.
//! The journal contains paths and process identities, never credential contents.
#[cfg(windows)]
use crate::service::ServiceCredential;
use crate::service::{Component, Ownership, ServiceManager};
use crate::storage::{
    atomic_private_write, ensure_private_directory, read_private, EnvironmentLock,
};
use crate::tunnel::{tunnel_service_spec, TunnelCredentials};
use crate::{
    service_spec, EnvironmentMode, EnvironmentRecord, EnvironmentSetup, EnvironmentStore,
    NativeEnvironment, ProjectRecord, ServiceOperation, SetupDiagnostic, SetupJournal,
    SetupProgress, SetupRequest, SetupResult, SetupResultValue, SetupSecrets, ENVIRONMENT_SCHEMA,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const MIGRATION_SCHEMA: u16 = 1;
const MAX_COPY_ENTRIES: usize = 100_000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyProcess {
    pub kind: String,
    pub pid: u32,
    pub generation: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyOwnerSnapshot {
    /// Stable path/identity of the legacy owner; no bearer credential.
    pub owner_id: String,
    /// Fingerprint of non-secret binding fields and file identity.
    pub configuration_fingerprint: String,
    pub processes: Vec<LegacyProcess>,
}

impl LegacyOwnerSnapshot {
    fn same_configuration(&self, other: &Self) -> bool {
        self.owner_id == other.owner_id
            && self.configuration_fingerprint == other.configuration_fingerprint
    }

    fn stopped_against(&self, observed: &Self) -> bool {
        self.same_configuration(observed) && observed.processes.is_empty()
    }
}

/// A host adapter may only touch the exact processes whose PID and generation
/// were captured before handoff. Core controls native services separately.
pub trait LegacyOwner {
    async fn inspect(&mut self) -> SetupResultValue<LegacyOwnerSnapshot>;
    async fn stop(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()>;
    async fn restore(&mut self, expected: &LegacyOwnerSnapshot) -> SetupResultValue<()>;
    async fn verify_imported_data(&mut self) -> SetupResultValue<()> {
        Ok(())
    }
    /// Only an adapter backed by an exact, privileged frozen-unit handoff may
    /// reserve a native service name already occupied by the old owner.
    fn replaces_native_service(&self, _component: Component) -> bool {
        false
    }
    /// Retrieve a saved credential transiently; never serialize it in a journal.
    async fn tunnel_credentials(
        &mut self,
        _profile_id: &str,
    ) -> SetupResultValue<TunnelCredentials> {
        Err(diagnostic(
            "legacy_tunnel_credentials",
            "The original Tunnel credential source is unavailable",
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyTunnelProfile {
    pub profile_id: String,
    pub start: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImport {
    pub server_env_file: Option<PathBuf>,
    pub runner_config_file: Option<PathBuf>,
    pub user_token_file: PathBuf,
    pub username: String,
    pub runner_client_id: Option<String>,
    pub projects: Vec<ProjectRecord>,
    #[serde(default)]
    pub tunnel_profiles: Vec<LegacyTunnelProfile>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationPhase {
    Prepared,
    OldStopRequested,
    OldStopped,
    NewStartRequested,
    NewVerified,
    Committed,
    Restoring,
    RecoveryRequired,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationJournal {
    pub schema_version: u16,
    pub operation_id: String,
    pub phase: MigrationPhase,
    pub request: SetupRequest,
    pub import: LegacyImport,
    pub captured: LegacyOwnerSnapshot,
    pub last_diagnostic: Option<SetupDiagnostic>,
}

fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(
        code,
        message,
        "Inspect both owners and resume the saved migration only after their state is known",
    )
}

fn journal(store: &EnvironmentStore) -> SetupResultValue<Option<MigrationJournal>> {
    store.read_json("migration.json")
}

/// Read the non-secret handoff intent before gathering live legacy inventory.
/// A Desktop restart must resume this exact captured owner, not recapture PIDs.
pub fn migration_journal(store: &EnvironmentStore) -> SetupResultValue<Option<MigrationJournal>> {
    journal(store)
}

fn save(store: &EnvironmentStore, value: &MigrationJournal) -> SetupResultValue<()> {
    store.write_json("migration.json", value)
}

fn verify_legacy(
    expected: &LegacyOwnerSnapshot,
    actual: &LegacyOwnerSnapshot,
) -> SetupResultValue<()> {
    if !expected.same_configuration(actual) {
        return Err(diagnostic(
            "legacy_owner_changed",
            "The original runtime configuration changed during migration",
        ));
    }
    for process in &actual.processes {
        if !expected.processes.contains(process) {
            return Err(diagnostic(
                "legacy_process_changed",
                "An unrecognized legacy process generation is active",
            ));
        }
    }
    Ok(())
}

fn stopped_legacy(
    expected: &LegacyOwnerSnapshot,
    actual: &LegacyOwnerSnapshot,
) -> SetupResultValue<bool> {
    verify_legacy(expected, actual)?;
    Ok(expected.stopped_against(actual))
}

/// The caller provides a host owner adapter; setup, imported identities,
/// system-service reconciliation and rollback ordering remain in this Core.
pub async fn migrate_legacy_environment(
    store: &EnvironmentStore,
    request: SetupRequest,
    import: LegacyImport,
    owner: &mut impl LegacyOwner,
    secrets: &SetupSecrets,
    mut progress: impl FnMut(SetupProgress),
) -> SetupResultValue<SetupResult> {
    let lock = store.lock()?;
    crate::ensure_upgrade_idle_under_lock(store)?;
    crate::native::validate_request_with_preserved_listen(
        &request,
        owner.replaces_native_service(Component::Server),
    )?;
    validate_import(&request, &import)?;
    let mut migration = match journal(store)? {
        Some(existing) => {
            if existing.schema_version != MIGRATION_SCHEMA
                || existing.request != request
                || existing.import != import
            {
                return Err(diagnostic(
                    "migration_conflict",
                    "A different legacy runtime handoff is already recorded",
                ));
            }
            existing
        }
        None => {
            if store.load_environment()?.is_some() || store.load_journal()?.is_some() {
                return Err(diagnostic(
                    "environment_conflict",
                    "The destination already has an environment",
                ));
            }
            let captured = owner.inspect().await?;
            if captured.owner_id.is_empty() || captured.configuration_fingerprint.is_empty() {
                return Err(diagnostic(
                    "legacy_owner_unknown",
                    "The legacy owner identity is incomplete",
                ));
            }
            let mut active_tunnels = captured
                .processes
                .iter()
                .filter_map(|process| process.kind.strip_prefix("tunnel:").map(str::to_owned))
                .collect::<Vec<_>>();
            active_tunnels.sort();
            let mut requested_tunnels = import
                .tunnel_profiles
                .iter()
                .filter(|profile| profile.start)
                .map(|profile| profile.profile_id.clone())
                .collect::<Vec<_>>();
            requested_tunnels.sort();
            if active_tunnels != requested_tunnels {
                return Err(diagnostic(
                    "legacy_tunnel_owner",
                    "Active legacy Tunnel processes do not match the import list",
                ));
            }
            let value = MigrationJournal {
                schema_version: MIGRATION_SCHEMA,
                operation_id: uuid::Uuid::new_v4().to_string(),
                phase: MigrationPhase::Prepared,
                request,
                import,
                captured,
                last_diagnostic: None,
            };
            save(store, &value)?;
            value
        }
    };

    if migration.phase == MigrationPhase::RecoveryRequired {
        return Err(migration.last_diagnostic.clone().unwrap_or_else(|| {
            diagnostic(
                "migration_recovery_required",
                "Migration needs an explicit owner recovery review",
            )
        }));
    }
    if migration.phase == MigrationPhase::Committed {
        return NativeEnvironment::new()?.status(store).await;
    }

    if migration.phase == MigrationPhase::Restoring {
        // A prior restore may already have restarted the old unit with a new
        // PID generation before the user-owned journal was updated. The
        // owner adapter reconciles its root-protected receipt first.
        restore_original(store, &mut migration, owner).await?;
        return Err(diagnostic(
            "migration_restored",
            "The original runtime was restored after an interrupted migration",
        ));
    }

    let observed = owner.inspect().await?;
    verify_legacy(&migration.captured, &observed)?;
    if migration.phase == MigrationPhase::Prepared {
        // Reject service ownership and account conflicts before touching the
        // original processes. On Windows this also reserves the disabled
        // Runner service and verifies the account credential before cutover.
        inspect_import(&migration.request, &migration.import)?;
        for profile in &migration.import.tunnel_profiles {
            let credentials = owner.tunnel_credentials(&profile.profile_id).await?;
            if !credentials
                .tunnel_id
                .expose()
                .strip_prefix("tunnel_")
                .is_some_and(|suffix| {
                    suffix.len() == 32
                        && suffix
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                })
                || credentials.api_key.expose().is_empty()
            {
                return Err(diagnostic(
                    "legacy_tunnel_credentials",
                    "Original Tunnel credentials are incomplete",
                ));
            }
        }
        preflight_new_owner(store, &migration, secrets, owner).await?;
        migration.phase = MigrationPhase::OldStopRequested;
        save(store, &migration)?;
    }
    if migration.phase == MigrationPhase::OldStopRequested {
        if !stopped_legacy(&migration.captured, &observed)? {
            if let Err(error) = owner.stop(&migration.captured).await {
                migration.phase = MigrationPhase::Restoring;
                migration.last_diagnostic = Some(error.clone());
                save(store, &migration)?;
                restore_original(store, &mut migration, owner).await?;
                return Err(error);
            }
        }
        let after = owner.inspect().await?;
        if !stopped_legacy(&migration.captured, &after)? {
            let error = diagnostic(
                "legacy_stop_unconfirmed",
                "The original process owner has not confirmed shutdown",
            );
            migration.phase = MigrationPhase::Restoring;
            migration.last_diagnostic = Some(error.clone());
            save(store, &migration)?;
            restore_original(store, &mut migration, owner).await?;
            return Err(error);
        }
        migration.phase = MigrationPhase::OldStopped;
        save(store, &migration)?;
    }

    if migration.phase == MigrationPhase::OldStopped {
        let after = owner.inspect().await?;
        if !stopped_legacy(&migration.captured, &after)? {
            return Err(diagnostic(
                "legacy_owner_active",
                "The original runtime restarted before the new owner was prepared",
            ));
        }
        let replacement_preflight: SetupResultValue<()> = (|| {
            for component in [Component::Server, Component::Runner] {
                if owner.replaces_native_service(component) {
                    let record = EnvironmentRecord {
                        schema_version: ENVIRONMENT_SCHEMA,
                        environment_id: migration.operation_id.clone(),
                        request: migration.request.clone(),
                        username: Some(migration.import.username.clone()),
                        runner_client_id: migration.import.runner_client_id.clone(),
                        projects: migration.import.projects.clone(),
                        configured: false,
                    };
                    ServiceManager::preflight(&service_spec(store, &record, component)?)
                        .map_err(service_diagnostic)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = replacement_preflight {
            migration.phase = MigrationPhase::Restoring;
            migration.last_diagnostic = Some(error.clone());
            save(store, &migration)?;
            restore_original(store, &mut migration, owner).await?;
            return Err(error);
        }
        if let Err(error) = import_original_files(store, &lock, &migration) {
            migration.phase = MigrationPhase::Restoring;
            migration.last_diagnostic = Some(error.clone());
            save(store, &migration)?;
            restore_original(store, &mut migration, owner).await?;
            return Err(error);
        }
        if let Err(error) = owner.verify_imported_data().await {
            migration.phase = MigrationPhase::Restoring;
            migration.last_diagnostic = Some(error.clone());
            save(store, &migration)?;
            restore_original(store, &mut migration, owner).await?;
            return Err(error);
        }
        migration.phase = MigrationPhase::NewStartRequested;
        save(store, &migration)?;
    }

    if migration.phase == MigrationPhase::NewStartRequested {
        let after = owner.inspect().await?;
        if !stopped_legacy(&migration.captured, &after)? {
            return Err(diagnostic(
                "legacy_owner_active",
                "The original runtime is active while the new owner is pending",
            ));
        }
        let mut backend = NativeEnvironment::new()?;
        if owner.replaces_native_service(Component::Server) {
            backend = backend.preserving_legacy_listen();
        }
        let mut setup = EnvironmentSetup::new(backend);
        let result = setup
            .configure_under_lock(
                store,
                &lock,
                migration.request.clone(),
                secrets,
                &mut progress,
            )
            .await;
        match result {
            Ok(result) => {
                migration.phase = MigrationPhase::NewVerified;
                save(store, &migration)?;
                if let Err(error) = finish_tunnels(store, &lock, &migration, owner).await {
                    migration.phase = MigrationPhase::Restoring;
                    migration.last_diagnostic = Some(error.clone());
                    save(store, &migration)?;
                    restore_original(store, &mut migration, owner).await?;
                    return Err(error);
                }
                migration.phase = MigrationPhase::Committed;
                save(store, &migration)?;
                return Ok(result);
            }
            Err(error) => {
                migration.phase = MigrationPhase::Restoring;
                migration.last_diagnostic = Some(error.clone());
                save(store, &migration)?;
                restore_original(store, &mut migration, owner).await?;
                return Err(error);
            }
        }
    }

    if migration.phase == MigrationPhase::NewVerified {
        let result = NativeEnvironment::new()?.status(store).await?;
        if !result.observation.authenticated || result.observation.runner_online == Some(false) {
            return Err(diagnostic(
                "new_owner_unverified",
                "The new runtime no longer passes authenticated readiness",
            ));
        }
        if let Err(error) = finish_tunnels(store, &lock, &migration, owner).await {
            migration.phase = MigrationPhase::Restoring;
            migration.last_diagnostic = Some(error.clone());
            save(store, &migration)?;
            restore_original(store, &mut migration, owner).await?;
            return Err(error);
        }
        migration.phase = MigrationPhase::Committed;
        save(store, &migration)?;
        return Ok(result);
    }
    Err(diagnostic(
        "migration_state",
        "Migration has an unrecognized recovery state",
    ))
}

async fn finish_tunnels(
    store: &EnvironmentStore,
    lock: &EnvironmentLock,
    migration: &MigrationJournal,
    owner: &mut impl LegacyOwner,
) -> SetupResultValue<()> {
    if migration.import.tunnel_profiles.is_empty() {
        return Ok(());
    }
    let native = NativeEnvironment::new()?;
    for profile in &migration.import.tunnel_profiles {
        let credentials = owner.tunnel_credentials(&profile.profile_id).await?;
        let status = native
            .configure_tunnel_under_lock(
                store,
                lock,
                &profile.profile_id,
                Some(&credentials),
                profile.start,
            )
            .await?;
        if profile.start && (status.ownership != Ownership::Owned || status.running != Some(true)) {
            return Err(diagnostic(
                "tunnel_start_unconfirmed",
                "The new Tunnel service did not confirm startup",
            ));
        }
    }
    Ok(())
}

async fn preflight_new_owner(
    store: &EnvironmentStore,
    migration: &MigrationJournal,
    secrets: &SetupSecrets,
    owner: &impl LegacyOwner,
) -> SetupResultValue<()> {
    let record = EnvironmentRecord {
        schema_version: ENVIRONMENT_SCHEMA,
        environment_id: migration.operation_id.clone(),
        request: migration.request.clone(),
        username: Some(migration.import.username.clone()),
        runner_client_id: migration.import.runner_client_id.clone(),
        projects: migration.import.projects.clone(),
        configured: false,
    };
    if crate::current_account()?.identity != record.request.account.identity {
        return Err(diagnostic(
            "project_user_required",
            "Migration must run as the actual project user",
        ));
    }
    for component in [Component::Server, Component::Runner] {
        if (component == Component::Server && !record.request.local_server())
            || (component == Component::Runner && !record.request.local_runner())
        {
            continue;
        }
        let spec = service_spec(store, &record, component)?;
        if !owner.replaces_native_service(component) {
            ServiceManager::preflight(&spec).map_err(service_diagnostic)?;
        } else {
            #[cfg(target_os = "linux")]
            ServiceManager::preflight_replacing(&spec).map_err(service_diagnostic)?;
        }
    }
    for profile in &migration.import.tunnel_profiles {
        let spec = tunnel_service_spec(store, &record, &profile.profile_id)?;
        ServiceManager::preflight(&spec).map_err(service_diagnostic)?;
    }
    #[cfg(windows)]
    if record.request.local_runner() {
        let credential = secrets
            .service_password
            .as_ref()
            .map(|secret| ServiceCredential::from_password(secret.expose()));
        crate::privilege::service_operation(
            store,
            &record,
            Component::Runner,
            ServiceOperation::PrepareRunner,
            credential.as_ref(),
        )
        .await?;
    }
    #[cfg(not(windows))]
    let _ = secrets;
    Ok(())
}

async fn restore_original(
    store: &EnvironmentStore,
    migration: &mut MigrationJournal,
    owner: &mut impl LegacyOwner,
) -> SetupResultValue<()> {
    // Never revive the original process while a native service may still own
    // the same port, project or identity.
    let record = store
        .load_journal()?
        .map(|j| j.environment)
        .or(store.load_environment()?)
        .unwrap_or_else(|| EnvironmentRecord {
            schema_version: ENVIRONMENT_SCHEMA,
            environment_id: migration.operation_id.clone(),
            request: migration.request.clone(),
            username: Some(migration.import.username.clone()),
            runner_client_id: migration.import.runner_client_id.clone(),
            projects: migration.import.projects.clone(),
            configured: false,
        });
    for profile in migration.import.tunnel_profiles.clone() {
        let spec = tunnel_service_spec(store, &record, &profile.profile_id)?;
        stop_and_remove_new_service(store, migration, &record, spec, Component::Tunnel).await?;
    }
    for component in [Component::Runner, Component::Server] {
        if (component == Component::Runner && !record.request.local_runner())
            || (component == Component::Server && !record.request.local_server())
        {
            continue;
        }
        if owner.replaces_native_service(component) {
            // Before the root-protected old unit pair has released its names,
            // native inspect legitimately sees the old foreign unit. No new
            // Core service could have been installed at that name yet.
            let old = owner.inspect().await?;
            if !old.processes.is_empty() {
                verify_legacy(&migration.captured, &old)?;
                continue;
            }
        }
        let spec = service_spec(store, &record, component)?;
        stop_and_remove_new_service(store, migration, &record, spec, component).await?;
    }
    owner.restore(&migration.captured).await?;
    let restored = owner.inspect().await?;
    if !migration.captured.same_configuration(&restored)
        || migration.captured.processes.iter().any(|process| {
            !restored
                .processes
                .iter()
                .any(|current| current.kind == process.kind)
        })
    {
        migration.phase = MigrationPhase::RecoveryRequired;
        save(store, migration)?;
        return Err(diagnostic(
            "legacy_restore_unconfirmed",
            "The original runtime did not become available",
        ));
    }
    migration.phase = MigrationPhase::RecoveryRequired;
    save(store, migration)?;
    Ok(())
}

async fn stop_and_remove_new_service(
    store: &EnvironmentStore,
    migration: &mut MigrationJournal,
    record: &EnvironmentRecord,
    spec: crate::service::ServiceSpec,
    component: Component,
) -> SetupResultValue<()> {
    let status = ServiceManager::inspect(&spec).map_err(service_diagnostic)?;
    match status.ownership {
        Ownership::Absent => return Ok(()),
        Ownership::Owned => {}
        _ => {
            migration.phase = MigrationPhase::RecoveryRequired;
            save(store, migration)?;
            return Err(diagnostic(
                "new_owner_unknown",
                "The native service owner could not be verified for rollback",
            ));
        }
    }
    if status.running == Some(true) {
        if component == Component::Tunnel {
            crate::privilege::service_operation_spec(
                store,
                record,
                spec.clone(),
                ServiceOperation::Stop,
                None,
            )
            .await?;
        } else {
            crate::privilege::service_operation(
                store,
                record,
                component,
                ServiceOperation::Stop,
                None,
            )
            .await?;
        }
    }
    let stopped = ServiceManager::inspect(&spec).map_err(service_diagnostic)?;
    if stopped.ownership != Ownership::Owned || stopped.running != Some(false) {
        migration.phase = MigrationPhase::RecoveryRequired;
        save(store, migration)?;
        return Err(diagnostic(
            "new_stop_unconfirmed",
            "The native service did not confirm shutdown",
        ));
    }
    if component == Component::Tunnel {
        crate::privilege::service_operation_spec(
            store,
            record,
            spec.clone(),
            ServiceOperation::Uninstall,
            None,
        )
        .await?;
    } else {
        crate::privilege::service_operation(
            store,
            record,
            component,
            ServiceOperation::Uninstall,
            None,
        )
        .await?;
    }
    let removed = ServiceManager::inspect(&spec).map_err(service_diagnostic)?;
    if removed.ownership != Ownership::Absent {
        migration.phase = MigrationPhase::RecoveryRequired;
        save(store, migration)?;
        return Err(diagnostic(
            "new_remove_unconfirmed",
            "The native service may still start after reboot",
        ));
    }
    Ok(())
}

fn service_diagnostic(error: crate::service::ServiceError) -> SetupDiagnostic {
    SetupDiagnostic::new("service_inspection", &error.message, error.recovery_hint)
}

fn validate_import(request: &SetupRequest, import: &LegacyImport) -> SetupResultValue<()> {
    if import.username.trim().is_empty() || !import.user_token_file.is_absolute() {
        return Err(diagnostic(
            "legacy_identity",
            "The original user credential identity is incomplete",
        ));
    }
    if request.local_server() != import.server_env_file.is_some()
        || request.local_runner() != import.runner_config_file.is_some()
        || request.local_runner() != import.runner_client_id.is_some()
    {
        return Err(diagnostic(
            "legacy_component_conflict",
            "Original component files do not match the requested ownership",
        ));
    }
    if request.local_runner() && import.projects.is_empty() {
        return Err(diagnostic(
            "legacy_project_required",
            "The original Runner has no confirmed project registration",
        ));
    }
    if import.projects.iter().any(|project| {
        !project.path.is_absolute()
            || project.id.is_empty()
            || project.id.len() > 64
            || !project
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    }) {
        return Err(diagnostic(
            "legacy_project_identity",
            "The original project registration is incomplete",
        ));
    }
    if request.local_runner()
        && !import
            .projects
            .iter()
            .any(|project| Some(&project.path) == request.project.as_ref())
    {
        return Err(diagnostic(
            "legacy_project_identity",
            "The selected project is not registered under the original Runner",
        ));
    }
    if !request.local_server() && !import.tunnel_profiles.is_empty() {
        return Err(diagnostic(
            "legacy_tunnel_owner",
            "A Runner-only machine cannot import Server Tunnel profiles",
        ));
    }
    if import.tunnel_profiles.iter().any(|profile| {
        profile.profile_id.is_empty()
            || profile.profile_id.len() > 64
            || !profile
                .profile_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    }) || import
        .tunnel_profiles
        .iter()
        .enumerate()
        .any(|(index, profile)| {
            import.tunnel_profiles[..index]
                .iter()
                .any(|previous| previous.profile_id == profile.profile_id)
        })
    {
        return Err(diagnostic(
            "legacy_tunnel_profile",
            "Original Tunnel profile identifiers are invalid or duplicated",
        ));
    }
    Ok(())
}

fn inspect_import(request: &SetupRequest, import: &LegacyImport) -> SetupResultValue<()> {
    let user_token = read_private(&import.user_token_file)?;
    if user_token.is_empty() {
        return Err(diagnostic(
            "legacy_user_credential",
            "The original user credential is empty",
        ));
    }
    if let Some(path) = &import.server_env_file {
        let env = String::from_utf8(read_private(path)?).map_err(|_| {
            diagnostic(
                "legacy_server_configuration",
                "The original Server environment is not UTF-8",
            )
        })?;
        let listen = match &request.mode {
            EnvironmentMode::Create { listen } => listen,
            _ => {
                return Err(diagnostic(
                    "legacy_server_mode",
                    "A remote environment cannot import a local Server",
                ))
            }
        };
        if exactly_one_env(&env, "WEBCODEX_ADDR")?.as_deref() != Some(listen)
            || exactly_one_env(&env, "WEBCODEX_TOKEN")?.is_none()
        {
            return Err(diagnostic(
                "legacy_server_binding",
                "The original Server address or bootstrap identity differs",
            ));
        }
        let data = exactly_one_env(&env, "WEBCODEX_DATA")?.ok_or_else(|| {
            diagnostic(
                "legacy_data_path",
                "The original Server data path is missing",
            )
        })?;
        if !Path::new(&data).is_absolute() || !Path::new(&data).is_dir() {
            return Err(diagnostic(
                "legacy_data_path",
                "The original Server data directory is unavailable",
            ));
        }
    }
    if let Some(path) = &import.runner_config_file {
        let content = String::from_utf8(read_private(path)?).map_err(|_| {
            diagnostic(
                "legacy_runner_configuration",
                "The original Runner configuration is not UTF-8",
            )
        })?;
        let config: toml::Value = toml::from_str(&content).map_err(|_| {
            diagnostic(
                "legacy_runner_configuration",
                "The original Runner configuration is invalid",
            )
        })?;
        if config.get("server_url").and_then(toml::Value::as_str)
            != Some(request.server_url.as_str())
            || config.get("client_id").and_then(toml::Value::as_str)
                != import.runner_client_id.as_deref()
            || config.get("owner").and_then(toml::Value::as_str) != Some(import.username.as_str())
            || config
                .get("token")
                .and_then(toml::Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Err(diagnostic(
                "legacy_runner_binding",
                "The original Runner identity differs from the requested environment",
            ));
        }
        if let Some(registry) = config
            .get("project_registry_dir")
            .and_then(toml::Value::as_str)
        {
            let registry = Path::new(registry);
            if !registry.is_absolute() {
                return Err(diagnostic(
                    "legacy_project_registry",
                    "The original project registry path is not absolute",
                ));
            }
            for project in &import.projects {
                let source = registry.join(format!("{}.toml", project.id));
                let bytes = read_private(&source)?;
                let value: toml::Value =
                    toml::from_str(std::str::from_utf8(&bytes).map_err(|_| {
                        diagnostic(
                            "legacy_project_registry",
                            "The original project registry is invalid",
                        )
                    })?)
                    .map_err(|_| {
                        diagnostic(
                            "legacy_project_registry",
                            "The original project registry is invalid",
                        )
                    })?;
                if value.get("id").and_then(toml::Value::as_str) != Some(project.id.as_str())
                    || value.get("path").and_then(toml::Value::as_str) != project.path.to_str()
                {
                    return Err(diagnostic(
                        "legacy_project_registry",
                        "The original project registry does not match the saved project identity",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn exactly_one_env(content: &str, key: &str) -> SetupResultValue<Option<String>> {
    let mut found = None;
    for line in content.lines() {
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() == key {
            if found.is_some() || value.trim().is_empty() {
                return Err(diagnostic(
                    "legacy_env_ambiguous",
                    "The original Server environment has duplicate or empty required fields",
                ));
            }
            found = Some(value.trim().to_owned());
        }
    }
    Ok(found)
}

fn import_original_files(
    store: &EnvironmentStore,
    _lock: &EnvironmentLock,
    migration: &MigrationJournal,
) -> SetupResultValue<()> {
    inspect_import(&migration.request, &migration.import)?;
    let root = store.root();
    let token = read_private(&migration.import.user_token_file)?;
    write_same_or_new(&root.join("webcodex-user-token"), &token)?;
    if let Some(path) = &migration.import.server_env_file {
        let old_env = String::from_utf8(read_private(path)?).map_err(|_| {
            diagnostic(
                "legacy_server_configuration",
                "The original Server environment is not UTF-8",
            )
        })?;
        let old_data = exactly_one_env(&old_env, "WEBCODEX_DATA")?.ok_or_else(|| {
            diagnostic(
                "legacy_data_path",
                "The original Server data directory is missing",
            )
        })?;
        let server = root.join("server");
        ensure_private_directory(&server)?;
        let new_data = server.join("data");
        if !new_data.exists() {
            copy_data_tree(Path::new(&old_data), &new_data)?;
        }
        let env = old_env
            .lines()
            .map(|line| {
                if line
                    .split_once('=')
                    .is_some_and(|(key, _)| key.trim() == "WEBCODEX_DATA")
                {
                    format!("WEBCODEX_DATA={}", new_data.display())
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        write_same_or_new(&server.join("webcodex.env"), env.as_bytes())?;
    }
    if let Some(path) = &migration.import.runner_config_file {
        let original = String::from_utf8(read_private(path)?).map_err(|_| {
            diagnostic(
                "legacy_runner_configuration",
                "The original Runner configuration is not UTF-8",
            )
        })?;
        let mut config: toml::Value = toml::from_str(&original).map_err(|_| {
            diagnostic(
                "legacy_runner_configuration",
                "The original Runner configuration is invalid",
            )
        })?;
        let source_registry = config
            .get("project_registry_dir")
            .and_then(toml::Value::as_str)
            .map(PathBuf::from);
        let table = config.as_table_mut().ok_or_else(|| {
            diagnostic(
                "legacy_runner_configuration",
                "The original Runner configuration is not a table",
            )
        })?;
        table.insert(
            "project_registry_dir".into(),
            root.join("project-registry")
                .to_string_lossy()
                .to_string()
                .into(),
        );
        let new_config = toml::to_string(&config).map_err(|_| SetupDiagnostic::io())?;
        write_same_or_new(&root.join("runner.toml"), new_config.as_bytes())?;
        for project in &migration.import.projects {
            let registry = root
                .join("project-registry")
                .join(format!("{}.toml", project.id));
            if let Some(source) = &source_registry {
                let source = source.join(format!("{}.toml", project.id));
                let bytes = read_private(&source)?;
                write_same_or_new(&registry, &bytes)?;
                continue;
            }
            let mut value = toml::Table::new();
            value.insert("id".into(), project.id.clone().into());
            value.insert(
                "path".into(),
                project.path.to_string_lossy().to_string().into(),
            );
            value.insert("allow_patch".into(), true.into());
            value.insert("disabled".into(), false.into());
            write_same_or_new(
                &registry,
                toml::to_string(&value)
                    .map_err(|_| SetupDiagnostic::io())?
                    .as_bytes(),
            )?;
        }
    }
    match store.load_journal()? {
        Some(saved) if saved.environment.request != migration.request => {
            return Err(diagnostic(
                "setup_conflict",
                "The new environment journal has another identity",
            ));
        }
        Some(_) => {}
        None => {
            let record = EnvironmentRecord {
                schema_version: ENVIRONMENT_SCHEMA,
                environment_id: migration.operation_id.clone(),
                request: migration.request.clone(),
                username: Some(migration.import.username.clone()),
                runner_client_id: migration.import.runner_client_id.clone(),
                projects: migration.import.projects.clone(),
                configured: false,
            };
            store.save_journal(&SetupJournal {
                schema_version: ENVIRONMENT_SCHEMA,
                operation_id: migration.operation_id.clone(),
                environment: record,
                steps: BTreeMap::new(),
                last_diagnostic: None,
            })?;
        }
    }
    Ok(())
}

fn write_same_or_new(path: &Path, content: &[u8]) -> SetupResultValue<()> {
    if path.exists() {
        if read_private(path)? == content {
            return Ok(());
        }
        return Err(diagnostic(
            "migration_file_conflict",
            "An imported configuration file already has different contents",
        ));
    }
    atomic_private_write(path, content)
}

fn copy_data_tree(source: &Path, target: &Path) -> SetupResultValue<()> {
    if fs::symlink_metadata(source)
        .map_err(|_| SetupDiagnostic::io())?
        .file_type()
        .is_symlink()
    {
        return Err(diagnostic(
            "migration_data_link",
            "The Server data directory is a symbolic link",
        ));
    }
    let source = source.canonicalize().map_err(|_| SetupDiagnostic::io())?;
    if !source.is_dir() || target.exists() {
        return Err(diagnostic(
            "migration_data_conflict",
            "The Server data destination already exists or the source is unavailable",
        ));
    }
    let stage = target.with_extension(format!("migration-{}", uuid::Uuid::new_v4().simple()));
    ensure_private_directory(&stage)?;
    let mut pending = vec![(source, stage.clone(), 0usize)];
    let mut count = 0usize;
    while let Some((from, to, depth)) = pending.pop() {
        if depth > 32 {
            return Err(diagnostic(
                "migration_data_depth",
                "The Server data tree exceeds the migration depth limit",
            ));
        }
        for entry in fs::read_dir(from).map_err(|_| SetupDiagnostic::io())? {
            let entry = entry.map_err(|_| SetupDiagnostic::io())?;
            count += 1;
            if count > MAX_COPY_ENTRIES {
                return Err(diagnostic(
                    "migration_data_limit",
                    "The Server data tree exceeds the migration entry limit",
                ));
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(|_| SetupDiagnostic::io())?;
            if metadata.file_type().is_symlink() {
                return Err(diagnostic(
                    "migration_data_link",
                    "The Server data tree contains a symbolic link",
                ));
            }
            let next = to.join(entry.file_name());
            if metadata.is_dir() {
                ensure_private_directory(&next)?;
                pending.push((entry.path(), next, depth + 1));
            } else if metadata.is_file() {
                fs::copy(entry.path(), &next).map_err(|_| SetupDiagnostic::io())?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&next, fs::Permissions::from_mode(0o600))
                        .map_err(|_| SetupDiagnostic::io())?;
                }
            } else {
                return Err(diagnostic(
                    "migration_data_type",
                    "The Server data tree contains an unsupported file",
                ));
            }
        }
    }
    fs::rename(stage, target).map_err(|_| SetupDiagnostic::io())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_changed_owner_generation_and_unknown_process() {
        let initial = LegacyOwnerSnapshot {
            owner_id: "desktop".into(),
            configuration_fingerprint: "config".into(),
            processes: vec![LegacyProcess {
                kind: "server".into(),
                pid: 7,
                generation: 2,
            }],
        };
        let changed = LegacyOwnerSnapshot {
            processes: vec![LegacyProcess {
                kind: "server".into(),
                pid: 8,
                generation: 2,
            }],
            ..initial.clone()
        };
        assert_eq!(
            verify_legacy(&initial, &changed).unwrap_err().code,
            "legacy_process_changed"
        );
        let stopped = LegacyOwnerSnapshot {
            processes: Vec::new(),
            ..initial.clone()
        };
        assert!(stopped_legacy(&initial, &stopped).unwrap());
    }
    #[test]
    fn detects_ambiguous_server_environment() {
        assert_eq!(
            exactly_one_env(
                "WEBCODEX_ADDR=127.0.0.1:1\nWEBCODEX_ADDR=127.0.0.1:2\n",
                "WEBCODEX_ADDR"
            )
            .unwrap_err()
            .code,
            "legacy_env_ambiguous"
        );
    }

    #[test]
    fn imports_existing_runner_identity_but_redirects_registry_to_core() {
        let temp = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        let project = temp.path().join("project");
        fs::create_dir(&project).unwrap();
        let token_file = temp.path().join("legacy-user-token");
        let config_file = temp.path().join("legacy-runner.toml");
        let source_registry = temp.path().join("old-registry");
        fs::create_dir(&source_registry).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&source_registry, fs::Permissions::from_mode(0o700)).unwrap();
        }
        atomic_private_write(
            &source_registry.join("existing-project.toml"),
            format!(
                "id = 'existing-project'\npath = '{}'\nallow_patch = false\n",
                project.display()
            )
            .as_bytes(),
        )
        .unwrap();
        atomic_private_write(&token_file, b"wc_user_existing").unwrap();
        atomic_private_write(&config_file,
            format!("server_url = 'https://server.example'\nclient_id = 'existing-runner'\nowner = 'alice'\ntoken = 'wc_agent_existing'\nproject_registry_dir = '{}'\n", source_registry.display()).as_bytes()).unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let request = SetupRequest {
            mode: EnvironmentMode::Join,
            server_url: "https://server.example".into(),
            project: Some(project.clone()),
            account: crate::LocalAccount {
                name: "alice".into(),
                identity: "1001".into(),
                home: temp.path().to_path_buf(),
            },
            binaries: crate::RuntimeBinaries {
                cli: "/bin/webcodex".into(),
                server: "/bin/webcodex-server".into(),
                runner: "/bin/webcodex-runner".into(),
            },
        };
        let migration = MigrationJournal {
            schema_version: MIGRATION_SCHEMA,
            operation_id: "migration-test".into(),
            phase: MigrationPhase::OldStopped,
            request,
            import: LegacyImport {
                server_env_file: None,
                runner_config_file: Some(config_file),
                user_token_file: token_file,
                username: "alice".into(),
                runner_client_id: Some("existing-runner".into()),
                projects: vec![ProjectRecord {
                    id: "existing-project".into(),
                    path: project.clone(),
                }],
                tunnel_profiles: vec![],
            },
            captured: LegacyOwnerSnapshot {
                owner_id: "desktop".into(),
                configuration_fingerprint: "binding".into(),
                processes: vec![],
            },
            last_diagnostic: None,
        };
        let lock = store.lock().unwrap();
        import_original_files(&store, &lock, &migration).unwrap();
        let imported: toml::Value =
            toml::from_str(&fs::read_to_string(store.root().join("runner.toml")).unwrap()).unwrap();
        assert_eq!(imported["token"].as_str(), Some("wc_agent_existing"));
        assert_eq!(imported["client_id"].as_str(), Some("existing-runner"));
        assert_eq!(
            imported["project_registry_dir"].as_str(),
            store.root().join("project-registry").to_str()
        );
        assert_eq!(
            fs::read_to_string(store.root().join("webcodex-user-token")).unwrap(),
            "wc_user_existing"
        );
        let registry: toml::Value = toml::from_str(
            &fs::read_to_string(store.root().join("project-registry/existing-project.toml"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(registry["path"].as_str(), project.to_str());
        assert_eq!(registry["allow_patch"].as_bool(), Some(false));
        assert_eq!(
            store.load_journal().unwrap().unwrap().environment.projects[0].id,
            "existing-project"
        );
    }
}
