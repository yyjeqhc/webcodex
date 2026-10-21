-- Wake schema emitted by the published v0.4.6 initializer.
-- Source: 5072608fe74b2bfa6e8f208515c04242f475582f
-- crates/webcodex-store/src/agent_wake.rs::ensure_agent_wake_schema
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
