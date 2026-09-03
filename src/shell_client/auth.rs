use super::state::{ShellClientRecord, ShellClientRegistryInner, ShellJobRecord};
use webcodex_runner_registry::{DetachedInitiatorIdentity, RunnerAccess, RunnerAccessGroup};

pub(crate) fn runner_access_from_auth(
    auth: Option<&crate::auth::AuthContext>,
) -> Option<RunnerAccess> {
    auth.map(|auth| RunnerAccess {
        admin: auth.is_admin(),
        username: auth.username.clone(),
        group: match auth.kind {
            crate::auth::AuthKind::ProjectCredential | crate::auth::AuthKind::AgentToken => auth
                .project_grant_id
                .clone()
                .map(RunnerAccessGroup::ProjectGrant),
            crate::auth::AuthKind::SharedKey => auth
                .shared_key_hash
                .clone()
                .map(RunnerAccessGroup::SharedKey),
            crate::auth::AuthKind::OAuth2Token if auth.is_oauth_shared_key_subject() => auth
                .shared_key_hash
                .clone()
                .map(RunnerAccessGroup::SharedKey),
            crate::auth::AuthKind::OpenAnonymous => Some(RunnerAccessGroup::OpenAnonymous),
            _ => None,
        },
    })
}

pub(crate) fn detached_initiator_identity_from_auth(
    auth: Option<&crate::auth::AuthContext>,
) -> Result<DetachedInitiatorIdentity, String> {
    let Some(auth) = auth else {
        return Ok(DetachedInitiatorIdentity::internal());
    };
    if auth.is_bootstrap() {
        return Ok(DetachedInitiatorIdentity::from_stable_principal(
            "bootstrap".to_string(),
        ));
    }
    if auth.is_oauth_token() && !auth.is_oauth_shared_key_subject() {
        let user_id = auth.user_id.as_deref().ok_or_else(|| {
            "detached idempotency requires a stable OAuth user identity".to_string()
        })?;
        let client_id = auth
            .allowed_client_id
            .as_deref()
            .unwrap_or("unknown-client");
        return Ok(DetachedInitiatorIdentity::from_stable_principal(format!(
            "oauth2:{user_id}:{client_id}"
        )));
    }
    if let Some(api_key_id) = auth.api_key_id.as_deref() {
        return Ok(DetachedInitiatorIdentity::from_stable_principal(format!(
            "{}:{api_key_id}",
            auth.principal_kind()
        )));
    }
    Err("detached idempotency requires a stable authenticated caller identity".to_string())
}

pub(crate) fn requested_by_from_auth(auth: Option<&crate::auth::AuthContext>) -> String {
    if auth.map(|auth| auth.is_bootstrap).unwrap_or(false) {
        return "bootstrap".to_string();
    }
    auth.and_then(|auth| auth.username.as_deref())
        .filter(|username| !username.trim().is_empty())
        .unwrap_or("anonymous")
        .to_string()
}

pub(crate) fn assert_shell_client_owner(
    access: Option<&RunnerAccess>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    if access.map(|access| access.admin).unwrap_or(false) {
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

pub(super) fn shell_client_visible_to_auth(
    access: Option<&RunnerAccess>,
    client: &ShellClientRecord,
) -> bool {
    match access {
        None => true,
        Some(access) if access.admin => true,
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

pub(super) fn assert_shell_client_access(
    access: Option<&RunnerAccess>,
    client: &ShellClientRecord,
) -> Result<(), String> {
    if !shell_client_visible_to_auth(access, client) {
        return Err(format!("unknown shell client: {}", client.client_id));
    }
    if client.auth_group.is_some() {
        return Ok(());
    }
    assert_shell_client_owner(access, &client.client_id, client.owner.as_deref())
}

pub(super) fn shell_job_visible_to_auth(
    access: Option<&RunnerAccess>,
    inner: &ShellClientRegistryInner,
    job: &ShellJobRecord,
) -> bool {
    let Some(access) = access else {
        return true;
    };
    if access.admin {
        return true;
    }
    if let Some(group) = job.auth_group.as_ref() {
        return lightweight_group_matches(Some(access), Some(group));
    }
    inner
        .clients
        .get(&job.client_id)
        .map(|client| assert_shell_client_access(Some(access), client).is_ok())
        .unwrap_or(false)
}

/// Enforce the owner/auth boundary at registration time. Mirrors
/// [`assert_shell_client_owner`] but is intentionally a no-op when no
/// `AuthContext` is present (unit tests that do not install `AuthMiddleware`).
/// In production every agent route is behind `AuthMiddleware`, which rejects
/// anonymous requests before the handler runs, so `auth` is always `Some`.
///
/// Rules:
/// - bootstrap token (or auth disabled) may register any owner;
/// - a direct shared key is authorized by its hash group and ignores owner;
/// - a normal API key may only register `owner == username`;
/// - a normal API key with a missing/empty owner is rejected, matching the
///   existing owner boundary enforced on later operations.
///
/// Phase 3 additions:
/// - an agent token may register only when its `allowed_client_id` matches
///   `client_id`;
/// - when an agent token authenticates owner "alice" and the request's
///   `owner` is `None`, the effective owner is "alice";
/// - when an agent token authenticates and `owner` is `Some("alice")`, it is
///   accepted;
/// - when an agent token authenticates and `owner` is `Some("bob")`, it is
///   rejected (agents may not claim another owner);
/// - a user token (Phase 2 personal API token) is rejected from agent transport
///   registration. Only bootstrap or agent tokens may use agent transport
///   endpoints.
pub(crate) fn enforce_register_owner(
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
    owner: Option<&str>,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    // Bootstrap may register any owner.
    if auth.is_bootstrap {
        return Ok(());
    }
    // Direct shared-key runners are authorized exclusively by the non-secret
    // hash captured in `RunnerAccessGroup`. The request owner is
    // intentionally ignored so it cannot become an authorization input.
    if auth.is_shared_key() {
        return Ok(());
    }
    // Phase 3: agent tokens are bound to an allowed_client_id and an owner.
    if auth.is_agent_token() {
        // allowed_client_id must match the registering client_id.
        match auth.allowed_client_id.as_deref() {
            Some(allowed) if allowed == client_id => {}
            _ => {
                return Err(format!(
                    "agent token is not bound to client_id '{}'",
                    client_id
                ));
            }
        }
        let token_username = auth
            .username
            .as_deref()
            .filter(|u| !u.trim().is_empty())
            .ok_or_else(|| "agent token has no owner".to_string())?;
        // If owner is supplied, it must match the token's owner.
        if let Some(req_owner) = owner.filter(|o| !o.trim().is_empty()) {
            if req_owner != token_username {
                return Err(format!(
                    "agent token owner is '{}'; cannot register owner '{}'",
                    token_username, req_owner
                ));
            }
        }
        return Ok(());
    }
    // Phase 2 user tokens and every other identity kind are rejected from
    // agent transport endpoints.
    Err("user tokens are not allowed on agent transport endpoints".to_string())
}

/// Resolve the effective owner for an agent register request. When the caller
/// is an agent token, the owner is the token's username regardless of the
/// request body. A direct shared key stores no owner because authorization is
/// hash-group based. When the caller is bootstrap, the request body owner is
/// used (or `None` when absent). Returns the owner to store on the registry
/// record.
pub(crate) fn effective_register_owner(
    auth: Option<&crate::auth::AuthContext>,
    owner: Option<&str>,
) -> Option<String> {
    let Some(auth) = auth else {
        return owner.map(str::to_string);
    };
    if auth.is_agent_token() {
        return auth.username.clone();
    }
    if auth.is_shared_key() {
        return None;
    }
    owner.filter(|o| !o.trim().is_empty()).map(str::to_string)
}

/// Enforce the agent transport boundary for poll/result/job_update endpoints.
/// These endpoints accept bootstrap, direct shared keys, or agent tokens. An
/// agent token must be bound to the request's `client_id`; shared keys are
/// subsequently bound by the registry's `RunnerAccessGroup` check.
///
/// This complements [`enforce_register_owner`] which handles the register
/// endpoint. Poll/result/job_update do not carry an owner field; the registry
/// already knows the owner from registration, so we only need to verify the
/// client_id matches the token's `allowed_client_id`.
pub(crate) fn enforce_agent_transport(
    auth: Option<&crate::auth::AuthContext>,
    client_id: &str,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    if auth.is_bootstrap {
        return Ok(());
    }
    if auth.is_shared_key() {
        return Ok(());
    }
    if auth.is_agent_token() {
        match auth.allowed_client_id.as_deref() {
            Some(allowed) if allowed == client_id => Ok(()),
            _ => Err(format!(
                "agent token is not bound to client_id '{}'",
                client_id
            )),
        }
    } else {
        Err("user tokens are not allowed on agent transport endpoints".to_string())
    }
}

/// Require the caller to hold `scope`. Used by agent transport endpoints to
/// check `agent:register` / `agent:poll` / `agent:result` / `agent:job_update`.
/// Bootstrap is always treated as holding every scope.
pub(crate) fn require_agent_transport_scope(
    auth: Option<&crate::auth::AuthContext>,
    scope: &str,
) -> Result<(), String> {
    let Some(auth) = auth else {
        return Ok(());
    };
    if auth.is_admin() {
        return Ok(());
    }
    if (auth.is_agent_token() || auth.is_shared_key()) && auth.scopes.iter().any(|s| s == scope) {
        Ok(())
    } else {
        Err(format!("missing required scope: {}", scope))
    }
}
