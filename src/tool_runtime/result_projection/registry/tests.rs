use super::*;
use serde_json::json;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn call(name: &str, args: serde_json::Value) -> ToolCall {
    ToolCall::from_tool_name(name, args).unwrap_or_else(|error| panic!("{name}: {error}"))
}

#[test]
fn result_projection_registry_selects_exactly_one_owner_for_each_builtin_family() {
    let mut calls = vec![
        call(
            "read_agent_wait",
            json!({"wait_id":"wc_agent_wait_1234567890abcdef"}),
        ),
        call(
            "cancel_agent_wait",
            json!({"wait_id":"wc_agent_wait_1234567890abcdef", "idempotency_key":"cancel"}),
        ),
        call(
            "wait_for_job_readiness",
            json!({"job_ids":["wc_job_1234567890abcdef"], "mode":"all", "wait_secs":1}),
        ),
        call(
            "edit_project_files",
            json!({"project":"fixture", "changes":[{"kind":"create","path":"example.rs","content":""}]}),
        ),
        call("project_build", json!({"project":"fixture"})),
        call(
            "run_process",
            json!({"project":"fixture", "executable":"probe"}),
        ),
        call(
            "run_skill_resource",
            json!({"project":"fixture", "skill_id":"wc_skill_AAAAAAAAAAAAAAAAAAAAAA", "path":"scripts/probe.py", "expected_definition_revision":"0".repeat(64)}),
        ),
        call(
            "run_script",
            json!({"project":"fixture", "language":"python", "script":"pass"}),
        ),
        call("run_shell", json!({"project":"fixture", "command":"true"})),
        call("cargo_fmt", json!({"project":"fixture"})),
        call("cargo_check", json!({"project":"fixture"})),
        call(
            "cargo_test",
            json!({"project":"fixture", "require_tests":true, "min_tests":3}),
        ),
        call(
            "project_validate",
            json!({"project":"fixture", "action":"test"}),
        ),
        call("go_test", json!({"project":"fixture"})),
        call(
            "read_files",
            json!({"project":"fixture", "items":[{"path":"example.rs"}]}),
        ),
        call(
            "search_project_texts",
            json!({"project":"fixture", "queries":[{"pattern":"needle"}]}),
        ),
    ];
    // Capture is a pure pre-dispatch operation, not admission. The event payload
    // is irrelevant here; domain authorization/validation is exercised elsewhere.
    calls.push(ToolCall::WaitForAgentEvents {
        agent_id: "wc_dagent_1234567890abcdef".into(),
        endpoint_id: "wc_endpoint_1234567890abcdef".into(),
        expected_controller_generation: 1,
        mode: Default::default(),
        goal_id: None,
        events: Vec::new(),
        idempotency_key: "wait".into(),
    });
    for call in calls {
        let matches = BUILTIN_RESULT_PROJECTORS
            .projectors
            .iter()
            .filter(|capture| capture(&call).is_some())
            .count();
        assert_eq!(
            matches,
            1,
            "ambiguous/missing projection for {}",
            call.tool_name()
        );
        assert!(ModelFacingProjectionPlan::capture(&call)
            .projection
            .is_some());
    }
}

#[test]
fn result_projection_registry_unmatched_tools_preserve_the_complete_result() {
    let call = call("list_tools", json!({}));
    let mut plan = ModelFacingProjectionPlan::capture(&call);
    assert!(plan.projection.is_none());
    plan.bind_resolved_project(None);
    let mut result = ToolResult::err_with_output(
        "unchanged failure",
        json!({
            "execution_state":"outcome_unknown", "job_id":"wc_job_1234567890abcdef",
            "direct_retry_safe":false, "custom":{"unchanged":true}
        }),
    );
    let expected = serde_json::to_value(&result).unwrap();
    plan.project(&mut result);
    assert_eq!(serde_json::to_value(&result).unwrap(), expected);
}

struct Probe {
    applied: Arc<AtomicUsize>,
    dropped: Arc<AtomicUsize>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

impl ResultProjection for Probe {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        assert_eq!(self.applied.fetch_add(1, Ordering::SeqCst), 0);
        result.output["projected"] = json!(true);
    }
}

#[test]
fn result_projection_registry_consumes_and_releases_only_request_local_state() {
    for apply in [false, true] {
        let applied = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let plan = ModelFacingProjectionPlan {
            projection: Some(Box::new(Probe {
                applied: applied.clone(),
                dropped: dropped.clone(),
            })),
        };
        assert_eq!(applied.load(Ordering::SeqCst), 0);
        assert_eq!(dropped.load(Ordering::SeqCst), 0);
        if apply {
            let mut result = ToolResult::ok(json!({}));
            plan.project(&mut result);
            assert_eq!(result.output, json!({"projected":true}));
        } else {
            drop(plan);
        }
        assert_eq!(applied.load(Ordering::SeqCst), usize::from(apply));
        assert_eq!(dropped.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn native_result_echo_cleanup_keeps_fences_and_failure_evidence() {
    for name in ["find_definition", "find_references"] {
        let canonical = ToolResult::ok(
            json!({"project":"p","path":"src/a.rs","query_position":{"line":1,"column":1},"locations":[],"truncated":true,"invalid_results_omitted":1}),
        );
        let mut model = ToolResult::ok(canonical.output.clone());
        super::super::sparsify_success_model_result_metadata(name, &mut model);
        assert!(model.output.get("project").is_none());
        assert!(model.output.get("query_position").is_none());
        assert_eq!(model.output["truncated"], true);
        assert_eq!(model.output["invalid_results_omitted"], 1);
        assert_eq!(canonical.output["project"], "p");
    }
    let mut failed = ToolResult::err_with_output(
        "uncertain",
        json!({"executor":"agent","duration_ms":3,"purpose":"operation","execution_source":"run_shell","execution_state":"outcome_unknown","state_changed":null,"job_id":"exact","observation_token":"token","cwd":"subdir","shell":"bash","continuation":{"tool":"observe_jobs","arguments":{"items":[{"job_id":"exact"}]}}}),
    );
    let before = failed.output.clone();
    super::super::sparsify_failure_model_result_metadata("run_shell", &mut failed);
    for key in [
        "execution_state",
        "state_changed",
        "job_id",
        "observation_token",
        "cwd",
        "shell",
        "continuation",
    ] {
        assert_eq!(failed.output[key], before[key]);
    }
    for key in ["executor", "duration_ms", "purpose", "execution_source"] {
        assert!(failed.output.get(key).is_none());
    }
}
