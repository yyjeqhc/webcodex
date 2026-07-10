//! Agent-side read-only LSP navigation operations.
//!
//! Resolves project roots under policy, talks to `LspSupervisor`, normalizes
//! locations to project-relative paths, and never returns absolute paths,
//! file URIs, or executable paths to the model.

use super::super::config::AgentPolicy;
use super::super::output::CommandResult;
use super::super::projects::load_agent_project_summaries_from_dir;
use super::super::shell::cwd_allowed;
use super::position::{lsp_to_public, public_to_lsp, LineCache, MAX_LSP_DOCUMENT_BYTES};
use super::supervisor::{
    classify_uri_against_project_root, LspError, LspServerKind, LspServerStatus, LspSupervisor,
    PositionEncoding, ProjectUriClassification,
};
use crate::lsp_bridge::{
    bound_error_message, error_codes, AgentLspPayload, AgentLspRequest, AgentLspResultEnvelope,
    DocumentSymbolsResult, LocationsResult, LspAvailabilityStatus, LspServerStatusEntry,
    LspStatusResult, PublicLocation, PublicPosition, PublicRange, PublicSymbol,
    AGENT_LSP_REQUEST_KIND,
};
use crate::shell_protocol::ShellAgentShellRequest;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use url::Url;

const MAX_SYMBOL_NAME_CHARS: usize = 256;
const MAX_SYMBOL_DETAIL_CHARS: usize = 512;

pub(crate) fn is_lsp_request_kind(kind: &str) -> bool {
    kind == AGENT_LSP_REQUEST_KIND
}

pub(crate) fn handle_lsp_request(
    policy: &AgentPolicy,
    projects_dir: &Path,
    supervisor: &LspSupervisor,
    request: &ShellAgentShellRequest,
) -> CommandResult {
    let start = Instant::now();
    let Some(payload) = request.lsp.as_ref() else {
        return lsp_error_cmd(
            start,
            error_codes::MISSING_LSP_PAYLOAD,
            "LSP request missing typed payload",
        );
    };
    match execute_lsp(policy, projects_dir, supervisor, payload) {
        Ok(envelope) => CommandResult {
            // Always exit 0 for structured envelopes so the server can parse
            // success/failure from the versioned JSON rather than shell status.
            exit_code: Some(0),
            stdout: Some(envelope.to_stdout_json()),
            stderr: Some(String::new()),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
        Err(envelope) => CommandResult {
            exit_code: Some(0),
            stdout: Some(envelope.to_stdout_json()),
            stderr: Some(String::new()),
            duration_ms: Some(start.elapsed().as_millis() as u64),
            error: None,
        },
    }
}

fn execute_lsp(
    policy: &AgentPolicy,
    projects_dir: &Path,
    supervisor: &LspSupervisor,
    payload: &AgentLspPayload,
) -> Result<AgentLspResultEnvelope, AgentLspResultEnvelope> {
    let project = resolve_agent_project(projects_dir, &payload.project_id)?;
    let project_root = validate_project_root(policy, &project.path)?;
    match &payload.request {
        AgentLspRequest::Status => Ok(AgentLspResultEnvelope::ok(lsp_status(
            &payload.project_id,
            &project_root,
            supervisor,
        ))),
        AgentLspRequest::DocumentSymbols { path, limit } => {
            let result =
                document_symbols(&payload.project_id, &project_root, supervisor, path, *limit)?;
            Ok(AgentLspResultEnvelope::ok(result))
        }
        AgentLspRequest::GotoDefinition {
            path,
            line,
            column,
            limit,
        } => {
            let result = goto_definition(
                &payload.project_id,
                &project_root,
                supervisor,
                path,
                *line,
                *column,
                *limit,
            )?;
            Ok(AgentLspResultEnvelope::ok(result))
        }
        AgentLspRequest::FindReferences {
            path,
            line,
            column,
            include_declaration,
            limit,
        } => {
            let result = find_references(
                &payload.project_id,
                &project_root,
                supervisor,
                path,
                *line,
                *column,
                *include_declaration,
                *limit,
            )?;
            Ok(AgentLspResultEnvelope::ok(result))
        }
    }
}

struct ResolvedProject {
    path: PathBuf,
}

fn resolve_agent_project(
    projects_dir: &Path,
    project_id: &str,
) -> Result<ResolvedProject, AgentLspResultEnvelope> {
    let id = project_id.trim();
    if id.is_empty() {
        return Err(AgentLspResultEnvelope::err(
            error_codes::UNKNOWN_PROJECT,
            "project_id cannot be empty",
        ));
    }
    let projects = load_agent_project_summaries_from_dir(projects_dir);
    let project = projects.into_iter().find(|p| p.id == id).ok_or_else(|| {
        AgentLspResultEnvelope::err(error_codes::UNKNOWN_PROJECT, "unknown agent project")
    })?;
    Ok(ResolvedProject {
        path: PathBuf::from(project.path),
    })
}

fn validate_project_root(
    policy: &AgentPolicy,
    path: &Path,
) -> Result<PathBuf, AgentLspResultEnvelope> {
    cwd_allowed(policy, path).map_err(|message| {
        AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            sanitize_path_message(message),
        )
    })?;
    fs::canonicalize(path).map_err(|_| {
        AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "project root is not accessible",
        )
    })
}

fn lsp_status(
    project_id: &str,
    project_root: &Path,
    supervisor: &LspSupervisor,
) -> LspStatusResult {
    let has_cargo = project_root.join("Cargo.toml").is_file();
    let detected_languages = if has_cargo {
        vec!["rust".to_string()]
    } else {
        Vec::new()
    };
    let info = supervisor.resolve_command_info(LspServerKind::RustAnalyzer);
    let available = info.as_ref().map(|entry| entry.available).unwrap_or(false);
    let slot = supervisor.project_server_status(project_root, LspServerKind::RustAnalyzer);
    let (running, status, position_encoding) = match slot {
        Some(LspServerStatus::Running) => {
            let encoding = supervisor
                .project_position_encoding(project_root, LspServerKind::RustAnalyzer)
                .map(|encoding| encoding.as_public_label().to_string());
            (true, LspAvailabilityStatus::Running, encoding)
        }
        Some(LspServerStatus::Initializing) => (false, LspAvailabilityStatus::Initializing, None),
        Some(LspServerStatus::Crashed) => (false, LspAvailabilityStatus::Crashed, None),
        Some(LspServerStatus::Available) | Some(LspServerStatus::Unavailable) | None => {
            if available {
                (false, LspAvailabilityStatus::Available, None)
            } else {
                (false, LspAvailabilityStatus::Unavailable, None)
            }
        }
    };
    let source = info.as_ref().map(|entry| entry.source);
    LspStatusResult {
        project: project_id.to_string(),
        detected_languages,
        servers: vec![LspServerStatusEntry {
            language: "rust".to_string(),
            server: "rust-analyzer".to_string(),
            available,
            running,
            status,
            source,
            position_encoding,
        }],
        warnings: Vec::new(),
    }
}

/// Read a validated project document with a pre-allocation size guard.
///
/// The LSP wire cap (`MAX_LSP_MESSAGE_BYTES`) would reject an oversized
/// `didOpen` only after the whole file is already resident in agent memory;
/// checking metadata first keeps a model-chosen giant `.rs` file from forcing
/// that allocation. See `MAX_LSP_DOCUMENT_BYTES` for the race caveat.
fn read_document_text(file: &Path) -> Result<String, AgentLspResultEnvelope> {
    let metadata = fs::metadata(file).map_err(|_| {
        AgentLspResultEnvelope::err(error_codes::FILE_NOT_FOUND, "failed to read file")
    })?;
    if metadata.len() > MAX_LSP_DOCUMENT_BYTES {
        return Err(AgentLspResultEnvelope::err(
            error_codes::DOCUMENT_TOO_LARGE,
            "file exceeds the LSP navigation document size limit",
        ));
    }
    fs::read_to_string(file).map_err(|_| {
        AgentLspResultEnvelope::err(error_codes::FILE_NOT_FOUND, "failed to read file")
    })
}

fn document_symbols(
    project_id: &str,
    project_root: &Path,
    supervisor: &LspSupervisor,
    relative_path: &str,
    limit: usize,
) -> Result<DocumentSymbolsResult, AgentLspResultEnvelope> {
    let limit = limit.clamp(1, 500);
    let file = resolve_rust_file(project_root, relative_path)?;
    let uri = file_uri(&file)?;
    let text = read_document_text(&file)?;
    // Take the encoding from prepare_document like goto/references do: the
    // post-request slot lookup could race a slot transition and silently fall
    // back to UTF-16 while the server negotiated another encoding.
    let encoding = supervisor
        .prepare_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
        )
        .map_err(map_lsp_error)?;
    let value = supervisor
        .request_with_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
            "textDocument/documentSymbol",
            json!({ "textDocument": { "uri": uri } }),
        )
        .map_err(map_lsp_error)?;
    let mut cache = LineCache::new();
    cache.seed(&file, text);
    let mut invalid = 0usize;
    let mut external = 0usize;
    let all = normalize_document_symbols(
        project_root,
        &file,
        &value,
        encoding,
        &mut cache,
        &mut invalid,
        &mut external,
    );
    let total_count = count_symbol_nodes(&all);
    let mut truncated = false;
    let symbols = take_symbol_budget(all, limit, &mut truncated);
    let returned_count = count_symbol_nodes(&symbols);
    Ok(DocumentSymbolsResult {
        project: project_id.to_string(),
        path: normalize_relative_path(relative_path),
        language: "rust".to_string(),
        symbols,
        total_count,
        returned_count,
        truncated,
        external_results_omitted: external,
        invalid_results_omitted: invalid,
    })
}

fn goto_definition(
    project_id: &str,
    project_root: &Path,
    supervisor: &LspSupervisor,
    relative_path: &str,
    line: usize,
    column: usize,
    limit: usize,
) -> Result<LocationsResult, AgentLspResultEnvelope> {
    let limit = limit.clamp(1, 100);
    let file = resolve_rust_file(project_root, relative_path)?;
    let uri = file_uri(&file)?;
    let text = read_document_text(&file)?;
    let encoding = supervisor
        .prepare_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
        )
        .map_err(map_lsp_error)?;
    let (lsp_line, lsp_character) = public_to_lsp(&text, line, column, encoding)
        .map_err(|msg| AgentLspResultEnvelope::err(error_codes::INVALID_ARGUMENTS, msg))?;
    let value = supervisor
        .request_with_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": lsp_line, "character": lsp_character }
            }),
        )
        .map_err(map_lsp_error)?;
    let mut cache = LineCache::new();
    cache.seed(&file, text);
    let (locations, total, external, invalid) =
        normalize_locations_value(project_root, &value, encoding, &mut cache);
    finish_locations_result(
        project_id,
        relative_path,
        line,
        column,
        locations,
        total,
        external,
        invalid,
        limit,
    )
}

fn find_references(
    project_id: &str,
    project_root: &Path,
    supervisor: &LspSupervisor,
    relative_path: &str,
    line: usize,
    column: usize,
    include_declaration: bool,
    limit: usize,
) -> Result<LocationsResult, AgentLspResultEnvelope> {
    let limit = limit.clamp(1, 200);
    let file = resolve_rust_file(project_root, relative_path)?;
    let uri = file_uri(&file)?;
    let text = read_document_text(&file)?;
    let encoding = supervisor
        .prepare_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
        )
        .map_err(map_lsp_error)?;
    let (lsp_line, lsp_character) = public_to_lsp(&text, line, column, encoding)
        .map_err(|msg| AgentLspResultEnvelope::err(error_codes::INVALID_ARGUMENTS, msg))?;
    let value = supervisor
        .request_with_document(
            project_root,
            LspServerKind::RustAnalyzer,
            &uri,
            "rust",
            &text,
            "textDocument/references",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": lsp_line, "character": lsp_character },
                "context": { "includeDeclaration": include_declaration }
            }),
        )
        .map_err(map_lsp_error)?;
    let mut cache = LineCache::new();
    cache.seed(&file, text);
    let (locations, total, external, invalid) =
        normalize_locations_value(project_root, &value, encoding, &mut cache);
    finish_locations_result(
        project_id,
        relative_path,
        line,
        column,
        locations,
        total,
        external,
        invalid,
        limit,
    )
}

fn finish_locations_result(
    project_id: &str,
    relative_path: &str,
    line: usize,
    column: usize,
    mut locations: Vec<PublicLocation>,
    total_results: usize,
    external_results_omitted: usize,
    invalid_results_omitted: usize,
    limit: usize,
) -> Result<LocationsResult, AgentLspResultEnvelope> {
    locations.sort_by(|a, b| {
        (
            a.path.as_str(),
            a.range.start.line,
            a.range.start.column,
            a.range.end.line,
            a.range.end.column,
        )
            .cmp(&(
                b.path.as_str(),
                b.range.start.line,
                b.range.start.column,
                b.range.end.line,
                b.range.end.column,
            ))
    });
    locations.dedup();
    let project_valid = locations.len();
    let truncated = project_valid > limit;
    locations.truncate(limit);
    Ok(LocationsResult {
        project: project_id.to_string(),
        path: normalize_relative_path(relative_path),
        query_position: PublicPosition { line, column },
        returned_count: locations.len(),
        locations,
        total_results,
        truncated,
        external_results_omitted,
        invalid_results_omitted,
    })
}

fn resolve_rust_file(
    project_root: &Path,
    relative_path: &str,
) -> Result<PathBuf, AgentLspResultEnvelope> {
    let path = relative_path.trim();
    if path.is_empty() {
        return Err(AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "path cannot be empty",
        ));
    }
    let raw = Path::new(path);
    if raw.is_absolute() {
        return Err(AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "path must be project-relative",
        ));
    }
    if raw
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "path must not contain '..'",
        ));
    }
    if raw
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| !ext.eq_ignore_ascii_case("rs"))
        .unwrap_or(true)
    {
        return Err(AgentLspResultEnvelope::err(
            error_codes::UNSUPPORTED_LANGUAGE,
            "only .rs files are supported",
        ));
    }
    let joined = project_root.join(raw);
    let canonical = fs::canonicalize(&joined)
        .map_err(|_| AgentLspResultEnvelope::err(error_codes::FILE_NOT_FOUND, "file not found"))?;
    if !canonical.starts_with(project_root) {
        return Err(AgentLspResultEnvelope::err(
            error_codes::INVALID_PROJECT_PATH,
            "path resolves outside project root",
        ));
    }
    if !canonical.is_file() {
        return Err(AgentLspResultEnvelope::err(
            error_codes::FILE_NOT_FOUND,
            "path is not a regular file",
        ));
    }
    Ok(canonical)
}

fn file_uri(path: &Path) -> Result<String, AgentLspResultEnvelope> {
    Url::from_file_path(path)
        .map(|url| url.to_string())
        .map_err(|_| {
            AgentLspResultEnvelope::err(error_codes::INVALID_PROJECT_PATH, "invalid file path")
        })
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn project_relative_path(project_root: &Path, absolute: &Path) -> Option<String> {
    let rel = absolute.strip_prefix(project_root).ok()?;
    let text = rel.to_str()?;
    Some(text.replace('\\', "/"))
}

fn normalize_locations_value(
    project_root: &Path,
    value: &Value,
    encoding: PositionEncoding,
    cache: &mut LineCache,
) -> (Vec<PublicLocation>, usize, usize, usize) {
    let mut locations = Vec::new();
    let mut external = 0usize;
    let mut invalid = 0usize;
    let mut total = 0usize;
    if value.is_null() {
        return (locations, 0, 0, 0);
    }
    if let Some(array) = value.as_array() {
        for item in array {
            total += 1;
            match normalize_location_or_link(project_root, item, encoding, cache) {
                LocationNormalize::Ok(loc) => locations.push(loc),
                LocationNormalize::External => external += 1,
                LocationNormalize::Invalid => invalid += 1,
            }
        }
        return (locations, total, external, invalid);
    }
    total = 1;
    match normalize_location_or_link(project_root, value, encoding, cache) {
        LocationNormalize::Ok(loc) => locations.push(loc),
        LocationNormalize::External => external += 1,
        LocationNormalize::Invalid => invalid += 1,
    }
    (locations, total, external, invalid)
}

enum LocationNormalize {
    Ok(PublicLocation),
    External,
    Invalid,
}

fn normalize_location_or_link(
    project_root: &Path,
    value: &Value,
    encoding: PositionEncoding,
    cache: &mut LineCache,
) -> LocationNormalize {
    if value.get("targetUri").is_some() {
        // LocationLink
        let Some(uri) = value.get("targetUri").and_then(Value::as_str) else {
            return LocationNormalize::Invalid;
        };
        let path = match classify_uri_against_project_root(project_root, uri) {
            ProjectUriClassification::InsideProject(path) => path,
            ProjectUriClassification::OutsideProject => return LocationNormalize::External,
            ProjectUriClassification::Unsupported => return LocationNormalize::Invalid,
        };
        let Some(rel) = project_relative_path(project_root, &path) else {
            return LocationNormalize::External;
        };
        let Some(selection) = value.get("targetSelectionRange") else {
            return LocationNormalize::Invalid;
        };
        let Some(range) = convert_range(cache, &path, selection, encoding) else {
            return LocationNormalize::Invalid;
        };
        let target_range = value
            .get("targetRange")
            .and_then(|range| convert_range(cache, &path, range, encoding));
        return LocationNormalize::Ok(PublicLocation {
            path: rel,
            range,
            target_range,
        });
    }
    let Some(uri) = value.get("uri").and_then(Value::as_str) else {
        return LocationNormalize::Invalid;
    };
    let path = match classify_uri_against_project_root(project_root, uri) {
        ProjectUriClassification::InsideProject(path) => path,
        ProjectUriClassification::OutsideProject => return LocationNormalize::External,
        ProjectUriClassification::Unsupported => return LocationNormalize::Invalid,
    };
    let Some(rel) = project_relative_path(project_root, &path) else {
        return LocationNormalize::External;
    };
    let Some(range_value) = value.get("range") else {
        return LocationNormalize::Invalid;
    };
    let Some(range) = convert_range(cache, &path, range_value, encoding) else {
        return LocationNormalize::Invalid;
    };
    LocationNormalize::Ok(PublicLocation {
        path: rel,
        range,
        target_range: None,
    })
}

fn convert_range(
    cache: &mut LineCache,
    path: &Path,
    range: &Value,
    encoding: PositionEncoding,
) -> Option<PublicRange> {
    let start = range.get("start")?;
    let end = range.get("end")?;
    let start_line = start.get("line")?.as_u64()? as u32;
    let start_character = start.get("character")?.as_u64()? as u32;
    let end_line = end.get("line")?.as_u64()? as u32;
    let end_character = end.get("character")?.as_u64()? as u32;
    let text = cache.text(path)?;
    let (sl, sc) = lsp_to_public(text, start_line, start_character, encoding)?;
    let (el, ec) = lsp_to_public(text, end_line, end_character, encoding)?;
    Some(PublicRange {
        start: PublicPosition {
            line: sl,
            column: sc,
        },
        end: PublicPosition {
            line: el,
            column: ec,
        },
    })
}

fn normalize_document_symbols(
    project_root: &Path,
    document_path: &Path,
    value: &Value,
    encoding: PositionEncoding,
    cache: &mut LineCache,
    invalid: &mut usize,
    external: &mut usize,
) -> Vec<PublicSymbol> {
    let Some(array) = value.as_array() else {
        if !value.is_null() {
            *invalid += 1;
        }
        return Vec::new();
    };
    // Detect SymbolInformation[] (has location) vs DocumentSymbol[] (has range).
    if array
        .first()
        .and_then(|item| item.get("location"))
        .is_some()
    {
        let mut symbols = Vec::new();
        for item in array {
            match normalize_symbol_information(project_root, item, encoding, cache) {
                Some(symbol) => symbols.push(symbol),
                None => {
                    // Distinguish external URI vs invalid.
                    if item
                        .pointer("/location/uri")
                        .and_then(Value::as_str)
                        .is_some_and(|uri| {
                            matches!(
                                classify_uri_against_project_root(project_root, uri),
                                ProjectUriClassification::OutsideProject
                            )
                        })
                    {
                        *external += 1;
                    } else {
                        *invalid += 1;
                    }
                }
            }
        }
        return symbols;
    }
    let mut symbols = Vec::new();
    for item in array {
        if let Some(symbol) =
            normalize_document_symbol_node(document_path, item, encoding, cache, invalid)
        {
            symbols.push(symbol);
        }
    }
    symbols
}

fn normalize_document_symbol_node(
    document_path: &Path,
    value: &Value,
    encoding: PositionEncoding,
    cache: &mut LineCache,
    invalid: &mut usize,
) -> Option<PublicSymbol> {
    let name = value.get("name").and_then(Value::as_str).unwrap_or("");
    if name.is_empty() {
        *invalid += 1;
        return None;
    }
    let kind_code = value.get("kind").and_then(Value::as_i64).unwrap_or(0);
    let range = match value
        .get("range")
        .and_then(|range| convert_range(cache, document_path, range, encoding))
    {
        Some(range) => range,
        None => {
            *invalid += 1;
            return None;
        }
    };
    let selection_range = value
        .get("selectionRange")
        .and_then(|range| convert_range(cache, document_path, range, encoding))
        .unwrap_or_else(|| range.clone());
    let detail = value
        .get("detail")
        .and_then(Value::as_str)
        .map(bound_symbol_detail);
    let mut children = Vec::new();
    if let Some(child_values) = value.get("children").and_then(Value::as_array) {
        for child in child_values {
            if let Some(symbol) =
                normalize_document_symbol_node(document_path, child, encoding, cache, invalid)
            {
                children.push(symbol);
            }
        }
    }
    Some(PublicSymbol {
        name: bound_symbol_name(name),
        kind: symbol_kind_name(kind_code).to_string(),
        kind_code,
        detail,
        range,
        selection_range,
        children,
    })
}

fn normalize_symbol_information(
    project_root: &Path,
    value: &Value,
    encoding: PositionEncoding,
    cache: &mut LineCache,
) -> Option<PublicSymbol> {
    let name = value.get("name").and_then(Value::as_str)?;
    let kind_code = value.get("kind").and_then(Value::as_i64).unwrap_or(0);
    let location = value.get("location")?;
    match normalize_location_or_link(project_root, location, encoding, cache) {
        LocationNormalize::Ok(loc) => Some(PublicSymbol {
            name: bound_symbol_name(name),
            kind: symbol_kind_name(kind_code).to_string(),
            kind_code,
            detail: None,
            range: loc.range.clone(),
            selection_range: loc.range,
            children: Vec::new(),
        }),
        _ => None,
    }
}

fn bound_symbol_name(name: &str) -> String {
    bound_symbol_field(name, MAX_SYMBOL_NAME_CHARS)
}

fn bound_symbol_detail(detail: &str) -> String {
    bound_symbol_field(detail, MAX_SYMBOL_DETAIL_CHARS)
}

fn bound_symbol_field(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>()
        + "…"
}

/// Map LSP SymbolKind integer to a stable lowercase name.
fn symbol_kind_name(kind_code: i64) -> &'static str {
    match kind_code {
        1 => "file",
        2 => "module",
        3 => "namespace",
        4 => "package",
        5 => "class",
        6 => "method",
        7 => "property",
        8 => "field",
        9 => "constructor",
        10 => "enum",
        11 => "interface",
        12 => "function",
        13 => "variable",
        14 => "constant",
        15 => "string",
        16 => "number",
        17 => "boolean",
        18 => "array",
        19 => "object",
        20 => "key",
        21 => "null",
        22 => "enum_member",
        23 => "struct",
        24 => "event",
        25 => "operator",
        26 => "type_parameter",
        _ => "unknown",
    }
}

fn count_symbol_nodes(symbols: &[PublicSymbol]) -> usize {
    symbols
        .iter()
        .map(|symbol| 1 + count_symbol_nodes(&symbol.children))
        .sum()
}

fn take_symbol_budget(
    symbols: Vec<PublicSymbol>,
    budget: usize,
    truncated: &mut bool,
) -> Vec<PublicSymbol> {
    let mut remaining = budget;
    let mut out = Vec::new();
    for symbol in symbols {
        if remaining == 0 {
            *truncated = true;
            break;
        }
        remaining -= 1;
        let mut children_truncated = false;
        let children = take_symbol_budget(symbol.children, remaining, &mut children_truncated);
        let used = count_symbol_nodes(&children);
        remaining = remaining.saturating_sub(used);
        if children_truncated {
            *truncated = true;
        }
        out.push(PublicSymbol {
            name: symbol.name,
            kind: symbol.kind,
            kind_code: symbol.kind_code,
            detail: symbol.detail,
            range: symbol.range,
            selection_range: symbol.selection_range,
            children,
        });
        if remaining == 0 && children_truncated {
            break;
        }
    }
    out
}

fn map_lsp_error(error: LspError) -> AgentLspResultEnvelope {
    let (code, message) = match &error {
        LspError::ServerUnavailable => (
            error_codes::LSP_SERVER_UNAVAILABLE,
            "language server is unavailable".to_string(),
        ),
        LspError::RequestTimeout { .. } => (
            error_codes::LSP_REQUEST_TIMEOUT,
            "language server request timed out".to_string(),
        ),
        LspError::JsonRpc { message, .. } | LspError::ProtocolError(message) => (
            error_codes::LSP_PROTOCOL_ERROR,
            bound_error_message(message),
        ),
        LspError::RestartExhausted(message) => (
            error_codes::LSP_SERVER_FAILED,
            bound_error_message(format!("restart exhausted: {message}")),
        ),
        LspError::ServerExited => (
            error_codes::LSP_SERVER_FAILED,
            "language server exited".to_string(),
        ),
        LspError::SpawnFailed(_) | LspError::InitializeFailed(_) => (
            error_codes::LSP_SERVER_FAILED,
            "language server failed to start".to_string(),
        ),
        LspError::InvalidProjectRoot(_) => (
            error_codes::INVALID_PROJECT_PATH,
            "invalid project root".to_string(),
        ),
        other => (
            error_codes::LSP_SERVER_FAILED,
            bound_error_message(other.to_string()),
        ),
    };
    AgentLspResultEnvelope::err(code, sanitize_path_message(message))
}

fn sanitize_path_message(message: impl Into<String>) -> String {
    // `bound_error_message` redacts file URIs, absolute POSIX/Windows paths
    // (including quoted and `key=/...` embedded forms), scrubs control
    // characters, and truncates. Kept as a named wrapper so agent call sites
    // state intent.
    bound_error_message(message.into())
}

fn lsp_error_cmd(start: Instant, code: &str, message: &str) -> CommandResult {
    let envelope = AgentLspResultEnvelope::err(code, message);
    CommandResult {
        exit_code: Some(0),
        stdout: Some(envelope.to_stdout_json()),
        stderr: Some(String::new()),
        duration_ms: Some(start.elapsed().as_millis() as u64),
        error: None,
    }
}
