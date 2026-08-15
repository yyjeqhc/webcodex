//! Schema tests for tool_runtime.

macro_rules! assert_schema_fields {
    (
        $properties:expr,
        $context:expr,
        present: [$($present:expr),* $(,)?]
        $(, absent: [$($absent:expr),* $(,)?])?
        $(,)?
    ) => {{
        let properties = $properties;
        let context = $context;
        $(
            assert!(
                properties.contains_key($present),
                "{context}: missing schema field {}",
                $present
            );
        )*
        $(
            $(
                assert!(
                    !properties.contains_key($absent),
                    "{context}: unexpected schema field {}",
                    $absent
                );
            )*
        )?
    }};
}

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
