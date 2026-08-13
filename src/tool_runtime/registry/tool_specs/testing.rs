use super::super::input_schemas::{
    cargo_check_input_schema, cargo_fmt_input_schema, cargo_test_input_schema, go_test_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "cargo_fmt",
            "Run cargo fmt in an agent-registered project. Use check=true for cargo fmt -- --check before broader validation. check=true is a read-only validation: a short run returns immediately and a longer run continues as a Job and returns job_id.",
            cargo_fmt_input_schema(),
        ),
        tool_spec(
            "cargo_check",
            "Preferred structured Rust validation for cargo check. Defaults to --all-targets and supports features/package/cwd/timeout without shell interpolation. A short run returns immediately; a longer run continues as a Job and returns job_id. Use before raw run_shell when applicable.",
            cargo_check_input_schema(),
        ),
        tool_spec(
            "cargo_test",
            "Preferred structured Rust test runner. Supports filter, feature flags, package, --no-run, timeout, and bounded output tails. A short run returns immediately; a longer run continues as a Job and returns job_id. Use before raw run_shell when applicable.",
            cargo_test_input_schema(),
        ),
        tool_spec(
            "go_test",
            "Preferred structured Go test runner. Runs exactly go test -json ./... with an optional project-relative cwd. Requires Runner structured Go JSON validation support; a short run may return immediately and a longer run continues as the same Job.",
            go_test_input_schema(),
        ),
    ]
}
