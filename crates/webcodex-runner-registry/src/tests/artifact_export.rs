use super::*;

#[tokio::test]
async fn generic_file_enqueue_rejects_internal_artifact_export_chunk() {
    let registry = ShellClientRegistry::default();
    let error = registry
        .enqueue_file_op(
            ShellFileOpRequest {
                op: "read_project_artifact_export_chunk".to_string(),
                client_id: "internal-only-probe".to_string(),
                path: "paper/report.pdf".to_string(),
                cwd: Some("/tmp/proj".to_string()),
                content: Some(
                    r#"{"path":"paper/report.pdf","expected_file_bytes":1,"offset":0,"length":1}"#
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
            },
            "tester".to_string(),
        )
        .await
        .unwrap_err();
    assert!(error.contains("internal-only"), "error was: {error}");
}
