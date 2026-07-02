use super::{render_result, runtime};
use crate::action_audit::ActionAudit;
use crate::json_error;
use crate::tool_runtime::ToolCall;
use base64::{engine::general_purpose, Engine as _};
use salvo::prelude::*;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;

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
pub(super) const MAX_IMPORT_FILE_BYTES: usize = 10 * 1024 * 1024;

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
pub(super) fn set_import_test_download_base_url(base_url: Option<String>) {
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
                    session_id: None,
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
