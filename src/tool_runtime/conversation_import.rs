//! Bounded Control-side import of ChatGPT conversation attachments.
//!
//! ChatGPT supplies temporary OpenAI-hosted file references. WebCodex validates
//! and consumes those references immediately on the Control side, then routes
//! the downloaded bytes through the existing SaveProjectArtifact mutation path.

use super::sessions::SessionTransport;
use super::tool_call::OpenAiHostFileRef;
use super::{ToolCall, ToolResult, ToolRuntime};
use crate::artifact_policy::ooxml_extension_for_mime;
use crate::auth::AuthContext;
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::time::Duration;

pub(crate) const MAX_IMPORT_FILES: usize = 10;
pub(crate) const MAX_IMPORT_FILE_BYTES: usize = 10 * 1024 * 1024;
const IMPORT_OCTET_STREAM_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".webp", ".pdf", ".zip", ".docx", ".pptx", ".xlsx", ".txt", ".csv",
    ".json",
];

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct OpenAiFileIdRef {
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) id: Option<String>,
    #[serde(default)]
    pub(crate) mime_type: Option<String>,
    pub(crate) download_link: String,
}

pub(crate) struct ImportConversationFilesInput {
    pub(crate) openai_file_id_refs: Vec<OpenAiFileIdRef>,
    pub(crate) project: String,
    pub(crate) output_dir: Option<String>,
    pub(crate) targets: Option<Vec<String>>,
    pub(crate) overwrite: Option<bool>,
    pub(crate) session_id: Option<String>,
}

impl From<OpenAiHostFileRef> for OpenAiFileIdRef {
    fn from(value: OpenAiHostFileRef) -> Self {
        Self {
            name: value.file_name,
            // The MCP host file_id is part of the host transport shape only.
            // WebCodex never dereferences or returns it.
            id: None,
            mime_type: value.mime_type,
            download_link: value.download_url,
        }
    }
}

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

fn default_import_leaf(file_ref: &OpenAiFileIdRef, index: usize, mime: &str) -> String {
    let fallback = format!("artifact-{}", index + 1);
    match file_ref.name.as_deref().or(file_ref.id.as_deref()) {
        Some(source_name) => sanitize_import_name(source_name, &fallback),
        None => match ooxml_extension_for_mime(mime) {
            Some(extension) => format!("{fallback}{extension}"),
            None => fallback,
        },
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
        format!("{dir}/{leaf}")
    };
    crate::tool_runtime::files::validate_artifact_file_path(&candidate)?;
    Ok(candidate)
}

fn mime_allowed_for_import(mime: &str, path: &str) -> bool {
    let lower_path = path.to_ascii_lowercase();
    if let Some(required_extension) = ooxml_extension_for_mime(mime) {
        return lower_path.ends_with(required_extension);
    }
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
        && IMPORT_OCTET_STREAM_EXTENSIONS
            .iter()
            .any(|suffix| lower_path.ends_with(suffix)))
}

fn validate_openai_download_url(download_link: &str) -> Result<reqwest::Url, String> {
    let url =
        reqwest::Url::parse(download_link).map_err(|e| format!("invalid download_link: {e}"))?;
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
pub(crate) fn set_import_test_download_base_url(base_url: Option<String>) {
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
                "download for '{source_name}' exceeds {MAX_IMPORT_FILE_BYTES} bytes"
            ));
        }
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| format!("failed to read download for '{source_name}'"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_IMPORT_FILE_BYTES {
            return Err(format!(
                "download for '{source_name}' exceeds {MAX_IMPORT_FILE_BYTES} bytes"
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

impl ToolRuntime {
    pub(crate) async fn import_conversation_files(
        &self,
        input: ImportConversationFilesInput,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        if input.openai_file_id_refs.is_empty()
            || input.openai_file_id_refs.len() > MAX_IMPORT_FILES
        {
            return ToolResult::err(format!(
                "openaiFileIdRefs must contain 1..={MAX_IMPORT_FILES} files"
            ));
        }
        let client = match reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()
        {
            Ok(client) => client,
            Err(e) => return ToolResult::err(format!("failed to build HTTP client: {e}")),
        };
        let mut imported = Vec::new();
        for (idx, file_ref) in input.openai_file_id_refs.iter().enumerate() {
            let source_name = file_ref
                .name
                .as_deref()
                .or(file_ref.id.as_deref())
                .unwrap_or("artifact");
            let mime = file_ref
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            let fallback = format!("artifact-{}", idx + 1);
            let leaf = input
                .targets
                .as_ref()
                .and_then(|targets| targets.get(idx))
                .map(|target| sanitize_import_name(target, &fallback))
                .unwrap_or_else(|| default_import_leaf(file_ref, idx, mime));
            let path = match join_import_path(input.output_dir.as_deref(), &leaf) {
                Ok(path) => path,
                Err(e) => return ToolResult::err(e),
            };
            if !mime_allowed_for_import(mime, &path) {
                return ToolResult::err(format!(
                    "unsupported MIME type for '{source_name}': {mime}"
                ));
            }
            let url = match validate_openai_download_url(&file_ref.download_link) {
                Ok(url) => url,
                Err(e) => return ToolResult::err(e),
            };
            let mut response = match client.get(request_url_for_download(url)).send().await {
                Ok(response) => response,
                Err(_) => {
                    // reqwest error text can include the request URL. Keep the
                    // temporary host URL out of durable/model-visible errors.
                    return ToolResult::err(format!("failed to download '{source_name}'"));
                }
            };
            if !response.status().is_success() {
                return ToolResult::err(format!(
                    "download for '{source_name}' returned HTTP {}",
                    response.status()
                ));
            }
            let bytes = match read_bounded_download(&mut response, source_name).await {
                Ok(bytes) => bytes,
                Err(e) => return ToolResult::err(e),
            };
            let result = Box::pin(self.dispatch_with_auth_transport_options(
                ToolCall::SaveProjectArtifact {
                    project: input.project.clone(),
                    path: path.clone(),
                    content_base64: general_purpose::STANDARD.encode(&bytes),
                    session_id: input.session_id.clone(),
                    mime_type: Some(mime.to_string()),
                    overwrite: input.overwrite,
                },
                auth,
                transport.clone(),
                false,
                false,
            ))
            .await;
            if !result.success {
                return result;
            }
            let mut obj = Map::new();
            obj.insert(
                "source_name".to_string(),
                Value::String(source_name.to_string()),
            );
            obj.insert("project".to_string(), Value::String(input.project.clone()));
            obj.insert("path".to_string(), Value::String(path));
            obj.insert(
                "bytes_written".to_string(),
                result.output["bytes_written"].clone(),
            );
            obj.insert("mime_type".to_string(), Value::String(mime.to_string()));
            obj.insert("sha256".to_string(), result.output["sha256"].clone());
            imported.push(Value::Object(obj));
        }
        ToolResult::ok(json!({"imported": imported, "count": imported.len()}))
    }

    pub(crate) async fn dispatch_conversation_import_tool(
        &self,
        call: ToolCall,
        auth: Option<&AuthContext>,
        transport: SessionTransport,
    ) -> ToolResult {
        let ToolCall::ImportConversationFilesToProject {
            project,
            openai_file_id_refs,
            output_dir,
            targets,
            overwrite,
            session_id,
        } = call
        else {
            unreachable!("dispatch_conversation_import_tool called with non-import tool")
        };
        if !matches!(transport, SessionTransport::Mcp) {
            return ToolResult::err(
                "import_conversation_files_to_project requires the MCP host file-reference mechanism; use the dedicated /api/artifacts/import GPT Action outside MCP",
            );
        }
        self.import_conversation_files(
            ImportConversationFilesInput {
                openai_file_id_refs: openai_file_id_refs.into_iter().map(Into::into).collect(),
                project,
                output_dir,
                targets,
                overwrite,
                session_id,
            },
            auth,
            transport,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_import_leaf_preserves_ooxml_when_host_omits_filename() {
        let file_ref = OpenAiFileIdRef {
            name: None,
            id: None,
            mime_type: Some(crate::artifact_policy::PPTX_MIME.to_string()),
            download_link: "https://files.oaiusercontent.com/file".to_string(),
        };
        assert_eq!(
            default_import_leaf(&file_ref, 0, crate::artifact_policy::PPTX_MIME),
            "artifact-1.pptx"
        );
    }
}
