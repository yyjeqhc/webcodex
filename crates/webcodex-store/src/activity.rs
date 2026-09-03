//! Workspace activity ledger: one bounded row per mutating tool execution,
//! shared by the browser console and the offline CLI. Row count is capped at
//! insert time (oldest rows pruned) so long-running low-disk deployments never
//! grow without bound.

use super::Database;
use rusqlite::params;
use serde::Serialize;
use webcodex_core::activity_contract::{ActivityRecord, ActivityVisibility};

/// Persisted error summaries stay one short line.
const ERROR_SUMMARY_MAX_CHARS: usize = 200;

#[derive(Debug, Serialize)]
pub struct WorkspaceActivityRow {
    pub id: i64,
    pub created_at: i64,
    pub project: Option<String>,
    pub tool: String,
    pub surface: String,
    pub client: Option<String>,
    pub success: bool,
    pub session_id: Option<String>,
    pub command_preview: Option<String>,
    pub paths: Vec<String>,
    pub error_summary: Option<String>,
}

fn row_to_activity(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkspaceActivityRow> {
    let paths_json: String = row.get(9)?;
    Ok(WorkspaceActivityRow {
        id: row.get(0)?,
        created_at: row.get(1)?,
        project: row.get(2)?,
        tool: row.get(3)?,
        surface: row.get(4)?,
        client: row.get(5)?,
        success: row.get(6)?,
        session_id: row.get(7)?,
        command_preview: row.get(8)?,
        paths: serde_json::from_str(&paths_json).unwrap_or_default(),
        error_summary: row.get(10)?,
    })
}

impl Database {
    pub fn insert_workspace_activity(
        &self,
        created_at: i64,
        record: &ActivityRecord<'_>,
        command_preview: Option<&str>,
        max_rows: i64,
    ) -> anyhow::Result<()> {
        let paths_json = serde_json::to_string(&record.paths)?;
        let error_summary = record
            .error_summary
            .map(|error| truncate_chars(error, ERROR_SUMMARY_MAX_CHARS));
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO workspace_activity (
                created_at, project, tool, surface, client, success, session_id,
                command_preview, paths_json, error_summary, scope_kind, scope_id
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                created_at,
                record.project,
                record.tool,
                record.surface,
                record.client,
                record.success,
                record.session_id,
                command_preview,
                paths_json,
                error_summary,
                record.scope.kind(),
                record.scope.id(),
            ],
        )?;
        // Keep the ledger bounded: prune oldest rows beyond the cap in the
        // same connection so the table can never outgrow the operator budget.
        conn.execute(
            "DELETE FROM workspace_activity WHERE id NOT IN (
                SELECT id FROM workspace_activity ORDER BY id DESC LIMIT ?1
            )",
            params![max_rows.max(1)],
        )?;
        Ok(())
    }

    /// Activity rows the caller is allowed to see.
    ///
    /// `allowed_clients` is `None` only for principals entitled to host-global
    /// activity (bootstrap/admin). Otherwise it is the exact set of agent
    /// clients visible to this `AuthContext`, and rows outside it — including
    /// rows with no client at all — stay hidden: a row carries a command
    /// preview, the paths a call touched, and its error, so leaking one leaks
    /// another project's work.
    ///
    /// `requested_client` only narrows further. It can never widen the set,
    /// so a caller cannot name someone else's client to read their rows.
    ///
    /// The limit applies after filtering, in SQL, so a busy neighbouring
    /// project cannot push this caller's rows out of the window.
    /// Activity rows the caller is allowed to see.
    ///
    /// Attribution comes from the row's own `scope_kind`/`scope_id`, fixed when
    /// it was written. A client id is not an authorization boundary: the
    /// registry that owns it is in-memory, so after a restart a different grant
    /// can register the same id, and filtering on it alone would hand that
    /// grant the previous one's history.
    ///
    /// A project grant therefore sees only rows carrying its own grant, and
    /// within those only its currently visible clients. Rows with no client —
    /// and every `host_global`, `unscoped`, or `legacy_unscoped` row — stay
    /// invisible to it, because nothing proves they belong to this project.
    ///
    /// `requested_client` only narrows. The limit applies after every filter,
    /// in SQL, so a busy neighbour cannot crowd a caller's rows out.
    pub fn list_workspace_activity_for_clients(
        &self,
        limit: usize,
        requested_client: Option<&str>,
        visibility: ActivityVisibility<'_>,
        allowed_clients: &[String],
    ) -> anyhow::Result<Vec<WorkspaceActivityRow>> {
        let clients: Option<Vec<String>> = match (visibility, requested_client) {
            (ActivityVisibility::Global, Some(requested)) => Some(vec![requested.to_string()]),
            (ActivityVisibility::Global, None) => None,
            (ActivityVisibility::ProjectGrant(_), Some(requested)) => {
                if allowed_clients.iter().any(|client| client == requested) {
                    Some(vec![requested.to_string()])
                } else {
                    // Naming a client this caller cannot see is indistinguishable
                    // from naming an idle one: both are empty, never someone
                    // else's rows.
                    Some(Vec::new())
                }
            }
            (ActivityVisibility::ProjectGrant(_), None) => Some(allowed_clients.to_vec()),
        };
        if clients.as_ref().is_some_and(Vec::is_empty) {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        let select = "SELECT id, created_at, project, tool, surface, client, success, session_id,
                    command_preview, paths_json, error_summary
             FROM workspace_activity";

        // `?1` is always the limit; scope and client values follow, all bound.
        let mut bound: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(limit as i64)];
        let mut wheres: Vec<String> = Vec::new();
        if let ActivityVisibility::ProjectGrant(grant) = visibility {
            bound.push(Box::new(grant.to_string()));
            wheres.push(format!(
                "scope_kind = 'project_grant' AND scope_id = ?{}",
                bound.len()
            ));
        }
        if let Some(clients) = clients.as_ref() {
            let placeholders = clients
                .iter()
                .map(|client| {
                    bound.push(Box::new(client.clone()));
                    format!("?{}", bound.len())
                })
                .collect::<Vec<_>>()
                .join(", ");
            wheres.push(format!("client IN ({placeholders})"));
        }
        let filter = if wheres.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", wheres.join(" AND "))
        };

        let mut statement = conn.prepare(&format!("{select}{filter} ORDER BY id DESC LIMIT ?1"))?;
        let params: Vec<&dyn rusqlite::ToSql> = bound.iter().map(|value| value.as_ref()).collect();
        let rows = statement.query_map(params.as_slice(), row_to_activity)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use webcodex_core::activity_contract::ActivityScope;

    fn sample<'a>(tool: &'a str, success: bool, error: Option<&'a str>) -> ActivityRecord<'a> {
        ActivityRecord {
            tool,
            project: Some("demo"),
            surface: "mcp",
            client: Some("laptop"),
            success,
            session_id: None,
            command: None,
            paths: vec!["a.rs".to_string()],
            error_summary: error,
            scope: ActivityScope::ProjectGrant(GRANT_A.to_string()),
        }
    }

    const GRANT_A: &str = "wc_pgrant_aaaaaaaaaaaaaaaa";
    const GRANT_B: &str = "wc_pgrant_bbbbbbbbbbbbbbbb";

    /// Record attributed to `grant` and `client`.
    fn scoped<'a>(client: Option<&'a str>, grant: &str) -> ActivityRecord<'a> {
        ActivityRecord {
            client,
            scope: ActivityScope::ProjectGrant(grant.to_string()),
            ..sample("run_shell", true, Some("boom"))
        }
    }

    fn as_grant(db: &Database, grant: &str, clients: &[&str]) -> Vec<WorkspaceActivityRow> {
        let clients: Vec<String> = clients.iter().map(|c| c.to_string()).collect();
        db.list_workspace_activity_for_clients(
            50,
            None,
            ActivityVisibility::ProjectGrant(grant),
            &clients,
        )
        .unwrap()
    }

    fn as_global(db: &Database) -> Vec<WorkspaceActivityRow> {
        db.list_workspace_activity_for_clients(50, None, ActivityVisibility::Global, &[])
            .unwrap()
    }

    #[test]
    fn activity_roundtrip_orders_newest_first_and_prunes() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        for index in 0..5 {
            db.insert_workspace_activity(
                1_000 + index,
                &sample("run_shell", index % 2 == 0, None),
                Some("cargo test"),
                3,
            )
            .unwrap();
        }
        let rows = db
            .list_workspace_activity_for_clients(10, None, ActivityVisibility::Global, &[])
            .unwrap();
        assert_eq!(rows.len(), 3, "prune keeps only max_rows newest rows");
        assert!(rows[0].id > rows[1].id && rows[1].id > rows[2].id);
        assert_eq!(rows[0].tool, "run_shell");
        assert_eq!(rows[0].surface, "mcp");
        assert_eq!(rows[0].paths, vec!["a.rs".to_string()]);
        assert_eq!(rows[0].command_preview.as_deref(), Some("cargo test"));
        assert_eq!(
            db.list_workspace_activity_for_clients(2, None, ActivityVisibility::Global, &[])
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn activity_filter_by_client_matches_exactly() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        let mut other = sample("run_shell", true, None);
        other.client = Some("desktop");
        db.insert_workspace_activity(1, &sample("run_shell", true, None), None, 10)
            .unwrap();
        db.insert_workspace_activity(2, &other, None, 10).unwrap();
        assert_eq!(
            db.list_workspace_activity_for_clients(10, None, ActivityVisibility::Global, &[])
                .unwrap()
                .len(),
            2
        );
        let filtered = db
            .list_workspace_activity_for_clients(
                10,
                Some("laptop"),
                ActivityVisibility::Global,
                &[],
            )
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].client.as_deref(), Some("laptop"));
        assert!(db
            .list_workspace_activity_for_clients(
                10,
                Some("nobody"),
                ActivityVisibility::Global,
                &[]
            )
            .unwrap()
            .is_empty());
    }

    #[test]
    fn activity_insert_truncates_long_error_summaries() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        let long_error = "e".repeat(500);
        db.insert_workspace_activity(
            1,
            &sample("write_project_file", false, Some(&long_error)),
            None,
            10,
        )
        .unwrap();
        let rows = db
            .list_workspace_activity_for_clients(1, None, ActivityVisibility::Global, &[])
            .unwrap();
        let stored = rows[0].error_summary.as_deref().unwrap();
        assert!(stored.chars().count() <= ERROR_SUMMARY_MAX_CHARS + 1);
        assert!(stored.ends_with('…'));
        assert!(rows[0].command_preview.is_none());
    }

    #[test]
    fn activity_history_is_bound_to_original_project_grant() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("activity.db");
        {
            let db = Database::open(&path).unwrap();
            db.insert_workspace_activity(
                1,
                &ActivityRecord {
                    client: Some("laptop"),
                    paths: vec!["private/a.rs".to_string()],
                    error_summary: Some("grant-a-error"),
                    scope: ActivityScope::ProjectGrant(GRANT_A.to_string()),
                    ..sample("run_shell", true, None)
                },
                Some("grant-a-command"),
                50,
            )
            .unwrap();
        }

        // Reopen: the in-memory registry is gone, which is exactly the moment a
        // different grant can claim the same client id.
        let db = Database::open(&path).unwrap();

        // Grant B sees `laptop` among its live clients, but the row was not
        // written by it.
        let leaked = as_grant(&db, GRANT_B, &["laptop"]);
        assert!(leaked.is_empty(), "{leaked:?}");
        let serialized = serde_json::to_string(&leaked).unwrap();
        for secret in ["grant-a-command", "private/a.rs", "grant-a-error"] {
            assert!(
                !serialized.contains(secret),
                "{secret} leaked: {serialized}"
            );
        }

        // The grant that wrote it still sees it, and so does a global reader.
        assert_eq!(as_grant(&db, GRANT_A, &["laptop"]).len(), 1);
        assert_eq!(as_global(&db).len(), 1);
    }

    #[test]
    fn reused_client_id_does_not_reassign_activity_history() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        db.insert_workspace_activity(1, &scoped(Some("laptop"), GRANT_A), Some("a"), 50)
            .unwrap();
        db.insert_workspace_activity(2, &scoped(Some("laptop"), GRANT_B), Some("b"), 50)
            .unwrap();

        // One client id, two grants: each sees only what it wrote.
        let a = as_grant(&db, GRANT_A, &["laptop"]);
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].command_preview.as_deref(), Some("a"));
        let b = as_grant(&db, GRANT_B, &["laptop"]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].command_preview.as_deref(), Some("b"));
        assert_eq!(as_global(&db).len(), 2);
    }

    #[test]
    fn legacy_activity_rows_remain_visible_to_bootstrap() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO workspace_activity
                    (created_at, tool, surface, client, success, paths_json, scope_kind)
                 VALUES (1, 'run_shell', 'mcp', 'laptop', 1, '[]', 'legacy_unscoped')",
                [],
            )
            .unwrap();
        // Not deleted, not re-attributed: still readable by the host operator.
        assert_eq!(as_global(&db).len(), 1);
    }

    #[test]
    fn activity_client_filter_only_narrows_within_the_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        db.insert_workspace_activity(1, &scoped(Some("laptop"), GRANT_A), None, 50)
            .unwrap();
        db.insert_workspace_activity(2, &scoped(Some("desktop"), GRANT_A), None, 50)
            .unwrap();
        db.insert_workspace_activity(3, &scoped(Some("laptop"), GRANT_B), None, 50)
            .unwrap();

        // Narrowing inside the grant works.
        let narrowed = db
            .list_workspace_activity_for_clients(
                50,
                Some("laptop"),
                ActivityVisibility::ProjectGrant(GRANT_A),
                &["laptop".to_string(), "desktop".to_string()],
            )
            .unwrap();
        assert_eq!(narrowed.len(), 1);
        assert_eq!(narrowed[0].client.as_deref(), Some("laptop"));

        // Naming a client outside the visible set returns nothing…
        assert!(db
            .list_workspace_activity_for_clients(
                50,
                Some("desktop"),
                ActivityVisibility::ProjectGrant(GRANT_B),
                &["laptop".to_string()],
            )
            .unwrap()
            .is_empty());
        // …and even a visible client id cannot reach another grant's row.
        let b = as_grant(&db, GRANT_B, &["laptop"]);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn empty_client_allow_set_is_closed_even_within_the_grant() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        db.insert_workspace_activity(1, &scoped(Some("laptop"), GRANT_A), None, 50)
            .unwrap();
        // No client attached: cannot be shown to a project credential either.
        db.insert_workspace_activity(2, &scoped(None, GRANT_A), None, 50)
            .unwrap();

        assert!(as_grant(&db, GRANT_A, &[]).is_empty());
        // With a visible client, only the client-bearing row comes back.
        let rows = as_grant(&db, GRANT_A, &["laptop"]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].client.as_deref(), Some("laptop"));
        // Global sees both.
        assert_eq!(as_global(&db).len(), 2);
    }

    #[test]
    fn host_global_and_unscoped_rows_are_never_shown_to_a_grant() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        for scope in [ActivityScope::HostGlobal, ActivityScope::Unscoped] {
            db.insert_workspace_activity(
                1,
                &ActivityRecord {
                    client: Some("laptop"),
                    scope,
                    ..sample("run_shell", true, None)
                },
                Some("admin-only"),
                50,
            )
            .unwrap();
        }
        assert!(as_grant(&db, GRANT_A, &["laptop"]).is_empty());
        assert_eq!(as_global(&db).len(), 2);
    }

    #[test]
    fn activity_limit_applies_after_scope_and_client_filtering() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        // Another grant floods the table using the same client id.
        for index in 0..60 {
            db.insert_workspace_activity(index, &scoped(Some("laptop"), GRANT_B), None, 500)
                .unwrap();
        }
        for index in 0..3 {
            db.insert_workspace_activity(100 + index, &scoped(Some("laptop"), GRANT_A), None, 500)
                .unwrap();
        }
        let rows = db
            .list_workspace_activity_for_clients(
                50,
                None,
                ActivityVisibility::ProjectGrant(GRANT_A),
                &["laptop".to_string()],
            )
            .unwrap();
        assert_eq!(rows.len(), 3, "scoped rows were crowded out by the limit");
    }

    #[test]
    fn activity_scope_values_are_parameterized() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Database::open(&tmp.path().join("activity.db")).unwrap();
        db.insert_workspace_activity(1, &scoped(Some("laptop"), GRANT_A), None, 50)
            .unwrap();
        // Quote-bearing input is bound, not interpolated: it matches nothing
        // and does not disturb the query.
        let rows = db
            .list_workspace_activity_for_clients(
                50,
                None,
                ActivityVisibility::ProjectGrant("' OR 1=1 --"),
                &["laptop' OR '1'='1".to_string()],
            )
            .unwrap();
        assert!(rows.is_empty(), "{rows:?}");
        assert_eq!(as_grant(&db, GRANT_A, &["laptop"]).len(), 1);
    }

    #[test]
    fn activity_rows_never_store_credentials_as_scope_identity() {
        // Only a project grant carries an id, and a grant id is not secret.
        // Shared-key and anonymous callers become `unscoped` precisely so a
        // key hash never reaches the table.
        assert_eq!(
            ActivityScope::ProjectGrant(GRANT_A.to_string()).id(),
            Some(GRANT_A)
        );
        assert_eq!(ActivityScope::HostGlobal.id(), None);
        assert_eq!(ActivityScope::Unscoped.id(), None);
        // Each security meaning has its own kind; none is overloaded.
        let kinds = [
            ActivityScope::ProjectGrant(GRANT_A.to_string()).kind(),
            ActivityScope::HostGlobal.kind(),
            ActivityScope::Unscoped.kind(),
        ];
        let unique: std::collections::HashSet<_> = kinds.iter().collect();
        assert_eq!(unique.len(), kinds.len());
    }
}
