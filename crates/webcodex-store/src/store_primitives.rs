//! Store-internal identity, proof, digest and idempotency primitives.
//!
//! These have no dependency on Conversation/Task/Wake operations. Historical
//! domain strings, public types, table names and encodings remain unchanged.
//! Callers own the immediate transaction; helpers never commit independently.

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

pub const COMMUNICATION_PRINCIPAL_DIGEST_PREFIX: &str = "wc_commprincipal_";
pub(crate) const MAX_COMMUNICATION_IDEMPOTENCY_KEY_CHARS: usize = 128;
const MAX_COMMUNICATION_PRINCIPAL_KIND_CHARS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationStoreError {
    code: &'static str,
    message: String,
    current_profile_revision: Option<i64>,
}

impl CommunicationStoreError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            current_profile_revision: None,
        }
    }

    pub(super) fn profile_changed(current_profile_revision: i64) -> Self {
        Self {
            code: "agent_profile_changed",
            message: format!(
                "Agent profile changed; current profile revision is {current_profile_revision}"
            ),
            current_profile_revision: Some(current_profile_revision),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn current_profile_revision(&self) -> Option<i64> {
        self.current_profile_revision
    }
}

impl std::fmt::Display for CommunicationStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommunicationStoreError {}

pub(super) fn store_error(error: rusqlite::Error) -> CommunicationStoreError {
    tracing::warn!(error = %error, "durable communication store operation failed");
    CommunicationStoreError::new(
        "communication_store_unavailable",
        "Durable communication store is unavailable",
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommunicationPrincipal {
    pub kind: String,
    pub digest: String,
}

pub(super) fn lookup_idempotent_resource(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
) -> Result<Option<String>, CommunicationStoreError> {
    let key_hash = digest_text("webcodex.communication.idempotency-key.v1", idempotency_key);
    let existing: Option<(String, String)> = transaction
        .query_row(
            "SELECT request_hash, resource_id FROM wc_communication_idempotency
             WHERE principal_digest = ?1 AND operation = ?2 AND key_hash = ?3",
            params![principal.digest, operation, key_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(store_error)?;
    match existing {
        None => Ok(None),
        Some((existing_request_hash, resource_id)) if existing_request_hash == request_hash => {
            Ok(Some(resource_id))
        }
        Some(_) => Err(CommunicationStoreError::new(
            "communication_idempotency_conflict",
            "Idempotency key was already used with a different request",
        )),
    }
}

pub(super) fn record_idempotent_resource(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    operation: &str,
    idempotency_key: &str,
    request_hash: &str,
    resource_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let key_hash = digest_text("webcodex.communication.idempotency-key.v1", idempotency_key);
    transaction
        .execute(
            "INSERT INTO wc_communication_idempotency (
                principal_digest, operation, key_hash, request_hash,
                resource_id, created_at_unix_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                principal.digest,
                operation,
                key_hash,
                request_hash,
                resource_id,
                now,
            ],
        )
        .map_err(store_error)?;
    Ok(())
}

pub(crate) fn validate_communication_principal(
    principal: &CommunicationPrincipal,
) -> Result<(), CommunicationStoreError> {
    let kind = principal.kind.trim();
    if kind.is_empty()
        || kind.chars().count() > MAX_COMMUNICATION_PRINCIPAL_KIND_CHARS
        || !kind.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':' | '.')
        })
    {
        return Err(CommunicationStoreError::new(
            "invalid_communication_principal",
            "Communication principal kind is invalid",
        ));
    }
    validate_digest_id(
        &principal.digest,
        COMMUNICATION_PRINCIPAL_DIGEST_PREFIX,
        "invalid_communication_principal",
    )
}

fn validate_digest_id(
    value: &str,
    prefix: &str,
    code: &'static str,
) -> Result<(), CommunicationStoreError> {
    let suffix = value.strip_prefix(prefix).ok_or_else(|| {
        CommunicationStoreError::new(code, "Communication principal digest is invalid")
    })?;
    if suffix.len() != 64
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CommunicationStoreError::new(
            code,
            "Communication principal digest is invalid",
        ));
    }
    Ok(())
}

pub(super) fn validate_idempotency_key(value: &str) -> Result<String, CommunicationStoreError> {
    validate_nonempty_chars(
        value,
        MAX_COMMUNICATION_IDEMPOTENCY_KEY_CHARS,
        "invalid_communication_idempotency_key",
        "idempotency_key",
    )
}

pub(super) fn validate_nonempty_chars(
    value: &str,
    max_chars: usize,
    code: &'static str,
    label: &str,
) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > max_chars {
        return Err(CommunicationStoreError::new(
            code,
            format!("{label} must contain 1..={max_chars} characters"),
        ));
    }
    Ok(value.to_string())
}

pub(crate) fn validate_id(
    value: &str,
    prefix: &str,
    code: &'static str,
) -> Result<(), CommunicationStoreError> {
    let suffix = value.strip_prefix(prefix).ok_or_else(|| {
        CommunicationStoreError::new(code, format!("Invalid canonical id: {value}"))
    })?;
    if webcodex_core::compact::decode::<12>(suffix).is_none() {
        return Err(CommunicationStoreError::new(
            code,
            format!("Invalid canonical id: {value}"),
        ));
    }
    Ok(())
}

// The caller owns an IMMEDIATE transaction through insertion, so the check
// and subsequent PK insert are atomic with respect to all other writers.
pub(super) fn allocate_identity(
    conn: &Connection,
    prefix: &str,
    exists_query: &str,
) -> Result<String, CommunicationStoreError> {
    allocate_identity_with(conn, exists_query, || {
        format!("{prefix}{}", webcodex_core::compact::random_suffix::<12>())
    })
}

pub(super) fn allocate_identity_with(
    conn: &Connection,
    exists_query: &str,
    mut generate: impl FnMut() -> String,
) -> Result<String, CommunicationStoreError> {
    for _ in 0..16 {
        let id = generate();
        let occupied: bool = conn
            .query_row(exists_query, [&id], |row| row.get(0))
            .map_err(store_error)?;
        if !occupied {
            return Ok(id);
        }
    }
    Err(CommunicationStoreError::new(
        "identity_allocation_exhausted",
        "Unable to allocate an unoccupied identity",
    ))
}

pub(super) fn new_proof(prefix: &str) -> String {
    format!("{prefix}{}", webcodex_core::compact::random_suffix::<16>())
}

pub(super) fn validate_proof(
    value: &str,
    prefix: &str,
    code: &'static str,
) -> Result<(), CommunicationStoreError> {
    if value
        .strip_prefix(prefix)
        .and_then(webcodex_core::compact::decode::<16>)
        .is_some()
    {
        Ok(())
    } else {
        Err(CommunicationStoreError::new(
            code,
            "Invalid canonical proof",
        ))
    }
}

struct Sha256Writer<'a>(&'a mut Sha256);

impl Write for Sha256Writer<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.update(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn digest_json<T: Serialize + ?Sized>(
    domain: &str,
    value: &T,
) -> Result<String, serde_json::Error> {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    let mut writer = Sha256Writer(&mut hasher);
    serde_json::to_writer(&mut writer, value)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn digest_text(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn now_unix_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
