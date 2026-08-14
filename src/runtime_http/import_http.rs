use super::{parse_json_body, render_result, require_runtime};
use crate::action_audit::ActionAudit;
use crate::tool_runtime::conversation_import::{
    ConversationImportDownloadPolicy, ImportConversationFilesInput, OpenAiFileIdRef,
};
use crate::tool_runtime::sessions::SessionTransport;
use salvo::prelude::*;
use serde::Deserialize;

#[cfg(test)]
pub(super) use crate::tool_runtime::conversation_import::{
    set_import_test_download_base_url, MAX_IMPORT_FILE_BYTES,
};

#[derive(Debug, Deserialize)]
struct ImportConversationFilesRequest {
    #[serde(rename = "openaiFileIdRefs")]
    openai_file_id_refs: Vec<OpenAiFileIdRef>,
    project: String,
    #[serde(default)]
    output_dir: Option<String>,
    #[serde(default)]
    targets: Option<Vec<String>>,
    #[serde(default)]
    overwrite: Option<bool>,
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
    let Some(runtime) = require_runtime(depot, res) else {
        return;
    };
    let Some(body) = parse_json_body::<ImportConversationFilesRequest>(req, res).await else {
        return;
    };
    let auth = depot.obtain::<crate::auth::AuthContext>().ok().cloned();
    let project = body.project.clone();
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
            SessionTransport::Api,
            ConversationImportDownloadPolicy::GptActionOpenAiHost,
        )
        .await;
    render_result(
        res,
        &audit,
        "import_conversation_files",
        Some(project),
        result,
    );
}
