//! Desktop desired state for startup-owned ACP providers. Environment entries are
//! names-to-names only; this module never reads an environment value or credential.
use crate::error::{DesktopError, DesktopResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use webcodex_core::coding_agent::{validate_provider_id, CODING_AGENT_MAX_PROVIDERS};

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_SAVED: usize = 64;
const MAX_OWNED_IDS: usize = 4096;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodingAgentProfile {
    pub provider_id: String,
    pub name: String,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub enabled: bool,
    #[serde(default)]
    pub env_from_env: BTreeMap<String, String>,
    #[serde(default)]
    pub allowed_config_options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AcpGlobalSettings {
    pub max_concurrent_runs: usize,
    pub permission_timeout_secs: u64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    revision: u64,
    owner_id: String,
    profiles: Vec<CodingAgentProfile>,
    // Tombstones are retained across enrollment slots and regenerated TOML files.
    managed_ids: BTreeSet<String>,
    global_settings: Option<AcpGlobalSettings>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            owner_id: uuid::Uuid::new_v4().to_string(),
            profiles: Vec::new(),
            managed_ids: BTreeSet::new(),
            global_settings: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodingAgentsSnapshot {
    pub revision: u64,
    pub profiles: Vec<CodingAgentProfile>,
    pub global_settings: Option<AcpGlobalSettings>,
    pub restart_required: bool,
    pub config_error: bool,
    pub max_enabled: usize,
}

impl Default for CodingAgentsSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            profiles: Vec::new(),
            global_settings: None,
            restart_required: false,
            config_error: false,
            max_enabled: CODING_AGENT_MAX_PROVIDERS,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingAgentUpdate {
    pub target: crate::webcodex::settings::SettingsTarget,
    pub expected_revision: u64,
    pub previous_id: Option<String>,
    pub profile: CodingAgentProfile,
    pub global_settings: Option<AcpGlobalSettings>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodingAgentRemove {
    pub target: crate::webcodex::settings::SettingsTarget,
    pub expected_revision: u64,
    pub provider_id: String,
}

#[derive(Clone)]
pub struct CodingAgentStore {
    path: PathBuf,
    original: Option<Vec<u8>>,
    manifest: Manifest,
    invalid: bool,
}

impl CodingAgentStore {
    pub fn load(root: &Path) -> Self {
        let path = root.join("coding-agents.json");
        let loaded = read_optional(&path).and_then(|bytes| {
            let manifest = match bytes.as_ref() {
                Some(bytes) => serde_json::from_slice(bytes).map_err(|_| invalid())?,
                None => Manifest::default(),
            };
            validate_manifest(&manifest)?;
            Ok((bytes, manifest))
        });
        match loaded {
            Ok((original, manifest)) => Self {
                path,
                original,
                manifest,
                invalid: false,
            },
            Err(_) => Self {
                path,
                original: None,
                manifest: Manifest::default(),
                invalid: true,
            },
        }
    }

    pub fn snapshot(&self, applied_revision: Option<u64>) -> CodingAgentsSnapshot {
        CodingAgentsSnapshot {
            revision: self.manifest.revision,
            profiles: if self.invalid {
                Vec::new()
            } else {
                self.manifest.profiles.clone()
            },
            global_settings: if self.invalid {
                None
            } else {
                self.manifest.global_settings.clone()
            },
            restart_required: self.manifest.revision != 0
                && applied_revision != Some(self.manifest.revision),
            config_error: self.invalid,
            max_enabled: CODING_AGENT_MAX_PROVIDERS,
        }
    }

    pub fn revision(&self) -> u64 {
        self.manifest.revision
    }
    pub fn owner_id(&self) -> &str {
        &self.manifest.owner_id
    }
    pub fn managed_ids(&self) -> &BTreeSet<String> {
        &self.manifest.managed_ids
    }
    pub fn global_settings(&self) -> Option<&AcpGlobalSettings> {
        self.manifest.global_settings.as_ref()
    }
    pub fn needs_reconciliation(&self) -> bool {
        self.invalid
            || !self.manifest.managed_ids.is_empty()
            || self.manifest.global_settings.is_some()
    }
    pub fn profiles(&self) -> DesktopResult<&[CodingAgentProfile]> {
        if self.invalid {
            Err(invalid())
        } else {
            Ok(&self.manifest.profiles)
        }
    }

    /// Stage first, then preflight reconciliation against the exact current
    /// Runner, and only then commit. A collision must not become desired state.
    pub fn stage_update(&mut self, request: CodingAgentUpdate) -> DesktopResult<()> {
        self.check_revision(request.expected_revision)?;
        validate_profile(&request.profile)?;
        if request.profile.enabled {
            crate::mcp_providers::resolve_executable(&request.profile.executable)
                .map_err(|_| executable_unavailable())?;
        }
        let mut next = self.manifest.clone();
        if let Some(previous) = request.previous_id.as_deref() {
            let index = next
                .profiles
                .iter()
                .position(|p| p.provider_id == previous)
                .ok_or_else(changed)?;
            next.profiles.remove(index);
        }
        if next
            .profiles
            .iter()
            .any(|p| p.provider_id == request.profile.provider_id)
        {
            return Err(conflict());
        }
        next.managed_ids.insert(request.profile.provider_id.clone());
        next.profiles.push(request.profile);
        next.profiles
            .sort_by(|a, b| a.provider_id.cmp(&b.provider_id));
        next.global_settings = request.global_settings;
        next.revision = next.revision.checked_add(1).ok_or_else(invalid)?;
        validate_manifest(&next)?;
        self.manifest = next;
        Ok(())
    }

    pub fn stage_remove(&mut self, id: &str, revision: u64) -> DesktopResult<()> {
        self.check_revision(revision)?;
        let index = self
            .manifest
            .profiles
            .iter()
            .position(|p| p.provider_id == id)
            .ok_or_else(changed)?;
        self.manifest.profiles.remove(index);
        self.manifest.revision = self.manifest.revision.checked_add(1).ok_or_else(invalid)?;
        // Do not remove managed_ids: inactive A/B slots must not resurrect it.
        Ok(())
    }

    pub fn commit(&mut self) -> DesktopResult<()> {
        validate_manifest(&self.manifest)?;
        let bytes = serde_json::to_vec_pretty(&self.manifest).map_err(|_| invalid())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid());
        }
        crate::state::write_atomic_file_with_hook(&self.path, &bytes, |_| {
            if read_optional(&self.path).ok() != Some(self.original.clone()) {
                return Err(std::io::ErrorKind::WouldBlock.into());
            }
            Ok(())
        })
        .map_err(|_| changed())?;
        self.original = Some(bytes);
        Ok(())
    }

    fn check_revision(&self, revision: u64) -> DesktopResult<()> {
        if self.invalid {
            return Err(invalid());
        }
        if revision != self.manifest.revision || read_optional(&self.path)? != self.original {
            return Err(changed());
        }
        Ok(())
    }
}

fn read_optional(path: &Path) -> DesktopResult<Option<Vec<u8>>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid()),
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(invalid());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| invalid())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid());
    }
    Ok(Some(bytes))
}

fn validate_profile(profile: &CodingAgentProfile) -> DesktopResult<()> {
    if validate_provider_id(&profile.provider_id).is_err()
        || profile.name.trim().is_empty()
        || profile.name.len() > 128
        || profile.name.chars().any(char::is_control)
        || !Path::new(&profile.executable).is_absolute()
        || profile.executable.len() > 1024
        || profile.executable.chars().any(char::is_control)
        || profile.args.len() > 64
        || profile
            .args
            .iter()
            .any(|a| a.len() > 4096 || a.contains('\0'))
        || profile.args.iter().map(|a| a.len() + 1).sum::<usize>() > 16 * 1024
        || profile.env_from_env.len() > 64
        || profile.allowed_config_options.len() > 64
    {
        return Err(invalid());
    }
    let mut destinations = BTreeSet::new();
    for (child, source) in &profile.env_from_env {
        for name in [child, source] {
            let upper = name.to_ascii_uppercase();
            if name.is_empty()
                || name.len() > 256
                || !name.bytes().enumerate().all(|(i, b)| {
                    b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit())
                })
                || upper.starts_with("WEBCODEX_")
                || upper == "AUTHORIZATION"
            {
                return Err(invalid());
            }
        }
        if !destinations.insert(if cfg!(windows) {
            child.to_ascii_uppercase()
        } else {
            child.clone()
        }) {
            return Err(invalid());
        }
    }
    let mut options = BTreeSet::new();
    for option in &profile.allowed_config_options {
        if option.is_empty()
            || option.len() > 128
            || option.chars().any(char::is_control)
            || !options.insert(option)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &Manifest) -> DesktopResult<()> {
    if manifest.schema_version != 1
        || uuid::Uuid::parse_str(&manifest.owner_id).is_err()
        || manifest.profiles.len() > MAX_SAVED
        || manifest.managed_ids.len() > MAX_OWNED_IDS
        || manifest.profiles.iter().filter(|p| p.enabled).count() > CODING_AGENT_MAX_PROVIDERS
        || manifest
            .managed_ids
            .iter()
            .any(|id| validate_provider_id(id).is_err())
    {
        return Err(invalid());
    }
    if let Some(settings) = &manifest.global_settings {
        if !(1..=8).contains(&settings.max_concurrent_runs)
            || !(1..=60).contains(&settings.permission_timeout_secs)
        {
            return Err(invalid());
        }
    }
    let mut ids = BTreeSet::new();
    for profile in &manifest.profiles {
        validate_profile(profile)?;
        if !ids.insert(&profile.provider_id) || !manifest.managed_ids.contains(&profile.provider_id)
        {
            return Err(invalid());
        }
    }
    Ok(())
}

pub fn invalid() -> DesktopError {
    DesktopError::new("coding_agent_config_invalid", "Coding Agent configuration is invalid", "Use a logical Provider ID, an absolute executable path, bounded arguments, and environment variable names only. Refresh before retrying.")
}
pub fn conflict() -> DesktopError {
    DesktopError::new(
        "coding_agent_ownership_conflict",
        "Provider ID belongs to another configuration owner",
        "Choose another Provider ID. Desktop will not replace operator-owned Coding Agents.",
    )
}
fn changed() -> DesktopError {
    DesktopError::new(
        "coding_agent_state_changed",
        "Coding Agents changed since this form was opened",
        "Refresh Coding Agents before saving again.",
    )
}
fn executable_unavailable() -> DesktopError {
    DesktopError::new("coding_agent_executable_unavailable", "Coding Agent executable is unavailable", "Install the ACP provider first and select its absolute executable path. No provider is downloaded automatically.")
}

#[cfg(test)]
pub(crate) mod tests;
