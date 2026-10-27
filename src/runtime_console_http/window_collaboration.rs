use super::window_target::authorize;
use super::*;
use serde_json::json;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListInput {
    client_window_key: String,
    limit: Option<usize>,
    before_message_id: Option<String>,
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
    if input.limit.is_some_and(|limit| !(1..=100).contains(&limit))
        || input.before_message_id.as_deref().is_some_and(|id| {
            !webcodex_core::workflow_session_contract::is_valid_session_message_id(id)
        })
    {
        return render_error(res, RuntimeConsoleError::Invalid);
    }
    let mut output = runtime.window_collaboration_page_for_principal(
        &input.client_window_key,
        &recipient_kind,
        &recipient_principal,
        input.limit.unwrap_or(50),
        input.before_message_id.as_deref(),
    );
    if output["error_kind"] == "history_cursor_unavailable" {
        res.status_code(StatusCode::BAD_REQUEST);
    }
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
