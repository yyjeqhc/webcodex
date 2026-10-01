//! Agent-native artifact file-op routing tests.

use super::super::files::*;
use super::super::sessions::SessionTransport;
use super::super::{ProjectArtifactAction, ToolCall};
use super::support::*;
use crate::runner_protocol::RunnerCapabilities;
use serde_json::json;

#[tokio::test]
async fn save_project_artifact_routes_to_agent_file_op() {
    let runtime = runtime_with_agent_project("artifact-save");
    register_agent(
        &runtime,
        "artifact-save",
        None,
        RunnerCapabilities {
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

    let req = wait_for_patch_agent_request(&runtime, "artifact-save").await;
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
        RunnerCapabilities {
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

    let req = wait_for_patch_agent_request(&runtime, "artifact-meta").await;
    assert_eq!(req.kind, "file_read_project_artifact_metadata");
    assert!(req.command.is_empty());
    assert!(req.stdin.is_none());
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({"path":"sample.zip","max_bytes":super::super::MAX_PROJECT_ARTIFACT_EXPORT_BYTES,"allow_missing":false})
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
        RunnerCapabilities {
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

    let req = wait_for_patch_agent_request(&runtime, "artifact-meta-missing").await;
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
async fn read_project_artifact_emits_parser_ready_snapshot_fenced_continuation() {
    let runtime = runtime_with_agent_project("artifact-read");
    register_agent(
        &runtime,
        "artifact-read",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-read");
    let project_input = "agent-proj".to_string();
    let sha256 = "c".repeat(64);

    let first_task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_project_artifact(
                    project_input,
                    "data.bin".to_string(),
                    None,
                    Some(0),
                    Some(4),
                    None,
                    None,
                    None,
                )
                .await
        }
    });

    let first_request = wait_for_patch_agent_request(&runtime, "artifact-read").await;
    assert_eq!(first_request.kind, "file_read_project_artifact");
    let first_payload: serde_json::Value =
        serde_json::from_str(first_request.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        first_payload,
        json!({"path":"data.bin","offset":0,"length":4,"max_file_bytes":MAX_PROJECT_ARTIFACT_BYTES})
    );
    let first_stdout = json!({
        "path": "data.bin",
        "mime_type": null,
        "file_bytes": 8,
        "sha256": sha256,
        "offset": 0,
        "bytes_returned": 4,
        "content_base64": "YWJjZA==",
        "next_offset": 4,
        "truncated": true,
        "eof": false,
    })
    .to_string();
    complete_patch_agent_request(
        &runtime,
        "artifact-read",
        &first_request.request_id,
        0,
        &first_stdout,
        "",
    )
    .await;
    let first = first_task.await.unwrap();
    assert!(first.success, "{:?}", first.error);
    let next = &first.output["suggested_call"];
    assert_eq!(next["tool"], "read_project_artifact_chunk");
    assert_eq!(next["arguments"]["project"], project);
    assert_eq!(next["arguments"]["path"], "data.bin");
    assert_eq!(next["arguments"]["encoding"], "base64");
    assert_eq!(next["arguments"]["offset"], 4);
    assert_eq!(next["arguments"]["length"], 4);
    assert_eq!(next["arguments"]["expected_sha256"], sha256);
    let next_call =
        ToolCall::from_tool_name(next["tool"].as_str().unwrap(), next["arguments"].clone())
            .expect("artifact suggested_call must be parser-ready");
    let mut projected_first = crate::tool_runtime::ToolResult::ok(first.output.clone());
    crate::model_surface::project_tool_result_suggested_calls(
        "read_project_artifact_chunk",
        &mut projected_first,
        &|target| crate::model_surface::suggested_tool_call_route(target, false),
    );
    let projected_next = &projected_first.output["suggested_call"];
    assert_eq!(
        projected_next["tool"],
        crate::model_surface::ADAPTIVE_RUNTIME_GATEWAY_TOOL_NAME
    );
    assert_eq!(
        projected_next["arguments"]["tool"],
        "read_project_artifact_chunk"
    );
    assert_eq!(projected_next["arguments"]["arguments"], next["arguments"]);

    let second_task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_file_tool(next_call, SessionTransport::Api, None, None)
                .await
        }
    });
    let second_request = wait_for_patch_agent_request(&runtime, "artifact-read").await;
    let second_payload: serde_json::Value =
        serde_json::from_str(second_request.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(second_payload["offset"], 4);
    assert_eq!(second_payload["length"], 4);
    assert_eq!(second_payload["expected_sha256"], sha256);
    let second_stdout = json!({
        "path": "data.bin",
        "mime_type": null,
        "file_bytes": 8,
        "sha256": sha256,
        "offset": 4,
        "bytes_returned": 4,
        "content_base64": "ZWZnaA==",
        "next_offset": 8,
        "truncated": false,
        "eof": true,
    })
    .to_string();
    complete_patch_agent_request(
        &runtime,
        "artifact-read",
        &second_request.request_id,
        0,
        &second_stdout,
        "",
    )
    .await;
    let second = second_task.await.unwrap();
    assert!(second.success, "{:?}", second.error);
    assert_eq!(second.output["sha256"], sha256);
    assert_eq!(second.output["offset"], 4);
    assert_eq!(second.output["eof"], true);
    assert!(second.output.get("suggested_call").is_none());
}

#[tokio::test]
async fn read_project_artifact_snapshot_change_hides_digest_proof() {
    let runtime = runtime_with_agent_project("artifact-read-stale");
    register_agent(
        &runtime,
        "artifact-read-stale",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-read-stale");
    let expected_sha256 = "c".repeat(64);
    let actual_sha256 = "d".repeat(64);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let expected_sha256 = expected_sha256.clone();
        async move {
            runtime
                .read_project_artifact(
                    project,
                    "data.bin".to_string(),
                    None,
                    Some(4),
                    Some(4),
                    Some(expected_sha256),
                    None,
                    None,
                )
                .await
        }
    });

    let request = wait_for_patch_agent_request(&runtime, "artifact-read-stale").await;
    let payload: serde_json::Value =
        serde_json::from_str(request.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["expected_sha256"], expected_sha256);
    let stdout = json!({
        "path": "data.bin",
        "file_bytes": 0,
        "sha256": null,
        "offset": 4,
        "bytes_returned": 0,
        "content_base64": "",
        "next_offset": 0,
        "truncated": false,
        "eof": false,
        "error": "artifact snapshot changed",
        "error_kind": "snapshot_changed",
        "expected_sha256": expected_sha256,
        "actual_sha256": actual_sha256,
    })
    .to_string();
    complete_patch_agent_request(
        &runtime,
        "artifact-read-stale",
        &request.request_id,
        0,
        &stdout,
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert_eq!(result.error.as_deref(), Some("artifact snapshot changed"));
    assert_eq!(result.output["path"], "data.bin");
    assert_eq!(result.output["error_kind"], "snapshot_changed");
    assert_eq!(result.output["state_changed"], false);
    for field in [
        "expected_sha256",
        "actual_sha256",
        "sha256",
        "content_base64",
        "suggested_call",
    ] {
        assert!(
            result.output.get(field).is_none(),
            "{field}: {}",
            result.output
        );
    }
}

#[tokio::test]
async fn read_project_artifact_mcp_image_routes_complete_bounded_remote_read() {
    let runtime = runtime_with_agent_project("artifact-image");
    register_agent(
        &runtime,
        "artifact-image",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-image");
    let bytes = b"\x89PNG\r\n\x1a\nremote-image";
    let content_base64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
    let sha256 = sha256_hex_bytes(bytes);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .read_project_artifact(
                    project,
                    "docs/images/remote.png".to_string(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(true),
                )
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "artifact-image").await;
    assert_eq!(req.kind, "file_read_project_artifact");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({
            "path": "docs/images/remote.png",
            "offset": 0,
            "length": crate::artifact_policy::MAX_MCP_IMAGE_BYTES,
            "max_file_bytes": crate::artifact_policy::MAX_MCP_IMAGE_BYTES,
            "mcp_image": true,
        })
    );

    let stdout = json!({
        "path": "docs/images/remote.png",
        "mime_type": "image/png",
        "file_bytes": bytes.len(),
        "sha256": sha256,
        "offset": 0,
        "bytes_returned": bytes.len(),
        "content_base64": &content_base64,
        "next_offset": bytes.len(),
        "truncated": false,
        "eof": true,
    })
    .to_string();
    complete_patch_agent_request(&runtime, "artifact-image", &req.request_id, 0, &stdout, "").await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["mime_type"], "image/png");
    assert_eq!(result.output["content_base64"], content_base64);
    assert_eq!(result.output["eof"], true);
}

#[tokio::test]
async fn read_project_artifact_mcp_image_rejects_untrusted_remote_mime() {
    let runtime = runtime_with_agent_project("artifact-image-mime");
    register_agent(
        &runtime,
        "artifact-image-mime",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("artifact-image-mime");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .read_project_artifact(
                    project,
                    "docs/images/spoofed.png".to_string(),
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(true),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "artifact-image-mime").await;
    let pdf = b"%PDF-1.7\n";
    let stdout = json!({
        "path": "docs/images/spoofed.png",
        "mime_type": "image/png",
        "file_bytes": pdf.len(),
        "sha256": sha256_hex_bytes(pdf),
        "offset": 0,
        "bytes_returned": pdf.len(),
        "content_base64": base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            pdf
        ),
        "next_offset": pdf.len(),
        "truncated": false,
        "eof": true,
    })
    .to_string();
    complete_patch_agent_request(
        &runtime,
        "artifact-image-mime",
        &req.request_id,
        0,
        &stdout,
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("not a supported PNG, JPEG, or WebP"));
    assert_eq!(result.output["error_kind"], "invalid_mcp_image_artifact");
}

#[tokio::test]
async fn read_project_artifact_image_mode_is_rejected_outside_mcp() {
    let result = test_runtime()
        .dispatch_file_tool(
            ToolCall::ReadProjectArtifact {
                project: "agent:missing:missing".to_string(),
                path: "docs/images/sample.png".to_string(),
                session_id: None,
                encoding: None,
                offset: None,
                length: None,
                expected_sha256: None,
                as_image: Some(true),
            },
            SessionTransport::Api,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result
        .error
        .as_deref()
        .unwrap()
        .contains("only supported over MCP"));
}

#[tokio::test]
async fn artifact_upload_tools_route_to_agent_file_ops() {
    let runtime = runtime_with_agent_project("artifact-upload");
    register_agent(
        &runtime,
        "artifact-upload",
        None,
        RunnerCapabilities {
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
    let req = wait_for_patch_agent_request(&runtime, "artifact-upload").await;
    assert_eq!(req.kind, "file_artifact_upload_begin");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["path"], path);
    assert_eq!(payload["expected_bytes"], 5);
    assert_eq!(payload["expected_sha256"], expected_sha256);
    assert_eq!(payload["mime_type"], "application/zip");
    assert_eq!(payload["overwrite"], false);
    assert_eq!(payload["max_bytes"], MAX_PROJECT_ARTIFACT_UPLOAD_BYTES);
    complete_patch_agent_request(
        &runtime,
        "artifact-upload",
        &req.request_id,
        0,
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_1","received_bytes":0,"next_offset":0,"expected_bytes":5,"expected_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","max_bytes":268435456,"mime_type":"application/zip","committed":false}"#,
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
    let req = wait_for_patch_agent_request(&runtime, "artifact-upload").await;
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
        r#"{"path":"artifacts/imports/sample.zip","upload_id":"wc_upload_test_1","received_bytes":5,"next_offset":5,"expected_bytes":5,"expected_sha256":"dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd","max_bytes":268435456,"mime_type":"application/zip","committed":false}"#,
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
    let req = wait_for_patch_agent_request(&runtime, "artifact-upload").await;
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
    let req = wait_for_patch_agent_request(&runtime, "artifact-upload").await;
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
async fn project_artifact_metadata_routes_to_canonical_runner_operation() {
    let runtime = runtime_with_agent_project("project-artifact-meta");
    register_agent(
        &runtime,
        "project-artifact-meta",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("project-artifact-meta");

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_file_tool(
                    ToolCall::ProjectArtifact {
                        project,
                        path: "artifacts/smoke/missing.artifact".to_string(),
                        action: ProjectArtifactAction::Metadata,
                        session_id: None,
                        allow_missing: Some(true),
                        offset: None,
                        length: None,
                        expected_sha256: None,
                    },
                    SessionTransport::Api,
                    None,
                    None,
                )
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "project-artifact-meta").await;
    assert_eq!(req.kind, "file_read_project_artifact_metadata");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(payload["path"], "artifacts/smoke/missing.artifact");
    assert_eq!(payload["allow_missing"], true);
    assert_eq!(
        payload["max_bytes"],
        super::super::MAX_PROJECT_ARTIFACT_EXPORT_BYTES
    );
    assert!(payload.get("action").is_none());

    complete_patch_agent_request(
        &runtime,
        "project-artifact-meta",
        &req.request_id,
        0,
        r#"{"path":"artifacts/smoke/missing.artifact","exists":false,"missing":true}"#,
        "",
    )
    .await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["exists"], false);
}

#[tokio::test]
async fn project_artifact_inspect_reuses_runner_read_and_keeps_unified_continuation() {
    let runtime = runtime_with_agent_project("project-artifact-inspect");
    register_agent(
        &runtime,
        "project-artifact-inspect",
        None,
        RunnerCapabilities {
            file_read: true,
            ..Default::default()
        },
    )
    .await;
    let project = agent_test_project_id("project-artifact-inspect");
    let sha256 = "c".repeat(64);

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_file_tool(
                    ToolCall::ProjectArtifact {
                        project,
                        path: "data.bin".to_string(),
                        action: ProjectArtifactAction::Inspect,
                        session_id: None,
                        allow_missing: None,
                        offset: Some(0),
                        length: Some(4),
                        expected_sha256: None,
                    },
                    SessionTransport::Api,
                    None,
                    None,
                )
                .await
        }
    });

    let req = wait_for_patch_agent_request(&runtime, "project-artifact-inspect").await;
    assert_eq!(req.kind, "file_read_project_artifact");
    let payload: serde_json::Value =
        serde_json::from_str(req.content.as_deref().expect("artifact payload")).unwrap();
    assert_eq!(
        payload,
        json!({"path":"data.bin","offset":0,"length":4,"max_file_bytes":MAX_PROJECT_ARTIFACT_BYTES})
    );
    let stdout = json!({
        "path": "data.bin",
        "mime_type": null,
        "file_bytes": 8,
        "sha256": sha256,
        "offset": 0,
        "bytes_returned": 4,
        "content_base64": "YWJjZA==",
        "next_offset": 4,
        "truncated": true,
        "eof": false,
    })
    .to_string();
    complete_patch_agent_request(
        &runtime,
        "project-artifact-inspect",
        &req.request_id,
        0,
        &stdout,
        "",
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let next = &result.output["suggested_call"];
    assert_eq!(next["tool"], "inspect_project_artifact");
    assert_eq!(next["arguments"]["action"], "inspect");
    assert_eq!(next["arguments"]["project"], project);
    assert_eq!(next["arguments"]["path"], "data.bin");
    assert_eq!(next["arguments"]["offset"], 4);
    assert_eq!(next["arguments"]["length"], 4);
    assert_eq!(next["arguments"]["expected_sha256"], sha256);
    assert!(next["arguments"].get("encoding").is_none());
    let parsed = ToolCall::from_tool_name("inspect_project_artifact", next["arguments"].clone())
        .expect("unified artifact continuation must stay parser-ready");
    assert!(matches!(
        parsed,
        ToolCall::ProjectArtifact {
            action: ProjectArtifactAction::Inspect,
            ..
        }
    ));
}

#[tokio::test]
async fn project_artifact_mcp_only_delivery_modes_fail_closed_outside_mcp() {
    for action in [ProjectArtifactAction::Image, ProjectArtifactAction::Export] {
        let result = test_runtime()
            .dispatch_file_tool(
                ToolCall::ProjectArtifact {
                    project: "agent:missing:missing".to_string(),
                    path: "artifacts/smoke/file.bin".to_string(),
                    action,
                    session_id: None,
                    allow_missing: None,
                    offset: None,
                    length: None,
                    expected_sha256: None,
                },
                SessionTransport::Api,
                None,
                None,
            )
            .await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "unsupported_transport");
        assert_eq!(result.output["required_transport"], "mcp");
        assert_eq!(result.output["action"], action.as_str());
    }
}
