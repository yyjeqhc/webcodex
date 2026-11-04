use super::{Database, StoreDomain};
use rusqlite::{params, OptionalExtension, TransactionBehavior};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectReferenceRecord {
    pub ref_index: u64,
    pub canonical_project_id: String,
    pub root_fingerprint: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectReferenceStoreError {
    InvalidInput,
    Unavailable,
}

impl std::fmt::Display for ProjectReferenceStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidInput => "invalid project reference input",
            Self::Unavailable => "project reference store unavailable",
        })
    }
}

impl std::error::Error for ProjectReferenceStoreError {}

fn store_error(error: rusqlite::Error) -> ProjectReferenceStoreError {
    tracing::warn!(error = %error, "durable project reference store operation failed");
    ProjectReferenceStoreError::Unavailable
}

fn valid_input(principal_key: &str, canonical_project_id: &str, root_fingerprint: &str) -> bool {
    !principal_key.is_empty()
        && principal_key.len() <= 128
        && !canonical_project_id.is_empty()
        && canonical_project_id.len() <= 512
        && root_fingerprint
            .strip_prefix("wc_projroot_")
            .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

fn lookup_reference_indices(
    conn: &rusqlite::Connection,
    principal_key: &str,
    requested: &str,
) -> Result<Vec<Option<i64>>, ProjectReferenceStoreError> {
    let mut statement = conn
        .prepare_cached(
            "SELECT reference.ref_index
         FROM json_each(?2) AS requested
         LEFT JOIN project_references AS reference
           ON reference.principal_key = ?1
          AND reference.canonical_project_id = json_extract(requested.value, '$[0]')
          AND reference.root_fingerprint = json_extract(requested.value, '$[1]')
         ORDER BY CAST(requested.key AS INTEGER)",
        )
        .map_err(store_error)?;
    let indices = statement
        .query_map(params![principal_key, requested], |row| row.get(0))
        .map_err(store_error)?
        .collect::<Result<Vec<Option<i64>>, _>>()
        .map_err(store_error)?;
    Ok(indices)
}

impl Database {
    pub fn get_or_create_project_reference(
        &self,
        principal_key: &str,
        canonical_project_id: &str,
        root_fingerprint: &str,
        created_at: i64,
    ) -> Result<ProjectReferenceRecord, ProjectReferenceStoreError> {
        if !valid_input(principal_key, canonical_project_id, root_fingerprint) {
            return Err(ProjectReferenceStoreError::InvalidInput);
        }
        self.get_or_create_project_references(
            principal_key,
            &[(canonical_project_id, root_fingerprint)],
            created_at,
        )?
        .pop()
        .flatten()
        .ok_or(ProjectReferenceStoreError::Unavailable)
    }

    /// Resolve a bounded selector page using one connection acquisition and one
    /// indexed batch SELECT on the warm path. Allocate missing identities in one
    /// transaction; never retarget or recycle a principal's existing refs.
    /// Invalid individual identities have no reference, matching optional
    /// Console presentation without dropping valid rows from the same page.
    pub fn get_or_create_project_references(
        &self,
        principal_key: &str,
        identities: &[(&str, &str)],
        created_at: i64,
    ) -> Result<Vec<Option<ProjectReferenceRecord>>, ProjectReferenceStoreError> {
        if principal_key.is_empty() || principal_key.len() > 128 || identities.len() > 2_000 {
            return Err(ProjectReferenceStoreError::InvalidInput);
        }
        let valid: Vec<_> = identities
            .iter()
            .enumerate()
            .filter_map(|(index, &(id, root))| {
                valid_input(principal_key, id, root).then_some((index, id, root))
            })
            .collect();
        let mut result = vec![None; identities.len()];
        if valid.is_empty() {
            return Ok(result);
        }
        let requested: Vec<_> = valid.iter().map(|(_, id, root)| (*id, *root)).collect();
        let encoded = serde_json::to_string(&requested)
            .map_err(|_| ProjectReferenceStoreError::InvalidInput)?;
        let mut conn = self.lock_connection(StoreDomain::ProjectReference);
        let mut indices = lookup_reference_indices(&conn, principal_key, &encoded)?;
        if indices.iter().any(Option::is_none) {
            let transaction = conn
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(store_error)?;
            // Another Database connection/process may have filled a miss after
            // the read. Recheck under the SQLite writer fence before allocating.
            indices = lookup_reference_indices(&transaction, principal_key, &encoded)?;
            let mut next_index: i64 = if indices.iter().any(Option::is_none) {
                transaction.query_row(
                "SELECT COALESCE(MAX(ref_index), 0) + 1 FROM project_references WHERE principal_key = ?1",
                params![principal_key], |row| row.get(0),
            ).map_err(store_error)?
            } else {
                0
            };
            let mut allocated = std::collections::HashMap::new();
            for (position, &(id, root)) in requested.iter().enumerate() {
                if indices[position].is_some() {
                    continue;
                }
                if let Some(index) = allocated.get(&(id, root)) {
                    indices[position] = Some(*index);
                    continue;
                }
                if next_index <= 0 {
                    return Err(ProjectReferenceStoreError::Unavailable);
                }
                transaction
                    .execute(
                        "INSERT INTO project_references (
                        principal_key, ref_index, canonical_project_id, root_fingerprint, created_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![principal_key, next_index, id, root, created_at],
                    )
                    .map_err(store_error)?;
                indices[position] = Some(next_index);
                allocated.insert((id, root), next_index);
                next_index = next_index.checked_add(1).unwrap_or(0);
            }
            transaction.commit().map_err(store_error)?;
        }
        for ((input_index, id, root), index) in valid.into_iter().zip(indices) {
            result[input_index] = index.map(|ref_index| ProjectReferenceRecord {
                ref_index: ref_index as u64,
                canonical_project_id: id.to_string(),
                root_fingerprint: root.to_string(),
            });
        }
        Ok(result)
    }

    pub fn lookup_project_reference(
        &self,
        principal_key: &str,
        ref_index: u64,
    ) -> Result<Option<ProjectReferenceRecord>, ProjectReferenceStoreError> {
        if principal_key.is_empty() || principal_key.len() > 128 || ref_index == 0 {
            return Err(ProjectReferenceStoreError::InvalidInput);
        }
        let ref_index =
            i64::try_from(ref_index).map_err(|_| ProjectReferenceStoreError::InvalidInput)?;
        let conn = self.lock_connection(StoreDomain::ProjectReference);
        conn.query_row(
            "SELECT ref_index, canonical_project_id, root_fingerprint
             FROM project_references
             WHERE principal_key = ?1 AND ref_index = ?2",
            params![principal_key, ref_index],
            |row| {
                let ref_index: i64 = row.get(0)?;
                Ok(ProjectReferenceRecord {
                    ref_index: ref_index as u64,
                    canonical_project_id: row.get(1)?,
                    root_fingerprint: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(store_error)
    }
}
