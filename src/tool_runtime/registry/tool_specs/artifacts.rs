use super::super::input_schemas::{
    artifact_upload_abort_input_schema, artifact_upload_begin_input_schema,
    artifact_upload_chunk_input_schema, artifact_upload_finish_input_schema,
    export_project_artifact_input_schema, import_conversation_files_to_project_input_schema,
    read_project_artifact_input_schema, read_project_artifact_metadata_input_schema,
    save_project_artifact_input_schema,
};
use super::tool_spec;
use crate::tool_runtime::tool_spec::ToolSpec;

pub(super) fn tool_specs() -> Vec<ToolSpec> {
    vec![
        tool_spec(
            "save_project_artifact",
            "Write a bounded binary project artifact from base64. Use for imported session files, generated images, PDFs, ZIP archives, and DOCX/PPTX/XLSX Office artifacts; not for UTF-8 source edits.",
            save_project_artifact_input_schema(),
        ),
        tool_spec(
            "import_conversation_files_to_project",
            "Import 1..10 current ChatGPT attachments into an agent project using openaiFileIdRefs from the host file-reference mechanism. Do not base64-transfer files, construct download URLs, or use local /mnt/data paths; Control downloads and saves them as project artifacts.",
            import_conversation_files_to_project_input_schema(),
        ),
        tool_spec(
            "export_project_artifact",
            "Create one short-lived authenticated MCP resource link for a bounded project artifact. The tool result contains metadata only; use resources/read on the returned ResourceLink to fetch the complete binary without routing base64 through model output.",
            export_project_artifact_input_schema(),
        ),
        tool_spec(
            "read_project_artifact_metadata",
            "Read bounded metadata for a binary artifact; images include dimensions and zip archives are counted but never extracted. Set allow_missing=true to make a missing artifact a successful exists=false negative assertion.",
            read_project_artifact_metadata_input_schema(),
        ),
        tool_spec(
            "read_project_artifact",
            "Chunked content read for a project artifact. Returns base64 for one small segment plus full-file sha256/MIME metadata; not a large-file transfer tool.",
            read_project_artifact_input_schema(),
        ),
        tool_spec(
            "artifact_upload_begin",
            "Begin a bounded chunked binary artifact upload up to 256 MiB. Creates a project-local temporary upload session; finish commits atomically to the target path. For smoke octet-stream uploads, use artifacts/smoke/<name>.artifact or omit mime_type when appropriate.",
            artifact_upload_begin_input_schema(),
        ),
        tool_spec(
            "artifact_upload_chunk",
            "Append one base64 chunk up to 1 MiB decoded to an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id to the target path.",
            artifact_upload_chunk_input_schema(),
        ),
        tool_spec(
            "artifact_upload_finish",
            "Finish an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before atomic commit.",
            artifact_upload_finish_input_schema(),
        ),
        tool_spec(
            "artifact_upload_abort",
            "Abort an active artifact upload. path is required and must exactly match artifact_upload_begin; this binds upload_id before cleanup and reports final_file_exists without touching the final target.",
            artifact_upload_abort_input_schema(),
        ),
    ]
}
