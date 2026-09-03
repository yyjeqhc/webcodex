//! Root host adapter from ToolRuntime activity recording to the durable store.

use crate::tool_runtime::activity::ActivityRecorder;
use crate::Database;
use std::sync::Arc;
use webcodex_core::activity_contract::ActivityRecord;

const COMMAND_PREVIEW_MAX_CHARS: usize = 120;
const DEFAULT_MAX_ROWS: i64 = 2_000;

/// SQLite-backed [`ActivityRecorder`] wired into the server's `ToolRuntime`.
/// Env knobs (self-hosted operators own the privacy tradeoff):
/// - `WEBCODEX_ACTIVITY=0` disables recording entirely.
/// - `WEBCODEX_ACTIVITY_COMMAND_PREVIEW=0` drops command previews.
/// - `WEBCODEX_ACTIVITY_MAX_ROWS` bounds the ledger (default 2000).
pub(crate) struct WorkspaceActivityStore {
    db: Arc<Database>,
    preview_enabled: bool,
    max_rows: i64,
}

impl WorkspaceActivityStore {
    #[cfg(test)]
    fn with_preview(db: Arc<Database>, preview_enabled: bool) -> Self {
        Self {
            db,
            preview_enabled,
            max_rows: DEFAULT_MAX_ROWS,
        }
    }

    pub(crate) fn from_env(db: Arc<Database>) -> Option<Self> {
        if env_flag_disabled("WEBCODEX_ACTIVITY") {
            return None;
        }
        let max_rows = std::env::var("WEBCODEX_ACTIVITY_MAX_ROWS")
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_ROWS)
            .clamp(100, 100_000);
        Some(Self {
            db,
            preview_enabled: !env_flag_disabled("WEBCODEX_ACTIVITY_COMMAND_PREVIEW"),
            max_rows,
        })
    }
}

impl ActivityRecorder for WorkspaceActivityStore {
    fn record(&self, record: ActivityRecord<'_>) {
        let preview = record
            .command
            .filter(|_| self.preview_enabled)
            .map(|command| truncate_chars(command, COMMAND_PREVIEW_MAX_CHARS));
        if let Err(error) = self.db.insert_workspace_activity(
            chrono::Utc::now().timestamp(),
            &record,
            preview.as_deref(),
            self.max_rows,
        ) {
            tracing::warn!(error = %error, "workspace activity insert failed");
        }
    }
}

fn env_flag_disabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            value == "0" || value == "false" || value == "off"
        })
        .unwrap_or(false)
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
    use webcodex_core::activity_contract::{ActivityScope, ActivityVisibility};

    fn sample<'a>(command: Option<&'a str>) -> ActivityRecord<'a> {
        ActivityRecord {
            tool: "run_shell",
            project: Some("demo"),
            surface: "mcp",
            client: Some("laptop"),
            success: true,
            session_id: None,
            command,
            paths: vec!["a.rs".to_string()],
            error_summary: None,
            scope: ActivityScope::ProjectGrant("wc_pgrant_aaaaaaaaaaaaaaaa".to_string()),
        }
    }

    #[test]
    fn disabling_the_preview_stores_no_command_text() {
        let tmp = tempfile::tempdir().unwrap();
        let db = Arc::new(Database::open(&tmp.path().join("activity.db")).unwrap());

        let off = WorkspaceActivityStore::with_preview(db.clone(), false);
        off.record(sample(Some("deploy --token wc_pat_supersecret")));
        let rows = db
            .list_workspace_activity_for_clients(10, None, ActivityVisibility::Global, &[])
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].command_preview.is_none());
        assert_eq!(rows[0].tool, "run_shell");
        assert_eq!(rows[0].paths, vec!["a.rs".to_string()]);

        let on = WorkspaceActivityStore::with_preview(db.clone(), true);
        on.record(sample(Some("cargo test")));
        let rows = db
            .list_workspace_activity_for_clients(10, None, ActivityVisibility::Global, &[])
            .unwrap();
        assert_eq!(rows[0].command_preview.as_deref(), Some("cargo test"));
    }
}
