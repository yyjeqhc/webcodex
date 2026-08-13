use super::AgentCapability::ComputerObserve;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition, TOOL_CATEGORY_COMPUTER};
use crate::tool_runtime::metadata::{
    ToolPathHint::None, ToolRisk::ReadOnly, COMPUTER_READ, TOOL_PROVIDER_AGENT,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "computer_list_windows",
        ModelVisible,
        TOOL_CATEGORY_COMPUTER,
        Some(ComputerObserve),
        TOOL_PROVIDER_AGENT,
        ReadOnly,
        Some(COMPUTER_READ),
        false,
        None,
        false,
        false,
    ),
    def(
        "computer_snapshot",
        ModelVisible,
        TOOL_CATEGORY_COMPUTER,
        Some(ComputerObserve),
        TOOL_PROVIDER_AGENT,
        ReadOnly,
        Some(COMPUTER_READ),
        false,
        None,
        false,
        false,
    ),
];
