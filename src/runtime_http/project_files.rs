use super::{render_result, runtime};
use crate::action_audit::ActionAudit;
use crate::json_error;
use crate::tool_runtime::ToolCall;
use salvo::prelude::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ProjectIdRequest {
    pub project: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadProjectFileRequest {
    pub project: String,
    pub path: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub with_line_numbers: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ProjectGitDiffRequest {
    pub project: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ApplyPatchRequest {
    pub project: String,
    pub patch: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// `POST /api/projects/validate_patch` — dedicated read-only GPT Action
/// wrapper over `ToolCall::ValidatePatch`. Patch preflight / dry-run only;
/// accepts only raw standard unified diff, runs `git apply --check`/`--stat`
/// through the owning agent, never modifies the worktree, and never falls back
/// to a real apply.
#[derive(Debug, Deserialize)]
struct ValidatePatchRequest {
    pub project: String,
    pub patch: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub deny_sensitive_paths: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ApplyPatchCheckedRequest {
    pub project: String,
    pub patch: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub deny_sensitive_paths: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteProjectFilesRequest {
    pub project: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitRestorePathsRequest {
    pub project: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiscardUntrackedRequest {
    pub project: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListProjectFilesRequest {
    pub project: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SearchProjectTextRequest {
    pub project: String,
    pub pattern: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub context_before: Option<usize>,
    #[serde(default)]
    pub context_after: Option<usize>,
    #[serde(default)]
    pub include_globs: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_globs: Option<Vec<String>>,
    #[serde(default)]
    pub result_mode: Option<crate::tool_runtime::SearchResultMode>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
}

#[handler]
pub async fn projects_read_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/read_file", "readProjectFile");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ReadProjectFileRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ReadFile {
                project: body.project,
                path: body.path,
                session_id: body.session_id,
                start_line: body.start_line,
                limit: body.limit,
                with_line_numbers: body.with_line_numbers,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "read_file", project, result);
}

#[handler]
pub async fn projects_git_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/git_status",
        "getProjectGitStatus",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectIdRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitStatus {
                project: body.project,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_status", project, result);
}

/// `POST /api/projects/git_diff` — thin GPT Actions wrapper over
/// `ToolCall::GitDiff`. Read-only inspection routed to the owning agent.
#[handler]
pub async fn projects_git_diff(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/git_diff", "getProjectGitDiff");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectGitDiffRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitDiff {
                project: body.project,
                session_id: body.session_id,
                args: body.args,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_diff", project, result);
}

/// `POST /api/projects/apply_patch` — thin GPT Actions wrapper over
/// `ToolCall::ApplyPatch`. Executable mutation; requires the owning agent to
/// allow patching and the caller to pass Bearer auth.
#[handler]
pub async fn projects_apply_patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/apply_patch", "applyProjectPatch");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ApplyPatchRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ApplyPatch {
                project: body.project,
                patch: body.patch,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "apply_patch", project, result);
}

/// `POST /api/projects/validate_patch` — dedicated read-only GPT Action
/// wrapper over `ToolCall::ValidatePatch`. It accepts only raw standard
/// unified diff and performs `git apply --check`/`--stat` through the owning
/// agent via `ToolRuntime`. Never writes files.
#[handler]
pub async fn projects_validate_patch(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/validate_patch",
        "validateProjectPatch",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ValidatePatchRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ValidatePatch {
                project: body.project,
                patch: body.patch,
                session_id: body.session_id,
                deny_sensitive_paths: body.deny_sensitive_paths,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "validate_patch", project, result);
}

/// `POST /api/projects/apply_patch_checked` — thin GPT Actions wrapper over
/// `ToolCall::ApplyPatchChecked`. Mutation with side effects: runs the
/// `validate_patch` preflight first and, only when it passes, applies the
/// patch and returns the post-apply diff summary. Requires Bearer auth and
/// the agent shell capability.
#[handler]
pub async fn projects_apply_patch_checked(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/apply_patch_checked",
        "applyProjectPatchChecked",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ApplyPatchCheckedRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ApplyPatchChecked {
                project: body.project,
                patch: body.patch,
                session_id: body.session_id,
                deny_sensitive_paths: body.deny_sensitive_paths,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "apply_patch_checked", project, result);
}

/// `POST /api/projects/delete_files` — thin GPT Actions wrapper over
/// `ToolCall::DeleteProjectFiles`. Mutation with side effects: deletes the
/// selected project-relative files only (not directories). Requires Bearer
/// auth and the agent shell capability.
#[handler]
pub async fn projects_delete_files(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/delete_files",
        "deleteProjectFiles",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: DeleteProjectFilesRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::DeleteProjectFiles {
                project: body.project,
                paths: body.paths,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "delete_project_files", project, result);
}

/// `POST /api/projects/git_restore_paths` — thin GPT Actions wrapper over
/// `ToolCall::GitRestorePaths`. Mutation with side effects: runs
/// `git restore -- <paths>` on selected tracked project-relative paths.
/// Requires Bearer auth and the agent shell capability.
#[handler]
pub async fn projects_git_restore_paths(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/git_restore_paths",
        "gitRestorePaths",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: GitRestorePathsRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitRestorePaths {
                project: body.project,
                paths: body.paths,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_restore_paths", project, result);
}

/// `POST /api/projects/discard_untracked` — thin GPT Actions wrapper over
/// `ToolCall::DiscardUntracked`. Mutation with side effects: runs
/// `git clean -f -- <paths>` only for selected project-relative untracked
/// paths. Requires Bearer auth and the agent shell capability.
#[handler]
pub async fn projects_discard_untracked(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/discard_untracked",
        "discardUntrackedFiles",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: DiscardUntrackedRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = Some(body.project.clone());
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::DiscardUntracked {
                project: body.project,
                paths: body.paths,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "discard_untracked", project, result);
}

/// `ToolCall::ListProjectFiles`. Read-only, agent-backed file listing.
#[handler]
pub async fn projects_list_files(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/list_files", "listProjectFiles");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ListProjectFilesRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListProjectFiles {
                project: body.project,
                session_id: body.session_id,
                path: body.path,
                limit: body.limit,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "list_project_files", Some(project), result);
}

/// `ToolCall::SearchProjectText`. Read-only, agent-backed bounded text search.
#[handler]
pub async fn projects_search_text(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/search_text", "searchProjectText");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: SearchProjectTextRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::SearchProjectText {
                project: body.project,
                pattern: body.pattern,
                session_id: body.session_id,
                path: body.path,
                limit: body.limit,
                context_before: body.context_before,
                context_after: body.context_after,
                include_globs: body.include_globs,
                exclude_globs: body.exclude_globs,
                result_mode: body.result_mode,
                timeout_secs: body.timeout_secs,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "search_project_text", Some(project), result);
}

/// `ToolCall::GitDiffSummary`. Read-only git inspection.
#[handler]
pub async fn projects_git_diff_summary(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/git_diff_summary",
        "getProjectGitDiffSummary",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ProjectIdRequest = match req.parse_json().await {
        Ok(body) => body,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("Invalid JSON: {}", e),
            ));
            return;
        }
    };
    let project = body.project.clone();
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::GitDiffSummary {
                project: body.project,
                session_id: body.session_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_diff_summary", Some(project), result);
}
