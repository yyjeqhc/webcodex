//! Canonical public entry identity and OAuth grant audience. All transitions and
//! grant issuance share the SQLite writer transaction, including epoch checks.
use crate::Database;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicIngressMode {
    Named,
    Quick,
}
impl PublicIngressMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Named => "named",
            Self::Quick => "quick",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicIngressEntry {
    pub entry_id: String,
    pub profile_id: String,
    pub mode: PublicIngressMode,
    pub origin: String,
    pub owner_user_id: String,
    pub auth_epoch: i64,
    pub runtime_revision: i64,
    pub process_generation: i64,
    pub server_instance_id: String,
    pub active: bool,
    pub admission_open: bool,
    pub attempt_closed: bool,
}
impl PublicIngressEntry {
    pub fn fence(&self) -> PublicIngressFence {
        PublicIngressFence {
            entry_id: self.entry_id.clone(),
            runtime_revision: self.runtime_revision,
            process_generation: self.process_generation,
            server_instance_id: self.server_instance_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicIngressFence {
    pub entry_id: String,
    pub runtime_revision: i64,
    pub process_generation: i64,
    pub server_instance_id: String,
}

#[derive(Debug, Clone)]
pub struct PublicIngressEntrySpec {
    pub entry_id: String,
    pub profile_id: String,
    pub mode: PublicIngressMode,
    pub origin: String,
    pub owner_user_id: String,
    pub process_generation: i64,
    pub server_instance_id: String,
}

pub(crate) fn ensure_schema(conn: &mut Connection) -> anyhow::Result<()> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS public_ingress_entries (
            entry_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL UNIQUE,
            mode TEXT NOT NULL CHECK(mode IN ('named','quick')),
            origin TEXT NOT NULL,
            owner_user_id TEXT NOT NULL REFERENCES users(id),
            auth_epoch INTEGER NOT NULL CHECK(typeof(auth_epoch) = 'integer' AND auth_epoch > 0),
            runtime_revision INTEGER NOT NULL CHECK(runtime_revision > 0),
            process_generation INTEGER NOT NULL CHECK(process_generation > 0),
            server_instance_id TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 0 CHECK(active IN (0,1)),
            admission_open INTEGER NOT NULL DEFAULT 0 CHECK(admission_open IN (0,1)),
            attempt_closed INTEGER NOT NULL DEFAULT 0 CHECK(attempt_closed IN (0,1)),
            CHECK(admission_open = 0 OR (active = 1 AND attempt_closed = 0))
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_public_ingress_one_active
            ON public_ingress_entries(active) WHERE active = 1;
        CREATE TABLE IF NOT EXISTS public_ingress_configurations (
            profile_id TEXT PRIMARY KEY,
            configuration_id TEXT NOT NULL
        );
        CREATE TABLE IF NOT EXISTS oauth_public_ingress_clients (
            client_id TEXT PRIMARY KEY REFERENCES oauth_clients(client_id) ON DELETE CASCADE,
            entry_id TEXT NOT NULL REFERENCES public_ingress_entries(entry_id)
        );
        CREATE TABLE IF NOT EXISTS oauth_public_ingress_codes (
            grant_id TEXT PRIMARY KEY REFERENCES oauth_authorization_codes(id) ON DELETE CASCADE,
            entry_id TEXT NOT NULL REFERENCES public_ingress_entries(entry_id),
            auth_epoch INTEGER NOT NULL CHECK(typeof(auth_epoch) = 'integer' AND auth_epoch > 0)
        );
        CREATE TABLE IF NOT EXISTS oauth_public_ingress_access (
            grant_id TEXT PRIMARY KEY REFERENCES oauth_access_tokens(id) ON DELETE CASCADE,
            entry_id TEXT NOT NULL REFERENCES public_ingress_entries(entry_id),
            auth_epoch INTEGER NOT NULL CHECK(typeof(auth_epoch) = 'integer' AND auth_epoch > 0)
        );
        CREATE TABLE IF NOT EXISTS oauth_public_ingress_refresh (
            grant_id TEXT PRIMARY KEY REFERENCES oauth_refresh_tokens(id) ON DELETE CASCADE,
            entry_id TEXT NOT NULL REFERENCES public_ingress_entries(entry_id),
            auth_epoch INTEGER NOT NULL CHECK(typeof(auth_epoch) = 'integer' AND auth_epoch > 0)
        );
    ",
    )?;
    tx.commit()?;
    Ok(())
}

const ENTRY_COLUMNS: &str = "entry_id,profile_id,mode,origin,owner_user_id,auth_epoch,runtime_revision,process_generation,server_instance_id,active,admission_open,attempt_closed";
fn entry_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PublicIngressEntry> {
    let mode: String = row.get(2)?;
    let mode = match mode.as_str() {
        "named" => PublicIngressMode::Named,
        "quick" => PublicIngressMode::Quick,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(PublicIngressEntry {
        entry_id: row.get(0)?,
        profile_id: row.get(1)?,
        mode,
        origin: row.get(3)?,
        owner_user_id: row.get(4)?,
        auth_epoch: row.get(5)?,
        runtime_revision: row.get(6)?,
        process_generation: row.get(7)?,
        server_instance_id: row.get(8)?,
        active: row.get(9)?,
        admission_open: row.get(10)?,
        attempt_closed: row.get(11)?,
    })
}
fn entry_by_id(conn: &Connection, id: &str) -> anyhow::Result<Option<PublicIngressEntry>> {
    Ok(conn
        .query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM public_ingress_entries WHERE entry_id=?1"),
            [id],
            entry_row,
        )
        .optional()?)
}
pub(crate) fn active_entry_for_fence(
    conn: &Connection,
    fence: &PublicIngressFence,
) -> anyhow::Result<Option<PublicIngressEntry>> {
    Ok(entry_by_id(conn, &fence.entry_id)?
        .filter(|entry| entry.active && entry.admission_open && entry.fence() == *fence))
}
fn validate_spec(spec: &PublicIngressEntrySpec) -> anyhow::Result<()> {
    for value in [
        &spec.entry_id,
        &spec.profile_id,
        &spec.owner_user_id,
        &spec.server_instance_id,
    ] {
        anyhow::ensure!(
            !value.trim().is_empty() && value.len() <= 256,
            "invalid public ingress identity"
        );
    }
    anyhow::ensure!(
        spec.process_generation > 0 && spec.origin.len() <= 2048,
        "invalid public ingress generation or origin"
    );
    let url = url::Url::parse(&spec.origin)?;
    anyhow::ensure!(
        url.scheme() == "https" && url.origin().ascii_serialization() == spec.origin,
        "public ingress origin must be an exact canonical HTTPS origin"
    );
    Ok(())
}
fn owner_enabled(conn: &Connection, user_id: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM users WHERE id=?1 AND disabled=0)",
        [user_id],
        |row| row.get(0),
    )?)
}

impl Database {
    /// A deleted/recreated catalog profile never inherits its predecessor's
    /// grants, even when its display id, hostname and owner are reused.
    pub fn reconcile_public_ingress_configuration(
        &self,
        profile_id: &str,
        configuration_id: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            !profile_id.is_empty()
                && profile_id.len() <= 256
                && !configuration_id.is_empty()
                && configuration_id.len() <= 256,
            "invalid public ingress configuration identity"
        );
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row(
                "SELECT configuration_id FROM public_ingress_configurations WHERE profile_id=?1",
                [profile_id],
                |row| row.get(0),
            )
            .optional()?;
        if previous.as_deref().is_some_and(|id| id != configuration_id) {
            tx.execute("UPDATE public_ingress_entries SET active=0,admission_open=0,attempt_closed=1,auth_epoch=auth_epoch+1 WHERE profile_id=?1", [profile_id])?;
        }
        tx.execute("INSERT INTO public_ingress_configurations(profile_id,configuration_id) VALUES(?1,?2) ON CONFLICT(profile_id) DO UPDATE SET configuration_id=excluded.configuration_id", params![profile_id, configuration_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Issue and bind a client atomically. A concurrent private exchange cannot
    /// observe a briefly unbound public client. Replacement revokes previous
    /// clients only after the new binding has passed the same writer fence.
    pub fn insert_public_ingress_oauth_client(
        &self,
        record: &crate::models::OAuthClientRecord,
        fence: &PublicIngressFence,
        replace: bool,
    ) -> anyhow::Result<bool> {
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(entry) = active_entry_for_fence(&tx, fence)? else {
            return Ok(false);
        };
        if record.owner_user_id.as_deref() != Some(&entry.owner_user_id)
            || record.owner_project_grant_id.is_some()
            || record.owner_shared_key_hash.is_some()
            || record.revoked_at.is_some()
        {
            return Ok(false);
        }
        let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM oauth_public_ingress_clients b JOIN oauth_clients c ON c.client_id=b.client_id WHERE b.entry_id=?1 AND c.revoked_at IS NULL)", [&entry.entry_id], |row| row.get(0))?;
        if exists && !replace {
            return Ok(false);
        }
        tx.execute("INSERT INTO oauth_clients(id,client_id,client_secret_hash,name,owner_user_id,owner_project_grant_id,owner_shared_key_hash,redirect_uris,allowed_scopes,created_at,revoked_at) VALUES(?1,?2,?3,?4,?5,NULL,NULL,?6,?7,?8,NULL)", params![record.id, record.client_id, record.client_secret_hash, record.name, record.owner_user_id, record.redirect_uris, record.allowed_scopes, record.created_at])?;
        tx.execute(
            "INSERT INTO oauth_public_ingress_clients(client_id,entry_id) VALUES(?1,?2)",
            params![record.client_id, entry.entry_id],
        )?;
        if replace {
            tx.execute("UPDATE oauth_clients SET revoked_at=?3 WHERE revoked_at IS NULL AND client_id<>?2 AND client_id IN(SELECT client_id FROM oauth_public_ingress_clients WHERE entry_id=?1)", params![entry.entry_id, record.client_id, record.created_at])?;
        }
        tx.commit()?;
        Ok(true)
    }
    /// Discover Quick's origin before admission, without creating another
    /// transport generation. No grants can exist for a prepared attempt.
    pub fn finalize_public_ingress_origin(
        &self,
        fence: &PublicIngressFence,
        origin: &str,
    ) -> anyhow::Result<Option<PublicIngressEntry>> {
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(entry) = entry_by_id(&tx, &fence.entry_id)? else {
            return Ok(None);
        };
        if entry.fence() != *fence
            || entry.active
            || entry.admission_open
            || entry.attempt_closed
            || entry.mode != PublicIngressMode::Quick
        {
            return Ok(None);
        }
        validate_spec(&PublicIngressEntrySpec {
            entry_id: entry.entry_id.clone(),
            profile_id: entry.profile_id.clone(),
            mode: entry.mode,
            origin: origin.to_owned(),
            owner_user_id: entry.owner_user_id.clone(),
            process_generation: entry.process_generation,
            server_instance_id: entry.server_instance_id.clone(),
        })?;
        tx.execute(
            "UPDATE public_ingress_entries SET origin=?2 WHERE entry_id=?1",
            params![entry.entry_id, origin],
        )?;
        let updated = entry_by_id(&tx, &entry.entry_id)?;
        tx.commit()?;
        Ok(updated)
    }

    pub fn public_ingress_has_observed_authorization(
        &self,
        fence: &PublicIngressFence,
    ) -> anyhow::Result<bool> {
        let conn = self.lock_connection(crate::StoreDomain::OAuth);
        let Some(entry) = active_entry_for_fence(&conn, fence)? else {
            return Ok(false);
        };
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM oauth_public_ingress_access b JOIN oauth_access_tokens a ON a.id=b.grant_id JOIN oauth_clients c ON c.client_id=a.client_id JOIN users u ON u.id=c.owner_user_id WHERE b.entry_id=?1 AND b.auth_epoch=?2 AND a.last_used_at IS NOT NULL AND a.revoked_at IS NULL AND a.expires_at>?3 AND c.revoked_at IS NULL AND u.disabled=0 AND u.id=?4)", params![entry.entry_id, entry.auth_epoch, chrono::Utc::now().timestamp(), entry.owner_user_id], |row| row.get(0))?)
    }

    /// Prepare a new process attempt while closing admission. Only an exact
    /// revision can replace existing state. Quick attempts always rotate epochs;
    /// Named attempts preserve grants only while origin and owner are unchanged.
    pub fn prepare_public_ingress_entry(
        &self,
        spec: &PublicIngressEntrySpec,
        expected_revision: Option<i64>,
    ) -> anyhow::Result<Option<PublicIngressEntry>> {
        validate_spec(spec)?;
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old = entry_by_id(&tx, &spec.entry_id)?;
        if old.as_ref().map(|entry| entry.runtime_revision) != expected_revision {
            return Ok(None);
        }
        if !owner_enabled(&tx, &spec.owner_user_id)? {
            return Ok(None);
        }
        let (epoch, revision) = if let Some(old) = old {
            anyhow::ensure!(
                old.profile_id == spec.profile_id,
                "public ingress profile identity cannot change"
            );
            if old.server_instance_id == spec.server_instance_id
                && spec.process_generation <= old.process_generation
            {
                return Ok(None);
            }
            let rotate = spec.mode == PublicIngressMode::Quick
                || old.mode != spec.mode
                || old.origin != spec.origin
                || old.owner_user_id != spec.owner_user_id;
            (
                old.auth_epoch
                    .checked_add(i64::from(rotate))
                    .ok_or_else(|| anyhow::anyhow!("public ingress epoch exhausted"))?,
                old.runtime_revision
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("public ingress revision exhausted"))?,
            )
        } else {
            (1, 1)
        };
        tx.execute("INSERT INTO public_ingress_entries (entry_id,profile_id,mode,origin,owner_user_id,auth_epoch,runtime_revision,process_generation,server_instance_id,active,admission_open)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,0)
            ON CONFLICT(entry_id) DO UPDATE SET mode=excluded.mode,origin=excluded.origin,owner_user_id=excluded.owner_user_id,auth_epoch=excluded.auth_epoch,runtime_revision=excluded.runtime_revision,process_generation=excluded.process_generation,server_instance_id=excluded.server_instance_id,active=0,admission_open=0,attempt_closed=0",
            params![spec.entry_id,spec.profile_id,spec.mode.as_str(),spec.origin,spec.owner_user_id,epoch,revision,spec.process_generation,spec.server_instance_id])?;
        let entry = entry_by_id(&tx, &spec.entry_id)?;
        tx.commit()?;
        Ok(entry)
    }

    /// Activate exactly one ingress, or close this attempt. Superseded Quick
    /// entries lose their epoch immediately; stale process callbacks do nothing.
    pub fn set_public_ingress_admission(
        &self,
        fence: &PublicIngressFence,
        open: bool,
    ) -> anyhow::Result<bool> {
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(entry) = entry_by_id(&tx, &fence.entry_id)? else {
            return Ok(false);
        };
        if entry.fence() != *fence
            || (open && (entry.attempt_closed || !owner_enabled(&tx, &entry.owner_user_id)?))
        {
            return Ok(false);
        }
        if open {
            tx.execute("UPDATE public_ingress_entries SET active=0,admission_open=0,attempt_closed=1,auth_epoch=auth_epoch+CASE WHEN mode='quick' THEN 1 ELSE 0 END WHERE active=1 AND entry_id<>?1", [&fence.entry_id])?;
            tx.execute(
                "UPDATE public_ingress_entries SET active=1,admission_open=1 WHERE entry_id=?1",
                [&fence.entry_id],
            )?;
        } else {
            tx.execute("UPDATE public_ingress_entries SET active=0,admission_open=0,attempt_closed=1,auth_epoch=auth_epoch+CASE WHEN mode='quick' AND attempt_closed=0 THEN 1 ELSE 0 END WHERE entry_id=?1", [&fence.entry_id])?;
        }
        tx.commit()?;
        Ok(true)
    }
    pub fn get_public_ingress_entry(
        &self,
        entry_id: &str,
    ) -> anyhow::Result<Option<PublicIngressEntry>> {
        entry_by_id(&self.lock_connection(crate::StoreDomain::OAuth), entry_id)
    }
    pub fn get_public_ingress_entry_for_profile(
        &self,
        profile_id: &str,
    ) -> anyhow::Result<Option<PublicIngressEntry>> {
        let conn = self.lock_connection(crate::StoreDomain::OAuth);
        Ok(conn
            .query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM public_ingress_entries WHERE profile_id=?1"),
                [profile_id],
                entry_row,
            )
            .optional()?)
    }
    pub fn get_active_public_ingress_entry(&self) -> anyhow::Result<Option<PublicIngressEntry>> {
        let conn = self.lock_connection(crate::StoreDomain::OAuth);
        Ok(conn.query_row(&format!("SELECT {ENTRY_COLUMNS} FROM public_ingress_entries WHERE active=1 AND admission_open=1"), [], entry_row).optional()?)
    }
    pub fn bind_oauth_client_to_public_ingress(
        &self,
        client_id: &str,
        fence: &PublicIngressFence,
    ) -> anyhow::Result<bool> {
        let mut conn = self.lock_connection(crate::StoreDomain::OAuth);
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let Some(entry) = active_entry_for_fence(&tx, fence)? else {
            return Ok(false);
        };
        if !client_owner_matches(&tx, client_id, &entry.owner_user_id)? {
            return Ok(false);
        }
        let old = client_entry_id(&tx, client_id)?;
        if old.as_deref().is_some_and(|id| id != entry.entry_id) {
            return Ok(false);
        }
        // Do not convert clients with existing private grants into public clients.
        if old.is_none() {
            let has_grants: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM oauth_authorization_codes WHERE client_id=?1 UNION ALL SELECT 1 FROM oauth_access_tokens WHERE client_id=?1 UNION ALL SELECT 1 FROM oauth_refresh_tokens WHERE client_id=?1)", [client_id], |row| row.get(0))?;
            if has_grants {
                return Ok(false);
            }
            tx.execute(
                "INSERT INTO oauth_public_ingress_clients(client_id,entry_id) VALUES (?1,?2)",
                params![client_id, entry.entry_id],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }
    pub fn oauth_client_public_ingress_entry_id(
        &self,
        client_id: &str,
    ) -> anyhow::Result<Option<String>> {
        client_entry_id(&self.lock_connection(crate::StoreDomain::OAuth), client_id)
    }
    pub fn oauth_client_matches_public_ingress(
        &self,
        client_id: &str,
        fence: &PublicIngressFence,
    ) -> anyhow::Result<bool> {
        let conn = self.lock_connection(crate::StoreDomain::OAuth);
        let Some(entry) = active_entry_for_fence(&conn, fence)? else {
            return Ok(false);
        };
        Ok(
            client_entry_id(&conn, client_id)?.as_deref() == Some(&entry.entry_id)
                && client_owner_matches(&conn, client_id, &entry.owner_user_id)?,
        )
    }
    /// `None` is the private audience and rejects every entry-bound token.
    /// A public audience rejects legacy unbound grants, even with a resource.
    pub fn oauth_access_token_matches_public_ingress(
        &self,
        token_hash: &str,
        expected: Option<&PublicIngressFence>,
    ) -> anyhow::Result<bool> {
        let conn = self.lock_connection(crate::StoreDomain::OAuth);
        let grant: Option<(String,String,Option<String>,Option<String>,i64)> = conn.query_row("SELECT id,client_id,user_id,resource,expires_at FROM oauth_access_tokens WHERE token_hash=?1 AND revoked_at IS NULL", [token_hash], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
        let Some((id, client_id, user_id, resource, expires_at)) = grant else {
            return Ok(false);
        };
        if expires_at <= chrono::Utc::now().timestamp() {
            return Ok(false);
        }
        let binding = grant_binding(&conn, "oauth_public_ingress_access", &id)?;
        match (expected, binding) {
            (None, None) => Ok(client_entry_id(&conn, &client_id)?.is_none()),
            (Some(fence), Some(binding)) => {
                let Some(entry) = active_entry_for_fence(&conn, fence)? else {
                    return Ok(false);
                };
                Ok(entry.entry_id == binding.entry_id
                    && entry.auth_epoch == binding.auth_epoch
                    && binding_is_current(
                        &conn,
                        &binding,
                        &client_id,
                        user_id.as_deref(),
                        resource.as_deref(),
                    )?)
            }
            _ => Ok(false),
        }
    }
}

fn client_owner_matches(conn: &Connection, client_id: &str, owner: &str) -> anyhow::Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM oauth_clients c JOIN users u ON u.id=c.owner_user_id WHERE c.client_id=?1 AND c.owner_user_id=?2 AND c.revoked_at IS NULL AND u.disabled=0)", params![client_id,owner], |row| row.get(0))?)
}
pub(crate) fn client_entry_id(
    conn: &Connection,
    client_id: &str,
) -> anyhow::Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT entry_id FROM oauth_public_ingress_clients WHERE client_id=?1",
            [client_id],
            |row| row.get(0),
        )
        .optional()?)
}
pub(crate) fn require_unbound_client(conn: &Connection, client_id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        client_entry_id(conn, client_id)?.is_none(),
        "public ingress client requires bound OAuth issuance"
    );
    Ok(())
}
#[derive(Debug)]
pub(crate) struct GrantBinding {
    pub entry_id: String,
    pub auth_epoch: i64,
}
pub(crate) fn grant_binding(
    conn: &Connection,
    table: &str,
    id: &str,
) -> anyhow::Result<Option<GrantBinding>> {
    Ok(conn
        .query_row(
            &format!("SELECT entry_id,auth_epoch FROM {table} WHERE grant_id=?1"),
            [id],
            |row| {
                Ok(GrantBinding {
                    entry_id: row.get(0)?,
                    auth_epoch: row.get(1)?,
                })
            },
        )
        .optional()?)
}
pub(crate) fn insert_binding(
    conn: &Connection,
    table: &str,
    id: &str,
    binding: &GrantBinding,
) -> anyhow::Result<()> {
    conn.execute(
        &format!("INSERT INTO {table}(grant_id,entry_id,auth_epoch) VALUES (?1,?2,?3)"),
        params![id, binding.entry_id, binding.auth_epoch],
    )?;
    Ok(())
}
pub(crate) fn binding_is_current(
    conn: &Connection,
    binding: &GrantBinding,
    client_id: &str,
    user_id: Option<&str>,
    resource: Option<&str>,
) -> anyhow::Result<bool> {
    let Some(entry) = entry_by_id(conn, &binding.entry_id)? else {
        return Ok(false);
    };
    if !entry.active
        || !entry.admission_open
        || entry.auth_epoch != binding.auth_epoch
        || user_id != Some(&entry.owner_user_id)
        || client_entry_id(conn, client_id)?.as_deref() != Some(&entry.entry_id)
        || !client_owner_matches(conn, client_id, &entry.owner_user_id)?
    {
        return Ok(false);
    }
    if let Some(resource) = resource {
        let Ok(url) = url::Url::parse(resource) else {
            return Ok(false);
        };
        if url.origin().ascii_serialization() != entry.origin {
            return Ok(false);
        }
    }
    Ok(true)
}
