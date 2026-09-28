//! Late, conservative deduplication. Never changes canonical validation truth.
use super::*;
use serde_json::json;

#[derive(Default, Clone, Copy)]
pub(super) struct ValidationSuccessPolicy {
    pub require_tests: Option<bool>,
    pub no_run: Option<bool>,
    pub min_tests: Option<u64>,
}

pub(super) fn sparsify_structured_validation_success_evidence(
    tool: &str,
    policy: ValidationSuccessPolicy,
    result: &mut ToolResult,
) {
    if !matches!(
        tool,
        "cargo_check" | "cargo_test" | "go_test" | "project_validate"
    ) || !result.success
        || result.error.is_some()
    {
        return;
    }
    let o = &result.output;
    if o["execution_state"] != "completed"
        || o["exit_code"] != 0
        || ["passed", "command_started", "command_completed", "terminal"]
            .iter()
            .any(|k| o[*k] != true)
        || o["promoted_to_job"] != false
        || [
            "job_id",
            "job_status",
            "observation_token",
            "continuation",
            "failure_kind",
            "recovery",
            "recovery_state",
            "recovery_reason_code",
            "reconciled_at",
            "observation_error",
        ]
        .iter()
        .any(|k| o.get(*k).is_some_and(|v| !v.is_null()))
        || o["source_state"]["freshness"] != "unproven"
        || o["source_state"]["observed_mutation_fence"] != "uncrossed"
        || ["stdout_truncated", "stderr_truncated"]
            .iter()
            .any(|k| o[*k] != false)
    {
        return;
    }
    let adapter = if tool == "project_validate" {
        let Some(adapter) = o["adapter"].as_str() else {
            return;
        };
        let (backend, action) = match adapter {
            "cargo_check" => ("rust", "check"),
            "cargo_test" => ("rust", "test"),
            "go_vet" => ("go", "check"),
            "go_test" => ("go", "test"),
            _ => return,
        };
        if o["backend"] != backend
            || o["action"] != action
            || !o["validation_target_id"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        {
            return;
        }
        adapter
    } else {
        tool
    };
    if tool == "cargo_test"
        && (policy.no_run.is_some_and(|value| o["no_run"] != value)
            || policy
                .require_tests
                .is_some_and(|value| o["require_tests"] != value))
    {
        return;
    }
    let test = matches!(adapter, "cargo_test" | "go_test");
    let compile_only = adapter == "cargo_test" && o["no_run"] == true;
    let d = &o["diagnostics"];
    let Some(items) = d["diagnostics"].as_array() else {
        return;
    };
    if d["parser"] != "structured_validation_parser"
        || d["diagnostics_truncated"] != false
        || d["failed_test_details_truncated"] != false
        || d["invalid_diagnostics_omitted"] != 0
        || d.get("truncated").is_some_and(|v| v != false)
        || d["returned_diagnostic_count"].as_u64() != Some(items.len() as u64)
        || !d["failed_test_details"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || items.iter().any(|item| item["severity"] != "warning")
    {
        return;
    }
    // Unavailable with this exact reason means no stable diagnostics, not an
    // unavailable execution/count parser. All loss flags were checked above.
    if d["available"] == false {
        if d["reason"] != "no stable diagnostics found"
            || !items.is_empty()
            || d.get("test_summary").is_some()
            || d.get("diagnostic_count").is_some_and(|v| !v.is_null())
        {
            return;
        }
    } else if d["available"] != true || d.get("reason").is_some_and(|v| !v.is_null()) {
        return;
    }
    let mut retained_minimum = None;
    let mut accepted_zero = false;
    if test && !compile_only {
        let Some(count) = o["tests_run_count"].as_u64() else {
            return;
        };
        if o["tests_detected"] != true
            || o["tests_passed"].as_u64() != Some(count)
            || o["tests_failed"] != 0
            || o["zero_tests_run"] != (count == 0)
            || d["available"] != true
            || d["test_summary"]["passed"].as_u64() != Some(count)
            || d["test_summary"]["failed"] != 0
            || d["test_summary"]["ignored"] != 0
            || d["diagnostic_count"] != 0
        {
            return;
        }
        if let Some(a) = o.get("test_count_assertion") {
            let Some(minimum) = a["minimum_tests"].as_u64().filter(|n| *n > 0) else {
                return;
            };
            if a["status"] != "passed"
                || a["reason_code"] != "minimum_satisfied"
                || a["actual_tests_run"].as_u64() != Some(count)
                || count < minimum
                || (adapter == "cargo_test" && a["evidence_reason_code"] != "complete_summary")
                || (adapter == "go_test"
                    && !matches!(
                        a["evidence_reason_code"].as_str(),
                        Some("complete_summary" | "no_complete_summary")
                    ))
                || policy.min_tests.is_some_and(|n| n != minimum)
            {
                return;
            }
            // project_validate's minimum=1 is intrinsic; explicit Cargo
            // minimums (including 1) remain visible as caller policy.
            if tool != "project_validate" && (policy.min_tests.is_some() || minimum > 1) {
                retained_minimum = Some(minimum);
            }
        } else if policy.min_tests.is_some()
            || policy.require_tests == Some(true)
            || tool == "project_validate"
        {
            return;
        }
        accepted_zero = count == 0
            && adapter == "cargo_test"
            && o["require_tests"] == false
            && o.get("test_count_assertion").is_none();
        if count == 0 && !accepted_zero {
            return;
        }
    } else if compile_only {
        if policy.min_tests.is_some()
            || policy.require_tests == Some(true)
            || o.get("test_count_assertion").is_some()
            || o["tests_detected"] != false
            || [
                "tests_run_count",
                "tests_passed",
                "tests_failed",
                "zero_tests_run",
            ]
            .iter()
            .any(|k| !o[*k].is_null())
            || d.get("test_summary").is_some()
        {
            return;
        }
    } else if o["errors_count"] != 0
        || o["warnings_count"].as_u64().is_none()
        || (d["available"] == true && d["diagnostic_count"].as_u64() != Some(items.len() as u64))
        || (o["warnings_count"] == 0 && !items.is_empty())
    {
        return;
    }

    let diagnostics = (!items.is_empty()).then(|| json!({"diagnostics": items}));
    let o = result.output.as_object_mut().expect("validated object");
    for key in [
        "tests_detected",
        "tests_passed",
        "tests_failed",
        "zero_tests_run",
        "require_tests",
        "no_run",
        "test_count_assertion",
        "diagnostics",
    ] {
        o.remove(key);
    }
    if compile_only {
        o.remove("tests_run_count");
        o.insert("no_run".into(), json!(true));
    }
    if accepted_zero {
        o.insert("require_tests".into(), json!(false));
    }
    if let Some(minimum) = retained_minimum {
        o.insert(
            "test_count_assertion".into(),
            json!({"minimum_tests": minimum}),
        );
    }
    if let Some(diagnostics) = diagnostics {
        o.insert("diagnostics".into(), diagnostics);
    }
    if !test {
        o.remove("errors_count");
        if o.get("warnings_count") == Some(&json!(0)) {
            o.remove("warnings_count");
        }
    }
    if tool == "project_validate" {
        o.remove("action");
        o.remove("backend");
    }
}

#[cfg(test)]
#[path = "tests/validation_success_projection.rs"]
mod tests;
