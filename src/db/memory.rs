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

const MEMORY_ID_PREFIX: &str = "wc_mem_";
const MEMORY_REVISION_PREFIX: &str = "wc_memrev_";
const MEMORY_CATALOG_REVISION_PREFIX: &str = "wc_memcat_";

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
    pub(crate) revision: String,
    pub(crate) created_at_unix_ms: i64,
    pub(crate) updated_at_unix_ms: i64,
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

pub(crate) fn validate_memory_revision(value: &str) -> Result<(), MemoryStoreError> {
    if value.len() == MEMORY_REVISION_PREFIX.len() + 64
        && value.starts_with(MEMORY_REVISION_PREFIX)
        && value[MEMORY_REVISION_PREFIX.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(MemoryStoreError::InvalidRevision)
    }
}

fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

pub(crate) fn memory_revision(
    memory_key: &str,
    summary: &str,
    body: &str,
    priority: MemoryPriority,
    bootstrap: bool,
    tags: &[String],
) -> String {
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
    format!("{MEMORY_REVISION_PREFIX}{:x}", hasher.finalize())
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

fn parse_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProjectMemoryRecord> {
    let priority: String = row.get(4)?;
    let tags_json: String = row.get(6)?;
    let priority = MemoryPriority::parse(&priority).map_err(|_| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            "invalid memory priority".into(),
        )
    })?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(ProjectMemoryRecord {
        memory_id: row.get(0)?,
        memory_key: row.get(1)?,
        summary: row.get(2)?,
        body: row.get(3)?,
        priority,
        bootstrap: row.get::<_, i64>(5)? != 0,
        tags,
        revision: row.get(7)?,
        created_at_unix_ms: row.get(8)?,
        updated_at_unix_ms: row.get(9)?,
    })
}

const SELECT_RECORD: &str =
    "SELECT memory_id, memory_key, summary, body, priority, bootstrap, tags_json, revision,
            created_at_unix_ms, updated_at_unix_ms
     FROM project_memories";

impl Database {
    pub(crate) fn list_project_memories(
        &self,
        memory_scope_id: &str,
    ) -> Result<Vec<ProjectMemoryRecord>, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        let conn = self.conn.lock().unwrap();
        let mut statement = conn
            .prepare(&format!(
                "{SELECT_RECORD} WHERE memory_scope_id = ?1 ORDER BY memory_key ASC LIMIT ?2"
            ))
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let records = statement
            .query_map(
                params![memory_scope_id, (MAX_MEMORIES_PER_PROJECT + 1) as i64],
                parse_row,
            )
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        if records.len() > MAX_MEMORIES_PER_PROJECT {
            return Err(MemoryStoreError::DatabaseUnavailable);
        }
        Ok(records)
    }

    pub(crate) fn get_project_memory(
        &self,
        memory_scope_id: &str,
        memory_key: &str,
    ) -> Result<Option<ProjectMemoryRecord>, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_memory_key(memory_key)?;
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
            params![memory_scope_id, memory_key],
            parse_row,
        )
        .optional()
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)
    }

    pub(crate) fn set_project_memory(
        &self,
        memory_scope_id: &str,
        mut input: MemorySetInput,
    ) -> Result<MemorySetOutcome, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_memory_key(&input.memory_key)?;
        validate_memory_summary(&input.summary)?;
        validate_memory_body(&input.body)?;
        input.tags = canonicalize_memory_tags(input.tags)?;
        if let Some(expected) = input.expected_revision.as_deref() {
            validate_memory_revision(expected)?;
        }
        let requested_revision = memory_revision(
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
                parse_row,
            )
            .optional()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;

        if let Some(existing) = existing {
            match input.expected_revision.as_deref() {
                None if existing.revision == requested_revision => {
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
                Some(_) if existing.revision == requested_revision => {
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
            let old_revision = existing.revision.clone();
            let tags_json = serde_json::to_string(&input.tags)
                .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
            let changed = tx
                .execute(
                    "UPDATE project_memories
                     SET summary = ?1, body = ?2, priority = ?3, bootstrap = ?4,
                         tags_json = ?5, revision = ?6, updated_at_unix_ms = ?7
                     WHERE memory_scope_id = ?8 AND memory_key = ?9 AND revision = ?10",
                    params![
                        input.summary,
                        input.body,
                        input.priority.as_str(),
                        input.bootstrap,
                        tags_json,
                        requested_revision,
                        now,
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
                    parse_row,
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
        let memory_id = format!("{MEMORY_ID_PREFIX}{}", uuid::Uuid::new_v4().simple());
        let tags_json = serde_json::to_string(&input.tags)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        tx.execute(
            "INSERT INTO project_memories
             (memory_id, memory_scope_id, memory_key, summary, body, priority, bootstrap,
              tags_json, revision, created_at_unix_ms, updated_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)",
            params![
                memory_id,
                memory_scope_id,
                input.memory_key,
                input.summary,
                input.body,
                input.priority.as_str(),
                input.bootstrap,
                tags_json,
                requested_revision,
                now,
            ],
        )
        .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let record = tx
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_id = ?1"),
                params![memory_id],
                parse_row,
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

    pub(crate) fn delete_project_memory(
        &self,
        memory_scope_id: &str,
        memory_key: &str,
        expected_revision: &str,
    ) -> Result<MemoryDeleteOutcome, MemoryStoreError> {
        validate_scope(memory_scope_id)?;
        validate_memory_key(memory_key)?;
        validate_memory_revision(expected_revision)?;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        let existing = tx
            .query_row(
                &format!("{SELECT_RECORD} WHERE memory_scope_id = ?1 AND memory_key = ?2"),
                params![memory_scope_id, memory_key],
                parse_row,
            )
            .optional()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
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
        tx.commit()
            .map_err(|_| MemoryStoreError::DatabaseUnavailable)?;
        Ok(MemoryDeleteOutcome {
            memory_id: Some(existing.memory_id),
            revision: Some(existing.revision),
            deleted: true,
            state_changed: true,
        })
    }
}
