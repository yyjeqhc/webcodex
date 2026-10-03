use super::*;
use std::collections::HashSet;

#[test]
fn context_registry_keys_are_unique_and_discovery_preserves_optional_chapters() {
    let mut keys = HashSet::new();
    for provider in BUILTIN_CONTEXT_MATERIALS.providers {
        assert!(
            keys.insert(provider.key),
            "duplicate provider: {}",
            provider.key
        );
        assert!(!provider.key.is_empty());
        assert!(provider.key.len() <= super::super::MAX_CONTEXT_REQUEST_KEY_CHARS);
        assert!(std::ptr::eq(
            BUILTIN_CONTEXT_MATERIALS.get(provider.key).unwrap(),
            *provider
        ));
    }
    assert_eq!(
        super::super::context_material_keys_csv(),
        "project.instructions, webcodex.workflow, workflow.resume, jobs.attention, skills.catalog, plugins.catalog, memory.bootstrap"
    );
    assert!(BUILTIN_CONTEXT_MATERIALS
        .get(crate::model_workflow::GOAL_WORKFLOW_CONTEXT_KEY)
        .is_some());
    assert!(BUILTIN_CONTEXT_MATERIALS.get("future.material").is_none());
}

#[tokio::test]
async fn context_registry_rejects_before_invoking_provider() {
    let runtime = ToolRuntime::new_for_tests();
    let context = || ContextMaterialContext {
        runtime: &runtime,
        key: "test.denied",
        project: None,
        auth: None,
        window: None,
        instructions: None,
        budget: ContextProjectionBudget {
            preceding: &[],
            remaining: &[],
            workflow: None,
            truncated: false,
        },
    };
    let mut provider = ContextMaterialProvider {
        key: "test.denied",
        project_required: true,
        scope_policy: ScopePolicy::Require(crate::auth::SCOPE_PROJECT_READ),
        surface: Surface::SkillRuntime,
        advertised: false,
        scope_unavailable_reason: "denied_scope",
        provide: |_| panic!("ineligible material provider must not execute"),
    };
    let mut capabilities = ContextMaterialCapabilities::default();
    assert_eq!(
        provider.project(context(), capabilities).await["reason_code"],
        "context_material_surface_unavailable"
    );
    capabilities.skill_runtime = true;
    assert_eq!(
        provider.project(context(), capabilities).await["reason_code"],
        "project_target_unavailable"
    );
    provider.project_required = false;
    assert_eq!(
        provider.project(context(), capabilities).await["reason_code"],
        "denied_scope"
    );
}
