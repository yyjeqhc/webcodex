
#[test]
fn search_project_text_command_excludes_sensitive_dirs_and_bounds_output() {
    let options = SearchOptions::normalize(SearchRequest {
        pattern: "fn main".to_string(),
        path: Some("src".to_string()),
        limit: Some(25),
        context_before: None,
        context_after: None,
        include_globs: None,
        exclude_globs: None,
        result_mode: None,
        timeout_secs: None,
    })
    .unwrap();
    let cmd = search_project_text_command(&options);
    assert!(cmd.contains("command -v rg"));
    assert!(cmd.contains("\"backend\":\"rg\""));
    assert!(cmd.contains("\"backend\":\"grep\""));
    assert!(cmd.contains("rg --with-filename"));
    assert!(cmd.contains("--glob '!**/.git/**'"));
    assert!(cmd.contains("--glob '!**/.claude/**'"));
    assert!(cmd.contains("--glob '!**/target/**'"));
    assert!(cmd.contains("--glob '!**/node_modules/**'"));
    assert!(cmd.contains("--exclude-dir=.git"));
    assert!(cmd.contains("--exclude-dir=.claude"));
    assert!(cmd.contains("--exclude-dir=target"));
    assert!(cmd.contains("--exclude-dir=node_modules"));
    assert!(cmd.contains("--exclude-dir=secrets"));
    assert!(cmd.contains("--exclude=.env"));
    assert!(cmd.contains("--exclude=*.key"));
    assert!(cmd.contains("\"$head_cmd\" -n 26") || cmd.contains("$head_cmd -n 26"));
    assert!(cmd.contains("trap 'cleanup_search_status' EXIT"));
    assert!(cmd.contains("trap 'cleanup_search_status; exit 143' HUP INT TERM"));
    assert!(cmd.contains("grep -rHnI --null"));
    assert!(cmd.contains("command -v head"));
    assert!(cmd.contains("/usr/bin/head") || cmd.contains("/bin/head"));
    // No global path sort: matches must stream in traversal order so a small
    // limit can stop the backend early instead of buffering the whole repo.
    assert!(
        !cmd.contains("--sort"),
        "search command must not globally sort: {cmd}"
    );
    // A second head stage emits one probe byte beyond the formal budget; the
    // parser consumes the probe only to prove truncation and never exposes it.
    assert!(
        cmd.contains(&format!("-c {}", SEARCH_OUTPUT_BYTE_BUDGET + 1)),
        "search command must cap output bytes with one probe byte: {cmd}"
    );
}

#[cfg(unix)]
fn write_executable_script(path: &std::path::Path, body: &str) {
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

#[cfg(unix)]
fn fake_head_script() -> &'static str {
    "#!/bin/sh\nwhile IFS= read -r line; do\n  printf '%s\\n' \"$line\"\ndone\n"
}

/// Whether the host environment has a working real `rg` (ripgrep) on PATH.
///
/// Used only by integration tests that exercise advanced `search_project_text`
/// features against the installed backend. Fake-`rg` / controlled-PATH tests
/// must not call this — they supply their own backend.
fn host_ripgrep_available() -> bool {
    std::process::Command::new("rg")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(unix)]
fn symlink_host_command(command: &str, bin: &std::path::Path) {
    let found = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {command}"))
        .output()
        .unwrap();
    assert!(found.status.success(), "host must provide {command}");
    let target = String::from_utf8(found.stdout).unwrap();
    let target = target.trim();
    assert!(!target.is_empty(), "host must provide {command}");
    std::os::unix::fs::symlink(target, bin.join(command)).unwrap();
}

#[cfg(unix)]
fn run_search_with_path(
    bin: &std::path::Path,
    root: &std::path::Path,
    options: &SearchOptions,
) -> ToolResult {
    let command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&command, root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let result = search_project_text_output("demo", options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    result
}

#[cfg(unix)]
fn logical_search_matches(result: &ToolResult) -> Vec<(u64, String)> {
    let mut matches = result.output["matches"]
        .as_array()
        .expect("matches array")
        .iter()
        .map(|item| {
            (
                item["line"].as_u64().expect("match line"),
                item["preview"].as_str().expect("match preview").to_string(),
            )
        })
        .collect::<Vec<_>>();
    matches.sort();
    matches
}

#[cfg(unix)]
#[test]
fn search_project_text_command_prefers_rg_backend_when_available() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/lib.rs:2:needle from rg\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(
            &SearchOptions::normalize(SearchRequest {
                limit: Some(5),
                ..raw_search_request()
            })
            .unwrap(),
        )
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
    assert_eq!(result.output["matches"][0]["line"], 2);
    assert_eq!(result.output["matches"][0]["preview"], "needle from rg");
}

#[cfg(unix)]
#[test]
fn search_project_text_command_falls_back_to_grep_without_rg() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("grep"),
        "#!/bin/sh\nprintf 'src/lib.rs:3:needle from grep\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(
            &SearchOptions::normalize(SearchRequest {
                limit: Some(5),
                ..raw_search_request()
            })
            .unwrap(),
        )
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["truncated"], false);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
    assert_eq!(result.output["matches"][0]["line"], 3);
    assert_eq!(result.output["matches"][0]["preview"], "needle from grep");
}

#[cfg(unix)]
#[test]
fn search_project_text_grep_single_file_matches_and_no_match() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("one.txt"), "alpha\nneedle\nomega\n").unwrap();
    for command in ["grep", "head"] {
        symlink_host_command(command, &bin);
    }

    for (pattern, expected_exit) in [("needle", 0), ("definitely_missing", 1)] {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: pattern.to_string(),
                path: Some("one.txt".to_string()),
                ..raw_search_request()
            },
            Some(SearchPatternMode::Literal),
        )
        .unwrap();
        let command = format!(
            "PATH={}; export PATH\n{}",
            shell_escape_simple(&bin.to_string_lossy()),
            search_project_text_command(&options)
        );
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
        assert_eq!(exit_code, expected_exit, "{stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["backend"], "grep");
        assert_eq!(result.output["exit_code"], expected_exit);
        if expected_exit == 0 {
            assert_eq!(result.output["count"], 1);
            assert_eq!(result.output["matches"][0]["path"], "one.txt");
            assert_eq!(result.output["matches"][0]["line"], 2);
            assert_eq!(result.output["matches"][0]["preview"], "needle");
        } else {
            assert_eq!(result.output["matches"], json!([]));
        }
    }
}

#[cfg(unix)]
#[test]
fn search_project_text_grep_directory_matches_multiple_files() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.txt"), "needle\nneedle\n").unwrap();
    std::fs::write(root.join("src/b.txt"), "needle\n").unwrap();
    for command in ["grep", "head"] {
        symlink_host_command(command, &bin);
    }
    let options = SearchOptions::normalize(SearchRequest {
        path: Some("src".to_string()),
        ..raw_search_request()
    })
    .unwrap();
    let result = run_search_with_path(&bin, &root, &options);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["count"], 3);
    let mut paths = result.output["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(paths, vec!["src/a.txt", "src/a.txt", "src/b.txt"]);
}

#[cfg(unix)]
#[test]
fn search_project_text_grep_files_with_matches_scopes_and_bounds() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.txt"), "needle\nneedle\n").unwrap();
    std::fs::write(root.join("src/b.txt"), "needle\n").unwrap();
    std::fs::write(root.join("src/c.txt"), "quiet\n").unwrap();
    for command in ["grep", "head"] {
        symlink_host_command(command, &bin);
    }

    for (path, pattern, limit, expected_exit, expected_files, truncated) in [
        ("src/a.txt", "needle", 10, 0, vec!["src/a.txt"], false),
        ("src/c.txt", "needle", 10, 1, vec![], false),
        (
            "src",
            "needle",
            10,
            0,
            vec!["src/a.txt", "src/b.txt"],
            false,
        ),
        ("src", "missing", 10, 1, vec![], false),
        ("src", "needle", 1, 0, vec!["src/a.txt", "src/b.txt"], true),
    ] {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: pattern.to_string(),
                path: Some(path.to_string()),
                limit: Some(limit),
                result_mode: Some(SearchResultMode::FilesWithMatches),
                ..raw_search_request()
            },
            Some(SearchPatternMode::Literal),
        )
        .unwrap();
        assert!(!options.requires_ripgrep());
        let command = format!(
            "PATH={}; export PATH\n{}",
            shell_escape_simple(&bin.to_string_lossy()),
            search_project_text_command(&options)
        );
        assert!(command.contains("grep -rlI -F"), "{command}");
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
        assert_eq!(exit_code, expected_exit, "{stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["backend"], "grep");
        assert_eq!(result.output["exit_code"], expected_exit);
        let files = result.output["files"].as_array().unwrap();
        assert_eq!(files.len(), expected_files.len().min(limit));
        assert!(files
            .iter()
            .all(|item| expected_files.contains(&item["path"].as_str().unwrap())));
        assert_eq!(result.output["returned_file_count"], files.len());
        assert_eq!(result.output["truncated"], truncated);
        if truncated {
            assert_eq!(result.output["truncation_reason"], "limit");
        }
    }

    for (pattern_mode, expected_exit) in [
        (SearchPatternMode::Regex, 0),
        (SearchPatternMode::Literal, 1),
    ] {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: "need.e".to_string(),
                path: Some("src".to_string()),
                result_mode: Some(SearchResultMode::FilesWithMatches),
                ..raw_search_request()
            },
            Some(pattern_mode),
        )
        .unwrap();
        let command = format!(
            "PATH={}; export PATH\n{}",
            shell_escape_simple(&bin.to_string_lossy()),
            search_project_text_command(&options)
        );
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, &root, 10);
        assert_eq!(exit_code, expected_exit, "{stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        assert!(result.success, "{result:?}");
        assert_eq!(
            result.output["returned_file_count"],
            if expected_exit == 0 { 2 } else { 0 }
        );
    }
}

#[test]
fn search_project_text_grep_output_contract_handles_crlf_and_multiple_records() {
    let marker = "{\"webcodex_search\":{\"backend\":\"grep\",\"feature_unavailable\":false}}\r\n";
    let raw_path = if cfg!(windows) {
        "src\\foo.rs"
    } else {
        "src/foo.rs"
    };
    let expected_path = "src/foo.rs";
    let matches_options = SearchOptions::normalize(raw_search_request()).unwrap();
    let matches_stdout = format!("{marker}{raw_path}\01:needle\r\n{raw_path}\02:needle again\r\n");
    let matches =
        search_project_text_output("demo", &matches_options, &matches_stdout, Some(0), "");
    assert!(matches.success, "{matches:?}");
    assert_eq!(matches.output["backend"], "grep");
    assert_eq!(matches.output["count"], 2);
    assert_eq!(matches.output["matches"][0]["path"], expected_path);
    assert_eq!(matches.output["matches"][0]["line"], 1);
    assert_eq!(matches.output["matches"][0]["preview"], "needle");
    assert_eq!(matches.output["matches"][1]["line"], 2);

    let files_options = SearchOptions::normalize(SearchRequest {
        result_mode: Some(SearchResultMode::FilesWithMatches),
        ..raw_search_request()
    })
    .unwrap();
    let files_stdout = format!("{marker}{raw_path}\r\nsrc/bar.rs\r\n");
    let files = search_project_text_output("demo", &files_options, &files_stdout, Some(0), "");
    assert!(files.success, "{files:?}");
    assert_eq!(files.output["returned_file_count"], 2);
    assert_eq!(files.output["files"][0]["path"], expected_path);
    assert_eq!(files.output["files"][1]["path"], "src/bar.rs");
    assert_eq!(files.output["truncated"], false);

    for options in [&matches_options, &files_options] {
        let empty = search_project_text_output("demo", options, marker, Some(1), "");
        assert!(empty.success, "{empty:?}");
        assert_eq!(empty.output["exit_code"], 1);
        let field = if options.result_mode == SearchResultMode::Matches {
            "matches"
        } else {
            "files"
        };
        assert_eq!(empty.output[field], json!([]));

        let inconsistent = search_project_text_output("demo", options, marker, Some(0), "");
        assert!(!inconsistent.success);
        assert_eq!(
            inconsistent.output["reason_code"],
            "backend_output_inconsistent"
        );
        let missing_marker =
            search_project_text_output("demo", options, "src/foo.rs:1:needle\n", Some(0), "");
        assert!(!missing_marker.success);
        assert_eq!(
            missing_marker.output["reason_code"],
            "backend_identity_missing"
        );
    }
}

#[cfg(windows)]
#[test]
fn search_project_text_grep_windows_backend_smoke() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("one.txt"), "alpha\nneedle\nomega\n").unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::write(tmp.path().join("src/a.txt"), "needle\n").unwrap();
    std::fs::write(tmp.path().join("src/b.txt"), "needle\n").unwrap();
    for (mode, path, pattern, expected_exit, expected_paths) in [
        (
            SearchResultMode::Matches,
            "one.txt",
            "needle",
            0,
            vec!["one.txt"],
        ),
        (
            SearchResultMode::FilesWithMatches,
            "one.txt",
            "needle",
            0,
            vec!["one.txt"],
        ),
        (SearchResultMode::Matches, "one.txt", "missing", 1, vec![]),
        (
            SearchResultMode::FilesWithMatches,
            "one.txt",
            "missing",
            1,
            vec![],
        ),
        (
            SearchResultMode::Matches,
            "src",
            "needle",
            0,
            vec!["src/a.txt", "src/b.txt"],
        ),
        (
            SearchResultMode::FilesWithMatches,
            "src",
            "needle",
            0,
            vec!["src/a.txt", "src/b.txt"],
        ),
    ] {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: pattern.to_string(),
                path: Some(path.to_string()),
                result_mode: Some(mode),
                ..raw_search_request()
            },
            Some(SearchPatternMode::Literal),
        )
        .unwrap();
        // Git for Windows provides grep and head together in usr/bin. Scope
        // PATH to that directory so an unrelated installed rg cannot mask the
        // fallback branch being tested.
        let command = format!(
            "grep_bin=$(command -v grep)\nPATH=${{grep_bin%/*}}; export PATH\n{}",
            search_project_text_command(&options)
        );
        let (exit_code, stdout, stderr, _) = run_command_sync(&command, tmp.path(), 10);
        assert_eq!(exit_code, expected_exit, "{stderr}");
        let result =
            search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
        assert!(result.success, "{result:?}");
        assert_eq!(result.output["backend"], "grep");
        assert_eq!(result.output["exit_code"], expected_exit);
        assert_eq!(result.output["truncated"], false);
        match mode {
            SearchResultMode::Matches => {
                let mut paths = result.output["matches"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| item["path"].as_str().unwrap())
                    .collect::<Vec<_>>();
                paths.sort();
                assert_eq!(paths, expected_paths);
                assert_eq!(result.output["count"], paths.len());
                if path == "one.txt" && expected_exit == 0 {
                    assert_eq!(result.output["matches"][0]["line"], 2);
                    assert_eq!(result.output["matches"][0]["preview"], "needle");
                }
            }
            SearchResultMode::FilesWithMatches => {
                let mut paths = result.output["files"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| item["path"].as_str().unwrap())
                    .collect::<Vec<_>>();
                paths.sort();
                assert_eq!(paths, expected_paths);
                assert_eq!(result.output["returned_file_count"], paths.len());
            }
            SearchResultMode::Count => unreachable!(),
        }
    }
}

#[cfg(unix)]
#[test]
fn search_project_text_basic_regex_semantics_match_rg_and_grep_fallback() {
    if !host_ripgrep_available() {
        eprintln!("skipping regex backend-parity test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let rg_bin = tmp.path().join("rg-bin");
    let grep_bin = tmp.path().join("grep-bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&rg_bin).unwrap();
    std::fs::create_dir_all(&grep_bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("sample.txt"),
        "foo\nbar\nbaz\naaaa\na+\n(foo)\nfoo|bar\nRuntimeInfo {\n",
    )
    .unwrap();

    for command in ["rg", "grep", "head"] {
        symlink_host_command(command, &rg_bin);
    }
    for command in ["grep", "head"] {
        symlink_host_command(command, &grep_bin);
    }

    let cases = [
        (
            "foo|bar",
            vec![(1, "foo"), (2, "bar"), (6, "(foo)"), (7, "foo|bar")],
        ),
        (
            "a+",
            vec![
                (2, "bar"),
                (3, "baz"),
                (4, "aaaa"),
                (5, "a+"),
                (7, "foo|bar"),
            ],
        ),
        ("(foo)", vec![(1, "foo"), (6, "(foo)"), (7, "foo|bar")]),
        ("baz", vec![(3, "baz")]),
    ];

    for (pattern, expected) in cases {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: pattern.to_string(),
                limit: Some(20),
                ..raw_search_request()
            },
            Some(SearchPatternMode::Regex),
        )
        .unwrap();
        let command = search_project_text_command(&options);
        assert!(command.contains("-A 0 -E"), "{command}");
        assert!(!command.contains("-A 0 -F"), "{command}");

        let rg = run_search_with_path(&rg_bin, &root, &options);
        let grep = run_search_with_path(&grep_bin, &root, &options);
        assert_eq!(rg.output["backend"], "rg", "pattern {pattern}");
        assert_eq!(grep.output["backend"], "grep", "pattern {pattern}");
        assert_eq!(
            logical_search_matches(&rg),
            logical_search_matches(&grep),
            "pattern {pattern} must have backend-independent logical matches"
        );
        let expected = expected
            .into_iter()
            .map(|(line, preview)| (line, preview.to_string()))
            .collect::<Vec<_>>();
        assert_eq!(logical_search_matches(&grep), expected, "pattern {pattern}");
    }
}

#[cfg(unix)]
#[test]
fn search_project_text_grep_fallback_keeps_literal_patterns_literal() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("sample.txt"),
        "foo\nbar\nbaz\naaaa\na+\n(foo)\nfoo|bar\nRuntimeInfo {\n",
    )
    .unwrap();
    for command in ["grep", "head"] {
        symlink_host_command(command, &bin);
    }

    let cases = [
        ("foo|bar", 7, "foo|bar"),
        ("a+", 5, "a+"),
        ("(foo)", 6, "(foo)"),
        ("RuntimeInfo {", 8, "RuntimeInfo {"),
    ];
    for (pattern, line, preview) in cases {
        let options = SearchOptions::normalize_with_pattern_mode(
            SearchRequest {
                pattern: pattern.to_string(),
                limit: Some(20),
                ..raw_search_request()
            },
            Some(SearchPatternMode::Literal),
        )
        .unwrap();
        let command = search_project_text_command(&options);
        assert!(command.contains("-A 0 -F"), "{command}");
        assert!(!command.contains("-A 0 -E"), "{command}");

        let result = run_search_with_path(&bin, &root, &options);
        assert_eq!(result.output["backend"], "grep", "pattern {pattern}");
        assert_eq!(
            logical_search_matches(&result),
            vec![(line, preview.to_string())],
            "literal pattern {pattern}"
        );
    }
}

#[cfg(unix)]
#[test]
fn search_project_text_grep_fallback_excludes_ignored_claude_worktrees() {
    let tmp = tempfile::tempdir().unwrap();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let stale = root.join(".claude/worktrees/stale/src");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(root.join(".gitignore"), ".claude\n").unwrap();
    std::fs::write(root.join("src/live.txt"), "needle active\n").unwrap();
    std::fs::write(stale.join("old.txt"), "needle stale\n").unwrap();

    let git_init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(git_init.success());
    let git_ignore = std::process::Command::new("git")
        .args(["check-ignore", "-q", ".claude/worktrees/stale/src/old.txt"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(git_ignore.success(), "fixture must be ignored by Git");

    for command in ["grep", "head"] {
        let found = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {command}"))
            .output()
            .unwrap();
        assert!(found.status.success(), "host must provide {command}");
        let target = String::from_utf8(found.stdout).unwrap();
        let target = target.trim();
        assert!(!target.is_empty(), "host must provide {command}");
        std::os::unix::fs::symlink(target, bin.join(command)).unwrap();
    }

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(
        matches.len(),
        1,
        "stale ignored worktree must not be searched"
    );
    assert_eq!(matches[0]["path"], "src/live.txt");
    assert_eq!(matches[0]["preview"], "needle active");
}

#[test]
fn parse_search_project_text_output_accepts_leading_canonical_marker_and_reports_limit_truncation()
{
    let stdout = concat!(
        "\n",
        " \r\n",
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "src/a.rs:1:needle one\n",
        "src/b.rs:2:needle two\n",
        "{\"webcodex_search\":{\"backend\":\"grep\"}}\n",
    );
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(1),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output("demo", &options, stdout, Some(0), "");

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/a.rs");
}

fn raw_search_request() -> SearchRequest {
    SearchRequest {
        pattern: "needle".to_string(),
        path: None,
        limit: None,
        context_before: None,
        context_after: None,
        include_globs: None,
        exclude_globs: None,
        result_mode: None,
        timeout_secs: None,
    }
}

fn search_call(project: String, request: SearchRequest) -> ToolCall {
    ToolCall::SearchProjectTexts {
        project,
        queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
            pattern: request.pattern,
            pattern_mode: None,
            path: request.path,
            limit: request.limit,
            context_before: request.context_before,
            context_after: request.context_after,
            include_globs: request.include_globs,
            exclude_globs: request.exclude_globs,
            result_mode: request.result_mode,
            timeout_secs: request.timeout_secs,
        }],
        session_id: None,
        max_result_bytes: None,
    }
}

fn extract_single_search_batch_result(batch: ToolResult) -> ToolResult {
    if !batch.success {
        return batch;
    }
    let items = batch.output["items"]
        .as_array()
        .expect("one-query search batch items");
    assert_eq!(items.len(), 1, "one-query search batch: {}", batch.output);
    let item = &items[0];
    ToolResult {
        success: if batch.output.get("requested_count").is_none() {
            assert!(item.get("success").is_none());
            assert!(item.get("error").is_none());
            true // The complete sparse batch branch proves every item succeeded.
        } else {
            item["success"].as_bool().expect("full search item success")
        },
        output: item.get("output").cloned().unwrap_or(Value::Null),
        error: item
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
    }
}

fn assert_search_output_keys_are_declared(output: &Value) {
    if output.get("code").is_some() {
        return;
    }
    let schema = crate::tool_runtime::registry::output_schema_for_tool("search_project_texts");
    let item_output = &schema["properties"]["output"]["anyOf"][0]["anyOf"][0]["properties"]
        ["items"]["items"]["properties"]["output"];
    let mut declared = std::collections::BTreeSet::new();
    for variant in item_output["anyOf"][0]["anyOf"]
        .as_array()
        .expect("search success variants")
    {
        if let Some(properties) = variant["properties"].as_object() {
            declared.extend(properties.keys().cloned());
        }
    }
    if let Some(properties) = item_output["anyOf"][1]["properties"].as_object() {
        declared.extend(properties.keys().cloned());
    }
    let Some(output) = output.as_object() else {
        return;
    };
    for key in output.keys() {
        assert!(
            declared.contains(key),
            "runtime search output key {key} is not declared in search_project_texts item output schema"
        );
    }
}

async fn execute_agent_search(
    runtime: &ToolRuntime,
    client_id: &str,
    project: String,
    request: SearchRequest,
) -> (ToolResult, RunnerRequest) {
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(search_call(project, request), Some(&bootstrap))
                .await
        }
    });
    let req = wait_for_patch_agent_request(runtime, client_id).await;
    let inspected = req.clone();
    complete_agent_request_by_running_locally(runtime, client_id, req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert_search_output_keys_are_declared(&result.output);
    (result, inspected)
}

#[test]
fn search_options_default_to_compatible_matches_contract() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();

    assert_eq!(options.path, ".");
    assert_eq!(options.limit, 50);
    assert_eq!(options.context_before, 0);
    assert_eq!(options.context_after, 0);
    assert_eq!(options.result_mode, SearchResultMode::Matches);
    assert_eq!(options.timeout_secs, 30);
    assert!(options.include_globs.is_empty());
    assert!(options.exclude_globs.is_empty());
    assert!(!options.requires_ripgrep());
}

#[test]
fn search_options_clamp_timeout_and_context() {
    let mut low = raw_search_request();
    low.timeout_secs = Some(0);
    low.context_before = Some(usize::MAX);
    let low = SearchOptions::normalize(low).unwrap();
    assert_eq!(low.timeout_secs, 1);
    assert_eq!(low.context_before, MAX_SEARCH_CONTEXT_LINES);

    let high = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(999),
        context_after: Some(usize::MAX),
        ..raw_search_request()
    })
    .unwrap();
    assert_eq!(high.timeout_secs, 120);
    assert_eq!(high.context_after, MAX_SEARCH_CONTEXT_LINES);
}

#[test]
fn search_timeout_uses_structured_failure_with_effective_timeout() {
    let options = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(0),
        ..raw_search_request()
    })
    .unwrap();
    let result = search_project_text_output(
        "demo",
        &options,
        "{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
        Some(-1),
        "Command timed out after 1 seconds",
    );

    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_timeout");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert_eq!(result.output["result_mode"], "matches");
}

#[test]
fn search_agent_timeout_budget_keeps_outer_above_command_at_max() {
    // At max configured timeout, shell-client wait is capped at 120 so it may
    // equal command timeout; outer wait must still exceed command timeout.
    let (command, wait, outer) = search_agent_timeout_budget(120);
    assert_eq!(command, 120);
    assert_eq!(wait, 120);
    assert!(outer > command, "outer={outer} command={command}");
    assert!(outer >= wait, "outer={outer} wait={wait}");

    let (command, wait, outer) = search_agent_timeout_budget(30);
    assert_eq!(command, 30);
    assert_eq!(wait, 32);
    assert_eq!(outer, 34);
    assert!(command < wait && wait < outer);

    let (command, wait, outer) = search_agent_timeout_budget(118);
    assert_eq!(command, 118);
    assert_eq!(wait, 120);
    assert!(command < wait);
    assert!(wait < outer || outer > command);
}

#[test]
fn search_backend_exit_codes_map_to_results() {
    enum Expect {
        // Exit 1 (no matches) is a successful empty result.
        EmptySuccess,
        // Exit 2 is a structured execution failure.
        ExecutionFailure,
        // Exit 0 parses the emitted match lines.
        ParsedMatches(usize),
    }
    struct Case {
        backend: &'static str,
        exit_code: i32,
        // Extra stdout line emitted between the two backend marker lines.
        match_line: Option<&'static str>,
        limit: Option<usize>,
        expect: Expect,
    }
    let cases = [
        Case {
            backend: "rg",
            exit_code: 1,
            match_line: None,
            limit: None,
            expect: Expect::EmptySuccess,
        },
        Case {
            backend: "rg",
            exit_code: 2,
            match_line: None,
            limit: None,
            expect: Expect::ExecutionFailure,
        },
        Case {
            backend: "rg",
            exit_code: 0,
            match_line: Some("src/lib.rs:2:needle from rg\n"),
            limit: Some(5),
            expect: Expect::ParsedMatches(1),
        },
        Case {
            backend: "grep",
            exit_code: 1,
            match_line: None,
            limit: None,
            expect: Expect::EmptySuccess,
        },
        Case {
            backend: "grep",
            exit_code: 2,
            match_line: None,
            limit: None,
            expect: Expect::ExecutionFailure,
        },
    ];

    for case in cases {
        let ctx = format!("backend={} exit_code={}", case.backend, case.exit_code);
        let options = SearchOptions::normalize(SearchRequest {
            limit: case.limit,
            ..raw_search_request()
        })
        .unwrap();
        let marker = format!(
            "{{\"webcodex_search\":{{\"backend\":\"{}\",\"feature_unavailable\":false}}}}\n",
            case.backend
        );
        let stdout = format!("{marker}{}{marker}", case.match_line.unwrap_or(""));
        let result =
            search_project_text_output("demo", &options, &stdout, Some(case.exit_code), "");
        assert_eq!(result.output["backend"], case.backend, "{ctx}");
        match case.expect {
            Expect::EmptySuccess => {
                assert!(result.success, "{ctx}: {:?}", result.error);
                assert_eq!(result.output["matches"], json!([]), "{ctx}");
                assert_eq!(result.output["count"], 0, "{ctx}");
                assert_eq!(result.output["exit_code"], case.exit_code, "{ctx}");
            }
            Expect::ExecutionFailure => {
                assert!(!result.success, "{ctx}");
                assert_search_output_keys_are_declared(&result.output);
                assert_eq!(result.output["code"], "search_execution_failed", "{ctx}");
                assert_eq!(result.output["failure_stage"], "backend_execution", "{ctx}");
                assert_eq!(
                    result.output["reason_code"], "backend_process_failed",
                    "{ctx}"
                );
                assert_eq!(result.output["exit_code"], case.exit_code, "{ctx}");
                assert_eq!(result.output["result_mode"], "matches", "{ctx}");
            }
            Expect::ParsedMatches(expected) => {
                assert!(result.success, "{ctx}: {:?}", result.error);
                assert_eq!(
                    result.output["matches"].as_array().unwrap().len(),
                    expected,
                    "{ctx}"
                );
            }
        }
    }
}

#[test]
fn search_markerless_output_cannot_be_reported_as_a_search_result() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    for (stdout, exit_code, stderr) in [
        (
            "",
            Some(1),
            "PowerShell parser rejected the generated POSIX search script",
        ),
        ("src/lib.rs:1:startup noise\n", Some(0), ""),
        ("src/lib.rs:1:partial output\n", Some(1), ""),
    ] {
        let result = search_project_text_output("demo", &options, stdout, exit_code, stderr);
        assert!(!result.success, "stdout={stdout:?} exit_code={exit_code:?}");
        assert_search_output_keys_are_declared(&result.output);
        assert_eq!(result.output["code"], "search_execution_failed");
        assert_eq!(result.output["failure_stage"], "backend_protocol");
        assert_eq!(result.output["reason_code"], "backend_identity_missing");
        assert!(result.output["backend"].is_null());
        let rendered = serde_json::to_string(&result).unwrap();
        assert!(!rendered.contains("PowerShell parser"));
        assert!(!rendered.contains("startup noise"));
    }
}

#[test]
fn search_backend_identity_requires_leading_canonical_namespaced_marker() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    for stdout in [
        "{\"backend\":\"rg\"}\nsrc/a.rs:1:needle\n",
        "src/z.rs:1:needle\n{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
        "[output truncated to last 12000 bytes]\nsrc/z.rs:1:needle\n{\"webcodex_search\":{\"backend\":\"rg\"}}\n",
    ] {
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(!result.success, "stdout={stdout:?}");
        assert_search_output_keys_are_declared(&result.output);
        assert_eq!(result.output["code"], "search_execution_failed");
        assert_eq!(result.output["failure_stage"], "backend_protocol");
        assert_eq!(result.output["reason_code"], "backend_identity_missing");
        assert!(result.output["backend"].is_null());
        let rendered = serde_json::to_string(&result).unwrap();
        assert!(!rendered.contains("src/a.rs"));
        assert!(!rendered.contains("src/z.rs"));
    }
}

#[test]
fn search_invalid_backend_marker_reports_protocol_provenance() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    for stdout in [
        "{\"webcodex_search\":{\"backend\":\"unknown\"}}\n",
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":\"no\"}}\n",
        "{\"webcodex_search\":\n",
    ] {
        let result = search_project_text_output("demo", &options, stdout, Some(0), "");
        assert!(!result.success, "stdout={stdout:?}");
        assert_search_output_keys_are_declared(&result.output);
        assert_eq!(result.output["code"], "search_execution_failed");
        assert_eq!(result.output["failure_stage"], "backend_protocol");
        assert_eq!(result.output["reason_code"], "backend_identity_invalid");
        assert!(result.output["backend"].is_null());
    }
}

#[test]
fn search_missing_completion_status_cannot_prove_success() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let stdout = "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n";
    let result = search_project_text_output("demo", &options, stdout, None, "");

    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["failure_stage"], "backend_protocol");
    assert_eq!(result.output["reason_code"], "backend_status_unavailable");
    assert_eq!(result.output["backend"], "rg");
    assert!(result.output.get("exit_code").is_none());
}

#[test]
fn search_status_and_records_must_agree_before_empty_is_trusted() {
    let options = SearchOptions::normalize(raw_search_request()).unwrap();
    let marker = "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n";
    for (stdout, exit_code) in [
        (marker.to_string(), 0),
        (marker.to_string(), 141),
        (format!("{marker}src/lib.rs:1:needle\n"), 1),
        (format!("{marker}/private/absolute/secret.rs:1:needle\n"), 0),
    ] {
        let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), "");
        assert!(!result.success, "stdout={stdout:?} exit_code={exit_code}");
        assert_search_output_keys_are_declared(&result.output);
        assert_eq!(result.output["code"], "search_execution_failed");
        assert_eq!(result.output["failure_stage"], "backend_protocol");
        assert_eq!(result.output["reason_code"], "backend_output_inconsistent");
        assert_eq!(result.output["backend"], "rg");
        assert_eq!(result.output["exit_code"], exit_code);
        assert!(!serde_json::to_string(&result)
            .unwrap()
            .contains("/private/"));
    }

    let provider_marker =
        "{\"webcodex_search\":{\"backend\":\"claude_code\",\"feature_unavailable\":false}}\n";
    let unproven = search_project_text_output("demo", &options, provider_marker, Some(0), "");
    assert!(!unproven.success);
    assert_eq!(
        unproven.output["reason_code"],
        "backend_output_inconsistent"
    );
    let proven_empty = search_project_text_output("demo", &options, provider_marker, Some(1), "");
    assert!(proven_empty.success, "{:?}", proven_empty.error);
    assert_eq!(proven_empty.output["matches"], json!([]));
    assert_eq!(proven_empty.output["exit_code"], 1);
}

#[test]
fn search_count_uses_backend_evidence_without_claiming_filtered_absence() {
    let options = SearchOptions::normalize(SearchRequest {
        result_mode: Some(SearchResultMode::Count),
        limit: Some(10),
        ..raw_search_request()
    })
    .unwrap();
    let marker = "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n";

    let no_match = search_project_text_output("demo", &options, marker, Some(1), "");
    assert!(no_match.success, "{:?}", no_match.error);
    assert_eq!(no_match.output["files"], json!([]));
    assert_eq!(no_match.output["count_complete"], true);
    assert_eq!(no_match.output["total_matches"], 0);

    let safe_stdout = format!("{marker}src/lib.rs\u{0}2\n");
    let safe = search_project_text_output("demo", &options, &safe_stdout, Some(0), "");
    assert!(safe.success, "{:?}", safe.error);
    assert_eq!(safe.output["count_complete"], true);
    assert_eq!(safe.output["total_matches"], 2);
    assert_eq!(safe.output["files"][0]["path"], "src/lib.rs");

    let malformed_stdout = format!("{marker}src/lib.rs:not-a-number\n");
    let malformed = search_project_text_output("demo", &options, &malformed_stdout, Some(0), "");
    assert!(!malformed.success);
    assert_eq!(
        malformed.output["reason_code"],
        "backend_output_inconsistent"
    );

    let filtered_stdout = format!("{marker}/private/absolute/secret.rs\u{0}2\n");
    let filtered = search_project_text_output("demo", &options, &filtered_stdout, Some(0), "");
    assert!(filtered.success, "{:?}", filtered.error);
    assert_eq!(filtered.output["files"], json!([]));
    assert_eq!(filtered.output["returned_match_count"], 0);
    assert_eq!(filtered.output["count_complete"], false);
    assert_eq!(filtered.output["total_matches"], Value::Null);
    assert_eq!(filtered.output["truncated"], false);
    assert!(!serde_json::to_string(&filtered)
        .unwrap()
        .contains("/private/absolute/secret.rs"));

    let contradictory = search_project_text_output("demo", &options, &filtered_stdout, Some(1), "");
    assert!(!contradictory.success);
    assert_eq!(
        contradictory.output["reason_code"],
        "backend_output_inconsistent"
    );
}

#[cfg(unix)]
#[test]
fn search_command_preserves_rg_exit_2_despite_head() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    // Fake rg always exits 2 (regex/execution error); head would mask this in a bare pipeline.
    write_executable_script(&bin.join("rg"), "#!/bin/sh\nexit 2\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "rg");
}

#[cfg(unix)]
#[test]
fn search_command_preserves_rg_exit_1_as_success_empty() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(&bin.join("rg"), "#!/bin/sh\nexit 1\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 1, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"], json!([]));
    assert_eq!(result.output["backend"], "rg");
}

#[cfg(unix)]
#[test]
fn search_command_preserves_grep_exit_2_despite_head() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    // No rg → grep path.
    write_executable_script(&bin.join("grep"), "#!/bin/sh\nexit 2\n");
    write_executable_script(&bin.join("head"), fake_head_script());

    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["backend"], "grep");
}

#[cfg(unix)]
#[test]
fn search_command_illegal_regex_is_not_swallowed_by_head() {
    // Real rg with an illegal regex should surface exit >= 2 through the generated shell.
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), "hello\n").unwrap();
    let options = SearchOptions::normalize(SearchRequest {
        pattern: "[invalid".to_string(),
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let (exit_code, stdout, stderr, _) =
        run_command_sync(&search_project_text_command(&options), &root, 10);
    assert!(
        exit_code >= 2,
        "illegal regex should fail backend: exit={exit_code} stderr={stderr} stdout={stdout}"
    );
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
    assert_eq!(result.output["failure_stage"], "backend_execution");
    assert_eq!(result.output["reason_code"], "backend_process_failed");
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["exit_code"], exit_code);
    let rendered = serde_json::to_string(&result).unwrap();
    assert!(!rendered.contains("[invalid"));
}

#[cfg(unix)]
fn count_webcodex_search_status_files(dir: &std::path::Path) -> usize {
    let mut count = 0usize;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("webcodex-search-") {
            count += 1;
        }
        if entry.path().is_dir() {
            count += count_webcodex_search_status_files(&entry.path());
        }
    }
    count
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_relative_does_not_use_worktree() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let rel_tmp = root.join("rel-status-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&rel_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Relative TMPDIR must fall back to /tmp — never create status files under project.
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR=rel-status-tmp; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(count_webcodex_search_status_files(&root), 0);
    assert_eq!(count_webcodex_search_status_files(&rel_tmp), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_project_root_does_not_use_worktree() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let safe_tmp = tmp.path().join("safe-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&safe_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Absolute TMPDIR equal to project root is rejected; files must not land in worktree.
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&root.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(count_webcodex_search_status_files(&root), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_tmpdir_symlink_into_worktree_is_rejected() {
    // Outside symlink → inside worktree dir must not bypass physical-path checks.
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let inside = root.join("inner-tmp");
    let outside_link = tmp.path().join("outside-link-to-inner");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&inside).unwrap();
    std::os::unix::fs::symlink(&inside, &outside_link).expect("create symlink");
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&outside_link.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    assert_eq!(
        count_webcodex_search_status_files(&root),
        0,
        "symlink-into-worktree TMPDIR must not create status files under the project"
    );
    assert_eq!(count_webcodex_search_status_files(&inside), 0);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_status_file_is_removed_after_successful_run() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let safe_tmp = tmp.path().join("safe-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&safe_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&safe_tmp.to_string_lossy()),
        search_project_text_command(&options)
    );
    let before = count_webcodex_search_status_files(&safe_tmp);
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr} stdout={stdout}");
    let after = count_webcodex_search_status_files(&safe_tmp);
    assert_eq!(before, 0);
    assert_eq!(after, 0, "status files must be cleaned after success");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
}

#[cfg(unix)]
#[test]
fn search_early_stop_reaps_process_group_and_status_files() {
    // An infinite fake rg is stopped early by the head budget. The whole
    // wrapper process group (rg + the two head stages + the wrapper shell)
    // must be reaped and the status file removed — nothing is left behind to
    // be cleaned up by a later request.
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    let safe_tmp = tmp.path().join("safe-tmp");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&safe_tmp).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\ni=0\nwhile :; do\n  printf 'src/f%d.rs:%d:needle line\\n' \"$i\" \"$i\"\n  i=$((i + 1))\ndone\n",
    );
    write_executable_script(&bin.join("head"), truncating_fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(3),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\nTMPDIR={}; export TMPDIR\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        shell_escape_simple(&safe_tmp.to_string_lossy()),
        search_project_text_command(&options)
    );
    let before = count_webcodex_search_status_files(&safe_tmp);
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    let after = count_webcodex_search_status_files(&safe_tmp);
    assert_eq!(before, 0);
    assert_eq!(after, 0, "status files must be cleaned after early stop");
    // Exit 141 = the backend was SIGPIPEd by the head budget, an intentional
    // early stop, not a failure.
    assert_eq!(exit_code, 141, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 3);
    assert_eq!(result.output["truncation_reason"], "limit");
}

#[cfg(unix)]
#[test]
fn search_status_cleanup_trap_removes_file_on_term() {
    // Mirrors production cleanup_search_status + signal trap; verifies TERM path
    // without a long sleep. File removal is the contract.
    let tmp = tempfile::tempdir().unwrap();
    let status = tmp.path().join("webcodex-search-term-test");
    let script = format!(
        r#"status_file={path}
cleanup_search_status() {{
  if [ -n "${{status_file:-}}" ]; then
    /bin/rm -f "$status_file" 2>/dev/null || /usr/bin/rm -f "$status_file" 2>/dev/null || rm -f "$status_file" 2>/dev/null || true
    status_file=
  fi
}}
trap 'cleanup_search_status' EXIT
trap 'cleanup_search_status; exit 143' HUP INT TERM
: > "$status_file"
kill -s TERM $$
exit 1
"#,
        path = shell_escape_simple(&status.to_string_lossy())
    );
    let (exit_code, _stdout, stderr, _) = run_command_sync(&script, tmp.path(), 5);
    assert!(
        !status.exists(),
        "TERM trap must remove status file; exit={exit_code} stderr={stderr}"
    );
}

#[test]
fn resolve_search_head_command_prefers_path_then_absolute() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    #[cfg(unix)]
    {
        write_executable_script(&bin.join("head"), fake_head_script());
        let path = format!("{}:/usr/bin", bin.display());
        let resolved = resolve_search_head_command(Some(&path), &["/usr/bin/head", "/bin/head"])
            .expect("path head");
        assert!(resolved.contains("bin"), "{resolved}");
        assert!(resolved.ends_with("head"), "{resolved}");
    }
    let missing = resolve_search_head_command(Some("/nonexistent/path/for/head"), &[]);
    assert!(missing.is_none());
    let absolute = resolve_search_head_command(
        Some("/nonexistent/path/for/head"),
        DEFAULT_SEARCH_HEAD_ABSOLUTE_CANDIDATES,
    );
    // System may or may not have /usr/bin/head; when present, absolute resolves.
    if std::path::Path::new("/usr/bin/head").is_file()
        || std::path::Path::new("/bin/head").is_file()
    {
        assert!(absolute.is_some());
    }
}

#[cfg(unix)]
#[test]
fn search_command_fails_when_head_unavailable() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    // No head in PATH and no absolute fallbacks → fail closed (no unbounded output).
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command_with_head_fallbacks(&options, &[])
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
}

#[cfg(unix)]
#[test]
fn search_command_fails_when_head_exits_nonzero_even_if_backend_succeeds() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), "#!/bin/sh\nexit 2\n");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    // Prefer PATH head (broken) over absolute system head by using empty absolute fallbacks.
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command_with_head_fallbacks(&options, &[])
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 2, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(!result.success);
    assert_eq!(result.output["code"], "search_execution_failed");
}

#[cfg(unix)]
#[test]
fn search_command_keeps_success_when_head_is_available() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nprintf 'src/a.rs:1:needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr={stderr}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
}

#[cfg(unix)]
/// A `head` that actually bounds output: stops after `-n <count>` lines or
/// `-c <count>` bytes. The shared `fake_head_script()` is a passthrough that
/// never closes the pipe, which is exactly what the early-stop tests must not
/// use — they need the pipe to close so the backend is SIGPIPEd.
fn truncating_fake_head_script() -> &'static str {
    r#"#!/bin/sh
n=1000000
c=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    -n) n=$2; shift 2 ;;
    -c) c=$2; shift 2 ;;
    *) shift ;;
  esac
done
count=0
bytes=0
while IFS= read -r line; do
  count=$((count + 1))
  bytes=$((bytes + ${#line} + 1))
  if [ "$c" -gt 0 ] && [ "$bytes" -gt "$c" ]; then
    exit 0
  fi
  printf '%s\n' "$line"
  if [ "$count" -ge "$n" ]; then
    exit 0
  fi
done
"#
}

#[cfg(unix)]
#[test]
fn search_small_limit_stops_unbounded_backend_early() {
    // Fake rg emits a never-ending stream of matches and never exits on its
    // own. A small limit must close the pipe and return promptly with exactly
    // the requested records; without early stop this would run until the
    // command timeout. Deterministic: no reliance on machine speed — the fake
    // either keeps streaming (and the truncating head closes it) or the test
    // times out.
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        r#"#!/bin/sh
i=0
while :; do
  printf 'src/file%d.rs:%d:needle line\n' "$i" "$i"
  i=$((i + 1))
done
"#,
    );
    write_executable_script(&bin.join("head"), truncating_fake_head_script());
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(3),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 141, "stderr={stderr} stdout={stdout}");
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 3);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "limit");
    // The fake backend was stopped early: the output is bounded even though
    // the producer was infinite.
    assert!(
        stdout.len() < 4096,
        "stdout unexpectedly large: {}",
        stdout.len()
    );
}

#[cfg(unix)]
#[test]
fn search_overlong_match_line_does_not_overflow_byte_budget() {
    // A single match line far longer than the byte budget must be truncated by
    // the `head -c` stage, and the parser must return only complete records
    // with truncation_reason = "output_bytes" instead of surfacing a half
    // record. The fake `head` delegates to the real system head so the byte
    // boundary cut is byte-accurate and deterministic.
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    // Fake rg emits one small complete match followed by a single enormous
    // match line that itself exceeds the byte budget; it then exits cleanly.
    // Materialize the long payload once in the fixture instead of performing
    // hundreds of thousands of shell-loop iterations under the test deadline.
    let oversized_preview = "x".repeat(SEARCH_OUTPUT_BYTE_BUDGET + 1024);
    let fake_rg = format!(
        "#!/bin/sh\nprintf 'src/small.rs:1:needle ok\\n'\nprintf 'src/big.rs:2:{oversized_preview}\\n'\nexit 0\n"
    );
    write_executable_script(&bin.join("rg"), &fake_rg);
    // Real-head semantics: byte-accurate -n/-c truncation. The restricted PATH
    // contains only this delegating head plus the fake rg.
    write_executable_script(&bin.join("head"), "#!/bin/sh\nexec /usr/bin/head \"$@\"\n");
    let options = SearchOptions::normalize(SearchRequest {
        limit: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );
    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    let result = search_project_text_output("demo", &options, &stdout, Some(exit_code), &stderr);

    assert!(result.success, "{:?}", result.error);
    // The small complete record survived; the over-long record was cut by the
    // byte budget and its partial tail dropped.
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/small.rs");
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "output_bytes");
    // No half record is surfaced, and the raw stdout (marker + budget) stays
    // bounded: the budget bytes plus the small leading marker.
    assert!(
        stdout.len() <= SEARCH_OUTPUT_BYTE_BUDGET + 256,
        "stdout={} exceeds byte budget",
        stdout.len()
    );
}

#[test]
fn search_requires_ripgrep_based_on_effective_features_not_field_presence() {
    let empty_globs = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![]),
        exclude_globs: Some(vec![]),
        timeout_secs: Some(5),
        ..raw_search_request()
    })
    .unwrap();
    assert!(!empty_globs.requires_ripgrep());
    assert!(empty_globs.include_globs.is_empty());
    assert!(empty_globs.exclude_globs.is_empty());
    assert_eq!(empty_globs.timeout_secs, 5);

    let timeout_only = SearchOptions::normalize(SearchRequest {
        timeout_secs: Some(5),
        result_mode: Some(SearchResultMode::Matches),
        ..raw_search_request()
    })
    .unwrap();
    assert!(!timeout_only.requires_ripgrep());

    let files_mode = SearchOptions::normalize(SearchRequest {
        result_mode: Some(SearchResultMode::FilesWithMatches),
        ..raw_search_request()
    })
    .unwrap();
    assert!(!files_mode.requires_ripgrep());

    let with_include = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec!["**/*.rs".to_string()]),
        ..raw_search_request()
    })
    .unwrap();
    assert!(with_include.requires_ripgrep());

    let with_exclude = SearchOptions::normalize(SearchRequest {
        exclude_globs: Some(vec!["**/vendor/**".to_string()]),
        ..raw_search_request()
    })
    .unwrap();
    assert!(with_exclude.requires_ripgrep());

    let count_mode = SearchOptions::normalize(SearchRequest {
        result_mode: Some(SearchResultMode::Count),
        ..raw_search_request()
    })
    .unwrap();
    assert!(count_mode.requires_ripgrep());
}

#[test]
fn search_validation_errors_are_structured_without_raw_secrets() {
    let secret_pattern = "SUPER_SECRET_PATTERN_VALUE_XYZ";
    let secret_glob = "private-secret-name/**/*.rs";

    let empty = SearchOptions::normalize(SearchRequest {
        pattern: "   ".to_string(),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(empty.field, "pattern");
    assert_eq!(empty.reason, Some("empty"));

    let nul = SearchOptions::normalize(SearchRequest {
        pattern: format!("a{secret_pattern}\0b"),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(nul.field, "pattern");
    assert!(!nul.message.contains(secret_pattern));

    let path = SearchOptions::normalize(SearchRequest {
        path: Some("../outside".to_string()),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(path.field, "path");

    let glob = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![format!("!{secret_glob}")]),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(glob.field, "include_globs");
    assert_eq!(glob.reason, Some("negated"));
    assert_eq!(glob.index, Some(0));
    assert!(!glob.message.contains(secret_glob));
}

#[tokio::test]
async fn search_invalid_request_dispatch_returns_structured_error() {
    // Authorization runs before the tool body; register shell capability so
    // normalize validation is reached and returns structured output.
    let runtime = runtime_with_agent_project("search-invalid");
    register_agent(
        &runtime,
        "search-invalid",
        None,
        RunnerCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let secret_glob = "NEVER_ECHO_THIS_GLOB_VALUE/**";
    let result = runtime
        .dispatch_with_auth(
            search_call(
                agent_test_project_id("search-invalid"),
                SearchRequest {
                    include_globs: Some(vec![format!("!{secret_glob}")]),
                    ..raw_search_request()
                },
            ),
            Some(&auth_context(None, true)),
        )
        .await;
    let result = extract_single_search_batch_result(result);

    // Validation fails before any agent search request is enqueued.
    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "request_validation");
    assert_eq!(result.output["reason_code"], "invalid_glob");
    assert_eq!(result.output["detail_code"], "invalid_glob");
    assert_eq!(result.output["state_changed"], false);
    let rendered = serde_json::to_string(&result.output).unwrap();
    assert!(!rendered.contains("NEVER_ECHO_THIS_GLOB_VALUE"));
    assert!(!result.error.as_deref().unwrap_or("").contains("NEVER_ECHO"));
}

#[tokio::test]
async fn search_agent_command_timeout_returns_search_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-cmd-timeout", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-cmd-timeout").await;
    assert_eq!(req.timeout_secs, 1);
    // Simulate Runner-side command timeout response (lowercase message + error field).
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "search-cmd-timeout".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(-1),
            stdout: Some(
                "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n"
                    .to_string(),
            ),
            stderr: Some("command timed out after 1 seconds".to_string()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1000),
            error: Some("command timed out".to_string()),
        })
        .await
        .unwrap();
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "backend_execution");
    assert_eq!(result.output["reason_code"], "timeout");
    assert_eq!(result.output["detail_code"], "timeout");
    assert_eq!(result.output["state_changed"], false);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert_eq!(result.output["backend"], "rg");
}

#[tokio::test]
async fn search_agent_execution_failure_is_structured_and_does_not_leak_diagnostics() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-agent-failure", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    search_call(project, raw_search_request()),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-agent-failure").await;
    let private_diagnostic =
        "provider private prose at /private/runner/workspace with token=NEVER_RETURN";
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "search-agent-failure".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(9),
            stdout: Some(
                "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n"
                    .to_string(),
            ),
            stderr: Some(private_diagnostic.to_string()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(5),
            error: Some(private_diagnostic.to_string()),
        })
        .await
        .unwrap();

    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "agent_execution");
    assert_eq!(result.output["reason_code"], "search_execution_failed");
    assert_eq!(result.output["detail_code"], "agent_execution_failed");
    assert_eq!(result.output["state_changed"], false);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["exit_code"], 9);
    let rendered = serde_json::to_string(&result).unwrap();
    assert!(!rendered.contains("private prose"));
    assert!(!rendered.contains("/private/"));
    assert!(!rendered.contains("NEVER_RETURN"));
}

#[tokio::test]
async fn search_agent_timeout_without_trusted_marker_cannot_return_partial_success() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-timeout-no-marker", "demo", tmp.path())
            .await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-timeout-no-marker").await;
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "search-timeout-no-marker".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(-1),
            stdout: Some("src/a.rs:1:needle\n".to_string()),
            stderr: Some("command timed out after 1 seconds".to_string()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1000),
            error: Some("command timed out".to_string()),
        })
        .await
        .unwrap();

    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "agent_execution");
    assert_eq!(result.output["reason_code"], "timeout");
    assert_eq!(result.output["detail_code"], "timeout");
    assert_eq!(result.output["state_changed"], false);
    assert_search_output_keys_are_declared(&result.output);
    assert!(result.output["backend"].is_null());
    assert!(result.output.get("matches").is_none());
}

#[tokio::test]
async fn search_agent_timeout_with_complete_records_returns_partial_success() {
    // Local and agent paths share the same parser, so an agent-reported
    // timeout that arrived with complete records returns the same partial
    // success semantics as the local path.
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-ptimeout", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-ptimeout").await;
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "search-ptimeout".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id: req.request_id,
            exit_code: Some(-1),
            stdout: Some(
                "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n\
                 src/a.rs:1:needle\n"
                    .to_string(),
            ),
            stderr: Some("command timed out after 1 seconds".to_string()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1000),
            error: Some("command timed out".to_string()),
        })
        .await
        .unwrap();
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(result.success, "{:?}", result.error);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["backend"], "rg");
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "timeout");
    let matches = result.output["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["path"], "src/a.rs");
    assert_eq!(matches[0]["line"], 1);
    assert_eq!(matches[0]["preview"], "needle");
}

#[tokio::test]
async fn search_agent_outer_timeout_returns_search_timeout_and_cancels() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-outer-timeout", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            // Outer wait is command+4 seconds; leave request unanswered.
                            timeout_secs: Some(1),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-outer-timeout").await;
    let request_id = req.request_id.clone();
    assert_eq!(req.timeout_secs, 1);
    // Do not complete the agent request; outer tokio timeout should fire.
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "agent_transport");
    assert_eq!(result.output["reason_code"], "timeout");
    assert_eq!(result.output["detail_code"], "timeout");
    assert_eq!(result.output["state_changed"], false);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["result_mode"], "matches");
    assert_eq!(result.output["effective_timeout_secs"], 1);
    assert!(
        result.output.get("backend").is_none() || result.output["backend"].is_null(),
        "outer timeout should not invent backend: {}",
        result.output
    );
    // Request should have been cancelled (no longer pending for completion).
    let complete = runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "search-outer-timeout".to_string(),
            runner_instance_id: "inst".to_string(),
            request_id,
            exit_code: Some(0),
            stdout: Some(String::new()),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await;
    assert!(
        complete.is_err(),
        "cancelled request should reject late complete: {complete:?}"
    );
}

#[tokio::test]
async fn search_agent_request_dropped_returns_structured_error() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-dropped", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            timeout_secs: Some(30),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let first = wait_for_patch_agent_request(&runtime, "search-dropped").await;
    // A one-query canonical batch retries one dropped Runner request once.
    runtime
        .runner_registry
        .cancel_request(&first.request_id)
        .await;
    let second = wait_for_patch_agent_request(&runtime, "search-dropped").await;
    runtime
        .runner_registry
        .cancel_request(&second.request_id)
        .await;
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "agent_transport");
    assert_eq!(result.output["reason_code"], "search_request_dropped");
    assert_eq!(result.output["detail_code"], "search_request_dropped");
    assert_eq!(result.output["state_changed"], false);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["result_mode"], "matches");
    let effective_timeout = result.output["effective_timeout_secs"].as_u64().unwrap();
    assert!((29..=30).contains(&effective_timeout));
    assert!(
        result.error.as_deref().unwrap_or("").contains("dropped"),
        "{:?}",
        result.error
    );
}

#[cfg(unix)]
#[tokio::test]
async fn search_timeout_only_without_rg_still_allows_grep_fallback() {
    let tmp = crate::test_support::executable_tempdir();
    let root = tmp.path().join("project");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(root.join("a.rs"), "timeout_fallback_needle\n").unwrap();
    write_executable_script(
        &bin.join("grep"),
        "#!/bin/sh\nprintf 'a.rs:1:timeout_fallback_needle\\n'\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());

    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-timeout-fallback", "demo", &root).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            pattern: "timeout_fallback_needle".to_string(),
                            result_mode: Some(SearchResultMode::Matches),
                            timeout_secs: Some(5),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let mut req = wait_for_patch_agent_request(&runtime, "search-timeout-fallback").await;
    // Force grep path (no rg in PATH).
    req.command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        req.command
    );
    assert_eq!(req.timeout_secs, 5);
    complete_agent_request_by_running_locally(&runtime, "search-timeout-fallback", req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["effective_timeout_secs"], 5);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
}

#[test]
fn search_glob_validation_rejects_invalid_and_protected_inputs() {
    let invalid = [
        "",
        "!**/*.rs",
        "docs/\n*.md",
        "docs/\t*.md",
        "docs/\0*.md",
        "**/.env",
        "secrets/**",
        "**/*.key",
    ];
    for glob in invalid {
        let result = SearchOptions::normalize(SearchRequest {
            include_globs: Some(vec![glob.to_string()]),
            ..raw_search_request()
        });
        assert!(result.is_err(), "include glob {glob:?} should be rejected");
        let err = result.unwrap_err();
        assert_eq!(err.field, "include_globs");
        assert!(!err.message.contains(glob) || glob.is_empty());
    }

    for glob in ["", "!vendor/**", "vendor/\n**"] {
        let result = SearchOptions::normalize(SearchRequest {
            exclude_globs: Some(vec![glob.to_string()]),
            ..raw_search_request()
        });
        assert!(result.is_err(), "exclude glob {glob:?} should be rejected");
        assert_eq!(result.unwrap_err().field, "exclude_globs");
    }
}

#[test]
fn search_glob_validation_enforces_count_and_byte_limits() {
    let too_many = (0..=MAX_SEARCH_GLOBS)
        .map(|index| format!("src/{index}/**"))
        .collect::<Vec<_>>();
    let count_error = SearchOptions::normalize(SearchRequest {
        include_globs: Some(too_many),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(count_error.field, "include_globs");
    assert_eq!(count_error.reason, Some("too_many"));
    assert!(
        count_error.message.contains("at most 32"),
        "{}",
        count_error.message
    );

    let length_error = SearchOptions::normalize(SearchRequest {
        exclude_globs: Some(vec!["a".repeat(MAX_SEARCH_GLOB_BYTES + 1)]),
        ..raw_search_request()
    })
    .unwrap_err();
    assert_eq!(length_error.field, "exclude_globs");
    assert_eq!(length_error.reason, Some("too_long"));
    assert_eq!(length_error.index, Some(0));
    assert!(
        length_error.message.contains("256 bytes"),
        "{}",
        length_error.message
    );
}

#[test]
fn search_audit_arguments_record_bounded_feature_summary_without_pattern_or_globs() {
    let raw = json!({
        "project": "agent:demo:project",
        "queries": [{
            "pattern": "NEVER_LOG_PATTERN_VALUE",
            "path": "src",
            "limit": 7,
            "context_before": 1,
            "context_after": 2,
            "include_globs": ["private name/**/*.rs"],
            "exclude_globs": ["generated secret name/**"],
            "result_mode": "count",
            "timeout_secs": 45
        }]
    });
    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "search_project_texts",
        &raw,
    );
    assert_eq!(raw_summary["query_count"], 1);
    assert_eq!(raw_summary["patterns_present"], true);
    let raw_json = serde_json::to_string(&raw_summary).unwrap();
    assert!(!raw_json.contains("NEVER_LOG_PATTERN_VALUE"));
    assert!(!raw_json.contains("private name"));
    assert!(!raw_json.contains("generated secret name"));

    let call_summary = search_call(
        "agent:demo:project".to_string(),
        SearchRequest {
            pattern: "NEVER_LOG_PATTERN_VALUE".to_string(),
            include_globs: Some(vec!["private name/**/*.rs".to_string()]),
            exclude_globs: Some(vec!["generated secret name/**".to_string()]),
            result_mode: Some(SearchResultMode::Count),
            timeout_secs: Some(45),
            ..raw_search_request()
        },
    )
    .session_log_arguments();
    assert_eq!(call_summary["query_count"], 1);
    assert_eq!(call_summary["patterns_present"], true);
    let call_json = serde_json::to_string(&call_summary).unwrap();
    assert!(!call_json.contains("NEVER_LOG_PATTERN_VALUE"));
    assert!(!call_json.contains("private name"));
    assert!(!call_json.contains("generated secret name"));
}

#[cfg(unix)]
#[test]
fn search_command_passes_shell_metacharacter_globs_as_one_literal_argument() {
    let tmp = crate::test_support::executable_tempdir();
    let bin = tmp.path().join("bin");
    let root = tmp.path().join("project");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    write_executable_script(
        &bin.join("rg"),
        "#!/bin/sh\nfor arg do printf 'ARG=%s\\n' \"$arg\"; done\n",
    );
    write_executable_script(&bin.join("head"), fake_head_script());
    let literal = "src/**/space $HOME; 'double\" `tick`";
    let options = SearchOptions::normalize(SearchRequest {
        include_globs: Some(vec![literal.to_string()]),
        ..raw_search_request()
    })
    .unwrap();
    let cmd = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        search_project_text_command(&options)
    );

    let (exit_code, stdout, stderr, _) = run_command_sync(&cmd, &root, 10);
    assert_eq!(exit_code, 0, "stderr: {stderr}");
    assert!(stdout.contains(&format!("ARG={literal}")), "{stdout}");
    assert!(
        stdout.contains("$HOME"),
        "environment expansion leaked into argv: {stdout}"
    );
    assert!(
        !stdout.contains("\ndouble\" `tick`\n"),
        "glob split into a command: {stdout}"
    );
}

#[tokio::test]
async fn search_project_text_zero_match_include_glob_reports_nonleaking_hint() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = test_runtime();
    let client_id = "search-glob-hint";
    let project = register_runner_project_at_path(&runtime, client_id, "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            pattern: "SCOPE_NEEDLE".to_string(),
                            include_globs: Some(vec!["docs/**/*.md".to_string()]),
                            ..raw_search_request()
                        },
                    ),
                    Some(&auth_context(None, true)),
                )
                .await
        }
    });

    let scoped = wait_for_patch_agent_request(&runtime, client_id).await;
    let scoped_payload: Value = serde_json::from_str(scoped.stdin.as_deref().unwrap()).unwrap();
    assert_eq!(scoped_payload["include_globs"], json!(["docs/**/*.md"]));
    complete_patch_agent_request(
        &runtime,
        client_id,
        &scoped.request_id,
        1,
        "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
        "",
    )
    .await;

    let diagnostic = wait_for_patch_agent_request(&runtime, client_id).await;
    let diagnostic_payload: Value =
        serde_json::from_str(diagnostic.stdin.as_deref().unwrap()).unwrap();
    assert!(diagnostic_payload["include_globs"]
        .as_array()
        .is_some_and(Vec::is_empty));
    assert_eq!(diagnostic_payload["limit"], 1);
    complete_patch_agent_request(
        &runtime,
        client_id,
        &diagnostic.request_id,
        0,
        concat!(
            "{\"webcodex_search\":{\"backend\":\"rg\",\"feature_unavailable\":false}}\n",
            "src/private.rs\0",
            "1:SCOPE_NEEDLE\n"
        ),
        "",
    )
    .await;

    let result = extract_single_search_batch_result(task.await.unwrap());
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"], json!([]));
    assert_eq!(
        result.output["zero_match_hint"],
        "include_globs_excluded_matches"
    );
    assert!(!serde_json::to_string(&result.output)
        .unwrap()
        .contains("src/private.rs"));
    assert_search_output_keys_are_declared(&result.output);
}

#[tokio::test]
async fn search_project_text_include_and_exclude_globs_are_additive() {
    // include/exclude globs are ripgrep-only; without host rg this is a
    // capability error, not a product regression (see
    // advanced_search_without_rg_returns_structured_capability_error).
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
    std::fs::create_dir_all(tmp.path().join("vendor")).unwrap();
    std::fs::create_dir_all(tmp.path().join("secrets")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("docs/guide.md"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("vendor/generated.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("secrets/hidden.rs"), "SCOPE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("notes.txt"), "SCOPE_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-globs", "demo", tmp.path()).await;

    let (result, _) = execute_agent_search(
        &runtime,
        "search-globs",
        project,
        SearchRequest {
            pattern: "SCOPE_NEEDLE".to_string(),
            include_globs: Some(vec!["**/*.rs".to_string(), "docs/**/*.md".to_string()]),
            exclude_globs: Some(vec!["vendor/**".to_string()]),
            limit: Some(10),
            ..raw_search_request()
        },
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    // Result order is not deterministic (the search stops as soon as the
    // budget is met rather than sorting the whole repository), so compare as a
    // set.
    let mut paths = result.output["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    paths.sort();
    assert_eq!(
        paths,
        vec!["docs/guide.md".to_string(), "src/lib.rs".to_string()]
    );
}

#[tokio::test]
async fn search_project_text_files_with_matches_is_unique_stable_and_bounded() {
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("b.rs"), "FILE_NEEDLE\nFILE_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("a.rs"), "FILE_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-files", "demo", tmp.path()).await;

    let (result, _) = execute_agent_search(
        &runtime,
        "search-files",
        project,
        SearchRequest {
            pattern: "FILE_NEEDLE".to_string(),
            limit: Some(1),
            result_mode: Some(SearchResultMode::FilesWithMatches),
            ..raw_search_request()
        },
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["result_mode"], "files_with_matches");
    // Either file may be the one reported when limit=1 stops the scan early.
    let files = result.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 1);
    assert!(matches!(files[0], "a.rs" | "b.rs"));
    assert_eq!(result.output["returned_file_count"], 1);
    assert_eq!(result.output["truncated"], true);
    assert_eq!(result.output["truncation_reason"], "limit");
}

#[tokio::test]
async fn search_project_text_single_file_count_preserves_filename() {
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let path = "count file.rs";
    std::fs::write(tmp.path().join(path), "COUNT_NEEDLE\nCOUNT_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "single-file-count", "demo", tmp.path()).await;
    let (result, _) = execute_agent_search(
        &runtime,
        "single-file-count",
        project,
        SearchRequest {
            pattern: "COUNT_NEEDLE".to_string(),
            path: Some(path.to_string()),
            result_mode: Some(SearchResultMode::Count),
            ..raw_search_request()
        },
    )
    .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["total_matches"], 2);
    assert_eq!(result.output["files"][0]["path"], path);
}

#[tokio::test]
async fn search_project_text_count_distinguishes_complete_and_truncated_totals() {
    // count result mode is ripgrep-only; without host rg this is a capability
    // error, not a product regression (see
    // advanced_search_without_rg_returns_structured_capability_error).
    if !host_ripgrep_available() {
        eprintln!("skipping real-ripgrep integration test: rg is unavailable");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "COUNT_NEEDLE\nCOUNT_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("b.rs"), "COUNT_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-count", "demo", tmp.path()).await;

    let (truncated, _) = execute_agent_search(
        &runtime,
        "search-count",
        project.clone(),
        SearchRequest {
            pattern: "COUNT_NEEDLE".to_string(),
            limit: Some(1),
            result_mode: Some(SearchResultMode::Count),
            ..raw_search_request()
        },
    )
    .await;
    assert!(truncated.success, "{:?}", truncated.error);
    // The truncated count run may stop at whichever file rg happens to reach
    // first, so accept either file as the sole returned record.
    let truncated_files = truncated.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(truncated_files.len(), 1);
    assert!(matches!(truncated_files[0].as_str(), "a.rs" | "b.rs"));
    assert_eq!(truncated.output["returned_file_count"], 1);
    assert_eq!(truncated.output["count_complete"], false);
    assert_eq!(truncated.output["total_matches"], Value::Null);
    assert_eq!(truncated.output["truncated"], true);
    // The single returned file's count is the match count rg reported for it
    // (a.rs=2 or b.rs=1), never a claimed total.
    let returned_match_count = truncated.output["returned_match_count"].as_u64().unwrap();
    assert!(matches!(returned_match_count, 1 | 2));

    let (complete, _) = execute_agent_search(
        &runtime,
        "search-count",
        project,
        SearchRequest {
            pattern: "COUNT_NEEDLE".to_string(),
            limit: Some(10),
            result_mode: Some(SearchResultMode::Count),
            ..raw_search_request()
        },
    )
    .await;
    assert!(complete.success, "{:?}", complete.error);
    assert_eq!(complete.output["result_mode"], "count");
    assert_eq!(complete.output["total_matches"], 3);
    for omitted in [
        "backend",
        "returned_file_count",
        "returned_match_count",
        "count_complete",
        "truncated",
        "truncation_reason",
    ] {
        assert!(
            complete.output.get(omitted).is_none(),
            "{omitted}: {}",
            complete.output
        );
    }
    // Both files are present regardless of traversal order.
    let mut complete_files = complete.output["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| {
            (
                file["path"].as_str().unwrap().to_string(),
                file["match_count"].as_u64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    complete_files.sort();
    assert_eq!(
        complete_files,
        vec![("a.rs".to_string(), 2), ("b.rs".to_string(), 1),]
    );
}

#[tokio::test]
async fn search_project_text_reports_effective_clamped_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("a.rs"), "TIMEOUT_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-timeout", "demo", tmp.path()).await;

    let (low, low_req) = execute_agent_search(
        &runtime,
        "search-timeout",
        project.clone(),
        SearchRequest {
            pattern: "TIMEOUT_NEEDLE".to_string(),
            timeout_secs: Some(0),
            ..raw_search_request()
        },
    )
    .await;
    assert!(low.success, "{:?}", low.error);
    assert_eq!(low_req.timeout_secs, 1);
    assert_eq!(low.output["effective_timeout_secs"], 1);

    let (high, high_req) = execute_agent_search(
        &runtime,
        "search-timeout",
        project,
        SearchRequest {
            pattern: "TIMEOUT_NEEDLE".to_string(),
            timeout_secs: Some(999),
            ..raw_search_request()
        },
    )
    .await;
    assert!(high.success, "{:?}", high.error);
    assert!((29..=30).contains(&high_req.timeout_secs));
    let high_effective_timeout = high.output["effective_timeout_secs"].as_u64().unwrap();
    assert!((29..=30).contains(&high_effective_timeout));
}

#[tokio::test]
async fn advanced_search_without_rg_returns_structured_capability_error() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("project");
    let bin = tmp.path().join("bin");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(root.join("a.rs"), "needle\n").unwrap();
    let runtime = test_runtime();
    let project = register_runner_project_at_path(&runtime, "search-no-rg", "demo", &root).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    search_call(
                        project,
                        SearchRequest {
                            result_mode: Some(SearchResultMode::Count),
                            ..raw_search_request()
                        },
                    ),
                    Some(&bootstrap),
                )
                .await
        }
    });
    let mut req = wait_for_patch_agent_request(&runtime, "search-no-rg").await;
    req.command = format!(
        "PATH={}; export PATH\n{}",
        shell_escape_simple(&bin.to_string_lossy()),
        req.command
    );
    complete_agent_request_by_running_locally(&runtime, "search-no-rg", req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());

    assert!(!result.success);
    assert_search_output_keys_are_declared(&result.output);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "backend_selection");
    assert_eq!(
        result.output["reason_code"],
        "search_backend_feature_unavailable"
    );
    assert_eq!(result.output["detail_code"], "backend_feature_unavailable");
    assert_eq!(result.output["backend"], "grep");
    assert_eq!(result.output["state_changed"], false);
    assert!(result
        .error
        .unwrap()
        .contains("search_backend_feature_unavailable"));
}

#[tokio::test]
async fn search_project_text_no_matches_returns_empty_matches() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("lib.rs"), "pub fn present() {}\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-empty", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                            pattern: "absent_needle".to_string(),
                            pattern_mode: None,
                            path: None,
                            limit: Some(5),
                            context_before: None,
                            context_after: None,
                            include_globs: None,
                            exclude_globs: None,
                            result_mode: None,
                            timeout_secs: None,
                        }],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-empty").await;
    assert!((29..=30).contains(&req.timeout_secs));
    complete_agent_request_by_running_locally(&runtime, "search-empty", req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"], json!([]));
}

#[tokio::test]
async fn search_project_text_excludes_sensitive_and_build_dirs() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("src")).unwrap();
    std::fs::create_dir_all(tmp.path().join("target")).unwrap();
    std::fs::create_dir_all(tmp.path().join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(tmp.path().join("secrets")).unwrap();
    std::fs::create_dir_all(tmp.path().join("tokens")).unwrap();
    std::fs::write(tmp.path().join("src/lib.rs"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join(".env"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("target/out.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(
        tmp.path().join("node_modules/pkg/index.js"),
        "KEEP_SEARCH_NEEDLE\n",
    )
    .unwrap();
    std::fs::write(tmp.path().join("secrets/key.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("tokens/api.txt"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    std::fs::write(tmp.path().join("id.key"), "KEEP_SEARCH_NEEDLE\n").unwrap();
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "search-excludes", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                            pattern: "KEEP_SEARCH_NEEDLE".to_string(),
                            pattern_mode: None,
                            path: None,
                            limit: Some(10),
                            context_before: None,
                            context_after: None,
                            include_globs: None,
                            exclude_globs: None,
                            result_mode: None,
                            timeout_secs: None,
                        }],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-excludes").await;
    complete_agent_request_by_running_locally(&runtime, "search-excludes", req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["matches"].as_array().unwrap().len(), 1);
    assert_eq!(result.output["matches"][0]["path"], "src/lib.rs");
}

#[tokio::test]
async fn project_overview_routes_to_owning_agent_and_returns_structured_metadata() {
    let temp = tempfile::tempdir().unwrap();
    for path in [
        "AGENTS.md",
        "README.md",
        "Cargo.toml",
        "src/lib.rs",
        "target/debug/output",
        ".env",
    ] {
        let path = temp.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "private fixture content").unwrap();
    }
    let runtime = test_runtime();
    let project =
        register_runner_project_at_path(&runtime, "overview-agent", "demo", temp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ProjectOverview {
                        project,
                        session_id: None,
                        path: None,
                        max_depth: Some(99),
                        limit: Some(1),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let request = wait_for_patch_agent_request(&runtime, "overview-agent").await;
    assert_eq!(request.kind, "file_project_overview");
    assert!(
        request.command.is_empty(),
        "read_project_overview must not use shell"
    );
    let options: Value = serde_json::from_str(request.content.as_deref().unwrap()).unwrap();
    assert_eq!(options["max_depth"], 4);
    assert_eq!(options["limit"], 20);
    complete_project_overview_agent_request_locally(&runtime, "overview-agent", &request).await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["schema_version"], 1);
    assert_eq!(result.output["project"], project);
    assert_eq!(result.output["path"], "");
    assert_eq!(result.output["deterministic"], true);
    assert_eq!(result.output["scan"]["max_depth"], 4);
    assert_eq!(result.output["scan"]["limit"], 20);
    let declared_output = registered_tool_specs()
        .into_iter()
        .find(|spec| spec.name == "read_project_overview")
        .expect("read_project_overview spec")
        .output_schema["properties"]["output"]["properties"]
        .as_object()
        .expect("read_project_overview output schema")
        .clone();
    for key in result.output.as_object().unwrap().keys() {
        assert!(
            declared_output.contains_key(key),
            "runtime read_project_overview output key {key} is missing from schema"
        );
    }
    let serialized = result.output.to_string();
    assert!(!serialized.contains("private fixture content"));
    assert!(!serialized.contains("target"));
    assert!(!serialized.contains(".env"));
    assert!(!serialized.contains(&temp.path().display().to_string()));
}

#[tokio::test]
async fn project_read_adapters_reject_out_of_project_paths_before_agent_dispatch() {
    let runtime = runtime_with_agent_project("path-boundary");
    register_agent(
        &runtime,
        "path-boundary",
        None,
        RunnerCapabilities {
            file_read: true,
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    let project = agent_test_project_id("path-boundary");
    let calls = vec![
        (
            "read_file parent traversal",
            ToolCall::ReadFiles {
                project: project.clone(),
                items: vec![crate::tool_runtime::ReadFilesItem {
                    path: "../outside.txt".to_string(),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                }],
                session_id: None,
                with_line_numbers: None,
                max_result_bytes: None,
            },
            None,
        ),
        (
            "read_file nested parent traversal",
            ToolCall::ReadFiles {
                project: project.clone(),
                items: vec![crate::tool_runtime::ReadFilesItem {
                    path: "src/../../outside.txt".to_string(),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                }],
                session_id: None,
                with_line_numbers: None,
                max_result_bytes: None,
            },
            None,
        ),
        (
            "read_file absolute path",
            ToolCall::ReadFiles {
                project: project.clone(),
                items: vec![crate::tool_runtime::ReadFilesItem {
                    path: "/etc/passwd".to_string(),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                }],
                session_id: None,
                with_line_numbers: None,
                max_result_bytes: None,
            },
            None,
        ),
        (
            "read_file deep parent traversal",
            ToolCall::ReadFiles {
                project: project.clone(),
                items: vec![crate::tool_runtime::ReadFilesItem {
                    path: "sub/../../../etc/passwd".to_string(),
                    start_line: None,
                    limit: None,
                    expected_read_revision: None,
                }],
                session_id: None,
                with_line_numbers: None,
                max_result_bytes: None,
            },
            None,
        ),
        (
            "list_project_files absolute path",
            ToolCall::ListProjectFiles {
                project: project.clone(),
                session_id: None,
                path: Some("/etc".to_string()),
                limit: None,
                offset: None,
            },
            None,
        ),
        (
            "list_project_files parent traversal",
            ToolCall::ListProjectFiles {
                project: project.clone(),
                session_id: None,
                path: Some("../outside".to_string()),
                limit: None,
                offset: None,
            },
            None,
        ),
        (
            "read_project_overview absolute path",
            ToolCall::ProjectOverview {
                project: project.clone(),
                session_id: None,
                path: Some("/etc".to_string()),
                max_depth: None,
                limit: None,
            },
            None,
        ),
        (
            "read_project_overview parent traversal",
            ToolCall::ProjectOverview {
                project: project.clone(),
                session_id: None,
                path: Some("../outside".to_string()),
                max_depth: None,
                limit: None,
            },
            None,
        ),
        (
            "search_project_text absolute path",
            ToolCall::SearchProjectTexts {
                project: project.clone(),
                queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                    pattern: "needle".to_string(),
                    pattern_mode: None,
                    path: Some("/etc".to_string()),
                    limit: None,
                    context_before: None,
                    context_after: None,
                    include_globs: None,
                    exclude_globs: None,
                    result_mode: None,
                    timeout_secs: None,
                }],
                session_id: None,
                max_result_bytes: None,
            },
            Some("path"),
        ),
        (
            "search_project_text parent traversal",
            ToolCall::SearchProjectTexts {
                project,
                queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                    pattern: "needle".to_string(),
                    pattern_mode: None,
                    path: Some("../outside".to_string()),
                    limit: None,
                    context_before: None,
                    context_after: None,
                    include_globs: None,
                    exclude_globs: None,
                    result_mode: None,
                    timeout_secs: None,
                }],
                session_id: None,
                max_result_bytes: None,
            },
            Some("path"),
        ),
    ];

    for (case, call, structured_field) in calls {
        let is_batch = matches!(
            &call,
            ToolCall::ReadFiles { .. } | ToolCall::SearchProjectTexts { .. }
        );
        let result = runtime.dispatch_with_auth(call, Some(&bootstrap)).await;
        let result = if is_batch {
            extract_single_search_batch_result(result)
        } else {
            result
        };
        assert!(!result.success, "{case} escaped the project boundary");
        let error = result.error.as_deref().unwrap_or("");
        assert!(
            error.contains("project-relative")
                || error.contains("parent traversal")
                || error.contains("path"),
            "{case}: {error}"
        );
        if structured_field.is_some() {
            assert_eq!(
                result.output["error_kind"], "search_project_text_failed",
                "{case}"
            );
            assert_eq!(
                result.output["failure_stage"], "request_validation",
                "{case}"
            );
            assert_eq!(result.output["reason_code"], "invalid_path", "{case}");
            assert_eq!(result.output["detail_code"], "invalid_path", "{case}");
        }
        assert!(
            probe_patch_agent_request(&runtime, "path-boundary")
                .await
                .is_none(),
            "{case} must reject before Agent dispatch"
        );
    }
}

#[tokio::test]
async fn search_project_text_requires_shell_capability() {
    let runtime = runtime_with_agent_project("oe");
    let caps = RunnerCapabilities {
        shell: false,
        ..Default::default()
    };
    register_agent(&runtime, "oe", None, caps).await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectTexts {
                project: agent_test_project_id("oe"),
                queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                    pattern: "fn".to_string(),
                    pattern_mode: None,
                    path: None,
                    limit: None,
                    context_before: None,
                    context_after: None,
                    include_globs: None,
                    exclude_globs: None,
                    result_mode: None,
                    timeout_secs: None,
                }],
                session_id: None,
                max_result_bytes: None,
            },
            Some(&bootstrap),
        )
        .await;
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("shell"),
        "search_project_text should require shell capability"
    );
}

#[tokio::test]
async fn search_project_text_context_does_not_enqueue_python_helper() {
    let runtime = test_runtime();
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join("notes.txt"),
        "before\nneedle appears here\nafter\n",
    )
    .unwrap();
    let project =
        register_runner_project_at_path(&runtime, "search-native", "demo", tmp.path()).await;
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::SearchProjectTexts {
                        project,
                        queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                            pattern: "needle".to_string(),
                            pattern_mode: None,
                            path: None,
                            limit: Some(5),
                            context_before: Some(1),
                            context_after: Some(1),
                            include_globs: None,
                            exclude_globs: None,
                            result_mode: None,
                            timeout_secs: None,
                        }],
                        session_id: None,
                        max_result_bytes: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "search-native").await;
    let forbidden = ["python3", "-c"].join(" ");
    assert!(
        !req.command.contains(&forbidden),
        "search context must not enqueue a Python helper: {}",
        req.command
    );
    assert!(req.command.contains("command -v rg"));
    assert!(req.command.contains("rg --with-filename --null"));
    assert!(req.command.contains("grep -rHnI --null"));
    complete_agent_request_by_running_locally(&runtime, "search-native", req).await;
    let result = extract_single_search_batch_result(task.await.unwrap());

    assert!(result.success, "{:?}", result.error);
    assert!(matches!(
        result.output.get("backend").and_then(Value::as_str),
        None | Some("grep")
    ));
    assert_eq!(result.output["context_before"], 1);
    assert_eq!(result.output["context_after"], 1);
    let first = &result.output["matches"][0];
    assert_eq!(first["path"], "notes.txt");
    assert_eq!(first["line"], 2);
    assert_eq!(
        first["context_before"][0],
        json!({"line": 1, "text": "before"})
    );
    assert_eq!(
        first["context_after"][0],
        json!({"line": 3, "text": "after"})
    );
}

#[tokio::test]
async fn list_project_files_rejects_non_agent_project_id() {
    // A bare project id (not agent:<client>:<project>) is not resolved by
    // the runtime surface — proving routing goes through the owning agent.
    let runtime = test_runtime();
    let result = runtime
        .dispatch(ToolCall::ListProjectFiles {
            project: "some-local-id".to_string(),
            session_id: None,
            path: None,
            limit: None,
            offset: None,
        })
        .await;
    assert!(!result.success);
    let err = result.error.unwrap();
    assert!(err.contains("agent"), "{err}");
    assert!(!err.contains("projects.toml"), "{err}");
}

#[tokio::test]
async fn search_project_text_rejects_empty_pattern() {
    // Authorization runs before the tool body, so register an agent with
    // shell capability to reach the empty-pattern validation.
    let runtime = runtime_with_agent_project("oe");
    register_agent(
        &runtime,
        "oe",
        None,
        RunnerCapabilities {
            shell: true,
            ..Default::default()
        },
    )
    .await;
    let bootstrap = auth_context(None, true);
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectTexts {
                project: agent_test_project_id("oe"),
                queries: vec![crate::tool_runtime::SearchProjectTextsQuery {
                    pattern: "   ".to_string(),
                    pattern_mode: None,
                    path: None,
                    limit: None,
                    context_before: None,
                    context_after: None,
                    include_globs: None,
                    exclude_globs: None,
                    result_mode: None,
                    timeout_secs: None,
                }],
                session_id: None,
                max_result_bytes: None,
            },
            Some(&bootstrap),
        )
        .await;
    let result = extract_single_search_batch_result(result);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "search_project_text_failed");
    assert_eq!(result.output["failure_stage"], "request_validation");
    assert_eq!(result.output["reason_code"], "invalid_pattern");
    assert_eq!(result.output["detail_code"], "invalid_pattern");
    assert_eq!(result.output["state_changed"], false);
}
