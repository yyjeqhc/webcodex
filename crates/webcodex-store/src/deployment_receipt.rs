use super::Database;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const DEPLOYMENT_RECEIPT_ID_PREFIX: &str = "wc_deploy_";
pub const MAX_DEPLOYMENT_MANIFEST_BYTES: usize = 16 * 1024;
pub const MAX_DEPLOYMENT_IDEMPOTENCY_KEY_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentOperation {
    Deploy,
    Restart,
    Rollback,
}

impl DeploymentOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deploy => "deploy",
            Self::Restart => "restart",
            Self::Rollback => "rollback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentState {
    Planned,
    Draining,
    Ready,
    Switching,
    Verifying,
    Succeeded,
    Failed,
    RolledBack,
    OutcomeUnknown,
}

impl DeploymentState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Draining => "draining",
            Self::Ready => "ready",
            Self::Switching => "switching",
            Self::Verifying => "verifying",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::RolledBack => "rolled_back",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::RolledBack | Self::OutcomeUnknown
        )
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "planned" => Some(Self::Planned),
            "draining" => Some(Self::Draining),
            "ready" => Some(Self::Ready),
            "switching" => Some(Self::Switching),
            "verifying" => Some(Self::Verifying),
            "succeeded" => Some(Self::Succeeded),
            "failed" => Some(Self::Failed),
            "rolled_back" => Some(Self::RolledBack),
            "outcome_unknown" => Some(Self::OutcomeUnknown),
            _ => None,
        }
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Planned, Self::Draining | Self::Ready | Self::Failed)
                | (Self::Draining, Self::Ready | Self::Failed)
                | (Self::Ready, Self::Switching | Self::Failed)
                | (
                    Self::Switching,
                    Self::Verifying | Self::Failed | Self::RolledBack | Self::OutcomeUnknown
                )
                | (
                    Self::Verifying,
                    Self::Succeeded | Self::Failed | Self::RolledBack | Self::OutcomeUnknown
                )
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentPrincipal {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewDeploymentReceipt {
    pub principal: DeploymentPrincipal,
    pub operation: DeploymentOperation,
    pub client_id: String,
    pub idempotency_key: String,
    /// SHA-256 over the complete caller-visible deployment request. This binds
    /// idempotency replay to the exact operation, target, and manifest.
    pub request_hash: String,
    /// Bounded, non-secret manifest JSON: build identity, candidate hashes and
    /// rollback reference. Credentials, headers and native auth material are forbidden.
    pub target_manifest_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentReceiptRecord {
    pub receipt_id: String,
    pub owner_kind: String,
    pub owner_id: String,
    pub operation: String,
    pub client_id: String,
    pub request_hash: String,
    pub target_manifest_json: String,
    pub state: String,
    pub revision: u64,
    pub created_at: i64,
    pub updated_at: i64,
    pub terminal_at: Option<i64>,
    pub last_error_code: Option<String>,
    pub backup_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentReceiptBeginOutcome {
    Created(DeploymentReceiptRecord),
    Replayed(DeploymentReceiptRecord),
}

#[derive(Debug)]
pub enum DeploymentReceiptStoreError {
    InvalidInput(String),
    IdempotencyConflict,
    NotFound,
    RevisionConflict { actual_revision: u64 },
    InvalidTransition { from: String, to: String },
    Store(anyhow::Error),
}

impl std::fmt::Display for DeploymentReceiptStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(reason) => write!(f, "invalid deployment receipt input: {reason}"),
            Self::IdempotencyConflict => write!(
                f,
                "deployment idempotency key was already used with a different request"
            ),
            Self::NotFound => write!(f, "deployment receipt not found"),
            Self::RevisionConflict { actual_revision } => write!(
                f,
                "deployment receipt revision conflict (actual_revision={actual_revision})"
            ),
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid deployment receipt transition: {from} -> {to}")
            }
            Self::Store(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for DeploymentReceiptStoreError {}

impl From<anyhow::Error> for DeploymentReceiptStoreError {
    fn from(value: anyhow::Error) -> Self {
        Self::Store(value)
    }
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn validate_new(input: &NewDeploymentReceipt) -> Result<(), DeploymentReceiptStoreError> {
    if input.principal.kind.trim().is_empty() || input.principal.id.trim().is_empty() {
        return Err(DeploymentReceiptStoreError::InvalidInput(
            "principal kind/id must be non-empty".to_string(),
        ));
    }
    if input.client_id.trim().is_empty() || input.client_id.chars().count() > 128 {
        return Err(DeploymentReceiptStoreError::InvalidInput(
            "client_id must be 1..=128 characters".to_string(),
        ));
    }
    if input.idempotency_key.is_empty()
        || input.idempotency_key.len() > MAX_DEPLOYMENT_IDEMPOTENCY_KEY_BYTES
    {
        return Err(DeploymentReceiptStoreError::InvalidInput(format!(
            "idempotency_key must be 1..={MAX_DEPLOYMENT_IDEMPOTENCY_KEY_BYTES} bytes"
        )));
    }
    if input.request_hash.len() != 64
        || !input
            .request_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(DeploymentReceiptStoreError::InvalidInput(
            "request_hash must be a 64-character SHA-256 hex digest".to_string(),
        ));
    }
    if input.target_manifest_json.len() > MAX_DEPLOYMENT_MANIFEST_BYTES {
        return Err(DeploymentReceiptStoreError::InvalidInput(format!(
            "target manifest exceeds {MAX_DEPLOYMENT_MANIFEST_BYTES} bytes"
        )));
    }
    serde_json::from_str::<serde_json::Value>(&input.target_manifest_json).map_err(|_| {
        DeploymentReceiptStoreError::InvalidInput("target manifest must be valid JSON".to_string())
    })?;
    Ok(())
}

fn row_to_receipt(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeploymentReceiptRecord> {
    Ok(DeploymentReceiptRecord {
        receipt_id: row.get(0)?,
        owner_kind: row.get(1)?,
        owner_id: row.get(2)?,
        operation: row.get(3)?,
        client_id: row.get(4)?,
        request_hash: row.get(5)?,
        target_manifest_json: row.get(6)?,
        state: row.get(7)?,
        revision: row.get::<_, i64>(8)?.max(0) as u64,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        terminal_at: row.get(11)?,
        last_error_code: row.get(12)?,
        backup_id: row.get(13)?,
    })
}

const RECEIPT_SELECT: &str = "SELECT receipt_id, owner_kind, owner_id, operation, client_id, request_hash, target_manifest_json, state, revision, created_at, updated_at, terminal_at, last_error_code, backup_id FROM wc_deployment_receipts";

impl Database {
    /// Acquire the deployment receipt store's write lock and roll back without
    /// persisting state. Used by deployment preflight to fail before cutover if
    /// SQLite is read-only, busy beyond admission policy, or otherwise unhealthy.
    pub fn deployment_receipt_store_probe(&self) -> anyhow::Result<()> {
        let mut conn = self.lock_connection(crate::StoreDomain::DeploymentReceipts);
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let value: i64 = tx.query_row("SELECT 1", [], |row| row.get(0))?;
        anyhow::ensure!(
            value == 1,
            "deployment receipt store probe returned unexpected value"
        );
        tx.rollback()?;
        Ok(())
    }

    pub fn begin_deployment_receipt(
        &self,
        input: &NewDeploymentReceipt,
    ) -> Result<DeploymentReceiptBeginOutcome, DeploymentReceiptStoreError> {
        validate_new(input)?;
        let key_hash = sha256_hex(&input.idempotency_key);
        let mut conn = self.lock_connection(crate::StoreDomain::DeploymentReceipts);
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(anyhow::Error::from)?;

        let existing = tx
            .query_row(
                &format!("{RECEIPT_SELECT} WHERE owner_kind=?1 AND owner_id=?2 AND idempotency_key_hash=?3"),
                params![input.principal.kind, input.principal.id, key_hash],
                row_to_receipt,
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        if let Some(existing) = existing {
            if existing
                .request_hash
                .eq_ignore_ascii_case(&input.request_hash)
                && existing.operation == input.operation.as_str()
                && existing.client_id == input.client_id
                && existing.target_manifest_json == input.target_manifest_json
            {
                tx.commit().map_err(anyhow::Error::from)?;
                return Ok(DeploymentReceiptBeginOutcome::Replayed(existing));
            }
            return Err(DeploymentReceiptStoreError::IdempotencyConflict);
        }

        let receipt_id = format!(
            "{DEPLOYMENT_RECEIPT_ID_PREFIX}{}",
            uuid::Uuid::new_v4().simple()
        );
        tx.execute(
            "INSERT INTO wc_deployment_receipts (
                receipt_id, owner_kind, owner_id, idempotency_key_hash, request_hash,
                operation, client_id, target_manifest_json, state, revision,
                created_at, updated_at, terminal_at, last_error_code
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'planned',1,?9,?9,NULL,NULL)",
            params![
                receipt_id,
                input.principal.kind,
                input.principal.id,
                key_hash,
                input.request_hash.to_ascii_lowercase(),
                input.operation.as_str(),
                input.client_id,
                input.target_manifest_json,
                input.created_at,
            ],
        )
        .map_err(anyhow::Error::from)?;
        let created = tx
            .query_row(
                &format!("{RECEIPT_SELECT} WHERE receipt_id=?1"),
                params![receipt_id],
                row_to_receipt,
            )
            .map_err(anyhow::Error::from)?;
        tx.commit().map_err(anyhow::Error::from)?;
        Ok(DeploymentReceiptBeginOutcome::Created(created))
    }

    pub fn read_deployment_receipt(
        &self,
        principal: &DeploymentPrincipal,
        receipt_id: &str,
    ) -> Result<DeploymentReceiptRecord, DeploymentReceiptStoreError> {
        let conn = self.lock_connection(crate::StoreDomain::DeploymentReceipts);
        conn.query_row(
            &format!("{RECEIPT_SELECT} WHERE receipt_id=?1 AND owner_kind=?2 AND owner_id=?3"),
            params![receipt_id, principal.kind, principal.id],
            row_to_receipt,
        )
        .optional()
        .map_err(anyhow::Error::from)?
        .ok_or(DeploymentReceiptStoreError::NotFound)
    }

    pub fn transition_deployment_receipt(
        &self,
        principal: &DeploymentPrincipal,
        receipt_id: &str,
        expected_revision: u64,
        state: DeploymentState,
        now: i64,
        last_error_code: Option<&str>,
    ) -> Result<DeploymentReceiptRecord, DeploymentReceiptStoreError> {
        self.transition_deployment_receipt_with_backup(
            principal,
            receipt_id,
            expected_revision,
            state,
            now,
            last_error_code,
            None,
        )
    }

    pub fn transition_deployment_receipt_with_backup(
        &self,
        principal: &DeploymentPrincipal,
        receipt_id: &str,
        expected_revision: u64,
        state: DeploymentState,
        now: i64,
        last_error_code: Option<&str>,
        backup_id: Option<&str>,
    ) -> Result<DeploymentReceiptRecord, DeploymentReceiptStoreError> {
        if expected_revision == 0 {
            return Err(DeploymentReceiptStoreError::InvalidInput(
                "expected_revision must be >= 1".to_string(),
            ));
        }
        if let Some(code) = last_error_code {
            if code.is_empty()
                || code.len() > 128
                || !code
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_-.:".contains(&b))
            {
                return Err(DeploymentReceiptStoreError::InvalidInput(
                    "last_error_code must be a bounded safe diagnostic code".to_string(),
                ));
            }
        }
        if let Some(backup_id) = backup_id {
            if backup_id.is_empty()
                || backup_id.len() > 128
                || !backup_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
            {
                return Err(DeploymentReceiptStoreError::InvalidInput(
                    "backup_id must be a bounded safe identifier".to_string(),
                ));
            }
        }
        let mut conn = self.lock_connection(crate::StoreDomain::DeploymentReceipts);
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(anyhow::Error::from)?;
        let current: Option<(i64, String)> = tx
            .query_row(
                "SELECT revision, state FROM wc_deployment_receipts WHERE receipt_id=?1 AND owner_kind=?2 AND owner_id=?3",
                params![receipt_id, principal.kind, principal.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(anyhow::Error::from)?;
        let Some((actual_revision, current_state)) = current else {
            return Err(DeploymentReceiptStoreError::NotFound);
        };
        if actual_revision as u64 != expected_revision {
            return Err(DeploymentReceiptStoreError::RevisionConflict {
                actual_revision: actual_revision.max(0) as u64,
            });
        }
        let Some(current_state_value) = DeploymentState::parse(&current_state) else {
            return Err(DeploymentReceiptStoreError::Store(anyhow::anyhow!(
                "stored deployment state is invalid"
            )));
        };
        if !current_state_value.can_transition_to(state) {
            return Err(DeploymentReceiptStoreError::InvalidTransition {
                from: current_state,
                to: state.as_str().to_string(),
            });
        }
        tx.execute(
            "UPDATE wc_deployment_receipts
             SET state=?1, revision=revision+1, updated_at=?2,
                 terminal_at=CASE WHEN ?3=1 THEN ?2 ELSE NULL END,
                 last_error_code=?4,
                 backup_id=COALESCE(?5, backup_id)
             WHERE receipt_id=?6 AND owner_kind=?7 AND owner_id=?8 AND revision=?9",
            params![
                state.as_str(),
                now,
                i64::from(state.is_terminal()),
                last_error_code,
                backup_id,
                receipt_id,
                principal.kind,
                principal.id,
                expected_revision as i64,
            ],
        )
        .map_err(anyhow::Error::from)?;
        let updated = tx
            .query_row(
                &format!("{RECEIPT_SELECT} WHERE receipt_id=?1"),
                params![receipt_id],
                row_to_receipt,
            )
            .map_err(anyhow::Error::from)?;
        tx.commit().map_err(anyhow::Error::from)?;
        Ok(updated)
    }
}
