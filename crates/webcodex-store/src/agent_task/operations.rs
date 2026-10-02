impl Database {
    pub(crate) fn ensure_agent_task_schema(conn: &mut Connection) -> anyhow::Result<()> {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS wc_agent_tasks (
                task_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                assignee_agent_id TEXT,
                title TEXT NOT NULL,
                instruction TEXT NOT NULL,
                source_conversation_id TEXT,
                source_message_id TEXT,
                referenced_project_id TEXT,
                state TEXT NOT NULL CHECK(state IN ('ready', 'active', 'succeeded', 'failed')),
                latest_attempt_id TEXT,
                terminal_attempt_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                CHECK(
                    (state IN ('ready', 'active') AND terminal_attempt_id IS NULL AND terminal_at_unix_ms IS NULL)
                    OR (state IN ('succeeded', 'failed') AND terminal_attempt_id IS NOT NULL AND terminal_at_unix_ms IS NOT NULL)
                ),
                FOREIGN KEY(assignee_agent_id) REFERENCES wc_agent_identities(agent_id),
                FOREIGN KEY(source_conversation_id) REFERENCES wc_conversations(conversation_id),
                FOREIGN KEY(source_message_id) REFERENCES wc_conversation_messages(message_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_tasks_owner_updated
                ON wc_agent_tasks(owner_principal_digest, updated_at_unix_ms DESC, task_id);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_tasks_assignee
                ON wc_agent_tasks(owner_principal_digest, assignee_agent_id, updated_at_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS wc_agent_task_attempts (
                attempt_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                attempt_number INTEGER NOT NULL CHECK(attempt_number >= 1),
                assignee_agent_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('active', 'expired', 'succeeded', 'failed')),
                lease_expires_at_unix_ms INTEGER NOT NULL,
                attempt_fence TEXT NOT NULL UNIQUE,
                attempt_controller_generation INTEGER NOT NULL CHECK(attempt_controller_generation >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                started_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                terminal_result TEXT,
                terminal_reason TEXT,
                CHECK(
                    (state = 'active' AND terminal_at_unix_ms IS NULL)
                    OR (state != 'active' AND terminal_at_unix_ms IS NOT NULL)
                ),
                UNIQUE(task_id, attempt_number),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(assignee_agent_id) REFERENCES wc_agent_identities(agent_id)
            );
            CREATE TABLE IF NOT EXISTS wc_agent_task_coding_runs (
                task_id TEXT NOT NULL,
                attempt_id TEXT NOT NULL UNIQUE,
                run_id TEXT NOT NULL UNIQUE,
                runtime_project_id TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_instance_id TEXT NOT NULL,
                authority_fingerprint TEXT NOT NULL,
                coding_agent_intent_fingerprint TEXT NOT NULL,
                binding_intent_fingerprint TEXT NOT NULL,
                dispatch_state TEXT NOT NULL CHECK(dispatch_state IN ('prepared', 'not_started', 'outcome_unknown', 'bound', 'terminal')),
                last_observed_run_state TEXT,
                last_observed_execution_state TEXT,
                last_observation_revision INTEGER,
                terminal_stop_reason TEXT,
                terminal_error_code TEXT,
                terminal_message TEXT,
                completed_at_unix INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(attempt_id) REFERENCES wc_agent_task_attempts(attempt_id)
            );
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_coding_runs_task
                ON wc_agent_task_coding_runs(task_id, updated_at_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS wc_agent_task_endpoint_executions (
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
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_endpoint_executions_task
                ON wc_agent_task_endpoint_executions(task_id, updated_at_unix_ms DESC);

            CREATE UNIQUE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_one_active
                ON wc_agent_task_attempts(task_id) WHERE state = 'active';
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_task_number
                ON wc_agent_task_attempts(task_id, attempt_number DESC);
            CREATE INDEX IF NOT EXISTS idx_wc_agent_task_attempts_assignee
                ON wc_agent_task_attempts(assignee_agent_id, state, lease_expires_at_unix_ms);

            -- Model-facing selector only. Rows are immutable: a newer Attempt,
            -- assignee, fence, or controller generation inserts a new index and
            -- never rewrites an older one. The stored fence is the same proof
            -- start already issued; this row is not authority.
            CREATE TABLE IF NOT EXISTS wc_agent_task_attempt_references (
                principal_kind TEXT NOT NULL,
                principal_digest TEXT NOT NULL,
                ref_index INTEGER NOT NULL CHECK(ref_index >= 1),
                task_id TEXT NOT NULL,
                attempt_id TEXT NOT NULL,
                assignee_agent_id TEXT NOT NULL,
                attempt_fence TEXT NOT NULL,
                attempt_controller_generation INTEGER NOT NULL CHECK(attempt_controller_generation >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(principal_kind, principal_digest, ref_index),
                UNIQUE(
                    principal_kind, principal_digest, task_id, attempt_id, assignee_agent_id,
                    attempt_fence, attempt_controller_generation
                )
            );
            ",
        )?;
        Ok(())
    }

    pub fn create_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        input: NewAgentTask,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.create_agent_task_at(principal, input, now_unix_ms())
    }

    pub(crate) fn create_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        input: NewAgentTask,
        now: i64,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let title = validate_title(&input.title)?;
        let instruction = validate_instruction(&input.instruction)?;
        let assignee_agent_id = validate_optional_id(
            input.assignee_agent_id.as_deref(),
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let source_conversation_id = validate_optional_id(
            input.source_conversation_id.as_deref(),
            CONVERSATION_ID_PREFIX,
            "invalid_conversation_id",
        )?;
        let source_message_id = validate_optional_id(
            input.source_message_id.as_deref(),
            CONVERSATION_MESSAGE_ID_PREFIX,
            "invalid_message_id",
        )?;
        if source_message_id.is_some() && source_conversation_id.is_none() {
            return Err(CommunicationStoreError::new(
                "agent_task_source_invalid",
                "source_message_id requires source_conversation_id",
            ));
        }
        let referenced_project_id =
            validate_project_reference(input.referenced_project_id.as_deref())?;
        let idempotency_key = validate_idempotency_key(&input.idempotency_key)?;
        let request_hash = task_request_hash(&json!({
            "title": title,
            "instruction": instruction,
            "assignee_agent_id": assignee_agent_id,
            "source_conversation_id": source_conversation_id,
            "source_message_id": source_message_id,
            "referenced_project_id": referenced_project_id,
        }));

        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;

        if let Some(task_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_CREATE_AGENT_TASK,
            &idempotency_key,
            &request_hash,
        )? {
            let task = load_owned_task(&transaction, principal, &task_id, now)?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskMutation {
                task: task.detail(now),
                created: false,
                replayed: true,
                state_changed: false,
            });
        }

        if let Some(agent_id) = assignee_agent_id.as_deref() {
            require_owned_agent(&transaction, principal, agent_id)?;
        }
        validate_source_references(
            &transaction,
            principal,
            source_conversation_id.as_deref(),
            source_message_id.as_deref(),
        )?;

        let task_id = allocate_identity(
            &transaction,
            AGENT_TASK_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_tasks WHERE task_id = ?1)",
        )?;
        transaction
            .execute(
                "INSERT INTO wc_agent_tasks (
                    task_id, owner_principal_kind, owner_principal_digest,
                    assignee_agent_id, title, instruction, source_conversation_id,
                    source_message_id, referenced_project_id, state, latest_attempt_id,
                    terminal_attempt_id, created_at_unix_ms, updated_at_unix_ms,
                    terminal_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'ready', NULL, NULL, ?10, ?10, NULL)",
                params![
                    task_id,
                    principal.kind,
                    principal.digest,
                    assignee_agent_id,
                    title,
                    instruction,
                    source_conversation_id,
                    source_message_id,
                    referenced_project_id,
                    now,
                ],
            )
            .map_err(store_error)?;
        record_idempotent_resource(
            &transaction,
            principal,
            OP_CREATE_AGENT_TASK,
            &idempotency_key,
            &request_hash,
            &task_id,
            now,
        )?;
        let task = load_owned_task(&transaction, principal, &task_id, now)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskMutation {
            task: task.detail(now),
            created: true,
            replayed: false,
            state_changed: true,
        })
    }

    pub fn list_agent_tasks(
        &self,
        principal: &CommunicationPrincipal,
        assignee_agent_id: Option<&str>,
        offset: usize,
        limit: usize,
    ) -> Result<AgentTaskPage, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let assignee_agent_id = validate_optional_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        if limit == 0 || limit > MAX_AGENT_TASK_LIST_LIMIT {
            return Err(CommunicationStoreError::new(
                "invalid_agent_task_list_limit",
                format!("limit must be 1..={MAX_AGENT_TASK_LIST_LIMIT}"),
            ));
        }
        let now = now_unix_ms();
        let conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let (total_count, task_ids) = if let Some(agent_id) = assignee_agent_id.as_deref() {
            let total_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                       AND assignee_agent_id = ?3",
                    params![principal.kind, principal.digest, agent_id],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(store_error)?;
            let mut statement = conn
                .prepare(
                    "SELECT task_id FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                       AND assignee_agent_id = ?3
                     ORDER BY updated_at_unix_ms DESC, task_id
                     LIMIT ?4 OFFSET ?5",
                )
                .map_err(store_error)?;
            let ids = statement
                .query_map(
                    params![
                        principal.kind,
                        principal.digest,
                        agent_id,
                        limit as i64,
                        offset as i64
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(store_error)?;
            (total_count, ids)
        } else {
            let total_count = conn
                .query_row(
                    "SELECT COUNT(*) FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2",
                    params![principal.kind, principal.digest],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(store_error)?;
            let mut statement = conn
                .prepare(
                    "SELECT task_id FROM wc_agent_tasks
                     WHERE owner_principal_kind = ?1 AND owner_principal_digest = ?2
                     ORDER BY updated_at_unix_ms DESC, task_id
                     LIMIT ?3 OFFSET ?4",
                )
                .map_err(store_error)?;
            let ids = statement
                .query_map(
                    params![
                        principal.kind,
                        principal.digest,
                        limit as i64,
                        offset as i64
                    ],
                    |row| row.get::<_, String>(0),
                )
                .map_err(store_error)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(store_error)?;
            (total_count, ids)
        };
        let tasks = task_ids
            .iter()
            .map(|task_id| {
                load_owned_task(&conn, principal, task_id, now).map(|task| task.summary(now))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_offset = if offset.saturating_add(tasks.len()) < total_count as usize {
            Some(offset.saturating_add(tasks.len()))
        } else {
            None
        };
        Ok(AgentTaskPage {
            total_count,
            offset,
            next_offset,
            truncated: next_offset.is_some(),
            tasks,
        })
    }

    pub fn read_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
    ) -> Result<AgentTaskDetail, CommunicationStoreError> {
        self.read_agent_task_at(principal, task_id, now_unix_ms())
    }

    /// The current Attempt pin when that Attempt is the task assignee's live lease.
    /// Expired, terminal, or assignee-mismatched Attempts return `None` and never
    /// publish the fence on a Task summary.
    pub fn live_agent_task_attempt_pin(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
    ) -> Result<Option<LiveAgentTaskAttemptPin>, CommunicationStoreError> {
        self.live_agent_task_attempt_pin_at(principal, task_id, now_unix_ms())
    }

    pub(crate) fn live_agent_task_attempt_pin_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        now: i64,
    ) -> Result<Option<LiveAgentTaskAttemptPin>, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        let conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let task = load_owned_task(&conn, principal, task_id, now)?;
        let Some(attempt) = task.latest_attempt.as_ref() else {
            return Ok(None);
        };
        if attempt.effective_state(now) != AgentTaskAttemptState::Active
            || task.assignee_agent_id.as_deref() != Some(attempt.assignee_agent_id.as_str())
        {
            return Ok(None);
        }
        Ok(Some(LiveAgentTaskAttemptPin {
            attempt_id: attempt.attempt_id.clone(),
            assignee_agent_id: attempt.assignee_agent_id.clone(),
            attempt_fence: attempt.attempt_fence.clone(),
            attempt_controller_generation: attempt.attempt_controller_generation,
        }))
    }

    pub(crate) fn read_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        now: i64,
    ) -> Result<AgentTaskDetail, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        let conn = self.lock_connection(crate::StoreDomain::AgentTask);
        Ok(load_owned_task(&conn, principal, task_id, now)?.detail(now))
    }

    pub fn assign_agent_task(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.assign_agent_task_with_now(principal, task_id, assignee_agent_id, None)
    }

    #[cfg(test)]
    pub(crate) fn assign_agent_task_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        now: i64,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        self.assign_agent_task_with_now(principal, task_id, assignee_agent_id, Some(now))
    }

    fn assign_agent_task_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let mut task = load_owned_task(&transaction, principal, task_id, now)?;
        if task.stored_state.terminal() {
            return Err(task_terminal_error());
        }
        require_owned_agent(&transaction, principal, assignee_agent_id)?;
        materialize_expired_latest_attempt(&transaction, &mut task, now)?;
        if task.assignee_agent_id.as_deref() != Some(assignee_agent_id) {
            if let Some(error) = blocking_execution_error(task.latest_coding_run.as_ref()) {
                return Err(error);
            }
        }
        if task
            .latest_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.stored_state == AgentTaskAttemptState::Active)
        {
            if task.assignee_agent_id.as_deref() == Some(assignee_agent_id) {
                transaction.commit().map_err(store_error)?;
                return Ok(AgentTaskMutation {
                    task: task.detail(now),
                    created: false,
                    replayed: false,
                    state_changed: false,
                });
            }
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_active",
                "AgentTask has an unexpired active Attempt; reassignment is fenced",
            ));
        }
        if task.assignee_agent_id.as_deref() == Some(assignee_agent_id) {
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskMutation {
                task: task.detail(now),
                created: false,
                replayed: false,
                state_changed: false,
            });
        }
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET assignee_agent_id = ?2, state = 'ready', updated_at_unix_ms = ?3
                 WHERE task_id = ?1",
                params![task_id, assignee_agent_id, now.max(task.updated_at_unix_ms)],
            )
            .map_err(store_error)?;
        task = load_owned_task(&transaction, principal, task_id, now)?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskMutation {
            task: task.detail(now),
            created: false,
            replayed: false,
            state_changed: true,
        })
    }

    pub fn start_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        self.start_agent_task_attempt_with_now(
            principal,
            task_id,
            assignee_agent_id,
            idempotency_key,
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn start_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
        now: i64,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        self.start_agent_task_attempt_with_now(
            principal,
            task_id,
            assignee_agent_id,
            idempotency_key,
            Some(now),
        )
    }

    fn start_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        assignee_agent_id: &str,
        idempotency_key: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptStartMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            assignee_agent_id,
            DURABLE_AGENT_ID_PREFIX,
            "invalid_agent_id",
        )?;
        let idempotency_key = validate_idempotency_key(idempotency_key)?;
        let request_hash = task_request_hash(&json!({
            "task_id": task_id,
            "assignee_agent_id": assignee_agent_id,
        }));
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let mut task = load_owned_task(&transaction, principal, task_id, now)?;

        if let Some(attempt_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_START_AGENT_TASK_ATTEMPT,
            &idempotency_key,
            &request_hash,
        )? {
            let attempt = load_attempt_for_task(&transaction, task_id, &attempt_id, now)?
                .ok_or_else(|| {
                    CommunicationStoreError::new(
                        "agent_task_attempt_not_found",
                        "AgentTaskAttempt does not exist",
                    )
                })?;
            let record = attempt.record(now);
            let fence = attempt.attempt_fence.clone();
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskAttemptStartMutation {
                task: task.summary(now),
                attempt: record,
                attempt_fence: fence,
                replayed: true,
                state_changed: false,
            });
        }

        if task.stored_state.terminal() {
            return Err(task_terminal_error());
        }
        let current_assignee = task.assignee_agent_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_unassigned",
                "AgentTask must have an explicit assignee before an Attempt can start",
            )
        })?;
        if current_assignee != assignee_agent_id {
            return Err(CommunicationStoreError::new(
                "agent_task_assignee_mismatch",
                "Requested Agent does not match the AgentTask current assignee",
            ));
        }
        require_owned_agent(&transaction, principal, assignee_agent_id)?;
        materialize_expired_latest_attempt(&transaction, &mut task, now)?;
        if let Some(error) = blocking_execution_error(task.latest_coding_run.as_ref()) {
            return Err(error);
        }
        if task
            .latest_attempt
            .as_ref()
            .is_some_and(|attempt| attempt.stored_state == AgentTaskAttemptState::Active)
        {
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_active",
                "AgentTask already has an unexpired authoritative Attempt",
            ));
        }

        let attempt_number = task
            .latest_attempt
            .as_ref()
            .map(|attempt| attempt.attempt_number.saturating_add(1))
            .unwrap_or(1);
        let attempt_id = allocate_identity(
            &transaction,
            AGENT_TASK_ATTEMPT_ID_PREFIX,
            "SELECT EXISTS(SELECT 1 FROM wc_agent_task_attempts WHERE attempt_id = ?1)",
        )?;
        let attempt_fence = new_proof(AGENT_TASK_ATTEMPT_FENCE_PREFIX);
        let lease_expires_at = now.saturating_add(DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS);
        transaction
            .execute(
                "INSERT INTO wc_agent_task_attempts (
                    attempt_id, task_id, attempt_number, assignee_agent_id, state,
                    lease_expires_at_unix_ms, attempt_fence, attempt_controller_generation,
                    created_at_unix_ms, started_at_unix_ms, terminal_at_unix_ms,
                    terminal_result, terminal_reason
                 ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, 1, ?7, ?7, NULL, NULL, NULL)",
                params![
                    attempt_id,
                    task_id,
                    attempt_number,
                    assignee_agent_id,
                    lease_expires_at,
                    attempt_fence,
                    now,
                ],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET state = 'active', latest_attempt_id = ?2, updated_at_unix_ms = ?3
                 WHERE task_id = ?1",
                params![task_id, attempt_id, now.max(task.updated_at_unix_ms)],
            )
            .map_err(store_error)?;
        record_idempotent_resource(
            &transaction,
            principal,
            OP_START_AGENT_TASK_ATTEMPT,
            &idempotency_key,
            &request_hash,
            &attempt_id,
            now,
        )?;
        task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, &attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after creation",
                )
            })?;
        let record = attempt.record(now);
        let fence = attempt.attempt_fence.clone();
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptStartMutation {
            task: task.summary(now),
            attempt: record,
            attempt_fence: fence,
            replayed: false,
            state_changed: true,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn start_agent_task_endpoint_continuation(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
    ) -> Result<AgentTaskEndpointExecutionMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.start_agent_task_endpoint_continuation_with_now(principal, authority, now_unix_ms())
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn start_agent_task_endpoint_continuation_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskEndpointExecutionMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.start_agent_task_endpoint_continuation_with_now(principal, authority, now)
    }

    fn start_agent_task_endpoint_continuation_with_now(
        &self,
        principal: &CommunicationPrincipal,
        authority: AttemptAuthority<'_>,
        now: i64,
    ) -> Result<AgentTaskEndpointExecutionMutation, CommunicationStoreError> {
        let task_id = authority.task_id;
        let attempt_id = authority.attempt_id;
        let assignee_agent_id = authority.assignee_agent_id;
        let start_identity_fingerprint = digest_json(
            "webcodex.agent-task.endpoint-continuation.start.v1",
            &authority.identity_json(),
        )
        .expect("AgentTask endpoint continuation identity serializes");
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;

        if let Some(existing_fingerprint) = transaction
            .query_row(
                "SELECT start_identity_fingerprint
                 FROM wc_agent_task_endpoint_executions
                 WHERE task_id = ?1 AND attempt_id = ?2",
                params![task_id, attempt_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(store_error)?
        {
            if existing_fingerprint != start_identity_fingerprint {
                return Err(CommunicationStoreError::new(
                    "agent_task_endpoint_execution_binding_conflict",
                    "AgentTaskAttempt Endpoint execution already exists for a different exact start identity",
                ));
            }
            let execution = load_endpoint_execution_for_attempt(&transaction, task_id, attempt_id)?
                .ok_or_else(|| {
                    CommunicationStoreError::new(
                        "agent_task_storage_invariant",
                        "AgentTask Endpoint execution disappeared during exact replay",
                    )
                })?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskEndpointExecutionMutation {
                execution,
                replayed: true,
                state_changed: false,
            });
        }

        let _attempt = require_current_attempt(&transaction, &task, authority, now)?;
        if load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?.is_some() {
            return Err(CommunicationStoreError::new(
                "agent_task_execution_backend_conflict",
                "AgentTaskAttempt is already bound to the CodingAgentRun execution backend",
            ));
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
                 ) VALUES (?1, ?2, ?3, NULL, NULL, NULL, NULL, NULL, NULL, ?4, ?5, NULL,
                           'pending', 1, ?6, ?6, NULL, NULL, NULL, NULL, NULL, NULL, NULL)",
                params![
                    wake_id,
                    assignee_agent_id,
                    WAKE_TRIGGER_AGENT_TASK_ATTEMPT,
                    task_id,
                    attempt_id,
                    now,
                ],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "INSERT INTO wc_agent_task_endpoint_executions (
                    task_id, attempt_id, wake_id, start_identity_fingerprint,
                    endpoint_id, endpoint_controller_generation,
                    created_at_unix_ms, updated_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, NULL, NULL, ?5, ?5)",
                params![
                    task_id,
                    attempt_id,
                    wake_id,
                    start_identity_fingerprint,
                    now,
                ],
            )
            .map_err(store_error)?;
        let execution = load_endpoint_execution_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask Endpoint execution disappeared after creation",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskEndpointExecutionMutation {
            execution,
            replayed: false,
            state_changed: true,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn heartbeat_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.heartbeat_agent_task_attempt_with_now(principal, authority, None, None, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn heartbeat_agent_task_attempt_with_active_turn_proof(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        active_turn_wake_id: Option<&str>,
        active_turn_consume_token: Option<&str>,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.heartbeat_agent_task_attempt_with_now(
            principal,
            authority,
            active_turn_wake_id,
            active_turn_consume_token,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.heartbeat_agent_task_attempt_with_now(principal, authority, None, None, Some(now))
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn heartbeat_agent_task_attempt_with_active_turn_proof_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        active_turn_wake_id: Option<&str>,
        active_turn_consume_token: Option<&str>,
        now: i64,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.heartbeat_agent_task_attempt_with_now(
            principal,
            authority,
            active_turn_wake_id,
            active_turn_consume_token,
            Some(now),
        )
    }

    fn heartbeat_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        authority: AttemptAuthority<'_>,
        active_turn_wake_id: Option<&str>,
        active_turn_consume_token: Option<&str>,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptHeartbeatMutation, CommunicationStoreError> {
        let task_id = authority.task_id;
        let attempt_id = authority.attempt_id;
        let assignee_agent_id = authority.assignee_agent_id;
        let active_turn_proof = match (active_turn_wake_id, active_turn_consume_token) {
            (None, None) => None,
            (Some(wake_id), Some(consume_token)) => {
                validate_id(
                    wake_id,
                    AGENT_WAKE_ID_PREFIX,
                    "invalid_agent_task_active_turn_wake_id",
                )?;
                validate_proof(
                    consume_token,
                    AGENT_WAKE_CONSUME_TOKEN_PREFIX,
                    "invalid_agent_task_active_turn_consume_token",
                )?;
                Some((wake_id, consume_token))
            }
            _ => {
                return Err(CommunicationStoreError::new(
                    "invalid_agent_task_active_turn_proof",
                    "active-turn wake_id and consume_token must be provided together",
                ));
            }
        };
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt = require_current_attempt(&transaction, &task, authority, now)?;

        let renewal_window_ms = if let Some((wake_id, consume_token)) = active_turn_proof {
            let consume_token_hash =
                digest_text("webcodex.agent-wake.consume-token.v1", consume_token);
            let proof_matches: bool = transaction
                .query_row(
                    "SELECT EXISTS(
                         SELECT 1
                         FROM wc_agent_wakes w
                         JOIN wc_agent_wake_attempts wa
                           ON wa.attempt_id = w.claimed_attempt_id
                          AND wa.wake_id = w.wake_id
                         JOIN wc_agent_task_endpoint_executions e
                           ON e.wake_id = w.wake_id
                          AND e.task_id = w.source_task_id
                          AND e.attempt_id = w.source_task_attempt_id
                         WHERE w.wake_id = ?1
                           AND w.trigger_kind = 'agent_task_attempt'
                           AND w.source_task_id = ?2
                           AND w.source_task_attempt_id = ?3
                           AND w.target_agent_id = ?4
                           AND w.state = 'consumed'
                           AND w.consumed_at_unix_ms IS NOT NULL
                           AND wa.state = 'consumed'
                           AND wa.consumed_at_unix_ms IS NOT NULL
                           AND wa.consume_token_hash = ?5
                           AND wa.endpoint_id = w.consumed_by_endpoint_id
                           AND wa.controller_generation = w.consumed_controller_generation
                           AND e.endpoint_id IS NOT NULL
                           AND e.endpoint_id = w.consumed_by_endpoint_id
                           AND e.endpoint_controller_generation = w.consumed_controller_generation
                     )",
                    params![
                        wake_id,
                        task_id,
                        attempt_id,
                        assignee_agent_id,
                        consume_token_hash,
                    ],
                    |row| row.get(0),
                )
                .map_err(store_error)?;
            if !proof_matches {
                return Err(CommunicationStoreError::new(
                    "agent_task_active_turn_proof_invalid",
                    "active-turn proof does not identify the exact consumed A4b model turn",
                ));
            }
            AGENT_TASK_ENDPOINT_TAKEOVER_LEASE_MS
        } else {
            DEFAULT_AGENT_TASK_ATTEMPT_LEASE_MS
        };

        let new_lease_expires_at = attempt
            .lease_expires_at_unix_ms
            .max(now.saturating_add(renewal_window_ms));
        let state_changed = new_lease_expires_at > attempt.lease_expires_at_unix_ms;
        if state_changed {
            transaction
                .execute(
                    "UPDATE wc_agent_task_attempts
                     SET lease_expires_at_unix_ms = ?2
                     WHERE attempt_id = ?1",
                    params![attempt_id, new_lease_expires_at],
                )
                .map_err(store_error)?;
            transaction
                .execute(
                    "UPDATE wc_agent_tasks SET updated_at_unix_ms = MAX(updated_at_unix_ms, ?2)
                     WHERE task_id = ?1",
                    params![task_id, now],
                )
                .map_err(store_error)?;
        }
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after heartbeat",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptHeartbeatMutation {
            task: task.summary(now),
            attempt: attempt.record(now),
            state_changed,
        })
    }

    /// Replace only the carrier/controller generation for the same live Attempt.
    /// A3 keeps this internal until a concrete backend needs it; the durable
    /// primitive exists now so stale carriers are fenced independently from
    /// Attempt retry identity.
    #[allow(dead_code)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn replace_agent_task_attempt_controller_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        expected_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskAttemptRecord, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            expected_controller_generation,
        )?;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let record = replace_agent_task_attempt_controller_in_transaction(
            &transaction,
            principal,
            authority,
            now,
        )?;
        transaction.commit().map_err(store_error)?;
        Ok(record)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn complete_agent_task_attempt(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.complete_agent_task_attempt_with_now(
            principal,
            authority,
            outcome,
            terminal_result,
            terminal_reason,
            completion_key,
            None,
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn complete_agent_task_attempt_at(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
        now: i64,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.complete_agent_task_attempt_with_now(
            principal,
            authority,
            outcome,
            terminal_result,
            terminal_reason,
            completion_key,
            Some(now),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn complete_agent_task_attempt_with_now(
        &self,
        principal: &CommunicationPrincipal,
        authority: AttemptAuthority<'_>,
        outcome: AgentTaskState,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
        completion_key: &str,
        now: Option<i64>,
    ) -> Result<AgentTaskAttemptCompletionMutation, CommunicationStoreError> {
        let task_id = authority.task_id;
        let attempt_id = authority.attempt_id;
        let assignee_agent_id = authority.assignee_agent_id;
        if !outcome.terminal() {
            return Err(CommunicationStoreError::new(
                "invalid_agent_task_completion_outcome",
                "AgentTaskAttempt completion outcome must be succeeded or failed",
            ));
        }
        let terminal_result = validate_optional_terminal_text(terminal_result, "terminal_result")?;
        let terminal_reason = validate_optional_terminal_text(terminal_reason, "terminal_reason")?;
        let completion_key = validate_idempotency_key(completion_key)?;
        let request_hash = task_request_hash(&json!({
            "task_id": authority.task_id,
            "attempt_id": authority.attempt_id,
            "assignee_agent_id": authority.assignee_agent_id,
            "attempt_fence": authority.attempt_fence,
            "attempt_controller_generation": authority.attempt_controller_generation,
            "outcome": outcome.as_str(),
            "terminal_result": terminal_result,
            "terminal_reason": terminal_reason,
        }));
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let task = load_owned_task(&transaction, principal, task_id, now)?;

        if let Some(replayed_attempt_id) = lookup_idempotent_resource(
            &transaction,
            principal,
            OP_COMPLETE_AGENT_TASK_ATTEMPT,
            &completion_key,
            &request_hash,
        )? {
            if replayed_attempt_id != attempt_id {
                return Err(CommunicationStoreError::new(
                    "agent_task_idempotency_invariant",
                    "Completion replay points to a different AgentTaskAttempt",
                ));
            }
            let attempt = load_attempt_for_task(&transaction, task_id, attempt_id, now)?
                .ok_or_else(|| {
                    CommunicationStoreError::new(
                        "agent_task_attempt_not_found",
                        "AgentTaskAttempt does not exist",
                    )
                })?;
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskAttemptCompletionMutation {
                task: task.summary(now),
                attempt: attempt.record(now),
                replayed: true,
                state_changed: false,
                attention_event_count: 0,
                attention_target_agent_ids: Vec::new(),
                wait_target_agent_ids: Vec::new(),
            });
        }

        let _attempt = require_current_attempt(&transaction, &task, authority, now)?;
        let attempt_state = match outcome {
            AgentTaskState::Succeeded => AgentTaskAttemptState::Succeeded,
            AgentTaskState::Failed => AgentTaskAttemptState::Failed,
            AgentTaskState::Ready | AgentTaskState::Active => {
                unreachable!("validated terminal outcome")
            }
        };
        transaction
            .execute(
                "UPDATE wc_agent_task_attempts
                 SET state = ?2, terminal_at_unix_ms = ?3,
                     terminal_result = ?4, terminal_reason = ?5
                 WHERE attempt_id = ?1",
                params![
                    attempt_id,
                    attempt_state.as_str(),
                    now,
                    terminal_result,
                    terminal_reason,
                ],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET state = ?2, terminal_attempt_id = ?3, terminal_at_unix_ms = ?4,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
                 WHERE task_id = ?1",
                params![task_id, outcome.as_str(), attempt_id, now],
            )
            .map_err(store_error)?;
        retire_pre_dispatch_endpoint_execution_for_attempt(&transaction, attempt_id, now)?;
        let (attention_event_count, attention_target_agent_ids) =
            create_agent_task_terminal_attention_in_transaction(
                &transaction,
                principal,
                task_id,
                attempt_id,
                assignee_agent_id,
                outcome,
                now,
            )?;
        let wait_matches = record_agent_task_terminal_wait_matches_in_transaction(
            &transaction,
            principal,
            task_id,
            attempt_id,
            outcome,
            now,
        )?;
        record_idempotent_resource(
            &transaction,
            principal,
            OP_COMPLETE_AGENT_TASK_ATTEMPT,
            &completion_key,
            &request_hash,
            attempt_id,
            now,
        )?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after completion",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskAttemptCompletionMutation {
            task: task.summary(now),
            attempt: attempt.record(now),
            replayed: false,
            state_changed: true,
            attention_event_count,
            attention_target_agent_ids,
            wait_target_agent_ids: wait_matches.schedule_agent_ids,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn agent_task_coding_run_start_context(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
    ) -> Result<AgentTaskCodingRunStartContext, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.agent_task_coding_run_start_context_with_now(
            principal,
            project,
            authority,
            now_unix_ms(),
        )
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn agent_task_coding_run_start_context_at(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        now: i64,
    ) -> Result<AgentTaskCodingRunStartContext, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.agent_task_coding_run_start_context_with_now(principal, project, authority, now)
    }

    fn agent_task_coding_run_start_context_with_now(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        authority: AttemptAuthority<'_>,
        now: i64,
    ) -> Result<AgentTaskCodingRunStartContext, CommunicationStoreError> {
        let task_id = authority.task_id;
        let attempt_id = authority.attempt_id;
        let conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let task = load_owned_task(&conn, principal, task_id, now)?;
        let referenced_project = task.referenced_project_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_project_required",
                "AgentTask has no referenced_project_id and cannot dispatch a CodingAgentRun",
            )
        })?;
        if project != referenced_project {
            return Err(CommunicationStoreError::new(
                "agent_task_project_mismatch",
                "Requested Project does not match AgentTask referenced_project_id",
            ));
        }
        let attempt = require_current_attempt(&conn, &task, authority, now)?;
        if load_endpoint_execution_for_attempt(&conn, task_id, attempt_id)?.is_some() {
            return Err(CommunicationStoreError::new(
                "agent_task_execution_backend_conflict",
                "AgentTaskAttempt is already bound to the Agent Endpoint continuation backend",
            ));
        }
        Ok(AgentTaskCodingRunStartContext {
            task: task.detail(now),
            attempt: attempt.record(now),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare_agent_task_coding_run(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        intent: &AgentTaskCodingRunBindingIntent,
    ) -> Result<AgentTaskCodingRunPrepared, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.prepare_agent_task_coding_run_with_now(principal, project, authority, intent, None)
    }

    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_agent_task_coding_run_at(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        intent: &AgentTaskCodingRunBindingIntent,
        now: i64,
    ) -> Result<AgentTaskCodingRunPrepared, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        self.prepare_agent_task_coding_run_with_now(
            principal,
            project,
            authority,
            intent,
            Some(now),
        )
    }

    fn prepare_agent_task_coding_run_with_now(
        &self,
        principal: &CommunicationPrincipal,
        project: &str,
        authority: AttemptAuthority<'_>,
        intent: &AgentTaskCodingRunBindingIntent,
        now: Option<i64>,
    ) -> Result<AgentTaskCodingRunPrepared, CommunicationStoreError> {
        let task_id = authority.task_id;
        let attempt_id = authority.attempt_id;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now.unwrap_or_else(now_unix_ms);
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let referenced_project = task.referenced_project_id.as_deref().ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_project_required",
                "AgentTask has no referenced_project_id and cannot dispatch a CodingAgentRun",
            )
        })?;
        if project != referenced_project || intent.runtime_project_id != referenced_project {
            return Err(CommunicationStoreError::new(
                "agent_task_project_mismatch",
                "CodingAgentRun Project does not match AgentTask referenced_project_id",
            ));
        }
        let _attempt = require_current_attempt(&transaction, &task, authority, now)?;
        if load_endpoint_execution_for_attempt(&transaction, task_id, attempt_id)?.is_some() {
            return Err(CommunicationStoreError::new(
                "agent_task_execution_backend_conflict",
                "AgentTaskAttempt is already bound to the Agent Endpoint continuation backend",
            ));
        }
        if let Some(mut existing) =
            load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
        {
            let same_intent = existing.run_id == intent.run_id
                && existing.runtime_project_id == intent.runtime_project_id
                && existing.provider_id == intent.provider_id
                && existing.authority_fingerprint == intent.authority_fingerprint
                && existing.coding_agent_intent_fingerprint
                    == intent.coding_agent_intent_fingerprint
                && existing.binding_intent_fingerprint == intent.binding_intent_fingerprint;
            if !same_intent {
                return Err(CommunicationStoreError::new(
                    "agent_task_coding_run_binding_conflict",
                    "AgentTaskAttempt is already bound to a different CodingAgentRun intent",
                ));
            }
            if existing.provider_instance_id != intent.provider_instance_id {
                if matches!(
                    existing.dispatch_state,
                    AgentTaskCodingRunDispatchState::Prepared
                        | AgentTaskCodingRunDispatchState::NotStarted
                ) {
                    transaction
                        .execute(
                            "UPDATE wc_agent_task_coding_runs
                             SET provider_instance_id = ?3, updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
                             WHERE task_id = ?1 AND attempt_id = ?2",
                            params![task_id, attempt_id, intent.provider_instance_id, now],
                        )
                        .map_err(store_error)?;
                    existing =
                        load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
                            .ok_or_else(|| {
                                CommunicationStoreError::new(
                                    "agent_task_storage_invariant",
                                    "AgentTask CodingAgent binding disappeared during replay",
                                )
                            })?;
                } else {
                    return Err(CommunicationStoreError::new(
                        "agent_task_coding_run_binding_conflict",
                        "AgentTaskAttempt CodingAgent provider instance cannot change after dispatch may have started",
                    ));
                }
            }
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskCodingRunPrepared {
                binding: existing,
                replayed: true,
            });
        }
        let conflicting_attempt = transaction
            .query_row(
                "SELECT attempt_id FROM wc_agent_task_coding_runs WHERE run_id = ?1",
                [intent.run_id.as_str()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(store_error)?;
        if conflicting_attempt.is_some() {
            return Err(CommunicationStoreError::new(
                "agent_task_coding_run_binding_conflict",
                "CodingAgentRun is already bound to another AgentTaskAttempt",
            ));
        }
        transaction
            .execute(
                "INSERT INTO wc_agent_task_coding_runs (
                    task_id, attempt_id, run_id, runtime_project_id, provider_id,
                    provider_instance_id, authority_fingerprint, coding_agent_intent_fingerprint,
                    binding_intent_fingerprint, dispatch_state, created_at_unix_ms,
                    updated_at_unix_ms
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'prepared', ?10, ?10)",
                params![
                    task_id,
                    attempt_id,
                    intent.run_id,
                    intent.runtime_project_id,
                    intent.provider_id,
                    intent.provider_instance_id,
                    intent.authority_fingerprint,
                    intent.coding_agent_intent_fingerprint,
                    intent.binding_intent_fingerprint,
                    now,
                ],
            )
            .map_err(store_error)?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared after prepare",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskCodingRunPrepared {
            binding,
            replayed: false,
        })
    }

    pub fn read_agent_task_coding_run_binding(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
    ) -> Result<AgentTaskCodingRunBindingRecord, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        validate_id(task_id, AGENT_TASK_ID_PREFIX, "invalid_agent_task_id")?;
        validate_id(
            attempt_id,
            AGENT_TASK_ATTEMPT_ID_PREFIX,
            "invalid_agent_task_attempt_id",
        )?;
        let conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let task = load_owned_task(&conn, principal, task_id, now_unix_ms())?;
        load_attempt_for_task(&conn, task_id, attempt_id, now_unix_ms())?.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_attempt_not_found",
                "AgentTaskAttempt does not exist",
            )
        })?;
        load_coding_run_binding_for_attempt(&conn, &task.task_id, attempt_id)?.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_coding_run_not_found",
                "AgentTaskAttempt has no bound CodingAgentRun",
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn claim_agent_task_coding_run_dispatch(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        assignee_agent_id: &str,
        attempt_fence: &str,
        attempt_controller_generation: i64,
        binding_intent_fingerprint: &str,
    ) -> Result<AgentTaskCodingRunDispatchClaim, CommunicationStoreError> {
        let authority = AttemptAuthority::validated(
            principal,
            task_id,
            attempt_id,
            assignee_agent_id,
            attempt_fence,
            attempt_controller_generation,
        )?;
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let now = now_unix_ms();
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let _attempt = require_current_attempt(&transaction, &task, authority, now)?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_coding_run_not_found",
                    "AgentTaskAttempt has no prepared CodingAgentRun binding",
                )
            })?;
        if binding.binding_intent_fingerprint != binding_intent_fingerprint {
            return Err(CommunicationStoreError::new(
                "agent_task_coding_run_binding_conflict",
                "CodingAgentRun binding intent does not match the durable Attempt binding",
            ));
        }
        let may_dispatch = matches!(
            binding.dispatch_state,
            AgentTaskCodingRunDispatchState::Prepared | AgentTaskCodingRunDispatchState::NotStarted
        );
        if may_dispatch {
            transaction
                .execute(
                    "UPDATE wc_agent_task_coding_runs
                     SET dispatch_state = 'outcome_unknown', updated_at_unix_ms = MAX(updated_at_unix_ms, ?3)
                     WHERE task_id = ?1 AND attempt_id = ?2
                       AND dispatch_state IN ('prepared', 'not_started')",
                    params![task_id, attempt_id, now],
                )
                .map_err(store_error)?;
        }
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared during dispatch fencing",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskCodingRunDispatchClaim {
            binding,
            may_dispatch,
        })
    }

    pub fn record_agent_task_coding_run_not_started(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        run_id: &str,
    ) -> Result<AgentTaskCodingRunBindingRecord, CommunicationStoreError> {
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let _task = load_owned_task(&transaction, principal, task_id, now)?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_coding_run_not_found",
                    "AgentTaskAttempt has no bound CodingAgentRun",
                )
            })?;
        if binding.run_id != run_id {
            return Err(CommunicationStoreError::new(
                "agent_task_coding_run_identity_mismatch",
                "CodingAgentRun does not match the durable AgentTaskAttempt binding",
            ));
        }
        if binding.dispatch_state == AgentTaskCodingRunDispatchState::OutcomeUnknown {
            transaction
                .execute(
                    "UPDATE wc_agent_task_coding_runs
                     SET dispatch_state = 'not_started', updated_at_unix_ms = MAX(updated_at_unix_ms, ?3)
                     WHERE task_id = ?1 AND attempt_id = ?2 AND dispatch_state = 'outcome_unknown'",
                    params![task_id, attempt_id, now],
                )
                .map_err(store_error)?;
        }
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared after NotStarted observation",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(binding)
    }

    pub fn record_agent_task_coding_run_observation(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        observation: &AgentTaskCodingRunObservation,
    ) -> Result<AgentTaskCodingRunBindingRecord, CommunicationStoreError> {
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let _task = load_owned_task(&transaction, principal, task_id, now)?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_coding_run_not_found",
                    "AgentTaskAttempt has no bound CodingAgentRun",
                )
            })?;
        let (binding, _) =
            merge_agent_task_coding_run_observation(&transaction, &binding, observation, now)?;
        transaction.commit().map_err(store_error)?;
        Ok(binding)
    }

    pub fn mark_agent_task_coding_run_reconcile_unavailable(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
    ) -> Result<AgentTaskCodingRunBindingRecord, CommunicationStoreError> {
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let _task = load_owned_task(&transaction, principal, task_id, now)?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_coding_run_not_found",
                    "AgentTaskAttempt has no bound CodingAgentRun",
                )
            })?;
        if binding.dispatch_state == AgentTaskCodingRunDispatchState::Bound {
            transaction
                .execute(
                    "UPDATE wc_agent_task_coding_runs
                     SET dispatch_state = 'outcome_unknown', updated_at_unix_ms = MAX(updated_at_unix_ms, ?3)
                     WHERE task_id = ?1 AND attempt_id = ?2 AND dispatch_state = 'bound'",
                    params![task_id, attempt_id, now],
                )
                .map_err(store_error)?;
        }
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared during reconciliation",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(binding)
    }

    pub fn terminalize_agent_task_coding_run(
        &self,
        principal: &CommunicationPrincipal,
        task_id: &str,
        attempt_id: &str,
        observation: &AgentTaskCodingRunObservation,
        terminal_result: Option<&str>,
        terminal_reason: Option<&str>,
    ) -> Result<AgentTaskCodingRunReconcileMutation, CommunicationStoreError> {
        validate_communication_principal(principal)?;
        let terminal_result = validate_optional_terminal_text(terminal_result, "terminal_result")?;
        let terminal_reason = validate_optional_terminal_text(terminal_reason, "terminal_reason")?;
        let now = now_unix_ms();
        let mut conn = self.lock_connection(crate::StoreDomain::AgentTask);
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(store_error)?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        if task
            .latest_attempt
            .as_ref()
            .map(|attempt| attempt.attempt_id.as_str())
            != Some(attempt_id)
        {
            return Err(CommunicationStoreError::new(
                "agent_task_attempt_stale",
                "Bound CodingAgentRun no longer belongs to the latest authoritative AgentTaskAttempt",
            ));
        }
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_coding_run_not_found",
                    "AgentTaskAttempt has no bound CodingAgentRun",
                )
            })?;
        let (binding, disposition) =
            merge_agent_task_coding_run_observation(&transaction, &binding, observation, now)?;
        if disposition == CodingAgentObservationMerge::Stale {
            return Err(CommunicationStoreError::new(
                "agent_task_coding_run_observation_stale",
                "stale CodingAgentRun observation cannot terminalize AgentTask state",
            ));
        }
        let desired_task_state = match binding.last_observed_run_state.as_ref() {
            Some(CodingAgentRunState::Completed) => AgentTaskState::Succeeded,
            Some(CodingAgentRunState::Failed | CodingAgentRunState::Cancelled) => {
                AgentTaskState::Failed
            }
            _ => {
                return Err(CommunicationStoreError::new(
                    "agent_task_coding_run_not_terminal",
                    "CodingAgentRun is not an authoritative terminal result for AgentTask reconciliation",
                ))
            }
        };
        let observation_revision = binding.last_observation_revision.ok_or_else(|| {
            CommunicationStoreError::new(
                "agent_task_storage_invariant",
                "terminal CodingAgent reconciliation lacks an authoritative observation revision",
            )
        })?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt does not exist",
                )
            })?;
        if task.stored_state.terminal() {
            let terminal_attempt_id = transaction
                .query_row(
                    "SELECT terminal_attempt_id FROM wc_agent_tasks WHERE task_id = ?1",
                    [task_id],
                    |row| row.get::<_, Option<String>>(0),
                )
                .map_err(store_error)?;
            if terminal_attempt_id.as_deref() != Some(attempt_id)
                || task.stored_state != desired_task_state
            {
                return Err(CommunicationStoreError::new(
                    "agent_task_attempt_stale",
                    "AgentTask is already terminal from a different authoritative result",
                ));
            }
            transaction.commit().map_err(store_error)?;
            return Ok(AgentTaskCodingRunReconcileMutation {
                task: task.summary(now),
                attempt: attempt.record(now),
                binding,
                state_changed: false,
                attention_event_count: 0,
                attention_target_agent_ids: Vec::new(),
                wait_target_agent_ids: Vec::new(),
            });
        }
        let attempt_state = match desired_task_state {
            AgentTaskState::Succeeded => AgentTaskAttemptState::Succeeded,
            AgentTaskState::Failed => AgentTaskAttemptState::Failed,
            AgentTaskState::Ready | AgentTaskState::Active => unreachable!(),
        };
        transaction
            .execute(
                "UPDATE wc_agent_task_attempts
                 SET state = ?2, terminal_at_unix_ms = ?3,
                     terminal_result = ?4, terminal_reason = ?5
                 WHERE attempt_id = ?1",
                params![
                    attempt_id,
                    attempt_state.as_str(),
                    now,
                    terminal_result,
                    terminal_reason,
                ],
            )
            .map_err(store_error)?;
        transaction
            .execute(
                "UPDATE wc_agent_tasks
                 SET state = ?2, terminal_attempt_id = ?3, terminal_at_unix_ms = ?4,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
                 WHERE task_id = ?1",
                params![task_id, desired_task_state.as_str(), attempt_id, now],
            )
            .map_err(store_error)?;
        let binding_updated = transaction
            .execute(
                "UPDATE wc_agent_task_coding_runs
                 SET dispatch_state = 'terminal',
                     terminal_at_unix_ms = ?4,
                     updated_at_unix_ms = MAX(updated_at_unix_ms, ?4)
                 WHERE task_id = ?1 AND attempt_id = ?2 AND last_observation_revision = ?3",
                params![task_id, attempt_id, observation_revision, now],
            )
            .map_err(store_error)?;
        if binding_updated != 1 {
            return Err(CommunicationStoreError::new(
                "agent_task_storage_invariant",
                "terminal CodingAgent binding revision CAS did not update the exact durable binding",
            ));
        }
        let (attention_event_count, attention_target_agent_ids) =
            create_agent_task_terminal_attention_in_transaction(
                &transaction,
                principal,
                task_id,
                attempt_id,
                &attempt.assignee_agent_id,
                desired_task_state,
                now,
            )?;
        let wait_matches = record_agent_task_terminal_wait_matches_in_transaction(
            &transaction,
            principal,
            task_id,
            attempt_id,
            desired_task_state,
            now,
        )?;
        let task = load_owned_task(&transaction, principal, task_id, now)?;
        let attempt =
            load_attempt_for_task(&transaction, task_id, attempt_id, now)?.ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_attempt_not_found",
                    "AgentTaskAttempt disappeared after backend reconciliation",
                )
            })?;
        let binding = load_coding_run_binding_for_attempt(&transaction, task_id, attempt_id)?
            .ok_or_else(|| {
                CommunicationStoreError::new(
                    "agent_task_storage_invariant",
                    "AgentTask CodingAgent binding disappeared after terminal reconciliation",
                )
            })?;
        transaction.commit().map_err(store_error)?;
        Ok(AgentTaskCodingRunReconcileMutation {
            task: task.summary(now),
            attempt: attempt.record(now),
            binding,
            state_changed: true,
            attention_event_count,
            attention_target_agent_ids,
            wait_target_agent_ids: wait_matches.schedule_agent_ids,
        })
    }
}
