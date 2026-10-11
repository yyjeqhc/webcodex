use super::*;

fn completed() -> ToolResult {
    ToolResult::ok(json!({
        "execution_state":"completed", "command_started":true, "command_completed":true,
        "command_ok":true, "terminal":true, "promoted_to_job":false, "exit_code":0,
        "job_id":null, "job_status":null, "observation_token":null,
        "stdout_tail":"result that cannot be re-read", "stderr_tail":""
    }))
}

#[test]
fn execution_control_success_requires_canonical_proof_not_outer_success() {
    let result = completed();
    assert_eq!(command_control(&result)["outcome"], "passed");
    for (field, value) in [
        ("execution_state", json!("pending")),
        ("execution_state", json!("outcome_unknown")),
        ("command_started", json!(false)),
        ("command_completed", json!(false)),
        ("command_ok", json!(false)),
        ("terminal", json!(false)),
        ("promoted_to_job", json!(true)),
        ("exit_code", json!(1)),
        ("job_id", json!("another-job")),
        ("tool_failure", json!(true)),
        ("passed", json!(false)),
        ("recovery_state", json!("recovering")),
        ("failure_kind", json!("timeout")),
    ] {
        let mut changed = completed();
        changed.output[field] = value;
        assert_ne!(command_control(&changed)["outcome"], "passed", "{field}");
    }
    for field in [
        "execution_state",
        "command_started",
        "command_completed",
        "command_ok",
        "terminal",
        "promoted_to_job",
        "exit_code",
    ] {
        let mut changed = completed();
        changed.output.as_object_mut().unwrap().remove(field);
        assert_eq!(command_control(&changed)["outcome"], "unknown", "{field}");
    }
    let mut failed = completed();
    failed.success = false;
    assert_eq!(command_control(&failed)["outcome"], "failed");
}

#[test]
fn execution_control_pending_preserves_exact_identity_and_follow_up_posture() {
    let next = super::super::jobs::observe_job_continuation("job-1", Some("token-1"));
    let mut result = ToolResult::ok(json!({"execution_state":"running", "job_id":"job-1",
        "terminal":false, "continuation":next, "observation_token":"token-1"}));
    let plan = ExecutionControlProjection::capture("run_process", &result).unwrap();
    super::super::jobs::sparsify_job_handoff_model_result(&mut result);
    plan.project(&mut result);
    split_result(&mut result);
    assert_eq!(result.output["execution"]["job_id"], "job-1");
    assert_eq!(result.output["execution"]["outcome"], "pending");
    assert_eq!(result.output["execution"]["next"], next);
    assert_eq!(next["follow_up_kind"], "fallback_recovery");
    assert!(result.output.get("details").is_none());
}

#[test]
fn execution_control_keeps_logs_loss_evidence_and_collaboration_visible() {
    let mut result = completed();
    result.output["stdout_truncated"] = json!(true);
    result.output["session_hint"] = json!({"attention_required":true});
    result.output["operator_messages"] = json!({"requires_ack":true});
    result.output["peer_awareness"] = json!({"new_peers":[{"peer_id":"peer-1"}]});
    result.output["job_attention"] = json!({"failed_jobs":["job-other"]});
    result.output["control"] = json!({"after_failed":true});
    let plan = ExecutionControlProjection::capture("run_script", &result).unwrap();
    plan.project(&mut result);
    split_result(&mut result);
    assert_eq!(
        result.output["details"]["stdout_tail"],
        "result that cannot be re-read"
    );
    assert_eq!(result.output["details"]["stdout_truncated"], true);
    assert_eq!(result.output["session_hint"]["attention_required"], true);
    assert_eq!(result.output["operator_messages"]["requires_ack"], true);
    assert_eq!(
        result.output["peer_awareness"]["new_peers"][0]["peer_id"],
        "peer-1"
    );
    assert!(result.output["details"].get("session_hint").is_none());
    assert!(result.output["details"].get("peer_awareness").is_none());
    assert!(result.output.get("job_attention").is_some());
    assert_eq!(result.output["control"]["after_failed"], true);
    assert!(result.output["details"].get("execution_state").is_none());
}

fn observed(status: &str, terminal: bool, code: Value) -> Value {
    json!({"success":true,"job_id":"job-1","output":{
        "status":status,"terminal":terminal,"exit_code":code,"observation_token":"token-1"
    }})
}

#[test]
fn execution_control_observation_does_not_confuse_success_with_job_success() {
    for (status, terminal, code, expected) in [
        ("completed", true, json!(0), "passed"),
        ("completed", true, json!(1), "failed"),
        ("completed", true, Value::Null, "unknown"),
        ("completed", false, json!(0), "unknown"),
        ("running", false, Value::Null, "pending"),
        ("queued", false, Value::Null, "pending"),
        ("agent_queued", false, Value::Null, "pending"),
        ("started", false, Value::Null, "pending"),
        ("stop_requested", false, Value::Null, "pending"),
        ("failed", true, json!(1), "failed"),
        ("lost", true, json!(0), "unknown"),
        ("recovering", false, Value::Null, "unknown"),
        ("stopped", true, Value::Null, "failed"),
        ("timeout", true, Value::Null, "failed"),
        ("timed_out", true, Value::Null, "failed"),
        ("cancelled", true, Value::Null, "failed"),
    ] {
        assert_eq!(
            job_control(&observed(status, terminal, code))["outcome"],
            expected,
            "{status}"
        );
    }
    for (key, value) in [
        ("command_execution_state", json!("outcome_unknown")),
        ("recovery_state", json!("recovering")),
        ("validation", json!({"passed":false})),
    ] {
        let mut item = observed("completed", true, json!(0));
        item["output"][key] = value;
        assert_ne!(job_control(&item)["outcome"], "passed", "{key}");
    }
    let mut item = observed("completed", true, json!(0));
    item["success"] = json!(false);
    assert_eq!(job_control(&item)["outcome"], "unknown");
    // A zero exit code cannot override contradictory persisted diagnostics.
    for (key, value) in [
        ("failure_kind", json!("outcome_unknown")),
        ("error_kind", json!("observation_failed")),
        ("tool_failure", json!(true)),
        ("passed", json!(false)),
    ] {
        let mut item = observed("completed", true, json!(0));
        item["output"][key] = value;
        assert_eq!(job_control(&item)["outcome"], "unknown", "{key}");
    }
    let mut item = observed("completed", true, json!(0));
    item["error"] = json!("inconsistent observation");
    assert_eq!(job_control(&item)["outcome"], "unknown");
    for key in ["error_kind", "recovery_kind"] {
        let mut item = observed("completed", true, json!(0));
        item[key] = json!("inconsistent observation");
        assert_eq!(job_control(&item)["outcome"], "unknown", "{key}");
    }
}

#[test]
fn execution_control_observation_keeps_batch_identity_and_exact_cursor() {
    let mut first = observed("completed", true, json!(0));
    first["observation_ref"] = json!("~j1");
    let mut second = observed("running", false, Value::Null);
    second["job_id"] = json!("job-2");
    let mut result = ToolResult::ok(json!({"items":[first,second],"wait":{"outcome":"terminal"}}));
    let plan = ExecutionControlProjection::capture("observe_jobs", &result).unwrap();
    plan.project(&mut result);
    split_result(&mut result);
    assert_eq!(
        result.output["items"][0]["execution"]["observation_ref"],
        "~j1"
    );
    assert_eq!(result.output["items"][0]["execution"]["outcome"], "passed");
    assert_eq!(result.output["items"][1]["execution"]["job_id"], "job-2");
    assert_eq!(
        result.output["items"][1]["execution"]["observation_token"],
        "token-1"
    );
    assert_eq!(result.output["items"][1]["execution"]["outcome"], "pending");
    assert_eq!(result.output["wait"]["outcome"], "terminal");
}
