use super::AgentCapability::{FileRead, Shell};
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_FILE, TOOL_CATEGORY_PROJECT};
use crate::tool_runtime::metadata::{
    ToolPathHint::{None as NoPath, SinglePath},
    ToolRisk::ReadOnly,
    PROJECT_READ, TOOL_PROVIDER_AGENT,
};

pub(super) const SEARCH_DEFINITIONS: &[ToolDefinition] = &[
    def(
        "project_overview",
        ModelVisible,
        TOOL_CATEGORY_PROJECT,
        Some(FileRead),
        TOOL_PROVIDER_AGENT,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "list_project_files",
        ModelVisible,
        TOOL_CATEGORY_FILE,
        Some(FileRead),
        TOOL_PROVIDER_AGENT,
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
        TOOL_PROVIDER_AGENT,
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
    TOOL_PROVIDER_AGENT,
    ReadOnly,
    Some(PROJECT_READ),
    true,
    SinglePath,
    false,
    false,
)];
