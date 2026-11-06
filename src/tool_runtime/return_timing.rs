use super::{structured_execution::STRUCTURED_EXECUTION_SYNC_WAIT_SECS, ToolCall};

/// Trusted Server-owned constraints on how long the current call may wait
/// synchronously for an already-started durable execution before returning the
/// same Job. This is return latency policy only: it never changes execution
/// lifetime, Job identity, or observation waits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ToolReturnTimingPolicy {
    max_handoff_secs: Option<u64>,
    default_handoff_secs: Option<u64>,
}

impl ToolReturnTimingPolicy {
    pub(crate) const fn unconstrained() -> Self {
        Self {
            max_handoff_secs: None,
            default_handoff_secs: None,
        }
    }

    pub(crate) const fn handoff_max_secs(max_handoff_secs: u64) -> Self {
        Self {
            max_handoff_secs: Some(max_handoff_secs),
            default_handoff_secs: None,
        }
    }

    /// Adapter preference for an omitted wait. Intersections still honor every
    /// trusted upper bound; execution lifetime and caller-supplied waits are separate.
    pub(crate) const fn handoff_default_secs(default_handoff_secs: u64) -> Self {
        Self {
            max_handoff_secs: None,
            default_handoff_secs: Some(default_handoff_secs),
        }
    }

    pub(crate) const fn max_handoff_secs(self) -> Option<u64> {
        self.max_handoff_secs
    }

    /// Upper-bound policies compose by taking the strictest applicable bound.
    pub(crate) fn intersect(self, other: Self) -> Self {
        let max_handoff_secs = match (self.max_handoff_secs, other.max_handoff_secs) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
        let default_handoff_secs = match (self.default_handoff_secs, other.default_handoff_secs) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };
        Self {
            max_handoff_secs,
            default_handoff_secs,
        }
    }
}

/// Apply trusted return-latency constraints to structured executions. A legacy
/// caller-supplied sync_wait_secs remains a compatibility preference, but it
/// can only make handoff earlier; it can never exceed a Server-owned cap.
///
/// An adapter can supply a different default without changing execution lifetime.
pub(super) fn normalize_structured_handoff(call: &mut ToolCall, policy: ToolReturnTimingPolicy) {
    if policy.max_handoff_secs.is_none() && policy.default_handoff_secs.is_none() {
        return;
    }
    let default = policy
        .default_handoff_secs
        .unwrap_or(STRUCTURED_EXECUTION_SYNC_WAIT_SECS);
    let max_handoff_secs = policy.max_handoff_secs.unwrap_or(u64::MAX);

    match call {
        ToolCall::RunProcess { sync_wait_secs, .. }
        | ToolCall::RunScript { sync_wait_secs, .. }
        | ToolCall::RunShell { sync_wait_secs, .. }
        | ToolCall::RunSkillResource { sync_wait_secs, .. }
        | ToolCall::CargoCheck { sync_wait_secs, .. }
        | ToolCall::CargoTest { sync_wait_secs, .. }
        | ToolCall::GoTest { sync_wait_secs, .. } => {
            constrain_sync_wait(sync_wait_secs, default, max_handoff_secs);
        }
        ToolCall::CargoFmt {
            check,
            sync_wait_secs,
            ..
        } if *check == Some(true) => {
            constrain_sync_wait(sync_wait_secs, default, max_handoff_secs);
        }
        _ => {}
    }
}

fn constrain_sync_wait(sync_wait_secs: &mut Option<u64>, default: u64, max_handoff_secs: u64) {
    let requested = sync_wait_secs.unwrap_or(default);
    *sync_wait_secs = Some(requested.min(max_handoff_secs));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_default_and_explicit_early_return_obey_every_cap() {
        for (requested, cap, expected) in [
            (None, 50, 50),
            (None, 4, 4),
            (Some(1), 50, 1),
            (Some(60), 4, 4),
        ] {
            let policy = ToolReturnTimingPolicy::handoff_default_secs(50)
                .intersect(ToolReturnTimingPolicy::handoff_max_secs(cap));
            let mut args = serde_json::json!({"project":"demo", "command":"true"});
            if let Some(wait) = requested {
                args["sync_wait_secs"] = serde_json::json!(wait);
            }
            let mut call = ToolCall::from_tool_name("run_shell", args).unwrap();
            normalize_structured_handoff(&mut call, policy);
            assert!(
                matches!(call, ToolCall::RunShell { sync_wait_secs: Some(actual), .. } if actual == expected)
            );
        }
    }

    #[test]
    fn upper_bound_policies_intersect_to_the_strictest_limit() {
        assert_eq!(
            ToolReturnTimingPolicy::handoff_max_secs(5)
                .intersect(ToolReturnTimingPolicy::handoff_max_secs(55))
                .max_handoff_secs(),
            Some(5)
        );
        assert_eq!(
            ToolReturnTimingPolicy::unconstrained()
                .intersect(ToolReturnTimingPolicy::handoff_max_secs(4))
                .max_handoff_secs(),
            Some(4)
        );
        assert_eq!(
            ToolReturnTimingPolicy::unconstrained()
                .intersect(ToolReturnTimingPolicy::unconstrained())
                .max_handoff_secs(),
            None
        );
    }

    #[test]
    fn trusted_cap_applies_when_legacy_field_is_omitted_or_larger() {
        let policy = ToolReturnTimingPolicy::handoff_max_secs(5);
        let mut omitted =
            ToolCall::from_tool_name("cargo_check", serde_json::json!({"project":"demo"})).unwrap();
        normalize_structured_handoff(&mut omitted, policy);
        match omitted {
            ToolCall::CargoCheck { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut larger = ToolCall::from_tool_name(
            "cargo_test",
            serde_json::json!({"project":"demo","sync_wait_secs":55}),
        )
        .unwrap();
        normalize_structured_handoff(&mut larger, policy);
        match larger {
            ToolCall::CargoTest { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(5)),
            _ => unreachable!(),
        }

        let mut shorter = ToolCall::from_tool_name(
            "cargo_check",
            serde_json::json!({"project":"demo","sync_wait_secs":1}),
        )
        .unwrap();
        normalize_structured_handoff(&mut shorter, policy);
        match shorter {
            ToolCall::CargoCheck { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, Some(1)),
            _ => unreachable!(),
        }
    }

    #[test]
    fn unconstrained_policy_preserves_canonical_omission() {
        let mut call =
            ToolCall::from_tool_name("cargo_check", serde_json::json!({"project":"demo"})).unwrap();
        normalize_structured_handoff(&mut call, ToolReturnTimingPolicy::unconstrained());
        match call {
            ToolCall::CargoCheck { sync_wait_secs, .. } => assert_eq!(sync_wait_secs, None),
            _ => unreachable!(),
        }
    }
}
