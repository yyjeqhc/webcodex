//! Authenticated Server-side admission fence for Core-managed upgrades.
use crate::auth::scopes::SCOPE_RUNNER_MANAGE;
use crate::auth::{AuthContext, AuthKind};
use crate::tool_runtime::ToolRuntime;
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use webcodex_runner_registry::MaintenanceScope;

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum RequestBody {
    Inspect,
    Begin {
        client_id: Option<String>,
        operation_nonce: String,
    },
    Renew {
        lease_token: String,
    },
    End {
        lease_token: String,
    },
    EndPending {
        operation_nonce: String,
    },
    RecoverExpiredRunner,
}

fn respond(res: &mut Response, status: StatusCode, code: &'static str) {
    res.status_code(status);
    res.render(Json(json!({"error": {"code": code}})));
}

fn owner(auth: &AuthContext) -> Option<String> {
    if auth.is_bootstrap() {
        return Some("bootstrap".to_string());
    }
    let id = auth.api_key_id.as_deref()?;
    if id.is_empty() || id.len() > 192 {
        return None;
    }
    Some(format!("api_key:{id}"))
}

#[handler]
pub(crate) async fn maintenance(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Ok(auth) = depot.obtain::<AuthContext>() else {
        return respond(res, StatusCode::UNAUTHORIZED, "authentication_required");
    };
    if !matches!(auth.kind, AuthKind::Bootstrap | AuthKind::ApiToken)
        || !auth.has_scope(SCOPE_RUNNER_MANAGE)
    {
        return respond(res, StatusCode::FORBIDDEN, "maintenance_forbidden");
    }
    let Some(owner) = owner(auth) else {
        return respond(res, StatusCode::FORBIDDEN, "stable_identity_required");
    };
    let auth = auth.clone();
    let Ok(runtime) = depot.obtain::<Arc<ToolRuntime>>() else {
        return respond(res, StatusCode::SERVICE_UNAVAILABLE, "runtime_unavailable");
    };
    let registry = runtime.runner_registry.clone();
    let Ok(bytes) = req.payload().await else {
        return respond(res, StatusCode::BAD_REQUEST, "invalid_request");
    };
    if bytes.len() > 4096 {
        return respond(res, StatusCode::PAYLOAD_TOO_LARGE, "request_too_large");
    }
    let Ok(body) = serde_json::from_slice::<RequestBody>(bytes) else {
        return respond(res, StatusCode::BAD_REQUEST, "invalid_request");
    };
    match body {
        RequestBody::Inspect => res.render(Json(json!({
            "contract":"webcodex_upgrade_maintenance_v1",
            "server_epoch": registry.observation_epoch_for_maintenance(),
        }))),
        RequestBody::Begin {
            client_id,
            operation_nonce,
        } => {
            let scope = match client_id {
                Some(id) => MaintenanceScope::Runner(id),
                None if auth.is_bootstrap() => MaintenanceScope::AllRuntimes,
                None => {
                    return respond(
                        res,
                        StatusCode::FORBIDDEN,
                        "server_maintenance_requires_bootstrap",
                    )
                }
            };
            let access = crate::runner_http::runner_access_from_auth(Some(&auth))
                .expect("authenticated first-party identity has Runner access");
            match registry
                .begin_maintenance(scope.clone(), &owner, &operation_nonce, &access)
                .await
            {
                Ok(grant) => {
                    let client_id = match &scope {
                        MaintenanceScope::AllRuntimes => None,
                        MaintenanceScope::Runner(id) => Some(id.as_str()),
                    };
                    if runtime
                        .coding_agent_runs
                        .active_runs_for_maintenance(client_id)
                        .await
                        != 0
                    {
                        let _ = registry.end_maintenance(&owner, grant.token()).await;
                        return respond(res, StatusCode::CONFLICT, "maintenance_busy_or_active");
                    }
                    res.render(Json(json!({
                        "contract": "webcodex_upgrade_maintenance_v1",
                        "lease_token": grant.token(),
                        "server_epoch": grant.server_epoch,
                        "ttl_secs": grant.ttl_secs,
                    })))
                }
                Err(_) => respond(res, StatusCode::CONFLICT, "maintenance_busy_or_active"),
            }
        }
        RequestBody::Renew { lease_token } => {
            if !valid_token(&lease_token) {
                return respond(res, StatusCode::BAD_REQUEST, "invalid_request");
            }
            match registry.renew_maintenance(&owner, &lease_token).await {
                Ok(ttl_secs) => res.render(Json(
                    json!({"contract":"webcodex_upgrade_maintenance_v1", "ttl_secs": ttl_secs}),
                )),
                Err(_) => respond(
                    res,
                    StatusCode::CONFLICT,
                    "maintenance_lease_invalid_or_expired",
                ),
            }
        }
        RequestBody::End { lease_token } => {
            if !valid_token(&lease_token) {
                return respond(res, StatusCode::BAD_REQUEST, "invalid_request");
            }
            match registry.end_maintenance(&owner, &lease_token).await {
                Ok(()) => res.render(Json(
                    json!({"contract":"webcodex_upgrade_maintenance_v1", "ended":true}),
                )),
                Err(_) => respond(res, StatusCode::CONFLICT, "maintenance_lease_invalid"),
            }
        }
        RequestBody::EndPending { operation_nonce } => {
            if !valid_token(&operation_nonce) {
                return respond(res, StatusCode::BAD_REQUEST, "invalid_request");
            }
            match registry
                .end_pending_maintenance(&owner, &operation_nonce)
                .await
            {
                Ok(()) => res.render(Json(
                    json!({"contract":"webcodex_upgrade_maintenance_v1", "ended":true}),
                )),
                Err(_) => respond(res, StatusCode::CONFLICT, "maintenance_lease_invalid"),
            }
        }
        RequestBody::RecoverExpiredRunner => {
            if !auth.is_bootstrap() {
                return respond(res, StatusCode::FORBIDDEN, "recovery_requires_bootstrap");
            }
            match registry.recover_expired_runner_maintenance().await {
                Ok(()) => res.render(Json(
                    json!({"contract":"webcodex_upgrade_maintenance_v1", "recovered":true}),
                )),
                Err(_) => respond(res, StatusCode::CONFLICT, "maintenance_recovery_unsafe"),
            }
        }
    }
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use serde_json::Value;

    fn service(auth: AuthContext) -> Service {
        let runtime = Arc::new(ToolRuntime::new(
            Arc::new(crate::RunnerRegistry::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        ));
        Service::new(
            Router::new()
                .hoop(affix_state::inject(runtime))
                .hoop(affix_state::inject(auth))
                .push(Router::with_path("api/runtime/upgrade-maintenance").post(maintenance)),
        )
    }

    async fn call(service: &Service, body: Value) -> (StatusCode, Value) {
        let mut response = TestClient::post("http://127.0.0.1/api/runtime/upgrade-maintenance")
            .add_header("content-type", "application/json", true)
            .body(body.to_string())
            .send(service)
            .await;
        let status = response.status_code.unwrap();
        (status, response.take_json().await.unwrap())
    }

    #[tokio::test]
    async fn maintenance_http_requires_first_party_authority_and_exact_token() {
        let mut delegated = AuthContext::new(AuthKind::OAuth2Token);
        delegated.scopes = vec![SCOPE_RUNNER_MANAGE.to_string()];
        let delegated_service = service(delegated);
        assert_eq!(
            call(
                &delegated_service,
                json!({"action":"begin","operation_nonce":"a".repeat(64)})
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );

        let mut bootstrap = AuthContext::new(AuthKind::Bootstrap);
        bootstrap.is_bootstrap = true;
        let service = service(bootstrap);
        let (status, begin) = call(
            &service,
            json!({"action":"begin","operation_nonce":"a".repeat(64)}),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(begin["contract"], "webcodex_upgrade_maintenance_v1");
        let token = begin["lease_token"].as_str().unwrap();
        assert!(valid_token(token));
        let (retry_status, retry) = call(
            &service,
            json!({"action":"begin","operation_nonce":"a".repeat(64)}),
        )
        .await;
        assert_eq!(retry_status, StatusCode::OK);
        assert_eq!(retry["lease_token"], token);
        assert_eq!(
            call(
                &service,
                json!({"action":"begin","operation_nonce":"b".repeat(64)})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(
                &service,
                json!({"action":"end","lease_token":"0".repeat(64)})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(&service, json!({"action":"end","lease_token":token}))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &service,
                json!({"action":"begin","operation_nonce":"c".repeat(64)})
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &service,
                json!({"action":"end_pending","operation_nonce":"b".repeat(64)})
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                &service,
                json!({"action":"begin","operation_nonce":"d".repeat(64)})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(
            call(
                &service,
                json!({"action":"end_pending","operation_nonce":"c".repeat(64)})
            )
            .await
            .0,
            StatusCode::OK
        );
    }
}
