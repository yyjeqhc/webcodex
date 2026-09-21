use super::*;
use crate::db::{GoalCheckpoint, NewAgentEndpoint, NewGoal, NewGoalStep};
use crate::tool_runtime::{AgentWaitEventSelectorCall, AgentWaitModeCall, ToolResult};

const T0: i64 = 100_000_000;
const THRESHOLD: i64 = crate::db::GOAL_ACTIVITY_ATTENTION_AFTER_MS;
const WINDOW: &str = "goal-workflow-live-window";

struct Workflow {
    temp: tempfile::TempDir,
    db: Arc<crate::db::Database>,
    runtime: ToolRuntime,
    auth: crate::auth::AuthContext,
    project: String,
    session_id: String,
    goal_id: String,
    agent_id: String,
}

impl Workflow {
    async fn new(controller: bool) -> Self {
        let (temp, db, runtime) = runtime_with_goal_activity_db();
        let auth = goal_activity_auth("goal-workflow-owner");
        let project = register_goal_activity_project(
            &runtime,
            "goal-workflow-runner",
            "goal-workflow-owner",
            "demo",
            temp.path(),
        )
        .await;
        let session_id =
            start_goal_activity_session(&runtime, &auth, &project, "Current Goal workflow")
                .session_id;
        let agent = runtime.create_agent_identity(
            Some(&auth),
            "workflow-agent".into(),
            "Workflow Agent".into(),
            None,
            Vec::new(),
            "workflow-agent".into(),
        );
        assert!(agent.success, "{:?}", agent.output);
        let agent_id = agent.output["agent"]["agent_id"]
            .as_str()
            .unwrap()
            .to_string();
        let created = runtime.create_goal_with_plan(
            Some(&auth),
            NewGoal {
                title: "Goal workflow".into(),
                objective: "PRIVATE_OBJECTIVE_NOT_IN_CARD_OR_WAKE".into(),
                controller_agent_id: controller.then(|| agent_id.clone()),
                completion_conditions: vec![
                    "Fresh focused verification and independent review".into()
                ],
                steps: ["inspect", "implement", "validate", "review", "closeout"]
                    .into_iter()
                    .map(|id| NewGoalStep {
                        id: id.into(),
                        title: id.into(),
                    })
                    .collect(),
                idempotency_key: "workflow-goal".into(),
            },
        );
        assert!(created.success, "{:?}", created.output);
        let goal_id = created.output["goal"]["summary"]["goal_id"]
            .as_str()
            .unwrap()
            .to_string();
        link_goal_activity_session(
            &runtime,
            &auth,
            &goal_id,
            &session_id,
            "workflow-session-link",
        )
        .await;
        let fixture = Self {
            temp,
            db,
            runtime,
            auth,
            project,
            session_id,
            goal_id,
            agent_id,
        };
        fixture.work(T0, Some(&fixture.session_id));
        fixture
    }

    fn principal(&self) -> crate::db::CommunicationPrincipal {
        crate::tool_runtime::communication::communication_principal(Some(&self.auth)).unwrap()
    }

    fn work(&self, completed_at: i64, session: Option<&str>) {
        record_goal_window_event(
            &self.db,
            &self.auth,
            WINDOW,
            &self.project,
            "read_files",
            true,
            session,
            completed_at - 1,
        );
    }

    fn recorder_gap_work(
        &self,
        completed_at: i64,
        business_session_id: Option<&str>,
        event_project: Option<&str>,
    ) {
        let at_ms = completed_at - 1;
        record_goal_window_event(
            &self.db,
            &self.auth,
            WINDOW,
            event_project.unwrap_or(&self.project),
            "cargo_test",
            true,
            None,
            at_ms,
        );
        let ids = business_session_id
            .map(|session_id| json!({"business_session_id": session_id}))
            .unwrap_or_else(|| json!({}));
        self.db
            .conn_for_tests()
            .execute(
                "UPDATE action_events
                 SET recorder_gap_session_id = ?1, ids_json = ?2, project = ?3
                 WHERE server_trace_id = ?4",
                rusqlite::params![
                    self.session_id,
                    ids.to_string(),
                    event_project.unwrap_or(&self.project),
                    format!("goal-activity-{WINDOW}-cargo_test-{at_ms}")
                ],
            )
            .unwrap();
    }

    fn poll(&self, completed_at: i64) {
        self.poll_goal(&self.goal_id, completed_at);
    }

    fn poll_goal(&self, goal_id: &str, completed_at: i64) {
        record_goal_window_event(
            &self.db,
            &self.auth,
            WINDOW,
            &self.project,
            "goal_plan_sync",
            false,
            None,
            completed_at - 1,
        );
        self.db
            .conn_for_tests()
            .execute(
                "UPDATE action_events SET ids_json = ?1 WHERE server_trace_id = ?2",
                rusqlite::params![
                    json!({"goal_id": goal_id}).to_string(),
                    format!("goal-activity-{WINDOW}-goal_plan_sync-{}", completed_at - 1)
                ],
            )
            .unwrap();
    }

    async fn recheck(&self, now: i64) -> ToolResult {
        self.runtime
            .goal_plan_recheck_attention_at(
                Some(&self.auth),
                Some(&crate::client_window::ClientWindow::for_test(WINDOW)),
                self.goal_id.clone(),
                now,
            )
            .await
    }

    fn counts(&self) -> (i64, i64) {
        self.db.conn_for_tests().query_row(
            "SELECT (SELECT COUNT(*) FROM wc_agent_attention_events WHERE goal_id = ?1 AND kind = 'goal_workflow_stalled'),
                    (SELECT COUNT(*) FROM wc_agent_wakes w JOIN wc_agent_attention_events e ON e.event_id = w.source_event_id WHERE e.goal_id = ?1 AND e.kind = 'goal_workflow_stalled')",
            [&self.goal_id], |row| Ok((row.get(0)?, row.get(1)?)),
        ).unwrap()
    }

    fn candidate(&self, last_work: i64) -> crate::db::GoalStallCandidate {
        let (kind, id) =
            crate::tool_runtime::runtime_observation_principal(Some(&self.auth)).unwrap();
        crate::db::GoalStallCandidate {
            goal_id: self.goal_id.clone(),
            expected_revision: self
                .db
                .read_goal(&self.principal(), &self.goal_id)
                .unwrap()
                .summary
                .revision,
            controller_agent_id: self.agent_id.clone(),
            workflow_session_id: self.session_id.clone(),
            project_id: self.project.clone(),
            observed_window_key: crate::client_window::ClientWindow::for_test(WINDOW)
                .key()
                .into(),
            observation_principal_kind: kind,
            observation_principal_id: id,
            last_meaningful_activity_at_ms: last_work,
        }
    }
}

fn assert_no_attention(result: &ToolResult) {
    assert!(result.success, "{:?}", result.output);
    assert_eq!(
        result.output,
        json!({"attention": null, "state_changed": false})
    );
}

#[tokio::test]
async fn goal_workflow_recent_activity_and_polling_do_not_create_attention() {
    let fixture = Workflow::new(true).await;
    for quiet in [1, 1000, THRESHOLD - 1] {
        fixture.poll(T0 + quiet);
        assert_no_attention(&fixture.recheck(T0 + quiet).await);
    }
    assert_eq!(fixture.counts(), (0, 0));
}

#[tokio::test]
async fn goal_continuity_carrier_readiness_tracks_exact_endpoint_generation() {
    let fixture = Workflow::new(true).await;
    let initial = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(initial.output["goal_plan"]["continuity"]["state"], "ready");
    assert_eq!(
        initial.output["goal_plan"]["continuity"]["production_auto_resume_available"],
        false
    );

    let first = fixture.runtime.attach_agent_endpoint(
        Some(&fixture.auth),
        fixture.agent_id.clone(),
        "ChatGPT".into(),
        Some("goal-continuity-view-1".into()),
        "goal-continuity-endpoint-1".into(),
    );
    assert!(first.success, "{:?}", first.output);
    let first_endpoint = first.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let first_generation = first.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    let first_binding = format!(
        "wc_host_binding_{}",
        webcodex_core::compact::encode([0x51; 16])
    );
    let bound = fixture.runtime.agent_continuation_bind_for_window(
        Some(&fixture.auth),
        Some(&crate::client_window::ClientWindow::for_test(WINDOW)),
        fixture.agent_id.clone(),
        first_endpoint,
        first_generation,
        first_binding,
    );
    assert!(bound.success, "{:?}", bound.output);
    let ready = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(
        ready.output["goal_plan"]["continuity"]["production_auto_resume_available"],
        true
    );
    assert_eq!(ready.output["goal_plan"]["continuity"]["state"], "ready");

    let second = fixture.runtime.attach_agent_endpoint(
        Some(&fixture.auth),
        fixture.agent_id.clone(),
        "ChatGPT".into(),
        Some("goal-continuity-view-2".into()),
        "goal-continuity-endpoint-2".into(),
    );
    assert!(second.success, "{:?}", second.output);
    let second_endpoint = second.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_generation = second.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    assert!(second_generation > first_generation);
    let replaced = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(
        replaced.output["goal_plan"]["continuity"]["production_auto_resume_available"],
        false
    );
    assert_eq!(replaced.output["goal_plan"]["continuity"]["state"], "ready");

    let second_binding = format!(
        "wc_host_binding_{}",
        webcodex_core::compact::encode([0x52; 16])
    );
    let rebound = fixture.runtime.agent_continuation_bind_for_window(
        Some(&fixture.auth),
        Some(&crate::client_window::ClientWindow::for_test(WINDOW)),
        fixture.agent_id.clone(),
        second_endpoint,
        second_generation,
        second_binding,
    );
    assert!(rebound.success, "{:?}", rebound.output);
    let ready_again = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(
        ready_again.output["goal_plan"]["continuity"]["production_auto_resume_available"],
        true
    );
}

#[tokio::test]
async fn goal_workflow_live_stall_has_one_epoch_despite_poll_flood_revision_changes_and_reopen() {
    let mut fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    // More than the activity page bound of transport polls must not evict T0.
    for index in 1..=250 {
        fixture.poll(T0 + index * 1000);
    }
    fixture.poll(now);
    let state = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&state)["state"], "attention_needed");
    assert_eq!(goal_activity(&state)["coverage_partial"], false);
    assert_eq!(goal_activity(&state)["last_meaningful_activity_at_ms"], T0);
    let first = fixture.recheck(now).await;
    assert!(first.success, "{:?}", first.output);
    assert_eq!(first.output["state_changed"], true);
    let event_id = first.output["attention"]["event_id"]
        .as_str()
        .unwrap()
        .to_string();
    let wake_id = first.output["attention"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(fixture.counts(), (1, 1));
    let fact: (String, String, i64, i64, i64, bool) = fixture.db.conn_for_tests().query_row(
        "SELECT workflow_session_id, target_agent_id, goal_revision, last_meaningful_activity_at_ms, last_seen_at_ms,
                task_id IS NULL AND task_attempt_id IS NULL AND terminal_task_state IS NULL
         FROM wc_agent_attention_events WHERE event_id = ?1",
        [&event_id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    ).unwrap();
    assert_eq!(
        fact,
        (
            fixture.session_id.clone(),
            fixture.agent_id.clone(),
            2,
            T0,
            now,
            true
        )
    );
    for index in 1..=100 {
        fixture.poll(now + index);
        let repeated = fixture.recheck(now + index).await;
        assert!(repeated.success, "{:?}", repeated.output);
        assert_eq!(repeated.output["state_changed"], false);
        assert_eq!(repeated.output["attention"]["event_id"], event_id);
        assert_eq!(repeated.output["attention"]["wake_id"], wake_id);
    }
    let updated = fixture.runtime.update_goal(
        Some(&fixture.auth),
        fixture.goal_id.clone(),
        2,
        Some("Metadata revision alone is not new work evidence".into()),
        None,
        None,
        None,
        "metadata-only".into(),
    );
    assert!(updated.success, "{:?}", updated.output);
    assert_eq!(
        fixture.recheck(now + 100).await.output["attention"]["wake_id"],
        wake_id
    );
    let reopened =
        Arc::new(crate::db::Database::open(&fixture.temp.path().join("goal-activity.db")).unwrap());
    fixture.runtime = fixture
        .runtime
        .clone()
        .with_communication_database(reopened.clone())
        .with_window_activity_database(reopened.clone());
    fixture.db = reopened;
    assert_eq!(
        fixture.recheck(now + 100).await.output["attention"]["event_id"],
        event_id
    );
    assert_eq!(fixture.counts(), (1, 1));
    assert_eq!(
        fixture
            .db
            .read_goal(&fixture.principal(), &fixture.goal_id)
            .unwrap()
            .summary
            .revision,
        3
    );
}

#[tokio::test]
async fn goal_plan_sync_single_rpc_reconciles_and_dedups_one_stall_epoch() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let window = crate::client_window::ClientWindow::for_test(WINDOW);

    let first = fixture
        .runtime
        .goal_plan_sync_for_window_at(
            Some(&fixture.auth),
            Some(&window),
            fixture.goal_id.clone(),
            now,
        )
        .await;
    assert!(first.success, "{:?}", first.output);
    assert_eq!(
        first.output["goal_plan"]["continuity"]["state"],
        "wake_queued"
    );
    assert_eq!(
        first.output["goal_plan"]["continuity"]["wake_state"],
        "pending"
    );
    assert_eq!(fixture.counts(), (1, 1));

    let repeated = fixture
        .runtime
        .goal_plan_sync_for_window_at(
            Some(&fixture.auth),
            Some(&window),
            fixture.goal_id.clone(),
            now,
        )
        .await;
    assert!(repeated.success, "{:?}", repeated.output);
    assert_eq!(
        repeated.output["goal_plan"]["continuity"]["state"],
        "wake_queued"
    );
    assert_eq!(fixture.counts(), (1, 1));
}

#[tokio::test]
async fn goal_workflow_new_meaningful_work_is_required_for_a_new_inactivity_epoch() {
    let fixture = Workflow::new(true).await;
    let first_at = T0 + THRESHOLD;
    fixture.poll(first_at);
    let first = fixture.recheck(first_at).await;
    assert_eq!(first.output["state_changed"], true);
    let next_work = first_at + 1000;
    fixture.work(next_work, Some(&fixture.session_id));
    fixture.poll(next_work + 1);
    assert_no_attention(&fixture.recheck(next_work + 1).await);
    let second_at = next_work + THRESHOLD;
    fixture.poll(second_at);
    let second = fixture.recheck(second_at).await;
    assert_eq!(second.output["state_changed"], true);
    assert_ne!(
        first.output["attention"]["event_id"],
        second.output["attention"]["event_id"]
    );
    assert_ne!(
        first.output["attention"]["wake_id"],
        second.output["attention"]["wake_id"]
    );
    assert_eq!(fixture.counts(), (2, 2));
}

#[tokio::test]
async fn goal_continuity_does_not_retarget_an_existing_stall_wake_after_controller_replacement() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let first = fixture.recheck(now).await;
    assert_eq!(first.output["state_changed"], true);
    let old_wake = first.output["attention"]["wake_id"].as_str().unwrap();
    let before = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(
        before.output["goal_plan"]["continuity"]["wake_state"],
        "pending"
    );

    let replacement = fixture.runtime.create_agent_identity(
        Some(&fixture.auth),
        "replacement-controller".into(),
        "Replacement Controller".into(),
        None,
        Vec::new(),
        "replacement-controller-agent".into(),
    );
    assert!(replacement.success, "{:?}", replacement.output);
    let replacement_id = replacement.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let updated = fixture.runtime.update_goal_with_controller(
        Some(&fixture.auth),
        fixture.goal_id.clone(),
        2,
        None,
        None,
        Some(replacement_id),
        None,
        None,
        "replace-goal-controller".into(),
    );
    assert!(updated.success, "{:?}", updated.output);
    let after = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(after.output["goal_plan"]["continuity"]["state"], "stalled");
    assert!(after.output["goal_plan"]["continuity"]["wake_state"].is_null());
    assert_eq!(
        fixture
            .db
            .agent_wake(old_wake)
            .unwrap()
            .unwrap()
            .target_agent_id,
        fixture.agent_id
    );
}

#[tokio::test]
async fn goal_continuity_projects_retired_wake_and_fails_closed_on_malformed_event_wake_relation() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let attention = fixture.recheck(now).await;
    assert_eq!(attention.output["state_changed"], true);
    let wake_id = attention.output["attention"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    fixture
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_agent_wakes SET state = 'retired', revision = revision + 1 WHERE wake_id = ?1",
            [&wake_id],
        )
        .unwrap();
    let retired = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(
        retired.output["goal_plan"]["continuity"]["state"],
        "stalled"
    );
    assert_eq!(
        retired.output["goal_plan"]["continuity"]["wake_state"],
        "retired"
    );

    fixture
        .db
        .conn_for_tests()
        .execute("DELETE FROM wc_agent_wakes WHERE wake_id = ?1", [&wake_id])
        .unwrap();
    let malformed = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(
        malformed.output["goal_plan"]["continuity"]["available"],
        false
    );
    assert_eq!(
        malformed.output["goal_plan"]["continuity"]["state"],
        "unavailable"
    );
    assert!(malformed.output["goal_plan"]["continuity"]["wake_state"].is_null());
}

#[tokio::test]
async fn goal_continuity_ignores_unrelated_task_wait_and_other_goal_wakes_on_same_controller() {
    let fixture = Workflow::new(true).await;

    let task = fixture.runtime.create_agent_task(
        Some(&fixture.auth),
        "Unrelated controller task".into(),
        "Task continuation must not become Goal continuity.".into(),
        Some(fixture.agent_id.clone()),
        None,
        None,
        None,
        "unrelated-controller-task".into(),
    );
    assert!(task.success, "{:?}", task.output);
    let task_id = task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let attempt = fixture.runtime.start_agent_task_attempt(
        Some(&fixture.auth),
        task_id.clone(),
        fixture.agent_id.clone(),
        "unrelated-controller-attempt".into(),
    );
    assert!(attempt.success, "{:?}", attempt.output);
    let execution = fixture.runtime.start_agent_task_endpoint_continuation(
        Some(&fixture.auth),
        task_id,
        attempt.output["attempt"]["attempt_id"]
            .as_str()
            .unwrap()
            .to_string(),
        fixture.agent_id.clone(),
        attempt.output["attempt_fence"]
            .as_str()
            .unwrap()
            .to_string(),
        attempt.output["attempt"]["attempt_controller_generation"]
            .as_i64()
            .unwrap(),
    );
    assert!(execution.success, "{:?}", execution.output);
    let task_wake = execution.output["execution"]["wake_id"]
        .as_str()
        .unwrap()
        .to_string();
    let after_task_wake = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(
        after_task_wake.output["goal_plan"]["continuity"]["state"],
        "ready"
    );
    assert!(after_task_wake.output["goal_plan"]["continuity"]["wake_state"].is_null());

    let endpoint = fixture.runtime.attach_agent_endpoint(
        Some(&fixture.auth),
        fixture.agent_id.clone(),
        "ChatGPT".into(),
        Some("unrelated-wait-view".into()),
        "unrelated-wait-endpoint".into(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let endpoint_id = endpoint.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let endpoint_generation = endpoint.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    let worker = fixture.runtime.create_agent_identity(
        Some(&fixture.auth),
        "unrelated-wait-worker".into(),
        "Unrelated Wait Worker".into(),
        None,
        Vec::new(),
        "unrelated-wait-worker-agent".into(),
    );
    assert!(worker.success, "{:?}", worker.output);
    let worker_id = worker.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let source = fixture.runtime.create_agent_task(
        Some(&fixture.auth),
        "Unrelated wait source".into(),
        "Terminal source for an unrelated AgentWait.".into(),
        Some(worker_id.clone()),
        None,
        None,
        None,
        "unrelated-wait-source".into(),
    );
    assert!(source.success, "{:?}", source.output);
    let source_task = source.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let source_attempt = fixture.runtime.start_agent_task_attempt(
        Some(&fixture.auth),
        source_task.clone(),
        worker_id.clone(),
        "unrelated-wait-source-attempt".into(),
    );
    assert!(source_attempt.success, "{:?}", source_attempt.output);
    let source_done = fixture.runtime.complete_agent_task_attempt(
        Some(&fixture.auth),
        source_task.clone(),
        source_attempt.output["attempt"]["attempt_id"]
            .as_str()
            .unwrap()
            .to_string(),
        worker_id,
        source_attempt.output["attempt_fence"]
            .as_str()
            .unwrap()
            .to_string(),
        source_attempt.output["attempt"]["attempt_controller_generation"]
            .as_i64()
            .unwrap(),
        "succeeded".into(),
        Some("done".into()),
        None,
        "unrelated-wait-source-complete".into(),
    );
    assert!(source_done.success, "{:?}", source_done.output);
    let waited = fixture.runtime.wait_for_agent_events(
        Some(&fixture.auth),
        fixture.agent_id.clone(),
        endpoint_id,
        endpoint_generation,
        AgentWaitModeCall::Any,
        None,
        vec![AgentWaitEventSelectorCall {
            kind: "agent_task_terminal".into(),
            task_id: source_task,
        }],
        "unrelated-controller-wait".into(),
    );
    assert!(waited.success, "{:?}", waited.output);
    assert_eq!(waited.output["agent_wait"]["state"], "triggered");
    let wait_id = waited.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();
    let wait_wake: String = fixture
        .db
        .conn_for_tests()
        .query_row(
            "SELECT wake_id FROM wc_agent_wakes WHERE source_wait_id = ?1",
            [&wait_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(task_wake, wait_wake);
    let after_wait_wake = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    assert_eq!(
        after_wait_wake.output["goal_plan"]["continuity"]["state"],
        "ready"
    );
    assert!(after_wait_wake.output["goal_plan"]["continuity"]["wake_state"].is_null());

    let other = fixture.runtime.create_goal_with_controller(
        Some(&fixture.auth),
        "Other Goal".into(),
        "Its stall Wake must remain scoped to this exact Goal.".into(),
        Some(fixture.agent_id.clone()),
        "other-goal".into(),
    );
    assert!(other.success, "{:?}", other.output);
    let other_goal = other.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    link_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &other_goal,
        &fixture.session_id,
        "other-goal-session-link",
    )
    .await;
    let stalled_at = T0 + THRESHOLD + 10;
    fixture.poll_goal(&other_goal, stalled_at);
    let other_attention = fixture
        .runtime
        .goal_plan_recheck_attention_at(
            Some(&fixture.auth),
            Some(&crate::client_window::ClientWindow::for_test(WINDOW)),
            other_goal,
            stalled_at,
        )
        .await;
    assert_eq!(other_attention.output["state_changed"], true);
    let original = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), stalled_at)
        .await;
    assert_eq!(
        original.output["goal_plan"]["continuity"]["state"],
        "stalled"
    );
    assert!(original.output["goal_plan"]["continuity"]["wake_state"].is_null());
}

#[tokio::test]
async fn goal_workflow_running_meaningful_call_and_missing_completion_evidence_fail_closed() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let window = crate::client_window::ClientWindow::for_test(WINDOW);
    let (kind, id) =
        crate::tool_runtime::runtime_observation_principal(Some(&fixture.auth)).unwrap();
    let guard = fixture.runtime.window_activity_registry().start_observed(
        &window,
        "long-meaningful",
        "tools/call",
        Some("run_process"),
        Some((&kind, &id)),
        T0,
    );
    guard.update(Some("run_process"), Some(&fixture.project));
    assert_no_attention(&fixture.recheck(now).await);
    drop(guard);
    assert_no_attention(&fixture.recheck(now).await);
    assert_eq!(fixture.counts(), (0, 0));
    // A newly observed, durably completed meaningful request clears that Window's
    // coverage gap; its new inactivity epoch is independent from the missing turn.
    let next_work = now + 10;
    let guard = fixture.runtime.window_activity_registry().start_observed(
        &window,
        "recorded-work",
        "tools/call",
        Some("read_files"),
        Some((&kind, &id)),
        next_work - 1,
    );
    fixture.work(next_work, Some(&fixture.session_id));
    guard.complete(
        crate::tool_request_trace::RequestCompletionTiming {
            request_observed_at_ms: next_work - 1,
            response_handed_at_ms: next_work,
            elapsed_ms: 1,
        },
        true,
        true,
    );
    fixture.poll(next_work + THRESHOLD);
    assert_eq!(
        fixture.recheck(next_work + THRESHOLD).await.output["state_changed"],
        true
    );
}

#[tokio::test]
async fn goal_workflow_unobserved_or_stale_card_never_wakes_even_when_attention_is_needed() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    let state = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(goal_activity(&state)["state"], "attention_needed");
    assert_no_attention(&fixture.recheck(now).await);
    fixture.poll(now - crate::db::GOAL_CARD_OBSERVATION_LEASE_MS - 1);
    assert_no_attention(&fixture.recheck(now).await);
    // Unrelated transport activity is not a live Goal Plan View.
    record_goal_window_event(
        &fixture.db,
        &fixture.auth,
        WINDOW,
        &fixture.project,
        "agent_continuation_state",
        false,
        None,
        now - 1,
    );
    assert_no_attention(&fixture.recheck(now).await);
    assert_no_attention(
        &fixture
            .runtime
            .goal_plan_recheck_attention_at(Some(&fixture.auth), None, fixture.goal_id.clone(), now)
            .await,
    );
    assert_eq!(fixture.counts(), (0, 0));
}

#[tokio::test]
async fn goal_workflow_partial_window_coverage_never_commits_attention() {
    let fixture = Workflow::new(true).await;
    for index in 0..16 {
        record_goal_window_event(
            &fixture.db,
            &fixture.auth,
            &format!("other-window-{index}"),
            &fixture.project,
            "read_files",
            true,
            Some(&fixture.session_id),
            T0 - index - 2,
        );
    }
    fixture.poll(T0 + THRESHOLD);
    assert_no_attention(&fixture.recheck(T0 + THRESHOLD).await);
    assert_eq!(fixture.counts(), (0, 0));
}

#[tokio::test]
async fn goal_workflow_exact_business_session_evidence_covers_only_its_own_recorder_gap() {
    let covered = Workflow::new(true).await;
    let covered_work = T0 + 1_000;
    covered.recorder_gap_work(covered_work, Some(&covered.session_id), None);
    let covered_now = covered_work + THRESHOLD;
    covered.poll(covered_now);
    let resumed = covered.recheck(covered_now).await;
    assert!(resumed.success, "{:?}", resumed.output);
    assert_eq!(resumed.output["state_changed"], true);
    assert_eq!(covered.counts(), (1, 1));
    let forensic_gap_count: i64 = covered
        .db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM action_events
             WHERE recorder_gap_session_id = ?1
               AND json_valid(ids_json)
               AND json_extract(ids_json, '$.business_session_id') = ?1",
            [&covered.session_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        forensic_gap_count, 1,
        "covered gap must remain durable forensic truth"
    );

    let missing = Workflow::new(true).await;
    let missing_work = T0 + 2_000;
    missing.recorder_gap_work(missing_work, None, None);
    let missing_now = missing_work + THRESHOLD;
    missing.poll(missing_now);
    assert_no_attention(&missing.recheck(missing_now).await);
    assert_eq!(missing.counts(), (0, 0));

    let different = Workflow::new(true).await;
    let other_session = start_goal_activity_session(
        &different.runtime,
        &different.auth,
        &different.project,
        "Different business Session",
    )
    .session_id;
    let different_work = T0 + 3_000;
    different.recorder_gap_work(different_work, Some(&other_session), None);
    let different_now = different_work + THRESHOLD;
    different.poll(different_now);
    assert_no_attention(&different.recheck(different_now).await);
    assert_eq!(different.counts(), (0, 0));

    let malformed = Workflow::new(true).await;
    let malformed_work = T0 + 3_500;
    malformed.recorder_gap_work(malformed_work, Some(&malformed.session_id), None);
    malformed
        .db
        .conn_for_tests()
        .execute(
            "UPDATE action_events SET ids_json = 'malformed-json'
             WHERE server_trace_id = ?1",
            [format!(
                "goal-activity-{WINDOW}-cargo_test-{}",
                malformed_work - 1
            )],
        )
        .unwrap();
    let malformed_now = malformed_work + THRESHOLD;
    malformed.poll(malformed_now);
    assert_no_attention(&malformed.recheck(malformed_now).await);
    assert_eq!(malformed.counts(), (0, 0));

    let wrong_project = Workflow::new(true).await;
    let wrong_project_work = T0 + 4_000;
    wrong_project.recorder_gap_work(
        wrong_project_work,
        Some(&wrong_project.session_id),
        Some("agent:other:project"),
    );
    let wrong_project_now = wrong_project_work + THRESHOLD;
    wrong_project.poll(wrong_project_now);
    assert_no_attention(&wrong_project.recheck(wrong_project_now).await);
    assert_eq!(wrong_project.counts(), (0, 0));
}

#[tokio::test]
async fn goal_workflow_absent_controller_and_terminal_goals_do_not_create_attention() {
    let no_controller = Workflow::new(false).await;
    no_controller.poll(T0 + THRESHOLD);
    assert_no_attention(&no_controller.recheck(T0 + THRESHOLD).await);
    assert_eq!(no_controller.counts(), (0, 0));
    for lifecycle in ["completed", "cancelled"] {
        let fixture = Workflow::new(true).await;
        let checkpoint = fixture.runtime.checkpoint_goal(
            Some(&fixture.auth),
            fixture.goal_id.clone(),
            2,
            GoalCheckpoint {
                completed_step_ids: ["inspect", "implement", "validate", "review", "closeout"]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                current_step_id: None,
                summary: "Fresh verification and independent review complete".into(),
            },
            "complete-plan".into(),
        );
        assert!(checkpoint.success, "{:?}", checkpoint.output);
        let completed = fixture.runtime.update_goal(
            Some(&fixture.auth),
            fixture.goal_id.clone(),
            3,
            None,
            None,
            Some(lifecycle.into()),
            None,
            "terminal".into(),
        );
        assert!(completed.success, "{:?}", completed.output);
        fixture.poll(T0 + THRESHOLD);
        assert_no_attention(&fixture.recheck(T0 + THRESHOLD).await);
        assert_eq!(fixture.counts(), (0, 0));
    }
}

#[tokio::test]
async fn goal_workflow_foreign_principal_scopes_project_and_window_fail_closed_without_mapping_leaks(
) {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let foreign = goal_activity_auth("foreign-principal");
    let window = crate::client_window::ClientWindow::for_test(WINDOW);
    let denied = fixture
        .runtime
        .goal_plan_recheck_attention_at(Some(&foreign), Some(&window), fixture.goal_id.clone(), now)
        .await;
    assert!(!denied.success);
    assert_eq!(denied.output["error_kind"], "goal_not_found");
    for identity in [
        &fixture.session_id,
        &fixture.agent_id,
        &fixture.project,
        &fixture.goal_id,
    ] {
        assert!(!denied.output.to_string().contains(identity));
    }
    for removed in [
        SCOPE_RUNTIME_READ,
        SCOPE_COMMUNICATION_READ,
        SCOPE_COMMUNICATION_MANAGE,
        SCOPE_SESSION_COLLABORATE,
        SCOPE_PROJECT_READ,
    ] {
        let mut limited = fixture.auth.clone();
        limited.scopes.retain(|scope| scope != removed);
        assert_no_attention(
            &fixture
                .runtime
                .goal_plan_recheck_attention_at(
                    Some(&limited),
                    Some(&window),
                    fixture.goal_id.clone(),
                    now,
                )
                .await,
        );
    }
    assert_no_attention(
        &fixture
            .runtime
            .goal_plan_recheck_attention_at(
                Some(&fixture.auth),
                Some(&crate::client_window::ClientWindow::for_test(
                    "never-linked-window",
                )),
                fixture.goal_id.clone(),
                now,
            )
            .await,
    );
    // Revoke actual current Project visibility while preserving the old owned
    // Goal/Session link. allowed_client_id is not an account-credential restriction.
    let same_project = register_goal_activity_project(
        &fixture.runtime,
        "goal-workflow-runner",
        "new-project-owner",
        "demo",
        fixture.temp.path(),
    )
    .await;
    assert_eq!(same_project, fixture.project);
    assert!(fixture
        .runtime
        .resolve_project_input_for_auth(&fixture.project, Some(&fixture.auth))
        .await
        .is_err());
    assert_no_attention(&fixture.recheck(now).await);
    assert_eq!(fixture.counts(), (0, 0));
}

#[tokio::test]
async fn goal_workflow_revoked_correlated_project_is_partial_even_with_older_work() {
    let fixture = Workflow::new(true).await;
    let endpoint = fixture.runtime.attach_agent_endpoint(
        Some(&fixture.auth),
        fixture.agent_id.clone(),
        "ChatGPT".into(),
        Some("goal-partial-readiness-view".into()),
        "goal-partial-readiness-endpoint".into(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let bound = fixture.runtime.agent_continuation_bind_for_window(
        Some(&fixture.auth),
        Some(&crate::client_window::ClientWindow::for_test(WINDOW)),
        fixture.agent_id.clone(),
        endpoint.output["endpoint"]["endpoint_id"]
            .as_str()
            .unwrap()
            .to_string(),
        endpoint.output["endpoint"]["controller_generation"]
            .as_i64()
            .unwrap(),
        format!(
            "wc_host_binding_{}",
            webcodex_core::compact::encode([0x53; 16])
        ),
    );
    assert!(bound.success, "{:?}", bound.output);
    let other_project = register_goal_activity_project(
        &fixture.runtime,
        "older-workflow-runner",
        "goal-workflow-owner",
        "older",
        fixture.temp.path(),
    )
    .await;
    let older = start_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &other_project,
        "Earlier explicitly correlated work",
    );
    link_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &fixture.goal_id,
        &older.session_id,
        "older-session-link",
    )
    .await;
    record_goal_window_event(
        &fixture.db,
        &fixture.auth,
        "older-goal-workflow-window",
        &other_project,
        "read_files",
        true,
        Some(&older.session_id),
        T0 - 1001,
    );
    let revoked_project = register_goal_activity_project(
        &fixture.runtime,
        "older-workflow-runner",
        "new-project-owner",
        "older",
        fixture.temp.path(),
    )
    .await;
    assert_eq!(revoked_project, other_project);
    assert!(fixture
        .runtime
        .resolve_project_input_for_auth(&other_project, Some(&fixture.auth))
        .await
        .is_err());
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    // The inaccessible Window's work is older than the current work anchor, so
    // a MAX(timestamp) recheck alone cannot reveal this coverage gap.
    assert_no_attention(&fixture.recheck(now).await);
    assert_eq!(fixture.counts(), (0, 0));
    let state = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
        .await;
    assert_eq!(
        state.output["goal_plan"]["activity"]["coverage_partial"],
        true
    );
    assert_eq!(
        state.output["goal_plan"]["continuity"]["state"],
        "unavailable"
    );
    assert_eq!(
        state.output["goal_plan"]["continuity"]["production_auto_resume_available"],
        true
    );
    assert!(!state.output.to_string().contains(&other_project));
    assert!(!state.output.to_string().contains(&older.session_id));
}

#[tokio::test]
async fn goal_workflow_requires_current_unambiguous_active_session_not_historical_window_link() {
    for mode in [
        "closed",
        "evicted",
        "changed_session",
        "ambiguous",
        "missing_correlation",
        "wrong_latest_project",
        "future_relation",
    ] {
        let mut fixture = Workflow::new(true).await;
        let now = T0 + THRESHOLD;
        fixture.poll(now);
        match mode {
            "closed" => {
                fixture
                    .runtime
                    .sessions
                    .close_session(&fixture.session_id)
                    .unwrap();
            }
            "evicted" => {
                fixture.runtime.sessions = Default::default();
            }
            "changed_session" | "ambiguous" => {
                let other = start_goal_activity_session(
                    &fixture.runtime,
                    &fixture.auth,
                    &fixture.project,
                    "Different current work",
                );
                record_goal_window_event(
                    &fixture.db,
                    &fixture.auth,
                    WINDOW,
                    &fixture.project,
                    "goal_plan_sync",
                    false,
                    Some(&other.session_id),
                    if mode == "ambiguous" { T0 - 1 } else { now - 2 },
                );
            }
            "wrong_latest_project" => {
                record_goal_window_event(
                    &fixture.db,
                    &fixture.auth,
                    WINDOW,
                    "agent:wrong:project",
                    "goal_plan_sync",
                    false,
                    Some(&fixture.session_id),
                    now - 2,
                );
            }
            "future_relation" => {
                record_goal_window_event(
                    &fixture.db,
                    &fixture.auth,
                    WINDOW,
                    &fixture.project,
                    "goal_plan_sync",
                    false,
                    Some(&fixture.session_id),
                    now + 100,
                );
            }
            "missing_correlation" => {
                fixture.db.conn_for_tests().execute("DELETE FROM wc_goal_correlations WHERE goal_id = ?1 AND kind = 'workflow_session'", [&fixture.goal_id]).unwrap();
            }
            _ => unreachable!(),
        }
        assert_no_attention(&fixture.recheck(now).await);
        assert_eq!(fixture.counts(), (0, 0), "{mode}");
    }
}

#[tokio::test]
async fn goal_workflow_revalidates_durable_revision_and_work_snapshot_and_rolls_back_failed_wake() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let owner = fixture.principal();
    let mut stale = fixture.candidate(T0);
    stale.expected_revision -= 1;
    assert!(fixture
        .db
        .record_goal_workflow_stalled(&owner, &stale, || now)
        .unwrap()
        .is_none());
    stale = fixture.candidate(T0 - 1);
    assert!(fixture
        .db
        .record_goal_workflow_stalled(&owner, &stale, || now)
        .unwrap()
        .is_none());
    fixture.db.conn_for_tests().execute_batch(
        "CREATE TRIGGER fail_goal_attention_wake BEFORE INSERT ON wc_agent_wakes WHEN NEW.trigger_kind = 'attention_event'
         BEGIN SELECT RAISE(ABORT, 'test injected Wake failure'); END;",
    ).unwrap();
    assert!(fixture
        .db
        .record_goal_workflow_stalled(&owner, &fixture.candidate(T0), || now)
        .is_err());
    assert_eq!(fixture.counts(), (0, 0));
    fixture
        .db
        .conn_for_tests()
        .execute_batch("DROP TRIGGER fail_goal_attention_wake;")
        .unwrap();
    assert_eq!(fixture.recheck(now).await.output["state_changed"], true);
    assert_eq!(fixture.counts(), (1, 1));
}

#[tokio::test]
async fn goal_workflow_same_durable_agent_is_worker_and_controller_and_stall_uses_existing_host_fences(
) {
    for delivery_unknown in [false, true] {
        let fixture = Workflow::new(true).await;
        let owner = fixture.principal();
        let task = fixture
            .db
            .create_agent_task(
                &owner,
                crate::db::NewAgentTask {
                    title: "Callable worker task".into(),
                    instruction: "PRIVATE_TASK_BODY_NOT_IN_WAKE".into(),
                    assignee_agent_id: Some(fixture.agent_id.clone()),
                    source_conversation_id: None,
                    source_message_id: None,
                    referenced_project_id: None,
                    idempotency_key: "same-agent-task".into(),
                },
            )
            .unwrap();
        assert_eq!(
            task.task.summary.assignee_agent_id.as_deref(),
            Some(fixture.agent_id.as_str())
        );
        assert_eq!(
            fixture
                .db
                .conn_for_tests()
                .query_row("SELECT COUNT(*) FROM wc_agent_identities", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        let now = T0 + THRESHOLD;
        fixture.poll(now);
        let attention = fixture.recheck(now).await;
        assert_eq!(attention.output["state_changed"], true);
        let wake_id = attention.output["attention"]["wake_id"].as_str().unwrap();
        let pending_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
            .await;
        assert_eq!(
            pending_plan.output["goal_plan"]["continuity"]["state"],
            "wake_queued"
        );
        assert_eq!(
            pending_plan.output["goal_plan"]["continuity"]["wake_state"],
            "pending"
        );
        assert_eq!(
            pending_plan.output["goal_plan"]["continuity"]["fresh_turn"],
            "not_confirmed"
        );
        let endpoint = fixture
            .db
            .attach_agent_endpoint(
                &owner,
                NewAgentEndpoint {
                    agent_id: fixture.agent_id.clone(),
                    host: "ChatGPT".into(),
                    client_attachment_id: Some("goal-host-fence-test".into()),
                    wake_capable: true,
                    idempotency_key: "goal-host-endpoint".into(),
                },
            )
            .unwrap()
            .endpoint;
        let bootstrap = fixture
            .db
            .bootstrap_agent_conversation(
                &owner,
                &fixture.agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                None,
                Some(wake_id),
            )
            .unwrap();
        let selected = bootstrap.wake.unwrap();
        assert_eq!(
            selected.attention_kind.as_deref(),
            Some("goal_workflow_stalled")
        );
        assert_eq!(
            selected.workflow_session_id.as_deref(),
            Some(fixture.session_id.as_str())
        );
        assert_eq!(selected.goal_id.as_deref(), Some(fixture.goal_id.as_str()));
        assert!(selected.task_id.is_none() && selected.task_attempt_id.is_none());
        let claim = fixture
            .db
            .claim_next_agent_wake(
                &owner,
                &fixture.agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                "mcp_app",
            )
            .unwrap()
            .unwrap();
        assert_eq!(claim.wake.wake_id, wake_id);
        let claimed_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
            .await;
        assert_eq!(
            claimed_plan.output["goal_plan"]["continuity"]["state"],
            "wake_queued"
        );
        assert_eq!(
            claimed_plan.output["goal_plan"]["continuity"]["wake_state"],
            "claimed"
        );
        let prepared = fixture
            .db
            .prepare_agent_wake_dispatch(
                &owner,
                &fixture.agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                wake_id,
                &claim.attempt.attempt_id,
                &claim.claim_fence,
                &claim.consume_token,
            )
            .unwrap();
        let message = &prepared.envelope.resume_hint;
        assert!(
            message.chars().count() <= 1000,
            "{} chars: {message}",
            message.chars().count()
        );
        for required in [
            "Goal workflow stall continuation",
            "bootstrap_agent_conversation",
            "consume_agent_wake",
            "get_goal",
            "session_handoff_summary",
            "checkpoint_goal",
            "never repeat an uncertain effect",
            &fixture.goal_id,
            &fixture.session_id,
        ] {
            assert!(message.contains(required), "{required}");
        }
        for forbidden in [
            "PRIVATE_OBJECTIVE",
            "PRIVATE_TASK_BODY",
            fixture.project.as_str(),
            fixture.temp.path().to_str().unwrap(),
        ] {
            assert!(!message.contains(forbidden), "{forbidden}");
        }
        assert_eq!(prepared.wake.state, crate::db::AgentWakeState::Prepared);
        let prepared_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
            .await;
        assert_eq!(
            prepared_plan.output["goal_plan"]["continuity"]["state"],
            "dispatching"
        );
        assert_eq!(
            prepared_plan.output["goal_plan"]["continuity"]["host_delivery"],
            "dispatching"
        );
        assert_eq!(
            prepared_plan.output["goal_plan"]["continuity"]["fresh_turn"],
            "not_confirmed"
        );
        let dispatched = if delivery_unknown {
            fixture
                .db
                .mark_agent_wake_delivery_unknown(
                    &owner,
                    &fixture.agent_id,
                    &endpoint.endpoint_id,
                    endpoint.controller_generation,
                    wake_id,
                    &claim.attempt.attempt_id,
                    &claim.claim_fence,
                )
                .unwrap()
        } else {
            fixture
                .db
                .complete_agent_wake_delivery(
                    &owner,
                    &fixture.agent_id,
                    &endpoint.endpoint_id,
                    endpoint.controller_generation,
                    wake_id,
                    &claim.attempt.attempt_id,
                    &claim.claim_fence,
                )
                .unwrap()
        };
        assert_eq!(
            dispatched.state,
            if delivery_unknown {
                crate::db::AgentWakeState::DeliveryUnknown
            } else {
                crate::db::AgentWakeState::Delivered
            }
        );
        assert!(dispatched.consumed_at_unix_ms.is_none());
        let dispatched_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
            .await;
        assert_eq!(
            dispatched_plan.output["goal_plan"]["continuity"]["state"],
            if delivery_unknown {
                "host_unknown"
            } else {
                "host_accepted"
            }
        );
        assert_eq!(
            dispatched_plan.output["goal_plan"]["continuity"]["host_delivery"],
            if delivery_unknown {
                "unknown"
            } else {
                "accepted"
            }
        );
        assert_eq!(
            dispatched_plan.output["goal_plan"]["continuity"]["fresh_turn"],
            "not_confirmed"
        );
        assert_eq!(
            fixture.recheck(now).await.output["attention"]["wake_id"],
            wake_id
        );
        assert!(fixture
            .db
            .claim_next_agent_wake(
                &owner,
                &fixture.agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                "mcp_app"
            )
            .unwrap()
            .is_none());
        let consumed = fixture
            .db
            .consume_agent_wake(
                &owner,
                &fixture.agent_id,
                &endpoint.endpoint_id,
                endpoint.controller_generation,
                wake_id,
                &claim.consume_token,
            )
            .unwrap();
        assert_eq!(consumed.state, crate::db::AgentWakeState::Consumed);
        assert!(consumed.consumed_at_unix_ms > 0);
        let consumed_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), now)
            .await;
        assert_eq!(
            consumed_plan.output["goal_plan"]["continuity"]["state"],
            "resume_confirmed"
        );
        assert_eq!(
            consumed_plan.output["goal_plan"]["continuity"]["fresh_turn"],
            "confirmed"
        );
        assert_eq!(
            consumed_plan.output["goal_plan"]["continuity"]["last_resume_at_unix_ms"],
            consumed.consumed_at_unix_ms
        );
        let consumed_continuity = &consumed_plan.output["goal_plan"]["continuity"];
        assert_eq!(
            consumed_continuity["attention_candidate_at_unix_ms"],
            T0 + THRESHOLD
        );
        assert_eq!(consumed_continuity["attention_created_at_unix_ms"], now);
        assert_eq!(consumed_continuity["wake_created_at_unix_ms"], now);
        assert_eq!(
            consumed_continuity["wake_consumed_at_unix_ms"],
            consumed.consumed_at_unix_ms
        );
        assert!(consumed_continuity["host_dispatch_prepared_at_unix_ms"].is_i64());
        if delivery_unknown {
            assert!(consumed_continuity["host_dispatch_accepted_at_unix_ms"].is_null());
            assert!(consumed_continuity["host_dispatch_unknown_at_unix_ms"].is_i64());
        } else {
            assert!(consumed_continuity["host_dispatch_accepted_at_unix_ms"].is_i64());
            assert!(consumed_continuity["host_dispatch_unknown_at_unix_ms"].is_null());
        }
        assert!(consumed_continuity["first_post_resume_meaningful_at_unix_ms"].is_null());
        assert!(consumed_continuity["last_post_resume_meaningful_at_unix_ms"].is_null());
        assert_eq!(
            fixture.recheck(now).await.output["attention"]["wake_id"],
            wake_id
        );
        assert_eq!(fixture.counts(), (1, 1));
        assert!(fixture
            .db
            .read_agent_task(&owner, &task.task.summary.task_id)
            .unwrap()
            .summary
            .latest_attempt
            .is_none());
        let next_work = consumed.consumed_at_unix_ms + 1_000;
        fixture.work(next_work, Some(&fixture.session_id));
        let after_work = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), next_work)
            .await;
        assert_eq!(
            after_work.output["goal_plan"]["continuity"]["state"],
            "ready"
        );
        assert!(after_work.output["goal_plan"]["continuity"]["wake_state"].is_null());
        assert_eq!(
            after_work.output["goal_plan"]["continuity"]["last_resume_at_unix_ms"],
            consumed.consumed_at_unix_ms
        );
        let after_continuity = &after_work.output["goal_plan"]["continuity"];
        assert_eq!(
            after_continuity["host_delivery"],
            if delivery_unknown {
                "unknown"
            } else {
                "accepted"
            }
        );
        assert_eq!(after_continuity["fresh_turn"], "confirmed");
        assert_eq!(
            after_continuity["attention_candidate_at_unix_ms"],
            T0 + THRESHOLD
        );
        assert_eq!(after_continuity["attention_created_at_unix_ms"], now);
        assert_eq!(after_continuity["wake_created_at_unix_ms"], now);
        assert_eq!(
            after_continuity["wake_consumed_at_unix_ms"],
            consumed.consumed_at_unix_ms
        );
        assert_eq!(
            after_continuity["first_post_resume_meaningful_at_unix_ms"],
            next_work
        );
        assert_eq!(
            after_continuity["last_post_resume_meaningful_at_unix_ms"],
            next_work
        );
        fixture
            .db
            .conn_for_tests()
            .execute(
                "UPDATE wc_agent_wakes SET claimed_endpoint_id = NULL WHERE wake_id = ?1",
                [wake_id],
            )
            .unwrap();
        let malformed_history = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), next_work)
            .await;
        assert_eq!(
            malformed_history.output["goal_plan"]["continuity"]["state"],
            "unavailable"
        );
        assert!(
            malformed_history.output["goal_plan"]["continuity"]["wake_consumed_at_unix_ms"]
                .is_null()
        );
        fixture
            .db
            .conn_for_tests()
            .execute(
                "UPDATE wc_agent_wakes SET claimed_endpoint_id = ?2 WHERE wake_id = ?1",
                rusqlite::params![wake_id, endpoint.endpoint_id],
            )
            .unwrap();

        let replacement = fixture.runtime.create_agent_identity(
            Some(&fixture.auth),
            "post-resume-replacement".into(),
            "Post Resume Replacement".into(),
            None,
            Vec::new(),
            format!("post-resume-replacement-{delivery_unknown}"),
        );
        assert!(replacement.success, "{:?}", replacement.output);
        let replacement_id = replacement.output["agent"]["agent_id"]
            .as_str()
            .unwrap()
            .to_string();
        let replaced = fixture.runtime.update_goal_with_controller(
            Some(&fixture.auth),
            fixture.goal_id.clone(),
            2,
            None,
            None,
            Some(replacement_id),
            None,
            None,
            format!("replace-after-resume-{delivery_unknown}"),
        );
        assert!(replaced.success, "{:?}", replaced.output);
        let replaced_plan = fixture
            .runtime
            .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), next_work)
            .await;
        let replaced_continuity = &replaced_plan.output["goal_plan"]["continuity"];
        assert!(replaced_continuity["wake_state"].is_null());
        for field in [
            "attention_created_at_unix_ms",
            "wake_created_at_unix_ms",
            "host_dispatch_prepared_at_unix_ms",
            "host_dispatch_accepted_at_unix_ms",
            "host_dispatch_unknown_at_unix_ms",
            "wake_consumed_at_unix_ms",
            "first_post_resume_meaningful_at_unix_ms",
            "last_post_resume_meaningful_at_unix_ms",
            "last_resume_at_unix_ms",
        ] {
            assert!(
                replaced_continuity[field].is_null(),
                "prior-controller timeline leaked through {field}"
            );
        }
    }
}

#[tokio::test]
async fn goal_workflow_checkpoint_card_and_closeout_are_sparse_canonical_and_owner_scoped() {
    let fixture = Workflow::new(true).await;
    let checkpoint = fixture.runtime.checkpoint_goal(
        Some(&fixture.auth),
        fixture.goal_id.clone(),
        2,
        GoalCheckpoint {
            completed_step_ids: vec!["inspect".into(), "implement".into()],
            current_step_id: Some("validate".into()),
            summary: "Implementation complete; run focused validation".into(),
        },
        "milestone".into(),
    );
    assert!(checkpoint.success, "{:?}", checkpoint.output);
    let state = fixture
        .runtime
        .goal_plan_sync_at(Some(&fixture.auth), fixture.goal_id.clone(), T0)
        .await;
    let plan = &state.output["goal_plan"];
    assert_eq!(plan["version"], 3);
    assert_eq!(plan["total_step_count"], 5);
    assert_eq!(plan["completed_step_count"], 2);
    assert_eq!(plan["current_step_id"], "validate");
    assert_eq!(
        plan["progress_summary"],
        "Implementation complete; run focused validation"
    );
    assert_eq!(plan["steps"].as_array().unwrap().len(), 5);
    assert!(plan.get("objective").is_none());
    assert!(!plan.to_string().contains("PRIVATE_OBJECTIVE"));
    let follow_up = fixture
        .runtime
        .goal_follow_up_for_session(Some(&fixture.auth), &fixture.session_id)
        .unwrap();
    assert_eq!(
        follow_up["goals"],
        json!([{
            "goal_id": fixture.goal_id, "revision": 3, "incomplete_step_count": 3,
            "current_step": {"id": "validate", "title": "validate"}, "next_action": "checkpoint_goal"
        }])
    );
    assert_eq!(follow_up["available"], true);
    assert_eq!(follow_up["truncated"], false);
    assert!(follow_up.to_string().len() < 512);
    assert!(fixture
        .runtime
        .goal_follow_up_for_session(
            Some(&goal_activity_auth("foreign-closeout")),
            &fixture.session_id
        )
        .is_none());
    let other = start_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &fixture.project,
        "Same Project is not Goal correlation",
    );
    assert!(fixture
        .runtime
        .goal_follow_up_for_session(Some(&fixture.auth), &other.session_id)
        .is_none());
    let goal = fixture
        .db
        .read_goal(&fixture.principal(), &fixture.goal_id)
        .unwrap();
    assert_eq!(goal.summary.lifecycle.as_str(), "active");
    assert_eq!(goal.summary.revision, 3);
}

#[tokio::test]
async fn goal_workflow_malformed_observation_and_goal_plan_do_not_create_attention() {
    for malformed in [
        "plan",
        "work_time",
        "recorder_gap",
        "future_card",
        "card_time",
        "missing_card_start",
    ] {
        let fixture = Workflow::new(true).await;
        let now = T0 + THRESHOLD;
        fixture.poll(now);
        match malformed {
            "plan" => {
                fixture
                    .db
                    .conn_for_tests()
                    .execute(
                        "UPDATE wc_goals SET plan_json = '{}' WHERE goal_id = ?1",
                        [&fixture.goal_id],
                    )
                    .unwrap();
            }
            "work_time" => {
                fixture.db.conn_for_tests().execute("UPDATE action_events SET window_started_at_ms = window_ended_at_ms + 1 WHERE window_meaningful = 1", []).unwrap();
            }
            "recorder_gap" => {
                fixture.db.conn_for_tests().execute("UPDATE action_events SET recorder_gap_session_id = ?1 WHERE operation = 'goal_plan_sync'", [&fixture.session_id]).unwrap();
            }
            "card_time" => {
                fixture.db.conn_for_tests().execute("UPDATE action_events SET window_started_at_ms = window_ended_at_ms + 1 WHERE operation = 'goal_plan_sync'", []).unwrap();
            }
            "missing_card_start" => {
                fixture.db.conn_for_tests().execute("UPDATE action_events SET window_started_at_ms = NULL WHERE operation = 'goal_plan_sync'", []).unwrap();
            }
            "future_card" => {
                fixture.poll(now + 1);
            }
            _ => unreachable!(),
        }
        let result = fixture.recheck(now).await;
        assert!(
            result
                .output
                .get("attention")
                .is_none_or(|attention| attention.is_null()),
            "{malformed}: {:?}",
            result.output
        );
        assert_eq!(fixture.counts(), (0, 0), "{malformed}");
    }
}

#[tokio::test]
async fn goal_workflow_poll_must_name_exact_goal_and_cannot_borrow_another_cards_liveness() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    for ids in [
        "{}",
        r#"{"goal_id":"wc_goal_OtOtOtOtOtOtOtOt"}"#,
        "malformed",
    ] {
        fixture
            .db
            .conn_for_tests()
            .execute(
                "UPDATE action_events SET ids_json = ?1 WHERE operation = 'goal_plan_sync'",
                [ids],
            )
            .unwrap();
        assert_no_attention(&fixture.recheck(now).await);
        assert_eq!(fixture.counts(), (0, 0));
    }
    fixture.poll(now + 1);
    assert_eq!(fixture.recheck(now + 1).await.output["state_changed"], true);
}

#[tokio::test]
async fn goal_workflow_competing_rechecks_commit_one_fact_and_wake_and_replay_corruption_fails_closed(
) {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    let barrier = Arc::new(std::sync::Barrier::new(8));
    let owner = fixture.principal();
    let candidate = fixture.candidate(T0);
    let handles: Vec<_> = (0..8)
        .map(|_| {
            let (db, owner, candidate, barrier) = (
                fixture.db.clone(),
                owner.clone(),
                candidate.clone(),
                barrier.clone(),
            );
            std::thread::spawn(move || {
                barrier.wait();
                db.record_goal_workflow_stalled(&owner, &candidate, || now)
                    .unwrap()
                    .unwrap()
            })
        })
        .collect();
    let results: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(results.iter().filter(|result| result.created).count(), 1);
    assert!(results.iter().all(
        |result| result.event_id == results[0].event_id && result.wake_id == results[0].wake_id
    ));
    assert_eq!(fixture.counts(), (1, 1));
    fixture
        .db
        .conn_for_tests()
        .execute(
            "UPDATE wc_agent_attention_events SET goal_revision = 1000 WHERE event_id = ?1",
            [&results[0].event_id],
        )
        .unwrap();
    let rejected = fixture.recheck(now).await;
    assert!(!rejected.success);
    assert!(rejected.output.get("attention").is_none());
    assert_eq!(fixture.counts(), (1, 1));
}

#[tokio::test]
async fn goal_workflow_old_work_imported_through_new_correlation_does_not_manufacture_an_epoch() {
    let fixture = Workflow::new(true).await;
    let now = T0 + THRESHOLD;
    fixture.poll(now);
    assert_eq!(fixture.recheck(now).await.output["state_changed"], true);
    let other = start_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &fixture.project,
        "Already-existing prior work",
    );
    record_goal_window_event(
        &fixture.db,
        &fixture.auth,
        "older-independent-window",
        &fixture.project,
        "read_files",
        true,
        Some(&other.session_id),
        T0 + 999,
    );
    link_goal_activity_session(
        &fixture.runtime,
        &fixture.auth,
        &fixture.goal_id,
        &other.session_id,
        "new-correlation-not-new-work",
    )
    .await;
    fixture.poll(now + 1000);
    assert_no_attention(&fixture.recheck(now + 1000).await);
    assert_eq!(fixture.counts(), (1, 1));
}

#[tokio::test]
async fn goal_workflow_session_authority_fence_rejects_closed_wrong_project_and_wrong_owner() {
    let fixture = Workflow::new(true).await;
    let (_, fingerprint) = fixture
        .runtime
        .sessions
        .session_target_authority(&fixture.session_id)
        .unwrap();
    assert_eq!(
        fixture
            .runtime
            .sessions
            .with_active_session_authority_fence(
                &fixture.session_id,
                &fixture.project,
                &fingerprint,
                || 7
            ),
        Some(7)
    );
    assert!(fixture
        .runtime
        .sessions
        .with_active_session_authority_fence(
            &fixture.session_id,
            "other-project",
            &fingerprint,
            || panic!("wrong Project committed")
        )
        .is_none());
    assert!(fixture
        .runtime
        .sessions
        .with_active_session_authority_fence(
            &fixture.session_id,
            &fixture.project,
            "wrong-fingerprint",
            || panic!("wrong owner committed")
        )
        .is_none());
    fixture
        .runtime
        .sessions
        .close_session(&fixture.session_id)
        .unwrap();
    assert!(fixture
        .runtime
        .sessions
        .with_active_session_authority_fence(
            &fixture.session_id,
            &fixture.project,
            &fingerprint,
            || panic!("closed Session committed")
        )
        .is_none());
}

#[tokio::test]
async fn goal_workflow_kernel_rejects_sync_on_non_app_transports() {
    use crate::tool_runtime::kernel::{
        HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport,
    };
    let fixture = Workflow::new(true).await;
    let window = crate::client_window::ClientWindow::for_test(WINDOW);
    fixture.poll(T0 + THRESHOLD);
    for transport in [ToolTransport::Api, ToolTransport::Mcp] {
        let result = fixture
            .runtime
            .call_tool_with_context(
                ToolCallRequest {
                    tool_name: "goal_plan_sync".into(),
                    arguments: json!({"goal_id": fixture.goal_id}),
                },
                ToolCallContext {
                    transport,
                    session_id: None,
                    auth: Some(&fixture.auth),
                    window: Some(&window),
                    record_oauth_scope_denials: false,
                    host_file_import_trust: HostFileImportTrust::Untrusted,
                },
            )
            .await;
        assert!(!result.success);
        assert!(result.result.is_none());
        assert!(format!("{:?}", result.error_status).contains("Goal Plan App capability"));
    }
    assert_eq!(fixture.counts(), (0, 0));
}

#[tokio::test]
async fn goal_workflow_maximum_encoded_plan_remains_within_card_projection_budget() {
    let fixture = Workflow::new(true).await;
    let created = fixture.runtime.create_goal_with_plan(
        Some(&fixture.auth),
        NewGoal {
            title: "\0".repeat(200),
            objective: "PRIVATE_OBJECTIVE_NOT_IN_CARD".into(),
            controller_agent_id: Some(fixture.agent_id.clone()),
            completion_conditions: Vec::new(),
            steps: (0..32)
                .map(|index| NewGoalStep {
                    id: format!("step{index}"),
                    title: "\0".repeat(120),
                })
                .collect(),
            idempotency_key: "maximum-card".into(),
        },
    );
    assert!(created.success, "{:?}", created.output);
    let id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap();
    let base_bytes = created.output["goal"]["plan"].to_string().len();
    let summary_bytes = ((32_000 - base_bytes) / 6).min(2048);
    let checkpointed = fixture.runtime.checkpoint_goal(
        Some(&fixture.auth),
        id.into(),
        1,
        GoalCheckpoint {
            completed_step_ids: Vec::new(),
            current_step_id: Some("step0".into()),
            summary: "\0".repeat(summary_bytes),
        },
        "maximum-summary".into(),
    );
    assert!(checkpointed.success, "{:?}", checkpointed.output);
    let projection = fixture
        .runtime
        .present_goal_plan(Some(&fixture.auth), id.into())
        .await;
    assert!(projection.success);
    assert!(projection.output["goal_plan"].to_string().len() <= 40_960);
    assert!(!projection.output.to_string().contains("PRIVATE_OBJECTIVE"));
}
