//! Test-only adapter for the streaming import domain. Never mounted by production.
//! This fixture supplies a trusted MCP origin; production provenance remains
//! adapter-derived and cannot be supplied through /api/tools/call business JSON.
use crate::runtime_http::{parse_json_body, require_runtime};
use crate::tool_runtime::conversation_import::{
    ConversationImportDownloadPolicy, ImportConversationFilesInput, OpenAiFileIdRef,
};
use crate::tool_runtime::sessions::SessionTransport;
use salvo::prelude::*;
use serde::Deserialize;
use std::sync::Arc;

pub(super) fn router(
    config: Arc<crate::Config>,
    db: Arc<crate::Database>,
    runtime: Arc<crate::tool_runtime::ToolRuntime>,
) -> Router {
    Router::new()
        .hoop(affix_state::inject(config))
        .hoop(affix_state::inject(db))
        .hoop(affix_state::inject(runtime))
        .push(
            Router::with_path("test-host-file-import")
                .hoop(crate::AuthMiddleware)
                .post(import),
        )
}
#[derive(Deserialize)]
struct Input {
    #[serde(rename = "openaiFileIdRefs")]
    openai_file_id_refs: Vec<OpenAiFileIdRef>,
    project: String,
    output_dir: Option<String>,
    targets: Option<Vec<String>>,
    overwrite: Option<bool>,
}
#[handler]
async fn import(req: &mut Request, depot: &mut Depot, res: &mut Response) {
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    let Some(body) = parse_json_body::<Input>(req, res).await else {
        return;
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let result = runtime
        .import_conversation_files(
            ImportConversationFilesInput {
                openai_file_id_refs: body.openai_file_id_refs,
                project: body.project,
                output_dir: body.output_dir,
                targets: body.targets,
                overwrite: body.overwrite,
                session_id: None,
            },
            auth.as_ref(),
            SessionTransport::Mcp,
            ConversationImportDownloadPolicy::AuthenticatedMcpOpenAiHostFile,
        )
        .await;
    res.status_code(if result.success {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    });
    res.render(Json(result));
}
