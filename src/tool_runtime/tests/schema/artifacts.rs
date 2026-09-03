use super::*;

#[test]
fn read_project_artifact_rejects_retired_max_bytes_alias() {
    let error = ToolCall::from_tool_name(
        "read_project_artifact",
        json!({"project": "agent:test:demo", "path": "artifact.bin", "max_bytes": 32}),
    )
    .unwrap_err();
    assert!(error.contains("max_bytes"));
    assert!(error.contains("no longer supported"));
    assert!(error.contains("length"));
}
