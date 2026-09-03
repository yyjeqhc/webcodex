use salvo::prelude::*;

use crate::auth::{
    generate_oauth_authorization_code, hash_token, shared_key_hash_of, AuthContext,
    DIRECT_SHARED_KEY_MODEL_SCOPES, SCOPE_CODING_AGENT_RUN, SCOPE_COMMUNICATION_MANAGE,
    SCOPE_COMMUNICATION_READ, SCOPE_COMPUTER_CLIPBOARD_READ, SCOPE_COMPUTER_CLIPBOARD_WRITE,
    SCOPE_COMPUTER_CONTROL, SCOPE_COMPUTER_DISPLAY_READ, SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_POINTER_CONTROL, SCOPE_COMPUTER_READ, SCOPE_JOB_RUN, SCOPE_MCP_LOCAL,
    SCOPE_MEMORY_MANAGE, SCOPE_MEMORY_READ, SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE,
    SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
};
use crate::models::OAuthAuthorizationCodeRecord;
use crate::runner_http::{RunnerFeature, RunnerFeatureSet};

use super::{
    apply_oauth_no_store_headers, authorize_bridge_html, decoded_authorize_param, form_field,
    normalize_oauth_scopes, oauth_authorize_direct_error, parse_authorize_query, parse_form_body,
    redirect_with_authorization_code, redirect_with_oauth_error, validate_authorize_resource,
    validate_redirect_uri, BridgePermissionView, OAuthAuthorizeError, OAuthAuthorizeRequest,
    OAUTH_OFFLINE_ACCESS_SCOPE,
};

pub(crate) const OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE: &str =
    "bridge tokens exceed the ordinary shared-key OAuth scope ceiling";

pub(crate) const SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES: &[&str] = &[
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
];

/// Canonical ceiling for a fresh Computer-enabled shared-key OAuth client.
/// Existing clients may retain a narrower non-empty baseline subset and add the
/// same fixed optional Computer scopes.
pub(crate) const SHARED_KEY_OAUTH_COMPUTER_ENABLED_SCOPES: &[&str] = &[
    SCOPE_RUNTIME_READ,
    SCOPE_SESSION_COLLABORATE,
    SCOPE_PROJECT_READ,
    SCOPE_PROJECT_WRITE,
    SCOPE_MEMORY_READ,
    SCOPE_MEMORY_MANAGE,
    SCOPE_COMMUNICATION_READ,
    SCOPE_COMMUNICATION_MANAGE,
    SCOPE_JOB_RUN,
    SCOPE_COMPUTER_READ,
    SCOPE_COMPUTER_CONTROL,
    SCOPE_COMPUTER_LAUNCH,
    SCOPE_COMPUTER_DISPLAY_READ,
    SCOPE_COMPUTER_POINTER_CONTROL,
    SCOPE_COMPUTER_CLIPBOARD_READ,
    SCOPE_COMPUTER_CLIPBOARD_WRITE,
];

#[derive(Debug, Clone, Copy)]
struct BridgePermissionSpec {
    id: &'static str,
    label: &'static str,
    scopes: &'static [&'static str],
    request_scopes: &'static [&'static str],
}

const BRIDGE_PERMISSION_SPECS: &[BridgePermissionSpec] = &[
    BridgePermissionSpec {
        id: "launch",
        label: "Launch applications",
        scopes: &[SCOPE_COMPUTER_LAUNCH],
        request_scopes: &[SCOPE_COMPUTER_READ, SCOPE_COMPUTER_LAUNCH],
    },
    BridgePermissionSpec {
        id: "display",
        label: "Full-display observation",
        scopes: &[SCOPE_COMPUTER_DISPLAY_READ],
        request_scopes: &[SCOPE_COMPUTER_READ, SCOPE_COMPUTER_DISPLAY_READ],
    },
    BridgePermissionSpec {
        id: "pointer",
        label: "Pointer control",
        scopes: &[SCOPE_COMPUTER_DISPLAY_READ, SCOPE_COMPUTER_POINTER_CONTROL],
        request_scopes: &[
            SCOPE_COMPUTER_READ,
            SCOPE_COMPUTER_CONTROL,
            SCOPE_COMPUTER_DISPLAY_READ,
            SCOPE_COMPUTER_POINTER_CONTROL,
        ],
    },
    BridgePermissionSpec {
        id: "clipboard_read",
        label: "Read clipboard",
        scopes: &[SCOPE_COMPUTER_CLIPBOARD_READ],
        request_scopes: &[SCOPE_COMPUTER_READ, SCOPE_COMPUTER_CLIPBOARD_READ],
    },
    BridgePermissionSpec {
        id: "clipboard_write",
        label: "Write clipboard",
        scopes: &[SCOPE_COMPUTER_CLIPBOARD_WRITE],
        request_scopes: &[SCOPE_COMPUTER_CONTROL, SCOPE_COMPUTER_CLIPBOARD_WRITE],
    },
];

pub(crate) fn bridge_oauth_scopes() -> &'static [&'static str] {
    DIRECT_SHARED_KEY_MODEL_SCOPES
}

#[cfg(test)]
pub(crate) fn bridge_oauth_computer_enabled_scopes() -> &'static [&'static str] {
    SHARED_KEY_OAUTH_COMPUTER_ENABLED_SCOPES
}

fn bridge_permission_spec(id: &str) -> Option<&'static BridgePermissionSpec> {
    BRIDGE_PERMISSION_SPECS
        .iter()
        .find(|permission| permission.id == id)
}

fn bridge_scope_list_is_unique(scopes: &[String]) -> bool {
    scopes
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
        == scopes.len()
}

fn bridge_baseline_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    !scopes.is_empty()
        && bridge_scope_list_is_unique(scopes)
        && scopes
            .iter()
            .all(|scope| bridge_oauth_scopes().contains(&scope.as_str()))
}

fn bridge_computer_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    if scopes.is_empty() || !bridge_scope_list_is_unique(scopes) {
        return false;
    }

    let mut has_baseline_scope = false;
    for scope in scopes {
        if bridge_oauth_scopes().contains(&scope.as_str()) {
            has_baseline_scope = true;
        } else if !SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES.contains(&scope.as_str()) {
            return false;
        }
    }

    has_baseline_scope
        && SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES
            .iter()
            .all(|required| scopes.iter().any(|scope| scope == required))
}

fn bridge_computer_enabled_scope_ceiling(scopes: &[String]) -> Option<Vec<String>> {
    if bridge_computer_scope_ceiling_is_valid(scopes) {
        return Some(scopes.to_vec());
    }
    if !bridge_baseline_scope_ceiling_is_valid(scopes) {
        return None;
    }

    let mut expanded = scopes.to_vec();
    expanded.extend(
        SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES
            .iter()
            .map(|scope| (*scope).to_string()),
    );
    Some(expanded)
}

fn bridge_scope_ceiling_without_optional_class_scopes(scopes: &[String]) -> Option<Vec<String>> {
    if !bridge_scope_list_is_unique(scopes) {
        return None;
    }
    Some(
        scopes
            .iter()
            .filter(|scope| !matches!(scope.as_str(), SCOPE_MCP_LOCAL | SCOPE_CODING_AGENT_RUN))
            .cloned()
            .collect(),
    )
}

fn bridge_scope_ceiling_is_valid(scopes: &[String]) -> bool {
    let Some(base) = bridge_scope_ceiling_without_optional_class_scopes(scopes) else {
        return false;
    };
    bridge_baseline_scope_ceiling_is_valid(&base) || bridge_computer_scope_ceiling_is_valid(&base)
}

fn bridge_scope_ceiling_with_options(
    scopes: &[String],
    computer_permissions: bool,
    local_mcp: bool,
    coding_agent: bool,
) -> Option<Vec<String>> {
    let base = bridge_scope_ceiling_without_optional_class_scopes(scopes)?;
    let mut desired = if computer_permissions {
        bridge_computer_enabled_scope_ceiling(&base)?
    } else if bridge_baseline_scope_ceiling_is_valid(&base) {
        base
    } else {
        return None;
    };
    if local_mcp {
        desired.push(SCOPE_MCP_LOCAL.to_string());
    }
    if coding_agent {
        desired.push(SCOPE_CODING_AGENT_RUN.to_string());
    }
    Some(desired)
}

fn bridge_scope_profile_error(
    computer_permissions: bool,
    local_mcp: bool,
    coding_agent: bool,
) -> &'static str {
    match (computer_permissions, local_mcp, coding_agent) {
        (true, false, false) => "persisted OAuth scope ceiling is not valid for Computer opt-in",
        (false, true, false) => "persisted OAuth scope ceiling is not valid for local MCP opt-in",
        (false, false, true) => "persisted OAuth scope ceiling is not valid for coding-agent opt-in",
        _ => "persisted OAuth scope ceiling is not valid for the requested explicit permission profile",
    }
}

fn bridge_client_is_computer_enabled(client: &crate::models::OAuthClientRecord) -> bool {
    if !client.is_shared_key_owned() {
        return false;
    }
    bridge_scope_ceiling_without_optional_class_scopes(&client.allowed_scopes_vec())
        .is_some_and(|scopes| bridge_computer_scope_ceiling_is_valid(&scopes))
}

fn bridge_client_has_local_mcp_scope(client: &crate::models::OAuthClientRecord) -> bool {
    client
        .allowed_scopes_vec()
        .iter()
        .any(|scope| scope == SCOPE_MCP_LOCAL)
}

fn bridge_client_has_coding_agent_scope(client: &crate::models::OAuthClientRecord) -> bool {
    client
        .allowed_scopes_vec()
        .iter()
        .any(|scope| scope == SCOPE_CODING_AGENT_RUN)
}

fn bridge_client_has_optional_computer_scope(client: &crate::models::OAuthClientRecord) -> bool {
    client
        .allowed_scopes_vec()
        .iter()
        .any(|scope| SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES.contains(&scope.as_str()))
}

fn is_managed_credential_like(value: &str) -> bool {
    value.starts_with("wc_")
}

pub(crate) fn bridge_shared_key_hash(value: &str) -> Result<String, &'static str> {
    let value = value.trim();
    if value.is_empty() {
        return Err("a shared key is required");
    }
    if is_managed_credential_like(value) {
        return Err("managed credentials cannot be used as shared keys");
    }

    Ok(shared_key_hash_of(value))
}

pub(crate) fn normalize_bridge_oauth_scopes(
    requested: Option<&str>,
    client_allowed: &str,
) -> Result<String, OAuthAuthorizeError> {
    let normalized = normalize_oauth_scopes(requested, client_allowed)?;
    if normalized.split_whitespace().any(|scope| {
        scope != OAUTH_OFFLINE_ACCESS_SCOPE
            && scope != SCOPE_MCP_LOCAL
            && scope != SCOPE_CODING_AGENT_RUN
            && !SHARED_KEY_OAUTH_COMPUTER_ENABLED_SCOPES.contains(&scope)
    }) {
        return Err(OAuthAuthorizeError::InvalidScope(
            OAUTH_BRIDGE_INVALID_SCOPE_MESSAGE,
        ));
    }
    Ok(normalized)
}

#[derive(Clone)]
pub(super) struct BridgeAuthorizeValidated {
    parsed: OAuthAuthorizeRequest,
    client: crate::models::OAuthClientRecord,
    requestable_scopes: String,
    resource: Option<String>,
    computer_permissions_enabled: bool,
}

impl BridgeAuthorizeValidated {
    fn standard_grant_scopes(&self) -> Vec<String> {
        self.requestable_scopes
            .split_whitespace()
            .filter(|scope| {
                bridge_oauth_scopes().contains(scope)
                    || matches!(*scope, SCOPE_MCP_LOCAL | SCOPE_CODING_AGENT_RUN)
            })
            .map(str::to_string)
            .collect()
    }

    fn requested_scope_contains(&self, scope: &str) -> bool {
        self.requestable_scopes
            .split_whitespace()
            .any(|requested| requested == scope)
    }
}

fn bridge_permission_capable(permission_id: &str, features: &RunnerFeatureSet) -> bool {
    match permission_id {
        "launch" => {
            features.supports(RunnerFeature::ComputerApplicationDiscovery)
                && features.supports(RunnerFeature::ComputerApplicationLaunch)
        }
        "display" => features.supports(RunnerFeature::ComputerDisplayObserve),
        "pointer" => {
            features.supports(RunnerFeature::ComputerDisplayObserve)
                && features.supports(RunnerFeature::ComputerPointerControl)
        }
        "clipboard_read" => features.supports(RunnerFeature::ComputerClipboardRead),
        "clipboard_write" => features.supports(RunnerFeature::ComputerClipboardWrite),
        _ => false,
    }
}

async fn bridge_permission_views(
    validated: &BridgeAuthorizeValidated,
    registry: Option<&crate::RunnerRegistry>,
    selected_permissions: &[String],
) -> Vec<BridgePermissionView> {
    if !validated.computer_permissions_enabled {
        return Vec::new();
    }
    let capabilities = match (registry, validated.client.owner_shared_key_hash.as_deref()) {
        (Some(registry), Some(owner_hash)) => {
            registry
                .connected_shared_key_group_feature_sets(owner_hash)
                .await
        }
        _ => Vec::new(),
    };
    BRIDGE_PERMISSION_SPECS
        .iter()
        .map(|permission| {
            let request_allowed = permission
                .request_scopes
                .iter()
                .all(|scope| validated.requested_scope_contains(scope));
            let capability_available = capabilities
                .iter()
                .any(|runner| bridge_permission_capable(permission.id, runner));
            let available = request_allowed && capability_available;
            let availability = if !request_allowed {
                "Not requested by this OAuth authorization request"
            } else if capability_available {
                "Available on a connected Runner"
            } else {
                "Not currently supported by a connected Runner"
            };
            BridgePermissionView {
                id: permission.id,
                label: permission.label,
                available,
                selected: selected_permissions
                    .iter()
                    .any(|selected| selected == permission.id),
                availability,
            }
        })
        .collect()
}

fn selected_bridge_grant_scopes(
    validated: &BridgeAuthorizeValidated,
    selected_permissions: &[String],
) -> Result<String, &'static str> {
    let mut optional_scopes = std::collections::HashSet::new();
    let mut seen_permissions = std::collections::HashSet::new();
    for permission_id in selected_permissions {
        if !seen_permissions.insert(permission_id.as_str()) {
            return Err("duplicate Computer permission selection");
        }
        let permission =
            bridge_permission_spec(permission_id).ok_or("invalid Computer permission selection")?;
        if !validated.computer_permissions_enabled {
            return Err("Computer permissions are not enabled for this OAuth client");
        }
        if !permission
            .request_scopes
            .iter()
            .all(|scope| validated.requested_scope_contains(scope))
        {
            return Err("Computer permission was not fully requested by this OAuth request");
        }
        optional_scopes.extend(permission.scopes.iter().copied());
    }

    Ok(validated
        .requestable_scopes
        .split_whitespace()
        .filter(|scope| {
            *scope == OAUTH_OFFLINE_ACCESS_SCOPE
                || bridge_oauth_scopes().contains(scope)
                || *scope == SCOPE_MCP_LOCAL
                || optional_scopes.contains(scope)
        })
        .collect::<Vec<_>>()
        .join(" "))
}

pub(super) fn is_shared_key_bridge_query(query: &str) -> Result<bool, OAuthAuthorizeError> {
    match decoded_authorize_param(query, "bridge")? {
        Some(value) if value == "shared_key" => Ok(true),
        Some(_) => Err(OAuthAuthorizeError::InvalidRequest("unsupported bridge")),
        None => Ok(false),
    }
}

pub(super) fn validate_bridge_authorize_request(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    query: &str,
) -> Option<BridgeAuthorizeValidated> {
    let parsed = match parse_authorize_query(query) {
        Ok(parsed) => parsed,
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid authorization request",
            );
            return None;
        }
    };

    if parsed.client_id.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing client_id",
        );
        return None;
    }

    let client = match db.get_oauth_client_by_client_id(&parsed.client_id) {
        Ok(Some(client)) => client,
        Ok(None) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid client_id",
            );
            return None;
        }
        Err(_) => {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "internal error",
            );
            return None;
        }
    };

    if parsed.redirect_uri.is_empty() {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "missing redirect_uri",
        );
        return None;
    }

    if !client
        .redirect_uris_vec()
        .iter()
        .any(|registered| registered == &parsed.redirect_uri)
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri mismatch",
        );
        return None;
    }

    if parsed.response_type.is_empty() || parsed.response_type != "code" {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "unsupported_response_type",
            parsed.state.as_deref(),
        );
        return None;
    }

    if parsed.code_challenge.is_empty()
        || parsed.code_challenge_method.is_empty()
        || parsed.code_challenge_method != "S256"
    {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "invalid_request",
            parsed.state.as_deref(),
        );
        return None;
    }

    let requestable_scopes =
        match normalize_bridge_oauth_scopes(parsed.scope.as_deref(), &client.allowed_scopes) {
            Ok(scopes) => scopes,
            Err(_) => {
                redirect_with_oauth_error(
                    res,
                    config,
                    &parsed.redirect_uri,
                    "invalid_scope",
                    parsed.state.as_deref(),
                );
                return None;
            }
        };
    let computer_permissions_enabled = bridge_client_is_computer_enabled(&client);
    let local_mcp_enabled =
        client.is_shared_key_owned() && bridge_client_has_local_mcp_scope(&client);
    let coding_agent_enabled =
        client.is_shared_key_owned() && bridge_client_has_coding_agent_scope(&client);
    let client_bridge_ceiling = if computer_permissions_enabled {
        SHARED_KEY_OAUTH_COMPUTER_ENABLED_SCOPES
    } else {
        bridge_oauth_scopes()
    };
    if requestable_scopes.split_whitespace().any(|scope| {
        scope != OAUTH_OFFLINE_ACCESS_SCOPE
            && !(scope == SCOPE_MCP_LOCAL && local_mcp_enabled)
            && !(scope == SCOPE_CODING_AGENT_RUN && coding_agent_enabled)
            && !client_bridge_ceiling.contains(&scope)
    }) {
        redirect_with_oauth_error(
            res,
            config,
            &parsed.redirect_uri,
            "invalid_scope",
            parsed.state.as_deref(),
        );
        return None;
    }

    let resource = match validate_authorize_resource(parsed.resource.as_deref(), config) {
        Ok(resource) => resource,
        Err(_) => {
            redirect_with_oauth_error(
                res,
                config,
                &parsed.redirect_uri,
                "invalid_target",
                parsed.state.as_deref(),
            );
            return None;
        }
    };

    Some(BridgeAuthorizeValidated {
        parsed,
        client,
        requestable_scopes,
        resource,
        computer_permissions_enabled,
    })
}

pub(super) async fn render_bridge_authorize_form(
    res: &mut Response,
    validated: &BridgeAuthorizeValidated,
    query: &str,
    error: Option<&str>,
    registry: Option<&crate::RunnerRegistry>,
    selected_permissions: &[String],
) {
    let standard_scopes = validated.standard_grant_scopes();
    let permissions = bridge_permission_views(validated, registry, selected_permissions).await;
    let html = authorize_bridge_html(
        &validated.client.name,
        &validated.client.client_id,
        &validated.parsed.redirect_uri,
        &standard_scopes,
        &permissions,
        validated.resource.as_deref(),
        query,
        error,
    );
    res.status_code(if error.is_some() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::OK
    });
    res.render(Text::Html(html));
}

fn issue_bridge_authorization_code(
    res: &mut Response,
    config: &crate::Config,
    db: &crate::Database,
    validated: &BridgeAuthorizeValidated,
    shared_key_hash: String,
    granted_scopes: String,
) {
    let now = chrono::Utc::now().timestamp();
    let plaintext_code = generate_oauth_authorization_code();
    let code_hash = hash_token(&plaintext_code);
    let record = OAuthAuthorizationCodeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        code_hash,
        client_id: validated.client.client_id.clone(),
        subject_kind: "shared_key".to_string(),
        subject_id: shared_key_hash.clone(),
        user_id: None,
        redirect_uri: validated.parsed.redirect_uri.clone(),
        scopes: granted_scopes,
        resource: validated.resource.clone(),
        code_challenge: Some(validated.parsed.code_challenge.clone()),
        code_challenge_method: Some("S256".to_string()),
        shared_key_hash: Some(shared_key_hash),
        created_at: now,
        expires_at: now + config.oauth2.authorization_code_ttl_secs,
        used_at: None,
        revoked_at: None,
    };

    if db
        .insert_oauth_authorization_code(&record, &record.code_hash)
        .is_err()
    {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "internal error",
        );
        return;
    }

    redirect_with_authorization_code(
        res,
        config,
        &validated.parsed.redirect_uri,
        &plaintext_code,
        validated.parsed.state.as_deref(),
    );
}

#[derive(Debug, serde::Deserialize)]
struct ProvisionSharedKeyOAuthClientRequest {
    redirect_uri: String,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    previous_allowed_scopes: Option<Vec<String>>,
    #[serde(default)]
    computer_permissions: bool,
    #[serde(default)]
    local_mcp: bool,
    #[serde(default)]
    coding_agent: bool,
}

fn bridge_client_scopes_are_current(client: &crate::models::OAuthClientRecord) -> bool {
    bridge_scope_ceiling_is_valid(&client.allowed_scopes_vec())
}

#[handler]
pub(crate) async fn oauth_shared_key_client_provision(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "no config"})));
        return;
    };
    if !config.oauth2.enabled || !config.oauth2.shared_key_bridge_enabled {
        res.status_code(StatusCode::NOT_FOUND);
        res.render(Json(
            serde_json::json!({"error": "shared-key OAuth bridge is not enabled"}),
        ));
        return;
    }
    let Some(auth) = depot.obtain::<AuthContext>().ok() else {
        res.status_code(StatusCode::UNAUTHORIZED);
        res.render(Json(
            serde_json::json!({"error": "direct shared key required"}),
        ));
        return;
    };
    if !auth.is_shared_key() {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(
            serde_json::json!({"error": "direct shared key required"}),
        ));
        return;
    }
    let Some(shared_key_hash) = auth.shared_key_hash.as_deref() else {
        res.status_code(StatusCode::FORBIDDEN);
        res.render(Json(
            serde_json::json!({"error": "direct shared key identity is unavailable"}),
        ));
        return;
    };
    let Some(db) = crate::auth::get_db(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(serde_json::json!({"error": "DB not available"})));
        return;
    };
    let Some(registry) = depot.obtain::<std::sync::Arc<crate::RunnerRegistry>>().ok() else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            serde_json::json!({"error": "Runner registry unavailable"}),
        ));
        return;
    };
    if !registry
        .has_connected_shared_key_group(shared_key_hash)
        .await
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": "shared-key Runner group is not connected"
        })));
        return;
    }
    let body: ProvisionSharedKeyOAuthClientRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(Json(serde_json::json!({"error": "invalid request body"})));
            return;
        }
    };
    let redirect_uri = body.redirect_uri.trim().to_string();
    if let Err(error) = validate_redirect_uri(&redirect_uri) {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(Json(serde_json::json!({"error": error})));
        return;
    }

    if let Some(client_id) = body
        .client_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        match db.get_oauth_client_by_client_id(client_id) {
            Ok(Some(mut client)) => {
                if !client.is_shared_key_owned()
                    || client.owner_shared_key_hash.as_deref() != Some(shared_key_hash)
                {
                    res.status_code(StatusCode::FORBIDDEN);
                    res.render(Json(serde_json::json!({"error": "OAuth client belongs to a different shared-key group"})));
                    return;
                }
                if client.redirect_uris_vec() != vec![redirect_uri.clone()] {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(
                        serde_json::json!({"error": "OAuth client redirect URI does not match"}),
                    ));
                    return;
                }
                if !bridge_client_scopes_are_current(&client) {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({"error": "persisted OAuth client exceeds the current ordinary shared-key OAuth scope ceiling"})));
                    return;
                }
                if !body.computer_permissions && bridge_client_has_optional_computer_scope(&client)
                {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({
                        "error": "OAuth client has optional Computer permissions enabled; reconnect with --oauth-computer-permissions to reuse this client"
                    })));
                    return;
                }
                if !body.local_mcp && bridge_client_has_local_mcp_scope(&client) {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({
                        "error": "OAuth client has local MCP authority enabled; reconnect with --oauth-local-mcp to reuse this client"
                    })));
                    return;
                }
                if !body.coding_agent && bridge_client_has_coding_agent_scope(&client) {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({
                        "error": "OAuth client has coding-agent authority enabled; reconnect with --oauth-coding-agent to reuse this client"
                    })));
                    return;
                }

                let current_scopes = client.allowed_scopes_vec();
                let Some(desired_scope_vec) = bridge_scope_ceiling_with_options(
                    &current_scopes,
                    body.computer_permissions,
                    body.local_mcp,
                    body.coding_agent,
                ) else {
                    res.status_code(StatusCode::CONFLICT);
                    res.render(Json(serde_json::json!({
                        "error": bridge_scope_profile_error(
                            body.computer_permissions,
                            body.local_mcp,
                            body.coding_agent,
                        )
                    })));
                    return;
                };
                let desired_scopes = if desired_scope_vec == current_scopes {
                    // Preserve byte-for-byte formatting for a semantic no-op so
                    // reconnect cannot revoke grants by normalization alone.
                    client.allowed_scopes.clone()
                } else {
                    desired_scope_vec.join(" ")
                };
                let update = match db.update_oauth_client_allowed_scopes_and_revoke_grants(
                    &client.client_id,
                    &client.allowed_scopes,
                    &desired_scopes,
                    chrono::Utc::now().timestamp(),
                ) {
                    Ok(Some(update)) => update,
                    Ok(None) => {
                        res.status_code(StatusCode::CONFLICT);
                        res.render(Json(serde_json::json!({
                            "error": "OAuth client changed while reconnecting; retry the explicit connect operation"
                        })));
                        return;
                    }
                    Err(_) => {
                        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                        res.render(Json(serde_json::json!({
                            "error": "failed to update OAuth client scope ceiling"
                        })));
                        return;
                    }
                };
                let scope_ceiling_changed = update.0;
                client.allowed_scopes = desired_scopes;
                apply_oauth_no_store_headers(res);
                res.render(Json(serde_json::json!({
                    "success": true,
                    "reused": true,
                    "scope_ceiling_changed": scope_ceiling_changed,
                    "client": {
                        "client_id": client.client_id,
                        "redirect_uri": redirect_uri,
                        "allowed_scopes": client.allowed_scopes_vec(),
                    }
                })));
                return;
            }
            Ok(None) => {}
            Err(_) => {
                res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
                res.render(Json(serde_json::json!({
                    "error": "failed to read existing OAuth client"
                })));
                return;
            }
        }
    }

    let base_scopes = body.previous_allowed_scopes.clone().unwrap_or_else(|| {
        bridge_oauth_scopes()
            .iter()
            .map(|scope| (*scope).to_string())
            .collect()
    });
    if !body.computer_permissions
        && base_scopes
            .iter()
            .any(|scope| SHARED_KEY_OAUTH_OPTIONAL_COMPUTER_SCOPES.contains(&scope.as_str()))
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": "persisted OAuth profile is Computer-enabled; reconnect with --oauth-computer-permissions"
        })));
        return;
    }
    if !body.local_mcp && base_scopes.iter().any(|scope| scope == SCOPE_MCP_LOCAL) {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": "persisted OAuth profile has local MCP authority enabled; reconnect with --oauth-local-mcp"
        })));
        return;
    }
    if !body.coding_agent
        && base_scopes
            .iter()
            .any(|scope| scope == SCOPE_CODING_AGENT_RUN)
    {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": "persisted OAuth profile has coding-agent authority enabled; reconnect with --oauth-coding-agent"
        })));
        return;
    }
    let Some(create_scopes) = bridge_scope_ceiling_with_options(
        &base_scopes,
        body.computer_permissions,
        body.local_mcp,
        body.coding_agent,
    ) else {
        res.status_code(StatusCode::CONFLICT);
        res.render(Json(serde_json::json!({
            "error": bridge_scope_profile_error(
                body.computer_permissions,
                body.local_mcp,
                body.coding_agent,
            )
        })));
        return;
    };

    let plaintext_secret = crate::auth::generate_oauth_client_secret();
    let record = crate::models::OAuthClientRecord {
        id: uuid::Uuid::new_v4().to_string(),
        client_id: crate::auth::generate_oauth_client_id(),
        client_secret_hash: hash_token(&plaintext_secret),
        name: "WebCodex hosted shared-key bridge".to_string(),
        owner_user_id: None,
        owner_project_grant_id: None,
        owner_shared_key_hash: Some(shared_key_hash.to_string()),
        redirect_uris: redirect_uri.clone(),
        allowed_scopes: create_scopes.join(" "),
        created_at: chrono::Utc::now().timestamp(),
        revoked_at: None,
    };
    if db.insert_oauth_client(&record).is_err() {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(Json(
            serde_json::json!({"error": "failed to create OAuth client"}),
        ));
        return;
    }
    apply_oauth_no_store_headers(res);
    res.render(Json(serde_json::json!({
        "success": true,
        "reused": false,
        "client": {
            "client_id": record.client_id,
            "redirect_uri": redirect_uri,
            "allowed_scopes": record.allowed_scopes_vec(),
        },
        "client_secret": plaintext_secret,
    })));
}

#[handler]
pub(crate) async fn oauth_authorize_bridge(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let Some(config) = crate::auth::get_config(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "no config",
        );
        return;
    };

    if !config.oauth2.enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "OAuth2 is not enabled",
        );
        return;
    }

    if !config.oauth2.shared_key_bridge_enabled {
        oauth_authorize_direct_error(
            res,
            StatusCode::NOT_FOUND,
            "invalid_request",
            "shared-key OAuth bridge is not enabled",
        );
        return;
    }

    let Some(db) = crate::auth::get_db(depot) else {
        oauth_authorize_direct_error(
            res,
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "DB not available",
        );
        return;
    };

    let pairs = match parse_form_body(req).await {
        Some(pairs) => pairs,
        None => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "invalid request body",
            );
            return;
        }
    };

    let selected_permissions = pairs
        .iter()
        .filter(|(key, _)| key == "computer_permission")
        .map(|(_, value)| value.clone())
        .collect::<Vec<_>>();
    let query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in pairs
            .iter()
            .filter(|(key, _)| key != "shared_key" && key != "computer_permission")
        {
            serializer.append_pair(key, value);
        }
        serializer.finish()
    };

    match is_shared_key_bridge_query(&query) {
        Ok(true) => {}
        _ => {
            oauth_authorize_direct_error(
                res,
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "unsupported bridge",
            );
            return;
        }
    }

    let Some(validated) = validate_bridge_authorize_request(res, &config, &db, &query) else {
        return;
    };

    let registry = depot
        .obtain::<std::sync::Arc<crate::RunnerRegistry>>()
        .ok()
        .cloned();
    let submitted = form_field(&pairs, "shared_key").unwrap_or("");
    let shared_key_hash = match bridge_shared_key_hash(submitted) {
        Ok(hash) => hash,
        Err(message) => {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some(message),
                registry.as_deref(),
                &selected_permissions,
            )
            .await;
            return;
        }
    };
    if let Some(owner_hash) = validated.client.owner_shared_key_hash.clone() {
        if !crate::config::constant_time_eq(shared_key_hash.as_bytes(), owner_hash.as_bytes()) {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some("shared key is not valid for this OAuth client"),
                registry.as_deref(),
                &selected_permissions,
            )
            .await;
            return;
        }
        let Some(registry) = registry.as_deref() else {
            oauth_authorize_direct_error(
                res,
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "Runner registry unavailable",
            );
            return;
        };
        if !registry.has_connected_shared_key_group(&owner_hash).await {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some("shared-key Runner group is no longer connected"),
                Some(registry),
                &selected_permissions,
            )
            .await;
            return;
        }
    }

    let granted_scopes = match selected_bridge_grant_scopes(&validated, &selected_permissions) {
        Ok(scopes) => scopes,
        Err(message) => {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some(message),
                registry.as_deref(),
                &selected_permissions,
            )
            .await;
            return;
        }
    };
    if !selected_permissions.is_empty() {
        let permission_views =
            bridge_permission_views(&validated, registry.as_deref(), &selected_permissions).await;
        if selected_permissions.iter().any(|selected| {
            !permission_views
                .iter()
                .any(|permission| permission.id == selected && permission.available)
        }) {
            render_bridge_authorize_form(
                res,
                &validated,
                &query,
                Some("connected Runner capability changed or the selected Computer permission is no longer available for this OAuth request"),
                registry.as_deref(),
                &selected_permissions,
            )
            .await;
            return;
        }
    }
    issue_bridge_authorization_code(
        res,
        &config,
        &db,
        &validated,
        shared_key_hash,
        granted_scopes,
    );
}
