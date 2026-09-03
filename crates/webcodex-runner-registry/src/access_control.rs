use crate::state::{ShellClientRecord, ShellClientRegistryInner, ShellJobRecord};
use crate::{RunnerAccess, RunnerAccessGroup};

pub(crate) fn assert_shell_client_owner(
    access: Option<&RunnerAccess>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    if access.map(|access| access.owner_bypass).unwrap_or(false) {
        return Ok(());
    }
    let owner = owner
        .filter(|owner| !owner.trim().is_empty())
        .ok_or_else(|| format!("agent client {} has no owner", client_id))?;
    let username = access
        .and_then(|access| access.username.as_deref())
        .filter(|username| !username.trim().is_empty());
    if username == Some(owner) {
        return Ok(());
    }
    let username = username.unwrap_or("anonymous");
    Err(format!(
        "agent client {} is owned by {}; current api key belongs to {}",
        client_id, owner, username
    ))
}

fn lightweight_group_matches(
    access: Option<&RunnerAccess>,
    group: Option<&RunnerAccessGroup>,
) -> bool {
    match group {
        Some(group) => access.and_then(|access| access.group.as_ref()) == Some(group),
        None => access.and_then(|access| access.group.as_ref()).is_none(),
    }
}

pub(crate) fn runner_visible_to_access(
    access: Option<&RunnerAccess>,
    client: &ShellClientRecord,
) -> bool {
    match access {
        None => true,
        Some(access) if access.global_visibility => true,
        Some(access) if !lightweight_group_matches(Some(access), client.auth_group.as_ref()) => {
            false
        }
        Some(_) if client.auth_group.is_some() => true,
        Some(access) => {
            let username = access
                .username
                .as_deref()
                .filter(|username| !username.trim().is_empty());
            let owner = client
                .owner
                .as_deref()
                .filter(|owner| !owner.trim().is_empty());
            username.is_some() && username == owner
        }
    }
}

pub(crate) fn assert_runner_access(
    access: Option<&RunnerAccess>,
    client: &ShellClientRecord,
) -> Result<(), String> {
    if !runner_visible_to_access(access, client) {
        return Err(format!("unknown shell client: {}", client.client_id));
    }
    if client.auth_group.is_some() {
        return Ok(());
    }
    assert_shell_client_owner(access, &client.client_id, client.owner.as_deref())
}

pub(crate) fn job_visible_to_access(
    access: Option<&RunnerAccess>,
    inner: &ShellClientRegistryInner,
    job: &ShellJobRecord,
) -> bool {
    let Some(access) = access else {
        return true;
    };
    if access.global_visibility {
        return true;
    }
    if let Some(group) = job.auth_group.as_ref() {
        return lightweight_group_matches(Some(access), Some(group));
    }
    inner
        .clients
        .get(&job.client_id)
        .map(|client| assert_runner_access(Some(access), client).is_ok())
        .unwrap_or(false)
}
