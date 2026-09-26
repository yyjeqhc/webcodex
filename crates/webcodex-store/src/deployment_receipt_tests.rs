use super::{
    Database, DeploymentOperation, DeploymentPrincipal, DeploymentReceiptBeginOutcome,
    DeploymentReceiptStoreError, DeploymentState, NewDeploymentReceipt,
};

fn principal() -> DeploymentPrincipal {
    DeploymentPrincipal {
        kind: "oauth2".to_string(),
        id: "user-deployer".to_string(),
    }
}

fn request(key: &str) -> NewDeploymentReceipt {
    NewDeploymentReceipt {
        principal: principal(),
        operation: DeploymentOperation::Deploy,
        client_id: "webpi-local".to_string(),
        idempotency_key: key.to_string(),
        request_hash: "a".repeat(64),
        target_manifest_json: r#"{"version":"0.4.1","artifacts":[{"name":"webpi.exe","sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}]}"#.to_string(),
        created_at: 1_000,
    }
}

#[test]
fn deployment_receipt_is_durable_idempotent_and_revision_fenced() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("deployment-receipts.db");
    let db = Database::open(&path).unwrap();

    let first = match db.begin_deployment_receipt(&request("deploy-001")).unwrap() {
        DeploymentReceiptBeginOutcome::Created(record) => record,
        DeploymentReceiptBeginOutcome::Replayed(_) => panic!("first admission must create"),
    };
    assert_eq!(first.state, "planned");
    assert_eq!(first.revision, 1);

    let replay = match db.begin_deployment_receipt(&request("deploy-001")).unwrap() {
        DeploymentReceiptBeginOutcome::Replayed(record) => record,
        DeploymentReceiptBeginOutcome::Created(_) => panic!("same request must replay"),
    };
    assert_eq!(replay.receipt_id, first.receipt_id);

    let mut changed = request("deploy-001");
    changed.request_hash = "c".repeat(64);
    assert!(matches!(
        db.begin_deployment_receipt(&changed),
        Err(DeploymentReceiptStoreError::IdempotencyConflict)
    ));

    let ready = db
        .transition_deployment_receipt(
            &principal(),
            &first.receipt_id,
            1,
            DeploymentState::Ready,
            1_010,
            None,
        )
        .unwrap();
    assert_eq!(ready.state, "ready");
    assert_eq!(ready.revision, 2);
    assert!(matches!(
        db.transition_deployment_receipt(
            &principal(),
            &first.receipt_id,
            1,
            DeploymentState::Switching,
            1_020,
            None,
        ),
        Err(DeploymentReceiptStoreError::RevisionConflict { actual_revision: 2 })
    ));

    let switching = db
        .transition_deployment_receipt(
            &principal(),
            &first.receipt_id,
            2,
            DeploymentState::Switching,
            1_020,
            None,
        )
        .unwrap();
    let verifying = db
        .transition_deployment_receipt(
            &principal(),
            &first.receipt_id,
            switching.revision,
            DeploymentState::Verifying,
            1_025,
            None,
        )
        .unwrap();
    let terminal = db
        .transition_deployment_receipt(
            &principal(),
            &first.receipt_id,
            verifying.revision,
            DeploymentState::Succeeded,
            1_030,
            None,
        )
        .unwrap();
    assert_eq!(terminal.revision, 5);
    assert_eq!(terminal.terminal_at, Some(1_030));

    let raw_key_count: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM wc_deployment_receipts WHERE idempotency_key_hash=?1",
            ["deploy-001"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        raw_key_count, 0,
        "plaintext idempotency key must never be stored"
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let recovered = reopened
        .read_deployment_receipt(&principal(), &first.receipt_id)
        .unwrap();
    assert_eq!(recovered.state, "succeeded");
    assert_eq!(recovered.revision, 5);
    assert_eq!(recovered.terminal_at, Some(1_030));
}

#[test]
fn deployment_receipt_rejects_skipped_and_terminal_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("deployment-transitions.db")).unwrap();
    let receipt = match db
        .begin_deployment_receipt(&request("deploy-transitions"))
        .unwrap()
    {
        DeploymentReceiptBeginOutcome::Created(record) => record,
        _ => unreachable!(),
    };
    assert!(matches!(
        db.transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            1,
            DeploymentState::Switching,
            1_010,
            None,
        ),
        Err(DeploymentReceiptStoreError::InvalidTransition { .. })
    ));
    let failed = db
        .transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            1,
            DeploymentState::Failed,
            1_020,
            Some("preflight_failed"),
        )
        .unwrap();
    assert!(matches!(
        db.transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            failed.revision,
            DeploymentState::Ready,
            1_030,
            None,
        ),
        Err(DeploymentReceiptStoreError::InvalidTransition { .. })
    ));
}

#[test]
fn deployment_receipt_backup_identity_is_safe_and_persistent() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("deployment-backup.db");
    let db = Database::open(&path).unwrap();
    let receipt = match db
        .begin_deployment_receipt(&request("deploy-backup"))
        .unwrap()
    {
        DeploymentReceiptBeginOutcome::Created(record) => record,
        _ => unreachable!(),
    };
    let ready = db
        .transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            receipt.revision,
            DeploymentState::Ready,
            1_010,
            None,
        )
        .unwrap();
    let switching = db
        .transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            ready.revision,
            DeploymentState::Switching,
            1_020,
            None,
        )
        .unwrap();
    assert!(matches!(
        db.transition_deployment_receipt_with_backup(
            &principal(),
            &receipt.receipt_id,
            switching.revision,
            DeploymentState::RolledBack,
            1_030,
            Some("candidate_start_failed"),
            Some("../escape"),
        ),
        Err(DeploymentReceiptStoreError::InvalidInput(_))
    ));
    let rolled_back = db
        .transition_deployment_receipt_with_backup(
            &principal(),
            &receipt.receipt_id,
            switching.revision,
            DeploymentState::RolledBack,
            1_030,
            Some("candidate_start_failed"),
            Some("backup-123-r3"),
        )
        .unwrap();
    assert_eq!(rolled_back.backup_id.as_deref(), Some("backup-123-r3"));
    drop(db);
    let reopened = Database::open(&path).unwrap();
    let recovered = reopened
        .read_deployment_receipt(&principal(), &receipt.receipt_id)
        .unwrap();
    assert_eq!(recovered.backup_id.as_deref(), Some("backup-123-r3"));
}

#[test]
fn deployment_receipt_schema_adds_backup_id_to_existing_table() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("deployment-migration.db");
    let db = Database::open(&path).unwrap();
    db.conn_for_tests()
        .execute(
            "ALTER TABLE wc_deployment_receipts DROP COLUMN backup_id",
            [],
        )
        .unwrap();
    let before: i64 = db
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('wc_deployment_receipts') WHERE name='backup_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, 0);
    drop(db);

    let reopened = Database::open(&path).unwrap();
    let after: i64 = reopened
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('wc_deployment_receipts') WHERE name='backup_id'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(after, 1);
}

#[test]
fn deployment_receipt_terminal_error_code_is_bounded_and_safe() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("deployment-error.db")).unwrap();
    let receipt = match db
        .begin_deployment_receipt(&request("deploy-error"))
        .unwrap()
    {
        DeploymentReceiptBeginOutcome::Created(record) => record,
        _ => unreachable!(),
    };
    assert!(matches!(
        db.transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            1,
            DeploymentState::Failed,
            2_000,
            Some("contains secret value!"),
        ),
        Err(DeploymentReceiptStoreError::InvalidInput(_))
    ));
    let failed = db
        .transition_deployment_receipt(
            &principal(),
            &receipt.receipt_id,
            1,
            DeploymentState::Failed,
            2_000,
            Some("candidate_hash_mismatch"),
        )
        .unwrap();
    assert_eq!(
        failed.last_error_code.as_deref(),
        Some("candidate_hash_mismatch")
    );
}
