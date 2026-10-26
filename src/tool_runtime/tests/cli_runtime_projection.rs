//! Runtime-produced CLI regression samples. No final ToolResult is hand-built.
//!
//! One local Runner fixture serves each related group. Files/search/commands run
//! against a temporary checkout through the existing bounded local responder.
//! Job updates carry the actual command's captured output. Only lost-transport
//! injection is synthetic; no successful file-edit Runner receipt is fabricated.
use super::super::{helpers, ToolCall, ToolRuntime};
use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerRequest, RunnerResultPayload,
    RunnerResultRequest, ShellCommandExecutionState,
};
use regex::Regex;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

struct Fixture {
    root: tempfile::TempDir,
    runtime: ToolRuntime,
    client: &'static str,
    project: String,
}

impl Fixture {
    async fn new(client: &'static str, jobs: bool) -> Self {
        let rg = std::process::Command::new("rg")
            .arg("--version")
            .stdin(std::process::Stdio::null())
            .output()
            .expect("CLI Runtime snapshots require ripgrep (rg) on PATH; install it before running this fixture");
        assert!(
            rg.status.success(),
            "CLI Runtime snapshots require a working rg --version: {}",
            String::from_utf8_lossy(&rg.stderr)
        );
        let root = tempfile::tempdir().unwrap();
        init_git_repo(root.path());
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::create_dir(root.path().join("apps")).unwrap();
        commit_file(
            root.path(),
            "src/sample.txt",
            "before\nneedle 中文\nbetween\nneedle again\nafter\n",
            "CLI fixture",
        );
        commit_file(root.path(), "apps/main.txt", "hello\n", "second file");
        let runtime = ToolRuntime::new_for_tests();
        let project = register_runner_project_at_path_with_capabilities(
            &runtime,
            client,
            "demo",
            root.path(),
            RunnerCapabilities {
                shell: true,
                explicit_shell_selection: true,
                structured_process_argv: true,
                git: true,
                file_read: true,
                file_write: true,
                internal_posix_script: true,
                async_jobs: jobs,
                async_shell_jobs: jobs,
                ..Default::default()
            },
        )
        .await;
        Self {
            root,
            runtime,
            client,
            project,
        }
    }

    fn start(&self, tool: &'static str, arguments: Value) -> tokio::task::JoinHandle<Value> {
        let runtime = self.runtime.clone();
        tokio::spawn(async move {
            let (model, rendered) = crate::mcp::cli_call_with_model_for_test(
                &runtime,
                tool,
                arguments,
                &auth_context(None, true),
            )
            .await;
            assert_model_evidence_survives_cli(tool, &model, &rendered);
            rendered
        })
    }

    async fn call(&self, tool: &'static str, arguments: Value) -> Value {
        let task = self.start(tool, arguments);
        let deadline = Instant::now() + Duration::from_secs(15);
        while !task.is_finished() {
            assert!(
                Instant::now() < deadline,
                "{tool}: local Runner fixture deadline"
            );
            if let Some(request) = probe_patch_agent_request(&self.runtime, self.client).await {
                assert!(
                    !matches!(request.kind.as_str(), "run_script" | "start_script_job"),
                    "native script execution belongs in the real Runner smoke fixture"
                );
                if request.kind == "run_process" {
                    // The shared helper executes the actual native argv and
                    // rejects its own timeout. A nonnegative exit status proves
                    // a completed child; spawn/capture failures stay fixture errors.
                    let started = Instant::now();
                    let (exit_code, stdout, stderr) = run_runner_shell_request_locally(&request);
                    assert!(
                        exit_code >= 0,
                        "native process fixture did not obtain an exit status: {stderr}"
                    );
                    complete_patch_agent_execution(
                        &self.runtime,
                        self.client,
                        request.request_id,
                        ShellCommandExecutionState::Completed,
                        Some(exit_code),
                        &stdout,
                        &stderr,
                        None,
                        started.elapsed().as_millis() as u64,
                    )
                    .await;
                    continue;
                }
                // V2 registration supplies the structured-Job baseline: a
                // Process budget above its sync grace arrives as start_process_job.
                if matches!(request.kind.as_str(), "start_job" | "start_process_job") {
                    update_job(self, &request, "running", false, None).await;
                    let completion = run_runner_shell_request_locally(&request);
                    assert!(
                        completion.0 >= 0,
                        "Job fixture did not obtain an actual child exit status: {}",
                        completion.2
                    );
                    let status = if completion.0 == 0 {
                        "completed"
                    } else {
                        "failed"
                    };
                    update_job(self, &request, status, true, Some(completion)).await;
                    continue;
                }
                assert_ne!(
                    request.kind, "file_apply_text_edits",
                    "do not fabricate a successful native Runner edit receipt"
                );
                complete_agent_request_by_running_locally(&self.runtime, self.client, request)
                    .await;
            } else {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }
        task.await.unwrap()
    }
}

fn cli_text(body: &Value) -> &str {
    assert!(body["result"].get("structuredContent").is_none(), "{body}");
    body["result"]["content"][0]["text"]
        .as_str()
        .expect("CLI text")
}

fn recovery_calls(body: &Value) -> Vec<Value> {
    cli_text(body)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter(|value| value["tool"].is_string() && value["arguments"].is_object())
        .collect()
}

/// Compare the very same Runtime response across the adapter boundary. Text
/// layout may change; identities, ranges, uncertainty and callable recovery may not.
fn assert_model_evidence_survives_cli(tool: &str, model: &Value, body: &Value) {
    if !matches!(
        tool,
        "run_job"
            | "observe_jobs"
            | "run_process"
            | "run_shell"
            | "run_script"
            | "read_files"
            | "edit_project_files"
    ) {
        return;
    }
    assert_eq!(
        body["result"]["isError"].as_bool(),
        model["success"].as_bool().map(|success| !success)
    );
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        model,
        &crate::mcp::cli_model_output_schema_for_test(tool),
    )
    .unwrap_or_else(|error| {
        panic!("{tool}: projected output violates its Schema: {error:?}\n{model}")
    });
    let rendered = cli_text(body);
    if serde_json::from_str::<Value>(rendered).ok().as_ref() == Some(model) {
        return; // The bounded formatting fallback retains the entire model result.
    }
    let calls = recovery_calls(body);
    fn check(value: &Value, rendered: &str, calls: &[Value]) {
        if value["tool"].is_string() && value["arguments"].is_object() {
            assert!(
                calls.contains(value),
                "recovery changed: {value}\n{rendered}"
            );
            assert_call_parses(value);
            return;
        }
        match value {
            Value::Object(fields) => {
                for (key, value) in fields {
                    match key.as_str() {
                        "job_id"
                        | "observation_token"
                        | "execution_state"
                        | "command_execution_state"
                        | "status"
                        | "failure_kind"
                        | "error_kind"
                        | "recovery_reason_code"
                        | "stdout_tail"
                        | "stderr_tail"
                        | "text" => {
                            if let Some(evidence) = value.as_str().filter(|value| !value.is_empty())
                            {
                                assert!(
                                    rendered.contains(evidence),
                                    "{key}: {evidence}\n{rendered}"
                                );
                            }
                        }
                        "read_revision" | "expected_read_revision" => {
                            if let Some(revision) = value.as_u64() {
                                assert!(
                                    rendered.contains(&format!("{key} {revision}"))
                                        || rendered.contains(&format!("{key}: {revision}")),
                                    "{key}: {revision}\n{rendered}"
                                );
                            }
                        }
                        "terminal"
                        | "command_started"
                        | "command_completed"
                        | "promoted_to_job"
                        | "tool_failure"
                        | "direct_retry_safe"
                        | "has_more"
                        | "limit"
                        | "returned_lines"
                        | "stdout_truncated"
                        | "stderr_truncated"
                        | "stdout_delta_reset"
                        | "stderr_delta_reset"
                        | "earlier_stdout_unavailable"
                        | "earlier_stderr_unavailable" => {
                            if !value.is_object() && !value.is_array() {
                                assert!(
                                    rendered.contains(&format!("{key}: {value}")),
                                    "{key}: {value}\n{rendered}"
                                );
                            }
                        }
                        "output_truncated" if value == true => {
                            assert!(rendered.contains("output_truncated: true"), "{rendered}");
                        }
                        _ => {}
                    }
                    check(value, rendered, calls);
                }
            }
            Value::Array(items) => {
                for item in items {
                    check(item, rendered, calls);
                }
            }
            _ => {}
        }
    }
    check(&model["output"], rendered, &calls);
}

fn assert_call_parses(call: &Value) {
    let name = call["tool"].as_str().unwrap();
    let arguments = call["arguments"].clone();
    if name == "call_runtime_tool" {
        ToolCall::from_tool_name(
            arguments["tool"].as_str().unwrap(),
            arguments["arguments"].clone(),
        )
    } else {
        ToolCall::from_tool_name(name, arguments)
    }
    .unwrap_or_else(|error| panic!("uncallable recovery {call}: {error}"));
}

/// Normalize only measured fixture identity/clock fields. Preserve every
/// timeout, line/cursor, exit code, failure kind and complete recovery argument.
/// Raw recovery calls are parsed and followed/asserted before normalization.
fn normalized_snapshot(body: &Value, root: &Path, job: Option<&str>) -> String {
    let mut rendered = cli_text(body).to_owned();
    // A macOS canonical root may contain the noncanonical root as a suffix.
    // Replace only these two known fixture paths, longest first.
    let mut roots = [root.to_path_buf(), root.canonicalize().unwrap()];
    roots.sort_by_key(|path| std::cmp::Reverse(path.to_str().unwrap().len()));
    for path in roots {
        rendered = rendered.replace(path.to_str().unwrap(), "<ROOT>");
    }
    if let Some(job) = job {
        rendered = rendered.replace(job, "wc_job_FIXTURE");
    }
    let revisions =
        Regex::new(r#"(\b(?:expected_read_revision|read_revision)(?:[ :]+|":))([0-9]+)"#).unwrap();
    let mut identities = BTreeMap::<String, usize>::new();
    rendered = revisions
        .replace_all(&rendered, |captures: &regex::Captures<'_>| {
            let next = 101 + identities.len();
            let stable = *identities.entry(captures[2].to_owned()).or_insert(next);
            format!("{}{stable}", &captures[1])
        })
        .into_owned();
    // The nonce changes across runtimes; the version and all cursor components
    // remain visible, and the same token remains identical everywhere.
    let token_nonce = Regex::new(r"\bwj3_[A-Za-z0-9_-]+\.").unwrap();
    rendered = token_nonce
        .replace_all(&rendered, "wj3_EPOCH.")
        .into_owned();
    let clocks = Regex::new(
        r"(?m)^(\s*(?:created_at|started_at|ended_at|duration_ms|elapsed_secs|waited_ms):) [0-9]+$",
    )
    .unwrap();
    rendered = clocks.replace_all(&rendered, "$1 <CLOCK>").into_owned();
    format!("isError: {}\n{rendered}", body["result"]["isError"])
}

fn snapshot(name: &str, body: &Value, root: &Path, job: Option<&str>) {
    let raw = cli_text(body);
    let normalized = normalized_snapshot(body, root, job);
    eprintln!(
        "runtime_cli {name}: text_utf8_bytes={} text_chars={} response_json_bytes={} normalized_text_utf8_bytes={}",
        raw.len(),
        raw.chars().count(),
        serde_json::to_vec(body).unwrap().len(),
        normalized.len()
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/tool_runtime/tests/cli_runtime_snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("WEBCODEX_UPDATE_CLI_SNAPSHOTS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &normalized).unwrap();
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("snapshot {}: {error}", path.display()));
    assert_eq!(normalized, expected, "{name}");
}

#[tokio::test]
async fn cli_runtime_files_search_and_execution_snapshots() {
    let f = Fixture::new("cli-runtime-files", false).await;
    let projects = f
        .call(
            "list_projects",
            json!({"client_id":f.client,"summary_only":true}),
        )
        .await;
    assert_eq!(projects["result"]["isError"], false, "{projects}");
    assert!(cli_text(&projects).contains(&f.project), "{projects}");
    assert!(
        !cli_text(&projects).contains("git_available:"),
        "{projects}"
    );
    assert!(cli_text(&projects).contains("count: 1"), "{projects}");
    assert!(
        cli_text(&projects).contains("next_offset: null"),
        "{projects}"
    );
    // Maintenance details stay explicitly available from the same inventory.
    let details = f
        .call(
            "list_projects",
            json!({"project":f.project,"summary_only":false}),
        )
        .await;
    assert!(cli_text(&details).contains("git_available:"), "{details}");
    snapshot("projects", &projects, f.root.path(), None);
    let mut directory_texts = Vec::new();
    for (name, tool, extra) in [
        (
            "files",
            "list_project_files",
            json!({"path":".","limit":20}),
        ),
        (
            "tracked_folded",
            "list_project_tracked_files",
            json!({"depth":1,"limit":20}),
        ),
        (
            "tracked_flat",
            "list_project_tracked_files",
            json!({"limit":20}),
        ),
    ] {
        let mut arguments = extra;
        arguments["project"] = json!(f.project);
        let body = f.call(tool, arguments).await;
        assert_eq!(body["result"]["isError"], false, "{body}");
        snapshot(name, &body, f.root.path(), None);
        if name != "tracked_flat" {
            directory_texts.push(cli_text(&body).to_owned());
        }
    }
    let search = f
        .call(
            "search_project_texts",
            json!({
                "project":f.project,
                "queries":[{"pattern":"needle","pattern_mode":"literal","path":"src",
                    "context_before":1,"context_after":1,"limit":5}]
            }),
        )
        .await;
    assert_eq!(search["result"]["isError"], false, "{search}");
    for evidence in [
        "src/sample.txt:2:needle 中文",
        "before",
        "between",
        "needle again",
        "after",
    ] {
        assert!(cli_text(&search).contains(evidence), "{evidence}: {search}");
    }
    snapshot("search_context", &search, f.root.path(), None);
    let compound = f
        .call(
            "search_file_context",
            json!({
                "project":f.project,
                "queries":[{"pattern":"needle","pattern_mode":"literal","path":"src","limit":5}],
                "read_before":1,"read_after":1,"max_reads":2,"with_line_numbers":true
            }),
        )
        .await;
    assert_eq!(compound["result"]["isError"], false, "{compound}");
    for evidence in ["src/sample.txt", "needle 中文", "read_revision"] {
        assert!(cli_text(&compound).contains(evidence), "{compound}");
    }
    snapshot("search_file_context", &compound, f.root.path(), None);

    for (name, command, is_error, evidence) in [
        ("shell_success", "printf 'hello\\n'", false, "hello"),
        (
            "shell_failure",
            "printf 'captured\\n'; printf 'denied\\n' >&2; exit 7",
            true,
            "denied",
        ),
    ] {
        let body = f
            .call(
                "run_shell",
                json!({
                    "project":f.project,"command":command,"timeout_secs":30
                }),
            )
            .await;
        assert_eq!(body["result"]["isError"], is_error, "{body}");
        assert!(cli_text(&body).contains(evidence), "{body}");
        if is_error {
            assert!(cli_text(&body).contains("7"), "{body}");
        }
        snapshot(name, &body, f.root.path(), None);
    }
    std::fs::write(
        f.root.path().join("src/fixture_failure.py"),
        "import os, sys\nprint(os.getcwd(), flush=True)\n\
         print('denied', file=sys.stderr, flush=True)\nraise SystemExit(7)\n",
    )
    .unwrap();
    let mut contexts = Vec::new();
    for (name, tool, payload, is_error) in [
        (
            "shell_nondefault_cwd",
            "run_shell",
            json!({"command":"pwd","cwd":"src"}),
            false,
        ),
        (
            "process_success",
            "run_process",
            json!({"executable":"printf","args":["hello\n"]}),
            false,
        ),
        (
            "process_nondefault_cwd",
            "run_process",
            json!({"executable":"pwd","cwd":"src"}),
            false,
        ),
        (
            "process_failure_nondefault_cwd",
            "run_process",
            json!({"executable":"python3","args":["fixture_failure.py"],"cwd":"src"}),
            true,
        ),
    ] {
        let mut arguments = payload;
        arguments["project"] = json!(f.project);
        arguments["timeout_secs"] = json!(30);
        let body = f.call(tool, arguments).await;
        assert_eq!(body["result"]["isError"], is_error, "{body}");
        snapshot(name, &body, f.root.path(), None);
        contexts.push((name, body));
    }
    // Collect every planned observation before reporting presentation defects.
    // A mismatched execution state or actual cwd is a separate harness/runtime
    // failure and must not be counted as a compact-rendering improvement.
    let mut presentation_regressions = Vec::new();
    if !cli_text(&search).contains(concat!(
        "src/sample.txt-1-before\n",
        "src/sample.txt:2:needle 中文\n",
        "src/sample.txt-3-between\n",
        "--\n",
        "src/sample.txt-3-between\n",
        "src/sample.txt:4:needle again\n",
        "src/sample.txt-5-after\n"
    )) {
        presentation_regressions
            .push("search context lacks ordered compact match groups".to_owned());
    }
    for (name, body) in contexts {
        let rendered = cli_text(&body);
        if name == "process_success" {
            if rendered != "hello\n" {
                presentation_regressions
                    .push(format!("{name}: default success retains redundant fields"));
            }
            continue;
        }
        if name == "process_failure_nondefault_cwd" {
            assert!(
                rendered.contains("exit_code: 7") && rendered.contains("denied"),
                "{name}: command did not demonstrate actual exit/stderr: {rendered}"
            );
        }
        assert!(
            [
                f.root.path().to_path_buf(),
                f.root.path().canonicalize().unwrap()
            ]
            .iter()
            .any(|root| rendered.contains(root.join("src").to_str().unwrap())),
            "{name}: command did not demonstrate actual cwd: {rendered}"
        );
        if !rendered.contains("cwd: src") {
            presentation_regressions.push(format!("{name}: explicit cwd missing from receipt"));
        }
    }
    assert!(
        presentation_regressions.is_empty(),
        "CLI presentation regressions after all snapshots:\n{}",
        presentation_regressions.join("\n")
    );
    for rendered in directory_texts {
        assert!(
            rendered.contains("apps/") && rendered.contains("src/"),
            "{rendered}"
        );
        assert!(
            !rendered.contains("apps//") && !rendered.contains("src//"),
            "{rendered}"
        );
    }
}

#[tokio::test]
async fn cli_runtime_read_fences_and_truncation_snapshots() {
    let f = Fixture::new("cli-runtime-reads", false).await;
    let read = f
        .call(
            "read_files",
            json!({
                "project":f.project,"items":[{"path":"src/sample.txt","start_line":2,"limit":2}],
                "with_line_numbers":true
            }),
        )
        .await;
    assert_eq!(read["result"]["isError"], false, "{read}");
    let revision = Regex::new(r"read_revision ([0-9]+)")
        .unwrap()
        .captures(cli_text(&read))
        .unwrap()[1]
        .parse::<u64>()
        .unwrap();
    assert!(cli_text(&read).contains("2 | needle 中文"));
    snapshot("read_versioned", &read, f.root.path(), None);

    // A genuine read fence followed by an unknown revision exercises the real
    // edit preflight/recovery. Successful native edits need the Runner E2E lane.
    let edit = f.call("edit_project_files", json!({
        "project":f.project,"dry_run":true,
        "changes":[{"kind":"edit","path":"src/sample.txt",
            "expected_read_revision":revision + 10_000,
            "edits":[{"kind":"replace_exact","old_text":"needle 中文","new_text":"changed"}]}]
    })).await;
    assert_eq!(edit["result"]["isError"], true, "{edit}");
    assert!(cli_text(&edit).contains("unknown_read_revision"), "{edit}");
    assert!(cli_text(&edit).contains("not_started"), "{edit}");
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("src/sample.txt")).unwrap(),
        "before\nneedle 中文\nbetween\nneedle again\nafter\n"
    );
    let recovery = recovery_calls(&edit);
    assert_eq!(recovery.len(), 1, "{edit}");
    assert_call_parses(&recovery[0]);
    assert_eq!(recovery[0]["tool"], "read_files");
    assert_eq!(recovery[0]["arguments"]["project"], f.project);
    assert_eq!(
        recovery[0]["arguments"]["items"][0]["path"],
        "src/sample.txt"
    );
    snapshot("edit_unknown_revision", &edit, f.root.path(), None);

    for path in ["large-a.txt", "large-b.txt"] {
        std::fs::write(f.root.path().join(path), format!("{}\n", "x".repeat(3_000))).unwrap();
    }
    let truncated = f
        .call(
            "read_files",
            json!({
                "project":f.project,
                "items":[{"path":"large-a.txt"},{"path":"large-b.txt"}],
                "max_result_bytes":8192
            }),
        )
        .await;
    assert_eq!(truncated["result"]["isError"], false, "{truncated}");
    assert!(
        cli_text(&truncated).contains("output_truncated: true"),
        "{truncated}"
    );
    let calls = recovery_calls(&truncated);
    assert_eq!(calls.len(), 1, "{truncated}");
    assert_call_parses(&calls[0]);
    assert_eq!(calls[0]["tool"], "read_files");
    assert_eq!(calls[0]["arguments"]["project"], f.project);
    assert!(calls[0]["arguments"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| {
            item["path"]
                .as_str()
                .is_some_and(|p| p == "large-a.txt" || p == "large-b.txt")
        }));
    // Preserve and execute the exact emitted recovery, including every snapshot
    // fence/range/budget, before applying any snapshot normalization.
    let recovered = f.call("read_files", calls[0]["arguments"].clone()).await;
    assert_eq!(recovered["result"]["isError"], false, "{recovered}");
    snapshot("read_truncated", &truncated, f.root.path(), None);
    snapshot("read_recovered", &recovered, f.root.path(), None);
}

async fn update_job(
    f: &Fixture,
    request: &RunnerRequest,
    status: &str,
    finished: bool,
    completion: Option<(i32, String, String)>,
) {
    let (exit_code, stdout_chunk, stderr_chunk) = match completion {
        Some((code, out, err)) => (Some(code), Some(out), Some(err)),
        None => (None, None, None),
    };
    f.runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            request_id: Some(request.request_id.clone()),
            exit_code,
            stdout_chunk,
            stderr_chunk,
            command_execution_state: finished.then_some(ShellCommandExecutionState::Completed),
            ..runner_job_update(
                f.client.to_owned(),
                "inst".to_owned(),
                request.job_id.clone().unwrap(),
                status.to_owned(),
                finished,
            )
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn cli_runtime_job_start_observation_and_partial_failure_snapshots() {
    let f = Fixture::new("cli-runtime-jobs", true).await;
    let (model, started) = crate::mcp::cli_call_with_model_for_test(
        &f.runtime,
        "run_job",
        json!({"project":f.project,"command":"printf 'first\\n中文\\nlast\\n'","timeout_secs":30}),
        &auth_context(None, true),
    )
    .await;
    assert_model_evidence_survives_cli("run_job", &model, &started);
    assert_eq!(started["result"]["isError"], false, "{started}");
    let request = wait_for_patch_agent_request(&f.runtime, f.client).await;
    assert_eq!(request.kind, "start_job");
    let job = request.job_id.as_deref().unwrap();
    assert!(cli_text(&started).contains(job), "{started}");
    let calls = recovery_calls(&started);
    assert_eq!(calls.len(), 1, "{started}");
    assert_call_parses(&calls[0]);
    assert_eq!(calls[0]["tool"], "observe_jobs");
    assert_eq!(calls[0]["arguments"]["items"][0]["job_id"], job);
    snapshot("job_started", &started, f.root.path(), Some(job));

    update_job(&f, &request, "running", false, None).await;
    let baseline = f
        .call("observe_jobs", json!({"items":[{"job_id":job}]}))
        .await;
    assert_eq!(baseline["result"]["isError"], false, "{baseline}");
    assert!(cli_text(&baseline).contains("running"), "{baseline}");
    snapshot("job_running", &baseline, f.root.path(), Some(job));

    let completion = run_runner_shell_request_locally(&request);
    assert_eq!(completion.0, 0);
    update_job(&f, &request, "completed", true, Some(completion)).await;
    let completed = f
        .call(
            "observe_jobs",
            json!({
                "items":[{"job_id":job}],"tail_lines":1
            }),
        )
        .await;
    assert!(cli_text(&completed).contains("last"), "{completed}");
    assert!(
        cli_text(&completed).contains("stdout_truncated: true"),
        "{completed}"
    );
    snapshot(
        "job_completed_truncated",
        &completed,
        f.root.path(),
        Some(job),
    );

    let partial = f
        .call(
            "observe_jobs",
            json!({
                "items":[{"job_id":"missing-job"},{"job_id":job}],"tail_lines":1
            }),
        )
        .await;
    assert_eq!(partial["result"]["isError"], false, "{partial}");
    for evidence in ["missing-job", "unknown_job", job, "completed", "last"] {
        assert!(cli_text(&partial).contains(evidence), "{partial}");
    }
    snapshot("jobs_partial_unknown", &partial, f.root.path(), Some(job));
    assert!(
        !cli_text(&partial).lines().any(|line| line.trim() == "null"),
        "{partial}"
    );
    assert!(probe_patch_agent_request(&f.runtime, f.client)
        .await
        .is_none());
}

#[tokio::test]
async fn cli_runtime_timeout_and_lost_completion_snapshots() {
    let f = Fixture::new("cli-runtime-faults", false).await;
    let task = f.start(
        "run_shell",
        json!({
            "project":f.project,"command":"printf 'partial\\n'; sleep 2","timeout_secs":1
        }),
    );
    let request = wait_for_patch_agent_request(&f.runtime, f.client).await;
    let mut command = std::process::Command::new(helpers::test_shell());
    command
        .arg("-c")
        .arg(&request.command)
        .current_dir(f.root.path());
    let (exit_code, stdout, stderr, elapsed, timed_out) =
        helpers::run_test_command_with_timeout(command, None, 1);
    assert!(
        timed_out,
        "real fixture process must reach its execution deadline"
    );
    f.runtime
        .runner_registry
        .complete(RunnerResultPayload {
            result: RunnerResultRequest {
                exit_code: Some(exit_code),
                stdout: Some(stdout),
                stderr: Some(stderr),
                duration_ms: Some(elapsed),
                error: Some("runner timeout".to_owned()),
                ..runner_result_request(f.client.to_owned(), "inst".to_owned(), request.request_id)
            },
            command_execution_state: Some(ShellCommandExecutionState::TimedOut),
            mcp_gateway: None,
            plugin_gateway: None,
            coding_agent: None,
        })
        .await
        .unwrap();
    let timeout = task.await.unwrap();
    assert_eq!(timeout["result"]["isError"], true, "{timeout}");
    assert!(cli_text(&timeout).contains("timed_out"), "{timeout}");
    assert!(
        !cli_text(&timeout).contains("execution_state: outcome_unknown"),
        "{timeout}"
    );
    snapshot("shell_timeout", &timeout, f.root.path(), None);

    let task = f.start(
        "run_shell",
        json!({
            "project":f.project,"command":"printf 'executed\\n' >> effect.txt","timeout_secs":30
        }),
    );
    let request = wait_for_patch_agent_request(&f.runtime, f.client).await;
    let completed = run_runner_shell_request_locally(&request);
    assert_eq!(completed.0, 0);
    // Deliberately lose the completion at the actual transport/registry boundary,
    // after a real effect. Runtime, MCP and CLI must retain uncertainty.
    f.runtime
        .runner_registry
        .reconcile_disconnect(f.client, "inst")
        .await;
    let unknown = task.await.unwrap();
    assert_eq!(unknown["result"]["isError"], true, "{unknown}");
    assert!(cli_text(&unknown).contains("outcome_unknown"), "{unknown}");
    assert!(
        !cli_text(&unknown).contains("No command was started"),
        "{unknown}"
    );
    assert_eq!(
        std::fs::read_to_string(f.root.path().join("effect.txt")).unwrap(),
        "executed\n"
    );
    snapshot("shell_outcome_unknown", &unknown, f.root.path(), None);
}
