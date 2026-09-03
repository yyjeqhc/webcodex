//! Transport-neutral durable Connector runtime contracts.
//!
//! Authentication, credentials, HTTP, and ToolRuntime stay in the root
//! package. This crate receives only stable principal/access projections and a
//! deliberately narrow call-scoped host port.

mod context;
#[cfg(test)]
mod continuation_delivery_tests;
mod contracts;
mod execution;
#[cfg(test)]
mod host_tests;
mod projections;
mod runtime;
#[cfg(test)]
mod runtime_tests;
pub mod surface;
mod wire_models;
pub mod workspace;

pub use context::{
    required_env, ConnectorContext, CONNECTOR_SURFACE_ENV, CONNECTOR_SURFACE_TASK_V1,
    PROJECT_AGENT_TOKEN_FILE_ENV, PROJECT_CREDENTIAL_FILE_ENV,
};
pub use contracts::*;
pub use projections::{
    approval_projection, durable_task_review_projection, result_projection, store_error_outcome,
    validate_opaque_id,
};
pub use runtime::ConnectorRuntime;
pub use wire_models::{TaskCancelInput, TaskReviewInput};
pub use workspace::LocalResultDecision;
