//! Tool runtime — shared execution layer for MCP and GPT Actions.
//!
//! This module provides the core tool implementations that both MCP and
//! GPT Actions access layers call into.

pub mod git;
pub mod job;
pub mod patch;
pub mod shell;
