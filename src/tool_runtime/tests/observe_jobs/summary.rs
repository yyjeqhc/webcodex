use super::*;
use crate::tool_runtime::observe_jobs::summarize_observe_jobs_result;

fn successful_validation(job: &str) -> Value {
    let stdout = (0..120)
        .map(|index| format!("test module::regression_case_{index:03} ... ok\n"))
        .collect::<String>()
        + "\ntest result: ok. 120 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s\n";
    let stderr = "   Compiling fixture v0.1.0\n    Finished `test` profile in 1.00s\nwarning: preserve this diagnostic\n";
    let mut output =
        canonical_observation(job, "completed", "baseline", &stdout, stderr, true, true);
    output["exit_code"] = json!(0);
    // Real structured validation updates use canonical Job status/exit code;
    // they do not always populate the supplementary process execution field.
    output["command_execution_state"] = Value::Null;
    output["purpose"] = json!("test");
    output["command_summary"] = json!("cargo test -p fixture");
    output["stdout_lines"] = json!(stdout.lines().count());
    output["stderr_lines"] = json!(stderr.lines().count());
    output["validation"] = crate::tool_runtime::jobs::validation_job_projection(
        Some("cargo_test"),
        Some("test"),
        "completed",
        Some(0),
        &stdout,
        stderr,
        false,
        Some(1),
    )
    .expect("structured validation evidence");
    output
}

fn summarized(observation: Value, original_token: Option<String>) -> ToolResult {
    let job = observation["job_id"].as_str().unwrap().to_string();
    let mut result = canonical_batch(vec![canonical_success_item(0, observation)], "immediate", 0);
    summarize_observe_jobs_result(&mut result, &[item(&job, original_token)], 200);
    result
}

fn validate_result(result: &ToolResult) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("observe_jobs");
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &schema,
    )
    .unwrap();
}

fn without_observation_refs(output: &Value) -> Value {
    let mut normalized = output.clone();
    let Some(items) = normalized.get_mut("items").and_then(Value::as_array_mut) else {
        return normalized;
    };
    for item in items {
        if let Some(item) = item.as_object_mut() {
            item.remove("observation_ref");
        }
    }
    normalized
}

#[test]
fn summary_only_is_opt_in_closed_and_audited_without_tokens() {
    for value in [None, Some(json!(false)), Some(json!(true))] {
        let mut args = json!({"items": [{"job_id": "job"}]});
        if let Some(value) = &value {
            args["summary_only"] = value.clone();
        }
        let call = ToolCall::from_tool_name("observe_jobs", args).unwrap();
        let expected = value == Some(json!(true));
        assert!(
            matches!(&call, ToolCall::ObserveJobs { summary_only, .. } if *summary_only == expected)
        );
        assert_eq!(call.session_log_arguments()["summary_only"], expected);
    }
    for value in [json!("true"), json!(1), json!(null)] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({
                "items": [{"job_id": "job"}], "summary_only": value,
            })
        )
        .is_err());
    }
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "observe_jobs")
        .unwrap();
    assert_eq!(
        spec.input_schema["properties"]["summary_only"]["default"],
        false
    );
}

#[test]
fn summary_preserves_diagnostics_and_original_cursor_for_expansion() {
    let observation = successful_validation("summary-job");
    let original_token = observation["observation_token"]
        .as_str()
        .unwrap()
        .to_string();
    let mut current = observation.clone();
    current["observation_token"] =
        json!(crate::job_observation::JobObservationToken::new_baseline(
            "summary-job",
            "fixture_epoch",
            8,
        )
        .unwrap()
        .encode());
    let result = summarized(current.clone(), Some(original_token.clone()));
    let output = &result.output["items"][0]["output"];
    assert_eq!(output["validation"], observation["validation"]);
    assert_eq!(output["observation_token"], current["observation_token"]);
    assert_eq!(output["logs_omitted"], json!(["stdout", "stderr"]));
    assert!(!output["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("regression_case_"));
    assert!(output["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("120 passed; 0 failed"));
    assert_eq!(output["stderr_tail"], "warning: preserve this diagnostic\n");
    let detail = &output["suggested_call"];
    assert_eq!(
        detail["arguments"]["items"][0]["after_observation_token"],
        original_token
    );
    assert_ne!(
        detail["arguments"]["items"][0]["after_observation_token"],
        output["observation_token"]
    );
    assert_eq!(detail["arguments"]["summary_only"], false);
    assert!(detail["arguments"].get("wait_secs").is_none());
    ToolCall::from_tool_name("observe_jobs", detail["arguments"].clone()).unwrap();
    validate_result(&result);
    let sparse = compact_projection(&result);
    assert_eq!(
        sparse.output["items"][0]["logs_omitted"],
        output["logs_omitted"]
    );
    assert_eq!(sparse.output["items"][0]["suggested_call"], *detail);
    validate_result(&sparse);
    assert!(observation["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("regression_case_"));
}

#[test]
fn summary_accepts_optional_process_state_only_with_proven_validation() {
    for state in [None, Some(Value::Null), Some(json!("completed"))] {
        let mut observation = successful_validation("optional-process-state");
        if let Some(state) = state {
            observation["command_execution_state"] = state;
        } else {
            observation
                .as_object_mut()
                .unwrap()
                .remove("command_execution_state");
        }
        let result = summarized(observation.clone(), None);
        let output = &result.output["items"][0]["output"];
        assert!(output.get("logs_omitted").is_some());
        assert_eq!(output["validation"], observation["validation"]);
        assert_eq!(
            output.get("command_execution_state"),
            observation.get("command_execution_state")
        );
        for invalid in [
            json!("outcome_unknown"),
            json!("not_started"),
            json!("running"),
            json!(17),
        ] {
            let mut contradictory = observation.clone();
            contradictory["command_execution_state"] = invalid;
            assert_eq!(
                summarized(contradictory.clone(), None).output["items"][0]["output"],
                contradictory
            );
        }
        let mut unproven = observation.clone();
        unproven["validation"] = Value::Null;
        assert_eq!(
            summarized(unproven.clone(), None).output["items"][0]["output"],
            unproven
        );
    }
}

#[test]
fn summary_keeps_failure_unknown_zero_tests_and_unproven_evidence_unchanged() {
    for (pointer, value) in [
        ("/status", json!("failed")),
        ("/status", json!("cancelled")),
        ("/status", json!("timed_out")),
        ("/terminal", json!(false)),
        ("/exit_code", json!(1)),
        ("/command_execution_state", json!("outcome_unknown")),
        ("/validation", Value::Null),
        ("/validation/passed", json!(false)),
        ("/validation/truncated", json!(true)),
        ("/validation/tests_detected", json!(false)),
        ("/validation/tests_run_count", json!(0)),
        ("/validation/tests_run_count", Value::Null),
        ("/validation/tests_failed", json!(1)),
        ("/validation/zero_tests_run", json!(true)),
        ("/validation/tool", json!("arbitrary_command")),
        ("/validation/test_count_assertion/status", json!("unproven")),
        ("/recovery_state", json!("recovering")),
    ] {
        let mut observation = successful_validation("unsafe-summary");
        *observation.pointer_mut(pointer).expect(pointer) = value;
        assert_eq!(
            summarized(observation.clone(), None).output["items"][0]["output"],
            observation,
            "{pointer}"
        );
    }
    let mut observation = successful_validation("no-run-summary");
    observation["validation"]["no_run"] = json!(true);
    assert_eq!(
        summarized(observation.clone(), None).output["items"][0]["output"],
        observation
    );
}

#[test]
fn summary_ref_selector_preserves_original_ref_for_detail_expansion() {
    let observation = successful_validation("summary-ref-job");
    let mut result = canonical_batch(vec![canonical_success_item(0, observation)], "immediate", 0);
    summarize_observe_jobs_result(&mut result, &[item_ref("~j77")], 200);

    let output = &result.output["items"][0]["output"];
    assert!(output.get("logs_omitted").is_some(), "{output}");
    let detail = &output["suggested_call"];
    assert_eq!(detail["tool"], "observe_jobs");
    assert_eq!(
        detail["arguments"]["items"][0],
        json!({"observation_ref": "~j77"})
    );
    assert_eq!(detail["arguments"]["summary_only"], false);
    assert!(detail["arguments"]["items"][0].get("job_id").is_none());
    ToolCall::from_tool_name("observe_jobs", detail["arguments"].clone()).unwrap();
    validate_result(&result);
    validate_result(&compact_projection(&result));
}

#[test]
fn summary_retains_reset_truncation_and_nonroutine_lines() {
    let mut observation = successful_validation("reset-summary");
    observation["log_delta_status"] = json!("reset");
    observation["stdout_delta_reset"] = json!(true);
    observation["stdout_truncated"] = json!(true);
    observation["earlier_stdout_unavailable"] = json!(true);
    let old_stdout = observation["stdout_tail"].as_str().unwrap().to_string();
    observation["stdout_tail"] = json!(
        old_stdout
            + "custom diagnostic must remain\n    Checking custom stdout\ntest partial ... ok"
    );
    let stderr = observation["stderr_tail"].as_str().unwrap().to_string();
    observation["stderr_tail"] = json!(stderr + "test stderr diagnostic ... ok\n");
    let result = summarized(observation.clone(), None);
    let sparse = compact_projection(&result);
    let output = &sparse.output["items"][0];
    for field in [
        "log_delta_status",
        "stdout_delta_reset",
        "stdout_truncated",
        "earlier_stdout_unavailable",
    ] {
        assert_eq!(output[field], observation[field]);
    }
    assert!(output["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("custom diagnostic must remain"));
    assert!(output["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("    Checking custom stdout"));
    assert!(output["stderr_tail"]
        .as_str()
        .unwrap()
        .contains("test stderr diagnostic ... ok"));
    assert!(output["stdout_tail"]
        .as_str()
        .unwrap()
        .ends_with("test partial ... ok"));
    assert!(output["suggested_call"]["arguments"]["items"][0]
        .get("after_observation_token")
        .is_none());
    validate_result(&sparse);
}

#[test]
fn summary_preserves_mixed_batch_order_errors_and_suffix_preferences() {
    let good = successful_validation("good");
    let mut failed = successful_validation("failed");
    failed["exit_code"] = json!(1);
    let mut result = canonical_batch(
        vec![
            canonical_success_item(0, good),
            canonical_success_item(1, failed.clone()),
        ],
        "terminal",
        23,
    );
    result.output["output_truncated"] = json!(true);
    result.output["suggested_call"] = json!({"tool": "observe_jobs", "arguments": {"items": [{"job_id": "next"}], "tail_lines": 40}});
    summarize_observe_jobs_result(
        &mut result,
        &[item("good", None), item("failed", None), item("next", None)],
        200,
    );
    assert_eq!(result.output["items"][0]["job_id"], "good");
    assert_eq!(result.output["items"][1]["output"], failed);
    assert_eq!(
        result.output["wait"],
        json!({"outcome": "terminal", "waited_ms": 23})
    );
    assert_eq!(
        result.output["suggested_call"]["arguments"]["summary_only"],
        true
    );
    ToolCall::from_tool_name(
        "observe_jobs",
        result.output["suggested_call"]["arguments"].clone(),
    )
    .unwrap();
}

#[tokio::test]
async fn summary_runtime_roundtrip_expands_original_logs_without_reexecution() {
    use crate::tool_runtime::tests::validation_handoff::{
        cargo_test_update, completed_progress, running_progress,
    };
    let client = "summary-runtime";
    let runtime =
        runtime_with_agent_project(client).with_validation_sync_wait(Duration::from_millis(25));
    register_agent(
        &runtime,
        client,
        None,
        RunnerCapabilities {
            async_shell_jobs: true,
            structured_validation_argv: true,
            ..Default::default()
        },
    )
    .await;
    let auth = auth_context(None, true);
    let project = agent_test_project_id(client);
    let call = ToolCall::from_tool_name(
        "cargo_test",
        json!({
            "project": project, "min_tests": 1, "timeout_secs": 30
        }),
    )
    .unwrap();
    let start = tokio::spawn({
        let runtime = runtime.clone();
        let auth = auth.clone();
        async move { runtime.dispatch_with_auth(call, Some(&auth)).await }
    });
    let request = wait_for_patch_agent_request(&runtime, client).await;
    let job = request.job_id.clone().expect("structured validation Job");
    runtime
        .runner_registry
        .update_job(cargo_test_update(
            client,
            &request.request_id,
            &job,
            "running",
            "",
            "",
            None,
            running_progress("test"),
            false,
        ))
        .await
        .unwrap();
    let handoff = start.await.unwrap();
    assert!(handoff.success, "{handoff:?}");
    let original_token = observation_token(&runtime, &job, &auth).await;
    let fixture = successful_validation(&job);
    let terminal = cargo_test_update(
        client,
        &request.request_id,
        &job,
        "completed",
        fixture["stdout_tail"].as_str().unwrap(),
        fixture["stderr_tail"].as_str().unwrap(),
        Some(0),
        completed_progress(),
        true,
    );
    assert!(terminal.command_execution_state.is_none());
    runtime.runner_registry.update_job(terminal).await.unwrap();

    let base_args = json!({"items": [{"job_id": job, "after_observation_token": original_token}], "tail_lines": 200});
    let baseline = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("observe_jobs", base_args.clone()).unwrap(),
            Some(&auth),
        )
        .await;
    let mut explicit_args = base_args.clone();
    explicit_args["summary_only"] = json!(false);
    let explicit = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("observe_jobs", explicit_args.clone()).unwrap(),
            Some(&auth),
        )
        .await;
    assert_eq!(
        without_observation_refs(&baseline.output),
        without_observation_refs(&explicit.output),
        "omission must preserve the false path exactly apart from refreshed compact refs"
    );
    explicit_args["summary_only"] = json!(true);
    let compact = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("observe_jobs", explicit_args).unwrap(),
            Some(&auth),
        )
        .await;
    assert!(compact.success, "{compact:?}");
    let output = &compact.output["items"][0]["output"];
    assert_eq!(output["validation"]["tests_run_count"], 120);
    assert_eq!(output["validation"]["passed"], true);
    assert!(output.get("logs_omitted").is_some(), "{output}");
    let detail = &output["suggested_call"];
    assert_eq!(
        detail["arguments"]["items"][0]["after_observation_token"],
        original_token
    );
    let expanded = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("observe_jobs", detail["arguments"].clone()).unwrap(),
            Some(&auth),
        )
        .await;
    assert_eq!(
        without_observation_refs(&expanded.output),
        without_observation_refs(&baseline.output),
        "expansion must reread the same retained log selection apart from refreshed compact refs"
    );
    assert!(
        probe_patch_agent_request(&runtime, client).await.is_none(),
        "observation must not enqueue execution"
    );
    validate_result(&compact);
    validate_result(&compact_projection(&compact));
    let before = serialized_result_bytes(&compact_projection(&baseline));
    let after = serialized_result_bytes(&compact_projection(&compact));
    println!("OUTPUT_PROJECTION_BENCH {{\"case\":\"runtime_dispatch_and_log_recovery\",\"before_bytes\":{before},\"after_bytes\":{after}}}");
    assert!(after * 100 < before * 70);

    for summary_only in [false, true] {
        let unknown = runtime
            .dispatch_with_auth(
                ToolCall::from_tool_name(
                    "observe_jobs",
                    json!({
                        "items": [{"job_id": "missing-summary-job"}], "summary_only": summary_only
                    }),
                )
                .unwrap(),
                Some(&auth),
            )
            .await;
        assert_eq!(unknown.output["items"][0]["success"], false);
        assert_eq!(unknown.output["items"][0]["error_kind"], "unknown_job");
    }
}

#[test]
fn summary_never_inflates_tiny_results_and_reports_paired_output_bytes() {
    let observation = successful_validation("size-summary");
    let baseline = compact_projection(&canonical_batch(
        vec![canonical_success_item(0, observation.clone())],
        "immediate",
        0,
    ));
    let optimized = compact_projection(&summarized(observation.clone(), None));
    let before = serialized_result_bytes(&baseline);
    let after = serialized_result_bytes(&optimized);
    println!("OUTPUT_PROJECTION_BENCH {{\"case\":\"120_successful_tests_with_warning\",\"before_bytes\":{before},\"after_bytes\":{after}}}");
    assert!(
        after * 100 < before * 70,
        "summary must save at least 30% for this fixture: {before} -> {after}"
    );
    let mut tiny = observation;
    tiny["stdout_tail"] = json!("test short ... ok\n");
    tiny["stderr_tail"] = json!("");
    assert_eq!(
        summarized(tiny.clone(), None).output["items"][0]["output"],
        tiny
    );
}
