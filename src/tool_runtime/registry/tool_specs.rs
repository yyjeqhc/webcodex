use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod discovery;
mod edits;
mod files;
mod git;
mod hygiene;
mod jobs;
mod sessions;
mod testing;

use super::super::tool_definition::{lookup_tool_definition, model_visible_tool_definitions};
use super::super::tool_spec::ToolSpec;
use super::super::ToolRuntime;
use super::input_schemas::with_common_testing_metadata;
use super::{output_schema_for_tool, tool_annotations};

impl ToolRuntime {
    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        let mut declarations = discovery::tool_specs();
        declarations.extend(sessions::tool_specs());
        declarations.extend(jobs::tool_specs());
        declarations.extend(checkpoints::tool_specs());
        declarations.extend(coding_tasks::tool_specs());
        declarations.extend(hygiene::tool_specs());
        declarations.extend(files::tool_specs());
        declarations.extend(git::tool_specs());
        declarations.extend(testing::tool_specs());
        declarations.extend(artifacts::tool_specs());
        declarations.extend(edits::tool_specs());
        debug_assert!(
            declarations
                .iter()
                .all(|spec| super::super::tool_definition::is_model_visible_tool_name(&spec.name)),
            "ToolSpec declarations must only include model-visible tools"
        );
        model_visible_tool_definitions()
            .map(|definition| {
                declarations
                    .iter()
                    .find(|spec| spec.name == definition.name)
                    .unwrap_or_else(|| {
                        panic!(
                            "{} public ToolDefinition is missing a ToolSpec declaration",
                            definition.name
                        )
                    })
                    .clone()
            })
            .map(with_common_testing_metadata)
            .collect()
    }

    /// The sorted list of accepted runtime tool names (mirrors `tool_specs`).
    #[cfg(test)]
    pub fn tool_names(&self) -> Vec<String> {
        model_visible_tool_definitions()
            .map(|definition| definition.name.to_string())
            .collect()
    }
}

pub(super) fn tool_spec(
    name: &'static str,
    description: impl Into<String>,
    input_schema: Value,
) -> ToolSpec {
    debug_assert!(
        lookup_tool_definition(name).is_some(),
        "{name} ToolSpec is missing a ToolDefinition"
    );
    ToolSpec {
        name: name.to_string(),
        description: description.into(),
        input_schema,
        output_schema: output_schema_for_tool(name),
        annotations: tool_annotations(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CodexConfig;
    use crate::projects::ProjectsState;
    use crate::shell_client::ShellClientRegistry;
    use crate::tool_runtime::RuntimeInfo;
    use std::sync::Arc;

    fn test_runtime() -> ToolRuntime {
        ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "projects not configured for test".to_string(),
                "test".to_string(),
            )),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(RuntimeInfo::default()),
        )
    }

    #[test]
    fn tool_specs_patch_fields_reject_codex_wrapper() {
        let runtime = test_runtime();
        let specs = runtime.tool_specs();
        for tool in ["apply_patch", "apply_patch_checked", "validate_patch"] {
            let spec = specs
                .iter()
                .find(|spec| spec.name == tool)
                .unwrap_or_else(|| panic!("missing tool spec: {tool}"));
            let description = spec.input_schema["properties"]["patch"]["description"]
                .as_str()
                .unwrap_or_else(|| panic!("missing patch description for {tool}"));
            assert!(
                description.contains("raw standard unified diff"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("Codex apply_patch wrapper"),
                "{tool}: {description}"
            );
            assert!(
                description.contains("*** Begin Patch"),
                "{tool}: {description}"
            );
        }
    }
}
