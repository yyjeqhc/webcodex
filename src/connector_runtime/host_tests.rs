use super::workspace::{LocalResultDecision, WorkspaceManager};
use super::ConnectorContext;
use crate::db::{ConnectorBinding, ConnectorTaskStoreError, NewConnectorResult, NewConnectorTask};
use crate::Database;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};

const TASK_ID: &str = "wc_task_f123456789abcdef0123456789abcdef";
const RESULT_ID: &str = "wc_result_f123456789abcdef";
const SUBJECT: &str = "user:owner";

struct Fixture {
    temp: tempfile::TempDir,
    context: ConnectorContext,
    db: Database,
}

impl Fixture {
    fn decide(
        &self,
        result_id: Option<&str>,
        decision: LocalResultDecision,
        now: i64,
    ) -> Result<crate::db::ConnectorTaskResult, ConnectorTaskStoreError> {
        decide(&self.db, &self.context, result_id, decision, now)
    }
}

fn fixture(finish: bool) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("README.md"), "before\n").unwrap();
    git(&root, &["add", "README.md"]);
    git(
        &root,
        &[
            "-c",
            "user.name=WebCodex Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "-qm",
            "initial",
        ],
    );
    let state = temp.path().join("state");
    let context = ConnectorContext {
        project_id: "wc_proj_1234567890".to_string(),
        project_name: "project".to_string(),
        workspace_id: "wc_ws_1234567890".to_string(),
        executor_project: "agent:hosted:project".to_string(),
        executor_root: root.to_string_lossy().into_owned(),
        runs_root: state.join("runs").to_string_lossy().into_owned(),
        results_root: state.join("results").to_string_lossy().into_owned(),
        projects_dir: state
            .join("agent/projects.d")
            .to_string_lossy()
            .into_owned(),
        profile: "personal".to_string(),
        project_grant_id: "wc_pgrant_1111111111111111".to_string(),
    };
    let db = Database::open(&temp.path().join("connector.db")).unwrap();
    db.ensure_connector_binding(ConnectorBinding {
        project_id: &context.project_id,
        project_name: &context.project_name,
        workspace_id: &context.workspace_id,
        executor_ref: &context.executor_project,
        subject_id: SUBJECT,
        profile: "personal",
        now: 1,
    })
    .unwrap();
    let manager = WorkspaceManager::new(&context).unwrap();
    let prepared = manager
        .prepare(
            &context,
            TASK_ID,
            "wc_run_f123456789abcdef0123456789abcdef",
            false,
        )
        .unwrap();
    let task = db
        .start_connector_task(NewConnectorTask {
            task_id: TASK_ID,
            run_id: &prepared.run_id,
            project_id: &context.project_id,
            workspace_id: &context.workspace_id,
            subject_id: SUBJECT,
            goal: "update readme",
            mode: "normal",
            target_executor_ref: &context.executor_project,
            execution_executor_ref: &prepared.execution_executor_ref,
            target_root: &context.executor_root,
            execution_root: &prepared.execution_root,
            baseline_commit: prepared.baseline_commit.as_deref(),
            baseline_tree: prepared.baseline_tree.as_deref(),
            isolated: true,
            now: 2,
        })
        .unwrap();
    if finish {
        fs::write(Path::new(&task.execution_root).join("README.md"), "after\n").unwrap();
        let captured = manager.capture_result(&task).unwrap();
        db.finish_connector_task(
            TASK_ID,
            &context.project_id,
            SUBJECT,
            NewConnectorResult {
                result_id: RESULT_ID,
                summary: "updated readme",
                patch_artifact: captured.patch_artifact.as_deref(),
                patch_sha256: captured.patch_sha256.as_deref(),
                patch_bytes: captured.patch_bytes,
                changed_paths: &captured.changed_paths,
                validation: &serde_json::json!({"status": "not_run"}),
                warnings: &captured.warnings,
            },
            3,
        )
        .unwrap();
    }
    Fixture { temp, context, db }
}

fn decide(
    db: &Database,
    context: &ConnectorContext,
    result_id: Option<&str>,
    decision: LocalResultDecision,
    now: i64,
) -> Result<crate::db::ConnectorTaskResult, ConnectorTaskStoreError> {
    WorkspaceManager::decide_connector_result_local(
        db,
        &context.project_id,
        TASK_ID,
        result_id,
        Path::new(&context.executor_root),
        decision,
        "local_test",
        now,
    )
}

fn assert_decision_error(
    result: Result<crate::db::ConnectorTaskResult, ConnectorTaskStoreError>,
    expected: &str,
) {
    assert!(matches!(
        result,
        Err(ConnectorTaskStoreError::Decision(code, _)) if code == expected
    ));
}

fn git(root: &Path, args: &[&str]) {
    assert!(Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success());
}

fn target(context: &ConnectorContext) -> PathBuf {
    Path::new(&context.executor_root).join("README.md")
}

#[test]
fn queue_filters_completed_history_before_limit() {
    let fx = fixture(false);
    fx.db
        .conn_for_tests()
        .execute_batch(
            "WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM n WHERE x < 81)
         INSERT INTO wc_tasks
             (id, project_id, owner_subject_id, goal, mode, status, created_at, updated_at)
         SELECT printf('wc_task_history_%04d', x), 'wc_proj_1234567890', 'user:owner',
                'closed history', 'normal', 'ready_for_review', 100 + x, 100 + x FROM n;
         WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM n WHERE x < 81)
         INSERT INTO wc_runs (id, task_id, workspace_id, status, started_at, finished_at)
         SELECT printf('wc_run_history_%04d', x), printf('wc_task_history_%04d', x),
                'wc_ws_1234567890', 'completed', 100 + x, 100 + x FROM n;
         WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x + 1 FROM n WHERE x < 81)
         INSERT INTO wc_task_results
             (id, task_id, run_id, summary, patch_bytes, changed_paths_json,
              validation_json, warnings_json, decision_status, created_at)
         SELECT printf('wc_result_history_%04d', x), printf('wc_task_history_%04d', x),
                printf('wc_run_history_%04d', x), 'closed', 0, '[]',
                '{\"status\":\"passed\"}', '[]', 'accepted', 100 + x FROM n;",
        )
        .unwrap();
    let rows = fx
        .db
        .local_reviewable_tasks(&fx.context.project_id, false, 20)
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].task_id, TASK_ID);
}

#[test]
fn local_decision_binds_result_and_exact_retry() {
    let fx = fixture(true);
    let cursor = fx
        .db
        .local_connector_task(TASK_ID, &fx.context.project_id)
        .unwrap()
        .event_cursor;
    assert_decision_error(
        fx.decide(Some("wc_result_stale"), LocalResultDecision::Accept, 4),
        "result_changed",
    );
    assert_eq!(fs::read_to_string(target(&fx.context)).unwrap(), "before\n");
    assert_eq!(
        fx.db
            .local_connector_task(TASK_ID, &fx.context.project_id)
            .unwrap()
            .event_cursor,
        cursor
    );
    fx.decide(Some(RESULT_ID), LocalResultDecision::Accept, 5)
        .unwrap();
    assert_eq!(fs::read_to_string(target(&fx.context)).unwrap(), "after\n");
    assert_decision_error(
        fx.decide(Some(RESULT_ID), LocalResultDecision::Accept, 6),
        "result_already_decided",
    );
}

#[test]
fn local_accept_preserves_workspace_preconditions() {
    for case in ["head", "artifact"] {
        let fx = fixture(true);
        if case == "head" {
            git(
                Path::new(&fx.context.executor_root),
                &[
                    "-c",
                    "user.name=WebCodex Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    "moved",
                ],
            );
        } else {
            let result = fx
                .db
                .local_connector_task_result(TASK_ID, &fx.context.project_id)
                .unwrap()
                .unwrap();
            fs::write(result.patch_artifact.unwrap(), "tampered\n").unwrap();
        }
        let error = fx
            .decide(Some(RESULT_ID), LocalResultDecision::Accept, 4)
            .unwrap_err();
        assert!(matches!(
            error,
            ConnectorTaskStoreError::Decision(
                "target_checkout_changed" | "result_precondition_failed",
                _
            )
        ));
    }
}

#[test]
fn finalization_failure_is_recovered_once_after_reopen() {
    let fx = fixture(true);
    fx.db
        .conn_for_tests()
        .execute_batch(
            "CREATE TEMP TRIGGER fail_result_finalize
             BEFORE UPDATE OF decision_status ON wc_task_results
             WHEN NEW.decision_status = 'accepted'
             BEGIN SELECT RAISE(FAIL, 'injected finalization failure'); END;",
        )
        .unwrap();
    assert!(matches!(
        fx.decide(Some(RESULT_ID), LocalResultDecision::Accept, 4),
        Err(ConnectorTaskStoreError::Storage(_))
    ));
    assert_eq!(fs::read_to_string(target(&fx.context)).unwrap(), "after\n");
    assert_eq!(
        fx.db
            .local_connector_task_result(TASK_ID, &fx.context.project_id)
            .unwrap()
            .unwrap()
            .decision_status,
        "pending"
    );
    let Fixture { temp, context, db } = fx;
    drop(db);
    let reopened = Database::open(&temp.path().join("connector.db")).unwrap();
    assert_eq!(
        WorkspaceManager::recover_result_decisions(
            &reopened,
            &context.project_id,
            Path::new(&context.executor_root),
            5,
        )
        .unwrap(),
        1
    );
    assert_eq!(
        reopened
            .local_connector_task_result(TASK_ID, &context.project_id)
            .unwrap()
            .unwrap()
            .decision_status,
        "accepted"
    );
    assert_eq!(fs::read_to_string(target(&context)).unwrap(), "after\n");
    assert_eq!(
        WorkspaceManager::recover_result_decisions(
            &reopened,
            &context.project_id,
            Path::new(&context.executor_root),
            6,
        )
        .unwrap(),
        0
    );
}

#[test]
fn concurrent_decisions_apply_once() {
    let fx = fixture(true);
    let db = Arc::new(fx.db);
    let barrier = Arc::new(Barrier::new(3));
    let threads = [4, 5].map(|now| {
        let (db, context, barrier) = (db.clone(), fx.context.clone(), barrier.clone());
        std::thread::spawn(move || {
            barrier.wait();
            decide(
                &db,
                &context,
                Some(RESULT_ID),
                LocalResultDecision::Accept,
                now,
            )
        })
    });
    barrier.wait();
    let outcomes = threads.map(|thread| thread.join().unwrap());
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    assert_eq!(fs::read_to_string(target(&fx.context)).unwrap(), "after\n");
}

#[test]
fn interrupted_no_result_reject_is_the_only_identity_exception() {
    let fx = fixture(false);
    fx.db
        .reconcile_connector_executions(&fx.context.project_id, 4)
        .unwrap();
    assert_decision_error(
        fx.decide(Some("wc_result_claimed"), LocalResultDecision::Reject, 5),
        "result_changed",
    );
    fx.decide(None, LocalResultDecision::Reject, 6).unwrap();
    assert_eq!(
        fx.db
            .local_connector_task_result(TASK_ID, &fx.context.project_id)
            .unwrap()
            .unwrap()
            .decision_status,
        "rejected"
    );
}

#[test]
fn rejected_cleanup_can_be_retried() {
    let fx = fixture(true);
    let lease = Path::new(&fx.context.runs_root).join(".write-slot-01.lease.json");
    fs::write(&lease, "broken").unwrap();
    let first = fx
        .decide(Some(RESULT_ID), LocalResultDecision::Reject, 4)
        .unwrap();
    assert!(first.cleanup_warning.is_some());
    fs::remove_file(lease).unwrap();
    let retry = fx
        .decide(Some(RESULT_ID), LocalResultDecision::Reject, 5)
        .unwrap();
    assert!(retry.cleanup_warning.is_none());
}
