//! Operator consent for the first execution binding on a legacy Workflow Session.
use super::*;
use crate::auth::AuthKind;
use crate::tool_runtime::{
    kernel::check_runtime_tool_scope, workflow_session_authority_fingerprint,
};
use webcodex_core::runner_protocol::{workflow_project_identity, RunnerProjectSummary};
use webcodex_workflow_session::SessionReauthorizationError;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReauthorizeRequest {
    project: String,
    session_id: String,
    expected_project_identity: String,
}

#[derive(Debug, Serialize)]
pub(super) struct RegistrationPreview {
    path: String,
    registration_incarnation: String,
    project_identity: String,
}

fn registration(project: &RunnerProjectSummary) -> Option<RegistrationPreview> {
    let incarnation = project.registration_incarnation.as_deref()?;
    Some(RegistrationPreview {
        path: bounded_project_path(&project.path)?,
        registration_incarnation: incarnation.to_owned(),
        project_identity: workflow_project_identity(
            project.root_fingerprint.as_deref(),
            &project.path,
            Some(incarnation),
        )?,
    })
}

// Direct owner credentials can operate the Console. Delegated MCP tokens,
// anonymous callers, and Runner transport credentials cannot grant a binding.
fn operator_owner(auth: &AuthContext) -> Result<String, RuntimeConsoleError> {
    require_project_read(auth)?;
    require_session_collaborate(auth)?;
    if !matches!(
        auth.kind,
        AuthKind::ApiToken
            | AuthKind::Bootstrap
            | AuthKind::SharedKey
            | AuthKind::ProjectCredential
    ) || check_runtime_tool_scope(Some(auth), "start_session").is_err()
    {
        return Err(RuntimeConsoleError::Request {
            status: 403,
            message: "Use the original owner's direct Console credential with permission to start a Session.",
        });
    }
    workflow_session_authority_fingerprint(Some(auth)).map_err(|_| RuntimeConsoleError::NotFound)
}

pub(super) async fn preview(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    project: &str,
    id: &str,
) -> Option<RegistrationPreview> {
    if runtime.sessions.session_project_identity(id) != Some(None) {
        return None;
    }
    let owner = operator_owner(auth).ok()?;
    runtime
        .sessions
        .with_active_session_authority_fence(id, project, &owner, || ())?;
    let access = crate::runner_http::runner_access_from_auth(Some(auth))?;
    runtime
        .runner_registry
        .with_live_session_registration(&access, project, None, |p, _| registration(p))
        .await
        .flatten()
}

const CHANGED: RuntimeConsoleError = RuntimeConsoleError::Request {
    status: 409,
    message:
        "Project registration changed. Refresh the Session and review the current registration.",
};
const UNCERTAIN: RuntimeConsoleError = RuntimeConsoleError::Request {
    status: 503,
    message: "Storage acknowledgement unavailable. Read the original Session summary and check for the authorization event before another confirmation.",
};

fn grant_error(error: SessionReauthorizationError) -> RuntimeConsoleError {
    match error {
        SessionReauthorizationError::UnknownSession => RuntimeConsoleError::NotFound,
        SessionReauthorizationError::Closed => RuntimeConsoleError::Request {
            status: 409,
            message: "This Session is closed.",
        },
        SessionReauthorizationError::DifferentRegistration => CHANGED,
        SessionReauthorizationError::UnresolvedExecution => RuntimeConsoleError::Request {
            status: 409,
            message: "Inspect the original Session's unfinished calls, Jobs and Shells. Reconcile outstanding execution before granting access.",
        },
        SessionReauthorizationError::StorageUnavailable => UNCERTAIN,
    }
}

async fn confirm(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: ReauthorizeRequest,
) -> Result<bool, RuntimeConsoleError> {
    let owner = operator_owner(auth)?;
    let id = &input.session_id;
    check_runtime_session_project(runtime, &input.project, id)?;
    // Conceal foreign Sessions before reporting live target diagnostics.
    if runtime
        .sessions
        .session_target_authority(id)
        .is_none_or(|(_, actual)| actual != owner)
    {
        return Err(RuntimeConsoleError::NotFound);
    }
    let _permit = runtime
        .runner_registry
        .admit_runtime_call()
        .await
        .map_err(|message| RuntimeConsoleError::Request {
            status: 503,
            message,
        })?;
    runtime
        .sessions
        .prepare_request()
        .await
        .map_err(|_| UNCERTAIN)?;
    let access = crate::runner_http::runner_access_from_auth(Some(auth))
        .ok_or(RuntimeConsoleError::NotFound)?;
    // An unbound Session cannot open a new Shell. Check the process-owned
    // registry for an existing one without contacting the Runner.
    let local_work = runtime.session_shells.has_active_shell(id).await
        || runtime.coding_agent_runs.has_active_recorder(id).await;
    let changed = runtime
        .runner_registry
        .with_live_session_registration(&access, &input.project, Some(id), |project, busy| {
            let target = registration(project).ok_or(CHANGED)?;
            if target.project_identity != input.expected_project_identity {
                return Err(CHANGED);
            }
            runtime
                .sessions
                .stage_legacy_project_authorization(
                    id,
                    &input.project,
                    &owner,
                    &target.project_identity,
                    busy || local_work,
                )
                .map_err(grant_error)
        })
        .await
        .ok_or(RuntimeConsoleError::Request {
            status: 409,
            message: "Reconnect the authorized Runner and refresh its complete Project inventory.",
        })??;
    runtime.sessions.flush().await.map_err(|_| UNCERTAIN)?;
    Ok(changed)
}

#[handler]
pub(super) async fn reauthorize(req: &mut Request, depot: &mut Depot) -> ConsoleResult {
    let (runtime, auth) = prepared(req, depot).await?;
    let input = parse_console_json(req).await?;
    Ok(Json(
        serde_json::json!({"changed": confirm(&runtime, &auth, input).await?}),
    ))
}
