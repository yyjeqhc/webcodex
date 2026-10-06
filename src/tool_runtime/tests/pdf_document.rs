use super::super::*;
use super::support::*;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

async fn setup(client: &str) -> (ToolRuntime, crate::auth::AuthContext, String) {
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

async fn complete_chunk(
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

#[tokio::test]
async fn pdf_document_opens_without_git_or_session_and_pins_header_to_metadata() {
    let (runtime, auth, project) = setup("pdf-open").await;
    let data = b"%PDF-1.7\nunchanged document";
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .present_pdf(project, "reports/unchanged.pdf".into(), Some(&auth))
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "pdf-open").await;
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    complete_patch_agent_request(&runtime, "pdf-open", &request.request_id, 0,
        &json!({"path":"reports/unchanged.pdf", "bytes":data.len(), "sha256":format!("{:x}",Sha256::digest(data)), "mime_type":"application/pdf"}).to_string(), "").await;
    complete_chunk(
        &runtime,
        "pdf-open",
        "reports/unchanged.pdf",
        data,
        0,
        5,
        false,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["pdf_document"]["name"], "unchanged.pdf");
    assert_eq!(result.output["pdf_document"]["bytes"], data.len());
    assert!(!result.output.to_string().contains("content_base64"));
}

#[tokio::test]
async fn app_artifact_chunk_completes_small_document_and_hides_changed_version_proof() {
    let (runtime, auth, project) = setup("pdf-read").await;
    let data = b"%PDF-1.7\nunchanged document";
    for stale in [false, true] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let project = project.clone();
            async move {
                runtime
                    .read_app_artifact_chunk(
                        project,
                        "report.pdf".into(),
                        format!("{:x}", Sha256::digest(data)),
                        data.len(),
                        0,
                        Some(&auth),
                    )
                    .await
            }
        });
        complete_chunk(
            &runtime,
            "pdf-read",
            "report.pdf",
            data,
            0,
            data.len(),
            stale,
        )
        .await;
        let result = task.await.unwrap();
        if stale {
            assert!(!result.success);
            assert_eq!(result.output["error_kind"], "snapshot_changed");
            assert!(!result.output.to_string().contains("private-current-digest"));
        } else {
            assert!(result.success, "{:?}", result.error);
            assert_eq!(result.output["artifact_chunk"]["complete"], true);
        }
    }
}

#[tokio::test]
async fn pdf_document_rejects_foreign_principal_and_unsafe_paths_before_read() {
    let (runtime, _, project) = setup("pdf-owner").await;
    let foreign = managed_oauth_auth_context("bob", None);
    let result = runtime
        .read_app_artifact_chunk(
            project.clone(),
            "report.pdf".into(),
            "a".repeat(64),
            10,
            0,
            Some(&foreign),
        )
        .await;
    assert!(!result.success);
    assert!(
        probe_agent_request_for_instance(&runtime, "pdf-owner", "inst")
            .await
            .is_none()
    );
    for path in [
        "../escape.pdf",
        ".git/secret.pdf",
        "C:/private.pdf",
        "notes.txt",
    ] {
        assert!(
            !runtime
                .present_pdf(project.clone(), path.into(), Some(&foreign))
                .await
                .success
        );
    }
}

#[tokio::test]
async fn app_artifact_chunk_uses_bounded_host_segment_and_preserves_version_fence() {
    let (runtime, auth, project) = setup("app-artifact-read").await;
    let data = vec![b'x'; super::super::pdf_document::APP_ARTIFACT_CHUNK_BYTES + 17];
    let sha256 = format!("{:x}", Sha256::digest(&data));
    for stale in [false, true] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let project = project.clone();
            let sha256 = sha256.clone();
            let bytes = data.len();
            async move {
                runtime
                    .read_app_artifact_chunk(
                        project,
                        "report.pdf".into(),
                        sha256,
                        bytes,
                        0,
                        Some(&auth),
                    )
                    .await
            }
        });
        complete_chunk(
            &runtime,
            "app-artifact-read",
            "report.pdf",
            &data,
            0,
            super::super::pdf_document::APP_ARTIFACT_CHUNK_BYTES,
            stale,
        )
        .await;
        let result = task.await.unwrap();
        if stale {
            assert!(!result.success);
            assert_eq!(result.output["error_kind"], "snapshot_changed");
            assert!(!result.output.to_string().contains("private-current-digest"));
        } else {
            assert!(result.success, "{:?}", result.error);
            assert_eq!(result.output["artifact_chunk"]["complete"], false);
            assert_eq!(
                result.output["artifact_chunk"]["next_byte_offset"],
                super::super::pdf_document::APP_ARTIFACT_CHUNK_BYTES
            );
        }
    }
}
#[tokio::test]
async fn app_artifact_chunk_requires_app_capability_even_with_read_scope() {
    let runtime = test_runtime();
    let auth = managed_oauth_auth_context("alice", None);
    let result = runtime.call_tool_with_context(crate::tool_runtime::kernel::ToolCallRequest {
        tool_name: "read_app_artifact_chunk".into(), arguments: json!({"project":"agent:pdf:demo", "path":"report.pdf", "sha256":"a".repeat(64), "bytes":10, "byte_offset":0})
    }, crate::tool_runtime::kernel::ToolCallContext { transport: crate::tool_runtime::kernel::ToolTransport::Api,
        session_id:None, auth:Some(&auth), window:None, record_oauth_scope_denials:false, host_file_import_trust: crate::tool_runtime::kernel::HostFileImportTrust::Untrusted }).await;
    assert!(!result.success);
    assert!(
        matches!(result.error_status, Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments { message }) if message.contains("App capability"))
    );
}

#[tokio::test]
async fn app_artifact_chunk_capability_never_replaces_project_read_scope() {
    use crate::tool_runtime::kernel::*;
    let (runtime, mut auth, project) = setup("pdf-scope").await;
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
    let outcome = runtime.call_tool_with_invocation_metadata(ToolCallRequest {
        tool_name: "read_app_artifact_chunk".into(), arguments: json!({"project":project,"path":"report.pdf","sha256":"a".repeat(64),"bytes":10,"byte_offset":0}),
    }, ToolCallContext { transport:ToolTransport::Mcp, session_id:None, auth:Some(&auth), window:None,
        record_oauth_scope_denials:false, host_file_import_trust:HostFileImportTrust::Untrusted },
        ToolInvocationMetadata::default(), ToolProtocolCapabilities {artifact_app:true,..Default::default()}).await;
    assert!(!outcome.success);
    assert!(matches!(
        outcome.error_status,
        Some(ToolCallErrorStatus::InsufficientScope { .. })
    ));
    assert!(
        probe_agent_request_for_instance(&runtime, "pdf-scope", "inst")
            .await
            .is_none()
    );
}
