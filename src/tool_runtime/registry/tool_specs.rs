use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod computer;
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
    ToolDefinition,
};
use super::super::tool_spec::ToolSpec;
use super::{output_schema_for_tool, tool_annotations};
use std::collections::BTreeMap;

pub(crate) fn registered_tool_specs() -> Vec<ToolSpec> {
    resolve_tool_specs(
        model_visible_tool_definitions(),
        separate_tool_spec_declarations_by_name(),
    )
}

fn resolve_tool_specs<'a>(
    definitions: impl IntoIterator<Item = &'a ToolDefinition>,
    mut separate_declarations_by_name: BTreeMap<String, ToolSpec>,
) -> Vec<ToolSpec> {
    let mut specs = Vec::new();
    let mut seen_definition_names = std::collections::BTreeSet::new();
    for definition in definitions {
        if !seen_definition_names.insert(definition.name) {
            panic!(
                "{} model-visible ToolDefinition is duplicated",
                definition.name
            );
        }
        let spec = if let Some(model_spec) = definition.model_spec {
            if separate_declarations_by_name
                .remove(definition.name)
                .is_some()
            {
                panic!(
                    "{} ToolDefinition model spec duplicates a separate ToolSpec declaration",
                    definition.name
                );
            }
            tool_spec(
                definition.name,
                model_spec.description,
                (model_spec.input_schema)(),
            )
        } else {
            separate_declarations_by_name
                .remove(definition.name)
                .unwrap_or_else(|| {
                    panic!(
                        "{} public ToolDefinition is missing a ToolSpec declaration",
                        definition.name
                    )
                })
        };
        specs.push(spec);
    }
    if let Some(extra_name) = separate_declarations_by_name.keys().next() {
        panic!("{extra_name} separate ToolSpec declaration has no model-visible ToolDefinition");
    }
    specs
}

// Domains migrate off this list as their ToolDefinition rows gain embedded
// description + input-schema ownership. Keep output schemas and annotations
// independent: `tool_spec()` still derives those by the canonical tool name.
fn separate_tool_spec_declarations() -> Vec<ToolSpec> {
    let mut declarations = sessions::tool_specs();
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

fn separate_tool_spec_declarations_by_name() -> BTreeMap<String, ToolSpec> {
    let mut declarations_by_name = BTreeMap::new();
    for spec in separate_tool_spec_declarations() {
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

    fn collect_schema_descriptions<'a>(value: &'a Value, out: &mut Vec<&'a str>) {
        match value {
            Value::Object(map) => {
                if let Some(description) = map.get("description").and_then(Value::as_str) {
                    out.push(description);
                }
                for nested in map.values() {
                    collect_schema_descriptions(nested, out);
                }
            }
            Value::Array(values) => {
                for nested in values {
                    collect_schema_descriptions(nested, out);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn separate_tool_spec_declarations_are_unique_and_model_visible() {
        let declarations = separate_tool_spec_declarations();
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
    fn discovery_model_specs_are_owned_by_tool_definitions() {
        let separate_names = separate_tool_spec_declarations()
            .into_iter()
            .map(|spec| spec.name)
            .collect::<BTreeSet<_>>();
        for name in [
            "list_tools",
            "list_projects",
            "register_project",
            "unregister_project",
            "create_project",
            "list_agents",
            "runtime_status",
            "tool_manifest",
        ] {
            let definition = lookup_tool_definition(name)
                .unwrap_or_else(|| panic!("missing migrated ToolDefinition for {name}"));
            assert!(
                definition.model_spec.is_some(),
                "{name} must own description + input schema through ToolDefinition"
            );
            assert!(
                !separate_names.contains(name),
                "{name} must not keep a parallel ToolSpec registration"
            );
        }
    }

    #[test]
    fn register_project_definition_owned_model_spec_matches_exact_contract() {
        let actual = registered_tool_specs()
            .into_iter()
            .find(|spec| spec.name == "register_project")
            .expect("register_project spec");
        let expected = tool_spec(
            "register_project",
            "Register an existing directory, including a non-Git or ad-hoc workspace, as a Project on one Runner. Use this when the directory already exists; policy still bounds allowed paths.",
            super::super::input_schemas::register_project_input_schema(),
        );
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            serde_json::to_value(expected).unwrap(),
            "definition-owned register_project ToolSpec must preserve name, description, input/output schemas, and annotations exactly"
        );
    }

    #[test]
    #[should_panic(
        expected = "register_project public ToolDefinition is missing a ToolSpec declaration"
    )]
    fn migrated_model_spec_omission_fails_closed() {
        let mut definition =
            *lookup_tool_definition("register_project").expect("register_project ToolDefinition");
        assert!(definition.model_spec.is_some());
        definition.model_spec = None;
        let _ = resolve_tool_specs(std::iter::once(&definition), BTreeMap::new());
    }

    #[test]
    #[should_panic(expected = "register_project model-visible ToolDefinition is duplicated")]
    fn duplicate_definition_owned_model_specs_fail_closed() {
        let definition =
            *lookup_tool_definition("register_project").expect("register_project ToolDefinition");
        assert!(definition.model_spec.is_some());
        let duplicate = definition;
        let _ = resolve_tool_specs([&definition, &duplicate], BTreeMap::new());
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
            "unregister_project",
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
            let description = &find(name).description;
            assert!(
                description.contains("shared Job concurrency"),
                "{name}: {description}"
            );
            assert!(
                description.contains("host_context"),
                "{name}: {description}"
            );
            assert!(description.contains("advisory"), "{name}: {description}");
            assert!(description.contains("authority"), "{name}: {description}");
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

        for spec in &specs {
            let mut descriptions = Vec::new();
            collect_schema_descriptions(&spec.input_schema, &mut descriptions);
            collect_schema_descriptions(&spec.output_schema, &mut descriptions);
            for description in descriptions {
                if spec.name != "run_shell" {
                    assert!(
                        !description.contains("run_shell"),
                        "{} schema pollutes exact run_shell discovery: {description}",
                        spec.name
                    );
                }
                if spec.name != "run_job" {
                    assert!(
                        !description.contains("run_job"),
                        "{} schema pollutes exact run_job discovery: {description}",
                        spec.name
                    );
                }
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
