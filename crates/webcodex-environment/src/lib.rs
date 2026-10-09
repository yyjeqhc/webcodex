//! The deployment configuration authority shared by Desktop, CLI and installers.
//! Runtime observations and authorization remain the Server's responsibility.
#![allow(async_fn_in_trait)]

#[cfg(test)]
fn test_tempdir() -> std::io::Result<tempfile::TempDir> {
    // macOS exposes TMPDIR through /var -> /private/var. Use the real OS temp
    // root in fixtures, without weakening production no-symlink protections.
    #[cfg(target_os = "macos")]
    return tempfile::Builder::new().tempdir_in(std::env::temp_dir().canonicalize()?);
    #[cfg(not(target_os = "macos"))]
    tempfile::tempdir()
}

mod cloudflare_tunnel;
pub use cloudflare_tunnel::{
    cloudflare_ingress_port, cloudflare_tunnel_profile, cloudflare_tunnel_profiles,
    load_cloudflare_server_ingress_port, load_cloudflare_server_materializations,
    load_cloudflare_tunnel_materialization, materialize_cloudflare_tunnel_profiles,
    CloudflareTunnelProfileRequest, CloudflareTunnelRuntimeProfile, TunnelProvider,
};
mod embedded_tunnel;
mod engine;
pub use embedded_tunnel::{embedded_tunnel_profiles, EmbeddedTunnelProfile};
mod installer_authorization;
#[cfg(unix)]
mod installer_unix;
pub mod inventory;
pub use inventory::*;
mod layout;
mod local_status;
mod migration;
mod native;
mod privilege;
mod process;
mod runner_preflight;
pub use runner_preflight::preflight_runner_configuration;
pub mod runtime_entry;
pub mod service;
pub mod session_service;
mod storage;
mod tunnel;
mod types;
pub use local_status::{ComponentObservation, LocalEnvironmentStatus};
pub use upgrade::{upgrade_observation, UpgradeObservation, UpgradeOutcome};
pub use upgrade::{
    upgrade_status, upgrade_status_at, UpgradeFileComponent, UpgradePhase, UpgradeServiceComponent,
    UpgradeServiceKind, UpgradeStatus, UpgradeTarget,
};
pub mod unified_update;
mod upgrade;
pub mod upgrade_transport;

pub use engine::{EnvironmentBackend, EnvironmentSetup, Reconciliation};
pub use migration::{
    migrate_legacy_environment, migration_journal, LegacyImport, LegacyOwner, LegacyOwnerSnapshot,
    LegacyProcess, LegacyTunnelProfile, MigrationJournal, MigrationPhase,
};
pub use native::{
    canonical_server_url, current_account, read_secret, resolve_service_scope, service_spec,
    validate_request, NativeEnvironment,
};
pub use privilege::run_privileged_service_request;
pub use storage::{default_environment_dir, EnvironmentLock, EnvironmentStore};
pub use types::*;

pub use installer_authorization::{
    authorize_prepared_installation, authorize_prepared_installation_for_target,
    cancel_installer_authorization, verify_installer_authorization,
    verify_installer_package_target, verify_installer_targets,
};
#[cfg(unix)]
pub use installer_unix::{finish_authorized_installation, run_installer_upgrade_child};
pub use layout::installed_desktop_runtime_directory;
pub use upgrade::windows_package;
pub use upgrade::{
    ensure_upgrade_idle_under_lock, verify_prepared_installation, verify_same_installed_package,
    verify_upgrade_candidate, CandidateArtifact, CandidateDesktop, PreparedInstallationReceipt,
    UpgradeCandidate, UpgradePreflight,
};

pub use tunnel::{
    tunnel_profile_credentials, tunnel_profile_snapshots, tunnel_profiles, tunnel_service_spec,
    write_embedded_tunnel_health, write_tunnel_health, TunnelConfigurationNextAction,
    TunnelConfigurationResult, TunnelCredentials, TunnelHostMode, TunnelProfileSnapshot,
    TunnelRecord, TunnelRuntimeObservation,
};
