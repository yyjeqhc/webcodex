use super::*;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListInput {
    client_window_key: String,
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PostInput {
    client_window_key: String,
    message: String,
    delivery_key: String,
    context_session_id: Option<String>,
    kind: Option<String>,
    priority: Option<String>,
    requires_ack: Option<bool>,
}

pub(super) async fn authorize(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    key: &str,
) -> Result<(String, String), RuntimeConsoleError> {
    require_runtime_read(auth)?;
    if !auth.has_scope(SCOPE_SESSION_COLLABORATE) {
        return Err(RuntimeConsoleError::Request {
            status: 403,
            message: "collaboration access denied",
        });
    }
    if !valid_window_key(key) {
        return Err(RuntimeConsoleError::Invalid);
    }

    // Runtime Console is an operator surface: a Window may remain visible after
    // the WebUI credential rotates. Authorize through the same project-scoped
    // visibility rules as Window inventory, then route the durable message to
    // the latest visible recipient principal observed on that exact Window.
    // Project-scoped model credentials remain principal-bound because
    // window_principal_filter returns their exact principal.
    let discovery_principal = window_principal_filter(auth)?;
    let discovery_principal_ref = window_principal_ref(&discovery_principal);
    let caller_principal = if discovery_principal.is_none() && !auth.is_admin_caller() {
        crate::tool_runtime::runtime_observation_principal(Some(auth)).ok()
    } else {
        None
    };
    let caller_principal_ref = window_principal_ref(&caller_principal);
    let mut visibility_cache = HashMap::new();

    let latest_active = runtime
        .window_activity
        .list_for_window(key, discovery_principal_ref)
        .into_iter()
        .next();
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let latest_event = db
        .list_window_activity_events(key, discovery_principal_ref, 1)
        .map_err(|_| RuntimeConsoleError::Internal)?
        .into_iter()
        .next();

    let active_at = latest_active.as_ref().map(|request| request.started_at_ms);
    let event_at = latest_event
        .as_ref()
        .map(|event| event.request_observed_at_ms.unwrap_or(event.started_at_ms));
    if active_at.is_none() && event_at.is_none() {
        return Err(RuntimeConsoleError::NotFound);
    }
    if active_at >= event_at {
        let request = latest_active.ok_or(RuntimeConsoleError::NotFound)?;
        if !console_active_window_request_visible_cached(
            runtime,
            auth,
            discovery_principal_ref,
            caller_principal_ref,
            &mut visibility_cache,
            &request,
        )
        .await
        {
            return Err(RuntimeConsoleError::NotFound);
        }
        let (kind, principal) = request
            .principal_correlation()
            .ok_or(RuntimeConsoleError::NotFound)?;
        return Ok((kind.to_string(), principal.to_string()));
    }

    let event = latest_event.ok_or(RuntimeConsoleError::NotFound)?;
    if !console_window_event_visible_cached(
        runtime,
        auth,
        discovery_principal_ref,
        caller_principal_ref,
        &mut visibility_cache,
        &event,
    )
    .await
    {
        return Err(RuntimeConsoleError::NotFound);
    }
    match (
        event.principal_correlation_kind,
        event.principal_correlation_id,
    ) {
        (Some(kind), Some(principal)) => Ok((kind, principal)),
        _ => Err(RuntimeConsoleError::NotFound),
    }
}

#[handler]
pub(super) async fn list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(v) => v,
        Err(e) => return render_error(res, e),
    };
    let input = match req.parse_json::<ListInput>().await {
        Ok(v) => v,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    let (recipient_kind, recipient_principal) =
        match authorize(&runtime, &auth, &input.client_window_key).await {
            Ok(recipient) => recipient,
            Err(e) => return render_error(res, e),
        };
    let mut output = runtime.window_collaboration_for_principal(
        &input.client_window_key,
        &recipient_kind,
        &recipient_principal,
        input.limit.unwrap_or(50),
    );
    output["client_window_key"] = json!(input.client_window_key);
    res.render(Json(output));
}

#[handler]
pub(super) async fn post(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(v) => v,
        Err(e) => return render_error(res, e),
    };
    let input = match req.parse_json::<PostInput>().await {
        Ok(v) => v,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    let (recipient_kind, recipient_principal) =
        match authorize(&runtime, &auth, &input.client_window_key).await {
            Ok(recipient) => recipient,
            Err(e) => return render_error(res, e),
        };
    let result = runtime
        .post_window_operator_message_with_options_for_principal(
            &input.client_window_key,
            input.context_session_id.as_deref(),
            None,
            input.kind.as_deref().unwrap_or("guidance"),
            input.priority.as_deref().unwrap_or("normal"),
            input.requires_ack.unwrap_or(true),
            input.message,
            input.delivery_key,
            &recipient_kind,
            &recipient_principal,
            Some(&auth),
        )
        .await;
    if !result.success {
        res.status_code(if result.output["failure_kind"] == "outcome_unknown" {
            StatusCode::SERVICE_UNAVAILABLE
        } else if result.output["failure_kind"] == "conflict" {
            StatusCode::CONFLICT
        } else {
            StatusCode::BAD_REQUEST
        });
        res.render(Json(result));
    } else {
        res.render(Json(result.output));
    }
}
