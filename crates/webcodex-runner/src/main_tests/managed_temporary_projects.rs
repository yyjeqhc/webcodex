use super::*;

#[test]
fn managed_temporary_project_is_registered_persistent_and_ordinary_project_compatible() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let policy = project_policy(tmp.path());
    let request = project_request(
        "create_project",
        serde_json::json!({
            "managed_temporary_project": true,
            "name": "Scratch task"
        }),
    );

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &request,
    ));
    let id = created["agent_project_id"].as_str().unwrap();
    let path = PathBuf::from(created["path"].as_str().unwrap());
    let canonical_root = temporary_root.canonicalize().unwrap();

    assert_eq!(created["source"], "managed_temporary");
    assert_eq!(created["kind"], "managed_temporary");
    assert_eq!(created["git_initialized"], true);
    assert_eq!(path.parent(), Some(canonical_root.as_path()));
    assert!(path.is_dir());
    assert!(path.join(".git").is_dir());
    assert_eq!(
        path.canonicalize().unwrap(),
        path,
        "the returned project path must be canonical"
    );

    let persisted = parse_runner_project_toml(
        &std::fs::read_to_string(projects_dir.join(format!("{id}.toml"))).unwrap(),
    )
    .unwrap();
    assert_eq!(persisted.kind.as_deref(), Some("managed_temporary"));
    assert_eq!(persisted.path, path.to_string_lossy());

    // A fresh project-registry scan models a Runner restart: it finds the same
    // ordinary project record, including its source marker and canonical path.
    let reloaded = load_runner_project_summaries_from_dir(&projects_dir);
    assert_eq!(reloaded.len(), 1);
    assert_eq!(reloaded[0].id, id);
    assert_eq!(reloaded[0].kind.as_deref(), Some("managed_temporary"));
    assert_eq!(reloaded[0].path, path.to_string_lossy());

    // The normal shell and structured project-overview paths receive the
    // registered path with no temporary-project special case.
    let shell = run_shell(
        &policy,
        &ShellConfig::default(),
        Some(path.to_string_lossy().as_ref()),
        &shell_echo("managed-shell"),
        None,
        10,
        None,
    );
    assert_eq!(shell.exit_code, Some(0), "{shell:?}");
    assert_eq!(shell.stdout.as_deref(), Some("managed-shell"));

    std::fs::write(path.join("README.md"), "managed\n").unwrap();
    let overview_request = json_file_op_request(
        &path,
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 1, "limit": 20}),
    );
    let overview = project_ok(handle_file_request(&policy, &overview_request));
    assert_eq!(overview["schema_version"], 1);
    assert!(overview.to_string().contains("README.md"));
}

#[test]
fn managed_temporary_project_rejects_path_traversal_and_never_overwrites_existing_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let temporary_root = tmp.path().join("temporary-projects");
    let projects_dir = tmp.path().join("projects.d");
    std::fs::create_dir(&temporary_root).unwrap();
    let pre_existing = temporary_root.join("scratch");
    std::fs::create_dir(&pre_existing).unwrap();
    std::fs::write(pre_existing.join("keep.txt"), "keep").unwrap();
    let policy = project_policy(tmp.path());

    let traversal = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "Scratch",
                "path": "../escape"
            }),
        ),
    ));
    assert_eq!(traversal, "invalid_request");
    assert!(!tmp.path().join("escape").exists());

    let path_like_name = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "../escape"
            }),
        ),
    ));
    assert_eq!(path_like_name, "invalid_request");

    let created = project_ok(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(&temporary_root),
        &project_request(
            "create_project",
            serde_json::json!({
                "managed_temporary_project": true,
                "name": "scratch"
            }),
        ),
    ));
    assert_ne!(
        created["path"].as_str(),
        Some(pre_existing.to_string_lossy().as_ref())
    );
    assert_eq!(
        std::fs::read_to_string(pre_existing.join("keep.txt")).unwrap(),
        "keep"
    );
}

#[test]
fn managed_temporary_project_requires_root_inside_runner_policy() {
    let allowed = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let projects_dir = allowed.path().join("projects.d");
    let policy = project_policy(allowed.path());

    let error = project_err(handle_project_op_with_temporary_projects_root(
        &policy,
        &projects_dir,
        Some(outside.path()),
        &project_request(
            "create_project",
            serde_json::json!({"managed_temporary_project": true}),
        ),
    ));
    assert_eq!(error, "temporary_projects_root_outside_allowed_roots");
    assert!(!projects_dir.exists());
}
