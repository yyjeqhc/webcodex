use serde_json::{json, Value as JsonValue};
use std::path::Path;
use std::time::{Duration, Instant};
use webcodex_admin::ServerHttpOptions;

use super::super::http::{call_runtime_tool_status, fetch_runtime_status};
use super::process::local_runner_state_summary;

pub(super) async fn preflight_shared_key(
    server_url: &str,
    server_http: &ServerHttpOptions,
    key: &str,
) -> Result<(), String> {
    async {
        let (status, content_type, body) = call_runtime_tool_status(
            server_url,
            server_http,
            Some(key),
            "list_projects",
            json!({}),
        )
        .await?;
        if !(200..300).contains(&status) {
            return Err(format!(
                "Server returned HTTP {status} ({content_type}) for canonical list_projects"
            ));
        }
        if body
            .as_ref()
            .and_then(|value| value.get("success"))
            .and_then(JsonValue::as_bool)
            != Some(true)
        {
            return Err("Server rejected canonical list_projects".to_string());
        }
        Ok::<(), String>(())
    }
    .await
    .map_err(|error| {
        format!(
            "Server did not accept hosted shared-key access: {error}. Confirm shared-key mode is enabled and use a non-wc_ key"
        )
    })
}

pub(super) fn runtime_client_online(output: &JsonValue, client_id: &str) -> bool {
    output
        .pointer("/agents/clients")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .any(|client| {
            client.get("client_id").and_then(JsonValue::as_str) == Some(client_id)
                && (client.get("connected").and_then(JsonValue::as_bool) == Some(true)
                    || client.get("status").and_then(JsonValue::as_str) == Some("online"))
        })
}

pub(super) fn project_visible(
    output: &JsonValue,
    runtime_project_id: &str,
    client_id: &str,
) -> bool {
    output
        .pointer("/output/projects")
        .or_else(|| output.get("projects"))
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .any(|project| {
            project.get("id").and_then(JsonValue::as_str) == Some(runtime_project_id)
                && project.get("client_id").and_then(JsonValue::as_str) == Some(client_id)
                && project
                    .get("connected")
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(true)
        })
}

pub(super) async fn wait_for_connection(
    server_url: &str,
    server_http: &ServerHttpOptions,
    key: &str,
    client_id: &str,
    runtime_project_id: &str,
    state_dir: &Path,
    timeout_ms: u64,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    let mut last_error = None;
    loop {
        let summary = local_runner_state_summary(state_dir)?;
        if !summary.running {
            return Err("Runner exited before it registered with the Server".to_string());
        }
        let runtime = fetch_runtime_status(server_url, server_http, Some(key)).await;
        let projects = async {
            let (status, content_type, value) = call_runtime_tool_status(
                server_url,
                server_http,
                Some(key),
                "list_projects",
                json!({}),
            )
            .await?;
            if !(200..300).contains(&status) {
                return Err(format!(
                    "canonical list_projects returned HTTP {status} ({content_type})"
                ));
            }
            value.ok_or_else(|| "canonical list_projects returned no JSON body".to_string())
        }
        .await;
        match (runtime, projects) {
            (Ok(runtime), Ok(projects))
                if runtime
                    .output
                    .as_ref()
                    .is_some_and(|output| runtime_client_online(output, client_id))
                    && project_visible(&projects, runtime_project_id, client_id) =>
            {
                return Ok(())
            }
            (Ok(runtime), Ok(_)) if !runtime.reachable => {
                last_error = runtime.error;
            }
            (Err(error), _) | (_, Err(error)) => last_error = Some(error),
            _ => {}
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "timed out waiting for Runner and project visibility{}",
        last_error
            .map(|error| format!(": {error}"))
            .unwrap_or_default()
    ))
}
