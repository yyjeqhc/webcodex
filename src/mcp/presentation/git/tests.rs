use super::*;

#[test]
fn git_display_matches_diff_using_full_path_before_display_shortening() {
    let prefix = "a".repeat(300);
    let first = format!("{prefix}/first.rs");
    let second = format!("{prefix}/second.rs");
    let output = json!({"files":[{"path":first,"status":"modified"},{"path":second,"status":"modified"}],
        "hunks":[{"path":first,"hunks":[{"diff":"FIRST"}]},{"path":second,"hunks":[{"diff":"SECOND"}]}]});
    let projected = (CHANGES.project)("read_workspace_changes", &output).unwrap();
    assert_eq!(projected["files"][0]["path"], projected["files"][1]["path"]);
    assert_eq!(projected["files"][0]["diff_hunks"][0]["diff"], "FIRST");
    assert_eq!(projected["files"][1]["diff_hunks"][0]["diff"], "SECOND");
}

#[test]
fn git_display_keeps_path_and_text_filters_local_to_the_family() {
    for invalid in [
        "/absolute",
        "C:\\absolute",
        "../escape",
        "src/../escape",
        "\\\\server\\share",
        "https://remote",
        "src/\nfile",
    ] {
        assert!(
            safe_repo_relative_path(&json!(invalid)).is_none(),
            "{invalid:?}"
        );
    }
    let output = json!({"files":[{"path":"../escape"},{"path":"src/safe.rs","status":"modified"}],
        "hunks":[{"path":"src/safe.rs","hunks":[{"diff":"bad\u{0000}text"},{"diff":"+valid"}]}]});
    let before = output.clone();
    let projected = (CHANGES.project)("read_workspace_changes", &output).unwrap();
    assert_eq!(output, before);
    assert_eq!(projected["files"].as_array().unwrap().len(), 1);
    assert_eq!(projected["files"][0]["diff_hunks"][0]["diff"], "+valid");
    assert_eq!(projected["items_truncated"], true);
    assert_eq!(projected["diff_truncated"], true);
}
