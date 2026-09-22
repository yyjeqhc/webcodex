//! The small, additive Desktop/management-CLI contract, independent of build identity.
//!
//! Generation 1 specifies structured build identity, the existing CLI JSON setup,
//! status and workspace operations, and parent-liveness process ownership. Runner
//! wire admission and individual operations still use their own generation and
//! `RunnerFeature` capability fences. This declaration is not a code trust claim.
use serde::{Deserialize, Serialize};

pub const BUILD_INFO_SCHEMA_VERSION: u16 = 1;
pub const RELEASE_MANIFEST_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopRuntimeContract {
    pub min_generation: u16,
    pub max_generation: u16,
}

pub const DESKTOP_RUNTIME_CONTRACT: DesktopRuntimeContract = DesktopRuntimeContract {
    min_generation: 1,
    max_generation: 1,
};

impl DesktopRuntimeContract {
    pub const fn is_valid(self) -> bool {
        self.min_generation > 0 && self.min_generation <= self.max_generation
    }

    pub const fn overlaps(self, other: Self) -> bool {
        self.is_valid()
            && other.is_valid()
            && self.min_generation <= other.max_generation
            && other.min_generation <= self.max_generation
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        self.overlaps(other).then(|| Self {
            min_generation: self.min_generation.max(other.min_generation),
            max_generation: self.max_generation.min(other.max_generation),
        })
    }
}

/// Owned wire form: old readers ignore additive fields, but an unsupported
/// schema or a malformed declared contract is never inferred to be compatible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MachineBuildInfo {
    pub schema_version: u16,
    pub binary: String,
    pub version: String,
    pub git_commit: Option<String>,
    pub git_dirty: Option<bool>,
    pub built_at: Option<String>,
    pub target: String,
    pub architecture: String,
    pub desktop_runtime_contract: DesktopRuntimeContract,
    #[serde(default)]
    pub agent_protocol_generation: Option<u16>,
}

impl MachineBuildInfo {
    /// Validate bounded diagnostic identifiers, not the trustworthiness of code.
    /// No remote strings are returned in errors, reports or log messages.
    pub fn validate(&self, expected_binary: &str) -> Result<(), &'static str> {
        if self.schema_version != BUILD_INFO_SCHEMA_VERSION {
            return Err("build_info_schema_unsupported");
        }
        if self.binary != expected_binary {
            return Err("build_info_binary_mismatch");
        }
        if !identifier(&self.version, 96)
            || !identifier(&self.target, 128)
            || !identifier(&self.architecture, 32)
        {
            return Err("build_info_metadata_invalid");
        }
        if self.git_commit.as_deref().is_some_and(|v| {
            v != "unknown"
                && (v.len() < 7 || v.len() > 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        }) || self
            .built_at
            .as_deref()
            .is_some_and(|v| v.is_empty() || v.len() > 32 || !v.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err("build_info_metadata_invalid");
        }
        if !self.desktop_runtime_contract.is_valid() {
            return Err("runtime_contract_malformed");
        }
        Ok(())
    }
}

fn identifier(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolCompatibility {
    Compatible,
    Incompatible,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BuildAlignment {
    Exact,
    DifferentVersion,
    DifferentCommit,
    Dirty,
    Unknown,
}

pub fn build_alignment(
    left_version: Option<&str>,
    left_commit: Option<&str>,
    left_dirty: Option<bool>,
    right_version: Option<&str>,
    right_commit: Option<&str>,
    right_dirty: Option<bool>,
) -> BuildAlignment {
    if left_dirty == Some(true) || right_dirty == Some(true) {
        return BuildAlignment::Dirty;
    }
    if let (Some(left), Some(right)) = (left_version, right_version) {
        if left != right {
            return BuildAlignment::DifferentVersion;
        }
    } else {
        return BuildAlignment::Unknown;
    }
    fn known(value: Option<&str>) -> Option<&str> {
        value.filter(|s| !s.is_empty() && *s != "unknown")
    }
    match (
        known(left_commit),
        known(right_commit),
        left_dirty,
        right_dirty,
    ) {
        (Some(left), Some(right), _, _) if left != right => BuildAlignment::DifferentCommit,
        (Some(_), Some(_), Some(false), Some(false)) => BuildAlignment::Exact,
        _ => BuildAlignment::Unknown,
    }
}

/// Mirrors the authoritative Runner wire ingress generation; optional features
/// remain independently fenced by RunnerFeature, never by build identity.
pub fn runner_protocol_compatibility(generation: u16) -> ProtocolCompatibility {
    if generation == crate::runner_protocol::RUNNER_PROTOCOL_GENERATION_V2.get() {
        ProtocolCompatibility::Compatible
    } else if generation == 0 {
        ProtocolCompatibility::Unknown
    } else {
        ProtocolCompatibility::Incompatible
    }
}

/// Release metadata is advisory only: it never authorizes installation or launch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReleaseManifest {
    pub schema_version: u16,
    pub release_version: String,
    pub runtime_version: String,
    pub desktop_runtime_contract: DesktopRuntimeContract,
}

#[cfg(test)]
mod tests;
