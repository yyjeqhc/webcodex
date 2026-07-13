//! Permission unit tests: modes, evaluator, execution gate, hard-safety independence.

use super::policy::{self, EffectivePermissionConfig};
use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const WRITE_TOOL: &str = "write_project_file";
const READ_TOOL: &str = "read_file";

#[test]
fn default_mode_auto_approves_permission_bearing_tools() {
    let evaluator = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove);
    let decision = evaluator
        .evaluate(WRITE_TOOL, Some("agent:oe:private-drop"))
        .expect("write tools require permission");
    assert!(decision.required);
    assert_eq!(decision.policy, "dev_auto_approve");
    assert_eq!(decision.status, "auto_approved");
    assert_eq!(decision.reason, "dev_auto_approve");
    assert_eq!(decision.risk, "write");
    assert_eq!(decision.tool_name, WRITE_TOOL);
    assert_eq!(decision.project.as_deref(), Some("agent:oe:private-drop"));
    assert!(decision.request_id.starts_with("wc_perm_"));
    assert_eq!(decision.outcome(), Some(PermissionOutcome::AutoApproved));
}

#[test]
fn unset_permission_mode_matches_dev_auto_approve_behavior() {
    // resolve: unset / empty → default
    assert_eq!(
        resolve_permission_mode(None).unwrap(),
        PermissionMode::DevAutoApprove
    );
    assert_eq!(
        resolve_permission_mode(Some("")).unwrap(),
        PermissionMode::DevAutoApprove
    );

    let from_unset = PermissionEvaluator::with_config(EffectivePermissionConfig::from_raw(None));
    let from_default = PermissionEvaluator::with_mode(PermissionMode::DEFAULT);
    let a = from_unset.evaluate(WRITE_TOOL, None).unwrap();
    let b = from_default.evaluate(WRITE_TOOL, None).unwrap();
    assert_eq!(a.policy, b.policy);
    assert_eq!(a.status, b.status);
    assert_eq!(a.reason, b.reason);
    assert_eq!(a.risk, b.risk);
    assert_eq!(a.required, b.required);
}

#[test]
fn read_only_tools_do_not_emit_permission_decision() {
    let evaluator = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove);
    assert!(evaluator.evaluate(READ_TOOL, None).is_none());
    // Even under require_approval, non-permission tools stay not_required.
    let strict = PermissionEvaluator::with_mode(PermissionMode::RequireApproval);
    assert!(strict.evaluate(READ_TOOL, None).is_none());
}

#[test]
fn illegal_mode_has_explicit_handling_and_does_not_auto_approve() {
    let err = resolve_permission_mode(Some("totally_bogus")).unwrap_err();
    assert_eq!(err.value, "totally_bogus");
    let msg = err.to_string();
    assert!(msg.contains(PERMISSION_MODE_ENV), "{msg}");
    assert!(msg.contains("totally_bogus"), "{msg}");

    let config = EffectivePermissionConfig::from_raw(Some("totally_bogus"));
    assert!(matches!(
        config,
        EffectivePermissionConfig::InvalidMode { .. }
    ));
    assert!(!config.auto_approve());
    assert!(config.human_approval_required());

    let decision = PermissionEvaluator::with_config(config)
        .evaluate(WRITE_TOOL, None)
        .expect("still emits a decision for permission-bearing tools");
    assert_ne!(decision.status, "auto_approved");
    assert_eq!(decision.outcome(), Some(PermissionOutcome::Denied));
    assert!(
        decision.reason.contains("invalid_permission_mode"),
        "{}",
        decision.reason
    );
}

#[test]
fn require_approval_does_not_pretend_to_approve() {
    let evaluator = PermissionEvaluator::with_mode(PermissionMode::RequireApproval);
    let decision = evaluator.evaluate(WRITE_TOOL, None).unwrap();
    assert_eq!(decision.policy, "require_approval");
    assert_ne!(decision.status, "auto_approved");
    assert_ne!(decision.status, "approved");
    assert_eq!(decision.outcome(), Some(PermissionOutcome::Denied));
    assert_eq!(decision.reason, "require_approval_not_implemented");
    assert!(!evaluator.config().auto_approve());
    assert!(evaluator.config().human_approval_required());
}

#[test]
fn audit_only_allows_without_claiming_human_approval() {
    let evaluator = PermissionEvaluator::with_mode(PermissionMode::AuditOnly);
    let decision = evaluator.evaluate(WRITE_TOOL, None).unwrap();
    assert_eq!(decision.policy, "audit_only");
    assert_eq!(decision.status, "audit_only_allowed");
    assert_eq!(
        decision.outcome(),
        Some(PermissionOutcome::AuditOnlyAllowed)
    );
    assert!(evaluator.config().auto_approve());
    assert!(!evaluator.config().human_approval_required());
}

#[test]
fn hard_security_rules_are_not_bypassed_by_permission_mode() {
    // Hard-deny detection is independent of soft permission mode.
    let hard_kinds = [
        "policy_rejected",
        "session_guard_denied",
        "unknown_session_id",
        "session_project_mismatch",
    ];
    for kind in hard_kinds {
        let output = json!({ "error_kind": kind, "failure_kind": kind });
        assert!(
            is_hard_denied_output(&output, None),
            "expected hard deny for {kind}"
        );
    }
    assert!(is_hard_denied_output(
        &json!({}),
        Some("sensitive path blocked")
    ));
    assert!(is_hard_denied_output(
        &json!({}),
        Some("path cannot contain parent traversal")
    ));

    // Auto-approve decision exists for the tool class, but hard-deny filter
    // still drops attachment — mode never overrides hard safety signals.
    let decision = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
        .evaluate(WRITE_TOOL, None)
        .unwrap();
    assert_eq!(decision.status, "auto_approved");
    let hard = json!({
        "error_kind": "policy_rejected",
        "failure_kind": "policy_rejected",
    });
    assert!(is_hard_denied_output(&hard, None));
    let filtered = Some(decision).filter(|_| !is_hard_denied_output(&hard, None));
    assert!(
        filtered.is_none(),
        "hard deny must suppress permission attach even under dev_auto_approve"
    );

    // edit_path helper still produces policy_rejected hard-deny shape.
    let rejected = edit_path_policy_rejected_result("secret.env", "sensitive path".into());
    assert!(!rejected.success);
    assert!(is_hard_denied_output(
        &rejected.output,
        rejected.error.as_deref()
    ));
}

#[test]
fn default_profile_payload_matches_dev_auto_approve() {
    let payload = policy::permission_profile_payload_for(&EffectivePermissionConfig::with_mode(
        PermissionMode::DevAutoApprove,
    ));
    assert_eq!(payload["policy"], "dev_auto_approve");
    assert_eq!(payload["human_approval_required"], false);
    assert_eq!(payload["auto_approve"], true);
    assert_eq!(payload["release_recommended_policy"], "require_approval");
}

#[test]
fn permission_decision_for_tool_wrapper_uses_evaluator() {
    // When env is unset (typical test process), default equals explicit mode.
    let via_wrapper = permission_decision_for_tool(WRITE_TOOL, None);
    let via_evaluator =
        PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove).evaluate(WRITE_TOOL, None);
    let a = via_wrapper.expect("wrapper");
    let b = via_evaluator.expect("evaluator");
    assert_eq!(a.policy, b.policy);
    assert_eq!(a.status, b.status);
    assert_eq!(a.reason, b.reason);
}

#[test]
fn allows_execution_is_centralized_by_outcome() {
    assert!(PermissionOutcome::AutoApproved.allows_execution());
    assert!(PermissionOutcome::AuditOnlyAllowed.allows_execution());
    assert!(PermissionOutcome::Approved.allows_execution());
    assert!(!PermissionOutcome::Denied.allows_execution());
    assert!(!PermissionOutcome::Pending.allows_execution());
    assert!(!PermissionOutcome::HardDenied.allows_execution());

    let auto = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
        .evaluate(WRITE_TOOL, None)
        .unwrap();
    assert!(auto.allows_execution());

    let audit = PermissionEvaluator::with_mode(PermissionMode::AuditOnly)
        .evaluate(WRITE_TOOL, None)
        .unwrap();
    assert!(audit.allows_execution());

    let require = PermissionEvaluator::with_mode(PermissionMode::RequireApproval)
        .evaluate(WRITE_TOOL, None)
        .unwrap();
    assert!(!require.allows_execution());

    let invalid = PermissionEvaluator::with_config(EffectivePermissionConfig::from_raw(Some(
        "not_a_real_mode",
    )))
    .evaluate(WRITE_TOOL, None)
    .unwrap();
    assert!(!invalid.allows_execution());

    // Unparsable status fails closed.
    let mut bogus = auto.clone();
    bogus.status = "totally_unknown_status".to_string();
    assert!(!bogus.allows_execution());
}

#[test]
fn permission_execution_denied_result_is_stable_and_non_approving() {
    let decision = PermissionEvaluator::with_mode(PermissionMode::RequireApproval)
        .evaluate(WRITE_TOOL, None)
        .unwrap();
    let result = permission_execution_denied_result(&decision);
    assert!(!result.success);
    assert_eq!(result.output["error_kind"], "permission_denied");
    assert_eq!(result.output["failure_kind"], "permission_denied");
    assert_eq!(
        result.output["permission_reason"],
        "require_approval_not_implemented"
    );
    let err = result.error.as_deref().unwrap();
    assert!(err.contains("require_approval"), "{err}");
    assert!(err.contains("not implemented"), "{err}");
    // Must not look like hard-deny (permission attach must remain).
    assert!(!is_hard_denied_output(
        &result.output,
        result.error.as_deref()
    ));

    let invalid =
        PermissionEvaluator::with_config(EffectivePermissionConfig::from_raw(Some("weird_mode")))
            .evaluate(WRITE_TOOL, None)
            .unwrap();
    let invalid_result = permission_execution_denied_result(&invalid);
    assert!(!invalid_result.success);
    let msg = invalid_result.error.as_deref().unwrap();
    assert!(msg.contains(PERMISSION_MODE_ENV), "{msg}");
    assert!(
        !msg.contains("auto_approved"),
        "must not pretend auto-approve: {msg}"
    );
}

#[test]
fn evaluate_counter_increments_once_per_call() {
    let counter = Arc::new(AtomicUsize::new(0));
    let evaluator = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
        .with_eval_counter(counter.clone());
    let _ = evaluator.evaluate(WRITE_TOOL, None);
    let _ = evaluator.evaluate(READ_TOOL, None);
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[test]
fn permission_decision_from_output_roundtrips() {
    let decision = PermissionEvaluator::with_mode(PermissionMode::DevAutoApprove)
        .evaluate(WRITE_TOOL, Some("agent:oe:private-drop"))
        .unwrap();
    let mut result = ToolResult::ok(json!({"ok": true}));
    add_permission_to_result(&mut result, &decision);
    let restored = permission_decision_from_output(&result.output).expect("permission present");
    assert_eq!(restored.request_id, decision.request_id);
    assert_eq!(restored.status, decision.status);
    assert_eq!(restored.policy, decision.policy);
}
