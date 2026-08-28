use super::ToolVisibility::ModelHidden;
use super::{def, ToolDefinition, TOOL_CATEGORY_RUNTIME};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{MemoryManage, ReadOnly},
    PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_CONTROL,
};

/// Fixed Control-owned project Memory runtime contract. These tools remain
/// globally hidden and are projected only by the capable Stateless MCP Full
/// Operator surface; the kernel capability gate is authoritative.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    def(
        "memory_search",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "memory_read",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(PROJECT_READ),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "memory_set",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        MemoryManage,
        Some(PROJECT_WRITE),
        true,
        NoPath,
        false,
        false,
    ),
    def(
        "memory_delete",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        MemoryManage,
        Some(PROJECT_WRITE),
        true,
        NoPath,
        true,
        false,
    ),
];
