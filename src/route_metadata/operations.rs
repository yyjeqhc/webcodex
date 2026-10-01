use super::RouteAuth::AuthMiddleware;
use super::{
    route, AuditClass::*, RouteAuth, RouteId::*, RouteMethod::*, RouteSpec, RouteSurface::*,
};
use webcodex_core::authority::{OAuthRouteScopePolicy::*, SCOPE_ACCOUNT_MANAGE};

pub(super) const PUBLIC_WEB_ROUTES: &[RouteSpec] = &[route(
    Healthz,
    Get,
    "/healthz",
    Public,
    PublicWeb,
    Other,
    RouteAuth::Public,
)];
// Admin handlers impose their own admin identity check. Production route
// scope previously admitted only bootstrap because these paths were unknown;
// keep that behavior explicit rather than widening authority in this cleanup.
pub(super) const ADMIN_ROUTES: &[RouteSpec] = &[
    route(
        AdminDashboard,
        Post,
        "/api/admin/dashboard",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
    route(
        AdminProjectsRegister,
        Post,
        "/api/admin/projects/register",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
    route(
        AdminProjectsCreate,
        Post,
        "/api/admin/projects/create",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
    route(
        AdminProjectsEnable,
        Post,
        "/api/admin/projects/enable",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
    route(
        AdminProjectsDisable,
        Post,
        "/api/admin/projects/disable",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
    route(
        AdminProjectsUnregister,
        Post,
        "/api/admin/projects/unregister",
        BootstrapOnly,
        Admin,
        Other,
        AuthMiddleware,
    ),
];

pub(super) const AUDIT_ROUTES: &[RouteSpec] = &[
    route(
        AuditSessions,
        Post,
        "/api/audit/sessions",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Other,
        AuthMiddleware,
    ),
    route(
        AuditSession,
        Post,
        "/api/audit/session",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Other,
        AuthMiddleware,
    ),
    route(
        AuditStats,
        Post,
        "/api/audit/stats",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Other,
        AuthMiddleware,
    ),
];
