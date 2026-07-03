use super::{Database, PairingConsumeResult};
use crate::models::{AccountCredentialRecord, ApiKeyRecord, PairingCodeRecord, UserRecord};
use rusqlite::params;

impl Database {
    pub fn get_user_by_username(&self, username: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users WHERE username = ?1",
        )?;
        let mut rows = stmt.query_map(params![username], row_to_user)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn create_user(&self, user: &UserRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO users (id, username, created_at, disabled, display_name, role, disabled_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                user.id,
                user.username,
                user.created_at,
                user.disabled,
                user.display_name,
                user.role,
                user.disabled_at,
                user.updated_at,
            ],
        )?;
        Ok(())
    }

    /// List all users ordered by username. Phase 2 admin surface.
    pub fn list_users(&self) -> anyhow::Result<Vec<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users ORDER BY username ASC",
        )?;
        let rows = stmt.query_map([], row_to_user)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn get_api_key_by_hash(&self, hash: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys
             WHERE key_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![hash], row_to_api_key)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn insert_api_key(&self, key: &ApiKeyRecord, key_hash: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, name, key_hash, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                key.id,
                key.user_id,
                key.name,
                key_hash,
                key.key_prefix,
                key.created_at,
                key.last_used_at,
                key.revoked_at,
                key.scopes,
                key.expires_at,
                key.kind,
                key.allowed_client_id,
            ],
        )?;
        Ok(())
    }

    pub fn insert_account_credential(
        &self,
        record: &AccountCredentialRecord,
        credential_hash: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO account_credentials (
                id, user_id, credential_hash, credential_prefix, created_at, last_used_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.id,
                record.user_id,
                credential_hash,
                record.credential_prefix,
                record.created_at,
                record.last_used_at,
                record.revoked_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_account_credential_by_hash(
        &self,
        hash: &str,
    ) -> anyhow::Result<Option<AccountCredentialRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, credential_prefix, created_at, last_used_at, revoked_at
             FROM account_credentials
             WHERE credential_hash = ?1 AND revoked_at IS NULL",
        )?;
        let mut rows = stmt.query_map(params![hash], row_to_account_credential)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_account_credential_last_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE account_credentials SET last_used_at = ?2 WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }

    pub fn revoke_account_credential(
        &self,
        id: &str,
        ts: i64,
    ) -> anyhow::Result<Option<AccountCredentialRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE account_credentials SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        drop(conn);
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, credential_prefix, created_at, last_used_at, revoked_at
             FROM account_credentials WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_account_credential)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn list_account_credentials_by_user(
        &self,
        user_id: &str,
    ) -> anyhow::Result<Vec<AccountCredentialRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, credential_prefix, created_at, last_used_at, revoked_at
             FROM account_credentials WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_account_credential)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_pairing_code(&self, record: &PairingCodeRecord) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO pairing_codes (
                id, code_hash, user_id, username, client_id, created_at, expires_at, used_at,
                user_token_name, agent_token_name
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                record.id,
                record.code_hash,
                record.user_id,
                record.username,
                record.client_id,
                record.created_at,
                record.expires_at,
                record.used_at,
                record.user_token_name,
                record.agent_token_name,
            ],
        )?;
        Ok(())
    }

    pub fn get_pairing_code_by_hash(
        &self,
        code_hash: &str,
    ) -> anyhow::Result<Option<PairingCodeRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, code_hash, user_id, username, client_id, created_at, expires_at, used_at,
                    user_token_name, agent_token_name
             FROM pairing_codes WHERE code_hash = ?1",
        )?;
        let mut rows = stmt.query_map(params![code_hash], row_to_pairing_code)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn consume_pairing_code(
        &self,
        code_hash: &str,
        client_id: &str,
        now: i64,
    ) -> anyhow::Result<PairingConsumeResult> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let record = {
            let mut stmt = tx.prepare(
                "SELECT id, code_hash, user_id, username, client_id, created_at, expires_at,
                        used_at, user_token_name, agent_token_name
                 FROM pairing_codes WHERE code_hash = ?1",
            )?;
            let mut rows = stmt.query_map(params![code_hash], row_to_pairing_code)?;
            match rows.next() {
                Some(r) => r?,
                None => return Ok(PairingConsumeResult::NotFound),
            }
        };
        if record.used_at.is_some() {
            return Ok(PairingConsumeResult::AlreadyUsed(record));
        }
        if record.expires_at <= now {
            return Ok(PairingConsumeResult::Expired(record));
        }
        if record.client_id != client_id {
            return Ok(PairingConsumeResult::ClientMismatch(record));
        }
        let changed = tx.execute(
            "UPDATE pairing_codes SET used_at = ?2
             WHERE id = ?1 AND used_at IS NULL AND expires_at > ?2 AND client_id = ?3",
            params![record.id, now, client_id],
        )?;
        tx.commit()?;
        if changed == 1 {
            Ok(PairingConsumeResult::Consumed(PairingCodeRecord {
                used_at: Some(now),
                ..record
            }))
        } else {
            // The connection mutex serializes pairing consumption in this
            // process, so reaching this branch should only happen if SQLite
            // reports an unexpected no-op update. Do not call back into helper
            // methods here: they would try to re-lock the same DB mutex.
            Ok(PairingConsumeResult::AlreadyUsed(record))
        }
    }

    pub fn list_api_keys_by_user(&self, user_id: &str) -> anyhow::Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_api_key)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// List only agent tokens (`kind='agent'`) for a user. Phase 3 agent-token
    /// management surface. Ordered by `created_at DESC`.
    pub fn list_agent_api_keys_by_user(&self, user_id: &str) -> anyhow::Result<Vec<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE user_id = ?1 AND kind = 'agent' ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![user_id], row_to_api_key)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Fetch a single api token by id (including revoked/expired rows). Used by
    /// the revoke endpoint and self-management lookups. Phase 2.
    pub fn get_api_key_by_id(&self, id: &str) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, user_id, name, key_prefix, created_at, last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id
             FROM api_keys WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_api_key)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    /// Mark an api token as revoked at `ts`. Idempotent: revoking an already
    /// revoked token is a no-op. Returns the post-revoke record when a row
    /// exists. Phase 2.
    pub fn revoke_api_key(&self, id: &str, ts: i64) -> anyhow::Result<Option<ApiKeyRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE api_keys SET revoked_at = COALESCE(revoked_at, ?2) WHERE id = ?1",
            params![id, ts],
        )?;
        drop(conn);
        self.get_api_key_by_id(id)
    }

    /// Disable (or re-enable) a user. When disabling, both the legacy
    /// `disabled` flag and the Phase 2 `disabled_at` timestamp are set so the
    /// existing AuthMiddleware check (`disabled != 0`) and the new
    /// `disabled_at`-based check agree. Phase 2.
    pub fn set_user_disabled(
        &self,
        id: &str,
        disabled: bool,
        ts: i64,
    ) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE users
             SET disabled = ?2,
                 disabled_at = CASE WHEN ?2 = 1 THEN COALESCE(disabled_at, ?3) ELSE NULL END,
                 updated_at = ?3
             WHERE id = ?1",
            params![id, if disabled { 1 } else { 0 }, ts],
        )?;
        drop(conn);
        self.get_user_by_id(id)
    }

    pub fn get_user_by_id(&self, id: &str) -> anyhow::Result<Option<UserRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, username, created_at, disabled, display_name, role, disabled_at, updated_at
             FROM users WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_user)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn update_api_key_last_used(&self, id: &str, ts: i64) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE api_keys SET last_used_at = ?2 WHERE id = ?1",
            params![id, ts],
        )?;
        Ok(())
    }
}

fn row_to_pairing_code(row: &rusqlite::Row) -> rusqlite::Result<PairingCodeRecord> {
    Ok(PairingCodeRecord {
        id: row.get(0)?,
        code_hash: row.get(1)?,
        user_id: row.get(2)?,
        username: row.get(3)?,
        client_id: row.get(4)?,
        created_at: row.get(5)?,
        expires_at: row.get(6)?,
        used_at: row.get(7)?,
        user_token_name: row.get(8)?,
        agent_token_name: row.get(9)?,
    })
}

/// Map a `users` row (8 columns, Phase 2 order) to a `UserRecord`. Columns are
/// positional: id, username, created_at, disabled, display_name, role,
/// disabled_at, updated_at.
fn row_to_user(row: &rusqlite::Row) -> rusqlite::Result<UserRecord> {
    Ok(UserRecord {
        id: row.get(0)?,
        username: row.get(1)?,
        created_at: row.get(2)?,
        disabled: row.get(3)?,
        display_name: row.get(4)?,
        role: row
            .get::<_, Option<String>>(5)?
            .unwrap_or_else(|| "user".to_string()),
        disabled_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Map an `api_keys` row (11 columns, Phase 3 order) to an `ApiKeyRecord`.
/// Columns are positional: id, user_id, name, key_prefix, created_at,
/// last_used_at, revoked_at, scopes, expires_at, kind, allowed_client_id.
/// Older rows without `kind`/`allowed_client_id` are filled in via the column
/// default (`kind="user"`, `allowed_client_id=NULL`) at the SQL level, so this
/// mapper only ever sees the full 11-column projection.
fn row_to_api_key(row: &rusqlite::Row) -> rusqlite::Result<ApiKeyRecord> {
    Ok(ApiKeyRecord {
        id: row.get(0)?,
        user_id: row.get(1)?,
        name: row.get(2)?,
        key_prefix: row.get(3)?,
        created_at: row.get(4)?,
        last_used_at: row.get(5)?,
        revoked_at: row.get(6)?,
        scopes: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
        expires_at: row.get(8)?,
        kind: row
            .get::<_, Option<String>>(9)?
            .unwrap_or_else(|| "user".to_string()),
        allowed_client_id: row.get(10)?,
    })
}

fn row_to_account_credential(row: &rusqlite::Row) -> rusqlite::Result<AccountCredentialRecord> {
    Ok(AccountCredentialRecord {
        id: row.get(0)?,
        user_id: row.get(1)?,
        credential_prefix: row.get(2)?,
        created_at: row.get(3)?,
        last_used_at: row.get(4)?,
        revoked_at: row.get(5)?,
    })
}
