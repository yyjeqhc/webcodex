//! Persistent Tunnel profiles retain the existing OpenAI connection identity.
//! Runner machines never import or acquire these credentials.
use crate::native::{bootstrap_token, env_value, service_error};
use crate::service::{Component, Ownership, ServiceAccount, ServiceManager, ServiceSpec};
use crate::storage::{atomic_private_write, ensure_private_directory};
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct TunnelCredentials {
    pub tunnel_id: Secret,
    pub api_key: Secret,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelHostMode {
    #[default]
    Standalone,
    Embedded,
}

fn default_true() -> bool {
    true
}
fn default_revision() -> u64 {
    1
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelRecord {
    pub profile_id: String,
    #[serde(default)]
    pub provider: TunnelProvider,
    /// Private catalog incarnation; deleting and recreating a Cloudflare profile
    /// must not inherit the previous profile's public OAuth grants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub host_mode: TunnelHostMode,
    #[serde(default = "default_true")]
    pub autostart: bool,
    #[serde(default = "default_revision")]
    pub revision: u64,
    /// Revision of fields consumed only when an owner starts. Older records use
    /// the catalog revision; display-name-only edits do not force a Server restart.
    #[serde(default)]
    pub runtime_revision: u64,
    pub installed: bool,
    pub started: bool,
}
impl TunnelRecord {
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() {
            &self.profile_id
        } else {
            &self.name
        }
    }
    pub fn effective_configuration_id(&self) -> Option<String> {
        if self.provider == TunnelProvider::Openai {
            None
        } else {
            Some(
                self.configuration_id
                    .clone()
                    .unwrap_or_else(|| format!("legacy:{}", self.profile_id)),
            )
        }
    }
    pub fn effective_runtime_revision(&self) -> u64 {
        if self.runtime_revision == 0 {
            self.revision.max(1)
        } else {
            self.runtime_revision
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelProfileSnapshot {
    pub profile_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_id: Option<String>,
    #[serde(default)]
    pub provider: TunnelProvider,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ingress_port: Option<u16>,
    pub name: String,
    pub tunnel_id: String,
    pub credential_present: bool,
    pub host_mode: TunnelHostMode,
    pub autostart: bool,
    pub revision: u64,
    pub installed: bool,
    pub started: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TunnelConfigurationNextAction {
    None,
    StartServer,
    RestartServer,
    StartStandalone,
    RestartStandalone,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelConfigurationResult {
    pub profile: TunnelProfileSnapshot,
    pub owner_status: service::ServiceStatus,
    pub server_restart_required: bool,
    pub next_action: TunnelConfigurationNextAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TunnelRuntimeObservation {
    pub profile_id: String,
    pub tunnel_id: String,
    pub service_status: service::ServiceStatus,
    #[serde(default)]
    pub host_mode: TunnelHostMode,
    pub autostart: bool,
    pub ready: bool,
    pub tunnel_ready: bool,
    pub local_mcp_ready: bool,
    pub configured_revision: u64,
    pub applied_revision: Option<u64>,
    pub server_restart_required: bool,
}

#[derive(Debug)]
pub(crate) struct TunnelProfileBinding {
    pub tunnel_id: Secret,
    pub api_key: Secret,
    pub local_token: Secret,
    pub proxy: Option<Secret>,
}
fn diagnostic(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(code, message, "Inspect the saved Tunnel profile and its system service; keep its original Tunnel ID and API credential")
}
pub(crate) fn validate_id(id: &str) -> SetupResultValue<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(diagnostic(
            "tunnel_profile",
            "Tunnel profile identifier is invalid",
        ));
    }
    Ok(())
}
pub fn tunnel_service_spec(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    profile_id: &str,
) -> SetupResultValue<ServiceSpec> {
    validate_id(profile_id)?;
    let provider = tunnel_profiles(store)?
        .into_iter()
        .find(|profile| profile.profile_id == profile_id)
        .map(|profile| profile.provider)
        .unwrap_or_default();
    tunnel_service_spec_for_provider(store, record, profile_id, &provider)
}

fn tunnel_service_spec_for_provider(
    store: &EnvironmentStore,
    record: &EnvironmentRecord,
    profile_id: &str,
    provider: &TunnelProvider,
) -> SetupResultValue<ServiceSpec> {
    validate_id(profile_id)?;
    if !record.request.local_server() {
        return Err(diagnostic(
            "tunnel_local_server",
            "A Tunnel can only be hosted by the Server machine",
        ));
    }
    let id = if cfg!(windows) {
        format!("WebCodexTunnel-{profile_id}")
    } else {
        format!("webcodex-tunnel-{profile_id}")
    };
    let directory = store.root().join("server/tunnels").join(profile_id);
    let env_file = directory.join("webcodex.env");
    let mut args = vec![
        "server".into(),
        "tunnel".into(),
        "--env-file".into(),
        env_file.to_string_lossy().into_owned(),
        "--provider".into(),
        "openai".into(),
        "--json".into(),
    ];
    if provider != &TunnelProvider::Openai {
        args = vec![
            "server".into(),
            "tunnel".into(),
            "--runtime-binding".into(),
            directory
                .join("runtime.json")
                .to_string_lossy()
                .into_owned(),
            "--provider".into(),
            match provider {
                TunnelProvider::CloudflareNamed { .. } => "cloudflare_named",
                _ => "cloudflare_quick",
            }
            .into(),
            "--json".into(),
        ];
    }
    if cfg!(windows) && record.request.service_scope.is_system() {
        args.splice(0..0, ["--windows-service".into(), id.clone()]);
    }
    let account = if cfg!(windows) && record.request.service_scope.is_system() {
        ServiceAccount::WindowsVirtual {
            name: format!("NT SERVICE\\{id}"),
        }
    } else {
        ServiceAccount::SystemUser {
            name: record.request.account.name.clone(),
            group: None,
            expected_identity: record.request.account.identity.clone(),
            home: Some(record.request.account.home.clone()),
        }
    };
    let environment = if cfg!(target_os = "macos") {
        BTreeMap::from([(
            "WEBCODEX_SERVICE_LOG_DIR".into(),
            directory.to_string_lossy().into_owned(),
        )])
    } else {
        BTreeMap::new()
    };
    Ok(ServiceSpec {
        scope: record.request.service_scope,
        id,
        component: Component::Tunnel,
        program: record.request.binaries.cli.clone(),
        args,
        working_directory: directory,
        account,
        config_identity: format!("{}-tunnel-{profile_id}", record.environment_id),
        env_file: Some(env_file),
        environment,
        linux_socket: None,
    })
}
fn profile_directory(store: &EnvironmentStore, profile_id: &str) -> std::path::PathBuf {
    store.root().join("server/tunnels").join(profile_id)
}

fn validate_name(name: &str) -> SetupResultValue<String> {
    let name = name.trim();
    if name.is_empty() || name.len() > 160 || name.chars().any(|value| value.is_control()) {
        return Err(diagnostic(
            "tunnel_profile_name",
            "Tunnel profile name is invalid",
        ));
    }
    Ok(name.to_owned())
}

fn validate_credentials(credentials: &TunnelCredentials) -> SetupResultValue<()> {
    let id = credentials.tunnel_id.expose();
    let api = credentials.api_key.expose();
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        || api.is_empty()
        || api.len() > 8192
        || !api.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(diagnostic(
            "tunnel_credentials",
            "Tunnel credentials are invalid",
        ));
    }
    Ok(())
}

pub(crate) fn unique_env_value(content: &str, key: &str) -> SetupResultValue<Option<String>> {
    if content
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(name, _)| name.trim() == key)
        .count()
        > 1
    {
        return Err(diagnostic(
            "tunnel_profile_configuration",
            "Tunnel profile configuration is incomplete or ambiguous",
        ));
    }
    Ok(env_value(content, key))
}

fn required_env_value(content: &str, key: &str) -> SetupResultValue<String> {
    unique_env_value(content, key)?
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            diagnostic(
                "tunnel_profile_configuration",
                "Tunnel profile configuration is incomplete or ambiguous",
            )
        })
}

pub(crate) fn tunnel_profile_binding(
    store: &EnvironmentStore,
    profile_id: &str,
) -> SetupResultValue<TunnelProfileBinding> {
    validate_id(profile_id)?;
    let path = profile_directory(store, profile_id).join("webcodex.env");
    let source = read_secret(&path)?;
    if source.expose().len() > 32 * 1024 {
        return Err(diagnostic(
            "tunnel_profile_configuration",
            "Tunnel profile configuration is too large",
        ));
    }
    if required_env_value(source.expose(), "WEBCODEX_TUNNEL_PROFILE_ID")? != profile_id {
        return Err(diagnostic(
            "tunnel_profile_configuration",
            "Tunnel profile identity does not match its private binding",
        ));
    }
    Ok(TunnelProfileBinding {
        tunnel_id: Secret::new(required_env_value(
            source.expose(),
            "CONTROL_PLANE_TUNNEL_ID",
        )?),
        api_key: Secret::new(required_env_value(
            source.expose(),
            "CONTROL_PLANE_API_KEY",
        )?),
        local_token: Secret::new(required_env_value(source.expose(), "WEBCODEX_TOKEN")?),
        proxy: unique_env_value(source.expose(), "WEBCODEX_TUNNEL_PROXY")?.map(Secret::new),
    })
}

pub fn tunnel_profile_credentials(
    store: &EnvironmentStore,
    profile_id: &str,
) -> SetupResultValue<TunnelCredentials> {
    let binding = tunnel_profile_binding(store, profile_id)?;
    Ok(TunnelCredentials {
        tunnel_id: binding.tunnel_id,
        api_key: binding.api_key,
    })
}

fn snapshot(record: &TunnelRecord, binding: &TunnelProfileBinding) -> TunnelProfileSnapshot {
    TunnelProfileSnapshot {
        profile_id: record.profile_id.clone(),
        configuration_id: record.effective_configuration_id(),
        provider: record.provider.clone(),
        ingress_port: None,
        name: record.display_name().to_owned(),
        tunnel_id: binding.tunnel_id.expose().to_owned(),
        credential_present: !binding.api_key.expose().is_empty(),
        host_mode: record.host_mode,
        autostart: record.autostart,
        revision: record.revision.max(1),
        installed: record.installed,
        started: record.started,
    }
}

pub(crate) fn validate_catalog(profiles: &[TunnelRecord]) -> SetupResultValue<()> {
    if profiles.len() > 64 {
        return Err(diagnostic(
            "tunnel_profile_capacity",
            "Tunnel profile catalog exceeds its bounded capacity",
        ));
    }
    let mut names = BTreeSet::new();
    let mut embedded = 0usize;
    for profile in profiles {
        validate_id(&profile.profile_id)?;
        crate::cloudflare_tunnel::validate_provider(&profile.provider)?;
        if !profile.name.is_empty() {
            validate_name(&profile.name)?;
        }
        if !names.insert(profile.profile_id.as_str()) {
            return Err(diagnostic(
                "tunnel_profile_duplicate",
                "Tunnel profile identifiers must be unique",
            ));
        }
        if profile.host_mode == TunnelHostMode::Embedded {
            embedded += 1;
            if profile.installed || profile.started {
                return Err(diagnostic(
                    "tunnel_owner_conflict",
                    "An embedded profile still claims standalone lifecycle state",
                ));
            }
        }
    }
    if embedded > 16 {
        return Err(diagnostic(
            "tunnel_profile_capacity",
            "At most 16 Server-owned Tunnel profiles are supported",
        ));
    }
    crate::cloudflare_tunnel::validate_selection(profiles)?;
    Ok(())
}

pub fn tunnel_profile_snapshots(
    store: &EnvironmentStore,
) -> SetupResultValue<Vec<TunnelProfileSnapshot>> {
    let profiles = tunnel_profiles(store)?;
    validate_catalog(&profiles)?;
    let mut identities = BTreeSet::new();
    let mut result = Vec::with_capacity(profiles.len());
    for profile in &profiles {
        if profile.provider != TunnelProvider::Openai {
            result.push(crate::cloudflare_tunnel::snapshot(store, profile)?);
            continue;
        }
        let binding = tunnel_profile_binding(store, &profile.profile_id)?;
        if !identities.insert(binding.tunnel_id.expose().to_owned()) {
            return Err(diagnostic(
                "tunnel_identity_duplicate",
                "A Tunnel identity is bound to more than one profile",
            ));
        }
        result.push(snapshot(profile, &binding));
    }
    Ok(result)
}

fn ensure_unique_tunnel_identity(
    store: &EnvironmentStore,
    profiles: &[TunnelRecord],
    profile_id: &str,
    tunnel_id: &str,
) -> SetupResultValue<()> {
    for profile in profiles {
        if profile.profile_id == profile_id || profile.provider != TunnelProvider::Openai {
            continue;
        }
        let binding = tunnel_profile_binding(store, &profile.profile_id)?;
        if binding.tunnel_id.expose() == tunnel_id {
            return Err(diagnostic(
                "tunnel_identity_duplicate",
                "A Tunnel identity is already bound to another profile",
            ));
        }
    }
    Ok(())
}

pub(crate) fn ensure_server_tunnel_environment(store: &EnvironmentStore) -> SetupResultValue<()> {
    let path = store.root().join("server/webcodex.env");
    let source = read_secret(&path)?;
    let root = store.root().to_str().ok_or_else(|| {
        diagnostic(
            "tunnel_profile_configuration",
            "Environment path cannot be represented safely",
        )
    })?;
    if root.contains(['\n', '\r', '"']) {
        return Err(diagnostic(
            "tunnel_profile_configuration",
            "Environment path cannot be represented safely",
        ));
    }
    let existing = unique_env_value(source.expose(), "WEBCODEX_TUNNEL_ENVIRONMENT")?;
    if existing.as_deref().is_some_and(|value| value != root) {
        return Err(diagnostic(
            "tunnel_environment_conflict",
            "The Server is already bound to another Tunnel environment",
        ));
    }
    if existing.is_none() {
        let content = format!(
            "{}\nWEBCODEX_TUNNEL_ENVIRONMENT=\"{root}\"\n",
            source.expose()
        );
        atomic_private_write(&path, content.as_bytes())?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TunnelBindingMutation {
    None,
    Create,
    RotateCredential,
}

#[derive(Debug)]
struct TunnelBindingPlan {
    tunnel_id: String,
    mutation: TunnelBindingMutation,
}

fn plan_tunnel_binding(
    existing: Option<&TunnelProfileBinding>,
    candidate: Option<&TunnelCredentials>,
    expected_revision: Option<u64>,
) -> SetupResultValue<TunnelBindingPlan> {
    match (existing, candidate) {
        (Some(saved), Some(candidate)) => {
            validate_credentials(candidate)?;
            if saved.tunnel_id.expose() != candidate.tunnel_id.expose() {
                return Err(diagnostic(
                    "tunnel_binding_conflict",
                    "This profile already owns another Tunnel identity",
                ));
            }
            let mutation = if saved.api_key.expose() == candidate.api_key.expose() {
                TunnelBindingMutation::None
            } else if expected_revision.is_some() {
                TunnelBindingMutation::RotateCredential
            } else {
                return Err(SetupDiagnostic::new(
                    "tunnel_credential_rotation_requires_revision",
                    "Updating a saved Tunnel credential requires a current profile revision",
                    "Reload the profile and retry through the write-only credential editor; configure-tunnel remains idempotent for existing profiles",
                ));
            };
            Ok(TunnelBindingPlan {
                tunnel_id: saved.tunnel_id.expose().to_owned(),
                mutation,
            })
        }
        (Some(saved), None) => Ok(TunnelBindingPlan {
            tunnel_id: saved.tunnel_id.expose().to_owned(),
            mutation: TunnelBindingMutation::None,
        }),
        (None, Some(candidate)) => {
            validate_credentials(candidate)?;
            Ok(TunnelBindingPlan {
                tunnel_id: candidate.tunnel_id.expose().to_owned(),
                mutation: TunnelBindingMutation::Create,
            })
        }
        (None, None) => Err(diagnostic(
            "tunnel_credentials",
            "The original Tunnel credentials are required for the first configuration",
        )),
    }
}

fn write_profile_binding(
    store: &EnvironmentStore,
    profile_id: &str,
    credentials: &TunnelCredentials,
    previous: Option<&TunnelProfileBinding>,
) -> SetupResultValue<()> {
    validate_credentials(credentials)?;
    let directory = profile_directory(store, profile_id);
    ensure_private_directory(&directory)?;
    let server = read_secret(&store.root().join("server/webcodex.env"))?;
    let address = env_value(server.expose(), "WEBCODEX_ADDR").ok_or_else(|| {
        diagnostic(
            "server_configuration",
            "Server listening address is unavailable",
        )
    })?;
    let local_token = match previous {
        Some(saved) => Secret::new(saved.local_token.expose().to_owned()),
        None => bootstrap_token(store)?,
    };
    let proxy_line = previous
        .and_then(|saved| saved.proxy.as_ref())
        .map(|proxy| format!("WEBCODEX_TUNNEL_PROXY={}\n", proxy.expose()))
        .unwrap_or_default();
    let content = Secret::new(format!(
        "WEBCODEX_ADDR={address}\nWEBCODEX_TOKEN={}\nCONTROL_PLANE_TUNNEL_ID={}\nCONTROL_PLANE_API_KEY={}\nWEBCODEX_TUNNEL_PROFILE_ID={profile_id}\n{proxy_line}",
        local_token.expose(),
        credentials.tunnel_id.expose(),
        credentials.api_key.expose()
    ));
    atomic_private_write(&directory.join("webcodex.env"), content.expose().as_bytes())?;
    Ok(())
}

fn remove_created_profile_binding(
    store: &EnvironmentStore,
    profile_id: &str,
) -> SetupResultValue<()> {
    let path = profile_directory(store, profile_id).join("webcodex.env");
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SetupDiagnostic::io()),
        Ok(metadata) if metadata.is_file() && !crate::storage::is_link(&metadata) => {
            std::fs::remove_file(path).map_err(|_| SetupDiagnostic::io())
        }
        Ok(_) => Err(SetupDiagnostic::io()),
    }
}

fn restore_profile_binding(
    store: &EnvironmentStore,
    profile_id: &str,
    previous: &TunnelProfileBinding,
) -> SetupResultValue<()> {
    let credentials = TunnelCredentials {
        tunnel_id: Secret::new(previous.tunnel_id.expose().to_owned()),
        api_key: Secret::new(previous.api_key.expose().to_owned()),
    };
    write_profile_binding(store, profile_id, &credentials, Some(previous))
}

fn commit_profile_files(
    store: &EnvironmentStore,
    profile_id: &str,
    profiles: &[TunnelRecord],
    mutation: TunnelBindingMutation,
    credentials: Option<&TunnelCredentials>,
    previous: Option<&TunnelProfileBinding>,
    commit_catalog: impl FnOnce(&[TunnelRecord]) -> SetupResultValue<()>,
) -> SetupResultValue<()> {
    if mutation != TunnelBindingMutation::None {
        write_profile_binding(
            store,
            profile_id,
            credentials.expect("validated binding mutation"),
            previous,
        )?;
    }
    let Err(error) = commit_catalog(profiles) else {
        return Ok(());
    };
    let restored = match mutation {
        TunnelBindingMutation::None => Ok(()),
        TunnelBindingMutation::Create => remove_created_profile_binding(store, profile_id),
        TunnelBindingMutation::RotateCredential => previous
            .ok_or_else(SetupDiagnostic::io)
            .and_then(|binding| restore_profile_binding(store, profile_id, binding)),
    };
    if restored.is_err() {
        return Err(SetupDiagnostic::new(
            "tunnel_profile_recovery_required",
            "Tunnel profile files could not be restored after an interrupted catalog update",
            "Inspect this profile's private binding and tunnel.json before retrying; do not restart either owner until their exact saved state is known",
        ));
    }
    Err(error)
}

pub(crate) fn next_revision(value: u64) -> SetupResultValue<u64> {
    value.max(1).checked_add(1).ok_or_else(|| {
        diagnostic(
            "tunnel_revision",
            "Tunnel profile revision cannot advance safely",
        )
    })
}

pub(crate) fn validate_expected_revision(
    existing: Option<&TunnelRecord>,
    expected_revision: Option<u64>,
) -> SetupResultValue<()> {
    match (existing, expected_revision) {
        (Some(profile), Some(expected)) if expected != profile.revision.max(1) => Err(diagnostic(
            "tunnel_revision_stale",
            "Tunnel profile changed after it was opened",
        )),
        (None, Some(_)) => Err(diagnostic(
            "tunnel_revision_stale",
            "Tunnel profile no longer exists",
        )),
        _ => Ok(()),
    }
}

fn resolve_autostart(
    host_mode: TunnelHostMode,
    requested: Option<bool>,
    existing: Option<bool>,
) -> SetupResultValue<bool> {
    if host_mode == TunnelHostMode::Standalone {
        if requested == Some(false) {
            return Err(SetupDiagnostic::new(
                "tunnel_autostart_unsupported",
                "Separate managed Tunnel services do not use the Server-owned autostart setting",
                "Use the standalone service Start and Stop controls; choose Server-owned lifecycle for per-profile startup selection",
            ));
        }
        // Standalone preserves the historical managed-service lifecycle: install
        // owns boot/login enablement and explicit Start/Stop owns the current process.
        return Ok(true);
    }
    Ok(requested.or(existing).unwrap_or(true))
}

fn embedded_next_action(
    server: &service::ServiceStatus,
    configured_revision: u64,
    applied_revision: Option<u64>,
    runtime_changed: bool,
) -> (bool, TunnelConfigurationNextAction) {
    let owner_running = server.ownership == Ownership::Owned && server.running == Some(true);
    let restart_required =
        owner_running && (runtime_changed || applied_revision != Some(configured_revision));
    let next_action = if restart_required {
        TunnelConfigurationNextAction::RestartServer
    } else if !owner_running {
        TunnelConfigurationNextAction::StartServer
    } else {
        TunnelConfigurationNextAction::None
    };
    (restart_required, next_action)
}

pub fn tunnel_profiles(store: &EnvironmentStore) -> SetupResultValue<Vec<TunnelRecord>> {
    store
        .read_json("tunnel.json")
        .map(|value| value.unwrap_or_default())
}

impl NativeEnvironment {
    pub async fn configure_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        credentials: Option<&TunnelCredentials>,
    ) -> SetupResultValue<service::ServiceStatus> {
        Ok(self
            .configure_tunnel_profile(
                store,
                profile_id,
                None,
                TunnelHostMode::Standalone,
                None,
                None,
                credentials,
                true,
            )
            .await?
            .owner_status)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn configure_tunnel_profile(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        name: Option<&str>,
        host_mode: TunnelHostMode,
        autostart: Option<bool>,
        expected_revision: Option<u64>,
        credentials: Option<&TunnelCredentials>,
        start_standalone: bool,
    ) -> SetupResultValue<TunnelConfigurationResult> {
        let lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        self.configure_tunnel_profile_under_lock(
            store,
            &lock,
            profile_id,
            name,
            host_mode,
            autostart,
            expected_revision,
            credentials,
            start_standalone,
        )
        .await
    }

    pub(crate) async fn configure_tunnel_under_lock(
        &self,
        store: &EnvironmentStore,
        lock: &crate::storage::EnvironmentLock,
        profile_id: &str,
        credentials: Option<&TunnelCredentials>,
        start: bool,
    ) -> SetupResultValue<service::ServiceStatus> {
        Ok(self
            .configure_tunnel_profile_under_lock(
                store,
                lock,
                profile_id,
                None,
                TunnelHostMode::Standalone,
                None,
                None,
                credentials,
                start,
            )
            .await?
            .owner_status)
    }

    #[allow(clippy::too_many_arguments)]
    async fn configure_tunnel_profile_under_lock(
        &self,
        store: &EnvironmentStore,
        _lock: &crate::storage::EnvironmentLock,
        profile_id: &str,
        name: Option<&str>,
        host_mode: TunnelHostMode,
        autostart: Option<bool>,
        expected_revision: Option<u64>,
        credentials: Option<&TunnelCredentials>,
        start_standalone: bool,
    ) -> SetupResultValue<TunnelConfigurationResult> {
        validate_id(profile_id)?;
        crate::cloudflare_tunnel::ensure_cloudflare_profile_available(store, profile_id)?;
        let environment = store
            .load_environment()?
            .or_else(|| {
                store
                    .load_journal()
                    .ok()
                    .flatten()
                    .map(|journal| journal.environment)
            })
            .ok_or_else(|| {
                diagnostic("not_configured", "Configure the Server environment first")
            })?;
        if !environment.request.local_server() {
            return Err(diagnostic(
                "tunnel_local_server",
                "A Tunnel can only be hosted by the Server machine",
            ));
        }
        if cfg!(windows)
            && environment.request.service_scope.is_system()
            && host_mode == TunnelHostMode::Embedded
        {
            return Err(SetupDiagnostic::new(
                "tunnel_host_unsupported",
                "Embedded Tunnel setup currently requires a user-owned Server on Windows",
                "Keep the standalone managed service for this system-scope environment",
            ));
        }

        let mut profiles = tunnel_profiles(store)?;
        validate_catalog(&profiles)?;
        let existing_index = profiles
            .iter()
            .position(|profile| profile.profile_id == profile_id);
        validate_expected_revision(
            existing_index.map(|index| &profiles[index]),
            expected_revision,
        )?;
        if let Some(index) = existing_index {
            let existing = &profiles[index];
            if existing.provider != TunnelProvider::Openai {
                return Err(diagnostic(
                    "tunnel_provider_conflict",
                    "A saved profile cannot change Tunnel providers",
                ));
            }
            if existing.host_mode != host_mode {
                return Err(SetupDiagnostic::new(
                    "tunnel_host_transfer_required",
                    "An existing Tunnel profile cannot change owners during configuration",
                    "Stop and uninstall the previous owner, then use the explicit tunnel-host operation",
                ));
            }
        }
        if existing_index.is_none()
            && host_mode == TunnelHostMode::Embedded
            && profiles
                .iter()
                .filter(|profile| profile.host_mode == TunnelHostMode::Embedded)
                .count()
                == 16
        {
            return Err(diagnostic(
                "tunnel_profile_capacity",
                "At most 16 Server-owned Tunnel profiles are supported",
            ));
        }

        let standalone_spec = tunnel_service_spec(store, &environment, profile_id)?;
        let standalone_before = ServiceManager::inspect(&standalone_spec).map_err(service_error)?;
        let server_before = if host_mode == TunnelHostMode::Embedded {
            Some(
                ServiceManager::inspect(&crate::service_spec(
                    store,
                    &environment,
                    Component::Server,
                )?)
                .map_err(service_error)?,
            )
        } else {
            None
        };
        if host_mode == TunnelHostMode::Embedded {
            if standalone_before.ownership != Ownership::Absent {
                return Err(SetupDiagnostic::new(
                    "tunnel_host_busy",
                    "A standalone Tunnel service still owns this profile",
                    "Stop it cleanly and uninstall the exact service before transferring ownership",
                ));
            }
            if matches!(
                server_before
                    .as_ref()
                    .expect("embedded Server observation")
                    .ownership,
                Ownership::Foreign | Ownership::Unknown
            ) {
                return Err(diagnostic(
                    "tunnel_owner",
                    "The owning Server service cannot be verified",
                ));
            }
        } else {
            ServiceManager::preflight(&standalone_spec).map_err(service_error)?;
        }

        let env_path = profile_directory(store, profile_id).join("webcodex.env");
        let existing_binding = env_path
            .exists()
            .then(|| tunnel_profile_binding(store, profile_id))
            .transpose()?;
        let binding_plan =
            plan_tunnel_binding(existing_binding.as_ref(), credentials, expected_revision)?;
        let credential_changed = binding_plan.mutation == TunnelBindingMutation::RotateCredential;
        ensure_unique_tunnel_identity(store, &profiles, profile_id, &binding_plan.tunnel_id)?;

        let desired_name = match name {
            Some(value) => validate_name(value)?,
            None => existing_index
                .map(|index| profiles[index].display_name().to_owned())
                .unwrap_or_else(|| profile_id.to_owned()),
        };
        let desired_autostart = resolve_autostart(
            host_mode,
            autostart,
            existing_index.map(|index| profiles[index].autostart),
        )?;

        if host_mode == TunnelHostMode::Embedded {
            ensure_server_tunnel_environment(store)?;
        }

        let runtime_changed;
        let index = if let Some(index) = existing_index {
            let profile = &mut profiles[index];
            let previous_runtime_revision = profile.effective_runtime_revision();
            let name_changed = profile.display_name() != desired_name;
            runtime_changed = profile.autostart != desired_autostart || credential_changed;
            if name_changed || runtime_changed {
                profile.revision = next_revision(profile.revision)?;
            } else {
                profile.revision = profile.revision.max(1);
            }
            profile.runtime_revision = if runtime_changed {
                next_revision(previous_runtime_revision)?
            } else {
                previous_runtime_revision
            };
            profile.name = desired_name;
            profile.autostart = desired_autostart;
            if host_mode == TunnelHostMode::Embedded {
                profile.installed = false;
                profile.started = false;
            }
            index
        } else {
            runtime_changed = true;
            profiles.push(TunnelRecord {
                configuration_id: None,
                provider: crate::TunnelProvider::Openai,
                profile_id: profile_id.to_owned(),
                name: desired_name,
                host_mode,
                autostart: desired_autostart,
                revision: 1,
                runtime_revision: 1,
                installed: false,
                started: false,
            });
            profiles.len() - 1
        };
        validate_catalog(&profiles)?;
        let profile_directory = profile_directory(store, profile_id);
        ensure_private_directory(&profile_directory)?;
        let health_path = profile_directory.join("readiness.json");
        if !health_path.exists() {
            atomic_private_write(&health_path, b"{}")?;
        }
        commit_profile_files(
            store,
            profile_id,
            &profiles,
            binding_plan.mutation,
            credentials,
            existing_binding.as_ref(),
            |profiles| store.write_json("tunnel.json", &profiles),
        )?;

        if host_mode == TunnelHostMode::Embedded {
            let health = read_tunnel_health(&health_path).ok();
            let configured_revision = profiles[index].effective_runtime_revision();
            let applied_revision = health.as_ref().and_then(|value| value.profile_revision);
            let server_before = server_before.expect("embedded Server observation");
            let (server_restart_required, next_action) = embedded_next_action(
                &server_before,
                configured_revision,
                applied_revision,
                runtime_changed,
            );
            let binding = tunnel_profile_binding(store, profile_id)?;
            return Ok(TunnelConfigurationResult {
                profile: snapshot(&profiles[index], &binding),
                owner_status: server_before,
                server_restart_required,
                next_action,
            });
        }

        if !start_standalone {
            let owner_status = ServiceManager::inspect(&standalone_spec).map_err(service_error)?;
            let next_action = if owner_status.running == Some(true) {
                if credential_changed {
                    TunnelConfigurationNextAction::RestartStandalone
                } else {
                    TunnelConfigurationNextAction::None
                }
            } else {
                TunnelConfigurationNextAction::StartStandalone
            };
            let binding = tunnel_profile_binding(store, profile_id)?;
            return Ok(TunnelConfigurationResult {
                profile: snapshot(&profiles[index], &binding),
                owner_status,
                server_restart_required: false,
                next_action,
            });
        }

        if tunnel_install_required(&standalone_before)? {
            crate::privilege::service_operation_spec(
                store,
                &environment,
                standalone_spec.clone(),
                ServiceOperation::Install,
                None,
            )
            .await?;
        }
        profiles[index].installed = true;
        store.write_json("tunnel.json", &profiles)?;
        let standalone_ready = ServiceManager::inspect(&standalone_spec).map_err(service_error)?;
        let operation = if credential_changed && standalone_ready.running == Some(true) {
            ServiceOperation::Restart
        } else {
            ServiceOperation::Start
        };
        if operation == ServiceOperation::Restart || standalone_ready.running != Some(true) {
            write_tunnel_health(&health_path, false, false)?;
        }
        let owner_status = crate::privilege::service_operation_spec(
            store,
            &environment,
            standalone_spec.clone(),
            operation,
            None,
        )
        .await?;
        wait_tunnel_readiness(&standalone_spec).await?;
        profiles[index].started = owner_status.running == Some(true);
        store.write_json("tunnel.json", &profiles)?;
        let binding = tunnel_profile_binding(store, profile_id)?;
        Ok(TunnelConfigurationResult {
            profile: snapshot(&profiles[index], &binding),
            owner_status,
            server_restart_required: false,
            next_action: TunnelConfigurationNextAction::None,
        })
    }

    pub fn tunnel_status(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
    ) -> SetupResultValue<TunnelRuntimeObservation> {
        let _lock = store.lock()?;
        let environment = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure this environment first"))?;
        let snapshot = tunnel_profile_snapshots(store)?
            .into_iter()
            .find(|profile| profile.profile_id == profile_id)
            .ok_or_else(|| diagnostic("tunnel_profile", "Tunnel profile does not exist"))?;
        let profile = tunnel_profiles(store)?
            .into_iter()
            .find(|profile| profile.profile_id == profile_id)
            .ok_or_else(|| diagnostic("tunnel_profile", "Tunnel profile does not exist"))?;
        let spec = tunnel_service_spec(store, &environment, profile_id)?;
        let owner_spec = if profile.host_mode == TunnelHostMode::Embedded {
            crate::service_spec(store, &environment, Component::Server)?
        } else {
            spec.clone()
        };
        let service_status = ServiceManager::inspect(&owner_spec).map_err(service_error)?;
        let health = read_tunnel_health(&spec.working_directory.join("readiness.json")).ok();
        let owner_running =
            service_status.ownership == Ownership::Owned && service_status.running == Some(true);
        let configured_revision = profile.effective_runtime_revision();
        let applied_revision = health.as_ref().and_then(|value| value.profile_revision);
        let server_restart_required = profile.host_mode == TunnelHostMode::Embedded
            && owner_running
            && applied_revision != Some(configured_revision);
        let (tunnel_ready, local_mcp_ready) = if owner_running && !server_restart_required {
            health
                .as_ref()
                .map(|value| (value.tunnel_ready, value.local_mcp_ready))
                .unwrap_or((false, false))
        } else {
            (false, false)
        };
        Ok(TunnelRuntimeObservation {
            profile_id: profile.profile_id,
            tunnel_id: snapshot.tunnel_id,
            service_status,
            host_mode: profile.host_mode,
            autostart: profile.autostart,
            ready: tunnel_ready && local_mcp_ready,
            tunnel_ready,
            local_mcp_ready,
            configured_revision,
            applied_revision,
            server_restart_required,
        })
    }

    pub async fn remove_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
    ) -> SetupResultValue<()> {
        self.remove_tunnel_at_revision(store, profile_id, None)
            .await
    }

    pub async fn remove_tunnel_at_revision(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        expected_revision: Option<u64>,
    ) -> SetupResultValue<()> {
        self.remove_tunnel_fenced(store, profile_id, expected_revision, None, None)
            .await
    }

    pub async fn remove_tunnel_fenced(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        expected_revision: Option<u64>,
        expected_environment_id: Option<&str>,
        expected_configuration_id: Option<&str>,
    ) -> SetupResultValue<()> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let environment = store
            .load_environment()?
            .ok_or_else(|| diagnostic("not_configured", "Configure this environment first"))?;
        if expected_environment_id.is_some_and(|expected| expected != environment.environment_id) {
            return Err(diagnostic(
                "environment_changed",
                "The saved Environment changed; refresh before removing its Tunnel",
            ));
        }
        let mut profiles = tunnel_profiles(store)?;
        let current = profiles
            .iter()
            .find(|profile| profile.profile_id == profile_id)
            .cloned();
        let pending =
            crate::cloudflare_tunnel::pending_cloudflare_profile_cleanup(store, profile_id)?;
        if let Some(pending) = &pending {
            if pending.environment_id != environment.environment_id {
                return Err(diagnostic(
                    "environment_changed",
                    "The unfinished Cloudflare removal belongs to another Environment",
                ));
            }
            if current
                .as_ref()
                .is_some_and(|profile| *profile != pending.profile)
            {
                return Err(diagnostic(
                    "tunnel_revision_stale",
                    "The unfinished removal cannot target a replacement Tunnel profile",
                ));
            }
        }
        let profile = current.or_else(|| pending.map(|pending| pending.profile));
        validate_expected_revision(profile.as_ref(), expected_revision)?;
        let profile =
            profile.ok_or_else(|| diagnostic("tunnel_profile", "Tunnel profile does not exist"))?;
        if expected_configuration_id.is_some_and(|expected| {
            profile.effective_configuration_id().as_deref() != Some(expected)
        }) {
            return Err(diagnostic(
                "tunnel_revision_stale",
                "The Tunnel profile was replaced; reload it before removal",
            ));
        }
        let spec =
            tunnel_service_spec_for_provider(store, &environment, profile_id, &profile.provider)?;
        let standalone = ServiceManager::inspect(&spec).map_err(service_error)?;
        if profile.host_mode == TunnelHostMode::Embedded {
            let server = ServiceManager::inspect(&crate::service_spec(
                store,
                &environment,
                Component::Server,
            )?)
            .map_err(service_error)?;
            if (server.ownership == Ownership::Owned && server.running != Some(false))
                || matches!(server.ownership, Ownership::Foreign | Ownership::Unknown)
            {
                return Err(SetupDiagnostic::new(
                    "tunnel_host_busy",
                    "Stop the owning Server before deleting an embedded Tunnel profile",
                    "Observe a clean Server stop; do not delete credentials from a live owner",
                ));
            }
            if standalone.ownership != Ownership::Absent {
                return Err(SetupDiagnostic::new(
                    "tunnel_owner",
                    "A standalone service still claims this embedded profile",
                    "Inspect and remove only the exact foreign or stale service before retrying",
                ));
            }
        } else {
            match standalone.ownership {
                Ownership::Owned => {
                    crate::privilege::service_operation_spec(
                        store,
                        &environment,
                        spec.clone(),
                        ServiceOperation::Stop,
                        None,
                    )
                    .await?;
                    let removed = crate::privilege::service_operation_spec(
                        store,
                        &environment,
                        spec.clone(),
                        ServiceOperation::Uninstall,
                        None,
                    )
                    .await?;
                    if removed.ownership != Ownership::Absent {
                        return Err(diagnostic(
                            "tunnel_remove_uncertain",
                            "Tunnel service removal has not been confirmed",
                        ));
                    }
                }
                Ownership::Absent => {}
                _ => {
                    return Err(diagnostic(
                        "tunnel_owner",
                        "The saved Tunnel service owner cannot be verified",
                    ))
                }
            }
        }
        let env = spec.env_file.as_ref().expect("Tunnel service env");
        let health = spec.working_directory.join("readiness.json");
        if profile.provider == TunnelProvider::Openai {
            if env.exists() {
                std::fs::remove_file(env).map_err(|_| SetupDiagnostic::io())?;
            }
            if health.exists() {
                std::fs::remove_file(&health).map_err(|_| SetupDiagnostic::io())?;
            }
        } else {
            // Persist the exact retirement before withdrawing the sole live
            // catalog record. A crash on either side remains safely resumable.
            crate::cloudflare_tunnel::begin_cloudflare_profile_cleanup(
                store,
                &_lock,
                &environment.environment_id,
                &profile,
            )?;
        }
        profiles.retain(|profile| profile.profile_id != profile_id);
        store.write_json("tunnel.json", &profiles)?;
        if profile.provider != TunnelProvider::Openai {
            crate::cloudflare_tunnel::complete_cloudflare_profile_cleanup(
                store,
                &_lock,
                &environment.environment_id,
                &profile,
            )?;
        }
        Ok(())
    }

    pub async fn control_tunnel(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        operation: ServiceOperation,
    ) -> SetupResultValue<service::ServiceStatus> {
        self.control_tunnel_with_observation(store, profile_id, operation, None)
            .await
    }

    /// Control only the exact Cloudflare configuration observed by the caller.
    /// The observation is rechecked under the setup lock before any service effect.
    pub async fn control_cloudflare_tunnel(
        &self,
        store: &EnvironmentStore,
        expected_environment_id: &str,
        expected_profile: &CloudflareTunnelRuntimeProfile,
        operation: ServiceOperation,
    ) -> SetupResultValue<service::ServiceStatus> {
        self.control_tunnel_with_observation(
            store,
            &expected_profile.profile_id,
            operation,
            Some((expected_environment_id, expected_profile)),
        )
        .await
    }

    async fn control_tunnel_with_observation(
        &self,
        store: &EnvironmentStore,
        profile_id: &str,
        operation: ServiceOperation,
        observation: Option<(&str, &CloudflareTunnelRuntimeProfile)>,
    ) -> SetupResultValue<service::ServiceStatus> {
        let _lock = store.lock()?;
        crate::ensure_upgrade_idle_under_lock(store)?;
        let environment = store.load_environment()?.ok_or_else(|| {
            diagnostic("not_configured", "Configure the Server environment first")
        })?;
        let profile = tunnel_profiles(store)?
            .into_iter()
            .find(|profile| profile.profile_id == profile_id);
        if let Some((expected_environment_id, expected_profile)) = observation {
            if environment.environment_id != expected_environment_id {
                return Err(diagnostic(
                    "environment_changed",
                    "The saved Environment changed; refresh before controlling its Tunnel",
                ));
            }
            validate_expected_revision(profile.as_ref(), Some(expected_profile.revision))?;
            let current = profile.as_ref().ok_or_else(|| {
                diagnostic(
                    "tunnel_revision_stale",
                    "The Tunnel profile no longer exists",
                )
            })?;
            if current.provider == TunnelProvider::Openai
                || current.provider != expected_profile.provider
                || current.host_mode != expected_profile.host_mode
                || current.effective_configuration_id().as_deref()
                    != Some(expected_profile.configuration_id.as_str())
            {
                return Err(diagnostic(
                    "tunnel_revision_stale",
                    "The Cloudflare profile was replaced; refresh before controlling it",
                ));
            }
        }
        let cloudflare_profile = profile
            .as_ref()
            .filter(|profile| profile.provider != TunnelProvider::Openai);
        let cloudflare = cloudflare_profile.is_some();
        crate::cloudflare_tunnel::ensure_cloudflare_profile_available(store, profile_id)?;
        require_standalone(store, profile_id)?;
        let spec = tunnel_service_spec(store, &environment, profile_id)?;
        let mut status = ServiceManager::inspect(&spec).map_err(service_error)?;
        if cloudflare_profile
            .as_ref()
            .is_some_and(|profile| !profile.autostart)
            && matches!(
                operation,
                ServiceOperation::Install | ServiceOperation::Start | ServiceOperation::Restart
            )
        {
            return Err(diagnostic("cloudflare_standalone_selection", "Select this Cloudflare profile for startup before installing or starting its separate service"));
        }
        let mut effective_operation = operation;
        if cloudflare
            && cfg!(windows)
            && environment.request.service_scope.is_system()
            && operation == ServiceOperation::Restart
        {
            crate::privilege::service_operation_spec(
                store,
                &environment,
                spec.clone(),
                ServiceOperation::Stop,
                None,
            )
            .await?;
            status = ServiceManager::inspect(&spec).map_err(service_error)?;
            if status.ownership != Ownership::Owned || status.running != Some(false) {
                return Err(diagnostic(
                    "tunnel_host_busy",
                    "Stop the owning Tunnel before applying its private runtime binding",
                ));
            }
            effective_operation = ServiceOperation::Start;
        }
        if cloudflare
            && matches!(
                operation,
                ServiceOperation::Install | ServiceOperation::Start | ServiceOperation::Restart
            )
        {
            if cfg!(windows)
                && environment.request.service_scope.is_system()
                && status.ownership == Ownership::Owned
                && status.running != Some(false)
            {
                return Err(diagnostic(
                    "tunnel_host_busy",
                    "Stop the owning Tunnel before applying its private runtime binding",
                ));
            }
            materialize_cloudflare_tunnel_profiles(store)?;
            if matches!(
                operation,
                ServiceOperation::Install | ServiceOperation::Start
            ) && tunnel_install_required(&status)?
            {
                status = crate::privilege::service_operation_spec(
                    store,
                    &environment,
                    spec.clone(),
                    ServiceOperation::Install,
                    None,
                )
                .await?;
            }
            #[cfg(windows)]
            if environment.request.service_scope.is_system() && status.ownership == Ownership::Owned
            {
                crate::service::grant_service_directory(&spec, &spec.working_directory)
                    .map_err(service_error)?;
            }
        }
        if status.ownership != Ownership::Owned {
            return Err(diagnostic(
                "tunnel_owner",
                "This Tunnel service is not owned by the saved profile",
            ));
        }
        if cloudflare && operation == ServiceOperation::Install {
            save_cloudflare_lifecycle(store, profile_id, &status)?;
            return Ok(status);
        }
        if operation == ServiceOperation::Restart
            || operation == ServiceOperation::Start && status.running != Some(true)
        {
            write_tunnel_health(&spec.working_directory.join("readiness.json"), false, false)?;
        }
        let result = crate::privilege::service_operation_spec(
            store,
            &environment,
            spec.clone(),
            effective_operation,
            None,
        )
        .await?;
        if matches!(
            operation,
            ServiceOperation::Start | ServiceOperation::Restart
        ) {
            wait_tunnel_readiness(&spec).await?;
        }
        if cloudflare {
            save_cloudflare_lifecycle(store, profile_id, &result)?;
        }
        Ok(result)
    }
}

fn save_cloudflare_lifecycle(
    store: &EnvironmentStore,
    profile_id: &str,
    status: &service::ServiceStatus,
) -> SetupResultValue<()> {
    let mut profiles = tunnel_profiles(store)?;
    let profile = profiles
        .iter_mut()
        .find(|profile| {
            profile.profile_id == profile_id && profile.provider != TunnelProvider::Openai
        })
        .ok_or_else(|| diagnostic("tunnel_profile", "Cloudflare profile no longer exists"))?;
    profile.installed = status.ownership == Ownership::Owned;
    profile.started = profile.installed && status.running == Some(true);
    validate_catalog(&profiles)?;
    store.write_json("tunnel.json", &profiles)
}

pub(crate) fn profile_host_mode(
    store: &EnvironmentStore,
    profile_id: &str,
) -> SetupResultValue<TunnelHostMode> {
    validate_id(profile_id)?;
    Ok(tunnel_profiles(store)?
        .into_iter()
        .find(|p| p.profile_id == profile_id)
        .map(|p| p.host_mode)
        .unwrap_or_default())
}

fn require_standalone(store: &EnvironmentStore, profile_id: &str) -> SetupResultValue<()> {
    if profile_host_mode(store, profile_id)? == TunnelHostMode::Embedded {
        return Err(SetupDiagnostic::new("tunnel_server_owned", "This Tunnel lifecycle belongs to the Server", "Control the owning Server; change the host explicitly after a clean stop before removing or starting a standalone service"));
    }
    Ok(())
}

fn tunnel_install_required(status: &service::ServiceStatus) -> SetupResultValue<bool> {
    match status.ownership {
        Ownership::Absent => Ok(true),
        Ownership::Owned if status.enabled == Some(true) => Ok(false),
        Ownership::Owned if status.enabled == Some(false) && status.running == Some(false) => Ok(true),
        Ownership::Owned => Err(diagnostic("tunnel_service_state_uncertain", "Tunnel boot state cannot be changed safely while the service is running or its state is unknown")),
        Ownership::Foreign => Err(diagnostic("tunnel_owner", "A different owner already controls this Tunnel service")),
        Ownership::Unknown => Err(diagnostic("tunnel_owner", "Tunnel service ownership cannot be verified")),
    }
}

/// Service heartbeat contains no Tunnel ID, key or authorization header. This
/// precreated file keeps the installer-selected ACL when a virtual account writes.
pub fn write_tunnel_health(
    path: &std::path::Path,
    tunnel_ready: bool,
    local_mcp_ready: bool,
) -> SetupResultValue<()> {
    write_tunnel_health_inner(path, None, tunnel_ready, local_mcp_ready)
}

/// Embedded owners bind their heartbeat to the exact startup configuration
/// revision. This is safe metadata and lets Desktop distinguish "restart needed"
/// from an ordinary readiness failure without inspecting process command lines.
pub fn write_embedded_tunnel_health(
    path: &std::path::Path,
    profile_revision: u64,
    tunnel_ready: bool,
    local_mcp_ready: bool,
) -> SetupResultValue<()> {
    write_tunnel_health_inner(
        path,
        Some(profile_revision.max(1)),
        tunnel_ready,
        local_mcp_ready,
    )
}

fn write_tunnel_health_inner(
    path: &std::path::Path,
    profile_revision: Option<u64>,
    tunnel_ready: bool,
    local_mcp_ready: bool,
) -> SetupResultValue<()> {
    use std::io::Write;
    let metadata = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(windows)]
    crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| SetupDiagnostic::io())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::OpenOptionsExt;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
            || metadata.nlink() != 1
        {
            return Err(SetupDiagnostic::io());
        }
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path).map_err(|_| SetupDiagnostic::io())?;
    fs2::FileExt::try_lock_exclusive(&file).map_err(|_| SetupDiagnostic::io())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| SetupDiagnostic::io())?
        .as_millis();
    let value = serde_json::json!({
        "schema_version": 1,
        "observed_at_ms": now,
        "service_pid": std::process::id(),
        "profile_revision": profile_revision,
        "tunnel_ready": tunnel_ready,
        "local_mcp_ready": local_mcp_ready
    });
    file.set_len(0)
        .and_then(|_| file.write_all(value.to_string().as_bytes()))
        .and_then(|_| file.sync_all())
        .map_err(|_| SetupDiagnostic::io())
}

pub(crate) async fn wait_tunnel_readiness(spec: &ServiceSpec) -> SetupResultValue<()> {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(65);
    let path = spec.working_directory.join("readiness.json");
    loop {
        let status = ServiceManager::inspect(spec).map_err(service_error)?;
        if status.ownership != Ownership::Owned {
            return Err(diagnostic(
                "tunnel_owner",
                "Tunnel service ownership changed while checking readiness",
            ));
        }
        if status.running == Some(true) {
            let value = read_health(&path);
            if value.is_ok_and(|(tunnel, mcp)| tunnel && mcp) {
                return Ok(());
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(diagnostic("tunnel_not_ready", "The Tunnel service is configured but Tunnel and local MCP readiness have not both been verified"));
        }
        tokio::time::sleep_until(
            (tokio::time::Instant::now() + std::time::Duration::from_millis(250)).min(deadline),
        )
        .await;
    }
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct TunnelHealthObservation {
    pub tunnel_ready: bool,
    pub local_mcp_ready: bool,
    pub profile_revision: Option<u64>,
}

pub(crate) fn read_health(path: &std::path::Path) -> SetupResultValue<(bool, bool)> {
    let value = read_tunnel_health(path)?;
    Ok((value.tunnel_ready, value.local_mcp_ready))
}

pub(crate) fn read_tunnel_health(
    path: &std::path::Path,
) -> SetupResultValue<TunnelHealthObservation> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path).map_err(|_| SetupDiagnostic::io())?;
    if !meta.is_file() || meta.is_symlink() || meta.len() > 4096 {
        return Err(SetupDiagnostic::io());
    }
    #[cfg(windows)]
    crate::runtime_entry::validate_windows_env_acl(path).map_err(|_| SetupDiagnostic::io())?;
    let file = std::fs::File::open(path).map_err(|_| SetupDiagnostic::io())?;
    fs2::FileExt::try_lock_shared(&file).map_err(|_| SetupDiagnostic::io())?;
    let mut bytes = Vec::new();
    file.take(4097)
        .read_to_end(&mut bytes)
        .map_err(|_| SetupDiagnostic::io())?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| SetupDiagnostic::io())?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| SetupDiagnostic::io())?
        .as_millis();
    let time = value
        .get("observed_at_ms")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as u128;
    let fresh = time <= now && now.saturating_sub(time) < 7000;
    Ok(TunnelHealthObservation {
        tunnel_ready: fresh
            && value
                .get("tunnel_ready")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        local_mcp_ready: fresh
            && value
                .get("local_mcp_ready")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        profile_revision: value
            .get("profile_revision")
            .and_then(serde_json::Value::as_u64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(ownership: Ownership, running: Option<bool>) -> service::ServiceStatus {
        service::ServiceStatus {
            id: "webcodex".into(),
            ownership,
            installed: ownership != Ownership::Absent,
            enabled: None,
            running,
            detail: None,
        }
    }

    fn record(revision: u64) -> TunnelRecord {
        TunnelRecord {
            configuration_id: None,
            provider: crate::TunnelProvider::Openai,
            profile_id: "work".into(),
            name: "Work".into(),
            host_mode: TunnelHostMode::Embedded,
            autostart: true,
            revision,
            runtime_revision: revision,
            installed: false,
            started: false,
        }
    }

    fn binding(tunnel_id: &str, api_key: &str) -> TunnelProfileBinding {
        TunnelProfileBinding {
            tunnel_id: Secret::new(tunnel_id.to_owned()),
            api_key: Secret::new(api_key.to_owned()),
            local_token: Secret::new("local-token".to_owned()),
            proxy: Some(Secret::new("http://proxy.test".to_owned())),
        }
    }

    fn credentials(tunnel_id: &str, api_key: &str) -> TunnelCredentials {
        TunnelCredentials {
            tunnel_id: Secret::new(tunnel_id.to_owned()),
            api_key: Secret::new(api_key.to_owned()),
        }
    }

    #[test]
    fn credential_rotation_requires_a_current_revision_and_never_changes_identity() {
        let saved = binding("tunnel_work", "old-key");
        let same = credentials("tunnel_work", "old-key");
        assert_eq!(
            plan_tunnel_binding(Some(&saved), Some(&same), None)
                .unwrap()
                .mutation,
            TunnelBindingMutation::None
        );

        let rotated = credentials("tunnel_work", "new-key");
        assert_eq!(
            plan_tunnel_binding(Some(&saved), Some(&rotated), Some(7))
                .unwrap()
                .mutation,
            TunnelBindingMutation::RotateCredential
        );
        assert_eq!(
            plan_tunnel_binding(Some(&saved), Some(&rotated), None)
                .unwrap_err()
                .code,
            "tunnel_credential_rotation_requires_revision"
        );

        let foreign = credentials("tunnel_other", "new-key");
        assert_eq!(
            plan_tunnel_binding(Some(&saved), Some(&foreign), Some(7))
                .unwrap_err()
                .code,
            "tunnel_binding_conflict"
        );
    }

    #[test]
    fn rotating_a_credential_preserves_local_mcp_binding_and_proxy() {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let server_dir = store.root().join("server");
        ensure_private_directory(&server_dir).unwrap();
        atomic_private_write(
            &server_dir.join("webcodex.env"),
            b"WEBCODEX_ADDR=127.0.0.1:62645\nWEBCODEX_TOKEN=bootstrap-token\n",
        )
        .unwrap();
        let profile_dir = server_dir.join("tunnels/work");
        ensure_private_directory(&profile_dir).unwrap();
        atomic_private_write(
            &profile_dir.join("webcodex.env"),
            b"WEBCODEX_ADDR=127.0.0.1:62645\nWEBCODEX_TOKEN=local-token\nCONTROL_PLANE_TUNNEL_ID=tunnel_work\nCONTROL_PLANE_API_KEY=old-private-key\nWEBCODEX_TUNNEL_PROFILE_ID=work\nWEBCODEX_TUNNEL_PROXY=http://proxy.example:7890\n",
        )
        .unwrap();
        let previous = tunnel_profile_binding(&store, "work").unwrap();
        let rotated = credentials("tunnel_work", "new-private-key");
        write_profile_binding(&store, "work", &rotated, Some(&previous)).unwrap();

        let saved = tunnel_profile_binding(&store, "work").unwrap();
        assert_eq!(saved.tunnel_id.expose(), "tunnel_work");
        assert_eq!(saved.api_key.expose(), "new-private-key");
        assert_eq!(saved.local_token.expose(), "local-token");
        assert_eq!(
            saved.proxy.as_ref().map(Secret::expose),
            Some("http://proxy.example:7890")
        );
        let raw = read_secret(&profile_dir.join("webcodex.env")).unwrap();
        assert!(!raw.expose().contains("old-private-key"));
        let debug = format!("{saved:?}");
        for secret in ["new-private-key", "local-token", "proxy.example"] {
            assert!(!debug.contains(secret));
        }
    }

    #[test]
    fn failed_catalog_commit_restores_rotated_binding_and_removes_new_binding() {
        let temp = crate::test_tempdir().unwrap();
        let store = EnvironmentStore::open(temp.path().join("environment")).unwrap();
        let server_dir = store.root().join("server");
        ensure_private_directory(&server_dir).unwrap();
        atomic_private_write(
            &server_dir.join("webcodex.env"),
            b"WEBCODEX_ADDR=127.0.0.1:62645\nWEBCODEX_TOKEN=bootstrap-token\n",
        )
        .unwrap();
        let profile_dir = server_dir.join("tunnels/work");
        ensure_private_directory(&profile_dir).unwrap();
        atomic_private_write(
            &profile_dir.join("webcodex.env"),
            b"WEBCODEX_ADDR=127.0.0.1:62645\nWEBCODEX_TOKEN=local-token\nCONTROL_PLANE_TUNNEL_ID=tunnel_work\nCONTROL_PLANE_API_KEY=old-private-key\nWEBCODEX_TUNNEL_PROFILE_ID=work\nWEBCODEX_TUNNEL_PROXY=http://proxy.example:7890\n",
        )
        .unwrap();
        let previous = tunnel_profile_binding(&store, "work").unwrap();
        let rotated = credentials("tunnel_work", "new-private-key");
        let failure = commit_profile_files(
            &store,
            "work",
            &[record(8)],
            TunnelBindingMutation::RotateCredential,
            Some(&rotated),
            Some(&previous),
            |_| {
                Err(diagnostic(
                    "catalog_write_failed",
                    "fixture catalog failure",
                ))
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "catalog_write_failed");
        let restored = tunnel_profile_binding(&store, "work").unwrap();
        assert_eq!(restored.api_key.expose(), "old-private-key");
        assert_eq!(restored.local_token.expose(), "local-token");
        assert_eq!(
            restored.proxy.as_ref().map(Secret::expose),
            Some("http://proxy.example:7890")
        );

        let fresh = credentials("tunnel_new", "new-profile-key");
        let failure = commit_profile_files(
            &store,
            "new-profile",
            &[TunnelRecord {
                configuration_id: None,
                provider: crate::TunnelProvider::Openai,
                profile_id: "new-profile".into(),
                name: "New".into(),
                host_mode: TunnelHostMode::Embedded,
                autostart: true,
                revision: 1,
                runtime_revision: 1,
                installed: false,
                started: false,
            }],
            TunnelBindingMutation::Create,
            Some(&fresh),
            None,
            |_| {
                Err(diagnostic(
                    "catalog_write_failed",
                    "fixture catalog failure",
                ))
            },
        )
        .unwrap_err();
        assert_eq!(failure.code, "catalog_write_failed");
        assert!(!profile_directory(&store, "new-profile")
            .join("webcodex.env")
            .exists());
    }

    #[test]
    fn first_binding_requires_credentials_and_records_creation() {
        assert_eq!(
            plan_tunnel_binding(None, None, None).unwrap_err().code,
            "tunnel_credentials"
        );
        let candidate = credentials("tunnel_work", "first-key");
        let plan = plan_tunnel_binding(None, Some(&candidate), None).unwrap();
        assert_eq!(plan.tunnel_id, "tunnel_work");
        assert_eq!(plan.mutation, TunnelBindingMutation::Create);
    }

    #[test]
    fn revision_fence_rejects_stale_and_deleted_profiles() {
        let profile = record(7);
        assert!(validate_expected_revision(Some(&profile), Some(7)).is_ok());
        assert_eq!(
            validate_expected_revision(Some(&profile), Some(6))
                .unwrap_err()
                .code,
            "tunnel_revision_stale"
        );
        assert_eq!(
            validate_expected_revision(None, Some(7)).unwrap_err().code,
            "tunnel_revision_stale"
        );
        assert!(validate_expected_revision(None, None).is_ok());
    }

    #[test]
    fn standalone_profiles_reject_a_false_server_owned_autostart_setting() {
        assert!(resolve_autostart(TunnelHostMode::Standalone, None, Some(false)).unwrap());
        assert!(resolve_autostart(TunnelHostMode::Standalone, Some(true), None).unwrap());
        assert_eq!(
            resolve_autostart(TunnelHostMode::Standalone, Some(false), Some(true))
                .unwrap_err()
                .code,
            "tunnel_autostart_unsupported"
        );
        assert!(!resolve_autostart(TunnelHostMode::Embedded, Some(false), Some(true)).unwrap());
    }

    #[test]
    fn embedded_result_requires_only_one_explicit_server_lifecycle_action() {
        let running = status(Ownership::Owned, Some(true));
        assert_eq!(
            embedded_next_action(&running, 3, Some(2), true),
            (true, TunnelConfigurationNextAction::RestartServer)
        );
        assert_eq!(
            embedded_next_action(&running, 3, Some(3), false),
            (false, TunnelConfigurationNextAction::None)
        );
        let stopped = status(Ownership::Owned, Some(false));
        assert_eq!(
            embedded_next_action(&stopped, 3, None, true),
            (false, TunnelConfigurationNextAction::StartServer)
        );
        let absent = status(Ownership::Absent, None);
        assert_eq!(
            embedded_next_action(&absent, 3, None, true),
            (false, TunnelConfigurationNextAction::StartServer)
        );
    }

    #[test]
    fn running_owned_tunnel_skips_reinstall_and_ambiguous_state_fails_closed() {
        let mut status = service::ServiceStatus {
            id: "webcodex-tunnel-main".into(),
            ownership: Ownership::Owned,
            installed: true,
            enabled: Some(true),
            running: Some(true),
            detail: None,
        };
        assert!(!tunnel_install_required(&status).unwrap());
        status.running = Some(false);
        status.enabled = Some(false);
        assert!(tunnel_install_required(&status).unwrap());
        status.running = Some(true);
        assert_eq!(
            tunnel_install_required(&status).unwrap_err().code,
            "tunnel_service_state_uncertain"
        );
        status.ownership = Ownership::Foreign;
        assert_eq!(
            tunnel_install_required(&status).unwrap_err().code,
            "tunnel_owner"
        );
    }
}
