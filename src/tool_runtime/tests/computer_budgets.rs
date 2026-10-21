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

#[tokio::test]
async fn computer_subtree_dispatch_rejects_old_runner_scope_and_filters_fresh_matches() {
    for finder in [false, true] {
        for receipt in [None, Some("element_other"), Some("element_root")] {
            let client_id = "computer-subtree";
            let runtime = runtime_with_agent_project(client_id);
            let auth = open_auth_context();
            register_agent_projects_for_auth(
                &runtime,
                client_id,
                &auth,
                RunnerCapabilities {
                    computer_observe: true,
                    computer_accessibility_observe: true,
                    ..Default::default()
                },
                vec![registered_project("agent-proj", "/tmp/agent-proj")],
            )
            .await;
            let args = if finder {
                json!({"action":"find_elements", "client_id":client_id, "surface_id":"surface_test", "root_element_id":"element_root", "label":"Deep target"})
            } else {
                json!({"action":"accessibility_tree", "client_id":client_id, "surface_id":"surface_test", "root_element_id":"element_root", "max_depth":8, "max_nodes":256})
            };
            let call = serde_json::from_value::<ComputerObserveToolCall>(args).unwrap();
            let task = tokio::spawn({
                let runtime = runtime.clone();
                let auth = auth.clone();
                async move {
                    runtime
                        .dispatch_computer_tool(ToolCall::ComputerObserve(call), Some(&auth))
                        .await
                }
            });
            let request = wait_for_runner_request_for_client(&runtime, client_id).await;
            assert_eq!(request.kind, "computer_accessibility_tree");
            let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
            assert_eq!(payload["root_element_id"], "element_root");
            let mut output = json!({"platform":"windows", "surface_id":"surface_test", "observation_generation":2,
                "max_depth":8, "max_nodes":256, "truncated":false, "node_count":1,
                "nodes":[{"element_id":"element_new", "parent_element_id":null, "depth":0, "role":"AXButton", "subrole":null,
                    "title":"Deep target", "description":null, "placeholder":null, "value":null, "enabled":true, "focused":false, "child_count":0}]});
            if let Some(root) = receipt {
                output["root_element_id"] = json!(root);
            }
            runtime
                .runner_registry
                .complete(RunnerResultRequest {
                    exit_code: Some(0),
                    stdout: Some(output.to_string()),
                    duration_ms: Some(1),
                    ..runner_result_request(
                        client_id.into(),
                        format!("inst-{client_id}"),
                        request.request_id,
                    )
                })
                .await
                .unwrap();
            let result = task.await.unwrap();
            assert_eq!(
                result.success,
                receipt == Some("element_root"),
                "{result:?}"
            );
            if result.success {
                assert!(result.output.get("root_element_id").is_none());
                assert_eq!(result.output["observation_generation"], 2);
                if finder {
                    assert_eq!(result.output["count"], 1);
                    assert_eq!(result.output["truncated"], false);
                }
            } else {
                assert_eq!(result.output["error_kind"], "invalid_runner_response");
            }
        }
    }
}

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
            exit_code: Some(0),
            stdout: stdout.map(str::to_string),
            stderr: Some("PRIVATE_STDERR".to_string()),
            stdout_truncated: truncated,
            duration_ms: Some(1),
            ..runner_result_request(
                client_id.to_string(),
                format!("inst-{client_id}"),
                request.request_id,
            )
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
                exit_code: Some(0),
                stdout: Some(stdout),
                duration_ms: Some(1),
                ..runner_result_request(
                    client_id.to_string(),
                    format!("inst-{client_id}"),
                    request.request_id,
                )
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
            duration_ms: Some(1),
            ..runner_result_request(
                client_id.to_string(),
                format!("inst-{client_id}"),
                request.request_id,
            )
        })
        .await
        .unwrap();

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["width"], 1920);
    assert_eq!(result.output["height"], 1080);
}
