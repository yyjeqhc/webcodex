//! Reconcile only explicitly marked Desktop-owned ACP entries. The stable owner
//! marker plus desired-state tombstones prevents operator collisions and stale
//! enrollment slots from resurrecting removed providers. No environment is read.
use super::*;
use crate::coding_agents::{conflict, CodingAgentProfile, CodingAgentStore};
use std::collections::BTreeSet;
use toml_edit::{InlineTable, TableLike, Value};
use webcodex_core::coding_agent::CODING_AGENT_MAX_PROVIDERS;

const OWNER_KEY: &str = "desktop_owner";

pub fn reconcile_acp(
    runtime: &StoredRuntime,
    store: &CodingAgentStore,
    dry_run: bool,
) -> DesktopResult<()> {
    store.profiles()?;
    if !store.needs_reconciliation() {
        return Ok(());
    }
    let path = runtime.runner_config.as_ref().ok_or_else(error)?;
    let original = read(path)?;
    let mut doc = parse(&original, runtime)?;
    reconcile_document(&mut doc, store)?;
    if !dry_run && doc.to_string() != original {
        persist(path, &original, &doc)?;
    }
    Ok(())
}

fn reconcile_document(doc: &mut DocumentMut, store: &CodingAgentStore) -> DesktopResult<()> {
    let enabled: Vec<_> = store
        .profiles()?
        .iter()
        .filter(|profile| profile.enabled)
        .collect();
    let desired: BTreeSet<&str> = enabled.iter().map(|p| p.provider_id.as_str()).collect();
    if doc.get("acp").is_none() {
        doc["acp"] = Item::Table(Table::new());
    }
    if !doc["acp"].is_table_like() {
        return Err(error());
    }
    if doc["acp"].get("agents").is_none() {
        if let Some(inline) = doc["acp"].as_inline_table_mut() {
            inline.insert("agents", Value::Array(Array::new()));
        } else {
            doc["acp"]["agents"] = Item::ArrayOfTables(ArrayOfTables::new());
        }
    }
    let agents = &mut doc["acp"]["agents"];
    let mut seen = BTreeSet::new();
    let mut operator_count = 0;
    let mut check = |table: &dyn TableLike| -> DesktopResult<()> {
        let id = table.get("id").and_then(Item::as_str).ok_or_else(error)?;
        if !seen.insert(id.to_owned()) {
            return Err(error());
        }
        if store.managed_ids().contains(id) {
            if table.get(OWNER_KEY).and_then(Item::as_str) != Some(store.owner_id()) {
                return Err(conflict());
            }
        } else {
            operator_count += 1;
        }
        Ok(())
    };
    if let Some(tables) = agents.as_array_of_tables_mut() {
        for table in tables.iter() {
            check(table)?;
        }
        check_capacity(operator_count + enabled.len())?;
        for index in (0..tables.len()).rev() {
            let id = tables
                .get(index)
                .and_then(|t| t.get("id"))
                .and_then(Item::as_str)
                .ok_or_else(error)?;
            if store.managed_ids().contains(id) && !desired.contains(id) {
                tables.remove(index);
            }
        }
        for profile in enabled {
            let index = tables
                .iter()
                .position(|t| t.get("id").and_then(Item::as_str) == Some(&profile.provider_id));
            if let Some(index) = index {
                patch(
                    tables.get_mut(index).ok_or_else(error)?,
                    profile,
                    store.owner_id(),
                )?;
            } else {
                let mut table = Table::new();
                patch(&mut table, profile, store.owner_id())?;
                tables.push(table);
            }
        }
    } else if let Some(array) = agents.as_array_mut() {
        for value in array.iter() {
            check(value.as_inline_table().ok_or_else(error)?)?;
        }
        check_capacity(operator_count + enabled.len())?;
        for index in (0..array.len()).rev() {
            let id = array
                .get(index)
                .and_then(Value::as_inline_table)
                .and_then(|t| t.get("id"))
                .and_then(Value::as_str)
                .ok_or_else(error)?;
            if store.managed_ids().contains(id) && !desired.contains(id) {
                array.remove(index);
            }
        }
        for profile in enabled {
            let index = array.iter().position(|v| {
                v.as_inline_table()
                    .and_then(|t| t.get("id"))
                    .and_then(Value::as_str)
                    == Some(&profile.provider_id)
            });
            if let Some(index) = index {
                patch(
                    array
                        .get_mut(index)
                        .and_then(Value::as_inline_table_mut)
                        .ok_or_else(error)?,
                    profile,
                    store.owner_id(),
                )?;
            } else {
                let mut table = InlineTable::new();
                patch(&mut table, profile, store.owner_id())?;
                array.push(table);
            }
        }
    } else {
        return Err(error());
    }
    if let Some(settings) = store.global_settings() {
        let acp = doc["acp"].as_table_like_mut().ok_or_else(error)?;
        set(
            acp,
            "max_concurrent_runs",
            (settings.max_concurrent_runs as i64).into(),
        );
        set(
            acp,
            "permission_timeout_secs",
            (settings.permission_timeout_secs as i64).into(),
        );
    }
    Ok(())
}

fn patch(
    table: &mut dyn TableLike,
    profile: &CodingAgentProfile,
    owner: &str,
) -> DesktopResult<()> {
    crate::mcp_providers::resolve_executable(&profile.executable)
        .map_err(|_| crate::coding_agents::invalid())?;
    set(table, "id", profile.provider_id.clone().into());
    set(table, "name", profile.name.clone().into());
    set(table, "executable", profile.executable.clone().into());
    set(
        table,
        "args",
        profile.args.iter().cloned().collect::<Array>().into(),
    );
    let mut env = InlineTable::new();
    for (child, source) in &profile.env_from_env {
        env.insert(child, Value::from(source.as_str()));
    }
    set(table, "env_from_env", env.into());
    set(
        table,
        "allowed_config_options",
        profile
            .allowed_config_options
            .iter()
            .cloned()
            .collect::<Array>()
            .into(),
    );
    set(table, OWNER_KEY, owner.into());
    Ok(())
}

fn set(table: &mut dyn TableLike, key: &str, mut value: Value) {
    if let Some(old) = table.get(key).and_then(Item::as_value) {
        *value.decor_mut() = old.decor().clone();
    }
    table.insert(key, Item::Value(value));
}
fn check_capacity(count: usize) -> DesktopResult<()> {
    if count > CODING_AGENT_MAX_PROVIDERS {
        return Err(DesktopError::new(
            "coding_agent_capacity",
            "Runner Coding Agent capacity would be exceeded",
            "Disable a provider. Operator-owned providers count toward the same limit.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
