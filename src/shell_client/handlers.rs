use super::{
    effective_register_owner, enforce_agent_transport, enforce_register_owner, get_registry,
    require_agent_transport_scope,
};
use crate::shell_protocol::{
    ShellAgentJobUpdateRequest, ShellAgentJobUpdateResponse, ShellAgentPollRequest,
    ShellAgentPollResponse, ShellAgentResultRequest, ShellAgentResultResponse,
    ShellClientRegisterRequest, ShellClientRegisterResponse,
};
use salvo::prelude::*;

#[handler]
pub async fn shell_agent_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellClientRegisterRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    // Phase 3: agent transport endpoints require bootstrap or an agent token
    // with the agent:register scope. User tokens are rejected.
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_REGISTER)
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_register_owner(auth.as_ref(), &body.client_id, body.owner.as_deref()) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellClientRegisterResponse {
            success: false,
            client: None,
            error: Some(e),
        }));
        return;
    }
    // Resolve the effective owner: an agent token fills the owner from its
    // own username; bootstrap keeps the request body owner.
    let mut body = body;
    body.owner = effective_register_owner(auth.as_ref(), body.owner.as_deref());
    match registry.register_with_auth(body, auth.as_ref()).await {
        Ok(client) => res.render(Json(ShellClientRegisterResponse {
            success: true,
            client: Some(client),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellClientRegisterResponse {
                success: false,
                client: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_poll(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentPollRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_POLL) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = registry
        .assert_client_access(auth.as_ref(), &body.client_id)
        .await
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentPollResponse {
            success: false,
            request: None,
            error: Some(e),
        }));
        return;
    }
    match registry.poll(body).await {
        Ok(request) => res.render(Json(ShellAgentPollResponse {
            success: true,
            request,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentPollResponse {
                success: false,
                request: None,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_result(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentResultRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) = require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_RESULT) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = registry
        .assert_client_access(auth.as_ref(), &body.client_id)
        .await
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentResultResponse {
            success: false,
            error: Some(e),
        }));
        return;
    }
    match registry.complete(body).await {
        Ok(()) => res.render(Json(ShellAgentResultResponse {
            success: true,
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentResultResponse {
                success: false,
                error: Some(e),
            }));
        }
    }
}

#[handler]
pub async fn shell_agent_job_update(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(registry) = get_registry(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some("Shell client registry not configured".to_string()),
        }));
        return;
    };
    let body: ShellAgentJobUpdateRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(format!("Invalid JSON: {}", e)),
            }));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if let Err(e) =
        require_agent_transport_scope(auth.as_ref(), crate::auth::SCOPE_AGENT_JOB_UPDATE)
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = enforce_agent_transport(auth.as_ref(), &body.client_id) {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some(e),
        }));
        return;
    }
    if let Err(e) = registry
        .assert_client_access(auth.as_ref(), &body.client_id)
        .await
    {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(ShellAgentJobUpdateResponse {
            success: false,
            job: None,
            error: Some(e),
        }));
        return;
    }
    match registry.update_job(body).await {
        Ok(job) => res.render(Json(ShellAgentJobUpdateResponse {
            success: true,
            job: Some(job),
            error: None,
        })),
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(ShellAgentJobUpdateResponse {
                success: false,
                job: None,
                error: Some(e),
            }));
        }
    }
}
