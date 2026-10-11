//! Evidence-only Node native project-script check. Project scripts are untrusted
//! executable code, not structured lint output or guaranteed read-only operations.
use super::{ValidationEvidenceProfile, ValidationFailureEvidence};
use webcodex_core::validation_evidence::{ValidationDiagnostics, PARSER_KIND};

struct NodeScriptCheck;
static NODE_SCRIPT_CHECK: NodeScriptCheck = NodeScriptCheck;

pub(super) fn evidence_profile(identity: &str) -> Option<&'static dyn ValidationEvidenceProfile> {
    match identity {
        "node:script:check" => Some(&NODE_SCRIPT_CHECK),
        "node:tap:test" => Some(&NODE_NATIVE_TAP_TEST),
        _ => None,
    }
}

impl ValidationEvidenceProfile for NodeScriptCheck {
    fn tool_identity(&self) -> &'static str {
        "node:script:check"
    }

    fn validation_kind(&self) -> &'static str {
        "check"
    }

    fn parse(&self, _stdout: &str, _stderr: &str, truncated: bool) -> ValidationDiagnostics {
        // Arbitrary project script text must not be parsed as another tool's
        // diagnostics or test-count evidence.
        ValidationDiagnostics {
            available: false,
            parser: PARSER_KIND,
            reason: Some("project script has no trusted structured diagnostics"),
            diagnostic_count: None,
            diagnostics: Vec::new(),
            returned_diagnostic_count: 0,
            diagnostics_truncated: truncated,
            invalid_diagnostics_omitted: 0,
            test_summary: None,
            failed_test_details: Vec::new(),
            failed_test_details_truncated: false,
            truncated: Some(truncated),
        }
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
            "validation_failed"
        } else {
            "process_exit"
        }
    }
}

struct NodeNativeTapTest;
static NODE_NATIVE_TAP_TEST: NodeNativeTapTest = NodeNativeTapTest;

impl ValidationEvidenceProfile for NodeNativeTapTest {
    fn tool_identity(&self) -> &'static str {
        "node:tap:test"
    }
    fn validation_kind(&self) -> &'static str {
        "test"
    }

    fn parse(&self, stdout: &str, _stderr: &str, truncated: bool) -> ValidationDiagnostics {
        webcodex_core::validation_evidence::parse_node_native_test_diagnostics(stdout, truncated)
    }

    fn reports_test_run_metadata(&self) -> bool {
        true
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
            "test_failure"
        } else {
            "process_exit"
        }
    }
}
