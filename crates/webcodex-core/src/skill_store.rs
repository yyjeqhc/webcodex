use serde::{Deserialize, Serialize};

pub const SKILL_STORE_RESPONSE_FORMAT: &str = "webcodex.runner_skill_store.v1";
pub const MAX_OPERATOR_SKILLS: usize = 256;
pub const MAX_OPERATOR_REVISIONS_PER_SKILL: usize = 64;
pub const MAX_OPERATOR_SKILL_KEY_CHARS: usize = 96;
pub const MAX_SKILL_STORE_IDEMPOTENCY_KEY_CHARS: usize = 128;
/// Durable replay records are retention-bounded rather than permanent. A
/// pre-effect claim binds one intent for 24 hours; once an operation reaches
/// the prepared/effect boundary, its recovery record is retained for 7 days.
pub const SKILL_STORE_REPLAY_CLAIMED_RETENTION_SECS: u64 = 24 * 60 * 60;
pub const SKILL_STORE_REPLAY_EFFECT_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;
/// Hard retained replay cardinality. New idempotency keys fail closed at this
/// bound; unexpired prepared/completed records are never evicted for capacity.
pub const MAX_SKILL_STORE_REPLAY_RECORDS: usize = 1024;
/// Lazy GC never walks an unbounded replay directory. This allows bounded
/// cleanup of modest legacy overflow while remaining a small fixed multiple of
/// the retained record cap.
pub const MAX_SKILL_STORE_REPLAY_SCAN_ENTRIES: usize = MAX_SKILL_STORE_REPLAY_RECORDS * 4;
pub const MAX_SKILL_STORE_REPLAY_RECORD_BYTES: usize = 64 * 1024;
/// Permanent retained replay JSON ceiling, excluding one transient atomic-write
/// temp file: 1024 records * 64 KiB = 64 MiB.
pub const MAX_SKILL_STORE_REPLAY_RETAINED_BYTES: usize =
    MAX_SKILL_STORE_REPLAY_RECORDS * MAX_SKILL_STORE_REPLAY_RECORD_BYTES;
pub const MAX_SKILL_STORE_ARCHIVE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SKILL_STORE_TOTAL_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_SKILL_STORE_FILE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_SKILL_STORE_FILE_COUNT: usize = 512;
pub const MAX_SKILL_STORE_PATH_CHARS: usize = 512;
pub const MAX_SKILL_STORE_PATH_DEPTH: usize = 16;
pub const MAX_SKILL_STORE_READ_LINES: usize = 400;
pub const MAX_SKILL_STORE_READ_TEXT_BYTES: usize = 48 * 1024;
pub const MAX_SKILL_STORE_VERSIONS_LIMIT: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SkillStoreRequest {
    ListActive,
    Versions {
        skill_key: String,
        #[serde(default)]
        offset: usize,
        limit: usize,
    },
    Read {
        skill_id: String,
        path: String,
        start_line: usize,
        limit: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_package_revision: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_definition_revision: Option<String>,
    },
    Install {
        skill_key: String,
        /// Authorized source Project identity used as part of install intent.
        source_project_id: String,
        /// Runner-native source project root derived by Control after project
        /// authorization. This never appears in model-facing results/audit.
        source_project_root: String,
        artifact_path: String,
        expected_artifact_sha256: String,
        idempotency_key: String,
        #[serde(default)]
        activate: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expected_state_revision: Option<String>,
    },
    Activate {
        skill_key: String,
        package_revision: String,
        expected_state_revision: String,
        idempotency_key: String,
    },
    RemoveRevision {
        skill_key: String,
        package_revision: String,
        expected_state_revision: String,
        idempotency_key: String,
    },
}

impl SkillStoreRequest {
    /// Revision inventory belongs to the management surface even though it is
    /// read-only. Active guidance discovery/read remains a separate capability.
    pub fn requires_management_capability(&self) -> bool {
        matches!(
            self,
            Self::Versions { .. }
                | Self::Install { .. }
                | Self::Activate { .. }
                | Self::RemoveRevision { .. }
        )
    }

    pub fn is_mutation(&self) -> bool {
        matches!(
            self,
            Self::Install { .. } | Self::Activate { .. } | Self::RemoveRevision { .. }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerSkillDescriptor {
    pub skill_id: String,
    pub skill_key: String,
    pub name: String,
    pub description: String,
    pub package_revision: String,
    pub definition_revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerSkillVersion {
    pub package_revision: String,
    pub definition_revision: String,
    pub name: String,
    pub description: String,
    pub file_count: usize,
    pub total_bytes: usize,
    pub installed_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreListActiveResponse {
    pub format: String,
    pub namespace_revision: String,
    pub skills: Vec<RunnerSkillDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreVersionsResponse {
    pub format: String,
    pub skill_id: String,
    pub skill_key: String,
    pub state_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_package_revision: Option<String>,
    pub total_count: usize,
    pub offset: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
    pub versions: Vec<RunnerSkillVersion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreReadResponse {
    pub format: String,
    pub skill_id: String,
    pub skill_key: String,
    pub name: String,
    pub description: String,
    pub package_revision: String,
    pub definition_revision: String,
    pub path: String,
    pub sha256: String,
    pub text: String,
    pub start_line: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    pub returned_lines: usize,
    pub has_more: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_start_line: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreInstallResponse {
    pub format: String,
    pub skill_id: String,
    pub skill_key: String,
    pub package_revision: String,
    pub definition_revision: String,
    pub artifact_sha256: String,
    pub file_count: usize,
    pub total_bytes: usize,
    pub installed: bool,
    pub activated: bool,
    pub replayed: bool,
    pub state_revision: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_package_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreActivateResponse {
    pub format: String,
    pub skill_id: String,
    pub skill_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_active_package_revision: Option<String>,
    pub active_package_revision: String,
    pub state_revision: String,
    pub changed: bool,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillStoreRemoveResponse {
    pub format: String,
    pub skill_id: String,
    pub skill_key: String,
    pub package_revision: String,
    pub state_revision: String,
    pub removed: bool,
    pub replayed: bool,
}

pub fn valid_skill_key(value: &str) -> bool {
    !value.is_empty()
        && value.chars().count() <= MAX_OPERATOR_SKILL_KEY_CHARS
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        && value != "."
        && value != ".."
}

pub fn valid_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn valid_package_revision(value: &str) -> bool {
    value
        .strip_prefix("wc_skillpkg_")
        .is_some_and(valid_lower_sha256)
}

pub fn valid_state_revision(value: &str) -> bool {
    value
        .strip_prefix("wc_skillstate_")
        .is_some_and(valid_lower_sha256)
}
