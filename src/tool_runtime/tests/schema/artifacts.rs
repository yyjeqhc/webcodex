use super::*;

#[test]
fn read_project_artifact_rejects_retired_max_bytes_alias() {
    let error = ToolCall::from_tool_name(
        "read_project_artifact_chunk",
        json!({"project": "agent:test:demo", "path": "artifact.bin", "max_bytes": 32}),
    )
    .unwrap_err();
    assert!(error.contains("max_bytes"));
    assert!(error.contains("no longer supported"));
    assert!(error.contains("length"));
}

#[test]
fn read_project_artifact_expected_sha256_parser_requires_lowercase_64_hex() {
    let digest = "a".repeat(64);
    let call = ToolCall::from_tool_name(
        "read_project_artifact_chunk",
        json!({
            "project": "agent:test:demo",
            "path": "artifact.bin",
            "expected_sha256": digest,
        }),
    )
    .unwrap();
    match call {
        ToolCall::ReadProjectArtifact {
            expected_sha256, ..
        } => {
            assert_eq!(
                expected_sha256.as_deref(),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            );
        }
        other => panic!("expected ReadProjectArtifact, got {other:?}"),
    }
    for invalid in [
        "abc".to_string(),
        "A".repeat(64),
        format!("g{}", "a".repeat(63)),
    ] {
        let error = ToolCall::from_tool_name(
            "read_project_artifact_chunk",
            json!({
                "project": "agent:test:demo",
                "path": "artifact.bin",
                "expected_sha256": invalid,
            }),
        )
        .unwrap_err();
        assert!(error.contains("expected_sha256"), "{error}");
        assert!(error.contains("64 lowercase hexadecimal"), "{error}");
    }
}

#[test]
fn project_artifact_parses_typed_actions_and_rejects_cross_action_fields() {
    let digest = "a".repeat(64);
    let inspect = ToolCall::from_tool_name(
        "inspect_project_artifact",
        json!({
            "project": "agent:test:demo",
            "path": "artifact.bin",
            "action": "inspect",
            "offset": 32,
            "length": 64,
            "expected_sha256": digest,
        }),
    )
    .unwrap();
    match inspect {
        ToolCall::ProjectArtifact {
            action,
            offset,
            length,
            expected_sha256,
            ..
        } => {
            assert_eq!(action, ProjectArtifactAction::Inspect);
            assert_eq!(offset, Some(32));
            assert_eq!(length, Some(64));
            assert_eq!(
                expected_sha256.as_deref(),
                Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            );
        }
        other => panic!("expected ProjectArtifact, got {other:?}"),
    }

    for arguments in [
        json!({"project":"agent:test:demo","path":"x.png","action":"image","offset":0}),
        json!({"project":"agent:test:demo","path":"x.bin","action":"export","length":32}),
        json!({"project":"agent:test:demo","path":"x.bin","action":"metadata","expected_sha256":"a".repeat(64)}),
        json!({"project":"agent:test:demo","path":"x.bin","action":"inspect","allow_missing":true}),
    ] {
        let error = ToolCall::from_tool_name("inspect_project_artifact", arguments).unwrap_err();
        assert!(error.contains("does not accept field"), "{error}");
    }

    let invalid_digest = ToolCall::from_tool_name(
        "inspect_project_artifact",
        json!({"project":"agent:test:demo","path":"x.bin","action":"inspect","expected_sha256":"A".repeat(64)}),
    )
    .unwrap_err();
    assert!(invalid_digest.contains("64 lowercase hexadecimal"));
}
