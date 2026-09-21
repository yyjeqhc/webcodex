use crate::auth::AuthContext;
use crate::tool_runtime::{
    validate_project_artifact_export_snapshot, ProjectArtifactExportSnapshot, ToolResult,
    ToolRuntime, MAX_READ_PROJECT_ARTIFACT_LENGTH,
};
use base64::{engine::general_purpose, Engine as _};
use salvo::http::{HeaderValue, StatusCode};
use salvo::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const DOWNLOAD_TTL: Duration = Duration::from_secs(120);
const MAX_ACTIVE_DOWNLOADS: usize = 64;

#[derive(Clone)]
struct DownloadRecord {
    project: String,
    snapshot: ProjectArtifactExportSnapshot,
    auth: AuthContext,
    expires_at: Instant,
}

#[derive(Default)]
struct DownloadRegistry {
    entries: HashMap<String, DownloadRecord>,
}

impl DownloadRegistry {
    fn cleanup(&mut self) {
        let now = Instant::now();
        self.entries.retain(|_, record| record.expires_at > now);
    }

    fn insert(&mut self, record: DownloadRecord) -> String {
        self.cleanup();
        if self.entries.len() >= MAX_ACTIVE_DOWNLOADS {
            if let Some(key) = self.entries.keys().next().cloned() {
                self.entries.remove(&key);
            }
        }
        loop {
            let id = uuid::Uuid::new_v4().simple().to_string();
            if !self.entries.contains_key(&id) {
                self.entries.insert(id.clone(), record);
                return id;
            }
        }
    }

    fn take(&mut self, id: &str) -> Option<DownloadRecord> {
        self.cleanup();
        if !valid_capability_id(id) {
            return None;
        }
        self.entries.remove(id)
    }
}

static DOWNLOAD_REGISTRY: OnceLock<Mutex<DownloadRegistry>> = OnceLock::new();

fn registry() -> &'static Mutex<DownloadRegistry> {
    DOWNLOAD_REGISTRY.get_or_init(|| Mutex::new(DownloadRegistry::default()))
}

fn valid_capability_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn configured_https_origin(runtime: &ToolRuntime) -> Result<String, String> {
    let raw = runtime
        .runtime_info
        .configured_public_url
        .as_deref()
        .ok_or_else(|| "artifact download links require WEBPI_PUBLIC_URL".to_string())?;
    let url = url::Url::parse(raw.trim())
        .map_err(|_| "WEBPI_PUBLIC_URL is not a valid URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("artifact download links require an HTTPS WEBPI_PUBLIC_URL origin".to_string());
    }
    Ok(raw.trim().trim_end_matches('/').to_string())
}

pub(crate) async fn issue_download_link(
    runtime: &ToolRuntime,
    project: String,
    path: String,
    auth: Option<&AuthContext>,
) -> ToolResult {
    let Some(auth) = auth.cloned() else {
        return ToolResult::err(
            "artifact download links require authenticated project:read authority",
        );
    };
    let origin = match configured_https_origin(runtime) {
        Ok(origin) => origin,
        Err(error) => return ToolResult::err(error),
    };
    let metadata = runtime
        .read_project_artifact_export_metadata_internal(&project, &path, Some(&auth))
        .await;
    if !metadata.success {
        return metadata;
    }
    let canonical_project = metadata
        .output
        .get("project")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(project.as_str())
        .to_string();
    let snapshot = match validate_project_artifact_export_snapshot(&path, &metadata.output) {
        Ok(snapshot) => snapshot,
        Err(error) => return ToolResult::err(error),
    };
    let id = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(DownloadRecord {
            project: canonical_project.clone(),
            snapshot: snapshot.clone(),
            auth,
            expires_at: Instant::now() + DOWNLOAD_TTL,
        });
    ToolResult::ok(serde_json::json!({
        "project": canonical_project,
        "path": snapshot.path,
        "download_url": format!("{origin}/artifact-download?id={id}"),
        "expires_in_secs": DOWNLOAD_TTL.as_secs(),
        "bytes": snapshot.bytes,
        "sha256": snapshot.sha256,
        "mime_type": snapshot.mime_type,
        "name": snapshot.name,
    }))
}

fn not_found(res: &mut Response) {
    res.status_code(StatusCode::NOT_FOUND);
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
}

fn decode_chunk(
    snapshot: &ProjectArtifactExportSnapshot,
    offset: usize,
    length: usize,
    output: &serde_json::Value,
) -> Option<Vec<u8>> {
    if output.get("path").and_then(serde_json::Value::as_str)? != snapshot.path {
        return None;
    }
    let encoded = output
        .get("content_base64")
        .and_then(serde_json::Value::as_str)?;
    let decoded = general_purpose::STANDARD.decode(encoded).ok()?;
    let bytes_returned = usize::try_from(output.get("bytes_returned")?.as_u64()?).ok()?;
    let next_offset = usize::try_from(output.get("next_offset")?.as_u64()?).ok()?;
    let eof = output.get("eof")?.as_bool()?;
    let truncated = output.get("truncated")?.as_bool()?;
    let expected_next = offset.checked_add(decoded.len())?;
    if decoded.len() != bytes_returned
        || decoded.len() > length
        || expected_next != next_offset
        || next_offset > snapshot.bytes
        || (decoded.is_empty() && offset < snapshot.bytes)
        || eof != (next_offset == snapshot.bytes)
        || truncated == eof
    {
        return None;
    }
    Some(decoded)
}

#[handler]
pub(crate) async fn download(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(id) = req.query::<String>("id") else {
        not_found(res);
        return;
    };
    let Some(record) = registry()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take(id.trim())
    else {
        not_found(res);
        return;
    };
    let Ok(runtime) = depot.obtain::<Arc<ToolRuntime>>() else {
        not_found(res);
        return;
    };
    let metadata = runtime
        .read_project_artifact_export_metadata_internal(
            &record.project,
            &record.snapshot.path,
            Some(&record.auth),
        )
        .await;
    if !metadata.success {
        not_found(res);
        return;
    }
    let Ok(snapshot) =
        validate_project_artifact_export_snapshot(&record.snapshot.path, &metadata.output)
    else {
        not_found(res);
        return;
    };
    if snapshot.path != record.snapshot.path
        || snapshot.bytes != record.snapshot.bytes
        || snapshot.sha256 != record.snapshot.sha256
        || snapshot.mime_type != record.snapshot.mime_type
        || snapshot.name != record.snapshot.name
    {
        not_found(res);
        return;
    }

    let mut bytes = Vec::with_capacity(snapshot.bytes);
    let mut offset = 0usize;
    while offset < snapshot.bytes {
        let length = (snapshot.bytes - offset).min(MAX_READ_PROJECT_ARTIFACT_LENGTH);
        let Ok(output) = runtime
            .read_project_artifact_export_chunk_internal(
                &record.project,
                &snapshot.path,
                snapshot.bytes,
                offset,
                length,
                Some(&record.auth),
            )
            .await
        else {
            not_found(res);
            return;
        };
        let Some(chunk) = decode_chunk(&snapshot, offset, length, &output) else {
            not_found(res);
            return;
        };
        offset += chunk.len();
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != snapshot.bytes || format!("{:x}", Sha256::digest(&bytes)) != snapshot.sha256 {
        not_found(res);
        return;
    }

    let Ok(content_type) = HeaderValue::from_str(&snapshot.mime_type) else {
        not_found(res);
        return;
    };
    let Ok(disposition) =
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", snapshot.name))
    else {
        not_found(res);
        return;
    };
    res.status_code(StatusCode::OK);
    res.headers_mut().insert("content-type", content_type);
    res.headers_mut().insert("content-disposition", disposition);
    res.headers_mut()
        .insert("cache-control", HeaderValue::from_static("no-store"));
    if let Ok(length) = HeaderValue::from_str(&bytes.len().to_string()) {
        res.headers_mut().insert("content-length", length);
    }
    res.body(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthKind;

    fn record(expires_at: Instant) -> DownloadRecord {
        DownloadRecord {
            project: "agent:test:project".to_string(),
            snapshot: ProjectArtifactExportSnapshot {
                path: "artifacts/test.jpg".to_string(),
                bytes: 3,
                sha256: "00".repeat(32),
                mime_type: "image/jpeg".to_string(),
                name: "test.jpg".to_string(),
            },
            auth: AuthContext {
                user_id: Some("u".to_string()),
                username: Some("alice".to_string()),
                role: Some("user".to_string()),
                scopes: vec!["project:read".to_string()],
                token_kind: Some("oauth2".to_string()),
                ..AuthContext::new(AuthKind::OAuth2Token)
            },
            expires_at,
        }
    }

    #[test]
    fn capability_id_is_lowercase_hex_and_one_shot() {
        let mut registry = DownloadRegistry::default();
        let id = registry.insert(record(Instant::now() + Duration::from_secs(60)));
        assert!(valid_capability_id(&id));
        assert_eq!(id.len(), 32);
        assert!(registry.take(&id).is_some());
        assert!(registry.take(&id).is_none());
    }

    #[test]
    fn expired_capability_is_not_returned() {
        let mut registry = DownloadRegistry::default();
        let id = registry.insert(record(Instant::now() - Duration::from_secs(1)));
        assert!(registry.take(&id).is_none());
    }

    #[test]
    fn malformed_capability_ids_are_rejected_without_lookup() {
        let mut registry = DownloadRegistry::default();
        let id = registry.insert(record(Instant::now() + Duration::from_secs(60)));
        assert!(registry.take("not-a-capability").is_none());
        assert!(registry.take(&id.to_uppercase()).is_none());
        assert!(registry.take(&id).is_some());
    }
}
