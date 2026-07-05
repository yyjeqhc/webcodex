use super::ToolVisibility::ModelVisible;
use super::{creates_or_binds_session, current_session_control, def, ToolDefinition};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath, ToolRisk::ReadOnly, PROJECT_READ,
};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    creates_or_binds_session(current_session_control(def(
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
    ))),
    current_session_control(def(
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
    )),
    current_session_control(def(
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
    )),
];
