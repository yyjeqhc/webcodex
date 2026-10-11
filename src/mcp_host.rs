use serde::Serialize;
use webcodex_core::runtime_contract::{
    MAX_JOB_OBSERVATION_WAIT_SECS, STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS,
};
use webcodex_tool_contracts::tool_inputs::CodingGuidanceProfile;

pub(crate) const HOST_RETURN_GUARD_SECS: u64 = 5;
const DIRECT_DEFAULT_HOST_BUDGET_SECS: u64 = 60;
const HOST_CODE_MODE_DEFAULT_HOST_BUDGET_SECS: u64 = 55;

/// Optional deployment-owned latency caps. They never change Job execution
/// lifetimes or the protocol limits for structured and observation requests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct McpHostTimingOverrides {
    sync_wait_secs: Option<u64>,
    continuation_wait_secs: Option<u64>,
}

impl McpHostTimingOverrides {
    pub(crate) fn from_env() -> Result<Self, String> {
        fn read(name: &str, maximum: u64) -> Result<Option<u64>, String> {
            match std::env::var(name) {
                Err(std::env::VarError::NotPresent) => Ok(None),
                Ok(value) => value
                    .trim()
                    .parse::<u64>()
                    .ok()
                    .filter(|seconds| (1..=maximum).contains(seconds))
                    .map(Some)
                    .ok_or_else(|| format!("{name} must be an integer between 1 and {maximum}")),
                Err(std::env::VarError::NotUnicode(_)) => {
                    Err(format!("{name} must be an integer between 1 and {maximum}"))
                }
            }
        }
        Ok(Self {
            sync_wait_secs: read(
                "WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS",
                STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS,
            )?,
            continuation_wait_secs: read(
                "WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS",
                MAX_JOB_OBSERVATION_WAIT_SECS,
            )?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub(crate) enum McpHostProfile {
    #[default]
    Direct,
    HostCodeMode,
}

impl McpHostProfile {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::HostCodeMode => "host_code_mode",
        }
    }

    const fn default_host_budget_secs(self) -> u64 {
        match self {
            Self::Direct => DIRECT_DEFAULT_HOST_BUDGET_SECS,
            Self::HostCodeMode => HOST_CODE_MODE_DEFAULT_HOST_BUDGET_SECS,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct McpHostConfig {
    pub(crate) profile: McpHostProfile,
    pub(crate) host_budget_secs: Option<u64>,
}

impl Default for McpHostConfig {
    fn default() -> Self {
        Self {
            profile: McpHostProfile::Direct,
            host_budget_secs: None,
        }
    }
}

impl McpHostConfig {
    pub(crate) fn from_env() -> Self {
        let profile = std::env::var("WEBCODEX_MCP_HOST_PROFILE")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .and_then(|value| match value.as_str() {
                "direct" => Some(McpHostProfile::Direct),
                "host_code_mode" => Some(McpHostProfile::HostCodeMode),
                _ => None,
            })
            .unwrap_or_default();
        let host_budget_secs = std::env::var("WEBCODEX_MCP_HOST_BUDGET_SECS")
            .ok()
            .and_then(|value| value.trim().parse::<u64>().ok())
            .filter(|value| *value > 0);
        Self {
            profile,
            host_budget_secs,
        }
    }

    pub(crate) fn runtime_policy(self) -> McpHostRuntimePolicy {
        let host_budget_secs = self
            .host_budget_secs
            .unwrap_or_else(|| self.profile.default_host_budget_secs());
        let safe_wait_secs = host_budget_secs
            .saturating_sub(HOST_RETURN_GUARD_SECS)
            .max(1);

        let (initial_job_handoff_secs, max_sync_wait_secs, continuation_wait_secs) =
            match self.profile {
                McpHostProfile::Direct => (
                    10.min(safe_wait_secs),
                    60.min(safe_wait_secs),
                    MAX_JOB_OBSERVATION_WAIT_SECS.min(safe_wait_secs),
                ),
                McpHostProfile::HostCodeMode => {
                    let slice_secs = 5.min(safe_wait_secs);
                    (slice_secs, slice_secs, slice_secs)
                }
            };

        McpHostRuntimePolicy {
            profile: self.profile,
            host_budget_secs,
            initial_job_handoff_secs,
            max_sync_wait_secs,
            continuation_wait_secs,
            timing_overrides: McpHostTimingOverrides::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct McpHostRuntimePolicy {
    pub(crate) profile: McpHostProfile,
    pub(crate) host_budget_secs: u64,
    /// Legacy diagnostic-only projection. No execution path reads this field;
    /// synchronous handoff uses max_sync_wait_secs instead. Retained for
    /// existing runtime_status consumers, not a new operator tuning knob.
    pub(crate) initial_job_handoff_secs: u64,
    pub(crate) max_sync_wait_secs: u64,
    pub(crate) continuation_wait_secs: u64,
    /// Internal deployment authority carried through request profile changes.
    /// Never serialize into model-facing status or request-policy receipts.
    #[serde(skip)]
    timing_overrides: McpHostTimingOverrides,
}

/// Request-local diagnostic receipt. It records selection, not Host capability,
/// authority, execution lifetime, or proof that a response reached the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct McpHostPolicySelection {
    pub(crate) effective: McpHostRuntimePolicy,
    pub(crate) profile_source: McpHostPolicySource,
    pub(crate) budget_source: McpHostPolicySource,
    pub(crate) requested_budget_secs: Option<u64>,
    pub(crate) deployment_budget_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpHostPolicySource {
    Deployment,
    RequestHeader,
}

impl McpHostRuntimePolicy {
    /// Apply deployment choices to this request's profile/budget-derived policy.
    /// The explicit override can be above a profile's historical 5s default,
    /// but never above the request's safe Host budget or protocol caps.
    /// Reapply on each header-selected profile to prevent authority bypass.
    pub(crate) fn with_timing_overrides(mut self, overrides: McpHostTimingOverrides) -> Self {
        let safe_wait_secs = self
            .host_budget_secs
            .saturating_sub(HOST_RETURN_GUARD_SECS)
            .max(1);
        if let Some(limit) = overrides.sync_wait_secs {
            self.max_sync_wait_secs = limit
                .min(safe_wait_secs)
                .min(STRUCTURED_EXECUTION_SYNC_WAIT_MAX_SECS);
        }
        if let Some(limit) = overrides.continuation_wait_secs {
            self.continuation_wait_secs =
                limit.min(safe_wait_secs).min(MAX_JOB_OBSERVATION_WAIT_SECS);
        }
        self.timing_overrides = overrides;
        self
    }

    /// Request-local strategy resolution must keep the deployment's caps.
    pub(crate) fn request_timing_overrides(self) -> McpHostTimingOverrides {
        self.timing_overrides
    }

    /// One SSOT for model guidance defaults. Explicit request selection always
    /// wins; only MCP omission inherits the configured Host capability profile.
    pub(crate) const fn effective_guidance_profile(
        self,
        requested: Option<CodingGuidanceProfile>,
        mcp_transport: bool,
    ) -> CodingGuidanceProfile {
        if let Some(profile) = requested {
            return profile;
        }
        if !mcp_transport {
            return CodingGuidanceProfile::Direct;
        }
        match self.profile {
            McpHostProfile::Direct => CodingGuidanceProfile::Direct,
            McpHostProfile::HostCodeMode => CodingGuidanceProfile::HostCodeMode,
        }
    }
}

impl Default for McpHostRuntimePolicy {
    fn default() -> Self {
        McpHostConfig::default().runtime_policy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_policy_defaults_preserve_direct_behavior() {
        assert_eq!(
            McpHostConfig::default().runtime_policy(),
            McpHostRuntimePolicy {
                profile: McpHostProfile::Direct,
                host_budget_secs: 60,
                initial_job_handoff_secs: 10,
                max_sync_wait_secs: 55,
                continuation_wait_secs: 55,
                timing_overrides: McpHostTimingOverrides::default(),
            }
        );
        assert_eq!(
            McpHostConfig {
                profile: McpHostProfile::HostCodeMode,
                host_budget_secs: None,
            }
            .runtime_policy(),
            McpHostRuntimePolicy {
                profile: McpHostProfile::HostCodeMode,
                host_budget_secs: 55,
                initial_job_handoff_secs: 5,
                max_sync_wait_secs: 5,
                continuation_wait_secs: 5,
                timing_overrides: McpHostTimingOverrides::default(),
            }
        );
    }

    #[test]
    fn effective_guidance_profile_uses_explicit_then_mcp_host_then_direct() {
        let direct = McpHostConfig::default().runtime_policy();
        let host = McpHostConfig {
            profile: McpHostProfile::HostCodeMode,
            host_budget_secs: None,
        }
        .runtime_policy();

        assert_eq!(
            direct.effective_guidance_profile(None, true),
            CodingGuidanceProfile::Direct
        );
        assert_eq!(
            host.effective_guidance_profile(None, true),
            CodingGuidanceProfile::HostCodeMode
        );
        assert_eq!(
            host.effective_guidance_profile(Some(CodingGuidanceProfile::Direct), true),
            CodingGuidanceProfile::Direct
        );
        assert_eq!(
            direct.effective_guidance_profile(Some(CodingGuidanceProfile::HostCodeMode), true),
            CodingGuidanceProfile::HostCodeMode
        );
        assert_eq!(
            host.effective_guidance_profile(None, false),
            CodingGuidanceProfile::Direct
        );
        #[cfg(feature = "experimental-code-mode")]
        assert_eq!(
            direct.effective_guidance_profile(Some(CodingGuidanceProfile::CodeMode), true),
            CodingGuidanceProfile::CodeMode
        );
    }

    #[test]
    fn runtime_policy_honors_budget_overrides_and_small_budgets() {
        assert_eq!(
            McpHostConfig {
                profile: McpHostProfile::Direct,
                host_budget_secs: Some(20),
            }
            .runtime_policy()
            .continuation_wait_secs,
            15
        );
        assert_eq!(
            McpHostConfig {
                profile: McpHostProfile::HostCodeMode,
                host_budget_secs: Some(9),
            }
            .runtime_policy()
            .max_sync_wait_secs,
            4
        );
        assert_eq!(
            McpHostConfig {
                profile: McpHostProfile::Direct,
                host_budget_secs: Some(3),
            }
            .runtime_policy()
            .initial_job_handoff_secs,
            1
        );
    }

    #[test]
    fn explicit_deployment_waits_are_bounded_and_never_modify_legacy_handoff_hint() {
        let mut env = crate::test_support::TestEnvGuard::new();
        env.set("WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS", "8");
        env.set("WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS", "12");
        let overrides = McpHostTimingOverrides::from_env().unwrap();

        let host = McpHostConfig {
            profile: McpHostProfile::HostCodeMode,
            host_budget_secs: None,
        }
        .runtime_policy()
        .with_timing_overrides(overrides);
        assert_eq!(
            (host.max_sync_wait_secs, host.continuation_wait_secs),
            (8, 12)
        );
        assert_eq!(
            (host.host_budget_secs, host.initial_job_handoff_secs),
            (55, 5)
        );
        assert_eq!(host.request_timing_overrides(), overrides);
        let serde_value = serde_json::to_value(host).unwrap();
        assert_eq!(serde_value["max_sync_wait_secs"], 8);
        assert_eq!(serde_value["continuation_wait_secs"], 12);
        assert!(serde_value.get("timing_overrides").is_none());

        let small = McpHostConfig {
            profile: McpHostProfile::HostCodeMode,
            host_budget_secs: Some(9),
        }
        .runtime_policy()
        .with_timing_overrides(overrides);
        assert_eq!(
            (small.max_sync_wait_secs, small.continuation_wait_secs),
            (4, 4)
        );
        assert_eq!(small.host_budget_secs, 9);

        let direct = McpHostConfig {
            profile: McpHostProfile::Direct,
            host_budget_secs: None,
        }
        .runtime_policy()
        .with_timing_overrides(overrides);
        assert_eq!(
            (direct.max_sync_wait_secs, direct.continuation_wait_secs),
            (8, 12)
        );
        assert_eq!(direct.host_budget_secs, 60);
    }

    #[test]
    fn invalid_deployment_waits_fail_explicitly_without_echoing_secret_values() {
        let mut env = crate::test_support::TestEnvGuard::new();
        env.remove("WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS");
        env.remove("WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS");
        assert_eq!(
            McpHostTimingOverrides::from_env().unwrap(),
            McpHostTimingOverrides::default()
        );

        for invalid in ["0", "61", "-1", "1.5", "PRIVATE_INVALID_WAIT"] {
            env.set("WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS", invalid);
            let message = McpHostTimingOverrides::from_env().unwrap_err();
            assert!(message.contains("WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS"));
            if invalid.starts_with("PRIVATE_") {
                assert!(!message.contains(invalid));
            }
        }
        env.set("WEBCODEX_MCP_HOST_SYNC_WAIT_MAX_SECS", "60");
        for invalid in ["0", "101", "NaN", "PRIVATE_INVALID_WAIT"] {
            env.set("WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS", invalid);
            let message = McpHostTimingOverrides::from_env().unwrap_err();
            assert!(message.contains("WEBCODEX_MCP_HOST_CONTINUATION_WAIT_MAX_SECS"));
            if invalid.starts_with("PRIVATE_") {
                assert!(!message.contains(invalid));
            }
        }
    }

    #[test]
    fn from_env_uses_safe_defaults_for_missing_or_invalid_values() {
        let mut env = crate::test_support::TestEnvGuard::new();
        env.remove("WEBCODEX_MCP_HOST_PROFILE");
        env.remove("WEBCODEX_MCP_HOST_BUDGET_SECS");
        assert_eq!(McpHostConfig::from_env(), McpHostConfig::default());

        env.set("WEBCODEX_MCP_HOST_PROFILE", "host_code_mode");
        assert_eq!(
            McpHostConfig::from_env().runtime_policy().host_budget_secs,
            55
        );

        env.set("WEBCODEX_MCP_HOST_BUDGET_SECS", "37");
        assert_eq!(
            McpHostConfig::from_env().runtime_policy().host_budget_secs,
            37
        );

        env.set("WEBCODEX_MCP_HOST_PROFILE", "unsupported");
        env.set("WEBCODEX_MCP_HOST_BUDGET_SECS", "0");
        assert_eq!(
            McpHostConfig::from_env().runtime_policy(),
            McpHostRuntimePolicy::default()
        );
    }
}
