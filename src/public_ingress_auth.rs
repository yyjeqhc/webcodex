//! Listener-derived public authority. No request header can manufacture this context.
use salvo::prelude::*;
use std::sync::Arc;
use webcodex_store::PublicIngressEntry;

#[derive(Clone)]
pub(crate) struct PublicIngressRequest(pub(crate) PublicIngressEntry);

pub(crate) fn entry(depot: &Depot) -> Option<&PublicIngressEntry> {
    depot
        .obtain::<PublicIngressRequest>()
        .ok()
        .map(|request| &request.0)
}

pub(crate) fn binding(depot: &Depot) -> Option<(String, i64)> {
    entry(depot).map(|entry| (entry.entry_id.clone(), entry.auth_epoch))
}

pub(crate) fn require_authority(
    req: &Request,
    depot: &Depot,
    config: &crate::Config,
) -> Result<(), (u16, &'static str, &'static str)> {
    let Some(entry) = entry(depot) else {
        return crate::auth::require_mcp_request_authority(req, config);
    };
    let origin = url::Url::parse(&entry.origin)
        .map_err(|_| (503, "ingress_unavailable", "public ingress unavailable"))?;
    let host = req
        .headers()
        .get("host")
        .and_then(|value| value.to_str().ok())
        .or_else(|| req.uri().authority().map(|authority| authority.as_str()))
        .ok_or((400, "invalid_request_authority", "invalid Host header"))?;
    let requested = url::Url::parse(&format!("https://{host}"))
        .map_err(|_| (400, "invalid_request_authority", "invalid Host header"))?;
    if requested.origin() != origin.origin()
        || requested.path() != "/"
        || !requested.username().is_empty()
        || requested.password().is_some()
        || requested.query().is_some()
        || requested.fragment().is_some()
    {
        return Err((
            403,
            "untrusted_request_authority",
            "public ingress Host mismatch",
        ));
    }
    if let Some(value) = req.headers().get("origin") {
        let value = value
            .to_str()
            .map_err(|_| (400, "invalid_origin", "invalid Origin header"))?;
        let parsed =
            url::Url::parse(value).map_err(|_| (400, "invalid_origin", "invalid Origin header"))?;
        if parsed.origin() != origin.origin()
            || !matches!(parsed.path(), "" | "/")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err((
                403,
                "cross_origin_denied",
                "cross-origin requests are not allowed",
            ));
        }
    }
    Ok(())
}

pub(crate) fn require_json_authority(
    req: &Request,
    depot: &Depot,
    config: &crate::Config,
) -> Result<(), (u16, &'static str, &'static str)> {
    if entry(depot).is_none() {
        return crate::auth::middleware::require_mcp_json_request(req, config);
    }
    require_authority(req, depot, config)?;
    if req
        .content_type()
        .is_none_or(|content_type| content_type.essence_str() != "application/json")
    {
        return Err((
            415,
            "unsupported_media_type",
            "Content-Type must be application/json",
        ));
    }
    Ok(())
}

/// Attached directly to matched OAuth routes on both listeners. Route matching
/// owns path normalization, so URI aliases cannot skip the audience check.
pub(crate) enum ClientIngressGate {
    ClientId,
    LoginReturnTo,
}

#[async_trait]
impl Handler for ClientIngressGate {
    async fn handle(
        &self,
        req: &mut Request,
        depot: &mut Depot,
        res: &mut Response,
        ctrl: &mut FlowCtrl,
    ) {
        let mut client_id = req.query::<String>("client_id");
        if req.method() == salvo::http::Method::POST {
            if let Ok(body) = req.payload().await {
                if body.len() <= 16 * 1024 {
                    let pairs: Vec<(String, String)> =
                        url::form_urlencoded::parse(body).into_owned().collect();
                    client_id = pairs
                        .iter()
                        .find(|(name, _)| name == "client_id")
                        .map(|(_, value)| value.clone())
                        .or(client_id);
                    if matches!(self, Self::LoginReturnTo) {
                        if let Some((_, return_to)) =
                            pairs.iter().find(|(name, _)| name == "return_to")
                        {
                            client_id = return_to.split_once('?').and_then(|(_, query)| {
                                url::form_urlencoded::parse(query.as_bytes())
                                    .find(|(name, _)| name == "client_id")
                                    .map(|(_, value)| value.into_owned())
                            });
                        }
                    }
                }
            }
        }
        if let Some(client_id) = client_id {
            let allowed = crate::auth::get_db(depot).is_some_and(|db| match entry(depot) {
                Some(entry) => db
                    .oauth_client_matches_public_ingress(&client_id, &entry.fence())
                    .unwrap_or(false),
                None => matches!(
                    db.oauth_client_public_ingress_entry_id(&client_id),
                    Ok(None)
                ),
            });
            if !allowed {
                res.status_code(StatusCode::BAD_REQUEST);
                res.headers_mut().insert(
                    "cache-control",
                    salvo::http::HeaderValue::from_static("no-store"),
                );
                res.render(Json(serde_json::json!({"error":"invalid_client"})));
                ctrl.skip_rest();
                return;
            }
        }
        ctrl.call_next(req, depot, res).await;
    }
}

pub(crate) async fn authenticate_mcp(
    req: &Request,
    depot: &Depot,
    config: &crate::Config,
    db: Option<&Arc<crate::Database>>,
) -> Option<crate::auth::AuthContext> {
    let entry = entry(depot)?;
    let token = crate::auth::bearer_token(req)?;
    crate::auth::tokens::OAuth2Verifier::verify_for_ingress(
        config,
        db,
        &token,
        Some(&entry.fence()),
    )
    .await
    .ok()
    .flatten()
}
#[cfg(test)]
mod tests;
