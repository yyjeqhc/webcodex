use super::agent_attention::{require_agent_attention_event_for_wake, AgentAttentionSource};
use super::agent_task::{
    replace_agent_task_attempt_controller_in_transaction, AGENT_TASK_ENDPOINT_DISPATCH_GRACE_MS,
    AGENT_TASK_ENDPOINT_TAKEOVER_LEASE_MS,
};
use super::agent_wait::{
    require_agent_wait_for_wake, resume_agent_wait_for_wake_in_transaction,
    verify_agent_wait_resumed_for_consumed_wake, AgentWaitMode,
};
use super::communication::lookup_idempotent_resource;
use super::communication::{
    allocate_identity, digest_text, load_agent, new_proof, now_unix_ms,
    read_conversation_in_connection, record_idempotent_resource, require_current_endpoint,
    store_error, validate_communication_principal, validate_id, validate_idempotency_key,
    validate_proof, AgentEndpointRecord, CommunicationPrincipal, CommunicationStoreError,
    ConversationAccess, ConversationSummaryRecord, DurableAgentIdentity, AGENT_ENDPOINT_ID_PREFIX,
    CONVERSATION_ID_PREFIX, DURABLE_AGENT_ID_PREFIX,
};
use super::Database;
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;

pub const AGENT_WAKE_ID_PREFIX: &str = "wc_wake_";
pub(crate) const AGENT_WAKE_ATTEMPT_ID_PREFIX: &str = "wc_wake_attempt_";
pub(crate) const AGENT_WAKE_CLAIM_FENCE_PREFIX: &str = "wc_wake_claim_";
pub const AGENT_WAKE_CONSUME_TOKEN_PREFIX: &str = "wc_wake_consume_";

const DEFAULT_WAKE_CLAIM_LEASE_MS: i64 = 30_000;
const MAX_ADAPTER_KIND_CHARS: usize = 64;
const WAKE_TRIGGER_INBOX_CHANGED: &str = "inbox_changed";
pub(crate) const WAKE_TRIGGER_AGENT_TASK_ATTEMPT: &str = "agent_task_attempt";
pub(crate) const WAKE_TRIGGER_ATTENTION_EVENT: &str = "attention_event";
pub(crate) const WAKE_TRIGGER_AGENT_WAIT_EVENTS: &str = "agent_wait_events";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWakeState {
    Pending,
    Claimed,
    Prepared,
    Delivered,
    DeliveryUnknown,
    Consumed,
    Retired,
}

impl AgentWakeState {
    #[cfg(test)]
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Prepared => "prepared",
            Self::Delivered => "delivered",
            Self::DeliveryUnknown => "delivery_unknown",
            Self::Consumed => "consumed",
            Self::Retired => "retired",
        }
    }

    pub(crate) fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "prepared" => Ok(Self::Prepared),
            "delivered" => Ok(Self::Delivered),
            "delivery_unknown" => Ok(Self::DeliveryUnknown),
            "consumed" => Ok(Self::Consumed),
            "retired" => Ok(Self::Retired),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported Agent Wake state: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentWakeAttemptState {
    Claimed,
    Prepared,
    Delivered,
    DeliveryUnknown,
    Revoked,
    Consumed,
}

impl AgentWakeAttemptState {
    fn from_db(value: &str, index: usize) -> rusqlite::Result<Self> {
        match value {
            "claimed" => Ok(Self::Claimed),
            "prepared" => Ok(Self::Prepared),
            "delivered" => Ok(Self::Delivered),
            "delivery_unknown" => Ok(Self::DeliveryUnknown),
            "revoked" => Ok(Self::Revoked),
            "consumed" => Ok(Self::Consumed),
            other => Err(rusqlite::Error::FromSqlConversionFailure(
                index,
                Type::Text,
                format!("unsupported Agent Wake Attempt state: {other}").into(),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeRecord {
    pub wake_id: String,
    pub target_agent_id: String,
    pub trigger_kind: String,
    pub first_triggering_delivery_id: Option<String>,
    pub latest_triggering_delivery_id: Option<String>,
    pub latest_conversation_id: Option<String>,
    pub latest_message_id: Option<String>,
    pub inbox_high_watermark: Option<i64>,
    pub queued_delivery_count_snapshot: Option<i64>,
    pub source_task_id: Option<String>,
    pub source_task_attempt_id: Option<String>,
    pub source_event_id: Option<String>,
    pub source_wait_id: Option<String>,
    pub wait_match_count_snapshot: Option<i64>,
    pub wait_match_sequence_snapshot: Option<i64>,
    pub state: AgentWakeState,
    pub revision: i64,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
    pub claimed_attempt_id: Option<String>,
    pub claimed_endpoint_id: Option<String>,
    pub claimed_controller_generation: Option<i64>,
    pub claim_lease_expires_at_unix_ms: Option<i64>,
    pub consumed_at_unix_ms: Option<i64>,
    pub consumed_by_endpoint_id: Option<String>,
    pub consumed_controller_generation: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeAttemptRecord {
    pub attempt_id: String,
    pub wake_id: String,
    pub endpoint_id: String,
    pub controller_generation: i64,
    pub adapter_kind: String,
    pub state: AgentWakeAttemptState,
    pub claimed_at_unix_ms: i64,
    pub claim_lease_expires_at_unix_ms: i64,
    pub prepared_at_unix_ms: Option<i64>,
    pub delivered_at_unix_ms: Option<i64>,
    pub delivery_unknown_at_unix_ms: Option<i64>,
    pub revoked_at_unix_ms: Option<i64>,
    pub consumed_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeClaim {
    pub wake: AgentWakeRecord,
    pub attempt: AgentWakeAttemptRecord,
    #[serde(skip_serializing)]
    pub claim_fence: String,
    #[serde(skip_serializing)]
    pub consume_token: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeEnvelope {
    pub wake_id: String,
    pub agent_id: String,
    pub endpoint_id: String,
    pub controller_generation: i64,
    pub queued_delivery_count: i64,
    pub inbox_high_watermark: i64,
    pub consume_token: String,
    pub resume_hint: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakePrepared {
    pub wake: AgentWakeRecord,
    pub attempt: AgentWakeAttemptRecord,
    pub envelope: AgentWakeEnvelope,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeConsumeResult {
    pub wake_id: String,
    pub target_agent_id: String,
    pub state: AgentWakeState,
    pub already_consumed: bool,
    pub consumed_at_unix_ms: i64,
    pub state_changed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentInboxBootstrapSummary {
    pub queued_delivery_count: i64,
    pub inbox_high_watermark: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeBootstrapSummary {
    pub wake_id: String,
    pub state: AgentWakeState,
    pub revision: i64,
    pub trigger_kind: String,
    pub conversation_id: Option<String>,
    pub latest_message_id: Option<String>,
    pub queued_delivery_count: Option<i64>,
    pub inbox_high_watermark: Option<i64>,
    pub task_id: Option<String>,
    pub task_attempt_id: Option<String>,
    pub event_id: Option<String>,
    pub attention_kind: Option<String>,
    pub workflow_session_id: Option<String>,
    pub goal_id: Option<String>,
    pub wait_id: Option<String>,
    pub wait_match_count: Option<i64>,
    pub wait_match_sequence: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentConversationBootstrapRecord {
    pub acting_agent: DurableAgentIdentity,
    pub endpoint: AgentEndpointRecord,
    pub selected_conversation: Option<ConversationSummaryRecord>,
    pub inbox: AgentInboxBootstrapSummary,
    pub wake: Option<AgentWakeBootstrapSummary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AgentWakeExplicitActivation {
    pub wake: AgentWakeRecord,
    pub attempt_id: String,
    #[serde(skip_serializing)]
    pub consume_token: String,
    pub replayed: bool,
    pub state_changed: bool,
}

impl Database {
    pub(super) fn ensure_agent_wake_schema(conn: &mut Connection) -> anyhow::Result<()> {
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let wake_table_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'wc_agent_wakes')",
            [],
            |row| row.get(0),
        )?;
        let attempt_table_exists: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'wc_agent_wake_attempts')",
            [],
            |row| row.get(0),
        )?;
        if wake_table_exists != attempt_table_exists {
            anyhow::bail!(
                "unsupported partial Agent Wake schema; recreate post-v0.3.9 development state"
            );
        }
        if !wake_table_exists {
            let existing_deliveries: i64 =
                transaction.query_row("SELECT COUNT(*) FROM wc_agent_deliveries", [], |row| {
                    row.get(0)
                })?;
            if existing_deliveries != 0 {
                anyhow::bail!(
                    "unsupported pre-Wake communication state; existing deliveries are not backfilled"
                );
            }
        }
        let has_task_source: bool = if wake_table_exists {
            transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('wc_agent_wakes') WHERE name = 'source_task_attempt_id')",
                [],
                |row| row.get(0),
            )?
        } else {
            false
        };
        if wake_table_exists && !has_task_source {
            transaction.execute_batch(
                "
                ALTER TABLE wc_agent_wake_attempts RENAME TO wc_agent_wake_attempts_pre_a4b;
                ALTER TABLE wc_agent_wakes RENAME TO wc_agent_wakes_pre_a4b;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_target_state;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_queueable;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_dispatched;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_wake;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_endpoint;
                CREATE TABLE wc_agent_wakes (
                    wake_id TEXT PRIMARY KEY,
                    target_agent_id TEXT NOT NULL,
                    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('inbox_changed', 'agent_task_attempt')),
                    first_triggering_delivery_id TEXT,
                    latest_triggering_delivery_id TEXT,
                    latest_conversation_id TEXT,
                    latest_message_id TEXT,
                    inbox_high_watermark INTEGER CHECK(inbox_high_watermark IS NULL OR inbox_high_watermark >= 1),
                    queued_delivery_count_snapshot INTEGER CHECK(queued_delivery_count_snapshot IS NULL OR queued_delivery_count_snapshot >= 1),
                    source_task_id TEXT,
                    source_task_attempt_id TEXT,
                    state TEXT NOT NULL CHECK(state IN (
                        'pending', 'claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed', 'retired'
                    )),
                    revision INTEGER NOT NULL CHECK(revision >= 1),
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    claimed_attempt_id TEXT,
                    claimed_endpoint_id TEXT,
                    claimed_controller_generation INTEGER,
                    claim_lease_expires_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    consumed_by_endpoint_id TEXT,
                    consumed_controller_generation INTEGER,
                    FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                    FOREIGN KEY(first_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_conversation_id) REFERENCES wc_conversations(conversation_id),
                    FOREIGN KEY(latest_message_id) REFERENCES wc_conversation_messages(message_id),
                    FOREIGN KEY(source_task_id) REFERENCES wc_agent_tasks(task_id),
                    FOREIGN KEY(source_task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                    FOREIGN KEY(claimed_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    FOREIGN KEY(consumed_by_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    CHECK(
                        (trigger_kind = 'inbox_changed'
                            AND first_triggering_delivery_id IS NOT NULL
                            AND latest_triggering_delivery_id IS NOT NULL
                            AND latest_conversation_id IS NOT NULL
                            AND latest_message_id IS NOT NULL
                            AND inbox_high_watermark IS NOT NULL
                            AND queued_delivery_count_snapshot IS NOT NULL
                            AND source_task_id IS NULL
                            AND source_task_attempt_id IS NULL)
                        OR (trigger_kind = 'agent_task_attempt'
                            AND first_triggering_delivery_id IS NULL
                            AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL
                            AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL
                            AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NOT NULL
                            AND source_task_attempt_id IS NOT NULL)
                    ),
                    CHECK(
                        (state IN ('pending', 'retired')
                            AND claimed_attempt_id IS NULL
                            AND claimed_endpoint_id IS NULL
                            AND claimed_controller_generation IS NULL
                            AND claim_lease_expires_at_unix_ms IS NULL)
                        OR state IN ('claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed')
                    ),
                    CHECK(
                        state != 'consumed'
                        OR (consumed_at_unix_ms IS NOT NULL
                            AND consumed_by_endpoint_id IS NOT NULL
                            AND consumed_controller_generation IS NOT NULL)
                    )
                );
                INSERT INTO wc_agent_wakes (
                    wake_id, target_agent_id, trigger_kind,
                    first_triggering_delivery_id, latest_triggering_delivery_id,
                    latest_conversation_id, latest_message_id,
                    inbox_high_watermark, queued_delivery_count_snapshot,
                    source_task_id, source_task_attempt_id,
                    state, revision, created_at_unix_ms, updated_at_unix_ms,
                    claimed_attempt_id, claimed_endpoint_id,
                    claimed_controller_generation, claim_lease_expires_at_unix_ms,
                    consumed_at_unix_ms, consumed_by_endpoint_id,
                    consumed_controller_generation
                )
                SELECT wake_id, target_agent_id, trigger_kind,
                       first_triggering_delivery_id, latest_triggering_delivery_id,
                       latest_conversation_id, latest_message_id,
                       inbox_high_watermark, queued_delivery_count_snapshot,
                       NULL, NULL,
                       state, revision, created_at_unix_ms, updated_at_unix_ms,
                       claimed_attempt_id, claimed_endpoint_id,
                       claimed_controller_generation, claim_lease_expires_at_unix_ms,
                       consumed_at_unix_ms, consumed_by_endpoint_id,
                       consumed_controller_generation
                FROM wc_agent_wakes_pre_a4b;
                CREATE TABLE wc_agent_wake_attempts (
                    attempt_id TEXT PRIMARY KEY,
                    wake_id TEXT NOT NULL,
                    endpoint_id TEXT NOT NULL,
                    controller_generation INTEGER NOT NULL CHECK(controller_generation >= 1),
                    adapter_kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN (
                        'claimed', 'prepared', 'delivered', 'delivery_unknown', 'revoked', 'consumed'
                    )),
                    claim_fence_hash TEXT NOT NULL,
                    consume_token_hash TEXT NOT NULL,
                    claimed_at_unix_ms INTEGER NOT NULL,
                    claim_lease_expires_at_unix_ms INTEGER NOT NULL,
                    prepared_at_unix_ms INTEGER,
                    delivered_at_unix_ms INTEGER,
                    delivery_unknown_at_unix_ms INTEGER,
                    revoked_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                    FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
                );
                INSERT INTO wc_agent_wake_attempts SELECT * FROM wc_agent_wake_attempts_pre_a4b;
                DROP TABLE wc_agent_wake_attempts_pre_a4b;
                DROP TABLE wc_agent_wakes_pre_a4b;
                ",
            )?;
        }
        let has_event_source: bool = if wake_table_exists {
            transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('wc_agent_wakes') WHERE name = 'source_event_id')",
                [],
                |row| row.get(0),
            )?
        } else {
            false
        };
        if wake_table_exists && !has_event_source {
            let endpoint_execution_table_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'wc_agent_task_endpoint_executions')",
                [],
                |row| row.get(0),
            )?;
            if endpoint_execution_table_exists {
                transaction.execute_batch(
                    "ALTER TABLE wc_agent_task_endpoint_executions RENAME TO wc_agent_task_endpoint_executions_pre_attention;
                     DROP INDEX IF EXISTS idx_wc_agent_task_endpoint_executions_task;",
                )?;
            }
            transaction.execute_batch(
                "
                ALTER TABLE wc_agent_wake_attempts RENAME TO wc_agent_wake_attempts_pre_attention;
                ALTER TABLE wc_agent_wakes RENAME TO wc_agent_wakes_pre_attention;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_target_state;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_queueable_inbox;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_task_attempt;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_dispatched;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_wake;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_endpoint;
                CREATE TABLE wc_agent_wakes (
                    wake_id TEXT PRIMARY KEY,
                    target_agent_id TEXT NOT NULL,
                    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('inbox_changed', 'agent_task_attempt', 'attention_event')),
                    first_triggering_delivery_id TEXT,
                    latest_triggering_delivery_id TEXT,
                    latest_conversation_id TEXT,
                    latest_message_id TEXT,
                    inbox_high_watermark INTEGER CHECK(inbox_high_watermark IS NULL OR inbox_high_watermark >= 1),
                    queued_delivery_count_snapshot INTEGER CHECK(queued_delivery_count_snapshot IS NULL OR queued_delivery_count_snapshot >= 1),
                    source_task_id TEXT,
                    source_task_attempt_id TEXT,
                    source_event_id TEXT,
                    state TEXT NOT NULL CHECK(state IN (
                        'pending', 'claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed', 'retired'
                    )),
                    revision INTEGER NOT NULL CHECK(revision >= 1),
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    claimed_attempt_id TEXT,
                    claimed_endpoint_id TEXT,
                    claimed_controller_generation INTEGER,
                    claim_lease_expires_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    consumed_by_endpoint_id TEXT,
                    consumed_controller_generation INTEGER,
                    FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                    FOREIGN KEY(first_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_conversation_id) REFERENCES wc_conversations(conversation_id),
                    FOREIGN KEY(latest_message_id) REFERENCES wc_conversation_messages(message_id),
                    FOREIGN KEY(source_task_id) REFERENCES wc_agent_tasks(task_id),
                    FOREIGN KEY(source_task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                    FOREIGN KEY(claimed_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    FOREIGN KEY(consumed_by_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    CHECK(
                        (trigger_kind = 'inbox_changed'
                            AND first_triggering_delivery_id IS NOT NULL
                            AND latest_triggering_delivery_id IS NOT NULL
                            AND latest_conversation_id IS NOT NULL
                            AND latest_message_id IS NOT NULL
                            AND inbox_high_watermark IS NOT NULL
                            AND queued_delivery_count_snapshot IS NOT NULL
                            AND source_task_id IS NULL
                            AND source_task_attempt_id IS NULL
                            AND source_event_id IS NULL)
                        OR (trigger_kind = 'agent_task_attempt'
                            AND first_triggering_delivery_id IS NULL
                            AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL
                            AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL
                            AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NOT NULL
                            AND source_task_attempt_id IS NOT NULL
                            AND source_event_id IS NULL)
                        OR (trigger_kind = 'attention_event'
                            AND first_triggering_delivery_id IS NULL
                            AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL
                            AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL
                            AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NULL
                            AND source_task_attempt_id IS NULL
                            AND source_event_id IS NOT NULL)
                    ),
                    CHECK(
                        (state IN ('pending', 'retired')
                            AND claimed_attempt_id IS NULL
                            AND claimed_endpoint_id IS NULL
                            AND claimed_controller_generation IS NULL
                            AND claim_lease_expires_at_unix_ms IS NULL)
                        OR state IN ('claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed')
                    ),
                    CHECK(
                        state != 'consumed'
                        OR (consumed_at_unix_ms IS NOT NULL
                            AND consumed_by_endpoint_id IS NOT NULL
                            AND consumed_controller_generation IS NOT NULL)
                    )
                );
                INSERT INTO wc_agent_wakes (
                    wake_id, target_agent_id, trigger_kind,
                    first_triggering_delivery_id, latest_triggering_delivery_id,
                    latest_conversation_id, latest_message_id,
                    inbox_high_watermark, queued_delivery_count_snapshot,
                    source_task_id, source_task_attempt_id, source_event_id,
                    state, revision, created_at_unix_ms, updated_at_unix_ms,
                    claimed_attempt_id, claimed_endpoint_id,
                    claimed_controller_generation, claim_lease_expires_at_unix_ms,
                    consumed_at_unix_ms, consumed_by_endpoint_id,
                    consumed_controller_generation
                )
                SELECT wake_id, target_agent_id, trigger_kind,
                       first_triggering_delivery_id, latest_triggering_delivery_id,
                       latest_conversation_id, latest_message_id,
                       inbox_high_watermark, queued_delivery_count_snapshot,
                       source_task_id, source_task_attempt_id, NULL,
                       state, revision, created_at_unix_ms, updated_at_unix_ms,
                       claimed_attempt_id, claimed_endpoint_id,
                       claimed_controller_generation, claim_lease_expires_at_unix_ms,
                       consumed_at_unix_ms, consumed_by_endpoint_id,
                       consumed_controller_generation
                FROM wc_agent_wakes_pre_attention;
                CREATE TABLE wc_agent_wake_attempts (
                    attempt_id TEXT PRIMARY KEY,
                    wake_id TEXT NOT NULL,
                    endpoint_id TEXT NOT NULL,
                    controller_generation INTEGER NOT NULL CHECK(controller_generation >= 1),
                    adapter_kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN (
                        'claimed', 'prepared', 'delivered', 'delivery_unknown', 'revoked', 'consumed'
                    )),
                    claim_fence_hash TEXT NOT NULL,
                    consume_token_hash TEXT NOT NULL,
                    claimed_at_unix_ms INTEGER NOT NULL,
                    claim_lease_expires_at_unix_ms INTEGER NOT NULL,
                    prepared_at_unix_ms INTEGER,
                    delivered_at_unix_ms INTEGER,
                    delivery_unknown_at_unix_ms INTEGER,
                    revoked_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                    FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
                );
                INSERT INTO wc_agent_wake_attempts SELECT * FROM wc_agent_wake_attempts_pre_attention;
                ",
            )?;
            if endpoint_execution_table_exists {
                transaction.execute_batch(
                    "CREATE TABLE wc_agent_task_endpoint_executions (
                        task_id TEXT NOT NULL,
                        attempt_id TEXT NOT NULL UNIQUE,
                        wake_id TEXT NOT NULL UNIQUE,
                        start_identity_fingerprint TEXT NOT NULL CHECK(length(start_identity_fingerprint) = 64),
                        endpoint_id TEXT,
                        endpoint_controller_generation INTEGER,
                        created_at_unix_ms INTEGER NOT NULL,
                        updated_at_unix_ms INTEGER NOT NULL,
                        PRIMARY KEY(task_id, attempt_id),
                        CHECK(
                            (endpoint_id IS NULL AND endpoint_controller_generation IS NULL)
                            OR (endpoint_id IS NOT NULL AND endpoint_controller_generation >= 1)
                        ),
                        FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                        FOREIGN KEY(attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                        FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                        FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
                     );
                     INSERT INTO wc_agent_task_endpoint_executions
                         SELECT * FROM wc_agent_task_endpoint_executions_pre_attention;
                     DROP TABLE wc_agent_task_endpoint_executions_pre_attention;
                     CREATE INDEX idx_wc_agent_task_endpoint_executions_task
                         ON wc_agent_task_endpoint_executions(task_id, updated_at_unix_ms DESC);",
                )?;
            }
            transaction.execute_batch(
                "DROP TABLE wc_agent_wake_attempts_pre_attention;
                 DROP TABLE wc_agent_wakes_pre_attention;",
            )?;
        }
        let has_wait_source: bool = if wake_table_exists {
            transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('wc_agent_wakes') WHERE name = 'source_wait_id')",
                [],
                |row| row.get(0),
            )?
        } else {
            false
        };
        if wake_table_exists && !has_wait_source {
            let endpoint_execution_table_exists: bool = transaction.query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'wc_agent_task_endpoint_executions')",
                [],
                |row| row.get(0),
            )?;
            if endpoint_execution_table_exists {
                transaction.execute_batch(
                    "ALTER TABLE wc_agent_task_endpoint_executions RENAME TO wc_agent_task_endpoint_executions_pre_wait;
                     DROP INDEX IF EXISTS idx_wc_agent_task_endpoint_executions_task;",
                )?;
            }
            transaction.execute_batch(
                "
                ALTER TABLE wc_agent_wake_attempts RENAME TO wc_agent_wake_attempts_pre_wait;
                ALTER TABLE wc_agent_wakes RENAME TO wc_agent_wakes_pre_wait;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_target_state;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_queueable_inbox;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_task_attempt;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_attention_event;
                DROP INDEX IF EXISTS idx_wc_agent_wakes_one_dispatched;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_wake;
                DROP INDEX IF EXISTS idx_wc_agent_wake_attempts_endpoint;
                CREATE TABLE wc_agent_wakes (
                    wake_id TEXT PRIMARY KEY,
                    target_agent_id TEXT NOT NULL,
                    trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('inbox_changed', 'agent_task_attempt', 'attention_event', 'agent_wait_events')),
                    first_triggering_delivery_id TEXT,
                    latest_triggering_delivery_id TEXT,
                    latest_conversation_id TEXT,
                    latest_message_id TEXT,
                    inbox_high_watermark INTEGER CHECK(inbox_high_watermark IS NULL OR inbox_high_watermark >= 1),
                    queued_delivery_count_snapshot INTEGER CHECK(queued_delivery_count_snapshot IS NULL OR queued_delivery_count_snapshot >= 1),
                    source_task_id TEXT,
                    source_task_attempt_id TEXT,
                    source_event_id TEXT,
                    source_wait_id TEXT,
                    wait_match_count_snapshot INTEGER CHECK(wait_match_count_snapshot IS NULL OR wait_match_count_snapshot >= 1),
                    wait_match_sequence_snapshot INTEGER CHECK(wait_match_sequence_snapshot IS NULL OR wait_match_sequence_snapshot >= 1),
                    state TEXT NOT NULL CHECK(state IN (
                        'pending', 'claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed', 'retired'
                    )),
                    revision INTEGER NOT NULL CHECK(revision >= 1),
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL,
                    claimed_attempt_id TEXT,
                    claimed_endpoint_id TEXT,
                    claimed_controller_generation INTEGER,
                    claim_lease_expires_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    consumed_by_endpoint_id TEXT,
                    consumed_controller_generation INTEGER,
                    FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                    FOREIGN KEY(first_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                    FOREIGN KEY(latest_conversation_id) REFERENCES wc_conversations(conversation_id),
                    FOREIGN KEY(latest_message_id) REFERENCES wc_conversation_messages(message_id),
                    FOREIGN KEY(source_task_id) REFERENCES wc_agent_tasks(task_id),
                    FOREIGN KEY(source_task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                    FOREIGN KEY(source_wait_id) REFERENCES wc_agent_waits(wait_id),
                    FOREIGN KEY(claimed_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    FOREIGN KEY(consumed_by_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                    CHECK(
                        (trigger_kind = 'inbox_changed'
                            AND first_triggering_delivery_id IS NOT NULL
                            AND latest_triggering_delivery_id IS NOT NULL
                            AND latest_conversation_id IS NOT NULL
                            AND latest_message_id IS NOT NULL
                            AND inbox_high_watermark IS NOT NULL
                            AND queued_delivery_count_snapshot IS NOT NULL
                            AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                            AND source_event_id IS NULL AND source_wait_id IS NULL
                            AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                        OR (trigger_kind = 'agent_task_attempt'
                            AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NOT NULL AND source_task_attempt_id IS NOT NULL
                            AND source_event_id IS NULL AND source_wait_id IS NULL
                            AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                        OR (trigger_kind = 'attention_event'
                            AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                            AND source_event_id IS NOT NULL AND source_wait_id IS NULL
                            AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                        OR (trigger_kind = 'agent_wait_events'
                            AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                            AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                            AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                            AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                            AND source_event_id IS NULL AND source_wait_id IS NOT NULL
                            AND wait_match_count_snapshot IS NOT NULL AND wait_match_sequence_snapshot IS NOT NULL)
                    ),
                    CHECK(
                        (state IN ('pending', 'retired')
                            AND claimed_attempt_id IS NULL AND claimed_endpoint_id IS NULL
                            AND claimed_controller_generation IS NULL AND claim_lease_expires_at_unix_ms IS NULL)
                        OR state IN ('claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed')
                    ),
                    CHECK(state != 'consumed' OR (consumed_at_unix_ms IS NOT NULL
                        AND consumed_by_endpoint_id IS NOT NULL AND consumed_controller_generation IS NOT NULL))
                );
                INSERT INTO wc_agent_wakes (
                    wake_id, target_agent_id, trigger_kind,
                    first_triggering_delivery_id, latest_triggering_delivery_id,
                    latest_conversation_id, latest_message_id,
                    inbox_high_watermark, queued_delivery_count_snapshot,
                    source_task_id, source_task_attempt_id, source_event_id,
                    source_wait_id, wait_match_count_snapshot, wait_match_sequence_snapshot,
                    state, revision, created_at_unix_ms, updated_at_unix_ms,
                    claimed_attempt_id, claimed_endpoint_id, claimed_controller_generation,
                    claim_lease_expires_at_unix_ms, consumed_at_unix_ms,
                    consumed_by_endpoint_id, consumed_controller_generation
                )
                SELECT wake_id, target_agent_id, trigger_kind,
                       first_triggering_delivery_id, latest_triggering_delivery_id,
                       latest_conversation_id, latest_message_id,
                       inbox_high_watermark, queued_delivery_count_snapshot,
                       source_task_id, source_task_attempt_id, source_event_id,
                       NULL, NULL, NULL,
                       state, revision, created_at_unix_ms, updated_at_unix_ms,
                       claimed_attempt_id, claimed_endpoint_id, claimed_controller_generation,
                       claim_lease_expires_at_unix_ms, consumed_at_unix_ms,
                       consumed_by_endpoint_id, consumed_controller_generation
                FROM wc_agent_wakes_pre_wait;
                CREATE TABLE wc_agent_wake_attempts (
                    attempt_id TEXT PRIMARY KEY,
                    wake_id TEXT NOT NULL,
                    endpoint_id TEXT NOT NULL,
                    controller_generation INTEGER NOT NULL CHECK(controller_generation >= 1),
                    adapter_kind TEXT NOT NULL,
                    state TEXT NOT NULL CHECK(state IN ('claimed', 'prepared', 'delivered', 'delivery_unknown', 'revoked', 'consumed')),
                    claim_fence_hash TEXT NOT NULL,
                    consume_token_hash TEXT NOT NULL,
                    claimed_at_unix_ms INTEGER NOT NULL,
                    claim_lease_expires_at_unix_ms INTEGER NOT NULL,
                    prepared_at_unix_ms INTEGER,
                    delivered_at_unix_ms INTEGER,
                    delivery_unknown_at_unix_ms INTEGER,
                    revoked_at_unix_ms INTEGER,
                    consumed_at_unix_ms INTEGER,
                    FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                    FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
                );
                INSERT INTO wc_agent_wake_attempts SELECT * FROM wc_agent_wake_attempts_pre_wait;
                ",
            )?;
            if endpoint_execution_table_exists {
                transaction.execute_batch(
                    "CREATE TABLE wc_agent_task_endpoint_executions (
                        task_id TEXT NOT NULL,
                        attempt_id TEXT NOT NULL UNIQUE,
                        wake_id TEXT NOT NULL UNIQUE,
                        start_identity_fingerprint TEXT NOT NULL CHECK(length(start_identity_fingerprint) = 64),
                        endpoint_id TEXT,
                        endpoint_controller_generation INTEGER,
                        created_at_unix_ms INTEGER NOT NULL,
                        updated_at_unix_ms INTEGER NOT NULL,
                        PRIMARY KEY(task_id, attempt_id),
                        CHECK((endpoint_id IS NULL AND endpoint_controller_generation IS NULL)
                           OR (endpoint_id IS NOT NULL AND endpoint_controller_generation >= 1)),
                        FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                        FOREIGN KEY(attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                        FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                        FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
                     );
                     INSERT INTO wc_agent_task_endpoint_executions
                         SELECT * FROM wc_agent_task_endpoint_executions_pre_wait;
                     DROP TABLE wc_agent_task_endpoint_executions_pre_wait;
                     CREATE INDEX idx_wc_agent_task_endpoint_executions_task
                         ON wc_agent_task_endpoint_executions(task_id, updated_at_unix_ms DESC);",
                )?;
            }
            transaction.execute_batch(
                "DROP TABLE wc_agent_wake_attempts_pre_wait;
                 DROP TABLE wc_agent_wakes_pre_wait;",
            )?;
        }
        transaction.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_agent_wakes (
                wake_id TEXT PRIMARY KEY,
                target_agent_id TEXT NOT NULL,
                trigger_kind TEXT NOT NULL CHECK(trigger_kind IN ('inbox_changed', 'agent_task_attempt', 'attention_event', 'agent_wait_events')),
                first_triggering_delivery_id TEXT,
                latest_triggering_delivery_id TEXT,
                latest_conversation_id TEXT,
                latest_message_id TEXT,
                inbox_high_watermark INTEGER CHECK(inbox_high_watermark IS NULL OR inbox_high_watermark >= 1),
                queued_delivery_count_snapshot INTEGER CHECK(queued_delivery_count_snapshot IS NULL OR queued_delivery_count_snapshot >= 1),
                source_task_id TEXT,
                source_task_attempt_id TEXT,
                source_event_id TEXT,
                source_wait_id TEXT,
                wait_match_count_snapshot INTEGER CHECK(wait_match_count_snapshot IS NULL OR wait_match_count_snapshot >= 1),
                wait_match_sequence_snapshot INTEGER CHECK(wait_match_sequence_snapshot IS NULL OR wait_match_sequence_snapshot >= 1),
                state TEXT NOT NULL CHECK(state IN (
                    'pending', 'claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed', 'retired'
                )),
                revision INTEGER NOT NULL CHECK(revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                claimed_attempt_id TEXT,
                claimed_endpoint_id TEXT,
                claimed_controller_generation INTEGER,
                claim_lease_expires_at_unix_ms INTEGER,
                consumed_at_unix_ms INTEGER,
                consumed_by_endpoint_id TEXT,
                consumed_controller_generation INTEGER,
                FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                FOREIGN KEY(first_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                FOREIGN KEY(latest_triggering_delivery_id) REFERENCES wc_agent_deliveries(delivery_id),
                FOREIGN KEY(latest_conversation_id) REFERENCES wc_conversations(conversation_id),
                FOREIGN KEY(latest_message_id) REFERENCES wc_conversation_messages(message_id),
                FOREIGN KEY(source_task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(source_task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
                FOREIGN KEY(source_wait_id) REFERENCES wc_agent_waits(wait_id),
                FOREIGN KEY(claimed_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                FOREIGN KEY(consumed_by_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id),
                CHECK(
                    (trigger_kind = 'inbox_changed'
                        AND first_triggering_delivery_id IS NOT NULL AND latest_triggering_delivery_id IS NOT NULL
                        AND latest_conversation_id IS NOT NULL AND latest_message_id IS NOT NULL
                        AND inbox_high_watermark IS NOT NULL AND queued_delivery_count_snapshot IS NOT NULL
                        AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                        AND source_event_id IS NULL AND source_wait_id IS NULL
                        AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                    OR (trigger_kind = 'agent_task_attempt'
                        AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                        AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                        AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                        AND source_task_id IS NOT NULL AND source_task_attempt_id IS NOT NULL
                        AND source_event_id IS NULL AND source_wait_id IS NULL
                        AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                    OR (trigger_kind = 'attention_event'
                        AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                        AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                        AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                        AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                        AND source_event_id IS NOT NULL AND source_wait_id IS NULL
                        AND wait_match_count_snapshot IS NULL AND wait_match_sequence_snapshot IS NULL)
                    OR (trigger_kind = 'agent_wait_events'
                        AND first_triggering_delivery_id IS NULL AND latest_triggering_delivery_id IS NULL
                        AND latest_conversation_id IS NULL AND latest_message_id IS NULL
                        AND inbox_high_watermark IS NULL AND queued_delivery_count_snapshot IS NULL
                        AND source_task_id IS NULL AND source_task_attempt_id IS NULL
                        AND source_event_id IS NULL AND source_wait_id IS NOT NULL
                        AND wait_match_count_snapshot IS NOT NULL AND wait_match_sequence_snapshot IS NOT NULL)
                ),
                CHECK(
                    (state IN ('pending', 'retired')
                        AND claimed_attempt_id IS NULL AND claimed_endpoint_id IS NULL
                        AND claimed_controller_generation IS NULL AND claim_lease_expires_at_unix_ms IS NULL)
                    OR state IN ('claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed')
                ),
                CHECK(state != 'consumed' OR (consumed_at_unix_ms IS NOT NULL
                    AND consumed_by_endpoint_id IS NOT NULL AND consumed_controller_generation IS NOT NULL))
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wakes_target_state
                ON wc_agent_wakes(target_agent_id, state, created_at_unix_ms, wake_id);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wakes_target_created
                ON wc_agent_wakes(target_agent_id, created_at_unix_ms, wake_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_wakes_one_queueable_inbox
                ON wc_agent_wakes(target_agent_id)
                WHERE trigger_kind = 'inbox_changed' AND state IN ('pending', 'claimed');
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_wakes_task_attempt
                ON wc_agent_wakes(source_task_attempt_id)
                WHERE trigger_kind = 'agent_task_attempt';
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_wakes_attention_event
                ON wc_agent_wakes(source_event_id)
                WHERE trigger_kind = 'attention_event';
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_wakes_agent_wait
                ON wc_agent_wakes(source_wait_id)
                WHERE trigger_kind = 'agent_wait_events';
            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_wakes_one_dispatched
                ON wc_agent_wakes(target_agent_id)
                WHERE state IN ('prepared', 'delivered', 'delivery_unknown');

            CREATE TABLE IF NOT EXISTS wc_agent_wake_attempts (
                attempt_id TEXT PRIMARY KEY,
                wake_id TEXT NOT NULL,
                endpoint_id TEXT NOT NULL,
                controller_generation INTEGER NOT NULL CHECK(controller_generation >= 1),
                adapter_kind TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN (
                    'claimed', 'prepared', 'delivered', 'delivery_unknown', 'revoked', 'consumed'
                )),
                claim_fence_hash TEXT NOT NULL,
                consume_token_hash TEXT NOT NULL,
                claimed_at_unix_ms INTEGER NOT NULL,
                claim_lease_expires_at_unix_ms INTEGER NOT NULL,
                prepared_at_unix_ms INTEGER,
                delivered_at_unix_ms INTEGER,
                delivery_unknown_at_unix_ms INTEGER,
                revoked_at_unix_ms INTEGER,
                consumed_at_unix_ms INTEGER,
                FOREIGN KEY(wake_id) REFERENCES wc_agent_wakes(wake_id),
                FOREIGN KEY(endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wake_attempts_wake
                ON wc_agent_wake_attempts(wake_id, claimed_at_unix_ms, attempt_id);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_wake_attempts_endpoint
                ON wc_agent_wake_attempts(endpoint_id, controller_generation, state);
            ",
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn recover_agent_wakes_for_server_takeover(
        &self,
        ownership: &crate::server_instance::ServerInstanceGuard,
        now: i64,
    ) -> Result<(), CommunicationStoreError> {
        if !ownership.owns_database(self) {
            return Err(CommunicationStoreError::new(
                "server_instance_ownership_mismatch",
                "Server ownership proof does not match this database state",
            ));
        }
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        expire_stale_endpoints(&transaction, now)?;
        reserve_prepared_agent_task_dispatch_graces_for_server_takeover(&transaction, now)?;
        // Process-local Host callbacks/adapters never survive a Server
        // takeover. Clear their durable capability projection before any
        // successor can treat an old Endpoint as dispatchable. Deliberately
        // preserve the last current MCP App recovery fingerprint and canonical
        // ClientWindow key: neither is authority, but after ordinary exact
        // Endpoint checks they fence which View/Host window may recreate only
        // the lost process-local binding.
        transaction
            .execute(
                "UPDATE wc_agent_endpoints
                 SET wake_capable = 0
                 WHERE lifecycle = 'attached' AND wake_capable != 0",
                [],
            )
            .map_err(store_error)?;

        transaction
            .execute(
                "UPDATE wc_agent_task_endpoint_executions
                 SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?1)
                 WHERE wake_id IN (
                     SELECT wake_id FROM wc_agent_wakes
                     WHERE trigger_kind = 'agent_task_attempt' AND state = 'claimed'
                 )",
                params![now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wake_attempts
                 SET state = 'revoked', revoked_at_unix_ms = COALESCE(revoked_at_unix_ms, ?1)
                 WHERE state = 'claimed'",
                params![now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'pending', revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?1),
                     claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                     claimed_controller_generation = NULL,
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE state = 'claimed'",
                params![now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wake_attempts
                 SET state = 'delivery_unknown',
                     delivery_unknown_at_unix_ms = COALESCE(delivery_unknown_at_unix_ms, ?1)
                 WHERE state = 'prepared'",
                params![now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'delivery_unknown', revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?1),
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE state = 'prepared'",
                params![now],
            )
            .map_err(store_error)?;
        transaction.commit().map_err(store_error)?;
        Ok(())
    }

    pub fn claim_next_agent_wake(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        adapter_kind: &str,
    ) -> Result<Option<AgentWakeClaim>, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
        validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
        let adapter_kind = validate_adapter_kind(adapter_kind)?;
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let endpoint = require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        if !endpoint.wake_capable {
            return Err(CommunicationStoreError::new(
                "endpoint_not_wake_capable",
                "Agent Endpoint is not wake-capable",
            ));
        }
        release_expired_claims_for_agent(&transaction, agent_id, now)?;
        retire_stale_agent_task_wakes_for_agent(&transaction, agent_id, now)?;
        let blocked: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM wc_agent_wakes
                    WHERE target_agent_id = ?1
                      AND state IN ('prepared', 'delivered', 'delivery_unknown')
                 )",
                params![agent_id],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if blocked {
            transaction.commit().map_err(store_error)?;
            return Ok(None);
        }
        let wake_id: Option<String> = transaction
            .query_row(
                "SELECT wake_id FROM wc_agent_wakes
                 WHERE target_agent_id = ?1 AND state = 'pending'
                 ORDER BY created_at_unix_ms, wake_id LIMIT 1",
                params![agent_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(store_error)?;
        let Some(wake_id) = wake_id else {
            transaction.commit().map_err(store_error)?;
            return Ok(None);
        };
        let selected_wake = load_wake(&transaction, &wake_id)?.ok_or_else(|| {
            CommunicationStoreError::new("wake_not_found", "Selected Agent Wake disappeared")
        })?;
        if selected_wake.trigger_kind == WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
            bind_agent_task_wake_carrier(
                &transaction,
                principal,
                &selected_wake,
                endpoint_id,
                expected_controller_generation,
                now,
            )?;
        }
        if selected_wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
            require_agent_attention_event_for_wake(
                &transaction,
                principal,
                selected_wake.source_event_id.as_deref(),
                agent_id,
            )?;
        }
        if selected_wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
            require_agent_wait_for_wake(
                &transaction,
                principal,
                selected_wake.source_wait_id.as_deref(),
                agent_id,
            )?;
        }
        let attempt_id = allocate_identity(
            &transaction,
            AGENT_WAKE_ATTEMPT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_wake_attempts WHERE attempt_id = ?1)",
        )?;
        let claim_fence = new_proof(AGENT_WAKE_CLAIM_FENCE_PREFIX);
        let consume_token = new_proof(AGENT_WAKE_CONSUME_TOKEN_PREFIX);
        let claim_fence_hash = digest_text("webcodex.agent-wake.claim-fence.v1", &claim_fence);
        let consume_token_hash =
            digest_text("webcodex.agent-wake.consume-token.v1", &consume_token);
        let claim_lease_expires_at_unix_ms = now.saturating_add(DEFAULT_WAKE_CLAIM_LEASE_MS);
        transaction
            .execute(
                "INSERT INTO wc_agent_wake_attempts (
                    attempt_id, wake_id, endpoint_id, controller_generation,
                    adapter_kind, state, claim_fence_hash, consume_token_hash,
                    claimed_at_unix_ms, claim_lease_expires_at_unix_ms,
                    prepared_at_unix_ms, delivered_at_unix_ms,
                    delivery_unknown_at_unix_ms, revoked_at_unix_ms,
                    consumed_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'claimed', ?6, ?7, ?8, ?9,
                           NULL, NULL, NULL, NULL, NULL)",
                params![
                    attempt_id,
                    wake_id,
                    endpoint_id,
                    expected_controller_generation,
                    adapter_kind,
                    claim_fence_hash,
                    consume_token_hash,
                    now,
                    claim_lease_expires_at_unix_ms,
                ],
            )
            .map_err(store_error)?;
        let changed = transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'claimed', revision = revision + 1,
                     updated_at_unix_ms = ?2, claimed_attempt_id = ?3,
                     claimed_endpoint_id = ?4, claimed_controller_generation = ?5,
                     claim_lease_expires_at_unix_ms = ?6
                 WHERE wake_id = ?1 AND state = 'pending'",
                params![
                    wake_id,
                    now,
                    attempt_id,
                    endpoint_id,
                    expected_controller_generation,
                    claim_lease_expires_at_unix_ms,
                ],
            )
            .map_err(store_error)?;
        if changed != 1 {
            return Err(CommunicationStoreError::new(
                "wake_claim_conflict",
                "Agent Wake was claimed concurrently",
            ));
        }
        let wake = load_wake(&transaction, &wake_id)?.expect("claimed Wake must exist");
        let attempt =
            load_attempt(&transaction, &attempt_id)?.expect("inserted Wake Attempt must exist");
        transaction.commit().map_err(store_error)?;
        Ok(Some(AgentWakeClaim {
            wake,
            attempt,
            claim_fence,
            consume_token,
        }))
    }

    pub fn release_agent_wake_claim(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        validate_wake_mutation_ids(agent_id, endpoint_id, wake_id, attempt_id)?;
        validate_communication_principal(principal)?;
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        let wake = require_exact_claim(
            &transaction,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            None,
        )?;
        if wake.state != AgentWakeState::Claimed {
            return Err(CommunicationStoreError::new(
                "wake_dispatch_fence_crossed",
                "Agent Wake can only be released before the dispatch fence",
            ));
        }
        transaction
            .execute(
                "UPDATE wc_agent_wake_attempts
                 SET state = 'revoked', revoked_at_unix_ms = ?2
                 WHERE attempt_id = ?1 AND state = 'claimed'",
                params![attempt_id, now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'pending', revision = revision + 1,
                     updated_at_unix_ms = ?2, claimed_attempt_id = NULL,
                     claimed_endpoint_id = NULL, claimed_controller_generation = NULL,
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE wake_id = ?1 AND state = 'claimed'",
                params![wake_id, now],
            )
            .map_err(store_error)?;
        clear_agent_task_wake_carrier(&transaction, wake_id, now)?;
        let wake = load_wake(&transaction, wake_id)?.expect("released Wake must exist");
        transaction.commit().map_err(store_error)?;
        Ok(wake)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_agent_wake_dispatch(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
        consume_token: &str,
    ) -> Result<AgentWakePrepared, CommunicationStoreError> {
        validate_wake_mutation_ids(agent_id, endpoint_id, wake_id, attempt_id)?;
        validate_communication_principal(principal)?;
        validate_proof(
            consume_token,
            AGENT_WAKE_CONSUME_TOKEN_PREFIX,
            "invalid_wake_consume_token",
        )?;
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let endpoint = require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        if !endpoint.wake_capable {
            return Err(CommunicationStoreError::new(
                "endpoint_not_wake_capable",
                "Agent Endpoint is not wake-capable",
            ));
        }
        let wake = require_exact_claim(
            &transaction,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            Some(consume_token),
        )?;
        if wake.state != AgentWakeState::Claimed {
            return Err(CommunicationStoreError::new(
                "wake_not_claimed",
                "Agent Wake is not in the claimed pre-dispatch state",
            ));
        }
        if wake
            .claim_lease_expires_at_unix_ms
            .is_some_and(|expires| expires <= now)
        {
            return Err(CommunicationStoreError::new(
                "wake_claim_expired",
                "Agent Wake claim lease expired before the dispatch fence",
            ));
        }
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_TASK_ATTEMPT
            && !agent_task_wake_is_dispatchable(
                &transaction,
                principal,
                &wake,
                endpoint_id,
                expected_controller_generation,
                now,
            )?
        {
            retire_agent_task_wake_pre_dispatch(&transaction, &wake.wake_id, now)?;
            transaction.commit().map_err(store_error)?;
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_stale",
                "AgentTaskAttempt expired or became stale before Host continuation dispatch",
            ));
        }
        if wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
            require_agent_attention_event_for_wake(
                &transaction,
                principal,
                wake.source_event_id.as_deref(),
                agent_id,
            )?;
        }
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
            require_agent_wait_for_wake(
                &transaction,
                principal,
                wake.source_wait_id.as_deref(),
                agent_id,
            )?;
        }
        transaction
            .execute(
                "UPDATE wc_agent_wake_attempts
                 SET state = 'prepared', prepared_at_unix_ms = ?2
                 WHERE attempt_id = ?1 AND state = 'claimed'",
                params![attempt_id, now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'prepared', revision = revision + 1,
                     updated_at_unix_ms = ?2, claim_lease_expires_at_unix_ms = NULL
                 WHERE wake_id = ?1 AND state = 'claimed'",
                params![wake_id, now],
            )
            .map_err(store_error)?;
        let wake = load_wake(&transaction, wake_id)?.expect("prepared Wake must exist");
        let attempt = load_attempt(&transaction, attempt_id)?.expect("prepared Attempt must exist");
        let envelope = wake_envelope(
            &transaction,
            principal,
            &wake,
            endpoint_id,
            expected_controller_generation,
            consume_token,
        )?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentWakePrepared {
            wake,
            attempt,
            envelope,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_agent_wake_delivery(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        self.finish_agent_wake_delivery(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            true,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_wake_delivery_at(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
        now: i64,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        self.finish_agent_wake_delivery(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            true,
            Some(now),
        )
    }

    /// Revalidate the exact current Endpoint and prepared Attempt immediately
    /// before invoking an external Host callback. The durable dispatch fence
    /// has already been crossed, so replacement after preparation remains
    /// conservatively uncertain.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_agent_wake_dispatch_binding(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
    ) -> Result<(), CommunicationStoreError> {
        validate_wake_mutation_ids(agent_id, endpoint_id, wake_id, attempt_id)?;
        validate_communication_principal(principal)?;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let endpoint = require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        if !endpoint.wake_capable {
            return Err(CommunicationStoreError::new(
                "endpoint_not_wake_capable",
                "Agent Endpoint no longer has a registered continuation adapter",
            ));
        }
        let wake = require_exact_claim(
            &transaction,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            None,
        )?;
        if wake.state != AgentWakeState::Prepared {
            return Err(CommunicationStoreError::new(
                "wake_not_prepared",
                "Agent Wake is no longer prepared for this dispatch",
            ));
        }
        let attempt = load_attempt(&transaction, attempt_id)?.ok_or_else(|| {
            CommunicationStoreError::new(
                "wake_attempt_not_found",
                "Agent Wake Attempt does not exist",
            )
        })?;
        if attempt.state != AgentWakeAttemptState::Prepared {
            return Err(CommunicationStoreError::new(
                "wake_attempt_not_prepared",
                "Agent Wake Attempt is no longer prepared for dispatch",
            ));
        }
        if wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
            require_agent_attention_event_for_wake(
                &transaction,
                principal,
                wake.source_event_id.as_deref(),
                agent_id,
            )?;
        }
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
            require_agent_wait_for_wake(
                &transaction,
                principal,
                wake.source_wait_id.as_deref(),
                agent_id,
            )?;
        }
        transaction.commit().map_err(store_error)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn mark_agent_wake_delivery_unknown(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        self.finish_agent_wake_delivery(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            false,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mark_agent_wake_delivery_unknown_at(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
        now: i64,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        self.finish_agent_wake_delivery(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            false,
            Some(now),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_agent_wake_delivery(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        attempt_id: &str,
        claim_fence: &str,
        delivered: bool,
        now: Option<i64>,
    ) -> Result<AgentWakeRecord, CommunicationStoreError> {
        validate_wake_mutation_ids(agent_id, endpoint_id, wake_id, attempt_id)?;
        validate_communication_principal(principal)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        let wake = require_exact_claim(
            &transaction,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            attempt_id,
            claim_fence,
            None,
        )?;
        if wake.state == AgentWakeState::Consumed {
            transaction.commit().map_err(store_error)?;
            return Ok(wake);
        }
        if delivered && wake.state == AgentWakeState::Delivered {
            transaction.commit().map_err(store_error)?;
            return Ok(wake);
        }
        if !delivered && wake.state == AgentWakeState::DeliveryUnknown {
            transaction.commit().map_err(store_error)?;
            return Ok(wake);
        }
        if !matches!(
            wake.state,
            AgentWakeState::Prepared | AgentWakeState::DeliveryUnknown
        ) {
            return Err(CommunicationStoreError::new(
                "wake_not_prepared",
                "Agent Wake delivery outcome requires the exact prepared attempt",
            ));
        }
        if wake.state == AgentWakeState::Prepared {
            reserve_agent_task_endpoint_dispatch_grace_in_transaction(
                &transaction,
                principal,
                &wake,
                endpoint_id,
                expected_controller_generation,
                now,
            )?;
        }
        let (wake_state, attempt_state, timestamp_column) = if delivered {
            ("delivered", "delivered", "delivered_at_unix_ms")
        } else {
            (
                "delivery_unknown",
                "delivery_unknown",
                "delivery_unknown_at_unix_ms",
            )
        };
        let attempt_sql = format!(
            "UPDATE wc_agent_wake_attempts
             SET state = ?2, {timestamp_column} = COALESCE({timestamp_column}, ?3)
             WHERE attempt_id = ?1"
        );
        transaction
            .execute(&attempt_sql, params![attempt_id, attempt_state, now])
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = ?2, revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?3),
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE wake_id = ?1",
                params![wake_id, wake_state, now],
            )
            .map_err(store_error)?;
        let wake = load_wake(&transaction, wake_id)?.expect("finished Wake must exist");
        transaction.commit().map_err(store_error)?;
        Ok(wake)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn consume_agent_wake(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        consume_token: &str,
    ) -> Result<AgentWakeConsumeResult, CommunicationStoreError> {
        self.consume_agent_wake_with_now(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            consume_token,
            now_unix_ms(),
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn consume_agent_wake_at(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        consume_token: &str,
        now: i64,
    ) -> Result<AgentWakeConsumeResult, CommunicationStoreError> {
        self.consume_agent_wake_with_now(
            principal,
            agent_id,
            endpoint_id,
            expected_controller_generation,
            wake_id,
            consume_token,
            now,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn consume_agent_wake_with_now(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        consume_token: &str,
        now: i64,
    ) -> Result<AgentWakeConsumeResult, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
        validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
        validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
        validate_proof(
            consume_token,
            AGENT_WAKE_CONSUME_TOKEN_PREFIX,
            "invalid_wake_consume_token",
        )?;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let endpoint = require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        let wake = load_wake(&transaction, wake_id)?.ok_or_else(|| {
            CommunicationStoreError::new("wake_not_found", "Agent Wake does not exist")
        })?;
        if wake.target_agent_id != agent_id {
            return Err(CommunicationStoreError::new(
                "wake_agent_mismatch",
                "Agent Wake belongs to a different Agent",
            ));
        }
        if wake.claimed_endpoint_id.as_deref() != Some(endpoint_id)
            || wake.claimed_controller_generation != Some(expected_controller_generation)
        {
            return Err(CommunicationStoreError::new(
                "wake_endpoint_fence_mismatch",
                "Agent Wake is bound to a different Endpoint generation",
            ));
        }
        let wake_attempt_id = wake.claimed_attempt_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "wake_not_dispatched",
                "Agent Wake has no exact dispatched attempt to consume",
            )
        })?;
        let (expected_token_hash, adapter_kind): (String, String) = transaction
            .query_row(
                "SELECT consume_token_hash, adapter_kind FROM wc_agent_wake_attempts WHERE attempt_id = ?1",
                params![wake_attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(store_error)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "wake_attempt_not_found",
                    "Agent Wake Attempt does not exist",
                )
            })?;
        // A model turn that already crossed a durable Host dispatch fence must
        // remain exactly consumable even if the ephemeral Host carrier is torn
        // down before the new turn observes the Wake. explicit_activation never
        // requires Host capability; mcp_app may lose its View after prepare.
        if !endpoint.wake_capable
            && !matches!(adapter_kind.as_str(), "explicit_activation" | "mcp_app")
        {
            return Err(CommunicationStoreError::new(
                "endpoint_not_wake_capable",
                "Agent Endpoint is not wake-capable",
            ));
        }
        if expected_token_hash != digest_text("webcodex.agent-wake.consume-token.v1", consume_token)
        {
            return Err(CommunicationStoreError::new(
                "wake_consume_token_mismatch",
                "consume_token does not identify the exact Agent Wake continuation",
            ));
        }
        if wake.state == AgentWakeState::Consumed {
            if wake.consumed_by_endpoint_id.as_deref() != Some(endpoint_id)
                || wake.consumed_controller_generation != Some(expected_controller_generation)
            {
                return Err(CommunicationStoreError::new(
                    "wake_endpoint_fence_mismatch",
                    "Consumed Agent Wake is bound to a different Endpoint generation",
                ));
            }
            if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
                verify_agent_wait_resumed_for_consumed_wake(
                    &transaction,
                    principal,
                    wake.source_wait_id.as_deref(),
                    agent_id,
                )?;
            }
            let consumed_at = wake
                .consumed_at_unix_ms
                .expect("consumed Wake has timestamp");
            transaction.commit().map_err(store_error)?;
            return Ok(AgentWakeConsumeResult {
                wake_id: wake.wake_id,
                target_agent_id: wake.target_agent_id,
                state: AgentWakeState::Consumed,
                already_consumed: true,
                consumed_at_unix_ms: consumed_at,
                state_changed: false,
            });
        }
        if !matches!(
            wake.state,
            AgentWakeState::Prepared | AgentWakeState::Delivered | AgentWakeState::DeliveryUnknown
        ) {
            return Err(CommunicationStoreError::new(
                "wake_not_dispatched",
                "Agent Wake can only be consumed after the dispatch fence",
            ));
        }

        // The first exact Task-origin consume is the model-turn takeover boundary.
        // Promote only the exact still-current, active, unexpired A4b Attempt whose
        // durable endpoint execution remains bound to this Wake/Endpoint generation.
        // A zero-row match is intentionally not an error: a post-dispatch Wake stays
        // consumable after its source Attempt becomes stale, but stale work is never
        // revived or granted a fresh execution lease. Replays return above and never
        // slide this lease again.
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
            if let (Some(task_id), Some(task_attempt_id)) = (
                wake.source_task_id.as_deref(),
                wake.source_task_attempt_id.as_deref(),
            ) {
                let takeover_lease_expires_at =
                    now.saturating_add(AGENT_TASK_ENDPOINT_TAKEOVER_LEASE_MS);
                let promoted = transaction
                    .execute(
                        "UPDATE wc_agent_task_attempts
                         SET lease_expires_at_unix_ms = MAX(lease_expires_at_unix_ms, ?4)
                         WHERE attempt_id = ?1 AND task_id = ?2
                           AND assignee_agent_id = ?3 AND state = 'active'
                           AND lease_expires_at_unix_ms > ?5
                           AND EXISTS (
                               SELECT 1
                               FROM wc_agent_tasks t
                               JOIN wc_agent_task_endpoint_executions e
                                 ON e.task_id = t.task_id AND e.attempt_id = ?1
                               WHERE t.task_id = ?2
                                 AND t.owner_principal_kind = ?6
                                 AND t.owner_principal_digest = ?7
                                 AND t.latest_attempt_id = ?1
                                 AND t.assignee_agent_id = ?3
                                 AND t.state = 'active'
                                 AND e.wake_id = ?8
                                 AND e.endpoint_id = ?9
                                 AND e.endpoint_controller_generation = ?10
                           )",
                        params![
                            task_attempt_id,
                            task_id,
                            agent_id,
                            takeover_lease_expires_at,
                            now,
                            principal.kind,
                            principal.digest,
                            wake_id,
                            endpoint_id,
                            expected_controller_generation,
                        ],
                    )
                    .map_err(store_error)?;
                if promoted == 1 {
                    transaction
                        .execute(
                            "UPDATE wc_agent_tasks
                             SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                             WHERE task_id = ?1",
                            params![task_id, now],
                        )
                        .map_err(store_error)?;
                }
            }
        }

        if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
            resume_agent_wait_for_wake_in_transaction(
                &transaction,
                principal,
                wake.source_wait_id.as_deref(),
                agent_id,
                now,
            )?;
        }

        transaction
            .execute(
                "UPDATE wc_agent_wake_attempts
                 SET state = 'consumed', consumed_at_unix_ms = COALESCE(consumed_at_unix_ms, ?2)
                 WHERE attempt_id = ?1",
                params![wake_attempt_id, now],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'consumed', revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                     consumed_at_unix_ms = ?2, consumed_by_endpoint_id = ?3,
                     consumed_controller_generation = ?4,
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE wake_id = ?1 AND state IN ('prepared', 'delivered', 'delivery_unknown')",
                params![wake_id, now, endpoint_id, expected_controller_generation,],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_endpoints
                 SET last_seen_at_unix_ms = MAX(last_seen_at_unix_ms, ?2)
                 WHERE endpoint_id = ?1",
                params![endpoint_id, now],
            )
            .map_err(store_error)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentWakeConsumeResult {
            wake_id: wake_id.to_string(),
            target_agent_id: agent_id.to_string(),
            state: AgentWakeState::Consumed,
            already_consumed: false,
            consumed_at_unix_ms: now,
            state_changed: true,
        })
    }

    /// Return a bounded, authoritative Agent-turn bootstrap without copying
    /// transcript bodies, Inbox message bodies, raw fences/tokens, or principal
    /// identity into ambient Host state.
    pub fn bootstrap_agent_conversation(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        selected_conversation_id: Option<&str>,
        wake_id: Option<&str>,
    ) -> Result<AgentConversationBootstrapRecord, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
        validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
        if let Some(conversation_id) = selected_conversation_id {
            validate_id(
                conversation_id,
                CONVERSATION_ID_PREFIX,
                "invalid_conversation_id",
            )?;
        }
        if let Some(wake_id) = wake_id {
            validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
        }

        let conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let endpoint = require_current_endpoint(
            &conn,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        let acting_agent = load_agent(&conn, agent_id)?.ok_or_else(|| {
            CommunicationStoreError::new("agent_not_found", "Agent identity does not exist")
        })?;

        let wake = if let Some(wake_id) = wake_id {
            let wake = load_wake(&conn, wake_id)?
                .filter(|wake| wake.target_agent_id == agent_id)
                .ok_or_else(|| {
                    CommunicationStoreError::new("wake_not_found", "Agent Wake does not exist")
                })?;
            (!matches!(
                wake.state,
                AgentWakeState::Consumed | AgentWakeState::Retired
            ))
            .then_some(wake)
        } else {
            let selected_wake_id: Option<String> = conn
                .query_row(
                    "SELECT wake_id FROM wc_agent_wakes
                     WHERE target_agent_id = ?1 AND state NOT IN ('consumed', 'retired')
                     ORDER BY CASE
                         WHEN state IN ('prepared', 'delivered', 'delivery_unknown') THEN 0
                         WHEN state = 'claimed' THEN 1
                         ELSE 2
                     END, created_at_unix_ms, wake_id
                     LIMIT 1",
                    params![agent_id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(store_error)?;
            selected_wake_id
                .as_deref()
                .map(|wake_id| load_wake(&conn, wake_id))
                .transpose()?
                .flatten()
        };

        let effective_conversation_id =
            selected_conversation_id.map(ToOwned::to_owned).or_else(|| {
                wake.as_ref()
                    .and_then(|wake| wake.latest_conversation_id.clone())
            });
        let selected_conversation = effective_conversation_id
            .as_deref()
            .map(|conversation_id| {
                read_conversation_in_connection(
                    &conn,
                    principal,
                    &ConversationAccess::Agent {
                        agent_id: agent_id.to_string(),
                        endpoint_id: endpoint_id.to_string(),
                        expected_controller_generation,
                    },
                    conversation_id,
                    0,
                    0,
                )
                .map(|detail| detail.conversation)
            })
            .transpose()?;
        let (queued_delivery_count, inbox_high_watermark): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*), COALESCE(MAX(delivery_order), 0)
                 FROM wc_agent_deliveries
                 WHERE recipient_agent_id = ?1 AND state = 'queued'",
                params![agent_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(store_error)?;
        let wake = wake
            .map(|wake| {
                let attention_event = if wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
                    Some(require_agent_attention_event_for_wake(
                        &conn,
                        principal,
                        wake.source_event_id.as_deref(),
                        agent_id,
                    )?)
                } else {
                    None
                };
                let wait_snapshot = if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
                    Some(require_agent_wait_for_wake(
                        &conn,
                        principal,
                        wake.source_wait_id.as_deref(),
                        agent_id,
                    )?)
                } else {
                    None
                };
                Ok::<_, CommunicationStoreError>(AgentWakeBootstrapSummary {
                    wake_id: wake.wake_id,
                    state: wake.state,
                    revision: wake.revision,
                    trigger_kind: wake.trigger_kind,
                    conversation_id: wake.latest_conversation_id,
                    latest_message_id: wake.latest_message_id,
                    queued_delivery_count: wake.queued_delivery_count_snapshot,
                    inbox_high_watermark: wake.inbox_high_watermark,
                    task_id: attention_event
                        .as_ref()
                        .and_then(|event| event.task_id().map(str::to_string))
                        .or(wake.source_task_id),
                    task_attempt_id: attention_event
                        .as_ref()
                        .and_then(|event| event.task_attempt_id().map(str::to_string))
                        .or(wake.source_task_attempt_id),
                    event_id: attention_event.as_ref().map(|event| event.event_id.clone()),
                    attention_kind: attention_event.as_ref().map(|event| event.kind.clone()),
                    workflow_session_id: attention_event
                        .as_ref()
                        .and_then(|event| event.workflow_session_id().map(str::to_string)),
                    goal_id: attention_event.map(|event| event.goal_id),
                    wait_id: wait_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.wait_id.clone()),
                    wait_match_count: wake.wait_match_count_snapshot,
                    wait_match_sequence: wake.wait_match_sequence_snapshot,
                })
            })
            .transpose()?;

        Ok(AgentConversationBootstrapRecord {
            acting_agent,
            endpoint,
            selected_conversation,
            inbox: AgentInboxBootstrapSummary {
                queued_delivery_count,
                inbox_high_watermark,
            },
            wake,
        })
    }

    /// Accept one pending Wake into an already-active explicit model turn.
    /// Unlike a continuation adapter this does not request a new turn. The
    /// caller key makes both the durable Attempt and returned consume token
    /// exactly recoverable if the tool response is lost.
    pub fn accept_explicit_agent_wake_activation(
        &self,
        principal: &CommunicationPrincipal,
        agent_id: &str,
        endpoint_id: &str,
        expected_controller_generation: i64,
        wake_id: &str,
        activation_idempotency_key: &str,
    ) -> Result<AgentWakeExplicitActivation, CommunicationStoreError> {
        const OP_EXPLICIT_ACTIVATION: &str = "accept_explicit_agent_wake_activation";
        validate_communication_principal(principal)?;
        validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
        validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
        validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
        let activation_idempotency_key = validate_idempotency_key(activation_idempotency_key)?;
        let request_hash = digest_text(
            "webcodex.agent-wake.explicit-activation-request.v1",
            &format!("{agent_id}\0{endpoint_id}\0{expected_controller_generation}\0{wake_id}"),
        );
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        if let Some(receipt) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_EXPLICIT_ACTIVATION,
            &activation_idempotency_key,
            &request_hash,
        )? {
            let (attempt_id, consume_token): (String, String) = serde_json::from_str(&receipt)
                .map_err(|_| {
                    CommunicationStoreError::new(
                        "invalid_activation_receipt",
                        "Invalid explicit activation replay state",
                    )
                })?;
            validate_id(
                &attempt_id,
                AGENT_WAKE_ATTEMPT_ID_PREFIX,
                "invalid_attempt_id",
            )?;
            validate_proof(
                &consume_token,
                AGENT_WAKE_CONSUME_TOKEN_PREFIX,
                "invalid_consume_token",
            )?;
            let attempt = load_attempt(&transaction, &attempt_id)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "wake_attempt_not_found",
                    "Explicit Agent Wake activation no longer exists",
                )
            })?;
            let expected_hash: String = transaction
                .query_row(
                    "SELECT consume_token_hash FROM wc_agent_wake_attempts WHERE attempt_id = ?1",
                    [&attempt_id],
                    |row| row.get(0),
                )
                .map_err(store_error)?;
            if attempt.wake_id != wake_id
                || attempt.endpoint_id != endpoint_id
                || attempt.controller_generation != expected_controller_generation
                || expected_hash
                    != digest_text("webcodex.agent-wake.consume-token.v1", &consume_token)
            {
                return Err(CommunicationStoreError::new(
                    "invalid_activation_receipt",
                    "Explicit activation receipt does not match its attempt",
                ));
            }
            let wake = load_wake(&transaction, wake_id)?.ok_or_else(|| {
                CommunicationStoreError::new("wake_not_found", "Agent Wake does not exist")
            })?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentWakeExplicitActivation {
                wake,
                attempt_id: attempt.attempt_id,
                consume_token,
                replayed: true,
                state_changed: false,
            });
        }
        require_current_endpoint(
            &transaction,
            principal,
            agent_id,
            endpoint_id,
            Some(expected_controller_generation),
        )?;
        let wake = load_wake(&transaction, wake_id)?
            .filter(|wake| wake.target_agent_id == agent_id)
            .ok_or_else(|| {
                CommunicationStoreError::new("wake_not_found", "Agent Wake does not exist")
            })?;
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
            return Err(CommunicationStoreError::new(
                "agent_task_wake_requires_endpoint_dispatch",
                "AgentTask-origin Wake must be claimed through the continuation Endpoint carrier path",
            ));
        }
        if wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
            return Err(CommunicationStoreError::new(
                "attention_wake_requires_endpoint_dispatch",
                "Attention-event Wake must be claimed through the continuation Endpoint carrier path",
            ));
        }
        if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
            return Err(CommunicationStoreError::new(
                "agent_wait_wake_requires_endpoint_dispatch",
                "AgentWait-origin Wake must be claimed through the continuation Endpoint carrier path",
            ));
        }
        if wake.state != AgentWakeState::Pending {
            return Err(CommunicationStoreError::new(
                "wake_not_pending",
                "Explicit activation can accept only a pending Agent Wake",
            ));
        }
        let dispatched_exists: bool = transaction
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM wc_agent_wakes
                    WHERE target_agent_id = ?1
                      AND state IN ('prepared', 'delivered', 'delivery_unknown')
                 )",
                params![agent_id],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if dispatched_exists {
            return Err(CommunicationStoreError::new(
                "wake_dispatch_blocked",
                "Resolve the Agent's already-dispatched Wake before accepting another",
            ));
        }
        let attempt_id = allocate_identity(
            &transaction,
            AGENT_WAKE_ATTEMPT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_wake_attempts WHERE attempt_id = ?1)",
        )?;
        let consume_token = new_proof(AGENT_WAKE_CONSUME_TOKEN_PREFIX);
        let claim_fence = new_proof(AGENT_WAKE_CLAIM_FENCE_PREFIX);
        let claim_fence_hash = digest_text("webcodex.agent-wake.claim-fence.v1", &claim_fence);
        let consume_token_hash =
            digest_text("webcodex.agent-wake.consume-token.v1", &consume_token);
        transaction
            .execute(
                "INSERT INTO wc_agent_wake_attempts (
                    attempt_id, wake_id, endpoint_id, controller_generation,
                    adapter_kind, state, claim_fence_hash, consume_token_hash,
                    claimed_at_unix_ms, claim_lease_expires_at_unix_ms,
                    prepared_at_unix_ms, delivered_at_unix_ms,
                    delivery_unknown_at_unix_ms, revoked_at_unix_ms,
                    consumed_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, 'explicit_activation', 'delivered',
                           ?5, ?6, ?7, ?7, ?7, ?7, NULL, NULL, NULL)",
                params![
                    attempt_id,
                    wake_id,
                    endpoint_id,
                    expected_controller_generation,
                    claim_fence_hash,
                    consume_token_hash,
                    now,
                ],
            )
            .map_err(store_error)?;
        let changed = transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET state = 'delivered', revision = revision + 1,
                     updated_at_unix_ms = ?2, claimed_attempt_id = ?3,
                     claimed_endpoint_id = ?4, claimed_controller_generation = ?5,
                     claim_lease_expires_at_unix_ms = NULL
                 WHERE wake_id = ?1 AND state = 'pending'",
                params![
                    wake_id,
                    now,
                    attempt_id,
                    endpoint_id,
                    expected_controller_generation,
                ],
            )
            .map_err(store_error)?;
        if changed != 1 {
            return Err(CommunicationStoreError::new(
                "wake_activation_conflict",
                "Agent Wake changed before explicit activation was accepted",
            ));
        }
        // Persist this one exact random possession proof in the existing private
        // idempotency receipt so a lost response/restart returns the same proof.
        // Other delivery paths continue retaining only their proof hashes.
        record_idempotent_resource(
            &transaction,
            principal,
            OP_EXPLICIT_ACTIVATION,
            &activation_idempotency_key,
            &request_hash,
            &serde_json::to_string(&(&attempt_id, &consume_token))
                .expect("activation receipt serializes"),
            now,
        )?;
        let wake = load_wake(&transaction, wake_id)?.expect("activated Wake must exist");
        transaction.commit().map_err(store_error)?;
        Ok(AgentWakeExplicitActivation {
            wake,
            attempt_id,
            consume_token,
            replayed: false,
            state_changed: true,
        })
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub fn agent_wake(
        &self,
        wake_id: &str,
    ) -> Result<Option<AgentWakeRecord>, CommunicationStoreError> {
        validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
        let conn = self.lock_connection(crate::StoreDomain::AgentWake);
        load_wake(&conn, wake_id)
    }

    #[cfg(any(test, feature = "root-test-support"))]
    pub fn agent_wake_attempts(
        &self,
        wake_id: &str,
    ) -> Result<Vec<AgentWakeAttemptRecord>, CommunicationStoreError> {
        validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
        let conn = self.lock_connection(crate::StoreDomain::AgentWake);
        let mut statement = conn
            .prepare(
                "SELECT attempt_id, wake_id, endpoint_id, controller_generation,
                        adapter_kind, state, claimed_at_unix_ms,
                        claim_lease_expires_at_unix_ms, prepared_at_unix_ms,
                        delivered_at_unix_ms, delivery_unknown_at_unix_ms,
                        revoked_at_unix_ms, consumed_at_unix_ms
                 FROM wc_agent_wake_attempts WHERE wake_id = ?1
                 ORDER BY claimed_at_unix_ms, attempt_id",
            )
            .map_err(store_error)?;
        let attempts = statement
            .query_map(params![wake_id], row_to_attempt)
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        Ok(attempts)
    }
}

pub(super) fn coalesce_agent_wake_for_delivery(
    transaction: &Transaction<'_>,
    target_agent_id: &str,
    delivery_id: &str,
    conversation_id: &str,
    message_id: &str,
    inbox_high_watermark: i64,
    now: i64,
) -> Result<String, CommunicationStoreError> {
    let queued_delivery_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM wc_agent_deliveries
             WHERE recipient_agent_id = ?1 AND state = 'queued'",
            params![target_agent_id],
            |row| row.get(0),
        )
        .map_err(store_error)?;
    let existing: Option<String> = transaction
        .query_row(
            "SELECT wake_id FROM wc_agent_wakes
             WHERE target_agent_id = ?1 AND trigger_kind = 'inbox_changed'
               AND state IN ('pending', 'claimed')
             ORDER BY created_at_unix_ms, wake_id LIMIT 1",
            params![target_agent_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(store_error)?;
    if let Some(wake_id) = existing {
        transaction
            .execute(
                "UPDATE wc_agent_wakes
                 SET latest_triggering_delivery_id = ?2,
                     latest_conversation_id = ?3, latest_message_id = ?4,
                     inbox_high_watermark = MAX(inbox_high_watermark, ?5),
                     queued_delivery_count_snapshot = ?6,
                     revision = revision + 1,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?7)
                 WHERE wake_id = ?1 AND state IN ('pending', 'claimed')",
                params![
                    wake_id,
                    delivery_id,
                    conversation_id,
                    message_id,
                    inbox_high_watermark,
                    queued_delivery_count,
                    now,
                ],
            )
            .map_err(store_error)?;
        return Ok(wake_id);
    }
    let wake_id = allocate_identity(
        &transaction,
        AGENT_WAKE_ID_PREFIX,
        "SELECT EXISTS(SELECT 1 FROM wc_agent_wakes WHERE wake_id = ?1)",
    )?;
    transaction
        .execute(
            "INSERT INTO wc_agent_wakes (
                wake_id, target_agent_id, trigger_kind,
                first_triggering_delivery_id, latest_triggering_delivery_id,
                latest_conversation_id, latest_message_id,
                inbox_high_watermark, queued_delivery_count_snapshot,
                source_task_id, source_task_attempt_id, source_event_id,
                state, revision, created_at_unix_ms, updated_at_unix_ms,
                claimed_attempt_id, claimed_endpoint_id,
                claimed_controller_generation, claim_lease_expires_at_unix_ms,
                consumed_at_unix_ms, consumed_by_endpoint_id,
                consumed_controller_generation
             ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, NULL, NULL, NULL,
                       'pending', 1, ?9, ?9, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
            params![
                wake_id,
                target_agent_id,
                WAKE_TRIGGER_INBOX_CHANGED,
                delivery_id,
                conversation_id,
                message_id,
                inbox_high_watermark,
                queued_delivery_count,
                now,
            ],
        )
        .map_err(store_error)?;
    Ok(wake_id)
}

fn clear_agent_task_wake_carrier(
    transaction: &Transaction<'_>,
    wake_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE wake_id = ?1",
            params![wake_id, now],
        )
        .map_err(store_error)?;
    Ok(())
}

fn materialize_agent_task_wake_expiry(
    transaction: &Transaction<'_>,
    wake: &AgentWakeRecord,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let Some(task_id) = wake.source_task_id.as_deref() else {
        return Ok(());
    };
    let Some(task_attempt_id) = wake.source_task_attempt_id.as_deref() else {
        return Ok(());
    };
    transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET state = 'expired', terminal_at_unix_ms = COALESCE(terminal_at_unix_ms, ?2)
             WHERE attempt_id = ?1 AND state = 'active' AND lease_expires_at_unix_ms <= ?2",
            params![task_attempt_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_tasks
             SET state = 'ready', updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE task_id = ?1 AND state = 'active' AND latest_attempt_id = ?3
               AND EXISTS (
                   SELECT 1 FROM wc_agent_task_attempts a
                   WHERE a.attempt_id = ?3 AND a.state = 'expired'
               )
               AND NOT EXISTS (
                   SELECT 1 FROM wc_agent_task_coding_runs r
                   WHERE r.attempt_id = ?3 AND r.dispatch_state IN ('outcome_unknown', 'bound')
               )",
            params![task_id, now, task_attempt_id],
        )
        .map_err(store_error)?;
    Ok(())
}

fn retire_agent_task_wake_pre_dispatch(
    transaction: &Transaction<'_>,
    wake_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let wake = load_wake(transaction, wake_id)?.ok_or_else(|| {
        CommunicationStoreError::new("wake_not_found", "Agent Task Wake does not exist")
    })?;
    if wake.trigger_kind != WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
        return Ok(());
    }
    materialize_agent_task_wake_expiry(transaction, &wake, now)?;
    transaction
        .execute(
            "UPDATE wc_agent_wake_attempts
             SET state = 'revoked', revoked_at_unix_ms = COALESCE(revoked_at_unix_ms, ?2)
             WHERE wake_id = ?1 AND state = 'claimed'",
            params![wake_id, now],
        )
        .map_err(store_error)?;
    clear_agent_task_wake_carrier(transaction, wake_id, now)?;
    transaction
        .execute(
            "UPDATE wc_agent_wakes
             SET state = 'retired', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                 claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                 claimed_controller_generation = NULL,
                 claim_lease_expires_at_unix_ms = NULL
             WHERE wake_id = ?1 AND state IN ('pending', 'claimed')",
            params![wake_id, now],
        )
        .map_err(store_error)?;
    Ok(())
}

fn retire_stale_agent_task_wakes_for_agent(
    transaction: &Transaction<'_>,
    agent_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let stale_wake_ids = {
        let mut statement = transaction
            .prepare(
                "SELECT w.wake_id
                 FROM wc_agent_wakes w
                 WHERE w.target_agent_id = ?1
                   AND w.trigger_kind = 'agent_task_attempt'
                   AND w.state IN ('pending', 'claimed')
                   AND NOT EXISTS (
                       SELECT 1
                       FROM wc_agent_tasks t
                       JOIN wc_agent_task_attempts a ON a.attempt_id = w.source_task_attempt_id
                       WHERE t.task_id = w.source_task_id
                         AND t.latest_attempt_id = a.attempt_id
                         AND t.assignee_agent_id = w.target_agent_id
                         AND a.assignee_agent_id = w.target_agent_id
                         AND t.state = 'active'
                         AND a.state = 'active'
                         AND a.lease_expires_at_unix_ms > ?2
                   )",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(params![agent_id, now], |row| row.get::<_, String>(0))
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        rows
    };
    for wake_id in stale_wake_ids {
        retire_agent_task_wake_pre_dispatch(transaction, &wake_id, now)?;
    }
    Ok(())
}

fn bind_agent_task_wake_carrier(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wake: &AgentWakeRecord,
    endpoint_id: &str,
    endpoint_controller_generation: i64,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let task_id = wake.source_task_id.as_deref().ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Agent Task Wake is missing source_task_id",
        )
    })?;
    let task_attempt_id = wake.source_task_attempt_id.as_deref().ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Agent Task Wake is missing source_task_attempt_id",
        )
    })?;
    let (attempt_fence, attempt_controller_generation): (String, i64) = transaction
        .query_row(
            "SELECT attempt_fence, attempt_controller_generation
             FROM wc_agent_task_attempts WHERE attempt_id = ?1 AND task_id = ?2",
            params![task_attempt_id, task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(store_error)?;
    replace_agent_task_attempt_controller_in_transaction(
        transaction,
        principal,
        task_id,
        task_attempt_id,
        &wake.target_agent_id,
        &attempt_fence,
        attempt_controller_generation,
        now,
    )?;
    let changed = transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = ?4, endpoint_controller_generation = ?5,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?6)
             WHERE task_id = ?1 AND attempt_id = ?2 AND wake_id = ?3",
            params![
                task_id,
                task_attempt_id,
                wake.wake_id,
                endpoint_id,
                endpoint_controller_generation,
                now,
            ],
        )
        .map_err(store_error)?;
    if changed != 1 {
        return Err(CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Agent Task Wake has no matching durable Endpoint execution binding",
        ));
    }
    Ok(())
}

fn reserve_agent_task_endpoint_dispatch_grace_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wake: &AgentWakeRecord,
    endpoint_id: &str,
    endpoint_controller_generation: i64,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    if wake.trigger_kind != WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
        return Ok(());
    }
    let task_id = wake.source_task_id.as_deref().ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Agent Task Wake is missing source_task_id",
        )
    })?;
    let task_attempt_id = wake.source_task_attempt_id.as_deref().ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Agent Task Wake is missing source_task_attempt_id",
        )
    })?;
    let grace_expires_at = now.saturating_add(AGENT_TASK_ENDPOINT_DISPATCH_GRACE_MS);
    let extended = transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET lease_expires_at_unix_ms = MAX(lease_expires_at_unix_ms, ?4)
             WHERE attempt_id = ?1 AND task_id = ?2
               AND assignee_agent_id = ?3 AND state = 'active'
               AND lease_expires_at_unix_ms > ?5
               AND EXISTS (
                   SELECT 1
                   FROM wc_agent_tasks t
                   JOIN wc_agent_task_endpoint_executions e
                     ON e.task_id = t.task_id AND e.attempt_id = ?1
                   WHERE t.task_id = ?2
                     AND t.owner_principal_kind = ?6
                     AND t.owner_principal_digest = ?7
                     AND t.latest_attempt_id = ?1
                     AND t.assignee_agent_id = ?3
                     AND t.state = 'active'
                     AND e.wake_id = ?8
                     AND e.endpoint_id = ?9
                     AND e.endpoint_controller_generation = ?10
               )",
            params![
                task_attempt_id,
                task_id,
                wake.target_agent_id,
                grace_expires_at,
                now,
                principal.kind,
                principal.digest,
                wake.wake_id,
                endpoint_id,
                endpoint_controller_generation,
            ],
        )
        .map_err(store_error)?;
    if extended == 1 {
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                 WHERE task_id = ?1",
                params![task_id, now],
            )
            .map_err(store_error)?;
    }
    Ok(())
}

fn reserve_prepared_agent_task_dispatch_graces_for_server_takeover(
    transaction: &Transaction<'_>,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let rows = {
        let mut statement = transaction
            .prepare(
                "SELECT w.wake_id, w.claimed_endpoint_id, w.claimed_controller_generation,
                        t.owner_principal_kind, t.owner_principal_digest
                 FROM wc_agent_wakes w
                 JOIN wc_agent_tasks t ON t.task_id = w.source_task_id
                 WHERE w.trigger_kind = 'agent_task_attempt' AND w.state = 'prepared'
                   AND w.claimed_endpoint_id IS NOT NULL
                   AND w.claimed_controller_generation IS NOT NULL",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        rows
    };
    for (wake_id, endpoint_id, generation, principal_kind, principal_digest) in rows {
        let wake = load_wake(transaction, &wake_id)?.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_wake_invariant",
                "Prepared Agent Task Wake disappeared during Server takeover recovery",
            )
        })?;
        let principal = CommunicationPrincipal {
            kind: principal_kind,
            digest: principal_digest,
        };
        reserve_agent_task_endpoint_dispatch_grace_in_transaction(
            transaction,
            &principal,
            &wake,
            &endpoint_id,
            generation,
            now,
        )?;
    }
    Ok(())
}

fn reserve_prepared_agent_task_dispatch_grace_for_endpoint_loss(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let row = transaction
        .query_row(
            "SELECT w.wake_id, t.owner_principal_kind, t.owner_principal_digest
             FROM wc_agent_wakes w
             JOIN wc_agent_tasks t ON t.task_id = w.source_task_id
             WHERE w.target_agent_id = ?1 AND w.trigger_kind = 'agent_task_attempt'
               AND w.state = 'prepared' AND w.claimed_endpoint_id = ?2
               AND w.claimed_controller_generation = ?3",
            params![agent_id, endpoint_id, controller_generation],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?;
    let Some((wake_id, principal_kind, principal_digest)) = row else {
        return Ok(());
    };
    let wake = load_wake(transaction, &wake_id)?.ok_or_else(|| {
        CommunicationStoreError::new(
            "agent_task_wake_invariant",
            "Prepared Agent Task Wake disappeared during Host binding loss recovery",
        )
    })?;
    let principal = CommunicationPrincipal {
        kind: principal_kind,
        digest: principal_digest,
    };
    reserve_agent_task_endpoint_dispatch_grace_in_transaction(
        transaction,
        &principal,
        &wake,
        endpoint_id,
        controller_generation,
        now,
    )
}

fn agent_task_wake_is_dispatchable(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wake: &AgentWakeRecord,
    endpoint_id: &str,
    endpoint_controller_generation: i64,
    now: i64,
) -> Result<bool, CommunicationStoreError> {
    if wake.trigger_kind != WAKE_TRIGGER_AGENT_TASK_ATTEMPT {
        return Ok(true);
    }
    let dispatchable: bool = transaction
        .query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM wc_agent_tasks t
                JOIN wc_agent_task_attempts a ON a.task_id = t.task_id
                JOIN wc_agent_task_endpoint_executions e
                  ON e.task_id = t.task_id AND e.attempt_id = a.attempt_id
                WHERE t.task_id = ?1 AND a.attempt_id = ?2
                  AND t.owner_principal_kind = ?3 AND t.owner_principal_digest = ?4
                  AND t.latest_attempt_id = a.attempt_id
                  AND t.assignee_agent_id = ?5 AND a.assignee_agent_id = ?5
                  AND t.state = 'active' AND a.state = 'active'
                  AND a.lease_expires_at_unix_ms > ?6
                  AND e.wake_id = ?7
                  AND e.endpoint_id = ?8 AND e.endpoint_controller_generation = ?9
             )",
            params![
                wake.source_task_id,
                wake.source_task_attempt_id,
                principal.kind,
                principal.digest,
                wake.target_agent_id,
                now,
                wake.wake_id,
                endpoint_id,
                endpoint_controller_generation,
            ],
            |row| row.get(0),
        )
        .map_err(store_error)?;
    Ok(dispatchable)
}

fn fence_agent_task_controllers_for_endpoint_loss(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    endpoint_controller_generation: i64,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let controllers = {
        let mut statement = transaction
            .prepare(
                "SELECT t.owner_principal_kind, t.owner_principal_digest,
                        e.task_id, e.attempt_id, a.assignee_agent_id,
                        a.attempt_fence, a.attempt_controller_generation
                 FROM wc_agent_task_endpoint_executions e
                 JOIN wc_agent_wakes w ON w.wake_id = e.wake_id
                 JOIN wc_agent_tasks t ON t.task_id = e.task_id
                 JOIN wc_agent_task_attempts a
                   ON a.task_id = e.task_id AND a.attempt_id = e.attempt_id
                 WHERE e.endpoint_id = ?1 AND e.endpoint_controller_generation = ?2
                   AND w.target_agent_id = ?3 AND w.trigger_kind = 'agent_task_attempt'
                   AND t.latest_attempt_id = a.attempt_id AND t.state = 'active'
                   AND a.assignee_agent_id = ?3 AND a.state = 'active'
                   AND a.lease_expires_at_unix_ms > ?4",
            )
            .map_err(store_error)?;
        let rows = statement
            .query_map(
                params![endpoint_id, endpoint_controller_generation, agent_id, now],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        rows
    };

    for (
        principal_kind,
        principal_digest,
        task_id,
        attempt_id,
        assignee_agent_id,
        attempt_fence,
        attempt_controller_generation,
    ) in controllers
    {
        let principal = CommunicationPrincipal {
            kind: principal_kind,
            digest: principal_digest,
        };
        replace_agent_task_attempt_controller_in_transaction(
            transaction,
            &principal,
            &task_id,
            &attempt_id,
            &assignee_agent_id,
            &attempt_fence,
            attempt_controller_generation,
            now,
        )?;
    }
    Ok(())
}

pub(super) fn reconcile_wakes_for_endpoint_loss(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
    now: i64,
    fence_task_controller: bool,
) -> Result<(), CommunicationStoreError> {
    if !fence_task_controller {
        reserve_prepared_agent_task_dispatch_grace_for_endpoint_loss(
            transaction,
            agent_id,
            endpoint_id,
            controller_generation,
            now,
        )?;
    }
    if fence_task_controller {
        fence_agent_task_controllers_for_endpoint_loss(
            transaction,
            agent_id,
            endpoint_id,
            controller_generation,
            now,
        )?;
    }
    transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
             WHERE endpoint_id = ?1 AND endpoint_controller_generation = ?2
               AND wake_id IN (
                   SELECT wake_id FROM wc_agent_wakes
                   WHERE target_agent_id = ?3 AND trigger_kind = 'agent_task_attempt'
                     AND (?5 != 0 OR state = 'claimed')
               )",
            params![
                endpoint_id,
                controller_generation,
                agent_id,
                now,
                fence_task_controller as i64,
            ],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wake_attempts
             SET state = 'revoked', revoked_at_unix_ms = COALESCE(revoked_at_unix_ms, ?4)
             WHERE endpoint_id = ?1 AND controller_generation = ?2 AND state = 'claimed'
               AND wake_id IN (SELECT wake_id FROM wc_agent_wakes WHERE target_agent_id = ?3)",
            params![endpoint_id, controller_generation, agent_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wakes
             SET state = 'pending', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4),
                 claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                 claimed_controller_generation = NULL,
                 claim_lease_expires_at_unix_ms = NULL
             WHERE target_agent_id = ?1 AND state = 'claimed'
               AND claimed_endpoint_id = ?2 AND claimed_controller_generation = ?3",
            params![agent_id, endpoint_id, controller_generation, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wake_attempts
             SET state = 'delivery_unknown',
                 delivery_unknown_at_unix_ms = COALESCE(delivery_unknown_at_unix_ms, ?4)
             WHERE endpoint_id = ?1 AND controller_generation = ?2
               AND state IN ('prepared', 'delivered')
               AND wake_id IN (SELECT wake_id FROM wc_agent_wakes WHERE target_agent_id = ?3)",
            params![endpoint_id, controller_generation, agent_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wakes
             SET state = 'delivery_unknown', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?4),
                 claim_lease_expires_at_unix_ms = NULL
             WHERE target_agent_id = ?1 AND state IN ('prepared', 'delivered')
               AND claimed_endpoint_id = ?2 AND claimed_controller_generation = ?3",
            params![agent_id, endpoint_id, controller_generation, now],
        )
        .map_err(store_error)?;
    Ok(())
}

fn expire_stale_endpoints(
    transaction: &Transaction<'_>,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let endpoints = {
        let mut statement = transaction
            .prepare(
                "SELECT endpoint_id, agent_id, controller_generation
                 FROM wc_agent_endpoints
                 WHERE lifecycle = 'attached' AND lease_expires_at_unix_ms <= ?1",
            )
            .map_err(store_error)?;
        let endpoints = statement
            .query_map(params![now], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(store_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(store_error)?;
        endpoints
    };
    for (endpoint_id, agent_id, generation) in endpoints {
        transaction
            .execute(
                "UPDATE wc_agent_endpoints
                 SET lifecycle = 'expired', expired_at_unix_ms = COALESCE(expired_at_unix_ms, ?2),
                     wake_capable = 0,
                     mcp_app_recovery_fingerprint = NULL
                 WHERE endpoint_id = ?1 AND lifecycle = 'attached'",
                params![endpoint_id, now],
            )
            .map_err(store_error)?;
        reconcile_wakes_for_endpoint_loss(
            transaction,
            &agent_id,
            &endpoint_id,
            generation,
            now,
            true,
        )?;
    }
    Ok(())
}

fn release_expired_claims_for_agent(
    transaction: &Transaction<'_>,
    agent_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE wake_id IN (
                 SELECT wake_id FROM wc_agent_wakes
                 WHERE target_agent_id = ?1 AND state = 'claimed'
                   AND claim_lease_expires_at_unix_ms <= ?2
                   AND trigger_kind = 'agent_task_attempt'
             )",
            params![agent_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wake_attempts
             SET state = 'revoked', revoked_at_unix_ms = COALESCE(revoked_at_unix_ms, ?2)
             WHERE state = 'claimed'
               AND claim_lease_expires_at_unix_ms <= ?2
               AND wake_id IN (
                   SELECT wake_id FROM wc_agent_wakes WHERE target_agent_id = ?1
               )",
            params![agent_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wakes
             SET state = 'pending', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                 claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                 claimed_controller_generation = NULL,
                 claim_lease_expires_at_unix_ms = NULL
             WHERE target_agent_id = ?1 AND state = 'claimed'
               AND claim_lease_expires_at_unix_ms <= ?2",
            params![agent_id, now],
        )
        .map_err(store_error)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn require_exact_claim(
    transaction: &Transaction<'_>,
    agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
    wake_id: &str,
    attempt_id: &str,
    claim_fence: &str,
    consume_token: Option<&str>,
) -> Result<AgentWakeRecord, CommunicationStoreError> {
    validate_proof(
        claim_fence,
        AGENT_WAKE_CLAIM_FENCE_PREFIX,
        "invalid_wake_claim_fence",
    )?;
    let wake = load_wake(transaction, wake_id)?.ok_or_else(|| {
        CommunicationStoreError::new("wake_not_found", "Agent Wake does not exist")
    })?;
    if wake.target_agent_id != agent_id {
        return Err(CommunicationStoreError::new(
            "wake_agent_mismatch",
            "Agent Wake belongs to a different Agent",
        ));
    }
    if wake.claimed_attempt_id.as_deref() != Some(attempt_id)
        || wake.claimed_endpoint_id.as_deref() != Some(endpoint_id)
        || wake.claimed_controller_generation != Some(controller_generation)
    {
        return Err(CommunicationStoreError::new(
            "wake_claim_stale",
            "Agent Wake claim is stale or belongs to another Endpoint generation",
        ));
    }
    let hashes: Option<(String, String)> = transaction
        .query_row(
            "SELECT claim_fence_hash, consume_token_hash
             FROM wc_agent_wake_attempts
             WHERE attempt_id = ?1 AND wake_id = ?2 AND endpoint_id = ?3
               AND controller_generation = ?4",
            params![attempt_id, wake_id, endpoint_id, controller_generation],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(store_error)?;
    let Some((expected_claim_hash, expected_consume_hash)) = hashes else {
        return Err(CommunicationStoreError::new(
            "wake_attempt_not_found",
            "Agent Wake Attempt does not exist",
        ));
    };
    if expected_claim_hash != digest_text("webcodex.agent-wake.claim-fence.v1", claim_fence) {
        return Err(CommunicationStoreError::new(
            "wake_claim_stale",
            "Agent Wake claim fence is stale",
        ));
    }
    if let Some(consume_token) = consume_token {
        if expected_consume_hash
            != digest_text("webcodex.agent-wake.consume-token.v1", consume_token)
        {
            return Err(CommunicationStoreError::new(
                "wake_consume_token_mismatch",
                "consume_token does not identify the exact Agent Wake continuation",
            ));
        }
    }
    Ok(wake)
}

fn wake_envelope(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    wake: &AgentWakeRecord,
    endpoint_id: &str,
    controller_generation: i64,
    consume_token: &str,
) -> Result<AgentWakeEnvelope, CommunicationStoreError> {
    let (queued_delivery_count, inbox_high_watermark, resume_hint) = if wake.trigger_kind
        == WAKE_TRIGGER_AGENT_TASK_ATTEMPT
    {
        let task_id = wake.source_task_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_wake_invariant",
                "Agent Task Wake is missing source_task_id",
            )
        })?;
        let task_attempt_id = wake.source_task_attempt_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_wake_invariant",
                "Agent Task Wake is missing source_task_attempt_id",
            )
        })?;
        let (attempt_fence, attempt_controller_generation): (String, i64) = transaction
            .query_row(
                "SELECT attempt_fence, attempt_controller_generation
                     FROM wc_agent_task_attempts
                     WHERE task_id = ?1 AND attempt_id = ?2",
                params![task_id, task_attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(store_error)?;
        let resume_hint = format!(
                "WebCodex AgentTask continuation.\n\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\ntask_id={}\nattempt_id={}\nattempt_fence={}\nattempt_controller_generation={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation, consume it immediately with consume_agent_wake, then read_agent_task(task_id). Exact consume establishes the bounded 30-minute active-turn reservation only if this Attempt is still current. Work only if the Task still names this active Attempt with the same assignee/fence/generation; otherwise stop.\nComplete only this exact Attempt with complete_agent_task_attempt. Near reservation expiry, heartbeat_agent_task_attempt before expiry using this wake_id/consume_token as active-turn proof.\n",
                wake.target_agent_id,
                endpoint_id,
                controller_generation,
                wake.wake_id,
                consume_token,
                task_id,
                task_attempt_id,
                attempt_fence,
                attempt_controller_generation,
            );
        (0, 0, resume_hint)
    } else if wake.trigger_kind == WAKE_TRIGGER_ATTENTION_EVENT {
        let event = require_agent_attention_event_for_wake(
            transaction,
            principal,
            wake.source_event_id.as_deref(),
            &wake.target_agent_id,
        )?;
        let resume_hint = match &event.source {
            AgentAttentionSource::AgentTaskTerminal { task_id, task_attempt_id, terminal_task_state } => format!(
            "WebCodex Goal attention continuation.\n\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\nevent_id={}\ngoal_id={}\ntask_id={}\nattempt_id={}\nterminal_task_state={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation, then consume it immediately with consume_agent_wake. Call get_goal(goal_id) and read_agent_task(task_id), then make an explicit next Goal decision from authoritative state. Never rerun the terminal Task or reopen a terminal Goal.\n",
            wake.target_agent_id,
            endpoint_id,
            controller_generation,
            wake.wake_id,
            consume_token,
            event.event_id,
            event.goal_id,
            task_id,
            task_attempt_id,
            terminal_task_state.as_str(),
        ),
            AgentAttentionSource::GoalWorkflowStalled { workflow_session_id, .. } => format!(
                "WebCodex Goal workflow stall continuation.\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\ngoal_id={}\nsession_id={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation; immediately consume_agent_wake. Then get_goal(goal_id) and session_handoff_summary(session_id) for the exact correlated Workflow Session. Continue the latest checkpoint/current step using current authorized Job/Project state as needed. A vanished turn does not prove failure: never repeat an uncertain effect. Checkpoint with checkpoint_goal as work progresses; fresh verification/review precedes explicit update_goal completion. Stop if terminal or its controller changed.\n",
                wake.target_agent_id, endpoint_id, controller_generation, wake.wake_id,
                consume_token, event.goal_id, workflow_session_id,
            ),
        };
        (0, 0, resume_hint)
    } else if wake.trigger_kind == WAKE_TRIGGER_AGENT_WAIT_EVENTS {
        let wait = require_agent_wait_for_wake(
            transaction,
            principal,
            wake.source_wait_id.as_deref(),
            &wake.target_agent_id,
        )?;
        let match_count = wake.wait_match_count_snapshot.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_wait_wake_invariant",
                "Agent Wait Wake is missing its match-count snapshot",
            )
        })?;
        let match_sequence = wake.wait_match_sequence_snapshot.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_wait_wake_invariant",
                "Agent Wait Wake is missing its match-sequence snapshot",
            )
        })?;
        if wait.mode == AgentWaitMode::All
            && (match_count != wait.match_count
                || match_count != wait.source_count
                || match_sequence != wait.match_sequence)
        {
            return Err(CommunicationStoreError::new(
                "agent_wait_wake_invariant",
                "ALL Agent Wait Wake snapshot does not match its complete durable join",
            ));
        }
        let resume_hint = if let Some(goal_id) = wait.goal_id.as_deref() {
            format!(
                "WebCodex Goal-scoped AgentWait continuation.\n\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\nwait_id={}\ngoal_id={}\nmode={}\nmatched={}/{}\nmatch_sequence={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation, then consume it immediately with consume_agent_wake. Read this Wait with read_agent_wait(wait_id), read the exact Goal with get_goal(goal_id), re-read every authoritative source AgentTask, then explicitly decide and update the Goal from current durable state. This Wait is one-shot; create a new Wait if further waiting is needed.\n",
                wake.target_agent_id,
                endpoint_id,
                controller_generation,
                wake.wake_id,
                consume_token,
                wait.wait_id,
                goal_id,
                wait.mode.as_str(),
                match_count,
                wait.source_count,
                match_sequence,
            )
        } else {
            format!(
                "WebCodex AgentWait continuation.\n\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\nwait_id={}\nmode={}\nmatched={}/{}\nmatch_sequence={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation, then consume it immediately with consume_agent_wake. Read this Wait with read_agent_wait(wait_id), re-read the authoritative source AgentTasks, and decide the next action from current state. This Wait is one-shot; create a new Wait if further waiting is needed.\n",
                wake.target_agent_id,
                endpoint_id,
                controller_generation,
                wake.wake_id,
                consume_token,
                wait.wait_id,
                wait.mode.as_str(),
                match_count,
                wait.source_count,
                match_sequence,
            )
        };
        (0, 0, resume_hint)
    } else {
        let queued_delivery_count = wake.queued_delivery_count_snapshot.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_wake_invariant",
                "Inbox Agent Wake is missing queued_delivery_count_snapshot",
            )
        })?;
        let inbox_high_watermark = wake.inbox_high_watermark.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_wake_invariant",
                "Inbox Agent Wake is missing inbox_high_watermark",
            )
        })?;
        let resume_hint = format!(
                "WebCodex Agent Inbox continuation.\n\nagent_id={}\nendpoint_id={}\ncontroller_generation={}\nwake_id={}\nconsume_token={}\nqueued_delivery_count={}\ninbox_high_watermark={}\n\nBootstrap this exact Wake with bootstrap_agent_conversation. Re-read durable work with list_agent_inbox and read_conversation; no business Message body is embedded here. Process authoritative current deliveries, then consume this exact Wake with consume_agent_wake; consume processed Delivery ids separately. For replies keep wake_reply_id plus stable reply_operation_index. Re-authorize any non-communication capabilities through their ordinary workflow. Report the actual result or blocker.\n",
                wake.target_agent_id,
                endpoint_id,
                controller_generation,
                wake.wake_id,
                consume_token,
                queued_delivery_count,
                inbox_high_watermark,
            );
        (queued_delivery_count, inbox_high_watermark, resume_hint)
    };
    Ok(AgentWakeEnvelope {
        wake_id: wake.wake_id.clone(),
        agent_id: wake.target_agent_id.clone(),
        endpoint_id: endpoint_id.to_string(),
        controller_generation,
        queued_delivery_count,
        inbox_high_watermark,
        consume_token: consume_token.to_string(),
        resume_hint,
    })
}

fn load_wake(
    conn: &Connection,
    wake_id: &str,
) -> Result<Option<AgentWakeRecord>, CommunicationStoreError> {
    conn.query_row(
        "SELECT wake_id, target_agent_id, trigger_kind,
                first_triggering_delivery_id, latest_triggering_delivery_id,
                latest_conversation_id, latest_message_id,
                inbox_high_watermark, queued_delivery_count_snapshot,
                source_task_id, source_task_attempt_id, source_event_id,
                source_wait_id, wait_match_count_snapshot, wait_match_sequence_snapshot,
                state, revision, created_at_unix_ms, updated_at_unix_ms,
                claimed_attempt_id, claimed_endpoint_id,
                claimed_controller_generation, claim_lease_expires_at_unix_ms,
                consumed_at_unix_ms, consumed_by_endpoint_id,
                consumed_controller_generation
         FROM wc_agent_wakes WHERE wake_id = ?1",
        params![wake_id],
        row_to_wake,
    )
    .optional()
    .map_err(store_error)
}

fn row_to_wake(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentWakeRecord> {
    let state: String = row.get(15)?;
    Ok(AgentWakeRecord {
        wake_id: row.get(0)?,
        target_agent_id: row.get(1)?,
        trigger_kind: row.get(2)?,
        first_triggering_delivery_id: row.get(3)?,
        latest_triggering_delivery_id: row.get(4)?,
        latest_conversation_id: row.get(5)?,
        latest_message_id: row.get(6)?,
        inbox_high_watermark: row.get(7)?,
        queued_delivery_count_snapshot: row.get(8)?,
        source_task_id: row.get(9)?,
        source_task_attempt_id: row.get(10)?,
        source_event_id: row.get(11)?,
        source_wait_id: row.get(12)?,
        wait_match_count_snapshot: row.get(13)?,
        wait_match_sequence_snapshot: row.get(14)?,
        state: AgentWakeState::from_db(&state, 15)?,
        revision: row.get(16)?,
        created_at_unix_ms: row.get(17)?,
        updated_at_unix_ms: row.get(18)?,
        claimed_attempt_id: row.get(19)?,
        claimed_endpoint_id: row.get(20)?,
        claimed_controller_generation: row.get(21)?,
        claim_lease_expires_at_unix_ms: row.get(22)?,
        consumed_at_unix_ms: row.get(23)?,
        consumed_by_endpoint_id: row.get(24)?,
        consumed_controller_generation: row.get(25)?,
    })
}

fn load_attempt(
    conn: &Connection,
    attempt_id: &str,
) -> Result<Option<AgentWakeAttemptRecord>, CommunicationStoreError> {
    conn.query_row(
        "SELECT attempt_id, wake_id, endpoint_id, controller_generation,
                adapter_kind, state, claimed_at_unix_ms,
                claim_lease_expires_at_unix_ms, prepared_at_unix_ms,
                delivered_at_unix_ms, delivery_unknown_at_unix_ms,
                revoked_at_unix_ms, consumed_at_unix_ms
         FROM wc_agent_wake_attempts WHERE attempt_id = ?1",
        params![attempt_id],
        row_to_attempt,
    )
    .optional()
    .map_err(store_error)
}

fn row_to_attempt(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentWakeAttemptRecord> {
    let state: String = row.get(5)?;
    Ok(AgentWakeAttemptRecord {
        attempt_id: row.get(0)?,
        wake_id: row.get(1)?,
        endpoint_id: row.get(2)?,
        controller_generation: row.get(3)?,
        adapter_kind: row.get(4)?,
        state: AgentWakeAttemptState::from_db(&state, 5)?,
        claimed_at_unix_ms: row.get(6)?,
        claim_lease_expires_at_unix_ms: row.get(7)?,
        prepared_at_unix_ms: row.get(8)?,
        delivered_at_unix_ms: row.get(9)?,
        delivery_unknown_at_unix_ms: row.get(10)?,
        revoked_at_unix_ms: row.get(11)?,
        consumed_at_unix_ms: row.get(12)?,
    })
}

fn validate_adapter_kind(value: &str) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    if value.is_empty()
        || value.chars().count() > MAX_ADAPTER_KIND_CHARS
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | ':' | '.')
        })
    {
        return Err(CommunicationStoreError::new(
            "invalid_wake_adapter_kind",
            "Wake adapter kind is invalid",
        ));
    }
    Ok(value.to_string())
}

fn validate_wake_mutation_ids(
    agent_id: &str,
    endpoint_id: &str,
    wake_id: &str,
    attempt_id: &str,
) -> Result<(), CommunicationStoreError> {
    validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
    validate_id(endpoint_id, AGENT_ENDPOINT_ID_PREFIX, "invalid_endpoint_id")?;
    validate_id(wake_id, AGENT_WAKE_ID_PREFIX, "invalid_wake_id")?;
    validate_id(
        attempt_id,
        AGENT_WAKE_ATTEMPT_ID_PREFIX,
        "invalid_wake_attempt_id",
    )
}
