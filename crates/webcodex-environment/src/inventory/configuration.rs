//! Read known private files once, retaining only allowlisted path fields.
use super::*;
use crate::storage::{read_private, validate_directory_ancestors};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn configuration_text(path: &Path) -> Result<crate::Secret, PathStatus> {
    let parent = path.parent().ok_or(PathStatus::Invalid)?;
    validate_directory_ancestors(parent).map_err(|e| {
        if e.code == "unsafe_path" {
            PathStatus::UnsafePath
        } else {
            PathStatus::Unreadable
        }
    })?;
    let entry = local_path_entry(
        "configuration",
        "runtime",
        "configuration",
        "configured",
        path,
        PathKind::File,
        SafetyCategory::MixedConfiguration,
    );
    if entry.status != PathStatus::Present {
        return Err(entry.status);
    }
    let bytes = read_private(path).map_err(|_| PathStatus::Unreadable)?;
    String::from_utf8(bytes)
        .map(crate::Secret::new)
        .map_err(|_| PathStatus::Invalid)
}

fn location(
    id: &str,
    component: &str,
    value: &Path,
    working_directory: Option<&Path>,
) -> PathEntry {
    if !admissible_display_path(value) {
        return reference_entry(
            id,
            component,
            &id.replace('.', "_"),
            "configured",
            PathKind::Directory,
            SafetyCategory::Data,
            PathStatus::Invalid,
        );
    }
    if !admissible_path(value) || (!value.is_absolute() && working_directory.is_none()) {
        let mut entry = reference_entry(
            id,
            component,
            &id.replace('.', "_"),
            "configured",
            PathKind::Directory,
            if id.ends_with("trace") {
                SafetyCategory::Log
            } else {
                SafetyCategory::Data
            },
            PathStatus::Unconfirmed,
        );
        entry.configured_path = Some(value.to_path_buf());
        return entry;
    }
    let resolved = if value.is_absolute() {
        value.to_path_buf()
    } else {
        working_directory
            .expect("relative values require known working directory")
            .join(value)
    };
    let mut entry = local_path_entry(
        id,
        component,
        &id.replace('.', "_"),
        "configured",
        &resolved,
        PathKind::Directory,
        if id.ends_with("trace") {
            SafetyCategory::Log
        } else {
            SafetyCategory::Data
        },
    );
    entry.configured_path = Some(value.to_path_buf());
    entry
}

#[derive(Debug, Clone)]
pub struct ServerLocations {
    pub data: PathEntry,
    pub trace: PathEntry,
}
/// Uses the runtime parser; never loads values into process environment. Duplicate
/// location keys and malformed files fail closed instead of guessing precedence.
pub fn inspect_server_locations(
    config: &Path,
    working_directory: Option<&Path>,
) -> ServerLocations {
    let failed = |status| {
        let status = if matches!(status, PathStatus::Missing | PathStatus::Unreadable) {
            PathStatus::Unconfirmed
        } else {
            status
        };
        ServerLocations {
            data: reference_entry(
                "server.data",
                "server",
                "server_data",
                "configured",
                PathKind::Directory,
                SafetyCategory::Data,
                status,
            ),
            trace: reference_entry(
                "server.trace",
                "server",
                "request_trace",
                "configured",
                PathKind::Directory,
                SafetyCategory::Log,
                status,
            ),
        }
    };
    let text = match configuration_text(config) {
        Ok(text) => text,
        Err(status) => return failed(status),
    };
    let mut values = BTreeMap::new();
    for line in text.expose().lines() {
        let Some(parsed) = webcodex_core::server_environment::parse_env_file_line(line) else {
            continue;
        };
        let (key, value) = match parsed {
            Ok(pair) => pair,
            Err(_) => return failed(PathStatus::Invalid),
        };
        if matches!(
            key.as_str(),
            "WEBCODEX_DATA" | "WEBCODEX_TOOL_REQUEST_TRACE_DIR"
        ) && values.insert(key, value).is_some()
        {
            return failed(PathStatus::Unconfirmed);
        }
    }
    let data = PathBuf::from(
        values
            .get("WEBCODEX_DATA")
            .map(String::as_str)
            .unwrap_or("./data"),
    );
    if data.as_os_str().is_empty() {
        return failed(PathStatus::Invalid);
    }
    let trace = values
        .get("WEBCODEX_TOOL_REQUEST_TRACE_DIR")
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data.join("tool-request-traces"));
    ServerLocations {
        data: location("server.data", "server", &data, working_directory),
        trace: location("server.trace", "server", &trace, working_directory),
    }
}

#[derive(Debug, Clone)]
pub struct RunnerLocations {
    pub registry: PathEntry,
    pub client_id: Option<String>,
    pub owner: Option<String>,
    pub server_url: Option<String>,
    pub display_name: Setting<Option<String>>,
}
/// TOML parsing matches the Environment's existing Runner-binding projections;
/// no runtime initialization, registry enumeration or credential projection.
pub fn inspect_runner_locations(config: &Path, default_registry: &Path) -> RunnerLocations {
    let mut result = RunnerLocations {
        registry: reference_entry(
            "runner.registry",
            "runner",
            "project_registration",
            "configured",
            PathKind::Directory,
            SafetyCategory::Secret,
            PathStatus::Unconfirmed,
        ),
        client_id: None,
        owner: None,
        server_url: None,
        display_name: Setting::Unknown,
    };
    let text = match configuration_text(config) {
        Ok(text) => text,
        Err(status) => {
            result.registry.status =
                if matches!(status, PathStatus::Missing | PathStatus::Unreadable) {
                    PathStatus::Unconfirmed
                } else {
                    status
                };
            return result;
        }
    };
    let value: toml::Value = match toml::from_str(text.expose()) {
        Ok(value) => value,
        Err(_) => {
            result.registry.status = PathStatus::Invalid;
            return result;
        }
    };
    result.server_url = value
        .get("server_url")
        .and_then(toml::Value::as_str)
        .and_then(|url| crate::canonical_server_url(url).ok());
    result.client_id = value
        .get("client_id")
        .and_then(toml::Value::as_str)
        .filter(|v| safe_identifier(v))
        .map(str::to_string);
    result.owner = value
        .get("owner")
        .and_then(toml::Value::as_str)
        .filter(|v| safe_identifier(v))
        .map(str::to_string);
    result.display_name = match value.get("display_name") {
        None => Setting::Known { value: None },
        Some(value) => value
            .as_str()
            .filter(|value| settings::safe_display_name(value))
            .map_or(Setting::Unknown, |value| Setting::Known {
                value: Some(value.into()),
            }),
    };
    let registry = match value.get("project_registry_dir") {
        Some(value) => match value.as_str() {
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => {
                result.registry.status = PathStatus::Invalid;
                result.display_name = Setting::Unknown;
                return result;
            }
        },
        None if !default_registry.as_os_str().is_empty() => default_registry.to_path_buf(),
        None => {
            result.registry.source = "derived".into();
            return result;
        }
    };
    result.registry = if registry.is_absolute() {
        location("runner.registry", "runner", &registry, None)
    } else {
        let mut entry = reference_entry(
            "runner.registry",
            "runner",
            "project_registration",
            "configured",
            PathKind::Directory,
            SafetyCategory::Secret,
            PathStatus::Unconfirmed,
        );
        if admissible_display_path(&registry) {
            entry.configured_path = Some(registry);
        } else {
            entry.status = PathStatus::Invalid;
        }
        entry
    };
    result.registry.category = SafetyCategory::Secret;
    if value.get("project_registry_dir").is_none() {
        result.registry.source = "derived".into();
    }
    result
}

pub fn append_server_configuration_paths(
    inventory: &mut PathInventory,
    config: &Path,
    working_directory: Option<&Path>,
) {
    add(
        inventory,
        local_path_entry(
            "server.configuration",
            "server",
            "server_configuration",
            "saved_record",
            config,
            PathKind::File,
            SafetyCategory::MixedConfiguration,
        ),
    );
    let locations = inspect_server_locations(config, working_directory);
    if let Some(data) = resolved_location_path(&locations.data, working_directory) {
        add(
            inventory,
            local_path_entry(
                "server.database",
                "server",
                "server_database",
                "derived",
                &data.join("webcodex.db"),
                PathKind::File,
                SafetyCategory::Data,
            ),
        );
    }
    add(inventory, locations.data);
    add(inventory, locations.trace);
}

pub fn append_runner_configuration_paths(
    inventory: &mut PathInventory,
    config: &Path,
    expected_client_id: Option<&str>,
    expected_server_url: Option<&str>,
) {
    add(
        inventory,
        local_path_entry(
            "runner.configuration",
            "runner",
            "runner_configuration",
            "saved_record",
            config,
            PathKind::File,
            SafetyCategory::MixedConfiguration,
        ),
    );
    // Legacy default comes from the established Runner resolver, not the config
    // file's parent (which need not be the authoritative user config base).
    let default_registry = default_runner_registry().ok();
    let mut locations = inspect_runner_locations(
        config,
        default_registry.as_deref().unwrap_or_else(|| Path::new("")),
    );
    let server_matches = expected_server_url
        .is_none_or(|expected| locations.server_url.as_deref() == Some(expected));
    if expected_client_id.is_some_and(|expected| locations.client_id.as_deref() != Some(expected))
        || !server_matches
    {
        issue(
            inventory,
            "runner_binding_unconfirmed",
            "runner.configuration",
        );
        locations.registry.status = PathStatus::Unconfirmed;
        locations.registry.canonical_path = None;
        locations.registry.directory_to_open = None;
        locations.display_name = Setting::Unknown;
    }
    inventory.settings.device_display_name = locations.display_name;
    add(inventory, locations.registry);
}

/// Same absent-key fallback as Runner startup; this resolver performs no IO.
pub(super) fn default_runner_registry() -> Result<PathBuf, String> {
    let base = webcodex_runner_config::paths::default_client_config_base_dir()?;
    webcodex_runner_config::paths::select_project_registry_dir(&base)
}

pub(super) fn resolved_location_path(
    entry: &PathEntry,
    working_directory: Option<&Path>,
) -> Option<PathBuf> {
    if !matches!(
        entry.status,
        PathStatus::Present | PathStatus::Missing | PathStatus::Unreadable
    ) {
        return None;
    }
    entry.canonical_path.clone().or_else(|| {
        let configured = entry.configured_path.as_ref()?;
        if !admissible_path(configured) {
            return None;
        }
        if configured.is_absolute() {
            Some(configured.clone())
        } else {
            working_directory.map(|directory| directory.join(configured))
        }
    })
}
