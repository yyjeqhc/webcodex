use super::*;

fn occurrence_request(client_id: &str) -> ShellFileOpRequest {
    ShellFileOpRequest {
        op: "apply_text_edits".to_string(),
        client_id: client_id.to_string(),
        path: "src/lib.rs".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(
            r#"{"changes":[{"kind":"edit","path":"src/lib.rs","expected_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","edits":[{"kind":"replace_exact","old_text":"foo","new_text":"bar","occurrence":2}]}],"recovery_metadata_version":1}"#
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

async fn register_occurrence_instance(
    registry: &ShellClientRegistry,
    client_id: &str,
    instance: &str,
    supported: bool,
) -> Result<ShellClientView, String> {
    register_instance_with_capabilities(
        registry,
        client_id,
        instance,
        ShellClientCapabilities {
            file_write: true,
            apply_text_edit_occurrence: supported,
            ..Default::default()
        },
    )
    .await
}

#[tokio::test]
async fn enqueue_apply_text_edits_occurrence_requires_capability_and_queues_atomically() {
    let registry = ShellClientRegistry::default();
    register_occurrence_instance(&registry, "occurrence-on", "inst", true)
        .await
        .unwrap();
    let (request_id, _rx) = registry
        .enqueue_apply_text_edits_with_occurrence(
            occurrence_request("occurrence-on"),
            "tester".to_string(),
        )
        .await
        .expect("capable Runner should accept occurrence edit");
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "occurrence-on".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.request_id, request_id);
    assert_eq!(request.kind, "file_apply_text_edits");
    assert!(request
        .content
        .as_deref()
        .unwrap()
        .contains("\"occurrence\":2"));
}

#[test]
fn apply_text_edit_occurrence_missing_capability_defaults_false_and_is_omitted() {
    let legacy: ShellClientCapabilities = serde_json::from_str(
        r#"{"shell":true,"file_read":true,"file_write":true,"structured_file_delete":true}"#,
    )
    .unwrap();
    assert!(!legacy.apply_text_edit_occurrence);
    let serialized = serde_json::to_value(ShellClientCapabilities::default()).unwrap();
    assert!(serialized.get("apply_text_edit_occurrence").is_none());
}
