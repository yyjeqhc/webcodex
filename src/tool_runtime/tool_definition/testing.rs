use super::AgentCapability::Shell;
use super::ToolVisibility::ModelVisible;
use super::{captures_validation_output, def, ToolDefinition};
use crate::tool_runtime::metadata::{ToolPathHint::None as NoPath, ToolRisk::JobRun, JOB_RUN};

pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    captures_validation_output(def(
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
    )),
    captures_validation_output(def(
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
    )),
    captures_validation_output(def(
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
    )),
];
