//! Built-in material definitions. Each key, policy and provider is registered as
//! one unit; adding a material does not require a second string dispatch table.

use super::registry::{ContextMaterialContext, ContextMaterialProvider, ScopePolicy, Surface};
use super::unavailable;
use futures_util::future::BoxFuture;
use serde_json::Value;

const SCOPE_UNAVAILABLE: &str = "context_material_scope_unavailable";

pub(super) static PROJECT_INSTRUCTIONS: ContextMaterialProvider = ContextMaterialProvider {
    key: "project.instructions",
    project_required: true,
    scope_policy: ScopePolicy::Require(crate::auth::SCOPE_PROJECT_READ),
    surface: Surface::AnySidecar,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: project_instructions,
};

fn project_instructions(context: ContextMaterialContext<'_>) -> BoxFuture<'_, Value> {
    Box::pin(async move {
        let loaded;
        let snapshot = match context.instructions {
            Some(snapshot) => snapshot,
            None => {
                loaded = context
                    .runtime
                    .load_effective_coding_instructions(context.project(), context.auth)
                    .await;
                &loaded
            }
        };
        let mut material = if snapshot.scan_complete {
            context.available(Value::Null)
        } else {
            let mut material =
                unavailable(context.key, "project_instructions_observation_incomplete");
            material["projection"] = Value::Null;
            material
        };
        let budget = context.budget.for_instruction_projection(&material);
        material["projection"] =
            super::super::startup_brief::project_instructions_context_projection(snapshot, budget);
        material
    })
}

pub(super) static WORKFLOW: ContextMaterialProvider = ContextMaterialProvider {
    key: "webcodex.workflow",
    project_required: false,
    scope_policy: ScopePolicy::Public,
    surface: Surface::AnySidecar,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move { context.budget.workflow.expect("requested workflow").clone() })
    },
};

pub(super) static WORKFLOW_RESUME: ContextMaterialProvider = ContextMaterialProvider {
    key: "workflow.resume",
    project_required: false,
    scope_policy: ScopePolicy::Public,
    surface: Surface::AnySidecar,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move {
            context.project_result(
                context
                    .runtime
                    .workflow_resume_context_projection(context.window, context.auth)
                    .await,
            )
        })
    },
};

pub(super) static JOBS_ATTENTION: ContextMaterialProvider = ContextMaterialProvider {
    key: "jobs.attention",
    project_required: true,
    scope_policy: ScopePolicy::Require(crate::auth::SCOPE_RUNTIME_READ),
    surface: Surface::AnySidecar,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move {
            // Project attention only; a recorder/ambient Session never selects a
            // business Session or grants Job inventory authority.
            let projection = Box::pin(context.runtime.active_jobs_summary(
                Some(&context.project().resolved_id),
                None,
                context.auth,
                8,
            ))
            .await;
            context.available(projection)
        })
    },
};

pub(super) static SKILLS_CATALOG: ContextMaterialProvider = ContextMaterialProvider {
    key: "skills.catalog",
    project_required: true,
    scope_policy: ScopePolicy::Require(crate::auth::SCOPE_PROJECT_READ),
    surface: Surface::SkillRuntime,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move {
            context.project_result(
                context
                    .runtime
                    .skills_catalog_context_projection(context.project(), context.auth)
                    .await,
            )
        })
    },
};

pub(super) static PLUGINS_CATALOG: ContextMaterialProvider = ContextMaterialProvider {
    key: "plugins.catalog",
    project_required: true,
    scope_policy: ScopePolicy::RequireAll(&[
        crate::auth::SCOPE_PROJECT_READ,
        crate::auth::SCOPE_PLUGIN_INSPECT,
    ]),
    surface: Surface::AnySidecar,
    advertised: true,
    scope_unavailable_reason: "plugin_inspect_scope_unavailable",
    provide: |context| {
        Box::pin(async move {
            context.project_result(
                context
                    .runtime
                    .plugin_project_catalog_context_projection(context.project(), context.auth)
                    .await,
            )
        })
    },
};

pub(super) static MEMORY_BOOTSTRAP: ContextMaterialProvider = ContextMaterialProvider {
    key: "memory.bootstrap",
    project_required: true,
    scope_policy: ScopePolicy::RequireAll(webcodex_core::authority::MEMORY_READ_SCOPES),
    surface: Surface::Memory,
    advertised: true,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move {
            context.project_result(
                context
                    .runtime
                    .memory_bootstrap_context_projection(context.project()),
            )
        })
    },
};

// Optional chapters are discovered through the stable webcodex.workflow entry,
// not every cached tool descriptor. No new ToolCall or schema refresh is needed.
pub(super) static GOAL_WORKFLOW: ContextMaterialProvider = ContextMaterialProvider {
    key: crate::model_workflow::GOAL_WORKFLOW_CONTEXT_KEY,
    project_required: false,
    scope_policy: ScopePolicy::Public,
    surface: Surface::AnySidecar,
    advertised: false,
    scope_unavailable_reason: SCOPE_UNAVAILABLE,
    provide: |context| {
        Box::pin(async move {
            context.available(
                context
                    .runtime
                    .model_workflow_policy
                    .goal_workflow_projection(),
            )
        })
    },
};
