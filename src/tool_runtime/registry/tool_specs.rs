use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod computer;
mod discovery;
mod edits;
mod files;
mod git;
mod hygiene;
mod jobs;
mod lsp;
mod sessions;
mod testing;

use super::super::tool_definition::{
    is_model_visible_tool_name, lookup_tool_definition, model_visible_tool_definitions,
};
use super::super::tool_spec::ToolSpec;
use super::{output_schema_for_tool, tool_annotations};
use std::collections::BTreeMap;

pub(crate) fn registered_tool_specs() -> Vec<ToolSpec> {
    let mut declarations_by_name = tool_spec_declarations_by_name();
    let specs = model_visible_tool_definitions()
        .map(|definition| {
            declarations_by_name
                .remove(definition.name)
                .unwrap_or_else(|| {
                    panic!(
                        "{} public ToolDefinition is missing a ToolSpec declaration",
                        definition.name
                    )
                })
        })
        .collect::<Vec<_>>();
    if let Some(extra_name) = declarations_by_name.keys().next() {
        panic!("{extra_name} ToolSpec declaration has no model-visible ToolDefinition");
    }
    specs
}

fn tool_spec_declarations() -> Vec<ToolSpec> {
    let mut declarations = discovery::tool_specs();
    declarations.extend(sessions::tool_specs());
    declarations.extend(jobs::tool_specs());
    declarations.extend(checkpoints::tool_specs());
    declarations.extend(coding_tasks::tool_specs());
    declarations.extend(computer::tool_specs());
    declarations.extend(hygiene::tool_specs());
    declarations.extend(files::tool_specs());
    declarations.extend(lsp::tool_specs());
    declarations.extend(git::tool_specs());
    declarations.extend(testing::tool_specs());
    declarations.extend(artifacts::tool_specs());
    declarations.extend(edits::tool_specs());
    declarations
}

fn tool_spec_declarations_by_name() -> BTreeMap<String, ToolSpec> {
    let mut declarations_by_name = BTreeMap::new();
    for spec in tool_spec_declarations() {
        if !is_model_visible_tool_name(&spec.name) {
            panic!("{} ToolSpec declaration must be model-visible", spec.name);
        }
        let name = spec.name.clone();
        if declarations_by_name.insert(name.clone(), spec).is_some() {
            panic!("{name} ToolSpec declaration is duplicated");
        }
    }
    declarations_by_name
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
    use crate::tool_runtime::tool_definition::is_model_visible_tool_name;
    use std::collections::BTreeSet;

    #[test]
    fn tool_spec_declarations_are_unique_and_model_visible() {
        let declarations = tool_spec_declarations();
        let mut names = BTreeSet::new();
        for spec in declarations {
            assert!(
                is_model_visible_tool_name(&spec.name),
                "{} ToolSpec declaration must be model-visible",
                spec.name
            );
            assert!(
                names.insert(spec.name.clone()),
                "{} ToolSpec declaration is duplicated",
                spec.name
            );
        }
    }

    #[test]
    fn reliability_tool_descriptions_are_selection_dense_and_keep_lifecycle_contracts() {
        let specs = registered_tool_specs();
        let find = |name: &str| {
            specs
                .iter()
                .find(|spec| spec.name == name)
                .unwrap_or_else(|| panic!("missing tool spec: {name}"))
        };

        for name in [
            "run_process",
            "run_script",
            "run_shell",
            "open_session_shell",
            "run_job",
            "stop_job",
            "job_status",
            "job_log",
            "observe_jobs",
            "list_jobs",
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "go_test",
            "register_project",
            "create_project",
            "list_agents",
            "runtime_status",
            "work_on_project",
        ] {
            let description = &find(name).description;
            assert!(
                description.chars().count() <= 220,
                "{name} description is too diffuse ({} chars): {description}",
                description.chars().count()
            );
        }

        for name in [
            "run_process",
            "run_script",
            "cargo_fmt",
            "cargo_check",
            "cargo_test",
            "go_test",
        ] {
            let description = &find(name).description;
            assert!(
                description.contains("same execution"),
                "{name}: {description}"
            );
            assert!(!description.contains("run_shell"), "{name}: {description}");
            assert!(!description.contains("run_job"), "{name}: {description}");
        }

        let run_job = &find("run_job").description;
        assert!(run_job.contains("stable job_id"), "{run_job}");
        assert!(run_job.contains("observe"), "{run_job}");
        assert!(run_job.contains("retry"), "{run_job}");

        for name in ["job_status", "job_log", "observe_jobs"] {
            let description = &find(name).description;
            let lower = description.to_ascii_lowercase();
            assert!(lower.contains("never"), "{name}: {description}");
            assert!(lower.contains("retr"), "{name}: {description}");
        }

        let job_log = find("job_log");
        let token = job_log.input_schema["properties"]["after_observation_token"]["description"]
            .as_str()
            .expect("job_log observation token description");
        assert!(token.contains("not execution identity"), "{token}");
        assert!(token.contains("Server epoch"), "{token}");
        let wait = job_log.input_schema["properties"]["wait_secs"]["description"]
            .as_str()
            .expect("job_log wait description");
        assert!(wait.contains("not a subscription"), "{wait}");

        let register = find("register_project");
        assert!(
            register.description.contains("non-Git"),
            "{}",
            register.description
        );
        assert_eq!(
            register.input_schema["properties"]["path"]["description"].as_str(),
            Some("Existing absolute directory path on the Runner. Git is not required.")
        );
        assert!(find("work_on_project")
            .description
            .contains("Git not required"));
        for name in ["list_agents", "runtime_status"] {
            assert!(find(name).description.contains("shared Job concurrency"));
        }

        for spec in &specs {
            if spec.name != "run_shell" {
                assert!(
                    !spec.description.contains("run_shell"),
                    "{} pollutes exact run_shell discovery: {}",
                    spec.name,
                    spec.description
                );
            }
            if spec.name != "run_job" {
                assert!(
                    !spec.description.contains("run_job"),
                    "{} pollutes exact run_job discovery: {}",
                    spec.name,
                    spec.description
                );
            }
        }
    }

    #[test]
    fn tool_specs_patch_fields_reject_codex_wrapper() {
        let specs = registered_tool_specs();
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
