use super::Database;
use rusqlite::params;

const MAX_ROWS: i64 = 2_000;

pub struct AdminRunnerAudit<'a> {
    pub correlation_id: &'a str,
    pub subject_type: &'a str,
    pub subject_id: &'a str,
    pub operation: &'a str,
    pub runner: &'a str,
    pub outcome: &'a str,
    pub changed: bool,
    pub projects_removed: usize,
    pub reason_code: Option<&'a str>,
}

impl Database {
    pub fn insert_admin_runner_lifecycle_audit(
        &self,
        audit: &AdminRunnerAudit<'_>,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp();
        let conn = self.lock_connection(crate::StoreDomain::AdminRunnerLifecycle);
        conn.execute(
            "INSERT INTO admin_runner_lifecycle_audit
             (created_at, correlation_id, subject_type, subject_id, operation, runner,
              outcome, changed, projects_removed, reason_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                now,
                audit.correlation_id,
                audit.subject_type,
                audit.subject_id,
                audit.operation,
                audit.runner,
                audit.outcome,
                audit.changed,
                audit.projects_removed as i64,
                audit.reason_code
            ],
        )?;
        conn.execute(
            "DELETE FROM admin_runner_lifecycle_audit WHERE id NOT IN (
                SELECT id FROM admin_runner_lifecycle_audit ORDER BY id DESC LIMIT ?1
             )",
            params![MAX_ROWS],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_runner_audit_is_bounded_and_contains_only_safe_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("state.db")).unwrap();
        db.insert_admin_runner_lifecycle_audit(&AdminRunnerAudit {
            correlation_id: "corr-1",
            subject_type: "bootstrap",
            subject_id: "bootstrap",
            operation: "unregister",
            runner: "stale-runner",
            outcome: "unregistered",
            changed: true,
            projects_removed: 2,
            reason_code: None,
        })
        .unwrap();
        let conn = db.conn_for_tests();
        let row: String = conn
            .query_row(
                "SELECT subject_type || ':' || operation || ':' || runner || ':' || outcome || ':' || projects_removed FROM admin_runner_lifecycle_audit",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(row, "bootstrap:unregister:stale-runner:unregistered:2");
        assert!(!row.contains("Bearer "));
    }

    #[test]
    fn admin_runner_audit_keeps_only_the_newest_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("state.db")).unwrap();
        for index in 0..(MAX_ROWS + 5) {
            db.insert_admin_runner_lifecycle_audit(&AdminRunnerAudit {
                correlation_id: "corr",
                subject_type: "bootstrap",
                subject_id: "bootstrap",
                operation: "unregister",
                runner: "stale-runner",
                outcome: "unregistered",
                changed: true,
                projects_removed: index as usize,
                reason_code: None,
            })
            .unwrap();
        }
        let conn = db.conn_for_tests();
        let rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM admin_runner_lifecycle_audit", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(rows, MAX_ROWS);
    }
}
