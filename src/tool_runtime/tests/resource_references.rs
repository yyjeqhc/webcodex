//! Resource references remain locators: every discovery and read rechecks authority.
use super::support::*;
use crate::auth::{AuthContext, SCOPE_COMMUNICATION_READ, SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ};
use crate::runner_protocol::RunnerCapabilities;
use crate::tool_runtime::session_context::workflow_session_authority_fingerprint;
use crate::tool_runtime::sessions::{SessionCreateOptions, SessionTransport};
use crate::tool_runtime::{ToolResult, ToolRuntime};
use serde_json::{json, Value};
use std::sync::Arc;
use webcodex_tool_contracts::tool_call::WebcodexResourceKind;

fn fixture() -> (tempfile::TempDir, ToolRuntime) {
    let temp = tempfile::tempdir().unwrap();
    let db = Arc::new(crate::Database::open(&temp.path().join("resources.db")).unwrap());
    let runtime = ToolRuntime::new_for_tests()
        .with_communication_database(db.clone())
        .with_project_reference_database(db);
    (temp, runtime)
}

fn reader(name: &str, scopes: &[&str]) -> AuthContext {
    let mut auth = auth_context(Some(name), false);
    auth.scopes = scopes.iter().map(|scope| (*scope).to_string()).collect();
    auth
}

async fn search(runtime: &ToolRuntime, kind: WebcodexResourceKind, project: Option<&str>, session: Option<&str>, auth: &AuthContext) -> ToolResult {
    runtime.search_webcodex_resources(kind, None, project.map(str::to_string), session.map(str::to_string), None, None, Some(auth)).await
}

fn first_uri(result: &ToolResult) -> String {
    assert!(result.success, "{:?}", result.error);
    result.output["items"][0]["uri"].as_str().unwrap().to_string()
}

async fn register(runtime: &ToolRuntime, name: &str, root: char) -> String {
    let mut project = named_registered_project("resources", name, name, &format!("/srv/{name}"), 1);
    project.root_fingerprint = Some(format!("wc_projroot_{}", root.to_string().repeat(64)));
    register_agent_projects(runtime, "resources", None, RunnerCapabilities::default(), vec![project]).await;
    format!("agent:resources:{name}")
}

fn session(runtime: &ToolRuntime, project: &str, auth: &AuthContext) -> String {
    runtime.sessions.start_session_with_options(
        SessionCreateOptions::new(Some(project.into()), Some("Resource outputs".into()), crate::tool_runtime::SessionMode::Normal, Default::default())
            .with_owner_authority_fingerprint(Some(workflow_session_authority_fingerprint(Some(auth)).unwrap())),
    ).unwrap().session_id
}

fn finish(runtime: &ToolRuntime, project: &str, session: &str, output: Value) {
    let start = runtime.sessions.record_tool_call_started_with_options(
        Some(session), SessionTransport::Api, "finish_coding_task", &json!({"project":project,"session_id":session}), Some(project.into()),
        crate::tool_runtime::sessions::session_tool_contract("finish_coding_task"),
    );
    runtime.sessions.record_tool_call_finished(start, true, &output, None, None);
}

#[tokio::test]
async fn goal_reference_reads_latest_owned_goal_and_hides_foreign_content() {
    let (_temp, runtime) = fixture();
    let owner = reader("owner", &[SCOPE_COMMUNICATION_READ]);
    let foreign = reader("foreign", &[SCOPE_COMMUNICATION_READ]);
    let created = runtime.create_goal(Some(&owner), "Original title".into(), "Unicode 安全 %_ objective".into(), "resource-goal".into());
    assert!(created.success);
    let listed = runtime.search_webcodex_resources(WebcodexResourceKind::Goal, Some("安全 %_".into()), None, None, None, None, Some(&owner)).await;
    let uri = first_uri(&listed);
    let foreign_list = search(&runtime, WebcodexResourceKind::Goal, None, None, &foreign).await;
    assert!(foreign_list.success);
    assert_eq!(foreign_list.output["total"], 0);
    let denied = runtime.read_webcodex_resource(&uri, Some(&foreign)).await;
    assert!(!denied.success);
    assert!(!denied.output.to_string().contains("Original title"));

    let principal = crate::tool_runtime::communication::communication_principal(Some(&owner)).unwrap();
    let id = created.output["goal"]["summary"]["goal_id"].as_str().unwrap();
    runtime.communication_db.as_ref().unwrap().update_goal(&principal, id, 1, crate::db::GoalPatch {
        title: Some("Latest title".into()), objective: None, controller_agent_id: None, lifecycle: None, terminal_reason: None,
    }, "resource-update").unwrap();
    let current = runtime.read_webcodex_resource(&uri, Some(&owner)).await;
    assert!(current.success, "{:?}", current.error);
    assert!(current.output["data"].to_string().contains("Latest title"));
    assert!(!current.output["data"].to_string().contains("Original title"));
    assert_eq!(current.output["content_policy"], "latest_at_read");
    assert_eq!(current.output["version"],2);
    let no_scope = reader("owner", &[SCOPE_PROJECT_READ]);
    assert!(!runtime.read_webcodex_resource(&uri, Some(&no_scope)).await.success);
    assert!(!search(&runtime, WebcodexResourceKind::Goal, None, None, &no_scope).await.success);
    // Goal discovery does not demand unrelated project/runtime scopes.
    assert!(search(&runtime, WebcodexResourceKind::Goal, None, None, &owner).await.success);
}

#[tokio::test]
async fn project_references_reauthorize_reader_and_reject_replaced_root() {
    let (_temp, runtime) = fixture();
    let project = register(&runtime, "demo", '1').await;
    let admin = auth_context(None, true);
    let uri = first_uri(&search(&runtime, WebcodexResourceKind::Project, Some(&project), None, &admin).await);
    assert!(runtime.read_webcodex_resource(&uri, Some(&admin)).await.success);
    let foreign = reader("foreign", &[SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ]);
    assert!(!runtime.read_webcodex_resource(&uri, Some(&foreign)).await.success);
    let mut no_project_scope = admin.clone();
    no_project_scope.is_bootstrap = false;
    no_project_scope.scopes = vec![SCOPE_COMMUNICATION_READ.into()];
    assert!(!search(&runtime, WebcodexResourceKind::Project, Some(&project), None, &no_project_scope).await.success);
    register(&runtime, "demo", '2').await;
    let stale = runtime.read_webcodex_resource(&uri, Some(&admin)).await;
    assert!(!stale.success);
    assert_eq!(stale.output["error_kind"], "resource_unavailable");
    let new_uri = first_uri(&search(&runtime, WebcodexResourceKind::Project, Some(&project), None, &admin).await);
    assert_ne!(uri, new_uri);
}

#[tokio::test]
async fn artifact_discovery_uses_exact_session_latest_manifest_and_historical_provenance() {
    let (_temp, runtime) = fixture();
    let project = register(&runtime, "demo", '1').await;
    let auth = auth_context(None, true);
    let selected = session(&runtime, &project, &auth);
    let output = |path: &str| json!({"task_outputs":{"items":[{"path":path,"status":"verified","file_bytes":7,"sha256":"a".repeat(64),"mime_type":"text/plain"}],"verified_count":1,"missing_count":0,"unavailable_count":0,"observed_at":123}});
    finish(&runtime, &project, &selected, output("old.txt"));
    finish(&runtime, &project, &selected, output("latest.txt"));
    let listed = search(&runtime, WebcodexResourceKind::Artifact, Some(&project), Some(&selected), &auth).await;
    let uri = first_uri(&listed);
    assert_eq!(listed.output["total"], 1);
    assert_eq!(listed.output["items"][0]["_meta"]["path"], "latest.txt");
    assert_eq!(listed.output["items"][0]["_meta"]["provenance"]["observed_at"], 123);
    assert!(!uri.contains(&"a".repeat(64)), "historical SHA must not pin current file identity");
    let other_project_session = session(&runtime, "agent:resources:other", &auth);
    let mismatch = search(&runtime, WebcodexResourceKind::Artifact, Some(&project), Some(&other_project_session), &auth).await;
    assert!(!mismatch.success);
    assert!(!search(&runtime, WebcodexResourceKind::Artifact, Some(&project), None, &auth).await.success);
    let foreign_session = session(&runtime, &project, &reader("other", &[SCOPE_PROJECT_READ, SCOPE_RUNTIME_READ]));
    assert!(!search(&runtime, WebcodexResourceKind::Artifact, Some(&project), Some(&foreign_session), &auth).await.success);
    finish(&runtime, &project, &selected, json!({}));
    let invalidated = search(&runtime, WebcodexResourceKind::Artifact, Some(&project), Some(&selected), &auth).await;
    assert!(invalidated.success);
    assert_eq!(invalidated.output["items"], json!([]));
    assert!(invalidated.output["incomplete"].is_string());
}

#[tokio::test]
async fn resource_read_rejects_malformed_and_traversal_locators_without_dispatch() {
    let (_temp, runtime) = fixture();
    let auth = auth_context(None, true);
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let traversal = format!("webcodex-resource://file/{}/{}/{}", URL_SAFE_NO_PAD.encode("agent:resources:demo"), URL_SAFE_NO_PAD.encode("root"), URL_SAFE_NO_PAD.encode("../secret"));
    for uri in ["webcodex-resource://goal/eA==", "webcodex-resource://file/", traversal.as_str()] {
        let result = runtime.read_webcodex_resource(uri, Some(&auth)).await;
        assert!(!result.success);
        assert_eq!(result.output["error_kind"], "invalid_resource_uri");
    }
}

#[tokio::test]
async fn file_reference_reads_new_runner_content_and_reports_deletion() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let (_temp, runtime) = fixture();
    let mut project = named_registered_project("resources", "demo", "demo", "/srv/demo", 1);
    let root = format!("wc_projroot_{}", "1".repeat(64));
    project.root_fingerprint = Some(root.clone());
    register_agent_projects(&runtime, "resources", None, RunnerCapabilities {file_read:true, ..Default::default()}, vec![project]).await;
    let uri = format!("webcodex-resource://file/{}/{}/{}", URL_SAFE_NO_PAD.encode("agent:resources:demo"), URL_SAFE_NO_PAD.encode(&root), URL_SAFE_NO_PAD.encode("result.txt"));
    for text in ["first version\n", "latest version\n"] {
        let task = tokio::spawn({
            let runtime = runtime.clone();
            let uri = uri.clone();
            async move {runtime.read_webcodex_resource(&uri, Some(&auth_context(None,true))).await}
        });
        let metadata = wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
        assert_eq!(metadata.kind,"file_read_project_artifact_metadata");
        complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&metadata.request_id,0,&json!({"path":"result.txt","exists":true,"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"mime_type":"text/plain"}).to_string(),"").await;
        let read = wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
        assert_eq!(read.path.as_deref(),Some("result.txt"));
        let start = read.start_line.unwrap();
        let limit = read.end_line.unwrap().saturating_sub(start).saturating_add(1);
        let response = canonical_agent_file_read_range(text,start,limit);
        complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&read.request_id,0,&response,"").await;
        let result = task.await.unwrap();
        assert!(result.success,"{:?}",result.error);
        assert!(result.output["data"].to_string().contains(text.trim()));
        assert_eq!(result.output["content_policy"],"latest_at_read");
    }
    let task = tokio::spawn({
        let runtime=runtime.clone();
        let uri=uri.clone();
        async move {runtime.read_webcodex_resource(&uri,Some(&auth_context(None,true))).await}
    });
    let metadata=wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
    complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&metadata.request_id,0,&json!({"path":"result.txt","exists":false,"missing":true,"error":"File no longer exists"}).to_string(),"").await;
    let result=task.await.unwrap();
    assert!(!result.success);
    assert!(!result.output.to_string().contains("latest version"));
}

#[tokio::test]
async fn file_reference_marks_default_line_range_as_partial() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};
    let (_temp, runtime) = fixture();
    let mut project = named_registered_project("resources", "demo", "demo", "/srv/demo", 1);
    let root = format!("wc_projroot_{}", "1".repeat(64));
    project.root_fingerprint = Some(root.clone());
    register_agent_projects(&runtime,"resources",None,RunnerCapabilities {file_read:true,..Default::default()},vec![project]).await;
    let uri=format!("webcodex-resource://file/{}/{}/{}",URL_SAFE_NO_PAD.encode("agent:resources:demo"),URL_SAFE_NO_PAD.encode(&root),URL_SAFE_NO_PAD.encode("long.txt"));
    let task=tokio::spawn({let runtime=runtime.clone();async move {runtime.read_webcodex_resource(&uri,Some(&auth_context(None,true))).await}});
    let total_lines=webcodex_core::runtime_contract::FILE_READ_DEFAULT_LIMIT + 5;
    let text=(0..total_lines).map(|i|format!("line {i}\n")).collect::<String>();
    let metadata=wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
    complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&metadata.request_id,0,&json!({"path":"long.txt","exists":true,"bytes":text.len(),"sha256":format!("{:x}",Sha256::digest(text.as_bytes())),"mime_type":"text/plain"}).to_string(),"").await;
    let read=wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
    let start=read.start_line.unwrap();
    let limit=read.end_line.unwrap().saturating_sub(start).saturating_add(1);
    assert_eq!(limit,webcodex_core::runtime_contract::FILE_READ_DEFAULT_LIMIT);
    assert!(limit<total_lines);
    complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&read.request_id,0,&canonical_agent_file_read_range(&text,start,limit),"").await;
    let result=task.await.unwrap();
    assert!(result.success,"{:?}",result.error);
    assert_eq!(result.output["data"]["has_more"],true);
    assert_eq!(result.output["truncated"],true,"bounded current text must never be presented as a complete file");
}

#[tokio::test]
async fn binary_reference_returns_current_metadata_without_text_or_blob_transfer() {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let (_temp,runtime)=fixture();
    let mut project=named_registered_project("resources","demo","demo","/srv/demo",1);
    let root=format!("wc_projroot_{}","1".repeat(64));
    project.root_fingerprint=Some(root.clone());
    register_agent_projects(&runtime,"resources",None,RunnerCapabilities {file_read:true,..Default::default()},vec![project]).await;
    let uri=format!("webcodex-resource://file/{}/{}/{}",URL_SAFE_NO_PAD.encode("agent:resources:demo"),URL_SAFE_NO_PAD.encode(&root),URL_SAFE_NO_PAD.encode("result.pdf"));
    let task=tokio::spawn({let runtime=runtime.clone();async move {runtime.read_webcodex_resource(&uri,Some(&auth_context(None,true))).await}});
    let metadata=wait_for_runner_request_for_instance_with_timeout(&runtime,"resources","inst-resources",std::time::Duration::from_secs(10)).await;
    assert_eq!(metadata.kind,"file_read_project_artifact_metadata");
    complete_patch_agent_request_for_instance(&runtime,"resources","inst-resources",&metadata.request_id,0,&json!({"path":"result.pdf","exists":true,"bytes":512,"sha256":"b".repeat(64),"mime_type":"application/pdf"}).to_string(),"").await;
    let result=tokio::time::timeout(std::time::Duration::from_secs(2),task).await.expect("binary metadata must complete without scheduling a text read").unwrap();
    assert!(result.success,"{:?}",result.error);
    assert_eq!(result.output["data"]["mime_type"],"application/pdf");
    assert_eq!(result.output["data"]["bytes"],512);
    assert_eq!(result.output["version"],"b".repeat(64));
    assert!(result.output["data"].get("text").is_none());
    assert!(result.output["data"].get("blob").is_none());
    assert_eq!(result.output["content_policy"],"latest_at_read");
}
