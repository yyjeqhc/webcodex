//! Transport-neutral durable Connector runtime contracts.
//!
//! Authentication, credentials, HTTP, and ToolRuntime stay in the root
//! package. This crate receives only stable principal/access projections and a
//! deliberately narrow call-scoped host port.

mod contracts;

pub use contracts::*;
