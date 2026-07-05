use super::AgentCapability::{FileRead, Shell};
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ,
};

pub(super) const SEARCH_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "list_project_files",
        ModelVisible,
        "file",
        Some(FileRead),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "search_project_text",
        ModelVisible,
        "file",
        Some(Shell),
        "agent",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
];
