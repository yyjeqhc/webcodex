//! Git tests for tool_runtime.

use super::super::git::*;
use super::super::git_review::*;
use super::super::helpers::*;
use super::super::*;
use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerResultRequest};
use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use crate::tool_runtime::ToolRuntime;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use webcodex_core::runtime_contract::{
    DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES, MAX_GIT_DIFF_HUNKS_PAGE_BYTES,
    MIN_GIT_DIFF_HUNKS_PAGE_BYTES, MODEL_INSPECTION_MAX_RESULT_BYTES,
};

const ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES: usize = 256 * 1024;

async fn register_structured_git_agent_at_path(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    root: &Path,
) -> String {
    let project_path = root.to_string_lossy().to_string();
    register_agent_with_projects(
        runtime,
        client_id,
        None,
        RunnerCapabilities {
            shell: true,
            git: true,
            structured_process_argv: true,
            internal_posix_script: true,
            ..Default::default()
        },
        vec![registered_project(project_id, &project_path)],
    )
    .await;
    crate::tool_runtime::runner_project_runtime_id(client_id, project_id)
}

async fn collect_review_task(
    runtime: &ToolRuntime,
    client_id: &str,
    task: tokio::task::JoinHandle<ToolResult>,
) -> ToolResult {
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while !task.is_finished() {
        assert!(
            tokio::time::Instant::now() < deadline,
            "review fixture timed out"
        );
        if let Some(request) = probe_patch_agent_request(runtime, client_id).await {
            complete_agent_request_by_running_locally(runtime, client_id, request).await;
        } else {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
    task.await.unwrap()
}

async fn observe_review_source(runtime: &ToolRuntime, project: &str) -> Value {
    let client_id = project.split(':').nth(1).unwrap();
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.to_string();
        async move {
            match runtime.workspace_review_source_identity(&project).await {
                Ok(source) => ToolResult::ok(source.presentation_value()),
                Err(error) => error,
            }
        }
    });
    let result = collect_review_task(runtime, client_id, task).await;
    assert!(result.success, "{result:?}");
    result.output
}

include!("git/review_and_mutations.rs");
include!("git/diff_hunks.rs");
include!("git/show_changes.rs");
include!("git/read_and_review.rs");
