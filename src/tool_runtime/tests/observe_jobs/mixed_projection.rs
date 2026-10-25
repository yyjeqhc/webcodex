//! Mixed model projections preserve the canonical batch's outcome and recovery.
use super::*;

fn missing_item(index: usize) -> Value {
    json!({
        "index": index, "job_id": "missing-job", "success": false,
        "output": null, "error_kind": "unknown_job",
        "suggested_call": {
            "follow_up_kind": "fallback_recovery", "tool": "list_jobs", "arguments": {}
        },
        "error": "unknown job: missing-job"
    })
}

fn validate_projection(result: &ToolResult) {
    let schema = super::super::super::registry::output_schema_for_tool("observe_jobs");
    super::super::super::startup_brief::validate_schema_instance_for_test(
        &serde_json::to_value(result).unwrap(),
        &schema,
    )
    .unwrap();
}

fn assert_batch_facts_unchanged(canonical: &ToolResult, projected: &ToolResult) {
    assert_eq!(projected.success, canonical.success);
    assert_eq!(projected.error, canonical.error);
    for key in [
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "changed_count",
        "terminal_count",
        "output_truncated",
        "wait",
        "suggested_call",
    ] {
        assert_eq!(
            projected.output.get(key),
            canonical.output.get(key),
            "{key}"
        );
    }
}

#[test]
fn observe_jobs_mixed_projection_keeps_failure_and_unknown_outcome() {
    let running = canonical_success_item(
        0,
        canonical_observation("job-running", "running", "unchanged", "", "", false, false),
    );
    let mut completed = canonical_observation(
        "job-completed",
        "completed",
        "delta",
        "done\n",
        "",
        true,
        true,
    );
    completed["exit_code"] = json!(0);
    completed["command_execution_state"] = json!("completed");
    let mut uncertain = canonical_observation(
        "job-uncertain",
        "failed",
        "baseline",
        "partial write\n",
        "",
        true,
        true,
    );
    uncertain["command_execution_state"] = json!("outcome_unknown");
    uncertain["error"] = json!("Runner connection lost after command start");
    uncertain["recovery_state"] = json!("outcome_unknown");
    uncertain["recovery_reason_code"] = json!("runner_instance_replaced");
    uncertain["state_changed"] = Value::Null;
    uncertain["failure_kind"] = json!("outcome_unknown");
    let unknown = canonical_success_item(3, uncertain);
    for success in [true, false] {
        let mut canonical = canonical_batch(
            vec![
                running.clone(),
                canonical_success_item(1, completed.clone()),
                missing_item(2),
                unknown.clone(),
            ],
            "item_error",
            0,
        );
        canonical.success = success;
        canonical.error = (!success).then(|| "One observation failed".to_string());
        let original = serde_json::to_value(&canonical).unwrap();
        let projected = compact_projection(&canonical);
        println!(
            "OBSERVE_JOBS_MIXED_BYTES success={success} before={} after={}",
            serialized_result_bytes(&canonical),
            serialized_result_bytes(&projected),
        );
        assert_batch_facts_unchanged(&canonical, &projected);
        assert!(projected.output["items"][0].get("output").is_none());
        assert_eq!(projected.output["items"][0]["job_id"], "job-running");
        assert_eq!(projected.output["items"][0]["index"], 0);
        assert_eq!(projected.output["items"][1]["status"], "completed");
        assert_eq!(projected.output["items"][1]["exit_code"], 0);
        assert_eq!(projected.output["items"][1]["stdout_tail"], "done\n");
        assert_eq!(projected.output["items"][2], canonical.output["items"][2]);
        assert_eq!(projected.output["items"][3], unknown);
        assert!(serialized_result_bytes(&projected) < serialized_result_bytes(&canonical));
        assert_eq!(serde_json::to_value(&canonical).unwrap(), original);
        validate_projection(&projected);
    }
}

#[test]
fn observe_jobs_mixed_projection_retains_reset_truncation_and_one_wait() {
    for delta in ["reset", "delta"] {
        let mut observation = canonical_observation(
            "job-logs",
            "running",
            delta,
            "tail\n",
            "warning\n",
            true,
            false,
        );
        observation["stdout_truncated"] = json!(true);
        observation["stdout_delta_reset"] = json!(delta == "reset");
        observation["earlier_stdout_unavailable"] = json!(true);
        observation["stdout_retained_from_line"] = json!(41);
        observation["recovery_reason_code"] = json!("log_retention_exceeded");
        let canonical = canonical_batch(
            vec![
                canonical_success_item(0, observation.clone()),
                missing_item(1),
            ],
            "timeout",
            1_002,
        );
        let projected = compact_projection(&canonical);
        assert_batch_facts_unchanged(&canonical, &projected);
        let item = &projected.output["items"][0];
        assert!(item.get("output").is_none());
        for key in [
            "job_id",
            "log_delta_status",
            "stdout_tail",
            "stderr_tail",
            "stdout_truncated",
            "earlier_stdout_unavailable",
            "stdout_retained_from_line",
            "stdout_lines",
            "stderr_lines",
            "cursor",
            "observation_token",
            "recovery_reason_code",
        ] {
            assert_eq!(item[key], observation[key], "{key}");
        }
        if delta == "reset" {
            assert_eq!(item["stdout_delta_reset"], true);
            assert_eq!(item["stderr_delta_reset"], false);
        }
        validate_projection(&projected);
    }
}

#[test]
fn observe_jobs_mixed_projection_keeps_truncated_suffix_recovery() {
    let mut canonical = canonical_batch(
        vec![
            canonical_success_item(
                0,
                canonical_observation("job-page", "running", "unchanged", "", "", false, false),
            ),
            missing_item(1),
        ],
        "item_error",
        0,
    );
    canonical.output["requested_count"] = json!(3);
    canonical.output["output_truncated"] = json!(true);
    canonical.output["suggested_call"] = json!({
        "follow_up_kind": "mechanically_followable",
        "tool": "observe_jobs",
        "arguments": {
            "items": [{"job_id": "job-later", "after_observation_token": "original-token"}],
            "tail_lines": 40, "summary_only": false
        }
    });
    let projected = compact_projection(&canonical);
    assert_batch_facts_unchanged(&canonical, &projected);
    assert!(projected.output["items"][0].get("output").is_none());
    assert_eq!(projected.output["items"][0]["index"], 0);
    assert_eq!(projected.output["items"][1], canonical.output["items"][1]);
    let followup = &projected.output["suggested_call"]["arguments"];
    ToolCall::from_tool_name("observe_jobs", followup.clone()).unwrap();
    assert!(followup.get("wait_secs").is_none());
    assert!(followup.get("wake_on").is_none());
    validate_projection(&projected);
}

#[test]
fn observe_jobs_mixed_projection_isolates_ineligible_success_items() {
    let good = canonical_success_item(
        0,
        canonical_observation("job-normal", "running", "unchanged", "", "", false, false),
    );
    let mut uncertain = canonical_success_item(
        1,
        canonical_observation("job-uncertain", "failed", "baseline", "", "", true, true),
    );
    uncertain["output"]["command_execution_state"] = json!("outcome_unknown");
    let canonical = canonical_batch(vec![good, uncertain.clone()], "terminal", 25);
    let projected = compact_projection(&canonical);
    assert_batch_facts_unchanged(&canonical, &projected);
    assert!(projected.output["items"][0].get("output").is_none());
    assert_eq!(projected.output["items"][1], uncertain);
    validate_projection(&projected);

    // Invalid aggregate facts are never repaired by a presentation function.
    let mut malformed = ToolResult::ok(canonical.output.clone());
    malformed.output["succeeded_count"] = json!(1);
    for truncated in [false, true] {
        malformed.output["output_truncated"] = json!(truncated);
        if truncated {
            malformed.output["next_index"] = json!(1);
            malformed.output["continuation_semantics"] = json!("original suffix");
            malformed.output["suggested_call"] = json!({
                "follow_up_kind": "mechanically_followable", "tool": "observe_jobs",
                "arguments": {"items": [{"job_id": "job-later"}]}
            });
        }
        assert_eq!(
            serde_json::to_value(compact_projection(&malformed)).unwrap(),
            serde_json::to_value(&malformed).unwrap(),
        );
    }
}

#[tokio::test]
async fn observe_jobs_mixed_projection_kernel_preserves_canonical_semantics() {
    let runtime = test_runtime();
    let (job_id, request, auth) =
        register_and_start_agent_job(&runtime, "observe-mixed-kernel").await;
    update_observed_job(
        &runtime,
        "observe-mixed-kernel",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    let arguments = json!({"items": [{"job_id": job_id}, {"job_id": "missing-job"}]});
    let canonical = runtime
        .dispatch_with_auth(
            ToolCall::from_tool_name("observe_jobs", arguments.clone()).unwrap(),
            Some(&auth),
        )
        .await;
    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "observe_jobs".to_string(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth: Some(&auth),
                window: None,
                record_oauth_scope_denials: true,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        )
        .await;
    assert!(outcome.error_status.is_none(), "{:?}", outcome.error_status);
    let audit = outcome
        .canonical_audit_output
        .as_ref()
        .expect("canonical audit capture");
    assert_eq!(audit["requested_count"], 2);
    assert_eq!(audit["returned_count"], 2);
    assert_eq!(audit["succeeded_count"], 1);
    assert_eq!(audit["failed_count"], 1);
    assert_eq!(audit["items"][0]["output"]["job_id"], job_id);
    assert_eq!(audit["items"][1], canonical.output["items"][1]);
    let trace = outcome
        .correlation
        .job_audit
        .as_ref()
        .expect("canonical Job audit trace");
    assert_eq!(trace.requested_jobs, 2);
    assert_eq!(trace.returned_count, Some(2));
    assert_eq!(trace.metadata_unavailable_count, 1);
    let projected = outcome.result.unwrap();
    assert_batch_facts_unchanged(&canonical, &projected);
    assert!(projected.output["items"][0].get("output").is_none());
    assert_eq!(projected.output["items"][0]["job_id"], job_id);
    assert_eq!(projected.output["items"][1], canonical.output["items"][1]);
    assert_eq!(projected.output["failed_count"], 1);
    assert_eq!(projected.output["wait"]["outcome"], "immediate");
    assert!(probe_patch_agent_request(&runtime, "observe-mixed-kernel")
        .await
        .is_none());
    validate_projection(&projected);
}

#[test]
fn observe_jobs_mixed_projection_schema_keeps_failure_recovery_closed() {
    let canonical = canonical_batch(
        vec![
            canonical_success_item(
                0,
                canonical_observation("job-good", "running", "unchanged", "", "", false, false),
            ),
            missing_item(1),
        ],
        "item_error",
        0,
    );
    let projected = compact_projection(&canonical);
    assert!(projected.output["items"][0].get("output").is_none());
    validate_projection(&projected);
    let schema = super::super::super::registry::output_schema_for_tool("observe_jobs");
    let mut invalid = serde_json::to_value(&projected).unwrap();
    invalid["output"]["items"][1]["recovery_kind"] = json!("reobserve");
    assert!(
        super::super::super::startup_brief::validate_schema_instance_for_test(&invalid, &schema,)
            .is_err()
    );
    invalid = serde_json::to_value(&projected).unwrap();
    invalid["output"]["items"][1]["suggested_call"]["tool"] = json!("run_job");
    assert!(
        super::super::super::startup_brief::validate_schema_instance_for_test(&invalid, &schema,)
            .is_err()
    );
}
