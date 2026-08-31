use super::*;

fn line_scope_request(client_id: &str, occurrence: Option<usize>) -> ShellFileOpRequest {
    let mut edit = serde_json::json!({
        "kind": "replace_exact",
        "old_text": "foo",
        "new_text": "bar",
        "line_scope": {"start_line": 40, "end_line": 60}
    });
    if let Some(occurrence) = occurrence {
        edit["occurrence"] = occurrence.into();
    }
    ShellFileOpRequest {
        op: "apply_text_edits".to_string(),
        client_id: client_id.to_string(),
        path: "src/lib.rs".to_string(),
        cwd: Some("/tmp/proj".to_string()),
        content: Some(
            serde_json::json!({
                "changes": [{
                    "kind": "edit",
                    "path": "src/lib.rs",
                    "expected_sha256": "a".repeat(64),
                    "edits": [edit]
                }],
                "recovery_metadata_version": 1
            })
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

async fn register_line_scope_instance(
    registry: &ShellClientRegistry,
    client_id: &str,
    line_scope: bool,
) {
    register_instance_with_capabilities(
        registry,
        client_id,
        "inst",
        ShellClientCapabilities {
            file_write: true,
            apply_text_edit_line_scope: line_scope,
            apply_text_edit_occurrence: true,
            ..Default::default()
        },
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn enqueue_scoped_apply_text_edits_requires_explicit_line_scope_capability() {
    let registry = ShellClientRegistry::default();
    register_line_scope_instance(&registry, "scope-off", false).await;

    let error = registry
        .enqueue_apply_text_edits_with_line_scope(
            line_scope_request("scope-off", None),
            "tester".to_string(),
            false,
        )
        .await
        .unwrap_err();
    assert!(error.contains("capability_unavailable"));
    assert!(error.contains("apply_text_edit_line_scope"));
    assert!(registry
        .poll(ShellAgentPollRequest {
            client_id: "scope-off".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .is_none());
}

#[test]
fn enqueue_scoped_occurrence_requires_both_capabilities_before_admission() {
    let baseline = v2_baseline_capabilities();
    assert!(baseline.apply_text_edit_occurrence);
    assert!(!baseline.apply_text_edit_line_scope);

    let baseline_features = RunnerFeatureSet::try_from_registration(&baseline).unwrap();
    assert!(baseline_features.supports(RunnerFeature::ApplyTextEditOccurrence));
    assert!(!baseline_features.supports(RunnerFeature::ApplyTextEditLineScope));

    let mut scoped = baseline.clone();
    scoped.apply_text_edit_line_scope = true;
    let scoped_features = RunnerFeatureSet::try_from_registration(&scoped).unwrap();
    assert!(scoped_features.supports(RunnerFeature::ApplyTextEditOccurrence));
    assert!(scoped_features.supports(RunnerFeature::ApplyTextEditLineScope));

    // An accepted generation-2 Runner cannot reach scoped enqueue with the
    // occurrence capability absent: occurrence remains part of the frozen V2
    // baseline, while line scope is independently additive.
    scoped.apply_text_edit_occurrence = false;
    let error = RunnerFeatureSet::try_from_registration(&scoped).unwrap_err();
    assert_eq!(
        error,
        "runner generation baseline capability mismatch: apply_text_edit_occurrence"
    );
}

#[tokio::test]
async fn enqueue_scoped_apply_text_edits_preserves_scope_and_global_occurrence_payload() {
    let registry = ShellClientRegistry::default();
    register_line_scope_instance(&registry, "scope-on", true).await;
    let (request_id, _rx) = registry
        .enqueue_apply_text_edits_with_line_scope(
            line_scope_request("scope-on", Some(2)),
            "tester".to_string(),
            true,
        )
        .await
        .unwrap();
    let request = registry
        .poll(ShellAgentPollRequest {
            client_id: "scope-on".to_string(),
            agent_instance_id: "inst".to_string(),
            projects: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.request_id, request_id);
    let payload: serde_json::Value =
        serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    let edit = &payload["changes"][0]["edits"][0];
    assert_eq!(edit["occurrence"], 2);
    assert_eq!(edit["line_scope"]["start_line"], 40);
    assert_eq!(edit["line_scope"]["end_line"], 60);
}

#[test]
fn apply_text_edit_line_scope_missing_capability_defaults_false_and_is_omitted() {
    let legacy: ShellClientCapabilities = serde_json::from_str(
        r#"{"shell":true,"file_read":true,"file_write":true,"apply_text_edit_occurrence":true}"#,
    )
    .unwrap();
    assert!(!legacy.apply_text_edit_line_scope);
    let serialized = serde_json::to_value(ShellClientCapabilities::default()).unwrap();
    assert!(serialized.get("apply_text_edit_line_scope").is_none());
}
