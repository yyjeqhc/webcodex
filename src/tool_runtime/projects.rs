//! Agent-side project management tools: `register_project` and
//! `create_project`.
//!
//! Both tools route to the selected agent via `enqueue_project_op`. The agent
//! validates the path against its own policy, writes `projects_dir/<id>.toml`
//! atomically (and for `create_project` creates the directory / template files
//! / optional git init), and returns structured JSON in `stdout`. The runtime
//! parses the JSON, refreshes the server-side project cache via
//! `upsert_client_project` so `listProjects` sees the new project immediately,
//! and returns the structured result.
//!
//! The server never writes project config files or creates directories on the
//! agent host directly. OS permissions and agent policy
//! (`allow_cwd_anywhere` / `allowed_roots`) remain the real boundary; there is
//! no workspace abstraction.

use serde_json::{json, Value};
use std::time::Duration;

use super::tool_result::ToolResult;
use super::{agent_project_runtime_id, ToolRuntime};
use crate::auth::AuthContext;
use crate::shell_protocol::ShellAgentProjectSummary;

/// Maximum time the runtime waits for an agent project-op response. Project
/// operations are fast (write a small TOML, maybe create a directory + git
/// init), so 30s is generous while still bounding the caller.
const PROJECT_OP_WAIT_SECS: u64 = 32;

impl ToolRuntime {
    pub(crate) async fn list_projects(&self, auth: Option<&AuthContext>) -> ToolResult {
        let mut list: Vec<Value> = Vec::new();
        for client in self.shell_clients.list_clients_for_auth(auth).await {
            // Sanitized shell-profiles summary for this agent (carried inside
            // the registration policy). Used to resolve which profile a project
            // actually uses and whether that profile is configured. `None` for
            // older agents that did not report one.
            let shell_profiles = client
                .policy
                .as_ref()
                .and_then(|p| p.shell_profiles.as_ref());
            for project in client.projects.iter().filter(|p| !p.disabled) {
                let (resolved_shell_profile, shell_profile_status) =
                    resolve_project_shell_profile(project.shell_profile.as_deref(), shell_profiles);
                let capabilities = smoke_project_capabilities(&client, project);
                list.push(json!({
                    "id": agent_project_runtime_id(&client.client_id, &project.id),
                    "agent_project_id": project.id,
                    "name": project.name,
                    "path": project.path,
                    "executor": "agent",
                    "client_id": client.client_id,
                    "allow_patch": project.allow_patch,
                    "source": "agent_registered",
                    "agent_status": client.status,
                    "connected": client.connected,
                    "last_seen": client.last_seen,
                    "shell_profile": project.shell_profile,
                    "resolved_shell_profile": resolved_shell_profile,
                    "shell_profile_status": shell_profile_status,
                    "capabilities": capabilities,
                }));
            }
        }
        list.sort_by(|a, b| {
            a["id"]
                .as_str()
                .unwrap_or_default()
                .cmp(b["id"].as_str().unwrap_or_default())
        });
        let recommended_for_smoke: Vec<Value> = list
            .iter()
            .filter(|project| {
                project["capabilities"]["recommended_for_smoke"]
                    .as_bool()
                    .unwrap_or(false)
            })
            .filter_map(|project| project["id"].as_str().map(|id| json!(id)))
            .collect();
        ToolResult::ok(json!({
            "count": list.len(),
            "projects": list,
            "recommended_for_smoke": recommended_for_smoke,
        }))
    }

    /// Register an existing directory as a WebCodex project on the selected
    /// agent. See the `ToolCall::RegisterProject` doc comment for the full
    /// contract. The server validates the owner boundary, builds a JSON
    /// payload, routes it to the agent, parses the JSON response, and
    /// refreshes the server-side project cache.
    pub(crate) async fn register_project(
        &self,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.project_op(
            "register_project",
            client_id,
            id,
            name,
            path,
            description,
            allow_patch,
            None,
            false,
            false,
            overwrite,
            auth,
        )
        .await
    }

    /// Create a new directory on the selected agent and register it as a
    /// WebCodex project. See the `ToolCall::CreateProject` doc comment for the
    /// full contract.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn create_project(
        &self,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        template: Option<String>,
        git_init: bool,
        allow_existing_empty: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        self.project_op(
            "create_project",
            client_id,
            id,
            name,
            path,
            description,
            allow_patch,
            template,
            git_init,
            allow_existing_empty,
            overwrite,
            auth,
        )
        .await
    }

    /// Shared implementation for both `register_project` and `create_project`.
    /// `kind` is `"register_project"` or `"create_project"`. Fields not
    /// applicable to `register_project` (template, git_init,
    /// allow_existing_empty) are ignored by the agent for that kind.
    #[allow(clippy::too_many_arguments)]
    async fn project_op(
        &self,
        kind: &str,
        client_id: String,
        id: String,
        name: String,
        path: String,
        description: Option<String>,
        allow_patch: bool,
        template: Option<String>,
        git_init: bool,
        allow_existing_empty: bool,
        overwrite: bool,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        // -- basic server-side request shape validation ----------------------
        // The agent does the authoritative path/policy validation, but the
        // server rejects obviously malformed requests early so the agent is
        // never bothered with them.
        if let Err(e) = validate_project_op_id(&id) {
            return ToolResult::err(e);
        }
        if let Err(e) = validate_project_op_name(&name) {
            return ToolResult::err(e);
        }
        if let Some(ref desc) = description {
            if let Err(e) = validate_project_op_description(desc) {
                return ToolResult::err(e);
            }
        }
        if let Err(e) = validate_project_op_path(&path) {
            return ToolResult::err(e);
        }

        // -- owner boundary + client existence --------------------------------
        if self
            .shell_clients
            .get_client_view(&client_id)
            .await
            .is_none()
        {
            return ToolResult::err(format!(
                "unknown agent client '{}'. Call listAgents to discover registered client_ids.",
                client_id
            ));
        }
        if let Err(e) = self
            .shell_clients
            .assert_client_access(auth, &client_id)
            .await
        {
            return ToolResult::err(e);
        }

        // -- build JSON payload and route to the agent ------------------------
        let payload = json!({
            "kind": kind,
            "client_id": client_id,
            "id": id,
            "name": name,
            "path": path,
            "description": description,
            "allow_patch": allow_patch,
            "template": template,
            "git_init": git_init,
            "allow_existing_empty": allow_existing_empty,
            "overwrite": overwrite,
        });
        let payload_str = match serde_json::to_string(&payload) {
            Ok(s) => s,
            Err(e) => {
                return ToolResult::err(format!("failed to serialize project op payload: {}", e))
            }
        };
        let (request_id, rx) = match self
            .shell_clients
            .enqueue_project_op(
                client_id.clone(),
                kind,
                payload_str,
                "tool_runtime".to_string(),
            )
            .await
        {
            Ok(result) => result,
            Err(e) => return ToolResult::err(e),
        };
        let response =
            match tokio::time::timeout(Duration::from_secs(PROJECT_OP_WAIT_SECS), rx).await {
                Ok(Ok(response)) => response,
                Ok(Err(_)) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    return ToolResult::err("project op request waiter was dropped");
                }
                Err(_) => {
                    self.shell_clients.cancel_request(&request_id).await;
                    return ToolResult::err(format!(
                        "timed out waiting {} seconds for agent project op result",
                        PROJECT_OP_WAIT_SECS
                    ));
                }
            };

        // -- parse the agent response -----------------------------------------
        // The agent returns structured JSON in stdout. On error, stdout may be
        // empty and the error is in the `error` field.
        if let Some(err) = response.error.as_ref() {
            return ToolResult::err(err.clone());
        }
        let stdout = response.stdout.as_deref().unwrap_or("");
        let result: Value = if stdout.is_empty() {
            return ToolResult::err("agent returned empty project op result");
        } else if response.exit_code != Some(0) {
            return ToolResult::err(format!(
                "agent project op failed with exit_code {:?}: {}",
                response.exit_code, stdout
            ));
        } else {
            match serde_json::from_str::<Value>(stdout) {
                Ok(v) => v,
                Err(e) => {
                    return ToolResult::err(format!(
                        "failed to parse agent project op response: {} (stdout: {})",
                        e,
                        truncate_for_error(stdout)
                    ))
                }
            }
        };

        // -- refresh server-side project cache --------------------------------
        // After a successful operation the agent reports the new/updated
        // project summary in the response. The server upserts it into the
        // client's cached project list so listProjects sees it immediately,
        // without waiting for the agent's next register/poll cycle.
        if let Some(project) = parse_project_summary_from_result(&result, &client_id) {
            let _ = self
                .shell_clients
                .upsert_client_project(&client_id, project)
                .await;
        }

        ToolResult::ok(result)
    }
}

fn agent_protocol_reports_project_git(protocol: &str) -> bool {
    matches!(
        protocol,
        crate::shell_protocol::AGENT_PROTOCOL_VERSION_POLLING_V1
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_WEBSOCKET_V1
            | crate::shell_protocol::AGENT_PROTOCOL_VERSION_QUIC_V1
    )
}

fn project_git_available(
    client: &crate::shell_protocol::ShellClientView,
    project: &crate::shell_protocol::ShellAgentProjectSummary,
) -> Option<bool> {
    if project.git_branch.is_some() || project.git_head.is_some() || project.git_dirty.is_some() {
        Some(true)
    } else if agent_protocol_reports_project_git(&client.agent_protocol_version) {
        Some(false)
    } else {
        None
    }
}

fn smoke_marker_present(project: &crate::shell_protocol::ShellAgentProjectSummary) -> bool {
    let name = project.name.as_deref().unwrap_or_default();
    [project.id.as_str(), name, project.path.as_str()]
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .any(|value| value.contains("smoke") || value.contains("test") || value.contains("sandbox"))
}

fn smoke_project_capabilities(
    client: &crate::shell_protocol::ShellClientView,
    project: &crate::shell_protocol::ShellAgentProjectSummary,
) -> Value {
    let git_available = project_git_available(client, project);
    let safe_smoke_project =
        project.allow_patch && client.connected && smoke_marker_present(project);
    let supports_artifact_smoke = client.capabilities.file_read && client.capabilities.file_write;
    let supports_cleanup_verification =
        supports_artifact_smoke || git_available.is_some_and(|available| available);
    let recommended_for_smoke = safe_smoke_project
        && git_available.is_some_and(|available| available)
        && supports_cleanup_verification;

    json!({
        "git_available": git_available,
        "safe_smoke_project": safe_smoke_project,
        "supports_artifact_smoke": supports_artifact_smoke,
        "supports_cleanup_verification": supports_cleanup_verification,
        "recommended_for_smoke": recommended_for_smoke,
    })
}

/// Resolve which shell profile a project uses and whether it is configured.
/// Returns `(resolved_name, status)` where:
/// - `resolved_name` = `project_shell_profile` (if set) else the agent's
///   `default_profile` (if any) else `None`.
/// - `status` = `"configured"` if the resolved name exists in the agent's
///   configured profiles; `"missing"` if a name resolved but is not
///   configured; `"not_configured"` if no profile resolves at all; and
///   `"unknown"` if the agent did not report a shell-profiles summary so the
///   configured set cannot be checked.
fn resolve_project_shell_profile(
    project_shell_profile: Option<&str>,
    summary: Option<&crate::shell_protocol::ShellProfilesSummary>,
) -> (Option<String>, &'static str) {
    let resolved = project_shell_profile
        .map(str::to_string)
        .or_else(|| summary.and_then(|s| s.default_profile.clone()));
    match resolved {
        None => (None, "not_configured"),
        Some(name) => match summary {
            None => (Some(name), "unknown"),
            Some(s) => {
                if s.profiles.iter().any(|p| p.name == name) {
                    (Some(name), "configured")
                } else {
                    (Some(name), "missing")
                }
            }
        },
    }
}

// =============================================================================
// Server-side request-shape validation helpers
// =============================================================================

/// Validate the project `id` field server-side. The agent does the
/// authoritative validation, but this rejects obviously malformed ids early.
/// Rules: non-empty, <= 64 chars, ASCII letters/digits/dash/underscore only,
/// no slash, no backslash, no dot-dot, no NUL.
fn validate_project_op_id(id: &str) -> Result<(), String> {
    if id.is_empty() {
        return Err("id cannot be empty".to_string());
    }
    if id.contains('\0') {
        return Err("id must not contain NUL".to_string());
    }
    if id.len() > 64 {
        return Err("id must be at most 64 characters".to_string());
    }
    if id.contains('/') || id.contains('\\') {
        return Err("id must not contain slash or backslash".to_string());
    }
    if id == ".." || id == "." {
        return Err("id cannot be '.' or '..'".to_string());
    }
    if id.contains("..") {
        return Err("id must not contain dot-dot traversal".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("id may only contain ASCII letters, digits, '-', and '_'".to_string());
    }
    Ok(())
}

/// Validate the project `name` field server-side: non-empty after trim, <= 120
/// chars, no NUL.
fn validate_project_op_name(name: &str) -> Result<(), String> {
    if name.contains('\0') {
        return Err("name must not contain NUL".to_string());
    }
    if name.trim().is_empty() {
        return Err("name cannot be empty".to_string());
    }
    if name.len() > 120 {
        return Err("name must be at most 120 characters".to_string());
    }
    Ok(())
}

/// Validate the optional `description` field: <= 500 chars, no NUL.
fn validate_project_op_description(desc: &str) -> Result<(), String> {
    if desc.contains('\0') {
        return Err("description must not contain NUL".to_string());
    }
    if desc.len() > 500 {
        return Err("description must be at most 500 characters".to_string());
    }
    Ok(())
}

/// Validate the project `path` field server-side: non-empty, absolute, no NUL.
/// The agent does the authoritative existence/policy/canonicalization check.
fn validate_project_op_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("path must not contain NUL".to_string());
    }
    if !path.starts_with('/') {
        return Err("path must be an absolute path".to_string());
    }
    Ok(())
}

/// Truncate a string for inclusion in an error message (bounded).
fn truncate_for_error(s: &str) -> String {
    const MAX: usize = 200;
    if s.len() <= MAX {
        s.to_string()
    } else {
        format!("{}…", &s[..MAX])
    }
}

/// Parse a `ShellAgentProjectSummary` from the agent's project-op JSON
/// response so the server can upsert it into the cached project list. The
/// response includes `agent_project_id`, `client_id`, `name`, `path`, and
/// `allow_patch` — enough to build a summary that `listProjects` can show
/// immediately.
fn parse_project_summary_from_result(
    result: &Value,
    _client_id: &str,
) -> Option<ShellAgentProjectSummary> {
    let agent_project_id = result.get("agent_project_id")?.as_str()?;
    let name = result
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path = result.get("path")?.as_str()?;
    let allow_patch = result
        .get("allow_patch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    Some(ShellAgentProjectSummary {
        id: agent_project_id.to_string(),
        name: name.or_else(|| Some(agent_project_id.to_string())),
        path: path.to_string(),
        allow_patch,
        kind: None,
        description: result
            .get("description")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        hooks: Vec::new(),
        disabled: false,
        git_branch: None,
        git_head: None,
        git_dirty: None,
        updated_at: chrono::Utc::now().timestamp(),
        shell_profile: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_id_rejects_empty() {
        assert!(validate_project_op_id("").is_err());
    }

    #[test]
    fn validate_id_rejects_nul() {
        assert!(validate_project_op_id("a\0b").is_err());
    }

    #[test]
    fn validate_id_rejects_slash() {
        assert!(validate_project_op_id("a/b").is_err());
    }

    #[test]
    fn validate_id_rejects_backslash() {
        assert!(validate_project_op_id("a\\b").is_err());
    }

    #[test]
    fn validate_id_rejects_dot_dot() {
        assert!(validate_project_op_id("..").is_err());
        assert!(validate_project_op_id("a..b").is_err());
    }

    #[test]
    fn validate_id_rejects_long() {
        let id = "a".repeat(65);
        assert!(validate_project_op_id(&id).is_err());
    }

    #[test]
    fn validate_id_accepts_valid() {
        assert!(validate_project_op_id("my-project").is_ok());
        assert!(validate_project_op_id("hello_123").is_ok());
    }

    #[test]
    fn validate_name_rejects_empty_after_trim() {
        assert!(validate_project_op_name("   ").is_err());
    }

    #[test]
    fn validate_name_rejects_nul() {
        assert!(validate_project_op_name("a\0b").is_err());
    }

    #[test]
    fn validate_path_rejects_relative() {
        assert!(validate_project_op_path("relative/path").is_err());
    }

    #[test]
    fn validate_path_rejects_empty() {
        assert!(validate_project_op_path("").is_err());
    }

    #[test]
    fn validate_path_rejects_nul() {
        assert!(validate_project_op_path("/root/\0").is_err());
    }

    #[test]
    fn validate_path_accepts_absolute() {
        assert!(validate_project_op_path("/root/git/my-project").is_ok());
    }

    #[test]
    fn parse_summary_extracts_fields() {
        let result = json!({
            "agent_project_id": "my-project",
            "client_id": "oe",
            "name": "My Project",
            "path": "/root/git/my-project",
            "allow_patch": true,
            "description": "desc",
        });
        let summary = parse_project_summary_from_result(&result, "oe").unwrap();
        assert_eq!(summary.id, "my-project");
        assert_eq!(summary.name.as_deref(), Some("My Project"));
        assert_eq!(summary.path, "/root/git/my-project");
        assert!(summary.allow_patch);
        assert!(!summary.disabled);
    }

    #[test]
    fn parse_summary_defaults_name_to_id() {
        let result = json!({
            "agent_project_id": "hello",
            "client_id": "oe",
            "path": "/root/git/hello",
        });
        let summary = parse_project_summary_from_result(&result, "oe").unwrap();
        assert_eq!(summary.name.as_deref(), Some("hello"));
    }

    #[test]
    fn validate_id_rejects_single_dot() {
        assert!(validate_project_op_id(".").is_err());
    }

    #[test]
    fn validate_id_rejects_non_alphanumeric() {
        assert!(validate_project_op_id("a!b").is_err());
        assert!(validate_project_op_id("a b").is_err());
        assert!(validate_project_op_id("a.b").is_err());
    }

    #[test]
    fn validate_description_rejects_nul() {
        assert!(validate_project_op_description("a\0b").is_err());
    }

    #[test]
    fn validate_description_rejects_long() {
        let desc = "a".repeat(501);
        assert!(validate_project_op_description(&desc).is_err());
    }

    #[test]
    fn validate_description_accepts_none() {
        // None/empty description is valid.
        assert!(validate_project_op_description("").is_ok());
    }

    #[test]
    fn validate_name_rejects_long() {
        let name = "a".repeat(121);
        assert!(validate_project_op_name(&name).is_err());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(validate_project_op_name("My Project").is_ok());
        assert!(validate_project_op_name("A").is_ok());
    }
}
