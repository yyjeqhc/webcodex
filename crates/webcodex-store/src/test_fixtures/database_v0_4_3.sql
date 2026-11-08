-- Empty database schema emitted by Database::open from git archive v0.4.3.
-- Source: b96a59a712ca5355ad8609cd861cd0c7acb9f99e
-- Built on macOS with: cargo run --locked --profile dogfood -p webcodex-store
--   --example export_empty -- <temporary>/empty.sqlite
-- export_empty calls Database::open(&PathBuf::from(argv[1])); only schema SQL
-- is exported from sqlite_schema, excluding SQLite-owned autoindexes. No user data.
CREATE TABLE account_credentials (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                credential_hash TEXT NOT NULL UNIQUE,
                credential_prefix TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE TABLE action_event_workflow_links (
                event_id TEXT NOT NULL,
                workflow_session_id TEXT NOT NULL,
                workflow_session_relation TEXT NOT NULL
                    CHECK(workflow_session_relation IN ('recording', 'work_on_project')),
                project TEXT,
                linked_at_ms INTEGER NOT NULL,
                PRIMARY KEY(event_id, workflow_session_id, workflow_session_relation),
                FOREIGN KEY(event_id) REFERENCES action_events(event_id) ON DELETE CASCADE
            );
CREATE TABLE action_events (
                event_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                ended_at INTEGER NOT NULL,
                duration_ms INTEGER NOT NULL,
                endpoint TEXT NOT NULL,
                operation TEXT,
                action_name TEXT NOT NULL,
                project TEXT,
                principal_kind TEXT,
                principal_user_id TEXT,
                oauth_client_id TEXT,
                status TEXT NOT NULL,
                http_status INTEGER,
                error_summary TEXT,
                warning_summary TEXT,
                changed_files_json TEXT NOT NULL,
                ids_json TEXT NOT NULL,
                summary_json TEXT NOT NULL,
                request_bytes INTEGER,
                response_bytes INTEGER,
                client_window_key TEXT,
                client_window_source TEXT,
                server_trace_id TEXT,
                principal_correlation_kind TEXT,
                principal_correlation_id TEXT,
                window_started_at_ms INTEGER,
                window_ended_at_ms INTEGER,
                request_observed_at_ms INTEGER,
                response_handed_at_ms INTEGER,
                window_transition_kind TEXT,
                response_streaming INTEGER CHECK(response_streaming IS NULL OR response_streaming IN (0, 1)),
                window_continuity_eligible INTEGER CHECK(window_continuity_eligible IS NULL OR window_continuity_eligible IN (0, 1)),
                window_meaningful INTEGER NOT NULL DEFAULT 0 CHECK(window_meaningful IN (0, 1)),
                recorder_gap_session_id TEXT,
                FOREIGN KEY(session_id) REFERENCES action_sessions(session_id)
            );
CREATE TABLE action_sessions (
                session_id TEXT PRIMARY KEY,
                title TEXT,
                note TEXT,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                closed_at INTEGER,
                first_event_at INTEGER,
                last_event_at INTEGER,
                total_actions INTEGER NOT NULL DEFAULT 0,
                success_count INTEGER NOT NULL DEFAULT 0,
                failed_count INTEGER NOT NULL DEFAULT 0,
                timeout_or_unknown_count INTEGER NOT NULL DEFAULT 0,
                warning_count INTEGER NOT NULL DEFAULT 0,
                total_duration_ms INTEGER NOT NULL DEFAULT 0,
                changed_files_count INTEGER NOT NULL DEFAULT 0,
                job_ids_count INTEGER NOT NULL DEFAULT 0
            );
CREATE TABLE admin_project_idempotency (
                subject TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                http_status INTEGER NOT NULL,
                response_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY(subject, action, target, key_hash)
            );
CREATE TABLE admin_project_lifecycle_audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                correlation_id TEXT NOT NULL,
                subject_type TEXT NOT NULL,
                subject_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                project TEXT NOT NULL,
                client_id TEXT,
                outcome TEXT NOT NULL,
                changed INTEGER NOT NULL,
                reason_code TEXT,
                idempotency_digest TEXT NOT NULL
            );
CREATE TABLE api_keys (
                id TEXT PRIMARY KEY,
                user_id TEXT NOT NULL,
                name TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER,
                revoked_at INTEGER,
                scopes TEXT NOT NULL DEFAULT '',
                expires_at INTEGER,
                kind TEXT NOT NULL DEFAULT 'user',
                allowed_client_id TEXT,
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE VIEW code_mode_action_traces AS
             SELECT
                 event_id,
                 session_id AS action_session_id,
                 started_at,
                 ended_at,
                 duration_ms AS outer_duration_ms,
                 operation,
                 project,
                 status,
                 http_status,
                 principal_kind,
                 principal_user_id,
                 client_window_key,
                 client_window_source,
                 server_trace_id,
                 request_observed_at_ms,
                 response_handed_at_ms,
                 CASE
                     WHEN request_observed_at_ms IS NOT NULL
                      AND response_handed_at_ms IS NOT NULL
                      AND response_handed_at_ms >= request_observed_at_ms
                     THEN response_handed_at_ms - request_observed_at_ms
                 END AS service_ms,
                 window_transition_kind,
                 response_streaming,
                 window_continuity_eligible,
                 window_meaningful,
                 json_extract(summary_json, '$.model_ergonomics.duration_ms')
                     AS model_runtime_duration_ms,
                 json_extract(summary_json, '$.model_ergonomics.serialized_result_bytes')
                     AS serialized_result_bytes,
                 json_extract(summary_json, '$.model_ergonomics.context_recovery_bytes')
                     AS context_recovery_bytes,
                 json_extract(summary_json, '$.model_ergonomics.session_recovery_event_count')
                     AS session_recovery_event_count,
                 json_extract(summary_json, '$.model_ergonomics.session_recovery_truncated')
                     AS session_recovery_truncated,
                 json_extract(summary_json, '$.model_ergonomics.session_history_lost')
                     AS session_history_lost,
                 CASE
                     WHEN json_type(summary_json, '$.code_mode_composition') = 'object' THEN 1
                     ELSE 0
                 END AS composition_available,
                 json_extract(summary_json, '$.code_mode_composition.input_bytes') AS input_bytes,
                 json_extract(summary_json, '$.code_mode_composition.returned_bytes') AS returned_bytes,
                 json_extract(summary_json, '$.code_mode_composition.nested_raw_result_bytes_total')
                     AS nested_raw_result_bytes_total,
                 json_extract(summary_json, '$.code_mode_composition.nested_calls') AS nested_calls,
                 json_extract(summary_json, '$.code_mode_composition.nested_successes')
                     AS nested_successes,
                 json_extract(summary_json, '$.code_mode_composition.nested_failures')
                     AS nested_failures,
                 json_extract(summary_json, '$.code_mode_composition.max_in_flight')
                     AS max_in_flight,
                 json_extract(summary_json, '$.code_mode_composition.duration_ms')
                     AS composition_duration_ms,
                 json_extract(summary_json, '$.code_mode_composition.slot_wait_ms') AS slot_wait_ms,
                 json_extract(summary_json, '$.code_mode_composition.consequential_calls')
                     AS consequential_calls,
                 json_extract(summary_json, '$.code_mode_composition.known_results') AS known_results,
                 json_extract(summary_json, '$.code_mode_composition.job_handoffs') AS job_handoffs,
                 json_extract(summary_json, '$.code_mode_composition.outcome_unknown') AS outcome_unknown,
                 json_extract(summary_json, '$.code_mode_composition.nested_tool_counts')
                     AS nested_tool_counts_json
             FROM action_events
             WHERE operation IN (
                 'code_mode_exec', 'code_mode_exec_effectful', 'code_mode_exec_mutating'
             );
CREATE VIEW code_mode_nested_tool_usage AS
             SELECT
                 traces.event_id,
                 traces.action_session_id,
                 traces.started_at,
                 traces.operation,
                 traces.project,
                 traces.status,
                 tools.key AS tool_name,
                 CAST(tools.value AS INTEGER) AS calls
             FROM code_mode_action_traces AS traces
             JOIN json_each(traces.nested_tool_counts_json) AS tools
             WHERE tools.key IS NOT NULL
               AND tools.type = 'integer'
               AND CAST(tools.value AS INTEGER) >= 0;
CREATE TABLE oauth_access_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE TABLE oauth_authorization_codes (
                id TEXT PRIMARY KEY,
                code_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                redirect_uri TEXT NOT NULL,
                scopes TEXT NOT NULL DEFAULT '',
                code_challenge TEXT,
                code_challenge_method TEXT,
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER,
                revoked_at INTEGER,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE TABLE oauth_clients (
                id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL UNIQUE,
                client_secret_hash TEXT NOT NULL,
                name TEXT NOT NULL,
                owner_user_id TEXT,
                owner_project_grant_id TEXT,
                owner_shared_key_hash TEXT,
                redirect_uris TEXT NOT NULL DEFAULT '',
                allowed_scopes TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL,
                revoked_at INTEGER,
                CHECK (
                    (owner_user_id IS NOT NULL AND owner_project_grant_id IS NULL AND owner_shared_key_hash IS NULL)
                    OR (owner_user_id IS NULL AND owner_project_grant_id IS NOT NULL AND owner_shared_key_hash IS NULL)
                    OR (owner_user_id IS NULL AND owner_project_grant_id IS NULL AND owner_shared_key_hash IS NOT NULL)
                ),
                FOREIGN KEY(owner_user_id) REFERENCES users(id)
            );
CREATE TABLE oauth_refresh_tokens (
                id TEXT PRIMARY KEY,
                token_hash TEXT NOT NULL UNIQUE,
                client_id TEXT NOT NULL,
                subject_kind TEXT NOT NULL DEFAULT 'managed_user',
                subject_id TEXT NOT NULL,
                user_id TEXT,
                scopes TEXT NOT NULL DEFAULT '',
                resource TEXT,
                shared_key_hash TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                revoked_at INTEGER,
                last_used_at INTEGER,
                rotated_from_id TEXT,
                FOREIGN KEY(client_id) REFERENCES oauth_clients(client_id),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE TABLE pairing_codes (
                id TEXT PRIMARY KEY,
                code_hash TEXT NOT NULL UNIQUE,
                user_id TEXT NOT NULL,
                username TEXT NOT NULL,
                client_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                used_at INTEGER,
                user_token_name TEXT,
                agent_token_name TEXT,
                runner_capabilities INTEGER NOT NULL DEFAULT 0 CHECK(runner_capabilities IN (0, 1)),
                FOREIGN KEY(user_id) REFERENCES users(id)
            );
CREATE TABLE project_memories (
                memory_id TEXT PRIMARY KEY,
                memory_scope_id TEXT NOT NULL,
                memory_key TEXT NOT NULL,
                summary TEXT NOT NULL,
                body TEXT NOT NULL,
                priority TEXT NOT NULL CHECK(priority IN ('high', 'normal', 'low')),
                bootstrap INTEGER NOT NULL CHECK(bootstrap IN (0, 1)),
                tags_json TEXT NOT NULL,
                definition_hash TEXT NOT NULL,
                generation INTEGER NOT NULL CHECK(generation >= 1),
                revision TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                created_by_kind TEXT NOT NULL,
                created_by_principal_digest TEXT NOT NULL,
                updated_by_kind TEXT NOT NULL,
                updated_by_principal_digest TEXT NOT NULL,
                UNIQUE(memory_scope_id, memory_key)
            );
CREATE TABLE project_memory_scopes (
                memory_scope_id TEXT PRIMARY KEY,
                identity_state TEXT NOT NULL CHECK(identity_state = 'attributed'),
                project_runtime_id TEXT NOT NULL,
                runner_client_id TEXT NOT NULL,
                root_fingerprint TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                last_mutated_at_unix_ms INTEGER NOT NULL
            );
CREATE TABLE project_references (
                principal_key TEXT NOT NULL,
                ref_index INTEGER NOT NULL CHECK(ref_index >= 1),
                canonical_project_id TEXT NOT NULL,
                root_fingerprint TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                PRIMARY KEY(principal_key, ref_index),
                UNIQUE(principal_key, canonical_project_id, root_fingerprint)
            );
CREATE TABLE users (
                id TEXT PRIMARY KEY,
                username TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL,
                disabled INTEGER NOT NULL DEFAULT 0,
                display_name TEXT,
                role TEXT NOT NULL DEFAULT 'user',
                disabled_at INTEGER,
                updated_at INTEGER
            );
CREATE TABLE wc_agent_attention_events (
        event_id TEXT PRIMARY KEY,
        kind TEXT NOT NULL CHECK(kind IN ('agent_task_terminal', 'goal_workflow_stalled')),
        owner_principal_kind TEXT NOT NULL,
        owner_principal_digest TEXT NOT NULL,
        target_agent_id TEXT NOT NULL,
        goal_id TEXT NOT NULL,
        task_id TEXT,
        task_attempt_id TEXT,
        terminal_task_state TEXT,
        workflow_session_id TEXT,
        observed_window_key TEXT,
        observation_principal_kind TEXT,
        observation_principal_id TEXT,
        last_meaningful_activity_at_ms INTEGER,
        last_seen_at_ms INTEGER,
        goal_revision INTEGER,
        created_at_unix_ms INTEGER NOT NULL,
        FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
        FOREIGN KEY(goal_id) REFERENCES wc_goals(goal_id),
        FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
        FOREIGN KEY(task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id),
        CHECK (
            (kind = 'agent_task_terminal'
             AND task_id IS NOT NULL AND task_attempt_id IS NOT NULL
             AND terminal_task_state IS NOT NULL AND terminal_task_state IN ('succeeded', 'failed')
             AND workflow_session_id IS NULL AND observed_window_key IS NULL
             AND observation_principal_kind IS NULL AND observation_principal_id IS NULL
             AND last_meaningful_activity_at_ms IS NULL AND last_seen_at_ms IS NULL AND goal_revision IS NULL)
            OR
            (kind = 'goal_workflow_stalled'
             AND task_id IS NULL AND task_attempt_id IS NULL AND terminal_task_state IS NULL
             AND workflow_session_id IS NOT NULL AND observed_window_key IS NOT NULL
             AND observation_principal_kind IS NOT NULL AND observation_principal_id IS NOT NULL
             AND last_meaningful_activity_at_ms IS NOT NULL AND last_seen_at_ms IS NOT NULL
             AND goal_revision IS NOT NULL AND goal_revision >= 1
             AND last_meaningful_activity_at_ms < last_seen_at_ms
             AND last_seen_at_ms <= created_at_unix_ms)
        ),
        UNIQUE(kind, goal_id, task_attempt_id),
        UNIQUE(kind, goal_id, last_meaningful_activity_at_ms)
    );
CREATE TABLE wc_agent_deliveries (
                delivery_order INTEGER PRIMARY KEY AUTOINCREMENT,
                delivery_id TEXT NOT NULL UNIQUE,
                message_id TEXT NOT NULL,
                conversation_id TEXT NOT NULL,
                message_seq INTEGER NOT NULL CHECK(message_seq >= 1),
                recipient_agent_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK(state IN ('queued', 'consumed')),
                created_at_unix_ms INTEGER NOT NULL,
                consumed_at_unix_ms INTEGER,
                consumed_by_endpoint_id TEXT,
                UNIQUE(message_id, recipient_agent_id),
                FOREIGN KEY(conversation_id, message_id)
                    REFERENCES wc_conversation_messages(conversation_id, message_id),
                FOREIGN KEY(recipient_agent_id) REFERENCES wc_agent_identities(agent_id),
                FOREIGN KEY(consumed_by_endpoint_id) REFERENCES wc_agent_endpoints(endpoint_id)
            );
CREATE TABLE wc_agent_endpoints (
                endpoint_id TEXT PRIMARY KEY,
                agent_id TEXT NOT NULL,
                attachment_principal_kind TEXT NOT NULL,
                attachment_principal_digest TEXT NOT NULL,
                host TEXT NOT NULL,
                client_attachment_id TEXT,
                wake_capable INTEGER NOT NULL CHECK(wake_capable IN (0, 1)),
                mcp_app_recovery_fingerprint TEXT
                    CHECK(mcp_app_recovery_fingerprint IS NULL OR length(mcp_app_recovery_fingerprint) = 64),
                mcp_app_client_window_key TEXT
                    CHECK(mcp_app_client_window_key IS NULL OR length(mcp_app_client_window_key) = 64),
                controller_generation INTEGER NOT NULL CHECK(controller_generation >= 0),
                lifecycle TEXT NOT NULL CHECK(lifecycle IN ('attached', 'detached', 'expired')),
                attached_at_unix_ms INTEGER NOT NULL,
                last_seen_at_unix_ms INTEGER NOT NULL,
                lease_expires_at_unix_ms INTEGER NOT NULL,
                expired_at_unix_ms INTEGER,
                detached_at_unix_ms INTEGER,
                CHECK(
                    (lifecycle = 'attached' AND expired_at_unix_ms IS NULL AND detached_at_unix_ms IS NULL)
                    OR (lifecycle = 'expired' AND expired_at_unix_ms IS NOT NULL)
                    OR (lifecycle = 'detached' AND detached_at_unix_ms IS NOT NULL)
                ),
                FOREIGN KEY(agent_id) REFERENCES wc_agent_identities(agent_id)
            );
CREATE TABLE wc_agent_identities (
                agent_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                handle TEXT NOT NULL,
                display_name TEXT NOT NULL,
                description TEXT NOT NULL,
                specialty_labels_json TEXT NOT NULL,
                profile_revision INTEGER NOT NULL CHECK(profile_revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                current_controller_generation INTEGER NOT NULL DEFAULT 0 CHECK(current_controller_generation >= 0)
            );
CREATE TABLE wc_agent_task_attempts (
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
CREATE TABLE wc_agent_task_coding_runs (
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
CREATE TABLE wc_agent_task_endpoint_executions (
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
CREATE TABLE wc_agent_tasks (
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
CREATE TABLE wc_agent_wait_matches (
                wait_id TEXT NOT NULL,
                sequence INTEGER NOT NULL CHECK(sequence >= 1 AND sequence <= 8),
                kind TEXT NOT NULL CHECK(kind = 'agent_task_terminal'),
                task_id TEXT NOT NULL,
                task_attempt_id TEXT NOT NULL,
                terminal_task_state TEXT NOT NULL CHECK(terminal_task_state IN ('succeeded', 'failed')),
                occurred_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(wait_id, sequence),
                UNIQUE(wait_id, kind, task_id),
                FOREIGN KEY(wait_id) REFERENCES wc_agent_waits(wait_id),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id),
                FOREIGN KEY(task_attempt_id) REFERENCES wc_agent_task_attempts(attempt_id)
            );
CREATE TABLE wc_agent_wait_sources (
                wait_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL CHECK(ordinal >= 0 AND ordinal < 8),
                kind TEXT NOT NULL CHECK(kind = 'agent_task_terminal'),
                task_id TEXT NOT NULL,
                PRIMARY KEY(wait_id, ordinal),
                UNIQUE(wait_id, kind, task_id),
                FOREIGN KEY(wait_id) REFERENCES wc_agent_waits(wait_id),
                FOREIGN KEY(task_id) REFERENCES wc_agent_tasks(task_id)
            );
CREATE TABLE wc_agent_waits (
                wait_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                target_agent_id TEXT NOT NULL,
                goal_id TEXT,
                mode TEXT NOT NULL DEFAULT 'any' CHECK(mode IN ('any', 'all')),
                state TEXT NOT NULL CHECK(state IN ('waiting', 'triggered', 'resumed', 'cancelled')),
                revision INTEGER NOT NULL CHECK(revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                triggered_at_unix_ms INTEGER,
                resumed_at_unix_ms INTEGER,
                cancelled_at_unix_ms INTEGER,
                FOREIGN KEY(target_agent_id) REFERENCES wc_agent_identities(agent_id),
                CHECK((state = 'waiting' AND triggered_at_unix_ms IS NULL AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'triggered' AND triggered_at_unix_ms IS NOT NULL AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'resumed' AND triggered_at_unix_ms IS NOT NULL AND resumed_at_unix_ms IS NOT NULL AND cancelled_at_unix_ms IS NULL)
                   OR (state = 'cancelled' AND resumed_at_unix_ms IS NULL AND cancelled_at_unix_ms IS NOT NULL))
            );
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
CREATE TABLE wc_communication_idempotency (
                principal_digest TEXT NOT NULL,
                operation TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                resource_id TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(principal_digest, operation, key_hash)
            );
CREATE TABLE wc_conversation_messages (
                message_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                seq INTEGER NOT NULL CHECK(seq >= 1),
                author_participant_id TEXT NOT NULL,
                body TEXT NOT NULL,
                reply_to_message_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                UNIQUE(conversation_id, seq),
                UNIQUE(conversation_id, message_id),
                FOREIGN KEY(conversation_id) REFERENCES wc_conversations(conversation_id),
                FOREIGN KEY(author_participant_id) REFERENCES wc_conversation_participants(participant_id),
                FOREIGN KEY(conversation_id, reply_to_message_id)
                    REFERENCES wc_conversation_messages(conversation_id, message_id)
            );
CREATE TABLE wc_conversation_participants (
                participant_id TEXT PRIMARY KEY,
                conversation_id TEXT NOT NULL,
                participant_kind TEXT NOT NULL CHECK(participant_kind IN ('human', 'agent')),
                agent_id TEXT,
                principal_kind TEXT,
                principal_digest TEXT,
                joined_at_unix_ms INTEGER NOT NULL,
                CHECK(
                    (participant_kind = 'agent'
                        AND agent_id IS NOT NULL
                        AND principal_kind IS NULL
                        AND principal_digest IS NULL)
                    OR
                    (participant_kind = 'human'
                        AND agent_id IS NULL
                        AND principal_kind IS NOT NULL
                        AND principal_digest IS NOT NULL)
                ),
                FOREIGN KEY(conversation_id) REFERENCES wc_conversations(conversation_id),
                FOREIGN KEY(agent_id) REFERENCES wc_agent_identities(agent_id)
            );
CREATE TABLE wc_conversations (
                conversation_id TEXT PRIMARY KEY,
                title TEXT,
                lifecycle TEXT NOT NULL CHECK(lifecycle IN ('open', 'closed')),
                next_seq INTEGER NOT NULL CHECK(next_seq >= 1),
                created_by_principal_kind TEXT NOT NULL,
                created_by_principal_digest TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                closed_at_unix_ms INTEGER
            );
CREATE TABLE wc_external_observations (
                session_id TEXT NOT NULL,
                project TEXT NOT NULL,
                adapter_id TEXT NOT NULL,
                event_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                exit_code INTEGER,
                recorded_at INTEGER NOT NULL,
                PRIMARY KEY(session_id, adapter_id, event_id)
            );
CREATE TABLE wc_goal_correlations (
                goal_id TEXT NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('agent_task', 'workflow_session')),
                reference_id TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(goal_id, kind, reference_id),
                FOREIGN KEY(goal_id) REFERENCES wc_goals(goal_id)
            );
CREATE TABLE wc_goal_idempotency (
                principal_digest TEXT NOT NULL,
                operation TEXT NOT NULL,
                key_hash TEXT NOT NULL,
                request_hash TEXT NOT NULL,
                goal_id TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(principal_digest, operation, key_hash)
            );
CREATE TABLE wc_goals (
                goal_id TEXT PRIMARY KEY,
                owner_principal_kind TEXT NOT NULL,
                owner_principal_digest TEXT NOT NULL,
                title TEXT NOT NULL,
                objective TEXT NOT NULL,
                controller_agent_id TEXT REFERENCES wc_agent_identities(agent_id),
                lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'completed', 'cancelled')),
                revision INTEGER NOT NULL CHECK(revision >= 1),
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                terminal_at_unix_ms INTEGER,
                terminal_reason TEXT, plan_json TEXT NOT NULL DEFAULT '{"completion_conditions":[],"steps":[],"progress_summary":null,"checkpoint_at_unix_ms":null}',
                CHECK(
                    (lifecycle = 'active' AND terminal_at_unix_ms IS NULL AND terminal_reason IS NULL)
                    OR (lifecycle IN ('completed', 'cancelled') AND terminal_at_unix_ms IS NOT NULL)
                )
            );
CREATE TABLE wc_job_receipts (
                job_id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                runner_instance_id TEXT NOT NULL,
                auth_kind TEXT NOT NULL,
                auth_partition TEXT,
                owner_at_admission TEXT,
                kind TEXT NOT NULL,
                snapshot TEXT NOT NULL,
                terminal_observed_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL
            );
CREATE TABLE wc_job_terminal_waits (wait_id TEXT PRIMARY KEY, owner_kind TEXT NOT NULL, owner_digest TEXT NOT NULL,idempotency_key TEXT NOT NULL, request_digest TEXT NOT NULL,job_id TEXT NOT NULL, job_client_id TEXT NOT NULL,job_auth_kind TEXT NOT NULL CHECK(job_auth_kind IN ('shared_key','project_grant','open_anonymous','managed_owner','managed_unowned')),job_auth_value TEXT, state TEXT NOT NULL CHECK(state IN ('waiting','triggered')),delivery_state TEXT NOT NULL CHECK(delivery_state IN ('not_ready','pending','prepared','delivered','delivery_unknown')),terminal_status TEXT, terminal_outcome TEXT, terminal_observed_at INTEGER, delivery_attempt_id TEXT,created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, triggered_at INTEGER, expires_at INTEGER NOT NULL,UNIQUE(owner_kind, owner_digest, idempotency_key),CHECK((job_auth_kind IN ('shared_key','project_grant','managed_owner') AND job_auth_value IS NOT NULL) OR(job_auth_kind IN ('open_anonymous','managed_unowned') AND job_auth_value IS NULL)),CHECK((state='waiting' AND delivery_state='not_ready' AND terminal_status IS NULL AND terminal_outcome IS NULL AND terminal_observed_at IS NULL AND triggered_at IS NULL) OR(state='triggered' AND delivery_state<>'not_ready' AND terminal_status IS NOT NULL AND terminal_outcome IS NOT NULL AND terminal_observed_at IS NOT NULL AND triggered_at IS NOT NULL)),CHECK((delivery_state='prepared' AND delivery_attempt_id IS NOT NULL) OR delivery_state<>'prepared'));
CREATE TABLE window_peer_discoveries (
                principal_kind TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                observer_window_key TEXT NOT NULL,
                peer_window_key TEXT NOT NULL,
                project TEXT NOT NULL,
                first_projected_at_ms INTEGER NOT NULL,
                PRIMARY KEY(
                    principal_kind, principal_id, observer_window_key,
                    peer_window_key, project
                )
            );
CREATE TABLE window_peer_messages (
                message_id TEXT PRIMARY KEY,
                principal_kind TEXT NOT NULL,
                principal_id TEXT NOT NULL,
                sender_window_key TEXT NOT NULL,
                recipient_window_key TEXT NOT NULL,
                sender_peer_id TEXT NOT NULL,
                recipient_peer_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                priority TEXT NOT NULL,
                message TEXT NOT NULL,
                tags_json TEXT NOT NULL,
                requires_ack INTEGER NOT NULL CHECK(requires_ack IN (0, 1)),
                created_at_ms INTEGER NOT NULL,
                sender_session_id TEXT,
                sender_project TEXT,
                first_projected_at_ms INTEGER,
                last_projected_at_ms INTEGER,
                projection_count INTEGER NOT NULL DEFAULT 0,
                first_ack_observed_at_ms INTEGER,
                delivery_key_hash TEXT,
                delivery_payload_hash TEXT
            );
CREATE TABLE workspace_activity (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at INTEGER NOT NULL,
                project TEXT,
                tool TEXT NOT NULL,
                surface TEXT NOT NULL,
                success INTEGER NOT NULL,
                session_id TEXT,
                client TEXT,
                command_preview TEXT,
                paths_json TEXT NOT NULL DEFAULT '[]',
                error_summary TEXT,
                -- Attribution fixed at write time. 'legacy_unscoped' marks rows
                -- from before this column existed, whose owner cannot be proven.
                scope_kind TEXT NOT NULL DEFAULT 'legacy_unscoped',
                scope_id TEXT
            );
CREATE INDEX idx_account_credentials_hash ON account_credentials(credential_hash);
CREATE INDEX idx_account_credentials_user_id ON account_credentials(user_id);
CREATE INDEX idx_action_event_workflow_links_session
                ON action_event_workflow_links(workflow_session_id, linked_at_ms DESC);
CREATE INDEX idx_action_events_oauth_client_started
                ON action_events(oauth_client_id, started_at DESC);
CREATE INDEX idx_action_events_operation_started
                 ON action_events(operation, started_at DESC)
                 WHERE operation IS NOT NULL;
CREATE INDEX idx_action_events_principal_user_started
                ON action_events(principal_user_id, started_at DESC);
CREATE INDEX idx_action_events_session_started
                ON action_events(session_id, started_at DESC);
CREATE INDEX idx_action_events_window_completed
                ON action_events(client_window_key, window_ended_at_ms DESC, event_id DESC)
                WHERE window_started_at_ms IS NOT NULL AND window_ended_at_ms IS NOT NULL;
CREATE INDEX idx_action_events_window_meaningful_completed
                ON action_events(client_window_key, window_ended_at_ms DESC, event_id DESC)
                WHERE window_meaningful = 1 AND window_started_at_ms IS NOT NULL AND window_ended_at_ms IS NOT NULL;
CREATE INDEX idx_action_events_window_started
                ON action_events(client_window_key, window_started_at_ms DESC)
                WHERE client_window_key IS NOT NULL;
CREATE INDEX idx_action_sessions_status_last_event
                ON action_sessions(status, last_event_at DESC, updated_at DESC);
CREATE INDEX idx_admin_project_idempotency_created
                ON admin_project_idempotency(created_at DESC);
CREATE INDEX idx_admin_project_lifecycle_audit_created
                ON admin_project_lifecycle_audit(created_at DESC);
CREATE INDEX idx_api_keys_hash ON api_keys(key_hash);
CREATE INDEX idx_api_keys_user_id ON api_keys(user_id);
CREATE INDEX idx_job_receipts_expiry ON wc_job_receipts(expires_at);
CREATE INDEX idx_job_receipts_runner_history
                ON wc_job_receipts(client_id, terminal_observed_at DESC, job_id DESC);
CREATE INDEX idx_oauth_access_tokens_client ON oauth_access_tokens(client_id);
CREATE INDEX idx_oauth_access_tokens_hash ON oauth_access_tokens(token_hash);
CREATE INDEX idx_oauth_access_tokens_user ON oauth_access_tokens(user_id);
CREATE INDEX idx_oauth_auth_codes_client ON oauth_authorization_codes(client_id);
CREATE INDEX idx_oauth_auth_codes_hash ON oauth_authorization_codes(code_hash);
CREATE INDEX idx_oauth_clients_client_id ON oauth_clients(client_id);
CREATE INDEX idx_oauth_clients_owner ON oauth_clients(owner_user_id);
CREATE INDEX idx_oauth_clients_project_owner
                ON oauth_clients(owner_project_grant_id);
CREATE INDEX idx_oauth_clients_shared_key_owner
                ON oauth_clients(owner_shared_key_hash);
CREATE INDEX idx_oauth_refresh_tokens_client ON oauth_refresh_tokens(client_id);
CREATE INDEX idx_oauth_refresh_tokens_hash ON oauth_refresh_tokens(token_hash);
CREATE INDEX idx_oauth_refresh_tokens_user ON oauth_refresh_tokens(user_id);
CREATE INDEX idx_pairing_codes_expires_at ON pairing_codes(expires_at);
CREATE INDEX idx_pairing_codes_hash ON pairing_codes(code_hash);
CREATE INDEX idx_project_memories_scope_bootstrap
                ON project_memories(memory_scope_id, bootstrap, priority, memory_key);
CREATE INDEX idx_project_memories_scope_key
                ON project_memories(memory_scope_id, memory_key);
CREATE INDEX idx_wc_agent_attention_events_owner_created
                ON wc_agent_attention_events(owner_principal_digest, created_at_unix_ms, event_id);
CREATE INDEX idx_wc_agent_attention_events_target_created
                ON wc_agent_attention_events(target_agent_id, created_at_unix_ms, event_id);
CREATE INDEX idx_wc_agent_deliveries_inbox
                ON wc_agent_deliveries(recipient_agent_id, state, delivery_order);
CREATE INDEX idx_wc_agent_deliveries_message
                ON wc_agent_deliveries(message_id, recipient_agent_id);
CREATE INDEX idx_wc_agent_endpoints_agent_active
                ON wc_agent_endpoints(agent_id, detached_at_unix_ms, attached_at_unix_ms DESC);
CREATE INDEX idx_wc_agent_endpoints_agent_generation
                ON wc_agent_endpoints(agent_id, lifecycle, controller_generation DESC);
CREATE UNIQUE INDEX idx_wc_agent_endpoints_one_attached
                ON wc_agent_endpoints(agent_id) WHERE lifecycle = 'attached';
CREATE INDEX idx_wc_agent_identities_owner
                ON wc_agent_identities(owner_principal_digest, agent_id);
CREATE INDEX idx_wc_agent_identities_updated
                ON wc_agent_identities(updated_at_unix_ms DESC, agent_id);
CREATE INDEX idx_wc_agent_task_attempts_assignee
                ON wc_agent_task_attempts(assignee_agent_id, state, lease_expires_at_unix_ms);
CREATE UNIQUE INDEX idx_wc_agent_task_attempts_one_active
                ON wc_agent_task_attempts(task_id) WHERE state = 'active';
CREATE INDEX idx_wc_agent_task_attempts_task_number
                ON wc_agent_task_attempts(task_id, attempt_number DESC);
CREATE INDEX idx_wc_agent_task_coding_runs_task
                ON wc_agent_task_coding_runs(task_id, updated_at_unix_ms DESC);
CREATE INDEX idx_wc_agent_task_endpoint_executions_task
                ON wc_agent_task_endpoint_executions(task_id, updated_at_unix_ms DESC);
CREATE INDEX idx_wc_agent_tasks_assignee
                ON wc_agent_tasks(owner_principal_digest, assignee_agent_id, updated_at_unix_ms DESC);
CREATE INDEX idx_wc_agent_tasks_owner_updated
                ON wc_agent_tasks(owner_principal_digest, updated_at_unix_ms DESC, task_id);
CREATE INDEX idx_wc_agent_wait_matches_wait
                ON wc_agent_wait_matches(wait_id, sequence);
CREATE INDEX idx_wc_agent_wait_sources_task
                ON wc_agent_wait_sources(kind, task_id, wait_id);
CREATE INDEX idx_wc_agent_waits_owner_agent_state
                ON wc_agent_waits(owner_principal_kind, owner_principal_digest, target_agent_id, state, created_at_unix_ms);
CREATE INDEX idx_wc_agent_wake_attempts_endpoint
                ON wc_agent_wake_attempts(endpoint_id, controller_generation, state);
CREATE INDEX idx_wc_agent_wake_attempts_wake
                ON wc_agent_wake_attempts(wake_id, claimed_at_unix_ms, attempt_id);
CREATE UNIQUE INDEX idx_wc_agent_wakes_agent_wait
                ON wc_agent_wakes(source_wait_id)
                WHERE trigger_kind = 'agent_wait_events';
CREATE UNIQUE INDEX idx_wc_agent_wakes_attention_event
                ON wc_agent_wakes(source_event_id)
                WHERE trigger_kind = 'attention_event';
CREATE UNIQUE INDEX idx_wc_agent_wakes_one_dispatched
                ON wc_agent_wakes(target_agent_id)
                WHERE state IN ('prepared', 'delivered', 'delivery_unknown');
CREATE UNIQUE INDEX idx_wc_agent_wakes_one_queueable_inbox
                ON wc_agent_wakes(target_agent_id)
                WHERE trigger_kind = 'inbox_changed' AND state IN ('pending', 'claimed');
CREATE INDEX idx_wc_agent_wakes_target_state
                ON wc_agent_wakes(target_agent_id, state, created_at_unix_ms, wake_id);
CREATE UNIQUE INDEX idx_wc_agent_wakes_task_attempt
                ON wc_agent_wakes(source_task_attempt_id)
                WHERE trigger_kind = 'agent_task_attempt';
CREATE INDEX idx_wc_communication_idempotency_created
                ON wc_communication_idempotency(created_at_unix_ms DESC);
CREATE INDEX idx_wc_conversation_messages_order
                ON wc_conversation_messages(conversation_id, seq);
CREATE UNIQUE INDEX idx_wc_conversation_participants_agent
                ON wc_conversation_participants(conversation_id, agent_id)
                WHERE participant_kind = 'agent';
CREATE UNIQUE INDEX idx_wc_conversation_participants_human
                ON wc_conversation_participants(conversation_id, principal_digest)
                WHERE participant_kind = 'human';
CREATE INDEX idx_wc_conversation_participants_lookup
                ON wc_conversation_participants(conversation_id, participant_kind, participant_id);
CREATE INDEX idx_wc_conversations_updated
                ON wc_conversations(updated_at_unix_ms DESC, conversation_id);
CREATE INDEX idx_wc_goal_correlations_reference
                ON wc_goal_correlations(kind, reference_id, goal_id);
CREATE INDEX idx_wc_goal_idempotency_created
                ON wc_goal_idempotency(created_at_unix_ms DESC);
CREATE INDEX idx_wc_goals_owner_lifecycle
                ON wc_goals(owner_principal_digest, lifecycle, updated_at_unix_ms DESC, goal_id);
CREATE INDEX idx_wc_goals_owner_updated
                ON wc_goals(owner_principal_digest, updated_at_unix_ms DESC, goal_id);
CREATE INDEX idx_wc_job_terminal_waits_expiry ON wc_job_terminal_waits(expires_at);
CREATE INDEX idx_wc_job_terminal_waits_owner ON wc_job_terminal_waits(owner_kind,owner_digest,state,created_at);
CREATE INDEX idx_wc_job_terminal_waits_source ON wc_job_terminal_waits(job_id,job_client_id,job_auth_kind,job_auth_value,state);
CREATE UNIQUE INDEX idx_window_peer_messages_delivery
                 ON window_peer_messages(
                    principal_kind, principal_id, sender_window_key, delivery_key_hash
                 )
                 WHERE delivery_key_hash IS NOT NULL;
CREATE INDEX idx_window_peer_messages_recipient
                ON window_peer_messages(
                    principal_kind, principal_id, recipient_window_key,
                    requires_ack, first_projected_at_ms, created_at_ms
                );
CREATE INDEX idx_workspace_activity_id
                ON workspace_activity(id DESC);
CREATE INDEX idx_workspace_activity_scope
                ON workspace_activity(scope_kind, scope_id, id DESC);
