use serde::Serialize;
use webcodex_core::runtime_contract::MAX_JOB_OBSERVATION_WAIT_SECS;

pub(crate) const HOST_RETURN_GUARD_SECS: u64 = 5;
const DIRECT_DEFAULT_HOST_BUDGET_SECS: u64 = 60;
const HOST_CODE_MODE_DEFAULT_HOST_BUDGET_SECS: u64 = 55;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum McpHostProfile {
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

impl Default for McpHostProfile {
    fn default() -> Self {
        Self::Direct
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct McpHostRuntimePolicy {
    pub(crate) profile: McpHostProfile,
    pub(crate) host_budget_secs: u64,
    pub(crate) initial_job_handoff_secs: u64,
    pub(crate) max_sync_wait_secs: u64,
    pub(crate) continuation_wait_secs: u64,
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
            }
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
