//! Human-only Window annotations. A name belongs to the principal whose label
//! the caller's board row projects, so a saved name is the one it displays.
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
    require_runtime_read(auth)?;
    if !auth.has_scope(SCOPE_SESSION_COLLABORATE) {
        return Err(RuntimeConsoleError::Request {
            status: 403,
            message: "collaboration access denied",
        });
    }
    if !valid_window_key(&input.client_window_key) {
        return Err(RuntimeConsoleError::Invalid);
    }
    let label = webcodex_store::normalize_window_label(input.label.as_deref())
        .map_err(|_| RuntimeConsoleError::Invalid)?;
    // Resolve the owner through the same exact, freshly authorized inventory
    // read that projects the label. Collaboration's latest-recipient rule is
    // not reused: its newest event may be projectless and invisible to this
    // operator while an older Project cell still shows the Window, and its
    // active-request recipient may differ from the cell the board reads.
    let mut visibility = HashMap::new();
    let summary = visible_window_summary_for_auth(
        runtime,
        auth,
        &input.client_window_key,
        &mut visibility,
        None,
    )
    .await?
    .ok_or(RuntimeConsoleError::NotFound)?;
    let principal = summary.label_owner.ok_or(RuntimeConsoleError::Request {
        status: 409,
        message: "Window has no recorded activity to name yet",
    })?;
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
