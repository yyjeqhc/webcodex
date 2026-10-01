//! Host-independent, re-authorized resource discovery and latest-content reads.
use super::kernel::{HostFileImportTrust, ToolCallContext, ToolCallRequest, ToolTransport};
use super::{ToolResult, ToolRuntime};
use crate::auth::AuthContext;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::{json, Value};
use webcodex_tool_contracts::tool_call::WebcodexResourceKind;

pub(crate) const RESOURCE_PREFIX: &str = "webcodex-resource://";
const SOURCE_LIMIT: usize = 2000;

#[derive(Debug, PartialEq, Eq)]
enum ResourceIdentity {
    Project {
        id: String,
        root: String,
    },
    File {
        id: String,
        root: String,
        path: String,
    },
    Goal(String),
}

fn encode(value: &str) -> String {
    URL_SAFE_NO_PAD.encode(value)
}
fn decode(value: &str) -> Option<String> {
    let bytes = URL_SAFE_NO_PAD.decode(value).ok()?;
    let text = String::from_utf8(bytes).ok()?;
    (!text.is_empty() && !text.contains('\0') && encode(&text) == value).then_some(text)
}
fn project_uri(id: &str, root: &str) -> String {
    format!("{RESOURCE_PREFIX}project/{}/{}", encode(id), encode(root))
}
fn file_uri(id: &str, root: &str, path: &str) -> String {
    format!(
        "{RESOURCE_PREFIX}file/{}/{}/{}",
        encode(id),
        encode(root),
        encode(path)
    )
}
fn goal_uri(id: &str) -> String {
    format!("{RESOURCE_PREFIX}goal/{}", encode(id))
}
fn parse_uri(uri: &str) -> Option<ResourceIdentity> {
    if uri.len() > 8192 {
        return None;
    }
    let parts = uri
        .strip_prefix(RESOURCE_PREFIX)?
        .split('/')
        .collect::<Vec<_>>();
    match parts.as_slice() {
        ["project", id, root] => Some(ResourceIdentity::Project {
            id: decode(id)?,
            root: decode(root)?,
        }),
        ["file", id, root, path] => {
            let path = decode(path)?;
            super::helpers::validate_project_relative_path(&path).ok()?;
            Some(ResourceIdentity::File {
                id: decode(id)?,
                root: decode(root)?,
                path,
            })
        }
        ["goal", id] => {
            let id = decode(id)?;
            (id.starts_with("wc_goal_")
                && id.len() == 24
                && id[8..]
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
            .then_some(ResourceIdentity::Goal(id))
        }
        _ => None,
    }
}
fn error(kind: &str) -> ToolResult {
    ToolResult::err_with_output(kind, json!({"error_kind": kind, "state_changed": false}))
}
fn link(uri: String, name: &str, description: &str, meta: Value) -> Value {
    json!({"type":"resource_link","uri":uri,"name":name.chars().take(200).collect::<String>(),"title":name.chars().take(200).collect::<String>(),"description":description.chars().take(300).collect::<String>(),"_meta":meta})
}
fn page(
    mut items: Vec<Value>,
    total: usize,
    offset: usize,
    limit: usize,
    incomplete: bool,
) -> ToolResult {
    if items
        .iter()
        .any(|item| item["uri"].as_str().is_none_or(|uri| uri.len() > 8192))
    {
        return error("resource_identity_too_large");
    }
    let had_items = !items.is_empty();
    while serde_json::to_vec(&items)
        .map(|v| v.len())
        .unwrap_or(usize::MAX)
        > 48 * 1024
    {
        if items.pop().is_none() {
            break;
        }
    }
    if had_items && items.is_empty() {
        return error("resource_page_item_too_large");
    }
    let next = (!incomplete && offset.saturating_add(items.len()) < total)
        .then_some(offset.saturating_add(items.len()));
    ToolResult::ok(
        json!({"items":items,"total":total,"offset":offset,"limit":limit,"next_offset":next,"list_truncated":incomplete}),
    )
}

impl ToolRuntime {
    pub(crate) async fn open_webcodex_workbench(
        &self,
        project: Option<String>,
        session: Option<String>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if session.is_some() && project.is_none() {
            return error("resource_project_required");
        }
        let project = match project {
            Some(raw) => {
                if super::kernel::check_runtime_tool_scope(auth, "list_projects").is_err() {
                    return error("resource_access_denied");
                }
                match self.resolve_project_input_for_auth(&raw, auth).await {
                    Ok(r) => Some(r.resolved_id),
                    Err(_) => return error("resource_unavailable"),
                }
            }
            None => None,
        };
        let session = match session {
            Some(raw) => {
                if super::kernel::check_runtime_tool_scope(auth, "session_summary").is_err() {
                    return error("resource_access_denied");
                }
                let id = match self.canonicalize_explicit_session_selector(&raw, auth) {
                    Ok(id) => id,
                    Err(_) => return error("resource_unavailable"),
                };
                if self
                    .authorize_session_target(&id, "session_summary", auth)
                    .await
                    .is_err()
                {
                    return error("resource_unavailable");
                }
                if self
                    .sessions
                    .summary(&id, Some(1))
                    .and_then(|summary| summary.project)
                    .as_ref()
                    != project.as_ref()
                {
                    return error("resource_unavailable");
                }
                Some(id)
            }
            None => None,
        };
        let found = self
            .search_webcodex_resources(
                WebcodexResourceKind::Project,
                None,
                project.clone(),
                None,
                None,
                None,
                auth,
            )
            .await;
        if project.is_some() && !found.success {
            return found;
        }
        let projects = if found.success {
            found.output
        } else {
            json!({"items":[],"total":0,"offset":0,"limit":50,"next_offset":null,"list_truncated":true,"incomplete":"project_discovery_unavailable"})
        };
        ToolResult::ok(
            json!({"project":project,"session_id":session,"projects":projects,"selection":"caller_must_choose_project_and_session"}),
        )
    }

    // Use the canonical kernel for each exact domain observation. This includes
    // scope, Project, Runner, sensitive-path and permission checks, without a
    // Session recorder or an invented ClientWindow.
    async fn resource_observation(
        &self,
        tool: &str,
        arguments: Value,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let outcome = Box::pin(self.call_tool_with_context(
            ToolCallRequest {
                tool_name: tool.into(),
                arguments,
            },
            ToolCallContext {
                transport: ToolTransport::Api,
                session_id: None,
                auth,
                window: None,
                record_oauth_scope_denials: false,
                host_file_import_trust: HostFileImportTrust::Untrusted,
            },
        ))
        .await;
        outcome
            .result
            .unwrap_or_else(|| error("resource_access_denied"))
    }

    async fn pinned_project(
        &self,
        id: &str,
        root: &str,
        auth: Option<&AuthContext>,
    ) -> Result<String, ToolResult> {
        let resolved = self
            .resolve_project_input_for_auth(id, auth)
            .await
            .map_err(|_| error("resource_unavailable"))?;
        if resolved.resolved_id != id || resolved.root_fingerprint.as_deref() != Some(root) {
            return Err(error("resource_unavailable"));
        }
        // The issued ref forces later canonical domain dispatch to recheck the
        // same root incarnation. It stays internal, never the durable URI.
        self.project_reference_for_resolved(&resolved, auth)
            .ok_or_else(|| error("resource_identity_unavailable"))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn search_webcodex_resources(
        &self,
        kind: WebcodexResourceKind,
        query: Option<String>,
        project: Option<String>,
        session_id: Option<String>,
        offset: Option<usize>,
        limit: Option<usize>,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if query
            .as_ref()
            .is_some_and(|q| q.chars().count() > 200 || q.contains('\0'))
        {
            return error("invalid_resource_query");
        }
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50).clamp(1, 100);
        match kind {
            WebcodexResourceKind::Project => {
                if super::kernel::check_runtime_tool_scope(auth, "list_projects").is_err() {
                    return error("resource_access_denied");
                }
                let result = self
                    .list_projects_with_options_cap(
                        auth,
                        super::projects::ListProjectsOptions {
                            client_id: None,
                            project,
                            query: query.filter(|q| !q.trim().is_empty()),
                            limit: Some(SOURCE_LIMIT),
                            summary_only: true,
                        },
                        SOURCE_LIMIT,
                    )
                    .await;
                if !result.success {
                    return result;
                }
                let rows = result.output["projects"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let mut incomplete = result.output["truncated"].as_bool().unwrap_or(false);
                let mut items = Vec::new();
                for row in rows.iter().skip(offset).take(limit) {
                    let Some(id) = row["id"].as_str() else {
                        incomplete = true;
                        continue;
                    };
                    let resolved = match self.resolve_project_input_for_auth(id, auth).await {
                        Ok(r) => r,
                        Err(_) => {
                            incomplete = true;
                            continue;
                        }
                    };
                    let Some(root) = resolved.root_fingerprint.as_deref() else {
                        incomplete = true;
                        continue;
                    };
                    let meta = json!({"kind":"project","project":id,"project_ref":row["project_ref"],"connected":row["connected"],"enabled":row["enabled"],"active_jobs":row["active_jobs"]});
                    items.push(link(
                        project_uri(id, root),
                        row["name"].as_str().unwrap_or(id),
                        row["description"].as_str().unwrap_or("WebCodex project"),
                        meta,
                    ));
                }
                page(
                    items,
                    result.output["matched_count"]
                        .as_u64()
                        .unwrap_or(rows.len() as u64) as usize,
                    offset,
                    limit,
                    incomplete,
                )
            }
            WebcodexResourceKind::Goal => {
                let result = self
                    .resource_observation(
                        "list_goals",
                        json!({"query":query,"offset":offset,"limit":limit}),
                        auth,
                    )
                    .await;
                if !result.success {
                    return result;
                }
                let rows = result.output["goals"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                let items = rows.iter().filter_map(|row| {
                    let id=row["goal_id"].as_str().or_else(|| row["id"].as_str())?;
                    Some(link(goal_uri(id),row["title"].as_str().unwrap_or("Goal"),row["objective"].as_str().unwrap_or("WebCodex Goal"),json!({"kind":"goal","goal_id":id,"revision":row["revision"],"lifecycle":row["lifecycle"]})))
                }).collect();
                page(
                    items,
                    result.output["total_count"]
                        .as_u64()
                        .unwrap_or(rows.len() as u64) as usize,
                    offset,
                    limit,
                    false,
                )
            }
            WebcodexResourceKind::File | WebcodexResourceKind::Artifact => {
                let Some(project) = project else {
                    return error("resource_project_required");
                };
                if super::kernel::check_runtime_tool_scope(auth, "list_projects").is_err() {
                    return error("resource_access_denied");
                }
                let resolved = match self.resolve_project_input_for_auth(&project, auth).await {
                    Ok(r) => r,
                    Err(_) => return error("resource_unavailable"),
                };
                let Some(root) = resolved.root_fingerprint.as_deref() else {
                    return error("resource_identity_unavailable");
                };
                let Some(selector) = self.project_reference_for_resolved(&resolved, auth) else {
                    return error("resource_identity_unavailable");
                };
                if kind == WebcodexResourceKind::File {
                    let result=self.resource_observation("list_project_tracked_files",json!({"project":selector,"query":query.unwrap_or_default(),"limit":limit,"offset":offset}),auth).await;
                    if !result.success {
                        return result;
                    }
                    if self
                        .pinned_project(&resolved.resolved_id, root, auth)
                        .await
                        .is_err()
                    {
                        return error("resource_unavailable");
                    }
                    let rows = result.output["entries"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    let items=rows.iter().filter_map(|row| {let path=row["path"].as_str()?;Some(link(file_uri(&resolved.resolved_id,root,path),path,"Latest project file",json!({"kind":"file","project":resolved.resolved_id,"project_ref":selector,"path":path})))}).collect();
                    page(
                        items,
                        result.output["total_files"].as_u64().unwrap_or(0) as usize,
                        offset,
                        limit,
                        result.output["list_truncated"].as_bool().unwrap_or(false),
                    )
                } else {
                    let Some(session) = session_id else {
                        return error("resource_session_required");
                    };
                    if super::kernel::check_runtime_tool_scope(auth, "session_summary").is_err() {
                        return error("resource_access_denied");
                    }
                    let session = match self.canonicalize_explicit_session_selector(&session, auth)
                    {
                        Ok(s) => s,
                        Err(_) => return error("resource_unavailable"),
                    };
                    if self
                        .authorize_session_target(&session, "session_summary", auth)
                        .await
                        .is_err()
                    {
                        return error("resource_unavailable");
                    }
                    let Some(summary) = self.sessions.summary(&session, None) else {
                        return error("resource_unavailable");
                    };
                    if summary.project.as_deref() != Some(resolved.resolved_id.as_str()) {
                        return error("resource_unavailable");
                    }
                    let Some(outputs) = super::task_outputs::retained_task_outputs(&summary) else {
                        let mut result = page(vec![], 0, offset, limit, true);
                        result.output["incomplete"] =
                            json!("no_valid_output_manifest_in_retained_session_history");
                        return result;
                    };
                    let rows = outputs["items"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|row| {
                            query
                                .as_ref()
                                .is_none_or(|q| row["path"].as_str().is_some_and(|p| p.contains(q)))
                        })
                        .collect::<Vec<_>>();
                    let items=rows.iter().skip(offset).take(limit).filter_map(|row| {let path=row["path"].as_str()?;Some(link(file_uri(&resolved.resolved_id,root,path),path,"Recorded task output; read resolves current file",json!({"kind":"artifact","project":resolved.resolved_id,"project_ref":selector,"path":path,"provenance":{"session_id":session,"observed_at":outputs["observed_at"],"observation":row}})))}).collect();
                    page(items, rows.len(), offset, limit, false)
                }
            }
        }
    }

    pub(crate) async fn read_webcodex_resource(
        &self,
        uri: &str,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        let Some(identity) = parse_uri(uri) else {
            return error("invalid_resource_uri");
        };
        let (kind, mut observed) = match identity {
            ResourceIdentity::Goal(id) => (
                "goal",
                self.resource_observation("get_goal", json!({"goal_id":id}), auth)
                    .await,
            ),
            ResourceIdentity::Project { id, root } => {
                if super::kernel::check_runtime_tool_scope(auth, "list_projects").is_err() {
                    return error("resource_access_denied");
                }
                let _selector = match self.pinned_project(&id, &root, auth).await {
                    Ok(s) => s,
                    Err(r) => return r,
                };
                let r = self
                    .resource_observation(
                        "list_projects",
                        json!({"project":id,"summary_only":true}),
                        auth,
                    )
                    .await;
                if self.pinned_project(&id, &root, auth).await.is_err() {
                    return error("resource_unavailable");
                }
                ("project", r)
            }
            ResourceIdentity::File { id, root, path } => {
                let selector = match self.pinned_project(&id, &root, auth).await {
                    Ok(s) => s,
                    Err(r) => return r,
                };
                let metadata = self
                    .resource_observation(
                        "read_project_artifact_metadata",
                        json!({"project":selector,"path":path}),
                        auth,
                    )
                    .await;
                if !metadata.success {
                    return metadata;
                }
                let mime = metadata.output["mime_type"].as_str().unwrap_or("");
                let binary = mime.starts_with("image/")
                    || mime.starts_with("audio/")
                    || mime.starts_with("video/")
                    || matches!(
                        mime,
                        "application/pdf" | "application/zip" | "application/gzip"
                    );
                let r = if binary {
                    metadata
                } else {
                    let mut read=self.resource_observation("read_files",json!({"project":selector,"items":[{"path":path}],"max_result_bytes":24000}),auth).await;
                    if read.success {
                        let batch_truncated =
                            read.output["output_truncated"].as_bool().unwrap_or(false);
                        if let Some(item) = read.output["items"]
                            .as_array()
                            .and_then(|items| items.first())
                        {
                            let success = item["success"].as_bool().unwrap_or(
                                read.success
                                    && item.get("error").is_none()
                                    && item["output"].is_object(),
                            );
                            let error = item["error"].as_str().map(str::to_string);
                            read = ToolResult {
                                success,
                                output: item["output"].clone(),
                                error,
                            };
                            if batch_truncated {
                                read.output["output_truncated"] = json!(true);
                            }
                        } else {
                            return error("resource_read_incomplete");
                        }
                    }
                    if !read.success && read.output["reason_code"] == "invalid_utf8" {
                        self.resource_observation(
                            "read_project_artifact_metadata",
                            json!({"project":selector,"path":path}),
                            auth,
                        )
                        .await
                    } else {
                        read
                    }
                };
                if self.pinned_project(&id, &root, auth).await.is_err() {
                    return error("resource_unavailable");
                }
                ("file", r)
            }
        };
        if !observed.success {
            return observed;
        }
        let version = observed
            .output
            .get("revision")
            .cloned()
            .or_else(|| observed.output.pointer("/goal/summary/revision").cloned())
            .or_else(|| observed.output.get("read_revision").cloned())
            .or_else(|| observed.output.get("sha256").cloned());
        let mut budget_truncated = false;
        if serde_json::to_vec(&observed.output)
            .map(|v| v.len())
            .unwrap_or(usize::MAX)
            > 48 * 1024
        {
            if kind == "goal" {
                let goal = &observed.output["goal"];
                observed.output = json!({"goal":{"summary":goal["summary"],"objective":goal["objective"].as_str().unwrap_or("").chars().take(12000).collect::<String>()},"summary_only":true});
                budget_truncated = true;
            } else {
                return error("resource_read_too_large");
            }
        }
        let truncated = budget_truncated
            || [
                "truncated",
                "has_more",
                "budget_truncated",
                "output_truncated",
            ]
            .iter()
            .any(|key| observed.output[*key].as_bool() == Some(true));
        observed.output = json!({"uri":uri,"kind":kind,"version":version,"data":observed.output,"truncated":truncated,"content_policy":"latest_at_read"});
        observed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_resource_page_never_returns_a_nonadvancing_cursor() {
        let item = link(
            goal_uri("wc_goal_1234567890123456"),
            "Goal",
            "bounded",
            json!({"path":"x".repeat(60*1024)}),
        );
        let result = page(vec![item], 1, 0, 50, false);
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "resource_page_item_too_large");
        let result = page(
            vec![link("x".repeat(8193), "File", "bounded", json!({}))],
            1,
            0,
            50,
            false,
        );
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "resource_identity_too_large");
    }
    #[test]
    fn resource_identity_is_canonical_and_pins_root_and_path() {
        let uri = file_uri("agent:runner:project", "root-one", "src/中文 %_.rs");
        assert_eq!(
            parse_uri(&uri),
            Some(ResourceIdentity::File {
                id: "agent:runner:project".into(),
                root: "root-one".into(),
                path: "src/中文 %_.rs".into()
            })
        );
        assert_ne!(
            uri,
            file_uri("agent:runner:project", "root-two", "src/中文 %_.rs")
        );
        for invalid in [
            format!("{uri}/extra"),
            file_uri("a", "b", "../secret"),
            format!("{RESOURCE_PREFIX}goal/eA=="),
            "webcodex-resource://file/".into(),
        ] {
            assert!(parse_uri(&invalid).is_none(), "{invalid}");
        }
    }
}
