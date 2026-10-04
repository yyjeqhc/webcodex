use serde_json::{json, Value};

mod agent_tasks;
mod agent_waits;
mod artifacts;
mod browser;
#[cfg(feature = "workspace-checkpoints")]
mod checkpoints;
#[cfg(feature = "experimental-code-mode")]
mod code_mode;
mod coding_agents;
mod coding_tasks;
mod common;
mod communication;
mod computer;
mod discovery;
mod edits;
mod files;
mod git;
mod goals;
mod hygiene;
mod jobs;
mod lsp;
mod memory;
mod projects;
mod resource_references;
mod runner_config;
mod sessions;
mod skills;
mod ssh_resources;
mod testing;

use common::default_output_schema;
pub use common::{
    continuation_semantics_schema, suggested_tool_call_schema, suggested_tool_call_schema_target,
};

fn base_output_schema_for_tool(name: &str) -> Value {
    if matches!(name, "present_pdf" | "read_pdf_chunk") {
        let identity = json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "project": {"type":"string", "maxLength":512},
                "path": {"type":"string", "maxLength":512},
                "sha256": {"type":"string", "pattern":"^[0-9a-f]{64}$"},
            }, "required": ["project", "path", "sha256"]
        });
        let mut payload = identity;
        if name == "present_pdf" {
            payload["properties"]["name"] = json!({"type":"string", "maxLength":255});
            payload["properties"]["bytes"] =
                json!({"type":"integer", "minimum":5, "maximum":20971520});
            payload["required"]
                .as_array_mut()
                .unwrap()
                .extend([json!("name"), json!("bytes")]);
            return common::wrapped_output_schema(vec![("pdf_document", payload)]);
        }
        payload["properties"]["bytes_total"] =
            json!({"type":"integer", "minimum":5, "maximum":20971520});
        payload["properties"]["byte_offset"] =
            json!({"type":"integer", "minimum":0, "maximum":20971519});
        payload["properties"]["next_byte_offset"] =
            json!({"type":["integer","null"], "minimum":1, "maximum":20971519});
        payload["properties"]["complete"] = json!({"type":"boolean"});
        payload["properties"]["content_base64"] = json!({"type":"string", "maxLength":174764, "description":"Runtime-only bytes; MCP moves them to private App metadata before structured/text framing."});
        // Transport moves content_base64 to private _meta before framing.
        payload["required"].as_array_mut().unwrap().extend([
            json!("bytes_total"),
            json!("byte_offset"),
            json!("next_byte_offset"),
            json!("complete"),
        ]);
        return common::wrapped_output_schema(vec![("pdf_chunk", payload)]);
    }
    if let Some(schema) = resource_references::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = agent_tasks::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = agent_waits::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = goals::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = coding_agents::output_schema_for_tool(name) {
        return schema;
    }
    #[cfg(feature = "experimental-code-mode")]
    if let Some(schema) = code_mode::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = browser::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = computer::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = communication::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = jobs::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = discovery::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = projects::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = runner_config::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = coding_tasks::output_schema_for_tool(name) {
        return schema;
    }
    #[cfg(feature = "workspace-checkpoints")]
    if let Some(schema) = checkpoints::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = artifacts::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = git::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = edits::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = sessions::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = memory::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = skills::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = hygiene::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = files::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = lsp::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = ssh_resources::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = testing::output_schema_for_tool(name) {
        return schema;
    }

    default_output_schema()
}

pub fn output_schema_for_tool(name: &str) -> Value {
    let mut schema = base_output_schema_for_tool(name);
    if crate::runtime_tool_supports_passive_job_attention(name) {
        common::add_passive_job_attention_to_envelope(&mut schema);
    }
    schema
}

#[cfg(any(test, feature = "root-test-support"))]
pub fn coding_workflow_diagnostic_output_schema_for_test() -> Value {
    coding_tasks::coding_workflow_diagnostic_output_schema()
}
