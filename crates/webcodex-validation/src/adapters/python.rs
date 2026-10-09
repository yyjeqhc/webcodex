use super::{
    read_only_validation_plan, ReadOnlyValidationPlan, ValidationAdapter, ValidationCommandOptions,
    ValidationEvidenceProfile, ValidationFailureEvidence, ValidationPlanArg,
};
use webcodex_core::validation_evidence::{parse_pytest_diagnostics, ValidationDiagnostics};

struct RuffEvidenceProfile {
    kind: &'static str,
    identity: &'static str,
}
static RUFF_CHECK: RuffEvidenceProfile = RuffEvidenceProfile {
    kind: "check",
    identity: "python:ruff:check",
};
static RUFF_FORMAT: RuffEvidenceProfile = RuffEvidenceProfile {
    kind: "format",
    identity: "python:ruff:format",
};

pub(super) fn ruff_evidence_profile(
    identity: &str,
) -> Option<&'static dyn ValidationEvidenceProfile> {
    match identity {
        "python:ruff:check" => Some(&RUFF_CHECK),
        "python:ruff:format" => Some(&RUFF_FORMAT),
        _ => None,
    }
}

impl ValidationEvidenceProfile for RuffEvidenceProfile {
    fn validation_kind(&self) -> &'static str {
        self.kind
    }
    fn tool_identity(&self) -> &'static str {
        self.identity
    }
    fn parse(&self, stdout: &str, _stderr: &str, truncated: bool) -> ValidationDiagnostics {
        use sha2::{Digest, Sha256};
        // Do not copy filenames (Ruff commonly emits absolute paths), source,
        // messages, fixes or arbitrary JSON fields into safe diagnostic metadata.
        let truncated = truncated || stdout.len() > 64 * 1024;
        let mut result = ValidationDiagnostics {
            available: false,
            parser: "ruff_validation_parser_v1",
            reason: Some("no complete Ruff diagnostics"),
            diagnostic_count: None,
            diagnostics: Vec::new(),
            returned_diagnostic_count: 0,
            diagnostics_truncated: truncated,
            invalid_diagnostics_omitted: 0,
            test_summary: None,
            failed_test_details: Vec::new(),
            failed_test_details_truncated: false,
            truncated: Some(truncated),
        };
        if truncated {
            return result;
        }
        if self.kind == "format" {
            // Native exit status owns format success. Human output is version
            // dependent and never proves diagnostic or test counts.
            result.reason = Some("Ruff format file counts are unproven; use native exit status");
            return result;
        }
        let mut count = 0usize;
        let mut seen = std::collections::HashSet::new();
        for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
            let code = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|value| {
                    let code = value.get("code")?.as_str()?;
                    let location = value.get("location")?;
                    let row = location.get("row")?.as_u64()?;
                    let column = location.get("column")?.as_u64()?;
                    let filename = value.get("filename")?.as_str()?;
                    (code.len() <= 32
                        && !code.is_empty()
                        && code
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                        && row > 0
                        && column > 0
                        && !filename.is_empty())
                    .then(|| {
                        let mut fingerprint = Sha256::new();
                        fingerprint.update((code.len() as u64).to_be_bytes());
                        fingerprint.update(code.as_bytes());
                        fingerprint.update(row.to_be_bytes());
                        fingerprint.update(column.to_be_bytes());
                        fingerprint.update(filename.as_bytes());
                        (code.to_string(), <[u8; 32]>::from(fingerprint.finalize()))
                    })
                });
            let Some((code, fingerprint)) = code else {
                result.invalid_diagnostics_omitted += 1;
                continue;
            };
            if !seen.insert(fingerprint) {
                continue;
            }
            count += 1;
            if result.diagnostics.len() < webcodex_core::validation_evidence::MAX_DIAGNOSTICS {
                result
                    .diagnostics
                    .push(webcodex_core::validation_evidence::CargoDiagnostic {
                        severity: "error",
                        code: Some(code),
                        file: None,
                        line: None,
                        column: None,
                        message: "Ruff reported a lint violation".into(),
                    });
            }
        }
        result.available = count > 0;
        result.reason = (!result.available).then_some("no complete Ruff diagnostics");
        if result.invalid_diagnostics_omitted == 0 && count > 0 {
            result.diagnostic_count = Some(count);
        }
        result.returned_diagnostic_count = result.diagnostics.len();
        result.diagnostics_truncated = count > result.diagnostics.len();
        result
    }
    fn map_failure_kind(&self, evidence: ValidationFailureEvidence<'_>) -> &'static str {
        if evidence.success {
            "unknown"
        } else if matches!(
            evidence.reported_failure_kind,
            Some("timeout" | "timed_out" | "command_timeout")
        ) {
            "timeout"
        } else if evidence.exit_code == Some(1) {
            if self.kind == "format" {
                "format_diff"
            } else {
                "validation_failed"
            }
        } else {
            "process_exit"
        }
    }
}

struct PytestValidationAdapter;
static PYTEST_ADAPTER: PytestValidationAdapter = PytestValidationAdapter;
pub(super) fn test_adapter() -> &'static dyn ValidationAdapter {
    &PYTEST_ADAPTER
}

impl ValidationEvidenceProfile for PytestValidationAdapter {
    fn validation_kind(&self) -> &'static str {
        "test"
    }

    fn tool_identity(&self) -> &'static str {
        "python:pytest:test"
    }

    fn parse(&self, stdout: &str, _stderr: &str, truncated: bool) -> ValidationDiagnostics {
        parse_pytest_diagnostics(stdout, truncated)
    }

    fn reports_test_run_metadata(&self) -> bool {
        true
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
        if evidence.exit_code == Some(1) {
            "test_failure"
        } else {
            "process_exit"
        }
    }
}

impl ValidationAdapter for PytestValidationAdapter {
    fn build_readonly_plan(
        &self,
        options: ValidationCommandOptions,
    ) -> Result<ReadOnlyValidationPlan, String> {
        if options.check
            || options.lib.is_some()
            || options.all_targets.is_some()
            || options.all_features.is_some()
            || options.no_default_features.is_some()
            || options.features.is_some()
            || options.package.is_some()
            || options.cargo_packages.is_some()
            || options.all_packages
            || options.go_packages.is_some()
            || options.no_run.is_some()
            || options.dependency_mode.is_some()
        {
            return Err("pytest accepts only its bounded test filter".into());
        }
        let mut args = ["-m", "pytest", "--color=no", "-rA"]
            .into_iter()
            .map(ValidationPlanArg::Literal)
            .collect::<Vec<_>>();
        if let Some(filter) = options
            .filter
            .as_deref()
            .map(webcodex_core::runner_protocol::normalize_pytest_filter)
            .transpose()?
            .flatten()
        {
            args.push(ValidationPlanArg::Literal("-k"));
            args.push(ValidationPlanArg::Value(filter));
        }
        read_only_validation_plan("test", "python", args)
    }
}
