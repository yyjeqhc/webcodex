//! Canonical desired Cloudflare configuration. Credential files are private,
//! immutable runtime capabilities; safe projections never contain their paths.
use crate::native::{env_value, service_error};
use crate::storage::{atomic_private_write, ensure_private_directory};
use crate::tunnel::{next_revision, unique_env_value, validate_catalog, validate_id};
use crate::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TunnelProvider {
    #[default]
    Openai,
    CloudflareNamed {
        public_origin: String,
        tunnel_id: String,
    },
    CloudflareQuick,
}

/// Write-only credentials. None retains this exact profile's saved token.
pub struct CloudflareTunnelProfileRequest<'a> {
    pub profile_id: &'a str,
    pub name: Option<&'a str>,
    pub host_mode: TunnelHostMode,
    pub autostart: bool,
    pub expected_revision: Option<u64>,
    pub provider: TunnelProvider,
    pub token: Option<&'a Secret>,
    pub ingress_port: Option<u16>,
}

/// Private launch input, deliberately neither serializable nor Debug-printable.
pub struct CloudflareTunnelRuntimeProfile {
    pub profile_id: String,
    pub configuration_id: String,
    pub provider: TunnelProvider,
    pub host_mode: TunnelHostMode,
    pub autostart: bool,
    pub revision: u64,
    pub runtime_revision: u64,
    pub owner_username: String,
    pub runner_client_id: Option<String>,
    pub ingress_port: u16,
    pub local_target: String,
    pub local_server_url: String,
    pub bootstrap_token: Secret,
    pub token_file: Option<PathBuf>,
    pub readiness_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct IngressConfiguration {
    port: u16,
}

fn invalid(code: &str, message: &str) -> SetupDiagnostic {
    SetupDiagnostic::new(
        code,
        message,
        "Inspect the selected Cloudflare profile and its owning Server; credentials are write-only",
    )
}

const MAX_REVISION_TOMBSTONE_BYTES: usize = 8 * 1024;

#[derive(Serialize, Deserialize)]
struct RevisionTombstone {
    last_revision: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_cleanup: Option<CloudflareProfileCleanup>,
}

/// A retired configuration is retained only to resume its exact private cleanup.
/// The live catalog remains the sole configuration authority; this cannot launch.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct CloudflareProfileCleanup {
    pub environment_id: String,
    pub profile: TunnelRecord,
}

fn cleanup_required() -> SetupDiagnostic {
    invalid(
        "tunnel_profile_recovery_required",
        "Cloudflare profile removal is incomplete; retry removal with its original identity",
    )
}

fn parse_revision_tombstone(id: &str, bytes: &[u8]) -> SetupResultValue<RevisionTombstone> {
    if bytes.len() > MAX_REVISION_TOMBSTONE_BYTES {
        return Err(invalid(
            "tunnel_revision",
            "Cloudflare profile revision history exceeds its bounded size",
        ));
    }
    let saved: RevisionTombstone = serde_json::from_slice(bytes).map_err(|_| {
        invalid(
            "tunnel_revision",
            "Cloudflare profile revision history is invalid",
        )
    })?;
    if let Some(cleanup) = &saved.pending_cleanup {
        validate_catalog(std::slice::from_ref(&cleanup.profile))?;
        if cleanup.environment_id.is_empty()
            || cleanup.environment_id.len() > 256
            || cleanup.environment_id.chars().any(char::is_control)
            || cleanup.profile.profile_id != id
            || cleanup.profile.provider == TunnelProvider::Openai
            || cleanup.profile.revision.max(1) != saved.last_revision
        {
            return Err(invalid(
                "tunnel_revision",
                "Cloudflare cleanup identity is invalid",
            ));
        }
    }
    Ok(saved)
}

fn read_revision_tombstone(
    store: &EnvironmentStore,
    id: &str,
) -> SetupResultValue<Option<RevisionTombstone>> {
    validate_id(id)?;
    let path = profile_directory(store, id).join("revision.json");
    crate::storage::validate_directory_ancestors(path.parent().ok_or_else(SetupDiagnostic::io)?)?;
    if !path.try_exists().map_err(|_| SetupDiagnostic::io())? {
        return Ok(None);
    }
    parse_revision_tombstone(id, &crate::storage::read_private(&path)?).map(Some)
}

fn write_revision_tombstone(
    store: &EnvironmentStore,
    id: &str,
    saved: &RevisionTombstone,
) -> SetupResultValue<()> {
    let bytes = serde_json::to_vec(saved).map_err(|_| SetupDiagnostic::io())?;
    parse_revision_tombstone(id, &bytes)?;
    atomic_private_write(&profile_directory(store, id).join("revision.json"), &bytes)
}

pub(crate) fn pending_cloudflare_profile_cleanup(
    store: &EnvironmentStore,
    id: &str,
) -> SetupResultValue<Option<CloudflareProfileCleanup>> {
    Ok(read_revision_tombstone(store, id)?.and_then(|saved| saved.pending_cleanup))
}

pub(crate) fn ensure_cloudflare_profile_available(
    store: &EnvironmentStore,
    id: &str,
) -> SetupResultValue<()> {
    if pending_cloudflare_profile_cleanup(store, id)?.is_some() {
        return Err(cleanup_required());
    }
    Ok(())
}

pub(crate) fn next_cloudflare_profile_revision(
    store: &EnvironmentStore,
    id: &str,
) -> SetupResultValue<u64> {
    match read_revision_tombstone(store, id)? {
        None => Ok(1),
        Some(saved) if saved.pending_cleanup.is_some() => Err(cleanup_required()),
        Some(saved) => next_revision(saved.last_revision),
    }
}

pub(crate) fn retain_cloudflare_profile_revision(
    store: &EnvironmentStore,
    profile: &TunnelRecord,
) -> SetupResultValue<()> {
    let previous = next_cloudflare_profile_revision(store, &profile.profile_id)?.saturating_sub(1);
    let saved = RevisionTombstone {
        last_revision: previous.max(profile.revision.max(1)),
        pending_cleanup: None,
    };
    write_revision_tombstone(store, &profile.profile_id, &saved)
}

/// Called under the setup lock only after the old lifecycle owner is stopped.
/// Persist the cleanup intent before the caller withdraws the live catalog entry.
pub(crate) fn begin_cloudflare_profile_cleanup(
    store: &EnvironmentStore,
    _lock: &EnvironmentLock,
    environment_id: &str,
    profile: &TunnelRecord,
) -> SetupResultValue<()> {
    if let Some(pending) = pending_cloudflare_profile_cleanup(store, &profile.profile_id)? {
        return if pending.environment_id == environment_id && pending.profile == *profile {
            Ok(())
        } else {
            Err(invalid(
                "tunnel_revision_stale",
                "A different Cloudflare configuration has an unfinished removal",
            ))
        };
    }
    retain_cloudflare_profile_revision(store, profile)?;
    let mut saved =
        read_revision_tombstone(store, &profile.profile_id)?.ok_or_else(cleanup_required)?;
    saved.pending_cleanup = Some(CloudflareProfileCleanup {
        environment_id: environment_id.to_owned(),
        profile: profile.clone(),
    });
    write_revision_tombstone(store, &profile.profile_id, &saved)
}

fn cleanup_regular_file(path: &Path) -> SetupResultValue<Option<PathBuf>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if !crate::storage::is_link(&metadata) && metadata.is_file() => {
            Ok(Some(path.to_path_buf()))
        }
        Ok(_) => Err(invalid(
            "cloudflare_token",
            "Cloudflare credential cleanup found an unexpected private entry",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(SetupDiagnostic::io()),
    }
}

/// Reentrant cleanup for a withdrawn catalog record. Unknown artifacts remain
/// untouched, and any failure retains the exact retirement fact for a retry.
pub(crate) fn complete_cloudflare_profile_cleanup(
    store: &EnvironmentStore,
    _lock: &EnvironmentLock,
    environment_id: &str,
    profile: &TunnelRecord,
) -> SetupResultValue<()> {
    let id = &profile.profile_id;
    let mut saved = read_revision_tombstone(store, id)?.ok_or_else(cleanup_required)?;
    let pending = saved
        .pending_cleanup
        .as_ref()
        .ok_or_else(cleanup_required)?;
    if pending.environment_id != environment_id || pending.profile != *profile {
        return Err(invalid(
            "tunnel_revision_stale",
            "The Cloudflare cleanup no longer belongs to the observed configuration",
        ));
    }
    if tunnel_profiles(store)?
        .iter()
        .any(|current| current.profile_id == *id)
    {
        return Err(cleanup_required());
    }
    let directory = profile_directory(store, id);
    crate::storage::validate_existing_private_directory(&directory)?;
    // Atomic writes can leave a private .setup file after a crash. Its contents
    // are unknown, so retain the retirement instead of claiming cleanup finished.
    let entries = std::fs::read_dir(&directory)
        .map_err(|_| SetupDiagnostic::io())?
        .take(7)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| SetupDiagnostic::io())?;
    if entries.len() > 6 {
        return Err(invalid(
            "cloudflare_token_capacity",
            "Cloudflare profile cleanup exceeds its bounded artifact capacity",
        ));
    }
    let mut files = Vec::new();
    for entry in entries {
        let name = entry.file_name();
        match name.to_str() {
            Some("webcodex.env" | "readiness.json" | "runtime.json") => {
                if let Some(path) = cleanup_regular_file(&entry.path())? {
                    files.push(path);
                }
            }
            Some("tokens") => {} // Validated independently with its own bound below.
            Some("revision.json") => {
                let _ = cleanup_regular_file(&entry.path())?;
            }
            Some(name) if name == service::SERVICE_LOG_NAME => {
                // Fixed lifecycle events remain useful after removal and contain no credentials.
                let _ = cleanup_regular_file(&entry.path())?;
            }
            _ => {
                return Err(invalid(
                    "cloudflare_token",
                    "Cloudflare profile cleanup found an unexpected private entry",
                ));
            }
        }
    }
    let tokens = directory.join("tokens");
    let has_tokens = match std::fs::symlink_metadata(&tokens) {
        Ok(_) => {
            crate::storage::validate_existing_private_directory(&tokens)?;
            let entries = std::fs::read_dir(&tokens)
                .map_err(|_| SetupDiagnostic::io())?
                .take(65)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| SetupDiagnostic::io())?;
            if entries.len() > 64 {
                return Err(invalid(
                    "cloudflare_token_capacity",
                    "Cloudflare credential cleanup exceeds its bounded capacity",
                ));
            }
            for entry in entries {
                if uuid::Uuid::parse_str(&entry.file_name().to_string_lossy()).is_err() {
                    return Err(invalid(
                        "cloudflare_token",
                        "Cloudflare credential cleanup found an unexpected private entry",
                    ));
                }
                if let Some(path) = cleanup_regular_file(&entry.path())? {
                    files.push(path);
                }
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => return Err(SetupDiagnostic::io()),
    };
    // Update the existing Server materialization before removing capabilities;
    // the retirement guard also fences an older service-owned runtime binding.
    materialize_cloudflare_tunnel_profiles(store)?;
    for path in files {
        std::fs::remove_file(path).map_err(|_| SetupDiagnostic::io())?;
    }
    if has_tokens {
        std::fs::remove_dir(tokens).map_err(|_| SetupDiagnostic::io())?;
    }
    saved.pending_cleanup = None;
    write_revision_tombstone(store, id, &saved)
}

pub(crate) fn validate_provider(provider: &TunnelProvider) -> SetupResultValue<()> {
    if let TunnelProvider::CloudflareNamed {
        public_origin,
        tunnel_id,
    } = provider
    {
        let parsed = url::Url::parse(public_origin)
            .map_err(|_| invalid("cloudflare_origin", "A fixed HTTPS origin is required"))?;
        if public_origin.len() > 2048
            || parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != "/"
            || parsed.origin().ascii_serialization() != *public_origin
            || public_origin.chars().any(char::is_control)
        {
            return Err(invalid(
                "cloudflare_origin",
                "A canonical HTTPS origin without credentials, path, query, fragment or trailing slash is required",
            ));
        }
        if tunnel_id.is_empty()
            || tunnel_id.len() > 256
            || !tunnel_id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(invalid(
                "cloudflare_identity",
                "Cloudflare Tunnel identity is invalid",
            ));
        }
    }
    Ok(())
}

fn same_provider_identity(left: &TunnelProvider, right: &TunnelProvider) -> bool {
    match (left, right) {
        (
            TunnelProvider::CloudflareNamed {
                tunnel_id: left, ..
            },
            TunnelProvider::CloudflareNamed {
                tunnel_id: right, ..
            },
        ) => left == right,
        (TunnelProvider::CloudflareQuick, TunnelProvider::CloudflareQuick) => true,
        _ => false,
    }
}

pub(crate) fn validate_selection(profiles: &[TunnelRecord]) -> SetupResultValue<()> {
    // A dedicated ingress belongs to one Server. Saved alternatives may coexist,
    // but startup can select only one Cloudflare origin for that Server.
    if profiles
        .iter()
        .filter(|profile| profile.provider != TunnelProvider::Openai && profile.autostart)
        .count()
        > 1
    {
        return Err(invalid(
            "cloudflare_selection_conflict",
            "Only one Cloudflare profile can be selected for startup",
        ));
    }
    let mut ids = std::collections::BTreeSet::new();
    for profile in profiles {
        if profile.provider != TunnelProvider::Openai
            && profile
                .configuration_id
                .as_ref()
                .is_some_and(|id| uuid::Uuid::parse_str(id).is_err())
        {
            return Err(invalid(
                "tunnel_profile_configuration",
                "Cloudflare profile incarnation is invalid",
            ));
        }
        if profile.provider != TunnelProvider::Openai
            && profile.host_mode == TunnelHostMode::Standalone
            && !profile.autostart
            && (profile.installed || profile.started)
        {
            return Err(invalid("cloudflare_standalone_selection", "An installed Cloudflare service retains its native startup lifecycle; stop and uninstall it before deselecting it"));
        }
        if let TunnelProvider::CloudflareNamed { tunnel_id, .. } = &profile.provider {
            if !ids.insert(tunnel_id) {
                return Err(invalid(
                    "tunnel_identity_duplicate",
                    "A Cloudflare Tunnel identity is bound to more than one profile",
                ));
            }
        }
    }
    Ok(())
}

pub fn cloudflare_ingress_port(store: &EnvironmentStore) -> SetupResultValue<Option<u16>> {
    let configuration: Option<IngressConfiguration> =
        store.read_json("server/cloudflare-ingress.json")?;
    configuration
        .map(|config| {
            if config.port == 0 {
                Err(invalid(
                    "cloudflare_ingress",
                    "The saved Cloudflare ingress port is invalid",
                ))
            } else {
                Ok(config.port)
            }
        })
        .transpose()
}

fn profile_directory(store: &EnvironmentStore, id: &str) -> PathBuf {
    store.root().join("server/tunnels").join(id)
}

fn validate_token(token: &Secret) -> SetupResultValue<()> {
    if token.expose().is_empty()
        || token.expose().len() > 8192
        || !token.expose().bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(invalid(
            "cloudflare_token",
            "Cloudflare Tunnel token is invalid",
        ));
    }
    Ok(())
}

fn binding_token_file(
    store: &EnvironmentStore,
    record: &TunnelRecord,
) -> SetupResultValue<Option<PathBuf>> {
    let directory = profile_directory(store, &record.profile_id);
    crate::storage::validate_directory_ancestors(&directory)?;
    let binding = read_secret(&directory.join("webcodex.env"))?;
    if binding.expose().len() > 32 * 1024
        || unique_env_value(binding.expose(), "WEBCODEX_TUNNEL_PROFILE_ID")?.as_deref()
            != Some(record.profile_id.as_str())
    {
        return Err(invalid(
            "tunnel_profile_configuration",
            "Cloudflare private binding identity is invalid",
        ));
    }
    let provider = unique_env_value(binding.expose(), "WEBCODEX_TUNNEL_PROVIDER")?;
    let token_ref = unique_env_value(binding.expose(), "CLOUDFLARE_TUNNEL_TOKEN_REF")?;
    match &record.provider {
        TunnelProvider::CloudflareNamed { .. }
            if provider.as_deref() == Some("cloudflare_named") =>
        {
            let reference = token_ref.ok_or_else(|| {
                invalid(
                    "cloudflare_token",
                    "Cloudflare private token reference is missing",
                )
            })?;
            // Only opaque, generated basename references are admitted; the binding
            // cannot redirect a service to an arbitrary credential file.
            if uuid::Uuid::parse_str(&reference).is_err() || reference.len() != 36 {
                return Err(invalid(
                    "cloudflare_token",
                    "Cloudflare private token reference is invalid",
                ));
            }
            let path = directory.join("tokens").join(reference);
            crate::storage::validate_directory_ancestors(
                path.parent().ok_or_else(SetupDiagnostic::io)?,
            )?;
            let token = read_secret(&path)?;
            validate_token(&token)?;
            Ok(Some(path))
        }
        TunnelProvider::CloudflareQuick
            if provider.as_deref() == Some("cloudflare_quick") && token_ref.is_none() =>
        {
            Ok(None)
        }
        _ => Err(invalid(
            "tunnel_profile_configuration",
            "Cloudflare private binding provider is invalid",
        )),
    }
}

pub(crate) fn snapshot(
    store: &EnvironmentStore,
    profile: &TunnelRecord,
) -> SetupResultValue<TunnelProfileSnapshot> {
    let token_file = binding_token_file(store, profile)?;
    let ingress_port = cloudflare_ingress_port(store)?.ok_or_else(|| {
        invalid(
            "cloudflare_ingress",
            "The Server's dedicated Cloudflare ingress is missing",
        )
    })?;
    Ok(TunnelProfileSnapshot {
        profile_id: profile.profile_id.clone(),
        configuration_id: profile.effective_configuration_id(),
        provider: profile.provider.clone(),
        ingress_port: Some(ingress_port),
        name: profile.display_name().to_owned(),
        tunnel_id: match &profile.provider {
            TunnelProvider::CloudflareNamed { tunnel_id, .. } => tunnel_id.clone(),
            _ => String::new(),
        },
        credential_present: token_file.is_some(),
        host_mode: profile.host_mode,
        autostart: profile.autostart,
        revision: profile.revision.max(1),
        installed: profile.installed,
        started: profile.started,
    })
}

pub fn cloudflare_tunnel_profile(
    store: &EnvironmentStore,
    profile_id: &str,
) -> SetupResultValue<CloudflareTunnelRuntimeProfile> {
    validate_id(profile_id)?;
    ensure_cloudflare_profile_available(store, profile_id)?;
    let profiles = tunnel_profiles(store)?;
    validate_catalog(&profiles)?;
    let profile = profiles
        .into_iter()
        .find(|profile| {
            profile.profile_id == profile_id && profile.provider != TunnelProvider::Openai
        })
        .ok_or_else(|| invalid("tunnel_profile", "Cloudflare profile does not exist"))?;
    let environment = store
        .load_environment()?
        .ok_or_else(|| invalid("not_configured", "Configure the Server environment first"))?;
    if !environment.request.local_server() {
        return Err(invalid(
            "tunnel_local_server",
            "Cloudflare ingress requires a local Server",
        ));
    }
    let owner_username = environment
        .username
        .filter(|name| !name.is_empty() && name.len() <= 256 && !name.chars().any(char::is_control))
        .ok_or_else(|| {
            invalid(
                "cloudflare_owner",
                "The authenticated Server owner is unavailable",
            )
        })?;
    let ingress_port = cloudflare_ingress_port(store)?.ok_or_else(|| {
        invalid(
            "cloudflare_ingress",
            "The Server's dedicated Cloudflare ingress is missing",
        )
    })?;
    let token_file = binding_token_file(store, &profile)?;
    let (local_server_url, bootstrap_token) = server_control_binding(store)?;
    let configuration_id = profile
        .effective_configuration_id()
        .expect("Cloudflare configuration identity");
    Ok(CloudflareTunnelRuntimeProfile {
        local_target: format!("http://127.0.0.1:{ingress_port}"),
        local_server_url,
        bootstrap_token,
        readiness_path: profile_directory(store, profile_id).join("readiness.json"),
        runtime_revision: profile.effective_runtime_revision(),
        configuration_id,
        profile_id: profile.profile_id,
        provider: profile.provider,
        host_mode: profile.host_mode,
        autostart: profile.autostart,
        revision: profile.revision.max(1),
        owner_username,
        runner_client_id: environment.runner_client_id,
        ingress_port,
        token_file,
    })
}

/// Load saved alternatives as well as the selected profile; the caller owns the
/// exclusive runtime selection fence and the embedded/standalone lifecycle.
pub fn cloudflare_tunnel_profiles(
    root: &Path,
) -> SetupResultValue<Vec<CloudflareTunnelRuntimeProfile>> {
    let store = EnvironmentStore::open(root.to_path_buf())?;
    let profiles = tunnel_profiles(&store)?;
    validate_catalog(&profiles)?;
    profiles
        .iter()
        .filter(|profile| profile.provider != TunnelProvider::Openai)
        .map(|profile| cloudflare_tunnel_profile(&store, &profile.profile_id))
        .collect()
}

// Installer-owned runtime materializations are deliberately private. They let
// SCM read only its dedicated work directory rather than the user's catalog.
#[derive(Serialize, Deserialize)]
struct RuntimeBinding {
    profile_id: String,
    #[serde(default)]
    configuration_id: String,
    provider: TunnelProvider,
    host_mode: TunnelHostMode,
    autostart: bool,
    revision: u64,
    runtime_revision: u64,
    owner_username: String,
    runner_client_id: Option<String>,
    ingress_port: u16,
    token_ref: Option<String>,
    local_server_url: String,
    bootstrap_token: String,
}

/// Materialize current canonical desired state for the owner's next start.
/// The installer must grant only the exact service identity its work-directory
/// access through the existing service installation path.
pub fn materialize_cloudflare_tunnel_profiles(
    store: &EnvironmentStore,
) -> SetupResultValue<Vec<PathBuf>> {
    let profiles = cloudflare_tunnel_profiles(store.root())?;
    let mut paths = Vec::with_capacity(profiles.len());
    let mut ids = Vec::new();
    for profile in profiles {
        let binding = RuntimeBinding {
            profile_id: profile.profile_id.clone(),
            configuration_id: profile.configuration_id,
            provider: profile.provider,
            host_mode: profile.host_mode,
            autostart: profile.autostart,
            revision: profile.revision,
            runtime_revision: profile.runtime_revision,
            owner_username: profile.owner_username,
            runner_client_id: profile.runner_client_id,
            ingress_port: profile.ingress_port,
            local_server_url: profile.local_server_url,
            bootstrap_token: profile.bootstrap_token.expose().to_owned(),
            token_ref: profile
                .token_file
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned()),
        };
        let path = profile_directory(store, &profile.profile_id).join("runtime.json");
        write_runtime_if_changed(
            &path,
            &serde_json::to_vec(&binding).map_err(|_| SetupDiagnostic::io())?,
        )?;
        ids.push(binding.profile_id);
        paths.push(path);
    }
    write_runtime_if_changed(
        &store.root().join("server/cloudflare-tunnels.json"),
        &serde_json::to_vec(&ids).map_err(|_| SetupDiagnostic::io())?,
    )?;
    Ok(paths)
}

fn write_runtime_if_changed(path: &Path, bytes: &[u8]) -> SetupResultValue<()> {
    if path.try_exists().map_err(|_| SetupDiagnostic::io())?
        && crate::storage::read_private(path)? == bytes
    {
        // Atomic replacement reinstalls the user's owner-only ACL. Preserve a
        // stopped-owner installer grant on byte-identical runtime capabilities.
        return Ok(());
    }
    atomic_private_write(path, bytes)
}

/// Read the shared dedicated ingress configuration using service-file ACLs.
/// This does not grant a service access to the user's root Environment catalog.
pub fn load_cloudflare_server_ingress_port(
    server_directory: &Path,
) -> SetupResultValue<Option<u16>> {
    let path = server_directory.join("cloudflare-ingress.json");
    if !path.try_exists().map_err(|_| SetupDiagnostic::io())? {
        return Ok(None);
    }
    let bytes = read_runtime_private(&path)?;
    if bytes.len() > 1024 {
        return Err(invalid(
            "cloudflare_ingress",
            "The Server ingress configuration exceeds its bounded size",
        ));
    }
    let configuration: IngressConfiguration = serde_json::from_slice(&bytes).map_err(|_| {
        invalid(
            "cloudflare_ingress",
            "The Server ingress configuration is invalid",
        )
    })?;
    if configuration.port == 0 {
        return Err(invalid(
            "cloudflare_ingress",
            "The Server ingress port is invalid",
        ));
    }
    Ok(Some(configuration.port))
}

fn read_runtime_private(path: &Path) -> SetupResultValue<Vec<u8>> {
    crate::storage::validate_directory_ancestors(path.parent().ok_or_else(SetupDiagnostic::io)?)?;
    #[cfg(not(windows))]
    {
        crate::storage::read_private(path)
    }
    #[cfg(windows)]
    {
        crate::storage::validate_directory_ancestors(
            path.parent().ok_or_else(SetupDiagnostic::io)?,
        )?;
        crate::runtime_entry::validate_service_env_file(path).map_err(|_| SetupDiagnostic::io())?;
        use std::io::Read;
        let mut content = Vec::new();
        std::fs::File::open(path)
            .map_err(|_| SetupDiagnostic::io())?
            .take(32 * 1024 + 1)
            .read_to_end(&mut content)
            .map_err(|_| SetupDiagnostic::io())?;
        if content.len() > 32 * 1024 {
            return Err(SetupDiagnostic::io());
        }
        Ok(content)
    }
}

/// Load a private materialization in an installed service's own work directory.
/// No owner-SID requirement is substituted for the existing Windows ACL checks.
pub fn load_cloudflare_tunnel_materialization(
    path: &Path,
) -> SetupResultValue<CloudflareTunnelRuntimeProfile> {
    let bytes = read_runtime_private(path)?;
    if bytes.len() > 32 * 1024 {
        return Err(invalid(
            "tunnel_profile_configuration",
            "Cloudflare runtime materialization is too large",
        ));
    }
    let mut binding: RuntimeBinding = serde_json::from_slice(&bytes).map_err(|_| {
        invalid(
            "tunnel_profile_configuration",
            "Cloudflare runtime materialization is invalid",
        )
    })?;
    validate_id(&binding.profile_id)?;
    if binding.configuration_id.is_empty() {
        binding.configuration_id = format!("legacy:{}", binding.profile_id);
    }
    if uuid::Uuid::parse_str(&binding.configuration_id).is_err()
        && binding.configuration_id != format!("legacy:{}", binding.profile_id)
    {
        return Err(invalid(
            "tunnel_profile_configuration",
            "Cloudflare runtime incarnation is invalid",
        ));
    }
    validate_provider(&binding.provider)?;
    let directory = path.parent().ok_or_else(SetupDiagnostic::io)?;
    if directory.file_name().and_then(|name| name.to_str()) != Some(binding.profile_id.as_str())
        || path.file_name().and_then(|name| name.to_str()) != Some("runtime.json")
        || binding.ingress_port == 0
        || binding.revision == 0
        || binding.runtime_revision == 0
        || binding.owner_username.is_empty()
        || binding.owner_username.len() > 256
        || binding.owner_username.chars().any(char::is_control)
        || binding
            .runner_client_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 256 || id.chars().any(char::is_control))
    {
        return Err(invalid(
            "tunnel_profile_configuration",
            "Cloudflare runtime materialization identity is invalid",
        ));
    }
    let retirement_path = directory.join("revision.json");
    if retirement_path
        .try_exists()
        .map_err(|_| SetupDiagnostic::io())?
    {
        let retirement = parse_revision_tombstone(
            &binding.profile_id,
            &read_runtime_private(&retirement_path)?,
        )?;
        if retirement.pending_cleanup.is_some() {
            return Err(cleanup_required());
        }
    }
    validate_control_binding(&binding.local_server_url, &binding.bootstrap_token)?;
    let token_file = match (&binding.provider, binding.token_ref) {
        (TunnelProvider::CloudflareNamed { .. }, Some(reference))
            if reference.len() == 36 && uuid::Uuid::parse_str(&reference).is_ok() =>
        {
            let path = directory.join("tokens").join(reference);
            let bytes = read_runtime_private(&path)?;
            let token = Secret::new(String::from_utf8(bytes).map_err(|_| SetupDiagnostic::io())?);
            validate_token(&token)?;
            Some(path)
        }
        (TunnelProvider::CloudflareQuick, None) => None,
        _ => {
            return Err(invalid(
                "tunnel_profile_configuration",
                "Cloudflare runtime credential binding is invalid",
            ))
        }
    };
    Ok(CloudflareTunnelRuntimeProfile {
        profile_id: binding.profile_id,
        configuration_id: binding.configuration_id,
        provider: binding.provider,
        host_mode: binding.host_mode,
        autostart: binding.autostart,
        revision: binding.revision,
        runtime_revision: binding.runtime_revision,
        owner_username: binding.owner_username,
        runner_client_id: binding.runner_client_id,
        local_server_url: binding.local_server_url,
        bootstrap_token: Secret::new(binding.bootstrap_token),
        ingress_port: binding.ingress_port,
        local_target: format!("http://127.0.0.1:{}", binding.ingress_port),
        token_file,
        readiness_path: directory.join("readiness.json"),
    })
}

/// Read all bounded Cloudflare owner metadata materialized in a Server work
/// directory. The Server filters embedded startup; standalone records remain
/// available for exact owner admission without reading the root catalog.
pub fn load_cloudflare_server_materializations(
    server_directory: &Path,
) -> SetupResultValue<Vec<CloudflareTunnelRuntimeProfile>> {
    let path = server_directory.join("cloudflare-tunnels.json");
    if !path.try_exists().map_err(|_| SetupDiagnostic::io())? {
        return Ok(Vec::new());
    }
    let bytes = read_runtime_private(&path)?;
    if bytes.len() > 64 * 128 {
        return Err(invalid(
            "tunnel_profile_capacity",
            "Cloudflare runtime catalog is too large",
        ));
    }
    let ids: Vec<String> = serde_json::from_slice(&bytes).map_err(|_| {
        invalid(
            "tunnel_profile_configuration",
            "Cloudflare runtime catalog is invalid",
        )
    })?;
    if ids.len() > 64 {
        return Err(invalid(
            "tunnel_profile_capacity",
            "Cloudflare runtime catalog exceeds its capacity",
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut selected = 0;
    let mut embedded = 0;
    let mut shared_port = None;
    ids.into_iter()
        .map(|id| {
            validate_id(&id)?;
            if !seen.insert(id.clone()) {
                return Err(invalid(
                    "tunnel_profile_duplicate",
                    "Cloudflare runtime profiles must be unique",
                ));
            }
            let profile = load_cloudflare_tunnel_materialization(
                &server_directory
                    .join("tunnels")
                    .join(&id)
                    .join("runtime.json"),
            )?;
            if profile.host_mode == TunnelHostMode::Embedded {
                embedded += 1;
            }
            if embedded > 16 {
                return Err(invalid(
                    "tunnel_profile_capacity",
                    "Cloudflare embedded runtime profiles exceed their capacity",
                ));
            }
            if shared_port.is_some_and(|port| port != profile.ingress_port) {
                return Err(invalid(
                    "cloudflare_ingress",
                    "Cloudflare runtime profiles disagree about the shared ingress port",
                ));
            }
            shared_port = Some(profile.ingress_port);
            if profile.autostart {
                selected += 1;
            }
            if selected > 1 {
                return Err(invalid(
                    "cloudflare_selection_conflict",
                    "Only one Cloudflare profile can be selected for startup",
                ));
            }
            Ok(profile)
        })
        .collect()
}

fn validate_control_binding(url: &str, token: &str) -> SetupResultValue<()> {
    let parsed = url::Url::parse(url).map_err(|_| {
        invalid(
            "cloudflare_control",
            "Cloudflare local Server control binding is invalid",
        )
    })?;
    if parsed.scheme() != "http"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
        || !parsed.host_str().is_some_and(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        })
        || token.is_empty()
        || token.len() > 8192
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(invalid(
            "cloudflare_control",
            "Cloudflare local Server control binding is invalid",
        ));
    }
    Ok(())
}

fn server_control_binding(store: &EnvironmentStore) -> SetupResultValue<(String, Secret)> {
    let source = read_secret(&store.root().join("server/webcodex.env"))?;
    let address = unique_env_value(source.expose(), "WEBCODEX_ADDR")?
        .and_then(|address| address.parse::<std::net::SocketAddr>().ok())
        .ok_or_else(|| invalid("cloudflare_control", "The Server listener is unavailable"))?;
    let ip = match address.ip() {
        std::net::IpAddr::V4(ip) if ip.is_unspecified() => {
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
        }
        std::net::IpAddr::V6(ip) if ip.is_unspecified() => {
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        }
        ip if ip.is_loopback() => ip,
        _ => {
            return Err(invalid(
                "cloudflare_control",
                "Cloudflare requires a loopback or wildcard local Server listener",
            ))
        }
    };
    let url = format!("http://{}", std::net::SocketAddr::new(ip, address.port()));
    let token = unique_env_value(source.expose(), "WEBCODEX_TOKEN")?.ok_or_else(|| {
        invalid(
            "cloudflare_control",
            "The Server bootstrap credential is unavailable",
        )
    })?;
    validate_control_binding(&url, &token)?;
    Ok((url, Secret::new(token)))
}

fn desired_ingress_port(store: &EnvironmentStore, requested: Option<u16>) -> SetupResultValue<u16> {
    let saved = cloudflare_ingress_port(store)?;
    if requested == Some(0) || (saved.is_some() && requested.is_some() && saved != requested) {
        return Err(invalid(
            "cloudflare_ingress",
            "The shared Cloudflare ingress port cannot change through a profile edit",
        ));
    }
    let port = saved.or(requested).map(Ok).unwrap_or_else(|| {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .and_then(|listener| listener.local_addr())
            .map(|addr| addr.port())
            .map_err(|_| {
                invalid(
                    "cloudflare_ingress",
                    "A dedicated loopback ingress port could not be allocated",
                )
            })
    })?;
    let source = read_secret(&store.root().join("server/webcodex.env"))?;
    let management = env_value(source.expose(), "WEBCODEX_ADDR")
        .and_then(|addr| addr.parse::<std::net::SocketAddr>().ok());
    if management.is_none() || management.is_some_and(|addr| addr.port() == port) {
        return Err(invalid(
            "cloudflare_ingress",
            "Cloudflare ingress must use a dedicated port distinct from Server management",
        ));
    }
    Ok(port)
}

fn updated_record(
    existing: Option<&TunnelRecord>,
    request: &CloudflareTunnelProfileRequest<'_>,
    name: String,
    token_changed: bool,
) -> SetupResultValue<(TunnelRecord, bool)> {
    let runtime_changed = existing.is_none_or(|profile| {
        profile.autostart != request.autostart || profile.provider != request.provider
    }) || token_changed;
    let profile = match existing {
        Some(saved) => TunnelRecord {
            configuration_id: saved.configuration_id.clone(),
            profile_id: saved.profile_id.clone(),
            provider: request.provider.clone(),
            name: name.clone(),
            host_mode: saved.host_mode,
            autostart: request.autostart,
            revision: if runtime_changed || saved.display_name() != name {
                next_revision(saved.revision)?
            } else {
                saved.revision.max(1)
            },
            runtime_revision: if runtime_changed {
                next_revision(saved.effective_runtime_revision())?
            } else {
                saved.effective_runtime_revision()
            },
            installed: saved.installed,
            started: saved.started,
        },
        None => TunnelRecord {
            configuration_id: Some(uuid::Uuid::new_v4().to_string()),
            profile_id: request.profile_id.to_owned(),
            provider: request.provider.clone(),
            name,
            host_mode: request.host_mode,
            autostart: request.autostart,
            revision: 1,
            runtime_revision: 1,
            installed: false,
            started: false,
        },
    };
    Ok((profile, runtime_changed))
}

impl NativeEnvironment {
    /// Save desired state without starting a service or restarting the Server.
    pub async fn configure_cloudflare_tunnel_profile(
        &self,
        store: &EnvironmentStore,
        request: &CloudflareTunnelProfileRequest<'_>,
    ) -> SetupResultValue<TunnelConfigurationResult> {
        let _lock = store.lock()?;
        ensure_upgrade_idle_under_lock(store)?;
        validate_id(request.profile_id)?;
        ensure_cloudflare_profile_available(store, request.profile_id)?;
        validate_provider(&request.provider)?;
        if request.provider == TunnelProvider::Openai {
            return Err(invalid(
                "tunnel_provider",
                "Use the OpenAI profile configuration operation",
            ));
        }
        let environment = store
            .load_environment()?
            .ok_or_else(|| invalid("not_configured", "Configure the Server environment first"))?;
        if !environment.request.local_server()
            || environment
                .username
                .as_ref()
                .is_none_or(|name| name.is_empty())
        {
            return Err(invalid(
                "cloudflare_owner",
                "Cloudflare requires a configured local Server and authenticated owner",
            ));
        }
        let mut profiles = tunnel_profiles(store)?;
        validate_catalog(&profiles)?;
        let index = profiles
            .iter()
            .position(|profile| profile.profile_id == request.profile_id);
        let existing = index.map(|index| &profiles[index]);
        match (existing, request.expected_revision) {
            (Some(profile), Some(expected)) if expected == profile.revision.max(1) => {}
            (None, None) => {}
            _ => {
                return Err(invalid(
                    "tunnel_revision_stale",
                    "Reload the current Cloudflare profile before editing it",
                ))
            }
        }
        if existing.is_some_and(|profile| {
            !same_provider_identity(&profile.provider, &request.provider)
                || profile.host_mode != request.host_mode
        }) {
            return Err(invalid("tunnel_binding_conflict", "A saved Cloudflare profile's tunnel identity, provider and lifecycle owner cannot change through configuration"));
        }
        let name = request
            .name
            .map(str::trim)
            .map(str::to_owned)
            .or_else(|| existing.map(|profile| profile.display_name().to_owned()))
            .unwrap_or_else(|| request.profile_id.to_owned());
        if name.is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
            return Err(invalid(
                "tunnel_profile_name",
                "Tunnel profile name is invalid",
            ));
        }
        let directory = profile_directory(store, request.profile_id);
        if existing.is_none()
            && directory
                .join("webcodex.env")
                .try_exists()
                .map_err(|_| SetupDiagnostic::io())?
        {
            return Err(invalid(
                "tunnel_profile_recovery_required",
                "A private Cloudflare binding exists without its catalog record",
            ));
        }
        let previous_binding = if existing.is_some() {
            Some(read_secret(&directory.join("webcodex.env"))?)
        } else {
            None
        };
        let previous_token_file = existing
            .map(|profile| binding_token_file(store, profile))
            .transpose()?
            .flatten();
        let named = matches!(request.provider, TunnelProvider::CloudflareNamed { .. });
        if !named && request.token.is_some() {
            return Err(invalid(
                "cloudflare_token",
                "Quick Tunnel profiles do not accept credentials",
            ));
        }
        if named && request.token.is_none() && previous_token_file.is_none() {
            return Err(invalid(
                "cloudflare_token",
                "A token is required when first saving a Named Tunnel profile",
            ));
        }
        if let Some(token) = request.token {
            validate_token(token)?;
        }
        let token_changed = match (request.token, previous_token_file.as_ref()) {
            (Some(token), Some(path)) => read_secret(path)?.expose() != token.expose(),
            (Some(_), None) => true,
            _ => false,
        };
        let (mut profile, runtime_changed) =
            updated_record(existing, request, name, token_changed)?;
        if existing.is_none() {
            let initial_revision = next_cloudflare_profile_revision(store, request.profile_id)?;
            profile.revision = initial_revision;
            profile.runtime_revision = initial_revision;
        }
        let index = if let Some(index) = index {
            profiles[index] = profile;
            index
        } else {
            profiles.push(profile);
            profiles.len() - 1
        };
        validate_catalog(&profiles)?;
        // Inspect exact owners before changing runtime capability files.
        let standalone = tunnel_service_spec(store, &environment, request.profile_id)?;
        let standalone_status =
            service::ServiceManager::inspect(&standalone).map_err(service_error)?;
        if request.host_mode == TunnelHostMode::Standalone
            && !request.autostart
            && standalone_status.ownership != service::Ownership::Absent
        {
            return Err(invalid("cloudflare_standalone_selection", "Stop and uninstall the Cloudflare service before saving it as an unselected alternative"));
        }
        let server = service_spec(store, &environment, service::Component::Server)?;
        let server_status = service::ServiceManager::inspect(&server).map_err(service_error)?;
        let server_running = server_status.ownership == service::Ownership::Owned
            && server_status.running == Some(true);
        let owner_status = if request.host_mode == TunnelHostMode::Embedded {
            if standalone_status.ownership != service::Ownership::Absent {
                return Err(invalid(
                    "tunnel_host_busy",
                    "A standalone service still owns this profile",
                ));
            }
            server_status
        } else {
            standalone_status
        };
        if matches!(
            owner_status.ownership,
            service::Ownership::Foreign | service::Ownership::Unknown
        ) {
            return Err(invalid(
                "tunnel_owner",
                "The Cloudflare lifecycle owner cannot be verified",
            ));
        }
        let _ = server_control_binding(store)?;
        let first_ingress = cloudflare_ingress_port(store)?.is_none();
        let port = desired_ingress_port(store, request.ingress_port)?;
        ensure_private_directory(&directory)?;
        let token_file = if token_changed {
            let tokens = directory.join("tokens");
            ensure_private_directory(&tokens)?;
            // Retain old immutable capabilities while a running owner may still
            // hold them. Bound rotations rather than deleting a live capability.
            if std::fs::read_dir(&tokens)
                .map_err(|_| SetupDiagnostic::io())?
                .take(65)
                .count()
                >= 64
            {
                return Err(invalid(
                    "cloudflare_token_capacity",
                    "This profile has reached its retained credential capacity",
                ));
            }
            let path = tokens.join(uuid::Uuid::new_v4().to_string());
            atomic_private_write(
                &path,
                request
                    .token
                    .expect("validated token mutation")
                    .expose()
                    .as_bytes(),
            )?;
            Some(path)
        } else {
            previous_token_file
        };
        let provider = if named {
            "cloudflare_named"
        } else {
            "cloudflare_quick"
        };
        let token_line = token_file
            .as_ref()
            .map(|path| {
                format!(
                    "CLOUDFLARE_TUNNEL_TOKEN_REF={}\n",
                    path.file_name()
                        .expect("generated token basename")
                        .to_string_lossy()
                )
            })
            .unwrap_or_default();
        let content = format!(
            "WEBCODEX_TUNNEL_PROFILE_ID={}\nWEBCODEX_TUNNEL_PROVIDER={provider}\n{token_line}",
            request.profile_id
        );
        if !directory.join("readiness.json").exists() {
            atomic_private_write(&directory.join("readiness.json"), b"{}")?;
        }
        crate::tunnel::ensure_server_tunnel_environment(store)?;
        // The ingress is shared Server configuration, never a profile-specific
        // management target. Failed later writes may retain this safe allocation.
        store.write_json(
            "server/cloudflare-ingress.json",
            &IngressConfiguration { port },
        )?;
        atomic_private_write(&directory.join("webcodex.env"), content.as_bytes())?;
        if let Err(error) = store.write_json("tunnel.json", &profiles) {
            let restored = if let Some(previous) = previous_binding {
                atomic_private_write(
                    &directory.join("webcodex.env"),
                    previous.expose().as_bytes(),
                )
            } else {
                std::fs::remove_file(directory.join("webcodex.env"))
                    .map_err(|_| SetupDiagnostic::io())
            };
            if restored.is_err() {
                return Err(invalid("tunnel_profile_recovery_required", "The previous Cloudflare binding could not be restored after an interrupted catalog update"));
            }
            return Err(error);
        }
        let running = owner_status.ownership == service::Ownership::Owned
            && owner_status.running == Some(true);
        let applied_revision = crate::tunnel::read_tunnel_health(&directory.join("readiness.json"))
            .ok()
            .and_then(|health| health.profile_revision);
        // The Server owns admission even for a standalone process. Its first
        // dedicated listener cannot be added by starting only the Tunnel service.
        let server_restart_required = server_running && first_ingress
            || request.host_mode == TunnelHostMode::Embedded
                && running
                && (runtime_changed
                    || applied_revision != Some(profiles[index].effective_runtime_revision()));
        let next_action = if server_restart_required {
            TunnelConfigurationNextAction::RestartServer
        } else {
            match (request.host_mode, running, runtime_changed) {
                (TunnelHostMode::Embedded, true, true) => {
                    TunnelConfigurationNextAction::RestartServer
                }
                (TunnelHostMode::Embedded, false, _) => TunnelConfigurationNextAction::StartServer,
                (TunnelHostMode::Standalone, true, true) => {
                    TunnelConfigurationNextAction::RestartStandalone
                }
                (TunnelHostMode::Standalone, false, _) => {
                    TunnelConfigurationNextAction::StartStandalone
                }
                _ => TunnelConfigurationNextAction::None,
            }
        };
        Ok(TunnelConfigurationResult {
            profile: snapshot(store, &profiles[index])?,
            owner_status,
            server_restart_required,
            next_action,
        })
    }
}

#[cfg(test)]
mod tests;
