use serde_json::json;

use super::helpers::{bounded_tail, shell_escape_simple, validate_project_relative_path};
use super::types::ToolResult;
use super::ToolRuntime;

const CARGO_STDIO_TAIL_CHARS: usize = 12_000;

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

fn validate_arg(label: &str, value: Option<String>) -> Result<Option<String>, String> {
    match value {
        Some(raw) => {
            if raw.contains('\0') {
                return Err(format!("{} cannot contain NUL bytes", label));
            }
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        None => Ok(None),
    }
}

pub(crate) fn cargo_fmt_command(check: bool) -> String {
    if check {
        "cargo fmt -- --check".to_string()
    } else {
        "cargo fmt".to_string()
    }
}

pub(crate) fn cargo_check_command(
    all_targets: Option<bool>,
    all_features: Option<bool>,
    no_default_features: Option<bool>,
    features: Option<String>,
    package: Option<String>,
) -> Result<String, String> {
    let features = validate_arg("features", features)?;
    let package = validate_arg("package", package)?;
    let mut args = vec!["cargo".to_string(), "check".to_string()];
    if all_targets.unwrap_or(true) {
        args.push("--all-targets".to_string());
    }
    if all_features.unwrap_or(false) {
        args.push("--all-features".to_string());
    }
    if no_default_features.unwrap_or(false) {
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

pub(crate) fn cargo_test_command(
    filter: Option<String>,
    all_targets: Option<bool>,
    all_features: Option<bool>,
    no_default_features: Option<bool>,
    features: Option<String>,
    package: Option<String>,
    no_run: Option<bool>,
) -> Result<String, String> {
    let filter = validate_arg("filter", filter)?;
    let features = validate_arg("features", features)?;
    let package = validate_arg("package", package)?;
    let mut args = vec!["cargo".to_string(), "test".to_string()];
    if let Some(filter) = filter {
        args.push(shell_escape_simple(&filter));
    }
    if all_targets.unwrap_or(false) {
        args.push("--all-targets".to_string());
    }
    if all_features.unwrap_or(false) {
        args.push("--all-features".to_string());
    }
    if no_default_features.unwrap_or(false) {
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
    if no_run.unwrap_or(false) {
        args.push("--no-run".to_string());
    }
    Ok(args.join(" "))
}

fn count_rustc_diagnostics(text: &str, prefix: &str) -> usize {
    text.lines()
        .filter(|line| line.trim_start().starts_with(prefix))
        .count()
}

pub(crate) fn parse_cargo_test_counts(text: &str) -> (Option<u64>, Option<u64>) {
    let mut passed = None;
    let mut failed = None;
    for line in text.lines().filter(|line| line.contains("test result:")) {
        let tokens: Vec<&str> = line
            .split(|c: char| c.is_whitespace() || c == ';')
            .filter(|token| !token.is_empty())
            .collect();
        for window in tokens.windows(2) {
            if window[1] == "passed" {
                passed = window[0].parse::<u64>().ok();
            } else if window[1] == "failed" {
                failed = window[0].parse::<u64>().ok();
            }
        }
    }
    (passed, failed)
}

impl ToolRuntime {
    pub(crate) async fn cargo_fmt(
        &self,
        project: String,
        cwd: Option<String>,
        check: Option<bool>,
        timeout_secs: Option<u64>,
    ) -> ToolResult {
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => return ToolResult::err(e),
        };
        let command = cargo_fmt_command(check.unwrap_or(false));
        self.run_cargo_command(project, cwd, command, timeout_secs.unwrap_or(120), None)
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
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => return ToolResult::err(e),
        };
        let command = match cargo_check_command(
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
        ) {
            Ok(command) => command,
            Err(e) => return ToolResult::err(e),
        };
        self.run_cargo_command(
            project,
            cwd,
            command,
            timeout_secs.unwrap_or(300),
            Some("check"),
        )
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
        let cwd = match validate_cwd(cwd) {
            Ok(cwd) => cwd,
            Err(e) => return ToolResult::err(e),
        };
        let command = match cargo_test_command(
            filter,
            all_targets,
            all_features,
            no_default_features,
            features,
            package,
            no_run,
        ) {
            Ok(command) => command,
            Err(e) => return ToolResult::err(e),
        };
        self.run_cargo_command(
            project,
            cwd,
            command,
            timeout_secs.unwrap_or(600),
            Some("test"),
        )
        .await
    }

    async fn run_cargo_command(
        &self,
        project: String,
        cwd: Option<String>,
        command: String,
        timeout_secs: u64,
        mode: Option<&str>,
    ) -> ToolResult {
        let output = match self
            .run_project_command_capture(&project, command.clone(), timeout_secs, cwd.clone())
            .await
        {
            Ok(output) => output,
            Err(e) => return ToolResult::err(e),
        };
        let (stdout_tail, stdout_truncated) = bounded_tail(&output.stdout, CARGO_STDIO_TAIL_CHARS);
        let (stderr_tail, stderr_truncated) = bounded_tail(&output.stderr, CARGO_STDIO_TAIL_CHARS);
        let passed = output.exit_code == Some(0);
        let combined = format!("{}\n{}", output.stdout, output.stderr);
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
        match mode {
            Some("check") => {
                payload["warnings_count"] = json!(count_rustc_diagnostics(&combined, "warning:"));
                payload["errors_count"] = json!(count_rustc_diagnostics(&combined, "error:"));
            }
            Some("test") => {
                let (tests_passed, tests_failed) = parse_cargo_test_counts(&combined);
                payload["tests_passed"] = json!(tests_passed);
                payload["tests_failed"] = json!(tests_failed);
            }
            _ => {}
        }
        if passed {
            ToolResult::ok(payload)
        } else {
            ToolResult {
                success: false,
                output: payload,
                error: Some("cargo command failed".to_string()),
            }
        }
    }
}
