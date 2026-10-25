use super::*;

#[test]
#[ignore = "requires captured native Runtime response"]
fn cli_runtime_replay_captured_native_response() {
    use std::io::{Read, Write as _};

    const MAX_CAPTURE_BYTES: u64 = 2 * 1024 * 1024;
    let input = std::env::var_os("WEBCODEX_CLI_RUNTIME_CAPTURE")
        .expect("set WEBCODEX_CLI_RUNTIME_CAPTURE to a native MCP capture");
    let output = std::env::var_os("WEBCODEX_CLI_RUNTIME_REPLAY_OUTPUT")
        .expect("set WEBCODEX_CLI_RUNTIME_REPLAY_OUTPUT to a new output file");
    let file = std::fs::File::open(&input).expect("open native MCP capture");
    assert!(
        file.metadata().unwrap().is_file(),
        "capture must be a regular file"
    );
    let mut raw = Vec::new();
    file.take(MAX_CAPTURE_BYTES + 1)
        .read_to_end(&mut raw)
        .unwrap();
    assert!(
        raw.len() as u64 <= MAX_CAPTURE_BYTES,
        "native MCP capture exceeds 2 MiB"
    );
    let capture: Value = serde_json::from_slice(&raw).expect("native MCP capture JSON");
    assert_eq!(capture["format"], "webcodex.cli_runtime_capture.v1");
    let tool = capture["tool"]
        .as_str()
        .expect("captured canonical tool name");
    assert!(
        matches!(tool, "list_projects" | "list_workspace_symbols"),
        "only inventory and workspace-symbol native captures are admitted"
    );
    let mut response = capture["response"].clone();
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(response.get("id").is_some_and(|id| !id.is_null()));
    assert!(
        response.get("error").is_none(),
        "RPC failures are not ToolResult captures"
    );
    let result = response["result"]
        .as_object()
        .expect("native MCP result object");
    assert!(result.get("content").is_some_and(Value::is_array));
    let structured = result
        .get("structuredContent")
        .and_then(Value::as_object)
        .expect("native Adaptive MCP structuredContent");
    let success = structured
        .get("success")
        .and_then(Value::as_bool)
        .expect("native ToolResult.success");
    assert!(
        structured
            .get("output")
            .is_some_and(|value| value.is_object() || (!success && value.is_null())),
        "expected inventory/symbol output object or failed null output"
    );
    assert!(
        structured
            .get("error")
            .is_some_and(|value| value.is_null() || value.is_string()),
        "expected canonical ToolResult.error"
    );
    if let Some(is_error) = result.get("isError") {
        assert_eq!(
            is_error.as_bool(),
            Some(!success),
            "capture failure semantics disagree"
        );
    }
    crate::mcp::cli::project_response(Some(tool), &mut response);
    assert!(response["result"].get("structuredContent").is_none());
    assert_eq!(response["result"]["isError"], !success);
    let blocks = response["result"]["content"]
        .as_array()
        .expect("CLI content");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0]["type"], "text");
    let rendered = text(&response);
    assert!(
        !rendered.is_empty(),
        "native capture produced no visible CLI evidence"
    );
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .expect("replay output must be a new writable file");
    destination.write_all(rendered.as_bytes()).unwrap();
    eprintln!(
        "runtime_cli_replay {}",
        json!({
            "tool":tool,
            "is_error":!success,
            "text_utf8_bytes":rendered.len(),
            "text_chars":rendered.chars().count(),
            "response_json_bytes":serde_json::to_vec(&response).unwrap().len()
        })
    );
}

fn samples() -> Vec<(&'static str, Value)> {
    vec![
        (
            "find_definition",
            json!({"project":"agent:mini:webcodex","path":"src/mcp.rs","query_position":{"line":694,"column":45},"locations":[{"path":"src/mcp/request_policy.rs","range":{"start":{"line":31,"column":15},"end":{"line":31,"column":28}}}],"total_results":1,"returned_count":1,"truncated":false,"external_results_omitted":0,"invalid_results_omitted":0}),
        ),
        (
            "list_project_files",
            json!({"project":"agent:mini:webcodex","path":"src","entries":(0..17).map(|i|json!({"path":format!("src/module_{i:02}.rs"),"kind":"file"})).collect::<Vec<_>>(),"returned":17,"total_entries":17,"offset":0,"next_offset":null,"truncated":false}),
        ),
        (
            "observe_jobs",
            json!({"items":[{"job_id":"wc_job_exact","status":"completed","terminal":true,"changed":true,"meaningful_changed":true,"heartbeat_changed":false,"log_delta_status":"delta","observation_token":"obs_exact","project":"agent:mini:webcodex","exit_code":0,"command_execution_state":"completed","stdout_tail":"Tests passed.","stderr_tail":""}],"wait":{"outcome":"terminal"}}),
        ),
        (
            "run_shell",
            json!({"project":"agent:mini:webcodex","execution_state":"completed","exit_code":1,"tool_failure":false,"failure_kind":"command_exit_nonzero","command_started":true,"command_completed":true,"terminal":true,"promoted_to_job":false,"job_id":null,"job_status":null,"observation_token":null,"state_changed":true,"stdout_tail":"","stderr_tail":"permission denied","stdout_truncated":false,"stderr_truncated":false,"stdout_lines":0,"stderr_lines":1,"executor":"agent","execution_source":"run_shell","duration_ms":17,"purpose":"diagnostic","cwd":".","shell":"sh"}),
        ),
        (
            "read_workspace_changes",
            json!({"project":"agent:mini:webcodex","branch":"main","clean":true,"upstream_status":"observed","upstream_reason_code":null,"upstream":"origin/main","ahead":0,"behind":0,"head":{"commit":"abc123","summary":"example"},"counts":{"modified":0,"added":0,"deleted":0,"renamed":0,"copied":0,"untracked":0,"conflicted":0,"staged":0,"unstaged":0},"files":[],"diff_stat":"","suggested_next_actions":[],"session":null}),
        ),
        (
            "edit_project_files",
            json!({"project":"agent:mini:webcodex","dry_run":true,"execution_state":"completed","state_changed":false,"changed":false,"would_change":true,"applied_count":0,"planned_count":1,"files":[{"index":0,"kind":"edit","path":"src/main.rs","to_path":null,"changed":false,"would_change":true,"read_revision":7,"edits":[{"index":0,"kind":"replace_exact","old_start_line":1,"old_end_line":1,"new_line_count":1,"would_change":true}]}],"changed_paths":[],"change_summary":{"requested_changes":1,"changed_files":0,"logical_edits":1,"resolved_matches":1,"warnings":0}}),
        ),
        (
            "read_files",
            json!({"project":"agent:mini:webcodex","requested_count":1,"returned_count":1,"succeeded_count":1,"failed_count":0,"output_truncated":false,"items":[{"index":0,"path":"src/main.rs","success":true,"error":null,"output":{"text":"fn main() {}\n","start_line":1,"end_line":1,"returned_lines":1,"total_lines":20,"has_more":true,"read_revision":7}}],"next_index":null,"suggested_call":{"tool":"read_files","arguments":{"project":"agent:mini:webcodex","items":[{"path":"src/main.rs","start_line":2,"expected_read_revision":7}]},"follow_up_kind":"mechanically_followable"}}),
        ),
    ]
}

#[test]
fn cli_text_size_samples() {
    for (tool, output) in samples() {
        let response = project(tool, tool != "run_shell", output);
        let rendered = text(&response);
        assert_snapshot(tool, &response);
        let ceiling = match tool {
            "find_definition" => 60,
            "list_project_files" => 330,
            "observe_jobs" => 300,
            "run_shell" => 350,
            "read_workspace_changes" => 150,
            "edit_project_files" => 250,
            "read_files" => 350,
            _ => unreachable!(),
        };
        assert!(
            rendered.len() <= ceiling,
            "{tool}: {} > {ceiling}",
            rendered.len()
        );
        eprintln!(
            "{tool}: {} bytes, {} chars\n{rendered}",
            rendered.len(),
            rendered.chars().count()
        );
    }
}

#[test]
fn cli_navigation_empty_and_omitted_results_are_distinct() {
    for tool in ["find_definition", "find_references"] {
        let empty = project(
            tool,
            true,
            json!({"locations":[],"total_results":0,"returned_count":0,"truncated":false,"external_results_omitted":0,"invalid_results_omitted":0}),
        );
        assert_snapshot(&format!("{tool}_empty"), &empty);
        assert_eq!(text(&empty), "No locations.\n");
        let incomplete = project(
            tool,
            true,
            json!({"locations":[],"total_results":3,"returned_count":0,"truncated":true,"external_results_omitted":2,"invalid_results_omitted":1,"continuation":{"tool":"find_references","arguments":{"project":"p","path":"x.rs","line":1,"column":1}}}),
        );
        assert_snapshot(&format!("{tool}_truncated"), &incomplete);
        for evidence in [
            "total_results: 3",
            "truncated: true",
            "external_results_omitted: 2",
            "invalid_results_omitted: 1",
        ] {
            assert!(text(&incomplete).contains(evidence));
        }
        assert_eq!(
            project(tool, false, json!({"error_kind":"lsp_request_timeout"}))["result"]["isError"],
            true
        );
    }
}

#[test]
fn cli_job_lifecycles_and_exact_recovery_calls_survive() {
    let call = json!({"tool":"observe_jobs","arguments":{"items":[{"job_id":"wc_job_exact","after_observation_token":"opaque:token/+="}],"tail_lines":17},"follow_up_kind":"mechanically_followable"});
    for status in ["pending", "completed", "failed", "outcome_unknown"] {
        let body = project(
            "observe_jobs",
            true,
            json!({"items":[{"job_id":"wc_job_exact","status":status,"observation_token":"opaque:token/+=","stdout_truncated":true,"stderr_delta_reset":true,"earlier_stdout_unavailable":true,"log_delta_status":"reset","stdout_tail":"证据\nnext","state_changed":null,"continuation":call}],"output_truncated":true,"suggested_call":call}),
        );
        assert_snapshot(&format!("job_{status}"), &body);
        let rendered = text(&body);
        for evidence in [
            status,
            "opaque:token/+=",
            "stdout_truncated: true",
            "stderr_delta_reset: true",
            "earlier_stdout_unavailable: true",
            "log_delta_status: reset",
            "state_changed: null",
            "证据\nnext",
            "output_truncated: true",
        ] {
            assert!(rendered.contains(evidence), "{rendered}");
        }
        let calls: Vec<Value> = rendered
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .collect();
        assert_eq!(calls, vec![call.clone()]);
        assert_eq!(body["result"]["isError"], false); // observation succeeded; Job may have failed
    }
}

#[test]
fn cli_recovery_calls_with_distinct_semantics_are_not_deduplicated() {
    let continuation = json!({"tool":"observe_jobs","arguments":{"items":[{"job_id":"job-a"}]},"follow_up_kind":"fallback_recovery"});
    for suggested in [
        json!({"tool":"observe_jobs","arguments":{"items":[{"job_id":"job-b"}]},"follow_up_kind":"fallback_recovery"}),
        json!({"tool":"observe_jobs","arguments":{"items":[{"job_id":"job-a"}]},"follow_up_kind":"mechanically_followable"}),
    ] {
        let body = project(
            "observe_jobs",
            true,
            json!({"items":[{"job_id":"job-a","continuation":continuation}],"suggested_call":suggested}),
        );
        let calls: Vec<Value> = text(&body)
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
            .collect();
        assert_eq!(calls, vec![continuation.clone(), suggested]);
    }
}

#[test]
fn cli_readiness_partial_and_tracked_listing_keep_exact_evidence() {
    let ready = project(
        "wait_for_job_readiness",
        true,
        json!({"wait_state":"deadline","ready":[{"job_id":"job-a","status":"failed","outcome":"command_exit_nonzero"}],"pending_job_ids":["job-b"]}),
    );
    assert_snapshot("readiness_deadline", &ready);
    for evidence in [
        "wait_state: deadline",
        "job-a",
        "job-b",
        "failed",
        "command_exit_nonzero",
    ] {
        assert!(text(&ready).contains(evidence));
    }
    let listing = project(
        "list_project_tracked_files",
        true,
        json!({"entries":[{"path":"src/","kind":"dir","file_count":5}],"returned":1,"total_entries":1,"total_files":5,"depth":1,"list_truncated":true,"source":"git_index","next_offset":null,"truncated":false}),
    );
    assert_snapshot("tracked_files_truncated", &listing);
    assert!(text(&listing).starts_with("src/ (5 files)\n"));
    for evidence in [
        "list_truncated: true",
        "total_files: 5",
        "depth: 1",
        "source: git_index",
    ] {
        assert!(text(&listing).contains(evidence));
    }
    let partial = project(
        "observe_jobs",
        true,
        json!({"items":[{"index":0,"job_id":"missing","success":false,"output":null,"error_kind":"unknown_job","error":"Job unavailable","recovery_kind":"reobserve"}],"failed_count":1}),
    );
    assert_snapshot("jobs_partial_failure", &partial);
    assert!(!text(&partial).lines().any(|line| line.trim() == "null"));
    for evidence in [
        "missing",
        "success: false",
        "Job unavailable",
        "recovery_kind: reobserve",
        "failed_count: 1",
    ] {
        assert!(text(&partial).contains(evidence), "{}", text(&partial));
    }
}

#[test]
fn cli_edits_preserve_revision_conflicts_and_bulk_warnings() {
    let call = json!({"tool":"read_files","arguments":{"project":"p","items":[{"path":"a.rs"}]},"follow_up_kind":"fallback_recovery"});
    let stale = project(
        "edit_project_files",
        false,
        json!({"execution_state":"not_started","state_changed":false,"error_kind":"stale_read_revision","expected_read_revision":7,"recovery":{"suggested_call":call}}),
    );
    assert_snapshot("edit_stale_revision", &stale);
    for evidence in [
        "ERROR: failed",
        "stale_read_revision",
        "expected_read_revision: 7",
        "state_changed: false",
        "not_started",
        &call.to_string(),
    ] {
        assert!(text(&stale).contains(evidence));
    }
    assert_eq!(stale["result"]["isError"], true);
    let edited = project(
        "edit_project_files",
        true,
        json!({"files":[{"index":0,"kind":"edit","path":"a.rs","changed":true,"read_revision":8,"edits":[{"kind":"replace_exact","index":0,"old_start_line":1,"old_end_line":1,"new_line_count":1,"match_count":2,"match_ranges_truncated":true,"warning":"review repeated anchors"}]}]}),
    );
    assert_snapshot("edit_bulk_warning", &edited);
    for evidence in [
        "read_revision 8",
        "changed",
        "match_count: 2",
        "match_ranges_truncated: true",
        "review repeated anchors",
    ] {
        assert!(text(&edited).contains(evidence));
    }
}

#[test]
fn cli_execution_log_pointer_targets_visible_text() {
    for tool in ["run_shell", "run_process", "run_script"] {
        let mut body = json!({"result":{"structuredContent":{"success":false,"error":"Command failed. Logs: see output.stdout_tail and output.stderr_tail.","output":{"execution_state":"completed","exit_code":1,"failure_kind":"command_exit_nonzero","stdout_tail":"","stderr_tail":"拒绝访问"}},"content":[]}});
        crate::mcp::cli::project_response(Some(tool), &mut body);
        assert_snapshot(&format!("{tool}_failure_logs"), &body);
        assert!(!text(&body).contains("output.stdout_tail"));
        assert!(text(&body).contains("stderr_tail:\n拒绝访问"));
        assert_eq!(body["result"]["isError"], true);
    }
}

#[test]
fn cli_specialized_renderers_keep_unknown_fields_and_malformed_shapes() {
    for tool in [
        "find_definition",
        "find_references",
        "list_project_files",
        "list_project_tracked_files",
        "run_shell",
        "run_process",
        "observe_jobs",
        "wait_for_job_readiness",
        "read_workspace_changes",
        "edit_project_files",
    ] {
        let output = json!({"new_evidence":{"false_is_meaningful":false,"null_is_meaningful":null,"zero_is_meaningful":0},"locations":"invalid","entries":"invalid","files":"invalid"});
        let body = project(tool, true, output);
        for evidence in [
            "false_is_meaningful: false",
            "null_is_meaningful: null",
            "zero_is_meaningful: 0",
            "invalid",
        ] {
            assert!(text(&body).contains(evidence), "{tool}: {}", text(&body));
        }
    }
}

fn assert_snapshot(name: &str, body: &Value) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/mcp_tests/cli/text_snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("WEBCODEX_UPDATE_CLI_SNAPSHOTS").as_deref()
        == Some(std::ffi::OsStr::new("1"))
    {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text(body)).unwrap();
    }
    let expected = std::fs::read_to_string(path).expect("checked-in CLI text snapshot");
    assert_eq!(text(body), expected, "{name}");
}

#[test]
fn cli_ordinary_success_and_empty_snapshots() {
    for (name, tool, output) in [
        (
            "run_process_success",
            "run_process",
            json!({"stdout_tail":"done\n","stderr_tail":"","stdout_truncated":false,"stderr_truncated":false}),
        ),
        (
            "shell_compact_success",
            "run_shell",
            json!({"stdout_tail":"wrote file\n"}),
        ),
        (
            "files_empty",
            "list_project_files",
            json!({"entries":[],"returned":0,"total_entries":0,"offset":0,"next_offset":null,"truncated":false}),
        ),
        (
            "reads_complete",
            "read_files",
            json!({"items":[{"index":0,"path":"a.rs","output":{"text":"fn main() {}\n","read_revision":9,"total_lines":1}}]}),
        ),
        (
            "workspace_incomplete",
            "read_workspace_changes",
            json!({"clean":null,"status_observation":{"status":"unavailable"},"output_truncated":true,"warnings":["status unavailable"],"files":[]}),
        ),
    ] {
        assert_snapshot(name, &project(tool, true, output));
    }
}

#[test]
fn cli_document_symbols_use_identifier_positions_and_preserve_hierarchy_and_loss() {
    let symbol = |name: &str, kind: &str, line, column| {
        json!({
            "name":name, "kind":kind, "kind_code":12,
            "range":{"start":{"line":line,"column":1},"end":{"line":line+3,"column":2}},
            "selection_range":{"start":{"line":line,"column":column},"end":{"line":line,"column":column+3}},
            "children":[]
        })
    };
    let mut parent = symbol("Task", "enum", 10, 6);
    parent["children"] = json!([symbol("Ready", "enum_member", 11, 5)]);
    let output = json!({"project":"p", "path":"src/lib.rs", "language":"rust",
        "symbols":[parent, symbol("resolve_default_project", "function", 31, 15)],
        "total_count":3,"returned_count":3,"truncated":false,
        "invalid_results_omitted":0,"external_results_omitted":0});
    let body = project("list_document_symbols", true, output.clone());
    assert_eq!(text(&body), "src/lib.rs\n10:6 enum Task\n  11:5 enum_member Ready\n31:15 function resolve_default_project\n");
    let mut incomplete = output;
    incomplete["truncated"] = json!(true);
    incomplete["invalid_results_omitted"] = json!(1);
    let body = project("list_document_symbols", true, incomplete);
    assert!(text(&body).contains("truncated: true"));
    assert!(text(&body).contains("invalid_results_omitted: 1"));
}

#[test]
fn cli_single_observation_combines_wait_without_hiding_reset_or_timeout() {
    for outcome in ["updated", "timeout", "terminal"] {
        let body = project(
            "observe_jobs",
            true,
            json!({"items":[{
            "job_id":"job", "status": if outcome == "terminal" {"completed"} else {"running"},
            "terminal":outcome == "terminal", "changed":true,"meaningful_changed":true,
            "heartbeat_changed":false,"log_delta_status":"reset","stdout_delta_reset":true,
            "observation_token":"token"
        }],"wait":{"outcome":outcome,"waited_ms":267,"update_kind":"meaningful"}}),
        );
        let rendered = text(&body);
        assert!(!rendered.contains("wait:"));
        assert!(rendered.contains("log_delta_status: reset"));
        if outcome == "terminal" {
            assert!(!rendered.contains("updates:"));
        } else {
            assert!(rendered.contains("updates: meaningful (waited 267ms)"));
        }
        assert_eq!(
            rendered.contains("wait deadline reached"),
            outcome == "timeout"
        );
    }
}

#[test]
fn cli_compact_inventories_search_and_explicit_reads_keep_useful_evidence() {
    let jobs = project(
        "list_jobs",
        true,
        json!({"jobs":[{"job_id":"j1","status":"running",
        "kind":"shell", "command_summary":"python train.py", "created_at":1,"exit_code":null,
        "recovery_reason_code":"reconciling"}],"count":1}),
    );
    assert!(text(&jobs).contains("j1 | running | python train.py"));
    assert!(text(&jobs).contains("reconciling"));
    assert!(!text(&jobs).contains("null"));
    for files in [
        json!(["a.rs", "b.rs"]),
        json!([{"path":"a.rs","count":2},{"path":"b.rs","count":3}]),
    ] {
        let body = project(
            "search_project_texts",
            true,
            json!({"files":files,"truncated":true}),
        );
        assert!(!text(&body).contains("[0]"));
        assert!(text(&body).contains("a.rs"));
        assert!(text(&body).contains("truncated: true"));
    }
    let read = project(
        "read_files",
        true,
        json!({"items":[{"index":0,"path":"a.rs","success":true,"error":null,
        "output":{"text":"two","start_line":2,"end_line":2,"read_revision":9,"total_lines":20,
        "returned_lines":1,"limit":1,"has_more":true,"next_start_line":3,"truncated":false}}]}),
    );
    assert_eq!(
        text(&read),
        "==> a.rs:2-2 (read_revision 9)\ntwo\ntotal_lines: 20\n"
    );
    let clean = project(
        "read_workspace_changes",
        true,
        json!({"clean":true,"suggested_next_actions":["no action needed"],
        "verdict":{"status":"pass","blocking":false,"blocking_reasons":[],"warning_reasons":[],"suggested_next_actions":["no action needed"]}}),
    );
    assert_eq!(text(&clean), "Workspace clean.\n");
}

#[test]
fn cli_missing_file_uses_one_path_and_one_reason() {
    let body = project(
        "read_files",
        true,
        json!({"project":"p", "requested_count":1,
        "returned_count":1,"succeeded_count":0,"failed_count":1,"output_truncated":false,"next_index":null,
        "items":[{"index":0,"path":"missing.rs","success":false,"error":"read_file failed: not_found",
            "output":{"path":"missing.rs","error_kind":"read_file_failed","reason_code":"not_found","state_changed":false}}]}),
    );
    assert_eq!(text(&body), "missing.rs: read_file failed: not_found\n");
}

#[test]
fn cli_search_context_unknown_fields_and_partial_reads_survive() {
    let body = project(
        "search_project_texts",
        true,
        json!({
            "matches":[{"path":"src/a.rs","line":2,"preview":"hit",
                "context_before":[{"line":1,"text":"before","future_evidence":"keep"}],
                "context_after":[{"line":3,"text":"after"}]}],
            "truncated":true,"truncation_reason":"limit"
        }),
    );
    for evidence in [
        "src/a.rs:2:hit",
        "src/a.rs-3-after",
        "future_evidence: keep",
        "truncated: true",
        "truncation_reason: limit",
    ] {
        assert!(text(&body).contains(evidence), "{}", text(&body));
    }
    let partial = project(
        "search_file_context",
        true,
        json!({
            "search":{"matches":[{"path":"src/a.rs","line":2}]},
            "reads":{"items":[],"output_truncated":true},
            "read_success":false,"read_error":"permission denied",
            "read_request_count":1,"coalesced_read_count":1
        }),
    );
    for evidence in [
        "src/a.rs:2\n",
        "read_success: false",
        "permission denied",
        "output_truncated: true",
    ] {
        assert!(text(&partial).contains(evidence), "{}", text(&partial));
    }
}

#[test]
fn cli_workspace_symbols_preserve_missing_positions_and_index_evidence() {
    let body = project(
        "list_workspace_symbols",
        true,
        json!({
            "project":"p","query":"fixture",
            "symbols":[
                {"path":"src/a.rs","name":"fixture_a","kind":"function","kind_code":12,
                 "range":{"start":{"line":2,"column":4},"end":{"line":2,"column":9}},
                 "container_name":"Module"},
                {"path":"src/b.rs","name":"fixture_b","kind":"function","kind_code":12,"range":null}
            ],
            "returned_count":2,"total_results":4,"truncated":true,"external_results_omitted":1,
            "invalid_results_omitted":1,"index_state":"building"
        }),
    );
    for evidence in [
        "src/a.rs:2:4-2:9 function fixture_a",
        "container_name: Module",
        "src/b.rs (position unavailable) function fixture_b",
        "total_results: 4",
        "truncated: true",
        "external_results_omitted: 1",
        "invalid_results_omitted: 1",
        "index_state: building",
    ] {
        assert!(text(&body).contains(evidence), "{}", text(&body));
    }
}

#[test]
fn cli_job_receipt_preserves_exact_followup_and_distinct_tokens() {
    let call = json!({"tool":"observe_jobs","arguments":{
        "items":[{"job_id":"wc_job_exact","after_observation_token":"opaque:/+="}],
        "wait_secs":37,"wake_on":"terminal"},"follow_up_kind":"fallback_recovery"});
    for (token, expected_count) in [("opaque:/+=", 1), ("distinct-token", 1)] {
        let body = project(
            "run_job",
            true,
            json!({
                "job_id":"wc_job_exact","status":"queued","execution_state":"queued",
                "command_started":false,"execution_lifetime":"runner",
                "requested_timeout_secs":7200,"effective_timeout_secs":3600,
                "timeout_limit_source":"runner_policy",
                "timeout_policy_basis":"last_registered_runner_policy",
                "timeout_notice":"Dispatch budget only; Runner enforces its current policy.",
                "cwd":"subdir","shell":"bash",
                "observation_token":token,
                "continuation_semantics":webcodex_core::runtime_contract::ContinuationSemantics::new(
                    webcodex_core::runtime_contract::ContinuationKind::Observe,
                    webcodex_core::runtime_contract::ContinuationCarrier::ObservationToken
                ).to_value(),
                "continuation":call,"stdout_tail":"","stdout_lines":0,
                "new_evidence":{"unknown_is_meaningful":null}
            }),
        );
        let rendered = text(&body);
        for evidence in [
            "wc_job_exact",
            "queued",
            "command_started: false",
            "execution_lifetime: runner",
            "requested_timeout_secs: 7200",
            "effective_timeout_secs: 3600",
            "timeout_limit_source: runner_policy",
            "timeout_policy_basis: last_registered_runner_policy",
            "Dispatch budget only; Runner enforces its current policy.",
            "cwd: subdir",
            "shell: bash",
            "unknown_is_meaningful: null",
            &call.to_string(),
        ] {
            assert!(rendered.contains(evidence), "{rendered}");
        }
        assert_eq!(
            rendered.matches(token).count(),
            expected_count,
            "{rendered}"
        );
        let calls: Vec<Value> = rendered
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .collect();
        assert_eq!(calls, vec![call.clone()]);
    }
}

#[test]
fn cli_lsp_error_message_is_deduplicated_only_when_exact() {
    for (error, duplicate) in [
        ("lsp_not_ready: indexing is building", true),
        ("transport failure while indexing", false),
    ] {
        let mut body = json!({"result":{"structuredContent":{
            "success":false,"error":error,"output":{
                "code":"lsp_not_ready","message":"indexing is building",
                "index_state":"building","retry_after_ms":1000
            }},"content":[]}});
        crate::mcp::cli::project_response(Some("list_workspace_symbols"), &mut body);
        let rendered = text(&body);
        assert_eq!(body["result"]["isError"], true);
        assert_eq!(
            rendered.contains("message: indexing is building"),
            !duplicate
        );
        for evidence in [
            "lsp_not_ready",
            "index_state: building",
            "retry_after_ms: 1000",
        ] {
            assert!(rendered.contains(evidence), "{rendered}");
        }
    }
}

#[test]
fn cli_plain_stdout_requires_exact_visible_counts_and_no_loss() {
    let complete = project(
        "run_process",
        true,
        json!({"stdout_tail":"hello\n","stdout_lines":1}),
    );
    assert_eq!(text(&complete), "hello\n");
    for output in [
        json!({"stdout_tail":"hello\n","stdout_lines":2}),
        json!({"stdout_tail":"hello\n","stdout_lines":1,"stdout_truncated":true}),
        json!({"stdout_tail":"hello\n","stdout_lines":1,"cwd":"."}),
        json!({"stdout_tail":"hello\n","stdout_lines":1,"future_evidence":true}),
    ] {
        assert_ne!(text(&project("run_process", true, output)), "hello\n");
    }
}

#[test]
fn cli_job_receipt_keeps_unknown_runner_policy_budget() {
    let body = project(
        "run_job",
        true,
        json!({
            "job_id":"wc_job_unknown_policy","status":"queued","execution_state":"queued",
            "command_started":false,"execution_lifetime":"runner",
            "requested_timeout_secs":7200,"effective_timeout_secs":null,
            "dispatched_timeout_secs":7200,"timeout_limit_source":"runner_policy_unknown",
            "timeout_policy_basis":"runner_policy_unavailable",
            "timeout_notice":"Runner policy unavailable; actual execution budget may be lower.",
            "future_budget_evidence":{"confirmed":false}
        }),
    );
    for evidence in [
        "queued",
        "command_started: false",
        "execution_lifetime: runner",
        "requested_timeout_secs: 7200",
        "effective_timeout_secs: null",
        "dispatched_timeout_secs: 7200",
        "timeout_limit_source: runner_policy_unknown",
        "timeout_policy_basis: runner_policy_unavailable",
        "Runner policy unavailable; actual execution budget may be lower.",
        "confirmed: false",
    ] {
        assert!(text(&body).contains(evidence), "{}", text(&body));
    }
}

#[test]
fn cli_job_batch_totals_are_omitted_only_when_visible_complete_items_prove_them() {
    let complete = json!({
        "items":[
            {"job_id":"missing","success":false,"output":null,"error_kind":"unknown_job","error":"unavailable"},
            {"job_id":"done","status":"completed","stdout_lines":3,"stdout_returned_lines":1,
             "stdout_retained_from":1,"stdout_truncated":true}
        ],
        "output_truncated":false,"requested_count":2,"returned_count":2,
        "failed_count":1,"succeeded_count":1,"terminal_count":1,"changed_count":0,
        "future_evidence":null
    });
    let rendered = text(&project("observe_jobs", true, complete.clone())).to_owned();
    assert!(!rendered.contains("requested_count:"));
    assert!(!rendered.contains("returned_count:"));
    for evidence in [
        "missing",
        "unknown_job",
        "done",
        "output_truncated: false",
        "failed_count: 1",
        "succeeded_count: 1",
        "terminal_count: 1",
        "changed_count: 0",
        "stdout_lines: 3",
        "stdout_returned_lines: 1",
        "stdout_retained_from: 1",
        "stdout_truncated: true",
        "future_evidence: null",
    ] {
        assert!(rendered.contains(evidence), "{rendered}");
    }
    for (key, value) in [
        ("output_truncated", Some(json!(true))),
        ("output_truncated", None),
        ("requested_count", Some(json!(3))),
        ("requested_count", None),
        ("returned_count", Some(json!(1))),
        ("returned_count", None),
    ] {
        let mut incomplete = complete.clone();
        if let Some(value) = value {
            incomplete[key] = value;
        } else {
            incomplete.as_object_mut().unwrap().remove(key);
        }
        let expected_requested = incomplete.get("requested_count").is_some();
        let expected_returned = incomplete.get("returned_count").is_some();
        let rendered = text(&project("observe_jobs", true, incomplete)).to_owned();
        assert_eq!(
            rendered.contains("requested_count:"),
            expected_requested,
            "{rendered}"
        );
        assert_eq!(
            rendered.contains("returned_count:"),
            expected_returned,
            "{rendered}"
        );
    }
}

#[test]
fn cli_workspace_symbols_keep_unknown_or_inconsistent_kind_codes_and_ranges() {
    for (kind, code) in [
        ("unknown", json!(99)),
        ("function", json!(99)),
        ("function", json!("future-code")),
        ("function", Value::Null),
    ] {
        let body = project(
            "list_workspace_symbols",
            true,
            json!({
                "symbols":[{"path":"src/a.rs","name":"fixture","kind":kind,"kind_code":code,
                    "range":{"start":{"line":2,"column":4},"end":{"line":2,"column":9}}}]
            }),
        );
        let value = code
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| code.to_string());
        assert!(
            text(&body).contains(&format!("kind_code: {value}")),
            "{}",
            text(&body)
        );
        assert!(text(&body).contains(&format!("src/a.rs:2:4-2:9 {kind} fixture")));
    }
    let body = project(
        "list_workspace_symbols",
        true,
        json!({
            "symbols":[{"path":"src/a.rs","name":"fixture","kind":"function","kind_code":12,
                "range":{"start":{"line":2,"column":4,"future_encoding":"utf32"},
                         "end":{"line":2,"column":9}}}],
            "index_state":"building"
        }),
    );
    for evidence in [
        "src/a.rs",
        "future_encoding: utf32",
        "kind_code: 12",
        "index_state: building",
    ] {
        assert!(text(&body).contains(evidence), "{}", text(&body));
    }
    assert!(!text(&body).contains("src/a.rs:2:4-2:9"));
}

#[test]
fn cli_job_receipt_does_not_fold_a_different_jobs_continuation() {
    let call = json!({"tool":"observe_jobs","arguments":{
        "items":[{"job_id":"wc_job_other","after_observation_token":"same-looking-token"}]},
        "follow_up_kind":"fallback_recovery"});
    let body = project(
        "run_job",
        true,
        json!({
            "job_id":"wc_job_original","status":"queued","command_started":false,
            "observation_token":"same-looking-token","continuation":call
        }),
    );
    assert!(text(&body).contains("observation_token: same-looking-token"));
    assert!(text(&body).contains(&call.to_string()));
    assert_eq!(text(&body).matches("same-looking-token").count(), 2);
}
