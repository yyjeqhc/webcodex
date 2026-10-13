use super::support::artifacts::{
    complete_artifact_chunk as complete_chunk, setup_artifact_reader as setup,
};
use super::support::*;
use serde_json::json;
use sha2::{Digest, Sha256};

#[tokio::test]
async fn docx_document_opens_without_git_or_session_and_pins_header_to_metadata() {
    let (runtime, auth, project) = setup("docx-open").await;
    let data = b"PK\x03\x04unchanged document";
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .present_docx(project, "reports/unchanged.docx".into(), Some(&auth))
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "docx-open").await;
    assert_eq!(request.kind, "file_read_project_artifact_metadata");
    complete_patch_agent_request(&runtime, "docx-open", &request.request_id, 0,
        &json!({"path":"reports/unchanged.docx", "bytes":data.len(), "sha256":format!("{:x}",Sha256::digest(data)), "mime_type":"application/vnd.openxmlformats-officedocument.wordprocessingml.document"}).to_string(), "").await;
    complete_chunk(
        &runtime,
        "docx-open",
        "reports/unchanged.docx",
        data,
        0,
        4,
        false,
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["docx_document"]["name"], "unchanged.docx");
    assert_eq!(result.output["docx_document"]["bytes"], data.len());
    assert!(!result.output.to_string().contains("content_base64"));
}

#[tokio::test]
async fn docx_reuses_common_artifact_chunk_version_fence() {
    let (runtime, auth, project) = setup("docx-read").await;
    let data = b"PK\x03\x04unchanged document";
    for stale in [false, true] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            let project = project.clone();
            async move {
                runtime
                    .read_app_artifact_chunk(
                        project,
                        "report.docx".into(),
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
            "docx-read",
            "report.docx",
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
async fn docx_document_rejects_foreign_principal_and_unsafe_paths_before_read() {
    let (runtime, _, project) = setup("docx-owner").await;
    let foreign = managed_oauth_auth_context("bob", None);
    let result = runtime
        .read_app_artifact_chunk(
            project.clone(),
            "report.docx".into(),
            "a".repeat(64),
            10,
            0,
            Some(&foreign),
        )
        .await;
    assert!(!result.success);
    assert!(
        probe_agent_request_for_instance(&runtime, "docx-owner", "inst")
            .await
            .is_none()
    );
    for path in [
        "../escape.docx",
        ".git/secret.docx",
        "C:/private.docx",
        "notes.txt",
    ] {
        assert!(
            !runtime
                .present_docx(project.clone(), path.into(), Some(&foreign))
                .await
                .success
        );
    }
}

#[tokio::test]
async fn docx_common_artifact_read_requires_app_capability_even_with_read_scope() {
    let runtime = test_runtime();
    let auth = managed_oauth_auth_context("alice", None);
    let result = runtime.call_tool_with_context(crate::tool_runtime::kernel::ToolCallRequest {
        tool_name: "read_app_artifact_chunk".into(), arguments: json!({"project":"agent:docx:demo", "path":"report.docx", "sha256":"a".repeat(64), "bytes":10, "byte_offset":0})
    }, crate::tool_runtime::kernel::ToolCallContext { transport: crate::tool_runtime::kernel::ToolTransport::Api,
        session_id:None, auth:Some(&auth), window:None, record_oauth_scope_denials:false, host_file_import_trust: crate::tool_runtime::kernel::HostFileImportTrust::Untrusted }).await;
    assert!(!result.success);
    assert!(
        matches!(result.error_status, Some(crate::tool_runtime::kernel::ToolCallErrorStatus::InvalidArguments { message }) if message.contains("App capability"))
    );
}

#[tokio::test]
async fn docx_common_artifact_capability_never_replaces_project_read_scope() {
    use crate::tool_runtime::kernel::*;
    let (runtime, mut auth, project) = setup("docx-scope").await;
    auth.scopes = vec![crate::auth::SCOPE_RUNTIME_READ.to_string()];
    let outcome = runtime.call_tool_with_invocation_metadata(ToolCallRequest {
        tool_name: "read_app_artifact_chunk".into(), arguments: json!({"project":project,"path":"report.docx","sha256":"a".repeat(64),"bytes":10,"byte_offset":0}),
    }, ToolCallContext { transport:ToolTransport::Mcp, session_id:None, auth:Some(&auth), window:None,
        record_oauth_scope_denials:false, host_file_import_trust:HostFileImportTrust::Untrusted },
        ToolInvocationMetadata::default(), ToolProtocolCapabilities {artifact_app:true,..Default::default()}).await;
    assert!(!outcome.success);
    assert!(matches!(
        outcome.error_status,
        Some(ToolCallErrorStatus::InsufficientScope { .. })
    ));
    assert!(
        probe_agent_request_for_instance(&runtime, "docx-scope", "inst")
            .await
            .is_none()
    );
}
