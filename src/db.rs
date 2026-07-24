use crate::models::PairingCodeRecord;
#[cfg(test)]
use crate::models::{
    ApiKeyRecord, OAuthAccessTokenRecord, OAuthAuthorizationCodeRecord, OAuthClientRecord,
    OAuthRefreshTokenRecord, UserRecord,
};
use rusqlite::Connection;
use std::sync::Mutex;

mod accounts;
mod audit;
mod execution_model;
mod executions;
mod oauth;
mod schema;
mod task_kernel;

pub(crate) use self::execution_model::{
    ConnectorExecution, ConnectorExecutionFailure, ConnectorExecutionObservation,
    ConnectorExecutionReservation, MAX_ASSERTION_EVIDENCE_BYTES,
};
pub use self::oauth::RotateResult;
#[allow(unused_imports)]
pub(crate) use self::task_kernel::{
    ConnectorApproval, ConnectorApprovalGate, ConnectorBinding, ConnectorEditOperationGate,
    ConnectorPreservedWorkspace, ConnectorTaskEvent, ConnectorTaskResult, ConnectorTaskSnapshot,
    ConnectorTaskStoreError, LocalReviewableTask, NewConnectorResult, NewConnectorTask,
};

pub struct Database {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub enum PairingConsumeResult {
    NotFound,
    Consumed(PairingCodeRecord),
    AlreadyUsed(PairingCodeRecord),
    Expired(PairingCodeRecord),
    ClientMismatch(PairingCodeRecord),
}

#[cfg(test)]
impl Database {
    /// Test-only access to the underlying connection so tests can assert on
    /// raw storage (e.g. that a plaintext token is never stored as `key_hash`).
    pub fn conn_for_tests(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_drops_retired_legacy_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("legacy.db");
        // Seed a pre-cleanup database shape with tables the product no longer uses.
        {
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE messages (id TEXT PRIMARY KEY);
                CREATE TABLE command_requests (id TEXT PRIMARY KEY);
                CREATE TABLE codex_goals (id TEXT PRIMARY KEY);
                CREATE TABLE agent_specs (id TEXT PRIMARY KEY);
                CREATE TABLE agent_model_profiles (id TEXT PRIMARY KEY);
                CREATE TABLE desktop_tasks (id TEXT PRIMARY KEY);
                CREATE TABLE desktop_task_events (id TEXT PRIMARY KEY);
                ",
            )
            .unwrap();
        }
        let db = Database::open(&path).unwrap();
        let conn = db.conn_for_tests();
        for table in [
            "messages",
            "command_requests",
            "codex_goals",
            "agent_specs",
            "agent_model_profiles",
            "desktop_tasks",
            "desktop_task_events",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 0, "legacy table {table} must be dropped on open");
        }
        // Active surface still exists.
        for table in [
            "users",
            "api_keys",
            "action_sessions",
            "oauth_clients",
            "wc_run_contexts",
            "wc_task_results",
            "wc_approvals",
            "wc_edit_operations",
            "wc_executions",
        ] {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(exists, 1, "active table {table} must remain");
        }
    }

    #[test]
    fn open_enables_wal_busy_timeout_and_foreign_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("pragma.db")).unwrap();
        let conn = db.conn_for_tests();
        let journal: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal");
        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);
    }

    #[test]
    fn execution_provenance_columns_are_fresh_additive_and_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let upgrade = tmp.path().join("iteration-7.db");
        {
            let conn = rusqlite::Connection::open(&upgrade).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE wc_executions AS SELECT
                    'old-command' id, 'command' kind, 'task' task_id, 'run' run_id,
                    'succeeded' state, 1 submitted_at, NULL queued_at, 2 queue_deadline,
                    NULL started_at, NULL last_output_at, 3 finished_at,
                    1 stdout_cursor, 1 stderr_cursor, 0 exit_code,
                    NULL failure_source, NULL failure_code, NULL cancel_requested_at,
                    'exit_zero' terminal_reason, 'op' operation_id, 'hash' request_sha256,
                    NULL executor_reference, NULL first_status_failure_at,
                    NULL last_successful_observation_at, NULL status_failure_code,
                    NULL check_plan, 0 check_completed;
                ",
            )
            .unwrap();
        }
        let fresh = tmp.path().join("fresh.db");
        for path in [&upgrade, &fresh] {
            for _ in 0..2 {
                let db = Database::open(path).unwrap();
                let columns: i64 = db
                    .conn_for_tests()
                    .query_row(
                        "SELECT COUNT(*) FROM pragma_table_info('wc_executions')
                         WHERE name IN ('check_workspace_sha256', 'validated_workspace_sha256',
                                        'failed_check', 'assertion_evidence_json',
                                        'check_recipe_json')",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(columns, 5);
                if path == &upgrade {
                    let command = db.connector_execution("old-command").unwrap();
                    assert_eq!(command.kind, "command");
                    assert!(command.validated_workspace_sha256.is_none());
                    assert!(command.assertion_evidence.is_none());
                }
            }
        }
    }

    #[test]
    fn purge_stale_auth_rows_removes_dead_material_keeps_live() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("purge.db")).unwrap();
        let now = 1_700_000_000;
        let user = UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();

        // Live + dead API keys.
        db.insert_api_key(
            &ApiKeyRecord {
                id: "k-live".to_string(),
                user_id: "u-1".to_string(),
                name: "live".to_string(),
                key_prefix: "wc_pat_live".to_string(),
                created_at: now,
                last_used_at: None,
                revoked_at: None,
                scopes: "runtime:read".to_string(),
                expires_at: None,
                kind: "user".to_string(),
                allowed_client_id: None,
            },
            "hash-live",
        )
        .unwrap();
        db.insert_api_key(
            &ApiKeyRecord {
                id: "k-revoked".to_string(),
                user_id: "u-1".to_string(),
                name: "revoked".to_string(),
                key_prefix: "wc_pat_rev".to_string(),
                created_at: now,
                last_used_at: None,
                revoked_at: Some(now - 1),
                scopes: "runtime:read".to_string(),
                expires_at: None,
                kind: "user".to_string(),
                allowed_client_id: None,
            },
            "hash-revoked",
        )
        .unwrap();
        db.insert_api_key(
            &ApiKeyRecord {
                id: "k-expired".to_string(),
                user_id: "u-1".to_string(),
                name: "expired".to_string(),
                key_prefix: "wc_pat_exp".to_string(),
                created_at: now,
                last_used_at: None,
                revoked_at: None,
                scopes: "runtime:read".to_string(),
                expires_at: Some(now - 10),
                kind: "user".to_string(),
                allowed_client_id: None,
            },
            "hash-expired",
        )
        .unwrap();

        // Live + dead pairing codes.
        db.insert_pairing_code(&crate::models::PairingCodeRecord {
            id: "p-live".to_string(),
            code_hash: "pair-live".to_string(),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: "laptop".to_string(),
            created_at: now,
            expires_at: now + 600,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();
        db.insert_pairing_code(&crate::models::PairingCodeRecord {
            id: "p-dead".to_string(),
            code_hash: "pair-dead".to_string(),
            user_id: "u-1".to_string(),
            username: "alice".to_string(),
            client_id: "laptop".to_string(),
            created_at: now - 1000,
            expires_at: now - 1,
            used_at: None,
            user_token_name: None,
            agent_token_name: None,
        })
        .unwrap();

        let client = OAuthClientRecord {
            id: "oc-1".to_string(),
            client_id: "wc_client_test".to_string(),
            client_secret_hash: "secret-hash".to_string(),
            name: "test".to_string(),
            owner_user_id: "u-1".to_string(),
            redirect_uris: "https://example.com/cb".to_string(),
            allowed_scopes: "runtime:read".to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&client).unwrap();

        // Live access token + expired + revoked.
        db.insert_oauth_access_token(&OAuthAccessTokenRecord {
            id: "at-live".to_string(),
            token_hash: "ath-live".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: "u-1".to_string(),
            user_id: Some("u-1".to_string()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        })
        .unwrap();
        db.insert_oauth_access_token(&OAuthAccessTokenRecord {
            id: "at-exp".to_string(),
            token_hash: "ath-exp".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: "u-1".to_string(),
            user_id: Some("u-1".to_string()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now - 100,
            expires_at: now - 1,
            revoked_at: None,
            last_used_at: None,
        })
        .unwrap();
        db.insert_oauth_access_token(&OAuthAccessTokenRecord {
            id: "at-rev".to_string(),
            token_hash: "ath-rev".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: "u-1".to_string(),
            user_id: Some("u-1".to_string()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: Some(now - 1),
            last_used_at: None,
        })
        .unwrap();

        // Used authorization code should be purged.
        db.insert_oauth_authorization_code(
            &OAuthAuthorizationCodeRecord {
                id: "ac-used".to_string(),
                code_hash: "ach-used".to_string(),
                client_id: client.client_id.clone(),
                subject_kind: "managed_user".to_string(),
                subject_id: "u-1".to_string(),
                user_id: Some("u-1".to_string()),
                redirect_uri: "https://example.com/cb".to_string(),
                scopes: "runtime:read".to_string(),
                code_challenge: None,
                code_challenge_method: None,
                resource: None,
                shared_key_hash: None,
                created_at: now,
                expires_at: now + 60,
                used_at: Some(now),
                revoked_at: None,
            },
            "ach-used",
        )
        .unwrap();

        let deleted = db.purge_stale_auth_rows(now).unwrap();
        assert!(
            deleted >= 5,
            "expected several stale rows purged, got {deleted}"
        );

        assert!(db.get_api_key_by_hash("hash-live").unwrap().is_some());
        assert!(db.get_api_key_by_id("k-revoked").unwrap().is_none());
        assert!(db.get_api_key_by_id("k-expired").unwrap().is_none());
        assert!(db.get_pairing_code_by_hash("pair-live").unwrap().is_some());
        assert!(db.get_pairing_code_by_hash("pair-dead").unwrap().is_none());
        assert!(db
            .get_oauth_access_token_by_hash("ath-live")
            .unwrap()
            .is_some());
        assert!(db
            .get_oauth_access_token_by_hash("ath-exp")
            .unwrap()
            .is_none());
        // Revoked rows are deleted, not merely filtered.
        {
            let conn = db.conn_for_tests();
            let remaining: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM oauth_access_tokens WHERE id = 'at-rev'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(remaining, 0);
            let codes: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM oauth_authorization_codes WHERE id = 'ac-used'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(codes, 0);
        }
    }

    #[test]
    fn api_key_records_round_trip_and_revoked_keys_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let user = UserRecord {
            id: "user-1".to_string(),
            username: "alice".to_string(),
            created_at: 10,
            disabled: 0,
            display_name: Some("Alice".to_string()),
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(10),
        };
        db.create_user(&user).unwrap();

        let fetched = db.get_user_by_username("alice").unwrap().unwrap();
        assert_eq!(fetched.id, "user-1");
        assert_eq!(fetched.display_name.as_deref(), Some("Alice"));
        assert_eq!(fetched.role, "user");
        assert!(!fetched.is_disabled());
        assert_eq!(
            db.get_user_by_id("user-1").unwrap().unwrap().username,
            "alice"
        );

        let key = ApiKeyRecord {
            id: "key-1".to_string(),
            user_id: "user-1".to_string(),
            name: "main".to_string(),
            key_prefix: "pk_live".to_string(),
            created_at: 11,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:write".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, "hash-1").unwrap();
        let fetched_key = db.get_api_key_by_hash("hash-1").unwrap().unwrap();
        assert_eq!(fetched_key.name, "main");
        assert_eq!(
            fetched_key.scopes_vec(),
            vec!["runtime:read".to_string(), "project:write".to_string()]
        );

        db.update_api_key_last_used("key-1", 12).unwrap();
        assert_eq!(
            db.get_api_key_by_hash("hash-1")
                .unwrap()
                .unwrap()
                .last_used_at,
            Some(12)
        );

        let revoked_key = ApiKeyRecord {
            id: "key-2".to_string(),
            name: "revoked".to_string(),
            revoked_at: Some(13),
            ..key
        };
        db.insert_api_key(&revoked_key, "hash-2").unwrap();
        assert!(db.get_api_key_by_hash("hash-2").unwrap().is_none());
        assert_eq!(db.list_api_keys_by_user("user-1").unwrap().len(), 2);
        // revoke_api_key is idempotent and updates the existing row.
        let revoked = db.revoke_api_key("key-1", 99).unwrap().unwrap();
        assert_eq!(revoked.revoked_at, Some(99));
        let revoked_again = db.revoke_api_key("key-1", 100).unwrap().unwrap();
        assert_eq!(
            revoked_again.revoked_at,
            Some(99),
            "idempotent revoke must keep the original timestamp"
        );
    }

    #[test]
    fn list_users_returns_all_users_ordered_by_username() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        for (uname, role) in [("carol", "user"), ("alice", "admin"), ("bob", "user")] {
            db.create_user(&UserRecord {
                id: format!("u-{}", uname),
                username: uname.to_string(),
                created_at: now,
                disabled: 0,
                display_name: None,
                role: role.to_string(),
                disabled_at: None,
                updated_at: Some(now),
            })
            .unwrap();
        }
        let users = db.list_users().unwrap();
        let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
        assert_eq!(names, vec!["alice", "bob", "carol"]);
        assert_eq!(
            users.iter().find(|u| u.username == "alice").unwrap().role,
            "admin"
        );
    }

    #[test]
    fn set_user_disabled_marks_user_and_blocks_token_lookup_path() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let disabled = db.set_user_disabled("u-1", true, now).unwrap().unwrap();
        assert!(disabled.is_disabled());
        assert_eq!(disabled.disabled, 1);
        assert_eq!(disabled.disabled_at, Some(now));
        // Re-enabling clears both flags.
        let reenabled = db
            .set_user_disabled("u-1", false, now + 10)
            .unwrap()
            .unwrap();
        assert!(!reenabled.is_disabled());
        assert_eq!(reenabled.disabled, 0);
        assert_eq!(reenabled.disabled_at, None);
    }

    /// Phase 2 token lifecycle: create stores hash (not plaintext), lookup
    /// succeeds, revoked tokens are ignored, expired tokens report expired,
    /// disabled-user tokens are rejected at the auth layer, and last_used_at
    /// updates. Uses the same SHA-256 hash as the auth middleware.
    #[test]
    fn phase2_token_lifecycle_hash_revoked_expired_disabled_last_used() {
        use sha2::{Digest, Sha256};
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();

        // Create user.
        let user = UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        // Duplicate username rejected.
        let dup_err = db.create_user(&UserRecord {
            id: "u-2".to_string(),
            ..user.clone()
        });
        assert!(dup_err.is_err(), "duplicate username must be rejected");

        // Create token: store hash, never plaintext.
        let plaintext = "wc_pat_testsecretvalue1234567890";
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        let key_hash = format!("{:x}", hasher.finalize());
        let key = ApiKeyRecord {
            id: "k-1".to_string(),
            user_id: "u-1".to_string(),
            name: "main".to_string(),
            key_prefix: "wc_pat_testse".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read project:write".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, &key_hash).unwrap();

        // The stored key_hash must not be the plaintext token.
        let conn = db.conn_for_tests();
        let stored_hash: String = conn
            .query_row(
                "SELECT key_hash FROM api_keys WHERE id = 'k-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_hash, plaintext);
        assert_eq!(stored_hash, key_hash);
        drop(conn);

        // Lookup succeeds.
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert_eq!(fetched.name, "main");
        assert_eq!(
            fetched.scopes_vec(),
            vec!["runtime:read".to_string(), "project:write".to_string()]
        );
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));

        // last_used_at updates.
        db.update_api_key_last_used("k-1", now + 5).unwrap();
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert_eq!(fetched.last_used_at, Some(now + 5));

        // Revoked token is ignored by get_api_key_by_hash (returns None).
        db.revoke_api_key("k-1", now + 10).unwrap();
        assert!(db.get_api_key_by_hash(&key_hash).unwrap().is_none());
        // But get_api_key_by_id still returns it (with revoked_at set).
        let revoked = db.get_api_key_by_id("k-1").unwrap().unwrap();
        assert!(revoked.is_revoked());

        // Expired token: a non-revoked token with expires_at in the past
        // reports is_expired true (the auth middleware rejects it).
        let exp_key = ApiKeyRecord {
            id: "k-2".to_string(),
            revoked_at: None,
            expires_at: Some(now - 1),
            ..key.clone()
        };
        db.insert_api_key(&exp_key, "hash-exp").unwrap();
        let fetched = db.get_api_key_by_hash("hash-exp").unwrap().unwrap();
        assert!(fetched.is_expired(now));

        // Disabled-user token: the auth layer checks user.is_disabled(); here
        // we confirm the DB marks the user disabled and the record helper
        // reports it.
        db.set_user_disabled("u-1", true, now).unwrap();
        let disabled_user = db.get_user_by_id("u-1").unwrap().unwrap();
        assert!(disabled_user.is_disabled());
    }

    /// Phase 3: existing user tokens default to kind="user" after migration,
    /// and the model helpers correctly distinguish user vs agent tokens.
    #[test]
    fn phase3_existing_user_tokens_default_to_kind_user_after_migration() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        // Simulate a legacy Phase 2 row by constructing an ApiKeyRecord with
        // kind="user" (the migration default) and allowed_client_id=None.
        let key = ApiKeyRecord {
            id: "k-legacy".to_string(),
            user_id: "u-1".to_string(),
            name: "legacy".to_string(),
            key_prefix: "wc_pat_legacy".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&key, "hash-legacy").unwrap();
        let fetched = db.get_api_key_by_hash("hash-legacy").unwrap().unwrap();
        assert!(fetched.is_user_token(), "legacy token must be kind=user");
        assert!(!fetched.is_agent_token());
        assert_eq!(fetched.kind(), "user");
        assert!(fetched.allowed_client_id().is_none());
    }

    /// Phase 3: agent tokens are stored with kind=agent and allowed_client_id,
    /// and the hash (not plaintext) is persisted.
    #[test]
    fn phase3_agent_token_stored_with_kind_and_allowed_client_id() {
        use sha2::{Digest, Sha256};
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let plaintext = "wc_agent_secretvalue1234567890abcdef";
        let mut hasher = Sha256::new();
        hasher.update(plaintext.as_bytes());
        let key_hash = format!("{:x}", hasher.finalize());
        let key = ApiKeyRecord {
            id: "k-agent-1".to_string(),
            user_id: "u-1".to_string(),
            name: "laptop agent".to_string(),
            key_prefix: "wc_agent_secret".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register agent:poll agent:result agent:job_update".to_string(),
            expires_at: None,
            kind: "agent".to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&key, &key_hash).unwrap();
        // The stored key_hash must not be the plaintext token.
        let conn = db.conn_for_tests();
        let stored_hash: String = conn
            .query_row(
                "SELECT key_hash FROM api_keys WHERE id = 'k-agent-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_hash, plaintext);
        assert_eq!(stored_hash, key_hash);
        // The stored kind and allowed_client_id must match.
        let (stored_kind, stored_cid): (String, Option<String>) = conn
            .query_row(
                "SELECT kind, allowed_client_id FROM api_keys WHERE id = 'k-agent-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        drop(conn);
        assert_eq!(stored_kind, "agent");
        assert_eq!(stored_cid.as_deref(), Some("alice-laptop"));

        // Lookup succeeds and the record reports agent token.
        let fetched = db.get_api_key_by_hash(&key_hash).unwrap().unwrap();
        assert!(fetched.is_agent_token());
        assert!(!fetched.is_user_token());
        assert_eq!(fetched.kind(), "agent");
        assert_eq!(fetched.allowed_client_id(), Some("alice-laptop"));
        assert_eq!(
            fetched.scopes_vec(),
            vec![
                "agent:register".to_string(),
                "agent:poll".to_string(),
                "agent:result".to_string(),
                "agent:job_update".to_string(),
            ]
        );
    }

    /// Phase 3: revoked/expired/disabled checks apply to agent tokens too.
    #[test]
    fn phase3_agent_token_revoked_expired_disabled_checks_apply() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        let key = ApiKeyRecord {
            id: "k-agent".to_string(),
            user_id: "u-1".to_string(),
            name: "agent".to_string(),
            key_prefix: "wc_agent_pre".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "agent:register".to_string(),
            expires_at: None,
            kind: "agent".to_string(),
            allowed_client_id: Some("alice-laptop".to_string()),
        };
        db.insert_api_key(&key, "hash-agent").unwrap();
        // Revoked agent token is ignored by get_api_key_by_hash.
        db.revoke_api_key("k-agent", now + 10).unwrap();
        assert!(db.get_api_key_by_hash("hash-agent").unwrap().is_none());
        // But get_api_key_by_id returns it with revoked_at set.
        let revoked = db.get_api_key_by_id("k-agent").unwrap().unwrap();
        assert!(revoked.is_revoked());
        assert!(revoked.is_agent_token());

        // Expired agent token: is_expired reports true.
        let exp_key = ApiKeyRecord {
            id: "k-agent-exp".to_string(),
            revoked_at: None,
            expires_at: Some(now - 1),
            ..key.clone()
        };
        db.insert_api_key(&exp_key, "hash-agent-exp").unwrap();
        let fetched = db.get_api_key_by_hash("hash-agent-exp").unwrap().unwrap();
        assert!(fetched.is_expired(now));
        assert!(fetched.is_agent_token());

        // Disabled-user agent token: the auth layer checks user.is_disabled();
        // here we confirm the DB marks the user disabled.
        db.set_user_disabled("u-1", true, now).unwrap();
        let disabled_user = db.get_user_by_id("u-1").unwrap().unwrap();
        assert!(disabled_user.is_disabled());
    }

    /// Phase 3: list_user_tokens (list_api_keys_by_user) returns both user and
    /// agent tokens; list_agent_tokens returns only kind=agent.
    #[test]
    fn phase3_list_agent_tokens_returns_only_kind_agent() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("webcodex.db")).unwrap();
        let now = chrono::Utc::now().timestamp();
        db.create_user(&UserRecord {
            id: "u-1".to_string(),
            username: "alice".to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        })
        .unwrap();
        // One user token, two agent tokens.
        let user_key = ApiKeyRecord {
            id: "k-user".to_string(),
            user_id: "u-1".to_string(),
            name: "user".to_string(),
            key_prefix: "wc_pat_user".to_string(),
            created_at: now,
            last_used_at: None,
            revoked_at: None,
            scopes: "runtime:read".to_string(),
            expires_at: None,
            kind: "user".to_string(),
            allowed_client_id: None,
        };
        db.insert_api_key(&user_key, "hash-user").unwrap();
        let agent_key_1 = ApiKeyRecord {
            id: "k-agent-1".to_string(),
            name: "agent-1".to_string(),
            key_prefix: "wc_agent_a1".to_string(),
            kind: "agent".to_string(),
            allowed_client_id: Some("laptop".to_string()),
            scopes: "agent:register".to_string(),
            ..user_key.clone()
        };
        db.insert_api_key(&agent_key_1, "hash-agent-1").unwrap();
        let agent_key_2 = ApiKeyRecord {
            id: "k-agent-2".to_string(),
            name: "agent-2".to_string(),
            key_prefix: "wc_agent_a2".to_string(),
            kind: "agent".to_string(),
            allowed_client_id: Some("desktop".to_string()),
            scopes: "agent:poll agent:result".to_string(),
            ..user_key.clone()
        };
        db.insert_api_key(&agent_key_2, "hash-agent-2").unwrap();

        // list_api_keys_by_user returns all 3.
        let all = db.list_api_keys_by_user("u-1").unwrap();
        assert_eq!(all.len(), 3);

        // list_agent_api_keys_by_user returns only the 2 agent tokens.
        let agents = db.list_agent_api_keys_by_user("u-1").unwrap();
        assert_eq!(agents.len(), 2);
        assert!(agents.iter().all(|k| k.is_agent_token()));
        assert!(
            agents.iter().all(|k| k.allowed_client_id.is_some()),
            "agent tokens must have allowed_client_id"
        );
    }

    // -----------------------------------------------------------------------
    // Phase 2a: OAuth2 database tests
    // -----------------------------------------------------------------------

    fn oauth_seed_user(db: &Database, username: &str) -> UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = UserRecord {
            id: format!("u-{}", username),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn oauth_seed_client(
        db: &Database,
        user: &UserRecord,
        name: &str,
    ) -> (OAuthClientRecord, String) {
        let now = chrono::Utc::now().timestamp();
        let plaintext_secret = crate::auth::generate_oauth_client_secret();
        let secret_hash = crate::auth::hash_token(&plaintext_secret);
        let record = OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: secret_hash.clone(),
            name: name.to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read".to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        (record, plaintext_secret)
    }

    fn table_column_names(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    }

    fn assert_oauth_subject_columns(conn: &Connection, table: &str) {
        let cols = table_column_names(conn, table);
        for name in ["subject_kind", "subject_id", "shared_key_hash", "user_id"] {
            assert!(
                cols.iter().any(|c| c == name),
                "{table} must declare column {name}"
            );
        }
        // user_id is nullable so shared-key subjects can omit a managed user.
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let user_id_notnull: i64 = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let notnull: i64 = row.get(3)?;
                Ok((name, notnull))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .find(|(name, _)| name == "user_id")
            .map(|(_, notnull)| notnull)
            .expect("user_id column");
        assert_eq!(user_id_notnull, 0, "{table} user_id should allow NULL");
    }

    #[test]
    fn fresh_database_creates_oauth_tables() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let conn = db.conn_for_tests();
        // All four OAuth2 tables must exist.
        for table in [
            "oauth_clients",
            "oauth_authorization_codes",
            "oauth_access_tokens",
            "oauth_refresh_tokens",
        ] {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, 0, "table {} should be empty", table);
        }
        for table in [
            "oauth_authorization_codes",
            "oauth_access_tokens",
            "oauth_refresh_tokens",
        ] {
            assert_oauth_subject_columns(&conn, table);
        }
    }

    #[test]
    fn can_insert_and_get_oauth_client() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _secret) = oauth_seed_client(&db, &user, "Test App");

        let fetched = db
            .get_oauth_client_by_client_id(&client.client_id)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.name, "Test App");
        assert_eq!(fetched.owner_user_id, user.id);
        assert!(!fetched.is_revoked());
        assert_eq!(
            fetched.redirect_uris_vec(),
            vec!["https://example.com/callback"]
        );
    }

    #[test]
    fn verify_oauth_client_secret_works() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, plaintext_secret) = oauth_seed_client(&db, &user, "Test App");

        // Correct secret verifies.
        assert!(db
            .verify_oauth_client_secret(&client.client_id, &plaintext_secret)
            .unwrap());
        // Wrong secret rejects.
        assert!(!db
            .verify_oauth_client_secret(&client.client_id, "wrong-secret")
            .unwrap());
        // Unknown client_id rejects.
        assert!(!db
            .verify_oauth_client_secret("wc_client_nonexistent", &plaintext_secret)
            .unwrap());
    }

    #[test]
    fn revoked_oauth_client_not_returned_by_lookup() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        db.revoke_oauth_client(&client.id, 100).unwrap();
        // get_oauth_client_by_client_id filters revoked clients.
        assert!(db
            .get_oauth_client_by_client_id(&client.client_id)
            .unwrap()
            .is_none());
        // get_oauth_client_by_id still returns it.
        let revoked = db.get_oauth_client_by_id(&client.id).unwrap().unwrap();
        assert!(revoked.is_revoked());
    }

    #[test]
    fn can_insert_and_get_authorization_code_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_string()),
            code_challenge_method: Some("S256".to_string()),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        let fetched = db
            .get_oauth_authorization_code_by_hash(&code_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_used());
        assert!(!fetched.is_expired(now));
        assert!(fetched.is_expired(now + 301));
        assert_eq!(
            fetched.code_challenge.as_deref(),
            Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
        );
        assert_eq!(fetched.code_challenge_method.as_deref(), Some("S256"));
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn can_mark_authorization_code_used() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Mark as used.
        db.mark_oauth_authorization_code_used(&record.id, now + 10)
            .unwrap();
        let fetched = db
            .get_oauth_authorization_code_by_hash(&code_hash)
            .unwrap()
            .unwrap();
        assert!(fetched.is_used());
        assert_eq!(fetched.used_at, Some(now + 10));
    }

    #[test]
    fn can_insert_and_get_access_token_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        let fetched = db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));
        assert!(fetched.is_expired(now + 3601));
        assert!(fetched.last_used_at.is_none());
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn can_update_access_token_last_used() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        db.update_oauth_access_token_last_used(&record.id, now + 60)
            .unwrap();
        let fetched = db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.last_used_at, Some(now + 60));
    }

    #[test]
    fn can_revoke_access_token() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_access_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();

        db.revoke_oauth_access_token(&record.id, now + 100).unwrap();
        // Revoked token is not returned by hash lookup.
        assert!(db
            .get_oauth_access_token_by_hash(&token_hash)
            .unwrap()
            .is_none());
    }

    #[test]
    fn can_insert_and_get_refresh_token_by_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_refresh_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        let fetched = db
            .get_oauth_refresh_token_by_hash(&token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched.client_id, client.client_id);
        assert_eq!(fetched.subject_kind, "managed_user");
        assert_eq!(fetched.subject_id, user.id);
        assert_eq!(fetched.user_id, Some(user.id.clone()));
        assert!(!fetched.is_revoked());
        assert!(!fetched.is_expired(now));
        assert!(fetched.rotated_from_id.is_none());
        assert!(fetched.shared_key_hash.is_none());
    }

    #[test]
    fn oauth_shared_key_subject_records_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();

        let plaintext_ac = crate::auth::generate_oauth_authorization_code();
        let auth_code = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: crate::auth::hash_token(&plaintext_ac),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&auth_code, &auth_code.code_hash)
            .unwrap();
        let fetched_auth_code = db
            .get_oauth_authorization_code_by_hash(&auth_code.code_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_auth_code.subject_kind, "shared_key");
        assert_eq!(fetched_auth_code.subject_id, "hash-a");
        assert_eq!(fetched_auth_code.user_id, None);
        assert_eq!(fetched_auth_code.shared_key_hash.as_deref(), Some("hash-a"));

        let plaintext_at = crate::auth::generate_oauth_access_token();
        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext_at),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&access).unwrap();
        let fetched_access = db
            .get_oauth_access_token_by_hash(&access.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_access.subject_kind, "shared_key");
        assert_eq!(fetched_access.subject_id, "hash-a");
        assert_eq!(fetched_access.user_id, None);
        assert_eq!(fetched_access.shared_key_hash.as_deref(), Some("hash-a"));

        let plaintext_rt = crate::auth::generate_oauth_refresh_token();
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext_rt),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&refresh).unwrap();
        let fetched_refresh = db
            .get_oauth_refresh_token_by_hash(&refresh.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_refresh.subject_kind, "shared_key");
        assert_eq!(fetched_refresh.subject_id, "hash-a");
        assert_eq!(fetched_refresh.user_id, None);
        assert_eq!(fetched_refresh.shared_key_hash.as_deref(), Some("hash-a"));
    }

    #[test]
    fn oauth_subject_validation_rejects_invalid_combinations() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();

        let valid = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "hash-valid".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-shared-with-user".to_string();
        record.subject_kind = "shared_key".to_string();
        record.subject_id = "hash-a".to_string();
        record.user_id = Some(user.id.clone());
        record.shared_key_hash = Some("hash-a".to_string());
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-shared-missing-hash".to_string();
        record.subject_kind = "shared_key".to_string();
        record.subject_id = "hash-a".to_string();
        record.user_id = None;
        record.shared_key_hash = None;
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-managed-missing-user".to_string();
        record.user_id = None;
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid.clone();
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-managed-mismatch".to_string();
        record.subject_id = "other-user".to_string();
        assert!(db.insert_oauth_access_token(&record).is_err());

        let mut record = valid;
        record.id = uuid::Uuid::new_v4().to_string();
        record.token_hash = "hash-unknown-kind".to_string();
        record.subject_kind = "unknown".to_string();
        assert!(db.insert_oauth_access_token(&record).is_err());
    }

    #[test]
    fn can_revoke_refresh_token() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_token = crate::auth::generate_oauth_refresh_token();
        let token_hash = crate::auth::hash_token(&plaintext_token);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: token_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&record).unwrap();

        db.revoke_oauth_refresh_token(&record.id, now + 100)
            .unwrap();
        // Revoked token is not returned by hash lookup.
        assert!(db
            .get_oauth_refresh_token_by_hash(&token_hash)
            .unwrap()
            .is_none());
    }

    #[test]
    fn oauth_plaintext_tokens_are_never_stored() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");

        // Client secret: only hash stored.
        let (client, plaintext_secret) = oauth_seed_client(&db, &user, "Test App");
        let conn = db.conn_for_tests();
        let stored_secret_hash: String = conn
            .query_row(
                "SELECT client_secret_hash FROM oauth_clients WHERE id = ?1",
                rusqlite::params![client.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_secret_hash, plaintext_secret);
        assert_eq!(
            stored_secret_hash,
            crate::auth::hash_token(&plaintext_secret)
        );
        drop(conn);

        // Access token: only hash stored.
        let plaintext_at = crate::auth::generate_oauth_access_token();
        let at_hash = crate::auth::hash_token(&plaintext_at);
        let now = chrono::Utc::now().timestamp();
        let at_record = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: at_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&at_record).unwrap();
        let conn = db.conn_for_tests();
        let stored_at_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_access_tokens WHERE id = ?1",
                rusqlite::params![at_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_at_hash, plaintext_at);
        assert_eq!(stored_at_hash, at_hash);
        drop(conn);

        // Refresh token: only hash stored.
        let plaintext_rt = crate::auth::generate_oauth_refresh_token();
        let rt_hash = crate::auth::hash_token(&plaintext_rt);
        let rt_record = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: rt_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&rt_record).unwrap();
        let conn = db.conn_for_tests();
        let stored_rt_hash: String = conn
            .query_row(
                "SELECT token_hash FROM oauth_refresh_tokens WHERE id = ?1",
                rusqlite::params![rt_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_rt_hash, plaintext_rt);
        assert_eq!(stored_rt_hash, rt_hash);
        drop(conn);

        // Authorization code: only hash stored.
        let plaintext_ac = crate::auth::generate_oauth_authorization_code();
        let ac_hash = crate::auth::hash_token(&plaintext_ac);
        let ac_record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: ac_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&ac_record, &ac_hash)
            .unwrap();
        let conn = db.conn_for_tests();
        let stored_ac_hash: String = conn
            .query_row(
                "SELECT code_hash FROM oauth_authorization_codes WHERE id = ?1",
                rusqlite::params![ac_record.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(stored_ac_hash, plaintext_ac);
        assert_eq!(stored_ac_hash, ac_hash);
    }

    // -----------------------------------------------------------------------
    // consume_oauth_authorization_code_by_hash tests
    // -----------------------------------------------------------------------

    #[test]
    fn consume_authorization_code_succeeds_once() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // First consume succeeds.
        let consumed = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap();
        let consumed = consumed.expect("first consume should succeed");
        assert_eq!(consumed.used_at, Some(now + 10));
        assert_eq!(consumed.id, record.id);
    }

    #[test]
    fn consume_authorization_code_second_consume_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // First consume succeeds.
        db.consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap()
            .expect("first consume should succeed");

        // Second consume returns None.
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 20)
            .unwrap();
        assert!(result.is_none(), "second consume should return None");
    }

    #[test]
    fn consume_authorization_code_expired_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Consume after expiration returns None.
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 301)
            .unwrap();
        assert!(result.is_none(), "expired code should return None");
    }

    #[test]
    fn consume_authorization_code_revoked_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");

        let plaintext_code = crate::auth::generate_oauth_authorization_code();
        let code_hash = crate::auth::hash_token(&plaintext_code);
        let now = chrono::Utc::now().timestamp();
        let record = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&record, &code_hash)
            .unwrap();

        // Revoke, then consume returns None.
        db.revoke_oauth_authorization_code(&record.id, now + 5)
            .unwrap();
        let result = db
            .consume_oauth_authorization_code_by_hash(&code_hash, now + 10)
            .unwrap();
        assert!(result.is_none(), "revoked code should return None");
    }

    #[test]
    fn consume_authorization_code_unknown_hash_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let now = chrono::Utc::now().timestamp();

        let result = db
            .consume_oauth_authorization_code_by_hash("nonexistent-hash", now)
            .unwrap();
        assert!(result.is_none(), "unknown hash should return None");
    }

    #[test]
    fn exchange_authorization_code_rejects_subject_mismatch_with_consumed_code() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        let code_hash = "code-hash-subject-mismatch".to_string();
        let code = OAuthAuthorizationCodeRecord {
            id: uuid::Uuid::new_v4().to_string(),
            code_hash: code_hash.clone(),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            redirect_uri: "https://example.com/callback".to_string(),
            scopes: "runtime:read".to_string(),
            code_challenge: None,
            code_challenge_method: None,
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 300,
            used_at: None,
            revoked_at: None,
        };
        db.insert_oauth_authorization_code(&code, &code_hash)
            .unwrap();

        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "access-hash-mismatch".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "refresh-hash-mismatch".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };

        let err = db
            .exchange_oauth_authorization_code_for_tokens(&code_hash, now + 10, &access, &refresh)
            .expect_err("subject mismatch must abort exchange");
        assert!(err.to_string().contains("OAuth token subjects must match"));
        let conn = db.conn_for_tests();
        let used_at: Option<i64> = conn
            .query_row(
                "SELECT used_at FROM oauth_authorization_codes WHERE id = ?1",
                [&code.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(used_at, None);
        let access_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE id = ?1",
                [&access.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(access_count, 0);
    }

    #[test]
    fn rotate_refresh_token_rejects_subject_mismatch_with_old_refresh() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("oauth.db")).unwrap();
        let user = oauth_seed_user(&db, "alice");
        let (client, _) = oauth_seed_client(&db, &user, "Test App");
        let now = chrono::Utc::now().timestamp();
        let old = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "old-refresh-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "shared_key".to_string(),
            subject_id: "hash-a".to_string(),
            user_id: None,
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: Some("hash-a".to_string()),
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: None,
        };
        db.insert_oauth_refresh_token(&old).unwrap();

        let access = OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "new-access-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        let refresh = OAuthRefreshTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: "new-refresh-hash".to_string(),
            client_id: client.client_id.clone(),
            subject_kind: "managed_user".to_string(),
            subject_id: user.id.clone(),
            user_id: Some(user.id.clone()),
            scopes: "runtime:read".to_string(),
            resource: None,
            shared_key_hash: None,
            created_at: now,
            expires_at: now + 2_592_000,
            revoked_at: None,
            last_used_at: None,
            rotated_from_id: Some(old.id.clone()),
        };

        let err = db
            .rotate_oauth_refresh_token(
                &old.token_hash,
                &client.client_id,
                now + 10,
                &access,
                &refresh,
            )
            .expect_err("subject mismatch must abort rotation");
        assert!(err.to_string().contains("OAuth token subjects must match"));
        let fetched_old = db
            .get_oauth_refresh_token_by_hash(&old.token_hash)
            .unwrap()
            .unwrap();
        assert_eq!(fetched_old.revoked_at, None);
        assert_eq!(fetched_old.last_used_at, None);
        let conn = db.conn_for_tests();
        let access_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM oauth_access_tokens WHERE id = ?1",
                [&access.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(access_count, 0);
    }
}
