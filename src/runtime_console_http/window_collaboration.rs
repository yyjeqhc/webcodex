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
) -> Result<(), RuntimeConsoleError> {
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
    // Console inventory may span credentials; collaboration never does.
    let (kind, principal) = crate::tool_runtime::runtime_observation_principal(Some(auth))
        .map_err(|_| RuntimeConsoleError::Request {
            status: 403,
            message: "collaboration access denied",
        })?;
    let visible = visible_window_summary_for_auth(
        runtime,
        auth,
        Some((&kind, &principal)),
        key,
        &mut HashMap::new(),
        None,
    )
    .await?;
    if visible.is_none() {
        return Err(RuntimeConsoleError::NotFound);
    }
    Ok(())
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
    if let Err(e) = authorize(&runtime, &auth, &input.client_window_key).await {
        return render_error(res, e);
    }
    let mut output = runtime.window_collaboration(
        Some(&input.client_window_key),
        Some(&auth),
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
    if let Err(e) = authorize(&runtime, &auth, &input.client_window_key).await {
        return render_error(res, e);
    }
    let result = runtime
        .post_window_operator_message_with_options(
            &input.client_window_key,
            input.context_session_id.as_deref(),
            None,
            input.kind.as_deref().unwrap_or("guidance"),
            input.priority.as_deref().unwrap_or("normal"),
            input.requires_ack.unwrap_or(true),
            input.message,
            input.delivery_key,
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
