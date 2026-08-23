use super::super::input_schemas::{
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema, go_test_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "cargo_fmt",
            "Run cargo fmt. With check=true it is read-only validation; a long check continues as the same execution and returns job_id for observation. Mutating format stays synchronous.",
            cargo_fmt_input_schema(),
        ),
        tool_spec(
            "cargo_check",
            "Preferred structured cargo check (default --all-targets). Supports scoped flags without shell interpolation; a long validation continues as the same execution and returns job_id.",
            cargo_check_input_schema(),
        ),
        tool_spec(
            "cargo_test",
            "Preferred structured cargo test with scoped args and bounded output. require_tests/min_tests add a proven minimum executed-test postcondition; long validation continues as the same execution Job.",
            cargo_test_input_schema(),
        ),
        tool_spec(
            "go_test",
            "Preferred structured go test -json (default ./...) with bounded package scopes. Requires Runner Go JSON validation support; long validation continues as the same execution and returns job_id.",
            go_test_input_schema(),
        ),
    ]
}
