use super::AgentCapability::{FileRead, Shell};
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_FILE};
use crate::tool_runtime::metadata::{
    ToolPathHint::{None as NoPath, SinglePath},
    ToolRisk::ReadOnly,
    PROJECT_READ,
};

pub(super) const SEARCH_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "list_project_files",
        ModelVisible,
        TOOL_CATEGORY_FILE,
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
        TOOL_CATEGORY_FILE,
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

pub(super) const READ_DEFINITIONS: &[ToolDefinition] = &[def(
    "read_file",
    ModelVisible,
    TOOL_CATEGORY_FILE,
    Some(FileRead),
    "agent",
    ReadOnly,
    Some(PROJECT_READ),
    true,
    SinglePath,
    false,
    false,
)];
