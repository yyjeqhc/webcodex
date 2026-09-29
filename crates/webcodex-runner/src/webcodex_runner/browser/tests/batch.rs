use super::*;

#[test]
fn batch_uploads_use_project_path_policy_and_closed_operations() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    std::fs::write(root.join("resume.pdf"), b"fixture").unwrap();
    std::fs::write(root.join(".env"), b"fixture").unwrap();
    let mut policy = RunnerPolicy::default();
    policy.allowed_roots = vec![root.clone()];
    let operation = json!({"action": "upload_file", "element_id": "element_fixture",
        "project_root": root, "path": "resume.pdf"});
    let resolved = resolve_batch_operations(&policy, vec![operation.clone()]).unwrap();
    assert!(
        matches!(&resolved[0], webcodex_browser::BatchOperation::UploadFile { path, .. } if path == &root.join("resume.pdf"))
    );
    for path in ["../resume.pdf", "/resume.pdf", ".env", "missing.pdf"] {
        let mut invalid = operation.clone();
        invalid["path"] = json!(path);
        assert_eq!(
            resolve_batch_operations(&policy, vec![invalid])
                .unwrap_err()
                .execution_state,
            ExecutionState::NotStarted
        );
    }
    let mut invalid = operation.clone();
    invalid["path"] = json!("x".repeat(4097));
    assert_eq!(
        resolve_batch_operations(&policy, vec![invalid])
            .unwrap_err()
            .kind,
        "invalid_upload_path"
    );
    let mut invalid = operation.clone();
    invalid.as_object_mut().unwrap().remove("project_root");
    assert!(resolve_batch_operations(&policy, vec![invalid]).is_err());
    for field in [
        "page_id",
        "browser_id",
        "selector",
        "backend_node_id",
        "project",
    ] {
        let mut invalid = operation.clone();
        invalid[field] = json!("forbidden");
        assert!(resolve_batch_operations(&policy, vec![invalid]).is_err());
    }
    assert!(resolve_batch_operations(&policy, vec![operation; 33]).is_err());
    assert!(resolve_batch_operations(&policy, vec![]).is_err());
}

#[test]
fn batch_upload_preflight_rejects_before_browser_dispatch() {
    let request = RunnerBrowserOperation {
        kind: RunnerBrowserOperationKind::Batch,
        payload: json!({"browser_id": "missing", "page_id": "missing", "operations": [
            {"action": "input_text", "element_id": "element_fixture", "text": "Alice"},
            {"action": "upload_file", "element_id": "element_fixture", "path": "resume.pdf"}
        ]})
        .to_string(),
        timeout_secs: 120,
    };
    let result = handle_browser_operation(
        &BrowserSupervisor::new(),
        &RunnerPolicy::default(),
        &request,
    );
    let result: Value = serde_json::from_str(result.stdout.as_deref().unwrap()).unwrap();
    assert_eq!(result["execution_state"], "not_started");
    assert_eq!(result["error"]["kind"], "invalid_request");
}
