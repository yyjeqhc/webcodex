use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "bind_current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "unbind_current_session",
        ModelVisible,
        "session",
        None,
        "control",
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
];
