//! Canonical declarative metadata for mounted HTTP routes.
//!
//! Handler mounting stays explicit in the owning HTTP modules. This registry owns
//! the security- and surface-relevant metadata that previously drifted across
//! auth, OpenAPI, console, audit, and test-only route tables.

use crate::auth::scopes::{
    OAuthBodyAwarePolicy, OAuthRouteScopePolicy, SCOPE_ACCOUNT_MANAGE, SCOPE_JOB_RUN,
    SCOPE_PROJECT_READ, SCOPE_PROJECT_WRITE, SCOPE_RUNTIME_READ, SCOPE_SESSION_COLLABORATE,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RouteMethod {
    Get,
    Post,
}

impl RouteMethod {
    pub(crate) fn matches(self, method: &str) -> bool {
        match self {
            Self::Get => method.trim().eq_ignore_ascii_case("GET"),
            Self::Post => method.trim().eq_ignore_ascii_case("POST"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RouteAuth {
    /// No bearer AuthMiddleware. The endpoint is intentionally public at the
    /// HTTP route layer (it may still validate protocol credentials in-body).
    Public,
    /// No bearer AuthMiddleware because the handler owns its current identity
    /// or short-lived authorization contract.
    HandlerManaged,
    /// Mounted behind the shared bearer AuthMiddleware.
    AuthMiddleware,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RouteSurface {
    OAuth,
    Mcp,
    RuntimeApi,
    Connector,
    HostConsole,
    RuntimeConsole,
    Admin,
    Audit,
    AccountManagement,
    AccountControl,
    Pairing,
    AgentTransport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OpenApiVisibility {
    /// The normal server `/openapi.json` GPT Actions surface.
    PublicActions,
    /// The project-hosted Connector OpenAPI surface.
    ConnectorActions,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AuditClass {
    Other,
    Edit,
    Context,
    Job,
    Command,
    Report,
    Artifact,
    Git,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RouteId {
    WellKnownProtectedResource,
    WellKnownAuthorizationServer,
    OAuthToken,
    OAuthRevoke,
    OAuthAuthorize,
    OAuthAuthorizeLogin,
    OAuthAuthorizeConsent,
    OAuthAuthorizeBridge,
    OAuthAuthorizeProject,
    PairingEnroll,
    McpGet,
    McpPost,
    ConnectorReadiness,
    ConnectorTaskStart,
    ConnectorTaskList,
    ConnectorTaskResume,
    ConnectorFilesList,
    ConnectorFilesRead,
    ConnectorFilesSearch,
    ConnectorCodeNavigate,
    ConnectorCodeImpact,
    ConnectorEditsApply,
    ConnectorChecksRun,
    ConnectorCommandsRun,
    ConnectorTaskReview,
    ConnectorTaskCancel,
    ConnectorTaskFinish,
    HostConsoleReadiness,
    HostConsoleTasks,
    HostConsoleActivity,
    HostConsoleWorkflowSessions,
    HostConsoleWorkflowSession,
    HostConsoleTaskReview,
    HostConsoleTaskCancel,
    HostConsoleTaskGuide,
    HostConsoleApprovals,
    HostConsoleApprovalDecide,
    HostConsoleDevices,
    HostConsoleResultAccept,
    HostConsoleResultReject,
    HostConsoleConnect,
    RuntimeConsoleOverview,
    RuntimeConsoleRunner,
    RuntimeConsoleProjects,
    RuntimeConsoleWorkflowSessions,
    RuntimeConsoleWorkflowSession,
    RuntimeConsoleWorkflowSessionMessages,
    RuntimeConsoleWorkflowSessionObserve,
    RuntimeConsoleWorkflowSessionPostMessage,
    RuntimeConsoleWorkflowSessionWithdrawMessage,
    RuntimeConsoleWorkflowSessionReplaceMessage,
    AdminDashboard,
    AdminProjectsRegister,
    AdminProjectsCreate,
    AdminProjectsEnable,
    AdminProjectsDisable,
    AdminProjectsUnregister,
    ToolsList,
    ToolsCall,
    ArtifactsImport,
    JobsStatus,
    JobsLog,
    JobsStop,
    JobsList,
    JobsTail,
    ProjectsList,
    ProjectsRegister,
    ProjectsCreate,
    ProjectsUnregister,
    ProjectsReadFile,
    ProjectsGitStatus,
    ProjectsGitDiff,
    ProjectsGitDiffSummary,
    ProjectsListFiles,
    ProjectsSearchText,
    ProjectsApplyPatch,
    ProjectsValidatePatch,
    ProjectsRunShell,
    ProjectsApplyPatchChecked,
    ProjectsDeleteFiles,
    ProjectsGitRestorePaths,
    ProjectsDiscardUntracked,
    ProjectsRunJob,
    RuntimeStatus,
    OAuthClientsCreate,
    OAuthClientsList,
    OAuthClientsUpdateScopes,
    OAuthClientsRevoke,
    OAuthSharedKeyClientProvision,
    UsersCreate,
    UsersList,
    UsersMe,
    TokensCreate,
    TokensRegisterHash,
    TokensList,
    TokensRevoke,
    AgentTokensCreate,
    AgentTokensRegisterHash,
    AgentTokensList,
    AgentTokensRevoke,
    PairingCreate,
    ShellRun,
    ShellFile,
    ShellJob,
    ShellJobsStatus,
    ShellJobsLog,
    ShellJobsStop,
    ShellJobsList,
    ShellAgentRegister,
    ShellAgentPoll,
    ShellAgentResult,
    ShellAgentPersistentShellResult,
    ShellAgentJobUpdate,
    AgentsWs,
    AuditSessions,
    AuditSession,
    AuditStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RouteSpec {
    pub(crate) id: RouteId,
    pub(crate) method: RouteMethod,
    pub(crate) path: &'static str,
    pub(crate) scope_policy: OAuthRouteScopePolicy,
    pub(crate) surface: RouteSurface,
    pub(crate) openapi_visibility: OpenApiVisibility,
    pub(crate) audit_class: AuditClass,
    pub(crate) auth: RouteAuth,
    /// Preserve the exact legacy PAT compatibility exception for account:manage
    /// routes without maintaining a second method/path allowlist.
    pub(crate) pat_account_manage_compat: bool,
}

const fn route(
    id: RouteId,
    method: RouteMethod,
    path: &'static str,
    scope_policy: OAuthRouteScopePolicy,
    surface: RouteSurface,
    openapi_visibility: OpenApiVisibility,
    audit_class: AuditClass,
    auth: RouteAuth,
    pat_account_manage_compat: bool,
) -> RouteSpec {
    RouteSpec {
        id,
        method,
        path,
        scope_policy,
        surface,
        openapi_visibility,
        audit_class,
        auth,
        pat_account_manage_compat,
    }
}

use AuditClass::*;
use OAuthRouteScopePolicy::*;
use OpenApiVisibility::*;
use RouteAuth::{AuthMiddleware, HandlerManaged};
use RouteId::*;
use RouteMethod::*;
use RouteSurface::*;

pub(crate) const ROUTES: &[RouteSpec] = &[
    route(
        WellKnownProtectedResource,
        Get,
        "/.well-known/oauth-protected-resource",
        Public,
        OAuth,
        Hidden,
        Other,
        RouteAuth::Public,
        false,
    ),
    route(
        WellKnownAuthorizationServer,
        Get,
        "/.well-known/oauth-authorization-server",
        Public,
        OAuth,
        Hidden,
        Other,
        RouteAuth::Public,
        false,
    ),
    route(
        OAuthToken,
        Post,
        "/oauth/token",
        Public,
        OAuth,
        Hidden,
        Other,
        RouteAuth::Public,
        false,
    ),
    route(
        OAuthRevoke,
        Post,
        "/oauth/revoke",
        Public,
        OAuth,
        Hidden,
        Other,
        RouteAuth::Public,
        false,
    ),
    route(
        OAuthAuthorize,
        Get,
        "/oauth/authorize",
        FirstPartyOnly,
        OAuth,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        OAuthAuthorizeLogin,
        Post,
        "/oauth/authorize/login",
        Public,
        OAuth,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        OAuthAuthorizeConsent,
        Post,
        "/oauth/authorize/consent",
        Public,
        OAuth,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        OAuthAuthorizeBridge,
        Post,
        "/oauth/authorize/bridge",
        Public,
        OAuth,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        OAuthAuthorizeProject,
        Post,
        "/oauth/authorize/project",
        Public,
        OAuth,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        PairingEnroll,
        Post,
        "/api/pairing/enroll",
        AgentSurface,
        Pairing,
        Hidden,
        Other,
        HandlerManaged,
        false,
    ),
    route(
        McpGet,
        Get,
        "/mcp",
        Require(SCOPE_RUNTIME_READ),
        Mcp,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        McpPost,
        Post,
        "/mcp",
        BodyAware(OAuthBodyAwarePolicy::McpToolCall),
        Mcp,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    // Project Connector. Four recovery/discovery routes historically had no
    // ordinary delegated scope entry; BootstrapOnly makes that existing
    // fail-closed behavior explicit while project-credential handling remains
    // owned by the specialized Connector gate.
    route(
        ConnectorReadiness,
        Post,
        "/api/connector/readiness",
        BootstrapOnly,
        Connector,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskStart,
        Post,
        "/api/connector/task/start",
        Require(SCOPE_RUNTIME_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskList,
        Post,
        "/api/connector/task/list",
        BootstrapOnly,
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskResume,
        Post,
        "/api/connector/task/resume",
        BootstrapOnly,
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorFilesList,
        Post,
        "/api/connector/files/list",
        BootstrapOnly,
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorFilesRead,
        Post,
        "/api/connector/files/read",
        Require(SCOPE_PROJECT_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorFilesSearch,
        Post,
        "/api/connector/files/search",
        Require(SCOPE_PROJECT_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorCodeNavigate,
        Post,
        "/api/connector/code/navigate",
        Require(SCOPE_PROJECT_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorCodeImpact,
        Post,
        "/api/connector/code/impact",
        Require(SCOPE_PROJECT_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorEditsApply,
        Post,
        "/api/connector/edits/apply",
        Require(SCOPE_PROJECT_WRITE),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorChecksRun,
        Post,
        "/api/connector/checks/run",
        Require(SCOPE_JOB_RUN),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorCommandsRun,
        Post,
        "/api/connector/commands/run",
        Require(SCOPE_JOB_RUN),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskReview,
        Post,
        "/api/connector/task/review",
        Require(SCOPE_PROJECT_READ),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskCancel,
        Post,
        "/api/connector/task/cancel",
        Require(SCOPE_JOB_RUN),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ConnectorTaskFinish,
        Post,
        "/api/connector/task/finish",
        Require(SCOPE_PROJECT_WRITE),
        Connector,
        ConnectorActions,
        Other,
        AuthMiddleware,
        false,
    ),
    // Browser host console. These routes were deliberately reachable through
    // the specialized project credential bypass and bootstrap, but had no
    // ordinary route-scope entry. Preserve that exact production authority.
    route(
        HostConsoleReadiness,
        Post,
        "/api/console/readiness",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleTasks,
        Post,
        "/api/console/tasks",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleActivity,
        Post,
        "/api/console/activity",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleWorkflowSessions,
        Post,
        "/api/console/workflow-sessions",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleWorkflowSession,
        Post,
        "/api/console/workflow-session",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleTaskReview,
        Post,
        "/api/console/task/review",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleTaskCancel,
        Post,
        "/api/console/task/cancel",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleTaskGuide,
        Post,
        "/api/console/task/guide",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleApprovals,
        Post,
        "/api/console/approvals",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleApprovalDecide,
        Post,
        "/api/console/approval/decide",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleDevices,
        Post,
        "/api/console/devices",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleResultAccept,
        Post,
        "/api/console/result/accept",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleResultReject,
        Post,
        "/api/console/result/reject",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        HostConsoleConnect,
        Post,
        "/api/console/connect",
        BootstrapOnly,
        HostConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleOverview,
        Post,
        "/api/runtime-console/overview",
        Require(SCOPE_RUNTIME_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleRunner,
        Post,
        "/api/runtime-console/runner",
        Require(SCOPE_RUNTIME_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleProjects,
        Post,
        "/api/runtime-console/projects",
        Require(SCOPE_PROJECT_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessions,
        Post,
        "/api/runtime-console/workflow-sessions",
        Require(SCOPE_PROJECT_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSession,
        Post,
        "/api/runtime-console/workflow-session",
        Require(SCOPE_PROJECT_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessionMessages,
        Post,
        "/api/runtime-console/workflow-session-messages",
        Require(SCOPE_RUNTIME_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessionObserve,
        Post,
        "/api/runtime-console/workflow-session-observe",
        Require(SCOPE_RUNTIME_READ),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessionPostMessage,
        Post,
        "/api/runtime-console/workflow-session-post-message",
        Require(SCOPE_SESSION_COLLABORATE),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessionWithdrawMessage,
        Post,
        "/api/runtime-console/workflow-session-withdraw-message",
        Require(SCOPE_SESSION_COLLABORATE),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeConsoleWorkflowSessionReplaceMessage,
        Post,
        "/api/runtime-console/workflow-session-replace-message",
        Require(SCOPE_SESSION_COLLABORATE),
        RuntimeConsole,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    // Admin handlers impose their own admin identity check. Production route
    // scope previously admitted only bootstrap because these paths were unknown;
    // keep that behavior explicit rather than widening authority in this cleanup.
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
    route(
        ToolsList,
        Post,
        "/api/tools/list",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ToolsCall,
        Post,
        "/api/tools/call",
        BodyAware(OAuthBodyAwarePolicy::RuntimeToolCall),
        RuntimeApi,
        PublicActions,
        Command,
        AuthMiddleware,
        false,
    ),
    route(
        ArtifactsImport,
        Post,
        "/api/artifacts/import",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Artifact,
        AuthMiddleware,
        false,
    ),
    route(
        JobsStatus,
        Post,
        "/api/jobs/status",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Job,
        AuthMiddleware,
        false,
    ),
    route(
        JobsLog,
        Post,
        "/api/jobs/log",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Job,
        AuthMiddleware,
        false,
    ),
    route(
        JobsStop,
        Post,
        "/api/jobs/stop",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        JobsList,
        Post,
        "/api/jobs/list",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        JobsTail,
        Post,
        "/api/jobs/tail",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Job,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsList,
        Post,
        "/api/projects/list",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Context,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsRegister,
        Post,
        "/api/projects/register",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsCreate,
        Post,
        "/api/projects/create",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsUnregister,
        Post,
        "/api/projects/unregister",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsReadFile,
        Post,
        "/api/projects/read_file",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Context,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsGitStatus,
        Post,
        "/api/projects/git_status",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Git,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsGitDiff,
        Post,
        "/api/projects/git_diff",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Git,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsGitDiffSummary,
        Post,
        "/api/projects/git_diff_summary",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Git,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsListFiles,
        Post,
        "/api/projects/list_files",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Context,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsSearchText,
        Post,
        "/api/projects/search_text",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Context,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsApplyPatch,
        Post,
        "/api/projects/apply_patch",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Edit,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsValidatePatch,
        Post,
        "/api/projects/validate_patch",
        Require(SCOPE_PROJECT_READ),
        RuntimeApi,
        PublicActions,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsRunShell,
        Post,
        "/api/projects/run_shell",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        PublicActions,
        Shell,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsApplyPatchChecked,
        Post,
        "/api/projects/apply_patch_checked",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Edit,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsDeleteFiles,
        Post,
        "/api/projects/delete_files",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Artifact,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsGitRestorePaths,
        Post,
        "/api/projects/git_restore_paths",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Artifact,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsDiscardUntracked,
        Post,
        "/api/projects/discard_untracked",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        PublicActions,
        Artifact,
        AuthMiddleware,
        false,
    ),
    route(
        ProjectsRunJob,
        Post,
        "/api/projects/run_job",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        PublicActions,
        Job,
        AuthMiddleware,
        false,
    ),
    route(
        RuntimeStatus,
        Post,
        "/api/runtime/status",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        PublicActions,
        Report,
        AuthMiddleware,
        false,
    ),
    route(
        OAuthClientsCreate,
        Post,
        "/api/oauth/clients/create",
        FirstPartyOnly,
        OAuth,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        OAuthClientsList,
        Post,
        "/api/oauth/clients/list",
        FirstPartyOnly,
        OAuth,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        OAuthClientsUpdateScopes,
        Post,
        "/api/oauth/clients/update_scopes",
        FirstPartyOnly,
        OAuth,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        OAuthClientsRevoke,
        Post,
        "/api/oauth/clients/revoke",
        FirstPartyOnly,
        OAuth,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        OAuthSharedKeyClientProvision,
        Post,
        "/api/oauth/shared-key-client/provision",
        Require(SCOPE_RUNTIME_READ),
        OAuth,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        UsersCreate,
        Post,
        "/api/users/create",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        UsersList,
        Post,
        "/api/users/list",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        UsersMe,
        Post,
        "/api/users/me",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountControl,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        TokensCreate,
        Post,
        "/api/tokens/create",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        TokensRegisterHash,
        Post,
        "/api/tokens/register_hash",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountControl,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        TokensList,
        Post,
        "/api/tokens/list",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountControl,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        TokensRevoke,
        Post,
        "/api/tokens/revoke",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountControl,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        AgentTokensCreate,
        Post,
        "/api/agent-tokens/create",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        AgentTokensRegisterHash,
        Post,
        "/api/agent-tokens/register_hash",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountControl,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        AgentTokensList,
        Post,
        "/api/agent-tokens/list",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        AgentTokensRevoke,
        Post,
        "/api/agent-tokens/revoke",
        Require(SCOPE_ACCOUNT_MANAGE),
        AccountManagement,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        PairingCreate,
        Post,
        "/api/pairing/create",
        Require(SCOPE_ACCOUNT_MANAGE),
        Pairing,
        Hidden,
        Other,
        AuthMiddleware,
        true,
    ),
    route(
        ShellRun,
        Post,
        "/api/shell/run",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        Hidden,
        Shell,
        AuthMiddleware,
        false,
    ),
    route(
        ShellFile,
        Post,
        "/api/shell/file",
        Require(SCOPE_PROJECT_WRITE),
        RuntimeApi,
        Hidden,
        Shell,
        AuthMiddleware,
        false,
    ),
    route(
        ShellJob,
        Post,
        "/api/shell/job",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        Hidden,
        Shell,
        AuthMiddleware,
        false,
    ),
    route(
        ShellJobsStatus,
        Post,
        "/api/shell/jobs/status",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellJobsLog,
        Post,
        "/api/shell/jobs/log",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellJobsStop,
        Post,
        "/api/shell/jobs/stop",
        Require(SCOPE_JOB_RUN),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellJobsList,
        Post,
        "/api/shell/jobs/list",
        Require(SCOPE_RUNTIME_READ),
        RuntimeApi,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellAgentRegister,
        Post,
        "/api/shell/agent/register",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellAgentPoll,
        Post,
        "/api/shell/agent/poll",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellAgentResult,
        Post,
        "/api/shell/agent/result",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellAgentPersistentShellResult,
        Post,
        "/api/shell/agent/persistent_shell_result",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        ShellAgentJobUpdate,
        Post,
        "/api/shell/agent/job_update",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
    route(
        AgentsWs,
        Get,
        "/api/agents/ws",
        AgentSurface,
        AgentTransport,
        Hidden,
        Other,
        AuthMiddleware,
        false,
    ),
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

#[allow(dead_code)]
pub(crate) fn routes() -> &'static [RouteSpec] {
    ROUTES
}

pub(crate) fn spec(id: RouteId) -> &'static RouteSpec {
    ROUTES
        .iter()
        .find(|spec| spec.id == id)
        .unwrap_or_else(|| panic!("RouteId {id:?} has no canonical RouteSpec"))
}

pub(crate) fn path(id: RouteId) -> &'static str {
    spec(id).path
}

/// Path relative to the production `/api` parent router.
pub(crate) fn api_path(id: RouteId) -> &'static str {
    spec(id)
        .path
        .strip_prefix("/api/")
        .unwrap_or_else(|| panic!("RouteId {id:?} is not an /api route"))
}

/// Path relative to the root Salvo router.
pub(crate) fn root_path(id: RouteId) -> &'static str {
    spec(id).path.trim_start_matches('/')
}

/// Return one root-relative path after proving two method-specific specs share
/// the same canonical path (used for one Salvo Router with GET + POST handlers).
pub(crate) fn shared_root_path(first: RouteId, second: RouteId) -> &'static str {
    let first_path = spec(first).path;
    assert_eq!(first_path, spec(second).path, "shared route path mismatch");
    first_path.trim_start_matches('/')
}

pub(crate) fn lookup(method: &str, path: &str) -> Option<&'static RouteSpec> {
    let path = normalize_path(path);
    ROUTES
        .iter()
        .find(|spec| spec.method.matches(method) && spec.path == path)
}

pub(crate) fn lookup_path(path: &str) -> Option<&'static RouteSpec> {
    let path = normalize_path(path);
    ROUTES.iter().find(|spec| spec.path == path)
}

pub(crate) fn path_has_surface(path: &str, surface: RouteSurface) -> bool {
    let path = normalize_path(path);
    ROUTES
        .iter()
        .any(|spec| spec.path == path && spec.surface == surface)
}

pub(crate) fn audit_class_for_path(path: &str) -> Option<AuditClass> {
    lookup_path(path).map(|spec| spec.audit_class)
}

fn normalize_path(path: &str) -> String {
    let path = path.trim();
    let path = path.split('?').next().unwrap_or(path);
    let path = if path.is_empty() { "/" } else { path };
    let with_slash = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    if with_slash.len() > 1 {
        with_slash.trim_end_matches('/').to_string()
    } else {
        with_slash
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn production_prefix(source: &'static str) -> &'static str {
        source.split("#[cfg(test)]").next().unwrap_or(source)
    }

    fn mounted_route_sources() -> [&'static str; 5] {
        [
            include_str!("lib.rs"),
            production_prefix(include_str!("connector_runtime/http.rs")),
            production_prefix(include_str!("host_console_http.rs")),
            production_prefix(include_str!("runtime_console_http.rs")),
            production_prefix(include_str!("admin_http.rs")),
        ]
    }

    fn exact_route_id_reference_count(source: &str, needle: &str) -> usize {
        source
            .match_indices(needle)
            .filter(|(index, _)| {
                source[*index + needle.len()..]
                    .chars()
                    .next()
                    .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
            })
            .count()
    }

    #[test]
    fn canonical_route_metadata_has_unique_method_path_pairs() {
        let mut seen = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for spec in routes() {
            assert!(spec.path.starts_with('/'), "{:?}: {}", spec.id, spec.path);
            assert!(
                ids.insert(spec.id as usize),
                "duplicate RouteId: {:?}",
                spec.id
            );
            assert!(
                seen.insert((spec.method as u8, spec.path)),
                "duplicate canonical route metadata for {:?} {}",
                spec.method,
                spec.path
            );
        }
        assert_eq!(lookup("GET", "/mcp").unwrap().id, McpGet);
        assert_eq!(lookup("POST", "/mcp").unwrap().id, McpPost);
    }

    #[test]
    fn canonical_lookup_normalizes_only_benign_request_path_variants() {
        assert_eq!(
            lookup(" post ", "api/runtime/status/?ignored=1")
                .unwrap()
                .id,
            RuntimeStatus
        );
        assert!(lookup("GET", "/api/runtime/status").is_none());
        assert!(lookup("POST", "/api/future/authenticated-route").is_none());
        assert!(lookup("POST", "/api/runtime/status/extra").is_none());
    }

    #[test]
    fn authenticated_mounts_reference_each_canonical_spec_exactly_once() {
        let sources = mounted_route_sources();
        let combined = sources.concat();
        for spec in routes().iter().filter(|spec| spec.auth == AuthMiddleware) {
            let needle = format!("RouteId::{:?}", spec.id);
            assert_eq!(
                exact_route_id_reference_count(&combined, &needle),
                1,
                "authenticated mount must reference {needle} exactly once"
            );
        }
    }

    #[test]
    fn authenticated_mount_blocks_do_not_reintroduce_literal_route_paths() {
        for (name, source) in [
            (
                "connector",
                production_prefix(include_str!("connector_runtime/http.rs")),
            ),
            (
                "host-console",
                production_prefix(include_str!("host_console_http.rs")),
            ),
            (
                "runtime-console",
                production_prefix(include_str!("runtime_console_http.rs")),
            ),
            ("admin", production_prefix(include_str!("admin_http.rs"))),
        ] {
            assert!(
                !source.contains("Router::with_path(\""),
                "{name} authenticated mount reintroduced a literal route path"
            );
        }

        let lib = include_str!("lib.rs");
        assert_eq!(
            lib.matches(".hoop(AuthMiddleware)").count(),
            3,
            "a new AuthMiddleware root must be covered by the canonical route inventory invariant"
        );
        let authed = lib
            .split_once("let authed_api_router")
            .unwrap()
            .1
            .split_once("let api_router")
            .unwrap()
            .0;
        assert!(
            !authed.contains("Router::with_path(\""),
            "main authenticated /api mount reintroduced a literal route path"
        );
        let oauth_mcp = lib
            .split_once("// OAuth2 token, revocation, and discovery endpoints")
            .unwrap()
            .1
            .split_once("// Read-only audit query API")
            .unwrap()
            .0;
        assert!(
            !oauth_mcp.contains("Router::with_path(\""),
            "OAuth/MCP mounts must use canonical RouteId-backed paths"
        );
        let audit = lib
            .split_once("// Read-only audit query API")
            .unwrap()
            .1
            .split_once("tracing::info!(\"Server started successfully!\")")
            .unwrap()
            .0;
        assert!(
            !audit.contains("Router::with_path(\""),
            "audit authenticated mounts must use canonical RouteId-backed paths"
        );
    }

    #[test]
    fn runtime_console_mutations_are_never_runtime_read_routes() {
        let mutations = [
            RuntimeConsoleWorkflowSessionPostMessage,
            RuntimeConsoleWorkflowSessionWithdrawMessage,
            RuntimeConsoleWorkflowSessionReplaceMessage,
        ];
        for id in mutations {
            assert_eq!(
                spec(id).scope_policy,
                Require(SCOPE_SESSION_COLLABORATE),
                "{id:?} must retain mutation-capable Session authority"
            );
        }
        for spec in routes()
            .iter()
            .filter(|spec| spec.surface == RuntimeConsole)
        {
            assert_eq!(spec.openapi_visibility, Hidden, "{:?}", spec.id);
        }
    }

    #[test]
    fn audit_class_is_unambiguous_per_canonical_path() {
        let mut classes = BTreeMap::new();
        for spec in routes() {
            match classes.insert(spec.path, spec.audit_class) {
                Some(existing) => assert_eq!(existing, spec.audit_class, "{}", spec.path),
                None => {}
            }
        }
    }

    #[test]
    fn audit_class_preserves_existing_http_stats_semantics() {
        for (path, class) in [
            ("/api/projects/apply_patch", Edit),
            ("/api/projects/read_file", Context),
            ("/api/projects/run_job", Job),
            ("/api/tools/call", Command),
            ("/api/runtime/status", Report),
            ("/api/artifacts/import", Artifact),
            ("/api/projects/git_diff", Git),
            ("/api/projects/run_shell", Shell),
        ] {
            assert_eq!(audit_class_for_path(path), Some(class), "{path}");
        }
        assert_eq!(
            audit_class_for_path("/api/connector/edits/apply"),
            Some(Other)
        );
        assert_eq!(
            audit_class_for_path("/api/runtime-console/projects"),
            Some(Other)
        );
    }
}
