//! Transport-neutral Tool Runtime wire contracts and audit-safe projections.
//!
//! Declarative tool catalog/schema/policy ownership remains in
//! `webcodex-tool-contracts`; execution, authorization, Runner dispatch, Store,
//! and HTTP/MCP adapters remain in the root `webcodex` crate.

pub mod tool_audit;
pub mod tool_call;
pub mod tool_inputs;
pub mod tool_result;

pub use tool_call::*;
pub use tool_inputs::*;
pub use tool_result::*;
