use super::ToolVisibility::ModelHidden;
use super::{def, require_all_scopes, ToolDefinition, TOOL_CATEGORY_RUNTIME};
use crate::auth::scopes::{MEMORY_MANAGE_SCOPES, MEMORY_READ_SCOPES};
use crate::tool_runtime::metadata::{
    ToolPathHint::None as NoPath,
    ToolRisk::{MemoryManage, ReadOnly},
    ADMIN, PROJECT_READ, PROJECT_WRITE, TOOL_PROVIDER_CONTROL,
};

/// Fixed Control-owned project Memory runtime contract. These tools remain
/// globally hidden and are projected only by the capable Stateless MCP Full
/// Operator surface; the kernel capability gate is authoritative.
pub(super) const DEFINITIONS: &[ToolDefinition] = &[
    require_all_scopes(
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
        MEMORY_READ_SCOPES,
    ),
    require_all_scopes(
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
        MEMORY_READ_SCOPES,
    ),
    require_all_scopes(
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
        MEMORY_MANAGE_SCOPES,
    ),
    require_all_scopes(
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
        MEMORY_MANAGE_SCOPES,
    ),
    def(
        "memory_scope_list",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        ReadOnly,
        Some(ADMIN),
        false,
        NoPath,
        false,
        false,
    ),
    def(
        "memory_scope_purge",
        ModelHidden,
        TOOL_CATEGORY_RUNTIME,
        None,
        TOOL_PROVIDER_CONTROL,
        MemoryManage,
        Some(ADMIN),
        false,
        NoPath,
        true,
        false,
    ),
];
