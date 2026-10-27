//! Human-only Window annotations. Reuse collaboration's exact recipient authority.
use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    pub client_window_key: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub label: Option<String>,
}

#[derive(Serialize)]
pub(super) struct Output {
    client_window_key: String,
    label: Option<String>,
}

pub(super) async fn set_for_auth(
    runtime: &ToolRuntime,
    auth: &AuthContext,
    input: Input,
) -> Result<Output, RuntimeConsoleError> {
    let principal = window_target::authorize(runtime, auth, &input.client_window_key).await?;
    let label = webcodex_store::normalize_window_label(input.label.as_deref())
        .map_err(|_| RuntimeConsoleError::Invalid)?;
    let db = runtime
        .window_activity_db
        .as_ref()
        .ok_or(RuntimeConsoleError::Internal)?;
    let label = db
        .set_window_label(
            (&principal.0, &principal.1),
            &input.client_window_key,
            label.as_deref(),
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(|_| RuntimeConsoleError::Internal)?;
    Ok(Output {
        client_window_key: input.client_window_key,
        label,
    })
}

#[handler]
pub(super) async fn set(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let (runtime, auth) = match prepared(req, depot).await {
        Ok(value) => value,
        Err(error) => return render_error(res, error),
    };
    let input = match req.parse_json::<Input>().await {
        Ok(value) => value,
        Err(_) => return render_error(res, RuntimeConsoleError::Invalid),
    };
    match set_for_auth(&runtime, &auth, input).await {
        Ok(value) => res.render(Json(value)),
        Err(error) => render_error(res, error),
    }
}
