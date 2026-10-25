//! Deterministic adapter/contract fixtures, not real Host conversations.
//! Counts are declared calls and UTF-8/JSON bytes, never tokens or latency.
use super::*;
use crate::mcp::request_policy::McpSurface;
use crate::tool_runtime::startup_brief::validate_schema_instance_for_test;

struct Step {
    tool: &'static str,
    arguments: Value,
    success: bool,
    output: Value,
    evidence: &'static [&'static str],
}

fn step(
    tool: &'static str,
    arguments: Value,
    success: bool,
    output: Value,
    evidence: &'static [&'static str],
) -> Step {
    Step {
        tool,
        arguments,
        success,
        output,
        evidence,
    }
}

fn scenarios() -> Vec<(&'static str, Vec<Step>)> {
    let project = "agent:fixture:repo";
    let continuation = json!({
        "tool":"observe_jobs", "follow_up_kind":"fallback_recovery",
        "arguments":{"items":[{"job_id":"wc_job_fixture","after_observation_token":"opaque/+token"}]}
    });
    let read = |revision: u64, value: i32| {
        step(
            "read_files",
            json!({"project":project,"items":[{"path":"src/lib.rs","start_line":1,"limit":2}]}),
            true,
            json!({"items":[{"index":0,"path":"src/lib.rs","output":{"text":format!("fn sum() -> i32 {{ {value} }}\n"),"read_revision":revision,"start_line":1,"end_line":1}}]}),
            &["src/lib.rs", "read_revision", "fn sum()"],
        )
    };
    let pending = || {
        step(
            "run_process",
            json!({"project":project,"executable":"cargo","args":["test"],"timeout_secs":600}),
            true,
            json!({"execution_state":"pending","continuation":continuation}),
            &[
                "pending",
                "wc_job_fixture",
                "opaque/+token",
                "fallback_recovery",
                "observe_jobs",
            ],
        )
    };
    vec![
        (
            "coding_failure_analysis",
            vec![
                step(
                    "search_project_texts",
                    json!({"project":project,"queries":[{"pattern":"fn sum","pattern_mode":"literal","limit":4}]}),
                    true,
                    json!({"items":[{"index":0,"output":{"matches":[{"path":"src/lib.rs","line":1,"preview":"fn sum() -> i32 { 1 }"}]}}]}),
                    &["src/lib.rs", "fn sum()"],
                ),
                read(7, 1),
                step(
                    "edit_project_files",
                    json!({"project":project,"changes":[{"kind":"edit","path":"src/lib.rs","expected_read_revision":7,"edits":[{"kind":"replace_exact","old_text":"{ 1 }","new_text":"{ 2 }"}]}]}),
                    true,
                    json!({"changed":true,"files":[{"index":0,"kind":"edit","path":"src/lib.rs","changed":true,"read_revision":8}]}),
                    &["src/lib.rs", "changed", "read_revision", "8"],
                ),
                step(
                    "run_process",
                    json!({"project":project,"executable":"cargo","args":["test"]}),
                    false,
                    json!({"execution_state":"completed","exit_code":101,"failure_kind":"command_exit_nonzero","stderr_tail":"sum_test failed at src/lib.rs:2"}),
                    &[
                        "completed",
                        "101",
                        "command_exit_nonzero",
                        "sum_test failed",
                    ],
                ),
                read(8, 2),
            ],
        ),
        (
            "unfamiliar_repository",
            vec![
                step(
                    "list_project_tracked_files",
                    json!({"project":project,"query":"sum","limit":4}),
                    true,
                    json!({"entries":[{"path":"src/sum.rs","kind":"file"}],"returned":1,"total_files":1,"total_entries":1,"depth":null,"depth_auto":false,"truncated":false,"next_offset":null,"list_truncated":false,"source":"git_index"}),
                    &["src/sum.rs"],
                ),
                step(
                    "search_file_context",
                    json!({"project":project,"queries":[{"pattern":"fn sum","pattern_mode":"literal"}],"max_reads":1}),
                    true,
                    json!({"search":{"items":[{"index":0,"output":{"matches":[{"path":"src/sum.rs","line":1,"preview":"fn sum() {}"}]}}]},"reads":{"items":[{"index":0,"path":"src/sum.rs","output":{"text":"fn sum() {}","read_revision":9}}]}}),
                    &["src/sum.rs", "fn sum()", "read_revision", "9"],
                ),
            ],
        ),
        (
            "long_job_success",
            vec![
                pending(),
                step(
                    "wait_for_job_readiness",
                    json!({"job_ids":["wc_job_fixture"],"mode":"all","wait_secs":45}),
                    true,
                    json!({"wait_state":"ready","ready":[{"job_id":"wc_job_fixture","status":"completed","outcome":"succeeded"}],"pending_job_ids":[]}),
                    &["ready", "wc_job_fixture", "completed"],
                ),
                step(
                    "observe_jobs",
                    continuation["arguments"].clone(),
                    true,
                    json!({"items":[{"job_id":"wc_job_fixture","status":"completed","terminal":true,"exit_code":0,"stdout_tail":"all checks passed","observation_token":"opaque/+done"}]}),
                    &[
                        "wc_job_fixture",
                        "completed",
                        "all checks passed",
                        "opaque/+done",
                    ],
                ),
            ],
        ),
        (
            "long_job_timeout",
            vec![
                pending(),
                step(
                    "observe_jobs",
                    continuation["arguments"].clone(),
                    true,
                    json!({"items":[{"job_id":"wc_job_fixture","status":"failed","terminal":true,"command_execution_state":"timed_out","failure_kind":"timeout","effective_timeout_secs":60,"observation_token":"opaque/+timeout"}]}),
                    &[
                        "wc_job_fixture",
                        "failed",
                        "timed_out",
                        "timeout",
                        "60",
                        "opaque/+timeout",
                    ],
                ),
            ],
        ),
        (
            "unknown_execution",
            vec![
                step(
                    "run_shell",
                    json!({"project":project,"command":"example-command","timeout_secs":60}),
                    false,
                    json!({"execution_state":"outcome_unknown","state_changed":null,"job_id":"wc_job_fixture","continuation":continuation}),
                    &[
                        "outcome_unknown",
                        "wc_job_fixture",
                        "state_changed",
                        "null",
                        "fallback_recovery",
                        "observe_jobs",
                    ],
                ),
                step(
                    "observe_jobs",
                    continuation["arguments"].clone(),
                    true,
                    json!({"items":[{"job_id":"wc_job_fixture","status":"lost","terminal":true,"command_execution_state":"outcome_unknown","observation_token":"opaque/+unknown","recovery_kind":"reconcile"}]}),
                    &["wc_job_fixture", "lost", "outcome_unknown", "reconcile"],
                ),
            ],
        ),
        (
            "discovery_and_call",
            vec![
                step(
                    "read_tool_manifest",
                    json!({"query":"run_script","limit":1}),
                    true,
                    json!({"tools":[{"name":"run_script","description":"Execute typed script data.","route":{"mode":"gateway","via":"call_runtime_tool"}}]}),
                    &["run_script"],
                ),
                step(
                    "read_tool_manifest",
                    json!({"tool_name":"run_script"}),
                    true,
                    json!({"name":"run_script","input_schema":webcodex_tool_contracts::input_schema_for_tool("run_script"),"route":{"primary":{"mode":"gateway","tool":"call_runtime_tool","target":"run_script"}}}),
                    &[
                        "run_script",
                        "input_schema",
                        "language",
                        "script",
                        "required",
                    ],
                ),
                step(
                    "run_script",
                    json!({"project":project,"language":"python","script":"print('ready')"}),
                    true,
                    json!({"stdout_tail":"ready"}),
                    &["ready"],
                ),
            ],
        ),
    ]
}

#[test]
fn tool_experience_scenario_costs_preserve_contract_and_recovery() {
    let mut rows = Vec::new();
    for (name, steps) in scenarios() {
        // This fixture checks explicit read-to-edit evidence, not Session-latest inference.
        if name == "coding_failure_analysis" {
            assert_eq!(
                steps[1].output["items"][0]["output"]["read_revision"],
                steps[2].arguments["changes"][0]["expected_read_revision"]
            );
            assert_eq!(
                steps[2].output["files"][0]["read_revision"],
                steps[4].output["items"][0]["output"]["read_revision"]
            );
        }
        if name.starts_with("long_job") || name == "unknown_execution" {
            let continuation = &steps[0].output["continuation"];
            assert_eq!(continuation["tool"], "observe_jobs");
            assert_eq!(continuation["follow_up_kind"], "fallback_recovery");
            for observed in steps.iter().filter(|step| step.tool == "observe_jobs") {
                assert_eq!(observed.arguments, continuation["arguments"]);
                assert_eq!(observed.output["items"][0]["job_id"], "wc_job_fixture");
            }
            if name == "unknown_execution" {
                assert_eq!(steps[0].output["execution_state"], "outcome_unknown");
                assert_eq!(
                    steps[1].output["items"][0]["command_execution_state"],
                    "outcome_unknown"
                );
            }
        }
        for surface in [McpSurface::Adaptive, McpSurface::Cli] {
            let mut result_bytes = 0_u64;
            let mut text_bytes = 0_u64;
            let mut structured_bytes = 0_u64;
            for step in &steps {
                let schema = webcodex_tool_contracts::input_schema_for_tool(step.tool);
                validate_schema_instance_for_test(&step.arguments, &schema)
                    .unwrap_or_else(|error| panic!("{name}/{}: {error}", step.tool));
                crate::tool_runtime::ToolCall::from_tool_name(step.tool, step.arguments.clone())
                    .unwrap_or_else(|error| panic!("{name}/{}: {error}", step.tool));
                let mut body = json!({"result":{
                    "structuredContent":{"success":step.success,"output":step.output,"error":if step.success {Value::Null} else {json!("fixture failure")}},
                    "content":[{"type":"text","text":"WebCodex tool completed successfully."}],
                    "isError":!step.success
                }});
                if surface == McpSurface::Cli {
                    crate::mcp::cli::project_response(Some(step.tool), &mut body);
                }
                assert_eq!(body["result"]["isError"], !step.success);
                let visible = if surface == McpSurface::Cli {
                    assert!(body["result"].get("structuredContent").is_none());
                    body["result"]["content"][0]["text"]
                        .as_str()
                        .unwrap()
                        .to_string()
                } else {
                    body["result"]["structuredContent"].to_string()
                };
                for evidence in step.evidence {
                    assert!(
                        visible.contains(evidence),
                        "{name}/{} lost {evidence}: {visible}",
                        step.tool
                    );
                }
                if let Some(call) = step.output.get("continuation") {
                    if surface == McpSurface::Cli {
                        let rendered_call = visible
                            .lines()
                            .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
                            .find(|value| value.get("tool").is_some())
                            .expect("exact parser-ready recovery call");
                        assert_eq!(&rendered_call, call);
                    }
                    validate_schema_instance_for_test(
                        &call["arguments"],
                        &webcodex_tool_contracts::input_schema_for_tool(
                            call["tool"].as_str().unwrap(),
                        ),
                    )
                    .unwrap();
                }
                let measured = mcp_tool_response_audit_summary(&body["result"], surface).unwrap();
                result_bytes += measured["result_json_bytes"].as_u64().unwrap();
                text_bytes += measured["text_utf8_bytes"].as_u64().unwrap();
                structured_bytes += measured["structured_json_bytes"].as_u64().unwrap_or(0);
            }
            rows.push(json!({"scenario":name,"surface":if surface == McpSurface::Cli {"cli"} else {"adaptive"},
                "declared_calls":steps.len(),"result_json_bytes":result_bytes,
                "text_utf8_bytes":text_bytes,"structured_json_bytes":structured_bytes}));
        }
    }
    eprintln!(
        "TOOL_EXPERIENCE_BASELINE {}",
        json!({
            "measurement_kind":"deterministic_adapter_fixture",
            "host_tokens":null,"end_to_end_latency_ms":null,"client_parameter_error_rate":null,
            "scenarios":rows
        })
    );
}

#[tokio::test]
async fn tool_experience_exact_discovery_constructs_callable_arguments() {
    use crate::mcp::request_policy::SURFACE_HEADER;
    let (_tmp, db) = test_db();
    let service = Service::new(build_test_router(
        test_config(Some("secret")),
        db,
        Arc::new(test_runtime()),
    ));
    for cli in [false, true] {
        let mut request = TestClient::post("http://localhost/mcp").bearer_auth("secret");
        if cli {
            request = request.add_header(SURFACE_HEADER, "cli", true);
        }
        let mut response = request
            .json(&json!({
                "jsonrpc":"2.0","id":501,"method":"tools/call",
                "params":{"name":"read_tool_manifest","arguments":{"tool_name":"run_script"}}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&response), StatusCode::OK);
        let body: Value = response.take_json().await.unwrap();
        let schema = if cli {
            body["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("input_schema: "))
                .map(|line| serde_json::from_str::<Value>(line).unwrap())
                .unwrap()
        } else {
            body["result"]["structuredContent"]["output"]["input_schema"].clone()
        };
        let arguments =
            json!({"project":"agent:fixture:repo","language":"python","script":"print('ready')"});
        assert_eq!(
            schema,
            webcodex_tool_contracts::input_schema_for_tool("run_script")
        );
        validate_schema_instance_for_test(&arguments, &schema).unwrap();
        crate::tool_runtime::ToolCall::from_tool_name("run_script", arguments.clone()).unwrap();
        for invalid in [
            json!({"project":"agent:fixture:repo","language":"python"}),
            json!({"project":"agent:fixture:repo","language":"python","script":"print(1)","workdir":"."}),
        ] {
            assert!(validate_schema_instance_for_test(&invalid, &schema).is_err());
            assert!(crate::tool_runtime::ToolCall::from_tool_name("run_script", invalid).is_err());
        }
        // Gateway wrapping does not remove the canonical inner contract.
        let gateway = json!({"tool":"run_script","arguments":arguments});
        validate_schema_instance_for_test(
            &gateway,
            &adaptive_runtime_gateway_input_schema_for_test(),
        )
        .unwrap();
    }
}
