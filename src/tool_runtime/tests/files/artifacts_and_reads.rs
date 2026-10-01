
#[test]
fn validate_edit_file_path_rejects_unsafe_and_sensitive_paths() {
    // Safe relative paths accepted.
    assert!(validate_edit_file_path("README.md").is_ok());
    assert!(validate_edit_file_path("src/main.rs").is_ok());
    assert!(validate_edit_file_path("a/b/c.txt").is_ok());
    assert!(validate_edit_file_path(".env.example").is_ok());
    assert!(validate_edit_file_path(".ENV.SAMPLE").is_ok());
    // Empty / NUL / absolute / traversal rejected.
    assert!(validate_edit_file_path("").is_err());
    assert!(validate_edit_file_path("src\0main.rs").is_err());
    assert!(validate_edit_file_path("/etc/passwd").is_err());
    assert!(validate_edit_file_path("../outside").is_err());
    assert!(validate_edit_file_path("src/../../outside").is_err());
    // Sensitive paths hard-rejected.
    for sensitive in [
        "runner.toml",
        "config/runner.toml",
        "runner.toml.bak",
        "agent.toml",
        "config/agent.toml",
        "agent.toml.bak",
        "webcodex.env",
        ".env",
        ".env.local",
        "secrets/projects.d/x",
        "project-registry",
        "projects.d",
        ".git/config",
        "target/debug/bin",
        "node_modules/pkg/index.js",
    ] {
        assert!(
            validate_edit_file_path(sensitive).is_err(),
            "sensitive path should be rejected: {}",
            sensitive
        );
    }
}

#[test]
fn is_sensitive_edit_path_is_component_wise_not_substring() {
    // Component-wise: a filename that merely contains a sensitive token
    // as a substring is NOT rejected.
    assert!(!is_sensitive_edit_path("targeting.md"));
    assert!(!is_sensitive_edit_path("enviroment.rs"));
    assert!(!is_sensitive_edit_path("docs/agent-toml-notes.md"));
    // Exact component matches ARE rejected.
    assert!(is_sensitive_edit_path("target/foo"));
    assert!(is_sensitive_edit_path(".git/HEAD"));
    assert!(is_sensitive_edit_path("node_modules/x"));
    assert!(is_sensitive_edit_path("a/b/.env"));
    assert!(!is_sensitive_edit_path(".env.example"));
    assert!(!is_sensitive_edit_path(".ENV.TEMPLATE"));
    assert!(is_sensitive_edit_path(".env.example.local"));
}

#[test]
fn is_hex_sha256_validates_lowercase_digest() {
    assert!(is_hex_sha256(
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    ));
    assert!(!is_hex_sha256("abc"));
    assert!(!is_hex_sha256(
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
    ));
    assert!(!is_hex_sha256(
        "z3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    ));
}

#[tokio::test]
async fn write_project_file_rejects_invalid_input_before_agent_dispatch() {
    let runtime = test_runtime();
    // NUL content
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "a\0b".to_string(),
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("NUL"));
    // sensitive path
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            ".env".to_string(),
            "x".to_string(),
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("sensitive"));
    // invalid model-facing read revision
    let result = runtime
        .write_project_file(
            "agent:c:p".to_string(),
            "EDIT_PROBE.txt".to_string(),
            "x".to_string(),
            Some(true),
            Some(0),
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("expected_read_revision"));
}

#[test]
fn removed_legacy_edit_tools_are_rejected_as_unknown() {
    // The 7 legacy edit tools are no longer known ToolDefinitions, so any
    // dispatch attempt resolves to the standard unknown-tool rejection before
    // any project or file-op work. This replaces the old back-compat dispatch
    // tests for these tools.
    for name in [
        "replace_in_file",
        "replace_exact_block",
        "insert_before_pattern",
        "insert_after_pattern",
        "replace_line_range",
        "insert_at_line",
        "delete_line_range",
    ] {
        let err = ToolCall::from_tool_name(name, json!({})).unwrap_err();
        assert!(err.contains("unknown tool"), "{name}: {err}");
    }
}

#[test]
fn validate_artifact_file_path_rejects_unsafe_and_sensitive_paths() {
    assert!(validate_artifact_file_path("docs/assets/generated.png").is_ok());
    for path in [
        "/absolute/evil.png",
        "\\rooted\\evil.png",
        "C:\\absolute\\evil.png",
        "C:drive-relative\\evil.png",
        "../evil.png",
        "nested\\..\\evil.png",
        ".git/config",
        ".git\\config",
        ".env",
        "secrets/key.pem",
        "secrets\\key.pem",
        "tokens/api.txt",
        "certs\\server.key",
        "config\\agent.toml",
        "target/out.bin",
        "node_modules/pkg/file",
    ] {
        assert!(
            validate_artifact_file_path(path).is_err(),
            "{} should be rejected",
            path
        );
    }
}

#[tokio::test]
async fn read_project_artifact_rejects_sensitive_path_before_resolving_project() {
    let out = test_runtime()
        .read_project_artifact(
            "agent:missing:missing".to_string(),
            ".env".to_string(),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await;
    assert!(!out.success);
    assert!(out.error.unwrap().contains("sensitive artifact path"));
}

#[tokio::test]
async fn read_project_artifact_rejects_invalid_length_before_resolving_project() {
    let out = test_runtime()
        .read_project_artifact(
            "agent:missing:missing".to_string(),
            "docs/assets/file.png".to_string(),
            None,
            None,
            Some(crate::tool_runtime::files::MAX_READ_PROJECT_ARTIFACT_LENGTH + 1),
            None,
            None,
            None,
        )
        .await;
    assert!(!out.success);
    assert!(out.error.unwrap().contains("length too large"));
}

#[tokio::test]
async fn read_project_artifact_rejects_invalid_expected_sha256_before_resolving_project() {
    let out = test_runtime()
        .read_project_artifact(
            "agent:missing:missing".to_string(),
            "docs/assets/file.bin".to_string(),
            None,
            None,
            Some(4),
            Some("A".repeat(64)),
            None,
            None,
        )
        .await;
    assert!(!out.success);
    assert_eq!(out.output["error_kind"], "invalid_expected_sha256");
    assert!(out.error.unwrap().contains("expected_sha256"));
}

#[tokio::test]
async fn office_artifact_mime_policy_accepts_matching_save_and_upload_paths() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let cases = [
        (
            "docs/report.docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docs/report.pptx",
        ),
        (
            "slides/deck.pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "slides/deck.xlsx",
        ),
        (
            "data/book.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "data/book.docx",
        ),
    ];

    for (path, mime, mismatched_path) in cases {
        let save = runtime
            .save_project_artifact(
                missing_project.clone(),
                path.to_string(),
                "YQ==".to_string(),
                Some(mime.to_string()),
                Some(false),
            )
            .await;
        assert!(!save.success, "{path}");
        assert!(
            !save
                .error
                .as_deref()
                .unwrap()
                .contains("unsupported mime_type")
                && !save
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("requires a matching"),
            "matching Office MIME should pass policy before project resolution: {:?}",
            save.error
        );

        let upload = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                Some(1),
                None,
                Some(mime.to_string()),
                Some(false),
            )
            .await;
        assert!(!upload.success, "{path}");
        assert!(
            !upload
                .error
                .as_deref()
                .unwrap()
                .contains("unsupported mime_type")
                && !upload
                    .error
                    .as_deref()
                    .unwrap()
                    .contains("requires a matching"),
            "matching Office upload MIME should pass policy before project resolution: {:?}",
            upload.error
        );

        let octet = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                Some(1),
                None,
                Some("application/octet-stream".to_string()),
                Some(false),
            )
            .await;
        assert!(!octet.success, "{path}");
        assert!(
            !octet.error.as_deref().unwrap().contains("policy"),
            "generic binary MIME should pass policy before project resolution: {:?}",
            octet.error
        );

        let mismatched = runtime
            .save_project_artifact(
                missing_project.clone(),
                mismatched_path.to_string(),
                "YQ==".to_string(),
                Some(mime.to_string()),
                Some(false),
            )
            .await;
        assert!(!mismatched.success);
        assert!(
            mismatched
                .error
                .as_deref()
                .unwrap()
                .contains("requires a matching"),
            "{:?}",
            mismatched.error
        );
    }

    let unknown_mime = runtime
        .save_project_artifact(
            missing_project,
            "docs/report.customblob".to_string(),
            "YQ==".to_string(),
            Some("application/x-unknown".to_string()),
            Some(false),
        )
        .await;
    assert!(!unknown_mime.success);
    assert!(
        !unknown_mime
            .error
            .as_deref()
            .unwrap()
            .contains("mime_type"),
        "unknown presentation MIME should normalize to generic binary before project resolution: {:?}",
        unknown_mime.error
    );
}

#[tokio::test]
async fn common_media_artifact_mime_policy_accepts_save_upload_and_octet_paths() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    for (path, mime) in [
        ("media/sample.mp3", "audio/mpeg"),
        ("media/sample.mp4", "video/mp4"),
    ] {
        let save = runtime
            .save_project_artifact(
                missing_project.clone(),
                path.to_string(),
                "YQ==".to_string(),
                Some(mime.to_string()),
                Some(false),
            )
            .await;
        assert!(!save.success, "{path}");
        assert!(
            !save
                .error
                .as_deref()
                .unwrap()
                .contains("unsupported mime_type"),
            "media MIME should pass policy before project resolution: {:?}",
            save.error
        );

        let upload = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                Some(1),
                None,
                Some(mime.to_string()),
                Some(false),
            )
            .await;
        assert!(!upload.success, "{path}");
        assert!(
            !upload
                .error
                .as_deref()
                .unwrap()
                .contains("unsupported mime_type"),
            "media upload MIME should pass policy before project resolution: {:?}",
            upload.error
        );

        let octet = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                Some(1),
                None,
                Some("application/octet-stream".to_string()),
                Some(false),
            )
            .await;
        assert!(!octet.success, "{path}");
        assert!(
            !octet.error.as_deref().unwrap().contains("mime_type"),
            "generic binary MIME should pass policy before project resolution: {:?}",
            octet.error
        );
    }
}

#[tokio::test]
async fn artifact_upload_begin_rejects_invalid_inputs_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let cases = [
        (
            ".env",
            Some(1),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            Some("text/plain"),
            "sensitive artifact path",
        ),
        (
            "artifacts/imports/bad-hash.txt",
            Some(1),
            Some("not-a-sha"),
            Some("text/plain"),
            "expected_sha256 must be a lowercase 64-char hex sha256 digest",
        ),
        (
            "artifacts/imports/too-large.txt",
            Some(MAX_PROJECT_ARTIFACT_UPLOAD_BYTES + 1),
            None,
            Some("text/plain"),
            "expected_bytes too large",
        ),
    ];

    for (path, expected_bytes, expected_sha256, mime_type, expected_error) in cases {
        let out = runtime
            .artifact_upload_begin(
                missing_project.clone(),
                path.to_string(),
                expected_bytes,
                expected_sha256.map(str::to_string),
                mime_type.map(str::to_string),
                Some(false),
            )
            .await;
        assert!(!out.success, "{path}");
        assert!(
            out.error.as_deref().unwrap().contains(expected_error),
            "{path}: {:?}",
            out.error
        );
    }
}

#[tokio::test]
async fn artifact_upload_chunk_rejects_invalid_inputs_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let path = "artifacts/imports/chunk.txt".to_string();

    let invalid_id = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "bad-upload-id".to_string(),
            0,
            "YQ==".to_string(),
        )
        .await;
    assert!(!invalid_id.success);
    assert!(invalid_id.error.unwrap().contains("upload_id must start"));

    let invalid_base64 = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "wc_upload_test_1".to_string(),
            0,
            "not valid base64!".to_string(),
        )
        .await;
    assert!(!invalid_base64.success);
    assert!(invalid_base64.error.unwrap().contains("invalid base64"));

    let empty = runtime
        .artifact_upload_chunk(
            missing_project.clone(),
            path.clone(),
            "wc_upload_test_1".to_string(),
            0,
            "".to_string(),
        )
        .await;
    assert!(!empty.success);
    assert!(empty
        .error
        .unwrap()
        .contains("decoded chunk must contain at least 1 byte"));

    let oversized = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        vec![b'x'; MAX_PROJECT_ARTIFACT_UPLOAD_CHUNK_BYTES + 1],
    );
    let oversized = runtime
        .artifact_upload_chunk(
            missing_project,
            path,
            "wc_upload_test_1".to_string(),
            0,
            oversized,
        )
        .await;
    assert!(!oversized.success);
    assert!(oversized.error.unwrap().contains("decoded chunk too large"));
}

#[tokio::test]
async fn artifact_upload_finish_and_abort_reject_invalid_upload_id_before_resolving_project() {
    let runtime = test_runtime();
    let missing_project = "agent:missing:missing".to_string();
    let path = "artifacts/imports/file.txt".to_string();

    let finish = runtime
        .artifact_upload_finish(missing_project.clone(), path.clone(), "bad".to_string())
        .await;
    assert!(!finish.success);
    assert!(finish.error.unwrap().contains("upload_id must start"));

    let abort = runtime
        .artifact_upload_abort(missing_project, path, "bad".to_string())
        .await;
    assert!(!abort.success);
    assert!(abort.error.unwrap().contains("upload_id must start"));
}

#[tokio::test]
async fn read_file_routes_safe_and_bulk_skipped_explicit_paths_to_agent() {
    for (client_id, path, content) in [
        ("relative-read", "src/main.rs", "fn main() {}\n"),
        (
            "bulk-explicit-read",
            "node_modules/foo/package.json",
            "{}\n",
        ),
        (
            "dotenv-template-read",
            ".env.example",
            "EXAMPLE_ONLY=fake\n",
        ),
    ] {
        let runtime = runtime_with_agent_project(client_id);
        register_agent(
            &runtime,
            client_id,
            None,
            RunnerCapabilities {
                file_read: true,
                ..Default::default()
            },
        )
        .await;
        let project = agent_test_project_id(client_id);
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let path = path.to_string();
            async move { runtime.read_file(project, path, None, None, None).await }
        });

        let request = wait_for_patch_agent_request(&runtime, client_id).await;
        assert_eq!(request.kind, "file_read", "{path}");
        assert_eq!(request.path.as_deref(), Some(path), "{path}");
        complete_agent_ranged_file_read_request(&runtime, client_id, &request, content).await;
        let result = task.await.unwrap();
        assert!(result.success, "{path}: {:?}", result.error);
    }
}

#[tokio::test]
async fn read_file_refuses_secret_paths_before_reaching_agent() {
    // Search excluded credentials, artifacts and edits rejected them, but
    // read_file returned them verbatim. Case variants must be refused too:
    // the old search predicate was case-sensitive.
    let runtime = runtime_with_agent_project("secret-read");
    let caps = RunnerCapabilities {
        file_read: true,
        ..Default::default()
    };
    register_agent(&runtime, "secret-read", None, caps).await;
    let project = agent_test_project_id("secret-read");

    for path in [
        ".git/config",
        ".git/HEAD",
        ".env",
        ".env.production",
        "app/.env.local",
        ".env.example.local",
        ".env.example.bak",
        ".env.production.example",
        ".ENV",
        "certs/server.pem",
        "certs/server.key",
        "certs/Server.PEM",
        "runner.toml",
        "agent.toml",
        "secrets/token",
        "tokens/agent",
        "project-registry/demo.toml",
        "projects.d/demo.toml",
    ] {
        let result = runtime
            .read_file(project.clone(), path.to_string(), None, None, None)
            .await;
        assert!(!result.success, "read_file returned secret path {path:?}");
        assert!(
            result
                .error
                .as_deref()
                .is_some_and(|error| error.contains("sensitive")),
            "unexpected error for {path:?}: {:?}",
            result.error
        );
    }

    assert!(
        probe_patch_agent_request(&runtime, "secret-read")
            .await
            .is_none(),
        "a refused secret path still reached the agent"
    );
}

/// `--no-ignore` used to be passed to ripgrep, so a search walked straight
/// through `.gitignore` and returned build output, virtualenvs, and vendored
/// trees. Dropping it means the project's own ignore rules apply.
#[cfg(unix)]
#[test]
fn search_project_text_respects_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    std::fs::create_dir_all(root.join("build")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
    std::fs::write(root.join("src/kept.rs"), "needle here\n").unwrap();
    std::fs::write(root.join("build/generated.rs"), "needle here\n").unwrap();

    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let command = search_project_text_command(&options);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
    if exit_code == 127 {
        return; // no ripgrep on this host; the grep fallback is covered below
    }
    assert!(exit_code == 0 || exit_code == 1, "stderr={stderr}");
    assert!(stdout.contains("kept.rs"), "stdout={stdout}");
    assert!(
        !stdout.contains("generated.rs"),
        "gitignored build output was searched: {stdout}"
    );
}

/// Honouring `.gitignore` must not become the only protection: a repository
/// that commits its `.env` — or simply does not ignore it — still has to be
/// excluded from search results.
#[cfg(unix)]
#[test]
fn search_project_text_still_excludes_sensitive_paths_not_in_gitignore() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    // Deliberately no .gitignore: nothing here is ignored by the project.
    std::fs::write(root.join("src/kept.rs"), "needle here\n").unwrap();
    std::fs::write(root.join(".env"), "needle here\n").unwrap();

    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let command = search_project_text_command(&options);
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
    if exit_code == 127 {
        return;
    }
    assert!(exit_code == 0 || exit_code == 1, "stderr={stderr}");
    assert!(stdout.contains("kept.rs"), "stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(
        !serialized.contains(".env"),
        "a sensitive path reached the search result: {serialized}"
    );
}

/// The grep fallback cannot read `.gitignore`, so it keeps an explicit exclude
/// list. That list is defence in depth, not a substitute — it must still be
/// there.
#[cfg(unix)]
#[test]
fn grep_fallback_keeps_defense_in_depth_excludes() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(root.join("node_modules")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/kept.rs"), "needle here\n").unwrap();
    std::fs::write(root.join("node_modules/vendored.js"), "needle here\n").unwrap();
    // A PATH holding the real grep but no ripgrep, so the command genuinely
    // takes the fallback branch.
    for tool in ["grep", "head", "sh"] {
        if let Ok(found) = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {tool}"))
            .output()
        {
            let path = String::from_utf8_lossy(&found.stdout).trim().to_string();
            if !path.is_empty() {
                let _ = std::os::unix::fs::symlink(&path, bin.join(tool));
            }
        }
    }

    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
    assert!(
        exit_code == 0 || exit_code == 1,
        "exit={exit_code} stderr={stderr} stdout={stdout}"
    );
    assert!(stdout.contains("kept.rs"), "stdout={stdout}");
    assert!(
        !stdout.contains("vendored.js"),
        "grep fallback lost its node_modules exclude: {stdout}"
    );
}
