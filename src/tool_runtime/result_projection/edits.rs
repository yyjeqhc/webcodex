//! Guarded edit projection retains only the original request facts it needs.

use super::registry::{CapturedProjection, ResultProjection};
use super::{ToolCall, ToolResult};

struct EditProjection {
    change_count: usize,
    dry_run: bool,
}

pub(super) fn capture(call: &ToolCall) -> Option<CapturedProjection> {
    let ToolCall::ApplyTextEdits {
        changes, dry_run, ..
    } = call
    else {
        return None;
    };
    Some(Box::new(EditProjection {
        change_count: changes.len(),
        dry_run: dry_run.unwrap_or(false),
    }))
}

impl ResultProjection for EditProjection {
    fn project(self: Box<Self>, result: &mut ToolResult) {
        super::apply_text_edits_model_projection(result, self.change_count, self.dry_run);
    }
}
