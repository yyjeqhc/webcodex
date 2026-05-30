use crate::{Config, Database};
use salvo::prelude::*;
use std::sync::Arc;

pub(crate) fn get_config(depot: &Depot) -> Option<Arc<Config>> {
    depot.obtain::<Arc<Config>>().ok().cloned()
}

pub(crate) fn get_db(depot: &Depot) -> Option<Arc<Database>> {
    depot.obtain::<Arc<Database>>().ok().cloned()
}

pub(crate) fn check_auth(req: &Request, config: &Config) -> bool {
    if !config.is_auth_enabled() {
        return true;
    }
    let token = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(t) = token {
        return config.validate_token(t);
    }
    req.query::<String>("token")
        .map(|t| config.validate_token(&t))
        .unwrap_or(false)
}

pub(crate) fn json_error(_status: StatusCode, msg: &str) -> Json<serde_json::Value> {
    Json(serde_json::json!({"error": msg}))
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
            res.render(json_error(StatusCode::INTERNAL_SERVER_ERROR, "No config"));
            ctrl.skip_rest();
            return;
        };
        if !check_auth(req, &config) {
            res.status_code(StatusCode::UNAUTHORIZED);
            res.render(json_error(StatusCode::UNAUTHORIZED, "Unauthorized"));
            ctrl.skip_rest();
            return;
        }
        ctrl.call_next(req, depot, res).await;
    }
}
