use super::*;

#[test]
fn legacy_managed_temporary_record_remains_readable_as_registry_data() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("legacy-project");
    let project_registry_dir = tmp.path().join("project-registry");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(&project_registry_dir).unwrap();
    std::fs::write(
        project_registry_dir.join("legacy.toml"),
        format!(
            "id = \"legacy\"\nname = \"Legacy\"\npath = {:?}\nkind = \"managed_temporary\"\nallow_patch = true\n",
            project_dir.to_string_lossy()
        ),
    )
    .unwrap();

    let summaries = load_runner_project_summaries_from_dir(&project_registry_dir);
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "legacy");
    assert_eq!(summaries[0].kind.as_deref(), Some("managed_temporary"));
    assert_eq!(
        summaries[0].registration_source.as_deref(),
        Some("explicit")
    );
    assert_eq!(
        summaries[0].path,
        project_dir.canonicalize().unwrap().to_string_lossy()
    );
}

#[test]
fn legacy_auto_registered_record_keeps_raw_revision_and_projects_as_auto_registered() {
    let project = parse_runner_project_toml(
        r#"id = "legacy-auto"
path = "/tmp/legacy-auto"
name = "Legacy Auto"
kind = "auto_registered"
allow_patch = true
"#,
    )
    .unwrap();
    assert_eq!(project.registration_source, None);
    let summary = runner_project_summary(&project, 1, false);
    assert_eq!(
        summary.revision.as_deref(),
        Some("sha256:1275b96a7ed780e5fb6eecc9c4a0132bb8c2f358cb9ecf7415bc02aaab22f653"),
        "adding compatibility interpretation must not change the normalized legacy record CAS"
    );
    assert_eq!(summary.kind.as_deref(), Some("auto_registered"));
    assert_eq!(
        summary.registration_source.as_deref(),
        Some("auto_registered")
    );

    let mut conflicting = project.clone();
    conflicting.registration_source = Some("explicit".to_string());
    let summary = runner_project_summary(&conflicting, 1, false);
    assert_eq!(summary.registration_source.as_deref(), Some("explicit"));
}

#[test]
fn create_post_rename_sync_failure_preserves_source_and_registry() {
    let tmp = tempfile::tempdir().unwrap();
    let project_registry_dir = tmp.path().join("project-registry");
    let create_dir = tmp.path().join("created-after-rename");
    let policy = project_policy(tmp.path());
    webcodex_runner::projects::fail_next_project_parent_sync_after_rename();
    let error = project_err(handle_project_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "create_project",
            serde_json::json!({
                "id":"indeterminate", "name":"Indeterminate",
                "description":"Preserve me", "path":create_dir.to_string_lossy(),
                "allow_patch":true, "template":"basic", "git_init":true
            }),
        ),
    ));
    assert_eq!(error, "operation_indeterminate");
    assert!(project_registry_dir.join("indeterminate.toml").is_file());
    assert!(create_dir.join("README.md").is_file());
    assert!(create_dir.join(".gitignore").is_file());
    assert!(create_dir.join(".git").is_dir());
}

#[test]
fn register_and_create_retries_converge_without_duplicate_side_effects() {
    let tmp = tempfile::tempdir().unwrap();
    let project_registry_dir = tmp.path().join("project-registry");
    let register_dir = tmp.path().join("existing");
    std::fs::create_dir(&register_dir).unwrap();
    let policy = project_policy(tmp.path());
    let register = project_request(
        "register_project",
        serde_json::json!({
            "id":"registered", "name":"Registered",
            "path":register_dir.to_string_lossy(), "allow_patch":true
        }),
    );
    let first = project_ok(handle_project_op(&policy, &project_registry_dir, &register));
    assert_eq!(first["registration_source"], "explicit");
    let retry = project_ok(handle_project_op(&policy, &project_registry_dir, &register));
    assert_eq!(retry["recovered"], true);
    assert_eq!(retry["changed"], false);
    assert_eq!(retry["revision"], first["revision"]);

    let create_dir = tmp.path().join("created");
    let create = project_request(
        "create_project",
        serde_json::json!({
            "id":"created", "name":"Created", "description":"Fixture",
            "path":create_dir.to_string_lossy(), "allow_patch":true,
            "template":"basic", "git_init":true,
            "adopt_existing_empty":false
        }),
    );
    let created = project_ok(handle_project_op(&policy, &project_registry_dir, &create));
    assert_eq!(created["registration_source"], "explicit");
    let readme_before = std::fs::read(create_dir.join("README.md")).unwrap();
    let recovered = project_ok(handle_project_op(&policy, &project_registry_dir, &create));
    assert_eq!(recovered["recovered"], true);
    assert_eq!(recovered["changed"], false);
    assert_eq!(recovered["revision"], created["revision"]);
    assert_eq!(
        std::fs::read(create_dir.join("README.md")).unwrap(),
        readme_before
    );
    assert!(create_dir.join(".git").is_dir());

    let mismatch = project_err(handle_project_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id":"registered", "name":"Different",
                "path":register_dir.to_string_lossy(), "allow_patch":true
            }),
        ),
    ));
    assert_eq!(mismatch, "project_already_exists");
}

#[test]
fn project_lifecycle_persists_state_and_unregister_preserves_source() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let project_registry_dir = tmp.path().join("project-registry");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::create_dir(project_dir.join(".git")).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());
    let registered = project_ok(handle_project_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy()
            }),
        ),
    ));
    let revision = registered["revision"].as_str().unwrap().to_string();

    let disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":revision}),
        ),
    ));
    assert_eq!(disabled["outcome"], "disabled");
    let retry_disabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_disable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(retry_disabled["outcome"], "already_disabled");
    let disabled_revision = disabled["revision"].as_str().unwrap().to_string();
    let summaries = load_runner_project_summaries_from_dir(&project_registry_dir);
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].disabled);

    let stale = project_err(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(stale, "revision_conflict");

    let enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled_revision}),
        ),
    ));
    assert_eq!(enabled["outcome"], "enabled");
    let retry_enabled = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_enable",
            serde_json::json!({"project_id":"demo","expected_revision":disabled["revision"]}),
        ),
    ));
    assert_eq!(retry_enabled["outcome"], "already_enabled");

    let unregistered = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({
                "project_id":"demo",
                "expected_revision":enabled["revision"]
            }),
        ),
    ));
    assert_eq!(unregistered["outcome"], "unregistered");
    assert!(!project_registry_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());
    assert!(project_dir.join(".git").is_dir());

    let repeated = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(repeated["outcome"], "already_unregistered");

    let stale_tombstone = project_registry_dir.join(".demo.crash.toml.unregistering");
    std::fs::write(&stale_tombstone, "stale").unwrap();
    assert!(load_runner_project_summaries_from_dir(&project_registry_dir).is_empty());
    let recovered = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":enabled["revision"]}),
        ),
    ));
    assert_eq!(recovered["outcome"], "already_unregistered");
    assert!(!stale_tombstone.exists());
}

#[test]
fn project_unregister_post_rename_sync_failure_is_indeterminate_and_retry_converges() {
    let tmp = tempfile::tempdir().unwrap();
    let project_dir = tmp.path().join("repo");
    let project_registry_dir = tmp.path().join("project-registry");
    std::fs::create_dir(&project_dir).unwrap();
    std::fs::write(project_dir.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());
    let registered = project_ok(handle_project_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "register_project",
            serde_json::json!({
                "id": "demo",
                "name": "Demo",
                "path": project_dir.to_string_lossy()
            }),
        ),
    ));
    let revision = registered["revision"].as_str().unwrap().to_string();

    webcodex_runner::projects::fail_next_project_parent_sync_after_rename();
    let error = project_error_value(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":revision}),
        ),
    ));
    assert_eq!(error["error_code"], "operation_indeterminate");
    assert_eq!(error["state_changed"], true);
    assert!(!project_registry_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());

    let retry = project_ok(handle_project_lifecycle_op(
        &policy,
        &project_registry_dir,
        &project_request(
            "project_lifecycle_unregister",
            serde_json::json!({"project_id":"demo","expected_revision":registered["revision"]}),
        ),
    ));
    assert_eq!(retry["outcome"], "already_unregistered");
    assert_eq!(retry["changed"], false);
    assert!(!project_registry_dir.join("demo.toml").exists());
    assert!(project_dir.join("keep.txt").exists());
}
