//! Durable handoff from a known legacy process owner to its selected native service manager.
//! The journal contains paths and process identities, never credential contents.
use crate::native::{canonical_server_url, same_project_path};
#[cfg(windows)]
use crate::service::ServiceCredential;
use crate::service::{Component, Ownership, ServiceManager};
use crate::storage::{
    atomic_private_write, ensure_private_directory, is_link, read_legacy_source, read_private,
    EnvironmentLock,
};
use crate::tunnel::{
    import_embedded_tunnel_profile_under_lock, tunnel_profiles, tunnel_service_spec,
    TunnelCredentials, TunnelHostMode, TunnelProxyUpdate,
};
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
    /// Read-only interruption check after service preflight. A rejection leaves
    /// the captured owner running and the same journal available for recovery.
    async fn check_before_stop(&mut self, _observed: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        Ok(())
    }
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
    /// Effective HTTP(S) proxy URL for OpenAI Tunnel control-plane traffic, if configured.
    fn tunnel_proxy(&self) -> Option<crate::Secret> {
        None
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyTunnelProfile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub profile_id: String,
    pub start: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_mode: Option<TunnelHostMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autostart: Option<bool>,
}

impl LegacyTunnelProfile {
    pub fn effective_host_mode(&self) -> TunnelHostMode {
        self.host_mode.unwrap_or(TunnelHostMode::Standalone)
    }

    pub fn effective_autostart(&self) -> bool {
        self.autostart.unwrap_or(self.start)
    }
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

fn recovery_hint_for(code: &str) -> &'static str {
    match code {
        "legacy_project_registry" | "legacy_project_identity" => {
            "Check that each saved project folder still exists and matches its Runner registration, then check again."
        }
        "legacy_server_binding"
        | "legacy_server_configuration"
        | "legacy_server_mode"
        | "legacy_data_path"
        | "legacy_env_ambiguous" => {
            "Check that the original local Server configuration and data directory are readable, then check again."
        }
        "legacy_runner_binding"
        | "legacy_runner_configuration"
        | "legacy_identity"
        | "legacy_user_credential"
        | "legacy_component_conflict" => {
            "Check that the original Runner configuration and user credential match this Server, then check again."
        }
        "legacy_tunnel_credentials" | "legacy_tunnel_profile" => {
            "Check the saved ChatGPT Tunnel ID and API key in Connections, then check again."
        }
        "legacy_tunnel_owner" => {
            "Check the running Tunnel connections in Connections, then check again."
        }
        "legacy_process_changed" | "legacy_owner_changed" => {
            "The running Desktop processes or settings changed since the unfinished migration was recorded. Clear the unfinished record if no background services were installed, then check again."
        }
        "migration_conflict" => {
            "Another unfinished migration record exists. Review Diagnostics or clear the unfinished record if no background services were installed."
        }
        "project_user_required" => {
            "Run migration as the same local user account that owns the projects."
        }
        "tunnel_host_unsupported" => {
            "Embedded Tunnel hosting requires user-session services on Windows; use user startup or standalone Tunnel services."
        }
        _ => "Inspect both owners and resume the saved migration only after their state is known",
    }
}

fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, recovery_hint_for(code))
        .with_component("migration")
        .with_stage("migration_handoff")
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

impl MigrationJournal {
    pub fn verify_current_owner(&self, observed: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
        verify_legacy(&self.captured, observed)
    }
}

fn stopped_legacy(
    expected: &LegacyOwnerSnapshot,
    actual: &LegacyOwnerSnapshot,
) -> SetupResultValue<bool> {
    verify_legacy(expected, actual)?;
    Ok(expected.stopped_against(actual))
}

/// Read-only validation of legacy files and bindings without mutating state or
/// creating `migration.json`.
pub fn inspect_legacy_import(
    request: &SetupRequest,
    import: &LegacyImport,
) -> SetupResultValue<()> {
    validate_import(request, import)?;
    inspect_import(request, import)
}

/// Full read-only preflight before writing `migration.json` or stopping any
/// legacy process. Never creates a journal or installs services.
pub fn preflight_legacy_import(
    store: &EnvironmentStore,
    request: &SetupRequest,
    import: &LegacyImport,
    replaces_server: bool,
    replaces_runner: bool,
) -> SetupResultValue<()> {
    crate::native::validate_request_with_preserved_listen(request, replaces_server)?;
    validate_import(request, import)?;
    inspect_import(request, import)?;
    preflight_new_owner_readonly(
        store,
        request,
        import,
        "preflight",
        replaces_server,
        replaces_runner,
    )
}

fn check_resettable_migration(
    store: &EnvironmentStore,
    journal: &MigrationJournal,
) -> SetupResultValue<bool> {
    if !matches!(
        journal.phase,
        MigrationPhase::Prepared | MigrationPhase::RecoveryRequired
    ) {
        return Ok(false);
    }
    if store.load_environment()?.is_some() {
        return Ok(false);
    }
    if journal.phase == MigrationPhase::Prepared && store.load_journal()?.is_some() {
        return Ok(false);
    }
    let record = store
        .load_journal()?
        .map(|saved| saved.environment)
        .unwrap_or_else(|| EnvironmentRecord {
            schema_version: ENVIRONMENT_SCHEMA,
            environment_id: journal.operation_id.clone(),
            request: journal.request.clone(),
            username: Some(journal.import.username.clone()),
            runner_client_id: journal.import.runner_client_id.clone(),
            projects: journal.import.projects.clone(),
            configured: false,
        });
    for component in [Component::Server, Component::Runner] {
        if (component == Component::Server && !record.request.local_server())
            || (component == Component::Runner && !record.request.local_runner())
        {
            continue;
        }
        let spec = service_spec(store, &record, component)?;
        if ServiceManager::inspect(&spec)
            .map_err(service_diagnostic)?
            .ownership
            != Ownership::Absent
        {
            return Ok(false);
        }
    }
    let mut tunnel_ids = journal
        .import
        .tunnel_profiles
        .iter()
        .map(|profile| profile.profile_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    if let Ok(saved_tunnels) = tunnel_profiles(store) {
        for profile in saved_tunnels {
            tunnel_ids.insert(profile.profile_id);
        }
    }
    for profile_id in tunnel_ids {
        let spec = tunnel_service_spec(store, &record, &profile_id)?;
        if ServiceManager::inspect(&spec)
            .map_err(service_diagnostic)?
            .ownership
            != Ownership::Absent
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Returns true only when a saved migration journal is in `Prepared` or
/// `RecoveryRequired`, no persistent environment has been committed, and every
/// target native service is verified `Ownership::Absent`.
pub fn can_reset_migration(store: &EnvironmentStore) -> SetupResultValue<bool> {
    let Some(journal) = journal(store)? else {
        return Ok(false);
    };
    check_resettable_migration(store, &journal)
}

fn remove_staged_path(path: &Path) -> SetupResultValue<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SetupDiagnostic::io()),
        Ok(metadata) if is_link(&metadata) => Err(diagnostic(
            "migration_recovery_required",
            "Refusing to remove a symbolic link inside the environment store",
        )),
        Ok(metadata) if metadata.is_file() => {
            fs::remove_file(path).map_err(|_| SetupDiagnostic::io())
        }
        Ok(metadata) if metadata.is_dir() => remove_staged_dir(path, 0),
        Ok(_) => Err(SetupDiagnostic::io()),
    }
}

fn remove_staged_dir(dir: &Path, depth: usize) -> SetupResultValue<()> {
    if depth > 32 {
        return Err(SetupDiagnostic::io());
    }
    for entry in fs::read_dir(dir).map_err(|_| SetupDiagnostic::io())? {
        let entry = entry.map_err(|_| SetupDiagnostic::io())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| SetupDiagnostic::io())?;
        if is_link(&metadata) {
            return Err(diagnostic(
                "migration_recovery_required",
                "Refusing to remove a symbolic link inside the environment store",
            ));
        }
        if metadata.is_dir() {
            remove_staged_dir(&path, depth + 1)?;
        } else {
            fs::remove_file(&path).map_err(|_| SetupDiagnostic::io())?;
        }
    }
    fs::remove_dir(dir).map_err(|_| SetupDiagnostic::io())
}

/// Explicitly discard a verified-clean unstarted (`Prepared`) or fully rolled-back
/// (`RecoveryRequired`) migration journal and any uncommitted staged copies inside
/// the destination EnvironmentStore. Fails closed whenever a committed environment
/// or any target native service is present or unverifiable.
pub fn reset_failed_migration(store: &EnvironmentStore) -> SetupResultValue<()> {
    let _lock = store.lock()?;
    crate::ensure_upgrade_idle_under_lock(store)?;
    let Some(existing) = journal(store)? else {
        return Ok(());
    };
    if !check_resettable_migration(store, &existing)? {
        return Err(diagnostic(
            "migration_recovery_required",
            "The unfinished migration cannot be cleared while a persistent environment or native service remains",
        ));
    }
    if existing.phase == MigrationPhase::RecoveryRequired {
        for relative in [
            "setup.json",
            "tunnel.json",
            "runner.toml",
            "webcodex-user-token",
            "enrollment-recovery.json",
            "project-registry",
            "server",
            "cu",
        ] {
            remove_staged_path(&store.root().join(relative))?;
        }
    }
    remove_staged_path(&store.root().join("migration.json"))
}

async fn validate_legacy_tunnel_credentials(
    import: &LegacyImport,
    owner: &mut impl LegacyOwner,
) -> SetupResultValue<()> {
    for profile in &import.tunnel_profiles {
        let credentials = owner.tunnel_credentials(&profile.profile_id).await?;
        crate::tunnel::validate_credentials(&credentials).map_err(|_| {
            diagnostic(
                "legacy_tunnel_credentials",
                "Original Tunnel credentials are incomplete or invalid",
            )
        })?;
    }
    Ok(())
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
            // Complete all read-only file, credential, and service preflight checks
            // BEFORE persisting migration.json so a validation failure never leaves
            // a stale Prepared journal behind.
            inspect_import(&request, &import)?;
            validate_legacy_tunnel_credentials(&import, owner).await?;
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
            let operation_id = uuid::Uuid::new_v4().to_string();
            preflight_new_owner_readonly(
                store,
                &request,
                &import,
                &operation_id,
                owner.replaces_native_service(Component::Server),
                owner.replaces_native_service(Component::Runner),
            )?;
            let value = MigrationJournal {
                schema_version: MIGRATION_SCHEMA,
                operation_id,
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
    migration.verify_current_owner(&observed)?;
    if migration.phase == MigrationPhase::Prepared {
        // Re-verify read-only checks for resumed Prepared journals and, on Windows
        // system scope, reserve the disabled Runner service before cutover.
        inspect_import(&migration.request, &migration.import)?;
        validate_legacy_tunnel_credentials(&migration.import, owner).await?;
        preflight_new_owner(store, &migration, secrets, owner).await?;
        migration.phase = MigrationPhase::OldStopRequested;
        save(store, &migration)?;
    }
    if migration.phase == MigrationPhase::OldStopRequested {
        if !stopped_legacy(&migration.captured, &observed)? {
            owner.check_before_stop(&observed).await?;
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
        if let Err(error) = import_embedded_tunnels(store, &lock, &migration, owner).await {
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

async fn import_embedded_tunnels(
    store: &EnvironmentStore,
    lock: &EnvironmentLock,
    migration: &MigrationJournal,
    owner: &mut impl LegacyOwner,
) -> SetupResultValue<()> {
    let proxy_update = match owner.tunnel_proxy() {
        Some(proxy) => TunnelProxyUpdate::Explicit(proxy),
        None => TunnelProxyUpdate::Direct,
    };
    for profile in &migration.import.tunnel_profiles {
        if profile.effective_host_mode() != TunnelHostMode::Embedded {
            continue;
        }
        let credentials = owner.tunnel_credentials(&profile.profile_id).await?;
        import_embedded_tunnel_profile_under_lock(
            store,
            lock,
            &profile.profile_id,
            profile.name.as_deref(),
            profile.effective_autostart(),
            &credentials,
            &proxy_update,
        )?;
    }
    Ok(())
}

async fn finish_tunnels(
    store: &EnvironmentStore,
    lock: &EnvironmentLock,
    migration: &MigrationJournal,
    owner: &mut impl LegacyOwner,
) -> SetupResultValue<()> {
    let standalone = migration
        .import
        .tunnel_profiles
        .iter()
        .filter(|profile| profile.effective_host_mode() == TunnelHostMode::Standalone)
        .collect::<Vec<_>>();
    if standalone.is_empty() {
        return Ok(());
    }
    let proxy_update = match owner.tunnel_proxy() {
        Some(proxy) => TunnelProxyUpdate::Explicit(proxy),
        None => TunnelProxyUpdate::Direct,
    };
    let native = NativeEnvironment::new()?;
    for profile in standalone {
        let credentials = owner.tunnel_credentials(&profile.profile_id).await?;
        let status = native
            .configure_tunnel_under_lock(
                store,
                lock,
                &profile.profile_id,
                profile.name.as_deref(),
                Some(&credentials),
                &proxy_update,
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

fn preflight_new_owner_readonly(
    store: &EnvironmentStore,
    request: &SetupRequest,
    import: &LegacyImport,
    operation_id: &str,
    replaces_server: bool,
    replaces_runner: bool,
) -> SetupResultValue<()> {
    let record = EnvironmentRecord {
        schema_version: ENVIRONMENT_SCHEMA,
        environment_id: operation_id.to_owned(),
        request: request.clone(),
        username: Some(import.username.clone()),
        runner_client_id: import.runner_client_id.clone(),
        projects: import.projects.clone(),
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
        let replaces = match component {
            Component::Server => replaces_server,
            Component::Runner => replaces_runner,
            Component::Tunnel => false,
        };
        let spec = service_spec(store, &record, component)?;
        if !replaces {
            ServiceManager::preflight(&spec).map_err(service_diagnostic)?;
        } else {
            #[cfg(target_os = "linux")]
            ServiceManager::preflight_replacing(&spec).map_err(service_diagnostic)?;
        }
    }
    for profile in &import.tunnel_profiles {
        let spec = tunnel_service_spec(store, &record, &profile.profile_id)?;
        if profile.effective_host_mode() == TunnelHostMode::Embedded {
            let status = ServiceManager::inspect(&spec).map_err(service_diagnostic)?;
            if status.ownership != Ownership::Absent {
                return Err(diagnostic(
                    "tunnel_owner_conflict",
                    "A standalone Tunnel service already exists for an imported profile",
                ));
            }
        } else {
            ServiceManager::preflight(&spec).map_err(service_diagnostic)?;
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
    preflight_new_owner_readonly(
        store,
        &migration.request,
        &migration.import,
        &migration.operation_id,
        owner.replaces_native_service(Component::Server),
        owner.replaces_native_service(Component::Runner),
    )?;
    #[cfg(windows)]
    if migration.request.local_runner() && migration.request.service_scope.is_system() {
        let record = EnvironmentRecord {
            schema_version: ENVIRONMENT_SCHEMA,
            environment_id: migration.operation_id.clone(),
            request: migration.request.clone(),
            username: Some(migration.import.username.clone()),
            runner_client_id: migration.import.runner_client_id.clone(),
            projects: migration.import.projects.clone(),
            configured: false,
        };
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
    if !request.local_runner() && !import.projects.is_empty() {
        return Err(diagnostic(
            "legacy_project_identity",
            "A viewer cannot import local project registrations",
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
    if request.project.as_ref().is_some_and(|selected| {
        !import.projects.iter().any(|project| {
            project
                .path
                .to_str()
                .is_some_and(|recorded| same_project_path(recorded, selected))
                || project.path == *selected
        })
    }) {
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
    if cfg!(windows)
        && request.service_scope.is_system()
        && import
            .tunnel_profiles
            .iter()
            .any(|profile| profile.effective_host_mode() == TunnelHostMode::Embedded)
    {
        return Err(diagnostic(
            "tunnel_host_unsupported",
            "Embedded Tunnel setup currently requires a user-owned Server on Windows",
        ));
    }
    if import
        .tunnel_profiles
        .iter()
        .filter(|profile| profile.effective_host_mode() == TunnelHostMode::Embedded)
        .count()
        > 16
    {
        return Err(diagnostic(
            "tunnel_profile_capacity",
            "At most 16 Server-owned Tunnel profiles are supported",
        ));
    }
    for profile in &import.tunnel_profiles {
        if let Some(name) = &profile.name {
            crate::tunnel::validate_name(name)?;
        }
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
    let user_token = read_legacy_source(&import.user_token_file)?;
    if user_token.is_empty() {
        return Err(diagnostic(
            "legacy_user_credential",
            "The original user credential is empty",
        ));
    }
    if let Some(path) = &import.server_env_file {
        let env = String::from_utf8(read_legacy_source(path)?).map_err(|_| {
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
        let content = String::from_utf8(read_legacy_source(path)?).map_err(|_| {
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
        let recorded_server = config
            .get("server_url")
            .and_then(toml::Value::as_str)
            .and_then(|url| canonical_server_url(url).ok());
        let expected_server = canonical_server_url(&request.server_url).ok();
        if recorded_server.is_none()
            || recorded_server != expected_server
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
                let bytes = read_legacy_source(&source)?;
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
                    || !value
                        .get("path")
                        .and_then(toml::Value::as_str)
                        .is_some_and(|recorded| same_project_path(recorded, &project.path))
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

fn parse_env_key(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed
        .strip_prefix("export ")
        .map(str::trim_start)
        .unwrap_or(trimmed)
}

fn unquote_env_value(raw: &str) -> &str {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    }
}

fn exactly_one_env(content: &str, key: &str) -> SetupResultValue<Option<String>> {
    let mut found = None;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        if parse_env_key(name) == key {
            let value = unquote_env_value(value);
            if found.is_some() || value.is_empty() {
                return Err(diagnostic(
                    "legacy_env_ambiguous",
                    "The original Server environment has duplicate or empty required fields",
                ));
            }
            found = Some(value.to_owned());
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
    let token = read_legacy_source(&migration.import.user_token_file)?;
    write_same_or_new(&root.join("webcodex-user-token"), &token)?;
    if let Some(path) = &migration.import.server_env_file {
        let old_env = String::from_utf8(read_legacy_source(path)?).map_err(|_| {
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
                if !line.trim_start().starts_with('#')
                    && line
                        .split_once('=')
                        .is_some_and(|(key, _)| parse_env_key(key) == "WEBCODEX_DATA")
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
        let original = String::from_utf8(read_legacy_source(path)?).map_err(|_| {
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
            "server_url".into(),
            migration.request.server_url.clone().into(),
        );
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
                let bytes = read_legacy_source(&source)?;
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
    fn legacy_tunnel_receipts_retain_the_original_wire_format_without_a_name() {
        let original = serde_json::json!({"profile_id": "primary", "start": true});
        let mut profile: LegacyTunnelProfile = serde_json::from_value(original.clone()).unwrap();
        assert_eq!(serde_json::to_value(&profile).unwrap(), original);
        profile.name = Some("ChatGPT Personal".into());
        let encoded = serde_json::to_vec(&profile).unwrap();
        assert_eq!(
            serde_json::from_slice::<LegacyTunnelProfile>(&encoded).unwrap(),
            profile
        );
    }

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
    fn migration_scope_is_retained_before_setup_exists() {
        use crate::service::ServiceScope;
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        assert_eq!(
            crate::resolve_service_scope(&store, None).unwrap(),
            ServiceScope::User
        );
        for scope in [ServiceScope::User, ServiceScope::System] {
            let mut migration = MigrationJournal {
                schema_version: MIGRATION_SCHEMA,
                operation_id: "scope-test".into(),
                phase: MigrationPhase::Prepared,
                request: SetupRequest {
                    runner_display_name: None,
                    service_scope: scope,
                    mode: EnvironmentMode::Join,
                    server_url: "https://server.example".into(),
                    project: None,
                    runner: Some(true),
                    account: crate::LocalAccount {
                        name: "owner".into(),
                        identity: "1001".into(),
                        home: temp.path().to_path_buf(),
                    },
                    binaries: crate::RuntimeBinaries {
                        cli: temp.path().join("cli"),
                        server: temp.path().join("server"),
                        runner: temp.path().join("runner"),
                    },
                },
                import: LegacyImport {
                    server_env_file: None,
                    runner_config_file: None,
                    user_token_file: temp.path().join("token"),
                    username: "owner".into(),
                    runner_client_id: Some("runner".into()),
                    projects: vec![],
                    tunnel_profiles: vec![],
                },
                captured: LegacyOwnerSnapshot {
                    owner_id: "desktop".into(),
                    configuration_fingerprint: "binding".into(),
                    processes: vec![],
                },
                last_diagnostic: None,
            };
            for phase in [
                MigrationPhase::Prepared,
                MigrationPhase::OldStopped,
                MigrationPhase::RecoveryRequired,
            ] {
                migration.phase = phase;
                save(&store, &migration).unwrap();
                assert!(store.load_journal().unwrap().is_none());
                assert_eq!(crate::resolve_service_scope(&store, None).unwrap(), scope);
                assert_eq!(
                    crate::resolve_service_scope(&store, Some(scope)).unwrap(),
                    scope
                );
                let other = if scope == ServiceScope::User {
                    ServiceScope::System
                } else {
                    ServiceScope::User
                };
                assert_eq!(
                    crate::resolve_service_scope(&store, Some(other))
                        .unwrap_err()
                        .code,
                    "service_scope_conflict"
                );
            }
        }
        fs::write(store.root().join("migration.json"), "broken").unwrap();
        assert!(crate::resolve_service_scope(&store, None).is_err());
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
        let temp = crate::test_tempdir().unwrap();
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
            runner_display_name: None,
            service_scope: crate::service::ServiceScope::System,
            mode: EnvironmentMode::Join,
            server_url: "https://server.example".into(),
            project: Some(project.clone()),
            runner: None,
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
        // Default/display project absence must not invalidate a real saved Runner.
        let mut projectless = migration.request.clone();
        projectless.project = None;
        projectless.runner = Some(true);
        let mut empty_import = migration.import.clone();
        empty_import.projects.clear();
        validate_import(&projectless, &empty_import).unwrap();
        projectless.runner = Some(false);
        assert!(validate_import(&projectless, &empty_import).is_err());

        let lock = store.lock().unwrap();
        import_original_files(&store, &lock, &migration).unwrap();
        // Conflicting handoff and setup journals must fail closed, not select
        // whichever one happened to be loaded first.
        let mut conflicting = migration.clone();
        conflicting.request.service_scope = crate::service::ServiceScope::User;
        save(&store, &conflicting).unwrap();
        assert_eq!(
            crate::resolve_service_scope(&store, None).unwrap_err().code,
            "service_scope_conflict"
        );
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

    #[test]
    fn accepts_legacy_0644_project_registry_quoted_env_and_canonical_paths() {
        let temp = crate::test_tempdir().unwrap();
        let project = temp.path().join("project with spaces");
        fs::create_dir(&project).unwrap();
        let canonical_project = project.canonicalize().unwrap();
        let data_dir = temp.path().join("server data");
        fs::create_dir(&data_dir).unwrap();
        let token_file = temp.path().join("legacy-user-token");
        let server_env = temp.path().join("legacy-webcodex.env");
        let config_file = temp.path().join("legacy-runner.toml");
        let source_registry = temp.path().join("old-registry");
        fs::create_dir(&source_registry).unwrap();

        // Simulate Runner's File::create default permissions (0644 on Unix) and
        // non-canonical project path (or symlink alias on Unix).
        let recorded_path = {
            #[cfg(unix)]
            {
                let alias = temp.path().join("project-link");
                std::os::unix::fs::symlink(&canonical_project, &alias).unwrap();
                alias
            }
            #[cfg(not(unix))]
            {
                project.clone()
            }
        };
        let registry_file = source_registry.join("existing-project.toml");
        fs::write(
            &registry_file,
            format!(
                "id = 'existing-project'\npath = '{}'\nallow_patch = true\n",
                recorded_path.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&registry_file, fs::Permissions::from_mode(0o644)).unwrap();
        }
        fs::write(&token_file, b"wc_user_existing").unwrap();
        fs::write(
            &server_env,
            format!(
                "# legacy server env\nWEBCODEX_ADDR=127.0.0.1:62645\nexport WEBCODEX_DATA=\"{}\"\nWEBCODEX_TOKEN='wc_boot_existing'\n",
                data_dir.display()
            ),
        )
        .unwrap();
        fs::write(
            &config_file,
            format!(
                "server_url = 'http://127.0.0.1:62645/'\nclient_id = 'existing-runner'\nowner = 'alice'\ntoken = 'wc_agent_existing'\nproject_registry_dir = '{}'\n",
                source_registry.display()
            ),
        )
        .unwrap();

        let request = SetupRequest {
            runner_display_name: None,
            service_scope: crate::service::ServiceScope::User,
            mode: EnvironmentMode::Create {
                listen: "127.0.0.1:62645".into(),
            },
            server_url: "http://127.0.0.1:62645".into(),
            project: Some(canonical_project.clone()),
            runner: Some(true),
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
        let import = LegacyImport {
            server_env_file: Some(server_env),
            runner_config_file: Some(config_file),
            user_token_file: token_file,
            username: "alice".into(),
            runner_client_id: Some("existing-runner".into()),
            projects: vec![ProjectRecord {
                id: "existing-project".into(),
                path: canonical_project,
            }],
            tunnel_profiles: vec![],
        };
        inspect_legacy_import(&request, &import).unwrap();

        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let migration = MigrationJournal {
            schema_version: MIGRATION_SCHEMA,
            operation_id: "migration-0644".into(),
            phase: MigrationPhase::OldStopped,
            request,
            import,
            captured: LegacyOwnerSnapshot {
                owner_id: "desktop".into(),
                configuration_fingerprint: "binding".into(),
                processes: vec![],
            },
            last_diagnostic: None,
        };
        let lock = store.lock().unwrap();
        import_original_files(&store, &lock, &migration).unwrap();
        let imported_env = read_private(&store.root().join("server/webcodex.env")).unwrap();
        let imported_env = String::from_utf8(imported_env).unwrap();
        assert_eq!(
            exactly_one_env(&imported_env, "WEBCODEX_DATA")
                .unwrap()
                .as_deref(),
            store.root().join("server/data").to_str()
        );
        let imported_runner: toml::Value = toml::from_str(
            std::str::from_utf8(&read_private(&store.root().join("runner.toml")).unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            imported_runner["server_url"].as_str(),
            Some("http://127.0.0.1:62645")
        );
    }

    #[test]
    fn preflight_failure_does_not_persist_a_stale_prepared_journal() {
        struct NoopOwner;
        impl LegacyOwner for NoopOwner {
            async fn inspect(&mut self) -> SetupResultValue<LegacyOwnerSnapshot> {
                Ok(LegacyOwnerSnapshot {
                    owner_id: "desktop".into(),
                    configuration_fingerprint: "fp".into(),
                    processes: vec![],
                })
            }
            async fn stop(&mut self, _: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
                panic!("preflight failure must not stop legacy processes");
            }
            async fn restore(&mut self, _: &LegacyOwnerSnapshot) -> SetupResultValue<()> {
                panic!("preflight failure must not restore legacy processes");
            }
        }
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let missing_token = temp.path().join("missing-token");
        let request = SetupRequest {
            runner_display_name: None,
            service_scope: crate::service::ServiceScope::User,
            mode: EnvironmentMode::Join,
            server_url: "https://server.example".into(),
            project: None,
            runner: Some(false),
            account: crate::current_account().unwrap(),
            binaries: crate::RuntimeBinaries {
                cli: temp.path().join("cli"),
                server: temp.path().join("server"),
                runner: temp.path().join("runner"),
            },
        };
        let import = LegacyImport {
            server_env_file: None,
            runner_config_file: None,
            user_token_file: missing_token,
            username: "alice".into(),
            runner_client_id: None,
            projects: vec![],
            tunnel_profiles: vec![],
        };
        let mut owner = NoopOwner;
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let err = runtime
            .block_on(migrate_legacy_environment(
                &store,
                request,
                import,
                &mut owner,
                &SetupSecrets::default(),
                |_| {},
            ))
            .unwrap_err();
        assert_eq!(err.code, "state_io");
        assert!(migration_journal(&store).unwrap().is_none());
    }

    #[test]
    fn reset_failed_migration_clears_clean_journal_and_fails_closed_when_committed() {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let request = SetupRequest {
            runner_display_name: None,
            service_scope: crate::service::ServiceScope::User,
            mode: EnvironmentMode::Join,
            server_url: "https://server.example".into(),
            project: None,
            runner: Some(false),
            account: crate::current_account().unwrap(),
            binaries: crate::RuntimeBinaries {
                cli: temp.path().join("cli"),
                server: temp.path().join("server"),
                runner: temp.path().join("runner"),
            },
        };
        let mut journal_value = MigrationJournal {
            schema_version: MIGRATION_SCHEMA,
            operation_id: "reset-test".into(),
            phase: MigrationPhase::RecoveryRequired,
            request: request.clone(),
            import: LegacyImport {
                server_env_file: None,
                runner_config_file: None,
                user_token_file: temp.path().join("token"),
                username: "alice".into(),
                runner_client_id: None,
                projects: vec![],
                tunnel_profiles: vec![],
            },
            captured: LegacyOwnerSnapshot {
                owner_id: "desktop".into(),
                configuration_fingerprint: "fp".into(),
                processes: vec![],
            },
            last_diagnostic: None,
        };
        save(&store, &journal_value).unwrap();
        atomic_private_write(&store.root().join("webcodex-user-token"), b"staged").unwrap();
        assert!(can_reset_migration(&store).unwrap());
        reset_failed_migration(&store).unwrap();
        assert!(migration_journal(&store).unwrap().is_none());
        assert!(!store.root().join("webcodex-user-token").exists());

        // Never reset when an environment has already been committed.
        journal_value.phase = MigrationPhase::Prepared;
        save(&store, &journal_value).unwrap();
        store
            .save_environment(&EnvironmentRecord {
                schema_version: ENVIRONMENT_SCHEMA,
                environment_id: "committed".into(),
                request,
                username: Some("alice".into()),
                runner_client_id: None,
                projects: vec![],
                configured: true,
            })
            .unwrap();
        assert!(!can_reset_migration(&store).unwrap());
        assert_eq!(
            reset_failed_migration(&store).unwrap_err().code,
            "migration_recovery_required"
        );
    }
}
