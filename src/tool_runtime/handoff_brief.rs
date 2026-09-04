//! Root compatibility facade for canonical Workflow Session handoff projection.

pub(crate) use webcodex_workflow_session::{build_handoff_brief, HandoffBriefInput};

#[cfg(test)]
pub(crate) use webcodex_workflow_session::{
    handoff_brief_size, HANDOFF_BRIEF_HARD_MAX_BYTES, HANDOFF_CHANGED_PATHS_MAX_ITEMS,
    HANDOFF_INSTRUCTION_MAX_CHARS, HANDOFF_NEXT_ACTIONS_MAX_ITEMS, HANDOFF_OPEN_FAILURES_MAX_ITEMS,
    HANDOFF_RECENT_FILES_MAX_ITEMS,
};
