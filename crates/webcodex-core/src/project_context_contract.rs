//! Durable, side-effect-free project-context fingerprint contracts.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectContextFingerprint {
    pub schema_version: u32,
    /// Hash of the canonical absolute project path. The path itself is never
    /// serialized into continuity state.
    pub project_root_sha256: String,
    pub target_directory: String,
    pub git: GitContextFingerprint,
    pub rules: Vec<ContextFileFingerprint>,
    pub manifests: Vec<ContextFileFingerprint>,
    #[serde(default)]
    pub completeness: FingerprintCompleteness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FingerprintCompleteness {
    #[serde(default = "default_true")]
    pub complete: bool,
    #[serde(default)]
    pub partial_slices: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Default for FingerprintCompleteness {
    fn default() -> Self {
        Self {
            complete: true,
            partial_slices: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitContextFingerprint {
    pub available: bool,
    pub branch: Option<String>,
    pub head: Option<String>,
    pub worktree_sha256: Option<String>,
    pub dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContextFileFingerprint {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    #[serde(default = "default_true")]
    pub complete: bool,
    #[serde(default = "default_full_hash_kind")]
    pub hash_kind: String,
    #[serde(default)]
    pub modified_unix_nanos: Option<u64>,
}

fn default_true() -> bool {
    true
}

fn default_full_hash_kind() -> String {
    "full".to_string()
}
