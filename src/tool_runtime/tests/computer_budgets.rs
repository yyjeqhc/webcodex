use super::support::*;
use crate::runner_protocol::{RunnerCapabilities, RunnerResultRequest};
use crate::tool_runtime::tool_call::{
    ComputerControlToolCall, ComputerObserveToolCall, ComputerSnapshotRegion,
};
use crate::tool_runtime::ToolCall;
use base64::{engine::general_purpose, Engine as _};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const DISPLAY_ID: &str = "display_iavN7wEjRWeJq83v";

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tokio::test]
async fn computer_response_parse_failures_keep_private_diagnostics_and_effect_uncertainty() {
    for (stdout, expected) in [
        (None, "stdout_present=false"),
        (Some(""), "stdout_bytes=0"),
        (Some("{\"PRIVATE_SCREENSHOT\":"), "parse_category=Eof"),
        (
            Some("{\"秘密PRIVATE_SCREENSHOT\":?}"),
            "parse_category=Syntax",
        ),
        (Some("null"), "Runner snapshot output is not an object"),
    ] {
        for truncated in [false, true] {
            for effect in [false, true] {
                let result = computer_fixture_response(stdout, truncated, effect).await;
                assert!(!result.success);
                assert_eq!(
                    result.output["error_kind"],
                    if effect {
                        "outcome_unknown"
                    } else {
                        "invalid_runner_response"
                    }
                );
                if effect {
                    assert_eq!(result.output["execution_state"], "outcome_unknown");
                    assert_eq!(result.output["recovery_kind"], "reobserve");
                } else {
                    assert!(
                        result.error.as_deref().unwrap().contains(expected),
                        "{:?}",
                        result.error
                    );
                    if stdout != Some("null") {
                        assert!(result
                            .error
                            .as_deref()
                            .unwrap()
                            .contains(&format!("stdout_truncated={truncated}")));
                    }
                }
                let serialized = serde_json::to_string(&result).unwrap();
                assert!(!serialized.contains("PRIVATE_SCREENSHOT"));
                assert!(!serialized.contains("PRIVATE_STDERR"));
                assert!(!serialized.contains("秘密"));
            }
        }
    }
}

async fn computer_fixture_response(
    stdout: Option<&str>,
    truncated: bool,
    effect: bool,
) -> crate::tool_runtime::ToolResult {
    let client_id = "computer-parse-diagnostic";
    let runtime = runtime_with_agent_project(client_id);
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        client_id,
        &auth,
        RunnerCapabilities {
            computer_observe: true,
            computer_control: true,
            ..Default::default()
        },
        vec![registered_project("agent-proj", "/tmp/agent-proj")],
    )
    .await;
    let call = if effect {
        ToolCall::ComputerControl(ComputerControlToolCall::Press {
            client_id: client_id.to_string(),
            surface_id: "surface_test".to_string(),
            element_id: "element_test".to_string(),
        })
    } else {
        ToolCall::ComputerObserve(ComputerObserveToolCall::SnapshotWindow {
            client_id: client_id.to_string(),
            surface_id: "surface_test".to_string(),
            region: None,
            max_width: None,
            max_height: None,
        })
    };
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_computer_tool(call, Some(&auth)).await }
    });
    let request = wait_for_runner_request_for_client(&runtime, client_id).await;
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: format!("inst-{client_id}"),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: stdout.map(str::to_string),
            stderr: Some("PRIVATE_STDERR".to_string()),
            stdout_truncated: truncated,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    task.await.unwrap()
}

#[tokio::test]
async fn computer_window_snapshots_preserve_large_responses_through_runtime_validation() {
    for region in [
        None,
        Some(ComputerSnapshotRegion {
            x: 0,
            y: 0,
            width: 640,
            height: 480,
        }),
    ] {
        let client_id = "computer-budget-window";
        let runtime = runtime_with_agent_project(client_id);
        let auth = open_auth_context();
        register_agent_projects_for_auth(
            &runtime,
            client_id,
            &auth,
            RunnerCapabilities {
                computer_observe: true,
                computer_snapshot_region: true,
                ..Default::default()
            },
            vec![registered_project("agent-proj", "/tmp/agent-proj")],
        )
        .await;
        let expected_region = region
            .as_ref()
            .map(|r| json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height}));
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move {
                runtime
                    .dispatch_computer_tool(
                        ToolCall::ComputerObserve(ComputerObserveToolCall::SnapshotWindow {
                            client_id: client_id.to_string(),
                            surface_id: "surface_test".to_string(),
                            region,
                            max_width: Some(1800),
                            max_height: Some(1000),
                        }),
                        Some(&auth),
                    )
                    .await
            }
        });
        let request = wait_for_runner_request_for_client(&runtime, client_id).await;
        assert_eq!(request.kind, "computer_snapshot_region");
        let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        assert_eq!(
            payload["region"],
            expected_region.clone().unwrap_or(Value::Null)
        );
        assert_eq!(payload["max_width"], 1800);
        assert_eq!(payload["max_height"], 1000);

        // Synthetic MIME-sniffing fixture, not native capture/decoder evidence.
        let mut image = vec![0x5a; 240_000];
        image[..4].copy_from_slice(&[0xff, 0xd8, 0xff, 0xe0]);
        let encoded = general_purpose::STANDARD.encode(&image);
        let hash = sha256_hex(&image);
        let region =
            expected_region.unwrap_or_else(|| json!({"x":0,"y":0,"width":1280,"height":720}));
        let stdout = json!({
            "surface":{"surface_id":"surface_test","application":"Example","title":"Window","width":1280,"height":720,"focused":false,"active":false},
            "source_width":1280,"source_height":720,"region":region,
            "width":region["width"],"height":region["height"],
            "mime_type":"image/jpeg","file_bytes":image.len(),"sha256":hash,
            "captured_at_unix_ms":1_700_000_000_000u64,"content_base64":encoded,
        }).to_string();
        assert!(stdout.len() > 256 * 1024);
        runtime
            .runner_registry
            .complete(RunnerResultRequest {
                client_id: client_id.to_string(),
                runner_instance_id: format!("inst-{client_id}"),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(stdout),
                stderr: None,
                stdout_truncated: false,
                stderr_truncated: false,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
        let result = task.await.unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["sha256"], hash);
        assert_eq!(result.output["file_bytes"], image.len());
        assert_eq!(result.output["region"], region);
        assert_eq!(
            general_purpose::STANDARD
                .decode(result.output["content_base64"].as_str().unwrap())
                .unwrap(),
            image
        );
    }
}

#[tokio::test]
async fn computer_display_snapshot_clamps_budget_before_runner_and_validates_effective_bound() {
    let client_id = "computer-budget-display";
    let runtime = runtime_with_agent_project(client_id);
    let auth = open_auth_context();
    register_agent_projects_for_auth(
        &runtime,
        client_id,
        &auth,
        RunnerCapabilities {
            computer_display_observe: true,
            ..Default::default()
        },
        vec![registered_project("agent-proj", "/tmp/agent-proj")],
    )
    .await;

    let task = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move {
            runtime
                .dispatch_computer_tool(
                    ToolCall::ComputerObserve(ComputerObserveToolCall::SnapshotDisplay {
                        client_id: client_id.to_string(),
                        display_id: DISPLAY_ID.to_string(),
                        max_width: Some(10_000),
                        max_height: Some(u32::MAX),
                    }),
                    Some(&auth),
                )
                .await
        }
    });

    let request = wait_for_runner_request_for_client(&runtime, client_id).await;
    assert_eq!(request.kind, "computer_snapshot_display");
    let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(payload["max_width"], 4096);
    assert_eq!(payload["max_height"], 4096);

    let image = [0xff, 0xd8, 0xff, 0xe0];
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: client_id.to_string(),
            runner_instance_id: format!("inst-{client_id}"),
            request_id: request.request_id,
            exit_code: Some(0),
            stdout: Some(
                json!({
                    "display_id": DISPLAY_ID,
                    "snapshot_generation": 7,
                    "source_width": 1920,
                    "source_height": 1080,
                    "width": 1920,
                    "height": 1080,
                    "mime_type": "image/jpeg",
                    "file_bytes": image.len(),
                    "sha256": sha256_hex(&image),
                    "captured_at_unix_ms": 1_700_000_000_000u64,
                    "content_base64": general_purpose::STANDARD.encode(image),
                })
                .to_string(),
            ),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["width"], 1920);
    assert_eq!(result.output["height"], 1080);
}
