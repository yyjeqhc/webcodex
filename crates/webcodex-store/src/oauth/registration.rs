//! Dynamic registration policy and atomic client allocation for both listeners.
use super::{insert_client, validate_oauth_client_owner};
use crate::models::OAuthClientRecord;
use crate::{public_ingress, Database, PublicIngressFence};
use rusqlite::{params, Connection, TransactionBehavior};

#[derive(Debug, PartialEq, Eq)]
pub enum DynamicRegistrationOutcome {
    Created,
    OwnerUnavailable,
    IngressUnavailable,
    AtCapacity,
}

pub const MAX_CLIENT_REDIRECT_URIS: usize = 16;

pub fn valid_s256_challenge(challenge: &str) -> bool {
    challenge.len() == 43
        && challenge
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

pub fn valid_dynamic_client_scopes(scopes: &str) -> bool {
    let mut requested = scopes.split_ascii_whitespace().peekable();
    requested.peek().is_some()
        && requested.all(|s| webcodex_core::authority::profiles::LOCAL_USER.contains(&s))
}

/// Validate a redirect URI for OAuth client registration.
///
/// Rules:
/// - Must parse as an absolute URL.
/// - Scheme must be `http` or `https`.
/// - `http` is only allowed for loopback hosts (`localhost`, `127.0.0.1`,
///   `[::1]`). All other hosts must use `https`.
pub fn validate_redirect_uri(uri: &str) -> Result<(), String> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err("redirect_uri cannot be empty".to_string());
    }
    if trimmed.chars().any(|ch| ch.is_ascii_control()) {
        return Err("redirect_uri must not contain ASCII control characters".to_string());
    }
    let parsed =
        url::Url::parse(trimmed).map_err(|_| "redirect_uri is not a valid URL".to_string())?;
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("redirect_uri must not contain userinfo".to_string());
    }
    if parsed.fragment().is_some() {
        return Err("redirect_uri must not contain a fragment".to_string());
    }
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err("redirect_uri must use http or https".to_string());
    }
    let host = parsed.host_str().unwrap_or("");
    if scheme == "http" {
        let is_loopback = matches!(host, "localhost" | "127.0.0.1" | "::1" | "[::1]");
        if !is_loopback {
            return Err("http redirect_uri is only allowed for localhost/127.0.0.1; use https for other hosts".to_string());
        }
    }
    if host.is_empty() {
        return Err("redirect_uri must have a host".to_string());
    }
    Ok(())
}

impl Database {
    /// Check capacity and insert under one writer transaction, including when
    /// another process opens the same SQLite database.
    pub fn register_dynamic_oauth_client(
        &self,
        record: &OAuthClientRecord,
        ingress: Option<&PublicIngressFence>,
    ) -> anyhow::Result<DynamicRegistrationOutcome> {
        const MAX_DYNAMIC_CLIENTS: i64 = 128;
        const MAX_ENTRY_DYNAMIC_CLIENTS: i64 = 16;
        validate_oauth_client_owner(record)?;
        anyhow::ensure!(record.is_managed_user_owned(), "invalid dynamic client");
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registration = record
            .dynamic_registration
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("missing dynamic registration"))?;
        let owner = record
            .owner_user_id
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("missing registration owner"))?;
        if !public_ingress::owner_enabled(&tx, owner)? {
            return Ok(DynamicRegistrationOutcome::OwnerUnavailable);
        }
        let entry = match ingress {
            Some(fence) => {
                let Some(entry) = public_ingress::active_entry_for_fence(&tx, fence)? else {
                    return Ok(DynamicRegistrationOutcome::IngressUnavailable);
                };
                if !entry.oauth_dynamic_registration_enabled
                    || entry.owner_user_id != owner
                    || registration.issuer != entry.origin
                {
                    return Ok(DynamicRegistrationOutcome::IngressUnavailable);
                }
                Some(entry)
            }
            None => None,
        };
        anyhow::ensure!(
            record.revoked_at.is_none() && valid_dynamic_client_scopes(&record.allowed_scopes),
            "invalid registration scopes or state"
        );
        let mut redirects = record.redirect_uris.split('\n');
        anyhow::ensure!(
            redirects.clone().take(MAX_CLIENT_REDIRECT_URIS + 1).count()
                <= MAX_CLIENT_REDIRECT_URIS
                && redirects.all(|uri| uri.trim() == uri && validate_redirect_uri(uri).is_ok()),
            "invalid registration redirects"
        );

        let reclaim_before = record.created_at.saturating_sub(86_400);
        if let Some(entry) = &entry {
            if !make_registration_room(
                &tx,
                Some((&entry.entry_id, entry.auth_epoch)),
                MAX_ENTRY_DYNAMIC_CLIENTS,
                reclaim_before,
            )? {
                return Ok(DynamicRegistrationOutcome::AtCapacity);
            }
        }
        if !make_registration_room(&tx, None, MAX_DYNAMIC_CLIENTS, reclaim_before)? {
            return Ok(DynamicRegistrationOutcome::AtCapacity);
        }
        insert_client(&tx, record)?;
        if let Some(entry) = entry {
            tx.execute(
                "INSERT INTO oauth_public_ingress_clients(client_id,entry_id,registration_epoch) VALUES(?1,?2,?3)",
                params![record.client_id, entry.entry_id, entry.auth_epoch],
            )?;
        }
        tx.commit()?;
        Ok(DynamicRegistrationOutcome::Created)
    }
}

/// Reclaim only an idle client within the exhausted quota. A grant, including
/// a spent code awaiting normal cleanup, protects its registration.
fn make_registration_room(
    conn: &Connection,
    entry: Option<(&str, i64)>,
    limit: i64,
    reclaim_before: i64,
) -> anyhow::Result<bool> {
    // A Quick reconnect rotates the authorization epoch. Its old registrations
    // are no longer usable, so they must not exhaust the new attempt's slots.
    // The global (entry=None) cap still counts every retained registration.
    const CANDIDATES: &str =
        "FROM oauth_clients c LEFT JOIN oauth_public_ingress_clients b USING(client_id)
        WHERE c.dynamic_registration IS NOT NULL
          AND (?1 IS NULL OR (b.entry_id=?1 AND b.registration_epoch=?2))";
    let (entry_id, registration_epoch) = entry
        .map(|(id, epoch)| (Some(id), Some(epoch)))
        .unwrap_or((None, None));
    let count: i64 = conn.query_row(
        &format!("SELECT count(*) {CANDIDATES}"),
        params![entry_id, registration_epoch],
        |r| r.get(0),
    )?;
    if count < limit {
        return Ok(true);
    }
    Ok(conn.execute(
        &format!(
            "DELETE FROM oauth_clients WHERE id = (SELECT c.id {CANDIDATES}
        AND (c.created_at <= ?3 OR c.revoked_at IS NOT NULL)
        AND NOT EXISTS (SELECT 1 FROM oauth_authorization_codes WHERE client_id=c.client_id)
        AND NOT EXISTS (SELECT 1 FROM oauth_access_tokens WHERE client_id=c.client_id)
        AND NOT EXISTS (SELECT 1 FROM oauth_refresh_tokens WHERE client_id=c.client_id)
        ORDER BY c.created_at, c.id LIMIT 1)"
        ),
        params![entry_id, registration_epoch, reclaim_before],
    )? == 1)
}
