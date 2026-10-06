mod annotations;
mod output_schemas;
mod tool_specs;

pub use annotations::tool_annotations;
#[cfg(any(test, feature = "root-test-support"))]
pub use output_schemas::coding_workflow_diagnostic_output_schema_for_test;
pub use output_schemas::{
    continuation_semantics_schema, output_schema_for_tool, suggested_tool_call_schema,
    suggested_tool_call_schema_target,
};
pub use tool_specs::{
    agent_continuation_app_tool_specs, artifact_app_tool_specs,
    exact_manifest_specialist_tool_specs, goal_plan_app_tool_specs,
    job_terminal_continuation_app_tool_specs, memory_management_tool_specs,
    memory_runtime_tool_specs, operator_diagnostic_tool_specs, registered_tool_specs,
    skill_management_tool_specs, skill_runtime_tool_specs, stateless_operator_extension_tool_specs,
    work_result_app_tool_specs,
};

#[cfg(any(test, feature = "root-test-support"))]
pub use tool_specs::take_tool_materialization_counts_for_test;
pub use tool_specs::{
    exact_manifest_specialist_tool_descriptors, registered_tool_descriptors,
    stateless_operator_extension_tool_descriptors, ToolDescriptor,
};
