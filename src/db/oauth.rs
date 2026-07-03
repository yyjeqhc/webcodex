use super::Database;
use crate::models::{
    OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    OAuthRefreshTokenRecord,
};
use rusqlite::params;

fn validate_oauth_subject(
    subject_kind: &str,
    subject_id: &str,
    user_id: Option<&str>,
    shared_key_hash: Option<&str>,
) -> anyhow::Result<()> {
    match subject_kind {
        "managed_user" => {
            let user_id = user_id
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("managed_user OAuth subject requires user_id"))?;
            if subject_id != user_id {
                anyhow::bail!("managed_user OAuth subject_id must match user_id");
            }
        }
        "shared_key" => {
            if user_id.is_some() {
                anyhow::bail!("shared_key OAuth subject must not include user_id");
            }
            let shared_key_hash = shared_key_hash
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("shared_key OAuth subject requires shared_key_hash")
                })?;
            if subject_id != shared_key_hash {
                anyhow::bail!("shared_key OAuth subject_id must match shared_key_hash");
            }
        }
        _ => anyhow::bail!("unknown OAuth subject_kind: {}", subject_kind),
    }
    if subject_id.trim().is_empty() {
        anyhow::bail!("OAuth subject_id must not be empty");
    }
    Ok(())
}

fn validate_oauth_authorization_code_subject(
    record: &OAuthAuthorizationCodeRecord,
) -> anyhow::Result<()> {
    validate_oauth_subject(
        &record.subject_kind,
        &record.subject_id,
        record.user_id.as_deref(),
        record.shared_key_hash.as_deref(),
    )
}

fn validate_oauth_access_token_subject(record: &OAuthAccessTokenRecord) -> anyhow::Result<()> {
    validate_oauth_subject(
        &record.subject_kind,
        &record.subject_id,
        record.user_id.as_deref(),
        record.shared_key_hash.as_deref(),
    )
}

fn validate_oauth_refresh_token_subject(record: &OAuthRefreshTokenRecord) -> anyhow::Result<()> {
    validate_oauth_subject(
        &record.subject_kind,
        &record.subject_id,
        record.user_id.as_deref(),
        record.shared_key_hash.as_deref(),
    )
}

fn validate_oauth_subjects_match(
    left_kind: &str,
    left_id: &str,
    right_kind: &str,
    right_id: &str,
) -> anyhow::Result<()> {
    if left_kind != right_kind || left_id != right_id {
        anyhow::bail!("OAuth token subjects must match");
    }
    Ok(())
}

impl Database {
    // --- OAuth clients ---

    pub fn insert_oauth_client(&self, record: &OAuthClientRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO oauth_clients (
                id, client_id, client_secret_hash, name, owner_user_id,
                redirect_uris, allowed_scopes, created_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                record.id,
                record.client_id,
                record.client_secret_hash,
                record.name,
                record.owner_user_id,
                record.redirect_uris,
                record.allowed_scopes,
                record.created_at,
                record.revoked_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_oauth_client_by_client_id(
        &self,
        client_id: &str,
    ) -> anyhow::Result<Option<OAuthClientRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, client_id, client_secret_hash, name, owner_user_id,
                    redirect_uris, allowed_scopes, created_at, revoked_at
             FROM oauth_clients WHERE client_id = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![client_id], row_to_oauth_client)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn get_oauth_client_by_id(&self, id: &str) -> anyhow::Result<Option<OAuthClientRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, client_id, client_secret_hash, name, owner_user_id,
                    redirect_uris, allowed_scopes, created_at, revoked_at
             FROM oauth_clients WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_oauth_client)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn revoke_oauth_client(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_clients SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    /// Verify that `plaintext_secret` matches the stored hash for the given
    /// client. Returns `true` if the hash matches, `false` otherwise. Uses
    /// constant-time comparison to avoid timing leaks. Does not leak the hash
    /// or plaintext on mismatch.
    pub fn verify_oauth_client_secret(
        &self,
        client_id: &str,
        plaintext_secret: &str,
    ) -> anyhow::Result<bool> {
        let client = self.get_oauth_client_by_client_id(client_id)?;
        let Some(client) = client else {
            return Ok(false);
        };
        let computed = crate::auth::hash_token(plaintext_secret);
        Ok(crate::config::constant_time_eq(
            computed.as_bytes(),
            client.client_secret_hash.as_bytes(),
        ))
    }

    /// List all OAuth clients (including revoked ones), ordered by creation
    /// time descending. Used by the first-party client management API.
    pub fn list_oauth_clients(&self) -> anyhow::Result<Vec<OAuthClientRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, client_id, client_secret_hash, name, owner_user_id,
                    redirect_uris, allowed_scopes, created_at, revoked_at
             FROM oauth_clients ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_oauth_client)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Revoke an OAuth client by its public `client_id` (e.g. `wc_client_*`).
    /// Idempotent: already-revoked clients are left untouched and still count
    /// as success. Returns `true` when a row matched the `client_id`.
    pub fn revoke_oauth_client_by_client_id(
        &self,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_clients SET revoked_at = COALESCE(revoked_at, ?2) \
             WHERE client_id = ?1",
            params![client_id, ts],
        )?;
        Ok(updated > 0)
    }

    /// Revoke all active access tokens belonging to `client_id`. Returns the
    /// number of rows updated. Idempotent (already-revoked tokens use
    /// `COALESCE` and are not double-stamped).
    pub fn revoke_oauth_access_tokens_for_client(
        &self,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_access_tokens SET revoked_at = COALESCE(revoked_at, ?2) \
             WHERE client_id = ?1",
            params![client_id, ts],
        )?;
        Ok(updated)
    }

    /// Revoke all active refresh tokens belonging to `client_id`. Returns the
    /// number of rows updated.
    pub fn revoke_oauth_refresh_tokens_for_client(
        &self,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_refresh_tokens SET revoked_at = COALESCE(revoked_at, ?2) \
             WHERE client_id = ?1",
            params![client_id, ts],
        )?;
        Ok(updated)
    }

    /// Revoke all active authorization codes belonging to `client_id`.
    /// Returns the number of rows updated.
    pub fn revoke_oauth_authorization_codes_for_client(
        &self,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<usize> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_authorization_codes SET revoked_at = COALESCE(revoked_at, ?2) \
             WHERE client_id = ?1",
            params![client_id, ts],
        )?;
        Ok(updated)
    }

    // --- OAuth authorization codes ---

    pub fn insert_oauth_authorization_code(
        &self,
        record: &OAuthAuthorizationCodeRecord,
        code_hash: &str,
    ) -> anyhow::Result<()> {
        validate_oauth_authorization_code_subject(record)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO oauth_authorization_codes (
                id, code_hash, client_id, subject_kind, subject_id, user_id,
                redirect_uri, scopes, code_challenge, code_challenge_method,
                resource, shared_key_hash, created_at, expires_at, used_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                record.id,
                code_hash,
                record.client_id,
                record.subject_kind,
                record.subject_id,
                record.user_id,
                record.redirect_uri,
                record.scopes,
                record.code_challenge,
                record.code_challenge_method,
                record.resource,
                record.shared_key_hash,
                record.created_at,
                record.expires_at,
                record.used_at,
                record.revoked_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_oauth_authorization_code_by_hash(
        &self,
        code_hash: &str,
    ) -> anyhow::Result<Option<OAuthAuthorizationCodeRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, code_hash, client_id, subject_kind, subject_id, user_id,
                    redirect_uri, scopes, code_challenge, code_challenge_method,
                    resource, shared_key_hash, created_at, expires_at, used_at, revoked_at
             FROM oauth_authorization_codes
             WHERE code_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![code_hash], row_to_oauth_authorization_code)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn mark_oauth_authorization_code_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_authorization_codes SET used_at = ?2 WHERE id = ?1 AND used_at IS NULL",
            params![id, ts],
        )?;
        Ok(())
    }

    /// Atomically consume an authorization code by its hash. The code is
    /// consumed (used_at set) only if **all** of the following hold:
    ///
    /// - `code_hash` matches
    /// - `revoked_at IS NULL`
    /// - `used_at IS NULL`
    /// - `expires_at > now`
    ///
    /// On success, returns the consumed record (with `used_at` set to `now`).
    /// On failure (already used, expired, revoked, or unknown), returns
    /// `Ok(None)`.
    ///
    /// This is the preferred helper for `/oauth/token` code exchange because it
    /// guarantees single-use semantics in a single SQL statement. The older
    /// `mark_oauth_authorization_code_used()` is retained for backward
    /// compatibility but should not be used for new token exchange flows.
    pub fn consume_oauth_authorization_code_by_hash(
        &self,
        code_hash: &str,
        now: i64,
    ) -> anyhow::Result<Option<OAuthAuthorizationCodeRecord>> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE oauth_authorization_codes
             SET used_at = ?2
             WHERE code_hash = ?1
               AND revoked_at IS NULL
               AND used_at IS NULL
               AND expires_at > ?2",
            params![code_hash, now],
        )?;
        if changed == 0 {
            return Ok(None);
        }
        // The UPDATE succeeded; fetch the consumed record.
        drop(conn);
        self.get_oauth_authorization_code_by_hash_for_consume(code_hash)
    }

    /// Internal helper: fetch an authorization code by hash **including** used
    /// and revoked rows. Only used after a successful consume to return the
    /// record with `used_at` set. Not for general lookups.
    fn get_oauth_authorization_code_by_hash_for_consume(
        &self,
        code_hash: &str,
    ) -> anyhow::Result<Option<OAuthAuthorizationCodeRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, code_hash, client_id, subject_kind, subject_id, user_id,
                    redirect_uri, scopes, code_challenge, code_challenge_method,
                    resource, shared_key_hash, created_at, expires_at, used_at, revoked_at
             FROM oauth_authorization_codes
             WHERE code_hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![code_hash], row_to_oauth_authorization_code)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn revoke_oauth_authorization_code(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_authorization_codes SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    /// Atomically exchange an authorization code for access + refresh tokens.
    ///
    /// Within a single SQLite transaction:
    /// 1. Consume the authorization code (same semantics as
    ///    `consume_oauth_authorization_code_by_hash`).
    /// 2. Insert the access token.
    /// 3. Insert the refresh token.
    /// 4. Commit.
    ///
    /// Returns:
    /// - `Ok(Some(record))` — exchange succeeded; record is the consumed code.
    /// - `Ok(None)` — code invalid, expired, used, or revoked.
    /// - `Err(_)` — DB error; nothing is committed.
    ///
    /// Pre-condition: client authentication must be verified before calling
    /// this method. Post-condition: client_id / redirect_uri / PKCE checks
    /// are **not** performed here — the caller must validate them after.
    pub fn exchange_oauth_authorization_code_for_tokens(
        &self,
        code_hash: &str,
        now: i64,
        access_token_record: &OAuthAccessTokenRecord,
        refresh_token_record: &OAuthRefreshTokenRecord,
    ) -> anyhow::Result<Option<OAuthAuthorizationCodeRecord>> {
        validate_oauth_access_token_subject(access_token_record)?;
        validate_oauth_refresh_token_subject(refresh_token_record)?;
        validate_oauth_subjects_match(
            &access_token_record.subject_kind,
            &access_token_record.subject_id,
            &refresh_token_record.subject_kind,
            &refresh_token_record.subject_id,
        )?;
        // Scope the transaction so the MutexGuard is dropped after commit,
        // allowing get_oauth_authorization_code_by_hash_for_consume to
        // re-acquire the lock.
        {
            let mut conn = self.conn.lock().unwrap();
            let tx = conn.transaction()?;

            // 1. Consume the authorization code atomically.
            let changed = tx.execute(
                "UPDATE oauth_authorization_codes
                 SET used_at = ?2
                 WHERE code_hash = ?1
                   AND revoked_at IS NULL
                   AND used_at IS NULL
                   AND expires_at > ?2",
                params![code_hash, now],
            )?;
            if changed == 0 {
                tx.commit()?;
                return Ok(None);
            }

            let code_record = {
                let mut stmt = tx.prepare(
                    "SELECT id, code_hash, client_id, subject_kind, subject_id, user_id,
                            redirect_uri, scopes, code_challenge, code_challenge_method,
                            resource, shared_key_hash, created_at, expires_at, used_at, revoked_at
                     FROM oauth_authorization_codes
                     WHERE code_hash = ?1",
                )?;
                let mut rows =
                    stmt.query_map(params![code_hash], row_to_oauth_authorization_code)?;
                match rows.next() {
                    Some(r) => r?,
                    None => anyhow::bail!("consumed OAuth authorization code disappeared"),
                }
            };
            validate_oauth_authorization_code_subject(&code_record)?;
            validate_oauth_subjects_match(
                &code_record.subject_kind,
                &code_record.subject_id,
                &access_token_record.subject_kind,
                &access_token_record.subject_id,
            )?;

            // 2. Insert access token.
            tx.execute(
                "INSERT INTO oauth_access_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    access_token_record.id,
                    access_token_record.token_hash,
                    access_token_record.client_id,
                    access_token_record.subject_kind,
                    access_token_record.subject_id,
                    access_token_record.user_id,
                    access_token_record.scopes,
                    access_token_record.resource,
                    access_token_record.shared_key_hash,
                    access_token_record.created_at,
                    access_token_record.expires_at,
                    access_token_record.revoked_at,
                    access_token_record.last_used_at,
                ],
            )?;

            // 3. Insert refresh token.
            tx.execute(
                "INSERT INTO oauth_refresh_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at, rotated_from_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    refresh_token_record.id,
                    refresh_token_record.token_hash,
                    refresh_token_record.client_id,
                    refresh_token_record.subject_kind,
                    refresh_token_record.subject_id,
                    refresh_token_record.user_id,
                    refresh_token_record.scopes,
                    refresh_token_record.resource,
                    refresh_token_record.shared_key_hash,
                    refresh_token_record.created_at,
                    refresh_token_record.expires_at,
                    refresh_token_record.revoked_at,
                    refresh_token_record.last_used_at,
                    refresh_token_record.rotated_from_id,
                ],
            )?;

            tx.commit()?;
        } // MutexGuard dropped here.

        // Fetch the consumed code record (including used rows).
        self.get_oauth_authorization_code_by_hash_for_consume(code_hash)
    }

    // --- OAuth access tokens ---

    pub fn insert_oauth_access_token(&self, record: &OAuthAccessTokenRecord) -> anyhow::Result<()> {
        validate_oauth_access_token_subject(record)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO oauth_access_tokens (
                id, token_hash, client_id, subject_kind, subject_id, user_id,
                scopes, resource, shared_key_hash, created_at, expires_at,
                revoked_at, last_used_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                record.id,
                record.token_hash,
                record.client_id,
                record.subject_kind,
                record.subject_id,
                record.user_id,
                record.scopes,
                record.resource,
                record.shared_key_hash,
                record.created_at,
                record.expires_at,
                record.revoked_at,
                record.last_used_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_oauth_access_token_by_hash(
        &self,
        token_hash: &str,
    ) -> anyhow::Result<Option<OAuthAccessTokenRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at
             FROM oauth_access_tokens
             WHERE token_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![token_hash], row_to_oauth_access_token)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_oauth_access_token_last_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_access_tokens SET last_used_at = ?2 WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    pub fn revoke_oauth_access_token(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_access_tokens SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    /// Revoke an access token by its hash, but only if it belongs to the given
    /// `client_id`. Returns `true` if a row was updated (i.e. the token was
    /// found for this client and marked revoked — or was already revoked).
    ///
    /// This does **not** update `last_used_at`; revocation is not a "use".
    pub fn revoke_oauth_access_token_by_hash_for_client(
        &self,
        token_hash: &str,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_access_tokens SET revoked_at = COALESCE(revoked_at, ?3) \
             WHERE token_hash = ?1 AND client_id = ?2",
            params![token_hash, client_id, ts],
        )?;
        Ok(updated > 0)
    }

    // --- OAuth refresh tokens ---

    pub fn insert_oauth_refresh_token(
        &self,
        record: &OAuthRefreshTokenRecord,
    ) -> anyhow::Result<()> {
        validate_oauth_refresh_token_subject(record)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO oauth_refresh_tokens (
                id, token_hash, client_id, subject_kind, subject_id, user_id,
                scopes, resource, shared_key_hash, created_at, expires_at,
                revoked_at, last_used_at, rotated_from_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                record.id,
                record.token_hash,
                record.client_id,
                record.subject_kind,
                record.subject_id,
                record.user_id,
                record.scopes,
                record.resource,
                record.shared_key_hash,
                record.created_at,
                record.expires_at,
                record.revoked_at,
                record.last_used_at,
                record.rotated_from_id,
            ],
        )?;
        Ok(())
    }

    pub fn get_oauth_refresh_token_by_hash(
        &self,
        token_hash: &str,
    ) -> anyhow::Result<Option<OAuthRefreshTokenRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at, rotated_from_id
             FROM oauth_refresh_tokens
             WHERE token_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![token_hash], row_to_oauth_refresh_token)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_oauth_refresh_token_last_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_refresh_tokens SET last_used_at = ?2 WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    pub fn revoke_oauth_refresh_token(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE oauth_refresh_tokens SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    /// Revoke a refresh token by its hash, but only if it belongs to the given
    /// `client_id`. Returns `true` if a row was updated (i.e. the token was
    /// found for this client and marked revoked — or was already revoked).
    ///
    /// This does **not** update `last_used_at`; revocation is not a "use".
    pub fn revoke_oauth_refresh_token_by_hash_for_client(
        &self,
        token_hash: &str,
        client_id: &str,
        ts: i64,
    ) -> anyhow::Result<bool> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE oauth_refresh_tokens SET revoked_at = COALESCE(revoked_at, ?3) \
             WHERE token_hash = ?1 AND client_id = ?2",
            params![token_hash, client_id, ts],
        )?;
        Ok(updated > 0)
    }

    /// Internal helper: fetch a refresh token by hash **including** revoked and
    /// expired rows. Used by `rotate_oauth_refresh_token` and the refresh_token
    /// grant handler to distinguish "not found" from "revoked/expired" for
    /// better error responses.
    pub fn get_oauth_refresh_token_by_hash_for_rotate(
        &self,
        token_hash: &str,
    ) -> anyhow::Result<Option<OAuthRefreshTokenRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at, rotated_from_id
             FROM oauth_refresh_tokens
             WHERE token_hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![token_hash], row_to_oauth_refresh_token)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Atomically rotate a refresh token: revoke the old token, insert a new
    /// access token, and insert a new refresh token linked to the old one.
    ///
    /// Within a single SQLite transaction:
    /// 1. Look up the old refresh token by hash (including revoked/expired).
    /// 2. If not found → `Ok(RotateResult::NotFound)`.
    /// 3. If revoked → `Ok(RotateResult::Revoked)`.
    /// 4. If expired → `Ok(RotateResult::Expired)`.
    /// 5. If `old.client_id != client_id` → `Ok(RotateResult::ClientMismatch)`.
    /// 6. Revoke old token (`revoked_at = now`, `last_used_at = now`).
    /// 7. Insert new access token.
    /// 8. Insert new refresh token (`rotated_from_id = old.id`).
    /// 9. Commit.
    ///
    /// Returns `Ok(RotateResult::Rotated(record))` on success, where `record`
    /// is the old (now-revoked) refresh token. The caller can use its
    /// `user_id`, `scopes`, `resource`, `shared_key_hash`, and `client_id` to
    /// construct the success response.
    pub fn rotate_oauth_refresh_token(
        &self,
        refresh_token_hash: &str,
        client_id: &str,
        now: i64,
        access_token_record: &OAuthAccessTokenRecord,
        new_refresh_token_record: &OAuthRefreshTokenRecord,
    ) -> anyhow::Result<RotateResult> {
        validate_oauth_access_token_subject(access_token_record)?;
        validate_oauth_refresh_token_subject(new_refresh_token_record)?;
        validate_oauth_subjects_match(
            &access_token_record.subject_kind,
            &access_token_record.subject_id,
            &new_refresh_token_record.subject_kind,
            &new_refresh_token_record.subject_id,
        )?;
        // Scope the transaction so the MutexGuard is dropped after commit.
        {
            let mut conn = self.conn.lock().unwrap();
            let tx = conn.transaction()?;

            // 1. Look up old refresh token (including revoked/expired).
            let old = {
                let mut stmt = tx.prepare(
                    "SELECT id, token_hash, client_id, subject_kind, subject_id, user_id,
                            scopes, resource, shared_key_hash, created_at, expires_at,
                            revoked_at, last_used_at, rotated_from_id
                     FROM oauth_refresh_tokens
                     WHERE token_hash = ?1",
                )?;
                let mut rows =
                    stmt.query_map(params![refresh_token_hash], row_to_oauth_refresh_token)?;
                match rows.next() {
                    Some(r) => r?,
                    None => return Ok(RotateResult::NotFound),
                }
            }; // stmt and rows dropped here, releasing borrow on tx

            // 2. Check revoked.
            if old.revoked_at.is_some() {
                return Ok(RotateResult::Revoked);
            }

            // 3. Check expired.
            if old.expires_at <= now {
                return Ok(RotateResult::Expired);
            }

            // 4. Check client_id match.
            if old.client_id != client_id {
                return Ok(RotateResult::ClientMismatch);
            }

            validate_oauth_refresh_token_subject(&old)?;
            validate_oauth_subjects_match(
                &old.subject_kind,
                &old.subject_id,
                &access_token_record.subject_kind,
                &access_token_record.subject_id,
            )?;

            // 5. Revoke old token.
            let changed = tx.execute(
                "UPDATE oauth_refresh_tokens
                 SET revoked_at = ?2, last_used_at = ?2
                 WHERE id = ?1 AND revoked_at IS NULL AND expires_at > ?2",
                params![old.id, now],
            )?;
            if changed == 0 {
                // Race: token was revoked or expired between SELECT and UPDATE.
                tx.commit()?;
                return Ok(RotateResult::NotFound);
            }

            // 6. Insert new access token.
            tx.execute(
                "INSERT INTO oauth_access_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    access_token_record.id,
                    access_token_record.token_hash,
                    access_token_record.client_id,
                    access_token_record.subject_kind,
                    access_token_record.subject_id,
                    access_token_record.user_id,
                    access_token_record.scopes,
                    access_token_record.resource,
                    access_token_record.shared_key_hash,
                    access_token_record.created_at,
                    access_token_record.expires_at,
                    access_token_record.revoked_at,
                    access_token_record.last_used_at,
                ],
            )?;

            // 7. Insert new refresh token.
            tx.execute(
                "INSERT INTO oauth_refresh_tokens (
                    id, token_hash, client_id, subject_kind, subject_id, user_id,
                    scopes, resource, shared_key_hash, created_at, expires_at,
                    revoked_at, last_used_at, rotated_from_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    new_refresh_token_record.id,
                    new_refresh_token_record.token_hash,
                    new_refresh_token_record.client_id,
                    new_refresh_token_record.subject_kind,
                    new_refresh_token_record.subject_id,
                    new_refresh_token_record.user_id,
                    new_refresh_token_record.scopes,
                    new_refresh_token_record.resource,
                    new_refresh_token_record.shared_key_hash,
                    new_refresh_token_record.created_at,
                    new_refresh_token_record.expires_at,
                    new_refresh_token_record.revoked_at,
                    new_refresh_token_record.last_used_at,
                    new_refresh_token_record.rotated_from_id,
                ],
            )?;

            tx.commit()?;

            // Save old record metadata before the block ends (old is moved
            // into the RotateResult below).
            let rotated = OAuthRefreshTokenRecord {
                revoked_at: Some(now),
                last_used_at: Some(now),
                ..old
            };
            return Ok(RotateResult::Rotated(rotated));
        } // MutexGuard dropped here (unreachable — all paths return above).
    }
}

/// Result of a refresh token rotation attempt.
#[derive(Debug)]
pub enum RotateResult {
    /// Rotation succeeded. Contains the old (now-revoked) refresh token.
    Rotated(OAuthRefreshTokenRecord),
    /// Token hash not found in the database.
    NotFound,
    /// Token was already revoked.
    Revoked,
    /// Token has expired.
    Expired,
    /// Token's client_id does not match the requesting client.
    ClientMismatch,
}

fn row_to_oauth_client(row: &rusqlite::Row) -> rusqlite::Result<OAuthClientRecord> {
    Ok(OAuthClientRecord {
        id: row.get(0)?,
        client_id: row.get(1)?,
        client_secret_hash: row.get(2)?,
        name: row.get(3)?,
        owner_user_id: row.get(4)?,
        redirect_uris: row.get(5)?,
        allowed_scopes: row.get(6)?,
        created_at: row.get(7)?,
        revoked_at: row.get(8)?,
    })
}

fn row_to_oauth_authorization_code(
    row: &rusqlite::Row,
) -> rusqlite::Result<OAuthAuthorizationCodeRecord> {
    Ok(OAuthAuthorizationCodeRecord {
        id: row.get(0)?,
        code_hash: row.get(1)?,
        client_id: row.get(2)?,
        subject_kind: row.get(3)?,
        subject_id: row.get(4)?,
        user_id: row.get(5)?,
        redirect_uri: row.get(6)?,
        scopes: row.get(7)?,
        code_challenge: row.get(8)?,
        code_challenge_method: row.get(9)?,
        resource: row.get(10)?,
        shared_key_hash: row.get(11)?,
        created_at: row.get(12)?,
        expires_at: row.get(13)?,
        used_at: row.get(14)?,
        revoked_at: row.get(15)?,
    })
}

fn row_to_oauth_access_token(row: &rusqlite::Row) -> rusqlite::Result<OAuthAccessTokenRecord> {
    Ok(OAuthAccessTokenRecord {
        id: row.get(0)?,
        token_hash: row.get(1)?,
        client_id: row.get(2)?,
        subject_kind: row.get(3)?,
        subject_id: row.get(4)?,
        user_id: row.get(5)?,
        scopes: row.get(6)?,
        resource: row.get(7)?,
        shared_key_hash: row.get(8)?,
        created_at: row.get(9)?,
        expires_at: row.get(10)?,
        revoked_at: row.get(11)?,
        last_used_at: row.get(12)?,
    })
}

fn row_to_oauth_refresh_token(row: &rusqlite::Row) -> rusqlite::Result<OAuthRefreshTokenRecord> {
    Ok(OAuthRefreshTokenRecord {
        id: row.get(0)?,
        token_hash: row.get(1)?,
        client_id: row.get(2)?,
        subject_kind: row.get(3)?,
        subject_id: row.get(4)?,
        user_id: row.get(5)?,
        scopes: row.get(6)?,
        resource: row.get(7)?,
        shared_key_hash: row.get(8)?,
        created_at: row.get(9)?,
        expires_at: row.get(10)?,
        revoked_at: row.get(11)?,
        last_used_at: row.get(12)?,
        rotated_from_id: row.get(13)?,
    })
}
