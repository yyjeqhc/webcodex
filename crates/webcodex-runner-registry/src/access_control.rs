use crate::state::{RunnerRecord, RunnerRegistryInner, ShellJobRecord};
use crate::{ExecutionAdmissionError, RunnerAccess, RunnerAccessGroup};

pub(crate) fn assert_runner_owner(
    access: Option<&RunnerAccess>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    if access.map(|access| access.owner_bypass).unwrap_or(false) {
        return Ok(());
    }
    let owner = owner
        .filter(|owner| !owner.trim().is_empty())
        .ok_or_else(|| format!("runner {} has no owner", client_id))?;
    let username = access
        .and_then(|access| access.username.as_deref())
        .filter(|username| !username.trim().is_empty());
    if username == Some(owner) {
        return Ok(());
    }
    let username = username.unwrap_or("anonymous");
    Err(format!(
        "runner {} is owned by {}; current api key belongs to {}",
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
    runner: &RunnerRecord,
) -> bool {
    match access {
        None => true,
        Some(access) if access.global_visibility => true,
        Some(access) if !lightweight_group_matches(Some(access), runner.auth_group.as_ref()) => {
            false
        }
        Some(_) if runner.auth_group.is_some() => true,
        Some(access) => {
            let username = access
                .username
                .as_deref()
                .filter(|username| !username.trim().is_empty());
            let owner = runner
                .owner
                .as_deref()
                .filter(|owner| !owner.trim().is_empty());
            username.is_some() && username == owner
        }
    }
}

pub(crate) fn assert_runner_access(
    access: Option<&RunnerAccess>,
    runner: &RunnerRecord,
) -> Result<(), String> {
    assert_runner_access_typed(access, runner).map_err(|error| error.to_string())
}

pub(crate) fn assert_runner_access_typed(
    access: Option<&RunnerAccess>,
    runner: &RunnerRecord,
) -> Result<(), ExecutionAdmissionError> {
    if !runner_visible_to_access(access, runner) {
        // Preserve concealment: an inaccessible Runner has the same typed
        // rejection as an unknown one, regardless of its identifier.
        return Err(ExecutionAdmissionError::unknown_runner(&runner.client_id));
    }
    if runner.auth_group.is_some() {
        return Ok(());
    }
    assert_runner_owner(access, &runner.client_id, runner.owner.as_deref())
        .map_err(ExecutionAdmissionError::permission_denied)
}

pub(crate) fn job_visible_to_access(
    access: Option<&RunnerAccess>,
    _inner: &RunnerRegistryInner,
    job: &ShellJobRecord,
) -> bool {
    if job
        .observation
        .receipt_expires_at
        .is_some_and(|expires| expires <= crate::now_ts())
    {
        return false;
    }
    let Some(access) = access else {
        return true;
    };
    if access.global_visibility {
        return true;
    }
    if let Some(group) = job.auth_group.as_ref() {
        return lightweight_group_matches(Some(access), Some(group));
    }
    access.group.is_none()
        && access.username.as_deref().is_some_and(|username| {
            !username.trim().is_empty() && job.owner_at_admission.as_deref() == Some(username)
        })
}
