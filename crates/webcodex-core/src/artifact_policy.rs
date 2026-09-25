pub const GENERIC_BINARY_MIME: &str = "application/octet-stream";
pub const DOCX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
pub const PPTX_MIME: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.presentation";
pub const XLSX_MIME: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";

#[derive(Debug, Clone, Copy)]
struct ArtifactFileType {
    extensions: &'static [&'static str],
    canonical_extension: &'static str,
    mime: &'static str,
    textual: bool,
}

const ARTIFACT_FILE_TYPES: &[ArtifactFileType] = &[
    ArtifactFileType {
        extensions: &[".txt", ".log"],
        canonical_extension: ".txt",
        mime: "text/plain",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".md", ".markdown"],
        canonical_extension: ".md",
        mime: "text/markdown",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".csv"],
        canonical_extension: ".csv",
        mime: "text/csv",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".json"],
        canonical_extension: ".json",
        mime: "application/json",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".yaml", ".yml"],
        canonical_extension: ".yaml",
        mime: "application/yaml",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".toml"],
        canonical_extension: ".toml",
        mime: "application/toml",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".xml"],
        canonical_extension: ".xml",
        mime: "application/xml",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".html", ".htm"],
        canonical_extension: ".html",
        mime: "text/html",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".rst"],
        canonical_extension: ".rst",
        mime: "text/x-rst",
        textual: true,
    },
    ArtifactFileType {
        extensions: &[".png"],
        canonical_extension: ".png",
        mime: "image/png",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".jpg", ".jpeg"],
        canonical_extension: ".jpg",
        mime: "image/jpeg",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".webp"],
        canonical_extension: ".webp",
        mime: "image/webp",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".mp3"],
        canonical_extension: ".mp3",
        mime: "audio/mpeg",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".mp4"],
        canonical_extension: ".mp4",
        mime: "video/mp4",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".pdf"],
        canonical_extension: ".pdf",
        mime: "application/pdf",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".zip"],
        canonical_extension: ".zip",
        mime: "application/zip",
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".docx"],
        canonical_extension: ".docx",
        mime: DOCX_MIME,
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".pptx"],
        canonical_extension: ".pptx",
        mime: PPTX_MIME,
        textual: false,
    },
    ArtifactFileType {
        extensions: &[".xlsx"],
        canonical_extension: ".xlsx",
        mime: XLSX_MIME,
        textual: false,
    },
];

fn mime_essence(mime: &str) -> String {
    mime.split(';')
        .next()
        .unwrap_or(mime)
        .trim()
        .to_ascii_lowercase()
}

fn file_type_for_mime(mime: &str) -> Option<&'static ArtifactFileType> {
    let mime = mime_essence(mime);
    ARTIFACT_FILE_TYPES
        .iter()
        .find(|file_type| file_type.mime.eq_ignore_ascii_case(&mime))
}

pub fn preferred_mime_for_path(path: &str) -> Option<&'static str> {
    let lower = path.to_ascii_lowercase();
    ARTIFACT_FILE_TYPES
        .iter()
        .find(|file_type| {
            file_type
                .extensions
                .iter()
                .any(|extension| lower.ends_with(extension))
        })
        .map(|file_type| file_type.mime)
}

pub fn canonical_extension_for_mime(mime: &str) -> Option<&'static str> {
    file_type_for_mime(mime).map(|file_type| file_type.canonical_extension)
}

pub fn canonical_known_mime(mime: &str) -> Option<&'static str> {
    let essence = mime_essence(mime);
    if essence == GENERIC_BINARY_MIME {
        return Some(GENERIC_BINARY_MIME);
    }
    file_type_for_mime(&essence).map(|file_type| file_type.mime)
}

pub fn is_known_textual_mime(mime: &str) -> bool {
    file_type_for_mime(mime).is_some_and(|file_type| file_type.textual)
}

pub fn is_known_textual_path(path: &str) -> bool {
    preferred_mime_for_path(path).is_some_and(is_known_textual_mime)
}

/// Normalize host-provided presentation metadata without treating MIME as an
/// authorization or file-safety boundary. Known MIME values are canonicalized;
/// generic or unknown values fall back to the path's known type, then to
/// application/octet-stream.
pub fn normalize_host_import_presentation_mime(path: &str, reported_mime: Option<&str>) -> String {
    if let Some(canonical) = reported_mime.and_then(canonical_known_mime) {
        if canonical != GENERIC_BINARY_MIME {
            return canonical.to_string();
        }
    }
    preferred_mime_for_path(path)
        .unwrap_or(GENERIC_BINARY_MIME)
        .to_string()
}

/// Produce stable export presentation metadata. Content-based detection wins
/// when it resolves to a known artifact type; otherwise use the path policy and
/// finally the generic binary fallback.
pub fn export_presentation_mime(path: &str, detected_mime: Option<&str>) -> String {
    if let Some(canonical) = detected_mime.and_then(canonical_known_mime) {
        if canonical != GENERIC_BINARY_MIME {
            return canonical.to_string();
        }
    }
    preferred_mime_for_path(path)
        .unwrap_or(GENERIC_BINARY_MIME)
        .to_string()
}

pub fn ooxml_extension_for_mime(mime: &str) -> Option<&'static str> {
    match canonical_known_mime(mime) {
        Some(DOCX_MIME) => Some(".docx"),
        Some(PPTX_MIME) => Some(".pptx"),
        Some(XLSX_MIME) => Some(".xlsx"),
        _ => None,
    }
}

/// Only OOXML presentation metadata has an extension/content-family
/// requirement. Other MIME values are descriptive metadata, not a safety gate.
pub fn mime_is_compatible_with_path(mime: &str, path: &str) -> bool {
    let Some(required_extension) = ooxml_extension_for_mime(mime) else {
        return true;
    };
    path.to_ascii_lowercase().ends_with(required_extension)
}

/// Maximum decoded image size returned as native MCP image content.
///
/// Native images are JSON/base64 encoded on the Runner path. Four decoded MiB
/// remain comfortably inside the shared 8 MiB Runner transport envelope after
/// base64 expansion plus bounded metadata. Polling result ingestion gets that
/// same narrow Runner-only allowance without broadening the server's ordinary
/// 2 MiB HTTP text/body limit.
pub const MAX_MCP_IMAGE_BYTES: usize = 4 * 1024 * 1024;

/// Maximum runner stdout retained for an MCP image artifact response.
///
/// Normal runner output remains capped at 256 KiB. Six MiB covers a maximum
/// native image's base64 plus its bounded artifact metadata while staying below
/// the shared Runner transport envelope.
pub const MAX_MCP_IMAGE_RESPONSE_BYTES: usize = 6 * 1024 * 1024;

/// Maximum retained Runner stdout for one internal 1 MiB artifact transfer
/// chunk encoded as base64 JSON. This is an internal transport envelope, not a
/// model-facing artifact inspection bound.
pub const MAX_INTERNAL_ARTIFACT_CHUNK_RESPONSE_BYTES: usize = 1536 * 1024;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_text_types_use_one_canonical_policy() {
        let cases = [
            ("README.md", "text/markdown"),
            ("README.markdown", "text/markdown"),
            ("notes.txt", "text/plain"),
            ("rows.csv", "text/csv"),
            ("data.json", "application/json"),
            ("config.yaml", "application/yaml"),
            ("config.yml", "application/yaml"),
            ("Cargo.toml", "application/toml"),
            ("doc.xml", "application/xml"),
            ("index.html", "text/html"),
            ("index.htm", "text/html"),
            ("server.log", "text/plain"),
            ("guide.rst", "text/x-rst"),
        ];
        for (path, mime) in cases {
            assert_eq!(preferred_mime_for_path(path), Some(mime), "{path}");
            assert!(is_known_textual_path(path), "{path}");
            assert!(is_known_textual_mime(mime), "{mime}");
        }
    }

    #[test]
    fn host_and_export_mime_fall_back_to_generic_binary() {
        assert_eq!(
            normalize_host_import_presentation_mime("data.customblob", Some("weird/vendor")),
            GENERIC_BINARY_MIME
        );
        assert_eq!(
            export_presentation_mime("data.customblob", None),
            GENERIC_BINARY_MIME
        );
        assert_eq!(
            normalize_host_import_presentation_mime("README.md", Some(GENERIC_BINARY_MIME)),
            "text/markdown"
        );
        assert_eq!(export_presentation_mime("README.md", None), "text/markdown");
    }

    #[test]
    fn mime_normalization_handles_parameters_and_canonical_extensions() {
        assert_eq!(
            canonical_known_mime("Text/Markdown; charset=utf-8"),
            Some("text/markdown")
        );
        assert_eq!(canonical_extension_for_mime("text/markdown"), Some(".md"));
        assert_eq!(
            canonical_extension_for_mime("application/yaml"),
            Some(".yaml")
        );
    }

    #[test]
    fn ooxml_keeps_matching_extension_requirement() {
        assert!(mime_is_compatible_with_path(DOCX_MIME, "paper.docx"));
        assert!(!mime_is_compatible_with_path(DOCX_MIME, "paper.bin"));
        assert!(mime_is_compatible_with_path(
            GENERIC_BINARY_MIME,
            "paper.custom"
        ));
    }
}
