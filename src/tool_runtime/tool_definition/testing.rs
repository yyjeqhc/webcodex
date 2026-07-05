use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{def, ToolDefinition};
use crate::tool_runtime::metadata::{ToolPathHint::None as NoPath, ToolRisk::JobRun, JOB_RUN};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "cargo_fmt",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "cargo_check",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "cargo_test",
        ModelVisible,
        "validation",
        Some(Shell),
        "agent",
        JobRun,
        Some(JOB_RUN),
        true,
        NoPath,
        false,
        false,
    ),
];
