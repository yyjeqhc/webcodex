//! Shared Runner fixture for snapshot-fenced artifact readers.
//!
//! Format signatures, path rules and App authorization assertions stay in each
//! reader's tests; the transport and version fence have one fixture owner.

use super::{
    agent_test_project_id, complete_patch_agent_request, managed_oauth_auth_context,
    register_agent, runtime_with_agent_project, wait_for_patch_agent_request,
};
use crate::tool_runtime::ToolRuntime;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(in crate::tool_runtime::tests) async fn setup_artifact_reader(
    client: &str,
) -> (ToolRuntime, crate::auth::AuthContext, String) {
    let runtime = runtime_with_agent_project(client);
    register_agent(
        &runtime,
        client,
        Some("alice"),
        crate::runner_protocol::RunnerCapabilities {
            file_read: true,
            artifact_export_chunk_read: true,
            artifact_export_streaming_metadata: true,
            ..Default::default()
        },
    )
    .await;
    (
        runtime,
        managed_oauth_auth_context("alice", None),
        agent_test_project_id(client),
    )
}

pub(in crate::tool_runtime::tests) async fn complete_artifact_chunk(
    runtime: &ToolRuntime,
    client: &str,
    path: &str,
    data: &[u8],
    offset: usize,
    length: usize,
    stale: bool,
) {
    let request = wait_for_patch_agent_request(runtime, client).await;
    assert_eq!(request.kind, "file_read_project_artifact_export_chunk");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["offset"], offset);
    assert_eq!(payload["length"], length);
    assert_eq!(
        payload["expected_sha256"],
        format!("{:x}", Sha256::digest(data))
    );
    assert_eq!(payload["expected_file_bytes"], data.len());
    let next = offset + length;
    let output = if stale {
        json!({"error_kind":"snapshot_changed", "error":"changed", "actual_sha256":"private-current-digest"})
    } else {
        json!({"path":path, "file_bytes":data.len(), "offset":offset, "bytes_returned":length,
            "next_offset":next, "eof":next==data.len(), "content_base64":general_purpose::STANDARD.encode(&data[offset..next])})
    };
    complete_patch_agent_request(
        runtime,
        client,
        &request.request_id,
        0,
        &output.to_string(),
        "",
    )
    .await;
}
