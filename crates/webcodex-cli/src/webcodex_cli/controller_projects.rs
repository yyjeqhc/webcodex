use super::{
    configured_server_url, runner_view, ControllerConfig, ProjectAction, RunnerConfigView,
};
use crate::webcodex_cli::{
    connections, http, project, read_optional_token, validate_user_api_token,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;
use webcodex_admin::ServerHttpOptions;

#[cfg(test)]
#[path = "tests/controller_projects.rs"]
mod tests;

fn default_token_file(config_path: &Path, view: &RunnerConfigView) -> Result<PathBuf, String> {
    let config_path = config_path.canonicalize().map_err(|e| e.to_string())?;
    let default_base = connections::default_base_dir()?;
    let mut bases = vec![default_base];
    // Also support a connection stored under an explicitly chosen login base.
    if let Some(base) = config_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
    {
        if !bases.iter().any(|candidate| candidate == base) {
            bases.push(base.to_path_buf());
        }
    }
    select_default_token(&config_path, view, &bases)
}

fn select_default_token(
    config_path: &Path,
    view: &RunnerConfigView,
    bases: &[PathBuf],
) -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    for base in bases {
        for connection in connections::connections_for_server(base, &view.server_url) {
            if runner_view(&connection.paths.runner_config).is_ok_and(|candidate| {
                candidate.client_id == view.client_id
                    && connections::canonical_server_url(&candidate.server_url).ok()
                        == connections::canonical_server_url(&view.server_url).ok()
            }) {
                candidates.push(connection.paths);
            }
        }
    }
    candidates.sort_by(|a, b| a.dir.cmp(&b.dir));
    candidates.dedup_by(|a, b| a.dir == b.dir);
    let exact = candidates
        .iter()
        .filter(|paths| {
            paths
                .runner_config
                .canonicalize()
                .is_ok_and(|path| path == config_path)
        })
        .collect::<Vec<_>>();
    if exact.len() == 1 {
        return Ok(exact[0].user_token.clone());
    }
    if candidates.len() == 1 {
        return Ok(candidates[0].user_token.clone());
    }
    Err(
        "Cannot identify one default user credential for this Runner; pass --user-token-file PATH"
            .to_string(),
    )
}

struct ProjectClient {
    server: String,
    client_id: String,
    token: String,
    http: ServerHttpOptions,
}

impl ProjectClient {
    async fn call(&self, tool: &str, params: Value, mutation: bool) -> Result<Value, String> {
        self.request(
            "/api/tools/call",
            json!({"tool":tool,"params":params}),
            mutation,
        )
        .await
    }

    async fn request(&self, path: &str, body: Value, mutation: bool) -> Result<Value, String> {
        let response = tokio::time::timeout(
            // The Server's project-op wait is 32 seconds; leave time for its terminal response.
            Duration::from_secs(45),
            http::http_post_json_status(&self.server, &self.http, path, Some(&self.token), body),
        )
        .await;
        let uncertain = "project_operation_outcome_unknown: response unavailable; observe project state before retrying";
        let (status, _, body) = match response {
            Ok(Ok(response)) => response,
            _ if mutation => return Err(uncertain.to_string()),
            _ => return Err("server_unreachable: cannot observe Runner/project state".to_string()),
        };
        if matches!(status, 401 | 403) {
            return Err(format!("project_access_denied: Server returned HTTP {status}; check the user credential and permissions"));
        }
        if matches!(status, 404 | 405) {
            return Err(
                "project_api_unavailable: Server does not support this project API".to_string(),
            );
        }
        let Some(body) = body else {
            return Err(if mutation {
                uncertain.to_string()
            } else {
                format!(
                    "invalid_project_response: Server returned HTTP {status} without valid JSON"
                )
            });
        };
        if body.get("success").and_then(Value::as_bool) == Some(false) {
            // Preserve canonical error codes and recovery details, but never echo the credential.
            return Err(body.to_string().replace(&self.token, "[redacted]"));
        }
        if !(200..300).contains(&status)
            || body.get("success").and_then(Value::as_bool) != Some(true)
            || !body.get("output").is_some_and(Value::is_object)
        {
            return Err(if mutation {
                uncertain.to_string()
            } else {
                format!("invalid_project_response: Server returned HTTP {status}")
            });
        }
        Ok(body["output"].clone())
    }

    async fn require_online(&self) -> Result<Value, String> {
        let output = self
            .call(
                "list_runners",
                json!({
                    "client_id":self.client_id,"summary_only":true,"include_projects":false
                }),
                false,
            )
            .await?;
        let runners = output
            .get("runners")
            .and_then(Value::as_array)
            .ok_or("invalid_runner_response: missing runners")?;
        let runner = runners.iter().find(|runner| runner["client_id"].as_str() == Some(&self.client_id))
            .ok_or_else(|| format!("runner_not_visible: Runner {} is not registered or is not visible to this credential", self.client_id))?;
        if runner["status"].as_str() != Some("online") {
            return Err(format!(
                "runner_offline: Runner {} is not online; project commands are unavailable",
                self.client_id
            ));
        }
        Ok(runner["project_inventory"].clone())
    }

    async fn list(&self, exact_project: Option<&str>) -> Result<Value, String> {
        let mut params = json!({"client_id":self.client_id,"limit":100});
        if let Some(project) = exact_project {
            params["project"] = json!(project);
        }
        let mut output = self.call("list_projects", params, false).await?;
        let projects = output
            .get("projects")
            .and_then(Value::as_array)
            .ok_or("invalid_project_response: missing projects")?;
        if projects
            .iter()
            .any(|project| project["client_id"].as_str() != Some(&self.client_id))
        {
            return Err("invalid_project_response: inventory contains another Runner".to_string());
        }
        // Re-observe after the inventory request so an observed disconnect is never an empty list.
        output["project_inventory"] = self.require_online().await?;
        Ok(output)
    }

    async fn execute(&self, action: ProjectAction) -> Result<Value, String> {
        self.require_online().await?;
        match action {
            ProjectAction::List => self.list(None).await,
            ProjectAction::Register { project } => {
                let project = project
                    .canonicalize()
                    .map_err(|e| format!("Cannot resolve project path: {e}"))?;
                if !project.is_dir() {
                    return Err("Project path must be an existing directory".to_string());
                }
                self.request(
                    "/api/projects/resolve-or-register",
                    json!({
                        "client_id":self.client_id,"path":project
                    }),
                    true,
                )
                .await
            }
            ProjectAction::Remove { target } => {
                // A caller-supplied runtime ID can be queried exactly without inventing identity.
                let exact = target.starts_with("agent:").then_some(target.as_str());
                let output = self.list(exact).await?;
                let project = select_project(&output, &target)?;
                let id = project["id"]
                    .as_str()
                    .ok_or("Project inventory omitted id")?;
                let revision = project["revision"]
                    .as_str()
                    .filter(|v| !v.is_empty())
                    .ok_or("Project inventory omitted revision; unregister was not dispatched")?;
                let result = self
                    .call(
                        "unregister_project",
                        json!({
                            "project":id,"expected_revision":revision
                        }),
                        true,
                    )
                    .await?;
                if result["project"].as_str() != Some(id)
                    || !matches!(
                        result["outcome"].as_str(),
                        Some("unregistered" | "already_unregistered")
                    )
                {
                    return Err("project_operation_outcome_unknown: unregister did not return a terminal outcome for the requested project".to_string());
                }
                Ok(result)
            }
        }
    }
}

fn select_project<'a>(output: &'a Value, target: &str) -> Result<&'a Value, String> {
    if output["truncated"].as_bool() != Some(false)
        || output
            .pointer("/project_inventory/sync_state")
            .and_then(Value::as_str)
            != Some("complete")
    {
        return Err("project_inventory_incomplete: no unregister was dispatched; use a full project ID if the list is truncated, or wait for inventory synchronization".to_string());
    }
    let canonical = Path::new(target).canonicalize().ok();
    let projects = output["projects"]
        .as_array()
        .ok_or("Project inventory omitted projects")?;
    let matches = projects
        .iter()
        .filter(|project| {
            ["id", "agent_project_id", "project_ref", "path"]
                .iter()
                .any(|key| project[*key].as_str() == Some(target))
                || canonical.as_ref().is_some_and(|requested| {
                    project["path"]
                        .as_str()
                        .and_then(|path| Path::new(path).canonicalize().ok())
                        .is_some_and(|stored| {
                            webcodex_runner_config::paths::paths_equal(&stored, requested)
                        })
                })
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [project] => Ok(project),
        [] => Err(
            "project_not_found: target is not in this Runner's visible project inventory"
                .to_string(),
        ),
        _ => Err("project_target_ambiguous: use the exact project id".to_string()),
    }
}

pub(super) async fn run(
    config: &ControllerConfig,
    action: ProjectAction,
    user_token_file: Option<&Path>,
    as_json: bool,
) -> Result<String, String> {
    let result = run_inner(config, action, user_token_file).await;
    let output = result.map_err(|error| format_error(error, as_json))?;
    if as_json {
        return serde_json::to_string_pretty(&output).map_err(|e| e.to_string());
    }
    if let Some(projects) = output.get("projects").and_then(Value::as_array) {
        let mut text = format!("Projects: {}\n", projects.len());
        for project in projects {
            text.push_str(&format!(
                "  {}  {}\n",
                project["id"].as_str().unwrap_or("?"),
                project["path"].as_str().unwrap_or("?")
            ));
        }
        if output["truncated"].as_bool() != Some(false) {
            text.push_str("Inventory is truncated; this is not the complete project list.\n");
        }
        if output
            .pointer("/project_inventory/sync_state")
            .and_then(Value::as_str)
            != Some("complete")
        {
            text.push_str("Runner project inventory is not fully synchronized.\n");
        }
        return Ok(text);
    }
    serde_json::to_string_pretty(&output)
        .map(|text| format!("{text}\n"))
        .map_err(|e| e.to_string())
}

pub(super) fn format_error(error: String, as_json: bool) -> String {
    if !as_json
        || serde_json::from_str::<Value>(&error)
            .is_ok_and(|value| value["success"].as_bool() == Some(false))
    {
        return error;
    }
    json!({"success":false,"error":error}).to_string()
}

async fn run_inner(
    config: &ControllerConfig,
    action: ProjectAction,
    token_file: Option<&Path>,
) -> Result<Value, String> {
    if !config.runner.enabled {
        return Err("runner_disabled: project commands require runner.enabled=true".to_string());
    }
    let view = runner_view(&config.runner.config)?;
    let server = connections::canonical_server_url(&view.server_url)?.url;
    if server != connections::canonical_server_url(&configured_server_url(config)?)?.url {
        return Err("Runner server_url does not match the Controller Server".to_string());
    }
    let token_file = match token_file {
        Some(path) => path.to_path_buf(),
        None => default_token_file(&config.runner.config, &view)?,
    };
    let token = read_optional_token(&Some(token_file), "--user-token-file")?
        .ok_or("User credential is missing; pass --user-token-file PATH")?;
    validate_user_api_token(&token)?;
    let http = ServerHttpOptions {
        no_system_proxy: project::server_url_is_loopback(&server),
        ..ServerHttpOptions::default()
    };
    ProjectClient {
        server,
        client_id: view.client_id,
        token,
        http,
    }
    .execute(action)
    .await
}
