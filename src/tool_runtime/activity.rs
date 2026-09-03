//! Workspace activity recording: a bounded, human-facing ledger of every
//! mutating tool execution, regardless of which client surface issued it.
//!
//! The runtime stays storage-agnostic: dispatch emits [`ActivityRecord`]s
//! through the injected [`ActivityRecorder`], and the host wires a durable
//! implementation (SQLite in the server binary). The default is a no-op so
//! embedded runtimes and tests pay nothing.

use serde_json::Value;
pub use webcodex_core::activity_contract::{ActivityRecord, ActivityScope, ActivityVisibility};

/// Project the already-authenticated root context into the protocol-neutral
/// persistence scope. Credential hashes and live client ownership are never
/// used as durable attribution.
pub(crate) fn activity_scope_from_auth(auth: Option<&crate::auth::AuthContext>) -> ActivityScope {
    let Some(auth) = auth else {
        return ActivityScope::Unscoped;
    };
    if auth.is_bootstrap() || auth.is_admin() {
        return ActivityScope::HostGlobal;
    }
    match auth.project_grant_id.as_deref() {
        Some(grant) if !grant.is_empty() => ActivityScope::ProjectGrant(grant.to_string()),
        _ => ActivityScope::Unscoped,
    }
}

pub trait ActivityRecorder: Send + Sync {
    fn record(&self, record: ActivityRecord<'_>);
}

/// Default recorder: drops everything.
pub struct NoopActivityRecorder;

impl ActivityRecorder for NoopActivityRecorder {
    fn record(&self, _record: ActivityRecord<'_>) {}
}

/// Executing device for an agent-backed project id (`agent:<client>:<name>`).
/// Local projects have no device.
pub(crate) fn agent_client_from_project(project: &str) -> Option<&str> {
    project
        .strip_prefix("agent:")
        .and_then(|rest| rest.split_once(':'))
        .map(|(client, _)| client)
        .filter(|client| !client.is_empty())
}

/// Extract the paths a call names from its audit-sanitized argument summary
/// (`ToolCall::session_log_arguments`), so activity rows reuse the exact same
/// sanitization discipline as session events.
pub(crate) fn paths_from_sanitized_arguments(arguments: &Value, cap: usize) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    {
        let mut push = |value: &Value| {
            if let Some(path) = value.as_str() {
                if !path.is_empty() && paths.len() < cap && !paths.iter().any(|p| p == path) {
                    paths.push(path.to_string());
                }
            }
        };
        push(&arguments["path"]);
        for key in ["paths", "destination_paths"] {
            if let Some(list) = arguments[key].as_array() {
                for value in list {
                    push(value);
                }
            }
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn paths_extraction_dedupes_and_caps() {
        let arguments = json!({
            "path": "a.rs",
            "paths": ["a.rs", "b.rs", "", "c.rs"],
            "destination_paths": ["d.rs"]
        });
        assert_eq!(
            paths_from_sanitized_arguments(&arguments, 3),
            vec!["a.rs", "b.rs", "c.rs"]
        );
        assert_eq!(
            paths_from_sanitized_arguments(&arguments, 16),
            vec!["a.rs", "b.rs", "c.rs", "d.rs"]
        );
        assert!(paths_from_sanitized_arguments(&json!({}), 16).is_empty());
    }

    #[test]
    fn client_extraction_handles_agent_and_local_projects() {
        assert_eq!(
            agent_client_from_project("agent:laptop:webcodex"),
            Some("laptop")
        );
        assert_eq!(agent_client_from_project("agent::webcodex"), None);
        assert_eq!(agent_client_from_project("demo"), None);
        assert_eq!(agent_client_from_project("agent:solo"), None);
    }
}
