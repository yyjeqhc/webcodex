use super::RouteAuth::AuthMiddleware;
use super::{
    route, AuditClass::*, OpenApiVisibility::*, RouteId::*, RouteMethod::*, RouteSpec,
    RouteSurface::*,
};
use crate::auth::scopes::{OAuthRouteScopePolicy::*, SCOPE_ACCOUNT_MANAGE};

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
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AdminProjectsRegister,
        Post,
        "/api/admin/projects/register",
        BootstrapOnly,
        Admin,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AdminProjectsCreate,
        Post,
        "/api/admin/projects/create",
        BootstrapOnly,
        Admin,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AdminProjectsEnable,
        Post,
        "/api/admin/projects/enable",
        BootstrapOnly,
        Admin,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AdminProjectsDisable,
        Post,
        "/api/admin/projects/disable",
        BootstrapOnly,
        Admin,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AdminProjectsUnregister,
        Post,
        "/api/admin/projects/unregister",
        BootstrapOnly,
        Admin,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
];

pub(super) const AUDIT_ROUTES: &[RouteSpec] = &[
    route(
        AuditSessions,
        Post,
        "/api/audit/sessions",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AuditSession,
        Post,
        "/api/audit/session",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AuditStats,
        Post,
        "/api/audit/stats",
        Require(SCOPE_ACCOUNT_MANAGE),
        Audit,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
];
