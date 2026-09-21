use super::communication::digest_json;
use super::Database;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;

pub const JOB_TERMINAL_WAIT_ID_PREFIX: &str = "wc_job_wait_";
pub const JOB_TERMINAL_DELIVERY_ATTEMPT_ID_PREFIX: &str = "wc_job_delivery_";
pub const MAX_JOB_TERMINAL_WAITS_PER_PRINCIPAL: i64 = 64;
pub const MAX_JOB_TERMINAL_WAITS_PER_SOURCE: i64 = 64;
pub const MAX_JOB_TERMINAL_WAITS_GLOBAL: i64 = 4096;
pub const MAX_JOB_TERMINAL_WAIT_IDEMPOTENCY_KEY_CHARS: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTerminalWaitStoreError {
    pub code: &'static str,
    pub message: String,
}
impl JobTerminalWaitStoreError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for JobTerminalWaitStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for JobTerminalWaitStoreError {}
fn store_error(error: rusqlite::Error) -> JobTerminalWaitStoreError {
    JobTerminalWaitStoreError::new("job_terminal_wait_store_error", error.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobTerminalWaitPrincipal {
    pub kind: String,
    pub digest: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobTerminalSourceIdentity {
    pub job_id: String,
    pub client_id: String,
    pub auth_kind: String,
    pub auth_value: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobTerminalFact {
    pub source: JobTerminalSourceIdentity,
    pub status: String,
    pub outcome: String,
    pub terminal_observed_at: i64,
    pub expires_at: i64,
}
#[derive(Debug, Clone)]
pub struct NewJobTerminalWait {
    pub source: JobTerminalSourceIdentity,
    pub idempotency_key: String,
    pub expires_at: i64,
    pub already_terminal: Option<JobTerminalFact>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobTerminalWaitState {
    Waiting,
    Triggered,
}
impl JobTerminalWaitState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Triggered => "triggered",
        }
    }
    fn parse(value: &str) -> Result<Self, JobTerminalWaitStoreError> {
        match value {
            "waiting" => Ok(Self::Waiting),
            "triggered" => Ok(Self::Triggered),
            _ => Err(invariant("unsupported Job terminal wait state")),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobTerminalDeliveryState {
    NotReady,
    Pending,
    Prepared,
    Delivered,
    DeliveryUnknown,
}
impl JobTerminalDeliveryState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotReady => "not_ready",
            Self::Pending => "pending",
            Self::Prepared => "prepared",
            Self::Delivered => "delivered",
            Self::DeliveryUnknown => "delivery_unknown",
        }
    }
    fn parse(value: &str) -> Result<Self, JobTerminalWaitStoreError> {
        match value {
            "not_ready" => Ok(Self::NotReady),
            "pending" => Ok(Self::Pending),
            "prepared" => Ok(Self::Prepared),
            "delivered" => Ok(Self::Delivered),
            "delivery_unknown" => Ok(Self::DeliveryUnknown),
            _ => Err(invariant("unsupported Job terminal delivery state")),
        }
    }
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct JobTerminalWaitRecord {
    pub wait_id: String,
    pub source: JobTerminalSourceIdentity,
    pub state: JobTerminalWaitState,
    pub delivery_state: JobTerminalDeliveryState,
    pub terminal_status: Option<String>,
    pub terminal_outcome: Option<String>,
    pub terminal_observed_at: Option<i64>,
    pub delivery_attempt_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub triggered_at: Option<i64>,
    pub expires_at: i64,
}
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct JobTerminalWaitMutation {
    pub wait: JobTerminalWaitRecord,
    pub replayed: bool,
    pub state_changed: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTerminalWaitMatch {
    pub matched_count: usize,
    pub delivery_candidates: Vec<(JobTerminalWaitPrincipal, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobTerminalDeliveryPrepared {
    pub wait: JobTerminalWaitRecord,
    pub attempt_id: String,
}

impl Database {
    pub(super) fn ensure_job_terminal_wait_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch("\
CREATE TABLE IF NOT EXISTS wc_job_terminal_waits (\
 wait_id TEXT PRIMARY KEY, owner_kind TEXT NOT NULL, owner_digest TEXT NOT NULL,\
 idempotency_key TEXT NOT NULL, request_digest TEXT NOT NULL,\
 job_id TEXT NOT NULL, job_client_id TEXT NOT NULL,\
 job_auth_kind TEXT NOT NULL CHECK(job_auth_kind IN ('shared_key','project_grant','open_anonymous','managed_owner','managed_unowned')),\
 job_auth_value TEXT, state TEXT NOT NULL CHECK(state IN ('waiting','triggered')),\
 delivery_state TEXT NOT NULL CHECK(delivery_state IN ('not_ready','pending','prepared','delivered','delivery_unknown')),\
 terminal_status TEXT, terminal_outcome TEXT, terminal_observed_at INTEGER, delivery_attempt_id TEXT,\
 created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, triggered_at INTEGER, expires_at INTEGER NOT NULL,\
 UNIQUE(owner_kind, owner_digest, idempotency_key),\
 CHECK((job_auth_kind IN ('shared_key','project_grant','managed_owner') AND job_auth_value IS NOT NULL) OR\
       (job_auth_kind IN ('open_anonymous','managed_unowned') AND job_auth_value IS NULL)),\
 CHECK((state='waiting' AND delivery_state='not_ready' AND terminal_status IS NULL AND terminal_outcome IS NULL AND terminal_observed_at IS NULL AND triggered_at IS NULL) OR\
       (state='triggered' AND delivery_state<>'not_ready' AND terminal_status IS NOT NULL AND terminal_outcome IS NOT NULL AND terminal_observed_at IS NOT NULL AND triggered_at IS NOT NULL)),\
 CHECK((delivery_state='prepared' AND delivery_attempt_id IS NOT NULL) OR delivery_state<>'prepared')\
);\
CREATE INDEX IF NOT EXISTS idx_wc_job_terminal_waits_source ON wc_job_terminal_waits(job_id,job_client_id,job_auth_kind,job_auth_value,state);\
CREATE INDEX IF NOT EXISTS idx_wc_job_terminal_waits_owner ON wc_job_terminal_waits(owner_kind,owner_digest,state,created_at);\
CREATE INDEX IF NOT EXISTS idx_wc_job_terminal_waits_expiry ON wc_job_terminal_waits(expires_at);" )?;
        Ok(())
    }

    pub fn create_job_terminal_wait(
        &self,
        principal: &JobTerminalWaitPrincipal,
        input: NewJobTerminalWait,
        now: i64,
    ) -> Result<JobTerminalWaitMutation, JobTerminalWaitStoreError> {
        validate_principal(principal)?;
        validate_source(&input.source)?;
        let key = validate_idempotency_key(&input.idempotency_key)?;
        if input.expires_at <= now {
            return Err(err(
                "job_terminal_wait_expired",
                "Job terminal wait deadline is already expired",
            ));
        }
        if let Some(fact) = &input.already_terminal {
            validate_fact(fact, now)?;
            if fact.source != input.source {
                return Err(err(
                    "job_terminal_wait_source_mismatch",
                    "terminal fact does not identify the registered Job",
                ));
            }
        }
        let request_digest = digest_json(
            "webcodex.job-terminal-wait.request.v1",
            &json!({"source": input.source}),
        )
        .map_err(|_| {
            err(
                "job_terminal_wait_request_invalid",
                "Job terminal wait request could not be canonicalized",
            )
        })?;
        let mut conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        prune_expired(&tx, now).map_err(store_error)?;
        let replay: Option<(String,String)> = tx.query_row(
            "SELECT wait_id,request_digest FROM wc_job_terminal_waits WHERE owner_kind=?1 AND owner_digest=?2 AND idempotency_key=?3",
            params![principal.kind,principal.digest,key], |r| Ok((r.get(0)?,r.get(1)?))).optional().map_err(store_error)?;
        if let Some((wait_id, digest)) = replay {
            if digest != request_digest {
                return Err(err(
                    "job_terminal_wait_idempotency_conflict",
                    "idempotency_key was already used for a different Job terminal wait",
                ));
            }
            let wait = load_owned_wait(&tx, principal, &wait_id, now)?;
            tx.commit().map_err(store_error)?;
            return Ok(JobTerminalWaitMutation {
                wait,
                replayed: true,
                state_changed: false,
            });
        }
        let principal_count:i64 = tx.query_row("SELECT COUNT(*) FROM wc_job_terminal_waits WHERE owner_kind=?1 AND owner_digest=?2", params![principal.kind,principal.digest], |r| r.get(0)).map_err(store_error)?;
        let source_count:i64 = tx.query_row("SELECT COUNT(*) FROM wc_job_terminal_waits WHERE job_id=?1 AND job_client_id=?2 AND job_auth_kind=?3 AND job_auth_value IS ?4", params![input.source.job_id,input.source.client_id,input.source.auth_kind,input.source.auth_value], |r| r.get(0)).map_err(store_error)?;
        let global_count: i64 = tx
            .query_row("SELECT COUNT(*) FROM wc_job_terminal_waits", [], |r| {
                r.get(0)
            })
            .map_err(store_error)?;
        if principal_count >= MAX_JOB_TERMINAL_WAITS_PER_PRINCIPAL
            || source_count >= MAX_JOB_TERMINAL_WAITS_PER_SOURCE
            || global_count >= MAX_JOB_TERMINAL_WAITS_GLOBAL
        {
            return Err(err(
                "job_terminal_wait_capacity_reached",
                "Job terminal wait capacity is exhausted",
            ));
        }
        let wait_id = allocate_id(&tx, JOB_TERMINAL_WAIT_ID_PREFIX, "wait_id")?;
        let (state, delivery, status, outcome, observed, triggered, expires) =
            if let Some(f) = input.already_terminal {
                (
                    "triggered",
                    "pending",
                    Some(f.status),
                    Some(f.outcome),
                    Some(f.terminal_observed_at),
                    Some(f.terminal_observed_at),
                    f.expires_at,
                )
            } else {
                (
                    "waiting",
                    "not_ready",
                    None,
                    None,
                    None,
                    None,
                    input.expires_at,
                )
            };
        tx.execute("INSERT INTO wc_job_terminal_waits (wait_id,owner_kind,owner_digest,idempotency_key,request_digest,job_id,job_client_id,job_auth_kind,job_auth_value,state,delivery_state,terminal_status,terminal_outcome,terminal_observed_at,delivery_attempt_id,created_at,updated_at,triggered_at,expires_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL,?15,?15,?16,?17)",
            params![wait_id,principal.kind,principal.digest,key,request_digest,input.source.job_id,input.source.client_id,input.source.auth_kind,input.source.auth_value,state,delivery,status,outcome,observed,now,triggered,expires]).map_err(store_error)?;
        let wait = load_owned_wait(&tx, principal, &wait_id, now)?;
        tx.commit().map_err(store_error)?;
        Ok(JobTerminalWaitMutation {
            wait,
            replayed: false,
            state_changed: true,
        })
    }
}

impl Database {
    pub fn read_job_terminal_wait(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        now: i64,
    ) -> Result<JobTerminalWaitRecord, JobTerminalWaitStoreError> {
        validate_principal(principal)?;
        let conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        load_owned_wait(&conn, principal, wait_id, now)
    }

    pub fn match_job_terminal_fact(
        &self,
        fact: &JobTerminalFact,
        now: i64,
    ) -> Result<JobTerminalWaitMatch, JobTerminalWaitStoreError> {
        validate_fact(fact, now)?;
        let mut conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        let needs_write: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM wc_job_terminal_waits WHERE expires_at<=?1 OR (job_id=?2 AND job_client_id=?3 AND job_auth_kind=?4 AND job_auth_value IS ?5))",
                params![
                    now,
                    fact.source.job_id,
                    fact.source.client_id,
                    fact.source.auth_kind,
                    fact.source.auth_value
                ],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if !needs_write {
            return Ok(JobTerminalWaitMatch {
                matched_count: 0,
                delivery_candidates: Vec::new(),
            });
        }
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        prune_expired(&tx, now).map_err(store_error)?;
        let mut stmt=tx.prepare("SELECT wait_id,owner_kind,owner_digest FROM wc_job_terminal_waits WHERE job_id=?1 AND job_client_id=?2 AND job_auth_kind=?3 AND job_auth_value IS ?4 AND state='waiting' ORDER BY wait_id").map_err(store_error)?;
        let rows = stmt
            .query_map(
                params![
                    fact.source.job_id,
                    fact.source.client_id,
                    fact.source.auth_kind,
                    fact.source.auth_value
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(store_error)?;
        let mut newly_matched = Vec::new();
        for row in rows {
            newly_matched.push(row.map_err(store_error)?);
        }
        drop(stmt);
        for (wait_id, _, _) in &newly_matched {
            tx.execute("UPDATE wc_job_terminal_waits SET state='triggered',delivery_state='pending',terminal_status=?2,terminal_outcome=?3,terminal_observed_at=?4,triggered_at=?4,expires_at=?5,updated_at=MAX(updated_at,?4) WHERE wait_id=?1 AND state='waiting'",params![wait_id,fact.status,fact.outcome,fact.terminal_observed_at,fact.expires_at]).map_err(store_error)?;
        }
        let matched_count = newly_matched.len();

        // A terminal-event sink failure may happen after the durable wait has
        // already moved from waiting to triggered/pending but before Host delivery
        // was prepared. Re-offer exactly those safe pending deliveries when the
        // same canonical terminal fact is retried. Prepared/finished deliveries
        // stay fenced and are never silently redispatched.
        let mut stmt=tx.prepare("SELECT wait_id,owner_kind,owner_digest FROM wc_job_terminal_waits WHERE job_id=?1 AND job_client_id=?2 AND job_auth_kind=?3 AND job_auth_value IS ?4 AND state='triggered' AND delivery_state='pending' AND terminal_status=?5 AND terminal_outcome=?6 AND terminal_observed_at=?7 ORDER BY wait_id").map_err(store_error)?;
        let rows = stmt
            .query_map(
                params![
                    fact.source.job_id,
                    fact.source.client_id,
                    fact.source.auth_kind,
                    fact.source.auth_value,
                    fact.status,
                    fact.outcome,
                    fact.terminal_observed_at
                ],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                    ))
                },
            )
            .map_err(store_error)?;
        let mut delivery_candidates = Vec::new();
        for row in rows {
            let (wait_id, kind, digest) = row.map_err(store_error)?;
            delivery_candidates.push((JobTerminalWaitPrincipal { kind, digest }, wait_id));
        }
        drop(stmt);
        tx.commit().map_err(store_error)?;
        Ok(JobTerminalWaitMatch {
            matched_count,
            delivery_candidates,
        })
    }

    pub fn prepare_job_terminal_delivery(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        now: i64,
    ) -> Result<Option<JobTerminalDeliveryPrepared>, JobTerminalWaitStoreError> {
        validate_principal(principal)?;
        let mut conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        prune_expired(&tx, now).map_err(store_error)?;
        let wait = load_owned_wait(&tx, principal, wait_id, now)?;
        if wait.state != JobTerminalWaitState::Triggered
            || wait.delivery_state != JobTerminalDeliveryState::Pending
        {
            tx.commit().map_err(store_error)?;
            return Ok(None);
        }
        let attempt_id = allocate_id(
            &tx,
            JOB_TERMINAL_DELIVERY_ATTEMPT_ID_PREFIX,
            "delivery_attempt_id",
        )?;
        let changed=tx.execute("UPDATE wc_job_terminal_waits SET delivery_state='prepared',delivery_attempt_id=?2,updated_at=?3 WHERE wait_id=?1 AND owner_kind=?4 AND owner_digest=?5 AND delivery_state='pending'",params![wait_id,attempt_id,now,principal.kind,principal.digest]).map_err(store_error)?;
        if changed != 1 {
            tx.commit().map_err(store_error)?;
            return Ok(None);
        }
        let wait = load_owned_wait(&tx, principal, wait_id, now)?;
        tx.commit().map_err(store_error)?;
        Ok(Some(JobTerminalDeliveryPrepared { wait, attempt_id }))
    }

    pub fn finish_job_terminal_delivery(
        &self,
        principal: &JobTerminalWaitPrincipal,
        wait_id: &str,
        attempt_id: &str,
        delivered: bool,
        now: i64,
    ) -> Result<JobTerminalWaitRecord, JobTerminalWaitStoreError> {
        validate_principal(principal)?;
        let mut conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        prune_expired(&tx, now).map_err(store_error)?;
        let state = if delivered {
            "delivered"
        } else {
            "delivery_unknown"
        };
        let changed=tx.execute("UPDATE wc_job_terminal_waits SET delivery_state=?4,updated_at=?5 WHERE wait_id=?1 AND owner_kind=?2 AND owner_digest=?3 AND delivery_state='prepared' AND delivery_attempt_id=?6",params![wait_id,principal.kind,principal.digest,state,now,attempt_id]).map_err(store_error)?;
        if changed != 1 {
            return Err(err(
                "job_terminal_delivery_fence_mismatch",
                "Job terminal delivery attempt is stale or already finished",
            ));
        }
        let wait = load_owned_wait(&tx, principal, wait_id, now)?;
        tx.commit().map_err(store_error)?;
        Ok(wait)
    }

    pub fn prune_job_terminal_waits(&self, now: i64) -> Result<usize, JobTerminalWaitStoreError> {
        let conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        prune_expired(&conn, now).map_err(store_error)
    }

    /// A process-local Host dispatch fence cannot survive Server restart. Any
    /// prepared delivery therefore becomes conservatively delivery_unknown;
    /// pending rows remain pending for a future eligible carrier.
    pub fn recover_job_terminal_deliveries_after_restart(
        &self,
        now: i64,
    ) -> Result<usize, JobTerminalWaitStoreError> {
        let mut conn = self.lock_connection(crate::StoreDomain::JobTerminalWait);
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        prune_expired(&tx, now).map_err(store_error)?;
        let changed=tx.execute("UPDATE wc_job_terminal_waits SET delivery_state='delivery_unknown',updated_at=?1 WHERE delivery_state='prepared' AND expires_at>?1",[now]).map_err(store_error)?;
        tx.commit().map_err(store_error)?;
        Ok(changed)
    }
}

fn load_owned_wait(
    conn: &Connection,
    principal: &JobTerminalWaitPrincipal,
    wait_id: &str,
    now: i64,
) -> Result<JobTerminalWaitRecord, JobTerminalWaitStoreError> {
    let row=conn.query_row("SELECT job_id,job_client_id,job_auth_kind,job_auth_value,state,delivery_state,terminal_status,terminal_outcome,terminal_observed_at,delivery_attempt_id,created_at,updated_at,triggered_at,expires_at FROM wc_job_terminal_waits WHERE wait_id=?1 AND owner_kind=?2 AND owner_digest=?3 AND expires_at>?4",params![wait_id,principal.kind,principal.digest,now],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,String>(4)?,r.get::<_,String>(5)?,r.get::<_,Option<String>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<i64>>(8)?,r.get::<_,Option<String>>(9)?,r.get::<_,i64>(10)?,r.get::<_,i64>(11)?,r.get::<_,Option<i64>>(12)?,r.get::<_,i64>(13)?))).optional().map_err(store_error)?;
    let Some((
        job_id,
        client_id,
        auth_kind,
        auth_value,
        state_text,
        delivery_text,
        terminal_status,
        terminal_outcome,
        terminal_observed_at,
        delivery_attempt_id,
        created_at,
        updated_at,
        triggered_at,
        expires_at,
    )) = row
    else {
        return Err(err(
            "job_terminal_wait_not_found",
            "Job terminal wait does not exist",
        ));
    };
    let source = JobTerminalSourceIdentity {
        job_id,
        client_id,
        auth_kind,
        auth_value,
    };
    validate_source(&source)?;
    let state = JobTerminalWaitState::parse(&state_text)?;
    let delivery_state = JobTerminalDeliveryState::parse(&delivery_text)?;
    if state == JobTerminalWaitState::Triggered {
        validate_terminal_fields(
            terminal_status.as_deref(),
            terminal_outcome.as_deref(),
            terminal_observed_at,
        )?;
    } else if terminal_status.is_some()
        || terminal_outcome.is_some()
        || terminal_observed_at.is_some()
        || triggered_at.is_some()
    {
        return Err(invariant(
            "waiting Job terminal wait contains terminal data",
        ));
    }
    Ok(JobTerminalWaitRecord {
        wait_id: wait_id.to_string(),
        source,
        state,
        delivery_state,
        terminal_status,
        terminal_outcome,
        terminal_observed_at,
        delivery_attempt_id,
        created_at,
        updated_at,
        triggered_at,
        expires_at,
    })
}

fn validate_principal(p: &JobTerminalWaitPrincipal) -> Result<(), JobTerminalWaitStoreError> {
    if p.kind.is_empty()
        || p.kind.len() > 64
        || p.digest.len() != 64
        || !p.digest.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(err(
            "invalid_job_terminal_wait_principal",
            "invalid Job terminal wait principal",
        ));
    }
    Ok(())
}
fn validate_source(s: &JobTerminalSourceIdentity) -> Result<(), JobTerminalWaitStoreError> {
    if s.job_id.is_empty()
        || s.job_id.len() > 128
        || s.client_id.is_empty()
        || s.client_id.len() > 128
    {
        return Err(err(
            "invalid_job_terminal_wait_source",
            "invalid Job terminal wait source identity",
        ));
    }
    let value_ok = s
        .auth_value
        .as_ref()
        .map(|v| !v.is_empty() && v.len() <= 256)
        .unwrap_or(true);
    let shape = match s.auth_kind.as_str() {
        "shared_key" => s
            .auth_value
            .as_ref()
            .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())),
        "project_grant" | "managed_owner" => s.auth_value.is_some() && value_ok,
        "open_anonymous" | "managed_unowned" => s.auth_value.is_none(),
        _ => false,
    };
    if !shape {
        return Err(err(
            "invalid_job_terminal_wait_source",
            "invalid Job terminal wait authority partition",
        ));
    }
    Ok(())
}
fn validate_terminal_fields(
    status: Option<&str>,
    outcome: Option<&str>,
    observed: Option<i64>,
) -> Result<(), JobTerminalWaitStoreError> {
    let a = status.is_some_and(|v| {
        matches!(
            v,
            "completed" | "failed" | "timed_out" | "stopped" | "cancelled" | "lost"
        )
    });
    let b =
        outcome.is_some_and(|v| matches!(v, "succeeded" | "failed" | "timed_out" | "cancelled"));
    if !a || !b || observed.is_none_or(|v| v <= 0) {
        return Err(invariant("invalid stored Job terminal fact"));
    }
    Ok(())
}
fn validate_fact(f: &JobTerminalFact, now: i64) -> Result<(), JobTerminalWaitStoreError> {
    validate_source(&f.source)?;
    validate_terminal_fields(
        Some(&f.status),
        Some(&f.outcome),
        Some(f.terminal_observed_at),
    )?;
    if f.terminal_observed_at > now || f.expires_at <= now || f.expires_at <= f.terminal_observed_at
    {
        return Err(err(
            "invalid_job_terminal_fact",
            "invalid Job terminal fact deadline",
        ));
    }
    Ok(())
}
fn validate_idempotency_key(v: &str) -> Result<String, JobTerminalWaitStoreError> {
    let v = v.trim();
    if v.is_empty() || v.chars().count() > MAX_JOB_TERMINAL_WAIT_IDEMPOTENCY_KEY_CHARS {
        return Err(err(
            "invalid_job_terminal_wait_idempotency_key",
            "invalid Job terminal wait idempotency_key",
        ));
    }
    Ok(v.to_string())
}
fn allocate_id(
    conn: &Connection,
    prefix: &str,
    column: &str,
) -> Result<String, JobTerminalWaitStoreError> {
    for _ in 0..16 {
        let id = format!("{prefix}{}", webcodex_core::compact::random_suffix::<12>());
        let sql = format!("SELECT EXISTS(SELECT 1 FROM wc_job_terminal_waits WHERE {column}=?1)");
        let occupied: bool = conn
            .query_row(&sql, [&id], |r| r.get(0))
            .map_err(store_error)?;
        if !occupied {
            return Ok(id);
        }
    }
    Err(err(
        "job_terminal_wait_identity_allocation_exhausted",
        "unable to allocate Job terminal wait identity",
    ))
}
fn prune_expired(conn: &Connection, now: i64) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM wc_job_terminal_waits WHERE expires_at<=?1",
        [now],
    )
}
fn err(code: &'static str, message: &'static str) -> JobTerminalWaitStoreError {
    JobTerminalWaitStoreError::new(code, message)
}
fn invariant(message: &'static str) -> JobTerminalWaitStoreError {
    err("job_terminal_wait_storage_invariant", message)
}
