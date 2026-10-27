use super::*;

fn service(runtime: Arc<ToolRuntime>, auth: AuthContext) -> Service {
    Service::new(
        Router::new()
            .hoop(affix_state::inject(runtime))
            .hoop(affix_state::inject(auth))
            .push(Router::with_path("api").push(routes())),
    )
}
async fn post(service: &Service, route: &str, body: Value) -> (StatusCode, Value) {
    let mut response = TestClient::post(format!("http://localhost/api/runtime-console/{route}"))
        .json(&body)
        .send(service)
        .await;
    (
        response.status_code.unwrap_or(StatusCode::OK),
        response.take_json::<Value>().await.unwrap(),
    )
}
fn writer() -> AuthContext {
    let mut auth = scoped_oauth(&[
        SCOPE_RUNTIME_READ,
        SCOPE_PROJECT_READ,
        SCOPE_SESSION_COLLABORATE,
    ]);
    auth.api_key_id = Some("label-writer".into());
    auth
}

#[tokio::test]
async fn window_labels_http_validation_and_exact_authority() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = writer();
    let key = "a".repeat(64);
    record_window_event(&db, &auth, &key, None, None, 1_000);
    let mut read_only = auth.clone();
    read_only
        .scopes
        .retain(|scope| scope != SCOPE_SESSION_COLLABORATE);
    let denied = service(runtime.clone(), read_only);
    assert_eq!(
        post(
            &denied,
            "window-label",
            json!({"client_window_key":key,"label":"Private"})
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    let allowed = service(runtime.clone(), auth.clone());
    assert_eq!(
        post(
            &allowed,
            "window-label",
            json!({"client_window_key":"b".repeat(64),"label":"Unknown"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    for label in [
        "x".repeat(81),
        "name\ncontrol".into(),
        "name\u{007f}".into(),
    ] {
        assert_eq!(
            post(
                &allowed,
                "window-label",
                json!({"client_window_key":key,"label":label})
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    for body in [
        json!({"client_window_key":key,"label":"valid","principal_id":"foreign"}),
        json!({"client_window_key":key}),
    ] {
        assert_eq!(
            post(&allowed, "window-label", body).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    let mut foreign = auth.clone();
    foreign.api_key_id = Some("foreign".into());
    foreign.user_id = Some("foreign-user".into());
    assert_eq!(
        post(
            &service(runtime.clone(), foreign),
            "window-label",
            json!({"client_window_key":key,"label":"Hidden"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let mut project_scoped = AuthContext::new(AuthKind::ProjectCredential);
    project_scoped.project_grant_id = Some("foreign-project-grant".into());
    project_scoped.scopes = auth.scopes.clone();
    assert_eq!(
        post(
            &service(runtime, project_scoped),
            "window-label",
            json!({"client_window_key":key,"label":"Hidden"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    let name = "窗".repeat(80);
    let (status, body) = post(
        &allowed,
        "window-label",
        json!({"client_window_key":key,"label":format!(" {name} ")}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"client_window_key":key,"label":name}));
    assert_eq!(
        post(
            &allowed,
            "window-label",
            json!({"client_window_key":key,"label":null})
        )
        .await
        .1["label"],
        Value::Null
    );
}

#[tokio::test]
async fn window_labels_admin_targets_window_principal_and_projections_stay_consistent() {
    let (_tmp, db, runtime) = test_runtime_with_window_db();
    let auth = writer();
    let key = "a".repeat(64);
    record_window_event(&db, &auth, &key, None, None, 1_000);
    let admin = service(runtime.clone(), test_bootstrap_auth());
    let (status, _) = post(
        &admin,
        "window-label",
        json!({"client_window_key":key,"label":"Research"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let principal = crate::tool_runtime::runtime_observation_principal(Some(&auth)).unwrap();
    assert_eq!(
        db.get_window_label((&principal.0, &principal.1), &key)
            .unwrap()
            .as_deref(),
        Some("Research")
    );
    let caller = service(runtime.clone(), auth.clone());
    let inventory = post(&caller, "windows", json!({"query":"research"}))
        .await
        .1;
    assert_eq!(inventory["windows"][0]["label"], "Research");
    for detail_level in ["primary", "full"] {
        let detail = post(
            &caller,
            "window",
            json!({"client_window_key":key,"detail_level":detail_level}),
        )
        .await
        .1;
        assert_eq!(detail["label"], inventory["windows"][0]["label"]);
    }
    post(
        &caller,
        "window-label",
        json!({"client_window_key":key,"label":"  "}),
    )
    .await;
    assert_eq!(
        post(&caller, "windows", json!({})).await.1["windows"][0].get("label"),
        Some(&Value::Null)
    );
    assert_eq!(
        post(&caller, "window", json!({"client_window_key":key}))
            .await
            .1
            .get("label"),
        Some(&Value::Null)
    );

    let target = crate::client_window::ClientWindow::for_test("live-label");
    let _guard = runtime.window_activity.start_authenticated(
        &target,
        "label-live",
        "tools/list",
        None,
        Some(&auth),
        2_000,
    );
    // A live-only Window has no recorded owner cell to attach a name to yet.
    let (status, _) = post(
        &caller,
        "window-label",
        json!({"client_window_key":target.key(),"label":"Too early"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    db.conn_for_tests()
        .authorizer(Some(|_: rusqlite::hooks::AuthContext<'_>| {
            rusqlite::hooks::Authorization::Deny
        }))
        .unwrap();
    let live = post(&caller, "windows", json!({"projection":"liveness"})).await;
    assert_eq!(live.0, StatusCode::OK);
    assert_eq!(live.1["windows"].as_array().unwrap().len(), 1);
    assert!(live.1["windows"][0].get("label").is_none());
}

#[tokio::test]
async fn window_labels_follow_the_board_row_when_latest_activity_is_projectless() {
    // A ChatGPT Window did Project work, then made a projectless call. A
    // non-admin WebUI credential with a different OAuth client sees the
    // Window through its Project cell and must be able to name that row.
    let (_tmp, db, runtime) = test_runtime_with_goal_db();
    let mut model = writer();
    model.api_key_id = Some("chatgpt-window-token".into());
    model.allowed_client_id = Some("chatgpt-client".into());
    let mut operator = model.clone();
    operator.api_key_id = Some("webui-operator-token".into());
    operator.allowed_client_id = Some("webui-client".into());
    assert!(!operator.is_admin_caller());
    let owner = crate::tool_runtime::runtime_observation_principal(Some(&model)).unwrap();
    assert_ne!(
        owner,
        crate::tool_runtime::runtime_observation_principal(Some(&operator)).unwrap()
    );
    let project = "agent:window-label:research";
    register_project(
        &runtime,
        "window-label",
        "research",
        "/private/window-label",
        Some(&model),
    )
    .await;
    let key = crate::client_window::ClientWindow::for_test("projectless-latest")
        .key()
        .to_string();
    record_window_event(&db, &model, &key, Some(project), None, 5_000);
    record_window_event(&db, &model, &key, None, None, 6_000);

    let operator_service = service(runtime.clone(), operator);
    let board = post(&operator_service, "windows", json!({})).await.1;
    assert_eq!(board["windows"][0]["client_window_key"], key);
    let (status, body) = post(
        &operator_service,
        "window-label",
        json!({"client_window_key":key,"label":"Research"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        db.get_window_label((&owner.0, &owner.1), &key)
            .unwrap()
            .as_deref(),
        Some("Research")
    );
    let board = post(&operator_service, "windows", json!({})).await.1;
    assert_eq!(board["windows"][0]["label"], "Research");
    let detail = post(
        &operator_service,
        "window",
        json!({"client_window_key":key}),
    )
    .await
    .1;
    assert_eq!(detail["label"], "Research");
    let model_board = post(&service(runtime, model), "windows", json!({})).await.1;
    assert_eq!(model_board["windows"][0]["label"], "Research");
}
