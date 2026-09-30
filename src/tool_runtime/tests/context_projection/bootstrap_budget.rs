use super::*;
use crate::tool_runtime::context_projection::{
    ContextMaterialCapabilities, MAX_CONTEXT_PROJECTION_BYTES,
};
use crate::tool_runtime::tool_inputs::CodingGuidanceProfile;
use webcodex_core::project_instructions::{
    InstructionSourceScope, LoadedInstructionCandidate, ProjectInstructionsSnapshot,
};

#[tokio::test]
async fn bootstrap_budget_keeps_requested_workflow_and_honest_instruction_continuations() {
    let root = tempfile::tempdir().unwrap();
    let runtime = ToolRuntime::new_for_tests();
    let id = register_runner_project_at_path(&runtime, "budget-runner", "demo", root.path()).await;
    let auth = auth_context(None, true);
    let project = runtime
        .resolve_project_input_for_auth(&id, Some(&auth))
        .await
        .unwrap();
    let body = format!(
        "# Rules\nKEEP_RULE_MARKER\n{}",
        "规则 \\\" quoted\n".repeat(4_000)
    );
    let snapshot = ProjectInstructionsSnapshot::from_candidates(
        vec![LoadedInstructionCandidate {
            source_scope: InstructionSourceScope::Project,
            path: "AGENTS.md".into(),
            total_lines: body.lines().count(),
            content: body,
            full_sha256: None,
        }],
        true,
    );
    for profile in [
        CodingGuidanceProfile::Direct,
        CodingGuidanceProfile::HostCodeMode,
    ] {
        for keys in [
            vec!["project.instructions", "webcodex.workflow"],
            vec!["webcodex.workflow", "project.instructions"],
            vec![
                "project.instructions",
                "webcodex.workflow",
                "unknown.context",
            ],
        ] {
            let mut result = ToolResult::ok(json!({"main_result":"unchanged"}));
            runtime
                .add_requested_context_projection_with_guidance(
                    &mut result,
                    &keys.iter().map(|key| key.to_string()).collect::<Vec<_>>(),
                    Some(&project),
                    Some(&auth),
                    ContextMaterialCapabilities::default(),
                    profile,
                    None,
                    Some(&snapshot),
                )
                .await;
            assert!(result.success);
            assert_eq!(result.output["main_result"], "unchanged");
            assert_eq!(
                context_material(&result, "webcodex.workflow")["status"],
                "available"
            );
            assert_eq!(
                context_material(&result, "webcodex.workflow")["projection"],
                crate::tool_runtime::startup_brief::builtin_coding_workflow_projection_with_policy(
                    profile,
                    runtime.model_workflow_policy
                )
            );
            let instruction = &context_material(&result, "project.instructions")["projection"];
            assert_eq!(instruction["truncated"], true);
            assert_eq!(instruction["content_included"], true);
            let source = &instruction["sources"][0];
            assert!(source["content"]
                .as_str()
                .unwrap()
                .contains("KEEP_RULE_MARKER"));
            assert_eq!(source["read_more"]["path"], "AGENTS.md");
            assert!(source["read_more"]["start_line"].as_u64().unwrap() > 1);
            assert!(
                serde_json::to_vec(&result.output["context_projection"])
                    .unwrap()
                    .len()
                    <= MAX_CONTEXT_PROJECTION_BYTES
            );
            let order: Vec<_> = result.output["context_projection"]["materials"]
                .as_array()
                .unwrap()
                .iter()
                .map(|item| item["key"].as_str().unwrap())
                .collect();
            assert_eq!(order, keys);
        }
    }
    let mut denied = auth_context(None, false);
    denied.scopes.clear();
    let mut result = ToolResult::ok(json!({}));
    runtime
        .add_requested_context_projection_with_guidance(
            &mut result,
            &["project.instructions".into(), "webcodex.workflow".into()],
            Some(&project),
            Some(&denied),
            ContextMaterialCapabilities::default(),
            CodingGuidanceProfile::HostCodeMode,
            None,
            Some(&snapshot),
        )
        .await;
    assert_eq!(
        context_material(&result, "project.instructions")["status"],
        "unavailable"
    );
    assert_eq!(
        context_material(&result, "webcodex.workflow")["status"],
        "available"
    );
    assert!(!result.output.to_string().contains("KEEP_RULE_MARKER"));
}
