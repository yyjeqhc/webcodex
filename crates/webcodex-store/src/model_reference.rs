use super::{Database, ProjectReferenceStoreError, StoreDomain};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use webcodex_core::{
    model_reference::ModelReferenceKind, workflow_session_contract::is_valid_session_id,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelReferenceRecord {
    pub ref_index: u64,
    pub canonical_id: String,
    pub incarnation_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelReferenceStoreError {
    InvalidInput,
    Unavailable,
}

impl std::fmt::Display for ModelReferenceStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "invalid model reference input",
            Self::Unavailable => "model reference store unavailable",
        })
    }
}

impl std::error::Error for ModelReferenceStoreError {}

impl From<ProjectReferenceStoreError> for ModelReferenceStoreError {
    fn from(error: ProjectReferenceStoreError) -> Self {
        match error {
            ProjectReferenceStoreError::InvalidInput => Self::InvalidInput,
            ProjectReferenceStoreError::Unavailable => Self::Unavailable,
        }
    }
}

fn store_error(error: rusqlite::Error) -> ModelReferenceStoreError {
    tracing::warn!(error = %error, "durable model reference store operation failed");
    ModelReferenceStoreError::Unavailable
}

fn valid_session_input(
    principal_key: &str,
    canonical_session_id: &str,
    incarnation_fingerprint: &str,
) -> bool {
    !principal_key.is_empty()
        && principal_key.len() <= 128
        && is_valid_session_id(canonical_session_id)
        && incarnation_fingerprint.len() == 64
        && incarnation_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
}

fn project_record(record: super::ProjectReferenceRecord) -> ModelReferenceRecord {
    ModelReferenceRecord {
        ref_index: record.ref_index,
        canonical_id: record.canonical_project_id,
        incarnation_fingerprint: record.root_fingerprint,
    }
}

impl Database {
    pub(super) fn ensure_model_reference_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS session_references (
                principal_key TEXT NOT NULL,
                ref_index INTEGER NOT NULL CHECK(ref_index >= 1),
                canonical_session_id TEXT NOT NULL,
                incarnation_fingerprint TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY(principal_key, ref_index),
                UNIQUE(principal_key, canonical_session_id, incarnation_fingerprint)
            );
            ",
        )?;
        Ok(())
    }

    pub fn get_or_create_model_reference(
        &self,
        principal_key: &str,
        kind: ModelReferenceKind,
        canonical_id: &str,
        incarnation_fingerprint: &str,
        created_at: i64,
    ) -> Result<ModelReferenceRecord, ModelReferenceStoreError> {
        if kind == ModelReferenceKind::Project {
            return self
                .get_or_create_project_reference(
                    principal_key,
                    canonical_id,
                    incarnation_fingerprint,
                    created_at,
                )
                .map(project_record)
                .map_err(Into::into);
        }

        if !valid_session_input(principal_key, canonical_id, incarnation_fingerprint) {
            return Err(ModelReferenceStoreError::InvalidInput);
        }
        let mut conn = self.lock_connection(StoreDomain::ModelReference);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let existing = transaction
            .query_row(
                "SELECT ref_index, canonical_session_id, incarnation_fingerprint
                 FROM session_references
                 WHERE principal_key = ?1
                   AND canonical_session_id = ?2
                   AND incarnation_fingerprint = ?3",
                params![principal_key, canonical_id, incarnation_fingerprint],
                |row| {
                    let ref_index: i64 = row.get(0)?;
                    Ok(ModelReferenceRecord {
                        ref_index: ref_index as u64,
                        canonical_id: row.get(1)?,
                        incarnation_fingerprint: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(store_error)?;
        if let Some(existing) = existing {
            transaction.commit().map_err(store_error)?;
            return Ok(existing);
        }

        let next_index: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(ref_index), 0) + 1
                 FROM session_references
                 WHERE principal_key = ?1",
                params![principal_key],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if next_index <= 0 {
            return Err(ModelReferenceStoreError::Unavailable);
        }
        transaction
            .execute(
                "INSERT INTO session_references (
                    principal_key, ref_index, canonical_session_id,
                    incarnation_fingerprint, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    principal_key,
                    next_index,
                    canonical_id,
                    incarnation_fingerprint,
                    created_at
                ],
            )
            .map_err(store_error)?;
        transaction.commit().map_err(store_error)?;
        Ok(ModelReferenceRecord {
            ref_index: next_index as u64,
            canonical_id: canonical_id.to_string(),
            incarnation_fingerprint: incarnation_fingerprint.to_string(),
        })
    }

    pub fn lookup_model_reference(
        &self,
        principal_key: &str,
        kind: ModelReferenceKind,
        ref_index: u64,
    ) -> Result<Option<ModelReferenceRecord>, ModelReferenceStoreError> {
        if kind == ModelReferenceKind::Project {
            return self
                .lookup_project_reference(principal_key, ref_index)
                .map(|record| record.map(project_record))
                .map_err(Into::into);
        }

        if principal_key.is_empty() || principal_key.len() > 128 || ref_index == 0 {
            return Err(ModelReferenceStoreError::InvalidInput);
        }
        let ref_index =
            i64::try_from(ref_index).map_err(|_| ModelReferenceStoreError::InvalidInput)?;
        let conn = self.lock_connection(StoreDomain::ModelReference);
        conn.query_row(
            "SELECT ref_index, canonical_session_id, incarnation_fingerprint
             FROM session_references
             WHERE principal_key = ?1 AND ref_index = ?2",
            params![principal_key, ref_index],
            |row| {
                let ref_index: i64 = row.get(0)?;
                Ok(ModelReferenceRecord {
                    ref_index: ref_index as u64,
                    canonical_id: row.get(1)?,
                    incarnation_fingerprint: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(store_error)
    }
}
