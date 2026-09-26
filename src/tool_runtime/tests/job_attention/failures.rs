use super::*;
use crate::tool_runtime::tests::jobs::compact_validation_job;
use serde_json::Value;
use webcodex_runner_registry::{JobAttentionSnapshot, JobValidationOutput};

fn snapshot(
    runtime: &ToolRuntime,
    tool: &str,
    kind: &str,
    stdout: &str,
    stderr: &str,
) -> JobAttentionSnapshot {
    let project = "agent:passive-validation:demo";
    let mut job = compact_validation_job(project, runtime.validation_sources.capture(project));
    job.exit_code = Some(1);
    let metadata = job.validation.as_mut().unwrap();
    metadata.tool = tool.into();
    metadata.kind = kind.into();
    JobAttentionSnapshot {
        recovery: None,
        job,
        validation_output: Some(JobValidationOutput {
            stdout: stdout.into(),
            stderr: stderr.into(),
            truncated: false,
        }),
    }
}

fn assert_schema(item: &Value) {
    let schema = crate::tool_runtime::registry::output_schema_for_tool("cargo_check");
    let schema = &schema["properties"]["output"]["properties"]["job_attention"];
    let attention = json!({"changed": true, "items": [item]});
    crate::tool_runtime::startup_brief::validate_schema_instance_for_test(&attention, schema)
        .unwrap();
}

#[test]
fn passive_cargo_check_failure_is_actionable_bounded_and_strict() {
    let runtime = ToolRuntime::new_for_tests();
    let stderr = (0..7)
        .map(|i| {
            format!(
                "error[E0308]: expected X, found Y {i}\n --> src/foo.rs:{}:9\n",
                123 + i
            )
        })
        .collect::<String>();
    let item = runtime.passive_job_attention_item(&snapshot(
        &runtime,
        "cargo_check",
        "check",
        "PRIVATE_STDOUT",
        &stderr,
    ));
    let diagnostics = &item["validation"]["diagnostics"];
    assert_eq!(item["validation"]["passed"], false);
    assert_eq!(diagnostics["diagnostic_count"], 7);
    assert_eq!(diagnostics["returned_diagnostic_count"], 3);
    assert_eq!(diagnostics["diagnostics_truncated"], true);
    assert_eq!(diagnostics["diagnostics"][0]["code"], "E0308");
    assert_eq!(diagnostics["diagnostics"][0]["file"], "src/foo.rs");
    assert_eq!(diagnostics["diagnostics"][0]["line"], 123);
    assert_eq!(item["details"]["tool"], "observe_jobs");
    assert_schema(&item);
    let encoded = item.to_string();
    for secret in [
        "PRIVATE_STDOUT",
        "secret validation command",
        "secret assertion label",
        "stdout",
        "stderr",
        "cwd",
        "env",
        "observation_token",
        "args",
    ] {
        assert!(!encoded.contains(secret), "{secret}");
    }
}

#[test]
fn passive_failed_test_names_and_success_are_sparse() {
    let runtime = ToolRuntime::new_for_tests();
    let output = "test a::one ... FAILED\ntest a::two ... FAILED\ntest a::three ... FAILED\ntest a::four ... FAILED\ntest result: FAILED. 0 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out\n";
    let mut record = snapshot(&runtime, "cargo_test", "test", output, "");
    let item = runtime.passive_job_attention_item(&record);
    assert_eq!(
        item["validation"]["diagnostics"]["failed_test_details"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        item["validation"]["diagnostics"]["failed_test_details"][0]["name"],
        "a::one"
    );
    assert_eq!(
        item["validation"]["diagnostics"]["failed_test_details_truncated"],
        true
    );
    assert_schema(&item);
    record.job.exit_code = Some(0);
    record.validation_output.as_mut().unwrap().stdout =
        "test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n".into();
    let success = runtime.passive_job_attention_item(&record);
    assert_eq!(success["validation"]["passed"], true);
    assert!(success["validation"].get("diagnostics").is_none());
    assert_schema(&success);
}

#[test]
fn passive_failure_bytes_are_bounded_without_changing_identities() {
    use webcodex_core::validation_evidence::*;
    let diagnostics = json!({"available": true, "diagnostic_count": 3,
        "diagnostics": (0..3).map(|_| json!({"severity": "error", "message": "界".repeat(240), "file": "界".repeat(510), "line": 1})).collect::<Vec<_>>(),
        "returned_diagnostic_count": 3, "diagnostics_truncated": false,
        "failed_test_details": (0..3).map(|_| json!({"name": "界".repeat(240), "failure_kind": "unknown", "file": "界".repeat(510), "line": null, "column": null})).collect::<Vec<_>>(),
        "failed_test_details_truncated": false});
    let bounded = crate::tool_runtime::job_attention::bounded_failure_diagnostics(diagnostics);
    assert!(serde_json::to_vec(&bounded).unwrap().len() <= PASSIVE_MAX_FAILURE_BYTES);
    assert_eq!(bounded["diagnostics_truncated"], true);
}

#[test]
fn passive_test_proof_and_source_fence_remain_conservative() {
    let runtime = ToolRuntime::new_for_tests();
    for tool in ["cargo_test", "go_test"] {
        let mut record = snapshot(&runtime, tool, "test", "incomplete output", "");
        record.job.exit_code = Some(0);
        let item = runtime.passive_job_attention_item(&record);
        assert_eq!(item["command_ok"], true);
        assert!(item["validation"]["passed"].is_null());
        assert_schema(&item);
    }
    let mut record = snapshot(
        &runtime,
        "cargo_test",
        "test",
        "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out\n",
        "",
    );
    record.job.exit_code = Some(0);
    record.job.validation.as_mut().unwrap().minimum_tests = Some(1);
    record.job.validation.as_mut().unwrap().require_tests = Some(true);
    let zero = runtime.passive_job_attention_item(&record);
    assert_eq!(zero["validation"]["passed"], false);
    assert_eq!(
        zero["validation"]["test_count_assertion"]["reason_code"],
        "minimum_not_met"
    );
    record.validation_output.as_mut().unwrap().truncated = true;
    let incomplete = runtime.passive_job_attention_item(&record);
    assert_eq!(incomplete["validation"]["passed"], false);
    assert_eq!(
        incomplete["validation"]["test_count_assertion"]["status"],
        "unproven"
    );
    assert_schema(&zero);
    assert_schema(&incomplete);
    let mut record = snapshot(&runtime, "cargo_check", "check", "", "");
    record.job.exit_code = Some(0);
    runtime
        .validation_sources
        .begin(record.job.project_id.as_deref().unwrap())
        .unwrap()
        .finish(&ToolResult::ok(json!({"state_changed": true})));
    let stale = runtime.passive_job_attention_item(&record);
    assert_eq!(stale["validation"]["passed"], true);
    assert_eq!(stale["validation"]["source_state"]["freshness"], "stale");
    assert_schema(&stale);
}

#[test]
fn passive_go_fmt_and_declared_process_validation_keep_canonical_evidence() {
    let runtime = ToolRuntime::new_for_tests();
    let go = snapshot(
        &runtime,
        "go_test",
        "test",
        "{\"Action\":\"fail\",\"Package\":\"example/a\",\"Test\":\"TestBroken\"}\n",
        "PRIVATE_STDERR",
    );
    let item = runtime.passive_job_attention_item(&go);
    assert_eq!(
        item["validation"]["diagnostics"]["failed_test_details"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_schema(&item);
    let fmt = runtime.passive_job_attention_item(&snapshot(
        &runtime,
        "cargo_fmt",
        "format",
        "PRIVATE_DIFF",
        "",
    ));
    assert_eq!(fmt["validation"]["passed"], false);
    assert!(fmt["validation"].get("diagnostics").is_none());
    assert_schema(&fmt);
    for source in ["run_process", "run_shell"] {
        let mut record = snapshot(
            &runtime,
            "cargo_check",
            "check",
            "",
            "error[E0308]: expected X, found Y\n --> src/foo.rs:123:9\n",
        );
        record.job.validation = None;
        let metadata = record.job.structured_execution.as_mut().unwrap();
        metadata.execution_source = source.into();
        metadata.validation_tool = Some("cargo_check".into());
        let item = runtime.passive_job_attention_item(&record);
        assert_eq!(item["validation"]["passed"], false);
        assert_eq!(
            item["validation"]["diagnostics"]["diagnostics"][0]["file"],
            "src/foo.rs"
        );
        assert_schema(&item);
    }
}

#[tokio::test]
async fn passive_failure_reads_retained_server_evidence_without_runner_poll() {
    use crate::runner_http::{ShellJobStartMetadata, ShellJobVisibility};
    use crate::runner_protocol::ShellJobOpRequest;
    use crate::tool_runtime::tests::support::wait_for_runner_request_for_instance;
    use crate::tool_runtime::tests::validation_handoff::{cargo_test_update, running_progress};
    let runtime = ToolRuntime::new_for_tests();
    let auth = shared_key_auth_context("passive-validation-owner");
    let client = "passive-validation";
    register_job_agent_for_auth(&runtime, client, "repo", &auth).await;
    let project = format!("agent:{client}:repo");
    let session = runtime
        .sessions
        .start_session(Some(project.clone()), None)
        .session_id;
    let window = ClientWindow::for_test("passive-validation-window");
    let metadata = compact_validation_job(&project, runtime.validation_sources.capture(&project))
        .validation
        .unwrap();
    let job = runtime
        .runner_registry
        .start_job_with_metadata(
            ShellJobOpRequest {
                login: false,
                op: "start".into(),
                client_id: Some(client.into()),
                cwd: Some("/tmp/agent-proj".into()),
                command: Some("cargo check".into()),
                timeout_secs: Some(600),
                job_id: None,
                since_stdout_line: None,
                since_stderr_line: None,
                tail_lines: None,
                limit: None,
                codex: None,
            },
            "passive-test".into(),
            ShellJobStartMetadata {
                project_id: Some(project.clone()),
                session_id: Some(session.clone()),
                purpose: Some("validation".into()),
                validation_steps: metadata.steps.clone(),
                validation: Some(metadata),
                visibility: ShellJobVisibility::Public,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let request = wait_for_runner_request_for_instance(&runtime, client, "inst").await;
    let _ = attention(&runtime, &project, Some(&session), &window, &auth).await;
    let mut update = cargo_test_update(
        client,
        &request.request_id,
        &job.job_id,
        "failed",
        "PRIVATE_STDOUT",
        "error[E0308]: expected X, found Y\n --> src/foo.rs:123:9\n",
        Some(1),
        running_progress("check"),
        true,
    );
    update.update_seq = Some(1);
    update.validation_progress = Some(crate::runner_protocol::ShellJobValidationProgress {
        completed: 0,
        current_step: None,
        failed_step: Some("check".into()),
    });
    update.activity = None;
    runtime.runner_registry.update_job(update).await.unwrap();
    let result = attention(&runtime, &project, Some(&session), &window, &auth).await;
    let item = &result.output["job_attention"]["items"][0];
    assert_eq!(item["job_id"], job.job_id);
    assert_eq!(
        item["validation"]["diagnostics"]["diagnostics"][0]["line"], 123,
        "{item}"
    );
    assert_schema(item);
    assert!(probe_agent_request_for_instance(&runtime, client, "inst")
        .await
        .is_none());
}
