fn require_coding_run_observation_identity(
    binding: &AgentTaskCodingRunBindingRecord,
    observation: &AgentTaskCodingRunObservation,
) -> Result<(), CommunicationStoreError> {
    if binding.run_id != observation.run_id
        || binding.runtime_project_id != observation.runtime_project_id
        || binding.provider_id != observation.provider_id
        || binding.provider_instance_id != observation.provider_instance_id
        || binding.authority_fingerprint != observation.authority_fingerprint
        || binding.coding_agent_intent_fingerprint != observation.coding_agent_intent_fingerprint
    {
        return Err(CommunicationStoreError::new(
            "agent_task_coding_run_identity_mismatch",
            "CodingAgentRun snapshot does not match the exact durable AgentTaskAttempt binding",
        ));
    }
    Ok(())
}

fn canonical_coding_run_snapshot(
    observation: &AgentTaskCodingRunObservation,
) -> Result<CodingAgentRunSnapshot, String> {
    for (label, value) in [
        (
            "terminal_stop_reason",
            observation.terminal_stop_reason.as_deref(),
        ),
        (
            "terminal_error_code",
            observation.terminal_error_code.as_deref(),
        ),
        ("terminal_message", observation.terminal_message.as_deref()),
    ] {
        if let Some(value) = value {
            if value.len() > MAX_AGENT_TASK_TERMINAL_TEXT_BYTES {
                return Err(format!(
                    "{label} exceeds the AgentTask bounded terminal text limit"
                ));
            }
        }
    }

    let state = observation.run_state.clone();
    let execution_state = observation.execution_state;
    let observation_revision = u64::try_from(observation.observation_revision).map_err(|_| {
        "CodingAgentRun snapshot contains a negative observation revision".to_string()
    })?;
    let has_terminal_metadata = observation.terminal_stop_reason.is_some()
        || observation.terminal_error_code.is_some()
        || observation.terminal_message.is_some()
        || observation.completed_at_unix.is_some();
    let terminal = if has_terminal_metadata {
        Some(CodingAgentTerminal {
            stop_reason: observation.terminal_stop_reason.clone(),
            error_code: observation.terminal_error_code.clone(),
            message: observation.terminal_message.clone(),
            completed_at: observation.completed_at_unix.ok_or_else(|| {
                "CodingAgentRun terminal observation lacks completed timestamp".to_string()
            })?,
        })
    } else {
        None
    };
    let snapshot = CodingAgentRunSnapshot {
        run_id: observation.run_id.clone(),
        intent_fingerprint: observation.coding_agent_intent_fingerprint.clone(),
        authority_fingerprint: observation.authority_fingerprint.clone(),
        runtime_project_id: observation.runtime_project_id.clone(),
        provider_id: observation.provider_id.clone(),
        provider_instance_id: observation.provider_instance_id.clone(),
        state,
        execution_state,
        observation_revision,
        // AgentTask persists the authoritative semantic observation fields but not
        // Runner wall-clock snapshot metadata. Neutralize those non-persisted fields
        // so durable replay/conflict classification compares exactly what is stored.
        created_at: 0,
        updated_at: 0,
        terminal,
    };
    validate_coding_agent_run_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn validate_coding_run_observation(
    observation: &AgentTaskCodingRunObservation,
) -> Result<CodingAgentRunSnapshot, CommunicationStoreError> {
    canonical_coding_run_snapshot(observation).map_err(|message| {
        CommunicationStoreError::new("invalid_agent_task_coding_run_observation", message)
    })
}

fn stored_coding_run_snapshot(
    binding: &AgentTaskCodingRunBindingRecord,
) -> Result<Option<CodingAgentRunSnapshot>, CommunicationStoreError> {
    let Some(revision) = binding.last_observation_revision else {
        if binding.last_observed_run_state.is_some()
            || binding.last_observed_execution_state.is_some()
            || binding.terminal_stop_reason.is_some()
            || binding.terminal_error_code.is_some()
            || binding.terminal_message.is_some()
            || binding.completed_at_unix.is_some()
        {
            return Err(CommunicationStoreError::new(
                "agent_task_storage_invariant",
                "CodingAgent binding has observation fields without an observation revision",
            ));
        }
        return Ok(None);
    };
    let observation = AgentTaskCodingRunObservation {
        run_id: binding.run_id.clone(),
        runtime_project_id: binding.runtime_project_id.clone(),
        provider_id: binding.provider_id.clone(),
        provider_instance_id: binding.provider_instance_id.clone(),
        authority_fingerprint: binding.authority_fingerprint.clone(),
        coding_agent_intent_fingerprint: binding.coding_agent_intent_fingerprint.clone(),
        run_state: binding.last_observed_run_state.clone().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_storage_invariant",
                "CodingAgent binding observation revision lacks run state",
            )
        })?,
        execution_state: binding.last_observed_execution_state.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_storage_invariant",
                "CodingAgent binding observation revision lacks execution state",
            )
        })?,
        observation_revision: revision,
        terminal_stop_reason: binding.terminal_stop_reason.clone(),
        terminal_error_code: binding.terminal_error_code.clone(),
        terminal_message: binding.terminal_message.clone(),
        completed_at_unix: binding.completed_at_unix,
    };
    canonical_coding_run_snapshot(&observation)
        .map(Some)
        .map_err(|message| {
            CommunicationStoreError::new(
                "agent_task_storage_invariant",
                format!("stored CodingAgent observation is invalid: {message}"),
            )
        })
}

fn merge_agent_task_coding_run_observation(
    transaction: &Transaction<'_>,
    binding: &AgentTaskCodingRunBindingRecord,
    observation: &AgentTaskCodingRunObservation,
    now: i64,
) -> Result<(AgentTaskCodingRunBindingRecord, CodingAgentObservationMerge), CommunicationStoreError>
{
    require_coding_run_observation_identity(binding, observation)?;
    let incoming = validate_coding_run_observation(observation)?;
    let disposition = match stored_coding_run_snapshot(binding)? {
        Some(stored) => merge_coding_agent_run_snapshot(&stored, &incoming).map_err(|message| {
            CommunicationStoreError::new("agent_task_coding_run_observation_conflict", message)
        })?,
        None => CodingAgentObservationMerge::Advance,
    };
    if matches!(
        disposition,
        CodingAgentObservationMerge::Stale | CodingAgentObservationMerge::ExactReplay
    ) {
        return Ok((binding.clone(), disposition));
    }

    let dispatch_state = if observation.run_state == CodingAgentRunState::Lost
        || observation.execution_state == CodingAgentExecutionState::OutcomeUnknown
    {
        AgentTaskCodingRunDispatchState::OutcomeUnknown
    } else {
        AgentTaskCodingRunDispatchState::Bound
    };
    let updated = if let Some(expected_revision) = binding.last_observation_revision {
        transaction
            .execute(
                "UPDATE wc_agent_task_coding_runs
                 SET dispatch_state = ?3,
                     last_observed_run_state = ?4,
                     last_observed_execution_state = ?5,
                     last_observation_revision = ?6,
                     terminal_stop_reason = ?7,
                     terminal_error_code = ?8,
                     terminal_message = ?9,
                     completed_at_unix = ?10,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?11)
                 WHERE task_id = ?1 AND attempt_id = ?2 AND last_observation_revision = ?12",
                params![
                    binding.task_id,
                    binding.attempt_id,
                    dispatch_state.as_str(),
                    observation.run_state.as_str(),
                    observation.execution_state.as_str(),
                    observation.observation_revision,
                    observation.terminal_stop_reason,
                    observation.terminal_error_code,
                    observation.terminal_message,
                    observation.completed_at_unix,
                    now,
                    expected_revision,
                ],
            )
            .map_err(store_error)?
    } else {
        transaction
            .execute(
                "UPDATE wc_agent_task_coding_runs
                 SET dispatch_state = ?3,
                     last_observed_run_state = ?4,
                     last_observed_execution_state = ?5,
                     last_observation_revision = ?6,
                     terminal_stop_reason = ?7,
                     terminal_error_code = ?8,
                     terminal_message = ?9,
                     completed_at_unix = ?10,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?11)
                 WHERE task_id = ?1 AND attempt_id = ?2 AND last_observation_revision IS NULL",
                params![
                    binding.task_id,
                    binding.attempt_id,
                    dispatch_state.as_str(),
                    observation.run_state.as_str(),
                    observation.execution_state.as_str(),
                    observation.observation_revision,
                    observation.terminal_stop_reason,
                    observation.terminal_error_code,
                    observation.terminal_message,
                    observation.completed_at_unix,
                    now,
                ],
            )
            .map_err(store_error)?
    };
    if updated != 1 {
        return Err(CommunicationStoreError::new(
            "agent_task_storage_invariant",
            "CodingAgent observation revision CAS did not update the exact durable binding",
        ));
    }
    let binding =
        load_coding_run_binding_for_attempt(transaction, &binding.task_id, &binding.attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared after monotonic observation merge",
                )
            })?;
    Ok((binding, disposition))
}

fn task_request_hash(value: &serde_json::Value) -> String {
    digest_json("webcodex.agent-task.request.v1", value).expect("AgentTask request serializes")
}

fn validate_title(value: &str) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_AGENT_TASK_TITLE_CHARS {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_title",
            format!("title must contain 1..={MAX_AGENT_TASK_TITLE_CHARS} characters"),
        ));
    }
    Ok(value.to_string())
}

fn validate_instruction(value: &str) -> Result<String, CommunicationStoreError> {
    let value = value.trim();
    let bytes = value.len();
    if bytes == 0 || bytes > MAX_AGENT_TASK_INSTRUCTION_BYTES {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_instruction",
            format!("instruction must contain 1..={MAX_AGENT_TASK_INSTRUCTION_BYTES} UTF-8 bytes"),
        ));
    }
    Ok(value.to_string())
}

fn validate_project_reference(
    value: Option<&str>,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    let chars = value.chars().count();
    if chars == 0 || chars > MAX_AGENT_TASK_PROJECT_REF_CHARS {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_project_reference",
            format!(
                "referenced_project_id must contain 1..={MAX_AGENT_TASK_PROJECT_REF_CHARS} characters"
            ),
        ));
    }
    Ok(Some(value.to_string()))
}

fn validate_optional_terminal_text(
    value: Option<&str>,
    label: &str,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() || value.len() > MAX_AGENT_TASK_TERMINAL_TEXT_BYTES {
        return Err(CommunicationStoreError::new(
            "invalid_agent_task_terminal_text",
            format!(
                "{label} must contain 1..={MAX_AGENT_TASK_TERMINAL_TEXT_BYTES} UTF-8 bytes when provided"
            ),
        ));
    }
    Ok(Some(value.to_string()))
}

fn validate_optional_id(
    value: Option<&str>,
    prefix: &str,
    code: &'static str,
) -> Result<Option<String>, CommunicationStoreError> {
    let Some(value) = value else {
        return Ok(None);
    };
    validate_id(value, prefix, code)?;
    Ok(Some(value.to_string()))
}

fn require_owned_agent(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    agent_id: &str,
) -> Result<(), CommunicationStoreError> {
    validate_id(agent_id, DURABLE_AGENT_ID_PREFIX, "invalid_agent_id")?;
    let owned = conn
        .query_row(
            "SELECT EXISTS(
                SELECT 1 FROM wc_agent_identities
                WHERE agent_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3
             )",
            params![agent_id, principal.kind, principal.digest],
            |row| row.get::<_, bool>(0),
        )
        .map_err(store_error)?;
    if !owned {
        return Err(CommunicationStoreError::new(
            "agent_not_found",
            "Agent identity does not exist",
        ));
    }
    Ok(())
}

fn validate_source_references(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    conversation_id: Option<&str>,
    message_id: Option<&str>,
) -> Result<(), CommunicationStoreError> {
    let Some(conversation_id) = conversation_id else {
        return Ok(());
    };
    authorize_conversation_access(conn, principal, &ConversationAccess::Human, conversation_id)?;
    if let Some(message_id) = message_id {
        let exists = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM wc_conversation_messages
                    WHERE conversation_id = ?1 AND message_id = ?2
                 )",
                params![conversation_id, message_id],
                |row| row.get::<_, bool>(0),
            )
            .map_err(store_error)?;
        if !exists {
            return Err(CommunicationStoreError::new(
                "agent_task_source_message_not_found",
                "Conversation Message does not exist in the referenced Conversation",
            ));
        }
    }
    Ok(())
}

fn load_owned_task(
    conn: &Connection,
    principal: &CommunicationPrincipal,
    task_id: &str,
    now: i64,
) -> Result<StoredTask, CommunicationStoreError> {
    validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
    let row = conn
        .query_row(
            "SELECT task_id, assignee_agent_id, title, instruction,
                    source_conversation_id, source_message_id, referenced_project_id,
                    state, latest_attempt_id, created_at_unix_ms, updated_at_unix_ms,
                    terminal_at_unix_ms
             FROM wc_agent_tasks
             WHERE task_id = ?1 AND owner_principal_kind = ?2 AND owner_principal_digest = ?3",
            params![task_id, principal.kind, principal.digest],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    AgentTaskState::from_db(&row.get::<_, String>(7)?, 7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Option<i64>>(11)?,
                ))
            },
        )
        .optional()
        .map_err(store_error)?
        .ok_or_else(|| {
            CommunicationStoreError::new("agent_task_not_found", "AgentTask does not exist")
        })?;
    let latest_attempt = match row.8.as_deref() {
        Some(attempt_id) => {
            load_attempt_for_task(conn, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask latest Attempt is missing",
                )
            })?
        }
        None => {
            return Ok(StoredTask {
                task_id: row.0,
                assignee_agent_id: row.1,
                title: row.2,
                instruction: row.3,
                source_conversation_id: row.4,
                source_message_id: row.5,
                referenced_project_id: row.6,
                stored_state: row.7,
                created_at_unix_ms: row.9,
                updated_at_unix_ms: row.10,
                terminal_at_unix_ms: row.11,
                latest_attempt: None,
                latest_coding_run: None,
                latest_endpoint_execution: None,
            })
        }
    };
    let latest_coding_run =
        load_coding_run_binding_for_attempt(conn, task_id, &latest_attempt.attempt_id)?;
    let latest_endpoint_execution =
        load_endpoint_execution_for_attempt(conn, task_id, &latest_attempt.attempt_id)?;
    Ok(StoredTask {
        task_id: row.0,
        assignee_agent_id: row.1,
        title: row.2,
        instruction: row.3,
        source_conversation_id: row.4,
        source_message_id: row.5,
        referenced_project_id: row.6,
        stored_state: row.7,
        created_at_unix_ms: row.9,
        updated_at_unix_ms: row.10,
        terminal_at_unix_ms: row.11,
        latest_attempt: Some(latest_attempt),
        latest_coding_run,
        latest_endpoint_execution,
    })
}

fn load_attempt_for_task(
    conn: &Connection,
    task_id: &str,
    attempt_id: &str,
    _now: i64,
) -> Result<Option<StoredAttempt>, CommunicationStoreError> {
    validate_id(
        attempt_id,
        AGENT_TASK_ATTEMPT_ID_PREFIX,
        "invalid_agent_task_attempt_id",
    )?;
    conn.query_row(
        "SELECT attempt_id, task_id, attempt_number, assignee_agent_id, state,
                lease_expires_at_unix_ms, attempt_fence, attempt_controller_generation,
                created_at_unix_ms, started_at_unix_ms, terminal_at_unix_ms,
                terminal_result, terminal_reason
         FROM wc_agent_task_attempts
         WHERE attempt_id = ?1 AND task_id = ?2",
        params![attempt_id, task_id],
        |row| {
            Ok(StoredAttempt {
                attempt_id: row.get(0)?,
                task_id: row.get(1)?,
                attempt_number: row.get(2)?,
                assignee_agent_id: row.get(3)?,
                stored_state: AgentTaskAttemptState::from_db(&row.get::<_, String>(4)?, 4)?,
                lease_expires_at_unix_ms: row.get(5)?,
                attempt_fence: row.get(6)?,
                attempt_controller_generation: row.get(7)?,
                created_at_unix_ms: row.get(8)?,
                started_at_unix_ms: row.get(9)?,
                terminal_at_unix_ms: row.get(10)?,
                terminal_result: row.get(11)?,
                terminal_reason: row.get(12)?,
            })
        },
    )
    .optional()
    .map_err(store_error)
}

fn load_endpoint_execution_for_attempt(
    conn: &Connection,
    task_id: &str,
    attempt_id: &str,
) -> Result<Option<AgentTaskEndpointExecutionRecord>, CommunicationStoreError> {
    conn.query_row(
        "SELECT e.task_id, e.attempt_id, e.wake_id, w.state,
                e.endpoint_id, e.endpoint_controller_generation,
                e.created_at_unix_ms, e.updated_at_unix_ms
         FROM wc_agent_task_endpoint_executions e
         JOIN wc_agent_wakes w ON w.wake_id = e.wake_id
         WHERE e.task_id = ?1 AND e.attempt_id = ?2",
        params![task_id, attempt_id],
        |row| {
            let wake_state: String = row.get(3)?;
            Ok(AgentTaskEndpointExecutionRecord {
                task_id: row.get(0)?,
                attempt_id: row.get(1)?,
                wake_id: row.get(2)?,
                wake_state: AgentWakeState::from_db(&wake_state, 3)?,
                endpoint_id: row.get(4)?,
                endpoint_controller_generation: row.get(5)?,
                created_at_unix_ms: row.get(6)?,
                updated_at_unix_ms: row.get(7)?,
            })
        },
    )
    .optional()
    .map_err(store_error)
}

fn load_coding_run_binding_for_attempt(
    conn: &Connection,
    task_id: &str,
    attempt_id: &str,
) -> Result<Option<AgentTaskCodingRunBindingRecord>, CommunicationStoreError> {
    conn.query_row(
        "SELECT task_id, attempt_id, run_id, runtime_project_id, provider_id,
                provider_instance_id, authority_fingerprint, coding_agent_intent_fingerprint,
                binding_intent_fingerprint, dispatch_state, last_observed_run_state,
                last_observed_execution_state, last_observation_revision,
                terminal_stop_reason, terminal_error_code, terminal_message,
                completed_at_unix, created_at_unix_ms, updated_at_unix_ms, terminal_at_unix_ms
         FROM wc_agent_task_coding_runs
         WHERE task_id = ?1 AND attempt_id = ?2",
        params![task_id, attempt_id],
        |row| {
            Ok(AgentTaskCodingRunBindingRecord {
                task_id: row.get(0)?,
                attempt_id: row.get(1)?,
                run_id: row.get(2)?,
                runtime_project_id: row.get(3)?,
                provider_id: row.get(4)?,
                provider_instance_id: row.get(5)?,
                authority_fingerprint: row.get(6)?,
                coding_agent_intent_fingerprint: row.get(7)?,
                binding_intent_fingerprint: row.get(8)?,
                dispatch_state: AgentTaskCodingRunDispatchState::from_db(
                    &row.get::<_, String>(9)?,
                    9,
                )?,
                last_observed_run_state: row
                    .get::<_, Option<String>>(10)?
                    .as_deref()
                    .map(|value| {
                        CodingAgentRunState::from_wire(value).ok_or_else(|| {
                            rusqlite::Error::FromSqlConversionFailure(
                                10,
                                Type::Text,
                                format!("unsupported observed CodingAgentRun state: {value}")
                                    .into(),
                            )
                        })
                    })
                    .transpose()?,
                last_observed_execution_state: row
                    .get::<_, Option<String>>(11)?
                    .as_deref()
                    .map(|value| {
                        CodingAgentExecutionState::from_wire(value).ok_or_else(|| {
                            rusqlite::Error::FromSqlConversionFailure(
                                11,
                                Type::Text,
                                format!(
                                    "unsupported observed CodingAgent execution state: {value}"
                                )
                                .into(),
                            )
                        })
                    })
                    .transpose()?,
                last_observation_revision: row.get(12)?,
                terminal_stop_reason: row.get(13)?,
                terminal_error_code: row.get(14)?,
                terminal_message: row.get(15)?,
                completed_at_unix: row.get(16)?,
                created_at_unix_ms: row.get(17)?,
                updated_at_unix_ms: row.get(18)?,
                terminal_at_unix_ms: row.get(19)?,
            })
        },
    )
    .optional()
    .map_err(store_error)
}

fn blocking_execution_error(
    binding: Option<&AgentTaskCodingRunBindingRecord>,
) -> Option<CommunicationStoreError> {
    let binding = binding.filter(|binding| binding.dispatch_state.blocks_replacement())?;
    if binding.dispatch_state == AgentTaskCodingRunDispatchState::OutcomeUnknown
        || binding.last_observed_run_state.as_ref() == Some(&CodingAgentRunState::Lost)
    {
        Some(CommunicationStoreError::new(
            "agent_task_execution_outcome_unknown",
            "AgentTask has a CodingAgent execution whose outcome is unresolved; reconcile the exact bound Run before replacement",
        ))
    } else {
        Some(CommunicationStoreError::new(
            "agent_task_execution_active",
            "AgentTask has a bound CodingAgent execution that must be reconciled before replacement",
        ))
    }
}

fn materialize_expired_latest_attempt(
    transaction: &Transaction<'_>,
    task: &mut StoredTask,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    let Some(attempt) = task.latest_attempt.as_mut() else {
        return Ok(());
    };
    if attempt.stored_state != AgentTaskAttemptState::Active
        || attempt.lease_expires_at_unix_ms > now
    {
        return Ok(());
    }
    transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET state = 'expired', terminal_at_unix_ms = ?2
             WHERE attempt_id = ?1 AND state = 'active'",
            params![attempt.attempt_id, now],
        )
        .map_err(store_error)?;
    let execution_blocks_replacement = task
        .latest_coding_run
        .as_ref()
        .is_some_and(|binding| binding.dispatch_state.blocks_replacement());
    if !execution_blocks_replacement {
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET state = 'ready', updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                 WHERE task_id = ?1 AND state = 'active'",
                params![task.task_id, now],
            )
            .map_err(store_error)?;
        task.stored_state = AgentTaskState::Ready;
    }
    attempt.stored_state = AgentTaskAttemptState::Expired;
    attempt.terminal_at_unix_ms = Some(now);
    retire_pre_dispatch_endpoint_execution_for_attempt(transaction, &attempt.attempt_id, now)?;
    task.updated_at_unix_ms = task.updated_at_unix_ms.max(now);
    Ok(())
}

fn retire_pre_dispatch_endpoint_execution_for_attempt(
    transaction: &Transaction<'_>,
    attempt_id: &str,
    now: i64,
) -> Result<(), CommunicationStoreError> {
    transaction
        .execute(
            "UPDATE wc_agent_wake_attempts
             SET state = 'revoked', revoked_at_unix_ms = COALESCE(revoked_at_unix_ms, ?2)
             WHERE state = 'claimed'
               AND wake_id IN (
                   SELECT w.wake_id
                   FROM wc_agent_wakes w
                   JOIN wc_agent_task_endpoint_executions e ON e.wake_id = w.wake_id
                   WHERE e.attempt_id = ?1
                     AND w.trigger_kind = 'agent_task_attempt'
                     AND w.state = 'claimed'
               )",
            params![attempt_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_task_endpoint_executions
             SET endpoint_id = NULL, endpoint_controller_generation = NULL,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE attempt_id = ?1
               AND wake_id IN (
                   SELECT wake_id FROM wc_agent_wakes
                   WHERE trigger_kind = 'agent_task_attempt'
                     AND state IN ('pending', 'claimed')
               )",
            params![attempt_id, now],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_wakes
             SET state = 'retired', revision = revision + 1,
                 updated_at_unix_ms = MAX(updated_at_unix_ms, ?2),
                 claimed_attempt_id = NULL, claimed_endpoint_id = NULL,
                 claimed_controller_generation = NULL,
                 claim_lease_expires_at_unix_ms = NULL
             WHERE trigger_kind = 'agent_task_attempt'
               AND source_task_attempt_id = ?1
               AND state IN ('pending', 'claimed')",
            params![attempt_id, now],
        )
        .map_err(store_error)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AttemptAuthority<'a> {
    task_id: &'a str,
    attempt_id: &'a str,
    assignee_agent_id: &'a str,
    attempt_fence: &'a str,
    attempt_controller_generation: i64,
}

impl<'a> AttemptAuthority<'a> {
    pub(crate) fn validated(
        principal: &CommunicationPrincipal,
        task_id: &'a str,
        attempt_id: &'a str,
        assignee_agent_id: &'a str,
        attempt_fence: &'a str,
        attempt_controller_generation: i64,
    ) -> Result<Self, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            attempt_id,
            AGENT_TASK_ATTEMPT_ID_PREFIX,
            "invalid_agent_task_attempt_id",
        )?;
        validate_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        validate_proof(
            attempt_fence,
            AGENT_TASK_ATTEMPT_FENCE_PREFIX,
            "invalid_agent_task_attempt_fence",
        )?;
        if attempt_controller_generation < 1 {
            return Err(CommunicationStoreError::new(
                "invalid_agent_task_attempt_controller_generation",
                "attempt_controller_generation must be at least 1",
            ));
        }
        Ok(Self {
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        })
    }

    fn identity_json(self) -> serde_json::Value {
        json!({
            "task_id": self.task_id,
            "attempt_id": self.attempt_id,
            "assignee_agent_id": self.assignee_agent_id,
            "attempt_fence": self.attempt_fence,
            "attempt_controller_generation": self.attempt_controller_generation,
        })
    }
}

fn require_current_attempt(
    conn: &Connection,
    task: &StoredTask,
    authority: AttemptAuthority<'_>,
    now: i64,
) -> Result<StoredAttempt, CommunicationStoreError> {
    if task.stored_state.terminal() {
        return Err(task_terminal_error());
    }
    if task
        .latest_attempt
        .as_ref()
        .map(|attempt| attempt.attempt_id.as_str())
        != Some(authority.attempt_id)
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt is not the latest authoritative Attempt",
        ));
    }
    if task.assignee_agent_id.as_deref() != Some(authority.assignee_agent_id) {
        return Err(CommunicationStoreError::new(
            "agent_task_assignee_mismatch",
            "AgentTask current assignee does not match the Attempt caller assertion",
        ));
    }
    let attempt = load_attempt_for_task(conn, &task.task_id, authority.attempt_id, now)?
        .ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_attempt_not_found",
                "AgentTaskAttempt does not exist",
            )
        })?;
    if attempt.assignee_agent_id != authority.assignee_agent_id
        || attempt.attempt_fence != authority.attempt_fence
        || attempt.attempt_controller_generation != authority.attempt_controller_generation
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt fence, assignee, or controller generation is stale",
        ));
    }
    if attempt.stored_state != AgentTaskAttemptState::Active
        || attempt.lease_expires_at_unix_ms <= now
    {
        return Err(CommunicationStoreError::new(
            "agent_task_attempt_stale",
            "AgentTaskAttempt is terminal, superseded, or its lease has expired",
        ));
    }
    Ok(attempt)
}

pub(crate) fn replace_agent_task_attempt_controller_in_transaction(
    transaction: &Transaction<'_>,
    principal: &CommunicationPrincipal,
    authority: AttemptAuthority<'_>,
    now: i64,
) -> Result<AgentTaskAttemptRecord, CommunicationStoreError> {
    let task = load_owned_task(transaction, principal, authority.task_id, now)?;
    let attempt = require_current_attempt(transaction, &task, authority, now)?;
    let next_generation = attempt.attempt_controller_generation.saturating_add(1);
    transaction
        .execute(
            "UPDATE wc_agent_task_attempts
             SET attempt_controller_generation = ?2
             WHERE attempt_id = ?1",
            params![authority.attempt_id, next_generation],
        )
        .map_err(store_error)?;
    transaction
        .execute(
            "UPDATE wc_agent_tasks SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
             WHERE task_id = ?1",
            params![authority.task_id, now],
        )
        .map_err(store_error)?;
    let attempt = load_attempt_for_task(transaction, authority.task_id, authority.attempt_id, now)?
        .ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_attempt_not_found",
                "AgentTaskAttempt disappeared after controller replacement",
            )
        })?;
    Ok(attempt.record(now))
}

fn task_terminal_error() -> CommunicationStoreError {
    CommunicationStoreError::new(
        "agent_task_terminal",
        "AgentTask is terminal and cannot start or mutate execution ownership",
    )
}
