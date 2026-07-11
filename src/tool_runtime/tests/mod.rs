//! Domain-organized test modules for tool_runtime.

mod support;

mod apply_text_edits;
mod checkpoint;
mod coding_task;
mod coding_task_semantic_navigation;
mod dispatch;
mod files;
mod files_helpers;
mod files_line_edit;
mod git;
mod handoff;
mod hygiene;
mod jobs;
mod lsp;
mod metadata;
mod schema;
mod sessions;
mod sessions_current;
mod sessions_git;
mod sessions_guards;
mod sessions_instructions;
mod sessions_resolver;
mod sync_timeout;
mod tool_call;
mod validation_events;
mod validation_parser;
mod validation_profile;
mod validation_summary;
