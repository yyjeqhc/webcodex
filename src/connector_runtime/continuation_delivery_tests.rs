use super::execution::execution_projection;
use super::{ConnectorBinding, ConnectorExecutionReservation, NewConnectorTask};
use crate::db::ConnectorExecutionObservation;
use crate::Database;

#[test]
fn model_execution_projection_never_exposes_terminal_continuation_claim_fence() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("continuation-projection.db")).unwrap();
    db.ensure_connector_binding(ConnectorBinding {
        project_id: "wc_proj_projection",
        project_name: "projection",
        workspace_id: "wc_ws_projection",
        executor_ref: "agent:hosted:projection",
        subject_id: "user:projection",
        profile: "personal",
        now: 10,
    })
    .unwrap();
    let task = db
        .start_connector_task(NewConnectorTask {
            task_id: "wc_task_projection",
            run_id: "wc_run_projection",
            project_id: "wc_proj_projection",
            workspace_id: "wc_ws_projection",
            subject_id: "user:projection",
            goal: "prove claim fence stays internal",
            mode: "normal",
            target_executor_ref: "agent:hosted:projection",
            execution_executor_ref: "agent:hosted:projection",
            target_root: "/workspace/projection",
            execution_root: "/workspace/runs/projection",
            baseline_commit: Some("0123456789abcdef"),
            baseline_tree: Some("fedcba9876543210"),
            isolated: true,
            now: 11,
        })
        .unwrap();
    let execution = match db
        .reserve_connector_execution(
            &task,
            "command",
            "op-projection",
            "request-hash",
            &[],
            None,
            None,
            100,
            12,
        )
        .unwrap()
    {
        ConnectorExecutionReservation::Created(execution) => execution,
        ConnectorExecutionReservation::Existing(_) => unreachable!(),
    };
    db.arm_connector_terminal_continuation(&execution.execution_id, 20)
        .unwrap();
    db.observe_connector_execution(
        &execution.execution_id,
        ConnectorExecutionObservation {
            executor_status: "completed",
            stdout_cursor: 1,
            stderr_cursor: 1,
            exit_code: Some(0),
            started_at: Some(20),
            finished_at: Some(21),
            check_completed: None,
            failed_check: None,
            assertion_evidence: None,
            validated_workspace_sha256: None,
            executor_failure_code: None,
            mcp_task_output_tail: None,
            now: 21,
        },
    )
    .unwrap();
    let claim = db.claim_next_terminal_continuation().unwrap().unwrap();

    let projection = execution_projection(&claim.execution, 22, None);
    let serialized = serde_json::to_string(&projection).unwrap();
    assert!(!serialized.contains(&claim.claim_fence));
    for forbidden in [
        "claim_fence",
        "terminal_continuation_claim_fence",
        "continuation_delivery_state",
        "terminal_continuation_delivery_state",
    ] {
        assert!(
            projection.get(forbidden).is_none(),
            "leaked internal field {forbidden}"
        );
    }
}
