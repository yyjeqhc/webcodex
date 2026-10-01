//! Files tests for tool_runtime.

use super::super::files::*;
use super::super::helpers::*;
use super::super::*;
use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerRequest, RunnerResultRequest};
use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use serde_json::{json, Value};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(windows)]
async fn run_windows_tracked_listing(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    path: Option<String>,
) -> (ToolResult, String) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .list_project_tracked_files(project, path, None, None, None, Some(100), Some(0))
                .await
        }
    });
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "run_internal_posix_script");
    assert!(request.command.is_empty());
    let payload = request
        .script
        .as_ref()
        .expect("tracked listing must carry a typed internal POSIX program");
    assert_eq!(
        payload.language,
        crate::runner_protocol::ShellScriptLanguage::Sh
    );
    let script = payload.script.clone();
    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        exit_code,
        &stdout,
        &stderr,
    )
    .await;
    (task.await.unwrap(), script)
}
include!("files/mutations_and_audit.rs");
include!("files/listing_and_parsing.rs");
include!("files/search.rs");
include!("files/artifacts_and_reads.rs");
