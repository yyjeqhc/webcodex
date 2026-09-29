use super::*;
use crate::runtime_console_http::goals;

#[tokio::test]
async fn goal_workbench_projects_durable_goal_truth_without_new_authority() {
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let auth = crate::auth::shared_key_context("goal-workbench-owner");
    let project = "agent:goal-runner:webcodex";
    register_project(
        &runtime,
        "goal-runner",
        "webcodex",
        "/private/goal-workbench",
        Some(&auth),
    )
    .await;
    let session = runtime.sessions.start_session(
        Some(project.to_string()),
        Some("Goal workbench Session".to_string()),
    );
    let agent = runtime.create_agent_identity(
        Some(&auth),
        "goal-controller".into(),
        "Goal Controller".into(),
        None,
        vec!["runtime-v2".into()],
        "goal-workbench-controller".into(),
    );
    assert!(agent.success, "{:?}", agent.output);
    let agent_id = agent.output["agent"]["agent_id"]
        .as_str()
        .unwrap()
        .to_string();
    let created = runtime.create_goal_with_plan(
        Some(&auth),
        NewGoal {
            title: "Runtime V2 Goal Workbench".into(),
            objective: "Expose durable Goal truth read-only".into(),
            controller_agent_id: Some(agent_id.clone()),
            completion_conditions: vec!["Dogfood passes".into()],
            steps: ["survey", "implement", "validate"]
                .into_iter()
                .map(|id| NewGoalStep {
                    id: id.into(),
                    title: id.into(),
                })
                .collect(),
            idempotency_key: "goal-workbench-goal".into(),
        },
    );
    assert!(created.success, "{:?}", created.output);
    let goal_id = created.output["goal"]["summary"]["goal_id"]
        .as_str()
        .unwrap()
        .to_string();
    let linked = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            goal_id.clone(),
            session.session_id.clone(),
            "goal-workbench-session".into(),
        )
        .await;
    assert!(linked.success, "{:?}", linked.output);
    let second_session = runtime.sessions.start_session(
        Some(project.to_string()),
        Some("Goal workbench validation Session".to_string()),
    );
    let second_link = runtime
        .associate_goal_workflow_session(
            Some(&auth),
            goal_id.clone(),
            second_session.session_id.clone(),
            "goal-workbench-session-2".into(),
        )
        .await;
    assert!(second_link.success, "{:?}", second_link.output);
    let task = runtime.create_agent_task(
        Some(&auth),
        "Validate workbench".into(),
        "Run focused UI validation".into(),
        Some(agent_id.clone()),
        None,
        None,
        Some(project.to_string()),
        "goal-workbench-task".into(),
    );
    assert!(task.success, "{:?}", task.output);
    let task_id = task.output["task"]["summary"]["task_id"]
        .as_str()
        .unwrap()
        .to_string();
    let task_link = runtime.associate_goal_agent_task(
        Some(&auth),
        goal_id.clone(),
        task_id.clone(),
        "goal-workbench-task-link".into(),
    );
    assert!(task_link.success, "{:?}", task_link.output);
    let endpoint = runtime.attach_agent_endpoint(
        Some(&auth),
        agent_id.clone(),
        "ChatGPT".into(),
        Some("goal-workbench-test".into()),
        "goal-workbench-endpoint".into(),
    );
    assert!(endpoint.success, "{:?}", endpoint.output);
    let endpoint_id = endpoint.output["endpoint"]["endpoint_id"]
        .as_str()
        .unwrap()
        .to_string();
    let endpoint_generation = endpoint.output["endpoint"]["controller_generation"]
        .as_i64()
        .unwrap();
    let wait = runtime.wait_for_agent_events(
        Some(&auth),
        agent_id.clone(),
        endpoint_id,
        endpoint_generation,
        AgentWaitModeCall::All,
        Some(goal_id.clone()),
        vec![AgentWaitEventSelectorCall {
            kind: "agent_task_terminal".into(),
            task_id: task_id.clone(),
        }],
        "goal-workbench-wait".into(),
    );
    assert!(wait.success, "{:?}", wait.output);
    let wait_id = wait.output["agent_wait"]["wait_id"]
        .as_str()
        .unwrap()
        .to_string();
    let window_key = "goal-workbench-window";
    record_window_event_with_activity(
        &db,
        &auth,
        window_key,
        Some(project),
        Some((&session.session_id, project)),
        10_000,
        "read_files",
        true,
    );
    record_window_event_with_activity(
        &db,
        &auth,
        window_key,
        Some(project),
        Some((&second_session.session_id, project)),
        20_000,
        "run_shell",
        true,
    );

    let listed = goals::goals_for_auth_test(&runtime, &auth, Some(project))
        .await
        .unwrap();
    let rows = listed["goals"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["goal_id"], goal_id);
    assert_eq!(rows[0]["project_ids"], json!([project]));
    assert_eq!(rows[0]["workflow_session_count"], 2);
    assert_eq!(rows[0]["agent_task_count"], 1);

    let detail = goals::goal_detail_for_auth_test(&runtime, &auth, &goal_id)
        .await
        .unwrap();
    assert_eq!(detail["goal"]["summary"]["goal_id"], goal_id);
    assert_eq!(detail["goal_plan"]["controller_agent_id"], agent_id);
    assert_eq!(detail["sessions"].as_array().unwrap().len(), 2);
    assert!(detail["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["session_id"] == session.session_id));
    assert!(detail["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["session_id"] == second_session.session_id));
    assert_eq!(detail["tasks"][0]["summary"]["task_id"], task_id);
    assert_eq!(detail["agents"][0]["agent_id"], agent_id);
    assert_eq!(detail["windows"][0]["client_window_key"], window_key);
    assert_eq!(detail["windows"][0]["last_seen_at_ms"], 20_001);
    let window_sessions = detail["windows"][0]["session_ids"].as_array().unwrap();
    assert_eq!(window_sessions.len(), 2);
    assert!(window_sessions
        .iter()
        .any(|value| value == &json!(session.session_id)));
    assert!(window_sessions
        .iter()
        .any(|value| value == &json!(second_session.session_id)));
    assert_eq!(detail["waits"][0]["wait_id"], wait_id);
    assert_eq!(detail["waits"][0]["goal_id"], goal_id);
    assert_eq!(detail["waits"][0]["mode"], "all");
    assert_eq!(detail["waits"][0]["source_count"], 1);
    assert_eq!(detail["waits"][0]["match_count"], 0);
    assert_eq!(detail["waits"][0]["sources"][0]["task_id"], task_id);
    assert_eq!(detail["waits_truncated"], false);
}
