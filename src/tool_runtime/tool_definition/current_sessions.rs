use super::ToolVisibility::ModelVisible;
use super::{
    creates_or_binds_session, current_session_control, def, ToolDefinition, TOOL_CATEGORY_SESSION,
};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ, TOOL_PROVIDER_CONTROL,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    creates_or_binds_session(current_session_control(def(
        "bind_current_session",
        ModelVisible,
        TOOL_CATEGORY_SESSION,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ))),
    current_session_control(def(
        "current_session",
        ModelVisible,
        TOOL_CATEGORY_SESSION,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    )),
    current_session_control(def(
        "unbind_current_session",
        ModelVisible,
        TOOL_CATEGORY_SESSION,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    )),
];
