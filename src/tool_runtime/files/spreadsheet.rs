//! Spreadsheet source observation; rendering remains in the optional MCP adapter.
use super::*;

const MAX_SPREADSHEET_BYTES: usize = 5 * 1024 * 1024;

impl ToolRuntime {
    pub(crate) async fn present_spreadsheet(
        &self,
        resolved: &ResolvedProject,
        path: String,
        auth: Option<&AuthContext>,
    ) -> ToolResult {
        if let Err(error) = validate_artifact_file_path(&path) {
            return artifacts::artifact_policy_rejected_result(&path, error);
        }
        if path.len() > 512 {
            return artifacts::artifact_policy_rejected_result(
                &path,
                "Spreadsheet path exceeds 512 bytes".into(),
            );
        }
        let extension = std::path::Path::new(&path)
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase);
        if !matches!(extension.as_deref(), Some("csv" | "tsv" | "xlsx")) {
            return ToolResult::err_with_output(
                "present_spreadsheet supports CSV, TSV and XLSX files",
                json!({"error_kind": "unsupported_spreadsheet_format"}),
            );
        }
        let result = self
            .export_project_artifact_metadata_resolved_with_limit(
                resolved,
                path,
                auth,
                MAX_SPREADSHEET_BYTES,
            )
            .await;
        if result.success
            && result.output["bytes"]
                .as_u64()
                .is_none_or(|bytes| bytes > MAX_SPREADSHEET_BYTES as u64)
        {
            return ToolResult::err_with_output(
                "Spreadsheet exceeds the 5 MiB reader limit",
                json!({"error_kind": "spreadsheet_size_limit"}),
            );
        }
        result
    }
}
