//! Runtime tool lookup and policy helpers derived from ToolDefinition.

use super::metadata::{
    tool_metadata as fallback_tool_metadata, ToolMetadata, ToolPathHint, ToolRisk,
};
use super::tool_definition::{tool_definitions, AgentCapability, ToolDefinition};

impl ToolDefinition {
    pub(crate) fn metadata(self) -> ToolMetadata {
        self.metadata
    }

    pub(crate) fn session_risk_class(self) -> &'static str {
        self.metadata.risk.session_risk_class()
    }

    pub(crate) fn is_read_like(self) -> bool {
        self.metadata.read_only
    }

    pub(crate) fn is_write_like(self) -> bool {
        self.metadata.risk == ToolRisk::ProjectWrite
    }

    pub(crate) fn is_shell_like(self) -> bool {
        self.metadata.shell_like || self.metadata.risk == ToolRisk::JobRun
    }

    pub(crate) fn is_git_like(self) -> bool {
        self.policy.git_like
    }

    pub(crate) fn is_change_summary_like(self) -> bool {
        self.policy.change_summary_like
    }

    pub(crate) fn captures_validation_output(self) -> bool {
        self.policy.captures_validation_output
    }

    pub(crate) fn is_current_session_control(self) -> bool {
        self.policy.current_session_control
    }

    pub(crate) fn requires_explicit_business_session(self) -> bool {
        self.policy.requires_explicit_business_session
    }

    pub(crate) fn creates_or_binds_session(self) -> bool {
        self.policy.creates_or_binds_session
    }

    pub(crate) fn disabled_message(self) -> Option<&'static str> {
        self.policy.disabled_message
    }

    pub(crate) fn extra_accepted_flattened_args(self) -> &'static [&'static str] {
        self.policy.extra_accepted_flattened_args
    }

    pub(crate) fn uses_unit_arguments(self) -> bool {
        self.policy.unit_arguments
    }

    pub(crate) fn requires_artifact_upload_path_binding(self) -> bool {
        self.policy.requires_artifact_upload_path_binding
    }

    pub(crate) fn allows_current_session_fallback(self) -> bool {
        self.metadata.requires_project
            && !self.is_current_session_control()
            && !self.requires_explicit_business_session()
            && !self.creates_or_binds_session()
    }

    pub(crate) fn requires_session_project_escape(self) -> bool {
        metadata_requires_write_or_shell_boundary(self.metadata)
    }

    pub(crate) fn requires_permission(self) -> bool {
        metadata_requires_write_or_shell_boundary(self.metadata)
    }

    pub(crate) fn permission_risk(self) -> &'static str {
        if self.captures_validation_output() {
            return "validation";
        }
        if let Some(permission_risk) = self.policy.permission_risk {
            return permission_risk;
        }
        permission_risk_from_metadata(self.metadata)
    }
}

fn permission_risk_from_metadata(metadata: ToolMetadata) -> &'static str {
    if metadata.shell_like {
        return "shell";
    }
    if metadata.destructive {
        return "destructive";
    }
    if metadata.path_hint == ToolPathHint::Artifact {
        return "artifact_write";
    }
    if metadata.path_hint == ToolPathHint::Patch {
        return "patch";
    }
    if matches!(
        metadata.risk,
        ToolRisk::ProjectWrite | ToolRisk::AccountManage
    ) {
        return "write";
    }
    "write"
}

fn fallback_permission_risk(name: &str, metadata: ToolMetadata) -> &'static str {
    if name.contains("patch") && metadata.path_hint != ToolPathHint::Patch {
        return "patch";
    }
    permission_risk_from_metadata(metadata)
}

fn metadata_requires_write_or_shell_boundary(metadata: ToolMetadata) -> bool {
    !metadata.read_only || metadata.destructive || metadata.shell_like
}

pub(crate) fn lookup_tool_definition(name: &str) -> Option<&'static ToolDefinition> {
    tool_definitions().find(|definition| definition.name == name)
}

/// Returns `true` if `name` is a recognized runtime tool name. Public so the
/// HTTP/MCP adapters can decide whether to emit the rich "unknown tool" error.
pub fn is_known_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some()
}

pub(crate) fn known_tool_names() -> impl Iterator<Item = &'static str> {
    tool_definitions().map(|definition| definition.name)
}

pub(crate) fn runtime_tool_metadata(name: &str) -> ToolMetadata {
    lookup_tool_definition(name)
        .map(|definition| definition.metadata())
        .unwrap_or_else(|| fallback_tool_metadata(name))
}

pub(crate) fn runtime_tool_agent_capability(name: &str) -> Option<AgentCapability> {
    lookup_tool_definition(name)
        .unwrap_or_else(|| panic!("missing ToolDefinition for {name}"))
        .agent_capability
}

pub(crate) fn runtime_tool_category(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.category)
        .unwrap_or("other")
}

pub(crate) fn runtime_tool_session_risk_class(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.session_risk_class())
        .unwrap_or_else(|| fallback_tool_metadata(name).risk.session_risk_class())
}

pub(crate) fn runtime_tool_is_read_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_read_like())
        .unwrap_or_else(|| fallback_tool_metadata(name).read_only)
}

pub(crate) fn runtime_tool_is_write_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_write_like())
        .unwrap_or_else(|| fallback_tool_metadata(name).risk == ToolRisk::ProjectWrite)
}

pub(crate) fn runtime_tool_is_shell_like(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.is_shell_like())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            metadata.shell_like || metadata.risk == ToolRisk::JobRun
        })
}

pub(crate) fn runtime_tool_is_git_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_git_like())
}

pub(crate) fn runtime_tool_is_change_summary_like(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_change_summary_like())
}

pub(crate) fn runtime_tool_captures_validation_output(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.captures_validation_output())
}

pub(crate) fn runtime_tool_is_current_session_control(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.is_current_session_control())
}

pub(crate) fn runtime_tool_requires_explicit_business_session(name: &str) -> bool {
    lookup_tool_definition(name)
        .is_some_and(|definition| definition.requires_explicit_business_session())
}

pub(crate) fn runtime_tool_creates_or_binds_session(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.creates_or_binds_session())
}

pub(crate) fn runtime_tool_disabled_message(name: &str) -> Option<&'static str> {
    lookup_tool_definition(name).and_then(|definition| definition.disabled_message())
}

pub(crate) fn runtime_tool_extra_accepted_flattened_args(name: &str) -> &'static [&'static str] {
    lookup_tool_definition(name)
        .map_or(&[], |definition| definition.extra_accepted_flattened_args())
}

pub(crate) fn runtime_tool_allows_current_session_fallback(name: &str) -> bool {
    lookup_tool_definition(name)
        .is_some_and(|definition| definition.allows_current_session_fallback())
}

pub(crate) fn runtime_tool_requires_session_project_escape(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.requires_session_project_escape())
        .unwrap_or_else(|| metadata_requires_write_or_shell_boundary(fallback_tool_metadata(name)))
}

pub(crate) fn runtime_tool_requires_permission(name: &str) -> bool {
    lookup_tool_definition(name)
        .map(|definition| definition.requires_permission())
        .unwrap_or_else(|| metadata_requires_write_or_shell_boundary(fallback_tool_metadata(name)))
}

pub(crate) fn runtime_tool_permission_risk(name: &str) -> &'static str {
    lookup_tool_definition(name)
        .map(|definition| definition.permission_risk())
        .unwrap_or_else(|| {
            let metadata = fallback_tool_metadata(name);
            fallback_permission_risk(name, metadata)
        })
}

pub(crate) fn is_model_visible_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_visible())
}

pub(crate) fn is_model_hidden_tool_name(name: &str) -> bool {
    lookup_tool_definition(name).is_some_and(|definition| definition.visibility.is_model_hidden())
}

pub(crate) fn model_hidden_tool_names() -> impl Iterator<Item = &'static str> {
    tool_definitions()
        .filter(|definition| definition.visibility.is_model_hidden())
        .map(|definition| definition.name)
}

pub(crate) fn model_visible_tool_definitions() -> impl Iterator<Item = &'static ToolDefinition> {
    tool_definitions().filter(|definition| definition.visibility.is_model_visible())
}

pub(crate) fn model_visible_tool_names_csv() -> String {
    model_visible_tool_definitions()
        .map(|definition| definition.name)
        .collect::<Vec<_>>()
        .join(", ")
}
