//! Search budget, sparse output, and actionable continuation are one ordered
//! projection, using the Project already selected by canonical governance.

use super::super::search_project_texts;
use super::registry::{CapturedProjection, ResultProjection};
use super::{ResolvedProject, SearchModelProjection, ToolCall, ToolResult};

pub(super) fn capture(call: &ToolCall) -> Option<CapturedProjection> {
    if matches!(call, ToolCall::SearchProjectTexts { .. }) {
        Some(Box::new(SearchModelProjection::capture(call)))
    } else {
        None
    }
}

impl ResultProjection for SearchModelProjection {
    fn bind_resolved_project(&mut self, resolved: Option<&ResolvedProject>) {
        if let (Some(resolved), Self::Batch { project, .. }) = (resolved, self) {
            *project = resolved.resolved_id.clone();
        }
    }

    fn project(self: Box<Self>, result: &mut ToolResult) {
        if let Self::Batch {
            project,
            queries,
            session_id,
            default_timeouts,
            max_result_bytes,
        } = self.as_ref()
        {
            search_project_texts::apply_model_facing_output_budget(
                result,
                default_timeouts,
                *max_result_bytes,
                project,
                queries,
                session_id.as_deref(),
            );
            search_project_texts::enforce_final_model_facing_hard_cap(
                result,
                default_timeouts,
                project,
                queries,
                session_id.as_deref(),
                *max_result_bytes,
            );
        }
        super::sparsify_search_success_for_model(&self, result);
        if let Self::Batch {
            project,
            queries,
            session_id,
            max_result_bytes,
            ..
        } = self.as_ref()
        {
            search_project_texts::add_actionable_search_continuation(
                result,
                project,
                queries,
                session_id.as_deref(),
                *max_result_bytes,
            );
        }
    }
}
