//! Desktop uses the same authorized, bounded product endpoints as the WebUI.
//! Plugin reload is an explicit action dispatched through the canonical Plugin gate.
//! The WebView never receives a credential or chooses a URL, header, or tool.
use crate::error::{DesktopError, DesktopResult};
use crate::models::StoredRuntime;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::io::AsyncReadExt;

const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkspaceRequest {
    Overview {},
    Windows {},
    Sessions {
        project: String,
    },
    Session {
        project: String,
        session_id: String,
    },
    Window {
        client_window_key: String,
    },
    Extensions {
        project: String,
    },
    ProjectGit {
        project: String,
    },
    Instruction {
        project: String,
        source_scope: String,
        path: String,
        fingerprint: String,
    },
    PluginReload {
        project: String,
        plugin: String,
    },
}

pub fn unavailable() -> DesktopError {
    DesktopError::new(
        "workspace_unavailable",
        "Workspace could not be refreshed",
        "Check the Server and Runner, then refresh.",
    )
}

fn request_body(request: WorkspaceRequest, runner: &str) -> DesktopResult<(&'static str, Value)> {
    // These selectors are identities, not paths. Authorization remains at the Server.
    let valid = |value: &str| {
        !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
    };
    let own_project = |value: &str| valid(value) && value.starts_with(&format!("agent:{runner}:"));
    Ok(match request {
        WorkspaceRequest::Overview {} => {
            ("runner", json!({"client_id": runner, "project_limit": 32}))
        }
        WorkspaceRequest::Windows {} => ("windows", json!({"limit": 64})),
        WorkspaceRequest::Sessions { project } if own_project(&project) => (
            "workflow-sessions",
            json!({"project": project, "limit": 50}),
        ),
        WorkspaceRequest::Session {
            project,
            session_id,
        } if own_project(&project) && valid(&session_id) => (
            "workflow-session",
            json!({"project": project, "session_id": session_id, "limit": 50}),
        ),
        WorkspaceRequest::Window { client_window_key } if valid(&client_window_key) => (
            "window",
            json!({"client_window_key": client_window_key, "activity_limit": 30, "session_limit": 20}),
        ),
        WorkspaceRequest::Extensions { project } if own_project(&project) => {
            ("extensions", json!({"project": project}))
        }
        WorkspaceRequest::ProjectGit { project } if own_project(&project) => {
            ("project-git", json!({"project": project}))
        }
        WorkspaceRequest::Instruction {
            project,
            source_scope,
            path,
            fingerprint,
        } if own_project(&project)
            && matches!(source_scope.as_str(), "runner" | "project")
            && path.len() <= 4096
            && fingerprint.len() <= 128 =>
        {
            (
                "instruction",
                json!({"project":project,"source_scope":source_scope,"path":path,"fingerprint":fingerprint}),
            )
        }
        WorkspaceRequest::PluginReload { project, plugin }
            if own_project(&project) && valid(&plugin) && plugin.len() <= 64 =>
        {
            ("plugin-reload", json!({"project":project,"plugin":plugin}))
        }
        _ => return Err(unavailable()),
    })
}

pub async fn query(runtime: &StoredRuntime, request: WorkspaceRequest) -> DesktopResult<Value> {
    let runner = runtime
        .runner_client_id
        .as_deref()
        .ok_or_else(unavailable)?;
    let (route, body) = request_body(request, runner)?;
    post(
        runtime,
        &format!("/api/runtime-console/{route}"),
        body,
        Duration::from_secs(12),
    )
    .await
}

// Native callers choose fixed routes and schemas; never expose a generic WebView dispatch.
pub(crate) async fn post(
    runtime: &StoredRuntime,
    route: &str,
    body: Value,
    timeout: Duration,
) -> DesktopResult<Value> {
    let mut url = url::Url::parse(&runtime.server_url).map_err(|_| unavailable())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(unavailable());
    }
    url.set_path(route);
    url.set_query(None);
    url.set_fragment(None);
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(timeout);
    if matches!(
        url.host_str(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    ) {
        builder = builder.no_proxy();
    }
    let token_path = runtime.user_token_file.as_ref().ok_or_else(unavailable)?;
    let mut token = String::new();
    tokio::fs::File::open(token_path)
        .await
        .map_err(|_| unavailable())?
        .take(16_385)
        .read_to_string(&mut token)
        .await
        .map_err(|_| unavailable())?;
    if token.len() > 16_384 || token.trim().is_empty() {
        return Err(unavailable());
    }
    let client = builder.build().map_err(|_| unavailable())?;
    let mut response = client
        .post(url)
        .bearer_auth(token.trim())
        .json(&body)
        .send()
        .await
        .map_err(|_| unavailable())?;
    drop(token);
    if !response.status().is_success() {
        return Err(unavailable());
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(unavailable());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| unavailable())? {
        if bytes.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(unavailable());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| unavailable())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_vocabulary_is_closed_and_selectors_are_bounded() {
        assert!(serde_json::from_value::<WorkspaceRequest>(
            json!({"kind":"overview","url":"https://other.example"})
        )
        .is_err());
        assert!(serde_json::from_value::<WorkspaceRequest>(json!({"kind":"run_shell"})).is_err());
        assert!(request_body(
            WorkspaceRequest::Sessions {
                project: "x".repeat(513)
            },
            "runner"
        )
        .is_err());
        assert_eq!(
            request_body(WorkspaceRequest::Overview {}, "mini")
                .unwrap()
                .1,
            json!({"client_id":"mini","project_limit":32})
        );
    }
}
