use super::*;

fn patch_request(client_id: &str) -> ShellFileOpRequest {
    let content = serde_json::json!({
        "patch": "*** Begin Patch\n*** Update File: src/lib.rs\n-old\n+new\n*** End Patch",
        "dry_run": false,
        "matching_mode": "unique",
    });
    ShellFileOpRequest {
        op: "apply_patch".to_string(),
        client_id: client_id.to_string(),
        path: "src/lib.rs".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(content.to_string()),
        max_bytes: None,
        old_text: None,
        pattern: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        line: None,
        create_dirs: false,
        wait_timeout_secs: 30,
    }
}

async fn register_patch_instance(
    registry: &RunnerRegistry,
    client_id: &str,
    supported: bool,
    metadata_supported: bool,
    matching_mode_supported: bool,
    strict_supported: bool,
) -> Result<RunnerView, String> {
    register_instance_with_capabilities(
        registry,
        client_id,
        "inst",
        RunnerCapabilities {
            file_write: true,
            apply_patch: supported,
            apply_patch_match_metadata: metadata_supported,
            apply_patch_matching_mode: matching_mode_supported,
            apply_patch_strict_matching: strict_supported,
            ..Default::default()
        },
    )
    .await
}

#[tokio::test]
async fn enqueue_apply_patch_requires_explicit_capability_and_queues_atomically() {
    let registry = RunnerRegistry::default();
    register_patch_instance(&registry, "patch-off", false, false, false, false)
        .await
        .unwrap();
    let error = registry
        .enqueue_apply_patch(patch_request("patch-off"), "tester".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("capability_unavailable"), "{error}");
    assert!(error.contains("apply_patch"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "patch-off".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    register_patch_instance(&registry, "legacy-patch", true, false, false, false)
        .await
        .unwrap();
    let error = registry
        .enqueue_apply_patch(patch_request("legacy-patch"), "tester".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("capability_unavailable"), "{error}");
    assert!(error.contains("apply_patch_match_metadata"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "legacy-patch".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    register_patch_instance(&registry, "mode-off", true, true, false, false)
        .await
        .unwrap();
    let error = registry
        .enqueue_apply_patch(patch_request("mode-off"), "tester".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("capability_unavailable"), "{error}");
    assert!(error.contains("apply_patch_matching_mode"), "{error}");
    assert!(registry
        .poll(RunnerPollRequest {
            client_id: "mode-off".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    register_patch_instance(&registry, "patch-on", true, true, true, false)
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_apply_patch(patch_request("patch-on"), "tester".to_string())
        .await
        .expect("current Runner should accept ordinary apply_patch");
    let queued = registry
        .poll(RunnerPollRequest {
            client_id: "patch-on".to_string(),
            runner_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .expect("apply_patch request should be queued");
    assert_eq!(queued.request_id, request_id);
    assert_eq!(queued.kind, "file_apply_patch");
    assert!(queued
        .content
        .as_deref()
        .unwrap()
        .contains("*** Begin Patch"));
    assert!(queued
        .content
        .as_deref()
        .unwrap()
        .contains("\"matching_mode\":\"unique\""));
}

#[test]
fn apply_patch_missing_capability_defaults_false_and_is_omitted() {
    let legacy: RunnerCapabilities = serde_json::from_str(
        r#"{"shell":true,"file_read":true,"file_write":true,"structured_file_delete":true}"#,
    )
    .unwrap();
    assert!(!legacy.apply_patch);
    assert!(!legacy.apply_patch_match_metadata);
    assert!(!legacy.apply_patch_matching_mode);
    assert!(!legacy.apply_patch_strict_matching);
    let serialized = serde_json::to_value(RunnerCapabilities::default()).unwrap();
    assert!(serialized.get("apply_patch").is_none());
    assert!(serialized.get("apply_patch_match_metadata").is_none());
    assert!(serialized.get("apply_patch_matching_mode").is_none());
    assert!(serialized.get("apply_patch_strict_matching").is_none());
}
