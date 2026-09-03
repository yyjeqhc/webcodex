use super::Database;
use rusqlite::params;

const MAX_ROWS: i64 = 2_000;
const RETENTION_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct AdminProjectIdempotencyRecord {
    pub request_hash: String,
    pub http_status: i64,
    pub response_json: String,
}

pub struct AdminProjectAudit<'a> {
    pub correlation_id: &'a str,
    pub subject_type: &'a str,
    pub subject_id: &'a str,
    pub operation: &'a str,
    pub project: &'a str,
    pub client_id: Option<&'a str>,
    pub outcome: &'a str,
    pub changed: bool,
    pub reason_code: Option<&'a str>,
    pub idempotency_digest: &'a str,
}

impl Database {
    pub fn insert_admin_project_lifecycle_audit(
        &self,
        audit: &AdminProjectAudit<'_>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO admin_project_lifecycle_audit
             (created_at, correlation_id, subject_type, subject_id, operation, project,
              client_id, outcome, changed, reason_code, idempotency_digest)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                now,
                audit.correlation_id,
                audit.subject_type,
                audit.subject_id,
                audit.operation,
                audit.project,
                audit.client_id,
                audit.outcome,
                audit.changed,
                audit.reason_code,
                audit.idempotency_digest
            ],
        )?;
        conn.execute(
            "DELETE FROM admin_project_lifecycle_audit WHERE id NOT IN (
                SELECT id FROM admin_project_lifecycle_audit ORDER BY id DESC LIMIT ?1
             )",
            params![MAX_ROWS],
        )?;
        Ok(())
    }

    pub fn get_admin_project_idempotency(
        &self,
        subject: &str,
        action: &str,
        target: &str,
        key_hash: &str,
    ) -> anyhow::Result<Option<AdminProjectIdempotencyRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut statement = conn.prepare(
            "SELECT request_hash, http_status, response_json
             FROM admin_project_idempotency
             WHERE subject = ?1 AND action = ?2 AND target = ?3 AND key_hash = ?4",
        )?;
        let mut rows = statement.query(params![subject, action, target, key_hash])?;
        Ok(rows.next()?.map(|row| AdminProjectIdempotencyRecord {
            request_hash: row.get(0).unwrap_or_default(),
            http_status: row.get(1).unwrap_or(500),
            response_json: row.get(2).unwrap_or_else(|_| "{}".to_string()),
        }))
    }

    pub fn delete_admin_project_idempotency(
        &self,
        subject: &str,
        action: &str,
        target: &str,
        key_hash: &str,
    ) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM admin_project_idempotency
             WHERE subject = ?1 AND action = ?2 AND target = ?3 AND key_hash = ?4",
            params![subject, action, target, key_hash],
        )?;
        Ok(())
    }

    pub fn insert_admin_project_idempotency(
        &self,
        subject: &str,
        action: &str,
        target: &str,
        key_hash: &str,
        request_hash: &str,
        http_status: i64,
        response_json: &str,
    ) -> anyhow::Result<bool> {
        let now = chrono::Utc::now().timestamp();
        let mut conn = self.conn.lock().unwrap();
        let transaction = conn.transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO admin_project_idempotency
             (subject, action, target, key_hash, request_hash, http_status, response_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![subject, action, target, key_hash, request_hash, http_status, response_json, now],
        )? == 1;
        transaction.execute(
            "DELETE FROM admin_project_idempotency WHERE created_at < ?1",
            params![now - RETENTION_SECS],
        )?;
        transaction.execute(
            "DELETE FROM admin_project_idempotency WHERE rowid NOT IN (
                SELECT rowid FROM admin_project_idempotency ORDER BY created_at DESC, rowid DESC LIMIT ?1
             )",
            params![MAX_ROWS],
        )?;
        transaction.commit()?;
        Ok(inserted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_project_idempotency_is_durable_and_payload_bound() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.db");
        let db = Database::open(&path).unwrap();
        assert!(db
            .insert_admin_project_idempotency(
                "admin-1",
                "disable",
                "agent:oe:demo",
                "sha256:key",
                "sha256:request-a",
                200,
                "{\"outcome\":\"disabled\"}"
            )
            .unwrap());
        assert!(!db
            .insert_admin_project_idempotency(
                "admin-1",
                "disable",
                "agent:oe:demo",
                "sha256:key",
                "sha256:request-b",
                409,
                "{\"error\":{\"code\":\"conflict\"}}"
            )
            .unwrap());
        drop(db);

        let reopened = Database::open(&path).unwrap();
        let stored = reopened
            .get_admin_project_idempotency("admin-1", "disable", "agent:oe:demo", "sha256:key")
            .unwrap()
            .unwrap();
        assert_eq!(stored.request_hash, "sha256:request-a");
        assert_eq!(stored.http_status, 200);
        assert!(!stored.response_json.contains("token"));
    }

    #[test]
    fn admin_project_audit_is_bounded_and_contains_only_safe_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("state.db")).unwrap();
        db.insert_admin_project_lifecycle_audit(&AdminProjectAudit {
            correlation_id: "corr-1",
            subject_type: "admin_pat",
            subject_id: "key-id-1",
            operation: "unregister",
            project: "agent:oe:demo",
            client_id: Some("oe"),
            outcome: "active_jobs_conflict",
            changed: false,
            reason_code: Some("active_jobs_conflict"),
            idempotency_digest: "sha256:digest",
        })
        .unwrap();
        let conn = db.conn_for_tests();
        let row: String = conn
            .query_row(
                "SELECT subject_type || ':' || subject_id || ':' || operation || ':' || outcome || ':' || idempotency_digest FROM admin_project_lifecycle_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            row,
            "admin_pat:key-id-1:unregister:active_jobs_conflict:sha256:digest"
        );
        assert!(!row.contains("Bearer "));
    }
    #[test]
    fn transient_idempotency_rows_can_be_removed_for_retry() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("state.db")).unwrap();
        db.insert_admin_project_idempotency(
            "admin-1",
            "disable",
            "agent:oe:demo",
            "sha256:key",
            "sha256:request",
            503,
            "{\"error\":{\"code\":\"agent_unavailable\"}}",
        )
        .unwrap();
        db.delete_admin_project_idempotency("admin-1", "disable", "agent:oe:demo", "sha256:key")
            .unwrap();
        assert!(db
            .get_admin_project_idempotency("admin-1", "disable", "agent:oe:demo", "sha256:key")
            .unwrap()
            .is_none());
    }
}
