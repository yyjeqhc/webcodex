//! Connection-configured, request-local MCP ergonomics. These headers are
//! preferences, never capability/authority claims or a new tool surface.
use crate::mcp_host::{
    McpHostConfig, McpHostPolicySelection, McpHostPolicySource, McpHostProfile,
    McpHostRuntimePolicy,
};
use salvo::http::HeaderMap;

pub(super) const PROFILE_HEADER: &str = "x-webcodex-mcp-profile";
pub(super) const BUDGET_HEADER: &str = "x-webcodex-mcp-budget-secs";

/// Resolve every request independently. Omission uses the deployment snapshot;
/// no ClientWindow, Session, brand, credential or preceding request is consulted.
/// A client may select a different waiting strategy, but cannot enlarge the
/// deployment's Host budget. This does not change any execution lifetime.
pub(super) fn resolve(
    headers: &HeaderMap,
    default: McpHostRuntimePolicy,
) -> Result<McpHostPolicySelection, &'static str> {
    let profile = singleton(headers, PROFILE_HEADER)?;
    let budget = singleton(headers, BUDGET_HEADER)?;
    let source = |present| {
        if present {
            McpHostPolicySource::RequestHeader
        } else {
            McpHostPolicySource::Deployment
        }
    };
    let profile_source = source(profile.is_some());
    let budget_source = source(budget.is_some());
    let profile = match profile {
        None => default.profile,
        Some("direct") => McpHostProfile::Direct,
        Some("host_code_mode") => McpHostProfile::HostCodeMode,
        Some(_) => return Err("X-WebCodex-MCP-Profile must be direct or host_code_mode"),
    };
    let requested_budget_secs = budget
        .map(|value| {
            value
                .parse::<u64>()
                .ok()
                .filter(|value| *value > 0)
                .ok_or("X-WebCodex-MCP-Budget-Secs must be a positive integer")
        })
        .transpose()?;
    let effective =
        if profile_source == McpHostPolicySource::Deployment && requested_budget_secs.is_none() {
            default
        } else {
            McpHostConfig {
                profile,
                host_budget_secs: Some(
                    requested_budget_secs
                        .unwrap_or(default.host_budget_secs)
                        .min(default.host_budget_secs),
                ),
            }
            .runtime_policy()
            .with_timing_overrides(default.request_timing_overrides())
        };
    Ok(McpHostPolicySelection {
        effective,
        profile_source,
        budget_source,
        requested_budget_secs,
        deployment_budget_secs: default.host_budget_secs,
    })
}

fn singleton<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, &'static str> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err("MCP request policy headers must not be repeated");
    }
    value
        .to_str()
        .map(|value| Some(value.trim()))
        .map_err(|_| "MCP request policy headers must contain ASCII text")
}
