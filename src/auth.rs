use crate::{Config, Database};
use salvo::prelude::*;
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AuthContext {
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_name: Option<String>,
    pub is_bootstrap: bool,
}

pub(crate) fn get_config(depot: &Depot) -> Option<Arc<Config>> {
    depot.obtain::<Arc<Config>>().ok().cloned()
}

pub(crate) fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

pub(crate) fn bearer_or_query_token(req: &Request) -> Option<String> {
    req.headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|v| v.to_string())
        .or_else(|| req.query::<String>("token"))
}

fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(crate) struct AuthMiddleware;

#[async_trait]
impl Handler for AuthMiddleware {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let Some(config) = get_config(depot) else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "No config"})));
            ctrl.skip_rest();
            return;
        };

        let db = get_db(depot);
        let token = bearer_or_query_token(req);

        if !config.is_auth_enabled() {
            let ctx = AuthContext {
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                is_bootstrap: true,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(token) = token else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        if config.validate_token(&token) {
            let ctx = AuthContext {
                user_id: None,
                username: None,
                api_key_id: None,
                api_key_name: None,
                is_bootstrap: true,
            };
            depot.inject(ctx);
            ctrl.call_next(req, depot, res).await;
            return;
        }

        let Some(db) = db else {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(Json(serde_json::json!({"error": "DB not available"})));
            ctrl.skip_rest();
            return;
        };

        let key_hash = hash_token(&token);

        let Ok(Some(api_key)) = db.get_api_key_by_hash(&key_hash) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "Unauthorized"})));
            ctrl.skip_rest();
            return;
        };

        let Ok(Some(user)) = db.get_user_by_id(&api_key.user_id) else {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "User not found"})));
            ctrl.skip_rest();
            return;
        };

        if user.disabled != 0 {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(Json(serde_json::json!({"error": "User disabled"})));
            ctrl.skip_rest();
            return;
        }

        if let Err(e) = db.update_api_key_last_used(&api_key.id, chrono::Utc::now().timestamp()) {
            tracing::warn!("failed to update api key last_used_at: {}", e);
        }

        let ctx = AuthContext {
            user_id: Some(user.id.clone()),
            username: Some(user.username.clone()),
            api_key_id: Some(api_key.id.clone()),
            api_key_name: Some(api_key.name.clone()),
            is_bootstrap: false,
        };

        depot.inject(ctx);
        ctrl.call_next(req, depot, res).await;
    }
}

pub(crate) fn json_error(status: StatusCode, msg: impl Into<String>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": status.as_u16(),
        "error": msg.into(),
    }))
}
