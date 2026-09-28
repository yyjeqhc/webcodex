use crate::model_workflow::{
    GoalWorkflowPreference, McpAppResumeMode, ModelWorkflowPolicy, GOAL_WORKFLOW_CONTEXT_KEY,
};
use crate::tool_runtime::startup_brief::{
    builtin_coding_workflow_projection_with_policy, validate_schema_instance_for_test,
};
use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
use serde_json::Value;

#[test]
fn model_workflow_policy_defaults_conservatively_and_rejects_invalid_declarations() {
    assert_eq!(
        ModelWorkflowPolicy::from_values(None, None).unwrap(),
        ModelWorkflowPolicy::default()
    );
    assert_eq!(
        ModelWorkflowPolicy::default().goal,
        GoalWorkflowPreference::OnDemand
    );
    assert_eq!(
        ModelWorkflowPolicy::default().mcp_app_resume,
        McpAppResumeMode::Unknown
    );
    for goal in ["on_demand", "preferred"] {
        for mode in ["unknown", "user_confirmed", "unattended"] {
            let policy = ModelWorkflowPolicy::from_values(Some(goal), Some(mode)).unwrap();
            assert_eq!(
                policy.mcp_app_resume.allows_unattended_claim(),
                mode == "unattended"
            );
        }
    }
    for invalid in [
        "",
        "true",
        "auto",
        "host_code_mode",
        "unexpected-private-value",
    ] {
        let goal = ModelWorkflowPolicy::from_values(Some(invalid), None).unwrap_err();
        let resume = ModelWorkflowPolicy::from_values(None, Some(invalid)).unwrap_err();
        assert!(goal.contains("must be on_demand or preferred"));
        assert!(resume.contains("must be unknown, user_confirmed or unattended"));
        assert!(!goal.contains("unexpected-private-value"));
        assert!(!resume.contains("unexpected-private-value"));
    }
}

fn workflow_schema() -> Value {
    let schema = crate::tool_runtime::registry::coding_workflow_diagnostic_output_schema_for_test();
    schema["properties"]["output"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| variant["properties"]["detail"]["const"] == "standard")
        .unwrap()["properties"]["workflow"]
        .clone()
}

#[test]
fn model_workflow_preferences_fit_the_unchanged_workflow_schema_and_keep_goal_rules_optional() {
    let schema = workflow_schema();
    let base =
        builtin_coding_workflow_projection_with_policy(Default::default(), Default::default());
    for preference in [
        GoalWorkflowPreference::OnDemand,
        GoalWorkflowPreference::Preferred,
    ] {
        for mode in [
            McpAppResumeMode::Unknown,
            McpAppResumeMode::UserConfirmed,
            McpAppResumeMode::Unattended,
        ] {
            let policy = ModelWorkflowPolicy {
                goal: preference,
                mcp_app_resume: mode,
            };
            let profiles = [
                CodingGuidanceProfile::Direct,
                CodingGuidanceProfile::HostCodeMode,
            ];
            for profile in profiles {
                let projection = builtin_coding_workflow_projection_with_policy(profile, policy);
                validate_schema_instance_for_test(&projection, &schema).unwrap();
                assert_eq!(projection["authority"], "model_guidance_only");
                assert_eq!(projection["guidance"], base["guidance"]);
                assert_eq!(projection["roles"], base["roles"]);
                assert!(projection["model_protocol"]["goal_checkpoint"]
                    .as_str()
                    .unwrap()
                    .contains("No Goal means no checkpoint"));
            }
            let details = policy.goal_workflow_projection();
            assert_eq!(details["authority"], "model_guidance_only");
            assert_eq!(details["selection"], policy.goal_selection_guidance());
            assert!(details["workflow"]
                .as_str()
                .unwrap()
                .contains("prepare_goal_workflow"));
            assert!(details["continuation"]
                .as_str()
                .unwrap()
                .contains("never retry an uncertain prior effect"));
            assert!(serde_json::to_vec(&details).unwrap().len() < 4096);
        }
    }
    let text = base.to_string();
    assert!(
        text.contains("Ordinary implementation, review and multi-step work do not require a Goal")
    );
    assert!(text.contains(GOAL_WORKFLOW_CONTEXT_KEY));
    assert!(
        !text.contains("prepare_goal_workflow"),
        "creation recipe belongs in optional chapter"
    );
    assert!(
        !text.contains("_wc.control.before.goal_progress"),
        "checkpoint recipe belongs in optional chapter"
    );
}

#[test]
fn model_workflow_optional_chapters_do_not_expand_cached_context_schema_examples() {
    assert_eq!(
        crate::tool_runtime::context_projection::context_material_keys_csv(),
        "project.instructions, webcodex.workflow, workflow.resume, jobs.attention, skills.catalog, plugins.catalog, memory.bootstrap"
    );
}
