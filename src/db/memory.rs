use super::Database;
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub(crate) const MAX_MEMORIES_PER_PROJECT: usize = 256;
pub(crate) const MAX_MEMORIES_GLOBAL: usize = 8_192;
pub(crate) const MAX_MEMORY_KEY_CHARS: usize = 96;
pub(crate) const MAX_MEMORY_SUMMARY_CHARS: usize = 512;
pub(crate) const MAX_MEMORY_BODY_BYTES: usize = 8 * 1024;
pub(crate) const MAX_MEMORY_TAGS: usize = 8;
pub(crate) const MAX_MEMORY_TAG_CHARS: usize = 64;
pub(crate) const MAX_MEMORY_QUERY_CHARS: usize = 200;
pub(crate) const MAX_MEMORY_SEARCH_LIMIT: usize = 50;
pub(crate) const MAX_MEMORY_BOOTSTRAP_BYTES: usize = 8 * 1024;
pub(crate) const MAX_MEMORY_SEARCH_RESULT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_MEMORY_SCOPE_LIST_LIMIT: usize = 100;

const MEMORY_ID_PREFIX: &str = "wc_mem_";
const MEMORY_DEFINITION_HASH_PREFIX: &str = "wc_memdef_";
const MEMORY_REVISION_PREFIX: &str = "wc_memrev_";
const MEMORY_CATALOG_REVISION_PREFIX: &str = "wc_memcat_";
const MEMORY_ROOT_FINGERPRINT_PREFIX: &str = "wc_memroot_";
const MEMORY_PRINCIPAL_DIGEST_PREFIX: &str = "wc_memprincipal_";
const MAX_MEMORY_TAGS_JSON_BYTES: usize = 4 * 1024;
const MEMORY_PROVENANCE_KINDS: &[&str] = &[
    "dev",
    "bootstrap",
    "api_token",
    "user",
    "agent_token",
    "account_credential",
    "oauth2",
    "shared-key",
    "project-credential",
    "open",
];
const MAX_MEMORY_PRINCIPAL_KIND_CHARS: usize = 64;
const MAX_MEMORY_PROJECT_RUNTIME_ID_CHARS: usize = 512;
const MAX_MEMORY_RUNNER_CLIENT_ID_CHARS: usize = 128;

pub(crate) const MEMORY_SCOPE_IDENTITY_ATTRIBUTED: &str = "attributed";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum MemoryPriority {
    High,
    Normal,
    Low,
}

impl MemoryPriority {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Normal => "normal",
            Self::Low => "low",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self, MemoryStoreError> {
        match value {
            "high" => Ok(Self::High),
            "normal" => Ok(Self::Normal),
            "low" => Ok(Self::Low),
            _ => Err(MemoryStoreError::InvalidPriority),
        }
    }

    pub(crate) fn bootstrap_rank(self) -> u8 {
        match self {
            Self::High => 0,
            Self::Normal => 1,
            Self::Low => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectMemoryRecord {
    pub(crate) memory_id: String,
    pub(crate) memory_key: String,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) priority: MemoryPriority,
    pub(crate) bootstrap: bool,
    pub(crate) tags: Vec<String>,
    /// Canonical model-relevant definition identity. Internal durable metadata;
    /// it is deliberately distinct from the model-facing CAS state revision.
    pub(crate) definition_hash: String,
    pub(crate) created_by_kind: String,
    pub(crate) created_by_principal_digest: Option<String>,
    pub(crate) updated_by_kind: String,
    pub(crate) updated_by_principal_digest: Option<String>,
    /// Monotonic generation within the current memory_id incarnation.
    pub(crate) generation: u64,
    /// Opaque state-generation ETag used for read/update/delete CAS.
    pub(crate) revision: String,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) updated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryPrincipalAttribution {
    pub(crate) kind: String,
    pub(crate) principal_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryScopeAttribution {
    pub(crate) project_runtime_id: String,
    pub(crate) runner_client_id: String,
    pub(crate) root_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectMemoryScopeRecord {
    pub(crate) memory_scope_id: String,
    pub(crate) identity_state: String,
    pub(crate) project_runtime_id: Option<String>,
    pub(crate) runner_client_id: Option<String>,
    pub(crate) root_fingerprint: Option<String>,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) last_mutated_at_unix_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectMemoryScopeSnapshot {
    pub(crate) scope: ProjectMemoryScopeRecord,
    pub(crate) memories: Vec<ProjectMemoryRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryScopePurgeOutcome {
    pub(crate) memory_scope_id: String,
    pub(crate) catalog_revision: Option<String>,
    pub(crate) purged_count: usize,
    pub(crate) purged: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemorySetInput {
    pub(crate) memory_key: String,
    pub(crate) summary: String,
    pub(crate) body: String,
    pub(crate) priority: MemoryPriority,
    pub(crate) bootstrap: bool,
    pub(crate) tags: Vec<String>,
    pub(crate) expected_revision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemorySetOutcome {
    pub(crate) record: ProjectMemoryRecord,
    pub(crate) old_revision: Option<String>,
    pub(crate) created: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryDeleteOutcome {
    pub(crate) memory_id: Option<String>,
    pub(crate) revision: Option<String>,
    pub(crate) deleted: bool,
    pub(crate) state_changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MemoryStoreError {
    InvalidScope,
    InvalidKey,
    InvalidSummary,
    InvalidBody,
    InvalidPriority,
    InvalidTags,
    InvalidRevision,
    InvalidQuery,
    InvalidLimit,
    NotFound,
    Changed { current_revision: String },
    ExpectedRevisionRequired { current_revision: String },
    ScopeChanged { current_catalog_revision: String },
    ProjectCapacityExceeded,
    GlobalCapacityExceeded,
    DatabaseUnavailable,
}

impl MemoryStoreError {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::InvalidScope => "memory_scope_invalid",
            Self::InvalidKey => "memory_key_invalid",
            Self::InvalidSummary => "memory_summary_invalid",
            Self::InvalidBody => "memory_body_invalid",
            Self::InvalidPriority => "memory_priority_invalid",
            Self::InvalidTags => "memory_tags_invalid",
            Self::InvalidRevision => "memory_revision_invalid",
            Self::InvalidQuery => "memory_query_invalid",
            Self::InvalidLimit => "memory_limit_invalid",
            Self::NotFound => "memory_not_found",
            Self::Changed { .. } => "memory_changed",
            Self::ExpectedRevisionRequired { .. } => "memory_expected_revision_required",
            Self::ScopeChanged { .. } => "memory_scope_changed",
            Self::ProjectCapacityExceeded => "memory_project_capacity_exceeded",
            Self::GlobalCapacityExceeded => "memory_global_capacity_exceeded",
            Self::DatabaseUnavailable => "memory_store_unavailable",
        }
    }

    pub(crate) fn current_revision(&self) -> Option<&str> {
        match self {
            Self::Changed { current_revision }
            | Self::ExpectedRevisionRequired { current_revision } => Some(current_revision),
            _ => None,
        }
    }

    pub(crate) fn current_catalog_revision(&self) -> Option<&str> {
        match self {
            Self::ScopeChanged {
                current_catalog_revision,
            } => Some(current_catalog_revision),
            _ => None,
        }
    }
}

fn validate_scope(scope_id: &str) -> Result<(), MemoryStoreError> {
    if scope_id.len() == "wc_memscope_".len() + 64
        && scope_id.starts_with("wc_memscope_")
        && scope_id["wc_memscope_".len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(MemoryStoreError::InvalidScope)
    }
}

pub(crate) fn validate_memory_key(value: &str) -> Result<(), MemoryStoreError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.chars().count() > MAX_MEMORY_KEY_CHARS
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        Err(MemoryStoreError::InvalidKey)
    } else {
        Ok(())
    }
}

fn valid_guidance_char(ch: char) -> bool {
    !ch.is_control() || matches!(ch, '\n' | '\t')
}

pub(crate) fn validate_memory_summary(value: &str) -> Result<(), MemoryStoreError> {
    if value.trim().is_empty()
        || value.chars().count() > MAX_MEMORY_SUMMARY_CHARS
        || value.chars().any(|ch| !valid_guidance_char(ch))
    {
        Err(MemoryStoreError::InvalidSummary)
    } else {
        Ok(())
    }
}

pub(crate) fn validate_memory_body(value: &str) -> Result<(), MemoryStoreError> {
    if value.len() > MAX_MEMORY_BODY_BYTES || value.chars().any(|ch| !valid_guidance_char(ch)) {
        Err(MemoryStoreError::InvalidBody)
    } else {
        Ok(())
    }
}

pub(crate) fn canonicalize_memory_tags(tags: Vec<String>) -> Result<Vec<String>, MemoryStoreError> {
    if tags.len() > MAX_MEMORY_TAGS {
        return Err(MemoryStoreError::InvalidTags);
    }
    let mut canonical = BTreeSet::new();
    for tag in tags {
        if tag.is_empty()
            || tag.chars().count() > MAX_MEMORY_TAG_CHARS
            || tag.chars().any(char::is_control)
        {
            return Err(MemoryStoreError::InvalidTags);
        }
        canonical.insert(tag);
    }
    if canonical.len() > MAX_MEMORY_TAGS {
        return Err(MemoryStoreError::InvalidTags);
    }
    Ok(canonical.into_iter().collect())
}

pub(crate) fn validate_memory_query(query: &str) -> Result<(), MemoryStoreError> {
    if query.chars().count() > MAX_MEMORY_QUERY_CHARS || query.chars().any(char::is_control) {
        Err(MemoryStoreError::InvalidQuery)
    } else {
        Ok(())
    }
}

fn valid_lower_hex_prefixed(value: &str, prefix: &str, hex_len: usize) -> bool {
    value.len() == prefix.len() + hex_len
        && value.starts_with(prefix)
        && value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub(crate) fn validate_memory_scope_id(value: &str) -> Result<(), MemoryStoreError> {
    validate_scope(value)
}

pub(crate) fn valid_memory_root_fingerprint(value: &str) -> bool {
    valid_lower_hex_prefixed(value, MEMORY_ROOT_FINGERPRINT_PREFIX, 64)
}

pub(crate) fn valid_memory_principal_digest(value: &str) -> bool {
    valid_lower_hex_prefixed(value, MEMORY_PRINCIPAL_DIGEST_PREFIX, 64)
}

fn valid_bounded_identity_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.chars().count() <= maximum && !value.chars().any(char::is_control)
}
fn valid_memory_principal_kind(value: &str) -> bool {
    MEMORY_PROVENANCE_KINDS.contains(&value)
}

fn validate_principal_attribution(
    attribution: &MemoryPrincipalAttribution,
) -> Result<(), MemoryStoreError> {
    if !valid_bounded_identity_text(&attribution.kind, MAX_MEMORY_PRINCIPAL_KIND_CHARS)
        || !valid_memory_principal_kind(&attribution.kind)
        || !valid_memory_principal_digest(&attribution.principal_digest)
    {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    Ok(())
}

fn validate_scope_attribution(
    attribution: &MemoryScopeAttribution,
) -> Result<(), MemoryStoreError> {
    if !valid_bounded_identity_text(
        &attribution.project_runtime_id,
        MAX_MEMORY_PROJECT_RUNTIME_ID_CHARS,
    ) || !valid_bounded_identity_text(
        &attribution.runner_client_id,
        MAX_MEMORY_RUNNER_CLIENT_ID_CHARS,
    ) || !valid_memory_root_fingerprint(&attribution.root_fingerprint)
    {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    Ok(())
}

fn validate_memory_id(value: &str) -> Result<(), MemoryStoreError> {
    if valid_lower_hex_prefixed(value, MEMORY_ID_PREFIX, 32) {
        Ok(())
    } else {
        Err(MemoryStoreError::DatabaseUnavailable)
    }
}

fn validate_memory_definition_hash(value: &str) -> Result<(), MemoryStoreError> {
    if valid_lower_hex_prefixed(value, MEMORY_DEFINITION_HASH_PREFIX, 64) {
        Ok(())
    } else {
        Err(MemoryStoreError::DatabaseUnavailable)
    }
}

pub(crate) fn validate_memory_revision(value: &str) -> Result<(), MemoryStoreError> {
    if valid_lower_hex_prefixed(value, MEMORY_REVISION_PREFIX, 64) {
        Ok(())
    } else {
        Err(MemoryStoreError::InvalidRevision)
    }
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn memory_definition_digest(
    memory_key: &str,
    summary: &str,
    body: &str,
    priority: MemoryPriority,
    bootstrap: bool,
    tags: &[String],
) -> sha2::digest::Output<Sha256> {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-project-memory-definition-v1\0");
    for value in [memory_key.as_bytes(), summary.as_bytes(), body.as_bytes()] {
        hash_field(&mut hasher, value);
    }
    hash_field(&mut hasher, priority.as_str().as_bytes());
    hasher.update([u8::from(bootstrap)]);
    hasher.update((tags.len() as u64).to_be_bytes());
    for tag in tags {
        hash_field(&mut hasher, tag.as_bytes());
    }
    hasher.finalize()
}

pub(crate) fn memory_definition_hash(
    memory_key: &str,
    summary: &str,
    body: &str,
    priority: MemoryPriority,
    bootstrap: bool,
    tags: &[String],
) -> String {
    format!(
        "{MEMORY_DEFINITION_HASH_PREFIX}{:x}",
        memory_definition_digest(memory_key, summary, body, priority, bootstrap, tags)
    )
}

pub(crate) fn memory_state_revision(
    memory_scope_id: &str,
    memory_id: &str,
    generation: u64,
    definition_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-project-memory-state-v2\0");
    hash_field(&mut hasher, memory_scope_id.as_bytes());
    hash_field(&mut hasher, memory_id.as_bytes());
    hash_field(&mut hasher, &generation.to_be_bytes());
    hash_field(&mut hasher, definition_hash.as_bytes());
    format!("{MEMORY_REVISION_PREFIX}{:x}", hasher.finalize())
}

fn validate_timestamp_pair(
    created_at_unix_ms: i64,
    updated_at_unix_ms: i64,
) -> Result<(), MemoryStoreError> {
    if created_at_unix_ms < 0 || updated_at_unix_ms < created_at_unix_ms {
        Err(MemoryStoreError::DatabaseUnavailable)
    } else {
        Ok(())
    }
}

fn parse_canonical_tags_json(tags_json: &str) -> Result<Vec<String>, MemoryStoreError> {
    if tags_json.len() > MAX_MEMORY_TAGS_JSON_BYTES {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    let tags: Vec<String> =
        serde_json::from_str(tags_json).map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
    let canonical = canonicalize_memory_tags(tags.clone())
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
    if canonical != tags {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    Ok(tags)
}

fn validate_provenance_pair(kind: &str, digest: Option<&str>) -> Result<(), MemoryStoreError> {
    if !valid_bounded_identity_text(kind, MAX_MEMORY_PRINCIPAL_KIND_CHARS)
        || !valid_memory_principal_kind(kind)
        || !digest.is_some_and(valid_memory_principal_digest)
    {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    Ok(())
}

fn parse_scope_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectMemoryScopeRecord> {
    // Reject malformed or hostile persisted metadata before materializing its
    // TEXT values. Scope inventory is bounded by Memory cardinality, and each
    // descriptor must also remain bounded independently.
    if row.get::<_, i64>(7)? != ("wc_memscope_".len() + 64) as i64 {
        return Err(corrupt_row(
            0,
            rusqlite::types::Type::Text,
            "invalid Memory scope id length",
        ));
    }
    for (length_column, value_column, maximum, message) in [
        (
            8,
            1,
            MEMORY_SCOPE_IDENTITY_ATTRIBUTED.len(),
            "invalid Memory scope identity_state length",
        ),
        (
            9,
            2,
            MAX_MEMORY_PROJECT_RUNTIME_ID_CHARS,
            "invalid Memory project attribution length",
        ),
        (
            10,
            3,
            MAX_MEMORY_RUNNER_CLIENT_ID_CHARS,
            "invalid Memory Runner attribution length",
        ),
    ] {
        let length = row.get::<_, i64>(length_column)?;
        if length < 0 || length > maximum as i64 {
            return Err(corrupt_row(
                value_column,
                rusqlite::types::Type::Text,
                message,
            ));
        }
    }
    let root_length = row.get::<_, i64>(11)?;
    if root_length != 0 && root_length != (MEMORY_ROOT_FINGERPRINT_PREFIX.len() + 64) as i64 {
        return Err(corrupt_row(
            4,
            rusqlite::types::Type::Text,
            "invalid Memory root attribution length",
        ));
    }

    let memory_scope_id: String = row.get(0)?;
    let identity_state: String = row.get(1)?;
    let project_runtime_id: Option<String> = row.get(2)?;
    let runner_client_id: Option<String> = row.get(3)?;
    let root_fingerprint: Option<String> = row.get(4)?;
    let created_at_unix_ms: i64 = row.get(5)?;
    let last_mutated_at_unix_ms: i64 = row.get(6)?;
    validate_scope(&memory_scope_id)
        .map_err(|_| corrupt_row(0, rusqlite::types::Type::Text, "invalid Memory scope id"))?;
    validate_timestamp_pair(created_at_unix_ms, last_mutated_at_unix_ms).map_err(|_| {
        corrupt_row(
            5,
            rusqlite::types::Type::Integer,
            "invalid Memory scope timestamps",
        )
    })?;
    match identity_state.as_str() {
        MEMORY_SCOPE_IDENTITY_ATTRIBUTED => {
            let attributed = MemoryScopeAttribution {
                project_runtime_id: project_runtime_id.clone().ok_or_else(|| {
                    corrupt_row(
                        2,
                        rusqlite::types::Type::Null,
                        "missing Memory project attribution",
                    )
                })?,
                runner_client_id: runner_client_id.clone().ok_or_else(|| {
                    corrupt_row(
                        3,
                        rusqlite::types::Type::Null,
                        "missing Memory Runner attribution",
                    )
                })?,
                root_fingerprint: root_fingerprint.clone().ok_or_else(|| {
                    corrupt_row(
                        4,
                        rusqlite::types::Type::Null,
                        "missing Memory root attribution",
                    )
                })?,
            };
            validate_scope_attribution(&attributed).map_err(|_| {
                corrupt_row(
                    2,
                    rusqlite::types::Type::Text,
                    "invalid Memory scope attribution",
                )
            })?;
        }
        _ => {
            return Err(corrupt_row(
                1,
                rusqlite::types::Type::Text,
                "invalid Memory scope identity_state",
            ))
        }
    }
    Ok(ProjectMemoryScopeRecord {
        memory_scope_id,
        identity_state,
        project_runtime_id,
        runner_client_id,
        root_fingerprint,
        created_at_unix_ms,
        last_mutated_at_unix_ms,
    })
}

const SELECT_SCOPE: &str = "SELECT memory_scope_id, identity_state, project_runtime_id,
    runner_client_id, root_fingerprint, created_at_unix_ms, last_mutated_at_unix_ms,
    length(memory_scope_id), length(identity_state), COALESCE(length(project_runtime_id), 0),
    COALESCE(length(runner_client_id), 0), COALESCE(length(root_fingerprint), 0)
    FROM project_memory_scopes";

fn get_scope_record(
    conn: &rusqlite::Connection,
    memory_scope_id: &str,
) -> Result<Option<ProjectMemoryScopeRecord>, MemoryStoreError> {
    conn.query_row(
        &format!("{SELECT_SCOPE} WHERE memory_scope_id = ?1"),
        params![memory_scope_id],
        parse_scope_row,
    )
    .optional()
    .map_err(|_| MemoryStoreError::DatabaseUnavailable)
}

fn validate_scope_record_for_memories(
    conn: &rusqlite::Connection,
    memory_scope_id: &str,
    has_memories: bool,
) -> Result<Option<ProjectMemoryScopeRecord>, MemoryStoreError> {
    let scope = get_scope_record(conn, memory_scope_id)?;
    if scope.is_some() != has_memories {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    Ok(scope)
}

pub(crate) fn memory_catalog_revision(records: &[ProjectMemoryRecord]) -> String {
    let mut pairs = records
        .iter()
        .map(|record| (record.memory_key.as_str(), record.revision.as_str()))
        .collect::<Vec<_>>();
    pairs.sort_unstable();
    let mut hasher = Sha256::new();
    hasher.update(b"webcodex-project-memory-catalog-v1\0");
    for (key, revision) in pairs {
        hash_field(&mut hasher, key.as_bytes());
        hash_field(&mut hasher, revision.as_bytes());
    }
    format!("{MEMORY_CATALOG_REVISION_PREFIX}{:x}", hasher.finalize())
}

pub(crate) fn valid_memory_catalog_revision(value: &str) -> bool {
    value.len() == MEMORY_CATALOG_REVISION_PREFIX.len() + 64
        && value.starts_with(MEMORY_CATALOG_REVISION_PREFIX)
        && value[MEMORY_CATALOG_REVISION_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn corrupt_row(
    column: usize,
    kind: rusqlite::types::Type,
    reason: &'static str,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        kind,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, reason)),
    )
}

fn parse_row(
    row: &rusqlite::Row<'_>,
    expected_scope: &str,
) -> rusqlite::Result<ProjectMemoryRecord> {
    // Read SQLite length metadata before materializing potentially corrupted
    // large TEXT values into Rust Strings. This is a local Memory-store bound,
    // not a generic DB streaming abstraction.
    let exact_lengths = [
        (17, MEMORY_ID_PREFIX.len() + 32),
        (18, "wc_memscope_".len() + 64),
        (24, MEMORY_DEFINITION_HASH_PREFIX.len() + 64),
        (25, MEMORY_REVISION_PREFIX.len() + 64),
    ];
    for (column, expected) in exact_lengths {
        if row.get::<_, i64>(column)? != expected as i64 {
            return Err(corrupt_row(
                column,
                rusqlite::types::Type::Integer,
                "invalid Memory identity length",
            ));
        }
    }
    let bounded_lengths = [
        (19, MAX_MEMORY_KEY_CHARS),
        (20, MAX_MEMORY_SUMMARY_CHARS),
        (21, MAX_MEMORY_BODY_BYTES),
        (22, 6usize),
        (23, MAX_MEMORY_TAGS_JSON_BYTES),
        (26, MAX_MEMORY_PRINCIPAL_KIND_CHARS),
        (27, MEMORY_PRINCIPAL_DIGEST_PREFIX.len() + 64),
        (28, MAX_MEMORY_PRINCIPAL_KIND_CHARS),
        (29, MEMORY_PRINCIPAL_DIGEST_PREFIX.len() + 64),
    ];
    for (column, maximum) in bounded_lengths {
        let length = row.get::<_, i64>(column)?;
        if length < 0 || length as usize > maximum {
            return Err(corrupt_row(
                column,
                rusqlite::types::Type::Integer,
                "persisted Memory field exceeds bound",
            ));
        }
    }

    let memory_id: String = row.get(0)?;
    let memory_scope_id: String = row.get(1)?;
    let memory_key: String = row.get(2)?;
    let summary: String = row.get(3)?;
    let body: String = row.get(4)?;
    let priority_raw: String = row.get(5)?;
    let bootstrap_raw: i64 = row.get(6)?;
    let tags_json: String = row.get(7)?;
    let definition_hash: String = row.get(8)?;
    let generation_raw: i64 = row.get(9)?;
    let revision: String = row.get(10)?;
    let created_at_unix_ms: i64 = row.get(11)?;
    let updated_at_unix_ms: i64 = row.get(12)?;
    let created_by_kind: String = row.get(13)?;
    let created_by_principal_digest: Option<String> = row.get(14)?;
    let updated_by_kind: String = row.get(15)?;
    let updated_by_principal_digest: Option<String> = row.get(16)?;

    validate_memory_id(&memory_id)
        .map_err(|_| corrupt_row(0, rusqlite::types::Type::Text, "invalid memory_id"))?;
    validate_scope(&memory_scope_id)
        .map_err(|_| corrupt_row(1, rusqlite::types::Type::Text, "invalid memory_scope_id"))?;
    if memory_scope_id != expected_scope {
        return Err(corrupt_row(
            1,
            rusqlite::types::Type::Text,
            "Memory scope mismatch",
        ));
    }
    validate_memory_key(&memory_key)
        .map_err(|_| corrupt_row(2, rusqlite::types::Type::Text, "invalid memory_key"))?;
    validate_memory_summary(&summary)
        .map_err(|_| corrupt_row(3, rusqlite::types::Type::Text, "invalid Memory summary"))?;
    validate_memory_body(&body)
        .map_err(|_| corrupt_row(4, rusqlite::types::Type::Text, "invalid Memory body"))?;
    let priority = MemoryPriority::parse(&priority_raw)
        .map_err(|_| corrupt_row(5, rusqlite::types::Type::Text, "invalid Memory priority"))?;
    let bootstrap = match bootstrap_raw {
        0 => false,
        1 => true,
        _ => {
            return Err(corrupt_row(
                6,
                rusqlite::types::Type::Integer,
                "invalid Memory bootstrap flag",
            ))
        }
    };
    let tags = parse_canonical_tags_json(&tags_json)
        .map_err(|_| corrupt_row(7, rusqlite::types::Type::Text, "invalid Memory tags"))?;
    validate_memory_definition_hash(&definition_hash).map_err(|_| {
        corrupt_row(
            8,
            rusqlite::types::Type::Text,
            "invalid Memory definition hash",
        )
    })?;
    if generation_raw < 1 {
        return Err(corrupt_row(
            9,
            rusqlite::types::Type::Integer,
            "invalid Memory generation",
        ));
    }
    let generation = u64::try_from(generation_raw).map_err(|_| {
        corrupt_row(
            9,
            rusqlite::types::Type::Integer,
            "invalid Memory generation",
        )
    })?;
    validate_memory_revision(&revision)
        .map_err(|_| corrupt_row(10, rusqlite::types::Type::Text, "invalid Memory revision"))?;
    validate_timestamp_pair(created_at_unix_ms, updated_at_unix_ms).map_err(|_| {
        corrupt_row(
            11,
            rusqlite::types::Type::Integer,
            "invalid Memory timestamps",
        )
    })?;
    validate_provenance_pair(&created_by_kind, created_by_principal_digest.as_deref()).map_err(
        |_| {
            corrupt_row(
                13,
                rusqlite::types::Type::Text,
                "invalid Memory created provenance",
            )
        },
    )?;
    validate_provenance_pair(&updated_by_kind, updated_by_principal_digest.as_deref()).map_err(
        |_| {
            corrupt_row(
                15,
                rusqlite::types::Type::Text,
                "invalid Memory updated provenance",
            )
        },
    )?;

    let expected_definition =
        memory_definition_hash(&memory_key, &summary, &body, priority, bootstrap, &tags);
    if definition_hash != expected_definition {
        return Err(corrupt_row(
            8,
            rusqlite::types::Type::Text,
            "Memory definition hash mismatch",
        ));
    }
    let expected_revision =
        memory_state_revision(&memory_scope_id, &memory_id, generation, &definition_hash);
    if revision != expected_revision {
        return Err(corrupt_row(
            10,
            rusqlite::types::Type::Text,
            "Memory state revision mismatch",
        ));
    }

    Ok(ProjectMemoryRecord {
        memory_id,
        memory_key,
        summary,
        body,
        priority,
        bootstrap,
        tags,
        definition_hash,
        created_by_kind,
        created_by_principal_digest,
        updated_by_kind,
        updated_by_principal_digest,
        generation,
        revision,
        created_at_unix_ms,
        updated_at_unix_ms,
    })
}

const SELECT_RECORD: &str =
    "SELECT memory_id, memory_scope_id, memory_key, summary, body, priority, bootstrap, tags_json,
            definition_hash, generation, revision, created_at_unix_ms, updated_at_unix_ms,
            created_by_kind, created_by_principal_digest, updated_by_kind, updated_by_principal_digest,
            length(memory_id), length(memory_scope_id), length(memory_key), length(summary),
            length(CAST(body AS BLOB)), length(priority), length(CAST(tags_json AS BLOB)),
            length(definition_hash), length(revision), length(created_by_kind),
            COALESCE(length(created_by_principal_digest), 0), length(updated_by_kind),
            COALESCE(length(updated_by_principal_digest), 0)
     FROM project_memories";

fn list_project_memories_with_conn(
    conn: &rusqlite::Connection,
    memory_scope_id: &str,
) -> Result<Vec<ProjectMemoryRecord>, MemoryStoreError> {
    let mut statement = conn
        .prepare(&format!(
            "{SELECT_RECORD} WHERE memory_scope_id = ?1 ORDER BY memory_key ASC LIMIT ?2"
        ))
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
    let records = statement
        .query_map(
            params![memory_scope_id, (MAX_MEMORIES_PER_PROJECT + 1) as i64],
            |row| parse_row(row, memory_scope_id),
        )
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
    if records.len() > MAX_MEMORIES_PER_PROJECT {
        return Err(MemoryStoreError::DatabaseUnavailable);
    }
    validate_scope_record_for_memories(conn, memory_scope_id, !records.is_empty())?;
    Ok(records)
}

fn ensure_scope_for_mutation(
    tx: &rusqlite::Transaction<'_>,
    memory_scope_id: &str,
    attribution: &MemoryScopeAttribution,
    now: i64,
    allow_create: bool,
) -> Result<(), MemoryStoreError> {
    validate_scope_attribution(attribution)?;
    match get_scope_record(tx, memory_scope_id)? {
        Some(scope) if scope.identity_state == MEMORY_SCOPE_IDENTITY_ATTRIBUTED => {
            if scope.project_runtime_id.as_deref() != Some(attribution.project_runtime_id.as_str())
                || scope.runner_client_id.as_deref() != Some(attribution.runner_client_id.as_str())
                || scope.root_fingerprint.as_deref() != Some(attribution.root_fingerprint.as_str())
            {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
            let changed = tx
                .execute(
                    "UPDATE project_memory_scopes SET last_mutated_at_unix_ms = ?1
                     WHERE memory_scope_id = ?2",
                    params![now, memory_scope_id],
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            if changed != 1 {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
        }
        Some(_) => return Err(MemoryStoreError::DatabaseUnavailable),
        None if allow_create => {
            tx.execute(
                "INSERT INTO project_memory_scopes
                 (memory_scope_id, identity_state, project_runtime_id, runner_client_id,
                  root_fingerprint, created_at_unix_ms, last_mutated_at_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![
                    memory_scope_id,
                    MEMORY_SCOPE_IDENTITY_ATTRIBUTED,
                    attribution.project_runtime_id,
                    attribution.runner_client_id,
                    attribution.root_fingerprint,
                    now,
                ],
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        }
        None => return Err(MemoryStoreError::DatabaseUnavailable),
    }
    Ok(())
}

impl Database {
    pub(crate) fn list_project_memories(
        &self,
        memory_scope_id: &str,
    ) -> Result<Vec<ProjectMemoryRecord>, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        let conn = self.conn.lock().unwrap();
        list_project_memories_with_conn(&conn, memory_scope_id)
    }

    pub(crate) fn get_project_memory(
        &self,
        memory_scope_id: &str,
        memory_key: &str,
    ) -> Result<Option<ProjectMemoryRecord>, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_memory_key(memory_key)?;
        let conn = self.conn.lock().unwrap();
        let record = conn
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
                params![memory_scope_id, memory_key],
                |row| parse_row(row, memory_scope_id),
            )
            .optional()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        validate_scope_record_for_memories(&conn, memory_scope_id, count > 0)?;
        Ok(record)
    }

    pub(crate) fn set_project_memory_attributed(
        &self,
        memory_scope_id: &str,
        scope_attribution: &MemoryScopeAttribution,
        principal: &MemoryPrincipalAttribution,
        mut input: MemorySetInput,
    ) -> Result<MemorySetOutcome, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_scope_attribution(scope_attribution)?;
        validate_principal_attribution(principal)?;
        validate_memory_key(&input.memory_key)?;
        validate_memory_summary(&input.summary)?;
        validate_memory_body(&input.body)?;
        input.tags = canonicalize_memory_tags(input.tags)?;
        if let Some(expected) = input.expected_revision.as_deref() {
            validate_memory_revision(expected)?;
        }
        let requested_definition_hash = memory_definition_hash(
            &input.memory_key,
            &input.summary,
            &input.body,
            input.priority,
            input.bootstrap,
            &input.tags,
        );
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let existing = tx
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
                params![memory_scope_id, input.memory_key],
                |row| parse_row(row, memory_scope_id),
            )
            .optional()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let scope_memory_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        validate_scope_record_for_memories(&tx, memory_scope_id, scope_memory_count > 0)?;

        if let Some(existing) = existing {
            match input.expected_revision.as_deref() {
                None if existing.definition_hash == requested_definition_hash => {
                    tx.commit()
                        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
                    return Ok(MemorySetOutcome {
                        record: existing,
                        old_revision: None,
                        created: false,
                        state_changed: false,
                    });
                }
                None => {
                    return Err(MemoryStoreError::ExpectedRevisionRequired {
                        current_revision: existing.revision,
                    });
                }
                Some(expected) if expected != existing.revision => {
                    return Err(MemoryStoreError::Changed {
                        current_revision: existing.revision,
                    });
                }
                Some(_) if existing.definition_hash == requested_definition_hash => {
                    tx.commit()
                        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
                    return Ok(MemorySetOutcome {
                        record: existing,
                        old_revision: None,
                        created: false,
                        state_changed: false,
                    });
                }
                Some(_) => {}
            }
            ensure_scope_for_mutation(&tx, memory_scope_id, scope_attribution, now, false)?;
            let old_revision = existing.revision.clone();
            let generation = existing
                .generation
                .checked_add(1)
                .filter(|value| *value <= i64::MAX as u64)
                .ok_or(MemoryStoreError::DatabaseUnavailable)?;
            let requested_revision = memory_state_revision(
                memory_scope_id,
                &existing.memory_id,
                generation,
                &requested_definition_hash,
            );
            let tags_json = serde_json::to_string(&input.tags)
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            let changed = tx
                .execute(
                    "UPDATE project_memories
                     SET summary = ?1, body = ?2, priority = ?3, bootstrap = ?4,
                         tags_json = ?5, definition_hash = ?6, generation = ?7,
                         revision = ?8, updated_at_unix_ms = ?9,
                         updated_by_kind = ?10, updated_by_principal_digest = ?11
                     WHERE memory_scope_id = ?12 AND memory_key = ?13 AND revision = ?14",
                    params![
                        input.summary,
                        input.body,
                        input.priority.as_str(),
                        input.bootstrap,
                        tags_json,
                        requested_definition_hash,
                        generation as i64,
                        requested_revision,
                        now,
                        principal.kind,
                        principal.principal_digest,
                        memory_scope_id,
                        input.memory_key,
                        old_revision,
                    ],
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            if changed != 1 {
                return Err(MemoryStoreError::Changed {
                    current_revision: old_revision,
                });
            }
            let record = tx
                .query_row(
                    &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
                    params![memory_scope_id, input.memory_key],
                    |row| parse_row(row, memory_scope_id),
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            tx.commit()
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            return Ok(MemorySetOutcome {
                record,
                old_revision: Some(old_revision),
                created: false,
                state_changed: true,
            });
        }

        if input.expected_revision.is_some() {
            return Err(MemoryStoreError::NotFound);
        }
        let project_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if project_count >= MAX_MEMORIES_PER_PROJECT as i64 {
            return Err(MemoryStoreError::ProjectCapacityExceeded);
        }
        let global_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM project_memories", [], |row| {
                row.get(0)
            })
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if global_count >= MAX_MEMORIES_GLOBAL as i64 {
            return Err(MemoryStoreError::GlobalCapacityExceeded);
        }
        ensure_scope_for_mutation(
            &tx,
            memory_scope_id,
            scope_attribution,
            now,
            project_count == 0,
        )?;
        let memory_id = format!("{MEMORY_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let generation = 1u64;
        let requested_revision = memory_state_revision(
            memory_scope_id,
            &memory_id,
            generation,
            &requested_definition_hash,
        );
        let tags_json = serde_json::to_string(&input.tags)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        tx.execute(
            "INSERT INTO project_memories
             (memory_id, memory_scope_id, memory_key, summary, body, priority, bootstrap,
              tags_json, definition_hash, generation, revision, created_at_unix_ms, updated_at_unix_ms,
              created_by_kind, created_by_principal_digest, updated_by_kind, updated_by_principal_digest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12, ?13, ?14, ?13, ?14)",
            params![
                memory_id,
                memory_scope_id,
                input.memory_key,
                input.summary,
                input.body,
                input.priority.as_str(),
                input.bootstrap,
                tags_json,
                requested_definition_hash,
                generation as i64,
                requested_revision,
                now,
                principal.kind,
                principal.principal_digest,
            ],
        )
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let record = tx
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_id = ?1"),
                params![memory_id],
                |row| parse_row(row, memory_scope_id),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        tx.commit()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        Ok(MemorySetOutcome {
            record,
            old_revision: None,
            created: true,
            state_changed: true,
        })
    }

    pub(crate) fn delete_project_memory_attributed(
        &self,
        memory_scope_id: &str,
        scope_attribution: &MemoryScopeAttribution,
        memory_key: &str,
        expected_revision: &str,
    ) -> Result<MemoryDeleteOutcome, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_scope_attribution(scope_attribution)?;
        validate_memory_key(memory_key)?;
        validate_memory_revision(expected_revision)?;
        let now = chrono::Utc::now().timestamp_millis();
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let existing = tx
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
                params![memory_scope_id, memory_key],
                |row| parse_row(row, memory_scope_id),
            )
            .optional()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let scope_memory_count: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        validate_scope_record_for_memories(&tx, memory_scope_id, scope_memory_count > 0)?;
        let Some(existing) = existing else {
            tx.commit()
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            return Ok(MemoryDeleteOutcome {
                memory_id: None,
                revision: None,
                deleted: false,
                state_changed: false,
            });
        };
        if existing.revision != expected_revision {
            return Err(MemoryStoreError::Changed {
                current_revision: existing.revision,
            });
        }
        ensure_scope_for_mutation(&tx, memory_scope_id, scope_attribution, now, false)?;
        let deleted = tx
            .execute(
                "DELETE FROM project_memories
                 WHERE memory_scope_id = ?1 AND memory_key = ?2 AND revision = ?3",
                params![memory_scope_id, memory_key, expected_revision],
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if deleted != 1 {
            return Err(MemoryStoreError::Changed {
                current_revision: expected_revision.to_string(),
            });
        }
        let remaining: i64 = tx
            .query_row(
                "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
                |row| row.get(0),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if remaining == 0 {
            let removed_scope = tx
                .execute(
                    "DELETE FROM project_memory_scopes WHERE memory_scope_id = ?1",
                    params![memory_scope_id],
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            if removed_scope != 1 {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
        }
        tx.commit()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        Ok(MemoryDeleteOutcome {
            memory_id: Some(existing.memory_id),
            revision: Some(existing.revision),
            deleted: true,
            state_changed: true,
        })
    }

    #[cfg(test)]
    pub(crate) fn set_project_memory(
        &self,
        memory_scope_id: &str,
        input: MemorySetInput,
    ) -> Result<MemorySetOutcome, MemoryStoreError> {
        let scope_attribution = MemoryScopeAttribution {
            project_runtime_id: "agent:test:memory".to_string(),
            runner_client_id: "test-runner".to_string(),
            root_fingerprint: format!("wc_memroot_{}", "0".repeat(64)),
        };
        let principal = MemoryPrincipalAttribution {
            kind: "dev".to_string(),
            principal_digest: format!("wc_memprincipal_{}", "1".repeat(64)),
        };
        self.set_project_memory_attributed(memory_scope_id, &scope_attribution, &principal, input)
    }

    #[cfg(test)]
    pub(crate) fn delete_project_memory(
        &self,
        memory_scope_id: &str,
        memory_key: &str,
        expected_revision: &str,
    ) -> Result<MemoryDeleteOutcome, MemoryStoreError> {
        let scope_attribution = MemoryScopeAttribution {
            project_runtime_id: "agent:test:memory".to_string(),
            runner_client_id: "test-runner".to_string(),
            root_fingerprint: format!("wc_memroot_{}", "0".repeat(64)),
        };
        self.delete_project_memory_attributed(
            memory_scope_id,
            &scope_attribution,
            memory_key,
            expected_revision,
        )
    }

    pub(crate) fn list_project_memory_scopes(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<(usize, Vec<ProjectMemoryScopeSnapshot>), MemoryStoreError> {
        if limit == 0 || limit > MAX_MEMORY_SCOPE_LIST_LIMIT {
            return Err(MemoryStoreError::InvalidLimit);
        }
        // Scope cardinality is globally bounded by Memory cardinality. Clamp a
        // hostile/out-of-range page offset before converting usize to SQLite's
        // signed integer representation; callers still observe the effective
        // offset normalized against total below.
        let offset = offset.min(MAX_MEMORIES_GLOBAL);
        let conn = self.conn.lock().unwrap();
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM project_memory_scopes", [], |row| {
                row.get(0)
            })
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if total < 0 || total as usize > MAX_MEMORIES_GLOBAL {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        let (memory_total, distinct_scope_total): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COUNT(DISTINCT memory_scope_id) FROM project_memories",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if memory_total < 0
            || memory_total as usize > MAX_MEMORIES_GLOBAL
            || distinct_scope_total < 0
            || distinct_scope_total != total
        {
            // A Memory row without its one scope-metadata row would otherwise
            // disappear from lifecycle inventory while still consuming global
            // capacity. Treat either direction of drift as store corruption.
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        let mut statement = conn
            .prepare(&format!(
                "{SELECT_SCOPE} ORDER BY memory_scope_id ASC LIMIT ?1 OFFSET ?2"
            ))
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let scopes = statement
            .query_map(params![limit as i64, offset as i64], parse_scope_row)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let mut snapshots = Vec::with_capacity(scopes.len());
        for scope in scopes {
            let memories = list_project_memories_with_conn(&conn, &scope.memory_scope_id)?;
            if memories.is_empty() {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
            snapshots.push(ProjectMemoryScopeSnapshot { scope, memories });
        }
        Ok((total as usize, snapshots))
    }

    pub(crate) fn get_project_memory_scope(
        &self,
        memory_scope_id: &str,
    ) -> Result<Option<ProjectMemoryScopeSnapshot>, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        let conn = self.conn.lock().unwrap();
        let Some(scope) = get_scope_record(&conn, memory_scope_id)? else {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                    params![memory_scope_id],
                    |row| row.get(0),
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            if count != 0 {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
            return Ok(None);
        };
        let memories = list_project_memories_with_conn(&conn, memory_scope_id)?;
        if memories.is_empty() {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        Ok(Some(ProjectMemoryScopeSnapshot { scope, memories }))
    }

    pub(crate) fn purge_project_memory_scope(
        &self,
        memory_scope_id: &str,
        expected_catalog_revision: &str,
    ) -> Result<MemoryScopePurgeOutcome, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        if !valid_memory_catalog_revision(expected_catalog_revision) {
            return Err(MemoryStoreError::InvalidRevision);
        }
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let Some(_scope) = get_scope_record(&tx, memory_scope_id)? else {
            let count: i64 = tx
                .query_row(
                    "SELECT COUNT(*) FROM project_memories WHERE memory_scope_id = ?1",
                    params![memory_scope_id],
                    |row| row.get(0),
                )
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            if count != 0 {
                return Err(MemoryStoreError::DatabaseUnavailable);
            }
            tx.commit()
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            return Ok(MemoryScopePurgeOutcome {
                memory_scope_id: memory_scope_id.to_string(),
                catalog_revision: None,
                purged_count: 0,
                purged: false,
                state_changed: false,
            });
        };
        let records = list_project_memories_with_conn(&tx, memory_scope_id)?;
        if records.is_empty() {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        let current_catalog_revision = memory_catalog_revision(&records);
        if current_catalog_revision != expected_catalog_revision {
            return Err(MemoryStoreError::ScopeChanged {
                current_catalog_revision,
            });
        }
        let purged_count = tx
            .execute(
                "DELETE FROM project_memories WHERE memory_scope_id = ?1",
                params![memory_scope_id],
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if purged_count != records.len() {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        let removed_scope = tx
            .execute(
                "DELETE FROM project_memory_scopes WHERE memory_scope_id = ?1",
                params![memory_scope_id],
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if removed_scope != 1 {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        tx.commit()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        Ok(MemoryScopePurgeOutcome {
            memory_scope_id: memory_scope_id.to_string(),
            catalog_revision: Some(current_catalog_revision),
            purged_count,
            purged: true,
            state_changed: true,
        })
    }
}
