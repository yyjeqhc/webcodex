//! Phase D bounded batch Job observation.

mod summary;

use super::super::kernel::{HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport};
use super::super::*;
use super::support::*;
use crate::runner_protocol::{
    RunnerCapabilities, RunnerJobUpdateRequest, RunnerRequest, ShellJobActivity,
    ShellJobActivityPhase, ShellJobActivitySource, ShellJobActivityState,
};
use crate::tool_runtime::tool_audit::ToolCallAuditProjection;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn item(job_id: &str, token: Option<String>) -> ObserveJobsItem {
    ObserveJobsItem {
        job_id: job_id.to_string(),
        after_observation_token: token,
        observation_ref: None,
    }
}

fn item_ref(observation_ref: &str) -> ObserveJobsItem {
    ObserveJobsItem {
        job_id: String::new(),
        after_observation_token: None,
        observation_ref: Some(observation_ref.to_string()),
    }
}

async fn register_and_start_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
) -> (
    String,
    crate::runner_protocol::RunnerRequest,
    crate::auth::AuthContext,
) {
    let caps = RunnerCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        ..Default::default()
    };
    register_agent(runtime, client_id, None, caps).await;
    let auth = bootstrap_auth_context();
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: agent_test_project_id(client_id),
                command: format!("echo {client_id}"),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: Some(ExecutionPurpose::Diagnostic),
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let job_id = started.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_patch_agent_request(runtime, client_id).await;
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    (job_id, request, auth)
}

fn process_activity() -> ShellJobActivity {
    ShellJobActivity {
        state: ShellJobActivityState::Working,
        phase: ShellJobActivityPhase::ProcessRunning,
        source: ShellJobActivitySource::RunnerExecution,
    }
}

async fn update_observed_job(
    runtime: &ToolRuntime,
    client_id: &str,
    request: &RunnerRequest,
    status: &str,
    stdout_chunk: Option<&str>,
    activity: Option<ShellJobActivity>,
    finished: bool,
) {
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client_id.to_string(),
            runner_instance_id: "inst".to_string(),
            update_seq: None,
            job_id: request.job_id.clone().expect("Job request id"),
            request_id: Some(request.request_id.clone()),
            status: status.to_string(),
            stdout_chunk: stdout_chunk.map(str::to_string),
            stderr_chunk: None,
            log_snapshot: None,
            exit_code: finished.then_some(0),
            duration_ms: finished.then_some(25),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity,
            finished,
        })
        .await
        .unwrap();
}

async fn observation_token(
    runtime: &ToolRuntime,
    job_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    runtime
        .job_log_for_auth(job_id.to_string(), None, Some(40), Some(auth), None, None)
        .await
        .output["observation_token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn assert_item_has_no_wait_metadata(item: &serde_json::Value) {
    let output = item["output"].as_object().expect("successful Job snapshot");
    assert!(!output.contains_key("wait_outcome"));
    assert!(!output.contains_key("waited_ms"));
}

fn canonical_observation(
    job_id: &str,
    status: &str,
    log_delta_status: &str,
    stdout_tail: &str,
    stderr_tail: &str,
    changed: bool,
    terminal: bool,
) -> Value {
    json!({
        "job_id": job_id,
        "status": status,
        "exit_code": null,
        "command_execution_state": null,
        "structured_execution": null,
        "activity": null,
        "stdout_tail": stdout_tail,
        "stderr_tail": stderr_tail,
        "stdout_lines": 4,
        "stderr_lines": 2,
        "stdout_returned_lines": stdout_tail.lines().count(),
        "stderr_returned_lines": stderr_tail.lines().count(),
        "stdout_truncated": false,
        "stderr_truncated": false,
        "stdout_retained_from_line": null,
        "stderr_retained_from_line": null,
        "earlier_stdout_unavailable": false,
        "earlier_stderr_unavailable": false,
        "recovery_state": null,
        "recovery_reason_code": null,
        "recovery_reason": null,
        "observation_token": crate::job_observation::JobObservationToken::new_baseline(job_id, "fixture_epoch", 7).unwrap().encode(),
        "log_delta_status": log_delta_status,
        "stdout_delta_reset": false,
        "stderr_delta_reset": false,
        "last_update_seq": 7,
        "cursor": {"stdout": 5, "stderr": 3},
        "changed": changed,
        "terminal": terminal,
        "executor": "agent",
        "session_id": null,
        "ssh_resource": null,
        "cwd": ".",
        "shell": "direct_argv",
        "purpose": "build",
        "command_summary": "cargo check -p webcodex --lib",
        "detected_summary": {
            "kind": "check",
            "outcome": if terminal { "passed" } else { "in_progress" }
        },
        "validation": null
    })
}

fn canonical_success_item(index: usize, observation: Value) -> Value {
    let job_id = observation["job_id"].as_str().unwrap().to_string();
    json!({
        "index": index,
        "job_id": job_id,
        "success": true,
        "output": observation,
        "error_kind": null,
        "error": null
    })
}

fn canonical_batch(items: Vec<Value>, wait_outcome: &str, waited_ms: u64) -> ToolResult {
    let returned_count = items.len();
    let succeeded_count = items.iter().filter(|item| item["success"] == true).count();
    let changed_count = items
        .iter()
        .filter(|item| item["success"] == true && item["output"]["changed"] == true)
        .count();
    let terminal_count = items
        .iter()
        .filter(|item| item["success"] == true && item["output"]["terminal"] == true)
        .count();
    ToolResult::ok(json!({
        "requested_count": returned_count,
        "returned_count": returned_count,
        "succeeded_count": succeeded_count,
        "failed_count": returned_count - succeeded_count,
        "items": items,
        "wait": {"outcome": wait_outcome, "waited_ms": waited_ms},
        "changed_count": changed_count,
        "terminal_count": terminal_count,
        "output_truncated": false
    }))
}

fn compact_projection(canonical: &ToolResult) -> ToolResult {
    let mut projected = ToolResult {
        success: canonical.success,
        output: canonical.output.clone(),
        error: canonical.error.clone(),
    };
    super::super::observe_jobs::sparsify_observe_jobs_model_result(&mut projected);
    projected
}

fn serialized_result_bytes(result: &ToolResult) -> usize {
    serde_json::to_vec(result).unwrap().len()
}

async fn start_owned_agent_job(
    runtime: &ToolRuntime,
    client_id: &str,
    project_id: &str,
    auth: &crate::auth::AuthContext,
) -> String {
    let caps = RunnerCapabilities {
        async_jobs: true,
        async_shell_jobs: true,
        ..Default::default()
    };
    register_agent_projects_for_auth(
        runtime,
        client_id,
        auth,
        caps,
        vec![registered_project(
            project_id,
            &format!("/tmp/{project_id}"),
        )],
    )
    .await;
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: format!("agent:{client_id}:{project_id}"),
                command: format!("echo {client_id}"),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: None,
                shell: None,
            },
            Some(auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let job_id = started.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_runner_request_for_client(runtime, client_id).await;
    assert_eq!(request.job_id.as_deref(), Some(job_id.as_str()));
    job_id
}

#[test]
fn observe_jobs_tool_call_enforces_batch_and_scalar_bounds() {
    for count in [1, 8] {
        let items = (0..count)
            .map(|index| json!({"job_id": format!("job-{index}")}))
            .collect::<Vec<_>>();
        let parsed = ToolCall::from_tool_name("observe_jobs", json!({"items": items})).unwrap();
        assert!(matches!(
            parsed,
            ToolCall::ObserveJobs {
                tail_lines: 40,
                wait_secs: None,
                wake_on: ObserveJobsWakeOn::Change,

                summary_only: false,
                ..
            }
        ));
    }
    for count in [0, 9] {
        let items = (0..count)
            .map(|index| json!({"job_id": format!("job-{index}")}))
            .collect::<Vec<_>>();
        assert!(ToolCall::from_tool_name("observe_jobs", json!({"items": items})).is_err());
    }

    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job", "unknown": true}]})
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job"}], "unknown": true})
    )
    .is_err());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "same"}, {"job_id": "same"}]})
    )
    .is_err());

    let max_token = "x".repeat(crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN);
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({"items": [{"job_id": "job", "after_observation_token": max_token}]})
    )
    .is_ok());
    assert!(ToolCall::from_tool_name(
        "observe_jobs",
        json!({
            "items": [{
                "job_id": "job",
                "after_observation_token": "x".repeat(
                    crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN + 1
                )
            }]
        })
    )
    .is_err());

    for tail_lines in [1, 200, 201, 500] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "tail_lines": tail_lines})
        )
        .is_ok());
    }
    for tail_lines in [0] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "tail_lines": tail_lines})
        )
        .is_err());
    }
    for wait_secs in [1, 100, 101, 120] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "wait_secs": wait_secs})
        )
        .is_ok());
    }
    for wait_secs in [0] {
        assert!(ToolCall::from_tool_name(
            "observe_jobs",
            json!({"items": [{"job_id": "job"}], "wait_secs": wait_secs})
        )
        .is_err());
    }
}

#[tokio::test]
async fn observe_jobs_direct_dispatch_rejects_duplicates_before_observation() {
    let runtime = test_runtime();
    let started = Instant::now();
    let result = runtime
        .dispatch(ToolCall::ObserveJobs {
            items: vec![item("duplicate", None), item("duplicate", None)],
            tail_lines: 40,
            wait_secs: Some(60),
            wake_on: Default::default(),

            summary_only: false,
        })
        .await;
    assert!(!result.success);
    assert!(result.error.unwrap().contains("duplicate"));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn observe_jobs_schema_catalog_permission_and_audit_are_public_and_token_safe() {
    let specs = registered_tool_specs();
    let spec = specs
        .iter()
        .find(|spec| spec.name == "observe_jobs")
        .expect("model-visible observe_jobs ToolSpec");
    assert_eq!(spec.input_schema["properties"]["items"]["minItems"], 1);
    assert_eq!(spec.input_schema["properties"]["items"]["maxItems"], 8);
    assert_eq!(spec.input_schema["properties"]["tail_lines"]["default"], 40);
    assert!(spec.input_schema["properties"]["tail_lines"]
        .get("maximum")
        .is_none());
    assert!(spec.input_schema["properties"]["wait_secs"]
        .get("maximum")
        .is_none());
    let selector_branches = spec.input_schema["properties"]["items"]["items"]["oneOf"]
        .as_array()
        .expect("observe_jobs selectors must be a closed oneOf");
    assert_eq!(selector_branches.len(), 2);
    assert_eq!(selector_branches[0]["required"], json!(["job_id"]));
    assert_eq!(selector_branches[1]["required"], json!(["observation_ref"]));
    assert_eq!(selector_branches[0]["additionalProperties"], false);
    assert_eq!(selector_branches[1]["additionalProperties"], false);
    let output = &spec.output_schema["properties"]["output"]["anyOf"][0];
    let sparse_output = &spec.output_schema["properties"]["output"]["anyOf"][1];
    assert_eq!(
        output["properties"]["wait"]["properties"]["outcome"]["enum"],
        json!(["immediate", "updated", "terminal", "item_error", "timeout"])
    );
    assert!(output["properties"].get("wake_reason").is_none());
    assert!(output["properties"].get("waited_ms").is_none());
    let observation = &output["properties"]["items"]["items"]["properties"]["output"]["anyOf"][0];
    assert!(observation["properties"].get("wait_outcome").is_none());
    assert!(observation["properties"].get("waited_ms").is_none());
    assert_eq!(
        observation["properties"]["log_delta_status"]["enum"],
        json!(["baseline", "delta", "unchanged", "reset"])
    );
    assert_eq!(
        observation["properties"]["observation_token"]["maxLength"],
        crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
    );
    assert!(
        observation["properties"].get("observation_ref").is_none(),
        "observation_ref belongs to the batch item, not the canonical Job observation body"
    );
    assert_eq!(
        output["properties"]["items"]["items"]["properties"]["observation_ref"]["maxLength"],
        crate::job_observation::MAX_OBSERVATION_REF_LEN
    );
    for required in [
        "activity",
        "log_delta_status",
        "stdout_delta_reset",
        "stderr_delta_reset",
    ] {
        assert!(
            observation["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|field| field == required),
            "observe_jobs item output must require {required}"
        );
    }
    assert!(observation["properties"]["activity"]["anyOf"]
        .as_array()
        .unwrap()
        .iter()
        .any(|schema| schema["type"] == "null"));
    let sparse_observation = &sparse_output["properties"]["items"]["items"];
    assert_eq!(
        sparse_observation["properties"]["observation_ref"]["maxLength"],
        crate::job_observation::MAX_OBSERVATION_REF_LEN
    );
    for required in [
        "job_id",
        "status",
        "terminal",
        "changed",
        "log_delta_status",
        "observation_token",
    ] {
        assert!(sparse_observation["required"]
            .as_array()
            .unwrap()
            .iter()
            .any(|field| field == required));
    }
    assert_eq!(
        sparse_observation["properties"]["observation_token"]["maxLength"],
        crate::job_observation::MAX_JOB_OBSERVATION_TOKEN_LEN
    );
    assert_eq!(
        sparse_output["properties"]["wait"]["required"],
        json!(["outcome"])
    );

    let definition = super::super::tool_definition::lookup_tool_definition("observe_jobs").unwrap();
    assert!(definition.visibility.is_model_visible());
    assert_eq!(definition.category, "job");
    assert_eq!(
        definition.metadata.authority,
        crate::tool_runtime::metadata::ToolAuthorityPolicy::Require(
            crate::auth::SCOPE_RUNTIME_READ
        )
    );
    assert_eq!(
        definition.metadata.effect,
        crate::tool_runtime::metadata::ToolEffect::Observe
    );
    let manifest = super::super::surface::registered_tool_categories();
    assert!(manifest["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "observe_jobs"));

    let opaque = "wj3_privateepochbody.7.0.0";
    let call = ToolCall::ObserveJobs {
        items: vec![item("job", Some(opaque.to_string()))],
        tail_lines: 40,
        wait_secs: Some(5),
        wake_on: Default::default(),

        summary_only: false,
    };
    let summary = call.session_log_arguments();
    assert_eq!(summary["item_count"], 1);
    assert_eq!(summary["token_count"], 1);
    assert_eq!(summary["observation_ref_count"], 0);
    assert_eq!(summary["job_ids"], json!(["job"]));
    assert_eq!(summary["tail_lines"], 40);
    assert_eq!(summary["wait_secs"], 5);
    assert!(!serde_json::to_string(&summary).unwrap().contains(opaque));

    let raw_summary = super::super::tool_audit::session_log_arguments_for_tool_request(
        "observe_jobs",
        &json!({
            "items": [{"job_id": "job", "after_observation_token": opaque}],
            "tail_lines": 40,
            "wait_secs": 5
        }),
    );
    assert_eq!(raw_summary["token_count"], 1);
    assert_eq!(raw_summary["observation_ref_count"], 0);
    assert!(!serde_json::to_string(&raw_summary)
        .unwrap()
        .contains(opaque));
    let defensive = super::super::sessions::session_input_summary_for_tool(
        "observe_jobs",
        &json!({
            "items": [{"job_id": "job", "after_observation_token": opaque}],
            "tail_lines": 40,
            "wait_secs": 5
        }),
    );
    assert!(!serde_json::to_string(&defensive).unwrap().contains(opaque));
}

#[test]
fn observe_jobs_compact_projection_single_running_unchanged_keeps_actionable_state() {
    let mut observation = canonical_observation(
        "job-unchanged",
        "running",
        "unchanged",
        "",
        "",
        false,
        false,
    );
    observation["activity"] = serde_json::to_value(process_activity()).unwrap();
    let token = observation["observation_token"].clone();
    let canonical = canonical_batch(vec![canonical_success_item(0, observation)], "immediate", 0);
    assert_eq!(
        super::super::tool_audit::session_log_result_for_tool("observe_jobs", &canonical.output),
        canonical.output,
        "Session/audit must consume the full canonical observe_jobs result"
    );

    let projected = compact_projection(&canonical);
    let output = projected.output.as_object().unwrap();
    for omitted in [
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "changed_count",
        "terminal_count",
        "output_truncated",
        "next_index",
    ] {
        assert!(output.get(omitted).is_none(), "mechanical {omitted} leaked");
    }
    assert_eq!(projected.output["wait"]["outcome"], "immediate");
    assert!(projected.output["wait"].get("waited_ms").is_none());
    let item = &projected.output["items"][0];
    assert_eq!(item["job_id"], "job-unchanged");
    assert_eq!(item["status"], "running");
    assert_eq!(item["terminal"], false);
    assert_eq!(item["changed"], false);
    assert_eq!(item["log_delta_status"], "unchanged");
    assert_eq!(item["observation_token"], token);
    assert!(item.get("continuation_semantics").is_none());
    assert!(projected.output.get("continuation_semantics").is_none());
    assert_eq!(
        item["activity"],
        serde_json::to_value(process_activity()).unwrap()
    );
    for omitted in [
        "index",
        "success",
        "output",
        "error_kind",
        "error",
        "executor",
        "cursor",
        "last_update_seq",
        "stdout_lines",
        "stderr_lines",
        "stdout_tail",
        "stderr_tail",
    ] {
        assert!(item.get(omitted).is_none(), "mechanical {omitted} leaked");
    }
}

#[tokio::test]
async fn observe_jobs_kernel_model_surface_applies_compact_projection() {
    let runtime = test_runtime();
    let (job_id, request, auth) =
        register_and_start_agent_job(&runtime, "observe-kernel-projection").await;
    update_observed_job(
        &runtime,
        "observe-kernel-projection",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;

    let outcome = runtime
        .call_tool_with_context(
            ToolCallRequest {
                tool_name: "observe_jobs".to_string(),
                arguments: json!({
                    "items": [{"job_id": job_id}],
                    "tail_lines": 40
                }),
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
    let result = outcome
        .result
        .expect("model-facing observe_jobs ToolResult");
    assert!(result.success, "{:?}", result.error);

    let output = result
        .output
        .as_object()
        .expect("compact observe_jobs output");
    for omitted in [
        "requested_count",
        "returned_count",
        "succeeded_count",
        "failed_count",
        "changed_count",
        "terminal_count",
        "output_truncated",
        "next_index",
    ] {
        assert!(output.get(omitted).is_none(), "kernel leaked {omitted}");
    }
    assert_eq!(result.output["wait"]["outcome"], "immediate");
    assert!(result.output["wait"].get("waited_ms").is_none());

    let item = &result.output["items"][0];
    assert_eq!(item["job_id"], job_id);
    assert_eq!(item["status"], "running");
    assert_eq!(item["terminal"], false);
    assert_eq!(item["log_delta_status"], "baseline");
    assert!(item["observation_token"]
        .as_str()
        .is_some_and(|token| !token.is_empty()));
    assert_eq!(
        item["activity"],
        serde_json::to_value(process_activity()).unwrap()
    );
    assert!(item.get("output").is_none());
    assert!(item.get("success").is_none());

    let schema = super::super::registry::output_schema_for_tool("observe_jobs");
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        super::super::startup_brief::validate_schema_instance_for_test(&value, &schema).is_ok(),
        "kernel-projected observe_jobs result did not satisfy output schema: {value}"
    );
}

#[test]
fn observe_jobs_compact_projection_delta_keeps_bodies_and_token() {
    let observation = canonical_observation(
        "job-delta",
        "running",
        "delta",
        "new stdout\n",
        "new stderr\n",
        true,
        false,
    );
    let token = observation["observation_token"].clone();
    let canonical = canonical_batch(vec![canonical_success_item(0, observation)], "updated", 84);
    let projected = compact_projection(&canonical);
    let item = &projected.output["items"][0];
    assert_eq!(item["log_delta_status"], "delta");
    assert_eq!(item["stdout_tail"], "new stdout\n");
    assert_eq!(item["stderr_tail"], "new stderr\n");
    assert_eq!(item["observation_token"], token);
    assert_eq!(projected.output["wait"]["outcome"], "updated");
    assert_eq!(projected.output["wait"]["waited_ms"], 84);
    assert!(item.get("stdout_lines").is_none());
    assert!(item.get("cursor").is_none());
}

#[test]
fn observe_jobs_compact_projection_terminal_keeps_validation_evidence() {
    let mut observation = canonical_observation(
        "job-terminal",
        "completed",
        "delta",
        "test result: ok\n",
        "",
        true,
        true,
    );
    observation["exit_code"] = json!(0);
    observation["command_execution_state"] = json!("completed");
    observation["detected_summary"] = json!({
        "kind": "test",
        "outcome": "passed",
        "tests_passed": 28,
        "tests_failed": 0
    });
    observation["validation"] = json!({
        "tool": "cargo_test",
        "kind": "test",
        "state": "completed",
        "passed": true,
        "truncated": false
    });
    let token = observation["observation_token"].clone();
    let canonical = canonical_batch(vec![canonical_success_item(0, observation)], "terminal", 91);
    let projected = compact_projection(&canonical);
    let item = &projected.output["items"][0];
    assert_eq!(item["terminal"], true);
    assert_eq!(item["status"], "completed");
    assert_eq!(item["exit_code"], 0);
    assert_eq!(item["command_execution_state"], "completed");
    assert_eq!(item["detected_summary"]["outcome"], "passed");
    assert_eq!(item["validation"]["passed"], true);
    assert_eq!(item["observation_token"], token);
}

#[test]
fn observe_jobs_compact_projection_reset_keeps_recovery_and_loss_evidence() {
    let mut observation = canonical_observation(
        "job-reset",
        "running",
        "reset",
        "bounded recovery stdout\n",
        "bounded recovery stderr\n",
        true,
        false,
    );
    observation["stdout_delta_reset"] = json!(true);
    observation["stderr_delta_reset"] = json!(true);
    observation["stderr_truncated"] = json!(true);
    observation["earlier_stdout_unavailable"] = json!(true);
    observation["stdout_retained_from_line"] = json!(41);
    observation["stderr_retained_from_line"] = json!(22);
    observation["recovery_state"] = json!("recovered");
    observation["recovery_reason_code"] = json!("server_epoch_changed");
    observation["recovery_reason"] = json!("Exact delta history was unavailable.");
    let token = observation["observation_token"].clone();
    let canonical = canonical_batch(vec![canonical_success_item(0, observation)], "updated", 12);
    let projected = compact_projection(&canonical);
    let item = &projected.output["items"][0];
    assert_eq!(item["log_delta_status"], "reset");
    assert_eq!(item["stdout_delta_reset"], true);
    assert_eq!(item["stderr_delta_reset"], true);
    assert_eq!(item["stderr_truncated"], true);
    assert_eq!(item["earlier_stdout_unavailable"], true);
    assert_eq!(item["stdout_retained_from_line"], 41);
    assert_eq!(item["cursor"], json!({"stdout": 5, "stderr": 3}));
    assert_eq!(item["recovery_reason_code"], "server_epoch_changed");
    assert_eq!(item["observation_token"], token);
    assert_eq!(item["command_summary"], "cargo check -p webcodex --lib");
    assert_eq!(item["purpose"], "build");
}

#[test]
fn observe_jobs_compact_projection_multi_success_preserves_order_and_one_wait() {
    let items = (0..4)
        .map(|index| {
            canonical_success_item(
                index,
                canonical_observation(
                    &format!("job-{index}"),
                    "running",
                    "unchanged",
                    "",
                    "",
                    false,
                    false,
                ),
            )
        })
        .collect();
    let canonical = canonical_batch(items, "timeout", 1_002);
    let projected = compact_projection(&canonical);
    let items = projected.output["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert_eq!(
        items
            .iter()
            .map(|item| item["job_id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["job-0", "job-1", "job-2", "job-3"]
    );
    assert_eq!(projected.output["wait"]["outcome"], "timeout");
    assert_eq!(projected.output["wait"]["waited_ms"], 1_002);
    assert!(projected.output.get("terminal_count").is_none());
}

#[test]
fn observe_jobs_compact_projection_preserves_mixed_failure_and_budget_recovery() {
    let success = canonical_success_item(
        0,
        canonical_observation("job-ok", "running", "unchanged", "", "", false, false),
    );
    let failure = json!({
        "index": 1,
        "job_id": "missing-job",
        "success": false,
        "output": null,
        "error_kind": "unknown_job",
        "suggested_call": {"tool": "list_jobs", "arguments": {}},
        "error": "unknown job: missing-job"
    });
    let mixed = canonical_batch(vec![success.clone(), failure], "item_error", 0);
    assert_eq!(
        serde_json::to_value(compact_projection(&mixed)).unwrap(),
        serde_json::to_value(&mixed).unwrap()
    );
    assert_eq!(
        mixed.output["items"][1]["suggested_call"],
        json!({"tool": "list_jobs", "arguments": {}})
    );

    let mut truncated = canonical_batch(vec![success], "immediate", 0);
    truncated.output["output_truncated"] = json!(true);
    truncated.output["next_index"] = json!(1);
    truncated.output["continuation_semantics"] = json!({
        "kind": "batch",
        "carrier": "index"
    });
    assert_eq!(
        serde_json::to_value(compact_projection(&truncated)).unwrap(),
        serde_json::to_value(&truncated).unwrap()
    );
    assert_eq!(truncated.output["next_index"], 1);
    assert_eq!(truncated.output["continuation_semantics"]["kind"], "batch");
    assert_eq!(
        truncated.output["continuation_semantics"]["carrier"],
        "index"
    );
}

#[test]
fn observe_jobs_canonical_and_compact_success_both_match_output_schema() {
    let canonical = canonical_batch(
        vec![canonical_success_item(
            0,
            canonical_observation("job-schema", "running", "unchanged", "", "", false, false),
        )],
        "immediate",
        0,
    );
    let projected = compact_projection(&canonical);
    let schema = super::super::registry::output_schema_for_tool("observe_jobs");
    for (label, result) in [("canonical", canonical), ("compact", projected)] {
        let value = serde_json::to_value(&result).unwrap();
        assert!(
            super::super::startup_brief::validate_schema_instance_for_test(&value, &schema).is_ok(),
            "{label} observe_jobs result did not satisfy output schema: {value}"
        );
    }
}

#[test]
fn observe_jobs_projection_reports_deterministic_byte_measurements() {
    let unchanged = canonical_batch(
        vec![canonical_success_item(
            0,
            canonical_observation(
                "job-measure-u",
                "running",
                "unchanged",
                "",
                "",
                false,
                false,
            ),
        )],
        "immediate",
        0,
    );
    let delta = canonical_batch(
        vec![canonical_success_item(
            0,
            canonical_observation(
                "job-measure-d",
                "running",
                "delta",
                &format!("{}\n", "stdout".repeat(20)),
                &format!("{}\n", "stderr".repeat(6)),
                true,
                false,
            ),
        )],
        "updated",
        75,
    );
    let mut terminal_observation = canonical_observation(
        "job-measure-t",
        "completed",
        "delta",
        "28 passed\n",
        "",
        true,
        true,
    );
    terminal_observation["exit_code"] = json!(0);
    terminal_observation["validation"] = json!({
        "tool": "cargo_test", "kind": "test", "state": "completed",
        "passed": true, "truncated": false, "tests_passed": 28, "tests_failed": 0
    });
    let terminal = canonical_batch(
        vec![canonical_success_item(0, terminal_observation)],
        "terminal",
        88,
    );
    let four = canonical_batch(
        (0..4)
            .map(|index| {
                canonical_success_item(
                    index,
                    canonical_observation(
                        &format!("job-measure-{index}"),
                        "running",
                        "unchanged",
                        "",
                        "",
                        false,
                        false,
                    ),
                )
            })
            .collect(),
        "timeout",
        1_000,
    );
    let mixed = canonical_batch(
        vec![
            canonical_success_item(
                0,
                canonical_observation(
                    "job-measure-ok",
                    "running",
                    "unchanged",
                    "",
                    "",
                    false,
                    false,
                ),
            ),
            json!({
                "index": 1, "job_id": "missing-measure", "success": false,
                "output": null, "error_kind": "unknown_job",
                "suggested_call": {"tool": "list_jobs", "arguments": {}},
                "error": "unknown job: missing-measure"
            }),
        ],
        "item_error",
        0,
    );

    for (name, canonical) in [
        ("one_running_unchanged", unchanged),
        ("one_delta", delta),
        ("one_terminal_validation", terminal),
        ("four_running", four),
        ("mixed_success_unknown", mixed),
    ] {
        let projected = compact_projection(&canonical);
        let canonical_bytes = serialized_result_bytes(&canonical);
        let projected_bytes = serialized_result_bytes(&projected);
        let reduction = if canonical_bytes == 0 {
            0.0
        } else {
            100.0 * (canonical_bytes.saturating_sub(projected_bytes)) as f64
                / canonical_bytes as f64
        };
        println!(
            "OBSERVE_JOBS_PROJECTION_BYTES {name} canonical={canonical_bytes} projected={projected_bytes} reduction_pct={reduction:.1}"
        );
        assert!(projected_bytes <= canonical_bytes);
        if name != "mixed_success_unknown" {
            assert!(projected_bytes < canonical_bytes);
        } else {
            assert_eq!(
                serde_json::to_value(&projected).unwrap(),
                serde_json::to_value(&canonical).unwrap()
            );
        }
    }
}

#[tokio::test]
async fn observe_jobs_inaccessible_and_unknown_items_are_indistinguishable() {
    let runtime = test_runtime();
    let auth_a = shared_key_auth_context("observe-owner-a");
    let auth_b = shared_key_auth_context("observe-owner-b");
    let job_a = start_owned_agent_job(&runtime, "observe-owner-a", "project-a", &auth_a).await;
    let job_b = start_owned_agent_job(&runtime, "observe-owner-b", "project-b", &auth_b).await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&job_b, None),
                    item(&job_a, None),
                    item("missing-owned-job", None),
                ],
                tail_lines: 40,
                wait_secs: Some(5),
                wake_on: Default::default(),

                summary_only: false,
            },
            Some(&auth_b),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["items"][0]["success"], true);
    for index in [1, 2] {
        assert_eq!(result.output["items"][index]["success"], false);
        assert_eq!(result.output["items"][index]["error_kind"], "unknown_job");
        let error = result.output["items"][index]["error"].as_str().unwrap();
        assert!(error.starts_with("unknown job:"));
        assert!(!error.contains("authorization"));
        assert!(!error.contains("owner"));
        assert!(!error.contains("project-"));
    }
}

#[tokio::test]
async fn observe_jobs_compact_ref_roundtrips_and_survives_model_projection() {
    let runtime = test_runtime();
    let (job_id, request, auth) =
        register_and_start_agent_job(&runtime, "observe-ref-roundtrip").await;
    update_observed_job(
        &runtime,
        "observe-ref-roundtrip",
        &request,
        "running",
        Some("first line\n"),
        Some(process_activity()),
        false,
    )
    .await;

    let first = runtime
        .observe_jobs_for_auth(
            vec![item(&job_id, None)],
            40,
            None,
            ObserveJobsWakeOn::Change,
            Some(&auth),
        )
        .await;
    assert!(first.success, "{first:?}");
    let first_ref = first.output["items"][0]["observation_ref"]
        .as_str()
        .expect("successful observation must mint compact ref")
        .to_string();
    assert!(first_ref.starts_with("~j"));
    assert_eq!(first.output["items"][0]["job_id"], job_id);
    assert!(first.output["items"][0]["output"]["observation_token"]
        .as_str()
        .is_some());

    let projected = compact_projection(&first);
    assert_eq!(projected.output["items"][0]["observation_ref"], first_ref);

    let second = runtime
        .observe_jobs_for_auth(
            vec![item_ref(&first_ref)],
            40,
            None,
            ObserveJobsWakeOn::Change,
            Some(&auth),
        )
        .await;
    assert!(second.success, "{second:?}");
    assert_eq!(second.output["items"][0]["success"], true);
    assert_eq!(second.output["items"][0]["job_id"], job_id);
    let second_ref = second.output["items"][0]["observation_ref"]
        .as_str()
        .expect("follow-up observation must mint a fresh ref");
    assert_ne!(second_ref, first_ref);
}

#[tokio::test]
async fn observe_jobs_rejects_raw_and_ref_alias_of_same_job_after_resolution() {
    let runtime = test_runtime();
    let (job_id, request, auth) =
        register_and_start_agent_job(&runtime, "observe-ref-duplicate").await;
    update_observed_job(
        &runtime,
        "observe-ref-duplicate",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;

    let baseline = runtime
        .observe_jobs_for_auth(
            vec![item(&job_id, None)],
            40,
            None,
            ObserveJobsWakeOn::Change,
            Some(&auth),
        )
        .await;
    assert!(baseline.success, "{baseline:?}");
    let observation_ref = baseline.output["items"][0]["observation_ref"]
        .as_str()
        .expect("baseline compact ref");

    let duplicate = runtime
        .observe_jobs_for_auth(
            vec![item(&job_id, None), item_ref(observation_ref)],
            40,
            None,
            ObserveJobsWakeOn::Change,
            Some(&auth),
        )
        .await;
    assert!(
        !duplicate.success,
        "resolved duplicate Job must fail closed"
    );
    assert!(duplicate
        .error
        .as_deref()
        .unwrap_or_default()
        .contains("duplicate"));
}

#[tokio::test]
async fn observe_jobs_unknown_ref_is_item_isolated_from_valid_ref() {
    let runtime = test_runtime();
    let (job_id, request, auth) =
        register_and_start_agent_job(&runtime, "observe-ref-isolation").await;
    update_observed_job(
        &runtime,
        "observe-ref-isolation",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;

    let baseline = runtime
        .observe_jobs_for_auth(
            vec![item(&job_id, None)],
            40,
            None,
            ObserveJobsWakeOn::Change,
            Some(&auth),
        )
        .await;
    assert!(baseline.success, "{baseline:?}");
    let valid_ref = baseline.output["items"][0]["observation_ref"]
        .as_str()
        .expect("baseline compact ref")
        .to_string();

    let result = runtime
        .observe_jobs_for_auth(
            vec![item_ref("~j999999999"), item_ref(&valid_ref)],
            40,
            Some(55),
            ObserveJobsWakeOn::AllTerminal,
            Some(&auth),
        )
        .await;
    assert!(
        result.success,
        "one expired ref must not fail the whole batch: {result:?}"
    );
    assert_eq!(result.output["requested_count"], 2);
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 1);
    assert_eq!(result.output["wait"]["outcome"], "item_error");
    assert_eq!(result.output["wait"]["waited_ms"], 0);

    let failed = &result.output["items"][0];
    assert_eq!(failed["success"], false);
    assert!(failed["job_id"].is_null());
    assert_eq!(failed["observation_ref"], "~j999999999");
    assert_eq!(failed["error_kind"], "unknown_observation_ref");
    assert_eq!(failed["recovery_kind"], "fix_input");
    assert!(failed.get("suggested_call").is_none());

    let succeeded = &result.output["items"][1];
    assert_eq!(succeeded["success"], true);
    assert_eq!(succeeded["job_id"], job_id);
    assert!(succeeded["observation_ref"].as_str().is_some());

    let schema = super::super::registry::output_schema_for_tool("observe_jobs");
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        super::super::startup_brief::validate_schema_instance_for_test(&value, &schema).is_ok(),
        "mixed compact-ref result did not satisfy output schema: {value}"
    );
}

#[tokio::test]
async fn observe_jobs_mixed_success_result_matches_declared_output_schema_and_enqueues_nothing() {
    let runtime = test_runtime();
    let (agent_job, request, auth) =
        register_and_start_agent_job(&runtime, "observe-no-enqueue").await;
    let initial = runtime
        .job_log_for_auth(agent_job.clone(), None, Some(40), Some(&auth), None, None)
        .await;
    let token = initial.output["observation_token"]
        .as_str()
        .unwrap()
        .to_string();
    update_observed_job(
        &runtime,
        "observe-no-enqueue",
        &request,
        "completed",
        None,
        None,
        true,
    )
    .await;
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&agent_job, Some(token)),
                    item("unknown-observe-job", Some("malformed".to_string())),
                ],
                tail_lines: 40,
                wait_secs: Some(5),
                wake_on: ObserveJobsWakeOn::Terminal,

                summary_only: false,
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["succeeded_count"], 1);
    assert_eq!(result.output["failed_count"], 1);
    assert_eq!(result.output["wait"]["outcome"], "item_error");
    assert_eq!(result.output["wait"]["waited_ms"], 0);
    assert_item_has_no_wait_metadata(&result.output["items"][0]);
    assert!(probe_patch_agent_request(&runtime, "observe-no-enqueue")
        .await
        .is_none());

    let schema = super::super::registry::output_schema_for_tool("observe_jobs");
    let value = serde_json::to_value(&result).unwrap();
    assert!(
        super::super::startup_brief::validate_schema_instance_for_test(&value, &schema).is_ok(),
        "mixed observe_jobs result did not satisfy output schema: {value}"
    );
}

#[tokio::test]
async fn observe_jobs_missing_baseline_is_immediate_and_projects_activity_without_item_wait() {
    let runtime = test_runtime();
    let (job_id, request, auth) = register_and_start_agent_job(&runtime, "observe-immediate").await;
    update_observed_job(
        &runtime,
        "observe-immediate",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;

    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![item(&job_id, None)],
                tail_lines: 40,
                wait_secs: Some(60),
                wake_on: ObserveJobsWakeOn::Terminal,

                summary_only: false,
            },
            Some(&auth),
        )
        .await;
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "immediate");
    assert_eq!(result.output["wait"]["waited_ms"], 0);
    assert_eq!(result.output["items"][0]["output"]["status"], "running");
    assert_eq!(
        result.output["items"][0]["output"]["activity"],
        serde_json::to_value(process_activity()).unwrap()
    );
    assert_item_has_no_wait_metadata(&result.output["items"][0]);
}

#[tokio::test]
async fn observe_jobs_timeout_waits_once_for_multiple_active_jobs() {
    let runtime = test_runtime();
    let (job_a, request_a, auth) =
        register_and_start_agent_job(&runtime, "observe-timeout-a").await;
    let (job_b, request_b, _) = register_and_start_agent_job(&runtime, "observe-timeout-b").await;
    let (job_c, request_c, _) = register_and_start_agent_job(&runtime, "observe-timeout-c").await;
    for (client, request) in [
        ("observe-timeout-a", &request_a),
        ("observe-timeout-b", &request_b),
        ("observe-timeout-c", &request_c),
    ] {
        update_observed_job(
            &runtime,
            client,
            request,
            "running",
            None,
            Some(process_activity()),
            false,
        )
        .await;
    }
    let token_a = observation_token(&runtime, &job_a, &auth).await;
    let token_b = observation_token(&runtime, &job_b, &auth).await;
    let token_c = observation_token(&runtime, &job_c, &auth).await;

    let started = Instant::now();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![
                    item(&job_a, Some(token_a)),
                    item(&job_b, Some(token_b)),
                    item(&job_c, Some(token_c)),
                ],
                tail_lines: 40,
                wait_secs: Some(1),
                wake_on: Default::default(),

                summary_only: false,
            },
            Some(&auth),
        )
        .await;
    let elapsed = started.elapsed();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "timeout");
    assert!(result.output["wait"]["waited_ms"].as_u64().unwrap() > 0);
    assert!(
        elapsed < Duration::from_millis(2500),
        "three Jobs must share one ~1s wait instead of multiplying it: {elapsed:?}"
    );
    assert_eq!(result.output["items"].as_array().unwrap().len(), 3);
    for item in result.output["items"].as_array().unwrap() {
        assert_eq!(item["output"]["status"], "running");
        assert_item_has_no_wait_metadata(item);
    }
}

#[tokio::test]
async fn observe_jobs_one_item_update_wakes_shared_wait_and_refreshes_all_snapshots() {
    let runtime = test_runtime();
    let (job_a, request_a, auth) = register_and_start_agent_job(&runtime, "observe-update-a").await;
    let (job_b, request_b, _) = register_and_start_agent_job(&runtime, "observe-update-b").await;
    update_observed_job(
        &runtime,
        "observe-update-a",
        &request_a,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    update_observed_job(
        &runtime,
        "observe-update-b",
        &request_b,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    let token_a = observation_token(&runtime, &job_a, &auth).await;
    let token_b = observation_token(&runtime, &job_b, &auth).await;

    let waiting_runtime = runtime.clone();
    let waiting_auth = auth.clone();
    let waiting_a = job_a.clone();
    let waiting_b = job_b.clone();
    let task = tokio::spawn(async move {
        waiting_runtime
            .dispatch_with_auth(
                ToolCall::ObserveJobs {
                    items: vec![
                        item(&waiting_a, Some(token_a)),
                        item(&waiting_b, Some(token_b)),
                    ],
                    tail_lines: 40,
                    wait_secs: Some(5),
                    wake_on: Default::default(),

                    summary_only: false,
                },
                Some(&waiting_auth),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(75)).await;
    update_observed_job(
        &runtime,
        "observe-update-b",
        &request_b,
        "running",
        Some("second changed\n"),
        Some(process_activity()),
        false,
    )
    .await;

    let result = task.await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "updated");
    assert!(result.output["wait"]["waited_ms"].as_u64().unwrap() < 5_000);
    assert_eq!(result.output["items"][0]["output"]["status"], "running");
    assert_eq!(result.output["items"][1]["output"]["status"], "running");
    assert_eq!(result.output["items"][1]["output"]["changed"], true);
    assert!(result.output["items"][1]["output"]["stdout_tail"]
        .as_str()
        .unwrap()
        .contains("second changed"));
    for item in result.output["items"].as_array().unwrap() {
        assert_item_has_no_wait_metadata(item);
    }
}

#[tokio::test]
async fn observe_jobs_terminal_transition_wakes_shared_wait() {
    let runtime = test_runtime();
    let (job_id, request, auth) = register_and_start_agent_job(&runtime, "observe-terminal").await;
    update_observed_job(
        &runtime,
        "observe-terminal",
        &request,
        "running",
        None,
        Some(process_activity()),
        false,
    )
    .await;
    let (other_job, _, _) = register_and_start_agent_job(&runtime, "observe-terminal-other").await;
    let other_token = observation_token(&runtime, &other_job, &auth).await;
    let token = observation_token(&runtime, &job_id, &auth).await;

    let waiting_runtime = runtime.clone();
    let waiting_auth = auth.clone();
    let waiting_job = job_id.clone();
    let task = tokio::spawn(async move {
        waiting_runtime
            .dispatch_with_auth(
                ToolCall::ObserveJobs {
                    items: vec![
                        item(&other_job, Some(other_token)),
                        item(&waiting_job, Some(token)),
                    ],
                    tail_lines: 40,
                    wait_secs: Some(100),
                    wake_on: ObserveJobsWakeOn::Terminal,

                    summary_only: false,
                },
                Some(&waiting_auth),
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(75)).await;
    update_observed_job(
        &runtime,
        "observe-terminal",
        &request,
        "completed",
        None,
        None,
        true,
    )
    .await;

    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("terminal must wake well before the 100-second maximum")
        .unwrap();
    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["wait"]["outcome"], "terminal");
    assert_eq!(result.output["terminal_count"], 1);
    assert_eq!(result.output["items"][1]["output"]["terminal"], true);
    assert!(result.output["items"][1]["output"]["activity"].is_null());
    assert_item_has_no_wait_metadata(&result.output["items"][1]);
}

#[test]
fn observe_jobs_session_sanitizer_removes_nested_token_bodies() {
    let opaque = "wj3_privateepochbody.3f.0.0";
    let summary = super::super::sessions::session_input_summary_for_tool(
        "observe_jobs",
        &json!({
            "items": [
                {"job_id": "job", "after_observation_token": opaque}
            ],
            "tail_lines": 40,
            "wait_secs": 5,
            "wake_on": "terminal"
        }),
    );
    assert_eq!(summary["wake_on"], "terminal");
    for policy in [
        json!("private-unknown-value"),
        json!(null),
        json!({"bad": true}),
    ] {
        let sanitized = super::super::sessions::session_input_summary_for_tool(
            "observe_jobs",
            &json!({"wake_on": policy}),
        );
        assert!(sanitized.get("wake_on").is_none());
    }
    let serialized = serde_json::to_string(&summary).unwrap();
    assert!(!serialized.contains(opaque));
    assert_eq!(summary["items"][0]["job_id"], "job");
    assert!(summary["items"][0].get("after_observation_token").is_none());
}

#[tokio::test]
async fn ordinary_receipts_production_sqlite_dual_restart_observe_and_list_filters() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("ordinary-receipts.db");
    let db = Arc::new(crate::Database::open(&path).unwrap());
    let registry = Arc::new(crate::job_receipts::production_registry(db.clone()).await);
    let runtime = ToolRuntime::new(registry, Arc::new(RuntimeInfo::default()));
    let client = "receipt-dual-restart";
    register_agent(
        &runtime,
        client,
        Some("tester"),
        RunnerCapabilities {
            async_jobs: true,
            async_shell_jobs: true,
            ..Default::default()
        },
    )
    .await;
    let auth = bootstrap_auth_context();
    let project = agent_test_project_id(client);
    let started = runtime
        .dispatch_with_auth(
            ToolCall::RunJob {
                project: project.clone(),
                command: "echo receipt".into(),
                session_id: None,
                timeout_secs: Some(60),
                cwd: None,
                purpose: Some(ExecutionPurpose::Diagnostic),
                shell: None,
            },
            Some(&auth),
        )
        .await;
    assert!(started.success, "{:?}", started.error);
    let job_id = started.output["job_id"].as_str().unwrap().to_string();
    let request = wait_for_patch_agent_request(&runtime, client).await;
    runtime
        .runner_registry
        .update_job(RunnerJobUpdateRequest {
            client_id: client.into(),
            runner_instance_id: "inst".into(),
            update_seq: None,
            job_id: job_id.clone(),
            request_id: Some(request.request_id),
            status: "completed".into(),
            stdout_chunk: Some("bounded stdout\n".into()),
            stderr_chunk: Some("bounded stderr\n".into()),
            log_snapshot: None,
            exit_code: Some(0),
            duration_ms: Some(42),
            error: None,
            command_execution_state: None,
            validation_progress: None,
            test_count_evidence: None,
            activity: None,
            finished: true,
        })
        .await
        .unwrap();
    let old_token = observation_token(&runtime, &job_id, &auth).await;
    let rows = db
        .load_job_receipts(chrono::Utc::now().timestamp())
        .unwrap();
    assert_eq!(
        rows.len(),
        1,
        "production wiring must persist through SQLite"
    );
    let deadline = rows[0].expires_at;
    drop(runtime);
    drop(db);
    let db = Arc::new(crate::Database::open(&path).unwrap());
    let registry = Arc::new(crate::job_receipts::production_registry(db.clone()).await);
    let runtime = ToolRuntime::new(registry, Arc::new(RuntimeInfo::default()));
    // No Runner registration/inventory exists in the replacement process.
    let started = Instant::now();
    let observed = runtime
        .dispatch_with_auth(
            ToolCall::ObserveJobs {
                items: vec![item(&job_id, Some(old_token))],
                tail_lines: 40,
                wait_secs: Some(30),
                wake_on: Default::default(),

                summary_only: false,
            },
            Some(&auth),
        )
        .await;
    assert!(observed.success, "{:?}", observed.error);
    let output = &observed.output["items"][0]["output"];
    assert_eq!(
        observed.output["items"][0]["success"], true,
        "{}",
        observed.output
    );
    assert_eq!(output["status"], "completed");
    assert_eq!(output["exit_code"], 0);
    assert_eq!(output["stdout_tail"], "bounded stdout\n");
    assert_eq!(output["stderr_tail"], "bounded stderr\n");
    assert_eq!(output["log_delta_status"], "reset");
    assert!(started.elapsed() < Duration::from_secs(2));
    for (status, project_filter, session_id, expected) in [
        (Some("completed".into()), Some(project.clone()), None, 1),
        (Some("running".into()), Some(project.clone()), None, 0),
        (None, Some("agent:other:project".into()), None, 0),
        (None, None, Some("wc_sess_other".into()), 0),
    ] {
        let listed = runtime
            .dispatch_with_auth(
                ToolCall::ListJobs {
                    limit: Some(1),
                    status,
                    project: project_filter,
                    session_id,
                },
                Some(&auth),
            )
            .await;
        assert!(listed.success, "{:?}", listed.error);
        assert_eq!(
            listed.output["jobs"].as_array().unwrap().len(),
            expected,
            "{}",
            listed.output
        );
    }
    assert_eq!(
        db.load_job_receipts(chrono::Utc::now().timestamp())
            .unwrap()[0]
            .expires_at,
        deadline
    );
    register_agent(
        &runtime,
        client,
        Some("new-owner"),
        RunnerCapabilities {
            async_jobs: true,
            async_shell_jobs: true,
            ..Default::default()
        },
    )
    .await;
    assert_eq!(
        runtime
            .runner_registry
            .get_job(&job_id)
            .await
            .unwrap()
            .status,
        "completed"
    );
    assert!(probe_patch_agent_request(&runtime, client).await.is_none());
}

#[tokio::test]
async fn observe_jobs_terminal_policy_coalesces_noisy_jobs_with_one_deadline_and_caller_delta() {
    for policy in [ObserveJobsWakeOn::Terminal, ObserveJobsWakeOn::AllTerminal] {
        let runtime = test_runtime();
        let (job_a, request_a, auth) = register_and_start_agent_job(&runtime, "coalesce-a").await;
        let (job_b, request_b, _) = register_and_start_agent_job(&runtime, "coalesce-b").await;
        let token_a = observation_token(&runtime, &job_a, &auth).await;
        let token_b = observation_token(&runtime, &job_b, &auth).await;
        // A change already visible on entry must also be coalesced, without moving
        // the caller's delta baseline to this newer state.
        update_observed_job(
            &runtime,
            "coalesce-a",
            &request_a,
            "running",
            Some("before wait\n"),
            None,
            false,
        )
        .await;
        let started = Instant::now();
        let observation = runtime.observe_jobs_for_auth(
            vec![
                item(&job_a, Some(token_a.clone())),
                item(&job_b, Some(token_b.clone())),
            ],
            40,
            Some(1),
            policy,
            Some(&auth),
        );
        tokio::pin!(observation);
        tokio::select! {
            result = &mut observation => panic!("non-terminal update woke early: {:?}", result.output["wait"]),
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
        update_observed_job(
            &runtime,
            "coalesce-a",
            &request_a,
            "running",
            Some("during wait\n"),
            Some(process_activity()),
            false,
        )
        .await;
        // Exercise the other stream as well as log-independent activity updates.
        runtime
            .runner_registry
            .update_job(RunnerJobUpdateRequest {
                client_id: "coalesce-b".into(),
                runner_instance_id: "inst".into(),
                update_seq: None,
                job_id: job_b.clone(),
                request_id: Some(request_b.request_id.clone()),
                status: "running".into(),
                stdout_chunk: None,
                stderr_chunk: Some("stderr during wait\n".into()),
                log_snapshot: None,
                exit_code: None,
                duration_ms: None,
                error: None,
                command_execution_state: None,
                validation_progress: None,
                test_count_evidence: None,
                activity: Some(process_activity()),
                finished: false,
            })
            .await
            .unwrap();
        if policy == ObserveJobsWakeOn::AllTerminal {
            update_observed_job(
                &runtime,
                "coalesce-a",
                &request_a,
                "completed",
                None,
                None,
                true,
            )
            .await;
            assert!(futures_util::poll!(&mut observation).is_pending());
        }
        let noisy = async {
            loop {
                tokio::time::sleep(Duration::from_millis(80)).await;
                update_observed_job(
                    &runtime,
                    "coalesce-b",
                    &request_b,
                    "running",
                    Some("noise\n"),
                    Some(process_activity()),
                    false,
                )
                .await;
            }
        };
        // Keep producing changes beyond the allowed wait: resetting the deadline
        // on progress would hit this watchdog instead of returning a result.
        let result = tokio::select! {
            result = &mut observation => result,
            _ = noisy => unreachable!(),
            _ = tokio::time::sleep_until(tokio::time::Instant::from_std(started + Duration::from_millis(2500))) => panic!("updates extended the shared deadline"),
        };
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(800),
            "returned early: {elapsed:?}"
        );
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["wait"]["outcome"], "timeout");
        let a = &result.output["items"][0]["output"];
        let b = &result.output["items"][1]["output"];
        assert_eq!(a["changed"], true);
        assert_eq!(b["changed"], true);
        assert_eq!(a["terminal"], policy == ObserveJobsWakeOn::AllTerminal);
        assert_eq!(a["stdout_tail"], "before wait\nduring wait\n");
        assert_eq!(b["stderr_tail"], "stderr during wait\n");
        assert!(b["stdout_tail"].as_str().unwrap().contains("noise"));
        assert_ne!(a["observation_token"], token_a);
        assert_ne!(b["observation_token"], token_b);
        assert_eq!(
            a["activity"],
            if policy == ObserveJobsWakeOn::AllTerminal {
                Value::Null
            } else {
                serde_json::to_value(process_activity()).unwrap()
            }
        );
    }
}

#[tokio::test]
async fn observe_jobs_updates_around_first_wait_registration_are_not_lost() {
    // First poll drives the baseline pass and canonical wait registration to
    // Pending without a scheduler sleep. Updates immediately before that poll
    // exercise revision comparison; updates immediately after it exercise Notify.
    for policy in [
        ObserveJobsWakeOn::Change,
        ObserveJobsWakeOn::Terminal,
        ObserveJobsWakeOn::AllTerminal,
    ] {
        let terminal = policy != ObserveJobsWakeOn::Change;
        for update_before_poll in [false, true] {
            let runtime = test_runtime();
            let (job_id, request, auth) =
                register_and_start_agent_job(&runtime, "observe-race").await;
            let token = observation_token(&runtime, &job_id, &auth).await;
            let observation = runtime.observe_jobs_for_auth(
                vec![item(&job_id, Some(token.clone()))],
                40,
                Some(5),
                policy,
                Some(&auth),
            );
            tokio::pin!(observation);
            if !update_before_poll {
                assert!(futures_util::poll!(&mut observation).is_pending());
            }
            update_observed_job(
                &runtime,
                "observe-race",
                &request,
                if terminal { "completed" } else { "running" },
                Some("raced update\n"),
                None,
                terminal,
            )
            .await;
            if update_before_poll {
                // Characterize the exact pass -> waiter gap directly: a
                // canonical waiter starting after this update must compare
                // the caller baseline before relying on a future notification.
                let canonical = tokio::time::timeout(
                    Duration::from_secs(2),
                    runtime.job_log_for_auth(
                        job_id.clone(),
                        None,
                        Some(1),
                        Some(&auth),
                        Some(token),
                        Some(5),
                    ),
                )
                .await
                .expect("revision comparison must catch an update before registration");
                assert!(canonical.success);
                assert_eq!(canonical.output["changed"], true);
                assert_eq!(canonical.output["terminal"], terminal);
            }
            let result = tokio::time::timeout(Duration::from_secs(2), observation)
                .await
                .expect("update near registration must not wait the five-second deadline");
            assert!(result.success, "{:?}", result.error);
            assert_eq!(
                result.output["wait"]["outcome"],
                if terminal { "terminal" } else { "updated" }
            );
            assert_eq!(result.output["items"][0]["output"]["changed"], true);
            assert_eq!(result.output["items"][0]["output"]["terminal"], terminal);
            assert_eq!(
                result.output["items"][0]["output"]["stdout_tail"],
                "raced update\n"
            );
        }
    }
}

#[test]
fn observe_jobs_canonical_continuation_is_parser_ready_with_or_without_baseline() {
    for token in [None, Some("opaque-observation-cursor")] {
        let hint = super::super::jobs::observe_job_continuation("job", token);
        let parsed =
            ToolCall::from_tool_name(hint["tool"].as_str().unwrap(), hint["arguments"].clone())
                .unwrap();
        assert!(matches!(
            parsed,
            ToolCall::ObserveJobs {
                wait_secs: Some(webcodex_core::runtime_contract::MODEL_JOB_CONTINUATION_WAIT_SECS),
                wake_on: ObserveJobsWakeOn::Terminal,

                summary_only: false,
                ..
            }
        ));
        assert_eq!(
            hint["arguments"]["items"][0]["after_observation_token"].as_str(),
            token
        );
    }
}

#[tokio::test]
async fn observe_jobs_all_terminal_waits_for_entire_set_and_preserves_original_delta() {
    for initially_terminal in [0, 1, 2] {
        let runtime = test_runtime();
        let (a, request_a, auth) = register_and_start_agent_job(&runtime, "all-a").await;
        let (b, request_b, _) = register_and_start_agent_job(&runtime, "all-b").await;
        let token_a = observation_token(&runtime, &a, &auth).await;
        let token_b = observation_token(&runtime, &b, &auth).await;
        update_observed_job(
            &runtime,
            "all-a",
            &request_a,
            if initially_terminal > 0 {
                "completed"
            } else {
                "running"
            },
            Some("before wait\n"),
            None,
            initially_terminal > 0,
        )
        .await;
        if initially_terminal == 2 {
            update_observed_job(
                &runtime,
                "all-b",
                &request_b,
                "completed",
                Some("b done\n"),
                None,
                true,
            )
            .await;
        }
        // Exactly one outer observation, including both staggered completions.
        let observation = runtime.observe_jobs_for_auth(
            vec![item(&a, Some(token_a)), item(&b, Some(token_b))],
            40,
            Some(5),
            ObserveJobsWakeOn::AllTerminal,
            Some(&auth),
        );
        tokio::pin!(observation);
        if initially_terminal < 2 {
            assert!(futures_util::poll!(&mut observation).is_pending());
            if initially_terminal == 0 {
                update_observed_job(
                    &runtime,
                    "all-a",
                    &request_a,
                    "completed",
                    Some("a done\n"),
                    None,
                    true,
                )
                .await;
                assert!(
                    futures_util::poll!(&mut observation).is_pending(),
                    "first terminal Job must not wake the batch"
                );
            }
            update_observed_job(
                &runtime,
                "all-b",
                &request_b,
                "completed",
                Some("b done\n"),
                None,
                true,
            )
            .await;
        }
        let result = tokio::time::timeout(Duration::from_secs(1), observation)
            .await
            .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["wait"]["outcome"], "terminal");
        assert_eq!(result.output["terminal_count"], 2);
        assert_eq!(result.output["items"][0]["job_id"], a);
        assert_eq!(result.output["items"][1]["job_id"], b);
        assert_eq!(
            result.output["items"][0]["output"]["stdout_tail"],
            if initially_terminal == 0 {
                "before wait\na done\n"
            } else {
                "before wait\n"
            }
        );
        if initially_terminal == 2 {
            assert_eq!(result.output["wait"]["waited_ms"], 0);
        }
    }
}

#[tokio::test]
async fn observe_jobs_all_terminal_item_errors_return_without_waiting_for_running_job() {
    let runtime = test_runtime();
    let (job, _, auth) = register_and_start_agent_job(&runtime, "all-error").await;
    let token = observation_token(&runtime, &job, &auth).await;
    let (foreign, _, _) = register_and_start_agent_job(&runtime, "all-foreign").await;
    let other_auth = auth_context(Some("other-observer"), false);
    for (bad_item, caller) in [
        (item("missing-job", Some(token.clone())), &auth),
        (item(&foreign, Some("invalid-token".into())), &auth),
        (item(&foreign, None), &other_auth),
    ] {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            runtime.observe_jobs_for_auth(
                vec![item(&job, Some(token.clone())), bad_item],
                40,
                Some(5),
                ObserveJobsWakeOn::AllTerminal,
                Some(caller),
            ),
        )
        .await
        .unwrap();
        assert!(result.success, "{:?}", result.error);
        assert_eq!(result.output["wait"]["outcome"], "item_error");
        assert!(result.output["failed_count"].as_u64().unwrap() > 0);
    }
}

#[tokio::test]
async fn observe_jobs_generic_failed_test_identity_survives_small_model_tail() {
    for tool in ["run_process", "run_shell"] {
        let runtime = test_runtime();
        let client = "generic-failure-tail";
        register_agent(
            &runtime,
            client,
            None,
            RunnerCapabilities {
                shell: true,
                async_jobs: true,
                async_shell_jobs: true,
                structured_process_argv: true,
                structured_execution_jobs: true,
                ..Default::default()
            },
        )
        .await;
        let auth = bootstrap_auth_context();
        let mut arguments = json!({"project":agent_test_project_id(client),
            "sync_wait_secs":1,"timeout_secs":30,"purpose":"test"});
        if tool == "run_process" {
            arguments["executable"] = json!("cargo");
            arguments["args"] = json!(["test", "--lib"]);
        } else {
            arguments["command"] = json!("cargo test --lib");
        }
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let auth = auth.clone();
            async move {
                runtime
                    .dispatch_with_auth(
                        ToolCall::from_tool_name(tool, arguments).unwrap(),
                        Some(&auth),
                    )
                    .await
            }
        });
        let request = wait_for_patch_agent_request(&runtime, client).await;
        let handoff = task.await.unwrap();
        assert!(handoff.success, "{handoff:?}");
        let job = handoff.output["job_id"].as_str().unwrap();
        let stdout = "running 1 test\ntest cases::outside_tail ... FAILED\n".to_string()
            + &"retained diagnostic padding\n".repeat(400)
            + "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s\n";
        runtime
            .runner_registry
            .update_job(RunnerJobUpdateRequest {
                client_id: client.into(),
                runner_instance_id: "inst".into(),
                update_seq: None,
                job_id: job.into(),
                request_id: Some(request.request_id),
                status: "failed".into(),
                stdout_chunk: Some(stdout),
                stderr_chunk: None,
                log_snapshot: None,
                exit_code: Some(101),
                duration_ms: Some(100),
                error: None,
                command_execution_state: Some(
                    crate::runner_protocol::ShellCommandExecutionState::Completed,
                ),
                validation_progress: None,
                test_count_evidence: None,
                activity: None,
                finished: true,
            })
            .await
            .unwrap();
        let observed = runtime
            .observe_jobs_for_auth(
                vec![item(job, None)],
                2,
                None,
                ObserveJobsWakeOn::Change,
                Some(&auth),
            )
            .await;
        assert!(observed.success, "{observed:?}");
        let snapshot = &observed.output["items"][0]["output"];
        assert!(!snapshot["stdout_tail"]
            .as_str()
            .unwrap()
            .contains("outside_tail"));
        assert_eq!(snapshot["detected_summary"]["tests_failed"], 1);
        assert_eq!(
            snapshot["detected_summary"]["failed_test_details"][0]["name"],
            "cases::outside_tail"
        );
        assert_eq!(
            snapshot["detected_summary"]["failed_test_details_truncated"],
            false
        );
        assert!(
            snapshot["validation"].is_null(),
            "generic detection grants no structured validation proof"
        );
        assert!(snapshot.get("validation_target_id").is_none());
    }
}
