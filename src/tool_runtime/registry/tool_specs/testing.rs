use super::super::input_schemas::{
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "cargo_fmt",
            "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation.",
            cargo_fmt_input_schema(),
        ),
        tool_spec(
            "cargo_check",
            "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation; use before raw run_shell when applicable.",
            cargo_check_input_schema(),
        ),
        tool_spec(
            "cargo_test",
            "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails; use before raw run_shell when applicable.",
            cargo_test_input_schema(),
        ),
    ]
}
