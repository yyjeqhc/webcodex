use crate::agent_wake::{
    ContinuationAdapter, ContinuationDispatchOutcome, ContinuationPreflight,
    ContinuationPreflightError,
};
use crate::db::{AgentWakeEnvelope, AgentWakeState};
use crate::tool_runtime::ToolRuntime;
use crate::{Database, ShellClientRegistry};
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct FakeHostAdapter {
    preflight_error: Option<&'static str>,
    outcome: ContinuationDispatchOutcome,
    preflight_count: AtomicUsize,
    envelopes: Mutex<Vec<AgentWakeEnvelope>>,
}

impl FakeHostAdapter {
    fn delivered() -> Self {
        Self {
            preflight_error: None,
            outcome: ContinuationDispatchOutcome::Delivered,
            preflight_count: AtomicUsize::new(0),
            envelopes: Mutex::new(Vec::new()),
        }
    }

    fn unavailable() -> Self {
        Self {
            preflight_error: Some("host_bridge_unavailable"),
            outcome: ContinuationDispatchOutcome::Delivered,
            preflight_count: AtomicUsize::new(0),
            envelopes: Mutex::new(Vec::new()),
        }
    }

    fn dispatch_count(&self) -> usize {
        self.envelopes.lock().unwrap().len()
    }

    fn latest_envelope(&self) -> AgentWakeEnvelope {
        self.envelopes.lock().unwrap().last().unwrap().clone()
    }
}

impl ContinuationAdapter for FakeHostAdapter {
    fn adapter_kind(&self) -> &'static str {
        "deterministic_fake"
    }

    fn preflight(
        &self,
        _continuation: &ContinuationPreflight,
    ) -> Result<(), ContinuationPreflightError> {
        self.preflight_count.fetch_add(1, Ordering::SeqCst);
        match self.preflight_error {
            Some(kind) => Err(ContinuationPreflightError::new(kind)),
            None => Ok(()),
        }
    }

    fn dispatch(&self, envelope: &AgentWakeEnvelope) -> ContinuationDispatchOutcome {
        self.envelopes.lock().unwrap().push(envelope.clone());
        self.outcome
    }
}

fn wait_until(label: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if ready() {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for {label}");
        std::thread::park_timeout(Duration::from_millis(5));
    }
}

fn runtime_with_db(db: Arc<Database>) -> ToolRuntime {
    ToolRuntime::new_for_tests_with_shell_clients(Arc::new(ShellClientRegistry::default()))
        .with_communication_database(db)
}

fn create_agent(
    runtime: &ToolRuntime,
    handle: &str,
    display_name: &str,
    description: &str,
    label: &str,
    key: &str,
) -> String {
    let result = runtime.create_agent_identity(
        None,
        handle.to_string(),
        display_name.to_string(),
        Some(description.to_string()),
        vec![label.to_string()],
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn attach(runtime: &ToolRuntime, agent_id: &str, key: &str) -> (String, i64) {
    let result = runtime.attach_agent_endpoint(
        None,
        agent_id.to_string(),
        "Deterministic Host".to_string(),
        Some(format!("attachment-{key}")),
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    assert_eq!(result.output["endpoint"]["wake_capable"], false);
    (
        result.output["endpoint"]["endpoint_id"]
            .as_str()
            .unwrap()
            .to_string(),
        result.output["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap(),
    )
}

fn create_conversation(runtime: &ToolRuntime, agent_a: &str, agent_b: &str, key: &str) -> String {
    let result = runtime.create_conversation(
        None,
        Some("Natural durable conversation".to_string()),
        vec![agent_a.to_string(), agent_b.to_string()],
        key.to_string(),
    );
    assert!(result.success, "{:?}", result.output);
    result.output["conversation"]["conversation"]["conversation_id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[allow(clippy::too_many_arguments)]
fn post_as_agent(
    runtime: &ToolRuntime,
    conversation_id: &str,
    body: &str,
    author_agent_id: &str,
    endpoint_id: &str,
    controller_generation: i64,
    recipient_agent_id: &str,
    idempotency_key: Option<&str>,
    wake_reply_id: Option<&str>,
    reply_operation_index: Option<i64>,
) -> Value {
    let result = runtime.post_conversation_message(
        None,
        conversation_id.to_string(),
        body.to_string(),
        Some(author_agent_id.to_string()),
        Some(endpoint_id.to_string()),
        Some(controller_generation),
        Some(vec![recipient_agent_id.to_string()]),
        None,
        idempotency_key.map(ToOwned::to_owned),
        wake_reply_id.map(ToOwned::to_owned),
        reply_operation_index,
    );
    assert!(result.success, "{:?}", result.output);
    result.output
}

fn count(db: &Database, table: &str) -> i64 {
    assert!(matches!(
        table,
        "wc_conversation_messages" | "wc_agent_deliveries" | "wc_agent_wakes"
    ));
    db.conn_for_tests()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn wake_id_for(db: &Database, agent_id: &str) -> String {
    db.conn_for_tests()
        .query_row(
            "SELECT wake_id FROM wc_agent_wakes
             WHERE target_agent_id = ?1 AND state != 'consumed'
             ORDER BY created_at_unix_ms, wake_id LIMIT 1",
            [agent_id],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn natural_agent_message_dispatches_once_and_burst_remains_bounded_and_private() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&temp.path().join("natural.db")).unwrap());
    let runtime = runtime_with_db(db.clone());
    let agent_a = create_agent(
        &runtime,
        "architect",
        "Architect",
        "private architect description",
        "private-architect-label",
        "natural-agent-a",
    );
    let agent_b = create_agent(
        &runtime,
        "reviewer",
        "Reviewer",
        "private reviewer description",
        "private-reviewer-label",
        "natural-agent-b",
    );
    let (endpoint_a, generation_a) = attach(&runtime, &agent_a, "natural-endpoint-a");
    let (endpoint_b, generation_b) = attach(&runtime, &agent_b, "natural-endpoint-b");
    let conversation_id = create_conversation(&runtime, &agent_a, &agent_b, "natural-conversation");
    let adapter = Arc::new(FakeHostAdapter::delivered());
    let registration = runtime.register_agent_continuation_adapter(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        adapter.clone(),
    );
    assert!(registration.success, "{:?}", registration.output);
    assert_eq!(registration.output["endpoint"]["wake_capable"], true);

    let private_body = "private body must stay out of the continuation envelope";
    post_as_agent(
        &runtime,
        &conversation_id,
        private_body,
        &agent_a,
        &endpoint_a,
        generation_a,
        &agent_b,
        Some("natural-message-0"),
        None,
        None,
    );
    wait_until("first continuation dispatch", || {
        adapter.dispatch_count() == 1
    });
    let first_wake_id = wake_id_for(&db, &agent_b);
    let envelope = adapter.latest_envelope();
    assert_eq!(envelope.wake_id, first_wake_id);
    assert_eq!(envelope.agent_id, agent_b);
    assert_eq!(envelope.endpoint_id, endpoint_b);
    assert_eq!(envelope.controller_generation, generation_b);
    let envelope_text = serde_json::to_string(&envelope).unwrap();
    for private in [
        private_body,
        "private reviewer description",
        "private-reviewer-label",
        "natural-message-0",
        "wc_commprincipal_",
    ] {
        assert!(
            !envelope_text.contains(private),
            "envelope leaked {private}"
        );
    }

    for index in 1..50 {
        post_as_agent(
            &runtime,
            &conversation_id,
            &format!("bounded burst message {index}"),
            &agent_a,
            &endpoint_a,
            generation_a,
            &agent_b,
            Some(&format!("natural-message-{index}")),
            None,
            None,
        );
    }
    assert_eq!(count(&db, "wc_conversation_messages"), 50);
    assert_eq!(count(&db, "wc_agent_deliveries"), 50);
    assert!(
        count(&db, "wc_agent_wakes") <= 2,
        "one delivered Wake plus at most one coalesced successor is bounded"
    );
    assert_eq!(
        adapter.dispatch_count(),
        1,
        "an unresolved delivered Wake blocks 49 duplicate model-turn dispatches"
    );

    let bootstrap = runtime.bootstrap_agent_conversation(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        Some(conversation_id),
        Some(first_wake_id.clone()),
        None,
    );
    assert!(bootstrap.success, "{:?}", bootstrap.output);
    assert_eq!(bootstrap.output["host_binding"]["adapter_registered"], true);
    assert_eq!(
        bootstrap.output["host_binding"]["production_auto_resume_available"],
        false
    );
    assert_eq!(
        bootstrap.output["host_binding"]["runtime_wake_capable"],
        true
    );
    assert!(bootstrap.output["selected_conversation"]["conversation_id"].is_string());
    assert!(
        bootstrap.output["inbox"]["queued_delivery_count"]
            .as_i64()
            .unwrap()
            >= 50
    );
    assert!(bootstrap.output.get("messages").is_none());
    assert!(!bootstrap.output.to_string().contains(private_body));

    let unregistered = runtime.unregister_agent_continuation_adapter(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
    );
    assert!(unregistered.success, "{:?}", unregistered.output);
    assert_eq!(unregistered.output["endpoint"]["wake_capable"], false);
    let bootstrap = runtime.bootstrap_agent_conversation(
        None,
        agent_b,
        endpoint_b,
        generation_b,
        None,
        Some(first_wake_id),
        None,
    );
    assert!(bootstrap.success, "{:?}", bootstrap.output);
    assert_eq!(
        bootstrap.output["host_binding"]["adapter_registered"],
        false
    );
    assert_eq!(
        bootstrap.output["host_binding"]["runtime_wake_capable"],
        false
    );
}

#[test]
fn offline_restart_and_replacement_dispatch_the_same_logical_wake() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("restart.db");
    let db = Arc::new(Database::open(&path).unwrap());
    let runtime = runtime_with_db(db.clone());
    let agent_a = create_agent(
        &runtime,
        "sender",
        "Sender",
        "sender description",
        "sender-label",
        "restart-agent-a",
    );
    let agent_b = create_agent(
        &runtime,
        "offline",
        "Offline Agent",
        "offline description",
        "offline-label",
        "restart-agent-b",
    );
    let (endpoint_a, generation_a) = attach(&runtime, &agent_a, "restart-endpoint-a");
    let (endpoint_b, generation_b) = attach(&runtime, &agent_b, "restart-endpoint-b");
    let conversation_id = create_conversation(&runtime, &agent_a, &agent_b, "restart-conversation");
    post_as_agent(
        &runtime,
        &conversation_id,
        "queued while no usable Host adapter exists",
        &agent_a,
        &endpoint_a,
        generation_a,
        &agent_b,
        Some("restart-message"),
        None,
        None,
    );
    let logical_wake_id = wake_id_for(&db, &agent_b);
    assert_eq!(
        db.agent_wake(&logical_wake_id).unwrap().unwrap().state,
        AgentWakeState::Pending
    );

    let unavailable = Arc::new(FakeHostAdapter::unavailable());
    let registration = runtime.register_agent_continuation_adapter(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        unavailable.clone(),
    );
    assert!(registration.success);
    wait_until("failed preflight", || {
        unavailable.preflight_count.load(Ordering::SeqCst) == 1
    });
    assert_eq!(unavailable.dispatch_count(), 0);
    assert_eq!(
        db.agent_wake(&logical_wake_id).unwrap().unwrap().state,
        AgentWakeState::Pending
    );

    drop(runtime);
    drop(db);
    let reopened = Arc::new(Database::open(&path).unwrap());
    let ownership = crate::server_instance::ServerInstanceGuard::acquire(&reopened).unwrap();
    reopened
        .recover_agent_wakes_for_server_takeover(&ownership, chrono::Utc::now().timestamp_millis())
        .unwrap();
    let runtime = runtime_with_db(reopened.clone());
    let bootstrap = runtime.bootstrap_agent_conversation(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        Some(conversation_id.clone()),
        Some(logical_wake_id.clone()),
        None,
    );
    assert!(bootstrap.success, "{:?}", bootstrap.output);
    assert_eq!(bootstrap.output["endpoint"]["wake_capable"], false);
    assert_eq!(
        bootstrap.output["host_binding"]["adapter_registered"],
        false
    );
    assert_eq!(
        bootstrap.output["host_binding"]["production_auto_resume_available"],
        false
    );
    assert_eq!(bootstrap.output["wake"]["wake_id"], logical_wake_id);

    let old_process_registration = runtime.register_agent_continuation_adapter(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        Arc::new(FakeHostAdapter::delivered()),
    );
    assert!(!old_process_registration.success);
    assert_eq!(
        old_process_registration.output["error_kind"], "endpoint_not_attached_in_process",
        "a successor process cannot assume a pre-restart Host callback survived"
    );

    let (replacement_endpoint, replacement_generation) =
        attach(&runtime, &agent_b, "restart-endpoint-b2");
    assert_eq!(replacement_generation, generation_b + 1);
    let replacement_adapter = Arc::new(FakeHostAdapter::delivered());
    let registration = runtime.register_agent_continuation_adapter(
        None,
        agent_b.clone(),
        replacement_endpoint.clone(),
        replacement_generation,
        replacement_adapter.clone(),
    );
    assert!(registration.success);
    wait_until("replacement continuation dispatch", || {
        replacement_adapter.dispatch_count() == 1
    });
    assert_eq!(
        replacement_adapter.latest_envelope().wake_id,
        logical_wake_id
    );
    let (replayed_old_endpoint, replayed_old_generation) =
        attach(&runtime, &agent_b, "restart-endpoint-b");
    assert_eq!(replayed_old_endpoint, endpoint_b);
    assert_eq!(replayed_old_generation, generation_b);
    let stale_registration = runtime.register_agent_continuation_adapter(
        None,
        agent_b.clone(),
        endpoint_b,
        generation_b,
        Arc::new(FakeHostAdapter::delivered()),
    );
    assert!(!stale_registration.success);
    assert_eq!(
        stale_registration.output["error_kind"], "endpoint_expired",
        "the stale generation cannot register itself again"
    );
    let bootstrap = runtime.bootstrap_agent_conversation(
        None,
        agent_b,
        replacement_endpoint,
        replacement_generation,
        Some(conversation_id),
        Some(logical_wake_id),
        None,
    );
    assert!(bootstrap.success, "{:?}", bootstrap.output);
    assert_eq!(
        bootstrap.output["host_binding"]["adapter_registered"], true,
        "a rejected stale registration must not dislodge the current binding"
    );
}

#[test]
fn wake_derived_reply_identity_closes_response_loss_without_merging_consumption() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&temp.path().join("reply-replay.db")).unwrap());
    let runtime = runtime_with_db(db.clone());
    let agent_a = create_agent(
        &runtime,
        "requester",
        "Requester",
        "requester description",
        "requester-label",
        "reply-agent-a",
    );
    let agent_b = create_agent(
        &runtime,
        "responder",
        "Responder",
        "responder description",
        "responder-label",
        "reply-agent-b",
    );
    let (endpoint_a, generation_a) = attach(&runtime, &agent_a, "reply-endpoint-a");
    let (endpoint_b, generation_b) = attach(&runtime, &agent_b, "reply-endpoint-b");
    let conversation_id = create_conversation(&runtime, &agent_a, &agent_b, "reply-conversation");
    post_as_agent(
        &runtime,
        &conversation_id,
        "please review",
        &agent_a,
        &endpoint_a,
        generation_a,
        &agent_b,
        Some("reply-request"),
        None,
        None,
    );
    let wake_id = wake_id_for(&db, &agent_b);

    let first = post_as_agent(
        &runtime,
        &conversation_id,
        "review completed",
        &agent_b,
        &endpoint_b,
        generation_b,
        &agent_a,
        None,
        Some(&wake_id),
        Some(0),
    );
    assert_eq!(first["replayed"], false);
    let (replacement_endpoint_b, replacement_generation_b) =
        attach(&runtime, &agent_b, "reply-endpoint-b2");
    let retry = post_as_agent(
        &runtime,
        &conversation_id,
        "review completed",
        &agent_b,
        &replacement_endpoint_b,
        replacement_generation_b,
        &agent_a,
        None,
        Some(&wake_id),
        Some(0),
    );
    assert_eq!(retry["replayed"], true);
    assert_eq!(
        retry["message"]["message_id"],
        first["message"]["message_id"]
    );
    assert_eq!(count(&db, "wc_conversation_messages"), 2);

    let changed = runtime.post_conversation_message(
        None,
        conversation_id.clone(),
        "changed replay must conflict".to_string(),
        Some(agent_b.clone()),
        Some(replacement_endpoint_b.clone()),
        Some(replacement_generation_b),
        Some(vec![agent_a.clone()]),
        None,
        None,
        Some(wake_id.clone()),
        Some(0),
    );
    assert!(!changed.success);
    assert_eq!(
        changed.output["error_kind"],
        "communication_idempotency_conflict"
    );
    post_as_agent(
        &runtime,
        &conversation_id,
        "a second intentional message",
        &agent_b,
        &replacement_endpoint_b,
        replacement_generation_b,
        &agent_a,
        None,
        Some(&wake_id),
        Some(1),
    );
    assert_eq!(count(&db, "wc_conversation_messages"), 3);

    let delivery_id: String = db
        .conn_for_tests()
        .query_row(
            "SELECT delivery_id FROM wc_agent_deliveries
             WHERE recipient_agent_id = ?1 AND state = 'queued'
             ORDER BY delivery_order LIMIT 1",
            [&agent_b],
            |row| row.get(0),
        )
        .unwrap();
    let consumed = runtime.consume_agent_deliveries(
        None,
        agent_b.clone(),
        replacement_endpoint_b,
        replacement_generation_b,
        vec![delivery_id],
    );
    assert!(consumed.success);
    assert_eq!(
        db.agent_wake(&wake_id).unwrap().unwrap().state,
        AgentWakeState::Pending,
        "Delivery consume remains independent from the logical Wake"
    );
}

#[test]
fn explicit_activation_bootstrap_is_replayable_and_consumes_wake_separately() {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(Database::open(&temp.path().join("explicit-activation.db")).unwrap());
    let runtime = runtime_with_db(db.clone());
    let agent_a = create_agent(
        &runtime,
        "manual-sender",
        "Manual Sender",
        "sender",
        "sender",
        "manual-agent-a",
    );
    let agent_b = create_agent(
        &runtime,
        "manual-receiver",
        "Manual Receiver",
        "receiver",
        "receiver",
        "manual-agent-b",
    );
    let (endpoint_a, generation_a) = attach(&runtime, &agent_a, "manual-endpoint-a");
    let (endpoint_b, generation_b) = attach(&runtime, &agent_b, "manual-endpoint-b");
    let conversation_id = create_conversation(&runtime, &agent_a, &agent_b, "manual-conversation");
    post_as_agent(
        &runtime,
        &conversation_id,
        "manual activation work",
        &agent_a,
        &endpoint_a,
        generation_a,
        &agent_b,
        Some("manual-message"),
        None,
        None,
    );
    let wake_id = wake_id_for(&db, &agent_b);
    let first = runtime.bootstrap_agent_conversation(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        Some(conversation_id.clone()),
        Some(wake_id.clone()),
        Some("manual-activation-key".to_string()),
    );
    assert!(first.success, "{:?}", first.output);
    assert_eq!(first.output["wake"]["state"], "delivered");
    assert_eq!(first.output["wake_activation"]["state_changed"], true);
    let consume_token = first.output["wake_activation"]["consume_token"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt_id = first.output["wake_activation"]["attempt_id"].clone();

    let replay = runtime.bootstrap_agent_conversation(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        Some(conversation_id),
        Some(wake_id.clone()),
        Some("manual-activation-key".to_string()),
    );
    assert!(replay.success, "{:?}", replay.output);
    assert_eq!(replay.output["wake_activation"]["replayed"], true);
    assert_eq!(replay.output["wake_activation"]["state_changed"], false);
    assert_eq!(replay.output["wake_activation"]["attempt_id"], attempt_id);
    assert_eq!(
        replay.output["wake_activation"]["consume_token"],
        consume_token
    );

    let consumed = runtime.consume_agent_wake(
        None,
        agent_b.clone(),
        endpoint_b.clone(),
        generation_b,
        wake_id.clone(),
        consume_token,
    );
    assert!(consumed.success, "{:?}", consumed.output);
    let inbox = runtime.list_agent_inbox(
        None,
        agent_b.clone(),
        endpoint_b,
        generation_b,
        Some(0),
        Some(10),
    );
    assert!(inbox.success, "{:?}", inbox.output);
    assert_eq!(
        inbox.output["total_queued_count"], 1,
        "Wake consume must not consume the Delivery"
    );
    assert_eq!(
        db.agent_wake(&wake_id).unwrap().unwrap().state,
        AgentWakeState::Consumed
    );
}
