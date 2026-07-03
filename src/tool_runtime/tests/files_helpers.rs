//! Agent-native artifact file-op routing tests.

use super::super::files::*;
use super::super::types::ToolCall;
use super::support::*;
use crate::shell_protocol::ShellClientCapabilities;
use serde_json::json;

#[tokio::test]
async fn save_project_artifact_routes_to_agent_file_op() {
    let runtime = runtime_with_agent_project("artifact-save");
    register_agent(
        &runtime,
        "artifact-save",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-save");
    let content_base64 = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        [0x89, b'P', b'N', b'G'],
    );

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let content_base64 = content_base64.clone();
        async move {
            runtime
                .save_project_artifact(
                    project,
                    "artifacts/imports/tiny.png".to_string(),
                    content_base64,
                    Some("image/png".to_string()),
                    Some(false),
                )
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "artifact-save")
        .await
        .expect("save_project_artifact should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_save_project_artifact");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["path"], "artifacts/imports/tiny.png");
    assert_eq!(payload["content_base64"], content_base64);
    assert_eq!(payload["mime_type"], "image/png");
    assert_eq!(payload["overwrite"], false);
    assert_eq!(payload["max_bytes"], MAX_PROJECT_ARTIFACT_BYTES);

    complete_patch_agent_request(
        &runtime,
        "artifact-save",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/tiny.png","bytes_written":4,"sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","mime_type":"image/png"}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["bytes_written"], 4);
    assert_eq!(result.output["mime_type"], "image/png");
}

#[tokio::test]
async fn read_project_artifact_metadata_routes_to_agent_file_op() {
    let runtime = runtime_with_agent_project("artifact-meta");
    register_agent(
        &runtime,
        "artifact-meta",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-meta");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .read_project_artifact_metadata(project, "sample.zip".to_string())
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "artifact-meta")
        .await
        .expect("read_project_artifact_metadata should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_read_project_artifact_metadata");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({"path":"sample.zip","max_bytes":MAX_PROJECT_ARTIFACT_BYTES})
    );

    complete_patch_agent_request(
        &runtime,
        "artifact-meta",
        &req.request_id,
        0,
        r#"{"path":"sample.zip","bytes":212,"sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","mime_type":"application/zip","archive_entries_count":2}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["mime_type"], "application/zip");
    assert_eq!(result.output["archive_entries_count"], 2);
}

#[tokio::test]
async fn read_project_artifact_routes_to_agent_file_op() {
    let runtime = runtime_with_agent_project("artifact-read");
    register_agent(
        &runtime,
        "artifact-read",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-read");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .read_project_artifact(
                    project,
                    "data.bin".to_string(),
                    None,
                    Some(5),
                    Some(7),
                    None,
                )
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "artifact-read")
        .await
        .expect("read_project_artifact should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_read_project_artifact");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({"path":"data.bin","offset":5,"length":7,"max_file_bytes":MAX_PROJECT_ARTIFACT_BYTES})
    );

    complete_patch_agent_request(
        &runtime,
        "artifact-read",
        &req.request_id,
        0,
        r#"{"path":"data.bin","mime_type":null,"file_bytes":12,"sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","offset":5,"bytes_returned":7,"content_base64":"ZmdoaWprbA==","next_offset":12,"truncated":false}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["offset"], 5);
    assert_eq!(result.output["bytes_returned"], 7);
    assert_eq!(result.output["content_base64"], "ZmdoaWprbA==");
}

#[tokio::test]
async fn project_artifact_tools_require_file_capabilities() {
    let runtime = runtime_with_agent_project("artifact-caps");
    register_agent(
        &runtime,
        "artifact-caps",
        None,
        ShellClientCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-caps");
    let bootstrap = auth_context(None, true);

    let save = runtime
        .dispatch_with_auth(
            ToolCall::SaveProjectArtifact {
                project: project.clone(),
                path: "artifact.bin".to_string(),
                content_base64: "YQ==".to_string(),
                session_id: None,
                mime_type: Some("text/plain".to_string()),
                overwrite: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!save.success);
    assert!(save.error.unwrap().contains("file_write"));

    let read = runtime
        .dispatch_with_auth(
            ToolCall::ReadProjectArtifact {
                project,
                path: "artifact.bin".to_string(),
                session_id: None,
                encoding: None,
                offset: None,
                length: None,
                max_bytes: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!read.success);
    assert!(read.error.unwrap().contains("file_read"));
}
