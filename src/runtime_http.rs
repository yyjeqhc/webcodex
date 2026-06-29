use crate::action_audit::{ActionAudit, ActionAuditRecord};
use crate::json_error;
use crate::tool_runtime::{ToolCall, ToolRuntime};
use base64::{engine::general_purpose, Engine as _};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::Map;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

fn enforce_oauth_runtime_tool_scope(
    auth: Option<&crate::auth::AuthContext>,
    tool_name: &str,
    res: &mut Response,
) -> bool {
    let Some(auth) = auth else {
        return true;
    };
    if !auth.is_oauth_token() {
        return true;
    }

    match crate::auth::scopes::oauth_scope_policy_for_runtime_tool(tool_name) {
        crate::auth::scopes::OAuthToolScopePolicy::Require(scope) => {
            if auth.has_scope(scope) {
                true
            } else {
                crate::auth::render_oauth_insufficient_scope(
                    res,
                    Some(scope),
                    format!("missing required scope: {}", scope),
                );
                false
            }
        }
        crate::auth::scopes::OAuthToolScopePolicy::FirstPartyOnly => {
            crate::auth::render_oauth_insufficient_scope(
                res,
                None,
                "OAuth2 access tokens cannot call first-party-only tools",
            );
            false
        }
        crate::auth::scopes::OAuthToolScopePolicy::Unknown => {
            crate::auth::render_oauth_insufficient_scope(
                res,
                None,
                "OAuth2 access tokens cannot call unknown runtime tools",
            );
            false
        }
    }
}

/// Generic runtime tool call body. `tool` is required; `params` carries the
/// tool-specific arguments. `arguments` is accepted as a compatibility alias
/// for `params` — when both are present, `params` wins. GPT Actions may also
/// pass tool-specific arguments as top-level fields, so parsing is done
/// manually in `tools_call` (via `extract_tool_call`) to preserve the
/// params-over-arguments-over-flattened precedence and the rich error messages
/// stay explicit. The OpenAPI `ToolCallRequest` schema documents the same wire
/// shape.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ToolCallRequest {
    pub tool: String,
    #[serde(default)]
    pub params: Value,
    /// Compatibility alias for `params`. Ignored when `params` is present.
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
struct CodexRunRequest {
    pub project: String,
    pub prompt: String,
    #[serde(default)]
    pub approval_mode: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub extra_args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct JobStatusRequest {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
struct JobLogRequest {
    pub job_id: String,
    #[serde(default)]
    pub offset: Option<usize>,
    #[serde(default)]
    pub tail_lines: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct JobStopRequest {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
struct ProjectIdRequest {
    pub project: String,
}

#[derive(Debug, Deserialize)]
struct ReadProjectFileRequest {
    pub project: String,
    pub path: String,
    #[serde(default)]
    pub start_line: Option<usize>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ProjectGitDiffRequest {
    pub project: String,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct ApplyPatchRequest {
    pub project: String,
    pub patch: String,
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
    pub deny_sensitive_paths: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct RunShellRequest {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApplyPatchCheckedRequest {
    pub project: String,
    pub patch: String,
    #[serde(default)]
    pub deny_sensitive_paths: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteProjectFilesRequest {
    pub project: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct GitRestorePathsRequest {
    pub project: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DiscardUntrackedRequest {
    pub project: String,
    pub paths: Vec<String>,
}

/// `POST /api/projects/replace_in_file` — thin REST wrapper over
/// `ToolCall::ReplaceInFile`. Mutation with side effects: replaces a substring
/// in a project file via the owning agent using a fixed helper (old/new travel
/// over stdin, never interpolated into the shell command). Dedicated GPT Action
/// (`replaceProjectFileText`); also reachable via callRuntimeTool / MCP.
#[derive(Debug, Deserialize)]
struct ReplaceInFileRequest {
    pub project: String,
    pub path: String,
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub expected_replacements: Option<i64>,
    #[serde(default)]
    pub allow_multiple: Option<bool>,
}

/// `POST /api/projects/write_file` — thin REST wrapper over
/// `ToolCall::WriteProjectFile`. Mutation with side effects: writes a UTF-8
/// file via the owning agent with optional overwrite guards. Dedicated GPT
/// Action (`writeProjectFile`); also reachable via callRuntimeTool / MCP
/// tools/call.
#[derive(Debug, Deserialize)]
struct WriteProjectFileRequest {
    pub project: String,
    pub path: String,
    pub content: String,
    #[serde(default)]
    pub overwrite: Option<bool>,
    #[serde(default)]
    pub expected_sha256: Option<String>,
    #[serde(default)]
    pub expected_content_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportConversationFilesRequest {
    #[serde(rename = "openaiFileIdRefs")]
    pub openai_file_id_refs: Vec<OpenAiFileIdRef>,
    pub project: String,
    #[serde(default)]
    pub output_dir: Option<String>,
    #[serde(default)]
    pub targets: Option<Vec<String>>,
    #[serde(default)]
    pub overwrite: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFileIdRef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub mime_type: Option<String>,
    pub download_link: String,
}

const MAX_IMPORT_FILES: usize = 10;
const MAX_IMPORT_FILE_BYTES: usize = 10 * 1024 * 1024;

fn sanitize_import_name(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in name.rsplit('/').next().unwrap_or(name).chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('.').trim_matches('_');
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn join_import_path(output_dir: Option<&str>, leaf: &str) -> Result<String, String> {
    let dir = output_dir
        .unwrap_or("artifacts/imports")
        .trim()
        .trim_matches('/');
    let candidate = if dir.is_empty() {
        leaf.to_string()
    } else {
        format!("{}/{}", dir, leaf)
    };
    crate::tool_runtime::files::validate_artifact_file_path(&candidate)?;
    Ok(candidate)
}

fn mime_allowed_for_import(mime: &str, path: &str) -> bool {
    matches!(
        mime,
        "image/png"
            | "image/jpeg"
            | "image/webp"
            | "application/pdf"
            | "application/zip"
            | "text/plain"
            | "text/csv"
            | "application/json"
    ) || (mime == "application/octet-stream"
        && [
            ".png", ".jpg", ".jpeg", ".webp", ".pdf", ".zip", ".txt", ".csv", ".json",
        ]
        .iter()
        .any(|suffix| path.to_lowercase().ends_with(suffix)))
}

#[derive(Debug, Deserialize)]
struct ListProjectFilesRequest {
    pub project: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

fn validate_openai_download_url(download_link: &str) -> Result<reqwest::Url, String> {
    let url =
        reqwest::Url::parse(download_link).map_err(|e| format!("invalid download_link: {}", e))?;
    if url.scheme() != "https" {
        return Err("download_link must use https".to_string());
    }
    let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
        return Err("download_link must include a host".to_string());
    };
    if host != "files.oaiusercontent.com" && !host.ends_with(".oaiusercontent.com") {
        return Err("download_link host is not an OpenAI file host".to_string());
    }
    Ok(url)
}

#[cfg(test)]
static IMPORT_TEST_DOWNLOAD_BASE_URL: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
fn set_import_test_download_base_url(base_url: Option<String>) {
    let slot = IMPORT_TEST_DOWNLOAD_BASE_URL.get_or_init(|| std::sync::Mutex::new(None));
    *slot
        .lock()
        .expect("import test download base mutex poisoned") = base_url;
}

fn request_url_for_download(validated_url: reqwest::Url) -> reqwest::Url {
    #[cfg(test)]
    {
        let base_url = IMPORT_TEST_DOWNLOAD_BASE_URL
            .get_or_init(|| std::sync::Mutex::new(None))
            .lock()
            .expect("import test download base mutex poisoned")
            .clone();
        if let Some(base_url) = base_url {
            let mut rewritten = reqwest::Url::parse(&base_url)
                .expect("test import download base URL must be valid");
            rewritten.set_path(validated_url.path());
            rewritten.set_query(validated_url.query());
            return rewritten;
        }
    }
    validated_url
}

async fn read_bounded_download(
    response: &mut reqwest::Response,
    source_name: &str,
) -> Result<Vec<u8>, String> {
    if let Some(len) = response.content_length() {
        if len > MAX_IMPORT_FILE_BYTES as u64 {
            return Err(format!(
                "download for '{}' exceeds {} bytes",
                source_name, MAX_IMPORT_FILE_BYTES
            ));
        }
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("failed to read download for '{}': {}", source_name, e))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_IMPORT_FILE_BYTES {
            return Err(format!(
                "download for '{}' exceeds {} bytes",
                source_name, MAX_IMPORT_FILE_BYTES
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[derive(Debug, Deserialize)]
struct SearchProjectTextRequest {
    pub project: String,
    pub pattern: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct ListJobsRequest {
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Deserialize)]
struct JobTailRequest {
    pub job_id: String,
    #[serde(default)]
    pub tail_lines: Option<usize>,
}

/// `POST /api/projects/register` — thin REST wrapper over
/// `ToolCall::RegisterProject`. Mutation with side effects; registers an
/// existing directory as a WebCodex project on the selected agent. Dedicated
/// GPT Action (`registerProject`); also reachable via callRuntimeTool / MCP
/// tools/call.
#[derive(Debug, Deserialize)]
struct RegisterProjectRequest {
    pub client_id: String,
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "crate::tool_runtime::default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub overwrite: bool,
}

/// `POST /api/projects/create` — thin REST wrapper over
/// `ToolCall::CreateProject`. Mutation with side effects; creates a new
/// directory on the selected agent and registers it as a WebCodex project.
/// Dedicated GPT Action (`createProject`); also reachable via callRuntimeTool
/// / MCP tools/call.
#[derive(Debug, Deserialize)]
struct CreateProjectRequest {
    pub client_id: String,
    pub id: String,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "crate::tool_runtime::default_true")]
    pub allow_patch: bool,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub git_init: bool,
    #[serde(default)]
    pub allow_existing_empty: bool,
    #[serde(default)]
    pub overwrite: bool,
}

fn runtime(depot: &Depot) -> Option<Arc<ToolRuntime>> {
    depot.obtain::<Arc<ToolRuntime>>().ok().cloned()
}

fn render_result(
    res: &mut Response,
    audit: &ActionAudit,
    operation: &str,
    project: Option<String>,
    result: crate::tool_runtime::ToolResult,
) {
    let status = if result.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    res.status_code(status);
    let mut event = ActionAuditRecord::new(operation.to_string(), result.success, status)
        .error(result.error.clone())
        .summary(json!({
            "output": result.output.clone(),
        }));
    event.project = project;
    audit.record(event);
    res.render(Json(result));
}

#[handler]
pub async fn tools_list(depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let specs = runtime.tool_specs();
    let names: Vec<String> = specs.iter().map(|s| s.name.clone()).collect();
    let count = specs.len();
    res.render(Json(json!({
        "success": true,
        "tools": specs,
        "names": names,
        "count": count,
        "categories": runtime.tool_categories(),
        "recommended_flows": ToolRuntime::recommended_flows(),
    })));
}

#[handler]
pub async fn tools_call(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/tools/call", "callTool");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Parse the body as a raw JSON value so we can apply the params/arguments
    // precedence rule explicitly and emit field-aware errors that include the
    // tool name. We never echo the raw body back, so tokens/headers/env never
    // leak through error messages.
    let body: Value = match req.parse_json().await {
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
    let (tool, params) = match extract_tool_call(&body) {
        Ok(pair) => pair,
        Err(msg) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, msg));
            return;
        }
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    if !enforce_oauth_runtime_tool_scope(auth.as_ref(), &tool, res) {
        return;
    }
    let call = match ToolCall::from_tool_name(&tool, params) {
        Ok(call) => call,
        Err(e) => {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(StatusCode::BAD_REQUEST, e));
            return;
        }
    };
    let project = tool_project(&call);
    let result = runtime.dispatch_with_auth(call, auth.as_ref()).await;
    render_result(res, &audit, &tool, project, result);
}

/// Extract `(tool, params)` from a raw `callRuntimeTool` request body.
///
/// Accepted shapes (all route to the same tool dispatch):
/// - `{"tool":"list_tools"}`
/// - `{"tool":"list_tools","params":null}`
/// - `{"tool":"git_diff_summary","params":{"project":"agent:c:p"}}`
/// - `{"tool":"git_diff_summary","arguments":{"project":"agent:c:p"}}`
/// - `{"tool":"git_diff_summary","project":"agent:c:p"}`
///
/// When both `params` and `arguments` are present, `params` wins; `arguments`
/// is only a compatibility alias. When neither is present, every top-level
/// field except `tool` is collected into the params object for GPT Action
/// compatibility. Returns a human-readable error string (never including the
/// raw body) when the body is not a JSON object or `tool` is missing/not a
/// non-empty string.
fn extract_tool_call(body: &Value) -> Result<(String, Value), String> {
    let obj = body
        .as_object()
        .ok_or_else(|| "request body must be a JSON object".to_string())?;
    let tool = match obj.get("tool") {
        Some(v) => match v.as_str() {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => {
                return Err("field 'tool' must be a non-empty string".to_string());
            }
        },
        None => {
            return Err("missing required field 'tool'".to_string());
        }
    };
    // params takes precedence over the `arguments` alias; flattened GPT Action
    // fields are collected only when neither object wrapper is present.
    let params = if obj.contains_key("params") {
        obj.get("params").cloned().unwrap_or(Value::Null)
    } else if obj.contains_key("arguments") {
        obj.get("arguments").cloned().unwrap_or(Value::Null)
    } else {
        let mut flattened = serde_json::Map::new();
        for (key, value) in obj {
            if key != "tool" {
                flattened.insert(key.clone(), value.clone());
            }
        }
        if flattened.is_empty() {
            Value::Null
        } else {
            Value::Object(flattened)
        }
    };
    Ok((tool, params))
}

#[handler]
pub async fn import_conversation_files_to_project(
    req: &mut Request,
    depot: &mut Depot,
    res: &mut Response,
) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/artifacts/import",
        "importConversationFilesToProject",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ImportConversationFilesRequest = match req.parse_json().await {
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
    if body.openai_file_id_refs.is_empty() || body.openai_file_id_refs.len() > MAX_IMPORT_FILES {
        res.status_code(StatusCode::BAD_REQUEST);
        res.render(json_error(
            StatusCode::BAD_REQUEST,
            format!(
                "openaiFileIdRefs must contain 1..={} files",
                MAX_IMPORT_FILES
            ),
        ));
        return;
    }
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
            res.render(json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to build HTTP client: {}", e),
            ));
            return;
        }
    };
    let mut imported = Vec::new();
    for (idx, file_ref) in body.openai_file_id_refs.iter().enumerate() {
        let source_name = file_ref
            .name
            .as_deref()
            .or(file_ref.id.as_deref())
            .unwrap_or("artifact");
        let fallback = format!("artifact-{}", idx + 1);
        let leaf = body
            .targets
            .as_ref()
            .and_then(|targets| targets.get(idx))
            .map(|target| sanitize_import_name(target, &fallback))
            .unwrap_or_else(|| sanitize_import_name(source_name, &fallback));
        let path = match join_import_path(body.output_dir.as_deref(), &leaf) {
            Ok(path) => path,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        };
        let mime = file_ref
            .mime_type
            .as_deref()
            .unwrap_or("application/octet-stream");
        if !mime_allowed_for_import(mime, &path) {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!("unsupported MIME type for '{}': {}", source_name, mime),
            ));
            return;
        }
        let url = match validate_openai_download_url(&file_ref.download_link) {
            Ok(url) => url,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        };
        let mut response = match client.get(request_url_for_download(url)).send().await {
            Ok(resp) => resp,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(
                    StatusCode::BAD_REQUEST,
                    format!("failed to download '{}': {}", source_name, e),
                ));
                return;
            }
        };
        if !response.status().is_success() {
            res.status_code(StatusCode::BAD_REQUEST);
            res.render(json_error(
                StatusCode::BAD_REQUEST,
                format!(
                    "download for '{}' returned HTTP {}",
                    source_name,
                    response.status()
                ),
            ));
            return;
        }
        let bytes = match read_bounded_download(&mut response, source_name).await {
            Ok(bytes) => bytes,
            Err(e) => {
                res.status_code(StatusCode::BAD_REQUEST);
                res.render(json_error(StatusCode::BAD_REQUEST, e));
                return;
            }
        };
        let result = runtime
            .dispatch_with_auth(
                ToolCall::SaveProjectArtifact {
                    project: body.project.clone(),
                    path: path.clone(),
                    content_base64: general_purpose::STANDARD.encode(&bytes),
                    mime_type: Some(mime.to_string()),
                    overwrite: body.overwrite,
                },
                auth.as_ref(),
            )
            .await;
        if !result.success {
            render_result(
                res,
                &audit,
                "import_conversation_files",
                Some(body.project.clone()),
                result,
            );
            return;
        }
        let mut obj = Map::new();
        obj.insert(
            "source_name".to_string(),
            Value::String(source_name.to_string()),
        );
        obj.insert("project".to_string(), Value::String(body.project.clone()));
        obj.insert("path".to_string(), Value::String(path));
        obj.insert(
            "bytes_written".to_string(),
            result.output["bytes_written"].clone(),
        );
        obj.insert("mime_type".to_string(), Value::String(mime.to_string()));
        obj.insert("sha256".to_string(), result.output["sha256"].clone());
        imported.push(Value::Object(obj));
    }
    let result =
        crate::tool_runtime::ToolResult::ok(json!({"imported": imported, "count": imported.len()}));
    render_result(
        res,
        &audit,
        "import_conversation_files",
        Some(body.project),
        result,
    );
}

#[handler]
pub async fn codex_run(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/codex/run", "runCodexTask");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: CodexRunRequest = match req.parse_json().await {
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
            ToolCall::RunCodex {
                project: body.project,
                prompt: body.prompt,
                approval_mode: body.approval_mode,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
                extra_args: body.extra_args,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_codex", project, result);
}

#[handler]
pub async fn job_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/status", "jobStatus");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobStatusRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobStatus {
                job_id: body.job_id,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_status", None, result);
}

#[handler]
pub async fn job_log(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/log", "jobLog");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobLogRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobLog {
                job_id: body.job_id,
                offset: body.offset,
                tail_lines: body.tail_lines,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_log", None, result);
}

/// Stop a local runtime job by terminating its process group and marking it
/// `stopped`. This is a thin wrapper over `ToolRuntime::stop_job`; it is
/// intentionally NOT exposed as a GPT Action (absent from openapi.json) so
/// remote ChatGPT callers cannot drive an explicit kill. Only jobs the
/// runtime created and recorded can be stopped.
#[handler]
pub async fn job_stop(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/stop", "jobStop");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobStopRequest = match req.parse_json().await {
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
    let result = runtime.stop_job(body.job_id).await;
    render_result(res, &audit, "job_stop", None, result);
}

#[handler]
pub async fn projects_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/list", "listProjects");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Body is optional; reject non-empty invalid JSON for consistency but
    // tolerate an empty/missing body since this call takes no arguments.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => Value::Null,
    };
    let _ = body;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(ToolCall::ListProjects, auth.as_ref())
        .await;
    render_result(res, &audit, "list_projects", None, result);
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
                start_line: body.start_line,
                limit: body.limit,
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
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "discard_untracked", project, result);
}

/// `POST /api/projects/replace_in_file` — thin REST wrapper over
/// `ToolCall::ReplaceInFile`. Mutation with side effects: replaces a substring
/// in a project file via the owning agent's fixed helper. Requires Bearer auth
/// and the agent shell capability. Dedicated GPT Action
/// (`replaceProjectFileText`); also reachable via callRuntimeTool / MCP
/// tools/call.
#[handler]
pub async fn projects_replace_in_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/replace_in_file", "replaceInFile");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ReplaceInFileRequest = match req.parse_json().await {
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
            ToolCall::ReplaceInFile {
                project: body.project,
                path: body.path,
                old: body.old,
                new: body.new,
                expected_replacements: body.expected_replacements,
                allow_multiple: body.allow_multiple,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "replace_in_file", project, result);
}

/// `POST /api/projects/write_file` — thin REST wrapper over
/// `ToolCall::WriteProjectFile`. Mutation with side effects: writes a UTF-8
/// file via the owning agent with optional overwrite guards. Requires Bearer
/// auth and the agent shell capability. Dedicated GPT Action
/// (`writeProjectFile`); also reachable via callRuntimeTool / MCP tools/call.
#[handler]
pub async fn projects_write_file(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/write_file", "writeProjectFile");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: WriteProjectFileRequest = match req.parse_json().await {
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
            ToolCall::WriteProjectFile {
                project: body.project,
                path: body.path,
                content: body.content,
                overwrite: body.overwrite,
                expected_sha256: body.expected_sha256,
                expected_content_prefix: body.expected_content_prefix,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "write_project_file", project, result);
}

/// `POST /api/projects/run_job` — thin REST wrapper over
/// `ToolCall::RunJob`. Starts an async background shell job in an
/// agent-registered project and returns a `job_id`. Execution with side
/// effects; requires Bearer auth and the agent async shell job capability.
/// Dedicated GPT Action (`startProjectShellJob`); also reachable via
/// callRuntimeTool / MCP tools/call. Poll with `getRuntimeJobStatus` and read
/// output with `getRuntimeJobTail` / `getRuntimeJobLog`.
#[derive(Debug, Deserialize)]
struct StartProjectShellJobRequest {
    pub project: String,
    pub command: String,
    #[serde(default)]
    pub timeout_secs: Option<i64>,
    #[serde(default)]
    pub cwd: Option<String>,
}

/// `POST /api/projects/run_job` handler. Thin wrapper: parse request, auth,
/// audit, and dispatch to `ToolRuntime` via `ToolCall::RunJob`. All business
/// logic (capability checks, owner boundary, job creation) stays in
/// `ToolRuntime`.
#[handler]
pub async fn projects_run_job(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/run_job", "startProjectShellJob");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: StartProjectShellJobRequest = match req.parse_json().await {
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
            ToolCall::RunJob {
                project: body.project,
                command: body.command,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_job", project, result);
}

/// `POST /api/projects/run_shell` — thin GPT Actions wrapper over
/// `ToolCall::RunShell`. Executable with side effects; requires the owning
/// agent's shell capability and Bearer auth.
#[handler]
pub async fn projects_run_shell(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(
        req,
        depot,
        "/api/projects/run_shell",
        "runProjectShellCommand",
    );
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: RunShellRequest = match req.parse_json().await {
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
            ToolCall::RunShell {
                project: body.project,
                command: body.command,
                timeout_secs: body.timeout_secs,
                cwd: body.cwd,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "run_shell", project, result);
}

#[handler]
pub async fn runtime_status(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/runtime/status", "getRuntimeStatus");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    // Body is optional; tolerate an empty/missing body since this call takes
    // no arguments.
    let body: Value = match req.parse_json().await {
        Ok(body) => body,
        Err(_) => Value::Null,
    };
    let _ = body;
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(ToolCall::RuntimeStatus, auth.as_ref())
        .await;
    render_result(res, &audit, "runtime_status", None, result);
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
                path: body.path,
                limit: body.limit,
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
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "git_diff_summary", Some(project), result);
}

/// `ToolCall::RegisterProject`. Registers an existing directory as a
/// WebCodex project on the selected agent. Mutation with side effects; executes
/// on the selected agent and is constrained by agent policy.
#[handler]
pub async fn projects_register(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/register", "registerProject");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: RegisterProjectRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::RegisterProject {
                client_id: body.client_id,
                id: body.id,
                name: body.name,
                path: body.path,
                description: body.description,
                allow_patch: body.allow_patch,
                overwrite: body.overwrite,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "register_project", None, result);
}

/// `ToolCall::CreateProject`. Creates a new directory on the selected agent
/// and registers it as a WebCodex project. Mutation with side effects; executes
/// on the selected agent and is constrained by agent policy.
#[handler]
pub async fn projects_create(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/projects/create", "createProject");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: CreateProjectRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::CreateProject {
                client_id: body.client_id,
                id: body.id,
                name: body.name,
                path: body.path,
                description: body.description,
                allow_patch: body.allow_patch,
                template: body.template,
                git_init: body.git_init,
                allow_existing_empty: body.allow_existing_empty,
                overwrite: body.overwrite,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "create_project", None, result);
}

/// `ToolCall::ListJobs`. Bounded job summaries (no stdout/stderr bodies).
#[handler]
pub async fn jobs_list(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/list", "listRuntimeJobs");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: ListJobsRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::ListJobs {
                limit: body.limit,
                status: body.status,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "list_jobs", None, result);
}

/// `ToolCall::JobTail`. Bounded stdout/stderr tails for a job.
#[handler]
pub async fn job_tail(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let audit = ActionAudit::start(req, depot, "/api/jobs/tail", "getRuntimeJobTail");
    let Some(runtime) = runtime(depot) else {
        res.status_code(StatusCode::INTERNAL_SERVER_ERROR);
        res.render(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Tool runtime not configured",
        ));
        return;
    };
    let body: JobTailRequest = match req.parse_json().await {
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
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .dispatch_with_auth(
            ToolCall::JobTail {
                job_id: body.job_id,
                tail_lines: body.tail_lines,
            },
            auth.as_ref(),
        )
        .await;
    render_result(res, &audit, "job_tail", None, result);
}

fn tool_project(call: &ToolCall) -> Option<String> {
    match call {
        ToolCall::RunShell { project, .. }
        | ToolCall::ApplyPatch { project, .. }
        | ToolCall::ApplyPatchChecked { project, .. }
        | ToolCall::DeleteProjectFiles { project, .. }
        | ToolCall::GitRestorePaths { project, .. }
        | ToolCall::DiscardUntracked { project, .. }
        | ToolCall::ValidatePatch { project, .. }
        | ToolCall::ReplaceInFile { project, .. }
        | ToolCall::WriteProjectFile { project, .. }
        | ToolCall::SaveProjectArtifact { project, .. }
        | ToolCall::ReadProjectArtifactMetadata { project, .. }
        | ToolCall::GitStatus { project }
        | ToolCall::GitDiff { project, .. }
        | ToolCall::GitDiffSummary { project }
        | ToolCall::ReadFile { project, .. }
        | ToolCall::ListProjectFiles { project, .. }
        | ToolCall::SearchProjectText { project, .. }
        | ToolCall::RunJob { project, .. }
        | ToolCall::RunCodex { project, .. } => Some(project.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::{Executor, ProjectConfig, ProjectsConfig, ProjectsState};
    use crate::shell_client::ShellClientRegistry;
    use crate::CodexConfig;
    use salvo::test::{ResponseExt, TestClient};
    use salvo::Service;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn test_config(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config::default(),
        })
    }

    fn test_config_oauth2(token: Option<&str>) -> Arc<crate::Config> {
        Arc::new(crate::Config {
            addr: "127.0.0.1:0".to_string(),
            data_dir: PathBuf::from("./data"),
            token: token.map(str::to_string),
            enable_ssh: false,
            max_text_size: 2 * 1024 * 1024,
            max_file_size: 100 * 1024 * 1024,
            codex: CodexConfig::default(),
            oauth2: crate::OAuth2Config {
                enabled: true,
                access_token_ttl_secs: 3600,
                refresh_token_ttl_secs: 2_592_000,
                ..crate::OAuth2Config::default()
            },
        })
    }

    fn test_db() -> (tempfile::TempDir, Arc<crate::Database>) {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::Database::open(&tmp.path().join("test.db")).unwrap();
        (tmp, Arc::new(db))
    }

    fn seed_user(db: &crate::Database, username: &str) -> crate::models::UserRecord {
        let now = chrono::Utc::now().timestamp();
        let user = crate::models::UserRecord {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.to_string(),
            created_at: now,
            disabled: 0,
            display_name: None,
            role: "user".to_string(),
            disabled_at: None,
            updated_at: Some(now),
        };
        db.create_user(&user).unwrap();
        user
    }

    fn seed_oauth_client(
        db: &crate::Database,
        user: &crate::models::UserRecord,
    ) -> crate::models::OAuthClientRecord {
        let now = chrono::Utc::now().timestamp();
        let secret = crate::auth::generate_oauth_client_secret();
        let record = crate::models::OAuthClientRecord {
            id: uuid::Uuid::new_v4().to_string(),
            client_id: crate::auth::generate_oauth_client_id(),
            client_secret_hash: crate::auth::hash_token(&secret),
            name: "Test App".to_string(),
            owner_user_id: user.id.clone(),
            redirect_uris: "https://example.com/callback".to_string(),
            allowed_scopes: "runtime:read project:read project:write job:run account:manage"
                .to_string(),
            created_at: now,
            revoked_at: None,
        };
        db.insert_oauth_client(&record).unwrap();
        record
    }

    fn seed_oauth_access_token(
        db: &crate::Database,
        client: &crate::models::OAuthClientRecord,
        user: &crate::models::UserRecord,
        scopes: &str,
    ) -> String {
        let now = chrono::Utc::now().timestamp();
        let plaintext = crate::auth::generate_oauth_access_token();
        let record = crate::models::OAuthAccessTokenRecord {
            id: uuid::Uuid::new_v4().to_string(),
            token_hash: crate::auth::hash_token(&plaintext),
            client_id: client.client_id.clone(),
            user_id: user.id.clone(),
            scopes: scopes.to_string(),
            resource: None,
            created_at: now,
            expires_at: now + 3600,
            revoked_at: None,
            last_used_at: None,
        };
        db.insert_oauth_access_token(&record).unwrap();
        plaintext
    }

    fn phase2_oauth_service(scopes: &str) -> (tempfile::TempDir, salvo::Service, String) {
        let config = test_config_oauth2(Some("secret"));
        let (tmp, db) = test_db();
        let user = seed_user(&db, "alice");
        let client = seed_oauth_client(&db, &user);
        let token = seed_oauth_access_token(&db, &client, &user, scopes);
        let project_dir = tmp.path().join("project");
        std::fs::create_dir(&project_dir).unwrap();
        std::fs::write(project_dir.join("README.md"), "hello\n").unwrap();
        let runtime = Arc::new(runtime_with_local_project(&project_dir, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));
        (tmp, service, token)
    }

    fn local_project_config(path: &str) -> ProjectConfig {
        ProjectConfig {
            path: path.to_string(),
            executor: Executor::Local,
            client_id: None,
            allow_patch: true,
            allow_command_requests: false,
            allow_raw_command_requests: false,
            default_apply_patch_backend: None,
            allowed_checks: vec![],
            checks: None,
            commands: HashMap::new(),
            hooks: HashMap::new(),
        }
    }

    /// Build a ToolRuntime backed by a single local project rooted at `root`.
    fn runtime_with_local_project(root: &std::path::Path, project_id: &str) -> ToolRuntime {
        let mut projects = HashMap::new();
        projects.insert(
            project_id.to_string(),
            local_project_config(&root.to_string_lossy()),
        );
        let config = ProjectsConfig { projects };
        let state = ProjectsState::loaded(config, "test".to_string());
        ToolRuntime::new(
            Arc::new(state),
            Arc::new(ShellClientRegistry::default()),
            Arc::new(CodexConfig::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        )
    }

    /// Build a router that mirrors the production /api wiring for the new
    /// dedicated project actions: Config, Database, and ToolRuntime are
    /// injected so AuthMiddleware and the handlers resolve state exactly as
    /// in `main.rs`.
    fn build_projects_router(
        config: Arc<crate::Config>,
        db: Arc<crate::Database>,
        runtime: Arc<ToolRuntime>,
    ) -> Router {
        Router::new()
            .hoop(affix_state::inject(config))
            .hoop(affix_state::inject(db))
            .hoop(affix_state::inject(runtime))
            .push(
                Router::with_path("api")
                    .hoop(crate::AuthMiddleware)
                    .push(Router::with_path("tools/list").post(tools_list))
                    .push(Router::with_path("tools/call").post(tools_call))
                    .push(
                        Router::with_path("artifacts/import")
                            .post(import_conversation_files_to_project),
                    )
                    .push(Router::with_path("projects/list").post(projects_list))
                    .push(Router::with_path("projects/register").post(projects_register))
                    .push(Router::with_path("projects/create").post(projects_create))
                    .push(Router::with_path("projects/read_file").post(projects_read_file))
                    .push(Router::with_path("projects/git_status").post(projects_git_status))
                    .push(Router::with_path("projects/git_diff").post(projects_git_diff))
                    .push(Router::with_path("projects/apply_patch").post(projects_apply_patch))
                    .push(
                        Router::with_path("projects/validate_patch").post(projects_validate_patch),
                    )
                    .push(Router::with_path("projects/run_shell").post(projects_run_shell))
                    .push(
                        Router::with_path("projects/apply_patch_checked")
                            .post(projects_apply_patch_checked),
                    )
                    .push(Router::with_path("projects/delete_files").post(projects_delete_files))
                    .push(
                        Router::with_path("projects/git_restore_paths")
                            .post(projects_git_restore_paths),
                    )
                    .push(
                        Router::with_path("projects/discard_untracked")
                            .post(projects_discard_untracked),
                    )
                    .push(
                        Router::with_path("projects/replace_in_file")
                            .post(projects_replace_in_file),
                    )
                    .push(Router::with_path("projects/write_file").post(projects_write_file))
                    .push(Router::with_path("projects/run_job").post(projects_run_job))
                    .push(Router::with_path("projects/list_files").post(projects_list_files))
                    .push(Router::with_path("projects/search_text").post(projects_search_text))
                    .push(
                        Router::with_path("projects/git_diff_summary")
                            .post(projects_git_diff_summary),
                    )
                    .push(Router::with_path("jobs/list").post(jobs_list))
                    .push(Router::with_path("jobs/tail").post(job_tail))
                    .push(Router::with_path("runtime/status").post(runtime_status)),
            )
    }

    fn effective_status(resp: &Response) -> StatusCode {
        resp.status_code.unwrap_or(StatusCode::OK)
    }

    // =========================================================================
    // importConversationFilesToProject HTTP-level safety
    // =========================================================================

    static IMPORT_HTTP_TEST_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> =
        std::sync::OnceLock::new();

    async fn lock_import_http_test() -> tokio::sync::MutexGuard<'static, ()> {
        IMPORT_HTTP_TEST_LOCK
            .get_or_init(|| tokio::sync::Mutex::new(()))
            .lock()
            .await
    }

    struct MockHttpServer {
        base_url: String,
        handle: tokio::task::JoinHandle<()>,
    }

    impl Drop for MockHttpServer {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    async fn start_mock_http_server(responses: Vec<Vec<u8>>) -> MockHttpServer {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let mut responses = std::collections::VecDeque::from(responses);
            while let Some(response) = responses.pop_front() {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut buf = [0_u8; 4096];
                let _ = stream.read(&mut buf).await;
                let _ = stream.write_all(&response).await;
                let _ = stream.shutdown().await;
            }
        });
        MockHttpServer {
            base_url: format!("http://{}", addr),
            handle,
        }
    }

    fn http_response(status: &str, headers: &[(&str, String)], body: &[u8]) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 {}\r\n", status).into_bytes();
        for (name, value) in headers {
            response.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        response.extend_from_slice(b"\r\n");
        response.extend_from_slice(body);
        response
    }

    fn import_body(download_link: &str, mime_type: &str, name: &str) -> Value {
        json!({"project":"agent:importer:demo","output_dir":"docs/assets","openaiFileIdRefs":[{"name":name,"id":"file_mock","mime_type":mime_type,"download_link":download_link}]})
    }

    async fn import_test_service_with_local_runtime() -> Service {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        Service::new(build_projects_router(config, db, runtime))
    }

    async fn register_import_agent(
        root: &std::path::Path,
    ) -> (Arc<ToolRuntime>, Arc<ShellClientRegistry>) {
        use crate::shell_protocol::{ShellAgentProjectSummary, ShellClientRegisterRequest};
        let registry = Arc::new(ShellClientRegistry::default());
        registry
            .register(ShellClientRegisterRequest {
                client_id: "importer".to_string(),
                agent_instance_id: "inst-import".to_string(),
                display_name: None,
                owner: None,
                hostname: None,
                capabilities: None,
                projects: Some(vec![ShellAgentProjectSummary {
                    id: "demo".to_string(),
                    name: Some("Demo".to_string()),
                    path: root.to_string_lossy().to_string(),
                    allow_patch: true,
                    kind: None,
                    description: None,
                    hooks: vec![],
                    disabled: false,
                    git_branch: None,
                    git_head: None,
                    git_dirty: None,
                    updated_at: 0,
                    shell_profile: None,
                }]),
                agent_protocol_version: None,
                policy: None,
            })
            .await
            .unwrap();
        let runtime = Arc::new(ToolRuntime::new(
            Arc::new(ProjectsState::failed(
                "server projects disabled in import tests".to_string(),
                "test".to_string(),
            )),
            registry.clone(),
            Arc::new(CodexConfig::default()),
            Arc::new(crate::tool_runtime::RuntimeInfo::default()),
        ));
        (runtime, registry)
    }

    async fn complete_one_save_artifact_request(registry: Arc<ShellClientRegistry>) {
        use crate::shell_protocol::{ShellAgentPollRequest, ShellAgentResultRequest};
        use sha2::{Digest, Sha256};
        let request = loop {
            if let Some(request) = registry
                .poll(ShellAgentPollRequest {
                    client_id: "importer".to_string(),
                    agent_instance_id: "inst-import".to_string(),
                    projects: None,
                })
                .await
                .unwrap()
            {
                break request;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        };
        let payload: Value = serde_json::from_str(request.stdin.as_deref().unwrap()).unwrap();
        let path = payload["path"].as_str().unwrap().to_string();
        let mime_type = payload["mime_type"].as_str().unwrap().to_string();
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            payload["content_base64"].as_str().unwrap(),
        )
        .unwrap();
        let full_path = std::path::Path::new(request.cwd.as_deref().unwrap()).join(&path);
        std::fs::create_dir_all(full_path.parent().unwrap()).unwrap();
        std::fs::write(&full_path, &bytes).unwrap();
        let sha256 = format!("{:x}", Sha256::digest(&bytes));
        let stdout =
            json!({"path":path,"bytes_written":bytes.len(),"sha256":sha256,"mime_type":mime_type})
                .to_string();
        registry
            .complete(ShellAgentResultRequest {
                client_id: "importer".to_string(),
                agent_instance_id: "inst-import".to_string(),
                request_id: request.request_id,
                exit_code: Some(0),
                stdout: Some(stdout),
                stderr: None,
                duration_ms: Some(1),
                error: None,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn import_http_rejects_http_download_link() {
        let service = import_test_service_with_local_runtime().await;
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&import_body(
                "http://files.oaiusercontent.com/a.png",
                "image/png",
                "a.png",
            ))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("https"));
    }

    #[tokio::test]
    async fn import_http_rejects_non_openai_file_host() {
        let service = import_test_service_with_local_runtime().await;
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&import_body(
                "https://example.com/a.png",
                "image/png",
                "a.png",
            ))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("OpenAI file host"));
    }

    #[tokio::test]
    async fn import_http_does_not_follow_302_redirect() {
        let _guard = lock_import_http_test().await;
        let server = start_mock_http_server(vec![http_response(
            "302 Found",
            &[(
                "Location",
                "https://files.oaiusercontent.com/other.png".to_string(),
            )],
            b"",
        )])
        .await;
        set_import_test_download_base_url(Some(server.base_url.clone()));
        let service = import_test_service_with_local_runtime().await;
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&import_body(
                "https://files.oaiusercontent.com/a.png",
                "image/png",
                "a.png",
            ))
            .send(&service)
            .await;
        set_import_test_download_base_url(None);
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("HTTP 302"));
    }

    #[tokio::test]
    async fn import_http_rejects_content_length_over_limit() {
        let _guard = lock_import_http_test().await;
        let server = start_mock_http_server(vec![http_response(
            "200 OK",
            &[("Content-Length", (MAX_IMPORT_FILE_BYTES + 1).to_string())],
            b"",
        )])
        .await;
        set_import_test_download_base_url(Some(server.base_url.clone()));
        let service = import_test_service_with_local_runtime().await;
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&import_body(
                "https://files.oaiusercontent.com/a.png",
                "image/png",
                "a.png",
            ))
            .send(&service)
            .await;
        set_import_test_download_base_url(None);
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("exceeds"));
    }

    #[tokio::test]
    async fn import_http_rejects_chunked_body_after_limit_without_content_length() {
        let _guard = lock_import_http_test().await;
        let body = vec![b'x'; MAX_IMPORT_FILE_BYTES + 1];
        let server = start_mock_http_server(vec![http_response("200 OK", &[], &body)]).await;
        set_import_test_download_base_url(Some(server.base_url.clone()));
        let service = import_test_service_with_local_runtime().await;
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&import_body(
                "https://files.oaiusercontent.com/a.png",
                "image/png",
                "a.png",
            ))
            .send(&service)
            .await;
        set_import_test_download_base_url(None);
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"].as_str().unwrap().contains("exceeds"));
    }

    #[tokio::test]
    async fn import_http_success_uses_source_name_fallback_for_missing_target() {
        let _guard = lock_import_http_test().await;
        let png = vec![0x89, b'P', b'N', b'G'];
        let webp = b"RIFF\x00\x00\x00\x00WEBP".to_vec();
        let server = start_mock_http_server(vec![
            http_response("200 OK", &[("Content-Length", png.len().to_string())], &png),
            http_response(
                "200 OK",
                &[("Content-Length", webp.len().to_string())],
                &webp,
            ),
        ])
        .await;
        set_import_test_download_base_url(Some(server.base_url.clone()));
        let tmp = tempfile::tempdir().unwrap();
        let (runtime, registry) = register_import_agent(tmp.path()).await;
        let config = test_config(Some("secret"));
        let (_db_tmp, db) = test_db();
        let service = Service::new(build_projects_router(config, db, runtime));
        let agent1 = tokio::spawn(complete_one_save_artifact_request(registry.clone()));
        let agent2 = tokio::spawn(complete_one_save_artifact_request(registry));
        let mut resp = TestClient::post("http://localhost/api/artifacts/import")
            .bearer_auth("secret")
            .json(&json!({
                "project":"agent:importer:demo",
                "output_dir":"docs/assets",
                "targets":["custom.png"],
                "openaiFileIdRefs":[
                    {"name":"generated.png","id":"file_png","mime_type":"image/png","download_link":"https://files.oaiusercontent.com/generated.png"},
                    {"name":"fallback.webp","id":"file_webp","mime_type":"image/webp","download_link":"https://files.oaiusercontent.com/fallback.webp"}
                ]
            }))
            .send(&service).await;
        set_import_test_download_base_url(None);
        agent1.await.unwrap();
        agent2.await.unwrap();
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let imported = body["output"]["imported"].as_array().unwrap();
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0]["path"], "docs/assets/custom.png");
        assert_eq!(imported[0]["bytes_written"], png.len());
        assert_eq!(imported[0]["mime_type"], "image/png");
        assert_eq!(imported[0]["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(imported[1]["path"], "docs/assets/fallback.webp");
        assert_eq!(imported[1]["bytes_written"], webp.len());
        assert_eq!(imported[1]["mime_type"], "image/webp");
        assert_eq!(imported[1]["sha256"].as_str().unwrap().len(), 64);
        assert_eq!(
            std::fs::read(tmp.path().join("docs/assets/custom.png")).unwrap(),
            png
        );
        assert_eq!(
            std::fs::read(tmp.path().join("docs/assets/fallback.webp")).unwrap(),
            webp
        );
    }

    // =========================================================================
    // listProjects
    // =========================================================================

    #[tokio::test]
    async fn http_projects_list_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/list")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_list_rejects_wrong_bearer() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/list")
            .bearer_auth("wrong")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_list_ignores_server_configured_projects() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let list = body["output"]
            .as_array()
            .expect("output is a project array");
        assert!(
            list.is_empty(),
            "runtime project discovery is agent-registered only"
        );
    }

    // =========================================================================
    // readProjectFile
    // =========================================================================

    #[tokio::test]
    async fn http_projects_read_file_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        std::fs::write(tmp_proj.path().join("README.md"), "hello").unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/read_file")
            .json(&json!({"project": "demo", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_read_file_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        std::fs::write(tmp_proj.path().join("README.md"), "line1\nline2\n").unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    #[tokio::test]
    async fn http_projects_read_file_rejects_unknown_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/read_file")
            .bearer_auth("secret")
            .json(&json!({"project": "nope", "path": "README.md"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("nope"));
    }

    // =========================================================================
    // getProjectGitStatus
    // =========================================================================

    #[tokio::test]
    async fn http_projects_git_status_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/git_status")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_git_status_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        // Initialize a real git repo so `git status --porcelain` succeeds.
        let root = tmp_proj.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init");
        std::fs::write(root.join("tracked.txt"), "a").unwrap();
        let runtime = Arc::new(runtime_with_local_project(root, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/git_status")
            .bearer_auth("secret")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // getProjectGitDiff
    // =========================================================================

    #[tokio::test]
    async fn http_projects_git_diff_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/git_diff")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_git_diff_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let root = tmp_proj.path();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(root)
            .output()
            .expect("git init");
        let runtime = Arc::new(runtime_with_local_project(root, "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/git_diff")
            .bearer_auth("secret")
            .json(&json!({"project": "demo"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // applyProjectPatch
    // =========================================================================

    #[tokio::test]
    async fn http_projects_apply_patch_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/apply_patch")
            .json(&json!({"project": "demo", "patch": "diff"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_apply_patch_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/apply_patch")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "patch": "diff"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // runProjectShellCommand
    // =========================================================================

    #[tokio::test]
    async fn http_projects_run_shell_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/run_shell")
            .json(&json!({"project": "demo", "command": "echo hi"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_run_shell_rejects_server_configured_project() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/run_shell")
            .bearer_auth("secret")
            .json(&json!({"project": "demo", "command": "echo hi"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(body["error"].as_str().unwrap().contains("projects.toml"));
    }

    // =========================================================================
    // getRuntimeStatus / /api/runtime/status
    // =========================================================================

    #[tokio::test]
    async fn http_runtime_status_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/runtime/status")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_runtime_status_rejects_wrong_bearer() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth("wrong")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_runtime_status_correct_bearer_returns_summary() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/runtime/status")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        let out = &body["output"];
        assert_eq!(out["service"], "webcodex");
        assert_eq!(out["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(out["projects"]["configured"], true);
        assert_eq!(out["projects"]["count"], 1);
        assert!(out["agents"]["count"].is_i64());
        assert!(out["jobs"]["active_count"].is_i64());
        assert!(out["tools"]["count"].is_i64());
        // No secrets in the HTTP response either.
        let serialized = serde_json::to_string(&body).unwrap();
        for forbidden in ["token", "api_key", "secret", "password"] {
            assert!(
                !serialized
                    .to_lowercase()
                    .contains(&forbidden.to_lowercase()),
                "runtime_status HTTP response must not contain '{}'",
                forbidden
            );
        }
    }

    // =========================================================================
    // Phase A read-only console REST wrappers (wiring + auth gate)
    // =========================================================================

    #[tokio::test]
    async fn http_console_routes_require_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        for (path, body) in [
            ("/api/projects/list_files", json!({"project": "demo"})),
            (
                "/api/projects/search_text",
                json!({"project": "demo", "pattern": "fn"}),
            ),
            ("/api/projects/git_diff_summary", json!({"project": "demo"})),
            ("/api/jobs/list", json!({})),
            ("/api/jobs/tail", json!({"job_id": "abc"})),
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{} should require auth",
                path
            );
        }
    }

    #[tokio::test]
    async fn http_console_routes_accept_correct_bearer_and_route_to_runtime() {
        // With a correct bearer token the routes reach the runtime. The
        // project id below is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/list_files")
            .bearer_auth("secret")
            .json(&json!({"project": "agent:nope:nope"}))
            .send(&service)
            .await;
        // Authenticated and dispatched to ToolRuntime: a structured error
        // (BAD_REQUEST + success=false), not a 401/404.
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "list_files should return a structured runtime error"
        );

        // list_jobs reaches the runtime and returns a bounded summary list
        // even with no jobs present.
        let mut resp = TestClient::post("http://localhost/api/jobs/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["jobs"].is_array());
    }

    // =========================================================================
    // validateProjectPatch (POST /api/projects/validate_patch)
    // =========================================================================

    #[tokio::test]
    async fn http_projects_validate_patch_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/validate_patch")
            .json(&json!({"project": "demo", "patch": "diff"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_validate_patch_dispatches_to_runtime() {
        // With a correct bearer token the route reaches the runtime. The
        // project id below is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/validate_patch")
            .bearer_auth("secret")
            .json(&json!({
                "project": "agent:nope:nope",
                "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "validate_patch should return a structured runtime error"
        );
    }

    #[tokio::test]
    async fn http_projects_validate_patch_rejects_empty_patch_via_runtime() {
        // An empty patch is rejected by the runtime with a structured error
        // (BAD_REQUEST + success=false), not a 401/404. This proves the
        // wrapper deserializes and dispatches even for invalid patches.
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/validate_patch")
            .bearer_auth("secret")
            .json(&json!({"project": "agent:nope:nope", "patch": ""}))
            .send(&service)
            .await;
        // Empty patch is rejected; because the project is not agent-registered
        // authorize_agent_tool fails first, but the request is still
        // authenticated + dispatched (structured error, not 401/404).
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
    }

    // =========================================================================
    // Phase 2: callRuntimeTool / /api/tools/call generic entry point
    // =========================================================================

    fn phase2_service() -> (tempfile::TempDir, salvo::Service) {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));
        (_tmp, service)
    }

    #[test]
    fn extract_tool_call_params_precede_flattened_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "wrong",
            "params": {"project": "right"},
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(params, json!({"project": "right"}));
    }

    #[test]
    fn extract_tool_call_arguments_precede_flattened_fields_without_params() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "wrong",
            "arguments": {"project": "right"},
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(params, json!({"project": "right"}));
    }

    #[test]
    fn extract_tool_call_collects_flattened_top_level_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "git_status",
            "project": "agent:oe:webcodex",
        }))
        .unwrap();

        assert_eq!(tool, "git_status");
        assert_eq!(params, json!({"project": "agent:oe:webcodex"}));
    }

    #[test]
    fn extract_tool_call_collects_flattened_line_edit_fields() {
        let (tool, params) = extract_tool_call(&json!({
            "tool": "replace_line_range",
            "project": "agent:oe:webcodex",
            "path": "x.tmp",
            "start_line": 2,
            "end_line": 3,
            "new_text": "BETA\nGAMMA\n",
            "expected_old_prefix": "beta\n",
        }))
        .unwrap();

        assert_eq!(tool, "replace_line_range");
        assert_eq!(params["project"], "agent:oe:webcodex");
        assert_eq!(params["path"], "x.tmp");
        assert_eq!(params["start_line"], 2);
        assert_eq!(params["end_line"], 3);
        assert_eq!(params["new_text"], "BETA\nGAMMA\n");
        assert_eq!(params["expected_old_prefix"], "beta\n");
    }

    #[test]
    fn extract_tool_call_no_argument_tool_keeps_null_params() {
        let (tool, params) = extract_tool_call(&json!({"tool": "list_tools"})).unwrap();

        assert_eq!(tool, "list_tools");
        assert!(params.is_null() || params.as_object().is_some_and(|m| m.is_empty()));
    }

    #[tokio::test]
    async fn http_tools_list_returns_names_and_count() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(
            body["tools"].is_array(),
            "tools array must remain for back-compat"
        );
        assert!(body["names"].is_array(), "names array must be present");
        let names = body["names"].as_array().unwrap();
        assert!(!names.is_empty(), "names must not be empty");
        assert!(names.iter().any(|n| n == "list_tools"));
        assert!(names.iter().any(|n| n == "git_diff_summary"));
        assert_eq!(body["count"], names.len());
        for tool in body["tools"].as_array().unwrap() {
            assert!(tool["inputSchema"].is_object());
            assert!(tool["outputSchema"].is_object());
        }
        // Optional enrichment fields.
        assert!(body["categories"].is_object(), "categories must be present");
        assert!(
            body["recommended_flows"].is_array(),
            "recommended_flows must be present"
        );
        // names and tools must stay in sync.
        let tools_count = body["tools"].as_array().unwrap().len();
        assert_eq!(tools_count, names.len());
    }

    #[tokio::test]
    async fn http_tools_list_requires_bearer_auth() {
        let (_tmp, service) = phase2_service();
        let resp = TestClient::post("http://localhost/api/tools/list")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_tools_call_params_omitted_works_for_list_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["tools"].is_array());
    }

    #[tokio::test]
    async fn http_tools_call_params_null_works_for_list_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools", "params": null}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
        assert!(body["output"]["tools"].is_array());
    }

    #[tokio::test]
    async fn http_tools_call_arguments_alias_works() {
        // `arguments` is accepted as a compatibility alias for `params`.
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "list_tools", "arguments": null}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], true);
    }

    #[tokio::test]
    async fn http_tools_call_params_wins_over_arguments() {
        // When both params and arguments are present, params wins. Use a tool
        // whose params shape we can distinguish: git_diff_summary takes a
        // `project`. The runtime returns a structured error for an unknown
        // project, but the project string from `params` is what gets routed,
        // so we assert the error names the params project (not the arguments
        // one).
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "git_diff_summary",
                "params": {"project": "agent:params-wins:p"},
                "arguments": {"project": "agent:arguments-loses:p"},
            }))
            .send(&service)
            .await;
        // Authenticated + dispatched to ToolRuntime (structured error, not 401).
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("params-wins"),
            "params must win over arguments; error was: {}",
            err
        );
        assert!(
            !err.contains("arguments-loses"),
            "arguments must not be used when params present; error was: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_unknown_tool_returns_useful_error() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "definitely_not_a_tool"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("definitely_not_a_tool"),
            "error must name the tool"
        );
        // Must point the caller at discovery and list available tools.
        assert!(
            err.contains("listRuntimeTools") || err.contains("list_tools"),
            "error should hint at discovery: {}",
            err
        );
        assert!(
            err.contains("git_diff_summary"),
            "error should list available tools: {}",
            err
        );
        // Must not leak secrets / config artifacts.
        let lower = err.to_lowercase();
        for forbidden in [
            "token",
            "authorization",
            "agent.toml",
            "webcodex.env",
            "secret",
        ] {
            assert!(
                !lower.contains(&forbidden),
                "unknown-tool error must not leak '{}': {}",
                forbidden,
                err
            );
        }
    }

    #[tokio::test]
    async fn http_tools_call_missing_required_field_names_tool_and_field() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "run_shell", "params": {"command": "echo"}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("run_shell"),
            "error must name the tool: {}",
            err
        );
        assert!(
            err.contains("project"),
            "error must name the missing field: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_wrong_field_type_names_tool() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "run_shell", "params": {"project": 123, "command": "echo"}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("run_shell"),
            "wrong-type error must name the tool: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_missing_tool_field_returns_field_error() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"params": {}}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        let err = body["error"].as_str().unwrap();
        assert!(
            err.contains("tool"),
            "error must mention the missing 'tool' field: {}",
            err
        );
    }

    #[tokio::test]
    async fn http_tools_call_git_diff_summary_dispatches() {
        // callRuntimeTool routes git_diff_summary to the runtime. With an
        // unknown agent project the runtime returns a structured error (not a
        // 401/404), proving the generic path deserializes + dispatches.
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({
                "tool": "git_diff_summary",
                "params": {"project": "agent:nope:nope"}
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"].as_str().is_some_and(|e| !e.is_empty()),
            "git_diff_summary should return a structured runtime error"
        );
    }

    #[tokio::test]
    async fn http_tools_call_requires_bearer_auth() {
        let (_tmp, service) = phase2_service();
        let resp = TestClient::post("http://localhost/api/tools/call")
            .json(&json!({"tool": "list_tools"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    async fn oauth_tools_call(
        service: &Service,
        token: &str,
        tool: &str,
        params: Value,
    ) -> (StatusCode, Value, Option<String>) {
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth(token)
            .json(&json!({"tool": tool, "params": params}))
            .send(service)
            .await;
        let status = effective_status(&resp);
        let challenge = resp
            .headers()
            .get("www-authenticate")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let body = resp.take_json::<Value>().await.unwrap();
        (status, body, challenge)
    }

    fn assert_oauth_scope_rejected(
        status: StatusCode,
        body: &Value,
        challenge: Option<&str>,
        scope: Option<&str>,
    ) {
        assert_eq!(status, StatusCode::FORBIDDEN, "body: {:?}", body);
        assert_eq!(body["error"], "insufficient_scope");
        let challenge = challenge.unwrap_or("");
        assert!(
            challenge.contains("error=\"insufficient_scope\""),
            "challenge: {}",
            challenge
        );
        if let Some(scope) = scope {
            assert!(
                body["error_description"]
                    .as_str()
                    .unwrap_or("")
                    .contains(scope),
                "body: {:?}",
                body
            );
            assert!(challenge.contains(scope), "challenge: {}", challenge);
        }
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_runtime_read_for_list_tools_or_runtime_status() {
        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, _) = oauth_tools_call(&service, &token, "list_tools", Value::Null).await;
        assert_eq!(status, StatusCode::OK, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) =
            oauth_tools_call(&service, &token, "runtime_status", Value::Null).await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_RUNTIME_READ),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_project_read_for_read_file() {
        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "read_file",
            json!({"project": "demo", "path": "README.md"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("runtime:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "read_file",
            json!({"project": "demo", "path": "README.md"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_PROJECT_READ),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_project_write_for_anchor_edit_tools() {
        let (_tmp, service, token) = phase2_oauth_service("project:write");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "replace_exact_block",
            json!({"project": "demo", "path": "README.md", "old_text": "old", "new_text": "new"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "insert_before_pattern",
            json!({
                "project": "demo",
                "path": "README.md",
                "pattern": "anchor",
                "text": "inserted\n"
            }),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_PROJECT_WRITE),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_requires_job_run_for_run_shell_or_run_job() {
        let (_tmp, service, token) = phase2_oauth_service("job:run");
        let (status, body, _) = oauth_tools_call(
            &service,
            &token,
            "run_shell",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .await;
        assert_ne!(status, StatusCode::FORBIDDEN, "body: {:?}", body);

        let (_tmp, service, token) = phase2_oauth_service("project:read");
        let (status, body, challenge) = oauth_tools_call(
            &service,
            &token,
            "run_job",
            json!({"project": "demo", "command": "echo hi"}),
        )
        .await;
        assert_oauth_scope_rejected(
            status,
            &body,
            challenge.as_deref(),
            Some(crate::auth::SCOPE_JOB_RUN),
        );
    }

    #[tokio::test]
    async fn oauth2_tools_call_unknown_tool_fails_closed() {
        let (_tmp, service, token) = phase2_oauth_service("runtime:read project:read");
        let (status, body, challenge) =
            oauth_tools_call(&service, &token, "definitely_not_a_tool", Value::Null).await;
        assert_oauth_scope_rejected(status, &body, challenge.as_deref(), None);
    }

    #[tokio::test]
    async fn api_token_tools_call_behavior_unchanged() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/call")
            .bearer_auth("secret")
            .json(&json!({"tool": "definitely_not_a_tool"}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert!(body["error"]
            .as_str()
            .unwrap_or("")
            .contains("definitely_not_a_tool"));
    }

    // =========================================================================
    // Phase 3: dedicated mutation actions (apply_patch_checked, delete_files,
    // git_restore_paths, discard_untracked) — auth gate + dispatch wiring
    // =========================================================================

    #[tokio::test]
    async fn http_phase3_mutation_actions_require_bearer_auth() {
        let (_tmp, service) = phase2_service();
        for (path, body) in [
            (
                "/api/projects/apply_patch_checked",
                json!({"project": "demo", "patch": "diff"}),
            ),
            (
                "/api/projects/delete_files",
                json!({"project": "demo", "paths": ["x.txt"]}),
            ),
            (
                "/api/projects/git_restore_paths",
                json!({"project": "demo", "paths": ["x.txt"]}),
            ),
            (
                "/api/projects/discard_untracked",
                json!({"project": "demo", "paths": ["x.txt"]}),
            ),
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{} should require auth",
                path
            );
        }
    }

    #[tokio::test]
    async fn http_phase3_mutation_actions_dispatch_to_runtime() {
        // With a correct bearer token the mutation routes reach the runtime.
        // The project id is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let (_tmp, service) = phase2_service();
        for (path, body) in [
            (
                "/api/projects/apply_patch_checked",
                json!({"project": "agent:nope:nope", "patch": "--- a/f.txt\n+++ b/f.txt\n@@ -1 +1,2 @@\nx\n+y\n"}),
            ),
            (
                "/api/projects/delete_files",
                json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
            ),
            (
                "/api/projects/git_restore_paths",
                json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
            ),
            (
                "/api/projects/discard_untracked",
                json!({"project": "agent:nope:nope", "paths": ["x.txt"]}),
            ),
        ] {
            let mut resp = TestClient::post(&format!("http://localhost{}", path))
                .bearer_auth("secret")
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST,
                "{} should reach runtime and return structured error",
                path
            );
            let body: Value = resp.take_json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(
                body["error"].as_str().is_some_and(|e| !e.is_empty()),
                "{} should return a structured runtime error",
                path
            );
        }
    }

    // =========================================================================
    // Phase 4/5: structured-edit endpoints — auth gate + dispatch wiring.
    // replace_in_file is now also a dedicated GPT Action; write_file remains
    // runtime-only. Both are still reachable via callRuntimeTool / MCP.
    // =========================================================================

    #[tokio::test]
    async fn http_phase4_edit_endpoints_require_bearer_auth() {
        let (_tmp, service) = phase2_service();
        for (path, body) in [
            (
                "/api/projects/replace_in_file",
                json!({"project": "demo", "path": "x.txt", "old": "a", "new": "b"}),
            ),
            (
                "/api/projects/write_file",
                json!({"project": "demo", "path": "x.txt", "content": "a"}),
            ),
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{} should require auth",
                path
            );
        }
    }

    #[tokio::test]
    async fn http_phase4_edit_endpoints_dispatch_to_runtime() {
        // With a correct bearer token the edit routes reach the runtime. The
        // project id is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let (_tmp, service) = phase2_service();
        for (path, body, tool) in [
            (
                "/api/projects/replace_in_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "old": "a", "new": "b"}),
                "replace_in_file",
            ),
            (
                "/api/projects/write_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
                "write_project_file",
            ),
        ] {
            let mut resp = TestClient::post(&format!("http://localhost{}", path))
                .bearer_auth("secret")
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST,
                "{} should reach runtime and return structured error",
                path
            );
            let body: Value = resp.take_json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(
                body["error"].as_str().is_some_and(|e| !e.is_empty()),
                "{} should return a structured runtime error",
                tool
            );
        }
    }

    #[tokio::test]
    async fn http_tools_list_includes_phase4_edit_tools() {
        let (_tmp, service) = phase2_service();
        let mut resp = TestClient::post("http://localhost/api/tools/list")
            .bearer_auth("secret")
            .json(&json!({}))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::OK);
        let body: Value = resp.take_json().await.unwrap();
        let names = body["names"].as_array().unwrap();
        assert!(names.iter().any(|n| n == "replace_in_file"));
        assert!(names.iter().any(|n| n == "write_project_file"));
        assert_eq!(body["count"], names.len());
    }

    #[tokio::test]
    async fn http_tools_call_dispatches_phase4_edit_tools() {
        // callRuntimeTool routes replace_in_file / write_project_file to the
        // runtime. With a non-agent project the runtime returns a structured
        // error (not a 401/404), proving the generic path dispatches them.
        let (_tmp, service) = phase2_service();
        for (tool, params) in [
            (
                "replace_in_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "old": "a", "new": "b"}),
            ),
            (
                "write_project_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
            ),
        ] {
            let mut resp = TestClient::post("http://localhost/api/tools/call")
                .bearer_auth("secret")
                .json(&json!({"tool": tool, "params": params}))
                .send(&service)
                .await;
            assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
            let body: Value = resp.take_json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(
                body["error"].as_str().is_some_and(|e| !e.is_empty()),
                "{} should return a structured runtime error",
                tool
            );
        }
    }

    // =========================================================================
    // Dedicated writeProjectFile / startProjectShellJob GPT Actions — auth gate
    // + dispatch wiring. write_file reuses the existing REST wrapper; run_job
    // is a new thin wrapper over ToolCall::RunJob. Both are still reachable
    // via callRuntimeTool / MCP.
    // =========================================================================

    #[tokio::test]
    async fn http_dedicated_write_file_and_run_job_require_bearer_auth() {
        let (_tmp, service) = phase2_service();
        for (path, body) in [
            (
                "/api/projects/write_file",
                json!({"project": "demo", "path": "x.txt", "content": "a"}),
            ),
            (
                "/api/projects/run_job",
                json!({"project": "demo", "command": "echo hi"}),
            ),
        ] {
            let resp = TestClient::post(&format!("http://localhost{}", path))
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::UNAUTHORIZED,
                "{} should require auth",
                path
            );
        }
    }

    #[tokio::test]
    async fn http_dedicated_write_file_and_run_job_dispatch_to_runtime() {
        // With a correct bearer token the dedicated routes reach the runtime.
        // The project id is not agent-registered, so the runtime returns a
        // structured error (not a 401/404) — proving the request was
        // authenticated, deserialized, and dispatched to ToolRuntime.
        let (_tmp, service) = phase2_service();
        for (path, body) in [
            (
                "/api/projects/write_file",
                json!({"project": "agent:nope:nope", "path": "x.txt", "content": "a"}),
            ),
            (
                "/api/projects/run_job",
                json!({"project": "agent:nope:nope", "command": "echo hi"}),
            ),
        ] {
            let mut resp = TestClient::post(&format!("http://localhost{}", path))
                .bearer_auth("secret")
                .json(&body)
                .send(&service)
                .await;
            assert_eq!(
                effective_status(&resp),
                StatusCode::BAD_REQUEST,
                "{} should reach runtime and return structured error",
                path
            );
            let body: Value = resp.take_json().await.unwrap();
            assert_eq!(body["success"], false);
            assert!(
                body["error"].as_str().is_some_and(|e| !e.is_empty()),
                "{} should return a structured runtime error",
                path
            );
        }
    }

    // =========================================================================
    // register_project / create_project REST endpoints
    // =========================================================================

    #[tokio::test]
    async fn http_projects_register_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/register")
            .json(&json!({
                "client_id": "oe",
                "id": "my-project",
                "name": "My Project",
                "path": "/root/git/my-project"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_create_requires_bearer_auth() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let resp = TestClient::post("http://localhost/api/projects/create")
            .json(&json!({
                "client_id": "oe",
                "id": "hello",
                "name": "Hello",
                "path": "/root/git/hello"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn http_projects_register_rejects_unknown_client_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/register")
            .bearer_auth("secret")
            .json(&json!({
                "client_id": "no-such-agent",
                "id": "my-project",
                "name": "My Project",
                "path": "/root/git/my-project"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown agent")),
            "register_project should reject unknown client_id: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn http_projects_create_rejects_unknown_client_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/create")
            .bearer_auth("secret")
            .json(&json!({
                "client_id": "no-such-agent",
                "id": "hello",
                "name": "Hello",
                "path": "/root/git/hello"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|e| e.contains("unknown agent")),
            "create_project should reject unknown client_id: {:?}",
            body["error"]
        );
    }

    #[tokio::test]
    async fn http_projects_register_rejects_unsafe_id() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/register")
            .bearer_auth("secret")
            .json(&json!({
                "client_id": "oe",
                "id": "a/b",
                "name": "Test",
                "path": "/root/git/test"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
    }

    #[tokio::test]
    async fn http_projects_create_rejects_relative_path() {
        let config = test_config(Some("secret"));
        let (_tmp, db) = test_db();
        let tmp_proj = tempfile::tempdir().unwrap();
        let runtime = Arc::new(runtime_with_local_project(tmp_proj.path(), "demo"));
        let service = Service::new(build_projects_router(config, db, runtime));

        let mut resp = TestClient::post("http://localhost/api/projects/create")
            .bearer_auth("secret")
            .json(&json!({
                "client_id": "oe",
                "id": "hello",
                "name": "Hello",
                "path": "relative/path"
            }))
            .send(&service)
            .await;
        assert_eq!(effective_status(&resp), StatusCode::BAD_REQUEST);
        let body: Value = resp.take_json().await.unwrap();
        assert_eq!(body["success"], false);
        assert!(
            body["error"]
                .as_str()
                .is_some_and(|e| e.contains("absolute")),
            "create_project should reject relative path: {:?}",
            body["error"]
        );
    }
}
