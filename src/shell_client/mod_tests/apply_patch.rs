use super::*;

fn patch_request(client_id: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: "apply_patch".to_string(),
        client_id: client_id.to_string(),
        path: "src/lib.rs".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(
            r#"{"patch":"*** Begin Patch\n*** Update File: src/lib.rs\n-old\n+new\n*** End Patch","dry_run":false}"#
                .to_string(),
        ),
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
    registry: &ShellClientRegistry,
    client_id: &str,
    supported: bool,
) -> Result<ShellClientView, String> {
    register_instance_with_capabilities(
        registry,
        client_id,
        "inst",
        ShellClientCapabilities {
            file_write: true,
            apply_patch: supported,
            ..Default::default()
        },
    )
    .await
}

#[tokio::test]
async fn enqueue_apply_patch_requires_explicit_capability_and_queues_atomically() {
    let registry = ShellClientRegistry::default();
    register_patch_instance(&registry, "patch-off", false)
        .await
        .unwrap();
    let error = registry
        .enqueue_apply_patch(patch_request("patch-off"), "tester".to_string())
        .await
        .unwrap_err();
    assert!(error.contains("capability_unavailable"), "{error}");
    assert!(error.contains("apply_patch"), "{error}");
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "patch-off".to_string(),
            agent_instance_id: "inst".to_string(),
        })
        .await
        .unwrap()
        .is_none());

    register_patch_instance(&registry, "patch-on", true)
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_apply_patch(patch_request("patch-on"), "tester".to_string())
        .await
        .expect("capable Runner should accept apply_patch");
    let queued = registry
        .poll(ShellAgentPollRequest {
            client_id: "patch-on".to_string(),
            agent_instance_id: "inst".to_string(),
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
}

#[test]
fn apply_patch_missing_capability_defaults_false_and_is_omitted() {
    let legacy: ShellClientCapabilities = serde_json::from_str(
        r#"{"shell":true,"file_read":true,"file_write":true,"structured_file_delete":true}"#,
    )
    .unwrap();
    assert!(!legacy.apply_patch);
    let serialized = serde_json::to_value(ShellClientCapabilities::default()).unwrap();
    assert!(serialized.get("apply_patch").is_none());
}
