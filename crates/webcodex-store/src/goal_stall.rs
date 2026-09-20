//! A narrow Goal-workflow inactivity attention source. This module has no timer,
//! scanner, execution authority, or Host transport. Runtime supplies an already
//! authorized Session/Window context; this transaction rechecks durable evidence.
use super::agent_attention::{
    insert_attention_wake_in_transaction, require_owned_attention_target,
    AgentAttentionEventRecord, AgentAttentionSource, AGENT_ATTENTION_EVENT_ID_PREFIX,
    AGENT_ATTENTION_EVENT_KIND_GOAL_WORKFLOW_STALLED,
};
use super::agent_wake::AgentWakeState;
use super::communication::{
    allocate_identity, store_error, validate_communication_principal, CommunicationPrincipal,
    CommunicationStoreError,
};
use super::goal::{load_owned_goal, GoalCorrelationKind, GoalLifecycle};
use super::Database;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::Serialize;

pub const GOAL_ACTIVITY_ATTENTION_AFTER_MS: i64 = 5 * 60_000;
/// A successful Goal Plan sync proves the card is still observed for this
/// Server-owned lease. The lease deliberately covers the App's 60s hidden
/// polling target plus 15s of Host/background scheduling slack; it is not a
/// browser-supplied timestamp and never grants authority.
pub const GOAL_CARD_OBSERVATION_LEASE_MS: i64 = 75_000;
pub const GOAL_CARD_OBSERVATION_ADVANCE_MS: i64 = 1_000;
pub(crate) const GOAL_ACTIVITY_WINDOW_SCAN_LIMIT: usize = 16;
pub(crate) const GOAL_ACTIVITY_SESSION_SCAN_LIMIT: usize = 16;

/// Server-internal evidence, deliberately not deserializable or a Tool input.
/// ClientWindow and observation-principal fields are correlation, never authority.
#[derive(Debug, Clone)]
pub struct GoalStallCandidate {
    pub goal_id: String,
    pub expected_revision: i64,
    pub controller_agent_id: String,
    pub workflow_session_id: String,
    pub project_id: String,
    pub observed_window_key: String,
    pub observation_principal_kind: String,
    pub observation_principal_id: String,
    pub last_meaningful_activity_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GoalStallAttention {
    pub event_id: String,
    pub wake_id: String,
    pub created: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoalStallHostDeliveryObservation {
    NotObserved,
    Accepted,
    Unknown,
}

/// Bounded semantic result of an exact-Goal continuity join. Correlation
/// identifiers remain local to the Store query and are deliberately not exposed
/// to Runtime/UI projection code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalStallWakeObservation {
    pub state: AgentWakeState,
    pub host_delivery: GoalStallHostDeliveryObservation,
    pub attention_created_at_unix_ms: i64,
    pub wake_created_at_unix_ms: i64,
    pub dispatch_prepared_at_unix_ms: Option<i64>,
    pub host_dispatch_accepted_at_unix_ms: Option<i64>,
    pub host_dispatch_unknown_at_unix_ms: Option<i64>,
    pub consumed_at_unix_ms: Option<i64>,
    pub first_post_resume_meaningful_at_unix_ms: Option<i64>,
    pub last_post_resume_meaningful_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalStallResumeObservation {
    pub attention_candidate_at_unix_ms: i64,
    pub attention_created_at_unix_ms: i64,
    pub wake_created_at_unix_ms: i64,
    pub dispatch_prepared_at_unix_ms: Option<i64>,
    pub host_dispatch_accepted_at_unix_ms: Option<i64>,
    pub host_dispatch_unknown_at_unix_ms: Option<i64>,
    pub consumed_at_unix_ms: i64,
    pub first_post_resume_meaningful_at_unix_ms: Option<i64>,
    pub last_post_resume_meaningful_at_unix_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalStallContinuityObservation {
    pub current_wake: Option<GoalStallWakeObservation>,
    pub last_resume: Option<GoalStallResumeObservation>,
}

fn invariant() -> CommunicationStoreError {
    CommunicationStoreError::new(
        "goal_stall_attention_invariant",
        "Goal workflow stall attention evidence is missing, unauthorized, or inconsistent",
    )
}

fn valid_observation_identity(window: &str, kind: &str, id: &str) -> bool {
    window.len() == 64
        && window
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        && !kind.is_empty()
        && kind.len() <= 64
        && !id.is_empty()
        && id.len() <= 512
}

fn live_quiet_observation(last_work: i64, last_seen: i64, now: i64) -> bool {
    last_work >= 0
        && last_seen <= now
        && now.saturating_sub(last_work) >= GOAL_ACTIVITY_ATTENTION_AFTER_MS
        && last_seen.saturating_sub(last_work) >= GOAL_CARD_OBSERVATION_ADVANCE_MS
        && now.saturating_sub(last_seen) <= GOAL_CARD_OBSERVATION_LEASE_MS
}

fn post_resume_meaningful_times(
    conn: &Connection,
    observed_window_key: &str,
    observation_principal_kind: &str,
    observation_principal_id: &str,
    workflow_session_id: &str,
    consumed_at_unix_ms: i64,
) -> Result<(Option<i64>, Option<i64>), CommunicationStoreError> {
    conn.query_row(
        "SELECT MIN(e.window_ended_at_ms), MAX(e.window_ended_at_ms)
         FROM action_events e
         JOIN action_event_workflow_links l ON l.event_id = e.event_id
         WHERE e.client_window_key = ?1
           AND e.principal_correlation_kind = ?2
           AND e.principal_correlation_id = ?3
           AND e.window_meaningful = 1
           AND e.window_ended_at_ms IS NOT NULL
           AND e.window_ended_at_ms >= ?4
           AND l.workflow_session_id = ?5",
        params![
            observed_window_key,
            observation_principal_kind,
            observation_principal_id,
            consumed_at_unix_ms,
            workflow_session_id
        ],
        |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
    )
    .map_err(store_error)
}

impl Database {
    /// Read bounded durable continuity evidence for one owned Goal.
    ///
    /// The current Wake is returned only when the exact current inactivity epoch
    /// targets the Goal's current controller. The most recent consumed stall
    /// continuation may retain only its bounded timing summary, and only when it
    /// targeted that same current controller. This prevents another Goal,
    /// AgentTask, AgentWait, prior controller, or prior inactivity epoch on the
    /// same Agent from becoming current Goal continuity state.
    pub fn read_goal_stall_continuity(
        &self,
        principal: &CommunicationPrincipal,
        goal_id: &str,
        current_last_meaningful_activity_at_ms: Option<i64>,
        current_controller_agent_id: &str,
    ) -> Result<GoalStallContinuityObservation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let conn = self.lock_connection(crate::StoreDomain::Goal);

        let malformed: bool = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1
                    FROM wc_agent_attention_events e
                    LEFT JOIN wc_agent_wakes w
                      ON w.source_event_id = e.event_id
                     AND w.trigger_kind = 'attention_event'
                     AND w.source_task_id IS NULL
                     AND w.source_task_attempt_id IS NULL
                     AND w.source_wait_id IS NULL
                     AND w.target_agent_id = e.target_agent_id
                    WHERE e.kind = 'goal_workflow_stalled'
                      AND e.goal_id = ?1
                      AND e.owner_principal_kind = ?2
                      AND e.owner_principal_digest = ?3
                    GROUP BY e.event_id
                    HAVING COUNT(w.wake_id) != 1
                )",
                params![goal_id, principal.kind, principal.digest],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if malformed {
            return Err(invariant());
        }

        let last_resume_row = conn
            .query_row(
                "SELECT e.last_meaningful_activity_at_ms, e.created_at_unix_ms,
                        w.created_at_unix_ms, w.consumed_at_unix_ms,
                        w.claimed_attempt_id, w.claimed_endpoint_id,
                        w.claimed_controller_generation, w.consumed_by_endpoint_id,
                        w.consumed_controller_generation,
                        a.attempt_id, a.endpoint_id, a.controller_generation, a.state,
                        a.prepared_at_unix_ms, a.delivered_at_unix_ms,
                        a.delivery_unknown_at_unix_ms, a.consumed_at_unix_ms,
                        e.workflow_session_id, e.observed_window_key,
                        e.observation_principal_kind, e.observation_principal_id
                 FROM wc_agent_attention_events e
                 JOIN wc_agent_wakes w
                   ON w.source_event_id = e.event_id
                  AND w.trigger_kind = 'attention_event'
                  AND w.source_task_id IS NULL
                  AND w.source_task_attempt_id IS NULL
                  AND w.source_wait_id IS NULL
                  AND w.target_agent_id = e.target_agent_id
                 LEFT JOIN wc_agent_wake_attempts a
                   ON a.attempt_id = w.claimed_attempt_id
                  AND a.wake_id = w.wake_id
                 WHERE e.kind = 'goal_workflow_stalled'
                   AND e.goal_id = ?1
                   AND e.owner_principal_kind = ?2
                   AND e.owner_principal_digest = ?3
                   AND e.target_agent_id = ?4
                   AND w.state = 'consumed'
                 ORDER BY w.consumed_at_unix_ms DESC, w.wake_id DESC
                 LIMIT 1",
                params![
                    goal_id,
                    principal.kind,
                    principal.digest,
                    current_controller_agent_id
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                        row.get::<_, Option<String>>(12)?,
                        row.get::<_, Option<i64>>(13)?,
                        row.get::<_, Option<i64>>(14)?,
                        row.get::<_, Option<i64>>(15)?,
                        row.get::<_, Option<i64>>(16)?,
                        row.get::<_, String>(17)?,
                        row.get::<_, String>(18)?,
                        row.get::<_, String>(19)?,
                        row.get::<_, String>(20)?,
                    ))
                },
            )
            .optional()
            .map_err(store_error)?;
        let last_resume = if let Some((
            last_work,
            attention_created_at_unix_ms,
            wake_created_at_unix_ms,
            consumed_at_unix_ms,
            claimed_attempt_id,
            claimed_endpoint_id,
            claimed_controller_generation,
            consumed_by_endpoint_id,
            consumed_controller_generation,
            attempt_id,
            attempt_endpoint_id,
            attempt_controller_generation,
            attempt_state,
            dispatch_prepared_at_unix_ms,
            host_dispatch_accepted_at_unix_ms,
            host_dispatch_unknown_at_unix_ms,
            attempt_consumed_at_unix_ms,
            workflow_session_id,
            observed_window_key,
            observation_principal_kind,
            observation_principal_id,
        )) = last_resume_row
        {
            let attention_candidate_at_unix_ms =
                last_work.saturating_add(GOAL_ACTIVITY_ATTENTION_AFTER_MS);
            if claimed_attempt_id.is_none()
                || claimed_endpoint_id.is_none()
                || claimed_controller_generation.is_none()
                || consumed_by_endpoint_id != claimed_endpoint_id
                || consumed_controller_generation != claimed_controller_generation
                || attempt_id.as_deref() != claimed_attempt_id.as_deref()
                || attempt_endpoint_id != claimed_endpoint_id
                || attempt_controller_generation != claimed_controller_generation
                || attempt_state.as_deref() != Some("consumed")
                || attempt_consumed_at_unix_ms != Some(consumed_at_unix_ms)
                || dispatch_prepared_at_unix_ms.is_none()
                || (host_dispatch_accepted_at_unix_ms.is_some()
                    && host_dispatch_unknown_at_unix_ms.is_some())
                || attention_created_at_unix_ms < attention_candidate_at_unix_ms
                || wake_created_at_unix_ms < attention_created_at_unix_ms
                || dispatch_prepared_at_unix_ms.is_some_and(|prepared| {
                    prepared < wake_created_at_unix_ms || prepared > consumed_at_unix_ms
                })
                || host_dispatch_accepted_at_unix_ms.is_some_and(|accepted| {
                    accepted < dispatch_prepared_at_unix_ms.unwrap()
                        || accepted > consumed_at_unix_ms
                })
                || host_dispatch_unknown_at_unix_ms.is_some_and(|unknown| {
                    unknown < dispatch_prepared_at_unix_ms.unwrap() || unknown > consumed_at_unix_ms
                })
            {
                return Err(invariant());
            }
            let (first_post_resume_meaningful_at_unix_ms, last_post_resume_meaningful_at_unix_ms) =
                post_resume_meaningful_times(
                    &conn,
                    &observed_window_key,
                    &observation_principal_kind,
                    &observation_principal_id,
                    &workflow_session_id,
                    consumed_at_unix_ms,
                )?;
            Some(GoalStallResumeObservation {
                attention_candidate_at_unix_ms,
                attention_created_at_unix_ms,
                wake_created_at_unix_ms,
                dispatch_prepared_at_unix_ms,
                host_dispatch_accepted_at_unix_ms,
                host_dispatch_unknown_at_unix_ms,
                consumed_at_unix_ms,
                first_post_resume_meaningful_at_unix_ms,
                last_post_resume_meaningful_at_unix_ms,
            })
        } else {
            None
        };

        let Some(epoch) = current_last_meaningful_activity_at_ms else {
            return Ok(GoalStallContinuityObservation {
                current_wake: None,
                last_resume,
            });
        };
        let row = conn
            .query_row(
                "SELECT e.target_agent_id,
                        w.wake_id, w.target_agent_id, w.state,
                        w.claimed_attempt_id, w.claimed_endpoint_id,
                        w.claimed_controller_generation, w.consumed_at_unix_ms,
                        a.attempt_id, a.prepared_at_unix_ms, a.delivered_at_unix_ms,
                        a.delivery_unknown_at_unix_ms,
                        e.created_at_unix_ms, w.created_at_unix_ms,
                        e.workflow_session_id, e.observed_window_key,
                        e.observation_principal_kind, e.observation_principal_id
                 FROM wc_agent_attention_events e
                 JOIN wc_agent_wakes w
                   ON w.source_event_id = e.event_id
                  AND w.trigger_kind = 'attention_event'
                  AND w.source_task_id IS NULL
                  AND w.source_task_attempt_id IS NULL
                  AND w.source_wait_id IS NULL
                  AND w.target_agent_id = e.target_agent_id
                 LEFT JOIN wc_agent_wake_attempts a
                   ON a.attempt_id = w.claimed_attempt_id
                  AND a.wake_id = w.wake_id
                 WHERE e.kind = 'goal_workflow_stalled'
                   AND e.goal_id = ?1
                   AND e.last_meaningful_activity_at_ms = ?2
                   AND e.owner_principal_kind = ?3
                   AND e.owner_principal_digest = ?4",
                params![goal_id, epoch, principal.kind, principal.digest],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, Option<i64>>(11)?,
                        row.get::<_, i64>(12)?,
                        row.get::<_, i64>(13)?,
                        row.get::<_, String>(14)?,
                        row.get::<_, String>(15)?,
                        row.get::<_, String>(16)?,
                        row.get::<_, String>(17)?,
                    ))
                },
            )
            .optional()
            .map_err(store_error)?;
        let Some((
            event_target_agent_id,
            _wake_id,
            wake_target_agent_id,
            wake_state,
            claimed_attempt_id,
            claimed_endpoint_id,
            claimed_controller_generation,
            consumed_at_unix_ms,
            attempt_id,
            prepared_at_unix_ms,
            delivered_at_unix_ms,
            delivery_unknown_at_unix_ms,
            attention_created_at_unix_ms,
            wake_created_at_unix_ms,
            workflow_session_id,
            observed_window_key,
            observation_principal_kind,
            observation_principal_id,
        )) = row
        else {
            return Ok(GoalStallContinuityObservation {
                current_wake: None,
                last_resume,
            });
        };
        if event_target_agent_id != wake_target_agent_id {
            return Err(invariant());
        }
        let state = AgentWakeState::from_db(&wake_state, 3).map_err(store_error)?;
        let has_claim = claimed_attempt_id.is_some()
            && claimed_endpoint_id.is_some()
            && claimed_controller_generation.is_some();
        let has_partial_claim = claimed_attempt_id.is_some()
            || claimed_endpoint_id.is_some()
            || claimed_controller_generation.is_some();
        if matches!(state, AgentWakeState::Pending | AgentWakeState::Retired) {
            if has_partial_claim || attempt_id.is_some() || consumed_at_unix_ms.is_some() {
                return Err(invariant());
            }
        } else if !has_claim || attempt_id.as_deref() != claimed_attempt_id.as_deref() {
            return Err(invariant());
        }
        if delivered_at_unix_ms.is_some() && delivery_unknown_at_unix_ms.is_some() {
            return Err(invariant());
        }
        let host_delivery = if delivered_at_unix_ms.is_some() {
            GoalStallHostDeliveryObservation::Accepted
        } else if delivery_unknown_at_unix_ms.is_some() {
            GoalStallHostDeliveryObservation::Unknown
        } else {
            GoalStallHostDeliveryObservation::NotObserved
        };
        if (state == AgentWakeState::Delivered
            && host_delivery != GoalStallHostDeliveryObservation::Accepted)
            || (state == AgentWakeState::DeliveryUnknown
                && host_delivery != GoalStallHostDeliveryObservation::Unknown)
            || (state == AgentWakeState::Consumed && consumed_at_unix_ms.is_none())
            || (state != AgentWakeState::Consumed && consumed_at_unix_ms.is_some())
        {
            return Err(invariant());
        }
        let (first_post_resume_meaningful_at_unix_ms, last_post_resume_meaningful_at_unix_ms) =
            if let Some(consumed_at) = consumed_at_unix_ms {
                post_resume_meaningful_times(
                    &conn,
                    &observed_window_key,
                    &observation_principal_kind,
                    &observation_principal_id,
                    &workflow_session_id,
                    consumed_at,
                )?
            } else {
                (None, None)
            };

        if event_target_agent_id != current_controller_agent_id {
            return Ok(GoalStallContinuityObservation {
                current_wake: None,
                last_resume,
            });
        }
        Ok(GoalStallContinuityObservation {
            current_wake: Some(GoalStallWakeObservation {
                state,
                host_delivery,
                attention_created_at_unix_ms,
                wake_created_at_unix_ms,
                dispatch_prepared_at_unix_ms: prepared_at_unix_ms,
                host_dispatch_accepted_at_unix_ms: delivered_at_unix_ms,
                host_dispatch_unknown_at_unix_ms: delivery_unknown_at_unix_ms,
                consumed_at_unix_ms,
                first_post_resume_meaningful_at_unix_ms,
                last_post_resume_meaningful_at_unix_ms,
            }),
            last_resume,
        })
    }

    /// Commit at most one Event and logical Wake for this Goal's last meaningful
    /// activity epoch. Caller holds the existing Window activity and active
    /// Session authority fences across this synchronous transaction.
    pub fn record_goal_workflow_stalled(
        &self,
        principal: &CommunicationPrincipal,
        candidate: &GoalStallCandidate,
        clock: impl FnOnce() -> i64,
    ) -> Result<Option<GoalStallAttention>, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        if !valid_observation_identity(
            &candidate.observed_window_key,
            &candidate.observation_principal_kind,
            &candidate.observation_principal_id,
        ) || candidate.expected_revision < 1
            || candidate.project_id.len() > 512
        {
            return Err(invariant());
        }
        let mut conn = self.lock_connection(crate::StoreDomain::Goal);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = clock();
        let goal = load_owned_goal(&transaction, principal, &candidate.goal_id)
            .map_err(|error| CommunicationStoreError::new(error.code(), error.message()))?;
        if goal.summary.lifecycle != GoalLifecycle::Active
            || goal.summary.revision != candidate.expected_revision
            || goal.controller_agent_id.as_deref() != Some(candidate.controller_agent_id.as_str())
            || goal.summary.workflow_session_count > GOAL_ACTIVITY_SESSION_SCAN_LIMIT as i64
            || !goal.correlations.iter().any(|correlation| {
                correlation.kind == GoalCorrelationKind::WorkflowSession
                    && correlation.reference_id == candidate.workflow_session_id
            })
        {
            return Ok(None);
        }
        require_owned_attention_target(&transaction, principal, &candidate.controller_agent_id)?;

        // A historical association is not a current Window relation. Revalidate
        // the latest explicit durable Session links, across Projects, and reject
        // same-time ambiguity instead of choosing a Session by recency guess.
        let relations = {
            let mut statement = transaction
                .prepare(
                    "SELECT l.workflow_session_id, MAX(l.project), MAX(l.linked_at_ms)
                 FROM action_event_workflow_links l JOIN action_events e ON e.event_id = l.event_id
                 WHERE e.client_window_key = ?1
                   AND e.principal_correlation_kind = ?2 AND e.principal_correlation_id = ?3
                 GROUP BY l.workflow_session_id
                 ORDER BY MAX(l.linked_at_ms) DESC, l.workflow_session_id ASC LIMIT 2",
                )
                .map_err(store_error)?;
            let rows = statement
                .query_map(
                    params![
                        candidate.observed_window_key,
                        candidate.observation_principal_kind,
                        candidate.observation_principal_id
                    ],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )
                .map_err(store_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(store_error)?
        };
        let Some((session_id, project, linked_at)) = relations.first() else {
            return Ok(None);
        };
        if session_id != &candidate.workflow_session_id
            || project.as_deref() != Some(candidate.project_id.as_str())
            || relations.get(1).is_some_and(|(_, _, at)| at >= linked_at)
        {
            return Ok(None);
        }

        // The aggregate above selects the current Session, not a Project from
        // unrelated historical rows. Every row at that exact latest boundary
        // must agree with the independently authorized current Session Project.
        let invalid_latest_relation: bool = transaction
            .query_row(
                "SELECT EXISTS(
                SELECT 1 FROM action_event_workflow_links l
                JOIN action_events e ON e.event_id = l.event_id
                WHERE e.client_window_key = ?1
                  AND e.principal_correlation_kind = ?2 AND e.principal_correlation_id = ?3
                  AND l.workflow_session_id = ?4 AND l.linked_at_ms = ?5
                  AND (l.project IS NULL OR l.project != ?6)
             )",
                params![
                    candidate.observed_window_key,
                    candidate.observation_principal_kind,
                    candidate.observation_principal_id,
                    candidate.workflow_session_id,
                    linked_at,
                    candidate.project_id
                ],
                |row| row.get(0),
            )
            .map_err(store_error)?;
        if *linked_at > now || invalid_latest_relation {
            return Ok(None);
        }

        let windows = {
            let mut statement = transaction
                .prepare(
                    "SELECT DISTINCT e.client_window_key
                 FROM wc_goal_correlations c
                 JOIN action_event_workflow_links l ON l.workflow_session_id = c.reference_id
                 JOIN action_events e ON e.event_id = l.event_id
                 WHERE c.goal_id = ?1 AND c.kind = 'workflow_session'
                   AND e.client_window_key IS NOT NULL
                   AND e.principal_correlation_kind = ?2 AND e.principal_correlation_id = ?3
                 ORDER BY e.client_window_key LIMIT ?4",
                )
                .map_err(store_error)?;
            let rows = statement
                .query_map(
                    params![
                        candidate.goal_id,
                        candidate.observation_principal_kind,
                        candidate.observation_principal_id,
                        (GOAL_ACTIVITY_WINDOW_SCAN_LIMIT + 1) as i64
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(store_error)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(store_error)?
        };
        if windows.len() > GOAL_ACTIVITY_WINDOW_SCAN_LIMIT
            || !windows.contains(&candidate.observed_window_key)
        {
            return Ok(None);
        }
        let mut last_work = None::<i64>;
        for window in &windows {
            let (latest, malformed): (Option<i64>, i64) = transaction
                .query_row(
                    "SELECT MAX(CASE WHEN window_meaningful = 1 THEN window_ended_at_ms END),
                        COALESCE(SUM(CASE WHEN window_meaningful = 1 AND
                            (window_started_at_ms IS NULL OR window_ended_at_ms IS NULL
                             OR window_ended_at_ms < window_started_at_ms) THEN 1 ELSE 0 END), 0)
                 FROM action_events WHERE client_window_key = ?1
                   AND principal_correlation_kind = ?2 AND principal_correlation_id = ?3",
                    params![
                        window,
                        candidate.observation_principal_kind,
                        candidate.observation_principal_id
                    ],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(store_error)?;
            if malformed != 0 {
                return Ok(None);
            }
            if let Some(latest) = latest {
                last_work = Some(last_work.unwrap_or(i64::MIN).max(latest));
            }
        }
        if last_work != Some(candidate.last_meaningful_activity_at_ms) {
            // New persisted work or changed coverage invalidates the snapshot.
            return Ok(None);
        }
        let (last_seen, latest_gap): (Option<i64>, Option<i64>) = transaction.query_row(
            "SELECT MAX(CASE WHEN action_name = 'toolsCall' AND operation = 'goal_plan_sync'
                                  AND window_meaningful = 0 AND status = 'success'
                                  AND window_started_at_ms IS NOT NULL
                                  AND window_started_at_ms >= 0
                                  AND window_ended_at_ms >= window_started_at_ms
                                  AND (CASE WHEN json_valid(ids_json) THEN json_extract(ids_json, '$.goal_id') ELSE NULL END) = ?4
                             THEN window_ended_at_ms END),
                    MAX(CASE WHEN recorder_gap_session_id IS NOT NULL THEN window_ended_at_ms END)
             FROM action_events WHERE client_window_key = ?1
               AND principal_correlation_kind = ?2 AND principal_correlation_id = ?3",
            params![candidate.observed_window_key, candidate.observation_principal_kind, candidate.observation_principal_id, candidate.goal_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(store_error)?;
        let Some(last_seen) = last_seen else {
            return Ok(None);
        };
        if !live_quiet_observation(candidate.last_meaningful_activity_at_ms, last_seen, now)
            || latest_gap.is_some_and(|gap| gap >= *linked_at)
        {
            return Ok(None);
        }

        // Neither a new View, Goal revision, controller replacement nor Session
        // selection can manufacture a second epoch without newer meaningful work.
        let existing: Option<(String, Option<String>)> = transaction.query_row(
            "SELECT e.event_id, w.wake_id FROM wc_agent_attention_events e
             LEFT JOIN wc_agent_wakes w ON w.source_event_id = e.event_id AND w.trigger_kind = 'attention_event'
             WHERE e.kind = 'goal_workflow_stalled' AND e.goal_id = ?1
               AND e.last_meaningful_activity_at_ms = ?2
               AND e.owner_principal_kind = ?3 AND e.owner_principal_digest = ?4",
            params![candidate.goal_id, candidate.last_meaningful_activity_at_ms, principal.kind, principal.digest],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(store_error)?;
        if let Some((event_id, wake_id)) = existing {
            let wake_id = wake_id.ok_or_else(invariant)?;
            let target: String = transaction.query_row(
                "SELECT e.target_agent_id FROM wc_agent_attention_events e
                 JOIN wc_agent_wakes w ON w.source_event_id = e.event_id
                 WHERE e.event_id = ?1 AND w.wake_id = ?2 AND w.trigger_kind = 'attention_event'
                   AND w.target_agent_id = e.target_agent_id
                   AND w.source_task_id IS NULL AND w.source_task_attempt_id IS NULL
                   AND w.source_wait_id IS NULL AND w.state IN ('pending', 'claimed', 'prepared', 'delivered', 'delivery_unknown', 'consumed', 'retired')",
                params![event_id, wake_id], |row| row.get(0),
            ).optional().map_err(store_error)?.ok_or_else(invariant)?;
            require_goal_stall_event_for_wake(&transaction, principal, &event_id, &target)?;
            transaction.commit().map_err(store_error)?;
            return Ok(Some(GoalStallAttention {
                event_id,
                wake_id,
                created: false,
            }));
        }
        let (previous_work, previous_attention_at): (Option<i64>, Option<i64>) = transaction.query_row(
            "SELECT MAX(last_meaningful_activity_at_ms), MAX(created_at_unix_ms) FROM wc_agent_attention_events
             WHERE kind = 'goal_workflow_stalled' AND goal_id = ?1",
            params![candidate.goal_id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).map_err(store_error)?;
        if previous_work
            .is_some_and(|previous| previous >= candidate.last_meaningful_activity_at_ms)
            || previous_attention_at
                .is_some_and(|previous| previous >= candidate.last_meaningful_activity_at_ms)
        {
            return Ok(None);
        }
        let event_id = allocate_identity(
            &transaction,
            AGENT_ATTENTION_EVENT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_attention_events WHERE event_id = ?1)",
        )?;
        transaction.execute(
            "INSERT INTO wc_agent_attention_events (
                event_id, kind, owner_principal_kind, owner_principal_digest,
                target_agent_id, goal_id, workflow_session_id, observed_window_key,
                observation_principal_kind, observation_principal_id,
                last_meaningful_activity_at_ms, last_seen_at_ms, goal_revision, created_at_unix_ms
             ) VALUES (?1, 'goal_workflow_stalled', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![event_id, principal.kind, principal.digest, candidate.controller_agent_id,
                candidate.goal_id, candidate.workflow_session_id, candidate.observed_window_key,
                candidate.observation_principal_kind, candidate.observation_principal_id,
                candidate.last_meaningful_activity_at_ms, last_seen, goal.summary.revision, now],
        ).map_err(store_error)?;
        let wake_id = insert_attention_wake_in_transaction(
            &transaction,
            &candidate.controller_agent_id,
            &event_id,
            now,
        )?;
        transaction.commit().map_err(store_error)?;
        Ok(Some(GoalStallAttention {
            event_id,
            wake_id,
            created: true,
        }))
    }
}

pub(super) fn require_goal_stall_event_for_wake(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    event_id: &str,
    target_agent_id: &str,
) -> Result<AgentAttentionEventRecord, CommunicationStoreError> {
    let event = conn
        .query_row(
            "SELECT event_id, kind, owner_principal_kind, owner_principal_digest,
                target_agent_id, goal_id, created_at_unix_ms, workflow_session_id,
                observed_window_key, observation_principal_kind, observation_principal_id,
                last_meaningful_activity_at_ms, last_seen_at_ms, goal_revision
         FROM wc_agent_attention_events
         WHERE event_id = ?1 AND kind = 'goal_workflow_stalled'
           AND owner_principal_kind = ?2 AND owner_principal_digest = ?3 AND target_agent_id = ?4
           AND task_id IS NULL AND task_attempt_id IS NULL AND terminal_task_state IS NULL",
            params![event_id, principal.kind, principal.digest, target_agent_id],
            |row| {
                Ok(AgentAttentionEventRecord {
                    event_id: row.get(0)?,
                    kind: row.get(1)?,
                    owner_principal_kind: row.get(2)?,
                    owner_principal_digest: row.get(3)?,
                    target_agent_id: row.get(4)?,
                    goal_id: row.get(5)?,
                    created_at_unix_ms: row.get(6)?,
                    source: AgentAttentionSource::GoalWorkflowStalled {
                        workflow_session_id: row.get(7)?,
                        observed_window_key: row.get(8)?,
                        observation_principal_kind: row.get(9)?,
                        observation_principal_id: row.get(10)?,
                        last_meaningful_activity_at_ms: row.get(11)?,
                        last_seen_at_ms: row.get(12)?,
                        goal_revision: row.get(13)?,
                    },
                })
            },
        )
        .optional()
        .map_err(store_error)?
        .ok_or_else(invariant)?;
    require_owned_attention_target(conn, principal, target_agent_id)?;
    let goal = load_owned_goal(conn, principal, &event.goal_id).map_err(|_| invariant())?;
    let AgentAttentionSource::GoalWorkflowStalled {
        workflow_session_id,
        observed_window_key,
        observation_principal_kind,
        observation_principal_id,
        last_meaningful_activity_at_ms,
        last_seen_at_ms,
        goal_revision,
    } = &event.source
    else {
        return Err(invariant());
    };
    if !valid_observation_identity(
        observed_window_key,
        observation_principal_kind,
        observation_principal_id,
    ) || !live_quiet_observation(
        *last_meaningful_activity_at_ms,
        *last_seen_at_ms,
        event.created_at_unix_ms,
    ) || *goal_revision < 1
        || *goal_revision > goal.summary.revision
        || !goal.correlations.iter().any(|correlation| {
            correlation.kind == GoalCorrelationKind::WorkflowSession
                && &correlation.reference_id == workflow_session_id
        })
    {
        return Err(invariant());
    }
    // This remains the original immutable attention fact even after work resumes
    // or the Goal is completed. It does not claim the Goal is still stalled and
    // cannot authorize Task execution. The fresh turn must re-read current truth.
    debug_assert_eq!(event.kind, AGENT_ATTENTION_EVENT_KIND_GOAL_WORKFLOW_STALLED);
    Ok(event)
}
