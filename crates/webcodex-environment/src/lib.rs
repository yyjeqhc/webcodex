//! The deployment configuration authority shared by Desktop, CLI and installers.
//! Runtime observations and authorization remain the Server's responsibility.
#![allow(async_fn_in_trait)]

mod engine;
mod installer_authorization;
#[cfg(unix)]
mod installer_unix;
mod layout;
#[cfg(target_os = "linux")]
mod legacy_cli;
#[cfg(target_os = "linux")]
mod legacy_system_server;
#[cfg(target_os = "linux")]
mod legacy_systemd;
mod migration;
mod native;
mod privilege;
mod process;
pub mod runtime_entry;
pub mod service;
pub mod session_service;
mod storage;
mod tunnel;
mod types;
mod upgrade;
pub mod upgrade_transport;

pub use engine::{EnvironmentBackend, EnvironmentSetup, Reconciliation};
pub use migration::{
    migrate_legacy_environment, migration_journal, LegacyImport, LegacyOwner, LegacyOwnerSnapshot,
    LegacyProcess, LegacyTunnelProfile, MigrationJournal, MigrationPhase,
};
pub use native::{
    canonical_server_url, current_account, read_secret, service_spec, validate_request,
    NativeEnvironment,
};
pub use privilege::run_privileged_service_request;
pub use storage::{default_environment_dir, EnvironmentStore};
pub use types::*;

pub use installer_authorization::{
    authorize_prepared_installation, cancel_installer_authorization,
    verify_installer_authorization, verify_installer_targets,
};
#[cfg(unix)]
pub use installer_unix::{finish_authorized_installation, run_installer_upgrade_child};
pub use layout::installed_desktop_runtime_directory;
#[cfg(target_os = "linux")]
pub use legacy_cli::{migrate_legacy_cli_user_runner, LegacyCliRunnerInput};
#[cfg(target_os = "linux")]
pub use legacy_system_server::{migrate_legacy_cli_system_server, LegacyCliServerInput};
pub use upgrade::{
    ensure_upgrade_idle_under_lock, verify_prepared_installation, verify_same_installed_package,
    verify_upgrade_candidate, CandidateArtifact, CandidateDesktop, PreparedInstallationReceipt,
    UpgradeCandidate, UpgradePreflight,
};

pub use tunnel::{
    tunnel_profiles, tunnel_service_spec, write_tunnel_health, TunnelCredentials, TunnelRecord,
    TunnelRuntimeObservation,
};
