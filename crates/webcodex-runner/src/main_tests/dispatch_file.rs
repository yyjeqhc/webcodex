use super::*;

#[test]
fn file_request_kind_includes_edit_and_basic_ops() {
    for kind in [
        "file_read",
        "file_write",
        "file_list",
        "file_project_overview",
        "file_delete_project_files",
        "file_skill_list_packages",
        "file_skill_read_file",
        "file_write_project_file",
        "file_apply_text_edits",
    ] {
        assert!(
            is_file_request_kind(kind),
            "{kind} should route to file handler"
        );
    }
    // Removed legacy edit request kinds no longer route to the file handler.
    for kind in [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
    ] {
        assert!(
            !is_file_request_kind(kind),
            "{kind} must no longer be a file request kind"
        );
    }
    assert!(!is_file_request_kind("run_shell"));
}

#[test]
fn project_overview_runner_request_returns_metadata_without_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());
    std::fs::write(tmp.path().join("Cargo.toml"), "private manifest content").unwrap();
    std::fs::write(tmp.path().join("README.md"), "private readme content").unwrap();
    std::fs::write(tmp.path().join(".env"), "TOKEN=not-returned").unwrap();
    let request = json_file_op_request(
        tmp.path(),
        "file_project_overview",
        ".",
        serde_json::json!({"max_depth": 2, "limit": 200}),
    );

    let output = line_edit_json(handle_file_request(&policy, &request));
    assert_eq!(output["schema_version"], 1);
    assert_eq!(output["deterministic"], true);
    assert!(output.to_string().contains("Cargo.toml"));
    assert!(!output.to_string().contains("private manifest content"));
    assert!(!output.to_string().contains("TOKEN=not-returned"));
    assert!(!output.to_string().contains(".env"));
    assert!(!output
        .to_string()
        .contains(&tmp.path().display().to_string()));
}

#[test]
fn skill_file_ops_are_project_contained_text_only_and_path_private() {
    let tmp = tempfile::tempdir().unwrap();
    let policy = project_policy(tmp.path());

    let empty_request = json_file_op_request(
        tmp.path(),
        "file_skill_list_packages",
        ".agents/skills",
        serde_json::json!({"limit": 257}),
    );
    let empty = line_edit_json(handle_file_request(&policy, &empty_request));
    assert_eq!(empty["format"], "webcodex.skill_package_list.v1");
    assert_eq!(empty["entries"].as_array().unwrap().len(), 0);
    assert_eq!(empty["truncated"], false);

    let skill_root = tmp.path().join(".agents/skills/foo");
    std::fs::create_dir_all(skill_root.join("references")).unwrap();
    std::fs::write(
        skill_root.join("SKILL.md"),
        "---\nname: foo\ndescription: demo\n---\nline one\nline two\n",
    )
    .unwrap();
    std::fs::write(
        skill_root.join("references/guide.md"),
        "alpha\nbeta\ngamma\n",
    )
    .unwrap();
    std::fs::write(skill_root.join("references/binary.dat"), [0xff, 0xfe]).unwrap();
    std::fs::write(skill_root.join(".env"), "TOKEN=secret\n").unwrap();

    let listed = line_edit_json(handle_file_request(
        &policy,
        &json_file_op_request(
            tmp.path(),
            "file_skill_list_packages",
            ".agents/skills",
            serde_json::json!({"limit": 257}),
        ),
    ));
    assert_eq!(listed["entries"][0]["name"], "foo");
    assert_eq!(listed["entries"][0]["kind"], "dir");
    assert!(!listed
        .to_string()
        .contains(&tmp.path().display().to_string()));

    let mut read = json_file_op_request(
        tmp.path(),
        "file_skill_read_file",
        ".agents/skills/foo/references/guide.md",
        serde_json::json!({
            "package_root": ".agents/skills/foo",
            "max_file_bytes": 524288
        }),
    );
    read.start_line = Some(2);
    read.end_line = Some(2);
    read.max_bytes = Some(48 * 1024);
    let output = line_edit_json(handle_file_request(&policy, &read));
    assert_eq!(output["format"], "webcodex.skill_file_read.v1");
    assert_eq!(output["content"], "beta");
    assert_eq!(output["start_line"], 2);
    assert_eq!(output["end_line"], 2);
    assert!(output["sha256"].as_str().unwrap().len() == 64);
    assert!(!output
        .to_string()
        .contains(&tmp.path().display().to_string()));

    let mut sensitive = read.clone();
    sensitive.path = Some(".agents/skills/foo/.env".to_string());
    let sensitive_result = handle_file_request(&policy, &sensitive);
    assert_eq!(sensitive_result.exit_code, None);
    assert_eq!(
        sensitive_result.error.as_deref(),
        Some("skill_sensitive_path")
    );
    assert_eq!(sensitive_result.stdout, None);

    let mut binary = read.clone();
    binary.path = Some(".agents/skills/foo/references/binary.dat".to_string());
    let binary_result = handle_file_request(&policy, &binary);
    assert_eq!(binary_result.exit_code, None);
    assert_eq!(binary_result.error.as_deref(), Some("skill_invalid_utf8"));
    assert_eq!(binary_result.stdout, None);

    let mut too_large = read.clone();
    too_large.content = Some(
        serde_json::json!({
            "package_root": ".agents/skills/foo",
            "max_file_bytes": 4
        })
        .to_string(),
    );
    let too_large_result = handle_file_request(&policy, &too_large);
    assert_eq!(
        too_large_result.error.as_deref(),
        Some("skill_file_too_large")
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let outside = tmp.path().join("outside.txt");
        std::fs::write(&outside, "outside\n").unwrap();
        symlink(&outside, skill_root.join("references/escape.md")).unwrap();
        let mut escape = read.clone();
        escape.path = Some(".agents/skills/foo/references/escape.md".to_string());
        let escape_result = handle_file_request(&policy, &escape);
        assert_eq!(escape_result.exit_code, None);
        assert_eq!(escape_result.error.as_deref(), Some("skill_path_escape"));
        assert_eq!(escape_result.stdout, None);
    }
}

#[test]
fn dispatch_request_edit_routes_to_file_handler() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let cwd = tmp.path().to_string_lossy().to_string();
    let (sink, mut rx) = ws_sink("ws-client");
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let request = ShellAgentShellRequest {
        request_id: "req-edit".to_string(),
        client_id: "ws-client".to_string(),
        kind: "file_write_project_file".to_string(),
        job_id: None,
        cwd: Some(cwd),
        path: Some("new.txt".to_string()),
        content: Some(
            serde_json::json!({
                "path": "new.txt",
                "content": "new content\n",
                "overwrite": false,
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
        timeout_secs: 10,
        requested_by: "tester".to_string(),
        created_at: 0,
        validation: None,
        lsp: None,
        job_context: None,
        mcp_gateway: None,
        coding_agent: None,
        persistent_shell: None,
    };
    let pdir = projects_dir(&cfg).unwrap();
    let lsp = webcodex_runner::LspSupervisor::default();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let ran = dispatch_request(
        &sink,
        &hot.snapshot(),
        &hot,
        &jobs,
        &persistent_shells,
        &pdir,
        &lsp,
        request,
    )
    .unwrap();
    assert!(ran);
    let env = rx.try_recv().expect("result envelope was sent");
    match env {
        AgentEnvelope::Result { payload } => {
            assert_eq!(payload.result.request_id, "req-edit");
            assert_eq!(payload.result.exit_code, Some(0));
            let stdout = payload
                .result
                .stdout
                .expect("file handler returns JSON stdout");
            assert!(stdout.contains("\"created\":true"), "stdout was {stdout}");
            assert_eq!(
                std::fs::read_to_string(tmp.path().join("new.txt")).unwrap(),
                "new content\n"
            );
        }
        other => panic!("expected result, got {:?}", other.kind()),
    }
}

#[test]
fn dispatch_request_rejects_unsupported_file_kinds_without_starting_command() {
    let tmp = tempfile::tempdir().unwrap();
    let cfg = test_config(tmp.path().join("config/projects.d"));
    let jobs = JobManager::new(max_concurrent_jobs(&cfg));
    let pdir = projects_dir(&cfg).unwrap();
    let hot = runtime_config(&cfg);
    let persistent_shells = webcodex_runner::PersistentShellManager::new(
        &cfg.shell,
        webcodex_runner::SshConnectionPool::default(),
    );
    let kinds = [
        "file_replace_line_range",
        "file_insert_at_line",
        "file_delete_line_range",
        "file_replace_exact_block",
        "file_insert_before_pattern",
        "file_insert_after_pattern",
        "file_replace_in_file",
        "file_future_unknown_operation",
    ];

    for (index, kind) in kinds.into_iter().enumerate() {
        let marker_name = format!("unsupported-file-marker-{index}");
        let target_name = format!("unsupported-file-target-{index}.txt");
        let marker = tmp.path().join(&marker_name);
        let target = tmp.path().join(&target_name);
        std::fs::write(&target, "original\n").unwrap();
        let command = format!(
            "printf shell-ran > {marker_name}; printf modified > {target_name}; printf shell-stdout"
        );
        let request: ShellAgentShellRequest = serde_json::from_value(serde_json::json!({
            "request_id": format!("req-unsupported-file-{index}"),
            "client_id": "ws-client",
            "kind": kind,
            "cwd": tmp.path().to_string_lossy(),
            "path": target_name,
            "content": "replacement",
            "command": command,
            "old_text": "old",
            "pattern": "needle",
            "line": 10,
            "timeout_secs": 10,
            "requested_by": "tester",
            "created_at": 0,
        }))
        .unwrap();
        let (sink, mut rx) = ws_sink("ws-client");

        let ran = dispatch_request(
            &sink,
            &hot.snapshot(),
            &hot,
            &jobs,
            &persistent_shells,
            &pdir,
            &webcodex_runner::LspSupervisor::default(),
            request,
        )
        .unwrap();

        assert!(ran, "{kind}");
        let env = rx.try_recv().expect("result envelope was sent");
        match env {
            AgentEnvelope::Result { payload } => {
                assert_eq!(payload.result.exit_code, None, "{kind}");
                assert_eq!(payload.result.stdout, None, "{kind}");
                assert_eq!(
                    payload.result.error.as_deref(),
                    Some(
                        "unsupported_file_request_kind: unsupported file request kind; command was not started"
                    ),
                    "{kind}"
                );
            }
            other => panic!("{kind}: expected result, got {:?}", other.kind()),
        }
        assert!(!marker.exists(), "{kind}: shell marker was created");
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "original\n",
            "{kind}: target file was modified"
        );
    }
}
