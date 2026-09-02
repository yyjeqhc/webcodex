use super::*;

fn apply_patch_request(cwd: &Path, patch: &str, dry_run: bool) -> ShellAgentShellRequest {
    ShellAgentShellRequest {
        request_id: "req-apply-patch".to_string(),
        client_id: "agent-1".to_string(),
        kind: "file_apply_patch".to_string(),
        job_id: None,
        cwd: Some(cwd.to_string_lossy().to_string()),
        path: Some("routing-placeholder".to_string()),
        content: Some(
            serde_json::json!({
                "patch": patch,
                "dry_run": dry_run,
            })
            .to_string(),
        ),
        max_bytes: None,
        expected_sha256: None,
        expected_prefix: None,
        start_line: None,
        end_line: None,
        create_dirs: false,
        command: String::new(),
        process: None,
        script: None,
        stdin: None,
        timeout_secs: 30,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    }
}

#[test]
fn file_apply_patch_applies_add_update_delete_and_update_move_transactionally() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("update.txt"), "header\nbefore\nafter\n").unwrap();
    std::fs::write(tmp.path().join("delete.txt"), "remove\n").unwrap();
    std::fs::write(tmp.path().join("move.txt"), "head\nold\ntail\n").unwrap();

    let patch = "*** Begin Patch\n*** Add File: added.txt\n+added\n*** Update File: update.txt\n@@ header\n-before\n+updated\n*** Delete File: delete.txt\n*** Update File: move.txt\n*** Move to: nested/moved.txt\n@@ head\n-old\n+new\n*** End Patch";
    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_patch_request(tmp.path(), patch, false),
    ));

    assert_eq!(out["changed"], true);
    assert_eq!(out["state_changed"], true);
    assert_eq!(out["execution_state"], "completed");
    assert_eq!(out["applied_count"], 4);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("added.txt")).unwrap(),
        "added\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("update.txt")).unwrap(),
        "header\nupdated\nafter\n"
    );
    assert!(!tmp.path().join("delete.txt").exists());
    assert!(!tmp.path().join("move.txt").exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("nested/moved.txt")).unwrap(),
        "head\nnew\ntail\n"
    );
    assert_eq!(out["changed_paths"].as_array().unwrap().len(), 5);
}

#[test]
fn file_apply_patch_dry_run_reports_plan_without_writing() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("target.txt"), "old\n").unwrap();
    let patch = "*** Begin Patch\n*** Update File: target.txt\n-old\n+new\n*** End Patch";

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_patch_request(tmp.path(), patch, true),
    ));

    assert_eq!(out["dry_run"], true);
    assert_eq!(out["changed"], false);
    assert_eq!(out["state_changed"], false);
    assert_eq!(out["would_change"], true);
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("target.txt")).unwrap(),
        "old\n"
    );
}

#[test]
fn file_apply_patch_context_conflict_keeps_whole_batch_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("first.txt"), "one\n").unwrap();
    std::fs::write(tmp.path().join("second.txt"), "actual\n").unwrap();
    let patch = "*** Begin Patch\n*** Update File: first.txt\n-one\n+ONE\n*** Update File: second.txt\n-missing\n+changed\n*** End Patch";

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_patch_request(tmp.path(), patch, false),
    ));

    assert_eq!(out["error_kind"], "context_mismatch");
    assert_eq!(out["state_changed"], false);
    assert_eq!(out["execution_state"], "not_started");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("first.txt")).unwrap(),
        "one\n"
    );
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("second.txt")).unwrap(),
        "actual\n"
    );
}

#[test]
fn file_apply_patch_rejects_sensitive_paths_before_write() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let patch = "*** Begin Patch\n*** Add File: .env\n+SECRET=value\n*** End Patch";

    let out = line_edit_json(handle_file_request(
        &policy,
        &apply_patch_request(tmp.path(), patch, false),
    ));

    assert_eq!(out["error_kind"], "invalid_path");
    assert_eq!(out["state_changed"], false);
    assert!(!tmp.path().join(".env").exists());
    assert!(!serde_json::to_string(&out)
        .unwrap()
        .contains("SECRET=value"));
}
