//! Read snapshots retain their exact resolved Project and continuation fences.

use super::super::read_files::{self, ReadModelProjection};
use super::registry::{CapturedProjection, ResultProjection};
use super::{ResolvedProject, ToolCall, ToolResult};

pub(super) fn capture(call: &ToolCall) -> Option<CapturedProjection> {
    if matches!(call, ToolCall::ReadFiles { .. }) {
        Some(Box::new(ReadModelProjection::capture(call)))
    } else {
        None
    }
}

impl ResultProjection for ReadModelProjection {
    fn bind_resolved_project(&mut self, resolved: Option<&ResolvedProject>) {
        ReadModelProjection::bind_resolved_project(self, resolved);
    }

    fn project(self: Box<Self>, result: &mut ToolResult) {
        let ReadModelProjection::Batch {
            max_result_bytes, ..
        } = self.as_ref()
        else {
            return;
        };
        read_files::apply_model_facing_output_budget(result, *max_result_bytes, &self);
        read_files::enforce_final_model_facing_hard_cap(result, &self);
        read_files::add_actionable_read_continuations(&self, result);
        super::sparsify_complete_read_success("read_files", result);
    }
}
