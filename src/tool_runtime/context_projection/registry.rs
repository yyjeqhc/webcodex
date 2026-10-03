//! Bundled-only registration. The immutable slice has deterministic discovery,
//! no runtime lock or reload lifecycle, and keeps policy beside the callable.

use super::super::project_instructions::ProjectInstructionsSnapshot;
use super::super::project_resolution::ResolvedProject;
use super::super::ToolRuntime;
use super::{
    providers, unavailable, ContextMaterialCapabilities, ContextProjectionMeasure,
    MAX_CONTEXT_PROJECTION_BYTES,
};
use crate::auth::AuthContext;
use crate::client_window::ClientWindow;
use crate::json_measurement::serialized_json_len;
use futures_util::future::BoxFuture;
use serde_json::{json, Value};

#[derive(Clone, Copy)]
pub(super) enum ScopePolicy {
    Public,
    Require(&'static str),
    RequireAll(&'static [&'static str]),
}

#[derive(Clone, Copy)]
pub(super) enum Surface {
    AnySidecar,
    SkillRuntime,
    Memory,
}

pub(super) struct ContextMaterialProvider {
    pub key: &'static str,
    pub project_required: bool,
    pub scope_policy: ScopePolicy,
    pub surface: Surface,
    pub advertised: bool,
    pub scope_unavailable_reason: &'static str,
    pub provide: for<'a> fn(ContextMaterialContext<'a>) -> BoxFuture<'a, Value>,
}

impl ContextMaterialProvider {
    // Gate before invoking a provider, including when a cached instruction
    // snapshot is present. Registration never grants project/scope authority.
    pub async fn project(
        &self,
        context: ContextMaterialContext<'_>,
        capabilities: ContextMaterialCapabilities,
    ) -> Value {
        let surface_available = match self.surface {
            Surface::AnySidecar => true,
            Surface::SkillRuntime => capabilities.skill_runtime,
            Surface::Memory => capabilities.memory_surface,
        };
        if !surface_available {
            return unavailable(self.key, "context_material_surface_unavailable");
        }
        if self.project_required && context.project.is_none() {
            return unavailable(self.key, "project_target_unavailable");
        }
        let scope_available = match self.scope_policy {
            ScopePolicy::Public => true,
            ScopePolicy::Require(scope) => context.auth.is_some_and(|auth| auth.has_scope(scope)),
            ScopePolicy::RequireAll(scopes) => context
                .auth
                .is_some_and(|auth| scopes.iter().all(|scope| auth.has_scope(scope))),
        };
        if !scope_available {
            return unavailable(self.key, self.scope_unavailable_reason);
        }
        (self.provide)(context).await
    }
}

pub(super) struct ContextMaterialContext<'a> {
    pub runtime: &'a ToolRuntime,
    pub key: &'static str,
    pub project: Option<&'a ResolvedProject>,
    pub auth: Option<&'a AuthContext>,
    pub window: Option<&'a ClientWindow>,
    pub instructions: Option<&'a ProjectInstructionsSnapshot>,
    pub budget: ContextProjectionBudget<'a>,
}

impl ContextMaterialContext<'_> {
    pub fn project(&self) -> &ResolvedProject {
        self.project
            .expect("registered provider requires project target")
    }

    pub fn available(&self, projection: Value) -> Value {
        json!({"key": self.key, "status": "available", "projection": projection})
    }

    pub fn project_result(&self, result: Result<Value, &'static str>) -> Value {
        match result {
            Ok(projection) => self.available(projection),
            Err(reason) => unavailable(self.key, reason),
        }
    }
}

/// The instruction provider must reserve the complete prospective envelope,
/// including the public workflow and omission receipts, without invoking later
/// providers speculatively. All other material bounds remain with their domains.
pub(super) struct ContextProjectionBudget<'a> {
    pub preceding: &'a [Value],
    pub remaining: &'a [&'a str],
    pub workflow: Option<&'a Value>,
    pub truncated: bool,
}

impl ContextProjectionBudget<'_> {
    pub fn for_instruction_projection(&self, material_with_null_projection: &Value) -> usize {
        let mut materials = self.preceding.to_vec();
        materials.push(material_with_null_projection.clone());
        for key in self.remaining {
            materials.push(if *key == providers::WORKFLOW.key {
                self.workflow.expect("requested workflow").clone()
            } else {
                unavailable(key, "context_projection_budget_exceeded")
            });
        }
        let reserved = serialized_json_len(&ContextProjectionMeasure {
            materials: &materials,
            truncated: self.truncated,
        })
        .unwrap_or(usize::MAX)
        .saturating_sub(4); // Replace the literal JSON null.
        MAX_CONTEXT_PROJECTION_BYTES.saturating_sub(reserved)
    }
}

pub(super) struct ContextMaterialRegistry {
    providers: &'static [&'static ContextMaterialProvider],
}

impl ContextMaterialRegistry {
    pub fn get(&self, key: &str) -> Option<&'static ContextMaterialProvider> {
        self.providers
            .iter()
            .copied()
            .find(|provider| provider.key == key)
    }

    pub fn advertised_keys(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.providers
            .iter()
            .filter(|provider| provider.advertised)
            .map(|provider| provider.key)
    }
}

pub(super) static BUILTIN_CONTEXT_MATERIALS: ContextMaterialRegistry = ContextMaterialRegistry {
    providers: &[
        &providers::PROJECT_INSTRUCTIONS,
        &providers::WORKFLOW,
        &providers::WORKFLOW_RESUME,
        &providers::JOBS_ATTENTION,
        &providers::SKILLS_CATALOG,
        &providers::PLUGINS_CATALOG,
        &providers::MEMORY_BOOTSTRAP,
        &providers::GOAL_WORKFLOW,
    ],
};

#[cfg(test)]
#[path = "../tests/context_projection/registry.rs"]
mod tests;
