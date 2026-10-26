use super::RouteAuth::AuthMiddleware;
use super::{route, AuditClass::*, RouteId::*, RouteMethod::*, RouteSpec, RouteSurface::*};
use webcodex_core::authority::{
    OAuthBodyAwarePolicy, OAuthRouteScopePolicy::*, SCOPE_PROJECT_WRITE, SCOPE_RUNNER_MANAGE,
    SCOPE_RUNTIME_READ,
};

pub(super) const ROUTES: &[RouteSpec] = &[
    route(
        ToolsList,
        Post,
        "/api/tools/list",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        Other,
        AuthMiddleware,
    ),
    route(
        ToolsCall,
        Post,
        "/api/tools/call",
        BodyAware(OAuthBodyAwarePolicy::RuntimeToolCall),
        RuntimeApi,
        Command,
        AuthMiddleware,
    ),
    route(
        ProjectsResolveOrRegister,
        Post,
        "/api/projects/resolve-or-register",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        Other,
        AuthMiddleware,
    ),
    route(
        RuntimeStatus,
        Post,
        "/api/runtime/status",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        Report,
        AuthMiddleware,
    ),
    route(
        RuntimeUpgradeMaintenance,
        Post,
        "/api/runtime/upgrade-maintenance",
        Require(SCOPE_RUNNER_MANAGE),
        RuntimeApi,
        Other,
        AuthMiddleware,
    ),
];
