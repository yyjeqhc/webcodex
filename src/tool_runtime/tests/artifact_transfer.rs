//! Project-to-Project artifact transfer protocol tests.

use super::super::sessions::SessionTransport;
use super::support::*;
use crate::auth::{AuthContext, AuthKind};
use crate::db::{
    ArtifactHandoffGrant, ArtifactHandoffPrincipal, ArtifactHandoffSourceSnapshot,
    NewArtifactHandoffGrant,
};
use crate::runner_protocol::RunnerCapabilities;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn transfer_caps(read: bool, write: bool) -> RunnerCapabilities {
    RunnerCapabilities {
        file_read: read,
        file_write: write,
        artifact_export_chunk_read: read,
        artifact_export_streaming_metadata: read,
        ..Default::default()
    }
}

fn transfer_auth(username: &str) -> AuthContext {
    AuthContext {
        kind: AuthKind::OAuth2Token,
        user_id: Some(format!("user-{username}")),
        username: Some(username.to_string()),
        api_key_id: Some(format!("oauth-{username}")),
        role: Some("user".to_string()),
        scopes: vec![
            crate::auth::SCOPE_PROJECT_READ.to_string(),
            crate::auth::SCOPE_PROJECT_WRITE.to_string(),
        ],
        is_bootstrap: false,
        token_kind: Some("oauth2".to_string()),
        allowed_client_id: Some("transfer-test-client".to_string()),
        shared_key_hash: None,
        project_grant_id: None,
    }
}

fn runtime_with_handoff_db(
    client_id: &str,
) -> (
    tempfile::TempDir,
    Arc<crate::db::Database>,
    super::super::ToolRuntime,
) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(
        crate::db::Database::open(&temp.path().join("artifact-handoff-transfer.db")).unwrap(),
    );
    let runtime = runtime_with_agent_project(client_id).with_communication_database(db.clone());
    (temp, db, runtime)
}

fn handoff_principal(auth: &AuthContext) -> ArtifactHandoffPrincipal {
    ArtifactHandoffPrincipal::try_from(
        super::super::communication::communication_principal(Some(auth)).unwrap(),
    )
    .unwrap()
}

fn create_handoff_grant(
    db: &crate::db::Database,
    source_auth: &AuthContext,
    destination_auth: &AuthContext,
    source_client: &str,
    destination_client: &str,
    source_path: &str,
    bytes: &[u8],
    mime_type: &str,
    one_shot: bool,
) -> ArtifactHandoffGrant {
    let name = source_path.rsplit('/').next().unwrap();
    db.create_artifact_handoff_grant(
        &handoff_principal(source_auth),
        NewArtifactHandoffGrant {
            source_project: agent_test_project_id(source_client),
            source_snapshot: ArtifactHandoffSourceSnapshot {
                path: source_path.to_string(),
                bytes: bytes.len() as u64,
                sha256: sha256_hex(bytes),
                mime_type: mime_type.to_string(),
                name: name.to_string(),
            },
            destination_principal: handoff_principal(destination_auth),
            destination_project: agent_test_project_id(destination_client),
            operation: crate::db::ArtifactHandoffOperation::Read,
            one_shot,
            ttl_ms: Some(60_000),
        },
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap()
}

fn spawn_accept(
    runtime: &super::super::ToolRuntime,
    grant: &ArtifactHandoffGrant,
    destination_path: &str,
    overwrite: bool,
    idempotency_key: &str,
    auth: &AuthContext,
) -> tokio::task::JoinHandle<crate::tool_runtime::ToolResult> {
    let runtime = runtime.clone();
    let grant_id = grant.grant_id.clone();
    let destination_project = grant.destination_project.clone();
    let destination_path = destination_path.to_string();
    let idempotency_key = idempotency_key.to_string();
    let auth = auth.clone();
    tokio::spawn(async move {
        runtime
            .accept_artifact_handoff(
                grant_id,
                destination_project,
                destination_path,
                Some(overwrite),
                idempotency_key,
                Some(&auth),
                SessionTransport::Api,
            )
            .await
    })
}

async fn complete_source_metadata(
    runtime: &super::super::ToolRuntime,
    client_id: &str,
    path: &str,
    bytes: &[u8],
    mime_type: Option<&str>,
) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(
        payload["max_bytes"],
        super::super::MAX_PROJECT_ARTIFACT_EXPORT_BYTES
    );
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &json!({
            "path": path,
            "exists": true,
            "missing": false,
            "bytes": bytes.len(),
            "sha256": sha256_hex(bytes),
            "mime_type": mime_type,
        })
        .to_string(),
        "",
    )
    .await;
}

async fn complete_destination_metadata(
    runtime: &super::super::ToolRuntime,
    client_id: &str,
    path: &str,
    bytes: &[u8],
    mime_type: &str,
) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], path);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &json!({
            "path": path,
            "exists": true,
            "missing": false,
            "bytes": bytes.len(),
            "sha256": sha256_hex(bytes),
            "mime_type": mime_type,
        })
        .to_string(),
        "",
    )
    .await;
}

async fn complete_destination_begin(
    runtime: &super::super::ToolRuntime,
    client_id: &str,
    destination_path: &str,
    bytes: &[u8],
    expected_mime: &str,
    overwrite: bool,
    upload_id: &str,
) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "file_artifact_upload_begin");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], destination_path);
    assert_eq!(payload["expected_bytes"], bytes.len());
    assert_eq!(payload["expected_sha256"], sha256_hex(bytes));
    assert_eq!(payload["mime_type"], expected_mime);
    assert_eq!(payload["overwrite"], overwrite);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &json!({
            "path": destination_path,
            "upload_id": upload_id,
            "received_bytes": 0,
            "next_offset": 0,
            "expected_bytes": bytes.len(),
            "expected_sha256": sha256_hex(bytes),
            "mime_type": expected_mime,
            "committed": false,
        })
        .to_string(),
        "",
    )
    .await;
}

async fn complete_one_transfer_chunk(
    runtime: &super::super::ToolRuntime,
    source_client: &str,
    destination_client: &str,
    source_path: &str,
    destination_path: &str,
    bytes: &[u8],
    offset: usize,
    upload_id: &str,
) -> usize {
    let source_request = wait_for_patch_agent_request(runtime, source_client).await;
    assert_eq!(
        source_request.kind,
        "file_read_project_artifact_export_chunk"
    );
    let source_payload: Value =
        serde_json::from_str(source_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(source_payload["path"], source_path);
    assert_eq!(source_payload["expected_file_bytes"], bytes.len());
    assert_eq!(source_payload["expected_sha256"], sha256_hex(bytes));
    assert_eq!(source_payload["offset"], offset);
    let requested = source_payload["length"].as_u64().unwrap() as usize;
    let next = (offset + requested).min(bytes.len());
    let segment = &bytes[offset..next];
    let content_base64 = general_purpose::STANDARD.encode(segment);
    complete_patch_agent_request(
        runtime,
        source_client,
        &source_request.request_id,
        0,
        &json!({
            "path": source_path,
            "file_bytes": bytes.len(),
            "offset": offset,
            "bytes_returned": segment.len(),
            "content_base64": content_base64,
            "next_offset": next,
            "truncated": next < bytes.len(),
            "eof": next == bytes.len(),
        })
        .to_string(),
        "",
    )
    .await;

    let destination_request = wait_for_patch_agent_request(runtime, destination_client).await;
    assert_eq!(destination_request.kind, "file_artifact_upload_chunk");
    let destination_payload: Value =
        serde_json::from_str(destination_request.content.as_deref().unwrap()).unwrap();
    assert_eq!(destination_payload["path"], destination_path);
    assert_eq!(destination_payload["upload_id"], upload_id);
    assert_eq!(destination_payload["offset"], offset);
    let uploaded = general_purpose::STANDARD
        .decode(destination_payload["content_base64"].as_str().unwrap())
        .unwrap();
    assert_eq!(uploaded, segment);
    complete_patch_agent_request(
        runtime,
        destination_client,
        &destination_request.request_id,
        0,
        &json!({
            "path": destination_path,
            "upload_id": upload_id,
            "received_bytes": next,
            "next_offset": next,
            "expected_bytes": bytes.len(),
            "expected_sha256": sha256_hex(bytes),
            "committed": false,
        })
        .to_string(),
        "",
    )
    .await;
    next
}

async fn complete_destination_finish(
    runtime: &super::super::ToolRuntime,
    client_id: &str,
    destination_path: &str,
    bytes: &[u8],
    mime_type: &str,
    upload_id: &str,
) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "file_artifact_upload_finish");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], destination_path);
    assert_eq!(payload["upload_id"], upload_id);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &json!({
            "path": destination_path,
            "upload_id": upload_id,
            "bytes": bytes.len(),
            "received_bytes": bytes.len(),
            "expected_bytes": bytes.len(),
            "expected_sha256": sha256_hex(bytes),
            "sha256": sha256_hex(bytes),
            "mime_type": mime_type,
            "committed": true,
        })
        .to_string(),
        "",
    )
    .await;
}

async fn complete_destination_abort(
    runtime: &super::super::ToolRuntime,
    client_id: &str,
    destination_path: &str,
    upload_id: &str,
) {
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.kind, "file_artifact_upload_abort");
    let payload: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(payload["path"], destination_path);
    assert_eq!(payload["upload_id"], upload_id);
    complete_patch_agent_request(
        runtime,
        client_id,
        &request.request_id,
        0,
        &json!({
            "path": destination_path,
            "upload_id": upload_id,
            "received_bytes": 0,
            "committed": false,
            "aborted": true,
            "final_file_exists": false,
        })
        .to_string(),
        "",
    )
    .await;
}

#[tokio::test]
async fn transfer_project_artifact_streams_markdown_across_runners() {
    let runtime = runtime_with_agent_project("transfer-runtime");
    register_agent(
        &runtime,
        "transfer-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "transfer-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let source_project = agent_test_project_id("transfer-source");
    let destination_project = agent_test_project_id("transfer-destination");
    let source_path = "paper/README.md";
    let destination_path = "artifacts/README.md";
    let bytes: Vec<u8> = (0..(super::super::INTERNAL_ARTIFACT_TRANSFER_CHUNK_BYTES + 17))
        .map(|index| b'a' + (index % 23) as u8)
        .collect();
    let expected_sha = sha256_hex(&bytes);
    let upload_id = "wc_upload_transfer_markdown";

    let auth = transfer_auth("alice");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let source_project = source_project.clone();
        let destination_project = destination_project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    source_project,
                    source_path.to_string(),
                    destination_project,
                    destination_path.to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });

    complete_source_metadata(
        &runtime,
        "transfer-source",
        source_path,
        &bytes,
        Some("text/markdown"),
    )
    .await;
    complete_destination_begin(
        &runtime,
        "transfer-destination",
        destination_path,
        &bytes,
        "text/markdown",
        false,
        upload_id,
    )
    .await;
    let mut offset = 0;
    let mut chunk_count = 0;
    while offset < bytes.len() {
        offset = complete_one_transfer_chunk(
            &runtime,
            "transfer-source",
            "transfer-destination",
            source_path,
            destination_path,
            &bytes,
            offset,
            upload_id,
        )
        .await;
        chunk_count += 1;
    }
    assert_eq!(
        chunk_count, 2,
        "1 MiB internal streaming should require two chunks"
    );

    complete_destination_finish(
        &runtime,
        "transfer-destination",
        destination_path,
        &bytes,
        "text/markdown",
        upload_id,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["source_project"], source_project);
    assert_eq!(result.output["destination_project"], destination_project);
    assert_eq!(result.output["bytes"], bytes.len());
    assert_eq!(result.output["sha256"], expected_sha);
    assert_eq!(result.output["mime_type"], "text/markdown");
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(!serialized.contains("content_base64"));
}

#[tokio::test]
async fn transfer_project_artifact_unknown_binary_preserves_generic_mime_and_overwrite_true() {
    let runtime = runtime_with_agent_project("transfer-binary");
    register_agent(
        &runtime,
        "binary-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "binary-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let source_project = agent_test_project_id("binary-source");
    let destination_project = agent_test_project_id("binary-destination");
    let bytes = b"custom-binary-payload".to_vec();
    let source_path = "data/data.customblob";
    let destination_path = "artifacts/data.customblob";
    let upload_id = "wc_upload_transfer_binary";
    let auth = transfer_auth("alice");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let source_project = source_project.clone();
        let destination_project = destination_project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    source_project,
                    source_path.to_string(),
                    destination_project,
                    destination_path.to_string(),
                    Some(true),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });

    complete_source_metadata(&runtime, "binary-source", source_path, &bytes, None).await;
    complete_destination_begin(
        &runtime,
        "binary-destination",
        destination_path,
        &bytes,
        "application/octet-stream",
        true,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "binary-source",
        "binary-destination",
        source_path,
        destination_path,
        &bytes,
        0,
        upload_id,
    )
    .await;
    complete_destination_finish(
        &runtime,
        "binary-destination",
        destination_path,
        &bytes,
        "application/octet-stream",
        upload_id,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["mime_type"], "application/octet-stream");
}

#[tokio::test]
async fn transfer_project_artifact_same_project_uses_existing_protocol() {
    let runtime = runtime_with_agent_project("same-transfer");
    register_agent(
        &runtime,
        "same-transfer",
        Some("alice"),
        transfer_caps(true, true),
    )
    .await;
    let project = agent_test_project_id("same-transfer");
    let bytes = b"same-project".to_vec();
    let source_path = "source.bin";
    let destination_path = "copy.bin";
    let upload_id = "wc_upload_same_project";
    let auth = transfer_auth("alice");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    project.clone(),
                    source_path.to_string(),
                    project,
                    destination_path.to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });

    complete_source_metadata(&runtime, "same-transfer", source_path, &bytes, None).await;
    complete_destination_begin(
        &runtime,
        "same-transfer",
        destination_path,
        &bytes,
        "application/octet-stream",
        false,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "same-transfer",
        "same-transfer",
        source_path,
        destination_path,
        &bytes,
        0,
        upload_id,
    )
    .await;
    complete_destination_finish(
        &runtime,
        "same-transfer",
        destination_path,
        &bytes,
        "application/octet-stream",
        upload_id,
    )
    .await;
    assert!(task.await.unwrap().success);
}

#[tokio::test]
async fn transfer_project_artifact_overwrite_false_is_definite_begin_failure() {
    let runtime = runtime_with_agent_project("overwrite-transfer");
    register_agent(
        &runtime,
        "overwrite-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "overwrite-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let source_project = agent_test_project_id("overwrite-source");
    let destination_project = agent_test_project_id("overwrite-destination");
    let bytes = b"payload".to_vec();
    let auth = transfer_auth("alice");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    source_project,
                    "source.bin".to_string(),
                    destination_project,
                    "existing.bin".to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });
    complete_source_metadata(&runtime, "overwrite-source", "source.bin", &bytes, None).await;
    let request = wait_for_patch_agent_request(&runtime, "overwrite-destination").await;
    assert_eq!(request.kind, "file_artifact_upload_begin");
    complete_patch_agent_request(
        &runtime,
        "overwrite-destination",
        &request.request_id,
        0,
        r#"{"path":"existing.bin","error":"file exists and overwrite is false","failure_kind":"policy_rejected"}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "destination_begin_failed");
    assert_eq!(result.output["outcome_unknown"], false);
}

#[tokio::test]
async fn transfer_project_artifact_source_missing_stops_before_destination_write() {
    let runtime = runtime_with_agent_project("missing-transfer");
    register_agent(
        &runtime,
        "missing-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "missing-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let auth = transfer_auth("alice");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    agent_test_project_id("missing-source"),
                    "missing.bin".to_string(),
                    agent_test_project_id("missing-destination"),
                    "copy.bin".to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "missing-source").await;
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    complete_patch_agent_request(
        &runtime,
        "missing-source",
        &request.request_id,
        0,
        r#"{"path":"missing.bin","error":"stat failed: not found"}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "source_read_failed");
    assert!(
        probe_agent_request_for_instance(&runtime, "missing-destination", "inst")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn transfer_project_artifact_source_snapshot_change_aborts_destination_upload() {
    let runtime = runtime_with_agent_project("snapshot-transfer");
    register_agent(
        &runtime,
        "snapshot-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "snapshot-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let source_project = agent_test_project_id("snapshot-source");
    let destination_project = agent_test_project_id("snapshot-destination");
    let bytes = b"original snapshot".to_vec();
    let upload_id = "wc_upload_snapshot_changed";
    let auth = transfer_auth("alice");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    source_project,
                    "source.bin".to_string(),
                    destination_project,
                    "copy.bin".to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });
    complete_source_metadata(&runtime, "snapshot-source", "source.bin", &bytes, None).await;
    complete_destination_begin(
        &runtime,
        "snapshot-destination",
        "copy.bin",
        &bytes,
        "application/octet-stream",
        false,
        upload_id,
    )
    .await;
    let request = wait_for_patch_agent_request(&runtime, "snapshot-source").await;
    assert_eq!(request.kind, "file_read_project_artifact_export_chunk");
    complete_patch_agent_request(
        &runtime,
        "snapshot-source",
        &request.request_id,
        0,
        &json!({
            "path": "source.bin",
            "error": "artifact snapshot changed",
            "error_kind": "snapshot_changed",
            "expected_sha256": sha256_hex(&bytes),
            "actual_sha256": "f".repeat(64),
        })
        .to_string(),
        "",
    )
    .await;
    complete_destination_abort(&runtime, "snapshot-destination", "copy.bin", upload_id).await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "source_snapshot_changed");
    assert_eq!(result.output["destination_upload_aborted"], true);
}

#[tokio::test]
async fn transfer_project_artifact_destination_sha_failure_aborts_known_upload() {
    let runtime = runtime_with_agent_project("sha-transfer");
    register_agent(
        &runtime,
        "sha-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "sha-destination",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let bytes = b"sha-protected".to_vec();
    let upload_id = "wc_upload_sha_mismatch";
    let auth = transfer_auth("alice");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .transfer_project_artifact(
                    agent_test_project_id("sha-source"),
                    "source.bin".to_string(),
                    agent_test_project_id("sha-destination"),
                    "copy.bin".to_string(),
                    Some(false),
                    Some(&auth),
                    SessionTransport::Api,
                )
                .await
        }
    });
    complete_source_metadata(&runtime, "sha-source", "source.bin", &bytes, None).await;
    complete_destination_begin(
        &runtime,
        "sha-destination",
        "copy.bin",
        &bytes,
        "application/octet-stream",
        false,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "sha-source",
        "sha-destination",
        "source.bin",
        "copy.bin",
        &bytes,
        0,
        upload_id,
    )
    .await;
    let finish = wait_for_patch_agent_request(&runtime, "sha-destination").await;
    assert_eq!(finish.kind, "file_artifact_upload_finish");
    complete_patch_agent_request(
        &runtime,
        "sha-destination",
        &finish.request_id,
        0,
        &json!({
            "path": "copy.bin",
            "upload_id": upload_id,
            "received_bytes": bytes.len(),
            "expected_bytes": bytes.len(),
            "expected_sha256": sha256_hex(&bytes),
            "sha256": "0".repeat(64),
            "committed": false,
            "error": "uploaded sha256 does not match expected_sha256",
        })
        .to_string(),
        "",
    )
    .await;
    complete_destination_abort(&runtime, "sha-destination", "copy.bin", upload_id).await;
    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "destination_finish_failed");
    assert_eq!(result.output["destination_upload_aborted"], true);
}

#[tokio::test]
async fn transfer_project_artifact_independently_authorizes_source_and_destination() {
    let runtime = runtime_with_agent_project("auth-transfer");
    register_agent(
        &runtime,
        "auth-source-alice",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "auth-destination-bob",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let bob = transfer_auth("bob");
    let source_denied = runtime
        .transfer_project_artifact(
            agent_test_project_id("auth-source-alice"),
            "source.bin".to_string(),
            agent_test_project_id("auth-destination-bob"),
            "copy.bin".to_string(),
            Some(false),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!source_denied.success);

    let runtime = runtime_with_agent_project("auth-transfer-2");
    register_agent(
        &runtime,
        "auth-source-bob",
        Some("bob"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "auth-destination-alice",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    let destination_denied = runtime
        .transfer_project_artifact(
            agent_test_project_id("auth-source-bob"),
            "source.bin".to_string(),
            agent_test_project_id("auth-destination-alice"),
            "copy.bin".to_string(),
            Some(false),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!destination_denied.success);
    assert!(
        probe_agent_request_for_instance(&runtime, "auth-source-bob", "inst")
            .await
            .is_none()
    );
}

#[tokio::test]
async fn accept_artifact_handoff_imports_one_exact_snapshot_with_bounded_provenance() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-happy");
    register_agent(
        &runtime,
        "handoff-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "handoff-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let source_path = "paper/README.md";
    let destination_path = "artifacts/README.md";
    let bytes = b"handoff payload\n".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "handoff-source",
        "handoff-destination",
        source_path,
        &bytes,
        "text/markdown",
        true,
    );
    let upload_id = "wc_upload_handoff_happy";

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let grant_id = grant.grant_id.clone();
        let destination_project = grant.destination_project.clone();
        let bob = bob.clone();
        async move {
            runtime
                .accept_artifact_handoff(
                    grant_id,
                    destination_project,
                    destination_path.to_string(),
                    Some(false),
                    "accept-happy".to_string(),
                    Some(&bob),
                    SessionTransport::Api,
                )
                .await
        }
    });

    complete_source_metadata(
        &runtime,
        "handoff-source",
        source_path,
        &bytes,
        Some("text/markdown"),
    )
    .await;
    complete_destination_begin(
        &runtime,
        "handoff-destination",
        destination_path,
        &bytes,
        "text/markdown",
        false,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "handoff-source",
        "handoff-destination",
        source_path,
        destination_path,
        &bytes,
        0,
        upload_id,
    )
    .await;
    complete_destination_finish(
        &runtime,
        "handoff-destination",
        destination_path,
        &bytes,
        "text/markdown",
        upload_id,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{result:?}");
    assert_eq!(result.output["grant_id"], grant.grant_id);
    assert_eq!(result.output["replayed"], false);
    assert_eq!(
        result.output["destination_project"],
        grant.destination_project
    );
    assert_eq!(result.output["destination_path"], destination_path);
    assert_eq!(result.output["bytes"], bytes.len());
    assert_eq!(result.output["sha256"], sha256_hex(&bytes));
    assert_eq!(
        result.output["provenance"]["source_project"],
        grant.source_project
    );
    assert_eq!(result.output["provenance"]["source_path"], source_path);
    assert_eq!(result.output["provenance"]["source_name"], "README.md");

    let replay = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            grant.destination_project.clone(),
            destination_path.to_string(),
            Some(false),
            "accept-happy".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(replay.success, "{replay:?}");
    assert_eq!(
        replay.output["acceptance_id"],
        result.output["acceptance_id"]
    );
    assert_eq!(replay.output["replayed"], true);

    let consumed = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            grant.destination_project.clone(),
            destination_path.to_string(),
            Some(false),
            "accept-new-key".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!consumed.success, "{consumed:?}");
    assert_eq!(
        consumed.output["error_kind"],
        "artifact_handoff_unavailable"
    );

    let conflicting = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            grant.destination_project.clone(),
            "artifacts/other.md".to_string(),
            Some(false),
            "accept-happy".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!conflicting.success, "{conflicting:?}");
    assert_eq!(
        conflicting.output["error_kind"],
        "artifact_handoff_idempotency_conflict"
    );

    let foreign_read = runtime
        .read_project_artifact_export_metadata_internal(
            &grant.source_project,
            source_path,
            Some(&bob),
        )
        .await;
    assert!(!foreign_read.success, "{foreign_read:?}");
    assert!(
        probe_agent_request_for_instance(&runtime, "handoff-source", "inst")
            .await
            .is_none(),
        "provenance must not trigger an ambient source read"
    );
}

#[tokio::test]
async fn accept_artifact_handoff_fails_stale_before_destination_write() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-stale");
    register_agent(
        &runtime,
        "stale-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "stale-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let expected = b"expected bytes".to_vec();
    let changed = b"changed bytes".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "stale-source",
        "stale-destination",
        "paper/result.bin",
        &expected,
        "application/octet-stream",
        true,
    );

    let task = spawn_accept(
        &runtime,
        &grant,
        "artifacts/result.bin",
        false,
        "accept-stale",
        &bob,
    );
    complete_source_metadata(
        &runtime,
        "stale-source",
        "paper/result.bin",
        &changed,
        Some("application/octet-stream"),
    )
    .await;
    let result = task.await.unwrap();
    assert!(!result.success, "{result:?}");
    assert_eq!(result.output["error_kind"], "artifact_handoff_source_stale");
    assert!(
        probe_agent_request_for_instance(&runtime, "stale-destination", "inst")
            .await
            .is_none(),
        "stale source must fail before destination upload"
    );
}

#[tokio::test]
async fn accept_artifact_handoff_rejects_wrong_principal_project_and_revoked_grant() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-denied");
    register_agent(
        &runtime,
        "denied-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "denied-destination-bob",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    register_agent(
        &runtime,
        "denied-destination-alice",
        Some("alice"),
        transfer_caps(false, true),
    )
    .await;
    register_agent(
        &runtime,
        "denied-destination-other",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let bytes = b"denied payload".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "denied-source",
        "denied-destination-bob",
        "paper/secret.bin",
        &bytes,
        "application/octet-stream",
        true,
    );

    let wrong_principal = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            agent_test_project_id("denied-destination-alice"),
            "artifacts/secret.bin".to_string(),
            Some(false),
            "accept-wrong-principal".to_string(),
            Some(&alice),
            SessionTransport::Api,
        )
        .await;
    assert!(!wrong_principal.success, "{wrong_principal:?}");
    assert_eq!(
        wrong_principal.output["error_kind"],
        "artifact_handoff_unavailable"
    );

    let wrong_project = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            agent_test_project_id("denied-destination-other"),
            "artifacts/secret.bin".to_string(),
            Some(false),
            "accept-wrong-project".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!wrong_project.success, "{wrong_project:?}");
    assert_eq!(
        wrong_project.output["error_kind"],
        "artifact_handoff_unavailable"
    );

    db.revoke_artifact_handoff_grant(
        &handoff_principal(&alice),
        &grant.source_project,
        &grant.grant_id,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    let revoked = runtime
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            grant.destination_project.clone(),
            "artifacts/secret.bin".to_string(),
            Some(false),
            "accept-revoked".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(!revoked.success, "{revoked:?}");
    assert_eq!(revoked.output["error_kind"], "artifact_handoff_unavailable");
    assert!(
        probe_agent_request_for_instance(&runtime, "denied-source", "inst")
            .await
            .is_none(),
        "denied accepts must not read the source"
    );
}

#[tokio::test]
async fn accept_artifact_handoff_definite_begin_failure_does_not_reconcile_preexisting_match() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-existing");
    register_agent(
        &runtime,
        "existing-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "existing-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let bytes = b"already present payload".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "existing-source",
        "existing-destination",
        "paper/existing.bin",
        &bytes,
        "application/octet-stream",
        true,
    );
    let destination_path = "artifacts/existing.bin";

    for attempt in 0..2 {
        let task = spawn_accept(
            &runtime,
            &grant,
            destination_path,
            false,
            "accept-existing",
            &bob,
        );
        complete_source_metadata(
            &runtime,
            "existing-source",
            "paper/existing.bin",
            &bytes,
            Some("application/octet-stream"),
        )
        .await;
        let request = wait_for_patch_agent_request(&runtime, "existing-destination").await;
        assert_eq!(
            request.kind, "file_artifact_upload_begin",
            "attempt {attempt} must retry overwrite=false admission instead of treating a pre-existing matching file as this handoff's committed outcome"
        );
        complete_patch_agent_request(
            &runtime,
            "existing-destination",
            &request.request_id,
            0,
            r#"{"path":"artifacts/existing.bin","error":"file exists and overwrite is false","failure_kind":"policy_rejected"}"#,
            "",
        )
        .await;
        let result = task.await.unwrap();
        assert!(!result.success, "{result:?}");
        assert_eq!(
            result.output["error_kind"],
            "artifact_handoff_transfer_failed"
        );
        assert_eq!(result.output["outcome_unknown"], false);
    }
}

#[tokio::test]
async fn accept_artifact_handoff_definite_finish_failure_does_not_enable_reconciliation() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-finish-failed");
    register_agent(
        &runtime,
        "finish-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "finish-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let bytes = b"definite finish failure payload".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "finish-source",
        "finish-destination",
        "paper/finish.bin",
        &bytes,
        "application/octet-stream",
        true,
    );
    let destination_path = "artifacts/finish.bin";
    let upload_id = "wc_upload_handoff_finish_failed";

    let first = spawn_accept(
        &runtime,
        &grant,
        destination_path,
        true,
        "accept-finish-failed",
        &bob,
    );
    complete_source_metadata(
        &runtime,
        "finish-source",
        "paper/finish.bin",
        &bytes,
        Some("application/octet-stream"),
    )
    .await;
    complete_destination_begin(
        &runtime,
        "finish-destination",
        destination_path,
        &bytes,
        "application/octet-stream",
        true,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "finish-source",
        "finish-destination",
        "paper/finish.bin",
        destination_path,
        &bytes,
        0,
        upload_id,
    )
    .await;
    let finish = wait_for_patch_agent_request(&runtime, "finish-destination").await;
    assert_eq!(finish.kind, "file_artifact_upload_finish");
    complete_patch_agent_request(
        &runtime,
        "finish-destination",
        &finish.request_id,
        0,
        &format!(
            r#"{{"path":"{destination_path}","upload_id":"{upload_id}","committed":false,"error":"finish rejected","failure_kind":"policy_rejected"}}"#
        ),
        "",
    )
    .await;
    complete_destination_abort(&runtime, "finish-destination", destination_path, upload_id).await;
    let failed = first.await.unwrap();
    assert!(!failed.success, "{failed:?}");
    assert_eq!(
        failed.output["error_kind"],
        "artifact_handoff_transfer_failed"
    );
    assert_eq!(failed.output["outcome_unknown"], false);

    let retry = spawn_accept(
        &runtime,
        &grant,
        destination_path,
        true,
        "accept-finish-failed",
        &bob,
    );
    complete_source_metadata(
        &runtime,
        "finish-source",
        "paper/finish.bin",
        &bytes,
        Some("application/octet-stream"),
    )
    .await;
    let next = wait_for_patch_agent_request(&runtime, "finish-destination").await;
    assert_eq!(
        next.kind, "file_artifact_upload_begin",
        "definite finish failure must retry transfer rather than reconcile arbitrary destination bytes"
    );
    complete_patch_agent_request(
        &runtime,
        "finish-destination",
        &next.request_id,
        0,
        r#"{"path":"artifacts/finish.bin","error":"retry stopped","failure_kind":"policy_rejected"}"#,
        "",
    )
    .await;
    let retry_result = retry.await.unwrap();
    assert!(!retry_result.success, "{retry_result:?}");
    assert_eq!(retry_result.output["outcome_unknown"], false);
}

#[tokio::test]
async fn accept_artifact_handoff_reconciles_unknown_finish_without_second_import() {
    let (_temp, db, runtime) = runtime_with_handoff_db("handoff-unknown");
    register_agent(
        &runtime,
        "unknown-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &runtime,
        "unknown-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let bytes = b"unknown outcome payload".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "unknown-source",
        "unknown-destination",
        "paper/unknown.bin",
        &bytes,
        "application/octet-stream",
        true,
    );
    let upload_id = "wc_upload_handoff_unknown";

    let first = spawn_accept(
        &runtime,
        &grant,
        "artifacts/unknown.bin",
        false,
        "accept-unknown",
        &bob,
    );
    complete_source_metadata(
        &runtime,
        "unknown-source",
        "paper/unknown.bin",
        &bytes,
        Some("application/octet-stream"),
    )
    .await;
    complete_destination_begin(
        &runtime,
        "unknown-destination",
        "artifacts/unknown.bin",
        &bytes,
        "application/octet-stream",
        false,
        upload_id,
    )
    .await;
    complete_one_transfer_chunk(
        &runtime,
        "unknown-source",
        "unknown-destination",
        "paper/unknown.bin",
        "artifacts/unknown.bin",
        &bytes,
        0,
        upload_id,
    )
    .await;
    let finish = wait_for_patch_agent_request(&runtime, "unknown-destination").await;
    assert_eq!(finish.kind, "file_artifact_upload_finish");
    complete_patch_agent_request(
        &runtime,
        "unknown-destination",
        &finish.request_id,
        0,
        "{}",
        "",
    )
    .await;
    let unknown = first.await.unwrap();
    assert!(!unknown.success, "{unknown:?}");
    assert_eq!(
        unknown.output["error_kind"],
        "artifact_handoff_outcome_unknown"
    );
    assert_eq!(unknown.output["outcome_unknown"], true);

    let retry = spawn_accept(
        &runtime,
        &grant,
        "artifacts/unknown.bin",
        false,
        "accept-unknown",
        &bob,
    );
    complete_source_metadata(
        &runtime,
        "unknown-source",
        "paper/unknown.bin",
        &bytes,
        Some("application/octet-stream"),
    )
    .await;
    complete_destination_metadata(
        &runtime,
        "unknown-destination",
        "artifacts/unknown.bin",
        &bytes,
        "application/octet-stream",
    )
    .await;
    let recovered = retry.await.unwrap();
    assert!(recovered.success, "{recovered:?}");
    assert_eq!(recovered.output["replayed"], true);
    assert_eq!(
        recovered.output["acceptance_id"],
        unknown.output["acceptance_id"]
    );
    assert!(
        probe_agent_request_for_instance(&runtime, "unknown-destination", "inst")
            .await
            .is_none(),
        "matching destination reconciliation must not start another upload"
    );
}

#[tokio::test]
async fn accept_artifact_handoff_replays_completed_result_after_restart() {
    let (_temp, db, first_runtime) = runtime_with_handoff_db("handoff-restart-first");
    register_agent(
        &first_runtime,
        "restart-source",
        Some("alice"),
        transfer_caps(true, false),
    )
    .await;
    register_agent(
        &first_runtime,
        "restart-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let alice = transfer_auth("alice");
    let bob = transfer_auth("bob");
    let bytes = b"restart payload".to_vec();
    let grant = create_handoff_grant(
        &db,
        &alice,
        &bob,
        "restart-source",
        "restart-destination",
        "paper/restart.bin",
        &bytes,
        "application/octet-stream",
        true,
    );
    let destination_path = "artifacts/restart.bin";
    let request = crate::db::ArtifactHandoffImportRequest {
        grant_id: grant.grant_id.clone(),
        destination_project: grant.destination_project.clone(),
        destination_path: destination_path.to_string(),
        overwrite: false,
    };
    let claim = db
        .begin_artifact_handoff_import(
            &handoff_principal(&bob),
            &request,
            "accept-restart",
            chrono::Utc::now().timestamp_millis(),
        )
        .unwrap();
    db.mark_artifact_handoff_acceptance_destination_reconcile_allowed(
        &handoff_principal(&bob),
        &grant.destination_project,
        &grant.grant_id,
        &claim.acceptance.acceptance_id,
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    db.complete_artifact_handoff_acceptance(
        &handoff_principal(&bob),
        &grant.destination_project,
        &grant.grant_id,
        &claim.acceptance.acceptance_id,
        crate::db::ArtifactHandoffAcceptanceOutcome {
            destination_path: destination_path.to_string(),
            destination_bytes: bytes.len() as u64,
            destination_sha256: sha256_hex(&bytes),
        },
        chrono::Utc::now().timestamp_millis(),
    )
    .unwrap();
    drop(first_runtime);

    let restarted = runtime_with_agent_project("handoff-restart-second")
        .with_communication_database(db.clone());
    register_agent(
        &restarted,
        "restart-destination",
        Some("bob"),
        transfer_caps(false, true),
    )
    .await;
    let replay = restarted
        .accept_artifact_handoff(
            grant.grant_id.clone(),
            grant.destination_project.clone(),
            destination_path.to_string(),
            Some(false),
            "accept-restart".to_string(),
            Some(&bob),
            SessionTransport::Api,
        )
        .await;
    assert!(replay.success, "{replay:?}");
    assert_eq!(replay.output["replayed"], true);
    assert_eq!(
        replay.output["acceptance_id"],
        claim.acceptance.acceptance_id
    );
    assert!(
        probe_agent_request_for_instance(&restarted, "restart-destination", "inst")
            .await
            .is_none(),
        "completed replay must not touch either Runner"
    );
}
