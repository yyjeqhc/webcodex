use serde_json::json;

use super::helpers::{
    bounded_tail, command_rejected_message, looks_like_command_timeout, resolve_sync_timeout_secs,
    sync_timeout_out_of_range_result, validate_project_relative_path, DEFAULT_CARGO_TIMEOUT_SECS,
};
use super::shell::ProjectCommandOutput;
use super::tool_result::ToolResult;
use super::validation_parser::aggregate_cargo_test_summaries;
use super::validation_profile::{
    validation_adapter_for_tool, ValidationAdapter, ValidationCommandOptions,
};
use super::ToolRuntime;

const CARGO_STDIO_TAIL_CHARS: usize = 12_000;
const CARGO_VALIDATION_FAILURE_KIND: &str = "validation_failed";
const CARGO_FAILURE_GUIDANCE: &str =
    "command was started; inspect stdout_tail/stderr_tail in output, then fix the reported issue or rerun with a narrower cargo filter.";

fn validate_cwd(cwd: Option<String>) -> Result<Option<String>, String> {
    match cwd {
        Some(raw) => {
            let trimmed = raw.trim().trim_start_matches("./").trim_end_matches('/');
            validate_project_relative_path(trimmed)?;
            if trimmed.is_empty() || trimmed == "." {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

fn count_rustc_diagnostics(text: &str, prefix: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with(prefix))
        .count()
}

/// Aggregate passed/failed counts across every Cargo test harness summary line.
///
/// Uses the same multi-harness aggregation as diagnostics `test_summary` so
/// top-level `tests_passed` / `tests_failed` stay consistent when the bounded
/// tails still contain every summary.
pub(crate) fn parse_cargo_test_counts(text: &str) -> (Option<u64>, Option<u64>) {
    match aggregate_cargo_test_summaries(text.lines()) {
        Some(summary) => (summary.passed, summary.failed),
        None => (None, None),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CargoTestRunMetadata {
    pub(crate) tests_detected: bool,
    pub(crate) tests_run_count: Option<u64>,
    pub(crate) zero_tests_run: Option<bool>,
}

pub(crate) fn parse_cargo_test_run_metadata(text: &str) -> CargoTestRunMetadata {
    let mut tests_run_count = 0_u64;
    let mut tests_detected = false;

    for line in text.lines() {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix("running ") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(raw_count) = parts.next() else {
            continue;
        };
        let Some(label) = parts.next() else {
            continue;
        };
        if label != "test" && label != "tests" {
            continue;
        }
        let Ok(count) = raw_count.parse::<u64>() else {
            continue;
        };
        tests_detected = true;
        tests_run_count = tests_run_count.saturating_add(count);
    }

    if tests_detected {
        CargoTestRunMetadata {
            tests_detected,
            tests_run_count: Some(tests_run_count),
            zero_tests_run: Some(tests_run_count == 0),
        }
    } else {
        CargoTestRunMetadata {
            tests_detected,
            tests_run_count: None,
            zero_tests_run: None,
        }
    }
}

fn is_cargo_validation_failure(output: &ProjectCommandOutput, timeout_secs: u64) -> bool {
    output.exit_code.is_some_and(|exit_code| exit_code != 0)
        && !looks_like_command_timeout(output.exit_code, &output.stderr, timeout_secs)
        && !looks_like_command_infrastructure_failure(&output.stderr)
}

fn looks_like_command_infrastructure_failure(stderr: &str) -> bool {
    let trimmed = stderr.trim_start();
    trimmed.starts_with("Failed to execute command:")
        || trimmed.starts_with("Failed to wait for command:")
        || trimmed.starts_with("Failed to collect command output:")
}

impl ToolRuntime {
    pub(crate) async fn cargo_fmt(
        &self,
        project: String,
        cwd: Option<String>,
        check: Option<bool>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let timeout = match resolve_sync_timeout_secs(timeout_secs, DEFAULT_CARGO_TIMEOUT_SECS) {
            Ok(timeout) => timeout,
            Err(_) => {
                return sync_timeout_out_of_range_result("cargo_fmt", DEFAULT_CARGO_TIMEOUT_SECS)
            }
        };
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "choose an existing project-relative cwd, then retry.",
                ))
            }
        };
        let adapter = validation_adapter_for_tool("cargo_fmt")
            .expect("Rust validation profile must register cargo_fmt");
        let command = adapter
            .build_command(ValidationCommandOptions {
                check: check.unwrap_or(false),
                ..ValidationCommandOptions::default()
            })
            .expect("cargo_fmt command builder is infallible");
        self.run_cargo_command(project, cwd, command, timeout, adapter)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_check(
        &self,
        project: String,
        cwd: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let timeout = match resolve_sync_timeout_secs(timeout_secs, DEFAULT_CARGO_TIMEOUT_SECS) {
            Ok(timeout) => timeout,
            Err(_) => {
                return sync_timeout_out_of_range_result("cargo_check", DEFAULT_CARGO_TIMEOUT_SECS)
            }
        };
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "choose an existing project-relative cwd, then retry.",
                ))
            }
        };
        let adapter = validation_adapter_for_tool("cargo_check")
            .expect("Rust validation profile must register cargo_check");
        let command = match adapter.build_command(ValidationCommandOptions {
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
            ..ValidationCommandOptions::default()
        }) {
            Ok(command) => command,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "fix the cargo argument format, then retry.",
                ))
            }
        };
        self.run_cargo_command(project, cwd, command, timeout, adapter)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn cargo_test(
        &self,
        project: String,
        cwd: Option<String>,
        filter: Option<String>,
        all_targets: Option<bool>,
        all_features: Option<bool>,
        no_default_features: Option<bool>,
        features: Option<String>,
        package: Option<String>,
        no_run: Option<bool>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let timeout = match resolve_sync_timeout_secs(timeout_secs, DEFAULT_CARGO_TIMEOUT_SECS) {
            Ok(timeout) => timeout,
            Err(_) => {
                return sync_timeout_out_of_range_result("cargo_test", DEFAULT_CARGO_TIMEOUT_SECS)
            }
        };
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "choose an existing project-relative cwd, then retry.",
                ))
            }
        };
        let adapter = validation_adapter_for_tool("cargo_test")
            .expect("Rust validation profile must register cargo_test");
        let command = match adapter.build_command(ValidationCommandOptions {
            filter,
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
            no_run,
            ..ValidationCommandOptions::default()
        }) {
            Ok(command) => command,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "fix the cargo argument format, then retry.",
                ))
            }
        };
        self.run_cargo_command(project, cwd, command, timeout, adapter)
            .await
    }

    async fn run_cargo_command(
        &self,
        project: String,
        cwd: Option<String>,
        command: String,
        timeout_secs: u64,
        adapter: &'static dyn ValidationAdapter,
    ) -> ToolResult {
        let output = match self
            .run_project_command_capture(&project, command.clone(), timeout_secs, cwd.clone())
            .await
        {
            Ok(output) => output,
            Err(e) => {
                return ToolResult::err(command_rejected_message(
                    e,
                    "verify the project id/cwd and agent connectivity, then retry or use run_shell for custom diagnostics.",
                ))
            }
        };
        let timed_out = looks_like_command_timeout(output.exit_code, &output.stderr, timeout_secs);
        if let Some(error) = output.error.as_ref().filter(|_| !timed_out) {
            return ToolResult::err(command_rejected_message(
                error.clone(),
                "verify the project id/cwd and agent connectivity, then retry or use run_shell for custom diagnostics.",
            ));
        }
        let (stdout_tail, stdout_truncated) = bounded_tail(&output.stdout, CARGO_STDIO_TAIL_CHARS);
        let (stderr_tail, stderr_truncated) = bounded_tail(&output.stderr, CARGO_STDIO_TAIL_CHARS);
        let passed = output.exit_code == Some(0);
        let validation_failed = is_cargo_validation_failure(&output, timeout_secs);
        let combined = format!("{}\n{}", output.stdout, output.stderr);
        let test_diagnostics = if adapter.reports_test_run_metadata() {
            Some(adapter.parse(
                &stdout_tail,
                &stderr_tail,
                stdout_truncated || stderr_truncated,
            ))
        } else {
            None
        };
        let mut payload = json!({
            "project": project,
            "command": command,
            "cwd": cwd.unwrap_or_default(),
            "exit_code": output.exit_code,
            "duration_ms": output.duration_ms,
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "stdout_truncated": stdout_truncated,
            "stderr_truncated": stderr_truncated,
            "passed": passed,
        });
        match adapter.validation_kind() {
            "check" => {
                payload["warnings_count"] = json!(count_rustc_diagnostics(&combined, "warning:"));
                payload["errors_count"] = json!(count_rustc_diagnostics(&combined, "error:"));
            }
            "test" => {
                let (tests_passed, tests_failed) = parse_cargo_test_counts(&combined);
                let run_metadata = parse_cargo_test_run_metadata(&combined);
                payload["tests_passed"] = json!(tests_passed);
                payload["tests_failed"] = json!(tests_failed);
                payload["tests_detected"] = json!(run_metadata.tests_detected);
                payload["tests_run_count"] = json!(run_metadata.tests_run_count);
                payload["zero_tests_run"] = json!(run_metadata.zero_tests_run);
                if let Some(diagnostics) = test_diagnostics {
                    payload["diagnostics"] = json!(diagnostics);
                }
            }
            _ => {}
        }
        if passed {
            ToolResult::ok(payload)
        } else {
            payload["failure_kind"] = json!(if timed_out {
                "timeout"
            } else if validation_failed {
                CARGO_VALIDATION_FAILURE_KIND
            } else {
                "process_exit"
            });
            ToolResult {
                success: false,
                output: payload,
                error: Some(format!("cargo command failed; {}", CARGO_FAILURE_GUIDANCE)),
            }
        }
    }
}
