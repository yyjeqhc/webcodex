use serde_json::Value;

mod artifacts;
mod checkpoints;
mod coding_tasks;
mod common;
mod discovery;
mod edits;
mod files;
mod git;
mod hygiene;
mod jobs;
mod lsp;
mod projects;
mod sessions;
mod testing;

use common::default_output_schema;

pub(crate) fn output_schema_for_tool(name: &str) -> Value {
    if let Some(schema) = jobs::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = discovery::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = projects::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = coding_tasks::output_schema_for_tool(name) {
        return schema;
    }
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
    if let Some(schema) = hygiene::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = files::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = lsp::output_schema_for_tool(name) {
        return schema;
    }
    if let Some(schema) = testing::output_schema_for_tool(name) {
        return schema;
    }

    default_output_schema()
}
