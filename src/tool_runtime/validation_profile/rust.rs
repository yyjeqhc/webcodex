use super::{ValidationAdapter, ValidationCommandOptions, ValidationFailureEvidence};
use crate::tool_runtime::helpers::shell_escape_simple;
use crate::tool_runtime::validation_parser::{
    parse_cargo_check_diagnostics, parse_cargo_test_diagnostics, ValidationDiagnostics,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RustAdapterKind {
    Format,
    Check,
    Test,
}

struct RustValidationAdapter {
    kind: RustAdapterKind,
    tool_identity: &'static str,
}

static CARGO_FMT_ADAPTER: RustValidationAdapter = RustValidationAdapter {
    kind: RustAdapterKind::Format,
    tool_identity: "cargo_fmt",
};
static CARGO_CHECK_ADAPTER: RustValidationAdapter = RustValidationAdapter {
    kind: RustAdapterKind::Check,
    tool_identity: "cargo_check",
};
static CARGO_TEST_ADAPTER: RustValidationAdapter = RustValidationAdapter {
    kind: RustAdapterKind::Test,
    tool_identity: "cargo_test",
};
static RUST_ADAPTERS: [&dyn ValidationAdapter; 3] = [
    &CARGO_FMT_ADAPTER,
    &CARGO_CHECK_ADAPTER,
    &CARGO_TEST_ADAPTER,
];

pub(super) fn validation_adapters() -> &'static [&'static dyn ValidationAdapter] {
    &RUST_ADAPTERS
}

impl ValidationAdapter for RustValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        match self.kind {
            RustAdapterKind::Format => "format",
            RustAdapterKind::Check => "check",
            RustAdapterKind::Test => "test",
        }
    }

    fn tool_identity(&self) -> &'static str {
        self.tool_identity
    }

    fn build_command(&self, options: ValidationCommandOptions) -> Result<String, String> {
        if options.go_packages.is_some() {
            return Err("Cargo validation does not accept go_test packages".to_string());
        }
        match self.kind {
            RustAdapterKind::Format => Ok(cargo_fmt_command(options.check)),
            RustAdapterKind::Check => cargo_check_command(options),
            RustAdapterKind::Test => cargo_test_command(options),
        }
    }

    fn parse(
        &self,
        stdout_excerpt: &str,
        stderr_excerpt: &str,
        truncated: bool,
    ) -> ValidationDiagnostics {
        match self.kind {
            RustAdapterKind::Format | RustAdapterKind::Check => {
                parse_cargo_check_diagnostics(stdout_excerpt, stderr_excerpt, truncated)
            }
            RustAdapterKind::Test => {
                parse_cargo_test_diagnostics(stdout_excerpt, stderr_excerpt, truncated)
            }
        }
    }

    fn map_failure_kind(&self, evidence: ValidationFailureEvidence<'_>) -> &'static str {
        if evidence.success {
            return "unknown";
        }
        if matches!(
            evidence.reported_failure_kind,
            Some("timeout" | "timed_out" | "command_timeout")
        ) {
            return "timeout";
        }
        if evidence.reported_failure_kind == Some("validation_failed")
            && evidence.exit_code == Some(0)
        {
            return "validation_failed";
        }

        let has_compile_error = evidence.diagnostics.is_some_and(|diagnostics| {
            diagnostics
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.severity == "error")
        });
        if matches!(self.kind, RustAdapterKind::Check | RustAdapterKind::Test) && has_compile_error
        {
            return "compile_error";
        }

        if self.kind == RustAdapterKind::Test
            && evidence.diagnostics.is_some_and(|diagnostics| {
                diagnostics
                    .test_summary
                    .as_ref()
                    .and_then(|summary| summary.failed)
                    .is_some_and(|failed| failed > 0)
                    || !diagnostics.failed_test_details.is_empty()
            })
        {
            return "test_failure";
        }

        if self.kind == RustAdapterKind::Format
            && has_stable_cargo_fmt_diff(evidence.stdout_excerpt, evidence.stderr_excerpt)
        {
            return "format_diff";
        }

        if evidence.exit_code.is_some_and(|exit_code| exit_code != 0)
            || matches!(
                evidence.reported_failure_kind,
                Some(
                    "command_exit_nonzero"
                        | "command_spawn_failed"
                        | "command_wait_failed"
                        | "command_output_failed"
                )
            )
        {
            return "process_exit";
        }
        "unknown"
    }

    fn reports_test_run_metadata(&self) -> bool {
        self.kind == RustAdapterKind::Test
    }
}

fn cargo_fmt_command(check: bool) -> String {
    if check {
        "cargo fmt -- --check".to_string()
    } else {
        "cargo fmt".to_string()
    }
}

fn cargo_check_command(options: ValidationCommandOptions) -> Result<String, String> {
    let features = validate_arg("features", options.features)?;
    let package = validate_arg("package", options.package)?;
    let mut args = vec!["cargo".to_string(), "check".to_string()];
    if options.all_targets.unwrap_or(true) {
        args.push("--all-targets".to_string());
    }
    if options.all_features.unwrap_or(false) {
        args.push("--all-features".to_string());
    }
    if options.no_default_features.unwrap_or(false) {
        args.push("--no-default-features".to_string());
    }
    if let Some(features) = features {
        args.push("--features".to_string());
        args.push(shell_escape_simple(&features));
    }
    if let Some(package) = package {
        args.push("-p".to_string());
        args.push(shell_escape_simple(&package));
    }
    Ok(args.join(" "))
}

fn cargo_test_command(options: ValidationCommandOptions) -> Result<String, String> {
    let filter = validate_filter(options.filter)?;
    let features = validate_arg("features", options.features)?;
    let package = validate_arg("package", options.package)?;
    let mut args = vec!["cargo".to_string(), "test".to_string()];
    if let Some(filter) = filter {
        args.push(shell_escape_simple(&filter));
    }
    if options.all_targets.unwrap_or(false) {
        args.push("--all-targets".to_string());
    }
    if options.all_features.unwrap_or(false) {
        args.push("--all-features".to_string());
    }
    if options.no_default_features.unwrap_or(false) {
        args.push("--no-default-features".to_string());
    }
    if let Some(features) = features {
        args.push("--features".to_string());
        args.push(shell_escape_simple(&features));
    }
    if let Some(package) = package {
        args.push("-p".to_string());
        args.push(shell_escape_simple(&package));
    }
    if options.no_run.unwrap_or(false) {
        args.push("--no-run".to_string());
    }
    Ok(args.join(" "))
}

/// Normalize one value-taking Cargo option using the same shared contract the
/// structured Job argv builder uses, so a request produces identical
/// arguments whether it runs synchronously or as a long Job. The error label
/// is mapped to the tool-facing message.
fn validate_arg(label: &str, value: Option<String>) -> Result<Option<String>, String> {
    match value {
        Some(raw) => match crate::runner_protocol::normalize_cargo_value(&raw) {
            Ok(normalized) => Ok(normalized),
            Err(reason) => Err(format!("{} {reason}", label)),
        },
        None => Ok(None),
    }
}

/// Normalize the libtest filter using the shared filter contract (which also
/// rejects option-like values) so the synchronous command matches the Job
/// argv builder.
fn validate_filter(value: Option<String>) -> Result<Option<String>, String> {
    match value {
        Some(raw) => match crate::runner_protocol::normalize_rust_test_filter(&raw) {
            Ok(normalized) => Ok(normalized),
            Err(reason) => Err(format!("filter {reason}")),
        },
        None => Ok(None),
    }
}

fn has_stable_cargo_fmt_diff(stdout_excerpt: &str, stderr_excerpt: &str) -> bool {
    [stdout_excerpt, stderr_excerpt]
        .into_iter()
        .flat_map(str::lines)
        .map(str::trim_start)
        .filter_map(|line| line.strip_prefix("Diff in "))
        .any(|location| {
            let location = location.trim_end_matches(':');
            location
                .rsplit_once(':')
                .is_some_and(|(_, line)| line.parse::<u64>().is_ok_and(|line| line > 0))
        })
}
