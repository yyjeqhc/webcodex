use super::agent_attention::create_agent_task_terminal_attention_in_transaction;
use super::agent_wait::record_agent_task_terminal_wait_matches_in_transaction;
use super::agent_wake::{
    AgentWakeState, AGENT_WAKE_CONSUME_TOKEN_PREFIX, AGENT_WAKE_ID_PREFIX,
    WAKE_TRIGGER_AGENT_TASK_ATTEMPT,
};
use super::communication::{
    allocate_identity, authorize_conversation_access, digest_json, digest_text,
    lookup_idempotent_resource, new_proof, now_unix_ms, record_idempotent_resource, store_error,
    validate_communication_principal, validate_id, validate_idempotency_key, validate_proof,
    CommunicationPrincipal, CommunicationStoreError, ConversationAccess, CONVERSATION_ID_PREFIX,
    CONVERSATION_MESSAGE_ID_PREFIX, DURABLE_AGENT_ID_PREFIX,
};
use super::Database;
use rusqlite::{
    params, types::Type, Connection, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::Serialize;
use serde_json::json;
use webcodex_core::coding_agent::{
    merge_coding_agent_run_snapshot, validate_coding_agent_run_snapshot, CodingAgentExecutionState,
    CodingAgentObservationMerge, CodingAgentRunSnapshot, CodingAgentRunState, CodingAgentTerminal,
};

pub(crate) const AGENT_TASK_ID_PREFIX: &str = "wc_agent_task_";
pub(crate) const AGENT_TASK_ATTEMPT_ID_PREFIX: &str = "wc_agent_task_attempt_";
pub(crate) const AGENT_TASK_ATTEMPT_FENCE_PREFIX: &str = "wc_agent_task_fence_";

pub(crate) const MAX_AGENT_TASK_TITLE_CHARS: usize = 200;
pub(crate) const MAX_AGENT_TASK_INSTRUCTION_BYTES: usize = 8_192;
pub const MAX_AGENT_TASK_TERMINAL_TEXT_BYTES: usize = 4_096;
pub(crate) const MAX_AGENT_TASK_PROJECT_REF_CHARS: usize = 256;
pub const MAX_AGENT_TASK_LIST_LIMIT: usize = 100;
pub(crate) const DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS: i64 = 60_000;
pub(crate) const AGENT_TASK_ENDPOINT_DISPATCH_GRACE_MS: i64 = 5 * 60_000;
pub(crate) const AGENT_TASK_ENDPOINT_TAKEOVER_LEASE_MS: i64 = 30 * 60_000;

const OP_CREATE_AGENT_TASK: &str = "create_agent_task";
const OP_START_AGENT_TASK_ATTEMPT: &str = "start_agent_task_attempt";
const OP_COMPLETE_AGENT_TASK_ATTEMPT: &str = "complete_agent_task_attempt";

include!("agent_task/models.rs");
include!("agent_task/operations.rs");
include!("agent_task/invariants.rs");
