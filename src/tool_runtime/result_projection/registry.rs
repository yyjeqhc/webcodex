//! Immutable, bundled-only projection selection. No callback receives a runtime,
//! dispatcher, Session store, permission evaluator, or canonical audit output.

use super::{ResolvedProject, ToolCall, ToolResult};

pub(super) trait ResultProjection: Send + Sync {
    fn bind_resolved_project(&mut self, _resolved: Option<&ResolvedProject>) {}

    /// Consume request-local state exactly once after canonical recording.
    fn project(self: Box<Self>, result: &mut ToolResult);
}

pub(super) type CapturedProjection = Box<dyn ResultProjection>;
type CaptureProjector = fn(&ToolCall) -> Option<CapturedProjection>;

struct ResultProjectionRegistry {
    projectors: &'static [CaptureProjector],
}

impl ResultProjectionRegistry {
    fn capture(&self, call: &ToolCall) -> ModelFacingProjectionPlan {
        ModelFacingProjectionPlan {
            projection: self.projectors.iter().find_map(|capture| capture(call)),
        }
    }
}

static BUILTIN_RESULT_PROJECTORS: ResultProjectionRegistry = ResultProjectionRegistry {
    projectors: &[
        super::waits::capture_agent_wait,
        super::waits::capture_job_readiness,
        super::edits::capture,
        super::execution::capture,
        super::reads::capture,
        super::searches::capture,
    ],
};

/// Request facts needed only after canonical execution/recording has finished.
/// The private, owned projection cannot be cloned or applied twice. Unmatched
/// tools retain the existing no-op path without allocating projection state.
pub(in crate::tool_runtime) struct ModelFacingProjectionPlan {
    projection: Option<CapturedProjection>,
}

impl ModelFacingProjectionPlan {
    pub(in crate::tool_runtime) fn capture(call: &ToolCall) -> Self {
        BUILTIN_RESULT_PROJECTORS.capture(call)
    }

    pub(in crate::tool_runtime) fn bind_resolved_project(
        &mut self,
        resolved: Option<&ResolvedProject>,
    ) {
        if let Some(projection) = &mut self.projection {
            projection.bind_resolved_project(resolved);
        }
    }

    /// Session/audit recorders must run before this terminal model-facing stage.
    pub(in crate::tool_runtime) fn project(self, result: &mut ToolResult) {
        if let Some(projection) = self.projection {
            projection.project(result);
        }
    }
}

#[cfg(test)]
#[path = "../tests/result_projection_registry.rs"]
mod tests;
