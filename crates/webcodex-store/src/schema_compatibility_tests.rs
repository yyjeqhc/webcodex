use super::Database;
use rusqlite::Connection;

#[test]
fn absent_optional_domain_initializes_and_current_domains_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("supported-domains.db");
    let db = Database::open(&path).unwrap();
    db.conn_for_tests()
        .execute("DROP TABLE wc_agent_attention_events", [])
        .unwrap();
    drop(db);
    drop(Database::open(&path).unwrap());
}

#[test]
fn pre_v046_domain_formats_are_rejected_without_writing() {
    for (table, retained_columns) in [
        ("wc_agent_waits", "goal_id TEXT"),
        ("wc_agent_waits", "mode TEXT"),
        ("wc_agent_endpoints", "mcp_app_recovery_fingerprint TEXT"),
        ("wc_agent_endpoints", "mcp_app_client_window_key TEXT"),
        ("wc_goals", "controller_agent_id TEXT"),
        ("wc_goals", "plan_json TEXT"),
        ("wc_agent_attention_events", "event_id TEXT"),
        ("wc_artifact_handoff_acceptances", "acceptance_id TEXT"),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("pre-v046.db");
        {
            let conn = Connection::open(&path).unwrap();
            // Only the retired migration's discriminator columns matter here.
            // A retained row also makes unintended rebuild/backfill writes visible.
            conn.execute_batch(&format!(
                "CREATE TABLE {table} (retained_value TEXT NOT NULL, {retained_columns});
                 INSERT INTO {table} (retained_value) VALUES ('keep');"
            ))
            .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        let error = Database::open(&path)
            .err()
            .expect("pre-v0.4.6 domain must be rejected")
            .to_string();
        assert!(
            error.contains(&format!("unsupported pre-v0.4.6 {table} schema")),
            "{table} ({retained_columns}): {error}"
        );
        assert!(error.contains("upgrade through v0.4.6 first"));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "rejected {table} ({retained_columns}) must remain byte-for-byte intact"
        );
        assert!(!temp.path().join("pre-v046.db-wal").exists());
    }
}
