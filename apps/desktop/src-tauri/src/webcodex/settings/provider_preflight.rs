//! Availability checks never launch arbitrary providers or mutate user configuration.
use super::*;
use crate::{coding_agents::CodingAgentStore, mcp_providers::McpProviderStore};
use toml_edit::TableLike;

fn check(
    kind: &str,
    id: &str,
    name: &str,
    executable: &str,
    config_path: Option<&Path>,
) -> DesktopResult<()> {
    validate_provider_id(id).map_err(|_| error())?;
    validate_provider_name(name).map_err(|_| error())?;
    crate::mcp_providers::resolve_executable(executable).map(|_| ()).map_err(|_| {
        let mut details = serde_json::json!({"provider_kind": kind, "provider_id": id, "provider_name": name});
        if let Some(path) = config_path { details["provider_config_path"] = serde_json::json!(path); }
        DesktopError::new(
            if kind == "mcp" { "mcp_provider_executable_unavailable" } else { "coding_agent_executable_unavailable" },
            format!("{} provider “{}” cannot find or access its configured program", kind.to_uppercase(), name),
            "Edit the provider's program path, install its program, or explicitly disable/delete the provider, then start again.",
        ).with_details(details)
    })
}

pub fn preflight_providers(
    runtime: Option<&StoredRuntime>,
    mcp: &McpProviderStore,
    acp: &CodingAgentStore,
) -> DesktopResult<()> {
    for provider in mcp.profiles()?.iter().filter(|p| p.enabled) {
        check("mcp", &provider.id, &provider.name, &provider.command, None)?;
    }
    for provider in acp.profiles()?.iter().filter(|p| p.enabled) {
        check(
            "acp",
            &provider.provider_id,
            &provider.name,
            &provider.executable,
            None,
        )?;
    }
    let Some(runtime) = runtime.filter(|r| r.runner_config.is_some()) else {
        return Ok(());
    };
    let path = runtime.runner_config.as_ref().expect("checked above");
    let doc = parse(&read(path)?, runtime)?;
    // Operator-owned entries also affect startup. Desktop-owned entries use the
    // desired state above, so a disabled provider in an old slot cannot block recovery.
    for (kind, section, key, managed) in [
        ("mcp", "mcp", "providers", mcp.managed_ids()),
        ("acp", "acp", "agents", acp.managed_ids()),
    ] {
        let Some(entries) = doc.get(section).and_then(|s| s.get(key)) else {
            continue;
        };
        let check_table = |table: &dyn TableLike| -> DesktopResult<()> {
            let id = table.get("id").and_then(Item::as_str).ok_or_else(error)?;
            if managed.contains(id) {
                return Ok(());
            }
            let name = table.get("name").and_then(Item::as_str).unwrap_or(id);
            let executable = table
                .get("executable")
                .and_then(Item::as_str)
                .ok_or_else(error)?;
            check(kind, id, name, executable, Some(path))
        };
        if let Some(tables) = entries.as_array_of_tables() {
            for table in tables {
                check_table(table)?;
            }
        } else if let Some(array) = entries.as_array() {
            for entry in array {
                check_table(entry.as_inline_table().ok_or_else(error)?)?;
            }
        } else {
            return Err(error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
