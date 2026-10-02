//! Durable authority state for one exact cross-principal artifact handoff.
//!
//! This module deliberately owns no artifact bytes and performs no transfer.
//! It binds a future accept/import operation to the exact source snapshot,
//! destination authority, mandatory expiry, and idempotency identity that the
//! existing snapshot-fenced transfer path can consume later.

use super::store_primitives::{
    digest_json, digest_text, validate_communication_principal, CommunicationPrincipal,
};
use super::Database;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::Serialize;
use std::fmt;

pub const ARTIFACT_HANDOFF_GRANT_ID_PREFIX: &str = "wc_handoff_";
pub const ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX: &str = "wc_handoff_accept_";
pub const DEFAULT_ARTIFACT_HANDOFF_TTL_MS: i64 = 15 * 60 * 1000;
pub const MAX_ARTIFACT_HANDOFF_TTL_MS: i64 = 24 * 60 * 60 * 1000;
pub const MAX_ARTIFACT_HANDOFF_IDEMPOTENCY_KEY_CHARS: usize = 128;
pub const MAX_ARTIFACT_HANDOFF_SOURCE_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_ARTIFACT_HANDOFF_PROJECT_BYTES: usize = 512;
pub const MAX_ARTIFACT_HANDOFF_PATH_BYTES: usize = 4_096;
pub const MAX_ARTIFACT_HANDOFF_MIME_BYTES: usize = 255;
pub const MAX_ARTIFACT_HANDOFF_NAME_BYTES: usize = 255;

const ARTIFACT_HANDOFF_KEY_DIGEST_DOMAIN: &str =
    "webcodex.artifact-handoff.acceptance-idempotency-key.v1";
const ARTIFACT_HANDOFF_IMPORT_REQUEST_DOMAIN: &str = "webcodex.artifact-handoff.import-request.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactHandoffStoreError {
    code: &'static str,
    message: String,
}

impl ArtifactHandoffStoreError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ArtifactHandoffStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ArtifactHandoffStoreError {}

fn unavailable() -> ArtifactHandoffStoreError {
    ArtifactHandoffStoreError::new(
        "artifact_handoff_grant_unavailable",
        "Artifact handoff grant is unavailable",
    )
}

fn invalid(code: &'static str, message: &'static str) -> ArtifactHandoffStoreError {
    ArtifactHandoffStoreError::new(code, message)
}

fn store_error(error: rusqlite::Error) -> ArtifactHandoffStoreError {
    tracing::warn!(error = %error, "durable artifact handoff store operation failed");
    ArtifactHandoffStoreError::new(
        "artifact_handoff_store_unavailable",
        "Durable artifact handoff store is unavailable",
    )
}

fn persisted_state_error() -> ArtifactHandoffStoreError {
    tracing::warn!("durable artifact handoff store contains invalid persisted state");
    ArtifactHandoffStoreError::new(
        "artifact_handoff_store_unavailable",
        "Durable artifact handoff store contains invalid persisted state",
    )
}

fn invalid_principal() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_principal",
        "Artifact handoff principal is invalid",
    )
}

fn invalid_project() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_project",
        "Artifact handoff project is invalid",
    )
}

fn invalid_snapshot() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_snapshot",
        "Artifact handoff source snapshot is invalid",
    )
}

fn invalid_ttl() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_ttl",
        "Artifact handoff TTL must be a positive bounded duration",
    )
}

fn invalid_idempotency_key() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_idempotency_key",
        "Artifact handoff idempotency key is invalid",
    )
}

fn invalid_request_hash() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_request_hash",
        "Artifact handoff acceptance request hash is invalid",
    )
}

fn invalid_outcome() -> ArtifactHandoffStoreError {
    invalid(
        "invalid_artifact_handoff_acceptance_outcome",
        "Artifact handoff acceptance outcome is invalid",
    )
}

fn idempotency_conflict() -> ArtifactHandoffStoreError {
    ArtifactHandoffStoreError::new(
        "artifact_handoff_acceptance_idempotency_conflict",
        "Artifact handoff acceptance idempotency key was already used for a different request",
    )
}

fn completion_conflict() -> ArtifactHandoffStoreError {
    ArtifactHandoffStoreError::new(
        "artifact_handoff_acceptance_completion_conflict",
        "Artifact handoff acceptance already completed with a different outcome",
    )
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffPrincipal {
    kind: String,
    digest: String,
}

impl ArtifactHandoffPrincipal {
    pub fn kind(&self) -> &str {
        &self.kind
    }

    pub fn digest(&self) -> &str {
        &self.digest
    }
}

impl TryFrom<CommunicationPrincipal> for ArtifactHandoffPrincipal {
    type Error = ArtifactHandoffStoreError;

    fn try_from(principal: CommunicationPrincipal) -> Result<Self, Self::Error> {
        validate_communication_principal(&principal).map_err(|_| invalid_principal())?;
        Ok(Self {
            kind: principal.kind,
            digest: principal.digest,
        })
    }
}

impl ArtifactHandoffPrincipal {
    fn as_communication_principal(&self) -> CommunicationPrincipal {
        CommunicationPrincipal {
            kind: self.kind.clone(),
            digest: self.digest.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactHandoffOperation {
    Read,
}

impl ArtifactHandoffOperation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
        }
    }

    fn from_db(value: &str) -> Result<Self, ArtifactHandoffStoreError> {
        match value {
            "read" => Ok(Self::Read),
            _ => Err(persisted_state_error()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactHandoffGrantState {
    Active,
    Consumed,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffSourceSnapshot {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub mime_type: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct NewArtifactHandoffGrant {
    pub source_project: String,
    pub source_snapshot: ArtifactHandoffSourceSnapshot,
    pub destination_principal: ArtifactHandoffPrincipal,
    pub destination_project: String,
    pub operation: ArtifactHandoffOperation,
    pub one_shot: bool,
    pub ttl_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffImportRequest {
    pub grant_id: String,
    pub destination_project: String,
    pub destination_path: String,
    pub overwrite: bool,
}

impl ArtifactHandoffImportRequest {
    /// Hash every caller-controlled semantic field before using an idempotency
    /// key. Transport/session metadata is intentionally excluded; rerunning the
    /// same logical import with a different key remains a distinct operation.
    pub fn request_hash(&self) -> Result<String, ArtifactHandoffStoreError> {
        self.validate()?;
        digest_json(ARTIFACT_HANDOFF_IMPORT_REQUEST_DOMAIN, self).map_err(|error| {
            tracing::warn!(error = %error, "artifact handoff import request serialization failed");
            invalid_request_hash()
        })
    }

    fn validate(&self) -> Result<(), ArtifactHandoffStoreError> {
        if !valid_artifact_handoff_id(&self.grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX) {
            return Err(unavailable());
        }
        validate_project(&self.destination_project)?;
        validate_import_path(&self.destination_path)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffGrant {
    pub grant_id: String,
    pub source_principal: ArtifactHandoffPrincipal,
    pub source_project: String,
    pub source_snapshot: ArtifactHandoffSourceSnapshot,
    pub destination_principal: ArtifactHandoffPrincipal,
    pub destination_project: String,
    pub operation: ArtifactHandoffOperation,
    pub one_shot: bool,
    pub created_at_unix_ms: i64,
    pub expires_at_unix_ms: i64,
    pub revoked_at_unix_ms: Option<i64>,
    pub consumed_at_unix_ms: Option<i64>,
}

impl ArtifactHandoffGrant {
    pub fn state(&self, now_unix_ms: i64) -> ArtifactHandoffGrantState {
        if self.revoked_at_unix_ms.is_some() {
            ArtifactHandoffGrantState::Revoked
        } else if self.consumed_at_unix_ms.is_some() {
            ArtifactHandoffGrantState::Consumed
        } else if now_unix_ms >= self.expires_at_unix_ms {
            ArtifactHandoffGrantState::Expired
        } else {
            ArtifactHandoffGrantState::Active
        }
    }

    fn active_at(&self, now_unix_ms: i64) -> bool {
        self.state(now_unix_ms) == ArtifactHandoffGrantState::Active
    }

    fn authorizes_read(&self, principal: &ArtifactHandoffPrincipal, project: &str) -> bool {
        (&self.source_principal == principal && self.source_project == project)
            || (&self.destination_principal == principal && self.destination_project == project)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactHandoffAcceptanceState {
    Prepared,
    Completed,
}

impl ArtifactHandoffAcceptanceState {
    fn from_db(value: &str) -> Result<Self, ArtifactHandoffStoreError> {
        match value {
            "prepared" => Ok(Self::Prepared),
            "completed" => Ok(Self::Completed),
            _ => Err(persisted_state_error()),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffAcceptanceOutcome {
    pub destination_path: String,
    pub destination_bytes: u64,
    pub destination_sha256: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ArtifactHandoffAcceptance {
    pub acceptance_id: String,
    pub grant_id: String,
    pub destination_principal: ArtifactHandoffPrincipal,
    pub destination_project: String,
    pub request_hash: String,
    pub state: ArtifactHandoffAcceptanceState,
    /// Durable evidence that the final destination commit returned success or an
    /// outcome-unknown result. Only such Prepared acceptances may reconcile an
    /// existing destination file on replay; earlier transfer failures must retry.
    pub destination_reconcile_allowed: bool,
    pub outcome: Option<ArtifactHandoffAcceptanceOutcome>,
    pub created_at_unix_ms: i64,
    pub completed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactHandoffAcceptanceClaim {
    pub grant: ArtifactHandoffGrant,
    pub acceptance: ArtifactHandoffAcceptance,
    pub replayed: bool,
}

#[derive(Debug)]
struct PersistedGrant {
    grant_id: String,
    source_principal_kind: String,
    source_principal_digest: String,
    source_project: String,
    source_path: String,
    source_bytes: i64,
    source_sha256: String,
    source_mime_type: String,
    source_name: String,
    destination_principal_kind: String,
    destination_principal_digest: String,
    destination_project: String,
    operation: String,
    one_shot: i64,
    created_at_unix_ms: i64,
    expires_at_unix_ms: i64,
    revoked_at_unix_ms: Option<i64>,
    consumed_at_unix_ms: Option<i64>,
}

#[derive(Debug)]
struct PersistedAcceptance {
    acceptance_id: String,
    grant_id: String,
    destination_principal_kind: String,
    destination_principal_digest: String,
    destination_project: String,
    idempotency_key_hash: String,
    request_hash: String,
    state: String,
    destination_reconcile_allowed: i64,
    destination_path: Option<String>,
    destination_bytes: Option<i64>,
    destination_sha256: Option<String>,
    created_at_unix_ms: i64,
    completed_at_unix_ms: Option<i64>,
}

impl Database {
    pub(super) fn ensure_artifact_handoff_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_artifact_handoff_grants (
                grant_id TEXT PRIMARY KEY,
                source_principal_kind TEXT NOT NULL,
                source_principal_digest TEXT NOT NULL,
                source_project TEXT NOT NULL,
                source_path TEXT NOT NULL,
                source_bytes INTEGER NOT NULL CHECK(source_bytes >= 0 AND source_bytes <= 268435456),
                source_sha256 TEXT NOT NULL CHECK(length(source_sha256) = 64),
                source_mime_type TEXT NOT NULL,
                source_name TEXT NOT NULL,
                destination_principal_kind TEXT NOT NULL,
                destination_principal_digest TEXT NOT NULL,
                destination_project TEXT NOT NULL,
                operation TEXT NOT NULL CHECK(operation = 'read'),
                one_shot INTEGER NOT NULL CHECK(one_shot IN (0, 1)),
                created_at_unix_ms INTEGER NOT NULL CHECK(created_at_unix_ms > 0),
                expires_at_unix_ms INTEGER NOT NULL CHECK(
                    expires_at_unix_ms > created_at_unix_ms
                    AND expires_at_unix_ms - created_at_unix_ms <= 86400000
                ),
                revoked_at_unix_ms INTEGER,
                consumed_at_unix_ms INTEGER,
                CHECK(revoked_at_unix_ms IS NULL OR revoked_at_unix_ms >= created_at_unix_ms),
                CHECK(consumed_at_unix_ms IS NULL OR consumed_at_unix_ms >= created_at_unix_ms),
                CHECK(revoked_at_unix_ms IS NULL OR consumed_at_unix_ms IS NULL)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_artifact_handoff_grants_source
                ON wc_artifact_handoff_grants(
                    source_principal_kind, source_principal_digest, source_project,
                    created_at_unix_ms DESC, grant_id
                );
            CREATE INDEX IF NOT EXISTS idx_wc_artifact_handoff_grants_destination
                ON wc_artifact_handoff_grants(
                    destination_principal_kind, destination_principal_digest, destination_project,
                    created_at_unix_ms DESC, grant_id
                );
            CREATE INDEX IF NOT EXISTS idx_wc_artifact_handoff_grants_expiry
                ON wc_artifact_handoff_grants(expires_at_unix_ms);

            CREATE TABLE IF NOT EXISTS wc_artifact_handoff_acceptances (
                acceptance_id TEXT PRIMARY KEY,
                grant_id TEXT NOT NULL,
                destination_principal_kind TEXT NOT NULL,
                destination_principal_digest TEXT NOT NULL,
                destination_project TEXT NOT NULL,
                idempotency_key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL CHECK(length(request_hash) = 64),
                state TEXT NOT NULL CHECK(state IN ('prepared', 'completed')),
                destination_reconcile_allowed INTEGER NOT NULL DEFAULT 0
                    CHECK(destination_reconcile_allowed IN (0, 1)),
                destination_path TEXT,
                destination_bytes INTEGER CHECK(destination_bytes IS NULL OR destination_bytes >= 0),
                destination_sha256 TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                completed_at_unix_ms INTEGER,
                UNIQUE(destination_principal_kind, destination_principal_digest, grant_id, idempotency_key_hash),
                FOREIGN KEY(grant_id) REFERENCES wc_artifact_handoff_grants(grant_id),
                CHECK(
                    (state = 'prepared'
                        AND destination_path IS NULL
                        AND destination_bytes IS NULL
                        AND destination_sha256 IS NULL
                        AND completed_at_unix_ms IS NULL)
                    OR
                    (state = 'completed'
                        AND destination_path IS NOT NULL
                        AND destination_bytes IS NOT NULL
                        AND destination_sha256 IS NOT NULL
                        AND completed_at_unix_ms IS NOT NULL)
                )
            );
            CREATE INDEX IF NOT EXISTS idx_wc_artifact_handoff_acceptances_grant
                ON wc_artifact_handoff_acceptances(grant_id, created_at_unix_ms, acceptance_id);
            ",
        )?;
        let has_destination_reconcile_allowed: bool = conn.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM pragma_table_info('wc_artifact_handoff_acceptances')
                WHERE name = 'destination_reconcile_allowed'
            )",
            [],
            |row| row.get(0),
        )?;
        if !has_destination_reconcile_allowed {
            conn.execute(
                "ALTER TABLE wc_artifact_handoff_acceptances
                 ADD COLUMN destination_reconcile_allowed INTEGER NOT NULL DEFAULT 0
                 CHECK(destination_reconcile_allowed IN (0, 1))",
                [],
            )?;
        }
        Ok(())
    }

    pub fn create_artifact_handoff_grant(
        &self,
        source_principal: &ArtifactHandoffPrincipal,
        input: NewArtifactHandoffGrant,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffGrant, ArtifactHandoffStoreError> {
        validate_principal(source_principal)?;
        validate_principal(&input.destination_principal)?;
        if source_principal == &input.destination_principal {
            return Err(invalid_principal());
        }
        let source_project = validate_project(&input.source_project)?;
        let destination_project = validate_project(&input.destination_project)?;
        let source_snapshot = validate_snapshot(input.source_snapshot)?;
        let ttl_ms = normalized_ttl(input.ttl_ms)?;
        if now_unix_ms <= 0 {
            return Err(invalid(
                "invalid_artifact_handoff_time",
                "Artifact handoff time is invalid",
            ));
        }
        let expires_at_unix_ms = now_unix_ms.checked_add(ttl_ms).ok_or_else(invalid_ttl)?;
        let conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let grant = ArtifactHandoffGrant {
            grant_id: allocate_artifact_handoff_id(
                &conn,
                ARTIFACT_HANDOFF_GRANT_ID_PREFIX,
                "SELECT 1 FROM wc_artifact_handoff_grants WHERE grant_id = ?1",
            )?,
            source_principal: source_principal.clone(),
            source_project,
            source_snapshot,
            destination_principal: input.destination_principal,
            destination_project,
            operation: input.operation,
            one_shot: input.one_shot,
            created_at_unix_ms: now_unix_ms,
            expires_at_unix_ms,
            revoked_at_unix_ms: None,
            consumed_at_unix_ms: None,
        };
        conn.execute(
            "INSERT INTO wc_artifact_handoff_grants (
                grant_id, source_principal_kind, source_principal_digest, source_project,
                source_path, source_bytes, source_sha256, source_mime_type, source_name,
                destination_principal_kind, destination_principal_digest, destination_project,
                operation, one_shot, created_at_unix_ms, expires_at_unix_ms,
                revoked_at_unix_ms, consumed_at_unix_ms
             ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, NULL, NULL
             )",
            params![
                grant.grant_id,
                grant.source_principal.kind,
                grant.source_principal.digest,
                grant.source_project,
                grant.source_snapshot.path,
                i64::try_from(grant.source_snapshot.bytes).map_err(|_| invalid_snapshot())?,
                grant.source_snapshot.sha256,
                grant.source_snapshot.mime_type,
                grant.source_snapshot.name,
                grant.destination_principal.kind,
                grant.destination_principal.digest,
                grant.destination_project,
                grant.operation.as_str(),
                i64::from(grant.one_shot),
                grant.created_at_unix_ms,
                grant.expires_at_unix_ms,
            ],
        )
        .map_err(store_error)?;
        Ok(grant)
    }

    pub fn read_artifact_handoff_grant(
        &self,
        principal: &ArtifactHandoffPrincipal,
        project: &str,
        grant_id: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffGrant, ArtifactHandoffStoreError> {
        validate_principal(principal).map_err(|_| unavailable())?;
        let project = validate_project(project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX) {
            return Err(unavailable());
        }
        let conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let grant = load_grant(&conn, grant_id)?.ok_or_else(unavailable)?;
        if !grant.authorizes_read(principal, &project) || !grant.active_at(now_unix_ms) {
            return Err(unavailable());
        }
        Ok(grant)
    }

    pub fn revoke_artifact_handoff_grant(
        &self,
        source_principal: &ArtifactHandoffPrincipal,
        source_project: &str,
        grant_id: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffGrant, ArtifactHandoffStoreError> {
        validate_principal(source_principal).map_err(|_| unavailable())?;
        let source_project = validate_project(source_project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX) {
            return Err(unavailable());
        }
        let mut conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let mut grant = load_grant(&transaction, grant_id)?.ok_or_else(unavailable)?;
        if &grant.source_principal != source_principal
            || grant.source_project != source_project
            || !grant.active_at(now_unix_ms)
        {
            return Err(unavailable());
        }
        let revoked_at = now_unix_ms.max(grant.created_at_unix_ms);
        transaction
            .execute(
                "UPDATE wc_artifact_handoff_grants
                    SET revoked_at_unix_ms = ?2
                  WHERE grant_id = ?1 AND revoked_at_unix_ms IS NULL AND consumed_at_unix_ms IS NULL",
                params![grant_id, revoked_at],
            )
            .map_err(store_error)?;
        grant.revoked_at_unix_ms = Some(revoked_at);
        transaction.commit().map_err(store_error)?;
        Ok(grant)
    }

    /// Reserve or replay one logical acceptance identity for this grant.
    ///
    /// This validates durable authority and idempotency state only. It does not
    /// read source bytes, contact a Runner, or write to the destination Project.
    pub fn begin_artifact_handoff_acceptance(
        &self,
        destination_principal: &ArtifactHandoffPrincipal,
        destination_project: &str,
        grant_id: &str,
        idempotency_key: &str,
        request_hash: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffAcceptanceClaim, ArtifactHandoffStoreError> {
        validate_principal(destination_principal).map_err(|_| unavailable())?;
        let destination_project =
            validate_project(destination_project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX) {
            return Err(unavailable());
        }
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        validate_request_hash(request_hash)?;
        let mut conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let grant = load_grant(&transaction, grant_id)?.ok_or_else(unavailable)?;
        if &grant.destination_principal != destination_principal
            || grant.destination_project != destination_project
        {
            return Err(unavailable());
        }
        let key_hash = digest_text(ARTIFACT_HANDOFF_KEY_DIGEST_DOMAIN, &idempotency_key);
        if let Some(existing) =
            load_acceptance_by_key(&transaction, destination_principal, grant_id, &key_hash)?
        {
            if existing.destination_project != grant.destination_project {
                return Err(persisted_state_error());
            }
            if existing.request_hash != request_hash {
                return Err(idempotency_conflict());
            }
            return Ok(ArtifactHandoffAcceptanceClaim {
                grant,
                acceptance: existing,
                replayed: true,
            });
        }
        if grant.one_shot && acceptance_exists_for_grant(&transaction, &grant.grant_id)? {
            return Err(unavailable());
        }
        if !grant.active_at(now_unix_ms) {
            return Err(unavailable());
        }
        let acceptance = ArtifactHandoffAcceptance {
            acceptance_id: allocate_artifact_handoff_id(
                &transaction,
                ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX,
                "SELECT 1 FROM wc_artifact_handoff_acceptances WHERE acceptance_id = ?1",
            )?,
            grant_id: grant.grant_id.clone(),
            destination_principal: destination_principal.clone(),
            destination_project: grant.destination_project.clone(),
            request_hash: request_hash.to_string(),
            state: ArtifactHandoffAcceptanceState::Prepared,
            destination_reconcile_allowed: false,
            outcome: None,
            created_at_unix_ms: now_unix_ms,
            completed_at_unix_ms: None,
        };
        transaction
            .execute(
                "INSERT INTO wc_artifact_handoff_acceptances (
                    acceptance_id, grant_id, destination_principal_kind,
                    destination_principal_digest, destination_project, idempotency_key_hash,
                    request_hash, state, destination_reconcile_allowed,
                    destination_path, destination_bytes, destination_sha256,
                    created_at_unix_ms, completed_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'prepared', 0, NULL, NULL, NULL, ?8, NULL)",
                params![
                    acceptance.acceptance_id,
                    acceptance.grant_id,
                    acceptance.destination_principal.kind,
                    acceptance.destination_principal.digest,
                    acceptance.destination_project,
                    key_hash,
                    request_hash,
                    acceptance.created_at_unix_ms,
                ],
            )
            .map_err(store_error)?;
        transaction.commit().map_err(store_error)?;
        Ok(ArtifactHandoffAcceptanceClaim {
            grant,
            acceptance,
            replayed: false,
        })
    }

    /// Compute the full semantic request hash and begin one import acceptance.
    /// This is the data-plane entry point; callers must not precompute a hash
    /// from only the grant id or destination project.
    pub fn begin_artifact_handoff_import(
        &self,
        destination_principal: &ArtifactHandoffPrincipal,
        request: &ArtifactHandoffImportRequest,
        idempotency_key: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffAcceptanceClaim, ArtifactHandoffStoreError> {
        let request_hash = request.request_hash()?;
        self.begin_artifact_handoff_acceptance(
            destination_principal,
            &request.destination_project,
            &request.grant_id,
            idempotency_key,
            &request_hash,
            now_unix_ms,
        )
    }

    /// Recheck durable grant/acceptance authority immediately before a
    /// prepared claim starts or resumes a transfer. A replay of the same key
    /// cannot bypass revocation, expiry, or consumption by another acceptance.
    pub fn revalidate_artifact_handoff_acceptance(
        &self,
        destination_principal: &ArtifactHandoffPrincipal,
        destination_project: &str,
        grant_id: &str,
        acceptance_id: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffAcceptanceClaim, ArtifactHandoffStoreError> {
        validate_principal(destination_principal).map_err(|_| unavailable())?;
        let destination_project =
            validate_project(destination_project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX)
            || !valid_artifact_handoff_id(acceptance_id, ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX)
            || now_unix_ms <= 0
        {
            return Err(unavailable());
        }
        let mut conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let grant = load_grant(&transaction, grant_id)?.ok_or_else(unavailable)?;
        if &grant.destination_principal != destination_principal
            || grant.destination_project != destination_project
        {
            return Err(unavailable());
        }
        let acceptance = load_acceptance(&transaction, acceptance_id)?.ok_or_else(unavailable)?;
        if acceptance.grant_id != grant_id
            || &acceptance.destination_principal != destination_principal
            || acceptance.destination_project != destination_project
        {
            return Err(unavailable());
        }
        if acceptance.state == ArtifactHandoffAcceptanceState::Completed {
            transaction.commit().map_err(store_error)?;
            return Ok(ArtifactHandoffAcceptanceClaim {
                grant,
                acceptance,
                replayed: true,
            });
        }
        if !grant.active_at(now_unix_ms) {
            return Err(unavailable());
        }
        transaction.commit().map_err(store_error)?;
        Ok(ArtifactHandoffAcceptanceClaim {
            grant,
            acceptance,
            replayed: true,
        })
    }

    /// Recheck grant/acceptance authority after the final destination commit
    /// returned success or outcome-unknown and durably enable replay-time
    /// destination reconciliation. Earlier transfer phases keep this bit false.
    pub fn mark_artifact_handoff_acceptance_destination_reconcile_allowed(
        &self,
        destination_principal: &ArtifactHandoffPrincipal,
        destination_project: &str,
        grant_id: &str,
        acceptance_id: &str,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffAcceptanceClaim, ArtifactHandoffStoreError> {
        validate_principal(destination_principal).map_err(|_| unavailable())?;
        let destination_project =
            validate_project(destination_project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX)
            || !valid_artifact_handoff_id(acceptance_id, ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX)
            || now_unix_ms <= 0
        {
            return Err(unavailable());
        }
        let mut conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let grant = load_grant(&transaction, grant_id)?.ok_or_else(unavailable)?;
        if &grant.destination_principal != destination_principal
            || grant.destination_project != destination_project
        {
            return Err(unavailable());
        }
        let mut acceptance =
            load_acceptance(&transaction, acceptance_id)?.ok_or_else(unavailable)?;
        if acceptance.grant_id != grant_id
            || &acceptance.destination_principal != destination_principal
            || acceptance.destination_project != destination_project
        {
            return Err(unavailable());
        }
        if acceptance.state == ArtifactHandoffAcceptanceState::Completed {
            transaction.commit().map_err(store_error)?;
            return Ok(ArtifactHandoffAcceptanceClaim {
                grant,
                acceptance,
                replayed: true,
            });
        }
        if !grant.active_at(now_unix_ms) {
            return Err(unavailable());
        }
        if !acceptance.destination_reconcile_allowed {
            transaction
                .execute(
                    "UPDATE wc_artifact_handoff_acceptances
                        SET destination_reconcile_allowed = 1
                      WHERE acceptance_id = ?1 AND state = 'prepared'",
                    params![acceptance_id],
                )
                .map_err(store_error)?;
            acceptance.destination_reconcile_allowed = true;
        }
        transaction.commit().map_err(store_error)?;
        Ok(ArtifactHandoffAcceptanceClaim {
            grant,
            acceptance,
            replayed: false,
        })
    }

    /// Record the exact destination result after the existing transfer path
    /// completes. This store method never moves artifact bytes itself.
    pub fn complete_artifact_handoff_acceptance(
        &self,
        destination_principal: &ArtifactHandoffPrincipal,
        destination_project: &str,
        grant_id: &str,
        acceptance_id: &str,
        outcome: ArtifactHandoffAcceptanceOutcome,
        now_unix_ms: i64,
    ) -> Result<ArtifactHandoffAcceptance, ArtifactHandoffStoreError> {
        validate_principal(destination_principal).map_err(|_| unavailable())?;
        let destination_project =
            validate_project(destination_project).map_err(|_| unavailable())?;
        if !valid_artifact_handoff_id(grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX)
            || !valid_artifact_handoff_id(acceptance_id, ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX)
        {
            return Err(unavailable());
        }
        let outcome = validate_outcome(outcome)?;
        let mut conn = self.lock_connection(crate::StoreDomain::ArtifactHandoff);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let grant = load_grant(&transaction, grant_id)?.ok_or_else(unavailable)?;
        if &grant.destination_principal != destination_principal
            || grant.destination_project != destination_project
        {
            return Err(unavailable());
        }
        if outcome.destination_bytes != grant.source_snapshot.bytes
            || outcome.destination_sha256 != grant.source_snapshot.sha256
        {
            return Err(invalid_outcome());
        }
        let acceptance = load_acceptance(&transaction, acceptance_id)?.ok_or_else(unavailable)?;
        if acceptance.grant_id != grant_id
            || &acceptance.destination_principal != destination_principal
            || acceptance.destination_project != destination_project
        {
            return Err(unavailable());
        }
        if acceptance.state == ArtifactHandoffAcceptanceState::Completed {
            if acceptance.outcome.as_ref() == Some(&outcome) {
                return Ok(acceptance);
            }
            return Err(completion_conflict());
        }
        if !acceptance.destination_reconcile_allowed {
            return Err(unavailable());
        }
        let completed_at = now_unix_ms.max(acceptance.created_at_unix_ms);
        transaction
            .execute(
                "UPDATE wc_artifact_handoff_acceptances
                    SET state = 'completed', destination_path = ?2, destination_bytes = ?3,
                        destination_sha256 = ?4, completed_at_unix_ms = ?5
                  WHERE acceptance_id = ?1 AND state = 'prepared'",
                params![
                    acceptance_id,
                    outcome.destination_path,
                    i64::try_from(outcome.destination_bytes).map_err(|_| invalid_outcome())?,
                    outcome.destination_sha256,
                    completed_at,
                ],
            )
            .map_err(store_error)?;
        if grant.one_shot {
            transaction
                .execute(
                    "UPDATE wc_artifact_handoff_grants
                        SET consumed_at_unix_ms = COALESCE(consumed_at_unix_ms, ?2)
                      WHERE grant_id = ?1 AND revoked_at_unix_ms IS NULL",
                    params![grant_id, completed_at],
                )
                .map_err(store_error)?;
        }
        let completed = ArtifactHandoffAcceptance {
            state: ArtifactHandoffAcceptanceState::Completed,
            outcome: Some(outcome),
            completed_at_unix_ms: Some(completed_at),
            ..acceptance
        };
        transaction.commit().map_err(store_error)?;
        Ok(completed)
    }
}

fn normalized_ttl(ttl_ms: Option<i64>) -> Result<i64, ArtifactHandoffStoreError> {
    let ttl_ms = ttl_ms.unwrap_or(DEFAULT_ARTIFACT_HANDOFF_TTL_MS);
    if ttl_ms <= 0 {
        return Err(invalid_ttl());
    }
    Ok(ttl_ms.min(MAX_ARTIFACT_HANDOFF_TTL_MS))
}

fn validate_principal(
    principal: &ArtifactHandoffPrincipal,
) -> Result<(), ArtifactHandoffStoreError> {
    validate_communication_principal(&principal.as_communication_principal())
        .map_err(|_| invalid_principal())
}

fn validate_project(project: &str) -> Result<String, ArtifactHandoffStoreError> {
    if project.is_empty()
        || project.trim() != project
        || project.len() > MAX_ARTIFACT_HANDOFF_PROJECT_BYTES
        || project.contains('/')
        || project.contains('\\')
        || project.chars().any(char::is_control)
    {
        return Err(invalid_project());
    }
    Ok(project.to_string())
}

fn validate_snapshot(
    snapshot: ArtifactHandoffSourceSnapshot,
) -> Result<ArtifactHandoffSourceSnapshot, ArtifactHandoffStoreError> {
    if snapshot.path.is_empty()
        || snapshot.path.len() > MAX_ARTIFACT_HANDOFF_PATH_BYTES
        || snapshot.path.contains('\0')
        || snapshot.path.contains('\\')
        || snapshot.path.starts_with('/')
        || snapshot.path.split('/').any(|component| component == "..")
        || snapshot
            .path
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':')
        || snapshot.bytes > MAX_ARTIFACT_HANDOFF_SOURCE_BYTES
        || !is_lower_hex_sha256(&snapshot.sha256)
        || snapshot.mime_type.is_empty()
        || snapshot.mime_type.len() > MAX_ARTIFACT_HANDOFF_MIME_BYTES
        || snapshot.mime_type.chars().any(char::is_control)
        || snapshot.name.is_empty()
        || snapshot.name.len() > MAX_ARTIFACT_HANDOFF_NAME_BYTES
        || snapshot.name == "."
        || snapshot.name == ".."
        || snapshot
            .name
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\'))
        || snapshot.path.rsplit('/').next() != Some(snapshot.name.as_str())
    {
        return Err(invalid_snapshot());
    }
    Ok(snapshot)
}

fn validate_import_path(path: &str) -> Result<(), ArtifactHandoffStoreError> {
    if path.is_empty()
        || path.len() > MAX_ARTIFACT_HANDOFF_PATH_BYTES
        || path.contains('\0')
        || path.contains('\\')
        || path.starts_with('/')
        || path.split('/').any(|component| component == "..")
        || path.as_bytes().get(1).is_some_and(|byte| *byte == b':')
    {
        return Err(invalid(
            "invalid_artifact_handoff_import_path",
            "Artifact handoff import path is invalid",
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<String, ArtifactHandoffStoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > MAX_ARTIFACT_HANDOFF_IDEMPOTENCY_KEY_CHARS {
        return Err(invalid_idempotency_key());
    }
    Ok(value.to_string())
}

fn validate_request_hash(value: &str) -> Result<(), ArtifactHandoffStoreError> {
    if !is_lower_hex_sha256(value) {
        return Err(invalid_request_hash());
    }
    Ok(())
}

fn validate_outcome(
    outcome: ArtifactHandoffAcceptanceOutcome,
) -> Result<ArtifactHandoffAcceptanceOutcome, ArtifactHandoffStoreError> {
    if outcome.destination_path.is_empty()
        || outcome.destination_path.len() > MAX_ARTIFACT_HANDOFF_PATH_BYTES
        || outcome.destination_path.contains('\0')
        || outcome.destination_path.contains('\\')
        || outcome.destination_path.starts_with('/')
        || outcome
            .destination_path
            .split('/')
            .any(|component| component == "..")
        || outcome
            .destination_path
            .as_bytes()
            .get(1)
            .is_some_and(|byte| *byte == b':')
        || outcome.destination_bytes > MAX_ARTIFACT_HANDOFF_SOURCE_BYTES
        || !is_lower_hex_sha256(&outcome.destination_sha256)
    {
        return Err(invalid_outcome());
    }
    Ok(outcome)
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_artifact_handoff_id(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(webcodex_core::compact::decode::<12>)
        .is_some()
}

fn allocate_artifact_handoff_id(
    conn: &Connection,
    prefix: &str,
    exists_query: &str,
) -> Result<String, ArtifactHandoffStoreError> {
    for _ in 0..16 {
        let id = format!("{prefix}{}", webcodex_core::compact::random_suffix::<12>());
        let occupied = conn
            .query_row(exists_query, [&id], |row| row.get::<_, i64>(0))
            .optional()
            .map_err(store_error)?
            .is_some();
        if !occupied {
            return Ok(id);
        }
    }
    Err(ArtifactHandoffStoreError::new(
        "artifact_handoff_identity_exhausted",
        "Unable to allocate an unoccupied artifact handoff identity",
    ))
}

fn load_grant(
    conn: &Connection,
    grant_id: &str,
) -> Result<Option<ArtifactHandoffGrant>, ArtifactHandoffStoreError> {
    let persisted = conn
        .query_row(
            "SELECT grant_id, source_principal_kind, source_principal_digest, source_project,
                    source_path, source_bytes, source_sha256, source_mime_type, source_name,
                    destination_principal_kind, destination_principal_digest, destination_project,
                    operation, one_shot, created_at_unix_ms, expires_at_unix_ms,
                    revoked_at_unix_ms, consumed_at_unix_ms
               FROM wc_artifact_handoff_grants
              WHERE grant_id = ?1",
            params![grant_id],
            |row| {
                Ok(PersistedGrant {
                    grant_id: row.get(0)?,
                    source_principal_kind: row.get(1)?,
                    source_principal_digest: row.get(2)?,
                    source_project: row.get(3)?,
                    source_path: row.get(4)?,
                    source_bytes: row.get(5)?,
                    source_sha256: row.get(6)?,
                    source_mime_type: row.get(7)?,
                    source_name: row.get(8)?,
                    destination_principal_kind: row.get(9)?,
                    destination_principal_digest: row.get(10)?,
                    destination_project: row.get(11)?,
                    operation: row.get(12)?,
                    one_shot: row.get(13)?,
                    created_at_unix_ms: row.get(14)?,
                    expires_at_unix_ms: row.get(15)?,
                    revoked_at_unix_ms: row.get(16)?,
                    consumed_at_unix_ms: row.get(17)?,
                })
            },
        )
        .optional()
        .map_err(store_error)?;
    persisted.map(persisted_grant).transpose()
}

fn persisted_grant(
    persisted: PersistedGrant,
) -> Result<ArtifactHandoffGrant, ArtifactHandoffStoreError> {
    if !valid_artifact_handoff_id(&persisted.grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX)
        || persisted.source_bytes < 0
        || persisted.one_shot != 0 && persisted.one_shot != 1
    {
        return Err(persisted_state_error());
    }
    let source_principal = ArtifactHandoffPrincipal::try_from(CommunicationPrincipal {
        kind: persisted.source_principal_kind,
        digest: persisted.source_principal_digest,
    })
    .map_err(|_| persisted_state_error())?;
    let destination_principal = ArtifactHandoffPrincipal::try_from(CommunicationPrincipal {
        kind: persisted.destination_principal_kind,
        digest: persisted.destination_principal_digest,
    })
    .map_err(|_| persisted_state_error())?;
    let source_snapshot = validate_snapshot(ArtifactHandoffSourceSnapshot {
        path: persisted.source_path,
        bytes: u64::try_from(persisted.source_bytes).map_err(|_| persisted_state_error())?,
        sha256: persisted.source_sha256,
        mime_type: persisted.source_mime_type,
        name: persisted.source_name,
    })
    .map_err(|_| persisted_state_error())?;
    let grant = ArtifactHandoffGrant {
        grant_id: persisted.grant_id,
        source_principal,
        source_project: validate_project(&persisted.source_project)
            .map_err(|_| persisted_state_error())?,
        source_snapshot,
        destination_principal,
        destination_project: validate_project(&persisted.destination_project)
            .map_err(|_| persisted_state_error())?,
        operation: ArtifactHandoffOperation::from_db(&persisted.operation)?,
        one_shot: persisted.one_shot == 1,
        created_at_unix_ms: persisted.created_at_unix_ms,
        expires_at_unix_ms: persisted.expires_at_unix_ms,
        revoked_at_unix_ms: persisted.revoked_at_unix_ms,
        consumed_at_unix_ms: persisted.consumed_at_unix_ms,
    };
    if grant.created_at_unix_ms <= 0
        || grant.expires_at_unix_ms <= grant.created_at_unix_ms
        || grant.expires_at_unix_ms - grant.created_at_unix_ms > MAX_ARTIFACT_HANDOFF_TTL_MS
        || grant.source_principal == grant.destination_principal
        || grant
            .revoked_at_unix_ms
            .is_some_and(|revoked_at| revoked_at < grant.created_at_unix_ms)
        || grant
            .consumed_at_unix_ms
            .is_some_and(|consumed_at| consumed_at < grant.created_at_unix_ms)
        || (grant.revoked_at_unix_ms.is_some() && grant.consumed_at_unix_ms.is_some())
        || (grant.consumed_at_unix_ms.is_some() && !grant.one_shot)
    {
        return Err(persisted_state_error());
    }
    Ok(grant)
}

fn load_acceptance(
    conn: &Connection,
    acceptance_id: &str,
) -> Result<Option<ArtifactHandoffAcceptance>, ArtifactHandoffStoreError> {
    let persisted = conn
        .query_row(
            "SELECT acceptance_id, grant_id, destination_principal_kind,
                    destination_principal_digest, destination_project, idempotency_key_hash,
                    request_hash, state, destination_reconcile_allowed, destination_path,
                    destination_bytes, destination_sha256, created_at_unix_ms, completed_at_unix_ms
               FROM wc_artifact_handoff_acceptances
              WHERE acceptance_id = ?1",
            params![acceptance_id],
            persisted_acceptance_row,
        )
        .optional()
        .map_err(store_error)?;
    persisted.map(persisted_acceptance).transpose()
}

fn load_acceptance_by_key(
    conn: &Connection,
    destination_principal: &ArtifactHandoffPrincipal,
    grant_id: &str,
    key_hash: &str,
) -> Result<Option<ArtifactHandoffAcceptance>, ArtifactHandoffStoreError> {
    let persisted = conn
        .query_row(
            "SELECT acceptance_id, grant_id, destination_principal_kind,
                    destination_principal_digest, destination_project, idempotency_key_hash,
                    request_hash, state, destination_reconcile_allowed, destination_path,
                    destination_bytes, destination_sha256, created_at_unix_ms, completed_at_unix_ms
               FROM wc_artifact_handoff_acceptances
              WHERE destination_principal_kind = ?1
                AND destination_principal_digest = ?2
                AND grant_id = ?3
                AND idempotency_key_hash = ?4",
            params![
                destination_principal.kind,
                destination_principal.digest,
                grant_id,
                key_hash
            ],
            persisted_acceptance_row,
        )
        .optional()
        .map_err(store_error)?;
    persisted.map(persisted_acceptance).transpose()
}

fn persisted_acceptance_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PersistedAcceptance> {
    Ok(PersistedAcceptance {
        acceptance_id: row.get(0)?,
        grant_id: row.get(1)?,
        destination_principal_kind: row.get(2)?,
        destination_principal_digest: row.get(3)?,
        destination_project: row.get(4)?,
        idempotency_key_hash: row.get(5)?,
        request_hash: row.get(6)?,
        state: row.get(7)?,
        destination_reconcile_allowed: row.get(8)?,
        destination_path: row.get(9)?,
        destination_bytes: row.get(10)?,
        destination_sha256: row.get(11)?,
        created_at_unix_ms: row.get(12)?,
        completed_at_unix_ms: row.get(13)?,
    })
}

fn persisted_acceptance(
    persisted: PersistedAcceptance,
) -> Result<ArtifactHandoffAcceptance, ArtifactHandoffStoreError> {
    if !valid_artifact_handoff_id(
        &persisted.acceptance_id,
        ARTIFACT_HANDOFF_ACCEPTANCE_ID_PREFIX,
    ) || !valid_artifact_handoff_id(&persisted.grant_id, ARTIFACT_HANDOFF_GRANT_ID_PREFIX)
        || !is_lower_hex_sha256(&persisted.idempotency_key_hash)
        || !is_lower_hex_sha256(&persisted.request_hash)
        || persisted.created_at_unix_ms <= 0
        || !matches!(persisted.destination_reconcile_allowed, 0 | 1)
    {
        return Err(persisted_state_error());
    }
    let destination_principal = ArtifactHandoffPrincipal::try_from(CommunicationPrincipal {
        kind: persisted.destination_principal_kind,
        digest: persisted.destination_principal_digest,
    })
    .map_err(|_| persisted_state_error())?;
    let state = ArtifactHandoffAcceptanceState::from_db(&persisted.state)?;
    let outcome = match state {
        ArtifactHandoffAcceptanceState::Prepared => {
            if persisted.destination_path.is_some()
                || persisted.destination_bytes.is_some()
                || persisted.destination_sha256.is_some()
                || persisted.completed_at_unix_ms.is_some()
            {
                return Err(persisted_state_error());
            }
            None
        }
        ArtifactHandoffAcceptanceState::Completed => {
            let completed_at_unix_ms = persisted
                .completed_at_unix_ms
                .ok_or_else(persisted_state_error)?;
            if completed_at_unix_ms < persisted.created_at_unix_ms {
                return Err(persisted_state_error());
            }
            Some(
                validate_outcome(ArtifactHandoffAcceptanceOutcome {
                    destination_path: persisted
                        .destination_path
                        .ok_or_else(persisted_state_error)?,
                    destination_bytes: u64::try_from(
                        persisted
                            .destination_bytes
                            .ok_or_else(persisted_state_error)?,
                    )
                    .map_err(|_| persisted_state_error())?,
                    destination_sha256: persisted
                        .destination_sha256
                        .ok_or_else(persisted_state_error)?,
                })
                .map_err(|_| persisted_state_error())?,
            )
        }
    };
    Ok(ArtifactHandoffAcceptance {
        acceptance_id: persisted.acceptance_id,
        grant_id: persisted.grant_id,
        destination_principal,
        destination_project: validate_project(&persisted.destination_project)
            .map_err(|_| persisted_state_error())?,
        request_hash: persisted.request_hash,
        state,
        destination_reconcile_allowed: persisted.destination_reconcile_allowed == 1,
        outcome,
        created_at_unix_ms: persisted.created_at_unix_ms,
        completed_at_unix_ms: persisted.completed_at_unix_ms,
    })
}

fn acceptance_exists_for_grant(
    transaction: &Transaction<'_>,
    grant_id: &str,
) -> Result<bool, ArtifactHandoffStoreError> {
    transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wc_artifact_handoff_acceptances WHERE grant_id = ?1
             )",
            params![grant_id],
            |row| row.get(0),
        )
        .map_err(store_error)
}
