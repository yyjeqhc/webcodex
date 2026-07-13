//! Permission Phase 1 unit tests: modes, evaluator, hard-safety independence.

use super::policy::{self, EffectivePermissionConfig};
use super::*;
use serde_json::json;

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
