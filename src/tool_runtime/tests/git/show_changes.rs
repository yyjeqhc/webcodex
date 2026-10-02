#[test]
fn show_changes_command_is_read_only() {
    let without_diff = show_changes_command(false, 20, 80);
    let with_diff = show_changes_command(true, 20, 80);
    assert!(
        without_diff.len() <= crate::runner_protocol::RAW_SHELL_COMMAND_MAX_BYTES,
        "include_diff=false command is {} bytes",
        without_diff.len()
    );
    assert!(
        with_diff.len() <= crate::runner_protocol::RAW_SHELL_COMMAND_MAX_BYTES,
        "include_diff=true command is {} bytes",
        with_diff.len()
    );
    eprintln!(
        "show_changes_command_lengths without_diff={} with_diff={}",
        without_diff.len(),
        with_diff.len()
    );
    for cmd in [&without_diff, &with_diff] {
        assert!(cmd.contains("git status --porcelain=v1 -b"));
        assert!(cmd.contains("git log -1"));
        assert!(cmd.contains("git diff --stat"));
        assert!(cmd.contains("LC_ALL=C; export LC_ALL"));
        assert!(!cmd.contains("head_buf=$("), "HEAD must be streamed: {cmd}");
        assert!(
            !cmd.contains("stat_buf=$("),
            "diff-stat must be streamed: {cmd}"
        );
        assert!(
            !cmd.contains("while IFS= read -r hline"),
            "HEAD subject must not be read into an unbounded shell variable: {cmd}"
        );
        assert!(
            !cmd.contains("head_buf="),
            "HEAD must not accumulate an unbounded multi-line buffer: {cmd}"
        );
        assert!(
            cmd.contains("head_subject=$(git log -1 --format=%s \"$head_commit\" 2>/dev/null | dd bs=1 count=$((head_subject_limit+1)) 2>/dev/null)"),
            "HEAD subject producer must be byte-bounded before command substitution: {cmd}"
        );
        assert!(
            cmd.contains(&format!(
                "head_subject_limit=$(({}-head_prefix_bytes))",
                SHOW_CHANGES_HEAD_BYTES
            )),
            "HEAD subject limit must derive from the remaining frame budget: {cmd}"
        );
        let forbidden = ["python3", "-c"].join(" ");
        assert!(
            !cmd.contains(&forbidden),
            "read_workspace_changes command must not invoke a Python helper: {cmd}"
        );
        for forbidden in [
            " clean",
            " restore",
            " add",
            " commit",
            " reset",
            " checkout",
            " push",
            " stash",
            " merge",
            " rebase",
            " rm ",
        ] {
            assert!(
                !cmd.contains(forbidden),
                "read_workspace_changes command must not contain '{}': {}",
                forbidden,
                cmd
            );
        }
    }
}

#[test]
fn show_changes_command_emits_bounded_metadata_frames() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    for i in 0..3 {
        std::fs::write(tmp.path().join(format!("new{i}.txt")), "x\n").unwrap();
    }
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let cmd = show_changes_command(true, 2, 5);
    let (exit, stdout, stderr, _) = run_command_sync(&cmd, tmp.path(), 30);
    assert_eq!(
        exit, 0,
        "command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("## "), "missing branch header: {stdout}");
    assert!(
        stdout.contains("files_total="),
        "missing files_total: {stdout}"
    );
    assert!(
        stdout.contains("files_returned="),
        "missing files_returned: {stdout}"
    );
    assert!(
        stdout.contains("files_limit="),
        "missing files_limit: {stdout}"
    );
    assert!(
        stdout.contains("diff_hunks_returned="),
        "missing diff meta: {stdout}"
    );
    assert!(
        stdout.contains("diff_hunks_truncated="),
        "missing diff truncation: {stdout}"
    );
    for field in [
        "status_bytes=",
        "head_bytes=",
        "diff_stat_bytes=",
        "diff_trunc_hunk_count=",
        "diff_trunc_hunk_lines=",
        "diff_trunc_bytes=",
        "diff_trunc_bytes_in_hunk=",
        "diff_bytes=",
    ] {
        assert!(stdout.contains(field), "missing {field}: {stdout}");
    }
    // No raw placeholder may leak into the generated command.
    assert!(
        !cmd.contains("__SENTINEL__"),
        "sentinel placeholder leaked: {cmd}"
    );
    assert!(
        !cmd.contains("__HUNK_LIMIT__") && !cmd.contains("__LINE_LIMIT__"),
        "limit placeholder leaked: {cmd}"
    );
}

#[test]
fn show_changes_bounds_status_files_and_keeps_totals_exact() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    // Exceed the production status-file cap by many untracked files.
    let cap = 200usize;
    let total = cap + 50;
    for i in 0..total {
        std::fs::write(tmp.path().join(format!("u{i}.txt")), "x\n").unwrap();
    }
    let output = bounded_show_changes_output(tmp.path(), true, 4, 80);
    assert_eq!(
        output["files_total"].as_u64(),
        Some(total as u64),
        "files_total must count every entry: {output}"
    );
    assert_eq!(
        output["files_returned"].as_u64(),
        Some(cap as u64),
        "files_returned must hit the cap: {output}"
    );
    assert_eq!(
        output["files_truncated"], true,
        "must report truncation: {output}"
    );
    assert_eq!(
        output["files_limit"].as_u64(),
        Some(cap as u64),
        "files_limit must equal the cap: {output}"
    );
    let files = output["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        cap,
        "files array length must equal returned: {output}"
    );
    // Per-category counts must reflect ALL entries, not the truncated subset.
    assert_eq!(
        output["counts"]["untracked"].as_u64(),
        Some(total as u64),
        "untracked count must be exact over all entries: {output}"
    );
    assert_eq!(output["clean"], false, "truncated dirty repo is not clean");
    // The production-side count cap fired, so output_truncated is true with a
    // stable reason; the bound is still provably transport-safe.
    assert_eq!(output["transport_safe"], true);
    assert_eq!(output["output_truncated"], true);
    let reasons = output["truncation_reasons"]
        .as_array()
        .expect("truncation reasons array");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("status_file_count_limit")),
        "expected status_file_count_limit reason: {reasons:?}"
    );
    assert_show_changes_envelope_value_matches_schema(&output, "bounded status");
}

/// Build a long (but <= NAME_MAX) leaf filename so a *real* >256 KiB `git
/// status` can be produced from many tracked files in a single flat directory,
/// without relying on a single path exceeding filesystem limits. Each name is
/// ~237 printable chars plus a per-file suffix and `.x`, staying under the
/// 255-byte NAME_MAX.
fn long_leaf_name(i: usize) -> String {
    format!("{}{i}.x", "g".repeat(234))
}

#[test]
fn show_changes_status_over_retention_floor_keeps_branch_header_observable() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    // A *real* >256 KiB status: ~1200 tracked files with long (~237-byte)
    // names in a single flat directory, committed and then modified. The full
    // status legitimately exceeds the ordinary Runner result-retention
    // compatibility floor, not just the count limit.
    let dir = tmp.path().join("d");
    std::fs::create_dir_all(&dir).unwrap();
    let total = 1200usize;
    for i in 0..total {
        let path = dir.join(long_leaf_name(i));
        std::fs::write(&path, "x\n").unwrap();
    }
    let (add_exit, _, add_stderr, _) = run_command_sync("git add -A", tmp.path(), 30);
    assert_eq!(add_exit, 0, "git add failed: {add_stderr}");
    // Use `-q` so the commit summary (which can be very large for many files)
    // does not flood stdout under the synchronous command runner.
    let (commit_exit, _, commit_stderr, _) = run_command_sync("git commit -qm add", tmp.path(), 30);
    assert_eq!(commit_exit, 0, "git commit failed: {commit_stderr}");
    for i in 0..total {
        let path = dir.join(long_leaf_name(i));
        std::fs::write(&path, "y\nx\n").unwrap();
    }
    // First assert the *unbounded* raw status actually exceeds 256 KiB. The raw
    // status legitimately exceeds the OS pipe buffer, so use the full-capture
    // helper that drains the pipes concurrently (the polling helper deadlocks).
    let (raw_exit, raw_stdout, raw_stderr) =
        run_command_full_capture("git status --porcelain=v1 -b", tmp.path(), 30);
    assert_eq!(
        raw_exit, 0,
        "raw status failed\nstdout:\n{raw_stdout}\nstderr:\n{raw_stderr}"
    );
    let raw_status_bytes = raw_stdout.len();
    assert!(
        raw_status_bytes > ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES,
        "raw status must exceed the ordinary Runner result-retention compatibility floor; got {raw_status_bytes} bytes"
    );
    // The bounded show_changes command must stay within the production budget.
    // Its output is also larger than the pipe buffer, so use full capture.
    let cmd = show_changes_command(false, 20, 80);
    let (exit, stdout, stderr) = run_command_full_capture(&cmd, tmp.path(), 30);
    assert_eq!(
        exit, 0,
        "command failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let bounded_stdout_bytes = stdout.len();
    assert!(
        bounded_stdout_bytes <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded stdout must stay within the production budget; got {bounded_stdout_bytes} bytes (budget {})",
        SHOW_CHANGES_OUTPUT_BUDGET_BYTES
    );
    // Simulate the ordinary Runner per-stream result-retention compatibility
    // floor. This is not a polling/WebSocket/QUIC wire-body or frame ceiling.
    let retained = simulate_runner_result_retention_tail(
        &stdout,
        ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES,
    );
    assert_eq!(
        retained, stdout,
        "bounded output must be unchanged by ordinary Runner result retention"
    );
    let frames = split_show_changes_stdout(&retained, false);
    assert!(
        frames
            .status
            .lines()
            .any(|line| parse_status_header(line).is_some()),
        "branch header must survive ordinary Runner result retention: {retained}"
    );
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, "");
    assert!(observation.status_observed());
    // Totals stay exact even though records were bounded.
    assert_eq!(frames.files_total, Some(total));
    assert_eq!(frames.files_truncated, Some(true));
    assert!(frames.files_returned.unwrap_or(0) <= SHOW_CHANGES_MAX_STATUS_FILES);
    // Parse the structured output to assert transport/truncation fields.
    let output = parse_show_changes_output_with_observation(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        None,
        20,
        80,
        Some(exit),
        &stderr,
        observation,
        &frames,
    );
    assert_eq!(output["transport_safe"], true);
    assert_eq!(output["output_truncated"], true);
    let reasons = output["truncation_reasons"]
        .as_array()
        .expect("truncation reasons array");
    assert!(
        reasons
            .iter()
            .any(|r| r.as_str() == Some("status_file_count_limit")
                || r.as_str() == Some("status_byte_budget")),
        "expected a status count or byte budget reason: {reasons:?}"
    );
    assert_show_changes_envelope_value_matches_schema(&output, "256k status");
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[test]
fn show_changes_head_fields_match_git_in_sh_and_bash() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let subject = "fix: preserve labelled HEAD fields exactly";
    commit_file(tmp.path(), "README.md", "hello\n", subject);

    let expected = |git_command: &str| {
        let (exit, stdout, stderr) = run_command_full_capture(git_command, tmp.path(), 30);
        assert_eq!(exit, 0, "{git_command} failed: {stderr}");
        stdout.trim_end_matches('\n').to_string()
    };
    let expected_commit = expected("git rev-parse HEAD");
    let expected_short = expected("git rev-parse --short HEAD");
    let expected_summary = expected("git log -1 --format=%s");
    let command = show_changes_command(false, 20, 80);

    for shell in ["sh", "bash"] {
        let wrapped = format!("{shell} -c {}", shell_single_quote(&command));
        let (exit, stdout, stderr) = run_command_full_capture(&wrapped, tmp.path(), 30);
        assert_eq!(exit, 0, "{shell} failed: {stderr}\n{stdout}");
        let frames = split_show_changes_stdout(&stdout, false);
        assert_eq!(frames.head_bytes, Some(frames.head.len()));
        let output =
            bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
        assert_eq!(
            output["head"]["commit"].as_str(),
            Some(expected_commit.as_str()),
            "{shell} commit"
        );
        assert_eq!(
            output["head"]["short"].as_str(),
            Some(expected_short.as_str()),
            "{shell} short"
        );
        assert_eq!(
            output["head"]["summary"].as_str(),
            Some(expected_summary.as_str()),
            "{shell} summary"
        );
        assert_eq!(output["transport_safe"], true, "{shell}: {output}");
        eprintln!(
            "head_shell={shell} commit={expected_commit} short={expected_short} summary={expected_summary}"
        );
    }
}

#[test]
fn show_changes_unborn_repository_emits_empty_complete_head_frame() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let command = show_changes_command(false, 20, 80);
    let wrapped = format!("sh -c {}", shell_single_quote(&command));
    let (exit, stdout, stderr) = run_command_full_capture(&wrapped, tmp.path(), 30);
    assert_eq!(
        exit, 0,
        "unborn read_workspace_changes failed: {stderr}\n{stdout}"
    );
    assert!(stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES);
    let frames = split_show_changes_stdout(&stdout, false);
    assert!(frames.head.is_empty());
    assert!(frames.head_exit.is_some_and(|value| value != 0));
    assert_eq!(frames.head_truncated, Some(false));
    assert_eq!(frames.head_bytes, Some(0));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["head"]["commit"], Value::Null);
    assert_eq!(output["head"]["short"], Value::Null);
    assert_eq!(output["head"]["summary"], Value::Null);
    assert_eq!(output["transport_safe"], true);
}

#[test]
fn show_changes_crlf_diff_bytes_match_parsed_frame_exactly() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "crlf.txt", "before\r\n", "track CRLF");
    std::fs::write(tmp.path().join("crlf.txt"), b"before\r\nafter\r\n").unwrap();

    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    assert!(
        frames.diff.ends_with('\r'),
        "final CR from CRLF diff line was lost: {:?}",
        frames.diff.as_bytes().last()
    );
    assert_eq!(frames.diff_bytes, Some(frames.diff.len()));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true, "{output}");
    eprintln!(
        "crlf_diff frame_bytes={} metadata_bytes={}",
        frames.diff.len(),
        frames.diff_bytes.unwrap()
    );
}

fn multibyte_status_path(i: usize) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::new();
    for level in 0..3 {
        path.push(format!("{}-{level}", "界".repeat(60)));
    }
    path.push(format!("文件-{i:03}.txt"));
    path
}

#[test]
fn show_changes_bash_utf8_locale_counts_multibyte_status_in_bytes() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let total = 500usize;
    for i in 0..total {
        let relative = multibyte_status_path(i);
        let path = tmp.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "before\n").unwrap();
    }
    let (setup_exit, _, setup_stderr) = run_command_full_capture(
        "git config core.quotePath false && git add -A && git commit -qm multibyte",
        tmp.path(),
        60,
    );
    assert_eq!(setup_exit, 0, "setup failed: {setup_stderr}");
    for i in 0..total {
        std::fs::write(tmp.path().join(multibyte_status_path(i)), "after\n").unwrap();
    }
    let (raw_exit, raw_status, raw_stderr) = run_command_full_capture(
        "LC_ALL=zh_CN.utf8 git status --porcelain=v1 -b",
        tmp.path(),
        60,
    );
    assert_eq!(raw_exit, 0, "raw status failed: {raw_stderr}");
    assert!(
        raw_status.len() > ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES,
        "raw status bytes={}",
        raw_status.len()
    );

    let cmd = show_changes_command(false, 20, 80);
    let bash_cmd = format!("LC_ALL=zh_CN.utf8 bash -c {}", shell_single_quote(&cmd));
    let (exit, stdout, stderr) = run_command_full_capture(&bash_cmd, tmp.path(), 60);
    assert_eq!(exit, 0, "bash command failed: {stderr}");
    assert!(
        stdout.len() <= SHOW_CHANGES_OUTPUT_BUDGET_BYTES,
        "bounded bytes={}",
        stdout.len()
    );
    assert_eq!(
        simulate_runner_result_retention_tail(
            &stdout,
            ORDINARY_RUNNER_RESULT_RETENTION_COMPAT_BYTES,
        )
        .as_bytes(),
        stdout.as_bytes()
    );

    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &stderr);
    assert!(observation.status_observed());
    assert!(frames
        .status
        .lines()
        .any(|line| parse_status_header(line).is_some()));
    assert_eq!(frames.files_total, Some(total));
    assert_eq!(frames.counts_modified, Some(total));
    let output =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), false, 20, 80, &stderr);
    assert_eq!(output["transport_safe"], true);
    eprintln!(
        "multibyte_status raw_bytes={} bounded_bytes={}",
        raw_status.len(),
        stdout.len()
    );
}

#[test]
fn show_changes_transport_safe_requires_every_modern_metadata_frame() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "before\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "after\n").unwrap();
    let (_, stdout, stderr) = run_bounded_show_changes_full(tmp.path(), true, 20, 80);
    let frames = split_show_changes_stdout(&stdout, true);
    let complete =
        bounded_show_changes_output_from_frames(&frames, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(complete["transport_safe"], true);

    let mut variants = Vec::new();
    let mut missing_status = frames.clone();
    missing_status.status_bytes = None;
    variants.push(("status", missing_status));
    let mut missing_head = frames.clone();
    missing_head.head_bytes = None;
    variants.push(("head", missing_head));
    let mut missing_stat = frames.clone();
    missing_stat.diff_stat_bytes = None;
    variants.push(("stat", missing_stat));
    let mut missing_diff = frames.clone();
    missing_diff.diff_trunc_bytes = None;
    variants.push(("diff", missing_diff));
    let mut missing_diff_hunk_byte_provenance = frames.clone();
    missing_diff_hunk_byte_provenance.diff_trunc_bytes_in_hunk = None;
    variants.push((
        "diff_hunk_byte_provenance",
        missing_diff_hunk_byte_provenance,
    ));

    for (label, incomplete) in variants {
        let output =
            bounded_show_changes_output_from_frames(&incomplete, tmp.path(), true, 20, 80, &stderr);
        assert_eq!(output["transport_safe"], false, "missing {label} metadata");
    }

    let mut wrong_diff_bytes = frames.clone();
    wrong_diff_bytes.diff_bytes = Some(frames.diff.len() + 1);
    let wrong_diff_output = bounded_show_changes_output_from_frames(
        &wrong_diff_bytes,
        tmp.path(),
        true,
        20,
        80,
        &stderr,
    );
    assert_eq!(wrong_diff_output["transport_safe"], false);

    let mut oversized_stat = frames.clone();
    oversized_stat.stat = "x".repeat(SHOW_CHANGES_DIFF_STAT_BYTES + 1);
    oversized_stat.diff_stat_bytes = Some(oversized_stat.stat.len());
    let oversized_output =
        bounded_show_changes_output_from_frames(&oversized_stat, tmp.path(), true, 20, 80, &stderr);
    assert_eq!(oversized_output["transport_safe"], false);
    eprintln!(
        "invalid_metadata wrong_diff_bytes_transport_safe={} oversized_stat_transport_safe={}",
        wrong_diff_output["transport_safe"], oversized_output["transport_safe"]
    );
}

#[test]
fn show_changes_status_config_error_is_not_reported_clean() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    let (config_exit, _, config_stderr, _) = run_command_sync(
        "git config status.showUntrackedFiles invalid",
        tmp.path(),
        30,
    );
    assert_eq!(
        config_exit, 0,
        "failed to set regression config: {config_stderr}"
    );
    let cmd = show_changes_command(false, 20, 80);
    let (exit, stdout, _stderr, _) = run_command_sync(&cmd, tmp.path(), 30);
    assert_ne!(exit, 0, "status config error must not exit 0");
    let frames = split_show_changes_stdout(&stdout, false);
    let observation =
        parse_show_changes_status_observation(&frames.status, &frames.status_result, &_stderr);
    assert_eq!(observation.as_json()["status"], "command_failed");
    assert_eq!(
        observation.as_json()["reason_code"],
        "git_status_config_error"
    );
    // A failed status must never claim cleanliness.
    let output = parse_show_changes_output(
        "demo",
        &frames.status,
        &frames.head,
        &frames.stat,
        None,
        20,
        80,
        Some(exit),
        &_stderr,
    );
    assert_eq!(output["clean"], Value::Null);
    assert_eq!(output["counts"]["conflicted"], Value::Null);
}

#[test]
fn show_changes_include_diff_false_omits_diff_and_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();
    let output = bounded_show_changes_output(tmp.path(), false, 20, 80);
    assert!(
        output.get("hunks").is_none(),
        "no hunks without include_diff"
    );
    assert!(output.get("hunk_count").is_none());
    assert!(output.get("hunks_truncated").is_none());
    assert!(output.get("diff_review_handoff").is_none());
}

#[test]
fn show_changes_complete_diff_does_not_handoff_to_git_diff_hunks() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();

    let output = bounded_show_changes_output(tmp.path(), true, 20, 80);
    assert_eq!(output["hunks_truncated"], false);
    assert_eq!(
        output["hunks"][0]["hunks"][0]["source_completeness"],
        "complete"
    );
    assert!(output.get("diff_review_handoff").is_none());
    assert!(!output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|action| action
            .as_str()
            .is_some_and(|action| action.contains("read_git_diff_hunks"))));
    assert_show_changes_envelope_value_matches_schema(&output, "complete diff handoff");
}

#[test]
fn show_changes_small_mid_file_edit_stays_within_default_hunk_lines() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let original = (0..240)
        .map(|index| format!("line-{index:03}\n"))
        .collect::<String>();
    commit_file(tmp.path(), "middle.txt", &original, "initial");
    let changed = original.replacen("line-120\n", "line-120 changed\n", 1);
    std::fs::write(tmp.path().join("middle.txt"), changed).unwrap();

    let output = bounded_show_changes_output(tmp.path(), true, 20, 80);
    assert_eq!(output["hunks_truncated"], false, "{output}");
    assert!(output.get("diff_review_handoff").is_none(), "{output}");
    let diff = output["hunks"][0]["hunks"][0]["diff"]
        .as_str()
        .expect("returned hunk diff");
    assert!(
        diff.lines().count() <= 80,
        "ordinary small mid-file change should fit the default hunk line budget: {diff}"
    );
}

#[test]
fn show_changes_complete_model_projection_removes_only_derived_review_metadata() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    std::fs::write(tmp.path().join("README.md"), "hello\nchanged\n").unwrap();

    let canonical = bounded_show_changes_output(tmp.path(), true, 20, 80);
    assert_eq!(canonical["transport_safe"], true);
    assert_eq!(canonical["output_truncated"], false);
    assert_eq!(canonical["hunks_truncated"], false);
    assert!(canonical.get("hunk_count").is_some());
    assert!(canonical["hunks"][0]["hunks"][0]
        .get("line_count")
        .is_some());

    let mut projected = ToolResult::ok(canonical.clone());
    sparsify_complete_git_review_success("read_workspace_changes", &mut projected);
    let output = &projected.output;
    for retained in [
        "project",
        "branch",
        "head",
        "counts",
        "files",
        "diff_stat",
        "hunks",
    ] {
        assert!(
            output.get(retained).is_some(),
            "missing retained {retained}: {output}"
        );
    }
    for omitted in [
        "git_available",
        "non_git_project",
        "status_observation",
        "files_total",
        "files_returned",
        "files_truncated",
        "files_limit",
        "transport_safe",
        "output_budget_bytes",
        "output_truncated",
        "truncation_reasons",
        "diff_exit",
        "diff_status",
        "diff_stat_exit",
        "diff_stat_status",
        "head_exit",
        "hunk_count",
        "hunks_truncated",
        "warnings",
        "exit_code",
        "stderr",
    ] {
        assert!(
            output.get(omitted).is_none(),
            "derived {omitted} leaked: {output}"
        );
    }
    assert!(output["hunks"][0].get("old_path").is_none());
    assert!(output["hunks"][0]["hunks"][0].get("line_count").is_none());
    assert!(output["hunks"][0]["hunks"][0].get("truncated").is_none());
    assert_eq!(
        output["hunks"][0]["hunks"][0]["source_completeness"],
        "complete"
    );
    assert!(canonical.get("transport_safe").is_some());
    assert!(canonical.get("hunk_count").is_some());
}

#[test]
fn show_changes_diff_respects_max_hunks() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "a.txt", "a\n", "initial");
    // Three modified files -> three diff hunks; cap at 1.
    for name in ["a.txt", "b.txt", "c.txt"] {
        commit_file(tmp.path(), name, "line\n", "add");
        std::fs::write(tmp.path().join(name), "line\nmore\n").unwrap();
    }
    let output = bounded_show_changes_output(tmp.path(), true, 1, 80);
    assert_eq!(
        output["hunk_count"].as_u64(),
        Some(1),
        "must cap at max_hunks"
    );
    assert_eq!(
        output["hunks_truncated"], true,
        "must report hunk truncation"
    );
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r == "diff_hunk_count_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_hunk_line_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_byte_budget"));
    assert_eq!(
        output["hunks"][0]["hunks"][0]["source_completeness"], "complete",
        "page-only truncation must not make a returned hunk look source-incomplete"
    );
    let next_call = &output["diff_review_handoff"]["next_call"];
    assert_eq!(next_call["tool"], "read_git_diff_hunks");
    assert_eq!(next_call["arguments"]["project"], "demo");
    assert_eq!(next_call["arguments"]["cached"], false);
    assert_eq!(next_call["arguments"]["paths"], json!([]));
    assert_eq!(next_call["arguments"]["max_hunks"], 30);
    assert_eq!(
        next_call["arguments"]["max_page_bytes"],
        DEFAULT_GIT_DIFF_HUNKS_PAGE_BYTES
    );
    assert_git_diff_hunks_recovery_call_parses(next_call);
    assert!(
        !crate::tool_runtime::tool_definition::is_adaptive_runtime_direct_tool(
            next_call["tool"].as_str().unwrap()
        ),
        "read_git_diff_hunks stays an exact/gateway specialist"
    );
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(!actions
        .iter()
        .any(|action| action == "review workspace changes with read_workspace_changes"));
    assert!(actions.iter().any(|action| action
        == "continue the diff review with read_git_diff_hunks; use paths to narrow scope when useful"));
    assert!(actions.iter().any(|action| action
        == "follow read_git_diff_hunks recovery.later_hunks.next_call while has_more=true"));
    assert!(!actions.iter().any(|action| action
        .as_str()
        .is_some_and(|action| action.contains("recovery.current_hunk.next_call"))));
    assert_show_changes_envelope_value_matches_schema(&output, "hunk count handoff");
}

#[test]
fn show_changes_diff_respects_max_hunk_lines() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    // One file with many changed lines -> one big hunk; cap lines at 3.
    let content = (0..20).map(|i| format!("line{i}\n")).collect::<String>();
    commit_file(tmp.path(), "big.txt", &content, "initial");
    std::fs::write(
        tmp.path().join("big.txt"),
        format!("{content}extra\nmore\n"),
    )
    .unwrap();
    let output = bounded_show_changes_output(tmp.path(), true, 20, 3);
    assert_eq!(output["hunk_count"].as_u64(), Some(1));
    let files = output["hunks"].as_array().unwrap();
    let hunks = files[0]["hunks"].as_array().unwrap();
    assert!(!hunks.is_empty(), "expected at least one hunk: {files:?}");
    let lines = hunks[0]["diff"].as_str().unwrap().lines().count();
    // header line + up to 3 content lines = at most 4 lines.
    assert!(lines <= 4, "hunk must be line-bounded: {hunks:?}");
    assert!(hunks[0].get("truncated").is_none());
    assert_eq!(
        hunks[0]["source_completeness"], "unknown",
        "producer-side line truncation must leave completeness explicitly unknown"
    );
    assert_eq!(output["hunks_truncated"], true);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(reasons.iter().any(|r| r == "diff_hunk_line_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_hunk_count_limit"));
    assert!(!reasons.iter().any(|r| r == "diff_byte_budget"));
    let next_call = &output["diff_review_handoff"]["next_call"];
    assert_eq!(
        next_call["arguments"]["paths"],
        json!([]),
        "line truncation must not guess a narrower path without per-hunk provenance"
    );
    assert_eq!(next_call["arguments"]["max_hunk_lines"], 400);
    assert_git_diff_hunks_recovery_call_parses(next_call);
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(actions.iter().any(|action| action
        == "follow read_git_diff_hunks recovery.current_hunk.next_call; after the fresh handoff observation it may use bounded refinement or an exact hunk-fragment continuation"));
    assert!(!actions.iter().any(|action| action
        == "follow read_git_diff_hunks recovery.later_hunks.next_call while has_more=true"));
    assert_show_changes_envelope_value_matches_schema(&output, "hunk line handoff");
}

#[test]
fn show_changes_combined_hunk_count_and_line_truncation_keeps_both_guidance_paths() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let original = (0..20).map(|i| format!("line-{i}\n")).collect::<String>();
    for name in ["a.txt", "b.txt"] {
        commit_file(tmp.path(), name, &original, "initial");
        let changed = (0..20)
            .map(|i| format!("changed-{name}-{i}\n"))
            .collect::<String>();
        std::fs::write(tmp.path().join(name), changed).unwrap();
    }

    let output = bounded_show_changes_output(tmp.path(), true, 1, 3);
    assert_eq!(output["hunks_truncated"], true);
    let reasons = output["truncation_reasons"].as_array().unwrap();
    assert!(reasons
        .iter()
        .any(|reason| reason == "diff_hunk_count_limit"));
    assert!(reasons
        .iter()
        .any(|reason| reason == "diff_hunk_line_limit"));
    assert_eq!(
        output["hunks"][0]["hunks"][0]["source_completeness"], "unknown",
        "mixed page/line truncation leaves per-hunk source completeness unknown"
    );
    let next_call = &output["diff_review_handoff"]["next_call"];
    assert_eq!(
        next_call["arguments"]["paths"],
        json!([]),
        "combined page truncation must not narrow away later files"
    );
    assert_git_diff_hunks_recovery_call_parses(next_call);
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(actions.iter().any(|action| action
        == "follow read_git_diff_hunks recovery.later_hunks.next_call while has_more=true"));
    assert!(actions.iter().any(|action| action
        .as_str()
        .is_some_and(|action| action.contains("recovery.current_hunk.next_call"))));
    assert_show_changes_envelope_value_matches_schema(&output, "combined diff handoff");
}

#[tokio::test]
async fn show_changes_untracked_preview_truncation_does_not_create_diff_handoff() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    commit_file(tmp.path(), "README.md", "hello\n", "initial");
    for i in 0..6 {
        std::fs::write(
            tmp.path().join(format!("untracked-{i}.txt")),
            format!("u{i}\n"),
        )
        .unwrap();
    }

    let runtime = test_runtime();
    let client_id = "show-untracked-handoff";
    let project = register_runner_project_at_path(&runtime, client_id, "repo", tmp.path()).await;
    let result = run_show_changes_via_agent(&runtime, client_id, project, None, true).await;
    assert!(result.success, "{:?}", result.error);
    assert_show_changes_envelope_matches_schema("untracked-only truncation", &result);
    let output = &result.output;
    assert_eq!(output["hunks_truncated"], false);
    assert_eq!(output["untracked_previews_truncated"], true);
    assert!(output.get("diff_review_handoff").is_none());
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(!actions
        .iter()
        .any(|action| action == "review workspace changes with read_workspace_changes"));
    assert!(actions
        .iter()
        .any(|action| action == "inspect the relevant untracked files separately"));
    assert!(!actions.iter().any(|action| action
        .as_str()
        .is_some_and(|action| action.contains("read_git_diff_hunks"))));
}

#[test]
fn show_changes_large_diff_does_not_depend_on_runner_retained_tail() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let big = (0..50).map(|i| format!("line{i}\n")).collect::<String>();
    commit_file(tmp.path(), "big.txt", &big, "initial");
    // Make a diff with many hunks; the command bounds to max_hunks=1, line=10.
    let modified = (0..50)
        .map(|i| {
            if i % 2 == 0 {
                format!("mod{i}\n")
            } else {
                format!("line{i}\n")
            }
        })
        .collect::<String>();
    std::fs::write(tmp.path().join("big.txt"), modified).unwrap();
    let output = bounded_show_changes_output(tmp.path(), true, 1, 10);
    // The first selected hunk must be present and the diff must be bounded.
    assert_eq!(output["hunk_count"].as_u64(), Some(1));
    // The structured fields (not a tail marker) report truncation.
    assert_eq!(output["hunks_truncated"], true);
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains("[output truncated"),
        "must not rely on transport tail marker: {serialized}"
    );
}

#[test]
fn show_changes_schema_covers_truncation_and_transport_fields() {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("read_workspace_changes");
    let properties = schema["properties"]["output"]["properties"]
        .as_object()
        .expect("read_workspace_changes output properties");
    for field in [
        "files_total",
        "files_returned",
        "files_truncated",
        "files_limit",
        "transport_safe",
        "output_budget_bytes",
        "output_truncated",
        "truncation_reasons",
        "diff_review_handoff",
    ] {
        assert!(
            properties.contains_key(field),
            "missing truncation/transport field {field}"
        );
    }
    let handoff = &properties["diff_review_handoff"];
    assert_eq!(handoff["type"], "object");
    assert_eq!(handoff["additionalProperties"], false);
    assert_eq!(handoff["required"], json!(["next_call"]));
    assert_eq!(
        handoff["properties"]["next_call"]["properties"]["tool"]["const"],
        "read_git_diff_hunks"
    );
    assert_eq!(
        handoff["properties"]["next_call"]["properties"]["arguments"]["additionalProperties"],
        false
    );
    assert_eq!(
        handoff["properties"]["next_call"]["properties"]["arguments"]["properties"]["cached"]
            ["const"],
        false
    );
}

#[tokio::test]
async fn show_changes_include_diff_agent_command_does_not_enqueue_python_helper() {
    let runtime = runtime_with_agent_project("show-native");
    let caps = RunnerCapabilities {
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "show-native", None, caps).await;
    let project = agent_test_project_id("show-native");
    let task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project,
                        session_id: None,
                        include_diff: Some(true),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "show-native").await;
    assert_eq!(req.kind, "run_internal_posix_script");
    assert!(req.command.is_empty());
    let payload = req
        .script
        .as_ref()
        .expect("read_workspace_changes must carry a typed internal script");
    assert_eq!(
        payload.language,
        crate::runner_protocol::ShellScriptLanguage::Sh
    );
    assert!(payload.args.is_empty());
    let forbidden = ["python3", "-c"].join(" ");
    assert!(
        !payload.script.contains(&forbidden),
        "read_workspace_changes include_diff must not enqueue a Python helper: {}",
        payload.script
    );
    assert!(payload.script.contains("git diff --unified="));
    let stdout = framed_clean_show_changes_test_stdout("head", true);
    complete_patch_agent_request(&runtime, "show-native", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["untracked_previews"], json!([]));
}

#[test]
fn show_changes_clean_worktree() {
    let output = parse_show_changes_output(
            "agent:oe:webcodex",
            "## main...origin/main",
            "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix: route anchor edit file ops through agent dispatch",
            "",
            None,
            20,
            80,
            Some(0),
            "",
        );
    assert_eq!(output["clean"], true);
    assert_eq!(output["branch"], "main");
    assert_eq!(output["head"]["short"], "b47e4fb");
    assert_eq!(output["counts"]["modified"], 0);
    assert!(output["files"].as_array().unwrap().is_empty());
    assert!(output.get("hunks").is_none());
    assert!(output["session"].is_null());
    assert_eq!(output["suggested_next_actions"][0], "no changes detected");
    assert_review_verdict_shape(&output["verdict"]);
    assert_ne!(output["verdict"]["status"], "fail");
    assert_eq!(output["verdict"]["blocking"], false);
}

#[test]
fn show_changes_without_session_id_treats_dirty_workspace_as_advisory() {
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/lib.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        " src/lib.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    apply_show_changes_session(&mut output, None, None, None);
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["modified"], 1);
    assert_review_verdict_shape(&output["verdict"]);
    assert_eq!(output["verdict"]["status"], "warn");
    assert_eq!(output["verdict"]["blocking"], false);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert!(output["session"].is_null());
    assert!(output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "review diff"));
}

#[test]
fn show_changes_with_session_id_defaults_to_compact_session_summary() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(
        Some("agent:oe:webcodex".to_string()),
        Some("finish task".to_string()),
    );
    let write_args = json!({"project": "agent:oe:webcodex", "path": "src/foo.rs"});
    let write = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "write_project_file",
        &write_args,
        crate::tool_runtime::sessions::session_tool_contract("write_project_file"),
    );
    runtime
        .sessions
        .record_tool_call_finished(write, true, &json!({}), None, None);
    let shell_args = json!({"project": "agent:oe:webcodex", "command": "cargo test"});
    let shell = runtime.sessions.record_tool_call_started(
        Some(&session.session_id),
        crate::tool_runtime::sessions::SessionTransport::Api,
        "run_shell",
        &shell_args,
        crate::tool_runtime::sessions::session_tool_contract("run_shell"),
    );
    runtime
        .sessions
        .record_tool_call_finished(shell, true, &json!({}), None, None);

    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/foo.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        " src/foo.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    let summary = runtime.sessions.summary(&session.session_id, Some(30));
    apply_show_changes_session(&mut output, Some(&session.session_id), summary, None);

    assert_eq!(output["session"]["found"], true);
    assert_eq!(output["session"]["session_id"], session.session_id);
    assert_eq!(output["session"]["title"], "finish task");
    assert_eq!(output["session"]["counts"]["tool_calls"], 2);
    assert_eq!(output["session"]["counts"]["write_like"], 1);
    assert_eq!(output["session"]["counts"]["shell_like"], 1);
    assert_eq!(output["session"]["changed_paths"], json!(["src/foo.rs"]));
    assert_eq!(output["session"]["signals"]["failed"], false);
    assert_eq!(output["session"]["signals"]["write_like"], true);
    assert_eq!(output["session"]["signals"]["shell_like"], true);
    assert!(output["session"].get("recent_events").is_none());
    let actions = output["suggested_next_actions"].as_array().unwrap();
    assert!(actions
        .iter()
        .any(|v| v == "review changed paths from this session"));
    assert!(actions
        .iter()
        .any(|v| v == "check command/test results before commit"));
}

#[test]
fn show_changes_with_missing_session_id_returns_warning_not_panic() {
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    apply_show_changes_session(&mut output, Some("wc_sess_missing"), None, None);
    assert_eq!(output["session"]["found"], false);
    assert_eq!(output["session"]["session_id"], "wc_sess_missing");
    assert!(output["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["kind"] == "session_not_found"));
    assert_eq!(output["suggested_next_actions"][0], "no changes detected");
}

#[test]
fn show_changes_session_changed_paths_are_deduped() {
    let runtime = test_runtime();
    let session = runtime.sessions.start_session(None, None);
    for path in ["src/foo.rs", "src/foo.rs", "src/bar.rs"] {
        let args = json!({"project": "agent:oe:webcodex", "path": path});
        let start = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "write_project_file",
            &args,
            crate::tool_runtime::sessions::session_tool_contract("write_project_file"),
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let mut output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/foo.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        " src/foo.rs | 2 +-",
        None,
        20,
        80,
        Some(0),
        "",
    );
    let summary = runtime.sessions.summary(&session.session_id, Some(30));
    apply_show_changes_session(&mut output, Some(&session.session_id), summary, None);
    assert_eq!(
        output["session"]["changed_paths"],
        json!(["src/foo.rs", "src/bar.rs"])
    );
}

#[tokio::test]
async fn show_changes_explicit_session_event_limit_is_bounded() {
    let runtime = runtime_with_agent_project("show");
    let caps = RunnerCapabilities {
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "show", None, caps).await;
    let session = runtime.sessions.start_session(None, None);
    for idx in 0..250 {
        let args =
            json!({"project": agent_test_project_id("show"), "path": format!("src/{idx}.rs")});
        let start = runtime.sessions.record_tool_call_started(
            Some(&session.session_id),
            crate::tool_runtime::sessions::SessionTransport::Api,
            "write_project_file",
            &args,
            crate::tool_runtime::sessions::session_tool_contract("write_project_file"),
        );
        runtime
            .sessions
            .record_tool_call_finished(start, true, &json!({}), None, None);
    }
    let runtime_for_task = runtime.clone();
    let project = agent_test_project_id("show");
    let session_id = session.session_id.clone();
    let task = tokio::spawn(async move {
        runtime_for_task
            .show_changes(project, Some(session_id), None, None, None, Some(999))
            .await
    });
    let req = wait_for_patch_agent_request(&runtime, "show").await;
    let stdout = framed_clean_show_changes_test_stdout("head", false);
    complete_patch_agent_request(&runtime, "show", &req.request_id, 0, &stdout, "").await;
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    let len = result.output["session"]["recent_events"]
        .as_array()
        .unwrap()
        .len();
    assert_eq!(len, 200);
}

#[test]
fn show_changes_reports_modified_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/users_http.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        " src/users_http.rs | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["modified"], 1);
    assert_eq!(output["counts"]["unstaged"], 1);
    assert_eq!(output["verdict"]["status"], "warn");
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert_eq!(output["files"][0]["path"], "src/users_http.rs");
    assert_eq!(output["files"][0]["status"], "modified");
    assert_eq!(output["files"][0]["kind"], "tracked");
    assert!(output["diff_stat"]
        .as_str()
        .unwrap()
        .contains("1 file changed"));
}

#[test]
fn show_changes_reports_untracked_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n?? webcodex-anchor-edit-smoke-c99f7de.txt",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["untracked"], 1);
    assert_eq!(output["counts"]["conflicted"], 0);
    assert_eq!(output["files"][0]["status"], "untracked");
    assert_eq!(output["files"][0]["staged"], false);
    assert_eq!(output["warnings"][0]["kind"], "untracked_smoke_file");
    assert_eq!(output["verdict"]["status"], "warn");
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
    assert!(output["suggested_next_actions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str().unwrap().contains("untracked")));
}

#[test]
fn show_changes_reports_conflicted_file() {
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\nUU conflicted.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        "",
        None,
        20,
        80,
        Some(0),
        "",
    );
    assert_eq!(output["clean"], false);
    assert_eq!(output["counts"]["conflicted"], 1);
    assert_eq!(output["counts"]["modified"], 0);
    assert_eq!(output["files"][0]["path"], "conflicted.rs");
    assert_eq!(output["files"][0]["status"], "conflicted");
    assert_eq!(output["files"][0]["kind"], "conflicted");
    assert!(
        output["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["kind"] == "workspace_conflicts"),
        "expected workspace_conflicts warning: {}",
        output["warnings"]
    );
    // A real merge conflict is a hard blocker; ordinary dirty state is advisory.
    assert_eq!(output["verdict"]["status"], "fail");
    assert_reason_list_contains(
        &output["verdict"],
        "blocking_reasons",
        "workspace_conflicts",
    );
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "workspace_dirty");
}

#[test]
fn show_changes_include_diff_true_returns_bounded_hunks() {
    let diff = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,3 @@
 line one
-old
+new
 line three
@@ -10,3 +10,3 @@
 alpha
-beta
+gamma
 omega
";
    let output = parse_show_changes_output(
        "agent:oe:webcodex",
        "## main\n M src/lib.rs",
        "commit=b47e4fb000000000000000000000000000000000\nshort=b47e4fb\nsummary=fix",
        " src/lib.rs | 4 ++--",
        Some(diff),
        1,
        4,
        Some(0),
        "",
    );
    assert_eq!(output["hunk_count"], 1);
    assert_eq!(output["hunks_truncated"], true);
    assert_reason_list_contains(&output["verdict"], "warning_reasons", "truncated_by_limit");
    let hunks = output["hunks"].as_array().unwrap();
    assert_eq!(hunks.len(), 1);
    assert_eq!(hunks[0]["path"], "src/lib.rs");
    assert_eq!(hunks[0]["hunks"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn show_changes_clean_repo_include_diff_false_has_no_untracked_previews() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());

    let output = show_changes_output_from_command(tmp.path(), false);

    assert_eq!(output["clean"], true);
    assert_eq!(output["counts"]["untracked"], 0);
    assert!(output.get("untracked_previews").is_none());
}

#[tokio::test]
async fn show_changes_untracked_text_include_diff_false_omits_preview() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    let content = "webcodex untracked preview body";
    fs::write(tmp.path().join("notes.txt"), content).unwrap();

    let output = show_changes_output_from_command(tmp.path(), false);

    assert_eq!(output["counts"]["untracked"], 1);
    assert!(output_has_file(&output, "notes.txt"));
    assert!(output.get("untracked_previews").is_none());
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(
        !serialized.contains(content),
        "include_diff=false leaked untracked file content: {serialized}"
    );
}

#[tokio::test]
async fn show_changes_untracked_text_include_diff_true_returns_bounded_preview() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("notes.txt"), "alpha\nbeta\n").unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    assert!(output_has_file(&output, "notes.txt"));
    let preview = preview_for_path(&output, "notes.txt");
    assert_eq!(preview["kind"], "text");
    assert_eq!(preview["line_count"], 2);
    assert_eq!(preview["truncated"], false);
    assert_eq!(preview["lines"][0]["line"], 1);
    assert_eq!(preview["lines"][0]["text"], "alpha");
    assert_eq!(preview["lines"][1]["line"], 2);
    assert_eq!(preview["lines"][1]["text"], "beta");
    assert_eq!(output["hunk_count"], 0);
}

#[tokio::test]
async fn show_changes_untracked_large_file_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("large.txt"), vec![b'x'; 8193]).unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    let preview = preview_for_path(&output, "large.txt");
    assert_eq!(preview["kind"], "skipped");
    assert_eq!(preview["reason"], "too_large");
    assert_eq!(preview["byte_count"], 8193);
}

#[tokio::test]
async fn show_changes_untracked_binary_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("binary.bin"), [0, 159, 146, 150]).unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 1);
    let preview = preview_for_path(&output, "binary.bin");
    assert_eq!(preview["kind"], "skipped");
    assert_eq!(preview["reason"], "binary_or_non_utf8");
}

#[tokio::test]
async fn show_changes_untracked_sensitive_path_preview_is_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(tmp.path().join("runner.toml"), "RUNNER_TOKEN=secret\n").unwrap();
    fs::write(tmp.path().join("agent.toml"), "API_TOKEN=secret\n").unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);

    assert_eq!(output["counts"]["untracked"], 2);
    for path in ["runner.toml", "agent.toml"] {
        let preview = preview_for_path(&output, path);
        assert_eq!(preview["kind"], "skipped", "{path}");
        assert_eq!(preview["reason"], "sensitive_or_excluded_path", "{path}");
    }
    let serialized = serde_json::to_string(&output).unwrap();
    assert!(!serialized.contains("RUNNER_TOKEN=secret"));
    assert!(
        !serialized.contains("API_TOKEN=secret"),
        "sensitive file content leaked: {serialized}"
    );
    assert_verdict_omits_raw_output_and_sensitive_values(
        &output["verdict"],
        &["RUNNER_TOKEN=secret", "API_TOKEN=secret"],
        "read_workspace_changes sensitive preview verdict",
    );
}

#[tokio::test]
async fn show_changes_untracked_public_dotenv_template_preview_is_visible() {
    let tmp = tempfile::tempdir().unwrap();
    init_git_repo(tmp.path());
    fs::write(
        tmp.path().join(".env.example"),
        "PUBLIC_EXAMPLE_MARKER=fake\n",
    )
    .unwrap();

    let output = show_changes_output_from_command(tmp.path(), true);
    let preview = preview_for_path(&output, ".env.example");
    assert_eq!(preview["kind"], "text");
    assert_eq!(preview["lines"][0]["text"], "PUBLIC_EXAMPLE_MARKER=fake");
}

#[test]
fn git_diff_hunks_command_is_read_only_and_scoped_to_paths() {
    let command = git_diff_hunks_command(&["src/lib.rs".to_string()], false).unwrap();
    assert!(command.contains("git diff"));
    assert!(command.contains("--no-ext-diff"));
    assert!(command.contains("--no-textconv"));
    assert!(command.contains("--unified=80 -- 'src/lib.rs'"));
}

#[tokio::test]
async fn git_diff_hunks_rejects_unsafe_paths_before_project_dispatch() {
    let runtime = test_runtime();
    let result = runtime
        .git_diff_hunks(
            "agent:oe:webcodex".to_string(),
            Some(vec!["../outside".to_string()]),
            None,
            None,
            None,
        )
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("parent traversal"));
}

#[tokio::test]
async fn show_changes_with_session_id_returns_session_block_and_records_call() {
    let runtime = runtime_with_agent_project("telemetry-show");
    let caps = RunnerCapabilities {
        file_read: true,
        shell: true,
        internal_posix_script: true,
        ..Default::default()
    };
    register_agent(&runtime, "telemetry-show", None, caps).await;
    let project = agent_test_project_id("telemetry-show");
    let session = runtime.sessions.start_session(Some(project.clone()), None);

    let read_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ReadFiles {
                        project,
                        items: vec![crate::tool_runtime::ReadFilesItem {
                            path: "README.md".to_string(),
                            start_line: None,
                            limit: Some(1),
                            expected_read_revision: None,
                        }],
                        session_id: Some(session_id),
                        with_line_numbers: None,
                        max_result_bytes: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_runner_request_for_instance(&runtime, "telemetry-show", "inst").await;
    complete_patch_agent_request(
        &runtime,
        "telemetry-show",
        &req.request_id,
        0,
        &canonical_agent_file_read_range("hello\n", 1, 1),
        "",
    )
    .await;
    let read = read_task.await.unwrap();
    assert!(read.success, "{:?}", read.error);

    let show_task = tokio::spawn({
        let runtime = runtime.clone();
        let project = project.clone();
        let session_id = session.session_id.clone();
        async move {
            let bootstrap = auth_context(None, true);
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project,
                        session_id: Some(session_id),
                        include_diff: Some(false),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: Some(20),
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_patch_agent_request(&runtime, "telemetry-show").await;
    let stdout = format!(
        "{}{}{}{}",
        framed_block(
            'S',
            "## main\n M README.md\n",
            "status_exit=0\nrepository_probe=inside_worktree\nrepository_probe_exit=0\nfiles_total=1\nfiles_returned=1\nfiles_truncated=0\nfiles_limit=200\nmodified=1\nadded=0\ndeleted=0\nrenamed=0\ncopied=0\nuntracked=0\nconflicted=0\nstaged=0\nunstaged=1\nstatus_trunc_count=0\nstatus_trunc_bytes=0\nstatus_trunc_path=0\nstatus_bytes=20\n"
        ),
        framed_block(
            'H',
            "commit=abc123\nshort=abc123\nsummary=head\n",
            "head_exit=0\nhead_truncated=0\nhead_bytes=39\n"
        ),
        framed_block(
            'T',
            "README.md | 1 +\n",
            "diff_stat_exit=0\ndiff_stat_truncated=0\ndiff_stat_bytes=15\n"
        ),
        framed_block(
            'N',
            "1\t0\tREADME.md\n",
            "numstat_exit=0\nnumstat_truncated=0\nnumstat_bytes=13\n"
        )
    );
    complete_patch_agent_request(&runtime, "telemetry-show", &req.request_id, 0, &stdout, "").await;
    let result = show_task.await.unwrap();

    assert!(result.success, "{:?}", result.error);
    assert!(result.output.get("session_recorded").is_none());
    assert!(result.output.get("session_event_id").is_none());
    assert!(result.output.get("session_id").is_none());
    assert_eq!(result.output["session"]["found"], true);
    assert_eq!(result.output["session"]["counts"]["tool_calls"], 1);
    assert!(result.output["session"]["recent_events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["tool_name"] == "read_files"));
    let summary = runtime
        .sessions
        .summary(&session.session_id, Some(20))
        .unwrap();
    assert_eq!(summary.counts.tool_calls, 2);
    assert_eq!(summary.counts.change_summary_like, 1);
    let event = finished_event(&summary, "read_workspace_changes");
    assert!(event.git_like);
    assert!(event.change_summary_like);
}

#[tokio::test]
async fn show_changes_accepts_unique_short_id() {
    let runtime = runtime_with_resolver_projects().await;
    let bootstrap = auth_context(None, true);
    let task = tokio::spawn({
        let runtime = runtime.clone();
        async move {
            runtime
                .dispatch_with_auth(
                    ToolCall::ShowChanges {
                        project: "other-repo".to_string(),
                        session_id: None,
                        include_diff: Some(false),
                        max_hunks: None,
                        max_hunk_lines: None,
                        session_event_limit: None,
                    },
                    Some(&bootstrap),
                )
                .await
        }
    });
    let req = wait_for_runner_request_for_client(&runtime, "workstation").await;
    assert_eq!(req.cwd.as_deref(), Some("/root/git/workstation-other-repo"));
    let stdout = framed_clean_show_changes_test_stdout("head", false);
    runtime
        .runner_registry
        .complete(RunnerResultRequest {
            client_id: "workstation".to_string(),
            runner_instance_id: "inst-workstation".to_string(),
            request_id: req.request_id,
            exit_code: Some(0),
            stdout: Some(stdout),
            stderr: Some(String::new()),
            stdout_truncated: false,
            stderr_truncated: false,
            duration_ms: Some(1),
            error: None,
        })
        .await
        .unwrap();
    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["project"], "other-repo");
}
