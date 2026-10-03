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
