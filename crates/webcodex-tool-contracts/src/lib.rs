#![recursion_limit = "512"]

//! Declarative WebCodex tool contracts: catalog, schemas, metadata, and policy queries.
//!
//! Execution, authorization enforcement, connector orchestration, and side effects remain in
//! the root application crate.

pub mod input_normalization;
pub mod metadata;
mod read_revision;
pub mod registry;
pub mod request_schema;
mod schema_generation;
pub mod tool_call;
pub mod tool_catalog;
pub mod tool_definition;
pub mod tool_inputs;
pub mod tool_policy;
pub mod tool_spec;

#[cfg(any(test, feature = "root-test-support"))]
pub mod test_support;

#[cfg(test)]
mod tests;

pub use input_normalization::ToolInputNormalizationCode;
pub use metadata::*;
pub use registry::*;
pub use request_schema::*;
pub use tool_call::*;
pub use tool_catalog::*;
pub use tool_definition::*;
pub use tool_inputs::*;
pub use tool_policy::*;
pub use tool_spec::{ToolSpec, GPT_ACTION_DESCRIPTION_MAX_CHARS, MODEL_TOOL_DESCRIPTION_MAX_CHARS};
