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
                .read_project_artifact_metadata(project, "sample.zip".to_string(), None)
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
        json!({"path":"sample.zip","max_bytes":MAX_PROJECT_ARTIFACT_BYTES,"allow_missing":false})
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
async fn read_project_artifact_metadata_allow_missing_routes_to_agent_file_op() {
    let runtime = runtime_with_agent_project("artifact-meta-missing");
    register_agent(
        &runtime,
        "artifact-meta-missing",
        None,
        ShellClientCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-meta-missing");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .read_project_artifact_metadata(
                    project,
                    "artifacts/smoke/missing.artifact".to_string(),
                    Some(true),
                )
                .await
        }
    });

    let req = next_patch_agent_request(&runtime, "artifact-meta-missing")
        .await
        .expect("read_project_artifact_metadata should enqueue an artifact file-op");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["allow_missing"], true);

    complete_patch_agent_request(
        &runtime,
        "artifact-meta-missing",
        &req.request_id,
        0,
        r#"{"path":"artifacts/smoke/missing.artifact","exists":false,"missing":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["exists"], false);
    assert_eq!(result.output["missing"], true);
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
        r#"{"path":"data.bin","mime_type":null,"file_bytes":12,"sha256":"cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc","offset":5,"bytes_returned":7,"content_base64":"ZmdoaWprbA==","next_offset":12,"truncated":false,"eof":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["offset"], 5);
    assert_eq!(result.output["bytes_returned"], 7);
    assert_eq!(result.output["content_base64"], "ZmdoaWprbA==");
    assert_eq!(result.output["eof"], true);
}

#[tokio::test]
async fn artifact_upload_tools_route_to_agent_file_ops() {
    let runtime = runtime_with_agent_project("artifact-upload");
    register_agent(
        &runtime,
        "artifact-upload",
        None,
        ShellClientCapabilities {
            file_write: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-upload");
    let path = "artifacts/imports/sample.zip".to_string();
    let expected_sha256 = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd";

    let begin_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let path = path.clone();
        async move {
            runtime
                .artifact_upload_begin(
                    project,
                    path,
                    Some(5),
                    Some(expected_sha256.to_string()),
                    Some("application/zip".to_string()),
                    Some(false),
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "artifact-upload")
        .await
        .expect("artifact_upload_begin should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_artifact_upload_begin");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["expected_bytes"], 5);
    assert_eq!(payload["expected_sha256"], expected_sha256);
    assert_eq!(payload["mime_type"], "application/zip");
    assert_eq!(payload["overwrite"], false);
    assert_eq!(payload["max_bytes"], MAX_PROJECT_ARTIFACT_BYTES);
    complete_patch_agent_request(
        &runtime,
        "artifact-upload",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_1","received_bytes":0,"next_offset":0,"expected_bytes":5,"expected_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","max_bytes":10485760,"mime_type":"application/zip","committed":false}"#,
        "",
    )
    .await;
    let result = begin_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["upload_id"], "wc_upload_test_1");

    let content_base64 =
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"hello");
    let chunk_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let path = path.clone();
        let content_base64 = content_base64.clone();
        async move {
            runtime
                .artifact_upload_chunk(
                    project,
                    path,
                    "wc_upload_test_1".to_string(),
                    0,
                    content_base64,
                )
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "artifact-upload")
        .await
        .expect("artifact_upload_chunk should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_artifact_upload_chunk");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["upload_id"], "wc_upload_test_1");
    assert_eq!(payload["offset"], 0);
    assert_eq!(payload["content_base64"], content_base64);
    assert_eq!(
        payload["max_chunk_bytes"],
        MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES
    );
    complete_patch_agent_request(
        &runtime,
        "artifact-upload",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_1","received_bytes":5,"next_offset":5,"expected_bytes":5,"expected_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","max_bytes":10485760,"mime_type":"application/zip","committed":false}"#,
        "",
    )
    .await;
    let result = chunk_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["received_bytes"], 5);

    let finish_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let path = path.clone();
        async move {
            runtime
                .artifact_upload_finish(project, path, "wc_upload_test_1".to_string())
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "artifact-upload")
        .await
        .expect("artifact_upload_finish should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_artifact_upload_finish");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({"path": path.clone(), "upload_id": "wc_upload_test_1"})
    );
    complete_patch_agent_request(
        &runtime,
        "artifact-upload",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_1","bytes":5,"received_bytes":5,"expected_bytes":5,"expected_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","mime_type":"application/zip","committed":true}"#,
        "",
    )
    .await;
    let result = finish_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["committed"], true);

    let abort_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let path = path.clone();
        async move {
            runtime
                .artifact_upload_abort(project, path, "wc_upload_test_2".to_string())
                .await
        }
    });
    let req = next_patch_agent_request(&runtime, "artifact-upload")
        .await
        .expect("artifact_upload_abort should enqueue an artifact file-op");
    assert_eq!(req.kind, "file_artifact_upload_abort");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["upload_id"], "wc_upload_test_2");
    complete_patch_agent_request(
        &runtime,
        "artifact-upload",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_2","received_bytes":0,"expected_bytes":null,"expected_sha256":null,"mime_type":null,"committed":false,"aborted":true}"#,
        "",
    )
    .await;
    let result = abort_task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["aborted"], true);
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

    let begin = runtime
        .dispatch_with_auth(
            ToolCall::ArtifactUploadBegin {
                project: project.clone(),
                path: "artifact.bin".to_string(),
                session_id: None,
                expected_bytes: Some(1),
                expected_sha256: None,
                mime_type: Some("text/plain".to_string()),
                overwrite: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!begin.success);
    assert!(begin.error.unwrap().contains("file_write"));

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
