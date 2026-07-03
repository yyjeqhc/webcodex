pub(crate) const SAFE_OCTET_STREAM_ARTIFACT_EXTENSIONS: &[&str] = &[
    ".artifact",
    ".dat",
    ".txt",
    ".csv",
    ".json",
    ".png",
    ".jpg",
    ".jpeg",
    ".webp",
    ".pdf",
    ".zip",
];

pub(crate) fn has_safe_octet_stream_artifact_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    SAFE_OCTET_STREAM_ARTIFACT_EXTENSIONS
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

pub(crate) fn safe_octet_stream_artifact_extensions_csv() -> String {
    SAFE_OCTET_STREAM_ARTIFACT_EXTENSIONS.join(", ")
}

pub(crate) fn octet_stream_safe_extension_error() -> String {
    format!(
        "application/octet-stream is only allowed for safe artifact extensions: {}. \
         For smoke tests, use artifacts/smoke/<name>.artifact or \
         artifacts/smoke/<name>.txt, or omit mime_type when appropriate.",
        safe_octet_stream_artifact_extensions_csv()
    )
}
