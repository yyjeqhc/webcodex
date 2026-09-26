//! Process-local admission fence used while a managed runtime is upgraded.
//! The fence shares the Runner registry mutex with every request enqueue.

use crate::jobs::runner_is_connected_locked;
use crate::{RunnerAccess, RunnerRegistry};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAINTENANCE_LEASE_SECS: u64 = 90;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MaintenanceScope {
    AllRuntimes,
    Runner(String),
}

impl MaintenanceScope {
    pub(crate) fn covers(&self, client_id: &str) -> bool {
        matches!(self, Self::AllRuntimes) || matches!(self, Self::Runner(id) if id == client_id)
    }
}

pub(crate) struct MaintenanceLeaseState {
    pub(crate) scope: MaintenanceScope,
    owner: String,
    nonce: String,
    token: String,
    pub(crate) expires: Instant,
    runner_instance_id: Option<String>,
    expires_at_unix_ms: u64,
}

/// Persisted under the Server data directory. Neither this type nor its store
/// may print the owner, nonce, or bearer token in diagnostics.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct SavedMaintenanceLease {
    pub schema_version: u16,
    pub scope: MaintenanceScope,
    pub owner: String,
    pub nonce: String,
    pub token: String,
    pub runner_instance_id: Option<String>,
    pub expires_at_unix_ms: u64,
}

impl std::fmt::Debug for SavedMaintenanceLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SavedMaintenanceLease")
            .field("scope", &self.scope)
            .finish_non_exhaustive()
    }
}

pub trait MaintenanceStore: std::fmt::Debug + Send + Sync {
    fn load(&self) -> Result<Option<SavedMaintenanceLease>, String>;
    fn save(&self, lease: &SavedMaintenanceLease) -> Result<(), String>;
    fn clear(&self) -> Result<(), String>;
}

fn unix_millis() -> Result<u64, &'static str> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is invalid")?
        .as_millis()
        .min(u64::MAX as u128) as u64)
}

impl MaintenanceLeaseState {
    fn saved(&self) -> SavedMaintenanceLease {
        SavedMaintenanceLease {
            schema_version: 1,
            scope: self.scope.clone(),
            owner: self.owner.clone(),
            nonce: self.nonce.clone(),
            token: self.token.clone(),
            runner_instance_id: self.runner_instance_id.clone(),
            expires_at_unix_ms: self.expires_at_unix_ms,
        }
    }
}

impl std::fmt::Debug for MaintenanceLeaseState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaintenanceLeaseState")
            .field("scope", &self.scope)
            .field("expires", &self.expires)
            .finish_non_exhaustive()
    }
}

/// This value contains a bearer capability. Its Debug implementation never prints it.
pub struct MaintenanceGrant {
    token: String,
    pub server_epoch: String,
    pub ttl_secs: u64,
}

impl MaintenanceGrant {
    pub fn token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for MaintenanceGrant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MaintenanceGrant")
            .field("server_epoch", &self.server_epoch)
            .field("ttl_secs", &self.ttl_secs)
            .finish_non_exhaustive()
    }
}

/// Counts an admitted local runtime call until every effect it can start has finished.
pub struct RuntimeCallPermit(Arc<AtomicUsize>);

impl Drop for RuntimeCallPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

impl RunnerRegistry {
    /// Called exactly once before the Server accepts any HTTP, QUIC, or Runner
    /// dispatch. A corrupt/unreadable recovery file aborts startup.
    pub async fn attach_maintenance_store(
        &mut self,
        store: Arc<dyn MaintenanceStore>,
    ) -> Result<(), String> {
        if self.maintenance_store.is_some() {
            return Err("maintenance store already attached".into());
        }
        let saved = store.load()?;
        let mut inner = self.inner.lock().await;
        if inner.maintenance.is_some() {
            return Err("maintenance already active before store attachment".into());
        }
        if let Some(saved) = saved {
            if saved.schema_version != 1
                || saved.owner.is_empty()
                || saved.owner.len() > 256
                || saved.nonce.len() != 64
                || !saved.nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
                || saved.token.len() != 64
                || !saved.token.bytes().all(|byte| byte.is_ascii_hexdigit())
                || matches!(&saved.scope, MaintenanceScope::Runner(id) if id.is_empty() || id.len() > 160)
            {
                return Err("invalid persisted maintenance lease".into());
            }
            let remaining_ms = saved
                .expires_at_unix_ms
                .saturating_sub(unix_millis().map_err(str::to_string)?);
            inner.maintenance = Some(MaintenanceLeaseState {
                scope: saved.scope,
                owner: saved.owner,
                nonce: saved.nonce,
                token: saved.token,
                expires: Instant::now()
                    + Duration::from_millis(remaining_ms.min(MAINTENANCE_LEASE_SECS * 1000)),
                runner_instance_id: saved.runner_instance_id,
                expires_at_unix_ms: saved.expires_at_unix_ms,
            });
        }
        self.maintenance_store = Some(store);
        Ok(())
    }

    fn persist_maintenance(&self, lease: &MaintenanceLeaseState) -> Result<(), &'static str> {
        if let Some(store) = &self.maintenance_store {
            store
                .save(&lease.saved())
                .map_err(|_| "maintenance persistence failed")?;
        }
        Ok(())
    }

    fn clear_maintenance(&self) -> Result<(), &'static str> {
        if let Some(store) = &self.maintenance_store {
            store
                .clear()
                .map_err(|_| "maintenance persistence failed")?;
        }
        Ok(())
    }
    pub fn observation_epoch_for_maintenance(&self) -> &str {
        &self.observation_epoch
    }
    pub async fn admit_runtime_call(&self) -> Result<RuntimeCallPermit, &'static str> {
        let inner = self.inner.lock().await;
        if inner
            .maintenance
            .as_ref()
            .is_some_and(|lease| matches!(lease.scope, MaintenanceScope::AllRuntimes))
        {
            return Err("runtime admission is paused for maintenance");
        }
        inner.runtime_calls.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCallPermit(inner.runtime_calls.clone()))
    }

    /// Begin is atomic with Runner enqueue and local runtime-tool admission.
    /// A stale lease keeps admission closed; it cannot be stolen by a second upgrader.
    pub async fn begin_maintenance(
        &self,
        scope: MaintenanceScope,
        owner: &str,
        nonce: &str,
        access: &RunnerAccess,
    ) -> Result<MaintenanceGrant, &'static str> {
        if owner.is_empty() || owner.len() > 256 {
            return Err("invalid maintenance owner");
        }
        if nonce.len() != 64 || !nonce.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("invalid maintenance nonce");
        }
        if let MaintenanceScope::Runner(id) = &scope {
            if id.is_empty() || id.len() > 160 {
                return Err("invalid runner client_id");
            }
        }
        let mut inner = self.inner.lock().await;
        if let Some(existing) = inner.maintenance.as_ref() {
            if existing.owner == owner
                && existing.nonce == nonce
                && existing.scope == scope
                && Instant::now() < existing.expires
            {
                let mut refreshed = MaintenanceLeaseState {
                    scope: existing.scope.clone(),
                    owner: existing.owner.clone(),
                    nonce: existing.nonce.clone(),
                    token: existing.token.clone(),
                    expires: Instant::now() + Duration::from_secs(MAINTENANCE_LEASE_SECS),
                    expires_at_unix_ms: unix_millis()?
                        .saturating_add(MAINTENANCE_LEASE_SECS * 1000),
                    runner_instance_id: existing.runner_instance_id.clone(),
                };
                self.persist_maintenance(&refreshed)?;
                std::mem::swap(inner.maintenance.as_mut().unwrap(), &mut refreshed);
                return Ok(MaintenanceGrant {
                    token: inner.maintenance.as_ref().unwrap().token.clone(),
                    server_epoch: self.observation_epoch.to_string(),
                    ttl_secs: MAINTENANCE_LEASE_SECS,
                });
            }
            return Err(
                "maintenance already in progress; the owner must release or recover the lease",
            );
        }
        let runner_instance_id = if let MaintenanceScope::Runner(id) = &scope {
            if !runner_is_connected_locked(&inner, id) {
                return Err("target runner is not online");
            }
            let runner = inner.runners.get(id).ok_or("target runner is unknown")?;
            crate::access_control::assert_runner_access(Some(access), runner)
                .map_err(|_| "target runner is not authorized")?;
            Some(runner.runner_instance_id.clone())
        } else {
            None
        };
        if matches!(scope, MaintenanceScope::AllRuntimes)
            && inner.runtime_calls.load(Ordering::SeqCst) != 0
        {
            return Err("runtime calls are still active");
        }
        if inner
            .pending_by_id
            .values()
            .any(|request| scope.covers(&request.request.client_id))
        {
            return Err("runner requests are still active");
        }
        if inner
            .jobs_by_id
            .values()
            .any(|job| scope.covers(&job.client_id) && !job.lifecycle.is_terminal())
        {
            return Err("runner jobs are still active");
        }
        let token = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let lease = MaintenanceLeaseState {
            scope,
            owner: owner.to_string(),
            nonce: nonce.to_string(),
            token: token.clone(),
            expires: Instant::now() + Duration::from_secs(MAINTENANCE_LEASE_SECS),
            expires_at_unix_ms: unix_millis()?.saturating_add(MAINTENANCE_LEASE_SECS * 1000),
            runner_instance_id,
        };
        self.persist_maintenance(&lease)?;
        inner.maintenance = Some(lease);
        Ok(MaintenanceGrant {
            token,
            server_epoch: self.observation_epoch.to_string(),
            ttl_secs: MAINTENANCE_LEASE_SECS,
        })
    }

    pub async fn renew_maintenance(&self, owner: &str, token: &str) -> Result<u64, &'static str> {
        let mut inner = self.inner.lock().await;
        let lease = inner
            .maintenance
            .as_ref()
            .ok_or("maintenance lease is absent")?;
        if lease.owner != owner || lease.token != token {
            return Err("maintenance lease owner mismatch");
        }
        if Instant::now() >= lease.expires {
            return Err("maintenance lease expired; admission remains paused until owner cleanup");
        }
        let mut refreshed = MaintenanceLeaseState {
            scope: lease.scope.clone(),
            owner: lease.owner.clone(),
            nonce: lease.nonce.clone(),
            token: lease.token.clone(),
            expires: Instant::now() + Duration::from_secs(MAINTENANCE_LEASE_SECS),
            expires_at_unix_ms: unix_millis()?.saturating_add(MAINTENANCE_LEASE_SECS * 1000),
            runner_instance_id: lease.runner_instance_id.clone(),
        };
        self.persist_maintenance(&refreshed)?;
        std::mem::swap(inner.maintenance.as_mut().unwrap(), &mut refreshed);
        Ok(MAINTENANCE_LEASE_SECS)
    }

    pub async fn end_maintenance(&self, owner: &str, token: &str) -> Result<(), &'static str> {
        let mut inner = self.inner.lock().await;
        let lease = inner
            .maintenance
            .as_ref()
            .ok_or("maintenance lease is absent")?;
        if lease.owner != owner || lease.token != token {
            return Err("maintenance lease owner mismatch");
        }
        self.clear_maintenance()?;
        inner.maintenance = None;
        Ok(())
    }

    /// Cancel an uncertain begin using the same authenticated owner and the
    /// client-generated nonce. No bearer token is returned to a different owner.
    pub async fn end_pending_maintenance(
        &self,
        owner: &str,
        nonce: &str,
    ) -> Result<(), &'static str> {
        let mut inner = self.inner.lock().await;
        match inner.maintenance.as_ref() {
            None => Ok(()),
            Some(lease) if lease.owner == owner && lease.nonce == nonce => {
                self.clear_maintenance()?;
                inner.maintenance = None;
                Ok(())
            }
            // A different owner holds the only lease. This nonce never
            // installed a fence, so its caller may discard an uncertain
            // pending begin without affecting the other owner.
            Some(_) => Ok(()),
        }
    }

    /// Recovery is limited to an expired Runner lease after a *new* process has
    /// registered and no work is outstanding. It never grants a second owner
    /// while the original Runner is stopped or merely stale.
    pub async fn recover_expired_runner_maintenance(&self) -> Result<(), &'static str> {
        let mut inner = self.inner.lock().await;
        let lease = inner
            .maintenance
            .as_ref()
            .ok_or("maintenance lease is absent")?;
        if Instant::now() < lease.expires {
            return Err("maintenance lease has not expired");
        }
        let MaintenanceScope::Runner(id) = &lease.scope else {
            return Err("all-runtime lease requires Server restart or original owner cleanup");
        };
        let id = id.clone();
        let old_instance = lease.runner_instance_id.clone();
        if !runner_is_connected_locked(&inner, &id) {
            return Err("target runner is not online");
        }
        if inner
            .runners
            .get(&id)
            .map(|runner| runner.runner_instance_id.clone())
            == old_instance
        {
            return Err("target runner has not registered a replacement process");
        }
        if inner
            .pending_by_id
            .values()
            .any(|request| request.request.client_id == id)
            || inner
                .jobs_by_id
                .values()
                .any(|job| job.client_id == id && !job.lifecycle.is_terminal())
        {
            return Err("runner work is still active");
        }
        self.clear_maintenance()?;
        inner.maintenance = None;
        Ok(())
    }
}
