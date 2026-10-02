#![allow(clippy::all)]

use super::*;
use rusqlite::params;

fn digest(character: char) -> String {
    character.to_string().repeat(64)
}

fn communication_principal(kind: &str, character: char) -> ArtifactHandoffPrincipal {
    ArtifactHandoffPrincipal::try_from(CommunicationPrincipal {
        kind: kind.to_string(),
        digest: format!("wc_commprincipal_{}", digest(character)),
    })
    .unwrap()
}

fn source_snapshot() -> ArtifactHandoffSourceSnapshot {
    ArtifactHandoffSourceSnapshot {
        path: "artifacts/private-report.txt".to_string(),
        bytes: 42,
        sha256: digest('a'),
        mime_type: "text/plain".to_string(),
        name: "private-report.txt".to_string(),
    }
}

fn new_grant(
    destination_principal: ArtifactHandoffPrincipal,
    ttl_ms: Option<i64>,
) -> NewArtifactHandoffGrant {
    NewArtifactHandoffGrant {
        source_project: "agent:source-runner:source-project".to_string(),
        source_snapshot: source_snapshot(),
        destination_principal,
        destination_project: "agent:destination-runner:destination-project".to_string(),
        operation: ArtifactHandoffOperation::Read,
        one_shot: true,
        ttl_ms,
    }
}

fn outcome() -> ArtifactHandoffAcceptanceOutcome {
    ArtifactHandoffAcceptanceOutcome {
        destination_path: "imports/handed-off-report.txt".to_string(),
        destination_bytes: 42,
        destination_sha256: digest('a'),
    }
}

fn import_request(grant_id: &str) -> ArtifactHandoffImportRequest {
    ArtifactHandoffImportRequest {
        grant_id: grant_id.to_string(),
        destination_project: "agent:destination-runner:destination-project".to_string(),
        destination_path: "imports/handed-off-report.txt".to_string(),
        overwrite: false,
    }
}

fn assert_unavailable(error: &ArtifactHandoffStoreError) {
    assert_eq!(error.code(), "artifact_handoff_grant_unavailable");
}

#[test]
fn create_read_revoke_and_restart_preserve_the_bound_grant() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact-handoff.db");
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let now = 1_000;

    let grant = {
        let db = Database::open(&path).unwrap();
        let grant = db
            .create_artifact_handoff_grant(
                &source,
                new_grant(destination.clone(), Some(60_000)),
                now,
            )
            .unwrap();
        assert!(grant.grant_id.starts_with(ARTIFACT_HANDOFF_GRANT_ID_PREFIX));
        assert_eq!(grant.source_principal, source);
        assert_eq!(grant.destination_principal, destination);
        assert_eq!(grant.expires_at_unix_ms, now + 60_000);
        assert_eq!(grant.state(now), ArtifactHandoffGrantState::Active);

        assert_eq!(
            db.read_artifact_handoff_grant(
                &source,
                "agent:source-runner:source-project",
                &grant.grant_id,
                now,
            )
            .unwrap(),
            grant
        );
        assert_eq!(
            db.read_artifact_handoff_grant(
                &destination,
                "agent:destination-runner:destination-project",
                &grant.grant_id,
                now,
            )
            .unwrap(),
            grant
        );
        grant
    };

    let db = Database::open(&path).unwrap();
    let reopened = db
        .read_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            &grant.grant_id,
            now + 1,
        )
        .unwrap();
    assert_eq!(reopened, grant);

    let revoked = db
        .revoke_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            &grant.grant_id,
            now + 2,
        )
        .unwrap();
    assert_eq!(revoked.state(now + 2), ArtifactHandoffGrantState::Revoked);
    assert!(db
        .read_artifact_handoff_grant(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            now + 3,
        )
        .is_err());
    assert_unavailable(
        &db.begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "revoked-before-acceptance",
            &digest('1'),
            now + 3,
        )
        .unwrap_err(),
    );

    drop(db);
    let reopened = Database::open(&path).unwrap();
    let revoked_at = reopened
        .conn_for_tests()
        .query_row(
            "SELECT revoked_at_unix_ms FROM wc_artifact_handoff_grants WHERE grant_id = ?1",
            params![grant.grant_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .unwrap();
    assert_eq!(revoked_at, Some(now + 2));
    assert_unavailable(
        &reopened
            .read_artifact_handoff_grant(
                &source,
                "agent:source-runner:source-project",
                &grant.grant_id,
                now + 4,
            )
            .unwrap_err(),
    );
}

#[test]
fn wrong_principal_project_and_unknown_grant_are_indistinguishable() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("artifact-handoff-private.db")).unwrap();
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let wrong = communication_principal("oauth2", 'c');
    let grant = db
        .create_artifact_handoff_grant(&source, new_grant(destination, Some(60_000)), 1_000)
        .unwrap();

    let wrong_principal = db
        .read_artifact_handoff_grant(
            &wrong,
            "agent:source-runner:source-project",
            &grant.grant_id,
            1_001,
        )
        .unwrap_err();
    let wrong_source_project = db
        .read_artifact_handoff_grant(&source, "agent:other:project", &grant.grant_id, 1_001)
        .unwrap_err();
    let wrong_destination_project = db
        .read_artifact_handoff_grant(
            &grant.destination_principal,
            "agent:other:project",
            &grant.grant_id,
            1_001,
        )
        .unwrap_err();
    let missing = db
        .read_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            "wc_handoff_mZmZmZmZmZmZmZmZ",
            1_001,
        )
        .unwrap_err();

    for error in [
        wrong_principal,
        wrong_source_project,
        wrong_destination_project,
        missing,
    ] {
        assert_unavailable(&error);
        let public = format!("{error:?} {error}");
        assert!(!public.contains("source-runner"));
        assert!(!public.contains("private-report"));
        assert!(!public.contains(&digest('a')));
    }
}

#[test]
fn expiry_defaults_and_server_cap_are_durable_and_fail_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("artifact-handoff-expiry.db")).unwrap();
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let now = 5_000;

    let default_grant = db
        .create_artifact_handoff_grant(&source, new_grant(destination.clone(), None), now)
        .unwrap();
    assert_eq!(
        default_grant.expires_at_unix_ms - now,
        DEFAULT_ARTIFACT_HANDOFF_TTL_MS
    );

    let capped_grant = db
        .create_artifact_handoff_grant(
            &source,
            new_grant(
                destination.clone(),
                Some(MAX_ARTIFACT_HANDOFF_TTL_MS.saturating_add(60_000)),
            ),
            now,
        )
        .unwrap();
    assert_eq!(
        capped_grant.expires_at_unix_ms - now,
        MAX_ARTIFACT_HANDOFF_TTL_MS
    );

    let already_expired = db
        .create_artifact_handoff_grant(&source, new_grant(destination.clone(), Some(0)), now)
        .unwrap_err();
    assert_eq!(already_expired.code(), "invalid_artifact_handoff_ttl");

    let expired = db
        .read_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            &default_grant.grant_id,
            default_grant.expires_at_unix_ms,
        )
        .unwrap_err();
    assert_unavailable(&expired);
    assert_unavailable(
        &db.begin_artifact_handoff_import(
            &destination,
            &import_request(&default_grant.grant_id),
            "expired-import",
            default_grant.expires_at_unix_ms,
        )
        .unwrap_err(),
    );
}

#[test]
fn acceptance_replay_reconciles_and_conflicting_replay_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("artifact-handoff-replay.db")).unwrap();
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let now = 10_000;
    let grant = db
        .create_artifact_handoff_grant(&source, new_grant(destination.clone(), Some(1_000)), now)
        .unwrap();
    let request_hash = digest('1');

    let started = db
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "accept-once",
            &request_hash,
            now + 1,
        )
        .unwrap();
    assert!(!started.replayed);
    assert_eq!(
        started.acceptance.state,
        ArtifactHandoffAcceptanceState::Prepared
    );

    let replay = db
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "accept-once",
            &request_hash,
            now + 2,
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        replay.acceptance.acceptance_id,
        started.acceptance.acceptance_id
    );

    let conflict = db
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "accept-once",
            &digest('2'),
            now + 3,
        )
        .unwrap_err();
    assert_eq!(
        conflict.code(),
        "artifact_handoff_acceptance_idempotency_conflict"
    );

    let mismatched_outcome = ArtifactHandoffAcceptanceOutcome {
        destination_sha256: digest('b'),
        ..outcome()
    };
    let integrity_error = db
        .complete_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            &started.acceptance.acceptance_id,
            mismatched_outcome,
            now + 4,
        )
        .unwrap_err();
    assert_eq!(
        integrity_error.code(),
        "invalid_artifact_handoff_acceptance_outcome"
    );
    assert_eq!(
        db.read_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            &grant.grant_id,
            now + 4,
        )
        .unwrap()
        .state(now + 4),
        ArtifactHandoffGrantState::Active
    );

    let premature_completion = db
        .complete_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            &started.acceptance.acceptance_id,
            outcome(),
            now + 4,
        )
        .unwrap_err();
    assert_unavailable(&premature_completion);

    let reconcile_allowed = db
        .mark_artifact_handoff_acceptance_destination_reconcile_allowed(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            &started.acceptance.acceptance_id,
            now + 4,
        )
        .unwrap();
    assert!(reconcile_allowed.acceptance.destination_reconcile_allowed);

    let completed = db
        .complete_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            &started.acceptance.acceptance_id,
            outcome(),
            now + 4,
        )
        .unwrap();
    assert_eq!(completed.state, ArtifactHandoffAcceptanceState::Completed);
    assert_eq!(completed.outcome, Some(outcome()));

    let replay_after_expiry = db
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "accept-once",
            &request_hash,
            grant.expires_at_unix_ms + 1,
        )
        .unwrap();
    assert!(replay_after_expiry.replayed);
    assert_eq!(replay_after_expiry.acceptance, completed);
    assert_unavailable(
        &db.begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant.grant_id,
            "new-key-after-consumption",
            &digest('3'),
            now + 5,
        )
        .unwrap_err(),
    );
    assert_unavailable(
        &db.read_artifact_handoff_grant(
            &source,
            "agent:source-runner:source-project",
            &grant.grant_id,
            now + 6,
        )
        .unwrap_err(),
    );
}

#[test]
fn acceptance_identity_and_completion_recover_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact-handoff-restart.db");
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let request_hash = digest('4');

    let (grant_id, acceptance_id) = {
        let db = Database::open(&path).unwrap();
        let grant = db
            .create_artifact_handoff_grant(
                &source,
                new_grant(destination.clone(), Some(60_000)),
                20_000,
            )
            .unwrap();
        let claim = db
            .begin_artifact_handoff_acceptance(
                &destination,
                "agent:destination-runner:destination-project",
                &grant.grant_id,
                "restart-replay",
                &request_hash,
                20_001,
            )
            .unwrap();
        (grant.grant_id, claim.acceptance.acceptance_id)
    };

    let db = Database::open(&path).unwrap();
    let replay = db
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant_id,
            "restart-replay",
            &request_hash,
            20_002,
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.acceptance.acceptance_id, acceptance_id);

    let reconcile_allowed = db
        .mark_artifact_handoff_acceptance_destination_reconcile_allowed(
            &destination,
            "agent:destination-runner:destination-project",
            &grant_id,
            &acceptance_id,
            20_003,
        )
        .unwrap();
    assert!(reconcile_allowed.acceptance.destination_reconcile_allowed);

    let completed = db
        .complete_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant_id,
            &acceptance_id,
            outcome(),
            20_003,
        )
        .unwrap();
    assert_eq!(completed.state, ArtifactHandoffAcceptanceState::Completed);
    drop(db);

    let reopened = Database::open(&path).unwrap();
    let replay = reopened
        .begin_artifact_handoff_acceptance(
            &destination,
            "agent:destination-runner:destination-project",
            &grant_id,
            "restart-replay",
            &request_hash,
            20_004,
        )
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.acceptance, completed);
    assert!(replay.acceptance.destination_reconcile_allowed);
}

#[test]
fn artifact_handoff_schema_migrates_destination_reconcile_allowed_fence() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("artifact-handoff-migration.db");
    let db = Database::open(&path).unwrap();
    db.conn_for_tests()
        .execute(
            "ALTER TABLE wc_artifact_handoff_acceptances DROP COLUMN destination_reconcile_allowed",
            [],
        )
        .unwrap();
    drop(db);

    let reopened = Database::open(&path).unwrap();
    let count: i64 = reopened
        .conn_for_tests()
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('wc_artifact_handoff_acceptances')
             WHERE name = 'destination_reconcile_allowed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn import_request_hash_binds_every_semantic_field_and_prepared_claim_revalidates_live_authority() {
    let temp = tempfile::tempdir().unwrap();
    let db = Database::open(&temp.path().join("artifact-handoff-import.db")).unwrap();
    let source = communication_principal("oauth2", 'a');
    let destination = communication_principal("oauth2", 'b');
    let grant = db
        .create_artifact_handoff_grant(
            &source,
            new_grant(destination.clone(), Some(60_000)),
            30_000,
        )
        .unwrap();
    let request = import_request(&grant.grant_id);
    let request_hash = request.request_hash().unwrap();
    assert_eq!(request.request_hash().unwrap(), request_hash);

    let mut changed_path = request.clone();
    changed_path.destination_path = "imports/other.txt".to_string();
    assert_ne!(changed_path.request_hash().unwrap(), request_hash);

    let mut changed_overwrite = request.clone();
    changed_overwrite.overwrite = true;
    assert_ne!(changed_overwrite.request_hash().unwrap(), request_hash);

    let mut changed_grant = request.clone();
    changed_grant.grant_id = "wc_handoff_mZmZmZmZmZmZmZmZ".to_string();
    assert_ne!(changed_grant.request_hash().unwrap(), request_hash);

    let mut changed_project = request.clone();
    changed_project.destination_project = "agent:other:destination-project".to_string();
    assert_ne!(changed_project.request_hash().unwrap(), request_hash);

    let claim = db
        .begin_artifact_handoff_import(&destination, &request, "accept-import", 30_001)
        .unwrap();
    assert!(!claim.replayed);
    assert_eq!(claim.grant, grant);
    assert_eq!(
        claim.acceptance.state,
        ArtifactHandoffAcceptanceState::Prepared
    );
    assert!(!claim.acceptance.destination_reconcile_allowed);

    let revalidated = db
        .revalidate_artifact_handoff_acceptance(
            &destination,
            &grant.destination_project,
            &grant.grant_id,
            &claim.acceptance.acceptance_id,
            30_002,
        )
        .unwrap();
    assert_eq!(revalidated.grant, grant);
    assert_eq!(revalidated.acceptance, claim.acceptance);

    let reconcile_allowed = db
        .mark_artifact_handoff_acceptance_destination_reconcile_allowed(
            &destination,
            &grant.destination_project,
            &grant.grant_id,
            &claim.acceptance.acceptance_id,
            30_002,
        )
        .unwrap();
    assert!(reconcile_allowed.acceptance.destination_reconcile_allowed);

    db.revoke_artifact_handoff_grant(&source, &grant.source_project, &grant.grant_id, 30_003)
        .unwrap();
    assert_unavailable(
        &db.revalidate_artifact_handoff_acceptance(
            &destination,
            &grant.destination_project,
            &grant.grant_id,
            &claim.acceptance.acceptance_id,
            30_004,
        )
        .unwrap_err(),
    );
}
