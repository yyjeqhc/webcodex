use super::*;

#[test]
fn structured_delete_project_files_is_os_neutral_file_only_and_bounded() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("delete-me.txt"), "content").unwrap();

    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": ["delete-me.txt", "missing.txt"]}),
    );
    assert!(request.command.is_empty());
    let result = handle_file_request(&policy, &request);
    assert_eq!(result.exit_code, Some(0), "{:?}", result.error);
    assert!(!tmp.path().join("delete-me.txt").exists());
    let output: serde_json::Value =
        serde_json::from_str(result.stdout.as_deref().expect("structured JSON result")).unwrap();
    assert_eq!(
        output["deleted_paths"],
        serde_json::json!(["delete-me.txt", "missing.txt"])
    );
    assert_eq!(output["missing_paths"], serde_json::json!([]));
    assert_eq!(output["refused_paths"], serde_json::json!([]));

    std::fs::create_dir(tmp.path().join("directory-target")).unwrap();
    let directory = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": ["directory-target"]}),
    );
    let result = handle_file_request(&policy, &directory);
    assert_eq!(
        result.error.as_deref(),
        Some("delete_project_files refuses directory targets")
    );
    assert!(tmp.path().join("directory-target").is_dir());

    for path in [".", "../escape", ".env", "target/cache"] {
        let refused = json_file_op_request(
            tmp.path(),
            "file_delete_project_files",
            ".",
            serde_json::json!({"paths": [path]}),
        );
        let result = handle_file_request(&policy, &refused);
        assert_eq!(
            result.error.as_deref(),
            Some("delete_project_files request contains a refused path"),
            "{path}"
        );
    }

    let too_many = (0..65)
        .map(|index| format!("file-{index}.txt"))
        .collect::<Vec<_>>();
    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": too_many}),
    );
    let result = handle_file_request(&policy, &request);
    assert_eq!(
        result.error.as_deref(),
        Some("delete_project_files request contains a refused path")
    );
}

#[test]
fn structured_delete_project_files_errors_do_not_leak_absolute_paths() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    let absolute = tmp.path().join("secret.txt").to_string_lossy().to_string();
    let request = json_file_op_request(
        tmp.path(),
        "file_delete_project_files",
        ".",
        serde_json::json!({"paths": [absolute]}),
    );
    let result = handle_file_request(&policy, &request);
    let error = result.error.expect("absolute path must be rejected");
    assert_eq!(
        error,
        "delete_project_files request contains a refused path"
    );
    assert!(!error.contains(&tmp.path().to_string_lossy().to_string()));
}
