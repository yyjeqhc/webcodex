//! Native operator settings for the exact saved Runner. Never return raw TOML.
use crate::error::{DesktopError, DesktopResult};
use crate::models::StoredRuntime;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table};
use webcodex_core::plugin::{validate_provider_id, validate_provider_name, PLUGIN_MAX_PROVIDERS};

mod mcp;
pub use mcp::reconcile_mcp;
mod acp;
pub use acp::reconcile_acp;

const MAX_BYTES: u64 = 256 * 1024;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunnerPaths {
    pub instruction_files: Vec<String>,
    pub skill_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SettingsTarget {
    pub config_path: std::path::PathBuf,
    pub client_id: String,
    pub server_url: String,
}

pub fn target(runtime: &StoredRuntime) -> DesktopResult<SettingsTarget> {
    Ok(SettingsTarget {
        config_path: runtime.runner_config.clone().ok_or_else(error)?,
        client_id: runtime
            .runner_client_id
            .clone()
            .filter(|s| !s.is_empty())
            .ok_or_else(error)?,
        server_url: runtime.server_url.clone(),
    })
}

pub fn verify_target(runtime: &StoredRuntime, expected: &SettingsTarget) -> DesktopResult<()> {
    if &target(runtime)? != expected {
        return Err(error());
    }
    Ok(())
}

#[derive(Serialize)]
pub struct RunnerSettings {
    pub paths: RunnerPaths,
    pub plugin_ids: Vec<String>,
    pub target: SettingsTarget,
    pub can_restart: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsUpdate {
    pub target: SettingsTarget,
    pub expected: RunnerPaths,
    pub paths: RunnerPaths,
}

fn error() -> DesktopError {
    DesktopError::new("runner_settings_unavailable", "Runner settings could not be read or saved safely", "Reload settings. Use at most 16 unique absolute paths per list, without parent traversal. The saved Runner identity must match.")
}

fn read(path: &Path) -> DesktopResult<String> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| error())?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(error());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| error())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| error())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(error());
    }
    String::from_utf8(bytes).map_err(|_| error())
}

fn parse(text: &str, runtime: &StoredRuntime) -> DesktopResult<DocumentMut> {
    let doc = text.parse::<DocumentMut>().map_err(|_| error())?;
    if doc.get("client_id").and_then(Item::as_str) != runtime.runner_client_id.as_deref()
        || doc.get("server_url").and_then(Item::as_str) != Some(runtime.server_url.as_str())
    {
        return Err(error());
    }
    Ok(doc)
}

fn paths(doc: &DocumentMut) -> DesktopResult<RunnerPaths> {
    fn list(doc: &DocumentMut, section: &str, key: &str) -> DesktopResult<Vec<String>> {
        let Some(section) = doc.get(section) else {
            return Ok(Vec::new());
        };
        if !section.is_table_like() {
            return Err(error());
        }
        let Some(value) = section.get(key) else {
            return Ok(Vec::new());
        };
        value
            .as_array()
            .ok_or_else(error)?
            .iter()
            .map(|v| v.as_str().map(str::to_string).ok_or_else(error))
            .collect()
    }
    Ok(RunnerPaths {
        instruction_files: list(doc, "instructions", "files")?,
        skill_roots: list(doc, "skills", "roots")?,
    })
}

fn validate(paths: &[String]) -> DesktopResult<()> {
    if paths.len() > 16 {
        return Err(error());
    }
    for (index, value) in paths.iter().enumerate() {
        let path = Path::new(value);
        if value.is_empty()
            || value.len() > 4096
            || value.contains(['\0', '\n', '\r'])
            || !path.is_absolute()
            || webcodex_runner_config::paths::validate_project_path_ingress(path).is_err()
            || paths[..index]
                .iter()
                .any(|other| webcodex_runner_config::paths::paths_equal(path, Path::new(other)))
        {
            return Err(error());
        }
    }
    Ok(())
}

pub fn inspect(runtime: &StoredRuntime, can_restart: bool) -> DesktopResult<RunnerSettings> {
    let path = runtime.runner_config.as_ref().ok_or_else(error)?;
    let doc = parse(&read(path)?, runtime)?;
    Ok(RunnerSettings {
        paths: paths(&doc)?,
        plugin_ids: plugin_ids(&doc)?,
        target: target(runtime)?,
        can_restart,
    })
}

fn plugin_ids(doc: &DocumentMut) -> DesktopResult<Vec<String>> {
    let Some(section) = doc.get("plugins") else {
        return Ok(Vec::new());
    };
    if !section.is_table_like() {
        return Err(error());
    }
    let Some(providers) = section.get("providers") else {
        return Ok(Vec::new());
    };
    let ids = if let Some(tables) = providers.as_array_of_tables() {
        tables
            .iter()
            .map(|p| {
                p.get("id")
                    .and_then(Item::as_str)
                    .ok_or_else(error)
                    .map(str::to_owned)
            })
            .collect::<DesktopResult<Vec<_>>>()?
    } else if let Some(array) = providers.as_array() {
        array
            .iter()
            .map(|p| {
                p.as_inline_table()
                    .and_then(|p| p.get("id"))
                    .and_then(toml_edit::Value::as_str)
                    .ok_or_else(error)
                    .map(str::to_owned)
            })
            .collect::<DesktopResult<Vec<_>>>()?
    } else {
        return Err(error());
    };
    if ids.len() > PLUGIN_MAX_PROVIDERS || ids.iter().any(|id| validate_provider_id(id).is_err()) {
        return Err(error());
    }
    Ok(ids)
}

pub fn update(runtime: &StoredRuntime, request: SettingsUpdate) -> DesktopResult<()> {
    verify_target(runtime, &request.target)?;
    validate(&request.paths.instruction_files)?;
    validate(&request.paths.skill_roots)?;
    let path = runtime.runner_config.as_ref().ok_or_else(error)?;
    let original = read(path)?;
    let mut doc = parse(&original, runtime)?;
    if paths(&doc)? != request.expected {
        return Err(error());
    }
    for (section, key, values) in [
        ("instructions", "files", request.paths.instruction_files),
        ("skills", "roots", request.paths.skill_roots),
    ] {
        let array: Array = values.into_iter().collect();
        doc[section][key] = toml_edit::value(array);
    }
    persist(path, &original, &doc)
}

fn persist(path: &Path, original: &str, doc: &DocumentMut) -> DesktopResult<()> {
    let updated = doc.to_string();
    if updated.len() as u64 > MAX_BYTES {
        return Err(error());
    }
    crate::state::write_atomic_file_with_hook(path, updated.as_bytes(), |_| {
        if read(path).ok().as_deref() != Some(original) {
            return Err(std::io::Error::other("Runner configuration changed"));
        }
        Ok(())
    })
    .map_err(|_| error())
}

/// Operator-entered launch arguments are write-only; inspection returns IDs only.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRegistration {
    pub id: String,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginAddRequest {
    pub target: SettingsTarget,
    pub provider: PluginRegistration,
}

pub fn add_plugin(runtime: &StoredRuntime, request: PluginAddRequest) -> DesktopResult<()> {
    verify_target(runtime, &request.target)?;
    let p = request.provider;
    if validate_provider_id(&p.id).is_err()
        || validate_provider_name(&p.name).is_err()
        || p.command.trim().is_empty()
        || p.command.len() > 1024
        || p.command.contains(['\0', '\n', '\r'])
        || p.args.len() > 64
        || p.args.iter().any(|a| a.len() > 4096 || a.contains('\0'))
        || p.args.iter().map(String::len).sum::<usize>() > 16 * 1024
    {
        return Err(error());
    }
    if let Some(cwd) = &p.cwd {
        validate(std::slice::from_ref(cwd))?;
    }
    let path = runtime.runner_config.as_ref().ok_or_else(error)?;
    let original = read(path)?;
    let mut doc = parse(&original, runtime)?;
    let ids = plugin_ids(&doc)?;
    if ids.len() >= PLUGIN_MAX_PROVIDERS || ids.contains(&p.id) {
        return Err(error());
    }
    let mut provider = Table::new();
    provider["id"] = toml_edit::value(p.id);
    provider["name"] = toml_edit::value(p.name);
    provider["command"] = toml_edit::value(p.command);
    provider["args"] = toml_edit::value(p.args.into_iter().collect::<Array>());
    if let Some(cwd) = p.cwd {
        provider["cwd"] = toml_edit::value(cwd);
    }
    if doc.get("plugins").is_none() {
        doc["plugins"] = Item::Table(Table::new());
    }
    if doc["plugins"].get("providers").is_none() {
        doc["plugins"]["providers"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    let providers = &mut doc["plugins"]["providers"];
    if let Some(tables) = providers.as_array_of_tables_mut() {
        tables.push(provider);
    } else if let Some(array) = providers.as_array_mut() {
        array.push(provider.into_inline_table());
    } else {
        return Err(error());
    }
    persist(path, &original, &doc)
}

#[cfg(test)]
mod regression_tests;
#[cfg(test)]
mod tests;
