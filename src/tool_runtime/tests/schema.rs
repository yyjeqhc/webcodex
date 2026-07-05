//! Schema tests for tool_runtime.

mod annotations;
mod artifacts;
mod consistency;
mod definitions;
mod descriptions;
mod discovery;
mod edits;
mod flattened_args;
mod migration;
mod outputs;
mod policy;
mod sessions;
mod specs;
mod spot_checks;

use super::super::*;
use super::support::*;
use serde_json::Value;
use std::collections::BTreeSet;
